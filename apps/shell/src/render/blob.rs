//! The surface shader: compiles `blob.hlsl` once and draws a full-screen
//! triangle with the scene in a constant buffer.

use std::{cell::RefCell, sync::Arc};

use anyhow::{Context, Result, bail};
use norbanner_core::sdf::Scene;
use norbanner_shell::icons::IconImage;
use norbanner_theme::Color;
use windows::{
    Win32::Graphics::{
        Direct3D::{D3D11_PRIMITIVE_TOPOLOGY_TRIANGLELIST, Fxc::D3DCompile, ID3DBlob},
        Direct3D11::{
            D3D11_BIND_CONSTANT_BUFFER, D3D11_BIND_SHADER_RESOURCE, D3D11_BUFFER_DESC,
            D3D11_COMPARISON_NEVER, D3D11_CPU_ACCESS_WRITE, D3D11_FILTER_MIN_MAG_MIP_LINEAR,
            D3D11_MAP_WRITE_DISCARD, D3D11_MAPPED_SUBRESOURCE, D3D11_SAMPLER_DESC,
            D3D11_SUBRESOURCE_DATA, D3D11_TEXTURE_ADDRESS_CLAMP, D3D11_TEXTURE2D_DESC,
            D3D11_USAGE_DYNAMIC, D3D11_USAGE_IMMUTABLE, D3D11_VIEWPORT, ID3D11Buffer, ID3D11Device,
            ID3D11DeviceContext, ID3D11PixelShader, ID3D11RenderTargetView, ID3D11SamplerState,
            ID3D11ShaderResourceView, ID3D11Texture2D, ID3D11VertexShader,
        },
        Dxgi::Common::{DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_SAMPLE_DESC},
    },
    core::{PCSTR, s},
};

const SOURCE: &str = include_str!("blob.hlsl");
pub const MAX_BLOBS: usize = 24;

/// What the shader draws, in physical pixels.
#[derive(Clone, Debug, Default)]
pub struct BlobScene {
    pub scene: Scene,
    pub color: Color,
    pub shadow: Color,
    pub shadow_size: f32,
    pub shadow_alpha: f32,
    /// The blurred wallpaper to show through the surface (frosted glass),
    /// with its generation, and how much of `color` lies over it.
    pub glass: Option<(u32, Arc<IconImage>)>,
    pub tint: f32,
    /// A line along the surface's edge (alpha 0: none), and its width.
    pub outline: Color,
    pub outline_width: f32,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct Constants {
    inner: [f32; 4],
    params: [f32; 4],
    surface: [f32; 4],
    shadow: [f32; 4],
    counts: [f32; 4],
    glass: [f32; 4],
    cover: [f32; 4],
    outline: [f32; 4],
    blobs: [[f32; 4]; MAX_BLOBS],
    radii: [[f32; 4]; MAX_BLOBS],
}

pub struct Pipeline {
    vs: ID3D11VertexShader,
    ps: ID3D11PixelShader,
    constants: ID3D11Buffer,
    sampler: ID3D11SamplerState,
    /// The wallpaper texture and the generation it was made from.
    wallpaper: RefCell<Option<(u32, ID3D11ShaderResourceView)>>,
}

/// A shader view of `image` on `device`.
fn texture(device: &ID3D11Device, image: &IconImage) -> Result<ID3D11ShaderResourceView> {
    let mut tex: Option<ID3D11Texture2D> = None;
    let mut view = None;
    // SAFETY: `bgra` holds width*height*4 bytes for the initial data.
    unsafe {
        device.CreateTexture2D(
            &D3D11_TEXTURE2D_DESC {
                Width: image.width,
                Height: image.height,
                MipLevels: 1,
                ArraySize: 1,
                Format: DXGI_FORMAT_B8G8R8A8_UNORM,
                SampleDesc: DXGI_SAMPLE_DESC {
                    Count: 1,
                    Quality: 0,
                },
                Usage: D3D11_USAGE_IMMUTABLE,
                BindFlags: D3D11_BIND_SHADER_RESOURCE.0 as u32,
                ..Default::default()
            },
            Some(&D3D11_SUBRESOURCE_DATA {
                pSysMem: image.bgra.as_ptr().cast(),
                SysMemPitch: image.width * 4,
                SysMemSlicePitch: 0,
            }),
            Some(&mut tex),
        )?;
        device.CreateShaderResourceView(&tex.context("no texture")?, None, Some(&mut view))?;
    }
    view.context("no texture view")
}

fn compile(entry: PCSTR, target: PCSTR) -> Result<Vec<u8>> {
    let mut code: Option<ID3DBlob> = None;
    let mut errors: Option<ID3DBlob> = None;
    // SAFETY: The source outlives the call; blobs are owned locals.
    let result = unsafe {
        D3DCompile(
            SOURCE.as_ptr().cast(),
            SOURCE.len(),
            s!("blob.hlsl"),
            None,
            None,
            entry,
            target,
            0,
            0,
            &mut code,
            Some(&mut errors),
        )
    };
    if let Err(error) = result {
        let message = errors.map_or_else(String::new, |blob| {
            // SAFETY: The error blob holds an ANSI string of its size.
            unsafe {
                let bytes = std::slice::from_raw_parts(
                    blob.GetBufferPointer() as *const u8,
                    blob.GetBufferSize(),
                );
                String::from_utf8_lossy(bytes).into_owned()
            }
        });
        bail!("shader did not compile: {error} {message}");
    }
    let code = code.context("shader compiler returned nothing")?;
    // SAFETY: The blob holds bytecode of its size.
    Ok(unsafe {
        std::slice::from_raw_parts(code.GetBufferPointer() as *const u8, code.GetBufferSize())
            .to_vec()
    })
}

impl Pipeline {
    pub fn new(device: &ID3D11Device) -> Result<Self> {
        let vs_code = compile(s!("vs"), s!("vs_4_0"))?;
        let ps_code = compile(s!("ps"), s!("ps_4_0"))?;
        let mut vs = None;
        let mut ps = None;
        let mut constants = None;
        let mut sampler = None;
        // SAFETY: Bytecode and descriptors are valid for each call.
        unsafe {
            device.CreateVertexShader(&vs_code, None, Some(&mut vs))?;
            device.CreatePixelShader(&ps_code, None, Some(&mut ps))?;
            device.CreateBuffer(
                &D3D11_BUFFER_DESC {
                    ByteWidth: size_of::<Constants>() as u32,
                    Usage: D3D11_USAGE_DYNAMIC,
                    BindFlags: D3D11_BIND_CONSTANT_BUFFER.0 as u32,
                    CPUAccessFlags: D3D11_CPU_ACCESS_WRITE.0 as u32,
                    ..Default::default()
                },
                None,
                Some(&mut constants),
            )?;
            device.CreateSamplerState(
                &D3D11_SAMPLER_DESC {
                    Filter: D3D11_FILTER_MIN_MAG_MIP_LINEAR,
                    AddressU: D3D11_TEXTURE_ADDRESS_CLAMP,
                    AddressV: D3D11_TEXTURE_ADDRESS_CLAMP,
                    AddressW: D3D11_TEXTURE_ADDRESS_CLAMP,
                    ComparisonFunc: D3D11_COMPARISON_NEVER,
                    MaxLOD: f32::MAX,
                    ..Default::default()
                },
                Some(&mut sampler),
            )?;
        }
        Ok(Self {
            vs: vs.context("no vertex shader")?,
            ps: ps.context("no pixel shader")?,
            constants: constants.context("no constant buffer")?,
            sampler: sampler.context("no sampler")?,
            wallpaper: RefCell::new(None),
        })
    }

    pub fn draw(
        &self,
        ctx: &ID3D11DeviceContext,
        rtv: &ID3D11RenderTargetView,
        width: u32,
        height: u32,
        scene: &BlobScene,
    ) -> Result<()> {
        let s = &scene.scene;
        let c = scene.color;
        let mut constants = Constants {
            inner: [s.inner.x, s.inner.y, s.inner.w, s.inner.h],
            params: [
                s.inner_radius,
                s.smoothing,
                scene.shadow_size,
                scene.shadow_alpha,
            ],
            // Premultiplied for the swap chain.
            surface: [c.r * c.a, c.g * c.a, c.b * c.a, c.a],
            shadow: [scene.shadow.r, scene.shadow.g, scene.shadow.b, 1.0],
            counts: [0.0, scene.outline_width, 0.0, 0.0],
            glass: [0.0; 4],
            cover: [0.0; 4],
            outline: [
                scene.outline.r,
                scene.outline.g,
                scene.outline.b,
                scene.outline.a,
            ],
            blobs: [[0.0; 4]; MAX_BLOBS],
            radii: [[0.0; 4]; MAX_BLOBS],
        };
        let mut count = 0;
        for blob in s
            .blobs
            .iter()
            .filter(|b| b.rect.w > 0.5 && b.rect.h > 0.5)
            .take(MAX_BLOBS)
        {
            constants.blobs[count] = [blob.rect.x, blob.rect.y, blob.rect.w, blob.rect.h];
            constants.radii[count] = [blob.radius, 0.0, 0.0, 0.0];
            count += 1;
        }
        constants.counts[0] = count as f32;
        let view = self.wallpaper_view(ctx, scene);
        if let (Some(_), Some((_, image))) = (&view, &scene.glass) {
            // Cover the screen, as Windows' "fill" does.
            let (iw, ih) = (image.width as f32, image.height as f32);
            let (w, h) = (width as f32, height as f32);
            let scale = (w / iw).max(h / ih);
            let (dw, dh) = (iw * scale, ih * scale);
            constants.glass = [1.0, scene.tint, 0.0, 0.0];
            constants.cover = [
                1.0 / dw,
                1.0 / dh,
                -(w - dw) / 2.0 / dw,
                -(h - dh) / 2.0 / dh,
            ];
        }
        // SAFETY: The mapped buffer is exactly `Constants` sized; every
        // object bound belongs to this device.
        unsafe {
            let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
            ctx.Map(
                &self.constants,
                0,
                D3D11_MAP_WRITE_DISCARD,
                0,
                Some(&mut mapped),
            )?;
            mapped.pData.cast::<Constants>().write_unaligned(constants);
            ctx.Unmap(&self.constants, 0);

            ctx.ClearRenderTargetView(rtv, &[0.0; 4]);
            ctx.OMSetRenderTargets(Some(&[Some(rtv.clone())]), None);
            ctx.RSSetViewports(Some(&[D3D11_VIEWPORT {
                TopLeftX: 0.0,
                TopLeftY: 0.0,
                Width: width as f32,
                Height: height as f32,
                MinDepth: 0.0,
                MaxDepth: 1.0,
            }]));
            ctx.IASetInputLayout(None);
            ctx.IASetPrimitiveTopology(D3D11_PRIMITIVE_TOPOLOGY_TRIANGLELIST);
            ctx.VSSetShader(&self.vs, None);
            ctx.PSSetShader(&self.ps, None);
            ctx.PSSetConstantBuffers(0, Some(&[Some(self.constants.clone())]));
            ctx.PSSetShaderResources(0, Some(&[view]));
            ctx.PSSetSamplers(0, Some(&[Some(self.sampler.clone())]));
            ctx.OMSetBlendState(None, None, u32::MAX);
            ctx.Draw(3, 0);
            ctx.OMSetRenderTargets(None, None);
        }
        Ok(())
    }

    /// The texture for the scene's wallpaper, made when it changes.
    fn wallpaper_view(
        &self,
        ctx: &ID3D11DeviceContext,
        scene: &BlobScene,
    ) -> Option<ID3D11ShaderResourceView> {
        let (generation, image) = scene.glass.as_ref()?;
        if image.width == 0 || image.height == 0 {
            return None;
        }
        let mut cached = self.wallpaper.borrow_mut();
        if let Some((made, view)) = cached.as_ref()
            && made == generation
        {
            return Some(view.clone());
        }
        // SAFETY: The context belongs to a live device.
        let device = unsafe { ctx.GetDevice() }.ok()?;
        let view = texture(&device, image).ok()?;
        *cached = Some((*generation, view.clone()));
        Some(view)
    }
}

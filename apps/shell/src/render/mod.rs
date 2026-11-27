//! Drawing: one Direct3D 11 device shared by every screen. Each screen's
//! window shows a DirectComposition swap chain; a frame is the surface
//! shader (frame + drawers as one distance field) followed by Direct2D for
//! everything on it (text, icons, images).

mod blob;
mod fonts;
pub mod paint;

use anyhow::{Context, Result};
use windows::{
    Win32::{
        Foundation::{HMODULE, HWND},
        Graphics::{
            Direct2D::{
                Common::{D2D1_ALPHA_MODE_PREMULTIPLIED, D2D1_PIXEL_FORMAT},
                D2D1_BITMAP_OPTIONS_CANNOT_DRAW, D2D1_BITMAP_OPTIONS_TARGET,
                D2D1_BITMAP_PROPERTIES1, D2D1_DEBUG_LEVEL_NONE, D2D1_DEVICE_CONTEXT_OPTIONS_NONE,
                D2D1_FACTORY_OPTIONS, D2D1_FACTORY_TYPE_SINGLE_THREADED, D2D1CreateFactory,
                ID2D1Bitmap1, ID2D1DeviceContext, ID2D1Factory1,
            },
            Direct3D::{D3D_DRIVER_TYPE, D3D_DRIVER_TYPE_HARDWARE, D3D_DRIVER_TYPE_WARP},
            Direct3D11::{
                D3D11_CREATE_DEVICE_BGRA_SUPPORT, D3D11_SDK_VERSION, D3D11CreateDevice,
                ID3D11Device, ID3D11DeviceContext, ID3D11RenderTargetView, ID3D11Texture2D,
            },
            DirectComposition::{
                DCompositionCreateDevice, IDCompositionDevice, IDCompositionTarget,
                IDCompositionVisual,
            },
            DirectWrite::{DWRITE_FACTORY_TYPE_SHARED, DWriteCreateFactory, IDWriteFactory6},
            Dxgi::{
                Common::{
                    DXGI_ALPHA_MODE_PREMULTIPLIED, DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_FORMAT_UNKNOWN,
                    DXGI_SAMPLE_DESC,
                },
                CreateDXGIFactory2, DXGI_CREATE_FACTORY_FLAGS, DXGI_PRESENT, DXGI_SCALING_STRETCH,
                DXGI_SWAP_CHAIN_DESC1, DXGI_SWAP_CHAIN_FLAG, DXGI_SWAP_EFFECT_FLIP_SEQUENTIAL,
                DXGI_USAGE_RENDER_TARGET_OUTPUT, IDXGIDevice, IDXGIFactory2, IDXGISurface,
                IDXGISwapChain1,
            },
        },
    },
    core::Interface,
};

pub use blob::BlobScene;
pub use fonts::Fonts;
pub use paint::Painter;

/// The device and everything created from it once.
pub struct Gpu {
    pub d3d: ID3D11Device,
    pub ctx: ID3D11DeviceContext,
    dxgi_factory: IDXGIFactory2,
    pub d2d_factory: ID2D1Factory1,
    pub dc: ID2D1DeviceContext,
    pub dcomp: IDCompositionDevice,
    pub dwrite: IDWriteFactory6,
    pub fonts: Fonts,
    blob: blob::Pipeline,
}

fn create_device(driver: D3D_DRIVER_TYPE) -> Result<(ID3D11Device, ID3D11DeviceContext)> {
    let mut device = None;
    let mut context = None;
    // SAFETY: Out-pointers are valid locals.
    unsafe {
        D3D11CreateDevice(
            None,
            driver,
            HMODULE::default(),
            D3D11_CREATE_DEVICE_BGRA_SUPPORT,
            None,
            D3D11_SDK_VERSION,
            Some(&mut device),
            None,
            Some(&mut context),
        )?;
    }
    Ok((
        device.context("no Direct3D device")?,
        context.context("no Direct3D context")?,
    ))
}

impl Gpu {
    pub fn new() -> Result<Self> {
        let (d3d, ctx) = create_device(D3D_DRIVER_TYPE_HARDWARE)
            .or_else(|_| create_device(D3D_DRIVER_TYPE_WARP))
            .context("could not create a Direct3D 11 device")?;
        let dxgi: IDXGIDevice = d3d.cast()?;
        // SAFETY: Plain factory and device creation on this thread.
        unsafe {
            let dxgi_factory: IDXGIFactory2 = CreateDXGIFactory2(DXGI_CREATE_FACTORY_FLAGS(0))?;
            let d2d_factory: ID2D1Factory1 = D2D1CreateFactory(
                D2D1_FACTORY_TYPE_SINGLE_THREADED,
                Some(&D2D1_FACTORY_OPTIONS {
                    debugLevel: D2D1_DEBUG_LEVEL_NONE,
                }),
            )?;
            let dc = d2d_factory
                .CreateDevice(&dxgi)?
                .CreateDeviceContext(D2D1_DEVICE_CONTEXT_OPTIONS_NONE)?;
            let dcomp: IDCompositionDevice = DCompositionCreateDevice(&dxgi)?;
            let dwrite: IDWriteFactory6 = DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED)?;
            let fonts = Fonts::load(&dwrite);
            let blob = blob::Pipeline::new(&d3d)?;
            Ok(Self {
                d3d,
                ctx,
                dxgi_factory,
                d2d_factory,
                dc,
                dcomp,
                dwrite,
                fonts,
                blob,
            })
        }
    }

    /// A swap chain shown in `hwnd`, sized `width`×`height` physical pixels.
    pub fn surface(&self, hwnd: HWND, width: u32, height: u32) -> Result<Surface> {
        let desc = DXGI_SWAP_CHAIN_DESC1 {
            Width: width.max(1),
            Height: height.max(1),
            Format: DXGI_FORMAT_B8G8R8A8_UNORM,
            SampleDesc: DXGI_SAMPLE_DESC {
                Count: 1,
                Quality: 0,
            },
            BufferUsage: DXGI_USAGE_RENDER_TARGET_OUTPUT,
            BufferCount: 2,
            Scaling: DXGI_SCALING_STRETCH,
            SwapEffect: DXGI_SWAP_EFFECT_FLIP_SEQUENTIAL,
            AlphaMode: DXGI_ALPHA_MODE_PREMULTIPLIED,
            ..Default::default()
        };
        // SAFETY: The device, window and descriptor are valid; the
        // composition objects are kept alive by the returned surface.
        unsafe {
            let swapchain = self
                .dxgi_factory
                .CreateSwapChainForComposition(&self.d3d, &desc, None)?;
            let target = self.dcomp.CreateTargetForHwnd(hwnd, true)?;
            let visual = self.dcomp.CreateVisual()?;
            visual.SetContent(&swapchain)?;
            target.SetRoot(&visual)?;
            self.dcomp.Commit()?;
            let mut surface = Surface {
                swapchain,
                _target: target,
                _visual: visual,
                views: None,
                width: desc.Width,
                height: desc.Height,
            };
            surface.create_views(self)?;
            Ok(surface)
        }
    }

    /// Draws one frame: the surface shape, then `content` with Direct2D
    /// at `dpi` (so it works in logical pixels), then presents.
    pub fn frame(
        &self,
        surface: &mut Surface,
        scene: &BlobScene,
        dpi: f32,
        content: impl FnOnce(&mut Painter),
    ) -> Result<()> {
        if surface.views.is_none() {
            surface.create_views(self)?;
        }
        let Some(views) = &surface.views else {
            return Ok(());
        };
        self.blob
            .draw(&self.ctx, &views.rtv, surface.width, surface.height, scene)?;
        // SAFETY: The target bitmap belongs to this device; it is unset
        // before the swap chain can be resized.
        unsafe {
            self.dc.SetTarget(&views.target);
            self.dc.SetDpi(dpi, dpi);
            self.dc.BeginDraw();
        }
        {
            let mut painter = Painter::new(self, dpi);
            content(&mut painter);
        }
        // SAFETY: Ends the draw begun above.
        let drawn = unsafe { self.dc.EndDraw(None, None) };
        // SAFETY: Releases the target so ResizeBuffers can succeed.
        unsafe { self.dc.SetTarget(None) };
        drawn.context("Direct2D could not draw")?;
        // SAFETY: Presents the frame just drawn, synced to vblank.
        unsafe { surface.swapchain.Present(1, DXGI_PRESENT(0)) }
            .ok()
            .context("could not present")?;
        Ok(())
    }

    /// Whether the device still works (it is lost when the driver resets).
    pub fn is_lost(&self) -> bool {
        // SAFETY: Read-only status query.
        unsafe { self.d3d.GetDeviceRemovedReason() }.is_err()
    }
}

struct Views {
    rtv: ID3D11RenderTargetView,
    target: ID2D1Bitmap1,
}

/// One screen's swap chain.
pub struct Surface {
    swapchain: IDXGISwapChain1,
    _target: IDCompositionTarget,
    _visual: IDCompositionVisual,
    views: Option<Views>,
    pub width: u32,
    pub height: u32,
}

impl Surface {
    fn create_views(&mut self, gpu: &Gpu) -> Result<()> {
        // SAFETY: Buffer 0 of a flip-model swap chain is always the current
        // back buffer in Direct3D 11; the views keep it alive.
        unsafe {
            let texture: ID3D11Texture2D = self.swapchain.GetBuffer(0)?;
            let mut rtv = None;
            gpu.d3d
                .CreateRenderTargetView(&texture, None, Some(&mut rtv))?;
            let dxgi: IDXGISurface = texture.cast()?;
            let target = gpu.dc.CreateBitmapFromDxgiSurface(
                &dxgi,
                Some(&D2D1_BITMAP_PROPERTIES1 {
                    pixelFormat: D2D1_PIXEL_FORMAT {
                        format: DXGI_FORMAT_B8G8R8A8_UNORM,
                        alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED,
                    },
                    dpiX: 96.0,
                    dpiY: 96.0,
                    bitmapOptions: D2D1_BITMAP_OPTIONS_TARGET | D2D1_BITMAP_OPTIONS_CANNOT_DRAW,
                    ..Default::default()
                }),
            )?;
            self.views = Some(Views {
                rtv: rtv.context("no render target view")?,
                target,
            });
        }
        Ok(())
    }

    pub fn resize(&mut self, gpu: &Gpu, width: u32, height: u32) -> Result<()> {
        let (width, height) = (width.max(1), height.max(1));
        if (width, height) == (self.width, self.height) {
            return Ok(());
        }
        self.views = None;
        // SAFETY: Every view of the old buffers was just released.
        unsafe {
            gpu.ctx.ClearState();
            self.swapchain.ResizeBuffers(
                0,
                width,
                height,
                DXGI_FORMAT_UNKNOWN,
                DXGI_SWAP_CHAIN_FLAG(0),
            )?;
        }
        self.width = width;
        self.height = height;
        self.create_views(gpu)
    }
}

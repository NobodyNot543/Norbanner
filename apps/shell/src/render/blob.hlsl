// The shell's surface: the frame around the screen smoothly merged with
// every open drawer, plus a soft shadow cast into the hole where windows
// show. Mirrors norbanner_core::sdf; keep the two in step.

cbuffer Scene : register(b0)
{
    float4 inner;        // hole: x, y, w, h (physical pixels)
    float4 params;       // hole radius, smoothing, shadow size, shadow alpha
    float4 surfaceColor; // premultiplied
    float4 shadowColor;  // straight rgb
    float4 counts;       // x: blob count, y: outline width (px)
    float4 glass;        // x: wallpaper behind (0/1), y: tint over it
    float4 cover;        // wallpaper uv = pixel * xy + zw
    float4 outlineColor; // straight rgb, a = strength (0: none)
    float4 blobs[24];    // x, y, w, h
    float4 blobRadii[24];// radius in x
};

Texture2D wallpaper : register(t0);
SamplerState linearClamp : register(s0);

struct VsOut
{
    float4 pos : SV_Position;
};

VsOut vs(uint id : SV_VertexID)
{
    // One triangle covering the whole viewport.
    float2 uv = float2((id << 1) & 2, id & 2);
    VsOut o;
    o.pos = float4(uv * float2(2.0, -2.0) + float2(-1.0, 1.0), 0.0, 1.0);
    return o;
}

float roundedBox(float2 p, float4 r, float radius)
{
    float2 h = r.zw * 0.5;
    float rr = clamp(radius, 0.0, min(h.x, h.y));
    float2 q = abs(p - (r.xy + h)) - h + rr;
    return length(max(q, 0.0)) + min(max(q.x, q.y), 0.0) - rr;
}

float smoothMin(float a, float b, float k)
{
    if (k <= 0.0)
        return min(a, b);
    float h = max(k - abs(a - b), 0.0) / k;
    return min(a, b) - h * h * k * 0.25;
}

float4 ps(VsOut i) : SV_Target
{
    float2 p = i.pos.xy;
    float d = (inner.z > 0.0 && inner.w > 0.0) ? -roundedBox(p, inner, params.x) : -1.0e4;
    int count = (int)counts.x;
    [loop]
    for (int b = 0; b < count; b++)
    {
        d = smoothMin(d, roundedBox(p, blobs[b], blobRadii[b].x), params.y);
    }
    float inside = saturate(0.5 - d);
    float4 surface = surfaceColor;
    if (glass.x > 0.5)
    {
        // Frosted glass: the blurred wallpaper under the surface colour.
        float3 behind = wallpaper.Sample(linearClamp, p * cover.xy + cover.zw).rgb;
        float3 tint = surfaceColor.a > 0.0 ? surfaceColor.rgb / surfaceColor.a : surfaceColor.rgb;
        surface = float4(lerp(behind, tint, glass.y), 1.0);
    }
    // A thin line along the surface's edge, inside it, so the frame and the
    // drawers stand out from dark windows and wallpapers.
    float ring = saturate(d + counts.y + 0.5) * outlineColor.a;
    surface = float4(outlineColor.rgb * ring, ring) + surface * (1.0 - ring);
    float fall = saturate(d / max(params.z, 1.0));
    float shadow = params.w * (1.0 - fall) * (1.0 - fall) * step(0.0, d);
    return surface * inside + float4(shadowColor.rgb * shadow, shadow) * (1.0 - inside);
}

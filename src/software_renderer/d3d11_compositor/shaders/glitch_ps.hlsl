cbuffer GpuParameters : register(b0)
{
    matrix worldProjection;
    float iTime;
    uint is_portal_active;
    float2 iResolution;
    float4 effect_bounds;

    float aberration_amount;
    float glitch_speed;
    float scanline_intensity;
    float _hologram_padding;
};

Texture2D texture0 : register(t0);
SamplerState sampler0 : register(s0);

struct vOut
{
    float4 pos : SV_POSITION;
    float2 uv  : TEXCOORD;
};

float hash(float n)
{
    return frac(sin(n) * 43758.5453123);
}

float4 PSMain(vOut input) : SV_TARGET
{
    if (is_portal_active > 0 &&
       (input.uv.x < effect_bounds.x || input.uv.x > effect_bounds.z ||
        input.uv.y < effect_bounds.y || input.uv.y > effect_bounds.w))
    {
        return texture0.Sample(sampler0, input.uv);
    }

    float2 uv = input.uv;
    float intensity = glitch_speed * 0.1;

    float block = floor(uv.y * 15.0 + iTime * 5.0);
    float displacement = (hash(block + iTime * 3.7) - 0.5) * 0.06 * intensity;
    uv.x += displacement;

    float scanline = sin(uv.y * iResolution.y * 1.5 + iTime * 40.0) * 0.5 + 0.5;
    scanline = lerp(1.0, scanline, scanline_intensity * intensity * 1.5);

    float aberration = aberration_amount * intensity;
    float r = texture0.Sample(sampler0, float2(uv.x + aberration, uv.y)).r;
    float g = texture0.Sample(sampler0, uv).g;
    float b = texture0.Sample(sampler0, float2(uv.x - aberration, uv.y)).b;
    float a = texture0.Sample(sampler0, input.uv).a;

    float corrupt_block = floor(uv.y * 30.0 + iTime * 8.0);
    float corrupt = step(0.96 - 0.15 * intensity, hash(corrupt_block + floor(iTime * 4.0)));
    float3 corrupt_color = float3(hash(corrupt_block), hash(corrupt_block + 1.0), hash(corrupt_block + 2.0));

    float3 color = float3(r, g, b) * scanline;
    color = lerp(color, corrupt_color, corrupt * intensity * 0.3);

    return float4(color, a);
}

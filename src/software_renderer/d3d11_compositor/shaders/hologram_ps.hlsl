// The CBuffer that mirrors the GpuParameters struct in Rust.
cbuffer GpuParameters : register(b0)
{
    matrix worldProjection;
    float time;
    uint is_portal_active;
    float2 padding1;
    float4 effect_bounds; // x=left, y=top, z=right, w=bottom

    // Hologram effect
    float aberration_amount;
    float glitch_speed;
    float scanline_intensity;
    float padding2;
};

//  input texture from Flutter
Texture2D texture0 : register(t0);
sampler sampler0 : register(s0);

//  data passed from the Vertex Shader to the Pixel Shader
struct vOut
{
    float4 pos : SV_POSITION;
    float2 uv  : TEXCOORD;
};

// pseudo-random noise helper
float random(float2 st)
{
    return frac(sin(dot(st.xy, float2(12.9898, 78.233))) * 43758.5453123);
}

float4 PSMain(vOut input) : SV_TARGET
{
    // If this effect is targeted at a widget and the current pixel is outside its bounds,
    // render the original texture pixel without any effect.
    if (is_portal_active > 0 && 
       (input.uv.x < effect_bounds.x || input.uv.x > effect_bounds.z ||
        input.uv.y < effect_bounds.y || input.uv.y > effect_bounds.w))
    {
        return texture0.Sample(sampler0, input.uv);
    }
    
    float2 uv = input.uv;
    
    // Effect 1: Vertical Sync Roll
    float roll_height = 0.01;
    float roll_wave = sin((uv.y + time * glitch_speed * 0.01) * 10.0) * roll_height;
    uv.x += roll_wave;

    // Effect 2: Chromatic Aberration
    float2 offset = float2(aberration_amount, 0.0);
    float r = texture0.Sample(sampler0, uv - offset).r;
    float g = texture0.Sample(sampler0, uv).g;
    float b = texture0.Sample(sampler0, uv + offset).b;

    float a = texture0.Sample(sampler0, input.uv).a;

    float4 finalColor = float4(r, g, b, a);

    // Effect 3: Scanlines (fine horizontal lines)
    float scanline = sin(input.uv.y * 1000.0) * scanline_intensity;
    finalColor.rgb -= scanline;
    
    // Effect 4: Noise/Flicker
    float noise_intensity = glitch_speed * 0.002;
    float noise = (random(input.uv + time) - 0.5) * noise_intensity;
    finalColor.rgb += noise;

    finalColor.rgb = clamp(finalColor.rgb, 0.0, 1.0);

    return finalColor;
}

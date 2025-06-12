// The Constant Buffer (cbuffer) that mirrors the GpuParameters struct in Rust.
// MUST be bound to register b0 as defined in Rust.
cbuffer GpuParameters : register(b0)
{

    matrix worldProjection;
    float iTime;           
    uint is_portal_active;  // Flag to check if effect is bound to a specific widget. TODO: Make it bound to multiple
    float2 iResolution;     // Viewport resolution (width, height).
    float4 effect_bounds;   // x=left, y=top, z=right, w=bottom (normalized bounds).

    // Hologram effect unused here. TODO: Maybe extract and make Gpu params better splitted but keep it dynamic
    float aberration_amount;
    float glitch_speed;
    float scanline_intensity;
    float _hologram_padding;

    // Warp
    float speed;             // Speed of the "flight" through hyperspace.
    float density;           // Density of stars/lines.
    float star_base_size;    // Base size of individual stars (before blur/glow).
    float glow_falloff;      // How quickly star glow fades from its center.
    float pulse_speed;       // Speed of the pulsating glow effect.
    float motion_blur_strength; // Strength of the motion blur applied to stars.
    float depth_blur_strength;  // Strength of the depth of field blur effect.

    float base_alpha;        // Base transparency of the overlay (0.0 = fully transparent, 1.0 = fully opaque).
    float3 color_inner;      // Color for stars closer to the center/camera (RGB).
    float3 color_outer;      // Color for stars further away/at the edges (RGB).
    float3 color_pulse;      // Color of the pulsating glow (RGB).
    float bloom_threshold;   // Luminance threshold for applying the bloom effect.
    float bloom_intensity;   // Intensity of the bloom effect.
    
};

// Input texture from Flutter.
// This is the rendered Flutter UI that the shader will overlay.
Texture2D texture0 : register(t0);
SamplerState sampler0 : register(s0); // Standard sampler for the texture.

// Data passed from the Vertex Shader to the Pixel Shader.
struct vOut
{
    float4 pos : SV_POSITION; // Pixel position in screen space.
    float2 uv  : TEXCOORD;    // UV coordinates (0.0-1.0) for texture sampling.
};

float hash12(float2 p) {
    float3 p3 = frac(p.xyx * 0.1031);
    p3 += dot(p3, p3.yzx + 33.33);
    return frac((p3.x + p3.y) * p3.z);
}

float hash31(float3 p) {
    float3 p3 = frac(p * .1031);
    p3 += dot(p3, p3.yzx + 33.33);
    return frac((p3.x + p3.y) * p3.z);
}


float4 PSMain(vOut input) : SV_TARGET
{
    // If 'is_portal_active' is true and the current pixel is outside the 'effect_bounds',
    // render the original texture pixel without applying the effect.
    if (is_portal_active > 0 && 
        (input.uv.x < effect_bounds.x || input.uv.x > effect_bounds.z ||
         input.uv.y < effect_bounds.y || input.uv.y > effect_bounds.w))
    {
        return texture0.Sample(sampler0, input.uv);
    }

    float2 uv = (input.uv * 2.0 - 1.0) * float2(iResolution.x / iResolution.y, 1.0);
    float current_time = iTime * speed; 
    
    float2 camera_shake = float2(
        sin(iTime * 17.0) * 0.001,
        cos(iTime * 13.0) * 0.001
    );
    uv += camera_shake;

    float total_star_brightness = 0.0;
    float3 final_color_rgb = float3(0.0, 0.0, 0.0);

    const int NUM_LAYERS = 4; 
    for (int i = 0; i < NUM_LAYERS; i++)
    {
        float layer_depth = float(i) / (NUM_LAYERS - 1.0);
        float layer_scale = 1.0 + (layer_depth * 1.5);    
        float layer_time_offset = iTime * 0.1 * (layer_depth + 0.5); 
        
        float2 current_uv_layer = uv * density * layer_scale;
        current_uv_layer.x += current_time + layer_time_offset; 

        float2 cell = floor(current_uv_layer); 
        float2 sub_cell = frac(current_uv_layer); 

        float2 star_center_offset = hash12(cell + layer_depth * 100.0) * 0.9 + 0.05; 
        float2 star_local_pos = sub_cell - star_center_offset; 

        float dist_to_star = length(star_local_pos);

        float star_core_alpha = saturate(1.0 - dist_to_star / star_base_size); 
        
        float star_random_factor = hash31(float3(cell, layer_depth * 5.0)) * 0.5 + 0.5;
        star_core_alpha *= star_random_factor;

        float pulse_effect = (0.8 + 0.2 * sin(current_time * pulse_speed + dot(cell, float2(1.0, 1.0))));
        
        float current_star_raw_brightness = star_core_alpha * pulse_effect;
        current_star_raw_brightness = pow(current_star_raw_brightness, glow_falloff); 

        float2 blur_dir = normalize(float2(1.0, uv.y * 0.1)); 
        float motion_blur_amount = current_star_raw_brightness * motion_blur_strength;
        float blurred_dist_to_star = length(star_local_pos - blur_dir * motion_blur_amount);
        float motion_blurred_star_alpha = smoothstep(star_base_size * 2.0, star_base_size * 0.5, blurred_dist_to_star);

        float depth_blur_factor = abs(layer_depth - 0.5) * 2.0; 
        float depth_blur_effect = saturate(1.0 - depth_blur_factor * depth_blur_strength * 100.0);

        float current_star_brightness = motion_blurred_star_alpha * depth_blur_effect * current_star_raw_brightness;

        total_star_brightness += current_star_brightness;

        float3 star_color = lerp(color_inner, color_outer, saturate(length(uv) * 2.0));
        star_color = lerp(star_color, color_pulse, pulse_effect * 0.5); 

        final_color_rgb += star_color * current_star_brightness;
    }

    float3 background_glow_color = lerp(color_inner * 0.1, color_pulse * 0.05, total_star_brightness * 0.3);
    final_color_rgb += background_glow_color;

    float3 bloom = max(0.0, final_color_rgb - bloom_threshold);
    final_color_rgb += bloom * bloom_intensity;

    float4 original_pixel = texture0.Sample(sampler0, input.uv); 
    
    float effective_warp_alpha = total_star_brightness * base_alpha;
    effective_warp_alpha = saturate(effective_warp_alpha);

    float4 final_output_color;
    final_output_color.rgb = original_pixel.rgb + final_color_rgb;
    final_output_color.a = original_pixel.a + (1.0 - original_pixel.a) * effective_warp_alpha;
    final_output_color.a = saturate(final_output_color.a); 

    return clamp(final_output_color, 0.0, 1.0);
}
// Passthrough pixel shader that flips the sampled texture vertically.
//
// Satellite Flutter views render via an FBO with a bound texture
// (render-to-texture), which OpenGL/ANGLE stores bottom-up. Sampling it with a
// top-down D3D quad shows the image upside-down. Flipping v (1 - v) here
// corrects it without touching the shared fullscreen_quad vertex shader.
struct vOut
{
    float4 pos : SV_POSITION;
    float2 uv : TEXCOORD;
};

sampler sampler0;
Texture2D texture0;

float4 PSMain(vOut input) : SV_TARGET {
    float2 uv = float2(input.uv.x, 1.0f - input.uv.y);
    return texture0.Sample(sampler0, uv);
}

Texture2D fontAtlas : register(t0);
SamplerState fontSampler : register(s0);

struct PS_INPUT
{
    float4 position : SV_POSITION;
    float2 uv       : TEXCOORD0;
    float4 color    : COLOR;
};

float4 PSMain(PS_INPUT input) : SV_TARGET
{
    float4 texColor = fontAtlas.Sample(fontSampler, input.uv);


    float alpha = texColor.a * input.color.a;

    if (alpha < 0.01f)
        discard;

    return float4(input.color.rgb, alpha);
}

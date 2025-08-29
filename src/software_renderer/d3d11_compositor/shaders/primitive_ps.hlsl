struct PS_INPUT
{
    float4 position : SV_POSITION;
    float4 color    : COLOR;
};

float4 PSMain(PS_INPUT input) : SV_TARGET
{
    return input.color;
}
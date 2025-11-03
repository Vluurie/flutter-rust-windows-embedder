cbuffer TimeConstants : register(b1)

{

    float g_time;

};


struct PS_INPUT

{

    float4 position : SV_POSITION;

    float4 color    : COLOR;

    float3 worldPos : TEXCOORD0;

};


float3 mod289(float3 x) {

    return x - floor(x / 289.0) * 289.0;

}


float4 mod289(float4 x) {

    return x - floor(x / 289.0) * 289.0;

}


float4 permute(float4 x) {

    return mod289(((x * 34.0) + 1.0) * x);

}


float4 taylorInvSqrt(float4 r) {

    return 1.79284291400159 - 0.85373472095314 * r;

}


float snoise(float3 v) {

    const float2 C = float2(1.0 / 6.0, 1.0 / 3.0);

    const float4 D = float4(0.0, 0.5, 1.0, 2.0);


    float3 i = floor(v + dot(v, C.yyy));

    float3 x0 = v - i + dot(i, C.xxx);


    float3 g = step(x0.yzx, x0.xyz);

    float3 l = 1.0 - g;

    float3 i1 = min(g.xyz, l.zxy);

    float3 i2 = max(g.xyz, l.zxy);


    float3 x1 = x0 - i1 + C.xxx;

    float3 x2 = x0 - i2 + C.yyy; // 2.0*C.x = 1/3 = C.y

    float3 x3 = x0 - D.yyy;      // -1.0+3.0*C.x = -0.5 = -D.y


    i = mod289(i);

    float4 p = permute(permute(permute(

        i.z + float4(0.0, i1.z, i2.z, 1.0))

        + i.y + float4(0.0, i1.y, i2.y, 1.0))

        + i.x + float4(0.0, i1.x, i2.x, 1.0));


    float4 j = p - D.xxxx;

    float4 n = j * taylorInvSqrt(j * j + 192.0);


    float3 o0 = n.xyz;

    float3 o1 = n.wzxy;

    float3 o2 = n.yxwz;

    float3 o3 = n.zwxy;


    float4 h = max(0.6 - float4(dot(x0, x0), dot(x1, x1), dot(x2, x2), dot(x3, x3)), 0.0);

    float4 b = h * h;

    float4 b2 = b * b;

    float4 sn = b2 * (-8.0 * b + 3.0 * h);


    float4 c = float4(dot(x0, o0), dot(x1, o1), dot(x2, o2), dot(x3, o3));


    return dot(sn, c) * 42.0;

}



float4 PSMain(PS_INPUT input) : SV_TARGET

{

    float3 world_pos = input.worldPos;

    float time = g_time * 0.5;



    float noise = snoise(world_pos * 0.02 + float3(0.0, 0.0, time));

    noise = (noise + 1.0) * 0.5; 


    float arcs = smoothstep(0.7, 0.75, noise) - smoothstep(0.8, 0.85, noise);


    float glow = smoothstep(0.5, 1.0, noise);


    float4 color = input.color;

    color.rgb += arcs * color.rgb * 2.0;

    color.rgb += glow * color.rgb * 0.2;


    return color;

} 
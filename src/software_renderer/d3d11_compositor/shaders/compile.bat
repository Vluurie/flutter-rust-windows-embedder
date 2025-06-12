@echo off

echo:
echo === Passthrough Shader ===
REM 
fxc "fullscreen_quad.hlsl" /nologo /T vs_4_0 /D VS /E VSMain /Fo "fullscreen_quad_vs.cso"

REM 
fxc "fullscreen_quad.hlsl" /nologo /T ps_4_0 /D PS /E PSMain /Fo "passthrough_ps.cso"

echo:
echo === Hologram Shader ===
fxc "hologram_ps.hlsl" /nologo /T ps_4_0 /D PS /E PSMain /Fo "hologram_ps.cso"

echo:
echo === Warp Shader ===
fxc "warp_field_ps.hlsl" /nologo /T ps_4_0 /D PS /E PSMain /Fo "warp_field_ps.cso"
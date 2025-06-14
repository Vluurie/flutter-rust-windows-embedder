use directx_math::{
    XMMatrix, XMMatrixIdentity, XMMatrixMultiply, XMMatrixOrthographicLH, XMMatrixScaling,
    XMMatrixTranslation,
};
use log::info;
use std::{collections::HashMap, mem};
use windows::Win32::Foundation::BOOL;
use windows::Win32::Graphics::Direct3D::D3D11_PRIMITIVE_TOPOLOGY_TRIANGLESTRIP;
use windows::Win32::Graphics::Direct3D11::*;

use crate::software_renderer::d3d11_compositor::effects::{
    EffectConfig, EffectParams, EffectTarget, PostEffect,
};

#[repr(C)]
#[derive(Clone, Copy)]
pub struct GpuParameters {
    pub world_projection: XMMatrix,
    pub iTime: f32,
    pub is_portal_active: u32,
    pub iResolution: [f32; 2],
    pub effect_bounds: [f32; 4],

    // Hologram Shader
    pub aberration_amount: f32,
    pub glitch_speed: f32,
    pub scanline_intensity: f32,
    pub _hologram_padding: f32,

    // TODO: Extract in extra struct
    // Warp Shader
    pub speed: f32,
    pub density: f32,
    pub star_base_size: f32,
    pub glow_falloff: f32,
    pub pulse_speed: f32,
    pub motion_blur_strength: f32,
    pub depth_blur_strength: f32,
    pub base_alpha: f32,
    pub color_inner: [f32; 3],
    pub color_outer: [f32; 3],
    pub color_pulse: [f32; 3],
    pub bloom_threshold: f32,
    pub bloom_intensity: f32,
    // ...
}

#[derive(Clone)]
pub struct D3D11Compositor {
    blend_state: ID3D11BlendState,
    vs: ID3D11VertexShader,
    pixel_shaders: HashMap<PostEffect, ID3D11PixelShader>,
    sampler_state: ID3D11SamplerState,
    parameters_buffer: ID3D11Buffer,
}

impl D3D11Compositor {
    pub fn new(device: &ID3D11Device) -> Self {
        Self {
            blend_state: Self::create_blend_state(device),
            vs: Self::load_vertex_shader(device), // maybe in future
            pixel_shaders: Self::load_pixel_shaders(device),
            sampler_state: Self::create_sampler_state(device),
            parameters_buffer: Self::create_parameters_buffer(device),
        }
    }

    pub fn render_texture(
        &self,
        context: &ID3D11DeviceContext,
        srv: &ID3D11ShaderResourceView,
        config: &EffectConfig,
        x: i32,
        y: i32,
        width: u32,
        height: u32,
        screen_width: f32,
        screen_height: f32,
        time: f32,
    ) {
        let mut gpu_params = GpuParameters {
            world_projection: XMMatrix(XMMatrixIdentity()),
            iTime: time,
            is_portal_active: 0,
            effect_bounds: [0.0; 4],
            aberration_amount: 0.0,
            glitch_speed: 0.0,
            scanline_intensity: 0.0,
            iResolution: [screen_width, screen_height],
            _hologram_padding: 0.0,

            speed: 1.0,
            density: 2.0,
            star_base_size: 0.003,
            glow_falloff: 5.0,
            pulse_speed: 1.8,
            motion_blur_strength: 0.05,
            depth_blur_strength: 0.0005,
            base_alpha: 0.7,
            color_inner: [0.1, 0.2, 0.6],
            color_outer: [0.9, 0.1, 0.8],
            color_pulse: [1.0, 0.7, 0.0],
            bloom_threshold: 0.5,
            bloom_intensity: 0.8,
        };

        let effect_type = match config.params {
            EffectParams::None => PostEffect::Passthrough,
            EffectParams::Hologram(p) => {
                gpu_params.aberration_amount = p.aberration_amount;
                gpu_params.glitch_speed = p.glitch_speed;
                gpu_params.scanline_intensity = p.scanline_intensity;
                PostEffect::Hologram
            }
            EffectParams::WarpField(p) => {
                gpu_params.speed = p.speed;
                gpu_params.density = p.density;
                gpu_params.star_base_size = p.star_base_size;
                gpu_params.glow_falloff = p.glow_falloff;
                gpu_params.pulse_speed = p.pulse_speed;
                gpu_params.motion_blur_strength = p.motion_blur_strength;
                gpu_params.depth_blur_strength = p.depth_blur_strength;
                gpu_params.base_alpha = p.base_alpha;
                gpu_params.color_inner = p.color_inner;
                gpu_params.color_outer = p.color_outer;
                gpu_params.color_pulse = p.color_pulse;
                gpu_params.bloom_threshold = p.bloom_threshold;
                gpu_params.bloom_intensity = p.bloom_intensity;
                PostEffect::WarpField
            }
        };

        if effect_type == PostEffect::Hologram {
            info!(
                "Rendering with params: aberration: {}, speed: {}, intensity: {}",
                gpu_params.aberration_amount,
                gpu_params.glitch_speed,
                gpu_params.scanline_intensity
            );
        }

        if let EffectTarget::Widget(bounds) = config.target {
            gpu_params.is_portal_active = 1;
            gpu_params.effect_bounds = bounds;
        }

        if let EffectTarget::Widget(bounds) = config.target {
            gpu_params.is_portal_active = 1;
            gpu_params.effect_bounds = bounds;
        }

        let pixel_shader = self
            .pixel_shaders
            .get(&effect_type)
            .unwrap_or_else(|| self.pixel_shaders.get(&PostEffect::Passthrough).unwrap());

        unsafe {
            let mut old_blend_state: Option<ID3D11BlendState> = None;
            let mut old_blend_factor = [0.0f32; 4];
            let mut old_sample_mask = 0;
            context.OMGetBlendState(
                Some(&mut old_blend_state),
                Some(&mut old_blend_factor),
                Some(&mut old_sample_mask),
            );

            context.OMSetBlendState(&self.blend_state, None, 0xffffffff);
            context.IASetPrimitiveTopology(D3D11_PRIMITIVE_TOPOLOGY_TRIANGLESTRIP);
            context.VSSetShader(&self.vs, None);
            context.PSSetShader(pixel_shader, None);

            let proj_matrix = XMMatrixOrthographicLH(screen_width, screen_height, 0.0, 1.0);
            let scale_matrix = XMMatrixScaling(width as f32, height as f32, 1.0);
            let translate_matrix = XMMatrixTranslation(x as f32, y as f32, 0.0);
            let world_matrix = XMMatrixMultiply(scale_matrix, &translate_matrix);
            gpu_params.world_projection =
                directx_math::XMMatrix(XMMatrixMultiply(world_matrix, &proj_matrix));

            let mut mapped_resource = D3D11_MAPPED_SUBRESOURCE::default();
            context
                .Map(
                    &self.parameters_buffer,
                    0,
                    D3D11_MAP_WRITE_DISCARD,
                    0,
                    Some(&mut mapped_resource),
                )
                .unwrap();
            *(mapped_resource.pData as *mut GpuParameters) = gpu_params;
            context.Unmap(&self.parameters_buffer, 0);

            context.VSSetConstantBuffers(0, Some(&[Some(self.parameters_buffer.clone())]));
            context.PSSetConstantBuffers(0, Some(&[Some(self.parameters_buffer.clone())]));

            context.PSSetShaderResources(0, Some(&[Some(srv.clone())]));
            context.PSSetSamplers(0, Some(&[Some(self.sampler_state.clone())]));

            context.Draw(4, 0);

            context.OMSetBlendState(
                old_blend_state.as_ref(),
                Some(&old_blend_factor),
                old_sample_mask,
            );
        }
    }

    fn create_blend_state(device: &ID3D11Device) -> ID3D11BlendState {
        let desc = D3D11_BLEND_DESC {
            RenderTarget: [D3D11_RENDER_TARGET_BLEND_DESC {
                BlendEnable: BOOL(1),
                SrcBlend: D3D11_BLEND_SRC_ALPHA,
                DestBlend: D3D11_BLEND_INV_SRC_ALPHA,
                BlendOp: D3D11_BLEND_OP_ADD,
                SrcBlendAlpha: D3D11_BLEND_ONE,
                DestBlendAlpha: D3D11_BLEND_ZERO,
                BlendOpAlpha: D3D11_BLEND_OP_ADD,
                RenderTargetWriteMask: D3D11_COLOR_WRITE_ENABLE_ALL.0 as u8,
            }; 8],
            ..Default::default()
        };
        let mut blend_state: Option<ID3D11BlendState> = None;
        unsafe {
            device
                .CreateBlendState(&desc, Some(&mut blend_state))
                .expect("CreateBlendState failed");
        }
        blend_state.unwrap()
    }

    fn load_vertex_shader(device: &ID3D11Device) -> ID3D11VertexShader {
        let bytes = include_bytes!("./shaders/fullscreen_quad_vs.cso");
        let mut vs: Option<ID3D11VertexShader> = None;
        unsafe {
            device
                .CreateVertexShader(bytes, None, Some(&mut vs))
                .expect("CreateVertexShader failed");
        }
        vs.unwrap()
    }

    fn load_pixel_shaders(device: &ID3D11Device) -> HashMap<PostEffect, ID3D11PixelShader> {
        let mut shaders = HashMap::new();

        let passthrough_bytes = include_bytes!("./shaders/passthrough_ps.cso");
        let mut passthrough_ps: Option<ID3D11PixelShader> = None;
        unsafe {
            device
                .CreatePixelShader(passthrough_bytes, None, Some(&mut passthrough_ps))
                .expect("CreatePixelShader for passthrough failed");
        }
        shaders.insert(PostEffect::Passthrough, passthrough_ps.unwrap());

        let hologram_bytes = include_bytes!("./shaders/hologram_ps.cso");
        let mut hologram_ps: Option<ID3D11PixelShader> = None;
        unsafe {
            device
                .CreatePixelShader(hologram_bytes, None, Some(&mut hologram_ps))
                .expect("CreatePixelShader for hologram failed");
        }
        shaders.insert(PostEffect::Hologram, hologram_ps.unwrap());

        let warp_field_bytes = include_bytes!("./shaders/warp_field_ps.cso");
        let mut warp_field_ps: Option<ID3D11PixelShader> = None;
        unsafe {
            device
                .CreatePixelShader(warp_field_bytes, None, Some(&mut warp_field_ps))
                .expect("CreatePixelShader for warp_field failed");
        }
        shaders.insert(PostEffect::WarpField, warp_field_ps.unwrap());

        shaders
    }

    fn create_sampler_state(device: &ID3D11Device) -> ID3D11SamplerState {
        let desc = D3D11_SAMPLER_DESC {
            Filter: D3D11_FILTER_MIN_MAG_MIP_LINEAR,
            AddressU: D3D11_TEXTURE_ADDRESS_CLAMP,
            AddressV: D3D11_TEXTURE_ADDRESS_CLAMP,
            AddressW: D3D11_TEXTURE_ADDRESS_CLAMP,
            ComparisonFunc: D3D11_COMPARISON_NEVER,
            ..Default::default()
        };
        let mut sampler_state: Option<ID3D11SamplerState> = None;
        unsafe {
            device
                .CreateSamplerState(&desc, Some(&mut sampler_state))
                .expect("CreateSamplerState failed");
        }
        sampler_state.unwrap()
    }

    fn create_parameters_buffer(device: &ID3D11Device) -> ID3D11Buffer {
        let desc = D3D11_BUFFER_DESC {
            ByteWidth: mem::size_of::<GpuParameters>() as u32,
            Usage: D3D11_USAGE_DYNAMIC,
            BindFlags: D3D11_BIND_CONSTANT_BUFFER.0 as u32,
            CPUAccessFlags: D3D11_CPU_ACCESS_WRITE.0 as u32,
            ..Default::default()
        };
        let mut buffer: Option<ID3D11Buffer> = None;
        unsafe {
            device
                .CreateBuffer(&desc, None, Some(&mut buffer))
                .expect("CreateBuffer for parameters failed");
        }
        buffer.unwrap()
    }
}

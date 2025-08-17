use std::ptr;

use windows::Win32::Foundation::HMODULE;
use windows::Win32::Graphics::Direct3D::{D3D_DRIVER_TYPE_HARDWARE, D3D_FEATURE_LEVEL_11_0};
use windows::Win32::Graphics::Direct3D11::{
    D3D11_CREATE_DEVICE_BGRA_SUPPORT, D3D11_CREATE_DEVICE_DEBUG, D3D11_SDK_VERSION,
    D3D11CreateDevice, ID3D11Device,
};
use windows::Win32::Graphics::{
    Direct3D::{D3D_PRIMITIVE_TOPOLOGY, D3D_PRIMITIVE_TOPOLOGY_UNDEFINED},
    Direct3D11::{
        D3D11_COMMONSHADER_CONSTANT_BUFFER_API_SLOT_COUNT,
        D3D11_COMMONSHADER_INPUT_RESOURCE_SLOT_COUNT, D3D11_COMMONSHADER_SAMPLER_SLOT_COUNT,
        D3D11_DEPTH_STENCIL_DESC, D3D11_SIMULTANEOUS_RENDER_TARGET_COUNT, D3D11_VIEWPORT,
        D3D11_VIEWPORT_AND_SCISSORRECT_OBJECT_COUNT_PER_PIPELINE, ID3D11BlendState, ID3D11Buffer,
        ID3D11DepthStencilState, ID3D11DepthStencilView, ID3D11DeviceContext, ID3D11InputLayout,
        ID3D11PixelShader, ID3D11RasterizerState, ID3D11RenderTargetView, ID3D11SamplerState,
        ID3D11ShaderResourceView, ID3D11VertexShader,
    },
};
use windows::core::Result;
pub struct D3D11StateBackup<'a> {
    context: &'a ID3D11DeviceContext,
    num_viewports: u32,
    original_viewports:
        [D3D11_VIEWPORT; D3D11_VIEWPORT_AND_SCISSORRECT_OBJECT_COUNT_PER_PIPELINE as usize],
    original_rtvs:
        [Option<ID3D11RenderTargetView>; D3D11_SIMULTANEOUS_RENDER_TARGET_COUNT as usize],
    original_dsv: Option<ID3D11DepthStencilView>,
    original_rs_state: Option<ID3D11RasterizerState>,
    original_blend_state: Option<ID3D11BlendState>,
    original_blend_factor: [f32; 4],
    original_blend_mask: u32,
    original_depth_state: Option<ID3D11DepthStencilState>,
    original_stencil_ref: u32,
    original_ps_shader: Option<ID3D11PixelShader>,
    original_vs_shader: Option<ID3D11VertexShader>,
    original_primitive_topology: D3D_PRIMITIVE_TOPOLOGY,
    original_input_layout: Option<ID3D11InputLayout>,
    original_vs_constant_buffers:
        [Option<ID3D11Buffer>; D3D11_COMMONSHADER_CONSTANT_BUFFER_API_SLOT_COUNT as usize],
    original_ps_constant_buffers:
        [Option<ID3D11Buffer>; D3D11_COMMONSHADER_CONSTANT_BUFFER_API_SLOT_COUNT as usize],
    original_vs_shader_resources:
        [Option<ID3D11ShaderResourceView>; D3D11_COMMONSHADER_INPUT_RESOURCE_SLOT_COUNT as usize],
    original_ps_shader_resources:
        [Option<ID3D11ShaderResourceView>; D3D11_COMMONSHADER_INPUT_RESOURCE_SLOT_COUNT as usize],
    original_vs_samplers:
        [Option<ID3D11SamplerState>; D3D11_COMMONSHADER_SAMPLER_SLOT_COUNT as usize],
    original_ps_samplers:
        [Option<ID3D11SamplerState>; D3D11_COMMONSHADER_SAMPLER_SLOT_COUNT as usize],
}

impl<'a> D3D11StateBackup<'a> {
    pub fn new(context: &'a ID3D11DeviceContext) -> Self {
        unsafe {
            let mut backup = Self {
                context,
                num_viewports: D3D11_VIEWPORT_AND_SCISSORRECT_OBJECT_COUNT_PER_PIPELINE,
                original_viewports: Default::default(),
                original_rtvs: Default::default(),
                original_dsv: None,
                original_rs_state: None,
                original_blend_state: None,
                original_blend_factor: [0.0; 4],
                original_blend_mask: 0,
                original_depth_state: None,
                original_stencil_ref: 0,
                original_ps_shader: None,
                original_vs_shader: None,
                original_primitive_topology: D3D_PRIMITIVE_TOPOLOGY_UNDEFINED,
                original_input_layout: None,
                original_vs_constant_buffers: Default::default(),
                original_ps_constant_buffers: Default::default(),
                original_vs_samplers: Default::default(),
                original_ps_samplers: Default::default(),
                original_vs_shader_resources: [const { None };
                    D3D11_COMMONSHADER_INPUT_RESOURCE_SLOT_COUNT as usize],
                original_ps_shader_resources: [const { None };
                    D3D11_COMMONSHADER_INPUT_RESOURCE_SLOT_COUNT as usize],
            };

            context.RSGetViewports(
                &mut backup.num_viewports,
                Some(backup.original_viewports.as_mut_ptr()),
            );
            context.OMGetRenderTargets(
                Some(&mut backup.original_rtvs),
                Some(&mut backup.original_dsv),
            );
            context.OMGetBlendState(
                Some(&mut backup.original_blend_state),
                Some(&mut backup.original_blend_factor),
                Some(&mut backup.original_blend_mask),
            );
            context.OMGetDepthStencilState(
                Some(&mut backup.original_depth_state),
                Some(&mut backup.original_stencil_ref),
            );
            context.PSGetShader(&mut backup.original_ps_shader, None, None);
            context.VSGetShader(&mut backup.original_vs_shader, None, None);
            backup.original_rs_state = context.RSGetState().ok();
            backup.original_input_layout = context.IAGetInputLayout().ok();
            backup.original_primitive_topology = context.IAGetPrimitiveTopology();
            context.VSGetConstantBuffers(0, Some(&mut backup.original_vs_constant_buffers));
            context.PSGetConstantBuffers(0, Some(&mut backup.original_ps_constant_buffers));
            context.VSGetShaderResources(0, Some(&mut backup.original_vs_shader_resources));
            context.PSGetShaderResources(0, Some(&mut backup.original_ps_shader_resources));
            context.VSGetSamplers(0, Some(&mut backup.original_vs_samplers));
            context.PSGetSamplers(0, Some(&mut backup.original_ps_samplers));

            backup
        }
    }
}

impl<'a> Drop for D3D11StateBackup<'a> {
    fn drop(&mut self) {
        unsafe {
            self.context.RSSetViewports(Some(
                &self.original_viewports[..self.num_viewports as usize],
            ));
            self.context
                .OMSetRenderTargets(Some(&self.original_rtvs), self.original_dsv.as_ref());
            self.context.OMSetBlendState(
                self.original_blend_state.as_ref(),
                Some(&self.original_blend_factor),
                self.original_blend_mask,
            );
            self.context.OMSetDepthStencilState(
                self.original_depth_state.as_ref(),
                self.original_stencil_ref,
            );
            self.context.RSSetState(self.original_rs_state.as_ref());
            self.context
                .PSSetShader(self.original_ps_shader.as_ref(), None);
            self.context
                .VSSetShader(self.original_vs_shader.as_ref(), None);
            self.context
                .IASetInputLayout(self.original_input_layout.as_ref());
            self.context
                .IASetPrimitiveTopology(self.original_primitive_topology);
            self.context
                .VSSetConstantBuffers(0, Some(&self.original_vs_constant_buffers));
            self.context
                .PSSetConstantBuffers(0, Some(&self.original_ps_constant_buffers));
            self.context
                .VSSetShaderResources(0, Some(&self.original_vs_shader_resources));
            self.context
                .PSSetShaderResources(0, Some(&self.original_ps_shader_resources));
            self.context
                .VSSetSamplers(0, Some(&self.original_vs_samplers));
            self.context
                .PSSetSamplers(0, Some(&self.original_ps_samplers));
        }
    }
}

pub fn is_depth_buffer_disabled(context: &ID3D11DeviceContext) -> bool {
    unsafe {
        let mut ds_state: Option<ID3D11DepthStencilState> = None;
        context.OMGetDepthStencilState(Some(ptr::addr_of_mut!(ds_state)), Some(&mut 0));
        if let Some(state) = ds_state {
            let mut desc: D3D11_DEPTH_STENCIL_DESC = Default::default();
            state.GetDesc(&mut desc);
            !desc.DepthEnable.as_bool()
        } else {
            false
        }
    }
}

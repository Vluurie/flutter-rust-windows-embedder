use directx_math::XMMatrix;
use std::mem;
use windows::Win32::{
    Foundation::BOOL,
    Graphics::{Direct3D::D3D11_PRIMITIVE_TOPOLOGY_TRIANGLELIST, Direct3D11::*},
};

use crate::software_renderer::d3d11_compositor::traits::{FrameParams, Renderer};

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct Vertex3D {
    pub position: [f32; 3],
    pub color: [f32; 4],
}

#[repr(C)]
struct SceneConstants {
    view_projection: XMMatrix,
}

pub struct PrimitiveRendererStates {
    pub blend_state: ID3D11BlendState,
    pub depth_stencil_state: ID3D11DepthStencilState,
}

#[derive(Clone)]
pub struct Primitive3DRenderer {
    vertex_shader: ID3D11VertexShader,
    pixel_shader: ID3D11PixelShader,
    input_layout: ID3D11InputLayout,
    vertex_buffer: ID3D11Buffer,
    constant_buffer: ID3D11Buffer,
    submit_buffer: Vec<Vertex3D>,
    render_buffer: Vec<Vertex3D>,
    buffer_capacity: usize,
    pub blend_state: ID3D11BlendState,
    pub depth_stencil_state: ID3D11DepthStencilState,
}

impl Primitive3DRenderer {
    pub fn new(device: &ID3D11Device) -> Self {
        let vs_bytes = include_bytes!("./shaders/primitive_vs.cso");
        let ps_bytes = include_bytes!("./shaders/primitive_ps.cso");

        let mut vertex_shader: Option<ID3D11VertexShader> = None;
        unsafe {
            device
                .CreateVertexShader(vs_bytes, None, Some(&mut vertex_shader))
                .expect("Failed to create primitive VS");
        }

        let mut pixel_shader: Option<ID3D11PixelShader> = None;
        unsafe {
            device
                .CreatePixelShader(ps_bytes, None, Some(&mut pixel_shader))
                .expect("Failed to create primitive PS");
        }

        let input_element_descs = [
            D3D11_INPUT_ELEMENT_DESC {
                SemanticName: windows::core::PCSTR("POSITION\0".as_ptr()),
                SemanticIndex: 0,
                Format: windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT_R32G32B32_FLOAT,
                InputSlot: 0,
                AlignedByteOffset: 0,
                InputSlotClass: D3D11_INPUT_PER_VERTEX_DATA,
                InstanceDataStepRate: 0,
            },
            D3D11_INPUT_ELEMENT_DESC {
                SemanticName: windows::core::PCSTR("COLOR\0".as_ptr()),
                SemanticIndex: 0,
                Format: windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT_R32G32B32A32_FLOAT,
                InputSlot: 0,
                AlignedByteOffset: 12,
                InputSlotClass: D3D11_INPUT_PER_VERTEX_DATA,
                InstanceDataStepRate: 0,
            },
        ];

        let mut input_layout: Option<ID3D11InputLayout> = None;
        unsafe {
            device
                .CreateInputLayout(&input_element_descs, vs_bytes, Some(&mut input_layout))
                .expect("Failed to create primitive input layout");
        }

        let buffer_capacity = 65536;
        let vertex_buffer_desc = D3D11_BUFFER_DESC {
            ByteWidth: (mem::size_of::<Vertex3D>() * buffer_capacity) as u32,
            Usage: D3D11_USAGE_DYNAMIC,
            BindFlags: D3D11_BIND_VERTEX_BUFFER.0 as u32,
            CPUAccessFlags: D3D11_CPU_ACCESS_WRITE.0 as u32,
            ..Default::default()
        };

        let mut vertex_buffer: Option<ID3D11Buffer> = None;
        unsafe {
            device
                .CreateBuffer(&vertex_buffer_desc, None, Some(&mut vertex_buffer))
                .expect("Failed to create dynamic vertex buffer");
        }

        let constant_buffer_desc = D3D11_BUFFER_DESC {
            ByteWidth: mem::size_of::<SceneConstants>() as u32,
            Usage: D3D11_USAGE_DYNAMIC,
            BindFlags: D3D11_BIND_CONSTANT_BUFFER.0 as u32,
            CPUAccessFlags: D3D11_CPU_ACCESS_WRITE.0 as u32,
            ..Default::default()
        };

        let mut constant_buffer: Option<ID3D11Buffer> = None;
        unsafe {
            device
                .CreateBuffer(&constant_buffer_desc, None, Some(&mut constant_buffer))
                .expect("Failed to create constant buffer");
        }

        let mut blend_desc = D3D11_BLEND_DESC::default();
        let rt_blend_desc = D3D11_RENDER_TARGET_BLEND_DESC {
            BlendEnable: BOOL(1),
            SrcBlend: D3D11_BLEND_ONE,
            DestBlend: D3D11_BLEND_INV_SRC_ALPHA,
            BlendOp: D3D11_BLEND_OP_ADD,
            SrcBlendAlpha: D3D11_BLEND_ONE,
            DestBlendAlpha: D3D11_BLEND_INV_SRC_ALPHA,
            BlendOpAlpha: D3D11_BLEND_OP_ADD,
            RenderTargetWriteMask: D3D11_COLOR_WRITE_ENABLE_ALL.0 as u8,
        };
        blend_desc.RenderTarget[0] = rt_blend_desc;

        let mut blend_state: Option<ID3D11BlendState> = None;
        unsafe {
            device
                .CreateBlendState(&blend_desc, Some(&mut blend_state))
                .expect("Failed to create blend state");
        }

        let mut depth_desc = D3D11_DEPTH_STENCIL_DESC::default();
        depth_desc.DepthEnable = BOOL(0);

        let mut depth_stencil_state: Option<ID3D11DepthStencilState> = None;
        unsafe {
            device
                .CreateDepthStencilState(&depth_desc, Some(&mut depth_stencil_state))
                .expect("Failed to create depth stencil state");
        }

        Self {
            vertex_shader: vertex_shader.unwrap(),
            pixel_shader: pixel_shader.unwrap(),
            input_layout: input_layout.unwrap(),
            vertex_buffer: vertex_buffer.unwrap(),
            constant_buffer: constant_buffer.unwrap(),
            submit_buffer: Vec::with_capacity(buffer_capacity),
            render_buffer: Vec::with_capacity(buffer_capacity),
            buffer_capacity,
            blend_state: blend_state.unwrap(),
            depth_stencil_state: depth_stencil_state.unwrap(),
        }
    }

    pub fn submit_triangles(&mut self, vertices: &[Vertex3D]) {
        self.submit_buffer.clear();
        self.submit_buffer.extend_from_slice(vertices);
    }

    pub fn latch_buffers(&mut self) {
        self.render_buffer = self.submit_buffer.clone();
    }
}

impl Renderer for Primitive3DRenderer {
    fn draw(&mut self, params: &FrameParams) {
        if self.render_buffer.is_empty() {
            return;
        }

        let context = params.context;
        let vertex_count = self.render_buffer.len().min(self.buffer_capacity);

        unsafe {
            let constants = SceneConstants {
                view_projection: *params.view_projection_matrix,
            };
            let mut mapped_cb = D3D11_MAPPED_SUBRESOURCE::default();
            context
                .Map(
                    &self.constant_buffer,
                    0,
                    D3D11_MAP_WRITE_DISCARD,
                    0,
                    Some(&mut mapped_cb),
                )
                .unwrap();
            *(mapped_cb.pData as *mut SceneConstants) = constants;
            context.Unmap(&self.constant_buffer, 0);

            let mut mapped_vb = D3D11_MAPPED_SUBRESOURCE::default();
            context
                .Map(
                    &self.vertex_buffer,
                    0,
                    D3D11_MAP_WRITE_DISCARD,
                    0,
                    Some(&mut mapped_vb),
                )
                .unwrap();

            std::ptr::copy_nonoverlapping(
                self.render_buffer.as_ptr(),
                mapped_vb.pData as *mut Vertex3D,
                vertex_count,
            );
            context.Unmap(&self.vertex_buffer, 0);

            context.IASetInputLayout(&self.input_layout);
            let stride = mem::size_of::<Vertex3D>() as u32;
            let offset = 0;
            context.IASetVertexBuffers(
                0,
                1,
                Some(&Some(self.vertex_buffer.clone())),
                Some(&stride),
                Some(&offset),
            );
            context.IASetPrimitiveTopology(D3D11_PRIMITIVE_TOPOLOGY_TRIANGLELIST);
            context.VSSetShader(&self.vertex_shader, None);
            context.VSSetConstantBuffers(0, Some(&[Some(self.constant_buffer.clone())]));
            context.PSSetShader(&self.pixel_shader, None);

            context.Draw(vertex_count as u32, 0);
        }
    }
}

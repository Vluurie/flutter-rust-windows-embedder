use directx_math::XMMatrix;
use windows::Win32::Graphics::Direct3D11::{ ID3D11DepthStencilView, ID3D11DeviceContext };
pub struct FrameParams<'a> {
    pub context: &'a ID3D11DeviceContext,
    pub view_projection_matrix: &'a XMMatrix,
    pub depth_stencil_view: &'a Option<ID3D11DepthStencilView>,
    pub screen_width: f32,
    pub screen_height: f32,
    pub time: f32,
}

pub trait Renderer {
    fn draw(&mut self, params: &FrameParams);
}

use windows::Win32::Foundation::{BOOL, RECT};
use windows::Win32::Graphics::Direct3D11::{
    D3D11_CULL_NONE, D3D11_FILL_SOLID, D3D11_RASTERIZER_DESC, ID3D11DeviceContext,
    ID3D11RasterizerState,
};

pub struct ScopedRenderState<'a> {
    context: &'a ID3D11DeviceContext,
    original_rs_state: Option<ID3D11RasterizerState>,
    original_scissor_rect: RECT,
    num_original_rects: u32,
}

impl<'a> ScopedRenderState<'a> {
    pub fn new(context: &'a ID3D11DeviceContext, rect: Option<&RECT>) -> Self {
        unsafe {
            let original_rs_state = context.RSGetState().ok();
            let mut num_rects = 1;
            let mut original_scissor_rect = RECT::default();

            context.RSGetScissorRects(&mut num_rects, Some(&mut original_scissor_rect));

            if let Some(r) = rect {
                if let Ok(device) = context.GetDevice() {
                    let rasterizer_desc = D3D11_RASTERIZER_DESC {
                        FillMode: D3D11_FILL_SOLID,
                        CullMode: D3D11_CULL_NONE,
                        ScissorEnable: BOOL(1),
                        ..Default::default()
                    };

                    let mut scissor_rs_state: Option<ID3D11RasterizerState> = None;
                    if device
                        .CreateRasterizerState(&rasterizer_desc, Some(&mut scissor_rs_state))
                        .is_ok()
                    {
                        context.RSSetState(scissor_rs_state.as_ref());
                    }
                }

                context.RSSetScissorRects(Some(std::slice::from_ref(r)));
            }

            Self {
                context,
                original_rs_state,
                original_scissor_rect,
                num_original_rects: num_rects,
            }
        }
    }
}

impl<'a> Drop for ScopedRenderState<'a> {
    fn drop(&mut self) {
        unsafe {
            self.context.RSSetState(self.original_rs_state.as_ref());
            if self.num_original_rects > 0 {
                self.context
                    .RSSetScissorRects(Some(&[self.original_scissor_rect]));
            } else {
                self.context.RSSetScissorRects(None);
            }
        }
    }
}

use std::{ffi::{c_char, c_void, CStr, CString}, ptr};

use log::info;
use windows::Win32::Graphics::Direct3D11::{ID3D11Device, ID3D11DeviceContext, ID3D11ShaderResourceView, ID3D11Texture2D};

use crate::{embedder::{self, FlutterEngine}, software_renderer::ticker};

use super::init;


pub static mut FLUTTER_OVERLAY_RAW_PTR: *mut FlutterOverlay = ptr::null_mut();
static FLUTTER_LOG_TAG: &CStr = unsafe { CStr::from_bytes_with_nul_unchecked(b"rust_embedder\0") };

unsafe extern "C" fn flutter_log_callback(
    tag: *const c_char,
    message: *const c_char,
    _user_data: *mut c_void,
) {
    let tag = unsafe { CStr::from_ptr(tag).to_string_lossy() };
    let msg = unsafe { CStr::from_ptr(message).to_string_lossy() };
    info!("[Flutter][{}] {}", tag, msg);
}

#[derive(Clone)]
#[repr(C)]
pub struct FlutterOverlay {
    pub engine: embedder::FlutterEngine,
    pub pixel_buffer: Vec<u8>,
    pub width: u32,
    pub height: u32,
    pub texture: ID3D11Texture2D,
    pub srv: ID3D11ShaderResourceView,
    pub(crate) _assets_c: CString,
    pub(crate) _icu_c:    CString,
    pub(crate) _argv_cs:  Vec<CString>,
    pub(crate) _aot_c:    Option<CString>,
    pub(crate) _platform_runner_context: Option<Box<ticker::task_scheduler::TaskRunnerContext>>,
    pub(crate) _platform_runner_description: Option<Box<embedder::FlutterTaskRunnerDescription>>,
    pub(crate) _custom_task_runners_struct: Option<Box<embedder::FlutterCustomTaskRunners>>,
}

impl FlutterOverlay {
    pub fn init(
        data_dir: Option<std::path::PathBuf>,
        device: &ID3D11Device,
        width: u32,
        height: u32,
    ) -> Box<Self> {
        init::init_overlay(data_dir, device, width, height)
    }

    pub unsafe fn tick_global(context: &ID3D11DeviceContext) {
        unsafe { crate::software_renderer::ticker::tick_global(context) }
    }

    pub fn get_engine_ptr(&self) -> FlutterEngine {
        self.engine
    }
}

pub extern "C" fn on_present(
    user_data: *mut std::ffi::c_void,
    allocation: *const std::ffi::c_void,
    row_bytes_flutter: usize,
    height_flutter: usize,
) -> bool {
    crate::software_renderer::ticker::on_present(
        user_data,
        allocation,
        row_bytes_flutter,
        height_flutter,
    )
}
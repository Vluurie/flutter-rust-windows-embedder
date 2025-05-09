use std::{ffi::c_void, path::PathBuf, ptr};
use log::info;
use windows::Win32::Graphics::Direct3D11::ID3D11Device;
use crate::software_renderer::overlay::{
    FlutterOverlay, FLUTTER_OVERLAY_RAW_PTR,
};
use crate::software_renderer::overlay::paths::load_flutter_paths;
use crate::software_renderer::overlay::d3d::{create_texture, create_srv};
use crate::software_renderer::overlay::project_args::{build_project_args, maybe_load_aot};
use crate::software_renderer::overlay::renderer::build_software_renderer_config;
use crate::software_renderer::overlay::engine::{run_engine, send_initial_metrics};

const FLUTTER_ENGINE_VERSION: usize = 1;

/// Core init logic, returns the fully‐initialized `FlutterOverlay`.
pub fn init_overlay(
    data_dir: Option<PathBuf>,
    device: &ID3D11Device,
    width: u32,
    height: u32,
) -> FlutterOverlay {
    info!("Initializing FlutterOverlay ({}×{})", width, height);
    assert!(width > 0 && height > 0, "Width and height must be non-zero");

    // load paths
    let (assets, icu, aot_opt) = load_flutter_paths(data_dir);
    info!("Assets: {:?}", assets);
    info!("ICU: {:?}", icu);
    if let Some(ref aot) = aot_opt {
        info!("AOT: {:?}", aot);
    } else {
        info!("No AOT; using JIT mode");
    }

    // create D3D11 resources
    let texture = create_texture(device, width, height);
    let srv     = create_srv(device, &texture);

    // allocate overlay
    let boxed = Box::new(FlutterOverlay {
        engine: ptr::null_mut(),
        pixel_buffer: vec![0; (width as usize) * (height as usize) * 4],
        width,
        height,
        texture,
        srv,
    });
    let raw_ptr = Box::into_raw(boxed);
    unsafe {
        assert!(FLUTTER_OVERLAY_RAW_PTR.is_null(), "Overlay already set");
        FLUTTER_OVERLAY_RAW_PTR = raw_ptr;
    }
    let user_data = raw_ptr as *mut c_void;

    // project args + AOT
    let mut args = build_project_args(&assets.to_string_lossy(), &icu.to_string_lossy());
    maybe_load_aot(&mut args, aot_opt.as_deref());

    // renderer config
    let rdr_cfg = build_software_renderer_config();

    // run engine
    let engine_handle = run_engine(FLUTTER_ENGINE_VERSION, &rdr_cfg, &args, user_data);
    unsafe { (*raw_ptr).engine = engine_handle; }

    // initial metrics
    send_initial_metrics(engine_handle, width as usize, height as usize);

    // clone & return
    unsafe {
        let tmp = Box::from_raw(raw_ptr);
        let result = (*tmp).clone();
        FLUTTER_OVERLAY_RAW_PTR = Box::into_raw(tmp);
        result
    }
}

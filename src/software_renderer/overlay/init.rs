use crate::software_renderer::overlay::platform_message_callback::set_global_engine_for_platform_messages;
use crate::software_renderer::overlay::{
    FlutterOverlay, FLUTTER_OVERLAY_RAW_PTR,
};
use crate::software_renderer::overlay::paths::load_flutter_paths;
use crate::software_renderer::overlay::d3d::{create_texture, create_srv};
use crate::software_renderer::overlay::project_args::{
    build_project_args_and_strings,
    maybe_load_aot,
};
use crate::software_renderer::overlay::renderer::build_software_renderer_config;
use crate::software_renderer::overlay::engine::{run_engine, send_initial_metrics};
use crate::software_renderer::ticker::spawn::spawn_task_runner;
use std::ffi::CString;
use std::{ffi::c_void, path::PathBuf, ptr};
use log::info;
use windows::Win32::Graphics::Direct3D11::ID3D11Device;

const FLUTTER_ENGINE_VERSION: usize = 1;

/// Core init logic; returns a fully‐initialized, CString-backed `FlutterOverlay`.
pub fn init_overlay(
    data_dir: Option<PathBuf>,
    device: &ID3D11Device,
    width: u32,
    height: u32,
) -> FlutterOverlay {
    crate::init_logging();
    info!("Initializing FlutterOverlay ({}×{})", width, height);
    assert!(width > 0 && height > 0, "Width and height must be non-zero");

    // 1) discover paths
    let (assets, icu, aot_opt) = load_flutter_paths(data_dir);
    info!("Assets: {:?}", assets);
    info!("ICU: {:?}", icu);
    if let Some(ref aot) = aot_opt {
        info!("AOT library: {:?}", aot);
    } else {
        info!("No AOT; falling back to JIT mode");
    }

    // 2) create D3D texture & SRV
    let texture = create_texture(device, width, height);
    let srv     = create_srv(device, &texture);

    // 3) allocate overlay on heap and register global pointer
    let boxed = Box::new(FlutterOverlay {
        engine: ptr::null_mut(),
        pixel_buffer: vec![0; (width as usize) * (height as usize) * 4],
        width,
        height,
        texture,
        srv,

        // ← placeholder for CString fields; we’ll overwrite them below
        _assets_c: CString::new("").unwrap(),
        _icu_c:    CString::new("").unwrap(),
        _argv_cs:  Vec::new(),
        _aot_c:    None,
    });
    let raw_ptr = Box::into_raw(boxed);
    unsafe {
        assert!(FLUTTER_OVERLAY_RAW_PTR.is_null(), "Overlay already set");
        FLUTTER_OVERLAY_RAW_PTR = raw_ptr;
    }
    let user_data = raw_ptr as *mut c_void;

    // 4) build project args *and* collect CStrings
    let (mut proj_args, assets_c, icu_c, argv_cs) =
        build_project_args_and_strings(
            &assets.to_string_lossy(),
            &icu.to_string_lossy(),
        );
    let aot_c = maybe_load_aot(&mut proj_args, aot_opt.as_deref());

    // 5) renderer config
    let rdr_cfg = build_software_renderer_config();

    // 6) run the engine
    let engine_handle =
        run_engine(FLUTTER_ENGINE_VERSION, &rdr_cfg, &proj_args, user_data);
    unsafe { (*raw_ptr).engine = engine_handle };

    // 7) send initial window metrics
    send_initial_metrics(engine_handle, width as usize, height as usize);

    unsafe { set_global_engine_for_platform_messages(engine_handle) };

    spawn_task_runner();

    // 8) finally, stash the CString owners into the overlay before returning
    unsafe {
        let mut overlay_box = Box::from_raw(raw_ptr);
        overlay_box._assets_c = assets_c;
        overlay_box._icu_c    = icu_c;
        overlay_box._argv_cs  = argv_cs;
        overlay_box._aot_c    = aot_c;
        let result = overlay_box.clone();
        FLUTTER_OVERLAY_RAW_PTR = Box::into_raw(overlay_box);
        *result
    }
}

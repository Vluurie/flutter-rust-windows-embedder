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
) -> FlutterOverlay { // Returning FlutterOverlay directly as per your original function signature
    // crate::init_logging(); // User's original code had this, ensure it's handled
    info!("Initializing FlutterOverlay ({}x{})", width, height);
    assert!(width > 0 && height > 0, "Width and height must be non-zero");

    // 1) discover paths
    let (assets, icu, aot_opt) = load_flutter_paths(data_dir);
    info!("Assets: {:?}", assets);
    info!("ICU: {:?}", icu);
    if let Some(ref aot) = aot_opt {
        info!("AOT library: {:?}", aot);
    } else {
        info!("No AOT; falling back to JIT mode or embedded snapshots.");
    }

    // 2) create D3D texture & SRV
    let texture = create_texture(device, width, height);
    let srv     = create_srv(device, &texture);

    // 3) allocate overlay on heap with initial placeholders for task runner data
    // The task runner fields are now part of FlutterOverlay struct definition
    let boxed_overlay_with_placeholders = Box::new(FlutterOverlay {
        engine: ptr::null_mut(),
        pixel_buffer: vec![0; (width as usize) * (height as usize) * 4],
        width,
        height,
        texture, // texture from step 2
        srv,     // srv from step 2
        _assets_c: CString::new("").unwrap(), // Placeholder
        _icu_c:    CString::new("").unwrap(), // Placeholder
        _argv_cs:  Vec::new(),               // Placeholder
        _aot_c:    None,                     // Placeholder
        _platform_runner_context: None,
        _platform_runner_description: None,
        _custom_task_runners_struct: None,
    });
    let raw_ptr = Box::into_raw(boxed_overlay_with_placeholders);
    unsafe {
        assert!(FLUTTER_OVERLAY_RAW_PTR.is_null(), "FLUTTER_OVERLAY_RAW_PTR should be null before being set");
        FLUTTER_OVERLAY_RAW_PTR = raw_ptr;
    }
    let user_data = raw_ptr as *mut c_void;

    // 4) build project args *and* collect CStrings AND TASK RUNNER DATA
    // This expects build_project_args_and_strings to return a 7-element tuple.
    // If it still returns a 4-element tuple, this line will cause a "mismatched types" error
    // for the tuple itself.
    let (
        mut proj_args,
        assets_c, 
        icu_c,    
        argv_cs,  
        platform_context_owner,      
        platform_description_owner,  
        custom_runners_struct_owner, 
    ) = build_project_args_and_strings(
        &assets.to_string_lossy(),
        &icu.to_string_lossy(),
    );
    let aot_c = maybe_load_aot(&mut proj_args, aot_opt.as_deref());

    // 5) renderer config
    let rdr_cfg = build_software_renderer_config();

    // 6) run the engine
    let engine_handle = run_engine(
        FLUTTER_ENGINE_VERSION,
        &rdr_cfg,
        &proj_args, 
        user_data, 
    );
    
    unsafe { 
        (*raw_ptr).engine = engine_handle;
    }

    // 7) send initial window metrics
    send_initial_metrics(engine_handle, width as usize, height as usize);

    unsafe { set_global_engine_for_platform_messages(engine_handle) };

    spawn_task_runner();

    // 8) finally, stash the CString owners AND TASK RUNNER DATA into the overlay before returning
    unsafe {
        let mut overlay_box = Box::from_raw(raw_ptr);
        
        overlay_box._assets_c = assets_c;
        overlay_box._icu_c    = icu_c;
        overlay_box._argv_cs  = argv_cs;
        overlay_box._aot_c    = aot_c;

        overlay_box._platform_runner_context = Some(platform_context_owner);
        overlay_box._platform_runner_description = Some(platform_description_owner);
        overlay_box._custom_task_runners_struct = Some(custom_runners_struct_owner);
        
        // If FlutterOverlay implements Clone, and all its fields (including TaskRunnerContext)
        // are Clone, then overlay_box.clone() returns a new Box<FlutterOverlay>.
        let result_overlay_boxed_clone = overlay_box.clone(); 
        
        // FLUTTER_OVERLAY_RAW_PTR should point to the data managed by the original overlay_box,
        // which is consumed by Box::into_raw.
        FLUTTER_OVERLAY_RAW_PTR = Box::into_raw(overlay_box); 
        
        // To return FlutterOverlay from Box<FlutterOverlay>, dereference the box.
        *result_overlay_boxed_clone 
    }
}
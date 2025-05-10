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
// Import the run_engine and send_initial_metrics from your engine module
use crate::software_renderer::overlay::engine::{run_engine, send_initial_metrics}; 
use crate::software_renderer::ticker::spawn::spawn_task_runner;

// Assuming these are the correct paths to your embedder and task_scheduler types
use crate::embedder::{
    FlutterTaskRunnerDescription, 
    FlutterCustomTaskRunners,
    // FlutterEngine is already brought in by crate::embedder via engine.rs
};
use crate::software_renderer::ticker::task_scheduler::TaskRunnerContext;

use std::ffi::CString;
use std::{ffi::c_void, path::PathBuf, ptr};
use log::{info, warn, error}; // Added error for logging
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
    info!("[InitOverlay] Initializing FlutterOverlay ({}x{})", width, height);
    assert!(width > 0 && height > 0, "Width and height must be non-zero");

    // 1) discover paths
    let (assets, icu, aot_opt) = load_flutter_paths(data_dir);
    info!("[InitOverlay] Assets: {:?}", assets);
    info!("[InitOverlay] ICU: {:?}", icu);
    if let Some(ref aot) = aot_opt {
        info!("[InitOverlay] AOT library: {:?}", aot);
    } else {
        info!("[InitOverlay] No AOT; falling back to JIT mode or embedded snapshots.");
    }

    // 2) create D3D texture & SRV
    let texture = create_texture(device, width, height);
    let srv     = create_srv(device, &texture);

    // 3) allocate overlay on heap with initial placeholders for task runner data
    let boxed_overlay_with_placeholders = Box::new(FlutterOverlay {
        engine: ptr::null_mut(),
        pixel_buffer: vec![0; (width as usize) * (height as usize) * 4],
        width,
        height,
        texture, 
        srv,     
        _assets_c: CString::new("").unwrap(), 
        _icu_c:    CString::new("").unwrap(), 
        _argv_cs:  Vec::new(),               
        _aot_c:    None,                     
        _platform_runner_context: None,
        _platform_runner_description: None,
        _custom_task_runners_struct: None,
    });
    let raw_ptr = Box::into_raw(boxed_overlay_with_placeholders);
    unsafe {
        assert!(FLUTTER_OVERLAY_RAW_PTR.is_null(), "FLUTTER_OVERLAY_RAW_PTR should be null before being set");
        FLUTTER_OVERLAY_RAW_PTR = raw_ptr;
    }
    let user_data_for_callbacks = raw_ptr as *mut c_void; // This is passed as user_data to run_engine

    // 4) build project args *and* collect CStrings AND TASK RUNNER DATA
    let (
        mut proj_args,
        assets_c, 
        icu_c,    
        argv_cs,  
        platform_context_owner,      
        platform_description_owner,  
        custom_runners_struct_owner, 
    ) = build_project_args_and_strings( // This must return the 7-tuple
        &assets.to_string_lossy(),
        &icu.to_string_lossy(),
    );
    let aot_c = maybe_load_aot(&mut proj_args, aot_opt.as_deref());

    // 5) renderer config
    let rdr_cfg = build_software_renderer_config();

    // 6) run the engine using the updated run_engine from engine.rs
    info!("[InitOverlay] Calling updated run_engine (staged)...");
    // The `run_engine` function from `engine_rs_staged_startup` artifact expects `overlay_raw_ptr`
    // which is `raw_ptr` in this context.
    let engine_handle_result = run_engine(
        FLUTTER_ENGINE_VERSION,
        &rdr_cfg,
        &proj_args, 
        user_data_for_callbacks, // This is the user_data for FlutterEngineInitialize
        raw_ptr                  // This is the *mut FlutterOverlay for run_engine to set the engine field
    );

    let engine_handle = match engine_handle_result {
        Ok(handle) => {
            info!("[InitOverlay] Engine initialized and run successfully via updated run_engine. Engine handle: {:?}", handle);
            handle
        }
        Err(e) => {
            error!("[InitOverlay] Failed to initialize and run engine via updated run_engine: {}", e);
            unsafe { FLUTTER_OVERLAY_RAW_PTR = ptr::null_mut(); } // Null out global ptr on failure
            // The `boxed_overlay_with_placeholders` (now pointed to by raw_ptr) will be dropped if we panic.
            // We need to reconstruct the box to drop it if not panicking, or ensure it's handled.
            // For now, panicking.
            panic!("Engine initialization failed: {}", e);
        }
    };
    
    // The engine field in the overlay pointed to by raw_ptr should have been set by run_engine
    // No need for: unsafe { (*raw_ptr).engine = engine_handle; } as run_engine does this.

    // 7) send initial window metrics
    send_initial_metrics(engine_handle, width as usize, height as usize);

    // 8) Set global engine for platform messages
    unsafe { set_global_engine_for_platform_messages(engine_handle) };

    // spawn_task_runner() is called *inside* the updated run_engine function.
    // So, no need to call it here directly.

    // 9) finally, stash the CString owners AND TASK RUNNER DATA into the overlay before returning
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
        // Your struct FlutterOverlay derives Clone, so this should work if all fields are Clone.
        let result_overlay_clone = overlay_box.clone(); 
        
        // FLUTTER_OVERLAY_RAW_PTR was already set to raw_ptr.
        // If overlay_box.clone() reallocates, and you want FLUTTER_OVERLAY_RAW_PTR
        // to point to the *original* un-cloned box's data that is about to be consumed by into_raw:
        FLUTTER_OVERLAY_RAW_PTR = Box::into_raw(overlay_box); 
        
        // Return the cloned instance by value.
        *result_overlay_clone 
    }
}

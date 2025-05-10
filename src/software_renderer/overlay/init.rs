use crate::software_renderer::overlay::platform_message_callback::set_global_engine_for_platform_messages;
use crate::software_renderer::overlay::{
    FlutterOverlay, FLUTTER_OVERLAY_RAW_PTR,
};
use crate::software_renderer::overlay::paths::load_flutter_paths;
use crate::software_renderer::overlay::d3d::{create_texture, create_srv};
use crate::software_renderer::overlay::project_args::{
    build_project_args_and_strings, // This MUST return proj_args with .custom_task_runners = null initially
                                    // AND the owned Boxed task runner data separately.
    maybe_load_aot,
};
use crate::software_renderer::overlay::renderer::build_software_renderer_config;
// This MUST refer to the updated run_engine in your engine.rs 
// (from engine_rs_corrected_staged_init artifact) which performs the two-stage init.
use crate::software_renderer::overlay::engine::{run_engine, send_initial_metrics}; 

use crate::embedder::{
    FlutterTaskRunnerDescription, 
    FlutterCustomTaskRunners,
    FlutterProjectArgs, // For modifying proj_args
};
use crate::software_renderer::ticker::task_scheduler::TaskRunnerContext;

use std::ffi::CString;
use std::{ffi::c_void, path::PathBuf, ptr};
use log::{info, warn, error};
use windows::Win32::Graphics::Direct3D11::ID3D11Device;

const FLUTTER_ENGINE_VERSION: usize = 1;

/// Core init logic; returns a fully‐initialized, CString-backed `FlutterOverlay`.
pub fn init_overlay(
    data_dir: Option<PathBuf>,
    device: &ID3D11Device,
    width: u32,
    height: u32,
) -> FlutterOverlay {
    crate::init_logging(); // Ensure this is called once at application startup
    info!("[InitOverlay] Initializing FlutterOverlay ({}x{}) - Entry", width, height); // Log Entry
    assert!(width > 0 && height > 0, "Width and height must be non-zero");

    // 1) discover paths
    info!("[InitOverlay] Step 1: Discovering paths...");
    let (assets, icu, aot_opt) = load_flutter_paths(data_dir);
    info!("[InitOverlay] Assets path: {:?}", assets);
    info!("[InitOverlay] ICU data path: {:?}", icu);
    if let Some(ref aot_path_val) = aot_opt {
        info!("[InitOverlay] AOT library path found: {:?}", aot_path_val);
    } else {
        info!("[InitOverlay] No AOT library path found; proceeding in JIT mode or with embedded snapshots.");
    }
    info!("[InitOverlay] Step 1: Paths discovered.");

    // 2) create D3D texture & SRV
    info!("[InitOverlay] Step 2: Creating D3D texture and SRV...");
    let texture = create_texture(device, width, height);
    let srv     = create_srv(device, &texture);
    info!("[InitOverlay] Step 2: D3D texture and SRV created.");

    // 3) Build project args (custom_task_runners is initially null by build_project_args_and_strings)
    //    and get owned data for CStrings and Task Runners.
    info!("[InitOverlay] Step 3: Building project args and CStrings (custom_task_runners initially null)...");
    let (
        mut proj_args, // Mutable, as we'll set custom_task_runners later
        assets_c, 
        icu_c,    
        argv_cs,  
        platform_context_owner,      
        platform_description_owner,  
        custom_runners_struct_owner, // This is Box<FlutterCustomTaskRunners>
    ) = build_project_args_and_strings(
        &assets.to_string_lossy(),
        &icu.to_string_lossy(),
    );
    info!("[InitOverlay] Step 3: Project args and CStrings built.");
    // At this point, proj_args.custom_task_runners should be ptr::null() as per
    // the updated build_project_args_and_strings.

    // 4) Call maybe_load_aot. 
    // proj_args here does NOT have custom_task_runners set yet.
    // This prevents FlutterEngineCreateAOTData from seeing/using them.
    info!("[InitOverlay] Step 4: Calling maybe_load_aot...");
    let aot_c = maybe_load_aot(&mut proj_args, aot_opt.as_deref());
    info!("[InitOverlay] Step 4: maybe_load_aot completed. AOT CString is Some: {}", aot_c.is_some());
    
    // 5) Allocate FlutterOverlay and store all owned data.
    info!("[InitOverlay] Step 5: Allocating FlutterOverlay instance...");
    let mut overlay_box = Box::new(FlutterOverlay {
        engine: ptr::null_mut(),
        pixel_buffer: vec![0; (width as usize) * (height as usize) * 4],
        width,
        height,
        texture, 
        srv,     
        _assets_c: assets_c,
        _icu_c:    icu_c,
        _argv_cs:  argv_cs,
        _aot_c:    aot_c,
        _platform_runner_context: Some(platform_context_owner),
        _platform_runner_description: Some(platform_description_owner),
        _custom_task_runners_struct: Some(custom_runners_struct_owner), 
    });
    info!("[InitOverlay] Step 5: FlutterOverlay instance allocated.");
    
    let raw_ptr: *mut FlutterOverlay = &mut *overlay_box;
    
    unsafe {
        assert!(FLUTTER_OVERLAY_RAW_PTR.is_null(), "FLUTTER_OVERLAY_RAW_PTR should be null before being set in init_overlay");
        FLUTTER_OVERLAY_RAW_PTR = raw_ptr;
    }
    let user_data_for_engine_callbacks = raw_ptr as *mut c_void; 
    info!("[InitOverlay] FLUTTER_OVERLAY_RAW_PTR set to: {:?}", raw_ptr);

    // 6) Renderer config
    let rdr_cfg = build_software_renderer_config();
    info!("[InitOverlay] Step 6: Renderer config built.");

    // 7) CRITICAL: Set proj_args.custom_task_runners NOW, right before calling run_engine.
    //    We get the pointer from the Box stored in overlay_box.
    if let Some(ref custom_runners_box_ref) = overlay_box._custom_task_runners_struct {
        proj_args.custom_task_runners = &**custom_runners_box_ref as *const FlutterCustomTaskRunners;
        info!("[InitOverlay] Step 7: proj_args.custom_task_runners set to: {:?}", proj_args.custom_task_runners);
    } else {
        error!("[InitOverlay] CRITICAL: _custom_task_runners_struct is None in overlay_box. This should not happen. Panicking.");
        panic!("Task runner setup failed: _custom_task_runners_struct is None in overlay_box");
    }
    
    // 8) Run the engine using the STAGED APPROACH from engine.rs
    //    Your engine::run_engine should perform Initialize -> Spawn Task Runner -> RunInitialized.
    info!("[InitOverlay] Step 8: Calling engine::run_engine (staged)...");
    let engine_handle = match crate::software_renderer::overlay::engine::run_engine(
        FLUTTER_ENGINE_VERSION,
        &rdr_cfg,
        &proj_args, 
        user_data_for_engine_callbacks, 
        raw_ptr 
    ) {
        Ok(handle) => {
            info!("[InitOverlay] engine::run_engine returned successfully. Engine handle: {:?}", handle);
            handle
        }
        Err(e) => {
            error!("[InitOverlay] Failed to initialize and run engine via engine::run_engine: {}", e);
            unsafe { FLUTTER_OVERLAY_RAW_PTR = ptr::null_mut(); }
            panic!("Engine initialization failed during run_engine: {}", e);
        }
    };
    info!("[InitOverlay] Step 8: engine::run_engine completed.");
    
    // 9) Send initial window metrics
    info!("[InitOverlay] Step 9: Sending initial window metrics...");
    send_initial_metrics(engine_handle, width as usize, height as usize);
    info!("[InitOverlay] Step 9: Initial window metrics sent.");

    // 10) Set global engine for platform messages
    unsafe { set_global_engine_for_platform_messages(engine_handle) };
    info!("[InitOverlay] Step 10: Global engine for platform messages set.");

    // spawn_task_runner() is called *inside* the updated engine::run_engine.

    // 11) Return the FlutterOverlay value
    info!("[InitOverlay] Step 11: FlutterOverlay initialization complete. Returning overlay instance.");
    // This moves FlutterOverlay out of the Box. The caller owns the data.
    // FLUTTER_OVERLAY_RAW_PTR will become dangling if the returned value is dropped
    // without updating/clearing FLUTTER_OVERLAY_RAW_PTR.
    *overlay_box 
}

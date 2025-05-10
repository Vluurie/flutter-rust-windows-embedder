use crate::software_renderer::overlay::d3d::{create_srv, create_texture};
use crate::software_renderer::overlay::paths::load_flutter_paths;
use crate::software_renderer::overlay::platform_message_callback::set_global_engine_for_platform_messages;
use crate::software_renderer::overlay::project_args::{
    build_project_args_and_strings, maybe_load_aot,
};
use crate::software_renderer::overlay::renderer::build_software_renderer_config;
use crate::software_renderer::overlay::{FLUTTER_OVERLAY_RAW_PTR, FlutterOverlay};

use crate::software_renderer::overlay::engine::{run_engine, send_initial_metrics};

use crate::embedder::{FlutterCustomTaskRunners, FlutterProjectArgs, FlutterTaskRunnerDescription};
use crate::software_renderer::ticker::task_scheduler::TaskRunnerContext;

use log::{error, info, warn};
use std::ffi::CString;
use std::{ffi::c_void, path::PathBuf, ptr};
use windows::Win32::Graphics::Direct3D11::ID3D11Device;

const FLUTTER_ENGINE_VERSION: usize = 1;

pub fn init_overlay(
    data_dir: Option<PathBuf>,
    device: &ID3D11Device,
    width: u32,
    height: u32,
) -> Box<FlutterOverlay> {
    crate::init_logging();
    info!(
        "[InitOverlay] Initializing FlutterOverlay ({}x{}) - Entry",
        width, height
    );
    assert!(width > 0 && height > 0, "Width and height must be non-zero");

    info!("[InitOverlay] Step 1: Discovering paths...");
    let (assets, icu, aot_opt) = load_flutter_paths(data_dir);
    info!("[InitOverlay] Assets path: {:?}", assets);
    info!("[InitOverlay] ICU data path: {:?}", icu);

    info!("[InitOverlay] Step 1: Paths discovered.");

    info!("[InitOverlay] Step 2: Creating D3D texture and SRV...");
    let texture = create_texture(device, width, height);
    let srv = create_srv(device, &texture);
    info!("[InitOverlay] Step 2: D3D texture and SRV created.");

    info!(
        "[InitOverlay] Step 3: Building project args and CStrings (custom_task_runners initially null)..."
    );
    let (
        mut proj_args,
        assets_c,
        icu_c,
        argv_cs,
        platform_context_owner,
        platform_description_owner,
        custom_runners_struct_owner,
    ) = build_project_args_and_strings(&assets.to_string_lossy(), &icu.to_string_lossy());
    info!("[InitOverlay] Step 3: Project args and CStrings built.");

    info!("[InitOverlay] Step 4: Calling maybe_load_aot...");
    let aot_c = maybe_load_aot(&mut proj_args, aot_opt.as_deref());
    info!(
        "[InitOverlay] Step 4: maybe_load_aot completed. AOT CString is Some: {}",
        aot_c.is_some()
    );

    info!("[InitOverlay] Step 5: Allocating FlutterOverlay instance...");
    let mut overlay_box = Box::new(FlutterOverlay {
        engine: ptr::null_mut(),
        pixel_buffer: vec![0; (width as usize) * (height as usize) * 4],
        width,
        height,
        texture,
        srv,
        _assets_c: assets_c,
        _icu_c: icu_c,
        _argv_cs: argv_cs,
        _aot_c: aot_c,
        _platform_runner_context: Some(platform_context_owner),
        _platform_runner_description: Some(platform_description_owner),
        _custom_task_runners_struct: Some(custom_runners_struct_owner),
    });
    info!("[InitOverlay] Step 5: FlutterOverlay instance allocated.");

    let raw_ptr_to_overlay_data: *mut FlutterOverlay = &mut *overlay_box;

    let user_data_for_engine_callbacks = raw_ptr_to_overlay_data as *mut c_void;
    info!(
        "[InitOverlay] User data for engine callbacks (points to overlay_box data): {:?}",
        user_data_for_engine_callbacks
    );

    let rdr_cfg = build_software_renderer_config();
    info!("[InitOverlay] Step 6: Renderer config built.");

    if let Some(ref custom_runners_box_ref) = overlay_box._custom_task_runners_struct {
        proj_args.custom_task_runners =
            &**custom_runners_box_ref as *const FlutterCustomTaskRunners;
        info!(
            "[InitOverlay] Step 7: proj_args.custom_task_runners set to: {:?}",
            proj_args.custom_task_runners
        );
    } else {
        error!(
            "[InitOverlay] CRITICAL: _custom_task_runners_struct is None in overlay_box. Panicking."
        );
        panic!("Task runner setup failed: _custom_task_runners_struct is None in overlay_box");
    }

    unsafe {
        assert!(
            FLUTTER_OVERLAY_RAW_PTR.is_null(),
            "FLUTTER_OVERLAY_RAW_PTR should be null before being set here"
        );
        FLUTTER_OVERLAY_RAW_PTR = raw_ptr_to_overlay_data;
        info!(
            "[InitOverlay] FLUTTER_OVERLAY_RAW_PTR set to: {:?} (points to overlay_box data)",
            FLUTTER_OVERLAY_RAW_PTR
        );
    }

    info!("[InitOverlay] Step 8: Calling engine::run_engine (staged)...");
    let engine_handle = match crate::software_renderer::overlay::engine::run_engine(
        FLUTTER_ENGINE_VERSION,
        &rdr_cfg,
        &proj_args,
        user_data_for_engine_callbacks,
        raw_ptr_to_overlay_data,
    ) {
        Ok(handle) => {
            info!(
                "[InitOverlay] engine::run_engine returned successfully. Engine handle: {:?}",
                handle
            );
            handle
        }
        Err(e) => {
            error!(
                "[InitOverlay] Failed to initialize and run engine via engine::run_engine: {}",
                e
            );
            unsafe {
                FLUTTER_OVERLAY_RAW_PTR = ptr::null_mut();
            }
            panic!("Engine initialization failed during run_engine: {}", e);
        }
    };

    assert_eq!(
        overlay_box.engine, engine_handle,
        "Engine handle in overlay_box mismatch after run_engine"
    );
    info!("[InitOverlay] Step 8: engine::run_engine completed.");

    info!("[InitOverlay] Step 9: Sending initial window metrics...");
    send_initial_metrics(engine_handle, width as usize, height as usize);
    info!("[InitOverlay] Step 9: Initial window metrics sent.");

    unsafe { set_global_engine_for_platform_messages(engine_handle) };
    info!("[InitOverlay] Step 10: Global engine for platform messages set.");

    info!(
        "[InitOverlay] Step 11: FlutterOverlay initialization complete. Returning Boxed overlay instance."
    );
    overlay_box
}

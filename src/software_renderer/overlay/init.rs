use crate::software_renderer::dynamic_flutter_engine_dll_loader::FlutterEngineDll;
use crate::software_renderer::overlay::d3d::{create_srv, create_texture};
use crate::software_renderer::overlay::engine::{run_engine, update_flutter_window_metrics};
use crate::software_renderer::overlay::overlay_impl::{FLUTTER_LOG_TAG, FLUTTER_OVERLAY_RAW_PTR};
use crate::software_renderer::overlay::paths::load_flutter_paths;
use crate::software_renderer::overlay::platform_message_callback::{set_global_engine_for_platform_messages, simple_platform_message_callback};
use crate::software_renderer::overlay::project_args::{
    build_project_args_and_strings, flutter_log_callback, maybe_load_aot_path_to_cstring
};
use crate::software_renderer::overlay::renderer::build_software_renderer_config;

use crate::embedder::{self, FlutterCustomTaskRunners, FlutterEngineAOTDataSource, FlutterEngineAOTDataSourceType_kFlutterEngineAOTDataSourceTypeElfPath, FlutterEngineResult_kSuccess, FlutterProjectArgs};
use crate::software_renderer::overlay::textinput::text_input_set_global_engine;

use log::{error, info, debug}; 
use std::ffi::{c_char, CString}; 
use std::sync::atomic::{AtomicBool, Ordering};
use std::{ffi::c_void, path::PathBuf, ptr};
use windows::Win32::Graphics::Direct3D11::ID3D11Device;

use super::overlay_impl::FlutterOverlay;

const FLUTTER_ENGINE_VERSION: usize = 1;
pub(crate) static FLUTTER_ASSETS_IS_DEBUG: AtomicBool = AtomicBool::new(false);

pub(crate) fn init_overlay(
    data_dir: Option<PathBuf>,
    device: &ID3D11Device,
    width: u32,
    height: u32,
    dart_args_opt: Option<&[String]>,
) -> Box<FlutterOverlay> {
    unsafe {
        let engine_dll_load_dir = data_dir.as_deref();
        
        info!("[InitOverlay] STEP_001: Attempting to load flutter_engine.dll from `{:?}`...", engine_dll_load_dir);
        let engine_dll_arc = FlutterEngineDll::get_for(engine_dll_load_dir).unwrap_or_else(|e| {
            error!(
                "Failed to load flutter_engine.dll from `{:?}`: {:?}",
                engine_dll_load_dir, e
            );
            std::process::exit(1); 
        });
        
        info!(
            "[InitOverlay] STEP_002: Loaded flutter_engine.dll from `{}`",
            engine_dll_load_dir
                .map(|p| p.display().to_string())
                .unwrap_or_else(|| "EXE folder".into())
        );

        assert!(width > 0 && height > 0, "Width and height must be non-zero");
        info!("[InitOverlay] STEP_003: Assertion for width ({}) and height ({}) passed.", width, height);

        info!("[InitOverlay] STEP_004: Calling load_flutter_paths. data_dir: {:?}", data_dir);
        let (assets, icu, aot_opt) = load_flutter_paths(data_dir.clone());
        
        info!("[InitOverlay] STEP_005: load_flutter_paths completed. Assets: {:?}, ICU: {:?}, AOT data present: {}", assets, icu, aot_opt.is_some());

        info!("[InitOverlay] STEP_006: Calling create_texture. Device Ptr: {:p}, Width: {}, Height: {}", device, width, height);
        let texture = create_texture(device, width, height);
        info!("[InitOverlay] STEP_007: create_texture call completed."); 

        info!("[InitOverlay] STEP_008: Calling create_srv.");
        let srv = create_srv(device, &texture);
        info!("[InitOverlay] STEP_009: create_srv call completed."); 

        info!("[InitOverlay] STEP_010: Calling build_project_args_and_strings. assets_path: '{}', icu_data_path: '{}', dart_args_opt: {:?}",
            assets.to_string_lossy(),
            icu.to_string_lossy(),
            dart_args_opt
        );
        let (
            assets_c_temp,
            icu_c_temp,
            engine_argv_cs_temp,
            dart_argv_cs_temp,
            platform_context_owner_temp,
            platform_description_owner_temp,
            custom_runners_struct_owner_temp,
        ) = build_project_args_and_strings(
            &assets.to_string_lossy(),
            &icu.to_string_lossy(),
            dart_args_opt,
        );
        info!("[InitOverlay] STEP_011: build_project_args_and_strings call completed. Dart CStrings count: {}", dart_argv_cs_temp.len());

        info!("[InitOverlay] STEP_012: Calling maybe_load_aot_path_to_cstring. AOT data source path: {:?}", aot_opt.as_deref().map(|p| p));
        let aot_c_temp = maybe_load_aot_path_to_cstring(aot_opt.as_deref());
        info!("[InitOverlay] STEP_013: maybe_load_aot_path_to_cstring call completed. aot_c_temp created: {}", aot_c_temp.is_some());

        info!("[InitOverlay] STEP_014: Creating FlutterOverlay box. Pixel buffer size: {} bytes", (width as usize) * (height as usize) * 4);
        let mut overlay_box = Box::new(FlutterOverlay {
            engine: ptr::null_mut(),
            pixel_buffer: vec![0; (width as usize) * (height as usize) * 4],
            width,
            height,
            texture, 
            srv,     
            _assets_c: assets_c_temp, 
            _icu_c: icu_c_temp,       
            _engine_argv_cs: engine_argv_cs_temp, 
            _dart_argv_cs: dart_argv_cs_temp,     
            _aot_c: aot_c_temp,                   
            _platform_runner_context: Some(platform_context_owner_temp), 
            _platform_runner_description: Some(platform_description_owner_temp), 
            _custom_task_runners_struct: Some(custom_runners_struct_owner_temp), 
            engine_dll: engine_dll_arc.clone(), 
        });
        info!("[InitOverlay] STEP_015: FlutterOverlay box created.");

        info!("[InitOverlay] STEP_016: Preparing CString argument pointers.");
        let engine_argv_ptrs: Vec<*const c_char> = overlay_box._engine_argv_cs.iter().map(|c| c.as_ptr()).collect();
        let dart_argv_ptrs: Vec<*const c_char> = overlay_box._dart_argv_cs.iter().map(|c| c.as_ptr()).collect();
        info!("[InitOverlay] STEP_017: CString argument pointers prepared. Engine args count: {}, Dart args count: {}", engine_argv_ptrs.len(), dart_argv_ptrs.len());

        info!("[InitOverlay] STEP_018: Initializing FlutterProjectArgs struct.");
        let mut proj_args = FlutterProjectArgs {
            struct_size: std::mem::size_of::<FlutterProjectArgs>(),
            assets_path: overlay_box._assets_c.as_ptr(),
            icu_data_path: overlay_box._icu_c.as_ptr(),
            command_line_argc: engine_argv_ptrs.len() as i32,
            command_line_argv: if engine_argv_ptrs.is_empty() { ptr::null() } else { engine_argv_ptrs.as_ptr() },
            platform_message_callback: Some(simple_platform_message_callback),
            log_message_callback: Some(flutter_log_callback),
            log_tag: FLUTTER_LOG_TAG.as_ptr(),
            
            custom_task_runners: ptr::null(), 
            aot_data: ptr::null_mut(), 
            dart_entrypoint_argc: dart_argv_ptrs.len() as i32,
            dart_entrypoint_argv: if dart_argv_ptrs.is_empty() { ptr::null() } else { dart_argv_ptrs.as_ptr() },
            shutdown_dart_vm_when_done: true,
            dart_old_gen_heap_size: -1,
            
            vm_snapshot_data: ptr::null(),
            vm_snapshot_data_size: 0,
            vm_snapshot_instructions: ptr::null(),
            vm_snapshot_instructions_size: 0,
            isolate_snapshot_data: ptr::null(),
            isolate_snapshot_data_size: 0,
            isolate_snapshot_instructions: ptr::null(),
            isolate_snapshot_instructions_size: 0,
            root_isolate_create_callback: None,
            update_semantics_node_callback: None,
            update_semantics_custom_action_callback: None,
            persistent_cache_path: ptr::null(),
            is_persistent_cache_read_only: false,
            vsync_callback: None,
            custom_dart_entrypoint: ptr::null(),
            compositor: ptr::null(),
            compute_platform_resolved_locale_callback: None,
            on_pre_engine_restart_callback: None,
            update_semantics_callback: None, 
            update_semantics_callback2: None, 
            channel_update_callback: None,
            main_path__unused__: ptr::null(),
            packages_path__unused__: ptr::null()
        };
        info!("[InitOverlay] STEP_019: FlutterProjectArgs struct initialized with basic values.");
        debug!("[InitOverlay] STEP_019_DEBUG: proj_args.assets_path: {:p}, proj_args.icu_data_path: {:p}, proj_args.dart_entrypoint_argv: {:p}",
            proj_args.assets_path, proj_args.icu_data_path, proj_args.dart_entrypoint_argv);


        info!("[InitOverlay] STEP_020: Processing AOT data if available.");
        if let Some(aot_c) = &overlay_box._aot_c {
            info!("[InitOverlay] STEP_020a: AOT CString found: {:?}", aot_c.to_string_lossy());
            let source = FlutterEngineAOTDataSource {
                type_: FlutterEngineAOTDataSourceType_kFlutterEngineAOTDataSourceTypeElfPath,
                __bindgen_anon_1: embedder::FlutterEngineAOTDataSource__bindgen_ty_1 {
                    elf_path: aot_c.as_ptr(),
                },
            };
            let res = (overlay_box.engine_dll.FlutterEngineCreateAOTData)(&source, &mut proj_args.aot_data);
            if res != FlutterEngineResult_kSuccess {
                error!("[InitOverlay] FlutterEngineCreateAOTData failed with result {:?}, for AOT path: {}", res, aot_c.to_string_lossy());
                proj_args.aot_data = ptr::null_mut(); 
                overlay_box._aot_c = None; 
            } else {
                info!("[InitOverlay] FlutterEngineCreateAOTData successful for AOT path: {}. proj_args.aot_data set to {:p}", aot_c.to_string_lossy(), proj_args.aot_data);
            }
        } else {
            info!("[InitOverlay] STEP_020b: No AOT CString (_aot_c is None). Setting proj_args.aot_data to null.");
            proj_args.aot_data = ptr::null_mut();
        }
        info!("[InitOverlay] STEP_021: AOT data processing finished. proj_args.aot_data is null: {}", proj_args.aot_data.is_null());

        info!("[InitOverlay] STEP_022: Setting FLUTTER_ASSETS_IS_DEBUG based on AOT status.");
        if overlay_box._aot_c.is_none() { 
            FLUTTER_ASSETS_IS_DEBUG.store(true, Ordering::SeqCst);
            info!("[InitOverlay] STEP_022a: AOT data not used or failed. FLUTTER_ASSETS_IS_DEBUG set to true.");
        } else {
            FLUTTER_ASSETS_IS_DEBUG.store(false, Ordering::SeqCst);
            info!("[InitOverlay] STEP_022b: AOT data likely used. FLUTTER_ASSETS_IS_DEBUG set to false.");
        }

        info!("[InitOverlay] STEP_023: Setting custom_task_runners for FlutterProjectArgs.");
        if let Some(ref custom_runners_box_ref) = overlay_box._custom_task_runners_struct {
            proj_args.custom_task_runners = &**custom_runners_box_ref as *const FlutterCustomTaskRunners;
            info!("[InitOverlay] STEP_023a: Custom task runners set in proj_args to {:p}", proj_args.custom_task_runners);
        } else {
            
            
            error!("[InitOverlay] CRITICAL_ERROR: _custom_task_runners_struct became None unexpectedly before setting proj_args.custom_task_runners. This should not happen.");
            panic!("Task runner setup failed: _custom_task_runners_struct is None during proj_args population");
        }
        info!("[InitOverlay] STEP_024: Custom task runners setup finished.");

        info!("[InitOverlay] STEP_025: Setting FLUTTER_OVERLAY_RAW_PTR.");
        let raw_ptr_to_overlay_data: *mut FlutterOverlay = &mut *overlay_box;
        
        assert!(FLUTTER_OVERLAY_RAW_PTR.is_null(), "FLUTTER_OVERLAY_RAW_PTR was not null before assignment!");
        FLUTTER_OVERLAY_RAW_PTR = raw_ptr_to_overlay_data;
        info!("[InitOverlay] STEP_026: FLUTTER_OVERLAY_RAW_PTR set to {:p}", FLUTTER_OVERLAY_RAW_PTR);

        info!("[InitOverlay] STEP_027: Building software renderer configuration.");
        let rdr_cfg = build_software_renderer_config();
        info!("[InitOverlay] STEP_028: Software renderer configuration built."); 

        info!("[InitOverlay] STEP_029: Preparing user_data for engine callbacks (pointer to overlay_box).");
        let user_data_for_engine_callbacks = raw_ptr_to_overlay_data as *mut c_void;
        info!("[InitOverlay] STEP_030: User_data for engine callbacks prepared: {:p}", user_data_for_engine_callbacks);

        info!("[InitOverlay] STEP_031: Calling run_engine (FlutterEngineRun).");
        let engine_run_result = run_engine(
            FLUTTER_ENGINE_VERSION,
            &rdr_cfg,
            &proj_args,
            user_data_for_engine_callbacks, 
            raw_ptr_to_overlay_data, 
            overlay_box.engine_dll.clone()
        );
        info!("[InitOverlay] STEP_032: run_engine call finished. Result is_ok: {}", engine_run_result.is_ok());

        let engine_handle = match engine_run_result {
            Ok(handle) => {
                info!(
                    "[InitOverlay] STEP_032a: engine::run_engine returned successfully. Engine handle: {:?}",
                    handle
                );
                handle
            }
            Err(e) => {
                error!(
                    "[InitOverlay] STEP_032b: Failed to initialize and run engine via engine::run_engine: {}",
                    e
                );
                FLUTTER_OVERLAY_RAW_PTR = ptr::null_mut(); 
                panic!("Engine initialization failed during run_engine: {}", e);
            }
        };
        info!("[InitOverlay] STEP_033: Engine handle obtained: {:?}", engine_handle);

        overlay_box.engine = engine_handle;
        info!("[InitOverlay] STEP_034: Engine handle stored in overlay_box.");

        assert_eq!(
            overlay_box.engine, engine_handle,
            "Engine handle in overlay_box mismatch after run_engine"
        );
        info!("[InitOverlay] STEP_035: Assertion for engine handle match passed.");

        info!("[InitOverlay] STEP_036: Calling update_flutter_window_metrics.");
        update_flutter_window_metrics(engine_handle, width, height, overlay_box.engine_dll.clone());
        info!("[InitOverlay] STEP_037: update_flutter_window_metrics call completed.");

        info!("[InitOverlay] STEP_038: Calling set_global_engine_for_platform_messages.");
        set_global_engine_for_platform_messages(engine_handle, overlay_box.engine_dll.clone());
        info!("[InitOverlay] STEP_039: set_global_engine_for_platform_messages call completed.");

        info!("[InitOverlay] STEP_040: Calling text_input_set_global_engine.");
        text_input_set_global_engine(engine_handle, overlay_box.engine_dll.clone());
        info!("[InitOverlay] STEP_041: text_input_set_global_engine call completed.");

        info!("[InitOverlay] STEP_042: init_overlay function finished successfully. Returning overlay_box.");
        overlay_box
    }
}
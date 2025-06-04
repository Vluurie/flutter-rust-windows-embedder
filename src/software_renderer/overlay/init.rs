use crate::software_renderer::dynamic_flutter_engine_dll_loader::FlutterEngineDll;
use crate::software_renderer::overlay::d3d::{create_srv, create_texture};
use crate::software_renderer::overlay::engine::{run_engine, update_flutter_window_metrics};
use crate::software_renderer::overlay::overlay_impl::FLUTTER_LOG_TAG;
use crate::software_renderer::overlay::paths::load_flutter_paths;
use crate::software_renderer::overlay::platform_message_callback::simple_platform_message_callback;
use crate::software_renderer::overlay::project_args::{
    build_project_args_and_strings, flutter_log_callback, maybe_load_aot_path_to_cstring,
};
use crate::software_renderer::overlay::renderer::build_software_renderer_config;

use crate::embedder::{
    self, FlutterCustomTaskRunners, FlutterEngineAOTDataSource,
    FlutterEngineAOTDataSourceType_kFlutterEngineAOTDataSourceTypeElfPath,
    FlutterEngineResult_kSuccess, FlutterProjectArgs, FlutterTaskRunnerDescription,
};
use crate::software_renderer::overlay::semantics_handler::semantics_update_callback;
use crate::software_renderer::ticker::spawn::start_task_runner;
use crate::software_renderer::ticker::task_scheduler::{
    TaskQueueState, TaskRunnerContext, destroy_task_runner_context_callback, post_task_callback,
    runs_task_on_current_thread_callback,
};

use log::{error, info};
use std::collections::HashMap;
use std::ffi::c_char;
use std::sync::atomic::{AtomicBool, AtomicI32, AtomicPtr, Ordering};
use std::sync::{Arc, Mutex};
use std::{ffi::c_void, path::PathBuf, ptr};
use windows::Win32::Graphics::Direct3D11::ID3D11Device;
use windows::Win32::Graphics::Dxgi::IDXGISwapChain;

use super::overlay_impl::FlutterOverlay;

const FLUTTER_ENGINE_VERSION: usize = 1;

pub(crate) fn init_overlay(
    name: String,
    data_dir: Option<PathBuf>,
    device: &ID3D11Device,
    swap_chain: &IDXGISwapChain,
    width: u32,
    height: u32,
    dart_args_opt: Option<&[String]>,
) -> Box<FlutterOverlay> {
    unsafe {
        let engine_dll_load_dir = data_dir.as_deref();
        let engine_dll_arc = FlutterEngineDll::get_for(engine_dll_load_dir).unwrap_or_else(|e| {
            error!(
                "Failed to load flutter_engine.dll from `{:?}`: {:?}",
                engine_dll_load_dir, e
            );
            std::process::exit(1);
        });

        assert!(width > 0 && height > 0, "Width and height must be non-zero");

        let (assets, icu, aot_opt) = load_flutter_paths(data_dir.clone());
        let initial_is_debug = aot_opt.is_none();

        let current_texture = create_texture(device, width, height);
        let current_srv = create_srv(device, &current_texture);

        let (assets_c_temp, icu_c_temp, engine_argv_cs_temp, dart_argv_cs_temp) =
            build_project_args_and_strings(
                &assets.to_string_lossy(),
                &icu.to_string_lossy(),
                dart_args_opt,
                initial_is_debug,
            );

        let aot_c_temp = maybe_load_aot_path_to_cstring(aot_opt.as_deref());

        let engine_atomic_ptr_instance = Arc::new(AtomicPtr::new(ptr::null_mut()));
        let task_queue_arc = Arc::new(TaskQueueState::new());

        let platform_context_owned_by_overlay = Box::new(TaskRunnerContext {
            task_runner_thread_id: None,
            task_queue: task_queue_arc.clone(),
        });

        let mut overlay_box = Box::new(FlutterOverlay {
            name: name,
            engine: ptr::null_mut(),
            engine_atomic_ptr: engine_atomic_ptr_instance.clone(),
            pixel_buffer: vec![0; (width * height * 4) as usize],
            width,
            height,
            texture: current_texture,
            srv: current_srv,
            desired_cursor: Arc::new(Mutex::new(None)),
            task_queue_state: task_queue_arc,
            task_runner_thread: None,
            _assets_c: assets_c_temp,
            _icu_c: icu_c_temp,
            _engine_argv_cs: engine_argv_cs_temp,
            _dart_argv_cs: dart_argv_cs_temp,
            _aot_c: aot_c_temp,

            _platform_runner_context: Some(platform_context_owned_by_overlay),
            _platform_runner_description: None,
            _custom_task_runners_struct: None,
            engine_dll: engine_dll_arc.clone(),
            text_input_state: Arc::new(Mutex::new(None)),
            mouse_buttons_state: AtomicI32::new(0),
            is_mouse_added: AtomicBool::new(false),
            semantics_tree_data: Arc::new(Mutex::new(HashMap::new())),
            is_interactive_widget_hovered: AtomicBool::new(false),
            windows_handler: {
                let mut desc = windows::Win32::Graphics::Dxgi::DXGI_SWAP_CHAIN_DESC::default();
                swap_chain
                    .GetDesc(&mut desc)
                    .expect("Failed to get swap chain description");
                desc.OutputWindow
            },
            is_debug_build: initial_is_debug,
        });

        start_task_runner(&mut overlay_box);

        let raw_ptr_to_overlay_data: *mut FlutterOverlay = &mut *overlay_box;
        let user_data_for_callbacks = raw_ptr_to_overlay_data as *mut c_void;

        let platform_description_box = Box::new(FlutterTaskRunnerDescription {
            struct_size: std::mem::size_of::<FlutterTaskRunnerDescription>(),

            user_data: overlay_box
                ._platform_runner_context
                .as_ref()
                .unwrap()
                .as_ref() as *const _ as *mut c_void,
            runs_task_on_current_thread_callback: Some(runs_task_on_current_thread_callback),
            post_task_callback: Some(post_task_callback),
            identifier: 1,
            destruction_callback: Some(destroy_task_runner_context_callback),
        });

        let custom_task_runners_box = Box::new(FlutterCustomTaskRunners {
            struct_size: std::mem::size_of::<FlutterCustomTaskRunners>(),
            platform_task_runner: &*platform_description_box,
            render_task_runner: &*platform_description_box,
            thread_priority_setter: None,
        });

        overlay_box._platform_runner_description = Some(platform_description_box);
        overlay_box._custom_task_runners_struct = Some(custom_task_runners_box);

        let engine_argv_ptrs: Vec<*const c_char> = overlay_box
            ._engine_argv_cs
            .iter()
            .map(|c| c.as_ptr())
            .collect();
        let dart_argv_ptrs: Vec<*const c_char> = overlay_box
            ._dart_argv_cs
            .iter()
            .map(|c| c.as_ptr())
            .collect();

        let mut proj_args = FlutterProjectArgs {
            struct_size: std::mem::size_of::<FlutterProjectArgs>(),
            assets_path: overlay_box._assets_c.as_ptr(),
            icu_data_path: overlay_box._icu_c.as_ptr(),
            command_line_argc: engine_argv_ptrs.len() as i32,
            command_line_argv: if engine_argv_ptrs.is_empty() {
                ptr::null()
            } else {
                engine_argv_ptrs.as_ptr()
            },
            platform_message_callback: Some(simple_platform_message_callback),
            log_message_callback: Some(flutter_log_callback),
            log_tag: FLUTTER_LOG_TAG.as_ptr(),
            custom_task_runners: overlay_box
                ._custom_task_runners_struct
                .as_ref()
                .map_or(ptr::null(), |b| &**b as *const _),
            aot_data: ptr::null_mut(),
            dart_entrypoint_argc: dart_argv_ptrs.len() as i32,
            dart_entrypoint_argv: if dart_argv_ptrs.is_empty() {
                ptr::null()
            } else {
                dart_argv_ptrs.as_ptr()
            },
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
            update_semantics_callback2: Some(semantics_update_callback),
            channel_update_callback: None,
            main_path__unused__: ptr::null(),
            packages_path__unused__: ptr::null(),
        };

        if let Some(aot_c_ref) = &overlay_box._aot_c {
            let source = FlutterEngineAOTDataSource {
                type_: FlutterEngineAOTDataSourceType_kFlutterEngineAOTDataSourceTypeElfPath,
                __bindgen_anon_1: embedder::FlutterEngineAOTDataSource__bindgen_ty_1 {
                    elf_path: aot_c_ref.as_ptr(),
                },
            };
            let res = (overlay_box.engine_dll.FlutterEngineCreateAOTData)(
                &source,
                &mut proj_args.aot_data,
            );
            if res != FlutterEngineResult_kSuccess {
                error!(
                    "[InitOverlay] FlutterEngineCreateAOTData failed: {:?}, for AOT: {}",
                    res,
                    aot_c_ref.to_string_lossy()
                );
                proj_args.aot_data = ptr::null_mut();
            } else {
                info!(
                    "[InitOverlay] AOTData successful: {}, {:p}",
                    aot_c_ref.to_string_lossy(),
                    proj_args.aot_data
                );
            }
        } else {
            proj_args.aot_data = ptr::null_mut();
        }

        overlay_box.is_debug_build = proj_args.aot_data.is_null();

        let rdr_cfg = build_software_renderer_config();
        let engine_run_result = run_engine(
            FLUTTER_ENGINE_VERSION,
            &rdr_cfg,
            &proj_args,
            user_data_for_callbacks,
            raw_ptr_to_overlay_data,
            overlay_box.engine_dll.clone(),
        );

        let engine_handle = match engine_run_result {
            Ok(handle) => handle,
            Err(e) => {
                engine_atomic_ptr_instance.store(ptr::null_mut(), Ordering::SeqCst);
                panic!("Engine initialization failed during run_engine: {}", e);
            }
        };

        (engine_dll_arc.FlutterEngineUpdateSemanticsEnabled)(engine_handle, true);

        overlay_box.engine = engine_handle;
        engine_atomic_ptr_instance.store(engine_handle, Ordering::SeqCst);

        assert_eq!(overlay_box.engine, engine_handle, "Engine handle mismatch");
        update_flutter_window_metrics(engine_handle, width, height, overlay_box.engine_dll.clone());

        overlay_box
    }
}

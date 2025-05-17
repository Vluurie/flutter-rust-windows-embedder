use crate::embedder::{
    self,
    FlutterProjectArgs,
    FlutterTaskRunnerDescription,
    FlutterCustomTaskRunners,
    FlutterEngineAOTDataSource,
    FlutterEngineAOTDataSourceType_kFlutterEngineAOTDataSourceTypeElfPath,
    FlutterEngineResult_kSuccess,
};
use crate::software_renderer::overlay::platform_message_callback::simple_platform_message_callback;
use crate::software_renderer::ticker::task_scheduler::{
    post_task_callback,
    runs_task_on_current_thread_callback,
    destroy_task_runner_context_callback,
    TaskRunnerContext,
};

use std::ffi::{CString, OsStr, c_void, CStr};
use std::ptr;
use log::{error, info};

const ARGS: &[&str] = &[
    "flutter_rust_embedder_app",
    "--enable-software-rendering",
    "--skia-deterministic-rendering",
    "--verbose-system-logs",
    "--show-performance-overlay",
    "--disable-service-auth-codes",
    "--observatory-port=8801",                 
];


static FLUTTER_LOG_TAG: &CStr = unsafe { CStr::from_bytes_with_nul_unchecked(b"rust_embedder\0") };

#[unsafe(no_mangle)]
unsafe extern "C" fn flutter_log_callback(
    tag: *const std::os::raw::c_char,
    message: *const std::os::raw::c_char,
    _user_data: *mut c_void,
) {
    let tag_str = if tag.is_null() {
        FLUTTER_LOG_TAG.to_string_lossy().into_owned()
    } else {
        unsafe { CStr::from_ptr(tag).to_string_lossy().into_owned() }
    };
    let msg_str = if message.is_null() {
        ""
    } else {
        unsafe { &CStr::from_ptr(message).to_string_lossy().into_owned() }
    };
    info!("[Flutter][{}] {}", tag_str, msg_str);
}

fn create_task_runner_description_with_context(
    identifier: usize,
) -> (FlutterTaskRunnerDescription, Box<TaskRunnerContext>) {
    let context = Box::new(TaskRunnerContext {
        task_runner_thread_id: None,
    });
    let context_ptr = Box::into_raw(context);

    let description = FlutterTaskRunnerDescription {
        struct_size: std::mem::size_of::<FlutterTaskRunnerDescription>(),
        user_data: context_ptr as *mut c_void,
        runs_task_on_current_thread_callback: Some(runs_task_on_current_thread_callback),
        post_task_callback: Some(post_task_callback),
        identifier,
        destruction_callback: Some(destroy_task_runner_context_callback),
    };
    (description, unsafe { Box::from_raw(context_ptr) })
}

pub fn build_project_args_and_strings(
    assets: &str,
    icu: &str,
) -> (
    FlutterProjectArgs,                // args.custom_task_runners will be null initially
    CString,                           // assets_c
    CString,                           // icu_c
    Vec<CString>,                      // argv_cs
    Box<TaskRunnerContext>,            
    Box<FlutterTaskRunnerDescription>, 
    Box<FlutterCustomTaskRunners>,     
) {
    let assets_c = CString::new(assets).expect("Failed to convert assets path to CString");
    let icu_c = CString::new(icu).expect("Failed to convert icu data path to CString");
    let argv_cs: Vec<CString> = ARGS.iter().map(|&s| CString::new(s).unwrap()).collect();
    let argv_ptrs: Vec<*const std::os::raw::c_char> = argv_cs.iter().map(|c| c.as_ptr()).collect();

    let (platform_desc_st, platform_context_owner) =
        create_task_runner_description_with_context(1);
    let platform_runner_description_owner = Box::new(platform_desc_st);
    let custom_task_runners_owner = Box::new(FlutterCustomTaskRunners {
        struct_size: std::mem::size_of::<FlutterCustomTaskRunners>(),
        platform_task_runner: &*platform_runner_description_owner as *const FlutterTaskRunnerDescription,
        render_task_runner: &*platform_runner_description_owner as *const FlutterTaskRunnerDescription,
        thread_priority_setter: None,
    });

    let args = FlutterProjectArgs {
        struct_size: std::mem::size_of::<FlutterProjectArgs>(),
        assets_path: assets_c.as_ptr(),
        icu_data_path: icu_c.as_ptr(),
        command_line_argc: argv_ptrs.len() as i32,
        command_line_argv: argv_ptrs.as_ptr(),
        platform_message_callback: Some(simple_platform_message_callback),
        log_message_callback: Some(flutter_log_callback),
        log_tag: FLUTTER_LOG_TAG.as_ptr(),        
        custom_task_runners: ptr::null(),
        vm_snapshot_data: ptr::null(),
        vm_snapshot_data_size: 0,
        vm_snapshot_instructions: ptr::null(),
        vm_snapshot_instructions_size: 0,
        isolate_snapshot_data: ptr::null(),
        isolate_snapshot_data_size: 0,
        isolate_snapshot_instructions: ptr::null(),
        isolate_snapshot_instructions_size: 0,
        aot_data: ptr::null_mut(),
        root_isolate_create_callback: None,
        update_semantics_node_callback: None,
        update_semantics_custom_action_callback: None,
        persistent_cache_path: ptr::null(),
        is_persistent_cache_read_only: false,
        vsync_callback: None,
        custom_dart_entrypoint: ptr::null(),
        shutdown_dart_vm_when_done: true,
        compositor: ptr::null(),
        dart_old_gen_heap_size: -1,
        compute_platform_resolved_locale_callback: None,
        dart_entrypoint_argc: 0,
        dart_entrypoint_argv: ptr::null(),
        on_pre_engine_restart_callback: None,
        update_semantics_callback: None,
        update_semantics_callback2: None,
        channel_update_callback: None,
        main_path__unused__:  ptr::null(),
        packages_path__unused__:  ptr::null()
    };

    (
        args,
        assets_c,
        icu_c,
        argv_cs,
        platform_context_owner,
        platform_runner_description_owner,
        custom_task_runners_owner,
    )
}

pub fn maybe_load_aot(
    args: &mut FlutterProjectArgs,
    aot_opt: Option<&OsStr>,
) -> Option<CString> {
    if let Some(os) = aot_opt {
        let path = os.to_string_lossy();
        let aot_c = CString::new(path.as_ref()).expect("Failed to create CString for AOT path");
        let source = FlutterEngineAOTDataSource {
            type_: FlutterEngineAOTDataSourceType_kFlutterEngineAOTDataSourceTypeElfPath,
            __bindgen_anon_1: embedder::FlutterEngineAOTDataSource__bindgen_ty_1 {
                elf_path: aot_c.as_ptr(),
            },
        };
        let res = unsafe {
            embedder::FlutterEngineCreateAOTData(&source, &mut args.aot_data)
        };
        if res != FlutterEngineResult_kSuccess {
            error!("[ProjectArgs] FlutterEngineCreateAOTData failed: {:?}, path: {}", res, path);
            args.aot_data = ptr::null_mut();
            None 
        } else {
            info!("[ProjectArgs] FlutterEngineCreateAOTData successful for path: {}", path);
            Some(aot_c) 
        }
    } else {
        args.aot_data = ptr::null_mut();
        None
    }
}

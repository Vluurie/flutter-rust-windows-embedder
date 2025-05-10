use crate::embedder::{
    self, FlutterCustomTaskRunners, FlutterEngineAOTDataSource,
    FlutterEngineAOTDataSourceType_kFlutterEngineAOTDataSourceTypeElfPath,
    FlutterEngineResult_kSuccess, FlutterProjectArgs, FlutterTaskRunnerDescription,
};
// Your existing platform message callback
use super::platform_message_callback::simple_platform_message_callback;
// Callbacks and context from your task_scheduler module
// Ensure this path is correct for your project structure
use super::super::ticker::task_scheduler::{
    TaskRunnerContext, destroy_task_runner_context_callback, post_task_callback,
    runs_task_on_current_thread_callback,
};

use log::{error, info};
use std::ffi::{CStr, CString, OsStr, c_void}; // Added CStr
use std::ptr; // Assuming you use the log crate

// Your existing ARGS constant
const ARGS: &[&str] = &[
    "flutter_rust_embedder_app", // It's good practice for the first arg to be an app identifier
    "--verbose-system-logs",
];

// Your existing log callback and tag definitions
// Ensure FLUTTER_LOG_TAG is defined as you had it.
// For example, if it was a static CStr:
// static FLUTTER_LOG_TAG: &CStr = unsafe { CStr::from_bytes_with_nul_unchecked(b"rust_embedder\0") };
// Or if it's part of the overlay struct and passed around.
// For this example, I'll assume it's a static CStr as per your overlay/mod.rs snippet.
// If it's not static, you'll need to adjust how `log_tag` is obtained.
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
        CStr::from_ptr(tag).to_string_lossy().into_owned()
    };
    let msg_str = if message.is_null() {
        ""
    } else {
        &CStr::from_ptr(message).to_string_lossy().into_owned()
    };
    info!("[Flutter][{}] {}", tag_str, msg_str);
}

/// Creates a `FlutterTaskRunnerDescription` and its owned `TaskRunnerContext`.
/// The `TaskRunnerContext` is boxed, and its raw pointer is used as `user_data`.
/// Ownership of the `Box<TaskRunnerContext>` is returned to the caller.
fn create_task_runner_description_with_context(
    identifier: usize,
) -> (FlutterTaskRunnerDescription, Box<TaskRunnerContext>) {
    let context = Box::new(TaskRunnerContext {
        task_runner_thread_id: None, // Will be set by the task runner thread later
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

/// Build the FlutterProjectArgs *and* return the CStrings and owned Task Runner data.
/// The caller (init_overlay) is responsible for storing the returned Boxed items
/// to manage their lifetimes.
pub fn build_project_args_and_strings(
    assets: &str,
    icu: &str,
) -> (
    FlutterProjectArgs,
    CString,                           // assets_c
    CString,                           // icu_c
    Vec<CString>,                      // argv_cs
    Box<TaskRunnerContext>,            // Owned platform_runner_context
    Box<FlutterTaskRunnerDescription>, // Owned platform_runner_description
    Box<FlutterCustomTaskRunners>,     // Owned custom_task_runners_struct
) {
    // 1) Create CStrings for assets & ICU
    let assets_c = CString::new(assets).expect("Failed to convert assets path to CString");
    let icu_c = CString::new(icu).expect("Failed to convert icu data path to CString");

    // 2) Build command line CStrings
    let argv_cs: Vec<CString> = ARGS.iter().map(|&s| CString::new(s).unwrap()).collect();
    let argv_ptrs: Vec<*const std::os::raw::c_char> = argv_cs.iter().map(|c| c.as_ptr()).collect();

    // 3) Setup Task Runners
    let (platform_desc_st, platform_context_owner) = create_task_runner_description_with_context(1); // Identifier 1 for a shared runner

    let platform_runner_description_owner = Box::new(platform_desc_st);

    let custom_task_runners_owner = Box::new(FlutterCustomTaskRunners {
        struct_size: std::mem::size_of::<FlutterCustomTaskRunners>(),
        platform_task_runner: &*platform_runner_description_owner
            as *const FlutterTaskRunnerDescription,
        render_task_runner: &*platform_runner_description_owner
            as *const FlutterTaskRunnerDescription, // Same runner
        thread_priority_setter: None,
    });

    // 4) Fill out the FlutterProjectArgs struct
    let mut args = FlutterProjectArgs {
        struct_size: std::mem::size_of::<FlutterProjectArgs>(),
        assets_path: assets_c.as_ptr(),
        icu_data_path: icu_c.as_ptr(),
        command_line_argc: argv_ptrs.len() as i32,
        command_line_argv: argv_ptrs.as_ptr(),
        platform_message_callback: Some(simple_platform_message_callback),
        log_message_callback: Some(flutter_log_callback), // Using the callback defined in this file
        log_tag: FLUTTER_LOG_TAG.as_ptr(), // Using the static CStr defined in this file

        custom_task_runners: &*custom_task_runners_owner as *const FlutterCustomTaskRunners,

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
        main_path__unused__: ptr::null(),
        packages_path__unused__: ptr::null(),
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

/// Load AOT data if present, returning its CString so it isn’t dropped.
pub fn maybe_load_aot(args: &mut FlutterProjectArgs, aot_opt: Option<&OsStr>) -> Option<CString> {
    if let Some(os) = aot_opt {
        let path = os.to_string_lossy();
        let aot_c = CString::new(path.as_ref()).expect("Failed to create CString for AOT path");
        let source = FlutterEngineAOTDataSource {
            type_: FlutterEngineAOTDataSourceType_kFlutterEngineAOTDataSourceTypeElfPath,
            __bindgen_anon_1: embedder::FlutterEngineAOTDataSource__bindgen_ty_1 {
                elf_path: aot_c.as_ptr(),
            },
        };
        let res = unsafe { embedder::FlutterEngineCreateAOTData(&source, &mut args.aot_data) };
        if res != FlutterEngineResult_kSuccess {
            error!(
                "[ProjectArgs] FlutterEngineCreateAOTData failed: {:?}, path: {}",
                res, path
            );
            args.aot_data = ptr::null_mut();
            None
        } else {
            info!(
                "[ProjectArgs] FlutterEngineCreateAOTData successful for path: {}",
                path
            );
            Some(aot_c)
        }
    } else {
        args.aot_data = ptr::null_mut();
        None
    }
}

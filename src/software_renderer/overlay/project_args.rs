use crate::embedder::FlutterTaskRunnerDescription;
use crate::software_renderer::overlay::overlay_impl::FLUTTER_LOG_TAG;
use crate::software_renderer::ticker::task_scheduler::{
    destroy_task_runner_context_callback, post_task_callback, runs_task_on_current_thread_callback, TaskQueueState, TaskRunnerContext
};

use log::info;
use std::ffi::{CStr, CString, OsStr, c_void};
use std::sync::Arc;

const ARGS: &[&str] = &[
    "flutter_rust_embedder_instance",
    "--enable-software-rendering",
    "--skia-deterministic-rendering",
    "--verbose-system-logs",
    "--show-performance-overlay",
    // "--disable-service-auth-codes",
    //TODO: Since we have now multi instances we dont know port upfront, if only one instance exist maybe we can add a new configure option to set the port from outside
    // "--observatory-port=8801",
];

#[unsafe(no_mangle)]
pub unsafe extern "C" fn flutter_log_callback(
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
    task_queue: Arc<TaskQueueState>,
) -> (FlutterTaskRunnerDescription, Box<TaskRunnerContext>) {
    let context = Box::new(TaskRunnerContext {
        task_runner_thread_id: None,
        task_queue,
    });
    let context_ptr = Box::into_raw(context);

    let description = FlutterTaskRunnerDescription {
        struct_size: std::mem::size_of::<FlutterTaskRunnerDescription>(),
        user_data: context_ptr as *mut c_void, // looks at TaskRunnerContext
        runs_task_on_current_thread_callback: Some(runs_task_on_current_thread_callback),
        post_task_callback: Some(post_task_callback),
        identifier,
        destruction_callback: Some(destroy_task_runner_context_callback),
    };
    (description, unsafe { Box::from_raw(context_ptr) })
}

pub(crate) fn build_project_args_and_strings(
    assets: &str,
    icu: &str,
    dart_args_opt: Option<&[String]>,
    is_debug: bool,
) -> (
    CString,          // assets_c
    CString,          // icu_c
    Vec<CString>,     // engine_argv_cs
    Vec<CString>,     // dart_argv_cs
) {
    let assets_c = CString::new(assets).expect("Failed to convert assets path to CString");
    let icu_c = CString::new(icu).expect("Failed to convert icu data path to CString");
    let engine_argv_cs: Vec<CString> = if is_debug {
        ARGS.iter()
            .map(|&s| CString::new(s).expect("Failed to create CString from ARGS"))
            .collect()
    } else {
        Vec::new()
    };

    let dart_argv_cs: Vec<CString> = dart_args_opt.unwrap_or(&[])
        .iter()
        .map(|s| CString::new(s.as_str()).unwrap())
        .collect();

    (
        assets_c,
        icu_c,
        engine_argv_cs,
        dart_argv_cs,
    )
}
pub(crate) fn maybe_load_aot_path_to_cstring(aot_opt: Option<&OsStr>) -> Option<CString> {
    if let Some(os) = aot_opt {
        let path = os.to_string_lossy();
        let aot_c = CString::new(path.as_ref()).expect("Failed to create CString for AOT path");
        Some(aot_c)
    } else {
        None
    }
}

use crate::embedder::{FlutterCustomTaskRunners, FlutterTaskRunnerDescription};
use crate::software_renderer::ticker::task_scheduler::{
    TaskRunnerContext, destroy_task_runner_context_callback, post_task_callback,
    runs_task_on_current_thread_callback,
};

use log::info;
use std::ffi::{CStr, CString, OsStr, c_void};
use std::sync::atomic::Ordering;

use super::init::FLUTTER_ASSETS_IS_DEBUG;

const ARGS: &[&str] = &[
    "flutter_rust_embedder_app",
    "--enable-software-rendering",
    "--skia-deterministic-rendering",
    "--verbose-system-logs",
    "--show-performance-overlay",
    "--disable-service-auth-codes",
    "--observatory-port=8801",
];

pub static FLUTTER_LOG_TAG: &CStr =
    unsafe { CStr::from_bytes_with_nul_unchecked(b"rust_embedder\0") };

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

pub(crate) fn build_project_args_and_strings(
    assets: &str,
    icu: &str,
    dart_args_opt: Option<&[String]>,
) -> (
    CString,      // assets_c
    CString,      // icu_c
    Vec<CString>, // engine_argv_cs
    Vec<CString>, // dart_argv_cs
    Box<TaskRunnerContext>,
    Box<FlutterTaskRunnerDescription>,
    Box<FlutterCustomTaskRunners>,
) {
    let assets_c = CString::new(assets).expect("Failed to convert assets path to CString");
    let icu_c = CString::new(icu).expect("Failed to convert icu data path to CString");
    let engine_argv_cs: Vec<CString>;
    if FLUTTER_ASSETS_IS_DEBUG.load(Ordering::Relaxed) {
        engine_argv_cs = ARGS
            .iter()
            .map(|&s| CString::new(s).expect("Failed to create CString from ARGS"))
            .collect();
    } else {
        engine_argv_cs = Vec::new();
    }

    let mut dart_argv_cs: Vec<CString> = Vec::new();
    if let Some(dart_args_slice) = dart_args_opt {
        for arg_str in dart_args_slice {
            let c_string_arg = CString::new(arg_str.as_str()).unwrap_or_else(|_| {
                panic!("Failed to convert Dart argument to CString: {}", arg_str)
            });
            dart_argv_cs.push(c_string_arg);
        }
    }

    let (platform_desc_st, platform_context_owner) = create_task_runner_description_with_context(1);
    let platform_runner_description_owner = Box::new(platform_desc_st);
    let custom_task_runners_owner = Box::new(FlutterCustomTaskRunners {
        struct_size: std::mem::size_of::<FlutterCustomTaskRunners>(),
        platform_task_runner: &*platform_runner_description_owner
            as *const FlutterTaskRunnerDescription,
        render_task_runner: &*platform_runner_description_owner
            as *const FlutterTaskRunnerDescription,
        thread_priority_setter: None,
    });

    (
        assets_c,
        icu_c,
        engine_argv_cs,
        dart_argv_cs,
        platform_context_owner,
        platform_runner_description_owner,
        custom_task_runners_owner,
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

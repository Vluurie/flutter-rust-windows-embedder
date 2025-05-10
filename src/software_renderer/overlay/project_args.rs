use crate::embedder;
use log::error;
use std::{ffi::{CString, OsStr}, ptr};

use super::platform_message_callback::simple_platform_message_callback;


const ARGS: &[&str] = &[
    "--verbose-system-logs",
    "--observe=0/127.0.0.1",
    "--disable-service-auth-codes",
];

/// Build the FlutterProjectArgs *and* return the CStrings you must hold onto.
pub fn build_project_args_and_strings(
    assets: &str,
    icu: &str,
) -> (
    embedder::FlutterProjectArgs,
    CString,  // assets_c
    CString,  // icu_c
    Vec<CString>, // argv_cs
) {
    // 1) Create CStrings for assets & ICU
    let assets_c = CString::new(assets)
        .expect("Failed to convert assets path to CString");
    let icu_c    = CString::new(icu)
        .expect("Failed to convert icu data path to CString");

    // 2) Build dummy argv CStrings
    let argv_cs: Vec<CString> = ARGS
        .iter()
        .map(|&s| CString::new(s).unwrap())
        .collect();
    let argv_ptrs: Vec<*const i8> = argv_cs.iter().map(|c| c.as_ptr()).collect();

    // 3) Fill out the args struct
    let mut args: embedder::FlutterProjectArgs = unsafe { std::mem::zeroed() };
    args.struct_size        = std::mem::size_of::<embedder::FlutterProjectArgs>();
    args.assets_path        = assets_c.as_ptr();
    args.icu_data_path      = icu_c.as_ptr();
    args.command_line_argc  = argv_ptrs.len() as i32;
    args.command_line_argv  = argv_ptrs.as_ptr();
    args.platform_message_callback =  Some(simple_platform_message_callback);
    args.log_message_callback      = Some(super::flutter_log_callback);
    args.log_tag                    = super::FLUTTER_LOG_TAG.as_ptr();

    (args, assets_c, icu_c, argv_cs)
}

/// Load AOT data if present, returning its CString so it isn’t dropped.
pub fn maybe_load_aot(
    args: &mut embedder::FlutterProjectArgs,
    aot_opt: Option<&OsStr>,
) -> Option<CString> {
    if let Some(os) = aot_opt {
        let path = os.to_string_lossy();
        let aot_c = CString::new(path.as_ref()).unwrap();
        let source = embedder::FlutterEngineAOTDataSource {
            type_: embedder::FlutterEngineAOTDataSourceType_kFlutterEngineAOTDataSourceTypeElfPath,
            __bindgen_anon_1: embedder::FlutterEngineAOTDataSource__bindgen_ty_1 {
                elf_path: aot_c.as_ptr(),
            },
        };
        let res = unsafe {
            embedder::FlutterEngineCreateAOTData(&source, &mut args.aot_data)
        };
        if res != embedder::FlutterEngineResult_kSuccess {
            error!("FlutterEngineCreateAOTData failed: {:?}", res);
            args.aot_data = ptr::null_mut();
        }
        Some(aot_c)
    } else {
        args.aot_data = ptr::null_mut();
        None
    }
}

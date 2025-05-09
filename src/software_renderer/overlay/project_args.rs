use crate::embedder;
use log::error;
use std::{ffi::CString, ptr};

const DUMMY_ARGS: &[&str] = &[
    "dummy_app_name",
    "--verbose-system-logs",
    "--enable-vm-service",
];

pub fn build_project_args(assets: &str, icu: &str) -> embedder::FlutterProjectArgs {
    let mut args: embedder::FlutterProjectArgs = unsafe { std::mem::zeroed() };
    args.struct_size = std::mem::size_of::<embedder::FlutterProjectArgs>();

    let assets_c = CString::new(assets).expect("Failed to convert assets path to CString");
    args.assets_path = assets_c.as_ptr();

    let icu_c = CString::new(icu).expect("Failed to convert icu data path to CString");
    args.icu_data_path = icu_c.as_ptr();

    let mut cstrs: Vec<CString> = Vec::with_capacity(DUMMY_ARGS.len());
    for &arg in DUMMY_ARGS {
        cstrs.push(CString::new(arg).expect("invalid dummy arg"));
    }
    let ptrs: Vec<*const i8> = cstrs.iter().map(|c| c.as_ptr()).collect();
    args.command_line_argc = ptrs.len() as i32;
    args.command_line_argv = ptrs.as_ptr();

    args.platform_message_callback = None;
    args.log_message_callback = Some(super::flutter_log_callback);
    args.log_tag = super::FLUTTER_LOG_TAG.as_ptr();

    args
}

pub fn maybe_load_aot(args: &mut embedder::FlutterProjectArgs, aot_opt: Option<&std::ffi::OsStr>) {
    if let Some(os) = aot_opt {
        let path = os.to_string_lossy();
        let c = CString::new(path.as_ref()).unwrap();
        let source = embedder::FlutterEngineAOTDataSource {
            type_: embedder::FlutterEngineAOTDataSourceType_kFlutterEngineAOTDataSourceTypeElfPath,
            __bindgen_anon_1: embedder::FlutterEngineAOTDataSource__bindgen_ty_1 {
                elf_path: c.as_ptr(),
            },
        };
        let r = unsafe { embedder::FlutterEngineCreateAOTData(&source, &mut args.aot_data) };
        if r != embedder::FlutterEngineResult_kSuccess {
            error!("FlutterEngineCreateAOTData failed: {:?}", r);
            args.aot_data = ptr::null_mut();
        }
    } else {
        args.aot_data = ptr::null_mut();
    }
}

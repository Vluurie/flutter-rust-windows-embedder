use crate::{
    constants,
    flutter_bindings,
    win32_utils,
};
use std::{
     ffi::{c_char, OsString}, mem, os::windows::ffi::OsStringExt, path::PathBuf, ptr
};
use windows::{core::PCWSTR, Win32::{
    Foundation::{HMODULE, HWND},
    System::{Com::CoUninitialize, LibraryLoader::{GetModuleFileNameW, GetModuleHandleExW, GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS, GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT}},
}};
use log::{debug, error, info};

/// Alias for the raw Flutter engine handles
pub type FlutterDesktopEngineRef = flutter_bindings::FlutterDesktopEngineRef;
pub type FlutterDesktopViewControllerRef = flutter_bindings::FlutterDesktopViewControllerRef;
pub type FlutterDesktopViewRef = flutter_bindings::FlutterDesktopViewRef;
pub type FlutterHWND = flutter_bindings::HWND;
pub type FlutterWPARAM = flutter_bindings::WPARAM;
pub type FlutterLPARAM = flutter_bindings::LPARAM;
pub type FlutterLRESULT = flutter_bindings::LRESULT;
pub type FlutterUINT = flutter_bindings::UINT;

/// Determine and validate the platform‐specific Flutter asset paths:
/// 1. `<dll_dir>/data/flutter_assets`  
/// 2. `<dll_dir>/data/icudtl.dat`  
/// 3. `<dll_dir>/data/app.so`  
/// 
/// Panics if any of the above are missing. Returns each path as a null-terminated UTF-16 vector.
fn get_flutter_paths() -> (Vec<u16>, Vec<u16>, Vec<u16>) {
    let root_dir = dll_directory();
    let data_dir_path = root_dir.join("data");
    info!("[Flutter Utils] Data directory path: {:?}", data_dir_path);

    let assets_dir = data_dir_path.join("flutter_assets");
    let icu_file   = data_dir_path.join("icudtl.dat");
    let aot_lib    = data_dir_path.join("app.so");

    if !assets_dir.is_dir() {
        panic!("[Flutter Utils] Missing flutter_assets at `{}`", assets_dir.display());
    }
    if !icu_file.is_file() {
        panic!("[Flutter Utils] Missing icudtl.dat at `{}`", icu_file.display());
    }
    if !aot_lib.is_file() {
        panic!("[Flutter Utils] Missing AOT library at `{}`", aot_lib.display());
    }

    debug!(
        "[Flutter Utils] Validated paths: assets=`{}`, icu=`{}`, aot=`{}`",
        assets_dir.display(),
        icu_file.display(),
        aot_lib.display(),
    );

    let assets_w = win32_utils::to_wide(assets_dir.to_str().unwrap());
    let icu_w    = win32_utils::to_wide(icu_file.to_str().unwrap());
    let aot_w    = win32_utils::to_wide(aot_lib.to_str().unwrap());

    (assets_w, icu_w, aot_w)
}

/// Creates and initializes the Flutter engine with the appropriate properties.
/// On failure, uninitializes COM and aborts the process.
pub fn create_flutter_engine() -> FlutterDesktopEngineRef {
    let (assets_w, icu_w, aot_w) = get_flutter_paths();

    // Prepare Dart entrypoint arguments
    let args_ptrs: Vec<*const c_char> = constants::DART_ENTRYPOINT_ARGS
        .iter()
        .map(|arg| arg.as_ptr() as *const c_char)
        .collect();

    let props = flutter_bindings::FlutterDesktopEngineProperties {
        assets_path:          assets_w.as_ptr(),
        icu_data_path:        icu_w.as_ptr(),
        aot_library_path:     aot_w.as_ptr(),
        dart_entrypoint:      ptr::null(),
        dart_entrypoint_argc: args_ptrs.len() as i32,
        dart_entrypoint_argv: if args_ptrs.is_empty() {
            ptr::null_mut()
        } else {
            args_ptrs.as_ptr() as *mut *const c_char
        },
        ..unsafe { mem::zeroed() }
    };

    info!("[Flutter Utils] Initializing Flutter engine");
    let engine = unsafe { flutter_bindings::FlutterDesktopEngineCreate(&props) };
    if engine.is_null() {
        error!("[Flutter Utils] Engine creation failed");
        unsafe { CoUninitialize() };
        win32_utils::panic_with_error("FlutterDesktopEngineCreate failed");
    }
    info!("[Flutter Utils] Engine created");
    engine
}


/// Like `create_flutter_engine`, but with explicit asset/ICU/AOT paths.
pub fn create_flutter_engine_with_paths(
    assets_path: Vec<u16>,
    icu_data_path: Vec<u16>,
    aot_library_path: Vec<u16>,
) -> FlutterDesktopEngineRef {
    // Prepare the same args as before...
    let args_ptrs: Vec<*const i8> = crate::constants::DART_ENTRYPOINT_ARGS
        .iter()
        .map(|s| s.as_ptr() as *const i8)
        .collect();

    let props = flutter_bindings::FlutterDesktopEngineProperties {
        assets_path:      assets_path.as_ptr(),
        icu_data_path:    icu_data_path.as_ptr(),
        aot_library_path: aot_library_path.as_ptr(),
        dart_entrypoint:  std::ptr::null(),
        dart_entrypoint_argc: args_ptrs.len() as i32,
        dart_entrypoint_argv: if args_ptrs.is_empty() {
            std::ptr::null_mut()
        } else {
            args_ptrs.as_ptr() as *mut *const i8
        },
        ..unsafe { std::mem::zeroed() }
    };

    let engine = unsafe { flutter_bindings::FlutterDesktopEngineCreate(&props) };
    if engine.is_null() {
        // cleanup & abort
        unsafe { windows::Win32::System::Com::CoUninitialize() };
        crate::win32_utils::panic_with_error("FlutterDesktopEngineCreate failed");
    }
    engine
}

/// Creates a Flutter view controller of the given size for the specified engine.
/// Panics (after cleanup) on failure.
pub fn create_flutter_view_controller(
    engine: FlutterDesktopEngineRef,
    width: i32,
    height: i32,
) -> FlutterDesktopViewControllerRef {
    info!("[Flutter Utils] Creating view controller");
    let controller = unsafe {
        flutter_bindings::FlutterDesktopViewControllerCreate(width, height, engine)
    };
    if controller.is_null() {
        error!("[Flutter Utils] View controller creation failed");
        unsafe {
            flutter_bindings::FlutterDesktopEngineDestroy(engine);
            CoUninitialize();
        }
        win32_utils::panic_with_error("FlutterDesktopViewControllerCreate failed");
    }
    info!("[Flutter Utils] View controller created");
    controller
}

/// Retrieves the Flutter view and underlying HWND from a view controller.
/// On failure, cleans up and aborts.
pub fn get_flutter_view_and_hwnd(
    controller: FlutterDesktopViewControllerRef,
) -> (FlutterDesktopViewRef, HWND) {
    info!("[Flutter Utils] Obtaining Flutter view");
    let view = unsafe { flutter_bindings::FlutterDesktopViewControllerGetView(controller) };
    if view.is_null() {
        error!("[Flutter Utils] Failed to get view");
        unsafe {
            flutter_bindings::FlutterDesktopViewControllerDestroy(controller);
            CoUninitialize();
        }
        win32_utils::panic_with_error("FlutterDesktopViewControllerGetView failed");
    }

    info!("[Flutter Utils] Obtaining HWND from view");
    let raw = unsafe { flutter_bindings::FlutterDesktopViewGetHWND(view) };
    if raw.is_null() {
        error!("[Flutter Utils] View returned null HWND");
        unsafe {
            flutter_bindings::FlutterDesktopViewControllerDestroy(controller);
            CoUninitialize();
        }
        win32_utils::panic_with_error("FlutterDesktopViewGetHWND failed");
    }

    let hwnd = HWND(raw as isize);
    info!("[Flutter Utils] Flutter child HWND = {:?}", hwnd);
    (view, hwnd)
}

pub fn dll_directory() -> PathBuf {
    unsafe {
        let mut hmod = HMODULE(0);

        let addr_pcwstr = PCWSTR(dll_directory as *const () as _);
        let ok = GetModuleHandleExW(
            GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS
                | GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT,
            addr_pcwstr,
            &mut hmod,
        );
        if !ok.as_bool() {
            panic!("GetModuleHandleExW failed");
        }

        const MAX_PATH: usize = 260;
        let mut buf = [0u16; MAX_PATH];
        let len = GetModuleFileNameW(hmod, &mut buf) as usize;
        if len == 0 {
            panic!("GetModuleFileNameW failed");
        }

        let os = OsString::from_wide(&buf[..len]);
        PathBuf::from(os).parent().unwrap().to_path_buf()
    }
}
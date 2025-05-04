use crate::{
    constants, dynamic_flutter_windows_dll_loader::DLL, flutter_bindings, path_utils, win32_utils,
};
use log::{error, info};
use std::{ffi::c_char, mem, ptr};
use windows::Win32::{Foundation::HWND, System::Com::CoUninitialize};

/// Alias for the raw Flutter engine handles
pub type FlutterDesktopEngineRef = flutter_bindings::FlutterDesktopEngineRef;
pub type FlutterDesktopViewControllerRef = flutter_bindings::FlutterDesktopViewControllerRef;
pub type FlutterDesktopViewRef = flutter_bindings::FlutterDesktopViewRef;
pub type FlutterHWND = flutter_bindings::HWND;
pub type FlutterWPARAM = flutter_bindings::WPARAM;
pub type FlutterLPARAM = flutter_bindings::LPARAM;
pub type FlutterLRESULT = flutter_bindings::LRESULT;
pub type FlutterUINT = flutter_bindings::UINT;

/// Creates and initializes the Flutter engine with the appropriate properties.
/// On failure, uninitializes COM and aborts the process.
pub fn create_flutter_engine() -> FlutterDesktopEngineRef {
    let (assets_path, icu_data_path, aot_w) = path_utils::get_flutter_paths();

    // Prepare Dart entrypoint arguments
    let args_ptrs: Vec<*const c_char> = constants::DART_ENTRYPOINT_ARGS
        .iter()
        .map(|arg| arg.as_ptr() as *const c_char)
        .collect();

    let aot_ptr = if aot_w.is_empty() {
        std::ptr::null()
    } else {
        aot_w.as_ptr()
    };

    let mut props: flutter_bindings::FlutterDesktopEngineProperties = unsafe { mem::zeroed() };
    props.assets_path = assets_path.as_ptr();
    props.icu_data_path = icu_data_path.as_ptr();
    props.aot_library_path = aot_ptr;
    props.dart_entrypoint = ptr::null();
    props.dart_entrypoint_argc = args_ptrs.len() as i32;
    props.dart_entrypoint_argv = if args_ptrs.is_empty() {
        ptr::null_mut()
    } else {
        args_ptrs.as_ptr() as *mut *const i8
    };

    info!("[Flutter Utils] Initializing Flutter engine");
    let dll = DLL.get().expect("flutter_windows.dll not loaded");
    let engine = unsafe { (dll.FlutterDesktopEngineCreate)(props) };
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

    let aot_ptr = if aot_library_path.is_empty() {
        std::ptr::null()
    } else {
        aot_library_path.as_ptr()
    };

    let props = flutter_bindings::FlutterDesktopEngineProperties {
        assets_path: assets_path.as_ptr(),
        icu_data_path: icu_data_path.as_ptr(),
        aot_library_path: aot_ptr,
        dart_entrypoint: std::ptr::null(),
        dart_entrypoint_argc: args_ptrs.len() as i32,
        dart_entrypoint_argv: if args_ptrs.is_empty() {
            std::ptr::null_mut()
        } else {
            args_ptrs.as_ptr() as *mut *const i8
        },
        ..unsafe { std::mem::zeroed() }
    };

    let dll = DLL.get().expect("flutter_windows.dll not loaded");
    let engine = unsafe { (dll.FlutterDesktopEngineCreate)(props) };
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
    let dll = DLL.get().expect("flutter_windows.dll not loaded");
    let controller = unsafe { (dll.FlutterDesktopViewControllerCreate)(width, height, engine) };
    if controller.is_null() {
        error!("[Flutter Utils] View controller creation failed");
        unsafe {
            (dll.FlutterDesktopEngineDestroy)(engine);
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
    let dll = DLL.get().expect("flutter_windows.dll not loaded");
    let view = unsafe { (dll.FlutterDesktopViewControllerGetView)(controller) };
    if view.is_null() {
        error!("[Flutter Utils] Failed to get view");
        unsafe {
            (dll.FlutterDesktopViewControllerDestroy)(controller);
            CoUninitialize();
        }
        win32_utils::panic_with_error("FlutterDesktopViewControllerGetView failed");
    }

    info!("[Flutter Utils] Obtaining HWND from view");
    let raw = unsafe { (dll.FlutterDesktopViewGetHWND)(view) };
    if raw.is_null() {
        error!("[Flutter Utils] View returned null HWND");
        unsafe {
            (dll.FlutterDesktopViewControllerDestroy)(controller);
            CoUninitialize();
        }
        win32_utils::panic_with_error("FlutterDesktopViewGetHWND failed");
    }

    let hwnd = HWND(raw as isize);
    info!("[Flutter Utils] Flutter child HWND = {:?}", hwnd);
    (view, hwnd)
}

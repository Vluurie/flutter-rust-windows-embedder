//! Win32 helper functions for registering and creating the main window,
//! embedding the Flutter child HWND, and running the message loop.

use crate::{
    app_state::AppState,
    constants, flutter_bindings,
    flutter_utils::{FlutterHWND, FlutterLPARAM, FlutterLRESULT, FlutterUINT, FlutterWPARAM},
};
use log::{debug, error, info, warn};
use std::{
    ffi::{OsStr, c_void},
    os::windows::ffi::OsStrExt,
    sync::Once,
};
use windows::{
    Win32::{
        Foundation::{GetLastError, HWND, LPARAM, LRESULT, RECT, WPARAM},
        Graphics::Gdi::HBRUSH,
        System::LibraryLoader::GetModuleHandleW,
        UI::WindowsAndMessaging::{
            CREATESTRUCTW, CS_HREDRAW, CS_VREDRAW, CreateWindowExW, DefWindowProcW,
            DispatchMessageW, GWLP_USERDATA, GetClientRect, GetMessageW, GetWindowLongPtrW, HICON,
            HMENU, IDC_ARROW, LoadCursorW, MSG, MoveWindow, PostQuitMessage, RegisterClassW,
            SetParent, SetWindowLongPtrW, TranslateMessage, WINDOW_EX_STYLE, WM_ACTIVATE,
            WM_DESTROY, WM_NCCREATE, WM_SIZE, WNDCLASSW, WS_CLIPCHILDREN, WS_OVERLAPPEDWINDOW,
            WS_VISIBLE,
        },
    },
    core::PCWSTR,
};

#[link(name = "user32")]
unsafe extern "system" {
    fn SetFocus(hWnd: HWND) -> HWND;
}

/// Window procedure: handles creation, sizing, activation, destruction,
/// and delegates unhandled messages to Flutter.
pub unsafe extern "system" fn wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    unsafe {
        let app_state_ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut AppState;

        match msg {
            WM_NCCREATE => {
                info!("[WndProc] WM_NCCREATE");
                let cs = (lparam.0 as *const CREATESTRUCTW).as_ref();
                if let Some(cs) = cs {
                    let param = cs.lpCreateParams;
                    debug!("[WndProc] Storing AppState ptr {:?}", param);
                    SetWindowLongPtrW(hwnd, GWLP_USERDATA, param as isize);
                } else {
                    warn!("[WndProc] CREATESTRUCTW was null");
                }
                DefWindowProcW(hwnd, msg, wparam, lparam)
            }

            WM_SIZE => {
                if let Some(state) = app_state_ptr.as_mut() {
                    let mut rc = RECT::default();
                    if GetClientRect(hwnd, &mut rc).as_bool() {
                        let w = rc.right - rc.left;
                        let h = rc.bottom - rc.top;
                        debug!(
                            "[WndProc] Resizing child {:?} to {}×{}",
                            state.child_hwnd, w, h
                        );
                        MoveWindow(state.child_hwnd, 0, 0, w, h, true);
                    }
                }
                LRESULT(0)
            }

            WM_ACTIVATE => {
                if let Some(state) = app_state_ptr.as_mut() {
                    debug!("[WndProc] WM_ACTIVATE: focusing {:?}", state.child_hwnd);
                    SetFocus(state.child_hwnd);
                }
                LRESULT(0)
            }

            WM_DESTROY => {
                info!("[WndProc] WM_DESTROY");
                if !app_state_ptr.is_null() {
                    debug!("[WndProc] Dropping AppState");
                    drop(Box::from_raw(app_state_ptr));
                    SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
                }
                PostQuitMessage(0);
                LRESULT(0)
            }

            _ => {
                if let Some(state) = app_state_ptr.as_mut() {
                    if !state.controller.is_null() {
                        let mut out: FlutterLRESULT = 0;
                        let handled =
                            flutter_bindings::FlutterDesktopViewControllerHandleTopLevelWindowProc(
                                state.controller,
                                hwnd.0 as FlutterHWND,
                                msg as FlutterUINT,
                                wparam.0 as FlutterWPARAM,
                                lparam.0 as FlutterLPARAM,
                                &mut out,
                            );
                        if handled {
                            return LRESULT(out as isize);
                        }
                    }
                }
                DefWindowProcW(hwnd, msg, wparam, lparam)
            }
        }
    }
}

/// we only registered once.
static REGISTER_CLASS_ONCE: Once = Once::new();

/// Registers the Win32 window class for the main application window.
/// Panics if registration fails.
pub fn register_window_class() {
    REGISTER_CLASS_ONCE.call_once(|| unsafe {
        let hinst = GetModuleHandleW(None).expect("GetModuleHandleW failed");
        let wc = WNDCLASSW {
            hInstance: hinst.into(),
            lpszClassName: constants::WINDOW_CLASS_NAME,
            lpfnWndProc: Some(wnd_proc),
            style: CS_HREDRAW | CS_VREDRAW,
            hCursor: LoadCursorW(None, IDC_ARROW).unwrap_or_default(),
            hbrBackground: HBRUSH::default(),
            lpszMenuName: PCWSTR::null(),
            hIcon: HICON::default(),
            cbClsExtra: 0,
            cbWndExtra: 0,
        };
        if RegisterClassW(&wc) == 0 {
            let e = GetLastError();
            panic!("[Win32 Utils] RegisterClassW failed: {:?}", e);
        }
        info!("[Win32 Utils] Window class registered");
    });
}

/// Creates the main parent window and associates the given `AppState` pointer.
/// Panics (after cleanup) if window creation fails.
pub fn create_main_window(app_state_ptr: *mut AppState) -> HWND {
    info!("[Win32 Utils] Creating main window");
    let hwnd = unsafe {
        CreateWindowExW(
            WINDOW_EX_STYLE::default(),
            constants::WINDOW_CLASS_NAME,
            constants::WINDOW_TITLE,
            WS_OVERLAPPEDWINDOW | WS_VISIBLE | WS_CLIPCHILDREN,
            100,
            100,
            constants::DEFAULT_WINDOW_WIDTH,
            constants::DEFAULT_WINDOW_HEIGHT,
            None,
            HMENU::default(),
            GetModuleHandleW(None).unwrap(),
            Some(app_state_ptr as *mut c_void),
        )
    };
    if hwnd.0 == 0 {
        let err = unsafe { GetLastError() };
        error!("[Win32 Utils] CreateWindowExW failed: {:?}", err);
        // cleanup
        unsafe {
            drop(Box::from_raw(app_state_ptr));
            flutter_bindings::FlutterDesktopViewControllerDestroy((*app_state_ptr).controller);
            windows::Win32::System::Com::CoUninitialize();
        }
        panic!("[Win32 Utils] Could not create main window");
    }
    info!("[Win32 Utils] Main window created: {:?}", hwnd);
    hwnd
}

/// Reparents the Flutter child window under the main parent and resizes it to fit.
pub fn set_flutter_window_as_child(parent: HWND, child: HWND) {
    info!(
        "[Win32 Utils] Embedding Flutter HWND {:?} into {:?}",
        child, parent
    );
    let prev = unsafe { SetParent(child, parent) };
    let err = unsafe { GetLastError() };
    if err.0 != 0 {
        warn!("[Win32 Utils] SetParent error: {:?}", err);
    } else if prev.0 != 0 {
        debug!("[Win32 Utils] Child was already parented under {:?}", prev);
    }

    let mut rc = RECT::default();
    if unsafe { GetClientRect(parent, &mut rc) }.as_bool() {
        let w = rc.right - rc.left;
        let h = rc.bottom - rc.top;
        debug!("[Win32 Utils] Initial resize to {}×{}", w, h);
        unsafe { MoveWindow(child, 0, 0, w, h, true) };
    }
}

/// Runs the Win32 message loop until WM_QUIT, then cleans up any remaining `AppState`.
pub fn run_message_loop(parent: HWND, _app_state_ptr: *mut AppState) {
    info!("[Win32 Utils] Entering message loop");
    let mut msg = MSG::default();
    unsafe {
        while GetMessageW(&mut msg, HWND::default(), 0, 0).as_bool() {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
    info!("[Win32 Utils] Exited message loop");

    // Final AppState cleanup if necessary
    let ptr = unsafe { GetWindowLongPtrW(parent, GWLP_USERDATA) as *mut AppState };
    if !ptr.is_null() {
        debug!("[Win32 Utils] Cleaning up AppState after loop");
        unsafe { drop(Box::from_raw(ptr)) };
    } else {
        debug!("[Win32 Utils] AppState already cleaned up");
    }
}

pub fn to_wide(s: &str) -> Vec<u16> {
    OsStr::new(s).encode_wide().chain(Some(0)).collect()
}

pub fn panic_with_error(message: &str) -> ! {
    let err = std::io::Error::last_os_error();
    let full = format!("{} OS error: {}", message, err);
    error!("{}", full);
    panic!("{}", full);
}

//! Win32 helper functions for Flutter desktop embedding on Windows.
//!
//! This module provides everything needed to:
//! 1. Register a Win32 window class.
//! 2. Create a parent window and embed a Flutter child HWND as a *real* child window.
//! 3. Run the Win32 message loop.
//! 4. Handle all the key window messages—size, focus, DPI changes, dragging, etc.—  
//!    forwarding them first to the Flutter engine (so plugins see them), then to  
//!    the Flutter view controller, and finally to `DefWindowProcW`.
//!
//! # Overview of message handling
//!
//! - **WM_NCCREATE**: capture and stash our `AppState` pointer in `GWLP_USERDATA`.
//! - **WM_SIZE**:  
//!     1. resize the embedded Flutter child to fill our client area,  
//!     2. forward the size event into Flutter so it can update its viewport.
//! - **WM_ACTIVATE / WM_SETFOCUS**: forward keyboard focus to the Flutter child.
//! - **WM_KILLFOCUS**: log when focus is lost.
//! - **WM_CLOSE**: destroy our window (triggers WM_DESTROY).
//! - **WM_DESTROY**: drop our `AppState`, post `WM_QUIT`.
//! - **WM_NCHITTEST**: treat clicks in the client area as `HTCAPTION` so the user  
//!   can drag the window by our custom header.
//! - **WM_DPICHANGED**: reposition/resize to the new DPI-aware bounds.
//! - **WM_PAINT**: we never paint the parent; the child covers the client area.
//! - **All others**:  
//!     1. first sent to the **engine** via  
//!        `FlutterDesktopEngineProcessExternalWindowMessage` (plugins see _every_ event),  
//!     2. then to the **view** via  
//!        `FlutterDesktopViewControllerHandleTopLevelWindowProc`,  
//!     3. finally to `DefWindowProcW`.
//!
//! # Safety
//! - This `wnd_proc` must be registered as `lpfnWndProc` in your `WNDCLASSW`.
//! - It assumes that `lpCreateParams` in `WM_NCCREATE` is a valid `*mut AppState`.
//!
//! # Usage
//! ```ignore
//! win32_utils::register_window_class();
//! let parent = win32_utils::create_main_window(app_state_ptr);
//! win32_utils::set_flutter_window_as_child(parent, flutter_child_hwnd);
//! win32_utils::run_message_loop(parent, app_state_ptr);
//! ```

#![allow(non_camel_case_types, non_upper_case_globals, non_snake_case)]
#![allow(dead_code)]

use crate::{
    app_state::AppState,
    constants,
    flutter_bindings::{
        self, FlutterDesktopEngineProcessExternalWindowMessage,
        FlutterDesktopViewControllerGetEngine,
        FlutterDesktopViewControllerHandleTopLevelWindowProc, HWND as RawHWND, LPARAM as RawLPARAM,
        LRESULT as RawLRESULT, UINT as RawUINT, WPARAM as RawWPARAM,
    },
};
use log::{debug, error, info, warn};
use std::{ffi::OsStr, ffi::c_void, os::windows::ffi::OsStrExt, sync::Once};
use windows::{
    Win32::{
        Foundation::{GetLastError, HWND, LPARAM, LRESULT, RECT, WPARAM},
        Graphics::Gdi::HBRUSH,
        System::LibraryLoader::GetModuleHandleW,
        UI::WindowsAndMessaging::{
            CREATESTRUCTW, CS_HREDRAW, CS_VREDRAW, CreateWindowExW, DefWindowProcW, DestroyWindow,
            DispatchMessageW, GWL_STYLE, GWLP_USERDATA, GetClientRect, GetMessageW,
            GetWindowLongPtrW, HMENU, HTCAPTION, HTCLIENT, IDC_ARROW, LoadCursorW, MSG, MoveWindow,
            PostMessageW, PostQuitMessage, RegisterClassW, SWP_ASYNCWINDOWPOS, SWP_FRAMECHANGED,
            SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER, SendMessageW, SetParent,
            SetWindowLongPtrW, SetWindowPos, TranslateMessage, WINDOW_EX_STYLE, WM_ACTIVATE,
            WM_CLOSE, WM_DESTROY, WM_DPICHANGED, WM_KILLFOCUS, WM_NCCREATE, WM_NCHITTEST, WM_PAINT,
            WM_SETFOCUS, WM_SIZE, WNDCLASSW, WS_CHILD, WS_CLIPCHILDREN, WS_OVERLAPPEDWINDOW,
            WS_POPUP, WS_VISIBLE,
        },
    },
    core::PCWSTR,
};

#[link(name = "user32")]
unsafe extern "system" {
    /// Forward keyboard focus to a child HWND.
    fn SetFocus(hWnd: HWND) -> HWND;
}

//---------------------------------------------------------------------------
// Window procedure: lifecycle, sizing, focus, DPI, dragging,
// plus forwarding to Flutter engine & view, then DefWindowProcW.
//---------------------------------------------------------------------------

/// Our WndProc: stores/drops `AppState`, embeds & sizes Flutter child,
/// handles custom dragging, DPI and focus, then delegates events to Flutter.
///
/// Safety:
/// - Must be installed as `lpfnWndProc`.
/// - `WM_NCCREATE`'s `lpCreateParams` must be a valid `*mut AppState`.
pub extern "system" fn wnd_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    unsafe {
        // Retrieve our AppState pointer
        let state_ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut AppState;

        match msg {
            WM_NCCREATE => {
                info!("[WndProc] WM_NCCREATE");
                // stash AppState in GWLP_USERDATA
                if let Some(cs) = (lparam.0 as *const CREATESTRUCTW).as_ref() {
                    let ptr = cs.lpCreateParams as isize;
                    debug!("[WndProc] Storing AppState ptr {:?}", ptr);
                    SetWindowLongPtrW(hwnd, GWLP_USERDATA, ptr);
                } else {
                    warn!("[WndProc] CREATESTRUCTW was null");
                }
                // let Windows do the default non-client setup
                DefWindowProcW(hwnd, msg, wparam, lparam)
            }

            WM_SIZE => {
                // 1) resize embedded Flutter child
                if let Some(state) = state_ptr.as_mut() {
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
                    // 2) forward WM_SIZE to Flutter view
                    let raw_hwnd: RawHWND = std::mem::transmute(hwnd);
                    let raw_wp: RawWPARAM = wparam.0 as _;
                    let raw_lp: RawLPARAM = lparam.0 as _;
                    let mut raw_out: RawLRESULT = 0;
                    let handled = FlutterDesktopViewControllerHandleTopLevelWindowProc(
                        state.controller,
                        raw_hwnd,
                        WM_SIZE as _,
                        raw_wp,
                        raw_lp,
                        &mut raw_out as *mut _,
                    );
                    if handled {
                        return LRESULT(raw_out.try_into().unwrap());
                    }
                }
                // unhandled size → return 0
                LRESULT(0)
            }

            WM_ACTIVATE | WM_SETFOCUS => {
                // forward focus into Flutter child
                if let Some(state) = state_ptr.as_mut() {
                    debug!("[WndProc] focus event: {}", msg);
                    SetFocus(state.child_hwnd);
                }
                LRESULT(0)
            }

            WM_KILLFOCUS => {
                if let Some(state) = state_ptr.as_mut() {
                    debug!(
                        "[WndProc] WM_KILLFOCUS: child {:?} lost focus",
                        state.child_hwnd
                    );
                }
                LRESULT(0)
            }

            WM_CLOSE => {
                info!("[WndProc] WM_CLOSE -> DestroyWindow");
                DestroyWindow(hwnd);
                LRESULT(0)
            }

            WM_DESTROY => {
                info!("[WndProc] WM_DESTROY");
                // cleanup AppState
                if !state_ptr.is_null() {
                    debug!("[WndProc] Dropping AppState");
                    drop(Box::from_raw(state_ptr));
                    SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
                }
                PostQuitMessage(0);
                LRESULT(0)
            }

            WM_NCHITTEST => {
                // 1) First let the plugin see it.
                if let Some(state) =
                    (GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut AppState).as_mut()
                {
                    if !state.controller.is_null() {
                        let raw_hwnd: RawHWND = std::mem::transmute(hwnd);
                        let raw_msg: RawUINT = WM_NCHITTEST as _;
                        let raw_wp: RawWPARAM = wparam.0 as _;
                        let raw_lp: RawLPARAM = lparam.0 as _;
                        let mut raw_out: RawLRESULT = 0;

                        let handled = FlutterDesktopViewControllerHandleTopLevelWindowProc(
                            state.controller,
                            raw_hwnd,
                            raw_msg,
                            raw_wp,
                            raw_lp,
                            &mut raw_out as *mut _,
                        );
                        if handled {
                            // plugin returned something (maybe HTCAPTION or custom drag region)
                            return LRESULT(raw_out.try_into().unwrap());
                        }
                    }
                }

                // 2) Fallback to our own HTCLIENT→HTCAPTION hack
                let hit = DefWindowProcW(hwnd, msg, wparam, lparam);
                if hit.0 as u32 == HTCLIENT {
                    return LRESULT(HTCAPTION as isize);
                }
                hit
            }

            WM_DPICHANGED => {
                info!("[WndProc] WM_DPICHANGED");
                // suggested new bounds live at lparam
                let new_rc = lparam.0 as *const RECT;
                if !new_rc.is_null() {
                    let r = *new_rc;
                    SetWindowPos(
                        hwnd,
                        HWND(0),
                        r.left,
                        r.top,
                        r.right - r.left,
                        r.bottom - r.top,
                        SWP_NOZORDER | SWP_NOACTIVATE | SWP_ASYNCWINDOWPOS,
                    );
                }
                LRESULT(0)
            }

            WM_PAINT => {
                // we never paint the parent ourselves
                debug!("[WndProc] WM_PAINT forwarded");
                DefWindowProcW(hwnd, msg, wparam, lparam)
            }

            other => {
                // 1) give _every_ raw event to the engine for plugins
                if let Some(state) = state_ptr.as_mut() {
                    let engine = FlutterDesktopViewControllerGetEngine(state.controller);
                    let raw_hwnd: RawHWND = std::mem::transmute(hwnd);
                    let raw_msg: RawUINT = other as _;
                    let raw_wp: RawWPARAM = wparam.0 as _;
                    let raw_lp: RawLPARAM = lparam.0 as _;
                    let mut ext_out: RawLRESULT = 0;

                    let ext_handled = FlutterDesktopEngineProcessExternalWindowMessage(
                        engine,
                        raw_hwnd,
                        raw_msg,
                        raw_wp,
                        raw_lp,
                        &mut ext_out as *mut _,
                    );
                    if ext_handled {
                        return LRESULT(ext_out.try_into().unwrap());
                    }

                    // 2) then give it to the view controller
                    let mut view_out: RawLRESULT = 0;
                    let view_handled = FlutterDesktopViewControllerHandleTopLevelWindowProc(
                        state.controller,
                        raw_hwnd,
                        raw_msg,
                        raw_wp,
                        raw_lp,
                        &mut view_out as *mut _,
                    );
                    if view_handled {
                        return LRESULT(view_out.try_into().unwrap());
                    }
                }

                // 3) fallback to default
                DefWindowProcW(hwnd, other, wparam, lparam)
            }
        }
    }
}

//---------------------------------------------------------------------------
// Class registration, creation, embedding, message loop
//---------------------------------------------------------------------------

static REGISTER_CLASS_ONCE: Once = Once::new();

/// Register our window class exactly once. Panics if registration fails.
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
            hIcon: Default::default(),
            cbClsExtra: 0,
            cbWndExtra: 0,
        };
        if RegisterClassW(&wc) == 0 {
            panic!("[Win32 Utils] RegisterClassW failed: {:?}", GetLastError());
        }
        info!("[Win32 Utils] Window class registered");
    });
}

/// Create the parent window, passing `app_state_ptr` via `lpCreateParams`.
/// Panics (and cleans up) on failure.
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
            None::<&HWND>,
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

/// Embed the Flutter `child` into our `parent` window:
/// 1. strip WS_POPUP / WS_OVERLAPPEDWINDOW → add WS_CHILD & WS_VISIBLE  
/// 2. force a style update (`SWP_FRAMECHANGED`)  
/// 3. SetParent  
/// 4. MoveWindow → fill client area  
/// 5. Send WM_PAINT (no white flash)  
/// 6. Post WM_SIZE to retrigger Flutter’s viewport resize
pub fn set_flutter_window_as_child(parent: HWND, child: HWND) {
    info!(
        "[Win32 Utils] Embedding Flutter HWND {:?} into {:?}",
        child, parent
    );

    // 1) adjust style
    let old = unsafe { GetWindowLongPtrW(child, GWL_STYLE) };
    let new = (old & !(WS_POPUP.0 as isize | WS_OVERLAPPEDWINDOW.0 as isize))
        | WS_CHILD.0 as isize
        | WS_VISIBLE.0 as isize;
    unsafe {
        SetWindowLongPtrW(child, GWL_STYLE, new);
        // 2) force Windows to re-evaluate the frame
        SetWindowPos(
            child,
            HWND(0),
            0,
            0,
            0,
            0,
            SWP_FRAMECHANGED | SWP_NOZORDER | SWP_NOMOVE | SWP_NOSIZE,
        );
    }
    debug!("[Win32 Utils] Child style {:#x} → {:#x}", old, new);

    // 3) reparent
    let prev = unsafe { SetParent(child, parent) };
    let err = unsafe { GetLastError() };
    if err.0 != 0 {
        warn!("[Win32 Utils] SetParent error: {:?}", err);
    } else if prev.0 != 0 {
        debug!("[Win32 Utils] Child already under {:?}", prev);
    }

    // 4) full‐client resize
    let mut rc = RECT::default();
    if unsafe { GetClientRect(parent, &mut rc) }.as_bool() {
        let w = rc.right - rc.left;
        let h = rc.bottom - rc.top;
        unsafe {
            MoveWindow(child, 0, 0, w, h, true);
        }
    }

    // 5) immediate paint
    unsafe {
        SendMessageW(child, WM_PAINT, WPARAM(0), LPARAM(0));
    }
    debug!("[Win32 Utils] WM_PAINT sent to child");

    // 6) retrigger our WM_SIZE (and Flutter’s)
    unsafe {
        PostMessageW(parent, WM_SIZE, WPARAM(0), LPARAM(0));
    }
    debug!("[Win32 Utils] Posted WM_SIZE to parent");
}

/// Run the message loop until WM_QUIT, then drop any leftover `AppState`.
pub fn run_message_loop(parent: HWND, _app_state_ptr: *mut AppState) {
    info!("[Win32 Utils] Entering message loop");
    let mut msg = MSG::default();
    unsafe {
        while GetMessageW(&mut msg, HWND(0), 0, 0).as_bool() {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
    info!("[Win32 Utils] Exited message loop");

    // final AppState drop if still present
    let ptr = unsafe { GetWindowLongPtrW(parent, GWLP_USERDATA) as *mut AppState };
    if !ptr.is_null() {
        debug!("[Win32 Utils] Cleaning up AppState after loop");
        unsafe {
            drop(Box::from_raw(ptr));
        }
    }
}

/// Build a null-terminated UTF-16 string for Win32 APIs.
pub fn to_wide(s: &str) -> Vec<u16> {
    OsStr::new(s).encode_wide().chain(Some(0)).collect()
}

/// Panic helper that logs the last OS error before panicking.
pub fn panic_with_error(message: &str) -> ! {
    let err = std::io::Error::last_os_error();
    error!("{} OS error: {}", message, err);
    panic!("{}", message);
}

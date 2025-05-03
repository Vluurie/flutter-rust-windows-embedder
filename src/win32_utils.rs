//! Win32 helper functions for Flutter desktop embedding on Windows.
//!
//! This module lets you:
//! 1. Register a Win32 window class once.
//! 2. Create a top-level parent window and embed Flutter’s HWND as a *real* child.
//! 3. Handle every important window message—sizing, focus, DPI, non-client styling,
//!    dragging, etc.—forwarding first to the Flutter engine (plugins), then to the
//!    view controller, and finally to `DefWindowProcW`.
//! 4. Run the Win32 message loop until `WM_QUIT`.
//!
//! # Message flow
//!
//! - **WM_NCCREATE**: stash your `AppState` in `GWLP_USERDATA`.
//! - **WM_NCCALCSIZE / WM_NCPAINT / WM_NCACTIVATE / WM_NCUAHDRAWCAPTION / WM_NCUAHDRAWFRAME**:  
//!   1. First offer to **engine** via `FlutterDesktopEngineProcessExternalWindowMessage`.  
//!   2. Then to **view controller** via `HandleTopLevelWindowProc`.  
//!   3. Finally fallback to `DefWindowProcW`.
//! - **WM_SIZE**:  
//!     1. Resize the embedded Flutter child to fill the client area.  
//!     2. Forward the size message to Flutter so its viewport updates.
//! - **WM_ACTIVATE / WM_SETFOCUS**: forward focus to the Flutter child.
//! - **WM_KILLFOCUS**: log focus loss.
//! - **WM_CLOSE**: call `DestroyWindow` to trigger cleanup.
//! - **WM_DESTROY**: drop your `AppState` and post `WM_QUIT`.
//! - **WM_NCHITTEST**:  
//!     1. First offer to **engine** via `FlutterDesktopEngineProcessExternalWindowMessage`.  
//!     2. Then to **view controller** via `HandleTopLevelWindowProc`.  
//!     3. Finally, treat client-area hits as `HTCAPTION` so the user can drag your custom titlebar.
//! - **WM_DPICHANGED**: reposition/resize to new DPI suggestion.
//! - **WM_PAINT**: we never paint the parent—Flutter covers it.
//! - **All others**:  
//!     1. Offer first to **engine**.  
//!     2. Then to **view controller**.  
//!     3. Finally fallback to `DefWindowProcW`.
//!
//! # Usage
//!
//! ```ignore
//! win32_utils::register_window_class();
//! let parent = win32_utils::create_main_window(app_state_ptr);
//! win32_utils::set_flutter_window_as_child(parent, flutter_child_hwnd);
//! win32_utils::run_message_loop(parent, app_state_ptr);
//! ```
//!
//! # Safety
//! - The `wnd_proc` must be installed as `lpfnWndProc` in your `WNDCLASSW`.
//! - It assumes that `lpCreateParams` passed to `CreateWindowExW` / `WM_NCCREATE`
//!   is a valid `*mut AppState`.

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
            WM_NCCALCSIZE, WM_NCPAINT, WM_NCACTIVATE,
            CREATESTRUCTW, CS_HREDRAW, CS_VREDRAW, CreateWindowExW, DefWindowProcW, DestroyWindow,
            DispatchMessageW, GWL_STYLE, GWLP_USERDATA, GetClientRect, GetMessageW,
            GetWindowLongPtrW, HMENU, HTCAPTION, HTCLIENT, IDC_ARROW, LoadCursorW, MSG, MoveWindow,
            PostMessageW, PostQuitMessage, RegisterClassW, SWP_ASYNCWINDOWPOS, SWP_FRAMECHANGED,
            SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER, SendMessageW, SetParent,
            SetWindowLongPtrW, SetWindowPos, TranslateMessage, WINDOW_EX_STYLE, WM_ACTIVATE,
            WM_CLOSE, WM_DESTROY, WM_DPICHANGED, WM_KILLFOCUS,
            WM_NCCREATE, WM_NCHITTEST, WM_PAINT, WM_SETFOCUS, WM_SIZE, WNDCLASSW,
            WS_CHILD, WS_CLIPCHILDREN, WS_OVERLAPPEDWINDOW, WS_POPUP, WS_VISIBLE,
        },
    },
    core::PCWSTR,
};

const WM_NCUAHDRAWCAPTION: u32 = 0xAE;
const WM_NCUAHDRAWFRAME:   u32 = 0xAF;

#[link(name = "user32")]
unsafe extern "system" {
    /// Forward keyboard focus to a child HWND.
    fn SetFocus(hWnd: HWND) -> HWND;
}

//---------------------------------------------------------------------------
// Window procedure: lifecycle, sizing, focus, DPI, non-client, dragging,
// and forwarding to Flutter engine & view, then DefWindowProcW.
//---------------------------------------------------------------------------

/// The window procedure for our parent window.
/// 
/// # Safety
/// - Must be registered as `lpfnWndProc` in your `WNDCLASSW`.
/// - Assumes `WM_NCCREATE`’s `lpCreateParams` is a valid `*mut AppState`.
pub extern "system" fn wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    unsafe {
        // Retrieve our AppState pointer from GWLP_USERDATA.
        let state_ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut AppState;

        match msg {
            // 1) Non-client create: stash AppState
            WM_NCCREATE => {
                info!("[WndProc] WM_NCCREATE");
                if let Some(cs) = (lparam.0 as *const CREATESTRUCTW).as_ref() {
                    let ptr = cs.lpCreateParams as isize;
                    debug!("[WndProc] Storing AppState ptr {:?}", ptr);
                    SetWindowLongPtrW(hwnd, GWLP_USERDATA, ptr);
                } else {
                    warn!("[WndProc] CREATESTRUCTW was null");
                }
                // Default non-client processing.
                DefWindowProcW(hwnd, msg, wparam, lparam)
            }

            // 2) Non-client sizing/paint/activate + UxTheme draw calls
            WM_NCCALCSIZE | WM_NCPAINT | WM_NCACTIVATE
            | WM_NCUAHDRAWCAPTION | WM_NCUAHDRAWFRAME => {
                if let Some(state) = state_ptr.as_mut() {
                    // a) engine/plugins see every NC message
                    let engine = FlutterDesktopViewControllerGetEngine(state.controller);
                    let raw_hwnd: RawHWND = std::mem::transmute(hwnd);
                    let raw_msg: RawUINT = msg as _;
                    let raw_wp: RawWPARAM = wparam.0 as _;
                    let raw_lp: RawLPARAM = lparam.0 as _;
                    let mut raw_out: RawLRESULT = 0;

                    if FlutterDesktopEngineProcessExternalWindowMessage(
                        engine,
                        raw_hwnd,
                        raw_msg,
                        raw_wp,
                        raw_lp,
                        &mut raw_out as *mut _,
                    ) {
                        return LRESULT(raw_out.try_into().unwrap());
                    }

                    // b) view controller
                    let mut view_out: RawLRESULT = 0;
                    if FlutterDesktopViewControllerHandleTopLevelWindowProc(
                        state.controller,
                        raw_hwnd,
                        raw_msg,
                        raw_wp,
                        raw_lp,
                        &mut view_out as *mut _,
                    ) {
                        return LRESULT(view_out.try_into().unwrap());
                    }
                }
                // c) default
                DefWindowProcW(hwnd, msg, wparam, lparam)
            }

            // 3) Client-area resize → child + Flutter
            WM_SIZE => {
                if let Some(state) = state_ptr.as_mut() {
                    // a) resize native child
                    let mut rc = RECT::default();
                    if GetClientRect(hwnd, &mut rc).as_bool() {
                        MoveWindow(
                            state.child_hwnd,
                            0, 0,
                            rc.right - rc.left,
                            rc.bottom - rc.top,
                            true,
                        );
                    }

                    // b) forward WM_SIZE to Flutter view
                    let raw_hwnd: RawHWND = std::mem::transmute(hwnd);
                    let raw_wp: RawWPARAM = wparam.0 as _;
                    let raw_lp: RawLPARAM = lparam.0 as _;
                    let mut raw_out: RawLRESULT = 0;

                    if FlutterDesktopViewControllerHandleTopLevelWindowProc(
                        state.controller,
                        raw_hwnd,
                        WM_SIZE as _,
                        raw_wp,
                        raw_lp,
                        &mut raw_out as *mut _,
                    ) {
                        return LRESULT(raw_out.try_into().unwrap());
                    }
                }
                LRESULT(0)
            }

            // 4) Activation & focus → child
            WM_ACTIVATE | WM_SETFOCUS => {
                if let Some(state) = state_ptr.as_mut() {
                    debug!("[WndProc] focus event: {}", msg);
                    SetFocus(state.child_hwnd);
                }
                LRESULT(0)
            }

            // 5) Log focus loss
            WM_KILLFOCUS => {
                if let Some(_state) = state_ptr.as_mut() {
                    debug!("[WndProc] WM_KILLFOCUS: child lost focus");
                }
                LRESULT(0)
            }

            // 6) Close → DestroyWindow → WM_DESTROY
            WM_CLOSE => {
                info!("[WndProc] WM_CLOSE → DestroyWindow");
                DestroyWindow(hwnd);
                LRESULT(0)
            }

            // 7) Destroy → cleanup + quit
            WM_DESTROY => {
                info!("[WndProc] WM_DESTROY");
                if !state_ptr.is_null() {
                    drop(Box::from_raw(state_ptr));
                    SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
                }
                PostQuitMessage(0);
                LRESULT(0)
            }

            // 8) Hit-test → client clicks become caption (after engine/view)
            WM_NCHITTEST => {
                if let Some(state) = state_ptr.as_mut() {
                    // a) engine sees this too (so window_manager plugin can override)
                    let engine = FlutterDesktopViewControllerGetEngine(state.controller);
                    let raw_hwnd: RawHWND = std::mem::transmute(hwnd);
                    let raw_msg: RawUINT = WM_NCHITTEST as _;
                    let raw_wp: RawWPARAM = wparam.0 as _;
                    let raw_lp: RawLPARAM = lparam.0 as _;
                    let mut ext_out: RawLRESULT = 0;

                    if FlutterDesktopEngineProcessExternalWindowMessage(
                        engine,
                        raw_hwnd,
                        raw_msg,
                        raw_wp,
                        raw_lp,
                        &mut ext_out as *mut _,
                    ) {
                        return LRESULT(ext_out.try_into().unwrap());
                    }

                    // b) view controller
                    let mut view_out: RawLRESULT = 0;
                    if FlutterDesktopViewControllerHandleTopLevelWindowProc(
                        state.controller,
                        raw_hwnd,
                        raw_msg,
                        raw_wp,
                        raw_lp,
                        &mut view_out as *mut _,
                    ) {
                        return LRESULT(view_out.try_into().unwrap());
                    }
                }

                // c) fallback → HTCAPTION on client
                let hit = DefWindowProcW(hwnd, msg, wparam, lparam);
                if hit.0 as u32 == HTCLIENT {
                    return LRESULT(HTCAPTION as isize);
                }
                hit
            }

            // 9) DPI → reposition/resize
            WM_DPICHANGED => {
                let new_rc = lparam.0 as *const RECT;
                if !new_rc.is_null() {
                    let r = *new_rc;
                    SetWindowPos(
                        hwnd,
                        HWND(0),
                        r.left, r.top,
                        r.right - r.left,
                        r.bottom - r.top,
                        SWP_NOZORDER | SWP_NOACTIVATE | SWP_ASYNCWINDOWPOS,
                    );
                }
                LRESULT(0)
            }

            // 10) Paint → always default
            WM_PAINT => DefWindowProcW(hwnd, msg, wparam, lparam),

            // 11) Everything else → engine → view → default
            other => {
                if let Some(state) = state_ptr.as_mut() {
                    // a) engine/plugins
                    let engine = FlutterDesktopViewControllerGetEngine(state.controller);
                    let raw_hwnd: RawHWND = std::mem::transmute(hwnd);
                    let raw_msg: RawUINT = other as _;
                    let raw_wp: RawWPARAM = wparam.0 as _;
                    let raw_lp: RawLPARAM = lparam.0 as _;
                    let mut ext_out: RawLRESULT = 0;

                    if FlutterDesktopEngineProcessExternalWindowMessage(
                        engine,
                        raw_hwnd,
                        raw_msg,
                        raw_wp,
                        raw_lp,
                        &mut ext_out as *mut _,
                    ) {
                        return LRESULT(ext_out.try_into().unwrap());
                    }

                    // b) view controller
                    let mut view_out: RawLRESULT = 0;
                    if FlutterDesktopViewControllerHandleTopLevelWindowProc(
                        state.controller,
                        raw_hwnd,
                        raw_msg,
                        raw_wp,
                        raw_lp,
                        &mut view_out as *mut _,
                    ) {
                        return LRESULT(view_out.try_into().unwrap());
                    }
                }
                // c) fallback
                DefWindowProcW(hwnd, other, wparam, lparam)
            }
        }
    }
}

static REGISTER_CLASS_ONCE: Once = Once::new();

/// Register our Win32 window class exactly once. Panics on failure.
pub fn register_window_class() {
    REGISTER_CLASS_ONCE.call_once(|| unsafe {
        let hinst = GetModuleHandleW(None).expect("GetModuleHandleW failed");
        let wc = WNDCLASSW {
            hInstance:    hinst.into(),
            lpszClassName: constants::WINDOW_CLASS_NAME,
            lpfnWndProc:   Some(wnd_proc),
            style:        CS_HREDRAW | CS_VREDRAW,
            hCursor:      LoadCursorW(None, IDC_ARROW).unwrap_or_default(),
            hbrBackground: HBRUSH::default(),
            lpszMenuName:  PCWSTR::null(),
            hIcon:         Default::default(),
            cbClsExtra:   0,
            cbWndExtra:   0,
        };
        if RegisterClassW(&wc) == 0 {
            panic!("[Win32 Utils] RegisterClassW failed: {:?}", GetLastError());
        }
        info!("[Win32 Utils] Window class registered");
    });
}

/// Create the main parent window, passing `app_state_ptr` via `lpCreateParams`.
/// On failure cleans up and panics.
pub fn create_main_window(app_state_ptr: *mut AppState) -> HWND {
    info!("[Win32 Utils] Creating main window");
    let hwnd = unsafe {
        CreateWindowExW(
            WINDOW_EX_STYLE::default(),
            constants::WINDOW_CLASS_NAME,
            constants::WINDOW_TITLE,
            WS_OVERLAPPEDWINDOW | WS_VISIBLE | WS_CLIPCHILDREN,
            100, 100,
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
/// 1. Strip WS_POPUP/WS_OVERLAPPEDWINDOW → add WS_CHILD & WS_VISIBLE  
/// 2. Force a non-client frame recalculation (SWP_FRAMECHANGED)  
/// 3. SetParent → reparent the child  
/// 4. MoveWindow → fill the client area  
/// 5. Send WM_PAINT → no white flash  
/// 6. Post WM_SIZE → retrigger Flutter’s viewport resize
pub fn set_flutter_window_as_child(parent: HWND, child: HWND) {
    info!("[Win32 Utils] Embedding Flutter HWND {:?} into {:?}", child, parent);

    // adjust style
    let old = unsafe { GetWindowLongPtrW(child, GWL_STYLE) };
    let new = (old & !(WS_POPUP.0 as isize | WS_OVERLAPPEDWINDOW.0 as isize))
        | WS_CHILD.0 as isize
        | WS_VISIBLE.0 as isize;
    unsafe {
        SetWindowLongPtrW(child, GWL_STYLE, new);
        // force Windows to re-evaluate non-client
        SetWindowPos(
            child,
            HWND(0),
            0, 0, 0, 0,
            SWP_NOZORDER | SWP_NOMOVE | SWP_NOSIZE | SWP_FRAMECHANGED,
        );
    }
    debug!("[Win32 Utils] Child style {:#x} → {:#x}", old, new);

    // reparent
    let prev = unsafe { SetParent(child, parent) };
    let err = unsafe { GetLastError() };
    if err.0 != 0 {
        warn!("[Win32 Utils] SetParent error: {:?}", err);
    } else if prev.0 != 0 {
        debug!("[Win32 Utils] Child already under {:?}", prev);
    }

    // fill client area
    let mut rc = RECT::default();
    if unsafe { GetClientRect(parent, &mut rc) }.as_bool() {
        unsafe { MoveWindow(child, 0, 0, rc.right - rc.left, rc.bottom - rc.top, true) };
    }

    // no white flash
    unsafe { SendMessageW(child, WM_PAINT, WPARAM(0), LPARAM(0)); }
    // retrigger WM_SIZE
    unsafe { PostMessageW(parent, WM_SIZE, WPARAM(0), LPARAM(0)); }
}

/// Run the Win32 message loop until `WM_QUIT`, then drop any leftover `AppState`.
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

    // final AppState drop
    let ptr = unsafe { GetWindowLongPtrW(parent, GWLP_USERDATA) as *mut AppState };
    if !ptr.is_null() {
        drop(unsafe { Box::from_raw(ptr) });
    }
}

/// Build a null-terminated UTF-16 string for Win32 APIs.
pub fn to_wide(s: &str) -> Vec<u16> {
    OsStr::new(s).encode_wide().chain(Some(0)).collect()
}

/// Log the last OS error, then panic with the given message.
pub fn panic_with_error(message: &str) -> ! {
    let err = std::io::Error::last_os_error();
    error!("{} OS error: {}", message, err);
    panic!("{}", message);
}

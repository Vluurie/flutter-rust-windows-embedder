//! Win32 helper functions for registering/creating the main window,
//! embedding the Flutter child HWND (as a *real* child window), and
//! running the message loop.
//!
//! Stripping `WS_POPUP` and adding `WS_CHILD` on the Flutter view
//! lets the parent window’s non-client (titlebar) hit-testing work
//! so you can drag the window header normally.
//!
//! ## Handled Messages
//!
//! - **WM_NCCREATE**: Capture and store our `AppState` pointer.
//! - **WM_SIZE**: Resize the Flutter child to fill our client area.
//! - **WM_ACTIVATE** / **WM_SETFOCUS**: Forward keyboard focus to the Flutter child.
//! - **WM_KILLFOCUS**: Log when focus is lost.
//! - **WM_CLOSE**: Invoke `DestroyWindow`, triggering cleanup.
//! - **WM_DESTROY**: Drop `AppState` and post `WM_QUIT`.
//! - **WM_NCHITTEST**: Translate client-area hits into `HTCAPTION` so titlebar dragging works.
//! - **WM_DPICHANGED**: Reposition/resize to the new DPI-aware bounds.
//! - **WM_PAINT**: Forward to `DefWindowProcW`; the child covers our client area.
//! - **All others**: First offered to Flutter via `HandleTopLevelWindowProc`, then
//!   fallback to `DefWindowProcW`.

use crate::{
    app_state::AppState,
    constants,
    flutter_bindings::{
        self, FlutterDesktopViewControllerHandleTopLevelWindowProc, HWND as RawHWND,
        LPARAM as RawLPARAM, LRESULT as RawLRESULT, UINT as RawUINT, WPARAM as RawWPARAM,
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
            GetWindowLongPtrW, HICON, HMENU, HTCAPTION, HTCLIENT, IDC_ARROW, LoadCursorW, MSG,
            MoveWindow, PostMessageW, PostQuitMessage, RegisterClassW, SWP_ASYNCWINDOWPOS,
            SWP_NOACTIVATE, SWP_NOZORDER, SendMessageW, SetParent, SetWindowLongPtrW, SetWindowPos,
            TranslateMessage, WINDOW_EX_STYLE, WM_ACTIVATE, WM_CLOSE, WM_DESTROY, WM_DPICHANGED,
            WM_KILLFOCUS, WM_NCCREATE, WM_NCHITTEST, WM_PAINT, WM_SETFOCUS, WM_SIZE, WNDCLASSW,
            WS_CHILD, WS_CLIPCHILDREN, WS_OVERLAPPEDWINDOW, WS_POPUP, WS_VISIBLE,
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
// Window procedure: manages AppState lifecycle, resizing, focus, DPI changes,
// and delegates unhandled messages to Flutter’s `HandleTopLevelWindowProc`.
//---------------------------------------------------------------------------

/// Our window proc:
/// - Stores/drops the `AppState` pointer.
/// - Keeps the Flutter child sized to fill our client area.
/// - Forwards focus events to the Flutter child.
/// - Handles window closing and DPI changes.
/// - Delegates all other messages to Flutter, then falls back to
///   `DefWindowProcW`.
///
/// # Safety
/// - Must be registered via `WNDCLASSW::lpfnWndProc`.
/// - Assumes `lpCreateParams` in `WM_NCCREATE` is a valid `*mut AppState`.
pub unsafe extern "system" fn wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    unsafe {
        // Retrieve the `AppState` pointer we stashed in WM_NCCREATE
        let state_ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut AppState;

        match msg {
            WM_NCCREATE => {
                info!("[WndProc] WM_NCCREATE");
                if let Some(cs) = (lparam.0 as *const CREATESTRUCTW).as_ref() {
                    let ptr = cs.lpCreateParams as isize;
                    debug!("[WndProc] Storing AppState ptr {:?}", ptr);
                    SetWindowLongPtrW(hwnd, GWLP_USERDATA, ptr);
                } else {
                    warn!("[WndProc] CREATESTRUCTW was null");
                }
                // Default non-client create processing
                DefWindowProcW(hwnd, msg, wparam, lparam)
            }

            WM_SIZE => {
                // Resize Flutter child to fill client
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
                }
                LRESULT(0)
            }

            WM_ACTIVATE => {
                // When window activates, forward focus
                if let Some(state) = state_ptr.as_mut() {
                    debug!("[WndProc] WM_ACTIVATE: focusing {:?}", state.child_hwnd);
                    SetFocus(state.child_hwnd);
                }
                LRESULT(0)
            }

            WM_SETFOCUS => {
                // When window gets focus, forward
                if let Some(state) = state_ptr.as_mut() {
                    debug!("[WndProc] WM_SETFOCUS: focusing {:?}", state.child_hwnd);
                    SetFocus(state.child_hwnd);
                }
                LRESULT(0)
            }

            WM_KILLFOCUS => {
                // Log focus loss
                if let Some(state) = state_ptr.as_mut() {
                    debug!(
                        "[WndProc] WM_KILLFOCUS: child {:?} lost focus",
                        state.child_hwnd
                    );
                }
                LRESULT(0)
            }

            WM_CLOSE => {
                info!("[WndProc] WM_CLOSE");
                // Triggers WM_DESTROY
                DestroyWindow(hwnd);
                LRESULT(0)
            }

            WM_DESTROY => {
                info!("[WndProc] WM_DESTROY");
                // Cleanup AppState and post quit
                if !state_ptr.is_null() {
                    debug!("[WndProc] Dropping AppState");
                    drop(Box::from_raw(state_ptr));
                    SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
                }
                PostQuitMessage(0);
                LRESULT(0)
            }

            WM_NCHITTEST => {
                let hit = DefWindowProcW(hwnd, msg, wparam, lparam);
                if (hit.0 as u32) == HTCLIENT {
                    return LRESULT(HTCAPTION as isize);
                }
                hit
            }

            WM_DPICHANGED => {
                info!("[WndProc] WM_DPICHANGED");
                // lparam → pointer to suggested new bounds
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
                    // child will follow on next WM_SIZE
                }
                LRESULT(0)
            }

            WM_PAINT => {
                // Parent never paints itself; child covers client
                debug!("[WndProc] WM_PAINT forwarded");
                DefWindowProcW(hwnd, msg, wparam, lparam)
            }

            other => {
                // Delegate all other messages to Flutter
                if let Some(state) = state_ptr.as_mut() {
                    if !state.controller.is_null() {
                        let raw_hwnd: RawHWND = std::mem::transmute(hwnd);
                        let raw_msg: RawUINT = other as _;
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
                            return LRESULT(raw_out.try_into().unwrap());
                        }
                    }
                }
                // Fallback
                DefWindowProcW(hwnd, other, wparam, lparam)
            }
        }
    }
}

//---------------------------------------------------------------------------
// Class registration / creation / embedding / message loop
//---------------------------------------------------------------------------

static REGISTER_CLASS_ONCE: Once = Once::new();

/// Registers our window class (once). Must be called before
/// `create_main_window`.  
///  
/// # Panics
/// If `RegisterClassW` fails.
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
            panic!("[Win32 Utils] RegisterClassW failed: {:?}", GetLastError());
        }
        info!("[Win32 Utils] Window class registered");
    });
}

/// Creates the main parent window, passing our `AppState` in `lpCreateParams`.
///
/// # Panics
/// On failure cleans up COM and AppState, then panics.
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

/// Embeds the Flutter `child` into `parent`, resizes, then forces an initial paint.
pub fn set_flutter_window_as_child(parent: HWND, child: HWND) {
    info!(
        "[Win32 Utils] Embedding Flutter HWND {:?} into {:?}",
        child, parent
    );

    // 1) Strip WS_POPUP/WS_OVERLAPPEDWINDOW, add WS_CHILD & WS_VISIBLE
    let old = unsafe { GetWindowLongPtrW(child, GWL_STYLE) };
    let new = (old & !(WS_POPUP.0 as isize | WS_OVERLAPPEDWINDOW.0 as isize))
        | WS_CHILD.0 as isize
        | WS_VISIBLE.0 as isize;
    unsafe { SetWindowLongPtrW(child, GWL_STYLE, new) };
    debug!("[Win32 Utils] Child style {:#x} → {:#x}", old, new);

    // 2) Reparent
    let prev = unsafe { SetParent(child, parent) };
    let err = unsafe { GetLastError() };
    if err.0 != 0 {
        warn!("[Win32 Utils] SetParent error: {:?}", err);
    } else if prev.0 != 0 {
        debug!("[Win32 Utils] Child already under {:?}", prev);
    }

    // 3) Resize to client
    let mut rc = RECT::default();
    if unsafe { GetClientRect(parent, &mut rc) }.as_bool() {
        let w = rc.right - rc.left;
        let h = rc.bottom - rc.top;
        debug!("[Win32 Utils] Parent client = {}×{}", w, h);
        unsafe { MoveWindow(child, 0, 0, w, h, true) };
    }

    // 4) Immediate paint of the child so don’t get a white flash
    unsafe {
        SendMessageW(child, WM_PAINT, WPARAM(0), LPARAM(0));
    }
    debug!("[Win32 Utils] WM_PAINT sent to child");

    // 5) Force our own WM_SIZE to re-fire
    //    This retriggers our WM_SIZE handler
    //    (which reapplies MoveWindow + any style/focus fixes).
    unsafe {
        PostMessageW(parent, WM_SIZE, WPARAM(0), LPARAM(0));
    }
    debug!("[Win32 Utils] Posted WM_SIZE to parent");
}

/// Runs the Win32 message loop until `WM_QUIT`, then drops any leftover `AppState`.
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

    // Final cleanup if needed
    let ptr = unsafe { GetWindowLongPtrW(parent, GWLP_USERDATA) as *mut AppState };
    if !ptr.is_null() {
        debug!("[Win32 Utils] Cleaning up AppState after loop");
        unsafe { drop(Box::from_raw(ptr)) };
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

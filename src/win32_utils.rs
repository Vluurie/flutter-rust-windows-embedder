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
//! - **WM_SIZE**:  
//!   1) Resize the Flutter child to fill our client area.  
//!   2) Forward `WM_SIZE` into Flutter so it can resize its viewport.  
//! - **WM_ACTIVATE** / **WM_SETFOCUS**: Forward keyboard focus to the Flutter child.
//! - **WM_KILLFOCUS**: Log when focus is lost.
//! - **WM_CLOSE**: Invoke `DestroyWindow`, triggering cleanup.
//! - **WM_DESTROY**: Drop `AppState` and post `WM_QUIT`.
//! - **WM_NCHITTEST**: Translate client-area hits into `HTCAPTION` so titlebar dragging works.
//! - **WM_DPICHANGED**: Reposition/resize to the new DPI-aware bounds.
//! - **WM_PAINT**: Forward to `DefWindowProcW`; the child covers our client area.
//! - **All others**:  
//!   1) First offered to the **engine** via `FlutterDesktopEngineProcessExternalWindowMessage` (so plugins get _every_ raw event) :contentReference[oaicite:0]{index=0}:contentReference[oaicite:1]{index=1}  
//!   2) Then to the **view** via `HandleTopLevelWindowProc` :contentReference[oaicite:2]{index=2}:contentReference[oaicite:3]{index=3}  
//!   3) Finally, fallback to `DefWindowProcW`.

use crate::{
    app_state::AppState,
    constants,
    flutter_bindings::{
        self,
        FlutterDesktopViewControllerGetEngine,
        FlutterDesktopEngineProcessExternalWindowMessage,
        FlutterDesktopViewControllerHandleTopLevelWindowProc,
        HWND   as RawHWND,
        WPARAM as RawWPARAM,
        LPARAM as RawLPARAM,
        LRESULT as RawLRESULT,
        UINT   as RawUINT,
    },
};
use log::{debug, error, info, warn};
use std::{ffi::c_void, ffi::OsStr, os::windows::ffi::OsStrExt, sync::Once};
use windows::{
    core::PCWSTR,
    Win32::{
        Foundation::{GetLastError, HWND, LPARAM, LRESULT, RECT, WPARAM},
        Graphics::Gdi::HBRUSH,
        System::LibraryLoader::GetModuleHandleW,
        UI::WindowsAndMessaging::{
            CREATESTRUCTW, CS_HREDRAW, CS_VREDRAW, CreateWindowExW, DefWindowProcW,
            DestroyWindow, DispatchMessageW, GWL_STYLE, GWLP_USERDATA, GetClientRect,
            GetMessageW, GetWindowLongPtrW, HICON, HMENU, HTCAPTION, HTCLIENT, IDC_ARROW,
            LoadCursorW, MoveWindow, PostMessageW, PostQuitMessage, RegisterClassW,
            SendMessageW, SetParent, SetWindowLongPtrW, SetWindowPos, TranslateMessage,
            WINDOW_EX_STYLE, WM_ACTIVATE, WM_CLOSE, WM_DESTROY, WM_DPICHANGED, WM_KILLFOCUS,
            WM_NCCREATE, WM_NCHITTEST, WM_PAINT, WM_SETFOCUS, WM_SIZE, WNDCLASSW, WS_CHILD,
            WS_CLIPCHILDREN, WS_OVERLAPPEDWINDOW, WS_POPUP, WS_VISIBLE, SWP_ASYNCWINDOWPOS,
            SWP_NOACTIVATE, SWP_NOZORDER, MSG,
        },
    },
};

#[link(name = "user32")]
unsafe extern "system" {
    /// Forward keyboard focus to a child HWND.
    fn SetFocus(hWnd: HWND) -> HWND;
}

//---------------------------------------------------------------------------
// Window procedure: manages AppState lifecycle, resizing, focus, DPI changes,
// and delegates unhandled messages first to the engine, then to Flutter’s
// HandleTopLevelWindowProc, then finally to DefWindowProcW.
//---------------------------------------------------------------------------

/// Our window proc:
/// 1. Stores/drops the `AppState` pointer in `WM_NCCREATE` / `WM_DESTROY`  
/// 2. Keeps the Flutter child sized to fill our client area on `WM_SIZE`  
/// 3. Forwards `WM_SIZE` into Flutter so its viewport resizes correctly  
/// 4. Forwards focus events to the Flutter child  
/// 5. Handles window closing & DPI changes  
/// 6. **Delegates all other messages** to the **engine** (so plugins get every event) :contentReference[oaicite:4]{index=4}:contentReference[oaicite:5]{index=5}  
/// 7. Then to the **view controller** via `HandleTopLevelWindowProc` :contentReference[oaicite:6]{index=6}:contentReference[oaicite:7]{index=7}  
/// 8. Finally falls back to `DefWindowProcW`  
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
    // Pull out our AppState pointer
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
            // Default non-client creation
            DefWindowProcW(hwnd, msg, wparam, lparam)
        }

        WM_SIZE => {
            // 1) Resize our Flutter child
            if let Some(state) = state_ptr.as_mut() {
                let mut rc = RECT::default();
                if GetClientRect(hwnd, &mut rc).as_bool() {
                    let w = rc.right - rc.left;
                    let h = rc.bottom - rc.top;
                    debug!("[WndProc] Resizing child {:?} to {}×{}", state.child_hwnd, w, h);
                    MoveWindow(state.child_hwnd, 0, 0, w, h, true);
                }

                // 2) Forward WM_SIZE into Flutter itself
                let raw_hwnd: RawHWND   = std::mem::transmute(hwnd);
                let raw_wp:    RawWPARAM = wparam.0 as _;
                let raw_lp:    RawLPARAM = lparam.0 as _;
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
                    return LRESULT(raw_out.try_into().unwrap())
                }
            }
            // If Flutter didn’t handle it, return 0
            LRESULT(0)
        }

        WM_ACTIVATE | WM_SETFOCUS => {
            // Forward focus to child
            if let Some(state) = state_ptr.as_mut() {
                debug!("[WndProc] focus event: {:?}", msg);
                SetFocus(state.child_hwnd);
            }
            LRESULT(0)
        }

        WM_KILLFOCUS => {
            // Log loss of focus
            if let Some(state) = state_ptr.as_mut() {
                debug!("[WndProc] WM_KILLFOCUS: child {:?} lost focus", state.child_hwnd);
            }
            LRESULT(0)
        }

        WM_CLOSE => {
            info!("[WndProc] WM_CLOSE → DestroyWindow");
            DestroyWindow(hwnd);
            LRESULT(0)
        }

        WM_DESTROY => {
            info!("[WndProc] WM_DESTROY");
            if !state_ptr.is_null() {
                debug!("[WndProc] Dropping AppState");
                drop(Box::from_raw(state_ptr));
                SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
            }
            PostQuitMessage(0);
            LRESULT(0)
        }

        WM_NCHITTEST => {
            // Map client → caption so titlebar dragging works
            let hit = DefWindowProcW(hwnd, msg, wparam, lparam);
            if hit.0 as u32 == HTCLIENT {
                // pretend the click was on the caption
                return LRESULT(HTCAPTION as isize);
            }
            hit
        }

        WM_DPICHANGED => {
            info!("[WndProc] WM_DPICHANGED");
            // lParam is a *const RECT of new bounds
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

        WM_PAINT => {
            // Parent never paints; child covers entire client area
            debug!("[WndProc] WM_PAINT → forwarded");
            DefWindowProcW(hwnd, msg, wparam, lparam)
        }

        other => {
            // 1) Let the **engine** see every raw event (so plugins get 'em) :contentReference[oaicite:8]{index=8}:contentReference[oaicite:9]{index=9}
            if let Some(state) = state_ptr.as_mut() {
                let engine = FlutterDesktopViewControllerGetEngine(state.controller);
                let raw_hwnd: RawHWND   = std::mem::transmute(hwnd);
                let raw_msg:   RawUINT   = other as _;
                let raw_wp:    RawWPARAM = wparam.0 as _;
                let raw_lp:    RawLPARAM = lparam.0 as _;
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

                // 2) Then let the **view** handle it
                let mut raw_out: RawLRESULT = 0;
                let view_handled =
                    FlutterDesktopViewControllerHandleTopLevelWindowProc(
                        state.controller,
                        raw_hwnd,
                        raw_msg,
                        raw_wp,
                        raw_lp,
                        &mut raw_out as *mut _,
                    );
                if view_handled {
                    return LRESULT(raw_out.try_into().unwrap());
                }
            }
            // 3) Fallback
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
/// Panics if `RegisterClassW` fails.
pub fn register_window_class() {
    REGISTER_CLASS_ONCE.call_once(|| unsafe {
        let hinst = GetModuleHandleW(None).expect("GetModuleHandleW failed");
        let wc = WNDCLASSW {
            hInstance:     hinst.into(),
            lpszClassName: constants::WINDOW_CLASS_NAME,
            lpfnWndProc:   Some(wnd_proc),
            style:         CS_HREDRAW | CS_VREDRAW,
            hCursor:       LoadCursorW(None, IDC_ARROW).unwrap_or_default(),
            hbrBackground: HBRUSH::default(),
            lpszMenuName:  PCWSTR::null(),
            hIcon:         HICON::default(),
            cbClsExtra:    0,
            cbWndExtra:    0,
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
/// On failure, cleans up COM, drops `AppState`, then panics.
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
            None, HMENU::default(),
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

/// Embeds the Flutter `child` into `parent`, strips WS_POPUP→CHILD,
/// re-parents, resizes, does an immediate paint, then posts WM_SIZE
/// so everything fires in the right order.
pub fn set_flutter_window_as_child(parent: HWND, child: HWND) {
    info!("[Win32 Utils] Embedding Flutter HWND {:?} into {:?}", child, parent);

    // 1) Strip WS_POPUP/WS_OVERLAPPEDWINDOW → add WS_CHILD & WS_VISIBLE
    let old = unsafe { GetWindowLongPtrW(child, GWL_STYLE) };
    let new = (old & !(WS_POPUP.0 as isize | WS_OVERLAPPEDWINDOW.0 as isize))
        | WS_CHILD.0 as isize
        | WS_VISIBLE.0 as isize;
    unsafe { SetWindowLongPtrW(child, GWL_STYLE, new) };
    debug!("[Win32 Utils] Child style {:#x} → {:#x}", old, new);

    // 2) Reparent
    let prev = unsafe { SetParent(child, parent) };
    let err  = unsafe { GetLastError() };
    if err.0 != 0 {
        warn!("[Win32 Utils] SetParent error: {:?}", err);
    } else if prev.0 != 0 {
        debug!("[Win32 Utils] Child already under {:?}", prev);
    }

    // 3) Resize to fill
    let mut rc = RECT::default();
    if unsafe { GetClientRect(parent, &mut rc) }.as_bool() {
        let w = rc.right - rc.left;
        let h = rc.bottom - rc.top;
        unsafe { MoveWindow(child, 0, 0, w, h, true) };
    }

    // 4) Immediate paint
    unsafe { SendMessageW(child, WM_PAINT, WPARAM(0), LPARAM(0)); }
    debug!("[Win32 Utils] WM_PAINT sent to child");

    // 5) Retrigger WM_SIZE on parent so our handler (and Flutter) re-fires
    unsafe { PostMessageW(parent, WM_SIZE, WPARAM(0), LPARAM(0)); }
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

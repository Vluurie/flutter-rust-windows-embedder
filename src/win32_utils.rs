//! Win32 helper functions for registering/creating the main window,
//! embedding the Flutter child HWND (as a *real* child window), and
//! running the message loop.
//!
//! Stripping `WS_POPUP` and adding `WS_CHILD` on the Flutter view
//! lets the parent window’s non-client (titlebar) hit-testing work
//! so you can drag the window header normally.

use crate::{
    app_state::AppState,
    constants,
    flutter_bindings::{
        self, FlutterDesktopViewControllerHandleTopLevelWindowProc, HWND   as RawHWND, LPARAM as RawLPARAM, LRESULT as RawLRESULT, UINT   as RawUINT, WPARAM as RawWPARAM
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
            // window-style constants:
            GWL_STYLE,
            WS_CHILD, WS_POPUP,
            WS_OVERLAPPEDWINDOW, WS_VISIBLE, WS_CLIPCHILDREN,
            // window creation & management:
            CreateWindowExW, DefWindowProcW, DispatchMessageW,
            GetClientRect, GetMessageW, GetWindowLongPtrW,
            LoadCursorW, MoveWindow, PostQuitMessage, RegisterClassW,
            SendMessageW, SetParent, SetWindowLongPtrW,
            TranslateMessage,
            // class/proc & messages:
            CREATESTRUCTW, CS_HREDRAW, CS_VREDRAW, GWLP_USERDATA,
            HICON, HMENU, IDC_ARROW, MSG, WINDOW_EX_STYLE,
            WM_ACTIVATE, WM_DESTROY, WM_NCCREATE, WM_PAINT, WM_SIZE, WNDCLASSW,
        },
    },
};

#[link(name = "user32")]
unsafe extern "system" {
    /// Forward focus into a child window.
    fn SetFocus(hWnd: HWND) -> HWND;
}

//---------------------------------------------------------------------------
// Window procedure: manages AppState lifecycle, resizing, focus, and
// delegates unhandled messages down to Flutter’s `HandleTopLevelWindowProc`.
//---------------------------------------------------------------------------

/// Our window proc: stores/drops `AppState`, resizes & focuses the Flutter child,
/// and delegates any other message to Flutter before falling back to `DefWindowProcW`.
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
            // Pull out the `lpCreateParams` (our AppState pointer).
            if let Some(cs) = (lparam.0 as *const CREATESTRUCTW).as_ref() {
                let ptr = cs.lpCreateParams as isize;
                debug!("[WndProc] Storing AppState ptr {:?}", ptr);
                SetWindowLongPtrW(hwnd, GWLP_USERDATA, ptr);
            } else {
                warn!("[WndProc] CREATESTRUCTW was null");
            }
            // Let default processing happen for non-client create.
            DefWindowProcW(hwnd, msg, wparam, lparam)
        }

        WM_SIZE => {
            // Keep Flutter child sized to fill our client area.
            if let Some(state) = state_ptr.as_mut() {
                let mut rc = RECT::default();
                if GetClientRect(hwnd, &mut rc).as_bool() {
                    let w = rc.right - rc.left;
                    let h = rc.bottom - rc.top;
                    debug!("[WndProc] Resizing child {:?} to {}×{}", state.child_hwnd, w, h);
                    MoveWindow(state.child_hwnd, 0, 0, w, h, true);
                }
            }
            LRESULT(0)
        }

        WM_ACTIVATE => {
            // Forward keyboard focus to the Flutter child when we activate.
            if let Some(state) = state_ptr.as_mut() {
                debug!("[WndProc] WM_ACTIVATE: focusing {:?}", state.child_hwnd);
                SetFocus(state.child_hwnd);
            }
            LRESULT(0)
        }

        WM_DESTROY => {
            info!("[WndProc] WM_DESTROY");
            // Clean up our AppState on window destruction.
            if !state_ptr.is_null() {
                debug!("[WndProc] Dropping AppState");
                drop(Box::from_raw(state_ptr));
                SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
            }
            PostQuitMessage(0);
            LRESULT(0)
        }

        _ => {
            // Let Flutter handle it first.
            if let Some(state) = state_ptr.as_mut() {
                if !state.controller.is_null() {
                    // Cast into the raw C types we bound.
                    let raw_hwnd: RawHWND   = std::mem::transmute(hwnd);
                    let raw_msg:   RawUINT   = msg   as _;
                    let raw_wp:    RawWPARAM = wparam.0 as _;
                    let raw_lp:    RawLPARAM = lparam.0 as _;
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
                        // Wrap back into windows-rs LRESULT and return.
                        return LRESULT(raw_out.try_into().unwrap());
                    }
                }
            }
            // Fallback to default behavior.
            DefWindowProcW(hwnd, msg, wparam, lparam)
        }
    }
}
}

//---------------------------------------------------------------------------
// Class registration / creation / embedding / message loop
//---------------------------------------------------------------------------

static REGISTER_CLASS_ONCE: Once = Once::new();

/// Registers the Win32 window class (once). Panics on failure.
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

/// Creates the main parent window, passing our `AppState` pointer via `lpCreateParams`.
/// On failure, cleans up and panics.
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
        // cleanup on error
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

/// Embeds the Flutter `child` window into `parent`:
/// 1) Downgrade its style from POPUP→CHILD so non-client hits go to parent  
/// 2) Reparent with `SetParent`  
/// 3) Resize to fill the client area  
/// 4) Send `WM_PAINT` immediately to avoid white flashes
pub fn set_flutter_window_as_child(parent: HWND, child: HWND) {
    info!("[Win32 Utils] Embedding Flutter HWND {:?} into {:?}", child, parent);

    // —— 1) Strip WS_POPUP / WS_OVERLAPPEDWINDOW, add WS_CHILD & WS_VISIBLE —— //
    let old_style = unsafe { GetWindowLongPtrW(child, GWL_STYLE) };
    let new_style = (old_style
        & !(WS_POPUP.0 as isize | WS_OVERLAPPEDWINDOW.0 as isize))
        | WS_CHILD.0 as isize
        | WS_VISIBLE.0 as isize;
    unsafe {
        SetWindowLongPtrW(child, GWL_STYLE, new_style);
    }
    debug!("[Win32 Utils] Updated child style: {:#x} → {:#x}", old_style, new_style);

    // —— 2) Reparent —— //
    let prev = unsafe { SetParent(child, parent) };
    let err  = unsafe { GetLastError() };
    if err.0 != 0 {
        warn!("[Win32 Utils] SetParent error: {:?}", err);
    } else if prev.0 != 0 {
        debug!("[Win32 Utils] Child was already parented under {:?}", prev);
    }

    // —— 3) Resize to fill —— //
    let mut rc = RECT::default();
    if unsafe { GetClientRect(parent, &mut rc) }.as_bool() {
        let w = rc.right - rc.left;
        let h = rc.bottom - rc.top;
        debug!("[Win32 Utils] Parent client size = {}×{}", w, h);
        unsafe { MoveWindow(child, 0, 0, w, h, true) };
    } else {
        warn!("[Win32 Utils] GetClientRect failed: {:?}", unsafe { GetLastError() });
    }

    // —— 4) Immediate paint —— //
    unsafe {
        SendMessageW(child, WM_PAINT, WPARAM(0), LPARAM(0));
    }
    debug!("[Win32 Utils] Sent WM_PAINT to child for initial draw");
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

/// Helper: build a wide (UTF-16) null-terminated string.
pub fn to_wide(s: &str) -> Vec<u16> {
    OsStr::new(s).encode_wide().chain(Some(0)).collect()
}

/// Panic helper that logs the last OS error.
pub fn panic_with_error(message: &str) -> ! {
    let err = std::io::Error::last_os_error();
    error!("{} OS error: {}", message, err);
    panic!("{}", message);
}

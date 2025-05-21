use std::ptr;
use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};

use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::Graphics::Gdi::ScreenToClient;
use windows::Win32::UI::WindowsAndMessaging::{
    self, HTCLIENT, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MBUTTONDOWN, WM_MBUTTONUP, WM_MOUSEMOVE,
    WM_MOUSEWHEEL, WM_NCMOUSELEAVE, WM_RBUTTONDOWN, WM_RBUTTONUP,
};

use winapi::shared::windef::HCURSOR as WinApiHCURSOR;
use winapi::um::winuser::{
    MK_LBUTTON as WINAPI_MK_LBUTTON, MK_MBUTTON as WINAPI_MK_MBUTTON,
    MK_RBUTTON as WINAPI_MK_RBUTTON, WHEEL_DELTA,
};

use crate::embedder::{
    FlutterEngine, FlutterEngineResult,
    FlutterPointerDeviceKind_kFlutterPointerDeviceKindMouse, FlutterPointerEvent,
    FlutterPointerPhase, FlutterPointerPhase_kAdd, FlutterPointerPhase_kDown,
    FlutterPointerPhase_kHover, FlutterPointerPhase_kMove, FlutterPointerPhase_kRemove,
    FlutterPointerPhase_kUp, FlutterPointerSignalKind_kFlutterPointerSignalKindNone,
    FlutterPointerSignalKind_kFlutterPointerSignalKindScroll,
};

use crate::software_renderer::dynamic_flutter_engine_dll_loader::FlutterEngineDll;
use crate::software_renderer::overlay::platform_message_callback::DESIRED_FLUTTER_CURSOR;

static MOUSE_BUTTONS_STATE: AtomicI32 = AtomicI32::new(0);
static IS_MOUSE_ADDED: AtomicBool = AtomicBool::new(false);

pub(crate) fn process_flutter_pointer_event_internal(
    engine: FlutterEngine,       
    engine_dll: &FlutterEngineDll,
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    flutter_is_active_and_can_process: bool,
) -> bool {
    unsafe {

        if engine.is_null() {
            return false;
        }

        if !flutter_is_active_and_can_process {
            if msg == WM_LBUTTONDOWN || msg == WM_LBUTTONUP || msg == WM_MOUSEMOVE {}
            return false;
        }

        match msg {
            WM_MOUSEMOVE => {
                let x = (lparam.0 & 0xFFFF) as i16 as f64;
                let y = ((lparam.0 >> 16) & 0xFFFF) as i16 as f64;
                let key_states_from_wparam = wparam.0 as usize;
                let mut calculated_mk_buttons_i32: i32 = 0;

                if (key_states_from_wparam & WINAPI_MK_LBUTTON) != 0 {
                    calculated_mk_buttons_i32 |= WINAPI_MK_LBUTTON as i32;
                }
                if (key_states_from_wparam & WINAPI_MK_RBUTTON) != 0 {
                    calculated_mk_buttons_i32 |= WINAPI_MK_RBUTTON as i32;
                }
                if (key_states_from_wparam & WINAPI_MK_MBUTTON) != 0 {
                    calculated_mk_buttons_i32 |= WINAPI_MK_MBUTTON as i32;
                }

                let current_buttons_state = calculated_mk_buttons_i32;
                let _previous_buttons_state_for_move = MOUSE_BUTTONS_STATE.load(Ordering::Relaxed);

                let phase = if current_buttons_state != 0 {
                    FlutterPointerPhase_kMove
                } else {
                    if !IS_MOUSE_ADDED.load(Ordering::SeqCst) {
                        IS_MOUSE_ADDED.store(true, Ordering::SeqCst);
                        FlutterPointerPhase_kAdd
                    } else {
                        FlutterPointerPhase_kHover
                    }
                };

                MOUSE_BUTTONS_STATE.store(current_buttons_state, Ordering::Relaxed);
                send_pointer_event_to_flutter(
                    engine,
                    engine_dll,
                    phase,
                    x,
                    y,
                    0.0,
                    0.0,
                    current_buttons_state as i64,
                );
                true
            }
            WM_LBUTTONDOWN | WM_RBUTTONDOWN | WM_MBUTTONDOWN => {
                let x = (lparam.0 & 0xFFFF) as i16 as f64;
                let y = ((lparam.0 >> 16) & 0xFFFF) as i16 as f64;
                let button_flag_to_set: i32 = match msg {
                    WM_LBUTTONDOWN => WINAPI_MK_LBUTTON as i32,
                    WM_RBUTTONDOWN => WINAPI_MK_RBUTTON as i32,
                    WM_MBUTTONDOWN => WINAPI_MK_MBUTTON as i32,
                    _ => 0,
                };
                let mut new_button_state = MOUSE_BUTTONS_STATE.load(Ordering::Relaxed);
                new_button_state |= button_flag_to_set;
                MOUSE_BUTTONS_STATE.store(new_button_state, Ordering::Relaxed);

                if !IS_MOUSE_ADDED.load(Ordering::SeqCst) {
                    IS_MOUSE_ADDED.store(true, Ordering::SeqCst);

                    send_pointer_event_to_flutter(
                        engine,
                        engine_dll,
                        FlutterPointerPhase_kAdd,
                        x,
                        y,
                        0.0,
                        0.0,
                        0,
                    );
                }

                send_pointer_event_to_flutter(
                    engine,
                    engine_dll,
                    FlutterPointerPhase_kDown,
                    x,
                    y,
                    0.0,
                    0.0,
                    new_button_state as i64,
                );
                true
            }
            WM_LBUTTONUP | WM_RBUTTONUP | WM_MBUTTONUP => {
                let x = (lparam.0 & 0xFFFF) as i16 as f64;
                let y = ((lparam.0 >> 16) & 0xFFFF) as i16 as f64;
                let button_flag_to_clear: i32 = match msg {
                    WM_LBUTTONUP => WINAPI_MK_LBUTTON as i32,
                    WM_RBUTTONUP => WINAPI_MK_RBUTTON as i32,
                    WM_MBUTTONUP => WINAPI_MK_MBUTTON as i32,
                    _ => 0,
                };
                let mut current_buttons_state = MOUSE_BUTTONS_STATE.load(Ordering::Relaxed);
                let buttons_for_kup_event = current_buttons_state;
                current_buttons_state &= !button_flag_to_clear;
                MOUSE_BUTTONS_STATE.store(current_buttons_state, Ordering::Relaxed);

                send_pointer_event_to_flutter(
                    engine,
                    engine_dll,
                    FlutterPointerPhase_kUp,
                    x,
                    y,
                    0.0,
                    0.0,
                    buttons_for_kup_event as i64,
                );
                true
            }
            WM_NCMOUSELEAVE => {
                if IS_MOUSE_ADDED.load(Ordering::SeqCst) {
                    IS_MOUSE_ADDED.store(false, Ordering::SeqCst);

                    send_pointer_event_to_flutter(
                        engine,
                        engine_dll,
                        FlutterPointerPhase_kRemove,
                        0.0,
                        0.0,
                        0.0,
                        0.0,
                        0,
                    );
                }
                MOUSE_BUTTONS_STATE.store(0, Ordering::Relaxed);
                false
            }
            WM_MOUSEWHEEL => {
                let wheel_delta = (wparam.0 >> 16) as i16;
                let x_screen = (lparam.0 & 0xFFFF) as i16;
                let y_screen = ((lparam.0 >> 16) & 0xFFFF) as i16;
                let mut point = windows::Win32::Foundation::POINT {
                    x: x_screen as i32,
                    y: y_screen as i32,
                };

                if ScreenToClient(hwnd, &mut point) == false {
                    return true;
                }
                let x_client = point.x as f64;
                let y_client = point.y as f64;
                let scroll_delta_y_flutter = -(wheel_delta as f64 / WHEEL_DELTA as f64) * 20.0;

                send_pointer_event_to_flutter(
                    engine,
                    engine_dll,
                    FlutterPointerPhase_kHover,
                    x_client,
                    y_client,
                    0.0,
                    scroll_delta_y_flutter,
                    MOUSE_BUTTONS_STATE.load(Ordering::Relaxed) as i64,
                );
                true
            }
            _ => false,
        }
    }
}

pub(crate) fn handle_flutter_set_cursor(
    hwnd_from_wparam: HWND,
    lparam_from_message: LPARAM,
    main_app_hwnd: HWND,
    flutter_should_set_cursor: bool,
) -> Option<LRESULT> {
    unsafe {
        if !flutter_should_set_cursor {
            return None;
        }

        let hit_test_code = (lparam_from_message.0 & 0xFFFF) as i16;

        if hwnd_from_wparam == main_app_hwnd && hit_test_code == HTCLIENT as i16 {
            match DESIRED_FLUTTER_CURSOR.try_lock() {
                Ok(desired_kind_guard) => {
                    if let Some(kind) = desired_kind_guard.as_ref() {
                        let mut h_cursor_to_set_winapi: WinApiHCURSOR = ptr::null_mut();
                        let mut flutter_did_request_cursor_change = true;
                        let h_instance_null: windows::Win32::Foundation::HINSTANCE =
                            windows::Win32::Foundation::HINSTANCE(0);

                        match kind.as_str() {
                            "basic" => {
                                h_cursor_to_set_winapi = WindowsAndMessaging::LoadCursorW(
                                    h_instance_null,
                                    WindowsAndMessaging::IDC_ARROW,
                                )
                                .ok()?
                                .0
                                    as WinApiHCURSOR
                            }
                            "click" | "pointer" => {
                                h_cursor_to_set_winapi = WindowsAndMessaging::LoadCursorW(
                                    h_instance_null,
                                    WindowsAndMessaging::IDC_HAND,
                                )
                                .ok()?
                                .0
                                    as WinApiHCURSOR
                            }
                            "text" => {
                                h_cursor_to_set_winapi = WindowsAndMessaging::LoadCursorW(
                                    h_instance_null,
                                    WindowsAndMessaging::IDC_IBEAM,
                                )
                                .ok()?
                                .0
                                    as WinApiHCURSOR
                            }
                            "forbidden" => {
                                h_cursor_to_set_winapi = WindowsAndMessaging::LoadCursorW(
                                    h_instance_null,
                                    WindowsAndMessaging::IDC_NO,
                                )
                                .ok()?
                                .0
                                    as WinApiHCURSOR
                            }
                            _ => {
                                flutter_did_request_cursor_change = false;
                            }
                        }

                        if flutter_did_request_cursor_change && !h_cursor_to_set_winapi.is_null() {
                            WindowsAndMessaging::SetCursor(
                                windows::Win32::UI::WindowsAndMessaging::HCURSOR(
                                    h_cursor_to_set_winapi as isize,
                                ),
                            );
                            return Some(LRESULT(1));
                        } else if flutter_did_request_cursor_change
                            && h_cursor_to_set_winapi.is_null()
                        {
                        }
                    } else {
                    }
                }
                Err(_e) => {}
            }
        }
        None
    }
}

fn send_pointer_event_to_flutter(
    engine: FlutterEngine,
    engine_dll: &FlutterEngineDll,
    phase: FlutterPointerPhase,
    x: f64,
    y: f64,
    scroll_delta_x: f64,
    scroll_delta_y: f64,
    buttons: i64,
) {
    unsafe {
        if engine.is_null() {
            return;
        }

        let event = FlutterPointerEvent {
            struct_size: std::mem::size_of::<FlutterPointerEvent>(),
            phase,
            timestamp: (engine_dll.FlutterEngineGetCurrentTime)() as usize,
            x,
            y,
            device: 0,
            signal_kind: if scroll_delta_x != 0.0 || scroll_delta_y != 0.0 {
                FlutterPointerSignalKind_kFlutterPointerSignalKindScroll
            } else {
                FlutterPointerSignalKind_kFlutterPointerSignalKindNone
            },
            scroll_delta_x,
            scroll_delta_y,
            device_kind: FlutterPointerDeviceKind_kFlutterPointerDeviceKindMouse,
            buttons,
            pan_x: 0.0,
            pan_y: 0.0,
            scale: 1.0,
            rotation: 0.0,
            view_id: 0,
        };

        let _res: FlutterEngineResult =
            (engine_dll.FlutterEngineSendPointerEvent)(engine, &event as *const _, 1);
    }
}

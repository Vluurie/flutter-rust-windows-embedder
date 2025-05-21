use std::ffi::CString;
use std::mem::MaybeUninit;
use std::ptr;

use serde_json::json;

use winapi::shared::minwindef::{HKL, UINT};
use winapi::um::winuser::{
    GetAsyncKeyState, GetKeyboardLayout, GetKeyboardState, MAPVK_VK_TO_VSC_EX, MAPVK_VSC_TO_VK_EX,
    MapVirtualKeyW, ToUnicodeEx, VK_BACK, VK_CONTROL, VK_LCONTROL, VK_LMENU, VK_LSHIFT, VK_LWIN,
    VK_MENU, VK_RCONTROL, VK_RETURN, VK_RMENU, VK_RSHIFT, VK_RWIN, VK_SHIFT,
};

use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::{
    WM_CHAR, WM_KEYDOWN, WM_KEYUP, WM_SYSKEYDOWN, WM_SYSKEYUP,
};

use crate::embedder::{
    FlutterEngine, FlutterEngineResult, FlutterKeyEvent,
    FlutterKeyEventDeviceType_kFlutterKeyEventDeviceTypeKeyboard, FlutterKeyEventType,
    FlutterKeyEventType_kFlutterKeyEventTypeDown, FlutterKeyEventType_kFlutterKeyEventTypeRepeat,
    FlutterKeyEventType_kFlutterKeyEventTypeUp, FlutterPlatformMessage,
};
use crate::software_renderer::dynamic_flutter_engine_dll_loader::FlutterEngineDll;
use crate::software_renderer::overlay::textinput::{
    ACTIVE_TEXT_INPUT_STATE, TextInputModel, send_perform_action_to_flutter,
    send_update_editing_state_to_flutter,
};

include!(concat!(env!("OUT_DIR"), "/generated_keyboard_map.rs"));

const LOGICAL_KEY_UNKNOWN_CONST: u64 = 0x0;
const PHYSICAL_KEY_UNKNOWN_CONST: u64 = 0x0;

 fn resolve_modifier_virtual_key(
    virtual_key: u16,
    is_extended_key: bool,
    scan_code: u32,
) -> u16 {
    match virtual_key as i32 {
        VK_SHIFT => {
            let mapped_vk = unsafe { MapVirtualKeyW(scan_code as UINT, MAPVK_VSC_TO_VK_EX) };
            if mapped_vk == VK_LSHIFT as UINT {
                VK_LSHIFT as u16
            } else if mapped_vk == VK_RSHIFT as UINT {
                VK_RSHIFT as u16
            } else {
                virtual_key
            }
        }
        VK_CONTROL => {
            if is_extended_key {
                VK_RCONTROL as u16
            } else {
                VK_LCONTROL as u16
            }
        }
        VK_MENU => {
            if is_extended_key {
                VK_RMENU as u16
            } else {
                VK_LMENU as u16
            }
        }
        _ => virtual_key,
    }
}

fn send_legacy_key_event_platform_message(
    engine: FlutterEngine,
    engine_dll: &FlutterEngineDll,
    msg_type: &str,
    original_virtual_key: u16,
    scan_code_raw: u32,
    lparam: LPARAM,
    hkl: HKL,
) {
    unsafe {
        if engine.is_null() {
            return;
        }
        let mut flutter_modifiers = 0;
        if (GetAsyncKeyState(VK_SHIFT) & 0x8000u16 as i16) != 0 {
            flutter_modifiers |= 0x1;
        }
        if (GetAsyncKeyState(VK_CONTROL) & 0x8000u16 as i16) != 0 {
            flutter_modifiers |= 0x2;
        }
        if (GetAsyncKeyState(VK_MENU) & 0x8000u16 as i16) != 0 {
            flutter_modifiers |= 0x4;
        }
        if ((GetAsyncKeyState(VK_LWIN) & 0x8000u16 as i16) != 0)
            || ((GetAsyncKeyState(VK_RWIN) & 0x8000u16 as i16) != 0)
        {
            flutter_modifiers |= 0x8;
        }
        let character_code_point: Option<u32> = if msg_type == "keydown" {
            let temp_char_bytes =
                get_characters_for_key_event(original_virtual_key, scan_code_raw, lparam, hkl);
            String::from_utf8_lossy(
                &temp_char_bytes
                    .iter()
                    .take_while(|&&b| b != 0)
                    .cloned()
                    .collect::<Vec<u8>>(),
            )
            .chars()
            .next()
            .map(|c| c as u32)
        } else {
            None
        };
        let platform_message_payload = json!({
            "type": msg_type, "keymap": "windows", "scanCode": scan_code_raw,
            "keyCode": original_virtual_key, "modifiers": flutter_modifiers,
            "unicodeScalarValues": character_code_point,
        });
        let payload_str = platform_message_payload.to_string();
        let payload_bytes = payload_str.as_bytes();
        let channel_name_cstring = CString::new("flutter/keyevent")
            .expect("CString::new für flutter/keyevent fehlgeschlagen");
        let platform_message = FlutterPlatformMessage {
            struct_size: std::mem::size_of::<FlutterPlatformMessage>(),
            channel: channel_name_cstring.as_ptr(),
            message: payload_bytes.as_ptr(),
            message_size: payload_bytes.len(),
            response_handle: ptr::null(),
        };
        let _send_result = (engine_dll.FlutterEngineSendPlatformMessage)(engine, &platform_message);
    }
}

pub(crate) fn process_flutter_key_event_internal(
    engine: FlutterEngine,       
    engine_dll: &FlutterEngineDll,
    _hwnd: HWND,
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
            return false;
        }

        let hkl: HKL = GetKeyboardLayout(0);

        match msg {
            WM_KEYDOWN | WM_SYSKEYDOWN => {
                let original_virtual_key = wparam.0 as u16;

                let mut send_action_args: Option<(i32, String)> = None;
                let mut send_update_args: Option<(i32, TextInputModel)> = None;
                let mut event_handled_by_text_input = false;

                {
                    let mut active_state_guard = ACTIVE_TEXT_INPUT_STATE
                        .lock()
                        .expect("Mutex panic: ACTIVE_TEXT_INPUT_STATE");
                    if let Some(active_state) = active_state_guard.as_mut() {
                        match original_virtual_key as i32 {
                            VK_RETURN => {
                                let client_id = active_state.client_id;
                                let input_action = active_state.input_action.clone();
                                if active_state.input_action.contains("newline") {
                                    active_state.model.insert_char('\n');
                                    send_update_args =
                                        Some((client_id, active_state.model.clone()));
                                }
                                send_action_args = Some((client_id, input_action));
                                event_handled_by_text_input = true;
                            }
                            VK_BACK => {
                                active_state.model.backspace();
                                send_update_args =
                                    Some((active_state.client_id, active_state.model.clone()));
                                event_handled_by_text_input = true;
                            }
                            _ => {}
                        }
                    }
                }

                if let Some((client_id, cloned_model)) = send_update_args {
                    send_update_editing_state_to_flutter(engine,engine_dll, client_id, &cloned_model);
                }
                if let Some((client_id, action_string)) = send_action_args {
                    send_perform_action_to_flutter(engine,engine_dll, client_id, &action_string);
                }

                if event_handled_by_text_input {
                    return true;
                }

                let scan_code_raw = ((lparam.0 >> 16) & 0xFF) as u32;
                let is_extended_key = ((lparam.0 >> 24) & 0x01) != 0;
                let is_repeat = ((lparam.0 >> 30) & 0x01) != 0;
                let flutter_event_type = if is_repeat {
                    FlutterKeyEventType_kFlutterKeyEventTypeRepeat
                } else {
                    FlutterKeyEventType_kFlutterKeyEventTypeDown
                };

                let (physical_key, logical_key) = windows_to_flutter_key_codes(
                    original_virtual_key,
                    scan_code_raw,
                    is_extended_key,
                    hkl,
                );

                let characters_bytes_for_flutter_key_event = if flutter_event_type
                    == FlutterKeyEventType_kFlutterKeyEventTypeDown
                    || flutter_event_type == FlutterKeyEventType_kFlutterKeyEventTypeRepeat
                {
                    get_characters_for_key_event(original_virtual_key, scan_code_raw, lparam, hkl)
                } else {
                    [0u8; 8]
                };

                send_key_event_to_flutter(
                    engine,
                    engine_dll,
                    flutter_event_type,
                    physical_key,
                    logical_key,
                    &characters_bytes_for_flutter_key_event,
                    false,
                );

                send_legacy_key_event_platform_message(
                    engine,
                    engine_dll,
                    "keydown",
                    original_virtual_key,
                    scan_code_raw,
                    lparam,
                    hkl,
                );
                return true;
            }
            WM_KEYUP | WM_SYSKEYUP => {
                let original_virtual_key = wparam.0 as u16;
                let scan_code_raw = ((lparam.0 >> 16) & 0xFF) as u32;
                let is_extended_key = ((lparam.0 >> 24) & 0x01) != 0;
                let (physical_key, logical_key) = windows_to_flutter_key_codes(
                    original_virtual_key,
                    scan_code_raw,
                    is_extended_key,
                    hkl,
                );
                send_key_event_to_flutter(
                    engine,
                    engine_dll,
                    FlutterKeyEventType_kFlutterKeyEventTypeUp,
                    physical_key,
                    logical_key,
                    &[0u8; 8],
                    false,
                );
                send_legacy_key_event_platform_message(
                    engine,
                    engine_dll,
                    "keyup",
                    original_virtual_key,
                    scan_code_raw,
                    lparam,
                    hkl,
                );
                return true;
            }
            WM_CHAR => {
                let char_code = wparam.0 as u32;
                let mut event_handled_by_text_input = false;
                let mut send_update_args_char: Option<(i32, TextInputModel)> = None;

                if let Some(char_val) = std::char::from_u32(char_code) {
                    {
                        let mut active_state_guard = ACTIVE_TEXT_INPUT_STATE
                            .lock()
                            .expect("Mutex panic: ACTIVE_TEXT_INPUT_STATE");
                        if let Some(active_state) = active_state_guard.as_mut() {
                            if char_val != '\x08' && char_val != '\r' && !char_val.is_control() {
                                active_state.model.insert_char(char_val);
                                send_update_args_char =
                                    Some((active_state.client_id, active_state.model.clone()));
                                event_handled_by_text_input = true;
                            }
                        }
                    }
                    if let Some((client_id, cloned_model)) = send_update_args_char {
                        send_update_editing_state_to_flutter(engine,engine_dll, client_id, &cloned_model);
                    }
                    if event_handled_by_text_input {
                        return true;
                    }
                }
                return false;
            }
            _ => {
                return false;
            }
        }
    }
}

fn send_key_event_to_flutter(
    engine: FlutterEngine,
    engine_dll: &FlutterEngineDll,
    type_: FlutterKeyEventType,
    physical: u64,
    logical: u64,
    characters_bytes: &[u8; 8],
    synthesized: bool,
) {
    unsafe {
        if engine.is_null() {
            return;
        }
        let mut len = 0;
        while len < characters_bytes.len() && characters_bytes[len] != 0 {
            len += 1;
        }
        let char_slice = &characters_bytes[0..len];
        let characters_cstring =
            CString::new(char_slice).unwrap_or_else(|_| CString::new("").unwrap());
        let characters_ptr = characters_cstring.as_ptr();
        let event_data = FlutterKeyEvent {
            struct_size: std::mem::size_of::<FlutterKeyEvent>(),
            timestamp: (engine_dll.FlutterEngineGetCurrentTime)() as f64,
            type_,
            physical,
            logical,
            character: characters_ptr,
            synthesized,
            device_type: FlutterKeyEventDeviceType_kFlutterKeyEventDeviceTypeKeyboard,
        };

        let _result: FlutterEngineResult = (engine_dll.FlutterEngineSendKeyEvent)(
            engine,
            &event_data as *const _,
            None,
            ptr::null_mut(),
        );
    }
}

fn windows_to_flutter_key_codes(
    original_virtual_key: u16,
    raw_scan_code: u32,
    is_extended_key: bool,
    _hkl: HKL,
) -> (u64, u64) {
    unsafe {
        let resolved_virtual_key =
            resolve_modifier_virtual_key(original_virtual_key, is_extended_key, raw_scan_code);
        let mut physical_key_id: u64 = PHYSICAL_KEY_UNKNOWN_CONST;
        let mut logical_key_id: u64 = LOGICAL_KEY_UNKNOWN_CONST;
        let platform_scan_code_from_vk =
            MapVirtualKeyW(original_virtual_key as UINT, MAPVK_VK_TO_VSC_EX);
        let key_map_data_owned: Vec<KeyMapEntry> = get_key_map();
        for entry in key_map_data_owned.iter() {
            if platform_scan_code_from_vk != 0
                && entry.platform == platform_scan_code_from_vk as i64
            {
                physical_key_id = entry.physical as u64;
                if let Some(log_val) = entry.logical.or(entry.fallback) {
                    if log_val != 0 {
                        logical_key_id = log_val as u64;
                    }
                }
                break;
            }
        }
        if logical_key_id == LOGICAL_KEY_UNKNOWN_CONST {
            if (resolved_virtual_key >= 0x30 && resolved_virtual_key <= 0x39)
                || (resolved_virtual_key >= 0x41 && resolved_virtual_key <= 0x5A)
            {
                logical_key_id = resolved_virtual_key as u64;
            }
        }
        (physical_key_id, logical_key_id)
    }
}

fn get_characters_for_key_event(
    original_virtual_key: u16,
    scan_code: u32,
    _lparam: LPARAM,
    hkl: HKL,
) -> [u8; 8] {
    unsafe {
        let mut buffer = [0u8; 8];
        let mut keyboard_state = [0u8; 256];
        if GetKeyboardState(keyboard_state.as_mut_ptr()) == 0 {
            if !buffer.is_empty() {
                buffer[0] = 0;
            }
            return buffer;
        }
        let mut wide_char_buffer: [MaybeUninit<u16>; 2] =
            [MaybeUninit::uninit(), MaybeUninit::uninit()];
        let num_chars = ToUnicodeEx(
            original_virtual_key as u32,
            scan_code,
            keyboard_state.as_ptr(),
            wide_char_buffer.as_mut_ptr() as *mut u16,
            wide_char_buffer.len() as i32,
            0,
            hkl,
        );
        if num_chars > 0 {
            let utf16_values: Vec<u16> = (0..num_chars as usize)
                .map(|i| wide_char_buffer[i].assume_init())
                .collect();
            let char_string = String::from_utf16_lossy(&utf16_values);
            let bytes = char_string.as_bytes();
            let len_to_copy = std::cmp::min(bytes.len(), buffer.len() - 1);
            buffer[..len_to_copy].copy_from_slice(&bytes[..len_to_copy]);
            if len_to_copy < buffer.len() {
                buffer[len_to_copy] = 0;
            } else if !buffer.is_empty() {
                buffer[buffer.len() - 1] = 0;
            }
        } else {
            if !buffer.is_empty() {
                buffer[0] = 0;
            }
        }
        buffer
    }
}

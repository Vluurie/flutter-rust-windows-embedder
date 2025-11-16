use std::mem::MaybeUninit;

use serde_json::json;

use winapi::shared::minwindef::{HKL, UINT};
use winapi::um::winuser::{
    GetAsyncKeyState, GetKeyboardLayout, GetKeyboardState, MAPVK_VK_TO_VSC_EX, MAPVK_VSC_TO_VK_EX,
    MapVirtualKeyW, ToUnicodeEx, VK_BACK, VK_CONTROL, VK_LCONTROL, VK_LMENU, VK_LSHIFT, VK_LWIN,
    VK_MENU, VK_RCONTROL, VK_RETURN, VK_RMENU, VK_RSHIFT, VK_RWIN, VK_SHIFT,
};

use windows::Win32::Foundation::{LPARAM, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::{
    WM_CHAR, WM_KEYDOWN, WM_KEYUP, WM_SYSKEYDOWN, WM_SYSKEYUP,
};

use crate::bindings::embedder::{
    FlutterKeyEventType, FlutterKeyEventType_kFlutterKeyEventTypeDown,
    FlutterKeyEventType_kFlutterKeyEventTypeRepeat, FlutterKeyEventType_kFlutterKeyEventTypeUp,
};
use crate::bindings::keyboard_layout::{KeyMapEntry, get_key_map};

use crate::software_renderer::overlay::overlay_impl::{
    FlutterOverlay, PendingKeyEvent, PendingKeyEventQueue, PendingPlatformMessageQueue,
};
use crate::software_renderer::overlay::textinput::{
    TextInputModel, send_perform_action_to_flutter, send_update_editing_state_to_flutter,
};

const LOGICAL_KEY_UNKNOWN_CONST: u64 = 0x0;
const PHYSICAL_KEY_UNKNOWN_CONST: u64 = 0x0;

fn resolve_modifier_virtual_key(virtual_key: u16, is_extended_key: bool, scan_code: u32) -> u16 {
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
    message_queue: &PendingPlatformMessageQueue,
    msg_type: &str,
    original_virtual_key: u16,
    scan_code_raw: u32,
    lparam: LPARAM,
    hkl: HKL,
) {
    unsafe {
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
        let payload_bytes = payload_str.into_bytes(); // Vec<u8>

        let pending_message =
            crate::software_renderer::overlay::overlay_impl::PendingPlatformMessage {
                channel: "flutter/keyevent".to_string(),
                payload_bytes,
            };

        if let Ok(mut queue) = message_queue.lock() {
            queue.push_back(pending_message);
        }
    }
}

pub fn handle_keyboard_event(
    overlay: &FlutterOverlay,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> bool {
    if overlay.engine.0.is_null() {
        return false;
    }

    unsafe {
        let hkl: HKL = GetKeyboardLayout(0);

        match msg {
            WM_KEYDOWN | WM_SYSKEYDOWN => {
                let original_virtual_key = wparam.0 as u16;

                let mut send_action_args: Option<(i32, String)> = None;
                let mut send_update_args: Option<(i32, TextInputModel)> = None;
                let mut event_handled_by_text_input = false;

                {
                    let mut active_state_guard = overlay.text_input_state.lock().unwrap();

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
                    send_update_editing_state_to_flutter(
                        &overlay.pending_platform_messages,
                        client_id,
                        &cloned_model,
                    );
                }
                if let Some((client_id, action_string)) = send_action_args {
                    send_perform_action_to_flutter(
                        &overlay.pending_platform_messages,
                        client_id,
                        &action_string,
                    );
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

                let characters_bytes_for_flutter_key_event =
                    get_characters_for_key_event(original_virtual_key, scan_code_raw, lparam, hkl);

                send_key_event_to_flutter(
                    &overlay.pending_key_events,
                    flutter_event_type,
                    physical_key,
                    logical_key,
                    &characters_bytes_for_flutter_key_event,
                    false,
                );

                send_legacy_key_event_platform_message(
                    &overlay.pending_platform_messages,
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
                    &overlay.pending_key_events,
                    FlutterKeyEventType_kFlutterKeyEventTypeUp,
                    physical_key,
                    logical_key,
                    &[0u8; 8],
                    false,
                );

                send_legacy_key_event_platform_message(
                    &overlay.pending_platform_messages,
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
                        let mut active_state_guard = overlay.text_input_state.lock().unwrap();

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
                        send_update_editing_state_to_flutter(
                            &overlay.pending_platform_messages,
                            client_id,
                            &cloned_model,
                        );
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
    message_queue: &PendingKeyEventQueue,
    type_: FlutterKeyEventType,
    physical: u64,
    logical: u64,
    characters_bytes: &[u8; 8],
    synthesized: bool,
) {
    let len = characters_bytes
        .iter()
        .position(|&b| b == 0)
        .unwrap_or(characters_bytes.len());
    let characters = String::from_utf8_lossy(&characters_bytes[..len]).to_string();

    let event_data = PendingKeyEvent {
        event_type: type_,
        physical,
        logical,
        characters,
        synthesized,
    };

    if let Ok(mut queue) = message_queue.lock() {
        queue.push_back(event_data);
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

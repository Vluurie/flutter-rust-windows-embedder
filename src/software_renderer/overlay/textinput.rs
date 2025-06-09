use std::ffi::{c_void, CStr, CString};
use std::ptr;

use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::bindings::embedder::{FlutterEngine, FlutterPlatformMessage};
use crate::software_renderer::dynamic_flutter_engine_dll_loader::FlutterEngineDll;
use crate::software_renderer::overlay::overlay_impl::FlutterOverlay;


#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct FlutterTextEditingState {
    text: String,
    #[serde(rename = "selectionBase")]
    selection_base: i32,
    #[serde(rename = "selectionExtent")]
    selection_extent: i32,
    #[serde(rename = "composingBase")]
    composing_base: i32,
    #[serde(rename = "composingExtent")]
    composing_extent: i32,
}

#[derive(Debug, Clone)]
pub struct TextInputModel {
    pub text: String,
    pub selection_base_utf8: usize,
    pub selection_extent_utf8: usize,
}

impl TextInputModel {
    pub(crate) fn new() -> Self {
        TextInputModel {
            text: String::new(),
            selection_base_utf8: 0,
            selection_extent_utf8: 0,
        }
    }

    pub(crate) fn to_flutter_editing_state(&self) -> FlutterTextEditingState {
        let selection_base_utf16 = utf8_byte_offset_to_utf16_code_unit_offset(
            &self.text, self.selection_base_utf8
        );
        let selection_extent_utf16 = utf8_byte_offset_to_utf16_code_unit_offset(
            &self.text, self.selection_extent_utf8
        );

        FlutterTextEditingState {
            text: self.text.clone(),
            selection_base: selection_base_utf16,
            selection_extent: selection_extent_utf16,
            composing_base: -1,
            composing_extent: -1,
        }
    }
    
    pub(crate) fn sanitize_offsets(&mut self) {
        let byte_len = self.text.len();
        self.selection_base_utf8 = self.selection_base_utf8.min(byte_len);
        self.selection_extent_utf8 = self.selection_extent_utf8.min(byte_len);
    }

    fn get_ordered_selection_utf8(&self) -> (usize, usize) {
        if self.selection_base_utf8 <= self.selection_extent_utf8 {
            (self.selection_base_utf8, self.selection_extent_utf8)
        } else {
            (self.selection_extent_utf8, self.selection_base_utf8)
        }
    }

    pub(crate) fn insert_char(&mut self, ch: char) {
        let (sel_start, sel_end) = self.get_ordered_selection_utf8();
        self.text.replace_range(sel_start..sel_end, &ch.to_string());
        let new_cursor_pos = sel_start + ch.len_utf8();
        self.selection_base_utf8 = new_cursor_pos;
        self.selection_extent_utf8 = new_cursor_pos;
        self.sanitize_offsets();
    }

    pub(crate) fn backspace(&mut self) {
        let (sel_start, sel_end) = self.get_ordered_selection_utf8();
        if sel_start == sel_end {
            if sel_start > 0 {
                let mut prev_char_boundary = 0;
                for (idx, _) in self.text.char_indices() {
                    if idx < sel_start { prev_char_boundary = idx; } else { break; }
                }
                self.text.replace_range(prev_char_boundary..sel_start, "");
                self.selection_base_utf8 = prev_char_boundary;
            }
        } else {
            self.text.replace_range(sel_start..sel_end, "");
            self.selection_base_utf8 = sel_start;
        }
        self.selection_extent_utf8 = self.selection_base_utf8;
        self.sanitize_offsets();
    }
}

#[derive(Debug, Clone)]
pub struct ActiveTextInputState {
    pub client_id: i32,
    pub input_action: String,
    pub model: TextInputModel,
}

fn utf8_byte_offset_to_utf16_code_unit_offset(s: &str, byte_offset: usize) -> i32 {
    let mut utf16_offset = 0;
    let mut current_byte_idx = 0;
    for ch in s.chars() {
        if current_byte_idx >= byte_offset { break; }
        utf16_offset += ch.encode_utf16(&mut [0u16; 2]).len() as i32;
        current_byte_idx += ch.len_utf8();
    }
    utf16_offset
}

 fn send_to_flutter_text_input_method_call(
    engine: FlutterEngine,
    engine_dll: &FlutterEngineDll,
    method_name: &str,
    args: serde_json::Value,
) {
    if engine.is_null() {
        return;
    }
    let call_payload = json!({ "method": method_name, "args": args });
    let payload_str = call_payload.to_string();
    let payload_bytes = payload_str.as_bytes();
    let channel_name_cstring = match CString::new("flutter/textinput") {
        Ok(s) => s,
        Err(_e) => { return; } 
    };
    let platform_message = FlutterPlatformMessage {
        struct_size: std::mem::size_of::<FlutterPlatformMessage>(),
        channel: channel_name_cstring.as_ptr(),
        message: payload_bytes.as_ptr(),
        message_size: payload_bytes.len(),
        response_handle: ptr::null(),
    };
    
    let _ = unsafe { (engine_dll.FlutterEngineSendPlatformMessage)(engine, &platform_message) };
}

pub(crate) fn send_update_editing_state_to_flutter(engine: FlutterEngine,  engine_dll: &FlutterEngineDll, client_id: i32, model: &TextInputModel) {
    let flutter_state = model.to_flutter_editing_state();
    let args = json!([client_id, flutter_state]);
    send_to_flutter_text_input_method_call(engine, engine_dll, "TextInputClient.updateEditingState", args);
}

pub(crate) fn send_perform_action_to_flutter(engine: FlutterEngine,  engine_dll: &FlutterEngineDll, client_id: i32, action: &str) {
    let args = json!([client_id, action]);
    send_to_flutter_text_input_method_call(engine, engine_dll, "TextInputClient.performAction", args);
}

#[unsafe(no_mangle)]
pub(crate) unsafe extern "C" fn custom_text_input_platform_message_handler(
    platform_message: *const FlutterPlatformMessage,
    user_data: *mut c_void,
) {
    unsafe {
    if platform_message.is_null() || user_data.is_null() {
        return;
    }

    let message = &*platform_message;


    let overlay: &mut FlutterOverlay = &mut *(user_data as *mut FlutterOverlay);
    let engine_handle = overlay.engine;
    let engine_dll_arc = overlay.engine_dll.clone();


    let channel_name_c_str = CStr::from_ptr(message.channel);
    if channel_name_c_str.to_string_lossy() != "flutter/textinput" {
        return;
    }

    let slice = std::slice::from_raw_parts(message.message, message.message_size);

    if let Ok(parsed_json) = serde_json::from_slice::<serde_json::Value>(slice) {
        if let Some(method_call) = parsed_json.as_object() {
            if let Some(method_name) = method_call.get("method").and_then(|m| m.as_str()) {
                let args = method_call.get("args");

                // --- NEU: Auf den Zustand der Instanz zugreifen ---
                let mut active_state_guard = overlay
                    .text_input_state
                    .lock()
                    .expect("Mutex panic: text_input_state");

                match method_name {
                    "TextInput.setClient" => {
                        if let Some(arr) = args.and_then(|a| a.as_array()) {
                            if let (Some(id_val), Some(cfg_map)) = (
                                arr.get(0).and_then(|v| v.as_i64()),
                                arr.get(1).and_then(|v| v.as_object()),
                            ) {
                                let client_id = id_val as i32;
                                let action = cfg_map
                                    .get("inputAction")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("TextInputAction.done")
                                    .to_string();

                                *active_state_guard = Some(ActiveTextInputState {
                                    client_id,
                                    input_action: action.clone(),
                                    model: TextInputModel::new(),
                                });
                            }
                        }
                    }
                    "TextInput.clearClient" => {
                        *active_state_guard = None;
                    }
                    "TextInput.setEditingState" => {
                        if let Some(current_state) = active_state_guard.as_mut() {
                            if let Some(state_map_val) = args.and_then(|a| a.as_object()) {
                                if let Ok(flutter_state) = serde_json::from_value::<
                                    FlutterTextEditingState,
                                >(serde_json::Value::Object(
                                    state_map_val.clone(),
                                )) {

                                    current_state.model.text = flutter_state.text;
                                    current_state.model.selection_base_utf8 =
                                        flutter_state.selection_base.max(0) as usize;
                                    current_state.model.selection_extent_utf8 =
                                        flutter_state.selection_extent.max(0) as usize;
                                    current_state.model.sanitize_offsets();
                                }
                            }
                        }
                    }
                    "TextInput.show" | "TextInput.hide" | "TextInput.setEditableSizeAndTransform" => {
                        // No-op in this version
                    }
                    _ => { /* TODO: Unhandled methods */ }
                }
            }
        }
    }

    if !message.response_handle.is_null() {
        let _ = (engine_dll_arc.FlutterEngineSendPlatformMessageResponse)(
            engine_handle,
            message.response_handle,
            ptr::null(),
            0,
        );
    }
}
}
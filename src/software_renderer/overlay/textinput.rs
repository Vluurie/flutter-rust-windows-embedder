use std::collections::{HashMap, HashSet};
use std::ffi::{CStr, c_void};
use std::ptr;
use std::sync::{Arc, Mutex, OnceLock};

use serde::{Deserialize, Serialize};
use serde_json::{Value, from_slice, from_value, json};

use crate::bindings::embedder::FlutterPlatformMessage;
use crate::software_renderer::overlay::overlay_impl::{
    FlutterOverlay, PendingPlatformMessage, PendingPlatformMessageQueue,
};

/// Per-view raw-key bookkeeping (down/up dedup). Text editing lives in the
/// single global [`ACTIVE`] model, like Flutter's one `TextInputPlugin`.
pub struct ViewKeyboardState {
    pub(crate) pressed_keys: Mutex<HashSet<u64>>,
}

impl ViewKeyboardState {
    pub(crate) fn new() -> Self {
        Self {
            pressed_keys: Mutex::new(HashSet::new()),
        }
    }
}

pub(crate) type SharedViewKeyboardState = Arc<ViewKeyboardState>;

static VIEW_KEYBOARD_STATES: OnceLock<Mutex<HashMap<i64, SharedViewKeyboardState>>> =
    OnceLock::new();

pub(crate) fn register_view_keyboard_state(view_id: i64, state: SharedViewKeyboardState) {
    let map = VIEW_KEYBOARD_STATES.get_or_init(Default::default);
    if let Ok(mut g) = map.lock() {
        g.insert(view_id, state);
    }
}

pub(crate) fn unregister_view_keyboard_state(view_id: i64) {
    let map = VIEW_KEYBOARD_STATES.get_or_init(Default::default);
    if let Ok(mut g) = map.lock() {
        g.remove(&view_id);
    }
}

pub(crate) fn lookup_view_keyboard_state(view_id: i64) -> Option<SharedViewKeyboardState> {
    let map = VIEW_KEYBOARD_STATES.get_or_init(Default::default);
    map.lock().ok()?.get(&view_id).cloned()
}


#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct FlutterTextEditingState {
    pub(crate) text: String,
    #[serde(rename = "selectionBase")]
    pub(crate) selection_base: i32,
    #[serde(rename = "selectionExtent")]
    pub(crate) selection_extent: i32,
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
        let selection_base_utf16 =
            utf8_byte_offset_to_utf16_code_unit_offset(&self.text, self.selection_base_utf8);
        let selection_extent_utf16 =
            utf8_byte_offset_to_utf16_code_unit_offset(&self.text, self.selection_extent_utf8);

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
        self.selection_base_utf8 =
            snap_to_char_boundary(&self.text, self.selection_base_utf8.min(byte_len));
        self.selection_extent_utf8 =
            snap_to_char_boundary(&self.text, self.selection_extent_utf8.min(byte_len));
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
                let prev = self.prev_char_boundary(sel_start);
                self.text.replace_range(prev..sel_start, "");
                self.selection_base_utf8 = prev;
            }
        } else {
            self.text.replace_range(sel_start..sel_end, "");
            self.selection_base_utf8 = sel_start;
        }
        self.selection_extent_utf8 = self.selection_base_utf8;
        self.sanitize_offsets();
    }

    pub(crate) fn delete_forward(&mut self) {
        let (sel_start, sel_end) = self.get_ordered_selection_utf8();
        if sel_start == sel_end {
            if sel_start < self.text.len() {
                let next = self.next_char_boundary(sel_start);
                self.text.replace_range(sel_start..next, "");
            }
        } else {
            self.text.replace_range(sel_start..sel_end, "");
        }
        self.selection_base_utf8 = sel_start;
        self.selection_extent_utf8 = sel_start;
        self.sanitize_offsets();
    }

    fn prev_char_boundary(&self, from: usize) -> usize {
        let mut prev = 0;
        for (idx, _) in self.text.char_indices() {
            if idx < from {
                prev = idx;
            } else {
                break;
            }
        }
        prev
    }

    fn next_char_boundary(&self, from: usize) -> usize {
        for (idx, _) in self.text.char_indices() {
            if idx > from {
                return idx;
            }
        }
        self.text.len()
    }

    fn collapse_or(&self, pick_start: bool) -> Option<usize> {
        if self.selection_base_utf8 != self.selection_extent_utf8 {
            let (start, end) = self.get_ordered_selection_utf8();
            Some(if pick_start { start } else { end })
        } else {
            None
        }
    }

    pub(crate) fn move_left(&mut self, extend: bool) {
        if !extend
            && let Some(pos) = self.collapse_or(true) {
                self.selection_base_utf8 = pos;
                self.selection_extent_utf8 = pos;
                return;
            }
        let pos = self.prev_char_boundary(self.selection_extent_utf8);
        self.selection_extent_utf8 = pos;
        if !extend {
            self.selection_base_utf8 = pos;
        }
    }

    pub(crate) fn move_right(&mut self, extend: bool) {
        if !extend
            && let Some(pos) = self.collapse_or(false) {
                self.selection_base_utf8 = pos;
                self.selection_extent_utf8 = pos;
                return;
            }
        let pos = self.next_char_boundary(self.selection_extent_utf8);
        self.selection_extent_utf8 = pos;
        if !extend {
            self.selection_base_utf8 = pos;
        }
    }

    fn line_start(&self, from: usize) -> usize {
        self.text[..from].rfind('\n').map(|i| i + 1).unwrap_or(0)
    }

    fn line_end(&self, from: usize) -> usize {
        self.text[from..]
            .find('\n')
            .map(|i| from + i)
            .unwrap_or(self.text.len())
    }

    pub(crate) fn move_home(&mut self, extend: bool) {
        let pos = self.line_start(self.selection_extent_utf8);
        self.selection_extent_utf8 = pos;
        if !extend {
            self.selection_base_utf8 = pos;
        }
    }

    pub(crate) fn move_end(&mut self, extend: bool) {
        let pos = self.line_end(self.selection_extent_utf8);
        self.selection_extent_utf8 = pos;
        if !extend {
            self.selection_base_utf8 = pos;
        }
    }

    fn column_chars(&self, pos: usize) -> usize {
        let start = self.line_start(pos);
        self.text[start..pos].chars().count()
    }

    fn offset_at_column(&self, line_start: usize, line_end: usize, column: usize) -> usize {
        let mut offset = line_start;
        for (col, ch) in self.text[line_start..line_end].chars().enumerate() {
            if col >= column {
                break;
            }
            offset += ch.len_utf8();
        }
        offset
    }

    pub(crate) fn move_up(&mut self, extend: bool) {
        let cur = self.selection_extent_utf8;
        let cur_line_start = self.line_start(cur);
        if cur_line_start == 0 {
            self.selection_extent_utf8 = 0;
            if !extend {
                self.selection_base_utf8 = 0;
            }
            return;
        }
        let column = self.column_chars(cur);
        let prev_line_end = cur_line_start - 1;
        let prev_line_start = self.line_start(prev_line_end);
        let pos = self.offset_at_column(prev_line_start, prev_line_end, column);
        self.selection_extent_utf8 = pos;
        if !extend {
            self.selection_base_utf8 = pos;
        }
    }

    pub(crate) fn move_down(&mut self, extend: bool) {
        let cur = self.selection_extent_utf8;
        let cur_line_end = self.line_end(cur);
        if cur_line_end >= self.text.len() {
            let end = self.text.len();
            self.selection_extent_utf8 = end;
            if !extend {
                self.selection_base_utf8 = end;
            }
            return;
        }
        let column = self.column_chars(cur);
        let next_line_start = cur_line_end + 1;
        let next_line_end = self.line_end(next_line_start);
        let pos = self.offset_at_column(next_line_start, next_line_end, column);
        self.selection_extent_utf8 = pos;
        if !extend {
            self.selection_base_utf8 = pos;
        }
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
        if current_byte_idx >= byte_offset {
            break;
        }
        utf16_offset += ch.encode_utf16(&mut [0u16; 2]).len() as i32;
        current_byte_idx += ch.len_utf8();
    }
    utf16_offset
}

fn snap_to_char_boundary(s: &str, byte_offset: usize) -> usize {
    if byte_offset >= s.len() {
        return s.len();
    }
    let mut offset = byte_offset;
    while offset > 0 && !s.is_char_boundary(offset) {
        offset -= 1;
    }
    offset
}

fn utf16_code_unit_offset_to_utf8_byte_offset(s: &str, utf16_offset: usize) -> usize {
    let mut utf16_idx = 0;
    let mut byte_idx = 0;
    for ch in s.chars() {
        if utf16_idx >= utf16_offset {
            break;
        }
        utf16_idx += ch.encode_utf16(&mut [0u16; 2]).len();
        byte_idx += ch.len_utf8();
    }
    byte_idx
}

fn send_to_flutter_text_input_method_call(
    message_queue: &PendingPlatformMessageQueue,
    method_name: &str,
    args: Value,
) {
    let call_payload = json!({ "method": method_name, "args": args });
    let payload_str = call_payload.to_string();
    let payload_bytes = payload_str.into_bytes();

    let pending_message = PendingPlatformMessage {
        channel: "flutter/textinput".to_string(),
        payload_bytes,
    };

    if let Ok(mut queue) = message_queue.lock() {
        queue.push_back(pending_message);
    }
}

pub(crate) fn send_update_editing_state_to_flutter(
    message_queue: &PendingPlatformMessageQueue,
    client_id: i32,
    model: &TextInputModel,
) {
    let flutter_state = model.to_flutter_editing_state();
    let args = json!([client_id, flutter_state]);
    send_to_flutter_text_input_method_call(
        message_queue,
        "TextInputClient.updateEditingState",
        args,
    );
}

pub(crate) fn send_perform_action_to_flutter(
    message_queue: &PendingPlatformMessageQueue,
    client_id: i32,
    action: &str,
) {
    let args = json!([client_id, action]);
    send_to_flutter_text_input_method_call(message_queue, "TextInputClient.performAction", args);
}

pub(crate) fn apply_text_input_method(
    method_name: &str,
    args: Option<&Value>,
    slot: &mut Option<ActiveTextInputState>,
) {
    match method_name {
        "TextInput.setClient" => {
            if let Some(arr) = args.and_then(|a| a.as_array())
                && let (Some(id_val), Some(cfg_map)) = (
                    arr.first().and_then(|v| v.as_i64()),
                    arr.get(1).and_then(|v| v.as_object()),
                )
            {
                let client_id = id_val as i32;
                let action = cfg_map
                    .get("inputAction")
                    .and_then(|v| v.as_str())
                    .unwrap_or("TextInputAction.done")
                    .to_string();
                *slot = Some(ActiveTextInputState {
                    client_id,
                    input_action: action,
                    model: TextInputModel::new(),
                });
            }
        }
        "TextInput.clearClient" => {
            *slot = None;
        }
        "TextInput.setEditingState" => {
            if let Some(current_state) = slot.as_mut()
                && let Some(state_map_val) = args.and_then(|a| a.as_object())
                && let Ok(flutter_state) =
                    from_value::<FlutterTextEditingState>(Value::Object(state_map_val.clone()))
            {
                current_state.model.text = flutter_state.text;
                current_state.model.selection_base_utf8 = utf16_code_unit_offset_to_utf8_byte_offset(
                    &current_state.model.text,
                    flutter_state.selection_base.max(0) as usize,
                );
                current_state.model.selection_extent_utf8 =
                    utf16_code_unit_offset_to_utf8_byte_offset(
                        &current_state.model.text,
                        flutter_state.selection_extent.max(0) as usize,
                    );
                current_state.model.sanitize_offsets();
            }
        }
        _ => {}
    }
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

        if let Ok(parsed_json) = from_slice::<Value>(slice)
            && let Some(method_call) = parsed_json.as_object()
                && let Some(method_name) = method_call.get("method").and_then(|m| m.as_str()) {
                    let args = method_call.get("args");

                    if let Ok(mut guard) = overlay.active_text_input.lock() {
                        apply_text_input_method(method_name, args, &mut guard);
                    }
                }

        if !message.response_handle.is_null() {
            let _ = (engine_dll_arc.FlutterEngineSendPlatformMessageResponse)(
                engine_handle.0,
                message.response_handle,
                ptr::null(),
                0,
            );
        }
    }
}

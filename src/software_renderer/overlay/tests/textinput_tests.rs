use crate::software_renderer::overlay::textinput::{
    ActiveTextInputState, TextInputModel, apply_text_input_method,
};
use serde_json::json;

fn model_with(text: &str, base: usize, extent: usize) -> TextInputModel {
    let mut m = TextInputModel::new();
    m.text = text.to_string();
    m.selection_base_utf8 = base;
    m.selection_extent_utf8 = extent;
    m
}

#[test]
fn insert_into_empty() {
    let mut m = TextInputModel::new();
    m.insert_char('a');
    assert_eq!(m.text, "a");
    assert_eq!(m.selection_base_utf8, 1);
    assert_eq!(m.selection_extent_utf8, 1);
}

#[test]
fn insert_newline() {
    let mut m = TextInputModel::new();
    m.insert_char('\n');
    assert_eq!(m.text, "\n");
    assert_eq!(m.selection_base_utf8, 1);
}

#[test]
fn insert_replaces_selection() {
    let mut m = model_with("hello", 1, 3);
    m.insert_char('X');
    assert_eq!(m.text, "hXlo");
    assert_eq!(m.selection_base_utf8, 2);
    assert_eq!(m.selection_extent_utf8, 2);
}

#[test]
fn insert_multibyte_advances_by_utf8_len() {
    let mut m = TextInputModel::new();
    m.insert_char('é');
    assert_eq!(m.text, "é");
    assert_eq!(m.selection_base_utf8, 2);
}

#[test]
fn backspace_single() {
    let mut m = model_with("hello", 3, 3);
    m.backspace();
    assert_eq!(m.text, "helo");
    assert_eq!(m.selection_base_utf8, 2);
}

#[test]
fn backspace_at_start_noop() {
    let mut m = model_with("abc", 0, 0);
    m.backspace();
    assert_eq!(m.text, "abc");
    assert_eq!(m.selection_base_utf8, 0);
}

#[test]
fn backspace_deletes_selection() {
    let mut m = model_with("xabcd", 1, 3);
    m.backspace();
    assert_eq!(m.text, "xcd");
    assert_eq!(m.selection_base_utf8, 1);
}

#[test]
fn backspace_multibyte() {
    let mut m = model_with("café", 5, 5);
    m.backspace();
    assert_eq!(m.text, "caf");
    assert_eq!(m.selection_base_utf8, 3);
}

#[test]
fn delete_forward_single() {
    let mut m = model_with("hello", 2, 2);
    m.delete_forward();
    assert_eq!(m.text, "helo");
    assert_eq!(m.selection_base_utf8, 2);
}

#[test]
fn delete_forward_at_end_noop() {
    let mut m = model_with("abc", 3, 3);
    m.delete_forward();
    assert_eq!(m.text, "abc");
}

#[test]
fn move_left_collapses_then_steps() {
    let mut m = model_with("abc", 2, 2);
    m.move_left(false);
    assert_eq!(m.selection_base_utf8, 1);
    assert_eq!(m.selection_extent_utf8, 1);
}

#[test]
fn move_left_extends_selection() {
    let mut m = model_with("abc", 2, 2);
    m.move_left(true);
    assert_eq!(m.selection_base_utf8, 2);
    assert_eq!(m.selection_extent_utf8, 1);
}

#[test]
fn move_right_steps() {
    let mut m = model_with("abc", 1, 1);
    m.move_right(false);
    assert_eq!(m.selection_extent_utf8, 2);
    assert_eq!(m.selection_base_utf8, 2);
}

#[test]
fn move_left_collapses_selection_to_start() {
    let mut m = model_with("abcdef", 1, 4);
    m.move_left(false);
    assert_eq!(m.selection_base_utf8, 1);
    assert_eq!(m.selection_extent_utf8, 1);
}

#[test]
fn move_right_collapses_selection_to_end() {
    let mut m = model_with("abcdef", 1, 4);
    m.move_right(false);
    assert_eq!(m.selection_base_utf8, 4);
    assert_eq!(m.selection_extent_utf8, 4);
}

#[test]
fn move_home_and_end_single_line() {
    let mut m = model_with("hello", 3, 3);
    m.move_home(false);
    assert_eq!(m.selection_extent_utf8, 0);
    m.move_end(false);
    assert_eq!(m.selection_extent_utf8, 5);
}

#[test]
fn move_home_on_second_line() {
    let text = "line1\nline2";
    let cursor = text.find("e2").unwrap();
    let mut m = model_with(text, cursor, cursor);
    m.move_home(false);
    assert_eq!(m.selection_extent_utf8, 6);
}

#[test]
fn move_up_preserves_column() {
    let text = "ab\ncd\nef";
    let cursor = text.rfind('f').unwrap();
    let mut m = model_with(text, cursor, cursor);
    m.move_up(false);
    let expected = text.find('d').unwrap();
    assert_eq!(m.selection_extent_utf8, expected);
}

#[test]
fn move_up_on_first_line_goes_to_start() {
    let mut m = model_with("abc", 2, 2);
    m.move_up(false);
    assert_eq!(m.selection_extent_utf8, 0);
}

#[test]
fn move_down_preserves_column() {
    let text = "ab\ncd\nef";
    let cursor = 1;
    let mut m = model_with(text, cursor, cursor);
    m.move_down(false);
    let expected = text.find('d').unwrap();
    assert_eq!(m.selection_extent_utf8, expected);
}

#[test]
fn move_down_on_last_line_goes_to_end() {
    let text = "abc";
    let mut m = model_with(text, 1, 1);
    m.move_down(false);
    assert_eq!(m.selection_extent_utf8, text.len());
}

#[test]
fn editing_state_utf16_ascii() {
    let m = model_with("hello", 2, 4);
    let state = m.to_flutter_editing_state();
    assert_eq!(state.text, "hello");
    assert_eq!(state.selection_base, 2);
    assert_eq!(state.selection_extent, 4);
}

#[test]
fn editing_state_utf16_multibyte() {
    let m = model_with("café", 5, 5);
    let state = m.to_flutter_editing_state();
    assert_eq!(state.selection_base, 4);
}

#[test]
fn editing_state_utf16_cjk() {
    let m = model_with("日本", 3, 3);
    let state = m.to_flutter_editing_state();
    assert_eq!(state.selection_base, 1);
}

#[test]
fn editing_state_utf16_emoji_surrogate_pair() {
    let m = model_with("😀", 4, 4);
    let state = m.to_flutter_editing_state();
    assert_eq!(state.selection_base, 2);
}

#[test]
fn apply_set_client_creates_state() {
    let mut slot: Option<ActiveTextInputState> = None;
    let args = json!([7, { "inputAction": "TextInputAction.done" }]);
    apply_text_input_method("TextInput.setClient", Some(&args), &mut slot);
    let st = slot.expect("state created");
    assert_eq!(st.client_id, 7);
    assert_eq!(st.input_action, "TextInputAction.done");
    assert_eq!(st.model.text, "");
}

#[test]
fn apply_clear_client_clears() {
    let mut slot = Some(ActiveTextInputState {
        client_id: 1,
        input_action: "x".to_string(),
        model: TextInputModel::new(),
    });
    apply_text_input_method("TextInput.clearClient", None, &mut slot);
    assert!(slot.is_none());
}

#[test]
fn apply_set_editing_state_updates_model() {
    let mut slot = Some(ActiveTextInputState {
        client_id: 1,
        input_action: "x".to_string(),
        model: TextInputModel::new(),
    });
    let args = json!({
        "text": "hello",
        "selectionBase": 2,
        "selectionExtent": 2,
        "composingBase": -1,
        "composingExtent": -1,
    });
    apply_text_input_method("TextInput.setEditingState", Some(&args), &mut slot);
    let st = slot.unwrap();
    assert_eq!(st.model.text, "hello");
    assert_eq!(st.model.selection_base_utf8, 2);
}

#[test]
fn apply_set_editing_state_utf16_to_utf8() {
    let mut slot = Some(ActiveTextInputState {
        client_id: 1,
        input_action: "x".to_string(),
        model: TextInputModel::new(),
    });
    let args = json!({
        "text": "café",
        "selectionBase": 4,
        "selectionExtent": 4,
        "composingBase": -1,
        "composingExtent": -1,
    });
    apply_text_input_method("TextInput.setEditingState", Some(&args), &mut slot);
    let st = slot.unwrap();
    assert_eq!(st.model.selection_base_utf8, 5);
}

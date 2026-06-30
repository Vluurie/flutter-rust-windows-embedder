//! Keybind parsing + matching used by the overlay manager to trigger visibility
//! toggles and generic keybind actions. Split out of the manager API so that file
//! stays focused on the public manager surface.

use std::sync::Arc;

use log::warn;

/// Callback invoked when a visibility toggle keybind fires.
/// Receives `(overlay_id, new_visibility)` and returns whether the event was consumed.
pub type VisibilityToggleCallback = Arc<dyn Fn(&str, bool) -> bool + Send + Sync + 'static>;

/// Callback for a generic keybind action. Receives the action_id.
pub type KeybindCallback = Arc<dyn Fn(&str) + Send + Sync + 'static>;

/// A parsed keybind with optional modifier requirements.
#[derive(Clone, Debug)]
pub struct Keybind {
    /// The primary virtual key code.
    pub vk: u16,
    /// Required modifier state.
    pub ctrl: bool,
    pub shift: bool,
    pub alt: bool,
}

impl Keybind {
    /// Checks if the required modifiers are currently held down.
    pub fn modifiers_match(&self) -> bool {
        use winapi::um::winuser::{GetAsyncKeyState, VK_CONTROL, VK_MENU, VK_SHIFT};
        unsafe {
            let ctrl_down = (GetAsyncKeyState(VK_CONTROL) & 0x8000u16 as i16) != 0;
            let shift_down = (GetAsyncKeyState(VK_SHIFT) & 0x8000u16 as i16) != 0;
            let alt_down = (GetAsyncKeyState(VK_MENU) & 0x8000u16 as i16) != 0;
            self.ctrl == ctrl_down && self.shift == shift_down && self.alt == alt_down
        }
    }
}

/// Converts a single key token (no modifiers) to a Windows virtual key code.
fn single_key_to_vk(name: &str) -> Option<u16> {
    match name {
        // Function keys
        "F1" => Some(0x70),
        "F2" => Some(0x71),
        "F3" => Some(0x72),
        "F4" => Some(0x73),
        "F5" => Some(0x74),
        "F6" => Some(0x75),
        "F7" => Some(0x76),
        "F8" => Some(0x77),
        "F9" => Some(0x78),
        "F10" => Some(0x79),
        "F11" => Some(0x7A),
        "F12" => Some(0x7B),
        "F13" => Some(0x7C),
        "F14" => Some(0x7D),
        "F15" => Some(0x7E),
        "F16" => Some(0x7F),
        "F17" => Some(0x80),
        "F18" => Some(0x81),
        "F19" => Some(0x82),
        "F20" => Some(0x83),
        "F21" => Some(0x84),
        "F22" => Some(0x85),
        "F23" => Some(0x86),
        "F24" => Some(0x87),
        // Common keys
        "ESCAPE" | "ESC" => Some(0x1B),
        "SPACE" => Some(0x20),
        "TAB" => Some(0x09),
        "ENTER" | "RETURN" => Some(0x0D),
        "BACKSPACE" | "BACK" => Some(0x08),
        "DELETE" | "DEL" => Some(0x2E),
        "INSERT" | "INS" => Some(0x2D),
        "HOME" => Some(0x24),
        "END" => Some(0x23),
        "PAGEUP" | "PGUP" => Some(0x21),
        "PAGEDOWN" | "PGDN" => Some(0x22),
        // Arrow keys
        "UP" => Some(0x26),
        "DOWN" => Some(0x28),
        "LEFT" => Some(0x25),
        "RIGHT" => Some(0x27),
        // Lock/toggle keys
        "PAUSE" => Some(0x13),
        "CAPSLOCK" | "CAPS" => Some(0x14),
        "NUMLOCK" => Some(0x90),
        "SCROLLLOCK" => Some(0x91),
        "PRINTSCREEN" | "PRTSC" => Some(0x2C),
        // Numpad
        "NUMPAD0" | "NUM0" => Some(0x60),
        "NUMPAD1" | "NUM1" => Some(0x61),
        "NUMPAD2" | "NUM2" => Some(0x62),
        "NUMPAD3" | "NUM3" => Some(0x63),
        "NUMPAD4" | "NUM4" => Some(0x64),
        "NUMPAD5" | "NUM5" => Some(0x65),
        "NUMPAD6" | "NUM6" => Some(0x66),
        "NUMPAD7" | "NUM7" => Some(0x67),
        "NUMPAD8" | "NUM8" => Some(0x68),
        "NUMPAD9" | "NUM9" => Some(0x69),
        "NUMPADMULTIPLY" | "NUMMUL" => Some(0x6A),
        "NUMPADADD" | "NUMADD" => Some(0x6B),
        "NUMPADSUBTRACT" | "NUMSUB" => Some(0x6D),
        "NUMPADDECIMAL" | "NUMDEC" => Some(0x6E),
        "NUMPADDIVIDE" | "NUMDIV" => Some(0x6F),
        "NUMPADENTER" => Some(0x0D),
        // Punctuation / OEM
        "SEMICOLON" => Some(0xBA),
        "EQUAL" | "EQUALS" => Some(0xBB),
        "COMMA" => Some(0xBC),
        "MINUS" => Some(0xBD),
        "PERIOD" | "DOT" => Some(0xBE),
        "SLASH" => Some(0xBF),
        "BACKQUOTE" | "TILDE" | "GRAVE" => Some(0xC0),
        "BRACKETLEFT" => Some(0xDB),
        "BACKSLASH" => Some(0xDC),
        "BRACKETRIGHT" => Some(0xDD),
        "QUOTE" | "APOSTROPHE" => Some(0xDE),
        // Media keys
        "MEDIAPLAYPAUSE" | "PLAYPAUSE" => Some(0xB3),
        "MEDIASTOP" | "STOP" => Some(0xB2),
        "MEDIANEXTTRACK" | "NEXT" => Some(0xB0),
        "MEDIAPREVTRACK" | "PREV" | "PREVIOUS" => Some(0xB1),
        "VOLUMEUP" | "VOLUP" => Some(0xAF),
        "VOLUMEDOWN" | "VOLDOWN" => Some(0xAE),
        "VOLUMEMUTE" | "MUTE" => Some(0xAD),
        // Browser keys
        "BROWSERBACK" => Some(0xA6),
        "BROWSERFORWARD" => Some(0xA7),
        "BROWSERREFRESH" => Some(0xA8),
        // Single character: A-Z, 0-9
        s if s.len() == 1 => {
            let ch = s.as_bytes()[0];
            match ch {
                b'A'..=b'Z' => Some(ch as u16),
                b'0'..=b'9' => Some(ch as u16),
                _ => None,
            }
        }
        _ => None,
    }
}

/// Parses a keybind string like `"Ctrl+Shift+F5"` into a [`Keybind`].
pub fn parse_keybind(input: &str) -> Option<Keybind> {
    let mut ctrl = false;
    let mut shift = false;
    let mut alt = false;
    let mut primary_vk: Option<u16> = None;

    for part in input.split('+') {
        let token = part.trim().to_uppercase();
        match token.as_str() {
            "CTRL" | "CONTROL" => ctrl = true,
            "SHIFT" => shift = true,
            "ALT" => alt = true,
            _ => {
                if primary_vk.is_some() {
                    warn!("[Keybind] Multiple non-modifier keys in '{input}', ignoring");
                    return None;
                }
                primary_vk = single_key_to_vk(&token);
                if primary_vk.is_none() {
                    warn!("[Keybind] Unknown key '{token}' in '{input}'");
                    return None;
                }
            }
        }
    }

    primary_vk.map(|vk| Keybind {
        vk,
        ctrl,
        shift,
        alt,
    })
}

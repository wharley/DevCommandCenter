//! A key as the bytes xterm.js sends the shell for it: control letters,
//! the arrows and the keypad (in the application mode when a program asks
//! for it), modifiers as xterm encodes them, and Option as Meta
//! (`macOptionIsMeta`). Command keys stay with the app.

use gpui::Keystroke;

/// xterm's modifier parameter: 1 + Shift + 2·Alt + 4·Ctrl.
fn modifier_code(keystroke: &Keystroke) -> u8 {
    let modifiers = &keystroke.modifiers;
    1 + modifiers.shift as u8 + 2 * modifiers.alt as u8 + 4 * modifiers.control as u8
}

/// `ESC [ … final`, with the modifiers when there are any.
fn csi(keystroke: &Keystroke, app_cursor: bool, last: char) -> String {
    match modifier_code(keystroke) {
        1 if app_cursor => format!("\x1bO{last}"),
        1 => format!("\x1b[{last}"),
        code => format!("\x1b[1;{code}{last}"),
    }
}

/// `ESC [ number ~`, with the modifiers when there are any.
fn tilde(keystroke: &Keystroke, number: u8) -> String {
    match modifier_code(keystroke) {
        1 => format!("\x1b[{number}~"),
        code => format!("\x1b[{number};{code}~"),
    }
}

fn control_byte(key: &str) -> Option<u8> {
    let mut chars = key.chars();
    let (Some(ch), None) = (chars.next(), chars.next()) else {
        return match key {
            "space" => Some(0),
            _ => None,
        };
    };
    match ch.to_ascii_lowercase() {
        letter @ 'a'..='z' => Some(letter as u8 - b'a' + 1),
        '@' | '2' | ' ' => Some(0),
        '[' | '3' => Some(0x1b),
        '\\' | '4' => Some(0x1c),
        ']' | '5' => Some(0x1d),
        '^' | '6' => Some(0x1e),
        '_' | '-' | '7' => Some(0x1f),
        '/' => Some(0x1f),
        '8' => Some(0x7f),
        _ => None,
    }
}

/// The bytes for a key, or `None` when the terminal leaves it to the app.
pub fn key_bytes(keystroke: &Keystroke, app_cursor: bool) -> Option<String> {
    let modifiers = &keystroke.modifiers;
    if modifiers.platform {
        return None;
    }
    let key = keystroke.key.as_str();
    let meta = |text: &str| {
        if modifiers.alt {
            format!("\x1b{text}")
        } else {
            text.to_string()
        }
    };
    let named = match key {
        "enter" => Some(meta("\r")),
        "backspace" if modifiers.control => Some(meta("\x08")),
        "backspace" => Some(meta("\x7f")),
        "tab" if modifiers.shift => Some("\x1b[Z".to_string()),
        "tab" => Some(meta("\t")),
        "escape" => Some(meta("\x1b")),
        "up" => Some(csi(keystroke, app_cursor, 'A')),
        "down" => Some(csi(keystroke, app_cursor, 'B')),
        "right" => Some(csi(keystroke, app_cursor, 'C')),
        "left" => Some(csi(keystroke, app_cursor, 'D')),
        "home" => Some(csi(keystroke, app_cursor, 'H')),
        "end" => Some(csi(keystroke, app_cursor, 'F')),
        "insert" => Some(tilde(keystroke, 2)),
        "delete" => Some(tilde(keystroke, 3)),
        "pageup" | "page_up" => Some(tilde(keystroke, 5)),
        "pagedown" | "page_down" => Some(tilde(keystroke, 6)),
        "f1" => Some(csi(keystroke, true, 'P')),
        "f2" => Some(csi(keystroke, true, 'Q')),
        "f3" => Some(csi(keystroke, true, 'R')),
        "f4" => Some(csi(keystroke, true, 'S')),
        "f5" => Some(tilde(keystroke, 15)),
        "f6" => Some(tilde(keystroke, 17)),
        "f7" => Some(tilde(keystroke, 18)),
        "f8" => Some(tilde(keystroke, 19)),
        "f9" => Some(tilde(keystroke, 20)),
        "f10" => Some(tilde(keystroke, 21)),
        "f11" => Some(tilde(keystroke, 23)),
        "f12" => Some(tilde(keystroke, 24)),
        _ => None,
    };
    if named.is_some() {
        return named;
    }
    if modifiers.control {
        return control_byte(key).map(|byte| meta(&(byte as char).to_string()));
    }
    if key == "space" {
        return Some(meta(" "));
    }
    // Option as Meta: the key itself, not the character Option makes.
    if modifiers.alt && key.chars().count() == 1 {
        let text = if modifiers.shift {
            key.to_uppercase()
        } else {
            key.to_string()
        };
        return Some(meta(&text));
    }
    keystroke
        .key_char
        .clone()
        .or_else(|| (key.chars().count() == 1).then(|| key.to_string()))
}

/// Pasted text, fenced when the program asked for bracketed paste.
pub fn paste_bytes(text: &str, bracketed: bool) -> String {
    let text = text.replace("\r\n", "\r").replace('\n', "\r");
    if bracketed {
        format!("\x1b[200~{}\x1b[201~", text.replace('\x1b', ""))
    } else {
        text
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bytes(spec: &str) -> Option<String> {
        key_bytes(&Keystroke::parse(spec).unwrap(), false)
    }

    #[test]
    fn letters_and_controls() {
        let mut typed = Keystroke::parse("a").unwrap();
        typed.key_char = Some("a".into());
        assert_eq!(key_bytes(&typed, false).as_deref(), Some("a"));
        assert_eq!(bytes("ctrl-c").as_deref(), Some("\x03"));
        assert_eq!(bytes("ctrl-d").as_deref(), Some("\x04"));
        assert_eq!(bytes("ctrl-[").as_deref(), Some("\x1b"));
        assert_eq!(bytes("ctrl-space").as_deref(), Some("\0"));
        assert_eq!(bytes("enter").as_deref(), Some("\r"));
        assert_eq!(bytes("backspace").as_deref(), Some("\x7f"));
        assert_eq!(bytes("shift-tab").as_deref(), Some("\x1b[Z"));
        assert_eq!(bytes("cmd-c"), None, "the app's copy");
    }

    #[test]
    fn option_is_meta() {
        assert_eq!(bytes("alt-b").as_deref(), Some("\x1bb"));
        assert_eq!(bytes("alt-backspace").as_deref(), Some("\x1b\x7f"));
        assert_eq!(bytes("alt-shift-f").as_deref(), Some("\x1bF"));
    }

    #[test]
    fn arrows_follow_the_cursor_mode_and_modifiers() {
        let up = Keystroke::parse("up").unwrap();
        assert_eq!(key_bytes(&up, false).as_deref(), Some("\x1b[A"));
        assert_eq!(key_bytes(&up, true).as_deref(), Some("\x1bOA"));
        assert_eq!(bytes("alt-left").as_deref(), Some("\x1b[1;3D"));
        assert_eq!(bytes("ctrl-right").as_deref(), Some("\x1b[1;5C"));
        assert_eq!(bytes("pageup").as_deref(), Some("\x1b[5~"));
        assert_eq!(bytes("shift-delete").as_deref(), Some("\x1b[3;2~"));
        assert_eq!(bytes("f1").as_deref(), Some("\x1bOP"));
        assert_eq!(bytes("f5").as_deref(), Some("\x1b[15~"));
    }

    #[test]
    fn pasting_turns_lines_into_returns() {
        assert_eq!(paste_bytes("a\nb\r\nc", false), "a\rb\rc");
        assert_eq!(paste_bytes("ls\n", true), "\x1b[200~ls\r\x1b[201~");
    }
}

//! A key pressed over the Device panel as the simulator's keyboard takes it:
//! a USB HID usage (page 7), with Left Shift around it for capitals and the
//! shifted symbols. US layout, like serve-sim's `type`. ⌘ shortcuts stay
//! the app's.

use gpui::Keystroke;

/// Left Shift.
pub const SHIFT: u32 = 0xE1;

/// The usage for a key name (`gpui::Keystroke::key`), unshifted.
fn usage(key: &str) -> Option<u32> {
    let named = match key {
        "enter" => 0x28,
        "escape" => 0x29,
        "backspace" => 0x2A,
        "tab" => 0x2B,
        "space" => 0x2C,
        "-" => 0x2D,
        "=" => 0x2E,
        "[" => 0x2F,
        "]" => 0x30,
        "\\" => 0x31,
        ";" => 0x33,
        "'" => 0x34,
        "`" => 0x35,
        "," => 0x36,
        "." => 0x37,
        "/" => 0x38,
        "delete" => 0x4C,
        "right" => 0x4F,
        "left" => 0x50,
        "down" => 0x51,
        "up" => 0x52,
        _ => 0,
    };
    if named != 0 {
        return Some(named);
    }
    let mut chars = key.chars();
    let (Some(ch), None) = (chars.next(), chars.next()) else {
        return None;
    };
    match ch {
        'a'..='z' => Some(0x04 + (ch as u32 - 'a' as u32)),
        '1'..='9' => Some(0x1E + (ch as u32 - '1' as u32)),
        '0' => Some(0x27),
        _ => None,
    }
}

/// What to press for `keystroke`: the key, and whether Shift goes with it.
/// `None` for what the simulator should not get (⌘ and ⌃ shortcuts).
pub fn for_keystroke(keystroke: &Keystroke) -> Option<(u32, bool)> {
    let modifiers = &keystroke.modifiers;
    if modifiers.platform || modifiers.control {
        return None;
    }
    Some((usage(&keystroke.key)?, modifiers.shift))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(text: &str) -> Option<(u32, bool)> {
        for_keystroke(&Keystroke::parse(text).unwrap())
    }

    #[test]
    fn keys_map_to_hid_usages() {
        assert_eq!(key("a"), Some((0x04, false)));
        assert_eq!(key("shift-z"), Some((0x1D, true)));
        assert_eq!(key("0"), Some((0x27, false)));
        assert_eq!(key("shift-1"), Some((0x1E, true)), "! is Shift-1");
        assert_eq!(key("enter"), Some((0x28, false)));
        assert_eq!(key("backspace"), Some((0x2A, false)));
        assert_eq!(key("left"), Some((0x50, false)));
        assert_eq!(key("cmd-c"), None, "the app's shortcut");
        assert_eq!(key("f5"), None);
    }
}

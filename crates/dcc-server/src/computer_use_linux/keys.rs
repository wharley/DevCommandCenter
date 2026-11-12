//! Keys as X11 keysyms. The shared layer names keys with the Mac's virtual
//! key codes and modifier flags (`key_code`, `modifier_flags`); here they
//! become keysyms, which X11 (XTEST, XSendEvent) and the RemoteDesktop
//! portal (`NotifyKeyboardKeysym`) both take. COMMAND is Control on Linux,
//! as cross-platform apps bind it (Cmd+C is Ctrl+C).

pub(super) const SHIFT_L: u32 = 0xffe1;
pub(super) const CONTROL_L: u32 = 0xffe3;
pub(super) const ALT_L: u32 = 0xffe9;
pub(super) const RETURN: u32 = 0xff0d;
pub(super) const TAB: u32 = 0xff09;

/// Modifiers held for a key.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct Modifiers {
    pub shift: bool,
    pub control: bool,
    pub alt: bool,
}

impl Modifiers {
    /// From the shared layer's flags (`modifier_flags`).
    pub fn from_flags(flags: u64) -> Self {
        Self {
            shift: flags & (1 << 17) != 0,
            control: flags & ((1 << 18) | (1 << 20)) != 0,
            alt: flags & (1 << 19) != 0,
        }
    }

    /// Their keysyms, in the order they are pressed.
    pub fn keysyms(self) -> Vec<u32> {
        [
            (self.control, CONTROL_L),
            (self.alt, ALT_L),
            (self.shift, SHIFT_L),
        ]
        .into_iter()
        .filter_map(|(held, keysym)| held.then_some(keysym))
        .collect()
    }

    /// The core protocol's state mask (Shift, Control, Mod1).
    pub fn x11_state(self) -> u16 {
        (if self.shift { 1 } else { 0 })
            | (if self.control { 4 } else { 0 })
            | (if self.alt { 8 } else { 0 })
    }
}

/// The keysym of a Mac virtual key code the shared layer produced.
pub(super) fn keysym_for_mac_key(code: u16) -> Option<u32> {
    const LETTERS: [(u16, char); 26] = [
        (0, 'a'),
        (11, 'b'),
        (8, 'c'),
        (2, 'd'),
        (14, 'e'),
        (3, 'f'),
        (5, 'g'),
        (4, 'h'),
        (34, 'i'),
        (38, 'j'),
        (40, 'k'),
        (37, 'l'),
        (46, 'm'),
        (45, 'n'),
        (31, 'o'),
        (35, 'p'),
        (12, 'q'),
        (15, 'r'),
        (1, 's'),
        (17, 't'),
        (32, 'u'),
        (9, 'v'),
        (13, 'w'),
        (7, 'x'),
        (16, 'y'),
        (6, 'z'),
    ];
    const DIGITS: [(u16, char); 10] = [
        (29, '0'),
        (18, '1'),
        (19, '2'),
        (20, '3'),
        (21, '4'),
        (23, '5'),
        (22, '6'),
        (26, '7'),
        (28, '8'),
        (25, '9'),
    ];
    const FUNCTION: [u16; 12] = [122, 120, 99, 118, 96, 97, 98, 100, 101, 109, 103, 111];
    let named = match code {
        36 => Some(RETURN),
        48 => Some(TAB),
        53 => Some(0xff1b),
        49 => Some(0x20),
        126 => Some(0xff52),
        125 => Some(0xff54),
        123 => Some(0xff51),
        124 => Some(0xff53),
        51 => Some(0xff08),
        117 => Some(0xffff),
        115 => Some(0xff50),
        119 => Some(0xff57),
        116 => Some(0xff55),
        121 => Some(0xff56),
        _ => None,
    };
    named
        .or_else(|| {
            LETTERS
                .iter()
                .chain(DIGITS.iter())
                .find(|(mac, _)| *mac == code)
                .map(|(_, c)| *c as u32)
        })
        .or_else(|| {
            FUNCTION
                .iter()
                .position(|mac| *mac == code)
                .map(|index| 0xffbe + index as u32)
        })
}

/// The keysym that types `c`: Latin-1 is its own code, Return and Tab
/// their keys, anything else the Unicode keysym range.
pub(super) fn keysym_for_char(c: char) -> u32 {
    match c {
        '\n' | '\r' => RETURN,
        '\t' => TAB,
        ' '..='~' | '\u{a0}'..='\u{ff}' => c as u32,
        other => 0x0100_0000 | other as u32,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mac_key_codes_become_keysyms() {
        assert_eq!(keysym_for_mac_key(36), Some(RETURN));
        assert_eq!(keysym_for_mac_key(0), Some('a' as u32));
        assert_eq!(keysym_for_mac_key(6), Some('z' as u32));
        assert_eq!(keysym_for_mac_key(29), Some('0' as u32));
        assert_eq!(keysym_for_mac_key(122), Some(0xffbe), "F1");
        assert_eq!(keysym_for_mac_key(111), Some(0xffc9), "F12");
        assert_eq!(keysym_for_mac_key(126), Some(0xff52), "Up");
        assert_eq!(keysym_for_mac_key(250), None);
    }

    #[test]
    fn command_is_control_on_linux() {
        let modifiers = Modifiers::from_flags((1 << 20) | (1 << 17));
        assert_eq!(
            modifiers,
            Modifiers {
                shift: true,
                control: true,
                alt: false
            }
        );
        assert_eq!(modifiers.keysyms(), [CONTROL_L, SHIFT_L]);
        assert_eq!(modifiers.x11_state(), 5);
    }

    #[test]
    fn characters_become_keysyms() {
        assert_eq!(keysym_for_char('a'), 0x61);
        assert_eq!(keysym_for_char('é'), 0xe9);
        assert_eq!(keysym_for_char('\n'), RETURN);
        assert_eq!(keysym_for_char('€'), 0x0100_20ac);
    }
}

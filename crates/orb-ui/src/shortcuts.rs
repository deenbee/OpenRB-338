//! The keyboard shortcuts of the menu and the browser (task-092): one definition each, used
//! both to catch the key ([`consume`]) and to write the text next to a menu item ([`text`]), so
//! the two cannot drift. The command modifier is Cmd on macOS and Ctrl elsewhere (egui's
//! `Modifiers::COMMAND`).

use eframe::egui::{Context, Key, KeyboardShortcut, ModifierNames, Modifiers, Ui};

pub const NEW: KeyboardShortcut = KeyboardShortcut::new(Modifiers::COMMAND, Key::N);
pub const OPEN: KeyboardShortcut = KeyboardShortcut::new(Modifiers::COMMAND, Key::O);
pub const SAVE: KeyboardShortcut = KeyboardShortcut::new(Modifiers::COMMAND, Key::S);
/// "Program Synth from Keyboard".
pub const KEYS_SYNTH: KeyboardShortcut = KeyboardShortcut::new(Modifiers::COMMAND, Key::F);
/// "Select Patterns from Keyboard".
pub const KEYS_PATTERNS: KeyboardShortcut = KeyboardShortcut::new(Modifiers::COMMAND, Key::G);
/// The file browser's path field.
pub const EDIT_PATH: KeyboardShortcut = KeyboardShortcut::new(Modifiers::COMMAND, Key::L);

/// Standard Copy.
pub const COPY: KeyboardShortcut = KeyboardShortcut::new(Modifiers::COMMAND, Key::C);
/// Standard Paste.
pub const PASTE: KeyboardShortcut = KeyboardShortcut::new(Modifiers::COMMAND, Key::V);
/// Copy Pattern only in Song mode.
pub const COPY_PATTERN_ONLY: KeyboardShortcut =
    KeyboardShortcut::new(Modifiers::COMMAND.plus(Modifiers::SHIFT), Key::C);
/// Copy Knobs only in Song mode.
pub const COPY_KNOBS_ONLY: KeyboardShortcut =
    KeyboardShortcut::new(Modifiers::COMMAND.plus(Modifiers::ALT), Key::C);
/// Copy all 4 machine patterns in Song mode.
pub const COPY_ALL_PATTERNS: KeyboardShortcut =
    KeyboardShortcut::new(Modifiers::COMMAND.plus(Modifiers::SHIFT), Key::A);
/// Copy Mixer & FX in Song mode.
pub const COPY_MIXER_FX: KeyboardShortcut =
    KeyboardShortcut::new(Modifiers::COMMAND.plus(Modifiers::ALT), Key::M);

/// Parses a shortcut string such as `"Ctrl+Shift+C"` or `"Cmd+V"`.
pub fn parse_shortcut(s: &str) -> Option<KeyboardShortcut> {
    let mut mods = Modifiers::NONE;
    let parts: Vec<&str> = s.split('+').map(|p| p.trim()).collect();
    if parts.is_empty() {
        return None;
    }
    let key_str = parts.last()?;
    for &m in &parts[..parts.len() - 1] {
        if m.eq_ignore_ascii_case("ctrl")
            || m.eq_ignore_ascii_case("cmd")
            || m.eq_ignore_ascii_case("command")
        {
            mods = mods.plus(Modifiers::COMMAND);
        } else if m.eq_ignore_ascii_case("shift") {
            mods = mods.plus(Modifiers::SHIFT);
        } else if m.eq_ignore_ascii_case("alt") || m.eq_ignore_ascii_case("opt") {
            mods = mods.plus(Modifiers::ALT);
        }
    }
    let key = match key_str.to_ascii_uppercase().as_str() {
        "A" => Key::A,
        "B" => Key::B,
        "C" => Key::C,
        "D" => Key::D,
        "E" => Key::E,
        "F" => Key::F,
        "G" => Key::G,
        "H" => Key::H,
        "I" => Key::I,
        "J" => Key::J,
        "K" => Key::K,
        "L" => Key::L,
        "M" => Key::M,
        "N" => Key::N,
        "O" => Key::O,
        "P" => Key::P,
        "Q" => Key::Q,
        "R" => Key::R,
        "S" => Key::S,
        "T" => Key::T,
        "U" => Key::U,
        "V" => Key::V,
        "W" => Key::W,
        "X" => Key::X,
        "Y" => Key::Y,
        "Z" => Key::Z,
        "DELETE" | "DEL" => Key::Delete,
        "SPACE" => Key::Space,
        _ => return None,
    };
    Some(KeyboardShortcut::new(mods, key))
}

/// The text of a shortcut for a platform: "Cmd+N" on macOS, "Ctrl+N" elsewhere (the macOS
/// symbol form is not in egui's default font).
pub fn text_for(sc: &KeyboardShortcut, is_mac: bool) -> String {
    sc.format(&ModifierNames::NAMES, is_mac)
}

/// The text of a shortcut on the running platform.
pub fn text(ctx: &Context, sc: &KeyboardShortcut) -> String {
    text_for(sc, ctx.os().is_mac())
}

/// Takes the shortcut's key press from the input, true if it was pressed.
pub fn consume(ui: &Ui, sc: &KeyboardShortcut) -> bool {
    ui.input_mut(|i| i.consume_shortcut(sc))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_names_cmd_on_mac_and_ctrl_elsewhere() {
        let all = [NEW, OPEN, SAVE, KEYS_SYNTH, KEYS_PATTERNS, EDIT_PATH];
        let keys = ["N", "O", "S", "F", "G", "L"];
        for (sc, k) in all.iter().zip(keys) {
            assert_eq!(text_for(sc, true), format!("Cmd+{k}"));
            assert_eq!(text_for(sc, false), format!("Ctrl+{k}"));
        }
    }

    #[test]
    fn parse_shortcut_parses_modifiers_and_keys() {
        let sc = parse_shortcut("Ctrl+Shift+C").unwrap();
        assert_eq!(sc.modifiers, Modifiers::COMMAND.plus(Modifiers::SHIFT));
        assert_eq!(sc.logical_key, Key::C);
        let sc2 = parse_shortcut("Cmd+V").unwrap();
        assert_eq!(sc2.modifiers, Modifiers::COMMAND);
        assert_eq!(sc2.logical_key, Key::V);
    }
}

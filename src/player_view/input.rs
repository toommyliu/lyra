use crate::player::view::{
    Key, KeyCode, Modifiers, MouseButton, NamedKey, Point, ViewEvent, Viewport, WheelDelta,
};
use gpui::{Bounds, Keystroke, Pixels, ScrollDelta};

pub(super) fn key(keystroke: &Keystroke) -> Option<Key> {
    let character = |text: &str| {
        let mut chars = text.chars();
        let c = chars.next()?;
        (chars.next().is_none() && !c.is_control()).then_some(c)
    };
    let code = keystroke
        .key_char
        .as_deref()
        .and_then(character)
        .map(KeyCode::Char)
        .or_else(|| named(&keystroke.key).map(KeyCode::Named))
        .or_else(|| (keystroke.key == "space").then_some(KeyCode::Char(' ')))
        .or_else(|| character(&keystroke.key).map(KeyCode::Char))?;
    Some(Key {
        code,
        modifiers: modifiers(keystroke.modifiers),
    })
}

fn named(name: &str) -> Option<NamedKey> {
    Some(match name {
        "enter" => NamedKey::Enter,
        "tab" => NamedKey::Tab,
        "backspace" => NamedKey::Backspace,
        "delete" => NamedKey::Delete,
        "escape" => NamedKey::Escape,
        "insert" => NamedKey::Insert,
        "home" => NamedKey::Home,
        "end" => NamedKey::End,
        "pageup" => NamedKey::PageUp,
        "pagedown" => NamedKey::PageDown,
        "left" => NamedKey::ArrowLeft,
        "right" => NamedKey::ArrowRight,
        "up" => NamedKey::ArrowUp,
        "down" => NamedKey::ArrowDown,
        "shift" => NamedKey::Shift,
        "control" => NamedKey::Control,
        "alt" => NamedKey::Alt,
        "command" => NamedKey::Command,
        "capslock" => NamedKey::CapsLock,
        "f1" => NamedKey::F1,
        "f2" => NamedKey::F2,
        "f3" => NamedKey::F3,
        "f4" => NamedKey::F4,
        "f5" => NamedKey::F5,
        "f6" => NamedKey::F6,
        "f7" => NamedKey::F7,
        "f8" => NamedKey::F8,
        "f9" => NamedKey::F9,
        "f10" => NamedKey::F10,
        "f11" => NamedKey::F11,
        "f12" => NamedKey::F12,
        "f13" => NamedKey::F13,
        "f14" => NamedKey::F14,
        "f15" => NamedKey::F15,
        "f16" => NamedKey::F16,
        "f17" => NamedKey::F17,
        "f18" => NamedKey::F18,
        "f19" => NamedKey::F19,
        "f20" => NamedKey::F20,
        "f21" => NamedKey::F21,
        "f22" => NamedKey::F22,
        "f23" => NamedKey::F23,
        "f24" => NamedKey::F24,
        _ => return None,
    })
}

fn modifiers(modifiers: gpui::Modifiers) -> Modifiers {
    Modifiers {
        shift: modifiers.shift,
        control: modifiers.control,
        alt: modifiers.alt,
        command: modifiers.platform,
    }
}

pub(super) fn modifier_keys(before: gpui::Modifiers, after: gpui::Modifiers) -> Vec<ViewEvent> {
    [
        (before.shift, after.shift, NamedKey::Shift),
        (before.control, after.control, NamedKey::Control),
        (before.alt, after.alt, NamedKey::Alt),
        (before.platform, after.platform, NamedKey::Command),
    ]
    .into_iter()
    .filter(|(before, after, _)| before != after)
    .map(|(_, pressed, code)| {
        let key = Key {
            code: KeyCode::Named(code),
            modifiers: modifiers(after),
        };
        if pressed {
            ViewEvent::KeyDown(key)
        } else {
            ViewEvent::KeyUp(key)
        }
    })
    .collect()
}

pub(super) fn button(button: gpui::MouseButton) -> MouseButton {
    match button {
        gpui::MouseButton::Left => MouseButton::Left,
        gpui::MouseButton::Right => MouseButton::Right,
        gpui::MouseButton::Middle => MouseButton::Middle,
        _ => MouseButton::Other,
    }
}

pub(super) fn point(position: gpui::Point<Pixels>, origin: gpui::Point<Pixels>) -> Point {
    let relative = position - origin;
    Point {
        x: relative.x.into(),
        y: relative.y.into(),
    }
}

pub(super) fn wheel(delta: ScrollDelta) -> WheelDelta {
    match delta {
        ScrollDelta::Lines(lines) => WheelDelta::Lines(lines.y),
        ScrollDelta::Pixels(pixels) => WheelDelta::Pixels(pixels.y.into()),
    }
}

pub(super) fn viewport(bounds: Bounds<Pixels>, scale: f32) -> Option<Viewport> {
    Viewport::new(bounds.size.width.into(), bounds.size.height.into(), scale)
}

pub(super) fn utf16_to_byte(text: &str, offset: usize) -> usize {
    let mut units = 0;
    for (byte, c) in text.char_indices() {
        units += c.len_utf16();
        if units > offset {
            return byte;
        }
    }
    text.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn keystroke(key: &str, key_char: Option<&str>, modifiers: gpui::Modifiers) -> Keystroke {
        Keystroke {
            modifiers,
            key: key.into(),
            key_char: key_char.map(Into::into),
        }
    }

    #[test]
    fn shift_a_types_capital_a() {
        let shift = gpui::Modifiers {
            shift: true,
            ..Default::default()
        };
        assert_eq!(
            key(&keystroke("a", Some("A"), shift)),
            Some(Key {
                code: KeyCode::Char('A'),
                modifiers: Modifiers {
                    shift: true,
                    ..Default::default()
                }
            }),
        );
    }

    #[test]
    fn enter_is_a_named_key() {
        assert_eq!(
            key(&keystroke("enter", Some("\n"), gpui::Modifiers::default())).map(|k| k.code),
            Some(KeyCode::Named(NamedKey::Enter)),
        );
    }

    #[test]
    fn windows_keystrokes_reach_the_player_as_keys() {
        let none = gpui::Modifiers::default();
        let control = gpui::Modifiers {
            control: true,
            ..none
        };
        let alt_gr = gpui::Modifiers {
            alt: true,
            ..control
        };
        let windows_key = gpui::Modifiers {
            platform: true,
            ..none
        };
        let cases = [
            (
                keystroke("c", None, control),
                "Some(Key { code: Char('c'), modifiers: Modifiers { shift: false, control: true, alt: false, command: false } })",
            ),
            (
                keystroke("!", Some("!"), none),
                "Some(Key { code: Char('!'), modifiers: Modifiers { shift: false, control: false, alt: false, command: false } })",
            ),
            (
                keystroke("enter", None, none),
                "Some(Key { code: Named(Enter), modifiers: Modifiers { shift: false, control: false, alt: false, command: false } })",
            ),
            (
                keystroke("space", Some(" "), none),
                "Some(Key { code: Char(' '), modifiers: Modifiers { shift: false, control: false, alt: false, command: false } })",
            ),
            (
                keystroke("q", Some("@"), alt_gr),
                "Some(Key { code: Char('@'), modifiers: Modifiers { shift: false, control: true, alt: true, command: false } })",
            ),
            (
                keystroke("e", None, windows_key),
                "Some(Key { code: Char('e'), modifiers: Modifiers { shift: false, control: false, alt: false, command: true } })",
            ),
            (keystroke("menu", None, none), "None"),
        ];
        for (keystroke, expected) in cases {
            assert_eq!(format!("{:?}", key(&keystroke)), expected, "{keystroke:?}");
        }
    }

    #[test]
    fn pressing_shift_is_a_shift_key_down() {
        let after = gpui::Modifiers {
            shift: true,
            ..Default::default()
        };
        assert_eq!(
            modifier_keys(gpui::Modifiers::default(), after),
            vec![ViewEvent::KeyDown(Key {
                code: KeyCode::Named(NamedKey::Shift),
                modifiers: Modifiers {
                    shift: true,
                    ..Default::default()
                },
            })],
        );
    }

    #[test]
    fn ime_offsets_convert_utf16_to_utf8_boundaries() {
        let offsets = [0, 1, 2, 3, 4, 5].map(|offset| utf16_to_byte("a😀é", offset));
        assert_eq!(offsets, [0, 1, 1, 5, 7, 7]);
    }
}

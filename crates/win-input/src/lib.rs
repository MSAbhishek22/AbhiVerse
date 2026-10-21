//! Remote input injection for ABHIVERSE (Task 006c).
//!
//! Translates protocol [`InputEvent`]s into platform input actions
//! and, on Windows, injects them with `SendInput`. The mapping layer
//! is platform-independent and unit-tested; only [`apply`] and
//! [`screen_size`] touch the OS, behind `#[cfg(windows)]`.
//!
//! The agent only calls [`apply`] when the local user started it
//! with input explicitly enabled (`--allow-input`) — receiving an
//! event never implies permission to inject it.

pub mod capture;

use abhiverse_protocol::InputEvent;

/// One concrete input action, ready for the platform layer.
#[derive(Debug, Clone, PartialEq)]
pub enum InputAction {
    /// Absolute pointer position in screen pixels.
    MoveTo {
        x: i32,
        y: i32,
    },
    /// Mouse button (0 = left, 1 = middle, 2 = right).
    ButtonDown {
        button: u8,
    },
    ButtonUp {
        button: u8,
    },
    /// Wheel movement in WHEEL_DELTA units (120 per notch).
    Wheel {
        dx: i32,
        dy: i32,
    },
    /// Key press by Windows virtual-key code.
    KeyDown {
        vk: u16,
    },
    KeyUp {
        vk: u16,
    },
    /// Literal text to type (sent as Unicode key events).
    Text(String),
}

fn clamp01(v: f64) -> f64 {
    v.clamp(0.0, 1.0)
}

/// Map a protocol event to platform actions for a screen of
/// `screen` = (width, height) pixels. Mouse coordinates arrive
/// normalised to 0.0..=1.0 over the captured frame.
pub fn event_to_actions(event: &InputEvent, screen: (i32, i32)) -> Vec<InputAction> {
    let (w, h) = screen;
    match event {
        InputEvent::MouseMove { x, y } => vec![InputAction::MoveTo {
            x: (clamp01(*x) * (w.max(2) - 1) as f64).round() as i32,
            y: (clamp01(*y) * (h.max(2) - 1) as f64).round() as i32,
        }],
        InputEvent::MouseDown { button } => vec![InputAction::ButtonDown { button: *button }],
        InputEvent::MouseUp { button } => vec![InputAction::ButtonUp { button: *button }],
        InputEvent::Scroll { dx, dy } => vec![InputAction::Wheel {
            dx: (dx * 120.0).round() as i32,
            dy: (dy * 120.0).round() as i32,
        }],
        InputEvent::KeyDown { code } => key_to_vk(code)
            .map(|vk| vec![InputAction::KeyDown { vk }])
            .unwrap_or_default(),
        InputEvent::KeyUp { code } => key_to_vk(code)
            .map(|vk| vec![InputAction::KeyUp { vk }])
            .unwrap_or_default(),
        InputEvent::Text { text } => {
            if text.is_empty() {
                Vec::new()
            } else {
                vec![InputAction::Text(text.clone())]
            }
        }
    }
}

/// Web `KeyboardEvent.code` → Windows virtual-key code.
/// Returns `None` for codes this build does not inject.
pub fn key_to_vk(code: &str) -> Option<u16> {
    let vk = match code {
        "Enter" => 0x0D,
        "Escape" => 0x1B,
        "Backspace" => 0x08,
        "Tab" => 0x09,
        "Space" => 0x20,
        "ShiftLeft" | "ShiftRight" => 0x10,
        "ControlLeft" | "ControlRight" => 0x11,
        "AltLeft" | "AltRight" => 0x12,
        "MetaLeft" | "MetaRight" => 0x5B,
        "CapsLock" => 0x14,
        "ArrowLeft" => 0x25,
        "ArrowUp" => 0x26,
        "ArrowRight" => 0x27,
        "ArrowDown" => 0x28,
        "Home" => 0x24,
        "End" => 0x23,
        "PageUp" => 0x21,
        "PageDown" => 0x22,
        "Insert" => 0x2D,
        "Delete" => 0x2E,
        "Minus" => 0xBD,
        "Equal" => 0xBB,
        "BracketLeft" => 0xDB,
        "BracketRight" => 0xDD,
        "Backslash" => 0xDC,
        "Semicolon" => 0xBA,
        "Quote" => 0xDE,
        "Comma" => 0xBC,
        "Period" => 0xBE,
        "Slash" => 0xBF,
        "Backquote" => 0xC0,
        _ => {
            if let Some(rest) = code.strip_prefix("Key") {
                let mut chars = rest.chars();
                if let (Some(c), None) = (chars.next(), chars.next()) {
                    if c.is_ascii_uppercase() {
                        return Some(c as u16); // 'A'..'Z' == VK codes
                    }
                }
                return None;
            }
            if let Some(rest) = code.strip_prefix("Digit") {
                let mut chars = rest.chars();
                if let (Some(c), None) = (chars.next(), chars.next()) {
                    if c.is_ascii_digit() {
                        return Some(c as u16); // '0'..'9' == VK codes
                    }
                }
                return None;
            }
            if let Some(rest) = code.strip_prefix('F') {
                if let Ok(n) = rest.parse::<u16>() {
                    if (1..=12).contains(&n) {
                        return Some(0x70 + (n - 1)); // VK_F1 = 0x70
                    }
                }
                return None;
            }
            return None;
        }
    };
    Some(vk)
}

/// Current primary screen size in pixels.
#[cfg(windows)]
pub fn screen_size() -> (i32, i32) {
    use windows::Win32::UI::WindowsAndMessaging::{GetSystemMetrics, SM_CXSCREEN, SM_CYSCREEN};
    let w = unsafe { GetSystemMetrics(SM_CXSCREEN) };
    let h = unsafe { GetSystemMetrics(SM_CYSCREEN) };
    if w > 0 && h > 0 {
        (w, h)
    } else {
        (1920, 1080)
    }
}

#[cfg(not(windows))]
pub fn screen_size() -> (i32, i32) {
    (1920, 1080)
}

/// Inject actions into the local desktop.
#[cfg(windows)]
pub fn apply(actions: &[InputAction]) -> Result<(), String> {
    platform_windows::apply(actions, screen_size())
}

#[cfg(not(windows))]
pub fn apply(_actions: &[InputAction]) -> Result<(), String> {
    Err("input injection is only available on Windows".to_string())
}

#[cfg(windows)]
mod platform_windows {
    use super::InputAction;
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, INPUT_MOUSE, KEYBDINPUT, KEYEVENTF_KEYUP,
        KEYEVENTF_UNICODE, MOUSEEVENTF_ABSOLUTE, MOUSEEVENTF_HWHEEL, MOUSEEVENTF_LEFTDOWN,
        MOUSEEVENTF_LEFTUP, MOUSEEVENTF_MIDDLEDOWN, MOUSEEVENTF_MIDDLEUP, MOUSEEVENTF_MOVE,
        MOUSEEVENTF_RIGHTDOWN, MOUSEEVENTF_RIGHTUP, MOUSEEVENTF_WHEEL, MOUSEINPUT,
        MOUSE_EVENT_FLAGS, VIRTUAL_KEY,
    };

    fn mouse(flags: MOUSE_EVENT_FLAGS, dx: i32, dy: i32, data: u32) -> INPUT {
        INPUT {
            r#type: INPUT_MOUSE,
            Anonymous: INPUT_0 {
                mi: MOUSEINPUT {
                    dx,
                    dy,
                    mouseData: data,
                    dwFlags: flags,
                    time: 0,
                    dwExtraInfo: 0,
                },
            },
        }
    }

    fn key(vk: u16, scan: u16, up: bool, unicode: bool) -> INPUT {
        let mut flags = windows::Win32::UI::Input::KeyboardAndMouse::KEYBD_EVENT_FLAGS(0);
        if up {
            flags |= KEYEVENTF_KEYUP;
        }
        if unicode {
            flags |= KEYEVENTF_UNICODE;
        }
        INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: VIRTUAL_KEY(vk),
                    wScan: scan,
                    dwFlags: flags,
                    time: 0,
                    dwExtraInfo: 0,
                },
            },
        }
    }

    pub fn apply(actions: &[InputAction], screen: (i32, i32)) -> Result<(), String> {
        let (w, h) = screen;
        let mut inputs: Vec<INPUT> = Vec::new();
        for action in actions {
            match action {
                InputAction::MoveTo { x, y } => {
                    let nx = if w > 1 { x * 65535 / (w - 1) } else { 0 };
                    let ny = if h > 1 { y * 65535 / (h - 1) } else { 0 };
                    inputs.push(mouse(MOUSEEVENTF_MOVE | MOUSEEVENTF_ABSOLUTE, nx, ny, 0));
                }
                InputAction::ButtonDown { button } => {
                    let flag = match button {
                        0 => MOUSEEVENTF_LEFTDOWN,
                        1 => MOUSEEVENTF_MIDDLEDOWN,
                        _ => MOUSEEVENTF_RIGHTDOWN,
                    };
                    inputs.push(mouse(flag, 0, 0, 0));
                }
                InputAction::ButtonUp { button } => {
                    let flag = match button {
                        0 => MOUSEEVENTF_LEFTUP,
                        1 => MOUSEEVENTF_MIDDLEUP,
                        _ => MOUSEEVENTF_RIGHTUP,
                    };
                    inputs.push(mouse(flag, 0, 0, 0));
                }
                InputAction::Wheel { dx, dy } => {
                    if *dy != 0 {
                        inputs.push(mouse(MOUSEEVENTF_WHEEL, 0, 0, *dy as u32));
                    }
                    if *dx != 0 {
                        inputs.push(mouse(MOUSEEVENTF_HWHEEL, 0, 0, *dx as u32));
                    }
                }
                InputAction::KeyDown { vk } => inputs.push(key(*vk, 0, false, false)),
                InputAction::KeyUp { vk } => inputs.push(key(*vk, 0, true, false)),
                InputAction::Text(text) => {
                    for unit in text.encode_utf16() {
                        inputs.push(key(0, unit, false, true));
                        inputs.push(key(0, unit, true, true));
                    }
                }
            }
        }
        if inputs.is_empty() {
            return Ok(());
        }
        let sent = unsafe { SendInput(&inputs, std::mem::size_of::<INPUT>() as i32) };
        if sent as usize == inputs.len() {
            Ok(())
        } else {
            Err(format!(
                "SendInput injected {sent} of {} events",
                inputs.len()
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mouse_move_scales_and_clamps() {
        let screen = (1920, 1080);
        assert_eq!(
            event_to_actions(&InputEvent::MouseMove { x: 0.5, y: 0.5 }, screen),
            vec![InputAction::MoveTo { x: 960, y: 540 }]
        );
        assert_eq!(
            event_to_actions(&InputEvent::MouseMove { x: 1.7, y: -0.2 }, screen),
            vec![InputAction::MoveTo { x: 1919, y: 0 }]
        );
    }

    #[test]
    fn keys_map_to_virtual_keys() {
        assert_eq!(key_to_vk("KeyA"), Some(0x41));
        assert_eq!(key_to_vk("Digit7"), Some(0x37));
        assert_eq!(key_to_vk("Enter"), Some(0x0D));
        assert_eq!(key_to_vk("F5"), Some(0x74));
        assert_eq!(key_to_vk("AudioVolumeUp"), None);
        assert_eq!(
            event_to_actions(
                &InputEvent::KeyDown {
                    code: "ArrowLeft".into()
                },
                (800, 600)
            ),
            vec![InputAction::KeyDown { vk: 0x25 }]
        );
    }

    #[test]
    fn scroll_scales_to_wheel_deltas() {
        assert_eq!(
            event_to_actions(&InputEvent::Scroll { dx: 0.0, dy: -1.5 }, (800, 600)),
            vec![InputAction::Wheel { dx: 0, dy: -180 }]
        );
    }

    #[test]
    fn non_windows_apply_reports_unavailable() {
        #[cfg(not(windows))]
        assert!(apply(&[InputAction::ButtonDown { button: 0 }]).is_err());
    }
}

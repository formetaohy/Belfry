use belfry_math::Vec2;
use belfry_ui::{Button, Event, Input, InputKey, Modifiers};
use belfry_window::{Event as WindowEvent, KeyCode, MouseButton};

pub fn handle(input: &mut Input, event: &WindowEvent, scale: f32) {
    let scale = scale.max(f32::EPSILON);
    match event {
        WindowEvent::CursorMoved { x, y } => {
            input.push(Event::Moved(Vec2::new(x / scale, y / scale)));
        }
        WindowEvent::CursorLeft => input.push(Event::Leave),
        WindowEvent::Button { button, pressed } => {
            if let Some(button) = button_of(*button) {
                input.push(if *pressed {
                    Event::Down(button)
                } else {
                    Event::Up(button)
                });
            }
        }
        WindowEvent::Wheel { lines, pixels } => {
            let delta = if pixels[1] != 0.0 {
                -pixels[1] / scale
            } else {
                -lines[1] * 40.0
            };
            input.push(Event::Wheel(Vec2::new(0.0, delta)));
        }
        WindowEvent::Key {
            code,
            pressed,
            repeat,
            text,
        } => {
            if let Some(key) = key_of(*code) {
                input.push(Event::Key {
                    key,
                    pressed: *pressed,
                    repeat: *repeat,
                });
            }
            if *pressed && let Some(text) = text {
                let modifiers = input.modifiers();
                if !modifiers.control && !modifiers.alt && !modifiers.logo {
                    input.push(Event::Text(text.clone()));
                }
            }
        }
        WindowEvent::Modifiers(state) => input.push(Event::Modifiers(Modifiers {
            shift: state.shift_key(),
            control: state.control_key(),
            alt: state.alt_key(),
            logo: state.super_key(),
        })),
        WindowEvent::Focused(focused) => {
            if !*focused {
                input.push(Event::Cancel);
            }
        }
        WindowEvent::Resized { .. } | WindowEvent::ScaleFactorChanged { .. } => {}
    }
}

fn button_of(button: MouseButton) -> Option<Button> {
    Some(match button {
        MouseButton::Left => Button::Primary,
        MouseButton::Right => Button::Secondary,
        MouseButton::Middle => Button::Middle,
        _ => return None,
    })
}

fn key_of(code: KeyCode) -> Option<InputKey> {
    Some(match code {
        KeyCode::Enter | KeyCode::NumpadEnter => InputKey::Enter,
        KeyCode::Space => InputKey::Space,
        KeyCode::Escape => InputKey::Escape,
        KeyCode::Backspace => InputKey::Backspace,
        KeyCode::Delete => InputKey::Delete,
        KeyCode::ArrowLeft => InputKey::Left,
        KeyCode::ArrowRight => InputKey::Right,
        KeyCode::ArrowUp => InputKey::Up,
        KeyCode::ArrowDown => InputKey::Down,
        KeyCode::Home => InputKey::Home,
        KeyCode::End => InputKey::End,
        KeyCode::PageUp => InputKey::PageUp,
        KeyCode::PageDown => InputKey::PageDown,
        KeyCode::Tab => InputKey::Tab,
        _ => return None,
    })
}

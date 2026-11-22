use crate::input::{Button, Event, Input, Key, Modifiers};
use belfry_math::Vec2;
use belfry_window::{Event as WindowEvent, KeyCode, MouseButton};

impl Input {
    pub fn window(&mut self, event: &WindowEvent, scale: f32) {
        let scale = scale.max(f32::EPSILON);
        match event {
            WindowEvent::CursorMoved { x, y } => {
                self.push(Event::Moved(Vec2::new(x / scale, y / scale)));
            }
            WindowEvent::CursorLeft => self.push(Event::Leave),
            WindowEvent::Button { button, pressed } => {
                if let Some(button) = button_of(*button) {
                    self.push(if *pressed {
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
                self.push(Event::Wheel(Vec2::new(0.0, delta)));
            }
            WindowEvent::Key {
                code,
                pressed,
                repeat,
                text,
            } => {
                if let Some(key) = key_of(*code) {
                    self.push(Event::Key {
                        key,
                        pressed: *pressed,
                        repeat: *repeat,
                    });
                }
                if *pressed && let Some(text) = text {
                    let modifiers = self.modifiers();
                    if !modifiers.control && !modifiers.alt && !modifiers.logo {
                        self.push(Event::Text(text.clone()));
                    }
                }
            }
            WindowEvent::Modifiers(state) => self.push(Event::Modifiers(Modifiers {
                shift: state.shift_key(),
                control: state.control_key(),
                alt: state.alt_key(),
                logo: state.super_key(),
            })),
            WindowEvent::Focused(focused) => {
                if !*focused {
                    self.push(Event::Cancel);
                }
            }
            WindowEvent::Resized { .. } | WindowEvent::ScaleFactorChanged { .. } => {}
        }
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

fn key_of(code: KeyCode) -> Option<Key> {
    Some(match code {
        KeyCode::Enter | KeyCode::NumpadEnter => Key::Enter,
        KeyCode::Space => Key::Space,
        KeyCode::Escape => Key::Escape,
        KeyCode::Backspace => Key::Backspace,
        KeyCode::Delete => Key::Delete,
        KeyCode::ArrowLeft => Key::Left,
        KeyCode::ArrowRight => Key::Right,
        KeyCode::ArrowUp => Key::Up,
        KeyCode::ArrowDown => Key::Down,
        KeyCode::Home => Key::Home,
        KeyCode::End => Key::End,
        KeyCode::PageUp => Key::PageUp,
        KeyCode::PageDown => Key::PageDown,
        KeyCode::Tab => Key::Tab,
        _ => return None,
    })
}

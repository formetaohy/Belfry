use belfry_math::Vec2;
use belfry_ui::{Clipboard, Edit, EditOutcome, Input};
use belfry_window::{Event as WindowEvent, KeyCode, ModifiersState};

struct Held(String);

impl Clipboard for Held {
    fn read(&mut self) -> Option<String> {
        Some(self.0.clone())
    }

    fn write(&mut self, text: &str) {
        self.0 = String::from(text);
    }
}

fn press(input: &mut Input, code: KeyCode, character: Option<char>, text: Option<&str>) {
    input.window(
        &WindowEvent::Key {
            code,
            character,
            pressed: true,
            repeat: false,
            text: text.map(String::from),
        },
        1.0,
    );
}

fn begin(input: &mut Input) {
    input.begin(Vec2::new(1280.0, 720.0), 1.0, 1.0);
}

#[test]
fn a_commit_key_leaves_no_trace_in_the_text_it_commits() {
    let mut input = Input::new();
    press(&mut input, KeyCode::Enter, Some('\r'), Some("\r"));
    begin(&mut input);
    let mut edit = Edit::new(1, "door", 0.0);
    let mut clipboard = Held(String::new());
    assert_eq!(
        edit.poll(&input, &mut clipboard, 0.1),
        Some(EditOutcome::Commit)
    );
    assert_eq!(edit.text(), "door");

    let mut input = Input::new();
    press(&mut input, KeyCode::Tab, Some('\t'), Some("\t"));
    begin(&mut input);
    let mut edit = Edit::new(2, "door", 0.0);
    assert_eq!(
        edit.poll(&input, &mut clipboard, 0.1),
        Some(EditOutcome::Advance)
    );
    assert_eq!(edit.text(), "door");
}

#[test]
fn a_command_character_reaches_the_field_it_edits() {
    let mut input = Input::new();
    input.window(&WindowEvent::Modifiers(ModifiersState::CONTROL), 1.0);
    press(&mut input, KeyCode::KeyV, Some('v'), None);
    begin(&mut input);
    let mut edit = Edit::new(1, "", 0.0);
    let mut clipboard = Held(String::from("pasted"));
    assert_eq!(edit.poll(&input, &mut clipboard, 0.1), None);
    assert_eq!(edit.text(), "pasted");
}

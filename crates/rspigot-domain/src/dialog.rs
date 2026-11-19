use std::collections::HashSet;

use crate::Identifier;
use crate::text::Component;

pub const MAX_INPUT_LENGTH: u32 = 4096;
pub const DEFAULT_INPUT_LENGTH: u32 = 256;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum DialogError {
    #[error("a dialog needs at least one button")]
    NoButton,
    #[error("`{0}` is not a valid input key: use letters, digits and underscores")]
    InputKey(String),
    #[error("the input key `{0}` is used twice")]
    DuplicateKey(String),
    #[error("`{0}` is not a valid button id: use `namespace:path`")]
    ButtonId(String),
    #[error("an input holds at most {MAX_INPUT_LENGTH} characters, got {0}")]
    InputLength(u32),
}

#[derive(Debug, Clone, PartialEq)]
pub struct DialogInput {
    pub key: String,
    pub label: Component,
    pub initial: String,
    pub max_length: u32,
    pub multiline: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DialogButton {
    pub label: Component,
    pub id: Identifier,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Dialog {
    pub title: Component,
    pub body: Vec<Component>,
    pub inputs: Vec<DialogInput>,
    pub buttons: Vec<DialogButton>,
    pub can_close_with_escape: bool,
}

impl Dialog {
    pub fn new(
        title: Component,
        body: Vec<Component>,
        inputs: Vec<DialogInput>,
        buttons: Vec<DialogButton>,
        can_close_with_escape: bool,
    ) -> Result<Self, DialogError> {
        if buttons.is_empty() {
            return Err(DialogError::NoButton);
        }
        let mut seen = HashSet::new();
        for input in &inputs {
            let valid = !input.key.is_empty()
                && input
                    .key
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '_');
            if !valid {
                return Err(DialogError::InputKey(input.key.clone()));
            }
            if !seen.insert(input.key.as_str()) {
                return Err(DialogError::DuplicateKey(input.key.clone()));
            }
            if input.max_length == 0 || input.max_length > MAX_INPUT_LENGTH {
                return Err(DialogError::InputLength(input.max_length));
            }
        }
        Ok(Self {
            title,
            body,
            inputs,
            buttons,
            can_close_with_escape,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn button() -> DialogButton {
        DialogButton {
            label: Component::text("Run"),
            id: Identifier::parse("agent:run").unwrap(),
        }
    }

    fn input(key: &str) -> DialogInput {
        DialogInput {
            key: key.to_owned(),
            label: Component::text("Task"),
            initial: String::new(),
            max_length: DEFAULT_INPUT_LENGTH,
            multiline: false,
        }
    }

    fn dialog(inputs: Vec<DialogInput>, buttons: Vec<DialogButton>) -> Result<Dialog, DialogError> {
        Dialog::new(Component::text("Task"), Vec::new(), inputs, buttons, true)
    }

    #[test]
    fn a_dialog_needs_a_button() {
        assert_eq!(dialog(Vec::new(), Vec::new()), Err(DialogError::NoButton));
        assert!(dialog(vec![input("task")], vec![button()]).is_ok());
    }

    #[test]
    fn input_keys_are_word_characters_and_unique() {
        assert_eq!(
            dialog(vec![input("bad key")], vec![button()]),
            Err(DialogError::InputKey("bad key".to_owned()))
        );
        assert_eq!(
            dialog(vec![input("")], vec![button()]),
            Err(DialogError::InputKey(String::new()))
        );
        assert_eq!(
            dialog(vec![input("a"), input("a")], vec![button()]),
            Err(DialogError::DuplicateKey("a".to_owned()))
        );
    }

    #[test]
    fn an_input_length_stays_within_bounds() {
        let mut long = input("a");
        long.max_length = MAX_INPUT_LENGTH + 1;
        assert_eq!(
            dialog(vec![long], vec![button()]),
            Err(DialogError::InputLength(MAX_INPUT_LENGTH + 1))
        );
        let mut empty = input("a");
        empty.max_length = 0;
        assert_eq!(
            dialog(vec![empty], vec![button()]),
            Err(DialogError::InputLength(0))
        );
    }
}

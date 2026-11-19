use rspigot_plugin_protocol::v1;

pub const DEFAULT_INPUT_LENGTH: u32 = 256;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DialogInput {
    key: String,
    label: String,
    initial: String,
    max_length: u32,
    multiline: bool,
}

impl DialogInput {
    pub fn new(key: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            label: label.into(),
            initial: String::new(),
            max_length: DEFAULT_INPUT_LENGTH,
            multiline: false,
        }
    }

    #[must_use]
    pub fn initial(mut self, text: impl Into<String>) -> Self {
        self.initial = text.into();
        self
    }

    #[must_use]
    pub fn max_length(mut self, length: u32) -> Self {
        self.max_length = length;
        self
    }

    #[must_use]
    pub fn multiline(mut self) -> Self {
        self.multiline = true;
        self
    }

    fn into_wire(self) -> v1::DialogInput {
        v1::DialogInput {
            key: self.key,
            label: self.label,
            initial: self.initial,
            max_length: self.max_length,
            multiline: self.multiline,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DialogButton {
    label: String,
    id: String,
}

impl DialogButton {
    pub fn new(label: impl Into<String>, id: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            id: id.into(),
        }
    }

    fn into_wire(self) -> v1::DialogButton {
        v1::DialogButton {
            label: self.label,
            id: self.id,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Dialog {
    title: String,
    body: Vec<String>,
    inputs: Vec<DialogInput>,
    buttons: Vec<DialogButton>,
    can_close_with_escape: bool,
}

impl Dialog {
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            body: Vec::new(),
            inputs: Vec::new(),
            buttons: Vec::new(),
            can_close_with_escape: true,
        }
    }

    #[must_use]
    pub fn line(mut self, text: impl Into<String>) -> Self {
        self.body.push(text.into());
        self
    }

    #[must_use]
    pub fn input(mut self, input: DialogInput) -> Self {
        self.inputs.push(input);
        self
    }

    #[must_use]
    pub fn button(mut self, button: DialogButton) -> Self {
        self.buttons.push(button);
        self
    }

    #[must_use]
    pub fn locked(mut self) -> Self {
        self.can_close_with_escape = false;
        self
    }

    pub(crate) fn into_wire(self) -> v1::Dialog {
        v1::Dialog {
            title: self.title,
            body: self.body,
            inputs: self
                .inputs
                .into_iter()
                .map(DialogInput::into_wire)
                .collect(),
            buttons: self
                .buttons
                .into_iter()
                .map(DialogButton::into_wire)
                .collect(),
            can_close_with_escape: self.can_close_with_escape,
        }
    }
}

impl From<Dialog> for v1::Dialog {
    fn from(dialog: Dialog) -> Self {
        dialog.into_wire()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_dialog_becomes_its_wire_form() {
        let wire: v1::Dialog = Dialog::new("Task")
            .line("Describe it")
            .input(
                DialogInput::new("task", "What")
                    .initial("x")
                    .max_length(80)
                    .multiline(),
            )
            .button(DialogButton::new("Run", "agent:run"))
            .locked()
            .into();
        assert_eq!(wire.title, "Task");
        assert_eq!(wire.body, ["Describe it"]);
        assert_eq!(wire.inputs[0].key, "task");
        assert_eq!(wire.inputs[0].max_length, 80);
        assert!(wire.inputs[0].multiline);
        assert_eq!(wire.buttons[0].id, "agent:run");
        assert!(!wire.can_close_with_escape);
    }

    #[test]
    fn a_dialog_can_be_closed_with_escape_by_default() {
        let wire: v1::Dialog = Dialog::new("T").into();
        assert!(wire.can_close_with_escape);
        assert_eq!(wire.inputs.len() + wire.buttons.len(), 0);
    }
}

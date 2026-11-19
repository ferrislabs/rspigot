use rspigot_plugin_protocol::v1::{self, rich_click};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Click {
    RunCommand(String),
    SuggestCommand(String),
    OpenUrl(String),
    CopyToClipboard(String),
    Custom(String),
}

impl Click {
    fn into_wire(self) -> v1::RichClick {
        let action = match self {
            Self::RunCommand(command) => rich_click::Action::RunCommand(command),
            Self::SuggestCommand(command) => rich_click::Action::SuggestCommand(command),
            Self::OpenUrl(url) => rich_click::Action::OpenUrl(url),
            Self::CopyToClipboard(text) => rich_click::Action::CopyToClipboard(text),
            Self::Custom(id) => rich_click::Action::Custom(id),
        };
        v1::RichClick {
            action: Some(action),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Part {
    text: String,
    hover: Option<String>,
    click: Option<Click>,
}

impl Part {
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            hover: None,
            click: None,
        }
    }

    #[must_use]
    pub fn hover(mut self, text: impl Into<String>) -> Self {
        self.hover = Some(text.into());
        self
    }

    #[must_use]
    pub fn click(mut self, click: Click) -> Self {
        self.click = Some(click);
        self
    }

    fn into_wire(self) -> v1::RichPart {
        v1::RichPart {
            text: self.text,
            hover: self.hover,
            click: self.click.map(Click::into_wire),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RichMessage {
    parts: Vec<Part>,
}

impl RichMessage {
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn text(self, text: impl Into<String>) -> Self {
        self.part(Part::new(text))
    }

    #[must_use]
    pub fn part(mut self, part: Part) -> Self {
        self.parts.push(part);
        self
    }
}

impl From<RichMessage> for v1::RichMessage {
    fn from(message: RichMessage) -> Self {
        Self {
            parts: message.parts.into_iter().map(Part::into_wire).collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_message_keeps_its_parts_hover_and_click() {
        let wire: v1::RichMessage = RichMessage::new()
            .text("Task ")
            .part(
                Part::new("[Cancel]")
                    .hover("Stops the task")
                    .click(Click::Custom("agent:cancel".to_owned())),
            )
            .into();
        assert_eq!(wire.parts.len(), 2);
        assert_eq!(wire.parts[0].click, None);
        assert_eq!(wire.parts[1].hover.as_deref(), Some("Stops the task"));
        assert_eq!(
            wire.parts[1].click.as_ref().and_then(|c| c.action.clone()),
            Some(rich_click::Action::Custom("agent:cancel".to_owned()))
        );
    }
}

use crate::wire::{RichClick, RichMessage, RichPart, rich_click};

pub fn rich() -> Rich {
    Rich { parts: Vec::new() }
}

#[derive(Debug, Clone, Default)]
pub struct Rich {
    parts: Vec<RichPart>,
}

impl Rich {
    #[must_use]
    pub fn text(mut self, text: &str) -> Self {
        self.parts.push(RichPart {
            text: text.to_owned(),
            hover: None,
            click: None,
        });
        self
    }

    #[must_use]
    pub fn command(self, text: &str, command: &str, hover: Option<&str>) -> Self {
        self.clickable(
            text,
            rich_click::Action::RunCommand(command.to_owned()),
            hover,
        )
    }

    #[must_use]
    pub fn suggest(self, text: &str, command: &str, hover: Option<&str>) -> Self {
        self.clickable(
            text,
            rich_click::Action::SuggestCommand(command.to_owned()),
            hover,
        )
    }

    #[must_use]
    pub fn link(self, text: &str, url: &str, hover: Option<&str>) -> Self {
        self.clickable(text, rich_click::Action::OpenUrl(url.to_owned()), hover)
    }

    fn clickable(mut self, text: &str, action: rich_click::Action, hover: Option<&str>) -> Self {
        self.parts.push(RichPart {
            text: text.to_owned(),
            hover: hover.map(str::to_owned),
            click: Some(RichClick {
                action: Some(action),
            }),
        });
        self
    }

    pub fn build(self) -> RichMessage {
        RichMessage { parts: self.parts }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parts_keep_their_order_click_and_hover() {
        let message = rich()
            .text("§7Encore une ? ")
            .command("§a[Rejouer]", "/rejouer", Some("Relancer"))
            .build();
        assert_eq!(message.parts.len(), 2);
        assert!(message.parts[0].click.is_none());
        assert_eq!(message.parts[1].hover.as_deref(), Some("Relancer"));
        assert_eq!(
            message.parts[1].click.as_ref().unwrap().action,
            Some(rich_click::Action::RunCommand("/rejouer".to_owned()))
        );
    }
}

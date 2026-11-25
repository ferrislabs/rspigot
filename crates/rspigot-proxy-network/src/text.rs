use rspigot_domain::text::{ClickEvent, Component};
use rspigot_domain::text_nbt::{component_from_tag, component_to_tag};
use rspigot_protocol::types::Text;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Click {
    RunCommand(String),
    SuggestCommand(String),
    OpenUrl(String),
    CopyToClipboard(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RichPart {
    pub text: String,
    pub hover: Option<String>,
    pub click: Option<Click>,
}

pub fn legacy(text: &str) -> Text {
    Text(component_to_tag(&Component::from_legacy(text)))
}

pub fn rich(parts: &[RichPart]) -> Text {
    let mut root = Component::text("");
    for part in parts {
        let mut component = Component::from_legacy(&part.text);
        if let Some(hover) = &part.hover {
            component.style.hover = Some(Box::new(Component::from_legacy(hover)));
        }
        if let Some(click) = &part.click {
            component.style.click = Some(Box::new(match click {
                Click::RunCommand(command) => ClickEvent::RunCommand(command.clone()),
                Click::SuggestCommand(command) => ClickEvent::SuggestCommand(command.clone()),
                Click::OpenUrl(url) => ClickEvent::OpenUrl(url.clone()),
                Click::CopyToClipboard(value) => ClickEvent::CopyToClipboard(value.clone()),
            }));
        }
        root = root.append(component);
    }
    Text(component_to_tag(&root))
}

pub fn to_legacy(text: &Text) -> String {
    component_from_tag(&text.0).map_or_else(String::new, |component| component.to_legacy())
}

#[cfg(test)]
mod tests {
    use rspigot_nbt::Tag;

    use super::*;

    #[test]
    fn section_codes_survive_the_round_trip() {
        assert_eq!(to_legacy(&legacy("§aHello §lworld")), "§aHello §lworld");
    }

    #[test]
    fn rich_parts_carry_their_click_and_hover() {
        let Text(Tag::Compound(root)) = rich(&[
            RichPart {
                text: "§7Encore une ? ".to_owned(),
                hover: None,
                click: None,
            },
            RichPart {
                text: "§a[Rejouer]".to_owned(),
                hover: Some("Relancer une partie".to_owned()),
                click: Some(Click::RunCommand("/rejouer".to_owned())),
            },
        ]) else {
            panic!("expected a compound");
        };
        let Some(Tag::List(extra)) = root.get("extra") else {
            panic!("expected extra parts");
        };
        assert_eq!(extra.len(), 2);
        let rendered = format!("{root:?}");
        assert!(rendered.contains("run_command"), "{rendered}");
        assert!(rendered.contains("/rejouer"), "{rendered}");
        assert!(rendered.contains("show_text"), "{rendered}");
    }
}

use rspigot_domain::text::{Color, Component, Content, Style};
use rspigot_nbt::{Compound, Tag};
use serde_json::{Map, Value};

fn color_name(color: Color) -> String {
    match color {
        Color::Named(named) => named.name().to_owned(),
        Color::Rgb(rgb) => format!("#{:06X}", rgb & 0xFF_FFFF),
    }
}

fn style_flags(style: &Style) -> [(&'static str, Option<bool>); 5] {
    [
        ("bold", style.bold),
        ("italic", style.italic),
        ("underlined", style.underlined),
        ("strikethrough", style.strikethrough),
        ("obfuscated", style.obfuscated),
    ]
}

fn is_plain(component: &Component) -> bool {
    component.style == Style::default()
        && component.extra.is_empty()
        && matches!(component.content, Content::Text(_))
}

pub fn to_nbt(component: &Component) -> Tag {
    if is_plain(component)
        && let Content::Text(text) = &component.content
    {
        return Tag::String(text.clone());
    }
    let mut compound = Compound::new();
    match &component.content {
        Content::Text(text) => {
            compound.insert("text", text.as_str());
        }
        Content::Translatable { key, args } => {
            compound.insert("translate", key.as_str());
            if !args.is_empty() {
                compound.insert(
                    "with",
                    Tag::List(args.iter().map(|a| Tag::Compound(as_compound(a))).collect()),
                );
            }
        }
    }
    if let Some(color) = component.style.color {
        compound.insert("color", color_name(color));
    }
    for (key, value) in style_flags(&component.style) {
        if let Some(value) = value {
            compound.insert(key, value);
        }
    }
    if let Some(click) = &component.style.click {
        compound.insert("click_event", click_tag(click.as_ref()));
    }
    if let Some(hover) = &component.style.hover {
        compound.insert(
            "hover_event",
            Compound::new()
                .with("action", "show_text")
                .with("value", to_nbt(hover)),
        );
    }
    if !component.extra.is_empty() {
        compound.insert(
            "extra",
            Tag::List(
                component
                    .extra
                    .iter()
                    .map(|c| Tag::Compound(as_compound(c)))
                    .collect(),
            ),
        );
    }
    Tag::Compound(compound)
}

fn click_tag(click: &rspigot_domain::text::ClickEvent) -> Compound {
    use rspigot_domain::text::ClickEvent;

    let (action, field, value) = match click {
        ClickEvent::RunCommand(command) => ("run_command", "command", command),
        ClickEvent::SuggestCommand(command) => ("suggest_command", "command", command),
        ClickEvent::OpenUrl(url) => ("open_url", "url", url),
        ClickEvent::CopyToClipboard(text) => ("copy_to_clipboard", "value", text),
        ClickEvent::Custom(id) => ("custom", "id", id),
    };
    Compound::new()
        .with("action", action)
        .with(field, value.as_str())
}

fn as_compound(component: &Component) -> Compound {
    match to_nbt(component) {
        Tag::Compound(c) => c,
        Tag::String(text) => Compound::new().with("text", text),
        _ => Compound::new(),
    }
}

pub fn to_json_value(component: &Component) -> Value {
    let mut object = Map::new();
    match &component.content {
        Content::Text(text) => {
            object.insert("text".to_owned(), Value::String(text.clone()));
        }
        Content::Translatable { key, args } => {
            object.insert("translate".to_owned(), Value::String(key.clone()));
            if !args.is_empty() {
                object.insert(
                    "with".to_owned(),
                    Value::Array(args.iter().map(to_json_value).collect()),
                );
            }
        }
    }
    if let Some(color) = component.style.color {
        object.insert("color".to_owned(), Value::String(color_name(color)));
    }
    for (key, value) in style_flags(&component.style) {
        if let Some(value) = value {
            object.insert(key.to_owned(), Value::Bool(value));
        }
    }
    if !component.extra.is_empty() {
        object.insert(
            "extra".to_owned(),
            Value::Array(component.extra.iter().map(to_json_value).collect()),
        );
    }
    Value::Object(object)
}

pub fn to_json(component: &Component) -> String {
    to_json_value(component).to_string()
}

#[cfg(test)]
mod tests {
    use rspigot_domain::text::NamedColor;

    use super::*;

    #[test]
    fn click_and_hover_events_use_the_snake_case_layout() {
        use rspigot_domain::text::ClickEvent;

        let cases = [
            (
                ClickEvent::RunCommand("/go".into()),
                "run_command",
                "command",
                "/go",
            ),
            (
                ClickEvent::SuggestCommand("/go".into()),
                "suggest_command",
                "command",
                "/go",
            ),
            (
                ClickEvent::OpenUrl("https://a.b".into()),
                "open_url",
                "url",
                "https://a.b",
            ),
            (
                ClickEvent::CopyToClipboard("x".into()),
                "copy_to_clipboard",
                "value",
                "x",
            ),
            (
                ClickEvent::Custom("agent:cancel".into()),
                "custom",
                "id",
                "agent:cancel",
            ),
        ];
        for (click, action, field, value) in cases {
            let mut component = Component::text("Cancel");
            component.style.click = Some(Box::new(click));
            component.style.hover = Some(Box::new(Component::text("Stops it")));
            let Tag::Compound(root) = to_nbt(&component) else {
                panic!("a clickable component is a compound");
            };
            let Some(Tag::Compound(event)) = root.get("click_event") else {
                panic!("expected a click_event");
            };
            assert_eq!(event.get("action"), Some(&Tag::String(action.into())));
            assert_eq!(event.get(field), Some(&Tag::String(value.into())));
            let Some(Tag::Compound(hover)) = root.get("hover_event") else {
                panic!("expected a hover_event");
            };
            assert_eq!(hover.get("action"), Some(&Tag::String("show_text".into())));
            assert_eq!(hover.get("value"), Some(&Tag::String("Stops it".into())));
        }
    }

    #[test]
    fn a_plain_component_stays_a_string() {
        assert_eq!(to_nbt(&Component::text("Hi")), Tag::String("Hi".into()));
    }

    #[test]
    fn plain_text_is_a_string_tag() {
        assert_eq!(to_nbt(&Component::text("hi")), Tag::String("hi".to_owned()));
    }

    #[test]
    fn styled_translation_becomes_compound() {
        let c =
            Component::translatable("multiplayer.player.joined", vec![Component::text("Alice")])
                .color(NamedColor::Yellow);
        let Tag::Compound(compound) = to_nbt(&c) else {
            panic!("expected compound");
        };
        assert_eq!(
            compound.get_str("translate"),
            Some("multiplayer.player.joined")
        );
        assert_eq!(compound.get_str("color"), Some("yellow"));
        assert_eq!(compound.get_list("with").map(<[Tag]>::len), Some(1));
        assert_eq!(
            to_json(&Component::text("x").bold()),
            r#"{"bold":true,"text":"x"}"#
        );
    }
}

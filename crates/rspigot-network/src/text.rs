use rspigot_domain::text::Component;
use rspigot_domain::text_nbt::component_to_tag;
use rspigot_nbt::Tag;
use serde_json::{Map, Number, Value};

const BOOLEAN_KEYS: [&str; 8] = [
    "bold",
    "italic",
    "underlined",
    "strikethrough",
    "obfuscated",
    "interpret",
    "plain",
    "hat",
];

pub fn to_nbt(component: &Component) -> Tag {
    component_to_tag(component)
}

fn tag_to_json(key: &str, tag: &Tag) -> Value {
    match tag {
        Tag::Byte(value) if BOOLEAN_KEYS.contains(&key) => Value::Bool(*value != 0),
        Tag::Byte(value) => Value::from(*value),
        Tag::Short(value) => Value::from(*value),
        Tag::Int(value) => Value::from(*value),
        Tag::Long(value) => Value::from(*value),
        Tag::Float(value) => Number::from_f64(f64::from(*value)).map_or(Value::Null, Value::Number),
        Tag::Double(value) => Number::from_f64(*value).map_or(Value::Null, Value::Number),
        Tag::String(text) => Value::String(text.clone()),
        Tag::List(items) => Value::Array(items.iter().map(|item| tag_to_json(key, item)).collect()),
        Tag::Compound(compound) => Value::Object(
            compound
                .iter()
                .map(|(name, value)| (name.to_owned(), tag_to_json(name, value)))
                .collect::<Map<_, _>>(),
        ),
        Tag::IntArray(values) => Value::from(values.clone()),
        Tag::LongArray(values) => Value::from(values.clone()),
        Tag::ByteArray(values) => Value::from(values.clone()),
    }
}

pub fn to_json_value(component: &Component) -> Value {
    match to_nbt(component) {
        Tag::String(text) => {
            let mut object = Map::new();
            object.insert("text".to_owned(), Value::String(text));
            Value::Object(object)
        }
        other => tag_to_json("", &other),
    }
}

pub fn to_json(component: &Component) -> String {
    to_json_value(component).to_string()
}

#[cfg(test)]
mod tests {
    use rspigot_domain::text::NamedColor;
    use rspigot_nbt::Tag;

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

    #[test]
    fn new_contents_and_style_fields_reach_the_wire() {
        use rspigot_domain::text::Content;

        let mut component = Component::text("");
        component.content = Content::Score {
            name: "@s".to_owned(),
            objective: "kills".to_owned(),
        };
        component.style.with_extras(|extras| {
            extras.insertion = Some("ins".into());
            extras.font = Some("minecraft:uniform".into());
            extras.shadow_color = Some(0xFF00_FF00);
        });
        let Tag::Compound(root) = to_nbt(&component) else {
            panic!("compound");
        };
        let score = root.get_compound("score").unwrap();
        assert_eq!(score.get_str("objective"), Some("kills"));
        assert_eq!(root.get_str("insertion"), Some("ins"));
        assert_eq!(root.get_str("font"), Some("minecraft:uniform"));
        assert_eq!(root.get_i32("shadow_color"), Some(0xFF00_FF00_u32 as i32));
        let json = to_json_value(&Component::text("a").bold());
        assert_eq!(json["bold"], serde_json::Value::Bool(true));
    }
}

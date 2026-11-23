use rspigot_domain::text::{Component, Style};
use rspigot_domain::text_nbt::{component_from_tag, read_style};
use rspigot_nbt::{Compound, Tag};
use serde_json::Value;

fn invalid(reason: &str) -> Component {
    Component::translatable(
        "argument.component.invalid",
        vec![Component::text(reason.to_owned())],
    )
}

fn json_to_tag(value: &Value) -> Tag {
    match value {
        Value::Null => Tag::String(String::new()),
        Value::Bool(flag) => Tag::Byte(i8::from(*flag)),
        Value::Number(number) => match number.as_i64() {
            Some(int) => i32::try_from(int).map_or(Tag::Long(int), Tag::Int),
            None => Tag::Double(number.as_f64().unwrap_or(0.0)),
        },
        Value::String(text) => Tag::String(text.clone()),
        Value::Array(items) => Tag::List(items.iter().map(json_to_tag).collect()),
        Value::Object(object) => {
            let mut compound = Compound::new();
            for (key, item) in object {
                compound.insert(key.as_str(), json_to_tag(item));
            }
            Tag::Compound(compound)
        }
    }
}

pub fn parse_tag(input: &str) -> Result<Tag, Component> {
    let trimmed = input.trim();
    match rspigot_nbt::parse_tag(trimmed) {
        Ok(tag) => Ok(tag),
        Err(error) => serde_json::from_str::<Value>(trimmed)
            .map(|value| json_to_tag(&value))
            .map_err(|_| invalid(error.key)),
    }
}

pub fn parse_style(input: &str) -> Result<Style, Component> {
    match parse_tag(input)? {
        Tag::Compound(compound) => Ok(read_style(&compound)),
        _ => Err(invalid("a style is an object")),
    }
}

pub fn parse(input: &str) -> Result<Component, Component> {
    let tag = parse_tag(input)?;
    component_from_tag(&tag).ok_or_else(|| invalid("not a component"))
}

#[cfg(test)]
mod tests {
    use rspigot_domain::text::{ClickEvent, Content};

    use super::*;

    #[test]
    fn strings_objects_arrays_and_translations_convert() {
        assert_eq!(parse("\"hi\"").unwrap().plain_text(), "hi");
        let styled =
            parse(r##"{"text":"a","color":"red","bold":true,"extra":[{"text":"b"}]}"##).unwrap();
        assert_eq!(styled.plain_text(), "ab");
        assert_eq!(styled.style.bold, Some(true));
        assert!(styled.style.color.is_some());
        assert_eq!(parse(r#"["x",{"text":"y"}]"#).unwrap().plain_text(), "xy");
        let translated = parse(r#"{"translate":"k","with":["1","2"]}"#).unwrap();
        assert_eq!(translated.plain_text(), "k 1 2");
        assert!(parse("{nope").is_err());
        let style = parse_style(r#"{"color":"gold","bold":true}"#).unwrap();
        assert_eq!(style.bold, Some(true));
        assert!(style.color.is_some());
        assert!(parse_style("[1]").is_err());
        assert!(parse(r#"{"color":"red"}"#).is_err());
    }

    #[test]
    fn snbt_components_parse_like_vanilla() {
        let snbt = parse("{text:'hi',bold:true,color:'red'}").unwrap();
        assert_eq!(snbt.plain_text(), "hi");
        assert_eq!(snbt.style.bold, Some(true));
        assert_eq!(parse("'plain'").unwrap().plain_text(), "plain");
        assert_eq!(parse("[{text:'a'},'b']").unwrap().plain_text(), "ab");
        let score = parse("{score:{name:'@s',objective:'o'}}").unwrap();
        assert!(matches!(score.content, Content::Score { .. }));
        let clicked =
            parse("{text:'x',click_event:{action:'change_page',page:2},insertion:'i'}").unwrap();
        assert_eq!(
            clicked.style.click.as_deref(),
            Some(&ClickEvent::ChangePage(2))
        );
        assert_eq!(clicked.style.insertion(), Some("i"));
        let style = parse_style("{shadow_color:-1,font:'minecraft:uniform'}").unwrap();
        assert_eq!(style.shadow_color(), Some(u32::MAX));
        assert_eq!(style.font(), Some("minecraft:uniform"));
    }
}

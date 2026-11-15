use rspigot_domain::text::{Color, Component};
use serde_json::Value;

fn invalid(reason: &str) -> Component {
    Component::translatable(
        "argument.component.invalid",
        vec![Component::text(reason.to_owned())],
    )
}

fn style(mut component: Component, object: &serde_json::Map<String, Value>) -> Component {
    if let Some(color) = object
        .get("color")
        .and_then(Value::as_str)
        .and_then(Color::parse)
    {
        component.style.color = Some(color);
    }
    let flag = |key: &str| object.get(key).and_then(Value::as_bool);
    component.style.bold = flag("bold");
    component.style.italic = flag("italic");
    component.style.underlined = flag("underlined");
    component.style.strikethrough = flag("strikethrough");
    component.style.obfuscated = flag("obfuscated");
    component
}

fn convert(value: &Value, depth: usize) -> Result<Component, Component> {
    if depth > 32 {
        return Err(invalid("too deep"));
    }
    match value {
        Value::String(text) => Ok(Component::text(text.clone())),
        Value::Number(number) => Ok(Component::text(number.to_string())),
        Value::Bool(flag) => Ok(Component::text(flag.to_string())),
        Value::Array(items) => {
            let mut parts = items.iter().map(|item| convert(item, depth + 1));
            let Some(first) = parts.next() else {
                return Ok(Component::text(""));
            };
            let mut head = first?;
            for part in parts {
                head = head.append(part?);
            }
            Ok(head)
        }
        Value::Object(object) => {
            let base = if let Some(text) = object.get("text") {
                Component::text(match text {
                    Value::String(s) => s.clone(),
                    other => other.to_string(),
                })
            } else if let Some(key) = object.get("translate").and_then(Value::as_str) {
                let args = match object.get("with") {
                    Some(Value::Array(items)) => items
                        .iter()
                        .map(|item| convert(item, depth + 1))
                        .collect::<Result<Vec<_>, _>>()?,
                    _ => Vec::new(),
                };
                Component::translatable(key, args)
            } else {
                return Err(invalid("no text or translate"));
            };
            let mut component = style(base, object);
            if let Some(Value::Array(extra)) = object.get("extra") {
                for item in extra {
                    component = component.append(convert(item, depth + 1)?);
                }
            }
            Ok(component)
        }
        Value::Null => Err(invalid("null")),
    }
}

pub fn parse_style(input: &str) -> Result<rspigot_domain::text::Style, Component> {
    let value: Value = serde_json::from_str(input.trim()).map_err(|e| invalid(&e.to_string()))?;
    let Value::Object(object) = value else {
        return Err(invalid("a style is an object"));
    };
    Ok(style(Component::text(""), &object).style)
}

pub fn parse(input: &str) -> Result<Component, Component> {
    let value: Value = serde_json::from_str(input.trim()).map_err(|e| invalid(&e.to_string()))?;
    convert(&value, 0)
}

#[cfg(test)]
mod tests {
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
}

use rspigot_nbt::{Compound, Tag};

use super::nbt::{compound_mut, list_mut, walk_compounds};

const STYLE_FLAGS: [&str; 5] = [
    "bold",
    "italic",
    "underlined",
    "strikethrough",
    "obfuscated",
];

fn convert_number(number: &serde_json::Number) -> Tag {
    if let Some(value) = number.as_i64() {
        return match i32::try_from(value) {
            Ok(value) => Tag::Int(value),
            Err(_) => Tag::Long(value),
        };
    }
    Tag::Double(number.as_f64().unwrap_or(0.0))
}

fn wrap_mixed(items: Vec<Tag>) -> Vec<Tag> {
    let first = items.first().map(Tag::tag_type);
    if items.iter().all(|item| Some(item.tag_type()) == first) {
        return items;
    }
    items
        .into_iter()
        .map(|item| match item {
            Tag::Compound(_) => item,
            other => Tag::Compound(Compound::new().with("", other)),
        })
        .collect()
}

fn convert_flag(value: Tag) -> Tag {
    match value {
        Tag::String(text) => Tag::Byte(i8::from(text.eq_ignore_ascii_case("true"))),
        other => other,
    }
}

fn convert_json(value: &serde_json::Value) -> Tag {
    match value {
        serde_json::Value::Null => Tag::String(String::new()),
        serde_json::Value::Bool(flag) => Tag::Byte(i8::from(*flag)),
        serde_json::Value::Number(number) => convert_number(number),
        serde_json::Value::String(text) => Tag::String(text.clone()),
        serde_json::Value::Array(items) => {
            Tag::List(wrap_mixed(items.iter().map(convert_json).collect()))
        }
        serde_json::Value::Object(entries) => {
            let mut compound = Compound::new();
            for (key, value) in entries {
                let mut converted = convert_json(value);
                if STYLE_FLAGS.contains(&key.as_str()) {
                    converted = convert_flag(converted);
                }
                compound.insert(key.as_str(), converted);
            }
            Tag::Compound(compound)
        }
    }
}

pub fn unflatten_json(text: &str) -> Tag {
    match serde_json::from_str::<serde_json::Value>(text) {
        Ok(value) => convert_json(&value),
        Err(_) => Tag::String(text.to_owned()),
    }
}

fn unflatten_in_place(tag: &mut Tag) {
    if let Tag::String(text) = tag {
        *tag = unflatten_json(text);
    }
}

fn unflatten_key(compound: &mut Compound, key: &str) {
    if let Some(value) = compound.get_mut(key) {
        unflatten_in_place(value);
    }
}

fn unflatten_list(compound: &mut Compound, key: &str) {
    if let Some(items) = list_mut(compound, key) {
        for item in items {
            unflatten_in_place(item);
        }
    }
}

fn unflatten_sign_side(sign: &mut Compound, side: &str) {
    if let Some(text) = compound_mut(sign, side) {
        unflatten_list(text, "messages");
        unflatten_list(text, "filtered_messages");
    }
}

fn unflatten_book_text(content: &mut Compound) {
    if let Some(title) = compound_mut(content, "title") {
        unflatten_key(title, "raw");
        unflatten_key(title, "filtered");
    }
    for page in list_mut(content, "pages").into_iter().flatten() {
        if let Some(page) = page.as_compound_mut() {
            unflatten_key(page, "raw");
            unflatten_key(page, "filtered");
        }
    }
}

fn unflatten_components(components: &mut Compound) {
    for key in ["minecraft:custom_name", "minecraft:item_name"] {
        unflatten_key(components, key);
    }
    unflatten_list(components, "minecraft:lore");
    if let Some(content) = compound_mut(components, "minecraft:written_book_content") {
        unflatten_book_text(content);
    }
}

pub fn unflatten_text_components_4290(root: &mut Compound) {
    walk_compounds(root, &mut |compound| {
        unflatten_key(compound, "CustomName");
        unflatten_key(compound, "LastOutput");
        unflatten_sign_side(compound, "front_text");
        unflatten_sign_side(compound, "back_text");
        if compound.get_str("id") == Some("minecraft:text_display") {
            unflatten_key(compound, "text");
        }
        if let Some(components) = compound_mut(compound, "components") {
            unflatten_components(components);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_json_strings_become_plain_strings() {
        assert_eq!(unflatten_json("\"Bob\""), Tag::String("Bob".into()));
        assert_eq!(unflatten_json("Bob"), Tag::String("Bob".into()));
    }

    #[test]
    fn json_objects_become_compounds_with_boolean_flags() {
        let tag = unflatten_json(
            "{\"text\":\"hi\",\"bold\":\"true\",\"italic\":false,\"color\":\"red\"}",
        );
        let compound = tag.as_compound().unwrap();
        assert_eq!(compound.get_str("text"), Some("hi"));
        assert_eq!(compound.get("bold"), Some(&Tag::Byte(1)));
        assert_eq!(compound.get("italic"), Some(&Tag::Byte(0)));
        assert_eq!(compound.get_str("color"), Some("red"));
    }

    #[test]
    fn mixed_arrays_wrap_their_plain_elements() {
        let tag = unflatten_json("[\"a\",{\"text\":\"b\"}]");
        let list = tag.as_list().unwrap();
        assert_eq!(list[0].as_compound().unwrap().get_str(""), Some("a"));
        assert_eq!(list[1].as_compound().unwrap().get_str("text"), Some("b"));
    }

    #[test]
    fn numbers_keep_their_integer_or_double_nature() {
        let tag = unflatten_json("{\"a\":1,\"b\":1.5}");
        let compound = tag.as_compound().unwrap();
        assert_eq!(compound.get("a"), Some(&Tag::Int(1)));
        assert_eq!(compound.get("b"), Some(&Tag::Double(1.5)));
    }

    #[test]
    fn names_signs_lore_and_books_are_unflattened() {
        let mut root = Compound::new()
            .with("CustomName", "{\"text\":\"Bob\"}")
            .with(
                "front_text",
                Compound::new().with(
                    "messages",
                    Tag::List(vec![Tag::String("{\"text\":\"x\"}".into()); 2]),
                ),
            )
            .with(
                "components",
                Compound::new()
                    .with("minecraft:custom_name", "\"Sword\"")
                    .with(
                        "minecraft:lore",
                        Tag::List(vec![Tag::String("{\"text\":\"l\"}".into())]),
                    )
                    .with(
                        "minecraft:written_book_content",
                        Compound::new().with(
                            "pages",
                            Tag::List(vec![Tag::Compound(
                                Compound::new().with("raw", "{\"text\":\"page\"}"),
                            )]),
                        ),
                    ),
            );
        unflatten_text_components_4290(&mut root);
        assert_eq!(
            root.get_compound("CustomName").unwrap().get_str("text"),
            Some("Bob")
        );
        let message = &root
            .get_compound("front_text")
            .unwrap()
            .get_list("messages")
            .unwrap()[1];
        assert_eq!(message.as_compound().unwrap().get_str("text"), Some("x"));
        let components = root.get_compound("components").unwrap();
        assert_eq!(components.get_str("minecraft:custom_name"), Some("Sword"));
        assert_eq!(
            components.get_list("minecraft:lore").unwrap()[0]
                .as_compound()
                .unwrap()
                .get_str("text"),
            Some("l")
        );
        let page = components
            .get_compound("minecraft:written_book_content")
            .unwrap()
            .get_list("pages")
            .unwrap()[0]
            .as_compound()
            .unwrap();
        assert_eq!(
            page.get_compound("raw").unwrap().get_str("text"),
            Some("page")
        );
    }
}

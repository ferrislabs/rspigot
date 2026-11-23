use rspigot_nbt::{Compound, Tag};

use crate::text::{
    ClickEvent, Color, Component, Content, HoverTarget, NbtContents, NbtSource, ObjectContents,
    ObjectNode, Style,
};

const STYLE_FLAGS: [&str; 5] = [
    "bold",
    "italic",
    "underlined",
    "strikethrough",
    "obfuscated",
];
const DEFAULT_ATLAS: &str = "minecraft:blocks";

pub fn component_to_tag(component: &Component) -> Tag {
    if component.extra.is_empty()
        && component.style == Style::default()
        && let Content::Text(text) = &component.content
    {
        return Tag::String(text.clone());
    }
    let mut out = Compound::new();
    write_content(&component.content, &mut out);
    write_style(&component.style, &mut out);
    if !component.extra.is_empty() {
        out.insert(
            "extra",
            Tag::List(component.extra.iter().map(component_to_tag).collect()),
        );
    }
    Tag::Compound(out)
}

fn write_content(content: &Content, out: &mut Compound) {
    match content {
        Content::Text(text) => {
            out.insert("text", text.as_str());
        }
        Content::Translatable {
            key,
            fallback,
            args,
        } => {
            out.insert("translate", key.as_str());
            if let Some(fallback) = fallback {
                out.insert("fallback", fallback.as_str());
            }
            if !args.is_empty() {
                out.insert(
                    "with",
                    Tag::List(args.iter().map(component_to_tag).collect()),
                );
            }
        }
        Content::Score { name, objective } => {
            out.insert(
                "score",
                Compound::new()
                    .with("name", name.as_str())
                    .with("objective", objective.as_str()),
            );
        }
        Content::Selector { pattern, separator } => {
            out.insert("selector", pattern.as_str());
            write_separator(separator.as_deref(), out);
        }
        Content::Keybind(key) => {
            out.insert("keybind", key.as_str());
        }
        Content::Nbt(nbt) => {
            let NbtContents {
                path,
                interpret,
                plain,
                separator,
                source,
            } = &**nbt;
            out.insert("nbt", path.as_str());
            if *interpret {
                out.insert("interpret", true);
            }
            if *plain {
                out.insert("plain", true);
            }
            write_separator(separator.as_deref(), out);
            match source {
                NbtSource::Block(pos) => out.insert("block", pos.as_str()),
                NbtSource::Entity(selector) => out.insert("entity", selector.as_str()),
                NbtSource::Storage(id) => out.insert("storage", id.as_str()),
            };
        }
        Content::Object(node) => {
            let ObjectNode { object, fallback } = &**node;
            match object {
                ObjectContents::Atlas { atlas, sprite } => {
                    if atlas != DEFAULT_ATLAS {
                        out.insert("atlas", atlas.as_str());
                    }
                    out.insert("sprite", sprite.as_str());
                }
                ObjectContents::Player { profile, hat } => {
                    out.insert("object", "player");
                    out.insert(
                        "player",
                        rspigot_nbt::parse_tag(profile)
                            .unwrap_or_else(|_| Tag::String(profile.clone())),
                    );
                    if !hat {
                        out.insert("hat", false);
                    }
                }
            }
            if let Some(fallback) = fallback {
                out.insert("fallback", component_to_tag(fallback));
            }
        }
    }
}

fn write_separator(separator: Option<&Component>, out: &mut Compound) {
    if let Some(separator) = separator {
        out.insert("separator", component_to_tag(separator));
    }
}

pub(crate) fn write_style(style: &Style, out: &mut Compound) {
    if let Some(color) = style.color {
        out.insert("color", color.serialize());
    }
    if let Some(shadow) = style.shadow_color() {
        out.insert("shadow_color", shadow as i32);
    }
    for (flag, value) in STYLE_FLAGS.into_iter().zip(style.flags()) {
        if let Some(value) = value {
            out.insert(flag, value);
        }
    }
    if let Some(insertion) = style.insertion() {
        out.insert("insertion", insertion);
    }
    if let Some(click) = &style.click {
        out.insert("click_event", click_to_tag(click));
    }
    if let Some(hover) = &style.hover {
        out.insert(
            "hover_event",
            Compound::new()
                .with("action", "show_text")
                .with("value", component_to_tag(hover)),
        );
    } else if let Some(target) = style.hover_target() {
        out.insert("hover_event", hover_target_to_tag(target));
    }
    if let Some(font) = style.font() {
        out.insert("font", font);
    }
}

fn click_to_tag(click: &ClickEvent) -> Compound {
    let event = |action: &str| Compound::new().with("action", action);
    match click {
        ClickEvent::RunCommand(command) => event("run_command").with("command", command.as_str()),
        ClickEvent::SuggestCommand(command) => {
            event("suggest_command").with("command", command.as_str())
        }
        ClickEvent::OpenUrl(url) => event("open_url").with("url", url.as_str()),
        ClickEvent::OpenFile(path) => event("open_file").with("path", path.as_str()),
        ClickEvent::ChangePage(page) => event("change_page").with("page", *page),
        ClickEvent::CopyToClipboard(text) => {
            event("copy_to_clipboard").with("value", text.as_str())
        }
        ClickEvent::ShowDialog(dialog) => event("show_dialog").with(
            "dialog",
            rspigot_nbt::parse_tag(dialog).unwrap_or_else(|_| Tag::String(dialog.clone())),
        ),
        ClickEvent::Custom(id) => event("custom").with("id", id.as_str()),
        ClickEvent::CustomPayload { id, payload } => {
            let mut out = event("custom").with("id", id.as_str());
            if let Ok(tag) = rspigot_nbt::parse_tag(payload) {
                out.insert("payload", tag);
            }
            out
        }
    }
}

fn hover_target_to_tag(target: &HoverTarget) -> Compound {
    match target {
        HoverTarget::Item {
            id,
            count,
            components,
        } => {
            let mut out = Compound::new()
                .with("action", "show_item")
                .with("id", id.as_str())
                .with("count", *count);
            if let Ok(Tag::Compound(parsed)) = rspigot_nbt::parse_tag(components)
                && !parsed.is_empty()
            {
                out.insert("components", parsed);
            }
            out
        }
        HoverTarget::Entity { id, uuid, name } => {
            let mut out = Compound::new()
                .with("action", "show_entity")
                .with("id", id.as_str())
                .with("uuid", Tag::IntArray(uuid.to_vec()));
            if let Some(name) = name {
                out.insert("name", component_to_tag(name));
            }
            out
        }
    }
}

pub fn component_from_tag(tag: &Tag) -> Option<Component> {
    match tag {
        Tag::String(text) => Some(Component::text(text.clone())),
        Tag::List(items) => {
            let mut parts = items.iter().filter_map(component_from_tag);
            let mut head = parts.next()?;
            for part in parts {
                head = head.append(part);
            }
            Some(head)
        }
        Tag::Compound(compound) => {
            if compound.len() == 1
                && let Some(inner) = compound.get("")
            {
                return component_from_tag(inner);
            }
            let content = read_content(compound)?;
            let mut component = Component::text("");
            component.content = content;
            component.style = read_style(compound);
            if let Some(extra) = compound.get_list("extra") {
                component.extra = extra.iter().filter_map(component_from_tag).collect();
            }
            Some(component)
        }
        Tag::Byte(v) => Some(Component::text(v.to_string())),
        Tag::Short(v) => Some(Component::text(v.to_string())),
        Tag::Int(v) => Some(Component::text(v.to_string())),
        Tag::Long(v) => Some(Component::text(v.to_string())),
        Tag::Float(v) => Some(Component::text(v.to_string())),
        Tag::Double(v) => Some(Component::text(v.to_string())),
        _ => None,
    }
}

fn read_content(compound: &Compound) -> Option<Content> {
    let explicit = compound.get_str("type");
    let wants = |name: &str, key: &str| match explicit {
        Some(kind) => kind.trim_start_matches("minecraft:") == name,
        None => compound.contains_key(key),
    };
    if wants("text", "text") {
        return Some(Content::Text(scalar_text(compound.get("text")?)?));
    }
    if wants("translatable", "translate") {
        let key = compound.get_str("translate")?.to_owned();
        let args = compound
            .get_list("with")
            .map(|items| items.iter().filter_map(component_from_tag).collect())
            .unwrap_or_default();
        return Some(Content::Translatable {
            key,
            fallback: compound
                .get_str("fallback")
                .map(|text| Box::new(text.to_owned())),
            args,
        });
    }
    if wants("score", "score") {
        let score = compound.get_compound("score")?;
        return Some(Content::Score {
            name: score.get_str("name")?.to_owned(),
            objective: score.get_str("objective")?.to_owned(),
        });
    }
    if wants("selector", "selector") {
        return Some(Content::Selector {
            pattern: compound.get_str("selector")?.to_owned(),
            separator: separator_of(compound),
        });
    }
    if wants("keybind", "keybind") {
        return Some(Content::Keybind(compound.get_str("keybind")?.to_owned()));
    }
    if wants("nbt", "nbt") {
        let source = if let Some(pos) = compound.get_str("block") {
            NbtSource::Block(pos.to_owned())
        } else if let Some(selector) = compound.get_str("entity") {
            NbtSource::Entity(selector.to_owned())
        } else {
            NbtSource::Storage(compound.get_str("storage")?.to_owned())
        };
        return Some(Content::Nbt(Box::new(NbtContents {
            path: compound.get_str("nbt")?.to_owned(),
            interpret: compound.get_bool("interpret").unwrap_or(false),
            plain: compound.get_bool("plain").unwrap_or(false),
            separator: separator_of(compound),
            source,
        })));
    }
    if explicit.is_some_and(|kind| kind.trim_start_matches("minecraft:") == "object")
        || compound.contains_key("object")
        || compound.contains_key("sprite")
        || compound.contains_key("player")
    {
        return read_object(compound);
    }
    None
}

fn read_object(compound: &Compound) -> Option<Content> {
    let kind = compound
        .get_str("object")
        .map(|k| k.trim_start_matches("minecraft:"));
    let object = match kind {
        Some("player") => player_object(compound)?,
        Some("atlas") | None if compound.contains_key("sprite") => ObjectContents::Atlas {
            atlas: compound
                .get_str("atlas")
                .unwrap_or(DEFAULT_ATLAS)
                .to_owned(),
            sprite: compound.get_str("sprite")?.to_owned(),
        },
        None => player_object(compound)?,
        Some(_) => return None,
    };
    Some(Content::Object(Box::new(ObjectNode {
        object,
        fallback: compound
            .get("fallback")
            .and_then(component_from_tag)
            .map(Box::new),
    })))
}

fn player_object(compound: &Compound) -> Option<ObjectContents> {
    let profile = compound.get("player")?;
    Some(ObjectContents::Player {
        profile: profile.to_string(),
        hat: compound.get_bool("hat").unwrap_or(true),
    })
}

fn separator_of(compound: &Compound) -> Option<Box<Component>> {
    compound
        .get("separator")
        .and_then(component_from_tag)
        .map(Box::new)
}

fn scalar_text(tag: &Tag) -> Option<String> {
    match tag {
        Tag::String(text) => Some(text.clone()),
        Tag::Byte(v) => Some(v.to_string()),
        Tag::Short(v) => Some(v.to_string()),
        Tag::Int(v) => Some(v.to_string()),
        Tag::Long(v) => Some(v.to_string()),
        Tag::Float(v) => Some(v.to_string()),
        Tag::Double(v) => Some(v.to_string()),
        _ => None,
    }
}

pub fn read_style(compound: &Compound) -> Style {
    let mut style = Style::from_parts(
        compound.get_str("color").and_then(Color::parse),
        STYLE_FLAGS.map(|flag| compound.get_bool(flag)),
    );
    let shadow = compound.get("shadow_color").and_then(shadow_color);
    let insertion = compound.get_str("insertion").map(Into::into);
    let font = compound.get_str("font").map(Into::into);
    style.with_extras(|extras| {
        extras.shadow_color = shadow;
        extras.insertion = insertion;
        extras.font = font;
    });
    if let Some(event) = compound.get_compound("click_event") {
        style.click = click_from_tag(event).map(Box::new);
    }
    if let Some(event) = compound.get_compound("hover_event") {
        read_hover(event, &mut style);
    }
    style
}

fn shadow_color(tag: &Tag) -> Option<u32> {
    match tag {
        Tag::Int(value) => Some(*value as u32),
        Tag::List(items) if items.len() == 3 || items.len() == 4 => {
            let channel = |tag: &Tag| match tag {
                Tag::Float(v) => Some(f64::from(*v)),
                Tag::Double(v) => Some(*v),
                _ => None,
            };
            let values = items.iter().map(channel).collect::<Option<Vec<_>>>()?;
            let alpha = values.get(3).copied().unwrap_or(1.0);
            let pack = |v: f64| (v.clamp(0.0, 1.0) * 255.0).round() as u32;
            Some(
                (pack(alpha) << 24)
                    | (pack(values[0]) << 16)
                    | (pack(values[1]) << 8)
                    | pack(values[2]),
            )
        }
        _ => None,
    }
}

fn click_from_tag(event: &Compound) -> Option<ClickEvent> {
    let text = |key: &str| event.get_str(key).map(str::to_owned);
    Some(match event.get_str("action")? {
        "run_command" => ClickEvent::RunCommand(text("command")?),
        "suggest_command" => ClickEvent::SuggestCommand(text("command")?),
        "open_url" => ClickEvent::OpenUrl(text("url")?),
        "open_file" => ClickEvent::OpenFile(text("path")?),
        "change_page" => ClickEvent::ChangePage(event.get_i32("page")?),
        "copy_to_clipboard" => ClickEvent::CopyToClipboard(text("value")?),
        "show_dialog" => ClickEvent::ShowDialog(match event.get("dialog")? {
            Tag::String(id) => id.clone(),
            other => other.to_string(),
        }),
        "custom" => match event.get("payload") {
            Some(payload) => ClickEvent::CustomPayload {
                id: text("id")?,
                payload: payload.to_string(),
            },
            None => ClickEvent::Custom(text("id")?),
        },
        _ => return None,
    })
}

fn read_hover(event: &Compound, style: &mut Style) {
    match event.get_str("action") {
        Some("show_text") => {
            style.hover = event
                .get("value")
                .and_then(component_from_tag)
                .map(Box::new);
        }
        Some("show_item") => {
            if let Some(id) = event.get_str("id") {
                style.with_extras(|extras| {
                    extras.hover_target = Some(HoverTarget::Item {
                        id: id.to_owned(),
                        count: event.get_i32("count").unwrap_or(1),
                        components: event
                            .get("components")
                            .map(Tag::to_string)
                            .unwrap_or_default(),
                    });
                });
            }
        }
        Some("show_entity") => {
            let uuid = event
                .get_int_array("uuid")
                .and_then(|v| <[i32; 4]>::try_from(v).ok());
            if let (Some(id), Some(uuid)) = (event.get_str("id"), uuid) {
                style.with_extras(|extras| {
                    extras.hover_target = Some(HoverTarget::Entity {
                        id: id.to_owned(),
                        uuid,
                        name: event.get("name").and_then(component_from_tag).map(Box::new),
                    });
                });
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn round_trip(component: &Component) -> Component {
        component_from_tag(&component_to_tag(component)).expect("decodes")
    }

    fn compound(snbt: &str) -> Tag {
        rspigot_nbt::parse_tag(snbt).expect("valid snbt")
    }

    #[test]
    fn every_content_kind_round_trips() {
        let separator = Some(Box::new(Component::text(", ")));
        let contents = [
            Content::Text("a".to_owned()),
            Content::Translatable {
                key: "k".to_owned(),
                fallback: Some(Box::new("fb %s".to_owned())),
                args: vec![Component::text("x")],
            },
            Content::Score {
                name: "@s".to_owned(),
                objective: "kills".to_owned(),
            },
            Content::Selector {
                pattern: "@a".to_owned(),
                separator: separator.clone(),
            },
            Content::Keybind("key.jump".to_owned()),
            Content::Nbt(Box::new(NbtContents {
                path: "a.b".to_owned(),
                interpret: true,
                plain: true,
                separator: separator.clone(),
                source: NbtSource::Storage("minecraft:s".to_owned()),
            })),
            Content::Nbt(Box::new(NbtContents {
                path: "a".to_owned(),
                interpret: false,
                plain: false,
                separator: None,
                source: NbtSource::Block("~ ~ ~".to_owned()),
            })),
            Content::Nbt(Box::new(NbtContents {
                path: "a".to_owned(),
                interpret: false,
                plain: false,
                separator: None,
                source: NbtSource::Entity("@e".to_owned()),
            })),
            Content::Object(Box::new(ObjectNode {
                object: ObjectContents::Atlas {
                    atlas: "minecraft:blocks".to_owned(),
                    sprite: "block/stone".to_owned(),
                },
                fallback: None,
            })),
            Content::Object(Box::new(ObjectNode {
                object: ObjectContents::Player {
                    profile: "\"Notch\"".to_owned(),
                    hat: false,
                },
                fallback: Some(Box::new(Component::text("?"))),
            })),
        ];
        for content in contents {
            let mut component = Component::text("");
            component.content = content;
            assert_eq!(round_trip(&component), component);
        }
    }

    #[test]
    fn style_round_trips_with_click_hover_insertion_font_and_shadow() {
        let clicks = [
            ClickEvent::RunCommand("/a".into()),
            ClickEvent::SuggestCommand("/b".into()),
            ClickEvent::OpenUrl("https://x.y".into()),
            ClickEvent::OpenFile("f.txt".into()),
            ClickEvent::ChangePage(3),
            ClickEvent::CopyToClipboard("c".into()),
            ClickEvent::ShowDialog("minecraft:d".into()),
            ClickEvent::Custom("a:b".into()),
            ClickEvent::CustomPayload {
                id: "a:b".into(),
                payload: "{k:1}".into(),
            },
        ];
        for click in clicks {
            let mut component = Component::text("t");
            component.style.click = Some(Box::new(click));
            component.style.with_extras(|extras| {
                extras.insertion = Some("ins".into());
                extras.font = Some("minecraft:uniform".into());
                extras.shadow_color = Some(0x80FF_0000);
            });
            component.style.bold = Some(true);
            assert_eq!(round_trip(&component), component);
        }
    }

    #[test]
    fn hover_item_and_entity_use_the_flattened_layout() {
        let mut item = Component::text("t");
        item.style.with_extras(|extras| {
            extras.hover_target = Some(HoverTarget::Item {
                id: "minecraft:stick".into(),
                count: 2,
                components: String::new(),
            });
        });
        let Tag::Compound(root) = component_to_tag(&item) else {
            panic!("compound");
        };
        let hover = root.get_compound("hover_event").unwrap();
        assert_eq!(hover.get_str("action"), Some("show_item"));
        assert_eq!(hover.get_str("id"), Some("minecraft:stick"));
        assert_eq!(hover.get_i32("count"), Some(2));
        assert_eq!(round_trip(&item), item);

        let mut entity = Component::text("t");
        entity.style.with_extras(|extras| {
            extras.hover_target = Some(HoverTarget::Entity {
                id: "minecraft:pig".into(),
                uuid: [1, 2, 3, 4],
                name: Some(Box::new(Component::text("Bob"))),
            });
        });
        let Tag::Compound(root) = component_to_tag(&entity) else {
            panic!("compound");
        };
        let hover = root.get_compound("hover_event").unwrap();
        assert_eq!(hover.get_int_array("uuid"), Some(&[1, 2, 3, 4][..]));
        assert_eq!(round_trip(&entity), entity);
    }

    #[test]
    fn decoding_guesses_the_content_like_the_fuzzy_codec() {
        let score =
            component_from_tag(&compound("{score:{name:'@s',objective:'o'},color:'red'}")).unwrap();
        assert!(matches!(score.content, Content::Score { .. }));
        let typed = component_from_tag(&compound("{type:'keybind',keybind:'key.use'}")).unwrap();
        assert_eq!(typed.content, Content::Keybind("key.use".into()));
        let sprite = component_from_tag(&compound("{sprite:'item/stick'}")).unwrap();
        assert!(matches!(
            &sprite.content,
            Content::Object(node) if matches!(node.object, ObjectContents::Atlas { .. })
        ));
        let shadow =
            component_from_tag(&compound("{text:'a',shadow_color:[1.0,0.0,0.0,0.5]}")).unwrap();
        assert_eq!(shadow.style.shadow_color(), Some(0x80FF_0000));
        let list = component_from_tag(&compound("['a',{text:'b'}]")).unwrap();
        assert_eq!(list.plain_text(), "ab");
        assert!(component_from_tag(&compound("{color:'red'}")).is_none());
    }
}

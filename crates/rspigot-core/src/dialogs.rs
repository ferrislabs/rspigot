use rspigot_nbt::Tag;

use crate::pack_data::registry_overlay::{self, json_to_nbt};

pub const REGISTRY: &str = "minecraft:dialog";

const KINDS: [&str; 5] = [
    "minecraft:notice",
    "minecraft:confirmation",
    "minecraft:multi_action",
    "minecraft:server_links",
    "minecraft:dialog_list",
];

#[derive(Debug, Clone, PartialEq)]
pub enum Resolved {
    Registry(String),
    Inline(Box<Tag>),
}

fn namespaced(id: &str) -> String {
    if id.contains(':') {
        id.to_owned()
    } else {
        format!("minecraft:{id}")
    }
}

pub fn validate(tag: &Tag) -> Result<(), String> {
    let Tag::Compound(compound) = tag else {
        return Err("a dialog is a compound".to_owned());
    };
    let kind = compound
        .get_str("type")
        .map(namespaced)
        .ok_or_else(|| "a dialog needs a type".to_owned())?;
    if !KINDS.contains(&kind.as_str()) {
        return Err(format!("unknown dialog type `{kind}`"));
    }
    if compound.get("title").is_none() {
        return Err("a dialog needs a title".to_owned());
    }
    match kind.as_str() {
        "minecraft:confirmation"
            if compound.get("yes").is_none() || compound.get("no").is_none() =>
        {
            Err("a confirmation needs `yes` and `no`".to_owned())
        }
        "minecraft:multi_action" if compound.get("actions").is_none() => {
            Err("a multi_action dialog needs `actions`".to_owned())
        }
        "minecraft:dialog_list" if compound.get("dialogs").is_none() => {
            Err("a dialog_list needs `dialogs`".to_owned())
        }
        _ => Ok(()),
    }
}

pub fn inline(text: &str) -> Result<Resolved, String> {
    let tag = rspigot_nbt::parse_tag(text).map_err(|error| error.to_string())?;
    validate(&tag)?;
    Ok(Resolved::Inline(Box::new(tag)))
}

pub fn lookup(spec: &str) -> Option<Result<Resolved, String>> {
    let id = namespaced(spec.trim());
    if let Some(overlay) = registry_overlay::published()
        && let Some(entry) = overlay.get(REGISTRY, &id)
    {
        let tag = match &entry.data {
            Some(data) => Tag::Compound(data.clone()),
            None => match serde_json::from_str::<serde_json::Value>(&entry.json) {
                Ok(value) => json_to_nbt(&value),
                Err(error) => return Some(Err(error.to_string())),
            },
        };
        return Some(validate(&tag).map(|()| Resolved::Inline(Box::new(tag))));
    }
    rspigot_data::registries::synced_index(REGISTRY, &id)
        .is_some()
        .then(|| Ok(Resolved::Registry(id)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inline_dialogs_of_every_vanilla_type_are_accepted() {
        for text in [
            r#"{type:"minecraft:notice",title:"Hi"}"#,
            r#"{type:"minecraft:confirmation",title:"Sure?",yes:{label:"Y"},no:{label:"N"}}"#,
            r#"{type:"minecraft:multi_action",title:"Pick",actions:[{label:"A",action:{type:"minecraft:run_command",command:"say a"}}]}"#,
            r#"{type:"minecraft:server_links",title:"Links"}"#,
            r##"{type:"minecraft:dialog_list",title:"List",dialogs:"#minecraft:quick_actions"}"##,
        ] {
            assert!(inline(text).is_ok(), "{text}");
        }
    }

    #[test]
    fn malformed_dialogs_are_refused() {
        assert!(inline(r#"{type:"minecraft:nope",title:"x"}"#).is_err());
        assert!(inline(r#"{type:"minecraft:notice"}"#).is_err());
        assert!(inline(r#"{type:"minecraft:confirmation",title:"x"}"#).is_err());
        assert!(inline(r#"{type:"minecraft:dialog_list",title:"x"}"#).is_err());
    }

    #[test]
    fn built_in_dialogs_resolve_to_registry_references() {
        assert_eq!(
            lookup("quick_actions"),
            Some(Ok(Resolved::Registry("minecraft:quick_actions".to_owned())))
        );
        assert!(lookup("t:never_defined").is_none());
    }
}

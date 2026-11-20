use rspigot_data::registries::static_index;
use rspigot_domain::entity::save::{component_from_tag, component_to_tag};
use rspigot_domain::meta::{Metadata, validate_key};
use rspigot_nbt::{Compound, Tag};

use crate::plugins::resolve::text;

const COMPONENT_TYPE: &str = "minecraft:data_component_type";
const ATTRIBUTE: &str = "minecraft:attribute";
const REMOVED: &str = "!";
const CUSTOM_DATA: &str = "minecraft:custom_data";
const PUBLIC_VALUES: &str = "PublicBukkitValues";
const ITEM_NAME: &str = "minecraft:item_name";
const OPERATIONS: [&str; 3] = ["add_value", "add_multiplied_base", "add_multiplied_total"];
const SLOT_GROUPS: [&str; 11] = [
    "any", "mainhand", "offhand", "hand", "feet", "legs", "chest", "head", "armor", "body",
    "saddle",
];
const MAX_EXPLOSIONS: usize = 256;

fn qualified(name: &str) -> String {
    if name.contains(':') {
        name.to_owned()
    } else {
        format!("minecraft:{name}")
    }
}

fn identifier(what: &str, key: &str) -> Result<(), String> {
    validate_key(&qualified(key)).map_err(|_| format!("{what} `{key}` is not a valid identifier"))
}

fn integer(components: &Compound, key: &str) -> Result<Option<i64>, String> {
    match components.get(key) {
        None => Ok(None),
        Some(tag) => tag
            .as_i64()
            .map(Some)
            .ok_or_else(|| format!("{key} is not a number")),
    }
}

fn at_least(components: &Compound, key: &str, minimum: i64) -> Result<(), String> {
    match integer(components, key)? {
        Some(value) if value < minimum => Err(format!("{key} {value} is below {minimum}")),
        Some(value) if i32::try_from(value).is_err() => Err(format!("{key} {value} is too large")),
        _ => Ok(()),
    }
}

fn component_known(name: &str) -> bool {
    static_index(COMPONENT_TYPE, &qualified(name)).is_some()
}

fn data(components: &Compound) -> Result<(), String> {
    let Some(values) = components
        .get_compound(CUSTOM_DATA)
        .and_then(|custom| custom.get_compound(PUBLIC_VALUES))
    else {
        return Ok(());
    };
    let mut checked = Metadata::new();
    for (key, value) in Metadata::from_nbt(values).own() {
        checked
            .set(key, value.clone())
            .map_err(|error| format!("item data `{key}`: {error}"))?;
    }
    Ok(())
}

fn attributes(components: &Compound) -> Result<(), String> {
    let Some(entries) = components.get("minecraft:attribute_modifiers") else {
        return Ok(());
    };
    let entries = entries
        .as_list()
        .ok_or_else(|| "attribute_modifiers is not a list".to_owned())?;
    for entry in entries {
        let entry = entry
            .as_compound()
            .ok_or_else(|| "an attribute modifier is not a compound".to_owned())?;
        let kind = entry
            .get_str("type")
            .ok_or_else(|| "an attribute modifier without a type".to_owned())?;
        if static_index(ATTRIBUTE, &qualified(kind)).is_none() {
            return Err(format!("unknown attribute {kind}"));
        }
        identifier(
            "attribute modifier id",
            entry
                .get_str("id")
                .ok_or_else(|| "an attribute modifier without an id".to_owned())?,
        )?;
        let amount = entry
            .get_f64("amount")
            .ok_or_else(|| "an attribute modifier without an amount".to_owned())?;
        if !amount.is_finite() {
            return Err(format!("attribute modifier amount {amount} is not finite"));
        }
        let operation = entry.get_str("operation").unwrap_or_default();
        if !OPERATIONS.contains(&operation) {
            return Err(format!("unknown attribute operation `{operation}`"));
        }
        if let Some(slot) = entry.get_str("slot")
            && !SLOT_GROUPS.contains(&slot)
        {
            return Err(format!("unknown slot group `{slot}`"));
        }
    }
    Ok(())
}

fn tooltip(components: &Compound) -> Result<(), String> {
    let Some(display) = components.get_compound("minecraft:tooltip_display") else {
        return Ok(());
    };
    for hidden in display.get_list("hidden_components").unwrap_or_default() {
        let name = hidden
            .as_str()
            .ok_or_else(|| "a hidden component is not a name".to_owned())?;
        if !component_known(name) {
            return Err(format!("unknown hidden component {name}"));
        }
    }
    Ok(())
}

fn fireworks(components: &Compound) -> Result<(), String> {
    let Some(rocket) = components.get_compound("minecraft:fireworks") else {
        return Ok(());
    };
    if let Some(duration) = integer(rocket, "flight_duration")?
        && !(0..=255).contains(&duration)
    {
        return Err(format!("flight duration {duration} is outside 0..=255"));
    }
    let explosions = rocket.get_list("explosions").unwrap_or_default().len();
    if explosions > MAX_EXPLOSIONS {
        return Err(format!(
            "{explosions} explosions, more than {MAX_EXPLOSIONS}"
        ));
    }
    Ok(())
}

pub(super) fn validate(components: &Compound) -> Result<(), String> {
    for (name, _) in components.iter() {
        let plain = name.strip_prefix(REMOVED).unwrap_or(name);
        if !component_known(plain) {
            return Err(format!("unknown component {name}"));
        }
    }
    at_least(components, "minecraft:damage", 0)?;
    at_least(components, "minecraft:repair_cost", 0)?;
    at_least(components, "minecraft:max_damage", 1)?;
    at_least(components, "minecraft:map_id", 0)?;
    if let Some(size) = integer(components, "minecraft:max_stack_size")?
        && !(1..=99).contains(&size)
    {
        return Err(format!("max stack size {size} is outside 1..=99"));
    }
    if let Some(model) = components.get("minecraft:item_model") {
        identifier(
            "item model",
            model
                .as_str()
                .ok_or_else(|| "item_model is not a string".to_owned())?,
        )?;
    }
    attributes(components)?;
    tooltip(components)?;
    fireworks(components)?;
    data(components)
}

pub(super) fn normalize(components: &mut Compound) {
    if let Some(Tag::String(name)) = components.get(ITEM_NAME) {
        let name = component_to_tag(&text(name));
        components.insert(ITEM_NAME, name);
    }
}

pub(super) fn legacy(components: &mut Compound) {
    if let Some(name) = components.get(ITEM_NAME).and_then(component_from_tag) {
        components.insert(ITEM_NAME, name.to_legacy());
    }
}

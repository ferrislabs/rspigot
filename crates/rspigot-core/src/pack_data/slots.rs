use rspigot_data::loot::{ContainerComponent, SlotContainer, SlotSource};
use serde_json::Value;

use super::json::{
    Parsed, invalid, kind_of, leak, leak_slice, leak_text, name, object, required, unsupported,
    whole,
};
use super::predicates::{item_predicate, target};

pub(super) fn container_component(value: &Value) -> Parsed<ContainerComponent> {
    match name(value)?.as_str() {
        "minecraft:container" => Ok(ContainerComponent::Container),
        "minecraft:bundle_contents" => Ok(ContainerComponent::BundleContents),
        "minecraft:charged_projectiles" => Ok(ContainerComponent::ChargedProjectiles),
        other => unsupported(format!("container component {other}")),
    }
}

fn child(map: &serde_json::Map<String, Value>) -> Parsed<&'static SlotSource> {
    slot_source(required(map, "slot_source")?).map(leak)
}

pub(super) fn slot_source(value: &Value) -> Parsed<SlotSource> {
    if let Value::Array(terms) = value {
        let parsed: Parsed<Vec<SlotSource>> = terms.iter().map(slot_source).collect();
        return Ok(SlotSource::Group(leak_slice(parsed?)));
    }
    let map = object(value)?;
    match kind_of(map, "type")?.as_str() {
        "minecraft:empty" => Ok(SlotSource::Empty),
        "minecraft:slot_range" => {
            let slots = match required(map, "slots")?.as_str() {
                Some(slots) if crate::commands::slots::slot_ids(slots).is_some() => slots,
                _ => return invalid("unknown slot range"),
            };
            let container = match required(map, "source")?.as_str() {
                Some("block_entity") => SlotContainer::BlockEntity,
                _ => SlotContainer::Entity(target(required(map, "source")?)?),
            };
            Ok(SlotSource::Range {
                container,
                slots: leak_text(slots.to_owned()),
            })
        }
        "minecraft:contents" => Ok(SlotSource::Contents {
            source: child(map)?,
            component: container_component(required(map, "component")?)?,
        }),
        "minecraft:filtered" => Ok(SlotSource::Filtered {
            source: child(map)?,
            filter: item_predicate(required(map, "item_filter")?)?,
        }),
        "minecraft:limit_slots" => Ok(SlotSource::Limit {
            source: child(map)?,
            limit: whole(required(map, "limit")?)?,
        }),
        "minecraft:group" => {
            let Value::Array(terms) = required(map, "terms")? else {
                return invalid("\"terms\" must be a list");
            };
            let parsed: Parsed<Vec<SlotSource>> = terms.iter().map(slot_source).collect();
            Ok(SlotSource::Group(leak_slice(parsed?)))
        }
        other => unsupported(format!("slot source {other}")),
    }
}

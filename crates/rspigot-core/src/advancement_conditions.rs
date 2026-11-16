use rspigot_data::{registries, tags};
use rspigot_domain::BlockStateId;
use serde_json::{Map, Value};

#[derive(Debug, Clone, Copy)]
pub(crate) struct EntityFacts<'a> {
    pub kind: &'a str,
    pub baby: bool,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct Place<'a> {
    pub dimension: &'a str,
    pub biome: Option<&'a str>,
    pub y: f64,
}

pub(crate) struct Facts<'a, B> {
    pub item: Option<u16>,
    pub entities: Vec<(&'static str, EntityFacts<'a>)>,
    pub player: Option<Place<'a>>,
    pub location: Option<Place<'a>>,
    pub recipe: Option<&'a str>,
    pub loot_table: Option<&'a str>,
    pub to: Option<&'a str>,
    pub amounts: Vec<(&'static str, f64)>,
    pub block_at: B,
}

#[cfg(test)]
pub(crate) type NoBlocks = fn(i32, i32, i32) -> Option<BlockStateId>;

#[cfg(test)]
fn nowhere(_: i32, _: i32, _: i32) -> Option<BlockStateId> {
    None
}

#[cfg(test)]
impl Facts<'_, NoBlocks> {
    pub(crate) fn bare() -> Self {
        Self::with_blocks(nowhere)
    }
}

impl<B> Facts<'_, B> {
    pub(crate) fn with_blocks(block_at: B) -> Self {
        Self {
            item: None,
            entities: Vec::new(),
            player: None,
            location: None,
            recipe: None,
            loot_table: None,
            to: None,
            amounts: Vec::new(),
            block_at,
        }
    }
}

const ENTITY_SLOTS: [&str; 9] = [
    "entity",
    "child",
    "parent",
    "partner",
    "source",
    "cause",
    "projectile",
    "bystander",
    "lightning",
];

fn names(value: &Value) -> Option<Vec<&str>> {
    match value {
        Value::String(one) => Some(vec![one.as_str()]),
        Value::Array(many) => many.iter().map(Value::as_str).collect(),
        _ => None,
    }
}

fn holds(registry: &str, value: &Value, id: Option<u32>) -> bool {
    let (Some(id), Some(wanted)) = (id, names(value)) else {
        return false;
    };
    wanted.iter().any(|name| match name.strip_prefix('#') {
        Some(tag) => {
            tags::tag(registry, tag).is_some_and(|members| members.binary_search(&id).is_ok())
        }
        None => {
            registries::static_index(registry, name)
                .or_else(|| registries::synced_index(registry, name))
                == Some(id)
        }
    })
}

fn in_range(value: &Value, actual: f64) -> bool {
    match value {
        Value::Number(exact) => exact.as_f64() == Some(actual),
        Value::Object(bounds) => {
            bounds.keys().all(|key| key == "min" || key == "max")
                && bounds
                    .get("min")
                    .and_then(Value::as_f64)
                    .is_none_or(|min| actual >= min)
                && bounds
                    .get("max")
                    .and_then(Value::as_f64)
                    .is_none_or(|max| actual <= max)
        }
        _ => false,
    }
}

fn item_matches(predicate: &Value, item: Option<u16>) -> bool {
    let Value::Object(map) = predicate else {
        return false;
    };
    map.iter()
        .all(|(key, value)| key == "items" && holds("minecraft:item", value, item.map(u32::from)))
}

fn block_matches(predicate: &Value, state: Option<BlockStateId>) -> bool {
    let (Value::Object(map), Some(state)) = (predicate, state) else {
        return false;
    };
    map.iter().all(|(key, value)| match key.as_str() {
        "blocks" => holds(
            "minecraft:block",
            value,
            state.kind().map(|kind| u32::from(kind.id().0)),
        ),
        "state" => state_matches(value, state),
        _ => false,
    })
}

fn state_matches(value: &Value, state: BlockStateId) -> bool {
    let Value::Object(wanted) = value else {
        return false;
    };
    wanted.iter().all(|(name, expected)| {
        expected
            .as_str()
            .is_some_and(|expected| state.property(name) == Some(expected))
    })
}

fn place_matches(predicate: &Value, place: Option<Place>) -> bool {
    let (Value::Object(map), Some(place)) = (predicate, place) else {
        return false;
    };
    map.iter().all(|(key, value)| match key.as_str() {
        "biomes" => holds(
            "minecraft:worldgen/biome",
            value,
            place.biome.and_then(registries::biome_id).map(u32::from),
        ),
        "dimension" => value.as_str() == Some(place.dimension),
        "position" => value.as_object().is_some_and(|axes| {
            axes.iter()
                .all(|(axis, range)| axis == "y" && in_range(range, place.y))
        }),
        _ => false,
    })
}

fn entity_matches(predicate: &Value, facts: Option<EntityFacts>, place: Option<Place>) -> bool {
    let Value::Object(wrapper) = predicate else {
        return false;
    };
    wrapper.iter().all(|(key, value)| match key.as_str() {
        "type" => value.as_str() == Some("minecraft:entity_properties"),
        "entity" => value.as_str() == Some("this"),
        "predicate" => value.as_object().is_some_and(|properties| {
            properties
                .iter()
                .all(|(property, wanted)| match property.as_str() {
                    "minecraft:entity_type" => holds(
                        "minecraft:entity_type",
                        wanted,
                        facts.and_then(|entity| {
                            registries::static_index("minecraft:entity_type", entity.kind)
                        }),
                    ),
                    "minecraft:flags" => wanted.as_object().is_some_and(|flags| {
                        flags.iter().all(|(flag, expected)| {
                            flag == "is_baby"
                                && facts
                                    .is_some_and(|entity| expected.as_bool() == Some(entity.baby))
                        })
                    }),
                    "minecraft:location" => place_matches(wanted, place),
                    _ => false,
                })
        }),
        _ => false,
    })
}

fn amount_matches<B>(key: &str, value: &Value, facts: &Facts<B>) -> bool {
    let amount = |name: &str| {
        facts
            .amounts
            .iter()
            .find(|(known, _)| *known == name)
            .map(|(_, actual)| *actual)
    };
    if key == "distance" {
        return value.as_object().is_some_and(|axes| {
            axes.iter().all(|(axis, range)| {
                amount(&format!("distance.{axis}")).is_some_and(|actual| in_range(range, actual))
            })
        });
    }
    amount(key).is_some_and(|actual| in_range(value, actual))
}

fn term_holds<B: Fn(i32, i32, i32) -> Option<BlockStateId>>(
    term: &Value,
    facts: &Facts<B>,
) -> bool {
    let Value::Object(map) = term else {
        return false;
    };
    let kind = map.get("type").and_then(Value::as_str).unwrap_or_default();
    let terms = || {
        map.get("terms")
            .and_then(Value::as_array)
            .map_or(&[][..], Vec::as_slice)
    };
    match kind {
        "minecraft:all_of" => terms().iter().all(|inner| term_holds(inner, facts)),
        "minecraft:any_of" => terms().iter().any(|inner| term_holds(inner, facts)),
        "minecraft:inverted" => map
            .get("term")
            .is_some_and(|inner| !term_holds(inner, facts)),
        "minecraft:match_block" => {
            let mut predicate = map.clone();
            predicate.remove("type");
            block_matches(&Value::Object(predicate), (facts.block_at)(0, 0, 0))
        }
        "minecraft:match_tool" => map
            .get("predicate")
            .is_some_and(|predicate| item_matches(predicate, facts.item)),
        "minecraft:location_check" => {
            let offset = |axis: &str| {
                map.get(axis)
                    .and_then(Value::as_i64)
                    .map_or(0, |value| value as i32)
            };
            let at = (facts.block_at)(offset("offsetX"), offset("offsetY"), offset("offsetZ"));
            let Some(Value::Object(predicate)) = map.get("predicate") else {
                return false;
            };
            predicate.iter().all(|(key, value)| match key.as_str() {
                "block" => block_matches(value, at),
                "biomes" | "dimension" | "position" => place_matches(
                    &Value::Object(Map::from_iter([(key.clone(), value.clone())])),
                    facts.location,
                ),
                _ => false,
            })
        }
        _ => false,
    }
}

pub(crate) fn matches<B: Fn(i32, i32, i32) -> Option<BlockStateId>>(
    conditions: Option<&str>,
    facts: &Facts<B>,
) -> bool {
    let Some(text) = conditions else {
        return true;
    };
    let Ok(Value::Object(map)) = serde_json::from_str::<Value>(text) else {
        return false;
    };
    map.iter().all(|(key, value)| match key.as_str() {
        "item" => item_matches(value, facts.item),
        "recipes" => facts
            .recipe
            .is_some_and(|recipe| names(value).is_some_and(|wanted| wanted.contains(&recipe))),
        "loot_tables" => facts
            .loot_table
            .is_some_and(|table| names(value).is_some_and(|wanted| wanted.contains(&table))),
        "to" => facts.to.is_some_and(|to| value.as_str() == Some(to)),
        "blocks" => block_matches(
            &Value::Object(Map::from_iter([(key.clone(), value.clone())])),
            (facts.block_at)(0, 0, 0),
        ),
        "location" => term_holds(value, facts),
        "player" => entity_matches(
            value,
            facts.player.map(|_| EntityFacts {
                kind: "minecraft:player",
                baby: false,
            }),
            facts.player,
        ),
        slot if ENTITY_SLOTS.contains(&slot) => {
            let found = facts.entities.iter().find(|(name, _)| *name == slot);
            entity_matches(value, found.map(|(_, entity)| *entity), facts.location)
        }
        other => amount_matches(other, value, facts),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use rspigot_blocks::BlockKind;

    fn item(name: &str) -> Option<u16> {
        registries::static_index("minecraft:item", name).map(|id| id as u16)
    }

    #[test]
    fn missing_conditions_always_hold() {
        assert!(matches(None, &Facts::bare()));
    }

    #[test]
    fn an_item_condition_reads_a_direct_item_and_a_tag() {
        let mut facts = Facts::bare();
        facts.item = item("minecraft:crossbow");
        let crossbow = r#"{"item":{"items":"minecraft:crossbow"}}"#;
        assert!(matches(Some(crossbow), &facts));
        assert!(!matches(Some(&crossbow.replace("crossbow", "bow")), &facts));
        let planks = r##"{"item":{"items":"#minecraft:planks"}}"##;
        assert!(!matches(Some(planks), &facts));
        facts.item = item("minecraft:oak_planks");
        assert!(matches(Some(planks), &facts));
    }

    #[test]
    fn an_unknown_condition_never_holds() {
        assert!(!matches(Some(r#"{"surprise":{}}"#), &Facts::bare()));
    }

    #[test]
    fn an_entity_slot_checks_its_type_and_baby_flag() {
        let mut facts = Facts::bare();
        facts.entities.push((
            "child",
            EntityFacts {
                kind: "minecraft:cow",
                baby: true,
            },
        ));
        let cow = r#"{"child":{"type":"minecraft:entity_properties","entity":"this","predicate":{"minecraft:entity_type":"minecraft:cow"}}}"#;
        assert!(matches(Some(cow), &facts));
        assert!(!matches(Some(&cow.replace("cow", "pig")), &facts));
        let grown = cow.replace(
            "\"minecraft:entity_type\"",
            "\"minecraft:flags\":{\"is_baby\":false},\"minecraft:entity_type\"",
        );
        assert!(!matches(Some(&grown), &facts));
    }

    #[test]
    fn a_player_place_checks_biome_dimension_and_height() {
        let mut facts = Facts::bare();
        facts.player = Some(Place {
            dimension: "minecraft:overworld",
            biome: Some("minecraft:plains"),
            y: 70.0,
        });
        let plains = r#"{"player":{"type":"minecraft:entity_properties","entity":"this","predicate":{"minecraft:location":{"biomes":"minecraft:plains"}}}}"#;
        assert!(matches(Some(plains), &facts));
        assert!(!matches(Some(&plains.replace("plains", "desert")), &facts));
        let high = r#"{"player":{"type":"minecraft:entity_properties","entity":"this","predicate":{"minecraft:location":{"position":{"y":{"min":100.0}}}}}}"#;
        assert!(!matches(Some(high), &facts));
    }

    #[test]
    fn ranges_read_numbers_and_distances() {
        let mut facts = Facts::bare();
        facts.amounts.push(("level", 4.0));
        facts.amounts.push(("distance.y", 60.0));
        assert!(matches(Some(r#"{"level":4}"#), &facts));
        assert!(matches(Some(r#"{"level":{"min":1}}"#), &facts));
        assert!(!matches(Some(r#"{"level":{"max":3}}"#), &facts));
        assert!(matches(Some(r#"{"distance":{"y":{"min":50.0}}}"#), &facts));
        assert!(!matches(
            Some(r#"{"distance":{"horizontal":{"min":50.0}}}"#),
            &facts
        ));
    }

    #[test]
    fn recipes_dimensions_and_loot_tables_compare_by_id() {
        let mut facts = Facts::bare();
        facts.recipe = Some("minecraft:crafter");
        facts.to = Some("minecraft:the_end");
        facts.loot_table = Some("minecraft:archaeology/desert_well");
        assert!(matches(Some(r#"{"recipes":"minecraft:crafter"}"#), &facts));
        assert!(!matches(Some(r#"{"recipes":"minecraft:other"}"#), &facts));
        assert!(matches(Some(r#"{"to":"minecraft:the_end"}"#), &facts));
        assert!(matches(
            Some(r#"{"loot_tables":"minecraft:archaeology/desert_well"}"#),
            &facts
        ));
    }

    #[test]
    fn location_terms_combine_block_state_tool_and_offsets() {
        let furnace = BlockKind::Furnace.default_state();
        let stone = BlockKind::Stone.default_state();
        let mut facts = Facts::with_blocks(|_, dy, _| Some(if dy == 0 { furnace } else { stone }));
        facts.item = item("minecraft:flint_and_steel");
        let there = r#"{"location":{"type":"minecraft:all_of","terms":[{"type":"minecraft:match_block","blocks":"minecraft:furnace"},{"type":"minecraft:location_check","offsetY":-1,"predicate":{"block":{"blocks":"minecraft:stone"}}},{"type":"minecraft:match_tool","predicate":{"items":"minecraft:flint_and_steel"}}]}}"#;
        assert!(matches(Some(there), &facts));
        assert!(!matches(
            Some(&there.replace("\"minecraft:stone\"", "\"minecraft:dirt\"")),
            &facts
        ));
        let either = r#"{"location":{"type":"minecraft:any_of","terms":[{"type":"minecraft:match_block","blocks":"minecraft:dirt"},{"type":"minecraft:inverted","term":{"type":"minecraft:match_block","blocks":"minecraft:dirt"}}]}}"#;
        assert!(matches(Some(either), &facts));
    }

    #[test]
    fn a_block_condition_matches_the_block_at_the_event() {
        let facts = Facts::with_blocks(|_, _, _| Some(BlockKind::Water.default_state()));
        assert!(matches(Some(r#"{"blocks":"minecraft:water"}"#), &facts));
        assert!(!matches(Some(r#"{"blocks":"minecraft:lava"}"#), &facts));
    }
}

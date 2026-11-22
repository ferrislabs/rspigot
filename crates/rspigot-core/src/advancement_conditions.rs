use rspigot_data::{registries, tags};
use rspigot_domain::BlockStateId;
use rspigot_domain::damage::DamageType;
use serde_json::{Map, Value};

#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct Distance {
    pub horizontal: f64,
    pub absolute: f64,
}

#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct EntityFacts<'a> {
    pub kind: &'a str,
    pub baby: bool,
    pub distance: Option<Distance>,
    pub vehicle: Option<&'a str>,
    pub vehicle_passenger: Option<&'a str>,
    pub passenger: Option<&'a str>,
    pub mainhand: Option<u16>,
    pub ominous_banner: bool,
    pub lightning_fire: Option<i32>,
    pub looking_at: Option<&'a str>,
}

impl<'a> EntityFacts<'a> {
    pub(crate) fn of(kind: &'a str, baby: bool) -> Self {
        Self {
            kind,
            baby,
            ..Self::default()
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct DamageFacts<'a> {
    pub kind: DamageType,
    pub dealt: f64,
    pub taken: f64,
    pub blocked: bool,
    pub direct: Option<EntityFacts<'a>>,
    pub causing: Option<EntityFacts<'a>>,
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
    pub damage: Option<DamageFacts<'a>>,
    pub victims: Vec<EntityFacts<'a>>,
    pub effects: Vec<&'a str>,
    pub weapon: Option<u16>,
    pub ingredients: Vec<u16>,
    pub player_entity: Option<EntityFacts<'a>>,
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
            damage: None,
            victims: Vec::new(),
            effects: Vec::new(),
            weapon: None,
            ingredients: Vec::new(),
            player_entity: None,
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

fn enchantment_held(entry: &Value, amounts: &[(&str, f64)]) -> bool {
    let Some(wanted) = entry.get("enchantments").and_then(names) else {
        return amounts.iter().any(|(name, level)| {
            name.starts_with("minecraft:")
                && entry
                    .get("levels")
                    .is_none_or(|range| in_range(range, *level))
        });
    };
    amounts.iter().any(|(name, level)| {
        let listed = wanted
            .iter()
            .any(|candidate| match candidate.strip_prefix('#') {
                Some(tag) => {
                    registries::synced_index("minecraft:enchantment", name).is_some_and(|id| {
                        tags::tag("minecraft:enchantment", tag)
                            .is_some_and(|members| members.binary_search(&id).is_ok())
                    })
                }
                None => candidate == name,
            });
        listed
            && entry
                .get("levels")
                .is_none_or(|range| in_range(range, *level))
    })
}

fn stack_matches(predicate: &Value, item: Option<u16>, amounts: &[(&str, f64)]) -> bool {
    let Value::Object(map) = predicate else {
        return false;
    };
    let mut plain = map.clone();
    let predicates = plain.remove("predicates");
    if !item_matches(&Value::Object(plain), item) {
        return false;
    }
    let Some(predicates) = predicates else {
        return true;
    };
    let Value::Object(predicates) = predicates else {
        return false;
    };
    predicates.iter().all(|(key, value)| match key.as_str() {
        "minecraft:enchantments" => value
            .as_array()
            .is_some_and(|entries| entries.iter().all(|entry| enchantment_held(entry, amounts))),
        _ => false,
    })
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
    match wrapper.get("type").and_then(Value::as_str) {
        Some("minecraft:inverted") => {
            return wrapper
                .get("term")
                .is_some_and(|term| !entity_matches(term, facts, place));
        }
        Some("minecraft:any_of") => {
            return wrapper
                .get("terms")
                .and_then(Value::as_array)
                .is_some_and(|terms| terms.iter().any(|term| entity_matches(term, facts, place)));
        }
        Some("minecraft:all_of") => {
            return wrapper
                .get("terms")
                .and_then(Value::as_array)
                .is_some_and(|terms| terms.iter().all(|term| entity_matches(term, facts, place)));
        }
        _ => {}
    }
    wrapper.iter().all(|(key, value)| match key.as_str() {
        "type" => value.as_str() == Some("minecraft:entity_properties"),
        "entity" => value.as_str() == Some("this"),
        "predicate" => value
            .as_object()
            .is_some_and(|properties| properties_match(properties, facts, place)),
        _ => false,
    })
}

fn properties_match(
    properties: &Map<String, Value>,
    facts: Option<EntityFacts>,
    place: Option<Place>,
) -> bool {
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
                        && facts.is_some_and(|entity| expected.as_bool() == Some(entity.baby))
                })
            }),
            "minecraft:location" => place_matches(wanted, place),
            "minecraft:distance" => distance_matches(wanted, facts.and_then(|e| e.distance)),
            "minecraft:vehicle" => wanted.as_object().is_some_and(|inner| {
                facts.and_then(|entity| entity.vehicle).is_some_and(|kind| {
                    let vehicle = EntityFacts {
                        passenger: facts.and_then(|entity| entity.vehicle_passenger),
                        ..EntityFacts::of(kind, false)
                    };
                    properties_match(inner, Some(vehicle), None)
                })
            }),
            "minecraft:passenger" => wanted.as_object().is_some_and(|inner| {
                facts
                    .and_then(|entity| entity.passenger)
                    .is_some_and(|kind| {
                        properties_match(inner, Some(EntityFacts::of(kind, false)), None)
                    })
            }),
            "minecraft:equipment" => wanted.as_object().is_some_and(|slots| {
                slots.iter().all(|(slot, stack)| match slot.as_str() {
                    "mainhand" => item_matches(stack, facts.and_then(|entity| entity.mainhand)),
                    "head" => {
                        facts.is_some_and(|entity| entity.ominous_banner)
                            && stack
                                .pointer("/components/minecraft:banner_patterns")
                                .is_some()
                    }
                    _ => false,
                })
            }),
            "minecraft:type_specific/lightning" => wanted.as_object().is_some_and(|inner| {
                inner.iter().all(|(field, expected)| {
                    field == "blocks_set_on_fire"
                        && facts
                            .and_then(|entity| entity.lightning_fire)
                            .is_some_and(|fires| expected.as_i64() == Some(i64::from(fires)))
                })
            }),
            "minecraft:type_specific/player" => wanted.as_object().is_some_and(|inner| {
                inner.iter().all(|(field, expected)| {
                    field == "looking_at"
                        && expected.as_object().is_some_and(|target| {
                            facts
                                .and_then(|entity| entity.looking_at)
                                .is_some_and(|kind| {
                                    properties_match(
                                        target,
                                        Some(EntityFacts::of(kind, false)),
                                        None,
                                    )
                                })
                        })
                })
            }),
            _ => false,
        })
}

fn distance_matches(wanted: &Value, distance: Option<Distance>) -> bool {
    let (Some(axes), Some(distance)) = (wanted.as_object(), distance) else {
        return false;
    };
    axes.iter().all(|(axis, range)| match axis.as_str() {
        "horizontal" => in_range(range, distance.horizontal),
        "absolute" => in_range(range, distance.absolute),
        _ => false,
    })
}

fn tags_hold(tags: &Value, kind: DamageType) -> bool {
    tags.as_array().is_some_and(|list| {
        list.iter().all(|entry| {
            let id = entry.get("id").and_then(Value::as_str);
            let expected = entry
                .get("expected")
                .and_then(Value::as_bool)
                .unwrap_or(true);
            id.is_some_and(|id| kind.in_tag(id.trim_start_matches('#')) == expected)
        })
    })
}

fn source_matches(wanted: &Value, damage: Option<DamageFacts>) -> bool {
    let (Some(map), Some(damage)) = (wanted.as_object(), damage) else {
        return false;
    };
    map.iter().all(|(key, value)| match key.as_str() {
        "tags" => tags_hold(value, damage.kind),
        "direct_entity" => value
            .as_object()
            .is_some_and(|inner| properties_match(inner, damage.direct, None)),
        "source_entity" => value
            .as_object()
            .is_some_and(|inner| properties_match(inner, damage.causing, None)),
        _ => false,
    })
}

fn damage_matches(wanted: &Value, damage: Option<DamageFacts>) -> bool {
    let (Some(map), Some(found)) = (wanted.as_object(), damage) else {
        return false;
    };
    map.iter().all(|(key, value)| match key.as_str() {
        "dealt" => in_range(value, found.dealt),
        "taken" => in_range(value, found.taken),
        "blocked" => value.as_bool() == Some(found.blocked),
        "type" => source_matches(value, damage),
        "source_entity" => value
            .as_object()
            .is_some_and(|inner| properties_match(inner, found.causing, None)),
        _ => false,
    })
}

fn victims_match(wanted: &Value, victims: &[EntityFacts]) -> bool {
    let Some(list) = wanted.as_array() else {
        return false;
    };
    let mut used = vec![false; victims.len()];
    list.iter().all(|predicate| {
        let found = victims
            .iter()
            .enumerate()
            .find(|(at, victim)| !used[*at] && entity_matches(predicate, Some(**victim), None));
        found.map(|(at, _)| used[at] = true).is_some()
    })
}

fn ingredients_match(wanted: &Value, items: &[u16]) -> bool {
    let Some(list) = wanted.as_array() else {
        return false;
    };
    let mut used = vec![false; items.len()];
    list.iter().all(|predicate| {
        let found = items
            .iter()
            .enumerate()
            .find(|(at, item)| !used[*at] && item_matches(predicate, Some(**item)));
        found.map(|(at, _)| used[at] = true).is_some()
    })
}

fn effects_match(wanted: &Value, active: &[&str]) -> bool {
    wanted.as_object().is_some_and(|map| {
        map.iter().all(|(name, bounds)| {
            bounds.as_object().is_some_and(Map::is_empty) && active.contains(&name.as_str())
        })
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
    if key == "start_position" {
        return value
            .pointer("/position/y")
            .is_some_and(|range| amount("start_position.y").is_some_and(|y| in_range(range, y)));
    }
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
            .is_some_and(|predicate| stack_matches(predicate, facts.item, &facts.amounts)),
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
        "item" => stack_matches(value, facts.item, &facts.amounts),
        "recipes" => facts
            .recipe
            .is_some_and(|recipe| names(value).is_some_and(|wanted| wanted.contains(&recipe))),
        "loot_tables" => facts
            .loot_table
            .is_some_and(|table| names(value).is_some_and(|wanted| wanted.contains(&table))),
        "to" => facts.to.is_some_and(|to| value.as_str() == Some(to)),
        "blocks" => block_matches(
            &Value::Object(Map::from_iter([(key.clone(), value.clone())])),
            facts
                .amounts
                .iter()
                .find(|(name, _)| *name == "block_state")
                .map(|(_, id)| BlockStateId(*id as _))
                .or_else(|| (facts.block_at)(0, 0, 0)),
        ),
        "location" => term_holds(value, facts),
        "player" => entity_matches(
            value,
            facts.player.map(|_| {
                facts
                    .player_entity
                    .unwrap_or_else(|| EntityFacts::of("minecraft:player", false))
            }),
            facts.player,
        ),
        "damage" => damage_matches(value, facts.damage),
        "killing_blow" => source_matches(value, facts.damage),
        "victims" => victims_match(value, &facts.victims),
        "effects" => effects_match(value, &facts.effects),
        "ingredients" => ingredients_match(value, &facts.ingredients),
        "fired_from_weapon" => item_matches(value, facts.weapon),
        slot if ENTITY_SLOTS.contains(&slot) => {
            let mut named = facts.entities.iter().filter(|(name, _)| *name == slot);
            let Some(first) = named.next() else {
                return entity_matches(value, None, facts.location);
            };
            std::iter::once(first)
                .chain(named)
                .any(|(_, entity)| entity_matches(value, Some(*entity), facts.location))
        }
        other => amount_matches(other, value, facts),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use rspigot_blocks::BlockKind;
    use rspigot_domain::damage::DamageType;

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
        facts
            .entities
            .push(("child", EntityFacts::of("minecraft:cow", true)));
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

    fn wrapped(properties: &str) -> String {
        format!(
            r#"{{"type":"minecraft:entity_properties","entity":"this","predicate":{properties}}}"#
        )
    }

    #[test]
    fn a_projectile_damage_condition_reads_type_tags_and_the_direct_entity() {
        let mut facts = Facts::bare();
        facts.damage = Some(DamageFacts {
            kind: DamageType::Arrow,
            dealt: 6.0,
            taken: 6.0,
            blocked: false,
            direct: Some(EntityFacts::of("minecraft:arrow", false)),
            causing: None,
        });
        let shot = r##"{"damage":{"type":{"direct_entity":{"minecraft:entity_type":"#minecraft:arrows"},"tags":[{"expected":true,"id":"#minecraft:is_projectile"}]}}}"##;
        assert!(matches(Some(shot), &facts));
        let melee = shot.replace("#minecraft:arrows", "minecraft:trident");
        assert!(!matches(Some(&melee), &facts));
        facts.damage = Some(DamageFacts {
            kind: DamageType::PlayerAttack,
            dealt: 6.0,
            taken: 6.0,
            blocked: false,
            direct: Some(EntityFacts::of("minecraft:player", false)),
            causing: None,
        });
        assert!(!matches(Some(shot), &facts));
    }

    #[test]
    fn damage_amounts_and_the_blocked_flag_are_compared() {
        let mut facts = Facts::bare();
        facts.damage = Some(DamageFacts {
            kind: DamageType::Arrow,
            dealt: 120.0,
            taken: 20.0,
            blocked: true,
            direct: None,
            causing: None,
        });
        assert!(matches(
            Some(r#"{"damage":{"dealt":{"min":100.0},"blocked":true}}"#),
            &facts
        ));
        assert!(!matches(
            Some(r#"{"damage":{"taken":{"min":100.0}}}"#),
            &facts
        ));
        assert!(!matches(Some(r#"{"damage":{"blocked":false}}"#), &facts));
    }

    #[test]
    fn a_killing_blow_checks_the_damage_source() {
        let mut facts = Facts::bare();
        facts.damage = Some(DamageFacts {
            kind: DamageType::Arrow,
            dealt: 1.0,
            taken: 1.0,
            blocked: false,
            direct: Some(EntityFacts::of("minecraft:arrow", false)),
            causing: None,
        });
        let blow =
            r##"{"killing_blow":{"tags":[{"expected":true,"id":"#minecraft:is_projectile"}]}}"##;
        assert!(matches(Some(blow), &facts));
        facts.damage = Some(DamageFacts {
            kind: DamageType::PlayerAttack,
            dealt: 1.0,
            taken: 1.0,
            blocked: false,
            direct: None,
            causing: None,
        });
        assert!(!matches(Some(blow), &facts));
    }

    #[test]
    fn an_entity_distance_vehicle_and_lightning_are_compared() {
        let mut facts = Facts::bare();
        let mut skeleton = EntityFacts::of("minecraft:skeleton", false);
        skeleton.distance = Some(Distance {
            horizontal: 60.0,
            absolute: 61.0,
        });
        facts.entities.push(("entity", skeleton));
        let far = format!(
            r#"{{"entity":{}}}"#,
            wrapped(
                r#"{"minecraft:distance":{"horizontal":{"min":50.0}},"minecraft:entity_type":"minecraft:skeleton"}"#
            )
        );
        assert!(matches(Some(&far), &facts));
        assert!(!matches(Some(&far.replace("50.0", "70.0")), &facts));
        let mut player = EntityFacts::of("minecraft:player", false);
        player.vehicle = Some("minecraft:strider");
        facts.player_entity = Some(player);
        facts.player = Some(Place {
            dimension: "minecraft:overworld",
            biome: None,
            y: 64.0,
        });
        let riding = format!(
            r#"{{"player":{}}}"#,
            wrapped(r#"{"minecraft:vehicle":{"minecraft:entity_type":"minecraft:strider"}}"#)
        );
        assert!(matches(Some(&riding), &facts));
        assert!(!matches(Some(&riding.replace("strider", "pig")), &facts));
        let mut bolt = EntityFacts::of("minecraft:lightning_bolt", false);
        bolt.lightning_fire = Some(0);
        facts.entities.push(("lightning", bolt));
        let quiet = format!(
            r#"{{"lightning":{}}}"#,
            wrapped(r#"{"minecraft:type_specific/lightning":{"blocks_set_on_fire":0}}"#)
        );
        assert!(matches(Some(&quiet), &facts));
        assert!(!matches(Some(&quiet.replace(":0}", ":2}")), &facts));
    }

    #[test]
    fn victims_need_one_distinct_entity_per_predicate() {
        let mut facts = Facts::bare();
        facts.victims = vec![EntityFacts::of("minecraft:phantom", false)];
        let two = format!(
            r#"{{"victims":[{0},{0}]}}"#,
            wrapped(r#"{"minecraft:entity_type":"minecraft:phantom"}"#)
        );
        assert!(!matches(Some(&two), &facts));
        facts
            .victims
            .push(EntityFacts::of("minecraft:phantom", false));
        assert!(matches(Some(&two), &facts));
    }

    #[test]
    fn effects_must_all_be_active() {
        let mut facts = Facts::bare();
        facts.effects = vec!["minecraft:speed", "minecraft:poison"];
        assert!(matches(
            Some(r#"{"effects":{"minecraft:speed":{},"minecraft:poison":{}}}"#),
            &facts
        ));
        assert!(!matches(
            Some(r#"{"effects":{"minecraft:speed":{},"minecraft:weakness":{}}}"#),
            &facts
        ));
    }

    #[test]
    fn a_weapon_and_a_start_height_are_compared() {
        let mut facts = Facts::bare();
        facts.weapon = item("minecraft:crossbow");
        facts.amounts.push(("start_position.y", 330.0));
        assert!(matches(
            Some(r#"{"fired_from_weapon":{"items":"minecraft:crossbow"}}"#),
            &facts
        ));
        assert!(!matches(
            Some(r#"{"fired_from_weapon":{"items":"minecraft:bow"}}"#),
            &facts
        ));
        assert!(matches(
            Some(r#"{"start_position":{"position":{"y":{"min":319.0}}}}"#),
            &facts
        ));
        assert!(!matches(
            Some(r#"{"start_position":{"position":{"y":{"min":400.0}}}}"#),
            &facts
        ));
    }
}

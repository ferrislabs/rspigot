use rspigot_data::loot::{
    BlockPredicate, Condition, DamageSourcePredicate, EnchantmentPredicate, EntityFlags,
    EntityPredicate, EntityTarget, FloatProvider, IntProvider, IntRange, ItemPredicate,
    LevelBasedValue, NumberProvider, TagPredicate,
};
use rspigot_data::registries;
use rspigot_data::tags::HolderSet;
use rspigot_domain::Identifier;
use serde_json::{Map, Value};

const BLOCK: &str = "minecraft:block";
const ITEM: &str = "minecraft:item";
const ENTITY_TYPE: &str = "minecraft:entity_type";

#[derive(Debug, thiserror::Error)]
pub enum DataError {
    #[error("{0}")]
    Invalid(String),
    #[error("unsupported: {0}")]
    Unsupported(String),
}

pub(super) type Parsed<T> = Result<T, DataError>;

pub(super) fn invalid<T>(message: impl Into<String>) -> Parsed<T> {
    Err(DataError::Invalid(message.into()))
}

pub(super) fn unsupported<T>(message: impl Into<String>) -> Parsed<T> {
    Err(DataError::Unsupported(message.into()))
}

pub(super) fn leak<T>(value: T) -> &'static T {
    Box::leak(Box::new(value))
}

pub(super) fn leak_slice<T>(values: Vec<T>) -> &'static [T] {
    Vec::leak(values)
}

pub(super) fn leak_text(text: String) -> &'static str {
    text.leak()
}

pub(super) fn object(value: &Value) -> Parsed<&Map<String, Value>> {
    match value.as_object() {
        Some(map) => Ok(map),
        None => invalid(format!("expected an object, found {value}")),
    }
}

pub(super) fn only_keys(map: &Map<String, Value>, known: &[&str], what: &str) -> Parsed<()> {
    match map.keys().find(|key| !known.contains(&key.as_str())) {
        Some(key) => unsupported(format!("{what} key \"{key}\"")),
        None => Ok(()),
    }
}

pub(super) fn name(value: &Value) -> Parsed<String> {
    let Some(text) = value.as_str() else {
        return invalid(format!("expected a resource name, found {value}"));
    };
    match Identifier::parse(text) {
        Ok(id) => Ok(id.to_string()),
        Err(_) => invalid(format!("invalid resource name {text}")),
    }
}

fn string_field<'a>(map: &'a Map<String, Value>, key: &str) -> Parsed<&'a Value> {
    match map.get(key) {
        Some(value) => Ok(value),
        None => invalid(format!("missing \"{key}\"")),
    }
}

pub(super) fn required<'a>(map: &'a Map<String, Value>, key: &str) -> Parsed<&'a Value> {
    string_field(map, key)
}

fn number(value: &Value) -> Parsed<f64> {
    match value.as_f64() {
        Some(number) => Ok(number),
        None => invalid(format!("expected a number, found {value}")),
    }
}

fn whole(value: &Value) -> Parsed<i32> {
    match value.as_i64().and_then(|n| i32::try_from(n).ok()) {
        Some(number) => Ok(number),
        None => invalid(format!("expected an integer, found {value}")),
    }
}

fn flag(value: &Value) -> Parsed<bool> {
    match value.as_bool() {
        Some(flag) => Ok(flag),
        None => invalid(format!("expected a boolean, found {value}")),
    }
}

pub(super) fn optional_flag(map: &Map<String, Value>, key: &str) -> Parsed<Option<bool>> {
    map.get(key).map(flag).transpose()
}

pub(super) fn registry_id(registry: &str, text: &str) -> Parsed<u32> {
    match registries::static_index(registry, text) {
        Some(id) => Ok(id),
        None => invalid(format!("unknown {registry} entry {text}")),
    }
}

pub(super) fn item_id(value: &Value) -> Parsed<u16> {
    let text = name(value)?;
    let id = registry_id(ITEM, &text)?;
    u16::try_from(id).or_else(|_| invalid("item id out of range"))
}

pub(super) fn holder_set(registry: &str, value: &Value) -> Parsed<HolderSet> {
    match value {
        Value::String(text) => match text.strip_prefix('#') {
            Some(tag) => Ok(HolderSet::Tag(leak_text(name(&Value::from(tag))?))),
            None => {
                let id = registry_id(registry, &name(value)?)?;
                Ok(HolderSet::Direct(leak_slice(vec![id])))
            }
        },
        Value::Array(items) => {
            let mut ids = Vec::with_capacity(items.len());
            for item in items {
                ids.push(registry_id(registry, &name(item)?)?);
            }
            ids.sort_unstable();
            ids.dedup();
            Ok(HolderSet::Direct(leak_slice(ids)))
        }
        other => invalid(format!("expected a holder set, found {other}")),
    }
}

pub(super) fn int_provider(value: &Value) -> Parsed<IntProvider> {
    match value {
        Value::Number(_) => Ok(IntProvider::Constant(whole(value)?)),
        Value::Object(map) => {
            let kind =
                map.get("type")
                    .and_then(Value::as_str)
                    .unwrap_or(if map.contains_key("min") {
                        "minecraft:uniform"
                    } else {
                        ""
                    });
            match kind {
                "minecraft:constant" | "constant" => {
                    Ok(IntProvider::Constant(whole(required(map, "value")?)?))
                }
                "minecraft:uniform" | "uniform" => Ok(IntProvider::Uniform {
                    min: whole(required(map, "min")?)?,
                    max: whole(required(map, "max")?)?,
                }),
                "minecraft:binomial" | "binomial" => Ok(IntProvider::Binomial {
                    n: whole(required(map, "n")?)?,
                    p: number(required(map, "p")?)? as f32,
                }),
                other => unsupported(format!("number provider \"{other}\"")),
            }
        }
        other => invalid(format!("expected a number provider, found {other}")),
    }
}

pub(super) fn float_provider(value: &Value) -> Parsed<FloatProvider> {
    match number_provider(value)? {
        NumberProvider::Constant(value) => Ok(FloatProvider::Constant(value)),
        NumberProvider::Uniform { min, max } => Ok(FloatProvider::Uniform { min, max }),
        NumberProvider::Binomial { .. } => unsupported("binomial float provider"),
    }
}

pub(super) fn number_provider(value: &Value) -> Parsed<NumberProvider> {
    match value {
        Value::Number(_) => Ok(NumberProvider::Constant(number(value)? as f32)),
        Value::Object(map) => {
            let kind =
                map.get("type")
                    .and_then(Value::as_str)
                    .unwrap_or(if map.contains_key("min") {
                        "minecraft:uniform"
                    } else {
                        ""
                    });
            match kind {
                "minecraft:constant" | "constant" => Ok(NumberProvider::Constant(number(required(
                    map, "value",
                )?)?
                    as f32)),
                "minecraft:uniform" | "uniform" => Ok(NumberProvider::Uniform {
                    min: number(required(map, "min")?)? as f32,
                    max: number(required(map, "max")?)? as f32,
                }),
                "minecraft:binomial" | "binomial" => Ok(NumberProvider::Binomial {
                    n: whole(required(map, "n")?)?,
                    p: number(required(map, "p")?)? as f32,
                }),
                other => unsupported(format!("number provider \"{other}\"")),
            }
        }
        other => invalid(format!("expected a number provider, found {other}")),
    }
}

pub(super) fn int_range(value: &Value) -> Parsed<IntRange> {
    match value {
        Value::Number(_) => {
            let exact = whole(value)?;
            Ok(IntRange {
                min: Some(exact),
                max: Some(exact),
            })
        }
        Value::Object(map) => Ok(IntRange {
            min: map.get("min").map(whole).transpose()?,
            max: map.get("max").map(whole).transpose()?,
        }),
        other => invalid(format!("expected a range, found {other}")),
    }
}

fn float_bounds(value: &Value) -> Parsed<(Option<f32>, Option<f32>)> {
    match value {
        Value::Number(_) => {
            let exact = number(value)? as f32;
            Ok((Some(exact), Some(exact)))
        }
        Value::Object(map) => Ok((
            map.get("min")
                .map(|v| number(v).map(|n| n as f32))
                .transpose()?,
            map.get("max")
                .map(|v| number(v).map(|n| n as f32))
                .transpose()?,
        )),
        other => invalid(format!("expected a range, found {other}")),
    }
}

fn level_based(value: &Value) -> Parsed<LevelBasedValue> {
    match value {
        Value::Number(_) => Ok(LevelBasedValue::Constant(number(value)? as f32)),
        Value::Object(map) => match map.get("type").and_then(Value::as_str) {
            Some("minecraft:linear" | "linear") => Ok(LevelBasedValue::Linear {
                base: number(required(map, "base")?)? as f32,
                per_level_above_first: number(required(map, "per_level_above_first")?)? as f32,
            }),
            Some("minecraft:constant" | "constant") => Ok(LevelBasedValue::Constant(number(
                required(map, "value")?,
            )?
                as f32)),
            Some("minecraft:fraction" | "fraction") => Ok(LevelBasedValue::Fraction {
                numerator: leak(level_based(required(map, "numerator")?)?),
                denominator: leak(level_based(required(map, "denominator")?)?),
            }),
            other => unsupported(format!("level based value {other:?}")),
        },
        other => invalid(format!("expected a level based value, found {other}")),
    }
}

fn state_pairs(value: &Value) -> Parsed<&'static [(&'static str, &'static str)]> {
    let mut pairs = Vec::new();
    for (key, wanted) in object(value)? {
        let text = match wanted {
            Value::String(text) => text.clone(),
            Value::Bool(flag) => flag.to_string(),
            Value::Number(number) => number.to_string(),
            _ => return unsupported(format!("block state range for \"{key}\"")),
        };
        pairs.push((leak_text(key.clone()), leak_text(text)));
    }
    Ok(leak_slice(pairs))
}

fn block_predicate(value: &Value) -> Parsed<BlockPredicate> {
    let map = object(value)?;
    only_keys(map, &["blocks", "state"], "block predicate")?;
    Ok(BlockPredicate {
        blocks: map
            .get("blocks")
            .map(|blocks| holder_set(BLOCK, blocks))
            .transpose()?,
        state: map.get("state").map_or(Ok(&[][..]), state_pairs)?,
    })
}

fn enchantment_predicate(value: &Value) -> Parsed<EnchantmentPredicate> {
    let map = object(value)?;
    only_keys(map, &["enchantments", "levels"], "enchantment predicate")?;
    let names = match map.get("enchantments") {
        None => None,
        Some(Value::String(text)) if text.starts_with('#') => {
            return unsupported("enchantment tags in an enchantment predicate");
        }
        Some(Value::String(_)) => Some(vec![leak_text(name(&map["enchantments"])?)]),
        Some(Value::Array(items)) => {
            let mut names = Vec::new();
            for item in items {
                names.push(leak_text(name(item)?));
            }
            Some(names)
        }
        Some(other) => return invalid(format!("bad enchantments {other}")),
    };
    Ok(EnchantmentPredicate {
        enchantments: names.map(leak_slice),
        levels: map.get("levels").map(int_range).transpose()?,
    })
}

fn item_predicate(value: &Value) -> Parsed<ItemPredicate> {
    let map = object(value)?;
    only_keys(map, &["items", "predicates"], "item predicate")?;
    let mut enchantments = Vec::new();
    if let Some(predicates) = map.get("predicates") {
        let predicates = object(predicates)?;
        only_keys(
            predicates,
            &["minecraft:enchantments"],
            "item sub-predicate",
        )?;
        if let Some(Value::Array(list)) = predicates.get("minecraft:enchantments") {
            for entry in list {
                enchantments.push(enchantment_predicate(entry)?);
            }
        }
    }
    Ok(ItemPredicate {
        items: map
            .get("items")
            .map(|items| holder_set(ITEM, items))
            .transpose()?,
        enchantments: leak_slice(enchantments),
    })
}

fn entity_predicate(value: &Value) -> Parsed<EntityPredicate> {
    let map = object(value)?;
    only_keys(
        map,
        &["type", "flags", "equipment", "vehicle"],
        "entity predicate",
    )?;
    let mut flags = EntityFlags {
        baby: None,
        on_fire: None,
    };
    if let Some(found) = map.get("flags") {
        let found = object(found)?;
        only_keys(found, &["is_baby", "is_on_fire"], "entity flag")?;
        flags.baby = optional_flag(found, "is_baby")?;
        flags.on_fire = optional_flag(found, "is_on_fire")?;
    }
    let mainhand = match map.get("equipment") {
        Some(equipment) => {
            let equipment = object(equipment)?;
            only_keys(equipment, &["mainhand"], "equipment slot")?;
            equipment.get("mainhand").map(item_predicate).transpose()?
        }
        None => None,
    };
    Ok(EntityPredicate {
        kind: map
            .get("type")
            .map(|kind| holder_set(ENTITY_TYPE, kind))
            .transpose()?,
        flags,
        mainhand,
        vehicle: map
            .get("vehicle")
            .map(|vehicle| entity_predicate(vehicle).map(leak))
            .transpose()?,
        components: &[],
        sub: &[],
    })
}

fn target(value: &Value) -> Parsed<EntityTarget> {
    match value.as_str() {
        Some("this") => Ok(EntityTarget::This),
        Some("attacker") => Ok(EntityTarget::Attacker),
        Some("direct_attacker") => Ok(EntityTarget::DirectAttacker),
        Some("attacking_player") => Ok(EntityTarget::AttackingPlayer),
        Some("target_entity") => Ok(EntityTarget::TargetEntity),
        Some("interacting_entity") => Ok(EntityTarget::InteractingEntity),
        _ => invalid(format!("unknown entity target {value}")),
    }
}

fn damage_predicate(value: &Value) -> Parsed<DamageSourcePredicate> {
    let map = object(value)?;
    only_keys(
        map,
        &["tags", "source_entity", "direct_entity", "is_direct"],
        "damage source predicate",
    )?;
    let mut tags = Vec::new();
    if let Some(Value::Array(list)) = map.get("tags") {
        for entry in list {
            let entry = object(entry)?;
            tags.push(TagPredicate {
                tag: leak_text(name(required(entry, "id")?)?),
                expected: flag(required(entry, "expected")?)?,
            });
        }
    }
    Ok(DamageSourcePredicate {
        tags: leak_slice(tags),
        source_entity: map
            .get("source_entity")
            .map(|entity| entity_predicate(entity).map(leak))
            .transpose()?,
        direct_entity: map
            .get("direct_entity")
            .map(|entity| entity_predicate(entity).map(leak))
            .transpose()?,
        is_direct: optional_flag(map, "is_direct")?,
    })
}

fn terms(map: &Map<String, Value>) -> Parsed<&'static [Condition]> {
    let Some(Value::Array(list)) = map.get("terms") else {
        return invalid("missing \"terms\"");
    };
    let mut parsed = Vec::with_capacity(list.len());
    for term in list {
        parsed.push(condition(term)?);
    }
    Ok(leak_slice(parsed))
}

pub fn condition(value: &Value) -> Parsed<Condition> {
    let map = object(value)?;
    let kind = match map.get("condition").and_then(Value::as_str) {
        Some(kind) => name(&Value::from(kind))?,
        None => return invalid("missing \"condition\""),
    };
    match kind.as_str() {
        "minecraft:inverted" => Ok(Condition::Inverted(leak(condition(required(
            map, "term",
        )?)?))),
        "minecraft:all_of" => Ok(Condition::AllOf(terms(map)?)),
        "minecraft:any_of" => Ok(Condition::AnyOf(terms(map)?)),
        "minecraft:killed_by_player" => Ok(Condition::KilledByPlayer),
        "minecraft:survives_explosion" => Ok(Condition::SurvivesExplosion),
        "minecraft:random_chance" => match number_provider(required(map, "chance")?)? {
            NumberProvider::Constant(chance) => Ok(Condition::RandomChance(chance)),
            _ => unsupported("random_chance with a non constant chance"),
        },
        "minecraft:random_chance_with_enchanted_bonus" => {
            Ok(Condition::RandomChanceWithEnchantedBonus {
                enchantment: leak_text(name(required(map, "enchantment")?)?),
                unenchanted_chance: match number_provider(required(map, "unenchanted_chance")?)? {
                    NumberProvider::Constant(chance) => chance,
                    _ => return unsupported("non constant unenchanted_chance"),
                },
                enchanted_chance: level_based(required(map, "enchanted_chance")?)?,
            })
        }
        "minecraft:table_bonus" => {
            let Some(Value::Array(list)) = map.get("chances") else {
                return invalid("missing \"chances\"");
            };
            let mut chances = Vec::with_capacity(list.len());
            for chance in list {
                chances.push(number(chance)? as f32);
            }
            Ok(Condition::TableBonus {
                enchantment: leak_text(name(required(map, "enchantment")?)?),
                chances: leak_slice(chances),
            })
        }
        "minecraft:block_state_property" => {
            let block = registry_id(BLOCK, &name(required(map, "block")?)?)?;
            Ok(Condition::MatchBlock(BlockPredicate {
                blocks: Some(HolderSet::Direct(leak_slice(vec![block]))),
                state: map.get("properties").map_or(Ok(&[][..]), state_pairs)?,
            }))
        }
        "minecraft:match_tool" => Ok(Condition::MatchTool(item_predicate(required(
            map,
            "predicate",
        )?)?)),
        "minecraft:location_check" => {
            only_keys(
                map,
                &["condition", "offsetX", "offsetY", "offsetZ", "predicate"],
                "location_check",
            )?;
            let offset = |key: &str| map.get(key).map_or(Ok(0), whole);
            let mut block = None;
            let mut biomes: &'static [&'static str] = &[];
            if let Some(predicate) = map.get("predicate") {
                let predicate = object(predicate)?;
                only_keys(predicate, &["block", "biomes"], "location predicate")?;
                block = predicate.get("block").map(block_predicate).transpose()?;
                if let Some(wanted) = predicate.get("biomes") {
                    biomes = match wanted {
                        Value::String(text) if text.starts_with('#') => {
                            return unsupported("biome tags in a location predicate");
                        }
                        Value::String(_) => leak_slice(vec![leak_text(name(wanted)?)]),
                        Value::Array(items) => {
                            let mut names = Vec::new();
                            for item in items {
                                names.push(leak_text(name(item)?));
                            }
                            leak_slice(names)
                        }
                        other => return invalid(format!("bad biomes {other}")),
                    };
                }
            }
            Ok(Condition::LocationCheck {
                offset: [offset("offsetX")?, offset("offsetY")?, offset("offsetZ")?],
                block,
                biomes,
            })
        }
        "minecraft:entity_properties" => Ok(Condition::EntityProperties {
            target: target(required(map, "entity")?)?,
            predicate: map
                .get("predicate")
                .map(|predicate| entity_predicate(predicate).map(leak))
                .transpose()?,
        }),
        "minecraft:damage_source_properties" => Ok(Condition::DamageSourceProperties(leak(
            damage_predicate(required(map, "predicate")?)?,
        ))),
        "minecraft:reference" => Ok(Condition::Reference(leak_text(name(required(
            map, "name",
        )?)?))),
        "minecraft:time_check" => {
            let (min, max) = float_bounds(required(map, "value")?)?;
            Ok(Condition::TimeCheck {
                period: map
                    .get("period")
                    .map(|period| number(period).map(|n| n as i64))
                    .transpose()?,
                min,
                max,
            })
        }
        "minecraft:weather_check" => Ok(Condition::WeatherCheck {
            raining: optional_flag(map, "raining")?,
            thundering: optional_flag(map, "thundering")?,
        }),
        "minecraft:value_check" => {
            let (min, max) = match required(map, "range")? {
                Value::Number(_) => {
                    let exact = number_provider(&map["range"])?;
                    (Some(exact), Some(exact))
                }
                Value::Object(range) => (
                    range.get("min").map(number_provider).transpose()?,
                    range.get("max").map(number_provider).transpose()?,
                ),
                other => return invalid(format!("bad range {other}")),
            };
            Ok(Condition::ValueCheck {
                value: number_provider(required(map, "value")?)?,
                min,
                max,
            })
        }
        "minecraft:entity_scores" => {
            let mut scores = Vec::new();
            for (objective, range) in object(required(map, "scores")?)? {
                scores.push((leak_text(objective.clone()), int_range(range)?));
            }
            Ok(Condition::EntityScores {
                target: target(required(map, "entity")?)?,
                scores: leak_slice(scores),
            })
        }
        other => unsupported(format!("condition {other}")),
    }
}

pub fn predicate_file(value: &Value) -> Parsed<Condition> {
    match value {
        Value::Array(list) => {
            let mut parsed = Vec::with_capacity(list.len());
            for item in list {
                parsed.push(condition(item)?);
            }
            Ok(Condition::AllOf(leak_slice(parsed)))
        }
        other => condition(other),
    }
}

pub(super) fn conditions(map: &Map<String, Value>) -> Parsed<Option<Condition>> {
    let Some(Value::Array(list)) = map.get("conditions") else {
        return Ok(None);
    };
    let mut parsed = Vec::with_capacity(list.len());
    for item in list {
        parsed.push(condition(item)?);
    }
    Ok(match parsed.len() {
        0 => None,
        1 => parsed.pop(),
        _ => Some(Condition::AllOf(leak_slice(parsed))),
    })
}

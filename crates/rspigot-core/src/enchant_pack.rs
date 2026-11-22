use std::cell::RefCell;
use std::sync::Arc;

use rspigot_data::enchantments::{
    AttributeEffect, BlockCondition, ChargingSounds, ConditionalEffect, Cost, EffectRole,
    EnchantmentDef, EnchantmentEffects, EnchantmentOverlay, EntityCheck, EntityEffect, Immunity,
    LevelValue, Requirement, TargetedEffect, ValueEffect, ValueOp,
};
use rspigot_data::loot::{Condition, EntityTarget, LevelBasedValue};
use serde_json::{Map, Value};

use crate::pack_data::registry_overlay::RegistryOverlay;

type Parsed<T> = Result<T, String>;

const ENCHANTMENT_REGISTRY: &str = "minecraft:enchantment";

fn leak<T>(value: T) -> &'static T {
    Box::leak(Box::new(value))
}

fn leak_slice<T>(values: Vec<T>) -> &'static [T] {
    Vec::leak(values)
}

fn leak_text(text: &str) -> &'static str {
    text.to_owned().leak()
}

fn object(value: &Value) -> Parsed<&Map<String, Value>> {
    value
        .as_object()
        .ok_or_else(|| format!("expected an object, found {value}"))
}

fn number(value: &Value) -> Parsed<f32> {
    value
        .as_f64()
        .map(|number| number as f32)
        .ok_or_else(|| format!("expected a number, found {value}"))
}

fn integer(value: &Value) -> Parsed<i32> {
    value
        .as_i64()
        .and_then(|number| i32::try_from(number).ok())
        .ok_or_else(|| format!("expected an integer, found {value}"))
}

fn text(value: &Value) -> Parsed<&str> {
    value
        .as_str()
        .ok_or_else(|| format!("expected a string, found {value}"))
}

fn normalized(name: &str) -> String {
    if name.contains(':') {
        name.to_owned()
    } else {
        format!("minecraft:{name}")
    }
}

fn one_or_many(value: &Value) -> Parsed<Vec<&str>> {
    match value {
        Value::String(single) => Ok(vec![single.as_str()]),
        Value::Array(list) => list.iter().map(text).collect(),
        other => Err(format!("expected a name or a list, found {other}")),
    }
}

fn level_based(value: &Value) -> Parsed<LevelBasedValue> {
    if let Some(constant) = value.as_f64() {
        return Ok(LevelBasedValue::Constant(constant as f32));
    }
    let map = object(value)?;
    match normalized(text(map.get("type").unwrap_or(&Value::Null))?).as_str() {
        "minecraft:linear" => Ok(LevelBasedValue::Linear {
            base: number(map.get("base").unwrap_or(&Value::Null))?,
            per_level_above_first: number(
                map.get("per_level_above_first").unwrap_or(&Value::Null),
            )?,
        }),
        "minecraft:fraction" => Ok(LevelBasedValue::Fraction {
            numerator: leak(level_based(map.get("numerator").unwrap_or(&Value::Null))?),
            denominator: leak(level_based(map.get("denominator").unwrap_or(&Value::Null))?),
        }),
        other => Err(format!("unsupported level based value {other}")),
    }
}

fn level_value(value: &Value) -> Parsed<LevelValue> {
    let Some(map) = value.as_object() else {
        return Ok(LevelValue::Plain(level_based(value)?));
    };
    match map
        .get("type")
        .and_then(Value::as_str)
        .map(normalized)
        .as_deref()
    {
        Some("minecraft:lookup") => Ok(LevelValue::Lookup {
            values: leak_slice(
                map.get("values")
                    .and_then(Value::as_array)
                    .ok_or("lookup without values")?
                    .iter()
                    .map(number)
                    .collect::<Parsed<Vec<f32>>>()?,
            ),
            fallback: level_based(map.get("fallback").unwrap_or(&Value::Null))?,
        }),
        Some("minecraft:clamped") => Ok(LevelValue::Clamped {
            value: leak(level_value(map.get("value").unwrap_or(&Value::Null))?),
            min: number(map.get("min").unwrap_or(&Value::Null))?,
            max: number(map.get("max").unwrap_or(&Value::Null))?,
        }),
        _ => Ok(LevelValue::Plain(level_based(value)?)),
    }
}

fn value_op(value: &Value) -> Parsed<ValueOp> {
    let map = object(value)?;
    let kind = normalized(text(map.get("type").unwrap_or(&Value::Null))?);
    let field = |key: &str| level_based(map.get(key).unwrap_or(&Value::Null));
    match kind.as_str() {
        "minecraft:add" => Ok(ValueOp::Add(field("value")?)),
        "minecraft:multiply" => Ok(ValueOp::Multiply(field("factor")?)),
        "minecraft:set" => Ok(ValueOp::Set(field("value")?)),
        "minecraft:remove_binomial" => Ok(ValueOp::RemoveBinomial(field("chance")?)),
        other => Err(format!("unsupported value effect {other}")),
    }
}

fn loot_condition(value: &Value) -> Parsed<Condition> {
    fn renamed(value: &Value) -> Value {
        let Some(map) = value.as_object() else {
            return value.clone();
        };
        let mut out = Map::new();
        for (key, inner) in map {
            let key = if key == "type" && map.get("type").is_some_and(Value::is_string) {
                "condition"
            } else {
                key.as_str()
            };
            let inner = match key {
                "terms" => Value::Array(
                    inner
                        .as_array()
                        .map(|list| list.iter().map(renamed).collect())
                        .unwrap_or_default(),
                ),
                "term" => renamed(inner),
                _ => inner.clone(),
            };
            out.insert(key.to_owned(), inner);
        }
        Value::Object(out)
    }
    crate::pack_data::condition(&renamed(value)).map_err(|error| error.to_string())
}

fn value_effect(value: &Value) -> Parsed<ValueEffect> {
    let map = object(value)?;
    Ok(ValueEffect {
        op: value_op(map.get("effect").ok_or("missing effect")?)?,
        requirements: map.get("requirements").map(loot_condition).transpose()?,
    })
}

fn value_list(value: &Value) -> Parsed<&'static [ValueEffect]> {
    let list = value.as_array().ok_or("expected a list")?;
    Ok(leak_slice(
        list.iter().map(value_effect).collect::<Parsed<_>>()?,
    ))
}

fn entity_target(name: &str) -> Parsed<EntityTarget> {
    Ok(match name {
        "this" => EntityTarget::This,
        "attacker" => EntityTarget::Attacker,
        "direct_attacker" => EntityTarget::DirectAttacker,
        "attacking_player" => EntityTarget::AttackingPlayer,
        "target_entity" => EntityTarget::TargetEntity,
        "interacting_entity" => EntityTarget::InteractingEntity,
        other => return Err(format!("unknown entity target {other}")),
    })
}

fn entity_checks(predicate: &Map<String, Value>) -> Parsed<Vec<EntityCheck>> {
    let mut checks = Vec::new();
    for (key, value) in predicate {
        match key.as_str() {
            "minecraft:flags" => {
                for (flag, expected) in object(value)? {
                    let expected = expected.as_bool().ok_or("flag without a boolean")?;
                    checks.push(match flag.as_str() {
                        "is_on_ground" => EntityCheck::OnGround(expected),
                        "is_flying" => EntityCheck::Flying(expected),
                        "is_fall_flying" => EntityCheck::FallFlying(expected),
                        "is_in_water" => EntityCheck::InWater(expected),
                        other => return Err(format!("unsupported entity flag {other}")),
                    });
                }
            }
            "minecraft:vehicle" => checks.push(EntityCheck::Vehicle(true)),
            "minecraft:movement" => {
                for (field, bounds) in object(value)? {
                    let min = number(object(bounds)?.get("min").unwrap_or(&Value::Null))?;
                    checks.push(match field.as_str() {
                        "horizontal_speed" => EntityCheck::HorizontalSpeedMin(min),
                        "fall_distance" => EntityCheck::FallDistanceMin(min),
                        other => return Err(format!("unsupported movement {other}")),
                    });
                }
            }
            "minecraft:movement_affected_by" => {
                let blocks = text(
                    object(object(value)?.get("block").unwrap_or(&Value::Null))?
                        .get("blocks")
                        .unwrap_or(&Value::Null),
                )?;
                let tag = blocks
                    .strip_prefix('#')
                    .ok_or("movement_affected_by needs a tag")?;
                checks.push(EntityCheck::MovementAffectedBy(leak_text(&normalized(tag))));
            }
            "minecraft:periodic_tick" => checks.push(EntityCheck::PeriodicTick(integer(value)?)),
            "minecraft:location" => {
                let sky = object(value)?
                    .get("can_see_sky")
                    .and_then(Value::as_bool)
                    .ok_or("location without can_see_sky")?;
                checks.push(EntityCheck::CanSeeSky(sky));
            }
            "minecraft:type_specific/player" => {
                checks.push(EntityCheck::IsPlayer(true));
                let map = object(value)?;
                if let Some(modes) = map.get("gamemode") {
                    let modes: Vec<&'static str> = modes
                        .as_array()
                        .ok_or("gamemode without a list")?
                        .iter()
                        .map(|mode| text(mode).map(leak_text))
                        .collect::<Parsed<_>>()?;
                    checks.push(EntityCheck::Gamemode(leak_slice(modes)));
                }
                if let Some(food) = map.get("food") {
                    let min = integer(
                        object(object(food)?.get("level").unwrap_or(&Value::Null))?
                            .get("min")
                            .unwrap_or(&Value::Null),
                    )?;
                    checks.push(EntityCheck::FoodMin(min));
                }
            }
            "minecraft:entity_type" => {
                let name = text(value)?;
                if name.starts_with('#') {
                    return Err("entity type tags need the loot path".to_owned());
                }
                checks.push(EntityCheck::EntityType(leak_text(&normalized(name))));
            }
            other => return Err(format!("unsupported entity predicate {other}")),
        }
    }
    Ok(checks)
}

fn requirement(value: &Value) -> Parsed<Requirement> {
    let map = object(value)?;
    let kind = normalized(text(
        map.get("type")
            .or(map.get("condition"))
            .unwrap_or(&Value::Null),
    )?);
    let terms = |key: &str| -> Parsed<&'static [Requirement]> {
        let list = map
            .get(key)
            .and_then(Value::as_array)
            .ok_or("missing terms")?;
        Ok(leak_slice(
            list.iter().map(requirement).collect::<Parsed<_>>()?,
        ))
    };
    match kind.as_str() {
        "minecraft:all_of" => Ok(Requirement::AllOf(terms("terms")?)),
        "minecraft:any_of" => Ok(Requirement::AnyOf(terms("terms")?)),
        "minecraft:inverted" => Ok(Requirement::Not(leak(requirement(
            map.get("term").ok_or("missing term")?,
        )?))),
        "minecraft:enchantment_active_check" => Ok(Requirement::ActiveCheck(
            map.get("active")
                .and_then(Value::as_bool)
                .ok_or("missing active")?,
        )),
        "minecraft:weather_check" => Ok(Requirement::Loot(Condition::WeatherCheck {
            raining: map.get("raining").and_then(Value::as_bool),
            thundering: map.get("thundering").and_then(Value::as_bool),
        })),
        "minecraft:location_check" => {
            let predicate = object(map.get("predicate").ok_or("missing predicate")?)?;
            let block_tag = match predicate.get("block") {
                Some(block) => {
                    let blocks = text(object(block)?.get("blocks").unwrap_or(&Value::Null))?;
                    let tag = blocks
                        .strip_prefix('#')
                        .ok_or("location block needs a tag")?;
                    Some(leak_text(&normalized(tag)))
                }
                None => None,
            };
            Ok(Requirement::Location {
                block_tag,
                can_see_sky: predicate.get("can_see_sky").and_then(Value::as_bool),
            })
        }
        "minecraft:entity_properties" => {
            let target = entity_target(text(map.get("entity").unwrap_or(&Value::Null))?)?;
            let predicate = match map.get("predicate") {
                Some(predicate) => object(predicate)?.clone(),
                None => Map::new(),
            };
            let loot_only = predicate.keys().all(|key| {
                matches!(
                    key.as_str(),
                    "minecraft:entity_type" | "minecraft:equipment" | "minecraft:components"
                ) || key.starts_with("minecraft:type_specific/")
                    && key != "minecraft:type_specific/player"
            }) && predicate.get("minecraft:flags").is_none_or(|flags| {
                flags
                    .as_object()
                    .is_some_and(|flags| flags.keys().all(|k| k == "baby" || k == "on_fire"))
            });
            let exact_type = predicate.len() == 1
                && predicate
                    .get("minecraft:entity_type")
                    .and_then(Value::as_str)
                    .is_some_and(|name| !name.starts_with('#'));
            if loot_only && !exact_type {
                return Ok(Requirement::Loot(loot_condition(value)?));
            }
            let checks: Vec<Requirement> = entity_checks(&predicate)?
                .into_iter()
                .map(|check| Requirement::Entity { target, check })
                .collect();
            match checks.as_slice() {
                [single] => Ok(*single),
                _ => Ok(Requirement::AllOf(leak_slice(checks))),
            }
        }
        _ => Ok(Requirement::Loot(loot_condition(value)?)),
    }
}

fn offset(value: Option<&Value>) -> Parsed<[i32; 3]> {
    let Some(list) = value else {
        return Ok([0; 3]);
    };
    let list = list
        .as_array()
        .filter(|list| list.len() == 3)
        .ok_or("bad offset")?;
    Ok([integer(&list[0])?, integer(&list[1])?, integer(&list[2])?])
}

fn vector(value: &Value) -> Parsed<[f32; 3]> {
    let list = value
        .as_array()
        .filter(|list| list.len() == 3)
        .ok_or("bad vector")?;
    Ok([number(&list[0])?, number(&list[1])?, number(&list[2])?])
}

fn block_condition(value: &Value) -> Parsed<BlockCondition> {
    let map = object(value)?;
    let kind = normalized(text(map.get("type").unwrap_or(&Value::Null))?);
    let at = offset(map.get("offset"))?;
    match kind.as_str() {
        "minecraft:all_of" => Ok(BlockCondition::AllOf(leak_slice(
            map.get("predicates")
                .and_then(Value::as_array)
                .ok_or("missing predicates")?
                .iter()
                .map(block_condition)
                .collect::<Parsed<_>>()?,
        ))),
        "minecraft:matching_block_tag" => Ok(BlockCondition::MatchingBlockTag {
            offset: at,
            tag: leak_text(&normalized(text(map.get("tag").unwrap_or(&Value::Null))?)),
        }),
        "minecraft:matching_blocks" => Ok(BlockCondition::MatchingBlocks {
            offset: at,
            blocks: leak_slice(
                one_or_many(map.get("blocks").unwrap_or(&Value::Null))?
                    .into_iter()
                    .map(|name| leak_text(&normalized(name)))
                    .collect(),
            ),
        }),
        "minecraft:matching_fluids" => Ok(BlockCondition::MatchingFluids {
            offset: at,
            fluids: leak_slice(
                one_or_many(map.get("fluids").unwrap_or(&Value::Null))?
                    .into_iter()
                    .map(|name| leak_text(&normalized(name)))
                    .collect(),
            ),
        }),
        "minecraft:unobstructed" => Ok(BlockCondition::Unobstructed),
        other => Err(format!("unsupported block predicate {other}")),
    }
}

fn particle_name(value: &Value) -> Parsed<&'static str> {
    let map = object(value)?;
    Ok(leak_text(&normalized(text(
        map.get("type").unwrap_or(&Value::Null),
    )?)))
}

fn entity_effect(value: &Value) -> Parsed<EntityEffect> {
    let map = object(value)?;
    let kind = normalized(text(map.get("type").unwrap_or(&Value::Null))?);
    let get = |key: &str| map.get(key).unwrap_or(&Value::Null);
    let based = |key: &str| level_based(get(key));
    match kind.as_str() {
        "minecraft:all_of" => Ok(EntityEffect::AllOf(leak_slice(
            get("effects")
                .as_array()
                .ok_or("missing effects")?
                .iter()
                .map(entity_effect)
                .collect::<Parsed<_>>()?,
        ))),
        "minecraft:ignite" => Ok(EntityEffect::Ignite(based("duration")?)),
        "minecraft:change_item_damage" => Ok(EntityEffect::ChangeItemDamage(based("amount")?)),
        "minecraft:damage_entity" => Ok(EntityEffect::DamageEntity {
            damage_type: leak_text(&normalized(text(get("damage_type"))?)),
            min_damage: based("min_damage")?,
            max_damage: based("max_damage")?,
        }),
        "minecraft:apply_mob_effect" => Ok(EntityEffect::ApplyMobEffect {
            effect: leak_text(&normalized(text(get("to_apply"))?)),
            min_duration: based("min_duration")?,
            max_duration: based("max_duration")?,
            min_amplifier: map
                .get("min_amplifier")
                .map_or(Ok(LevelBasedValue::Constant(0.0)), level_based)?,
            max_amplifier: map
                .get("max_amplifier")
                .map_or(Ok(LevelBasedValue::Constant(0.0)), level_based)?,
        }),
        "minecraft:apply_impulse" => Ok(EntityEffect::ApplyImpulse {
            direction: vector(get("direction"))?,
            coordinate_scale: vector(get("coordinate_scale"))?,
            magnitude: level_value(get("magnitude"))?,
        }),
        "minecraft:apply_exhaustion" => {
            Ok(EntityEffect::ApplyExhaustion(level_value(get("amount"))?))
        }
        "minecraft:explode" => Ok(EntityEffect::Explode {
            block_interaction: leak_text(
                map.get("block_interaction")
                    .and_then(Value::as_str)
                    .unwrap_or("trigger"),
            ),
            immune_blocks: map
                .get("immune_blocks")
                .and_then(Value::as_str)
                .map(|tag| leak_text(&normalized(tag.trim_start_matches('#')))),
            knockback_multiplier: map.get("knockback_multiplier").map_or(
                Ok(LevelValue::Plain(LevelBasedValue::Constant(1.0))),
                level_value,
            )?,
            radius: level_value(get("radius"))?,
            small_particle: map.get("small_particle").map(particle_name).transpose()?,
            large_particle: map.get("large_particle").map(particle_name).transpose()?,
            sound: leak_text(&normalized(text(get("sound"))?)),
            create_fire: map
                .get("create_fire")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            attribute_to_user: map
                .get("attribute_to_user")
                .and_then(Value::as_bool)
                .unwrap_or(false),
        }),
        "minecraft:summon_entity" => Ok(EntityEffect::SummonEntity {
            entity: leak_text(&normalized(text(get("entity"))?)),
            join_team: map
                .get("join_team")
                .and_then(Value::as_bool)
                .unwrap_or(false),
        }),
        "minecraft:replace_disk" => {
            let state = object(get("block_state"))?;
            let properties: Vec<(&'static str, &'static str)> = state
                .get("properties")
                .and_then(Value::as_object)
                .map(|properties| {
                    properties
                        .iter()
                        .filter_map(|(key, value)| {
                            Some((leak_text(key), leak_text(value.as_str()?)))
                        })
                        .collect()
                })
                .unwrap_or_default();
            Ok(EntityEffect::ReplaceDisk {
                radius: level_value(get("radius"))?,
                height: level_value(get("height"))?,
                offset: offset(map.get("offset"))?,
                block: leak_text(&normalized(text(state.get("id").unwrap_or(&Value::Null))?)),
                state: leak_slice(properties),
                predicate: map.get("predicate").map(block_condition).transpose()?,
                trigger_game_event: map
                    .get("trigger_game_event")
                    .and_then(Value::as_str)
                    .map(|name| leak_text(&normalized(name))),
            })
        }
        "minecraft:spawn_particles" => Ok(EntityEffect::SpawnParticles {
            particle: particle_name(get("particle"))?,
            speed: number(get("speed"))?,
            horizontal_movement_scale: map
                .get("horizontal_velocity")
                .and_then(|velocity| velocity.get("movement_scale"))
                .map_or(Ok(0.0), number)?,
            vertical_base: map
                .get("vertical_velocity")
                .and_then(|velocity| velocity.get("base"))
                .map_or(Ok(0.0), number)?,
            vertical_offset: map
                .get("vertical_position")
                .and_then(|position| position.get("offset"))
                .map_or(Ok(0.0), number)?,
        }),
        "minecraft:play_sound" => {
            let (low, high) = match get("pitch") {
                Value::Object(pitch) => (
                    number(pitch.get("min_inclusive").unwrap_or(&Value::Null))?,
                    number(pitch.get("max_exclusive").unwrap_or(&Value::Null))?,
                ),
                single => (number(single)?, number(single)?),
            };
            Ok(EntityEffect::PlaySound {
                sounds: leak_slice(
                    one_or_many(get("sound"))?
                        .into_iter()
                        .map(|name| leak_text(&normalized(name)))
                        .collect(),
                ),
                volume: number(get("volume"))?,
                pitch_min: low,
                pitch_max: high,
            })
        }
        "minecraft:attribute" => Ok(EntityEffect::Attribute {
            id: leak_text(&normalized(text(get("id"))?)),
            attribute: leak_text(&normalized(text(get("attribute"))?)),
            amount: level_value(get("amount"))?,
            operation: leak_text(text(get("operation"))?),
        }),
        other => Err(format!("unsupported entity effect {other}")),
    }
}

fn conditional_list(value: &Value) -> Parsed<&'static [ConditionalEffect]> {
    let list = value.as_array().ok_or("expected a list")?;
    let mut out = Vec::new();
    for entry in list {
        let map = object(entry)?;
        out.push(ConditionalEffect {
            effect: entity_effect(map.get("effect").ok_or("missing effect")?)?,
            requirements: map.get("requirements").map(requirement).transpose()?,
        });
    }
    Ok(leak_slice(out))
}

fn role(value: Option<&Value>) -> Parsed<EffectRole> {
    Ok(match value.and_then(Value::as_str) {
        Some("attacker") => EffectRole::Attacker,
        Some("victim") => EffectRole::Victim,
        Some("damaging_entity") => EffectRole::DamagingEntity,
        other => return Err(format!("unknown effect role {other:?}")),
    })
}

fn targeted_list(value: &Value) -> Parsed<&'static [TargetedEffect]> {
    let list = value.as_array().ok_or("expected a list")?;
    let mut out = Vec::new();
    for entry in list {
        let map = object(entry)?;
        out.push(TargetedEffect {
            enchanted: role(map.get("enchanted"))?,
            affected: role(map.get("affected"))?,
            effect: entity_effect(map.get("effect").ok_or("missing effect")?)?,
            requirements: map.get("requirements").map(requirement).transpose()?,
        });
    }
    Ok(leak_slice(out))
}

fn names(value: &Value) -> Parsed<&'static [&'static str]> {
    Ok(leak_slice(
        one_or_many(value)?
            .into_iter()
            .map(|name| leak_text(&normalized(name)))
            .collect(),
    ))
}

fn effects_of(
    slots: &'static [&'static str],
    effects: &Map<String, Value>,
) -> Parsed<EnchantmentEffects> {
    let values = |key: &str| {
        effects
            .get(&format!("minecraft:{key}"))
            .map_or(Ok(&[][..]), value_list)
    };
    let conditional = |key: &str| {
        effects
            .get(&format!("minecraft:{key}"))
            .map_or(Ok(&[][..]), conditional_list)
    };
    let has = |key: &str| effects.contains_key(&format!("minecraft:{key}"));
    let op = |key: &str| {
        effects
            .get(&format!("minecraft:{key}"))
            .map(value_op)
            .transpose()
    };
    let mut charging = Vec::new();
    if let Some(list) = effects.get("minecraft:crossbow_charging_sounds") {
        for entry in list.as_array().ok_or("charging sounds without a list")? {
            let map = object(entry)?;
            charging.push(ChargingSounds {
                start: leak_text(&normalized(text(map.get("start").unwrap_or(&Value::Null))?)),
                end: leak_text(&normalized(text(map.get("end").unwrap_or(&Value::Null))?)),
            });
        }
    }
    let mut immunity = Vec::new();
    if let Some(list) = effects.get("minecraft:damage_immunity") {
        for entry in list.as_array().ok_or("immunity without a list")? {
            let map = object(entry)?;
            immunity.push(Immunity {
                requirements: map.get("requirements").map(loot_condition).transpose()?,
            });
        }
    }
    let mut attributes = Vec::new();
    if let Some(list) = effects.get("minecraft:attributes") {
        for entry in list.as_array().ok_or("attributes without a list")? {
            let map = object(entry)?;
            attributes.push(AttributeEffect {
                attribute: leak_text(&normalized(text(
                    map.get("attribute").unwrap_or(&Value::Null),
                )?)),
                amount: level_based(map.get("amount").unwrap_or(&Value::Null))?,
                operation: leak_text(text(map.get("operation").unwrap_or(&Value::Null))?),
            });
        }
    }
    Ok(EnchantmentEffects {
        slots,
        damage: values("damage")?,
        damage_protection: values("damage_protection")?,
        knockback: values("knockback")?,
        armor_effectiveness: values("armor_effectiveness")?,
        smash_damage_per_fallen_block: values("smash_damage_per_fallen_block")?,
        equipment_drops: values("equipment_drops")?,
        ammo_use: values("ammo_use")?,
        projectile_count: values("projectile_count")?,
        projectile_spread: values("projectile_spread")?,
        projectile_piercing: values("projectile_piercing")?,
        block_experience: values("block_experience")?,
        mob_experience: values("mob_experience")?,
        fishing_luck_bonus: values("fishing_luck_bonus")?,
        fishing_time_reduction: values("fishing_time_reduction")?,
        item_damage: values("item_damage")?,
        repair_with_xp: values("repair_with_xp")?,
        trident_return_acceleration: values("trident_return_acceleration")?,
        trident_spin_attack_strength: op("trident_spin_attack_strength")?,
        trident_sound: match effects.get("minecraft:trident_sound") {
            Some(value) => names(value)?,
            None => &[],
        },
        crossbow_charge_time: op("crossbow_charge_time")?,
        crossbow_charging_sounds: leak_slice(charging),
        prevent_armor_change: has("prevent_armor_change"),
        prevent_equipment_drop: has("prevent_equipment_drop"),
        damage_immunity: leak_slice(immunity),
        post_attack: match effects.get("minecraft:post_attack") {
            Some(value) => targeted_list(value)?,
            None => &[],
        },
        post_piercing_attack: conditional("post_piercing_attack")?,
        projectile_spawned: conditional("projectile_spawned")?,
        hit_block: conditional("hit_block")?,
        location_changed: conditional("location_changed")?,
        tick: conditional("tick")?,
        attributes: leak_slice(attributes),
    })
}

fn cost(value: &Value) -> Parsed<Cost> {
    let map = object(value)?;
    Ok(Cost {
        base: integer(map.get("base").unwrap_or(&Value::Null))?,
        per_level_above_first: integer(map.get("per_level_above_first").unwrap_or(&Value::Null))?,
    })
}

fn tag_of(value: &Value) -> Parsed<&'static str> {
    let name = text(value)?;
    name.strip_prefix('#')
        .map(|_| leak_text(name))
        .ok_or_else(|| format!("expected a tag reference, found {name}"))
}

pub fn parse(name: &str, json: &str) -> Parsed<(EnchantmentDef, EnchantmentEffects)> {
    let value: Value = serde_json::from_str(json).map_err(|error| error.to_string())?;
    let map = object(&value)?;
    let get = |key: &str| map.get(key).ok_or_else(|| format!("missing {key}"));
    let level =
        |key: &str| u8::try_from(integer(get(key)?)?).map_err(|_| format!("{key} out of range"));
    let slots = names(get("slots")?).map(|slots| {
        leak_slice(
            slots
                .iter()
                .map(|slot| slot.trim_start_matches("minecraft:"))
                .collect(),
        )
    })?;
    let empty = Map::new();
    let effects = match map.get("effects") {
        Some(effects) => object(effects)?,
        None => &empty,
    };
    let def = EnchantmentDef {
        name: leak_text(&normalized(name)),
        max_level: level("max_level")?,
        min_cost: cost(get("min_cost")?)?,
        max_cost: cost(get("max_cost")?)?,
        anvil_cost: level("anvil_cost")?,
        weight: level("weight")?,
        exclusive_set: map.get("exclusive_set").map(tag_of).transpose()?,
        supported_items: tag_of(get("supported_items")?)?,
        primary_items: map.get("primary_items").map(tag_of).transpose()?,
    };
    Ok((def, effects_of(slots, effects)?))
}

pub fn build(overlay: &RegistryOverlay) -> EnchantmentOverlay {
    let mut built = EnchantmentOverlay::default();
    for entry in overlay.entries(ENCHANTMENT_REGISTRY) {
        match parse(&entry.id, &entry.json) {
            Ok((def, effects)) => built.insert(&normalized(&entry.id), leak(def), leak(effects)),
            Err(error) => {
                tracing::warn!("Failed to load enchantment {}: {error}", entry.id);
            }
        }
    }
    built
}

thread_local! {
    static SEEN: RefCell<Option<Arc<RegistryOverlay>>> = const { RefCell::new(None) };
}

pub(crate) fn activate(overlay: &Arc<RegistryOverlay>) {
    let unchanged = SEEN.with(|seen| {
        seen.borrow()
            .as_ref()
            .map_or(overlay.entries(ENCHANTMENT_REGISTRY).is_empty(), |known| {
                Arc::ptr_eq(known, overlay)
            })
    });
    if unchanged {
        return;
    }
    SEEN.with(|seen| *seen.borrow_mut() = Some(Arc::clone(overlay)));
    rspigot_data::enchantments::activate(&Arc::new(build(overlay)));
}

use rspigot_data::loot::{
    Aggregate, AttributeValue, Binary, BlockPredicate, Condition, FloatExpr, FloatProvider,
    IntExpr, IntProvider, IntRange, LevelBasedValue, NumberProvider, ScoreHolder, Unary,
};
use rspigot_data::registries;
use rspigot_data::tags::HolderSet;
use rspigot_domain::Identifier;
use serde_json::{Map, Value};

use super::predicates::{
    block_predicate, damage_predicate, entity_predicate, item_predicate, target,
};

const BLOCK: &str = "minecraft:block";
const ITEM: &str = "minecraft:item";

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

fn constant(value: &Value) -> &Value {
    match value {
        Value::Object(map)
            if map
                .get("type")
                .and_then(Value::as_str)
                .is_some_and(|kind| kind == "minecraft:constant" || kind == "constant") =>
        {
            map.get("value").unwrap_or(value)
        }
        other => other,
    }
}

fn number(value: &Value) -> Parsed<f64> {
    match constant(value).as_f64() {
        Some(number) => Ok(number),
        None => invalid(format!("expected a number, found {value}")),
    }
}

pub(super) fn whole(value: &Value) -> Parsed<i32> {
    match constant(value).as_i64().and_then(|n| i32::try_from(n).ok()) {
        Some(number) => Ok(number),
        None => invalid(format!("expected an integer, found {value}")),
    }
}

pub(super) fn flag(value: &Value) -> Parsed<bool> {
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

fn simple_int(value: &Value) -> Option<IntProvider> {
    match value {
        Value::Number(_) => whole(value).ok().map(IntProvider::Constant),
        Value::Object(map) => match provider_kind(map).ok()?.as_str() {
            "constant" => whole(map.get("value")?).ok().map(IntProvider::Constant),
            "uniform" => Some(IntProvider::Uniform {
                min: whole(map.get("min")?).ok()?,
                max: whole(map.get("max")?).ok()?,
            }),
            "binomial" => Some(IntProvider::Binomial {
                n: whole(map.get("n")?).ok()?,
                p: number(map.get("p")?).ok()? as f32,
            }),
            _ => None,
        },
        _ => None,
    }
}

fn simple_float(value: &Value) -> Option<FloatProvider> {
    match value {
        Value::Number(_) => number(value)
            .ok()
            .map(|n| FloatProvider::Constant(n as f32)),
        Value::Object(map) => match provider_kind(map).ok()?.as_str() {
            "constant" => number(map.get("value")?)
                .ok()
                .map(|n| FloatProvider::Constant(n as f32)),
            "uniform" => Some(FloatProvider::Uniform {
                min: number(map.get("min")?).ok()? as f32,
                max: number(map.get("max")?).ok()? as f32,
            }),
            _ => None,
        },
        _ => None,
    }
}

fn provider_kind(map: &Map<String, Value>) -> Parsed<String> {
    match map.get("type") {
        Some(Value::String(kind)) => Ok(kind.strip_prefix("minecraft:").unwrap_or(kind).to_owned()),
        Some(other) => invalid(format!("bad provider type {other}")),
        None if map.contains_key("min") => Ok("uniform".to_owned()),
        None => invalid("missing \"type\""),
    }
}

fn input_list<T>(
    map: &Map<String, Value>,
    key: &str,
    parse: impl Fn(&Value) -> Parsed<T>,
) -> Parsed<&'static [T]> {
    let items = match required(map, key)? {
        Value::Array(items) => items.iter().map(&parse).collect::<Parsed<Vec<T>>>()?,
        single => vec![parse(single)?],
    };
    Ok(leak_slice(items))
}

fn aggregate(kind: &str) -> Option<Aggregate> {
    Some(match kind {
        "add" => Aggregate::Sum,
        "mul" => Aggregate::Product,
        "avg" => Aggregate::Average,
        "min" => Aggregate::Minimum,
        "max" => Aggregate::Maximum,
        "length" => Aggregate::Length,
        _ => return None,
    })
}

fn unary(kind: &str) -> Option<Unary> {
    Some(match kind {
        "abs" => Unary::Absolute,
        "negate" => Unary::Negate,
        "ceil" => Unary::Ceiling,
        "floor" => Unary::Floor,
        "round" => Unary::Round,
        "truncate" => Unary::Truncate,
        "sin" => Unary::Sine,
        "cos" => Unary::Cosine,
        "sqrt" => Unary::SquareRoot,
        _ => return None,
    })
}

fn binary(kind: &str) -> Option<Binary> {
    Some(match kind {
        "sub" => Binary::Difference,
        "div" => Binary::Quotient,
        "floor_div" => Binary::FloorQuotient,
        "mod" => Binary::Modulus,
        "floor_mod" => Binary::FloorModulus,
        _ => return None,
    })
}

fn weights<T>(
    map: &Map<String, Value>,
    parse: impl Fn(&Value) -> Parsed<T>,
) -> Parsed<&'static [(T, i32)]> {
    let Value::Array(list) = required(map, "distribution")? else {
        return invalid("\"distribution\" must be a list");
    };
    let mut entries = Vec::with_capacity(list.len());
    for entry in list {
        let entry = object(entry)?;
        entries.push((
            parse(required(entry, "data")?)?,
            whole(required(entry, "weight")?)?,
        ));
    }
    Ok(leak_slice(entries))
}

fn cases<T>(
    map: &Map<String, Value>,
    parse: impl Fn(&Value) -> Parsed<T>,
) -> Parsed<&'static [(Condition, T)]> {
    let Value::Array(list) = required(map, "cases")? else {
        return invalid("\"cases\" must be a list");
    };
    let mut parsed = Vec::with_capacity(list.len());
    for case in list {
        let case = object(case)?;
        parsed.push((
            condition(required(case, "condition")?)?,
            parse(required(case, "value")?)?,
        ));
    }
    Ok(leak_slice(parsed))
}

fn score_holder(value: &Value) -> Parsed<ScoreHolder> {
    let map = object(value)?;
    match kind_of(map, "type")?.as_str() {
        "minecraft:context" => Ok(ScoreHolder::Context(target(required(map, "target")?)?)),
        "minecraft:fixed" => match required(map, "name")?.as_str() {
            Some(holder) => Ok(ScoreHolder::Fixed(leak_text(holder.to_owned()))),
            None => invalid("score holder name must be a string"),
        },
        other => unsupported(format!("score holder {other}")),
    }
}

fn storage_pair(map: &Map<String, Value>) -> Parsed<(&'static str, &'static str)> {
    let path = match required(map, "path")?.as_str() {
        Some(path) if rspigot_nbt::NbtPath::parse(path).is_ok() => path,
        _ => return invalid("bad storage path"),
    };
    Ok((
        leak_text(name(required(map, "storage")?)?),
        leak_text(path.to_owned()),
    ))
}

fn attribute_name(map: &Map<String, Value>) -> Parsed<&'static str> {
    let text = name(required(map, "attribute")?)?;
    registry_id("minecraft:environment_attribute", &text)?;
    Ok(leak_text(text))
}

fn zero_int() -> &'static IntExpr {
    leak(IntExpr::Constant(0))
}

fn zero_float() -> &'static FloatExpr {
    leak(FloatExpr::Constant(0.0))
}

fn int_child(map: &Map<String, Value>, key: &str) -> Parsed<&'static IntExpr> {
    int_expr(required(map, key)?).map(leak)
}

fn float_child(map: &Map<String, Value>, key: &str) -> Parsed<&'static FloatExpr> {
    float_expr(required(map, key)?).map(leak)
}

pub(super) fn int_expr(value: &Value) -> Parsed<IntExpr> {
    match value {
        Value::Number(_) => Ok(IntExpr::Constant(whole(value)?)),
        Value::String(_) => Ok(IntExpr::Reference(leak_text(name(value)?))),
        Value::Object(map) => {
            let kind = provider_kind(map)?;
            if let Some(op) = aggregate(&kind).filter(|op| *op != Aggregate::Length) {
                return Ok(IntExpr::Aggregate {
                    op,
                    inputs: input_list(map, "inputs", int_expr)?,
                });
            }
            if let Some(op) = binary(&kind) {
                return Ok(IntExpr::Binary {
                    op,
                    left: int_child(map, "left")?,
                    right: int_child(map, "right")?,
                });
            }
            match kind.as_str() {
                "constant" => Ok(IntExpr::Constant(whole(required(map, "value")?)?)),
                "uniform" => Ok(IntExpr::Uniform {
                    min: int_child(map, "min")?,
                    max: int_child(map, "max")?,
                }),
                "binomial" => Ok(IntExpr::Binomial {
                    n: int_child(map, "n")?,
                    p: float_child(map, "p")?,
                }),
                "abs" | "negate" => Ok(IntExpr::Unary {
                    op: unary(&kind).unwrap_or(Unary::Absolute),
                    input: int_child(map, "input")?,
                }),
                "pow" => Ok(IntExpr::Power {
                    base: int_child(map, "base")?,
                    exponent: int_child(map, "exponent")?,
                }),
                "from_float" => Ok(IntExpr::FromFloat(float_child(map, "input")?)),
                "conditional" => Ok(IntExpr::Conditional {
                    condition: leak(condition(required(map, "condition")?)?),
                    on_true: int_child(map, "on_true")?,
                    on_false: match map.get("on_false") {
                        Some(found) => leak(int_expr(found)?),
                        None => zero_int(),
                    },
                }),
                "number_dispatcher" => Ok(IntExpr::Dispatch {
                    cases: cases(map, int_expr)?,
                    default: match map.get("default") {
                        Some(found) => leak(int_expr(found)?),
                        None => zero_int(),
                    },
                }),
                "weighted_list" => Ok(IntExpr::Weighted(weights(map, int_expr)?)),
                "score" => Ok(IntExpr::Score {
                    holder: score_holder(required(map, "target")?)?,
                    objective: match required(map, "score")?.as_str() {
                        Some(objective) => leak_text(objective.to_owned()),
                        None => return invalid("\"score\" must be a string"),
                    },
                    fallback: match map.get("fallback") {
                        Some(found) => leak(int_expr(found)?),
                        None => zero_int(),
                    },
                }),
                "storage" => {
                    let (storage, path) = storage_pair(map)?;
                    Ok(IntExpr::Storage {
                        storage,
                        path,
                        fallback: match map.get("fallback") {
                            Some(found) => leak(int_expr(found)?),
                            None => zero_int(),
                        },
                    })
                }
                "environment_attribute" => Ok(IntExpr::Attribute(attribute_name(map)?)),
                other => unsupported(format!("int provider \"{other}\"")),
            }
        }
        other => invalid(format!("expected a number provider, found {other}")),
    }
}

pub(super) fn float_expr(value: &Value) -> Parsed<FloatExpr> {
    match value {
        Value::Number(_) => Ok(FloatExpr::Constant(number(value)? as f32)),
        Value::String(_) => Ok(FloatExpr::Reference(leak_text(name(value)?))),
        Value::Object(map) => {
            let kind = provider_kind(map)?;
            if let Some(op) = aggregate(&kind) {
                return Ok(FloatExpr::Aggregate {
                    op,
                    inputs: input_list(map, "inputs", float_expr)?,
                });
            }
            if let Some(op) = binary(&kind)
                .filter(|op| matches!(op, Binary::Difference | Binary::Quotient | Binary::Modulus))
            {
                return Ok(FloatExpr::Binary {
                    op,
                    left: float_child(map, "left")?,
                    right: float_child(map, "right")?,
                });
            }
            if let Some(op) = unary(&kind) {
                return Ok(FloatExpr::Unary {
                    op,
                    input: float_child(map, "input")?,
                });
            }
            match kind.as_str() {
                "constant" => Ok(FloatExpr::Constant(number(required(map, "value")?)? as f32)),
                "uniform" => Ok(FloatExpr::Uniform {
                    min: float_child(map, "min")?,
                    max: float_child(map, "max")?,
                }),
                "pow" => Ok(FloatExpr::Power {
                    base: float_child(map, "base")?,
                    exponent: float_child(map, "exponent")?,
                }),
                "from_int" => Ok(FloatExpr::FromInt(int_child(map, "input")?)),
                "enchantment_level" => Ok(FloatExpr::EnchantmentLevel(level_based(required(
                    map, "amount",
                )?)?)),
                "conditional" => Ok(FloatExpr::Conditional {
                    condition: leak(condition(required(map, "condition")?)?),
                    on_true: float_child(map, "on_true")?,
                    on_false: match map.get("on_false") {
                        Some(found) => leak(float_expr(found)?),
                        None => zero_float(),
                    },
                }),
                "number_dispatcher" => Ok(FloatExpr::Dispatch {
                    cases: cases(map, float_expr)?,
                    default: match map.get("default") {
                        Some(found) => leak(float_expr(found)?),
                        None => zero_float(),
                    },
                }),
                "weighted_list" => Ok(FloatExpr::Weighted(weights(map, float_expr)?)),
                "storage" => {
                    let (storage, path) = storage_pair(map)?;
                    Ok(FloatExpr::Storage {
                        storage,
                        path,
                        fallback: match map.get("fallback") {
                            Some(found) => leak(float_expr(found)?),
                            None => zero_float(),
                        },
                    })
                }
                "environment_attribute" => Ok(FloatExpr::Attribute(attribute_name(map)?)),
                other => unsupported(format!("float provider \"{other}\"")),
            }
        }
        other => invalid(format!("expected a number provider, found {other}")),
    }
}

pub(super) fn int_provider(value: &Value) -> Parsed<IntProvider> {
    match simple_int(value) {
        Some(simple) => Ok(simple),
        None => Ok(IntProvider::Expr(leak(int_expr(value)?))),
    }
}

pub(super) fn float_provider(value: &Value) -> Parsed<FloatProvider> {
    match simple_float(value) {
        Some(simple) => Ok(simple),
        None => Ok(FloatProvider::Expr(leak(float_expr(value)?))),
    }
}

fn number_provider_as(value: &Value, integral: bool) -> Parsed<NumberProvider> {
    if let Some(simple) = simple_float(value) {
        return Ok(match simple {
            FloatProvider::Constant(constant) => NumberProvider::Constant(constant),
            FloatProvider::Uniform { min, max } => NumberProvider::Uniform { min, max },
            FloatProvider::Expr(expr) => NumberProvider::Float(expr),
        });
    }
    if let Some(IntProvider::Binomial { n, p }) = simple_int(value) {
        return Ok(NumberProvider::Binomial { n, p });
    }
    if integral {
        Ok(NumberProvider::Int(leak(int_expr(value)?)))
    } else {
        Ok(NumberProvider::Float(leak(float_expr(value)?)))
    }
}

pub(super) fn number_provider(value: &Value) -> Parsed<NumberProvider> {
    number_provider_as(value, false)
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

fn constant_chance(value: &Value) -> Parsed<f32> {
    match number_provider(value)? {
        NumberProvider::Constant(chance) => Ok(chance),
        _ => unsupported("non constant chance"),
    }
}

fn biome_names(wanted: &Value) -> Parsed<&'static [&'static str]> {
    match wanted {
        Value::String(text) if text.starts_with('#') => {
            unsupported("biome tags in a location predicate")
        }
        Value::String(_) => Ok(leak_slice(vec![leak_text(name(wanted)?)])),
        Value::Array(items) => {
            let mut names = Vec::new();
            for item in items {
                names.push(leak_text(name(item)?));
            }
            Ok(leak_slice(names))
        }
        other => invalid(format!("bad biomes {other}")),
    }
}

fn value_check(map: &Map<String, Value>, integral: bool) -> Parsed<Condition> {
    let provider = |value: &Value| number_provider_as(value, integral);
    let (min, max) = match required(map, "range")? {
        Value::Object(range) if range.contains_key("min") || range.contains_key("max") => (
            range.get("min").map(provider).transpose()?,
            range.get("max").map(provider).transpose()?,
        ),
        other => {
            let exact = provider(other)?;
            (Some(exact), Some(exact))
        }
    };
    Ok(Condition::ValueCheck {
        value: provider(required(map, "value")?)?,
        min,
        max,
    })
}

pub(super) fn kind_of(map: &Map<String, Value>, legacy: &str) -> Parsed<String> {
    match map.get("type").or_else(|| map.get(legacy)) {
        Some(Value::String(kind)) => name(&Value::from(kind.as_str())),
        _ => invalid("missing \"type\""),
    }
}

pub fn condition(value: &Value) -> Parsed<Condition> {
    if let Value::String(_) = value {
        return Ok(Condition::Reference(leak_text(name(value)?)));
    }
    if let Value::Array(list) = value {
        let mut parsed = Vec::with_capacity(list.len());
        for item in list {
            parsed.push(condition(item)?);
        }
        return Ok(Condition::AllOf(leak_slice(parsed)));
    }
    let map = object(value)?;
    let kind = kind_of(map, "condition")?;
    match kind.as_str() {
        "minecraft:inverted" => Ok(Condition::Inverted(leak(condition(required(
            map, "term",
        )?)?))),
        "minecraft:all_of" => Ok(Condition::AllOf(terms(map)?)),
        "minecraft:any_of" => Ok(Condition::AnyOf(terms(map)?)),
        "minecraft:killed_by_player" => Ok(Condition::KilledByPlayer),
        "minecraft:survives_explosion" => Ok(Condition::SurvivesExplosion),
        "minecraft:random_chance" => match required(map, "chance")? {
            Value::Object(chance)
                if chance.get("type").and_then(Value::as_str)
                    == Some("minecraft:enchantment_level") =>
            {
                Ok(Condition::RandomChanceByLevel(level_based(required(
                    chance, "amount",
                )?)?))
            }
            other => match number_provider(other)? {
                NumberProvider::Constant(chance) => Ok(Condition::RandomChance(chance)),
                provider => Ok(Condition::RandomChanceRolled(provider)),
            },
        },
        "minecraft:random_chance_with_enchanted_bonus" => {
            Ok(Condition::RandomChanceWithEnchantedBonus {
                enchantment: leak_text(name(required(map, "enchantment")?)?),
                unenchanted_chance: constant_chance(required(map, "unenchanted_chance")?)?,
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
                state: match map.get("properties") {
                    Some(properties) => {
                        let wrapped = serde_json::json!({ "state": properties });
                        block_predicate(&wrapped)?.state
                    }
                    None => &[],
                },
            }))
        }
        "minecraft:match_block" => {
            let mut rest = map.clone();
            rest.remove("type");
            rest.remove("condition");
            Ok(Condition::MatchBlock(block_predicate(&Value::Object(
                rest,
            ))?))
        }
        "minecraft:match_tool" => Ok(Condition::MatchTool(item_predicate(required(
            map,
            "predicate",
        )?)?)),
        "minecraft:location_check" => {
            only_keys(
                map,
                &[
                    "type",
                    "condition",
                    "offsetX",
                    "offsetY",
                    "offsetZ",
                    "predicate",
                ],
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
                    biomes = biome_names(wanted)?;
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
                clock: map
                    .get("clock")
                    .map(|clock| name(clock).map(leak_text))
                    .transpose()?,
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
        "minecraft:int_value_check" => value_check(map, true),
        "minecraft:float_value_check" => value_check(map, false),
        "minecraft:enchantment_active_check" => match required(map, "active")?.as_bool() {
            Some(active) => Ok(Condition::EnchantmentActive(active)),
            None => invalid("\"active\" must be a boolean"),
        },
        "minecraft:environment_attribute_check" => {
            let value = match required(map, "value")? {
                Value::Bool(flag) => AttributeValue::Bool(*flag),
                Value::Number(found) => AttributeValue::Float(found.as_f64().unwrap_or(0.0) as f32),
                Value::String(text) => AttributeValue::Text(leak_text(text.clone())),
                other => return unsupported(format!("environment attribute value {other}")),
            };
            Ok(Condition::EnvironmentAttributeCheck {
                attribute: attribute_name(map)?,
                value,
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
    condition(value)
}

pub(super) fn conditions(map: &Map<String, Value>) -> Parsed<Option<Condition>> {
    if let Some(found) = map.get("condition") {
        return condition(found).map(Some);
    }
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

use std::collections::HashMap;

use serde_json::Value;

use super::processors::position_seed;
use crate::legacy::JavaRandom;

#[derive(Debug, Clone)]
pub(crate) enum Alias {
    Direct {
        alias: String,
        target: String,
    },
    Random {
        alias: String,
        targets: Vec<(String, i32)>,
    },
    Group(Vec<(Vec<Alias>, i32)>),
}

pub(crate) type Lookup = HashMap<String, String>;

fn named(value: &str) -> String {
    if value.contains(':') {
        value.to_owned()
    } else {
        format!("minecraft:{value}")
    }
}

fn weight(value: &Value) -> i32 {
    value
        .get("weight")
        .and_then(Value::as_i64)
        .and_then(|n| i32::try_from(n).ok())
        .unwrap_or(1)
}

fn text(value: &Value, key: &str) -> Option<String> {
    value.get(key).and_then(Value::as_str).map(named)
}

impl Alias {
    fn parse(value: &Value) -> Option<Self> {
        let kind = value.get("type")?.as_str()?;
        match kind.strip_prefix("minecraft:").unwrap_or(kind) {
            "direct" => Some(Self::Direct {
                alias: text(value, "alias")?,
                target: text(value, "target")?,
            }),
            "random" => Some(Self::Random {
                alias: text(value, "alias")?,
                targets: value
                    .get("targets")?
                    .as_array()?
                    .iter()
                    .filter_map(|entry| Some((text(entry, "data")?, weight(entry))))
                    .collect(),
            }),
            "random_group" => Some(Self::Group(
                value
                    .get("groups")?
                    .as_array()?
                    .iter()
                    .map(|group| {
                        let members = group
                            .get("data")
                            .and_then(Value::as_array)
                            .map(|items| items.iter().filter_map(Self::parse).collect())
                            .unwrap_or_default();
                        (members, weight(group))
                    })
                    .collect(),
            )),
            _ => None,
        }
    }

    pub(crate) fn parse_all(value: Option<&Value>) -> Option<Vec<Self>> {
        let Some(items) = value.and_then(Value::as_array) else {
            return Some(Vec::new());
        };
        items.iter().map(Self::parse).collect()
    }

    fn resolve(&self, random: &mut JavaRandom, out: &mut Lookup) {
        match self {
            Self::Direct { alias, target } => {
                out.insert(alias.clone(), target.clone());
            }
            Self::Random { alias, targets } => {
                if let Some(target) = pick(targets, random) {
                    out.insert(alias.clone(), target.clone());
                }
            }
            Self::Group(groups) => {
                if let Some(members) = pick(groups, random) {
                    for member in members {
                        member.resolve(random, out);
                    }
                }
            }
        }
    }
}

fn pick<'a, T>(items: &'a [(T, i32)], random: &mut JavaRandom) -> Option<&'a T> {
    let total: i32 = items.iter().map(|(_, weight)| *weight).sum();
    if total <= 0 {
        return None;
    }
    let mut roll = random.next_int(total);
    for (item, weight) in items {
        roll -= weight;
        if roll < 0 {
            return Some(item);
        }
    }
    None
}

pub(crate) fn lookup(aliases: &[Alias], position: (i32, i32, i32), seed: i64) -> Lookup {
    let mut out = Lookup::new();
    if aliases.is_empty() {
        return out;
    }
    let mut base = JavaRandom::new(seed);
    let factory = base.next_long();
    let mut random = JavaRandom::new(position_seed(position) ^ factory);
    for alias in aliases {
        alias.resolve(&mut random, &mut out);
    }
    out
}

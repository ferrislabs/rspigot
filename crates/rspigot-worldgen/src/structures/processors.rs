use rspigot_blocks::BlockKind;
use rspigot_domain::BlockStateId;
use serde_json::Value;

use super::bbox::Pos;
use super::library::Library;
use super::pool::Processors;
use crate::datapack::block_state;
use crate::legacy::JavaRandom;
use crate::overworld::features::place::Run;
use crate::overworld::features::prov::BlockSet;
use crate::overworld::region::Heightmap;

#[derive(Debug, Clone)]
pub(crate) enum Test {
    Always,
    Block(BlockKind),
    State(BlockStateId),
    RandomBlock(BlockKind, f32),
    RandomState(BlockStateId, f32),
    Tag(BlockSet),
}

#[derive(Debug, Clone)]
pub(crate) struct Rule {
    input: Test,
    location: Test,
    output: BlockStateId,
}

#[derive(Debug, Clone)]
pub(crate) enum Step {
    IgnoreStructureBlock,
    JigsawReplacement,
    Rules(Vec<Rule>),
    BlockRot {
        integrity: f32,
        rottable: Option<BlockSet>,
    },
    Protected(BlockSet),
    Gravity {
        map: Heightmap,
        offset: i32,
    },
    Nop,
}

pub(crate) struct Subject<'a> {
    pub(crate) local: Pos,
    pub(crate) at: Pos,
    pub(crate) state: BlockStateId,
    pub(crate) nbt: Option<&'a Value>,
}

pub(crate) fn position_seed((x, y, z): Pos) -> i64 {
    let mut l = i64::from(x.wrapping_mul(3_129_871))
        ^ i64::from(z).wrapping_mul(116_129_781)
        ^ i64::from(y);
    l = l
        .wrapping_mul(l)
        .wrapping_mul(42_317_861)
        .wrapping_add(l.wrapping_mul(11));
    l >> 16
}

fn kind_of(value: &Value) -> Option<BlockKind> {
    BlockKind::from_name(value.get("block")?.as_str()?)
}

fn probability(value: &Value) -> f32 {
    value
        .get("probability")
        .and_then(Value::as_f64)
        .map_or(1.0, |p| p as f32)
}

impl Test {
    fn compile(value: &Value) -> Option<Self> {
        let kind = value.get("predicate_type")?.as_str()?;
        Some(match kind.strip_prefix("minecraft:").unwrap_or(kind) {
            "always_true" => Self::Always,
            "block_match" => Self::Block(kind_of(value)?),
            "blockstate_match" => Self::State(block_state(value.get("block_state")?)?),
            "random_block_match" => Self::RandomBlock(kind_of(value)?, probability(value)),
            "random_blockstate_match" => {
                Self::RandomState(block_state(value.get("block_state")?)?, probability(value))
            }
            "tag_match" => Self::Tag(BlockSet::compile(&Value::String(format!(
                "#{}",
                value.get("tag")?.as_str()?
            )))),
            _ => return None,
        })
    }

    fn test(&self, state: BlockStateId, random: &mut JavaRandom) -> bool {
        match self {
            Self::Always => true,
            Self::Block(kind) => state.kind() == Some(*kind),
            Self::State(wanted) => state == *wanted,
            Self::RandomBlock(kind, chance) => {
                state.kind() == Some(*kind) && random.next_float() < *chance
            }
            Self::RandomState(wanted, chance) => state == *wanted && random.next_float() < *chance,
            Self::Tag(set) => set.contains(state),
        }
    }
}

impl Rule {
    fn compile(value: &Value) -> Option<Self> {
        Some(Self {
            input: Test::compile(value.get("input_predicate")?)?,
            location: Test::compile(value.get("location_predicate")?)?,
            output: block_state(value.get("output_state")?)?,
        })
    }
}

impl Step {
    fn compile(value: &Value) -> Self {
        let kind = value
            .get("processor_type")
            .and_then(Value::as_str)
            .unwrap_or("");
        match kind.strip_prefix("minecraft:").unwrap_or(kind) {
            "rule" => Self::Rules(
                value
                    .get("rules")
                    .and_then(Value::as_array)
                    .map(|rules| rules.iter().filter_map(Rule::compile).collect())
                    .unwrap_or_default(),
            ),
            "block_rot" => Self::BlockRot {
                integrity: value
                    .get("integrity")
                    .and_then(Value::as_f64)
                    .map_or(1.0, |n| n as f32),
                rottable: value.get("rottable_blocks").map(|set| {
                    BlockSet::compile(&match set {
                        Value::String(name) if !name.starts_with('#') && name.contains(':') => {
                            Value::String(name.clone())
                        }
                        other => other.clone(),
                    })
                }),
            },
            "protected_blocks" => value
                .get("value")
                .map_or(Self::Nop, |set| Self::Protected(BlockSet::compile(set))),
            "gravity" => Self::Gravity {
                map: Heightmap::from_name(
                    value
                        .get("heightmap")
                        .and_then(Value::as_str)
                        .unwrap_or("WORLD_SURFACE_WG"),
                ),
                offset: value
                    .get("offset")
                    .and_then(Value::as_i64)
                    .map_or(0, |n| n as i32),
            },
            _ => Self::Nop,
        }
    }

    pub(crate) fn gravity() -> Self {
        Self::Gravity {
            map: Heightmap::WorldSurface,
            offset: -1,
        }
    }

    pub(crate) fn process(&self, run: &mut Run, subject: &mut Subject) -> bool {
        match self {
            Self::Nop => true,
            Self::IgnoreStructureBlock => subject.state.kind() != Some(BlockKind::StructureBlock),
            Self::JigsawReplacement => {
                if subject.state.kind() != Some(BlockKind::Jigsaw) {
                    return true;
                }
                let name = subject
                    .nbt
                    .and_then(|nbt| nbt.get("final_state"))
                    .and_then(Value::as_str)
                    .unwrap_or("minecraft:air");
                let replacement = parse_state(name).unwrap_or(BlockStateId::AIR);
                if replacement.kind() == Some(BlockKind::StructureVoid) {
                    return false;
                }
                subject.state = replacement;
                subject.nbt = None;
                true
            }
            Self::Rules(rules) => {
                let mut random = JavaRandom::new(position_seed(subject.at));
                let world = run.region.get(subject.at.0, subject.at.1, subject.at.2);
                for rule in rules {
                    if rule.input.test(subject.state, &mut random)
                        && rule.location.test(world, &mut random)
                    {
                        subject.state = rule.output;
                        subject.nbt = None;
                        break;
                    }
                }
                true
            }
            Self::BlockRot {
                integrity,
                rottable,
            } => {
                if rottable
                    .as_ref()
                    .is_some_and(|set| !set.contains(subject.state))
                {
                    return true;
                }
                let mut random = JavaRandom::new(position_seed(subject.at));
                random.next_float() <= *integrity
            }
            Self::Protected(set) => {
                !set.contains(run.region.get(subject.at.0, subject.at.1, subject.at.2))
            }
            Self::Gravity { map, offset } => {
                let height = run.region.height_at(*map, subject.at.0, subject.at.2);
                subject.at.1 = height + offset + subject.local.1;
                true
            }
        }
    }
}

pub(crate) fn parse_state(text: &str) -> Option<BlockStateId> {
    let (name, rest) = match text.split_once('[') {
        Some((name, rest)) => (name, rest.trim_end_matches(']')),
        None => (text, ""),
    };
    let properties: Vec<(&str, &str)> = rest
        .split(',')
        .filter(|pair| !pair.is_empty())
        .filter_map(|pair| pair.split_once('='))
        .collect();
    crate::datapack::state_named(name, &properties)
}

pub(crate) fn compile_list(value: &Value) -> Vec<Step> {
    value
        .get("processors")
        .and_then(Value::as_array)
        .map(|items| items.iter().map(Step::compile).collect())
        .unwrap_or_default()
}

pub(crate) fn steps_for(library: &Library, processors: &Processors) -> Vec<Step> {
    match processors {
        Processors::Named(name) => library
            .steps(name)
            .map(<[Step]>::to_vec)
            .unwrap_or_default(),
        Processors::Inline(value) => compile_list(value),
    }
}

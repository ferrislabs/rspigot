use rspigot_blocks::BlockKind as Kind;
use rspigot_domain::BlockStateId;
use serde_json::Value;

use super::super::place::Run;
use super::super::prov::{BlockSet, Pos, number, text};
use crate::datapack::block_state;
use crate::legacy::JavaRandom;
use crate::structures::processors::{Modifier, position_seed};

#[derive(Debug, Clone)]
pub(crate) struct Rule {
    input: Kind,
    output: BlockStateId,
    modifier: Modifier,
}

#[derive(Debug, Clone)]
pub(crate) enum Processor {
    BlockRot(f32),
    Protected(BlockSet),
    Rules(Vec<Rule>),
}

fn tag<'a>(value: &'a Value, key: &str) -> &'a str {
    let name = text(value, key);
    name.strip_prefix("minecraft:").unwrap_or(name)
}

fn rule(value: &Value) -> Option<Rule> {
    let input = value.get("input_predicate")?;
    if tag(input, "predicate_type") != "block_match" {
        return None;
    }
    if tag(value.get("location_predicate")?, "predicate_type") != "always_true" {
        return None;
    }
    if let Some(position) = value.get("position_predicate")
        && tag(position, "predicate_type") != "always_true"
    {
        return None;
    }
    Some(Rule {
        input: Kind::from_name(text(input, "block"))?,
        output: block_state(value.get("output_state")?)?,
        modifier: Modifier::compile(value.get("block_entity_modifier")),
    })
}

impl Processor {
    fn compile(value: &Value) -> Option<Self> {
        match tag(value, "processor_type") {
            "block_rot" if value.get("rottable_blocks").is_none() => {
                Some(Self::BlockRot(number(value, "integrity", 1.0) as f32))
            }
            "protected_blocks" => Some(Self::Protected(BlockSet::compile(value.get("value")?))),
            "rule" => Some(Self::Rules(
                value
                    .get("rules")?
                    .as_array()?
                    .iter()
                    .map(rule)
                    .collect::<Option<_>>()?,
            )),
            _ => None,
        }
    }

    pub(crate) fn compile_list(value: &Value) -> Option<Vec<Self>> {
        value
            .get("processors")?
            .as_array()?
            .iter()
            .map(Self::compile)
            .collect()
    }

    pub(crate) fn process(
        &self,
        run: &mut Run,
        (x, y, z): Pos,
        state: BlockStateId,
        entity: &mut Option<Value>,
    ) -> Option<BlockStateId> {
        match self {
            Self::BlockRot(integrity) => (run.rng.next_float() <= *integrity).then_some(state),
            Self::Protected(set) => (!set.contains(run.region.get(x, y, z))).then_some(state),
            Self::Rules(rules) => {
                let Some(rule) = rules.iter().find(|rule| state.kind() == Some(rule.input)) else {
                    return Some(state);
                };
                let mut random = JavaRandom::new(position_seed((x, y, z)));
                *entity = rule.modifier.apply(&mut random, rule.output, entity.take());
                Some(rule.output)
            }
        }
    }
}

use std::borrow::Cow;

use rspigot_blocks::BlockKind;
use rspigot_domain::BlockStateId;
use serde_json::{Value, json};

use super::bbox::Pos;
use super::library::Library;
use super::pool::Processors;
use crate::datapack::block_state;
use crate::legacy::JavaRandom;
use crate::overworld::features::place::Run;
use crate::overworld::features::prov::{BlockSet, Int};
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

#[derive(Debug, Clone, Copy)]
pub(crate) enum Axis {
    X,
    Y,
    Z,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct Ramp {
    min_chance: f32,
    max_chance: f32,
    min_dist: i32,
    max_dist: i32,
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum PosTest {
    Always,
    Linear(Ramp),
    AxisAligned(Ramp, Axis),
}

#[derive(Debug, Clone)]
pub(crate) enum Modifier {
    Passthrough,
    Clear,
    AppendStatic(Value),
    AppendLoot(String),
}

#[derive(Debug, Clone)]
pub(crate) struct Rule {
    input: Test,
    location: Test,
    position: PosTest,
    output: BlockStateId,
    modifier: Modifier,
}

#[derive(Debug, Clone)]
pub(crate) enum Step {
    IgnoreStructureBlock,
    IgnoreAir,
    Ignore(BlockSet),
    BlockAge {
        mossiness: f32,
    },
    LavaSubmerged,
    Blackstone,
    JigsawReplacement,
    Rules(Vec<Rule>),
    Capped {
        limit: Int,
        delegate: Box<Step>,
    },
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
    pub(crate) reference: Pos,
    pub(crate) state: BlockStateId,
    pub(crate) nbt: Option<Cow<'a, Value>>,
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

fn inverse_lerp(value: f32, start: f32, end: f32) -> f32 {
    (value - start) / (end - start)
}

fn clamped_lerp(delta: f32, start: f32, end: f32) -> f32 {
    if delta < 0.0 {
        start
    } else if delta > 1.0 {
        end
    } else {
        start + delta * (end - start)
    }
}

impl Ramp {
    fn compile(value: &Value) -> Self {
        let float = |key: &str| value.get(key).and_then(Value::as_f64).unwrap_or(0.0) as f32;
        let int = |key: &str| value.get(key).and_then(Value::as_i64).unwrap_or(0) as i32;
        Self {
            min_chance: float("min_chance"),
            max_chance: float("max_chance"),
            min_dist: int("min_dist"),
            max_dist: int("max_dist"),
        }
    }

    fn passes(&self, distance: f32, random: &mut JavaRandom) -> bool {
        let roll = random.next_float();
        let delta = inverse_lerp(distance, self.min_dist as f32, self.max_dist as f32);
        roll <= clamped_lerp(delta, self.min_chance, self.max_chance)
    }
}

impl PosTest {
    fn compile(value: Option<&Value>) -> Option<Self> {
        let Some(value) = value else {
            return Some(Self::Always);
        };
        let kind = value.get("predicate_type")?.as_str()?;
        Some(match kind.strip_prefix("minecraft:").unwrap_or(kind) {
            "always_true" => Self::Always,
            "linear_pos" => Self::Linear(Ramp::compile(value)),
            "axis_aligned_linear_pos" => Self::AxisAligned(
                Ramp::compile(value),
                match value.get("axis").and_then(Value::as_str) {
                    Some("x") => Axis::X,
                    Some("z") => Axis::Z,
                    _ => Axis::Y,
                },
            ),
            _ => return None,
        })
    }

    fn test(&self, at: Pos, reference: Pos, random: &mut JavaRandom) -> bool {
        match self {
            Self::Always => true,
            Self::Linear(ramp) => {
                let distance = (at.0 - reference.0).abs()
                    + (at.1 - reference.1).abs()
                    + (at.2 - reference.2).abs();
                ramp.passes(distance as f32, random)
            }
            Self::AxisAligned(ramp, axis) => {
                let distance = match axis {
                    Axis::X => (at.0 - reference.0).abs(),
                    Axis::Y => (at.1 - reference.1).abs(),
                    Axis::Z => (at.2 - reference.2).abs(),
                };
                ramp.passes(distance as f32, random)
            }
        }
    }
}

fn merged(base: &mut Value, extra: &Value) {
    match (base, extra) {
        (Value::Object(base), Value::Object(extra)) => {
            for (key, value) in extra {
                match base.get_mut(key) {
                    Some(existing) => merged(existing, value),
                    None => {
                        base.insert(key.clone(), value.clone());
                    }
                }
            }
        }
        (base, extra) => *base = extra.clone(),
    }
}

pub(crate) fn block_entity_id(state: BlockStateId) -> Option<&'static str> {
    match state.kind()? {
        BlockKind::SuspiciousSand | BlockKind::SuspiciousGravel => {
            Some("minecraft:brushable_block")
        }
        BlockKind::Chest => Some("minecraft:chest"),
        BlockKind::TrappedChest => Some("minecraft:trapped_chest"),
        BlockKind::Barrel => Some("minecraft:barrel"),
        _ => None,
    }
}

impl Modifier {
    pub(crate) fn compile(value: Option<&Value>) -> Self {
        let Some(value) = value else {
            return Self::Passthrough;
        };
        let kind = value.get("type").and_then(Value::as_str).unwrap_or("");
        match kind.strip_prefix("minecraft:").unwrap_or(kind) {
            "clear" => Self::Clear,
            "append_static" => value
                .get("data")
                .cloned()
                .map_or(Self::Passthrough, Self::AppendStatic),
            "append_loot" => value
                .get("loot_table")
                .and_then(Value::as_str)
                .map_or(Self::Passthrough, |table| {
                    Self::AppendLoot(table.to_owned())
                }),
            _ => Self::Passthrough,
        }
    }

    pub(crate) fn apply(
        &self,
        random: &mut JavaRandom,
        state: BlockStateId,
        existing: Option<Value>,
    ) -> Option<Value> {
        let base = || {
            existing
                .clone()
                .or_else(|| block_entity_id(state).map(|id| json!({ "id": id })))
        };
        match self {
            Self::Passthrough => None,
            Self::Clear => Some(json!({})),
            Self::AppendStatic(data) => {
                let mut tag = base().unwrap_or_else(|| json!({}));
                merged(&mut tag, data);
                Some(tag)
            }
            Self::AppendLoot(table) => {
                let mut tag = base().unwrap_or_else(|| json!({}));
                if let Value::Object(map) = &mut tag {
                    map.insert("LootTable".to_owned(), Value::String(table.clone()));
                    map.insert(
                        "LootTableSeed".to_owned(),
                        json!({ "$l": random.next_long() }),
                    );
                }
                Some(tag)
            }
        }
    }
}

impl Rule {
    pub(crate) fn replace(input: Test, output: BlockStateId) -> Self {
        Self {
            input,
            location: Test::Always,
            position: PosTest::Always,
            output,
            modifier: Modifier::Passthrough,
        }
    }

    fn compile(value: &Value) -> Option<Self> {
        Some(Self {
            input: Test::compile(value.get("input_predicate")?)?,
            location: Test::compile(value.get("location_predicate")?)?,
            position: PosTest::compile(value.get("position_predicate"))?,
            output: block_state(value.get("output_state")?)?,
            modifier: Modifier::compile(value.get("block_entity_modifier")),
        })
    }

    fn apply(
        &self,
        state: BlockStateId,
        world: BlockStateId,
        at: Pos,
        reference: Pos,
        random: &mut JavaRandom,
    ) -> bool {
        self.input.test(state, random)
            && self.location.test(world, random)
            && self.position.test(at, reference, random)
    }
}

#[derive(Debug)]
pub(crate) struct Processed {
    pub(crate) local: Pos,
    pub(crate) at: Pos,
    pub(crate) state: BlockStateId,
    pub(crate) nbt: Option<Value>,
}

fn limit_sample(limit: &Int, random: &mut JavaRandom) -> i32 {
    match limit {
        Int::Uniform(min, max) if max >= min => min + random.next_int(max - min + 1),
        other => other.max_inclusive(),
    }
}

impl Step {
    pub(crate) fn finalize(
        &self,
        run: &mut Run,
        position: Pos,
        reference: Pos,
        list: &mut [Processed],
    ) {
        let Self::Capped { limit, delegate } = self else {
            return;
        };
        if limit.max_inclusive() == 0 || list.is_empty() {
            return;
        }
        let mut base = JavaRandom::new(run.seed);
        let factory = base.next_long();
        let mut random = JavaRandom::new(position_seed(position) ^ factory);
        let cap = limit_sample(limit, &mut random).min(list.len() as i32);
        if cap < 1 {
            return;
        }
        let mut order: Vec<usize> = (0..list.len()).collect();
        for i in (2..=list.len()).rev() {
            let j = random.next_int(i as i32) as usize;
            order.swap(i - 1, j);
        }
        let mut changed = 0;
        for index in order {
            if changed >= cap {
                break;
            }
            let item = &mut list[index];
            let mut subject = Subject {
                local: item.local,
                at: item.at,
                reference,
                state: item.state,
                nbt: item.nbt.as_ref().map(Cow::Borrowed),
            };
            if !delegate.process(run, &mut subject) {
                continue;
            }
            let nbt = subject.nbt.map(Cow::into_owned);
            if subject.state != item.state || nbt != item.nbt {
                item.state = subject.state;
                item.nbt = nbt;
                changed += 1;
            }
        }
    }

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
            "block_ignore" => {
                value
                    .get("blocks")
                    .and_then(Value::as_array)
                    .map_or(Self::Nop, |blocks| {
                        Self::Ignore(BlockSet::compile(&Value::Array(
                            blocks
                                .iter()
                                .filter_map(|block| block.get("Name").cloned())
                                .collect(),
                        )))
                    })
            }
            "blackstone_replace" => Self::Blackstone,
            "block_age" => Self::BlockAge {
                mossiness: value
                    .get("mossiness")
                    .and_then(Value::as_f64)
                    .map_or(0.0, |n| n as f32),
            },
            "jigsaw_replacement" => Self::JigsawReplacement,
            "lava_submerged_block" => Self::LavaSubmerged,
            "capped" => match value.get("delegate") {
                Some(delegate) => Self::Capped {
                    limit: Int::compile(value.get("limit").unwrap_or(&Value::Null)),
                    delegate: Box::new(Self::compile(delegate)),
                },
                None => Self::Nop,
            },
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
            Self::Nop | Self::Capped { .. } => true,
            Self::IgnoreStructureBlock => subject.state.kind() != Some(BlockKind::StructureBlock),
            Self::IgnoreAir => !matches!(
                subject.state.kind(),
                Some(BlockKind::Air | BlockKind::StructureBlock)
            ),
            Self::Ignore(set) => !set.contains(subject.state),
            Self::BlockAge { mossiness } => {
                let mut random = JavaRandom::new(position_seed(subject.at));
                if let Some(aged) = age(subject.state, *mossiness, &mut random) {
                    subject.state = aged;
                }
                true
            }
            Self::LavaSubmerged => {
                let world = run.region.get(subject.at.0, subject.at.1, subject.at.2);
                if world.kind() == Some(BlockKind::Lava)
                    && !subject
                        .state
                        .flags()
                        .contains(rspigot_blocks::StateFlags::FULL_COLLISION_CUBE)
                {
                    subject.state = BlockKind::Lava.default_state();
                    subject.nbt = None;
                }
                true
            }
            Self::Blackstone => {
                if let Some(replaced) = blackstone(subject.state) {
                    subject.state = replaced;
                }
                true
            }
            Self::JigsawReplacement => {
                if subject.state.kind() != Some(BlockKind::Jigsaw) {
                    return true;
                }
                let name = subject
                    .nbt
                    .as_deref()
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
                    if rule.apply(
                        subject.state,
                        world,
                        subject.at,
                        subject.reference,
                        &mut random,
                    ) {
                        subject.state = rule.output;
                        let existing = subject.nbt.take().map(Cow::into_owned);
                        subject.nbt = rule
                            .modifier
                            .apply(&mut random, rule.output, existing)
                            .map(Cow::Owned);
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

fn named(name: &str) -> Option<BlockKind> {
    BlockKind::from_name(name)
}

fn with_properties_of(target: BlockKind, source: BlockStateId) -> BlockStateId {
    source
        .properties()
        .fold(target.default_state(), |state, (key, value)| {
            state.with_property(key, value).unwrap_or(state)
        })
}

fn random_stairs(kind: BlockKind, random: &mut JavaRandom) -> BlockStateId {
    let facing = ["north", "east", "south", "west"][random.next_int(4) as usize];
    let half = ["top", "bottom"][random.next_int(2) as usize];
    kind.state_with(&[("facing", facing), ("half", half)])
        .unwrap_or(kind.default_state())
}

fn pick(
    random: &mut JavaRandom,
    mossiness: f32,
    plain: &[BlockStateId],
    mossy: &[BlockStateId],
) -> BlockStateId {
    let set = if random.next_float() < mossiness {
        mossy
    } else {
        plain
    };
    set[random.next_int(set.len() as i32) as usize]
}

fn age(state: BlockStateId, mossiness: f32, random: &mut JavaRandom) -> Option<BlockStateId> {
    let kind = state.kind()?;
    let name = kind.name();
    if matches!(
        kind,
        BlockKind::StoneBricks | BlockKind::Stone | BlockKind::ChiseledStoneBricks
    ) {
        if random.next_float() >= 0.5 {
            return None;
        }
        let plain = [
            BlockKind::CrackedStoneBricks.default_state(),
            random_stairs(BlockKind::StoneBrickStairs, random),
        ];
        let mossy = [
            BlockKind::MossyStoneBricks.default_state(),
            random_stairs(BlockKind::MossyStoneBrickStairs, random),
        ];
        return Some(pick(random, mossiness, &plain, &mossy));
    }
    if name.ends_with("_stairs") {
        if random.next_float() >= 0.5 {
            return None;
        }
        let plain = [
            BlockKind::StoneSlab.default_state(),
            BlockKind::StoneBrickSlab.default_state(),
        ];
        let mossy = [
            with_properties_of(BlockKind::MossyStoneBrickStairs, state),
            BlockKind::MossyStoneBrickSlab.default_state(),
        ];
        return Some(pick(random, mossiness, &plain, &mossy));
    }
    if name.ends_with("_slab") {
        return (random.next_float() < mossiness)
            .then(|| with_properties_of(BlockKind::MossyStoneBrickSlab, state));
    }
    if name.ends_with("_wall") {
        return (random.next_float() < mossiness)
            .then(|| with_properties_of(BlockKind::MossyStoneBrickWall, state));
    }
    if kind == BlockKind::Obsidian {
        return (random.next_float() < 0.15).then(|| BlockKind::CryingObsidian.default_state());
    }
    None
}

const BLACKSTONE: [(&str, &str); 24] = [
    ("cobblestone", "blackstone"),
    ("mossy_cobblestone", "blackstone"),
    ("stone", "polished_blackstone"),
    ("stone_bricks", "polished_blackstone_bricks"),
    ("mossy_stone_bricks", "polished_blackstone_bricks"),
    ("cobblestone_stairs", "blackstone_stairs"),
    ("mossy_cobblestone_stairs", "blackstone_stairs"),
    ("stone_stairs", "polished_blackstone_stairs"),
    ("stone_brick_stairs", "polished_blackstone_brick_stairs"),
    (
        "mossy_stone_brick_stairs",
        "polished_blackstone_brick_stairs",
    ),
    ("cobblestone_slab", "blackstone_slab"),
    ("mossy_cobblestone_slab", "blackstone_slab"),
    ("smooth_stone_slab", "polished_blackstone_slab"),
    ("stone_slab", "polished_blackstone_slab"),
    ("stone_brick_slab", "polished_blackstone_brick_slab"),
    ("mossy_stone_brick_slab", "polished_blackstone_brick_slab"),
    ("stone_brick_wall", "polished_blackstone_brick_wall"),
    ("mossy_stone_brick_wall", "polished_blackstone_brick_wall"),
    ("cobblestone_wall", "blackstone_wall"),
    ("mossy_cobblestone_wall", "blackstone_wall"),
    ("chiseled_stone_bricks", "chiseled_polished_blackstone"),
    ("cracked_stone_bricks", "cracked_polished_blackstone_bricks"),
    ("iron_bars", "iron_chain"),
    ("stone_brick_stairs", "polished_blackstone_brick_stairs"),
];

fn blackstone(state: BlockStateId) -> Option<BlockStateId> {
    let name = state.kind()?.name();
    let name = name.strip_prefix("minecraft:").unwrap_or(name);
    let (_, target) = BLACKSTONE.iter().find(|(source, _)| *source == name)?;
    let kind = named(target)?;
    let mut result = kind.default_state();
    for key in ["facing", "half", "type"] {
        if let Some(value) = state.property(key) {
            result = result.with_property(key, value).unwrap_or(result);
        }
    }
    Some(result)
}

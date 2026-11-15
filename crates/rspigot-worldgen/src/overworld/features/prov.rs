use std::sync::Arc;

use rspigot_blocks::{BlockKind as Kind, Face, Support};
use rspigot_data::tags::tag;
use rspigot_domain::BlockStateId;
use serde_json::Value;

use super::super::region::Region;
use crate::climate::{NoiseStack, Xoroshiro};
use crate::datapack::{block_state, inline_noise_parameters};

pub(crate) type Pos = (i32, i32, i32);

pub(crate) fn short(value: &Value) -> &str {
    let text = value.get("type").and_then(Value::as_str).unwrap_or("");
    text.strip_prefix("minecraft:").unwrap_or(text)
}

pub(crate) fn number(value: &Value, key: &str, default: f64) -> f64 {
    value.get(key).and_then(Value::as_f64).unwrap_or(default)
}

pub(crate) fn integer(value: &Value, key: &str, default: i32) -> i32 {
    value
        .get(key)
        .and_then(Value::as_f64)
        .map_or(default, |n| n as i32)
}

pub(crate) fn flag(value: &Value, key: &str, default: bool) -> bool {
    value.get(key).and_then(Value::as_bool).unwrap_or(default)
}

pub(crate) fn text<'a>(value: &'a Value, key: &str) -> &'a str {
    value.get(key).and_then(Value::as_str).unwrap_or("")
}

#[derive(Debug, Clone)]
pub(crate) enum Int {
    Constant(i32),
    Uniform(i32, i32),
    BiasedToBottom(i32, i32),
    VeryBiasedToBottom(i32, i32),
    Clamped(Box<Int>, i32, i32),
    ClampedNormal {
        mean: f32,
        deviation: f32,
        min: i32,
        max: i32,
    },
    Weighted(Vec<(Int, i32)>),
    Trapezoid {
        min: i32,
        max: i32,
        plateau: i32,
    },
}

fn between(rng: &mut Xoroshiro, min: i32, max: i32) -> i32 {
    if min > max {
        min
    } else {
        min + rng.below(max - min + 1)
    }
}

impl Int {
    pub(crate) fn compile(value: &Value) -> Self {
        if let Some(n) = value.as_f64() {
            return Self::Constant(n as i32);
        }
        let min = integer(value, "min_inclusive", integer(value, "min", 0));
        let max = integer(value, "max_inclusive", integer(value, "max", 0));
        match short(value) {
            "constant" => Self::Constant(integer(value, "value", 0)),
            "uniform" => Self::Uniform(min, max),
            "biased_to_bottom" => Self::BiasedToBottom(min, max),
            "very_biased_to_bottom" => Self::VeryBiasedToBottom(min, max),
            "clamped" => Self::Clamped(
                Box::new(Self::compile(value.get("source").unwrap_or(&Value::Null))),
                min,
                max,
            ),
            "clamped_normal" => Self::ClampedNormal {
                mean: number(value, "mean", 0.0) as f32,
                deviation: number(value, "deviation", 1.0) as f32,
                min,
                max,
            },
            "weighted_list" => Self::Weighted(
                value
                    .get("distribution")
                    .and_then(Value::as_array)
                    .map(|entries| {
                        entries
                            .iter()
                            .map(|entry| {
                                (
                                    Self::compile(entry.get("data").unwrap_or(&Value::Null)),
                                    integer(entry, "weight", 1),
                                )
                            })
                            .collect()
                    })
                    .unwrap_or_default(),
            ),
            "trapezoid" => Self::Trapezoid {
                min,
                max,
                plateau: integer(value, "plateau", 0),
            },
            _ => Self::Constant(0),
        }
    }

    pub(crate) fn sample(&self, rng: &mut Xoroshiro) -> i32 {
        match self {
            Self::Constant(n) => *n,
            Self::Uniform(min, max) => between(rng, *min, *max),
            Self::BiasedToBottom(min, max) => {
                let bound = rng.below(max - min + 1) + 1;
                min + rng.below(bound)
            }
            Self::VeryBiasedToBottom(min, max) => {
                let a = rng.below(max - min + 1) + 1;
                let b = rng.below(a) + 1;
                min + rng.below(b)
            }
            Self::Clamped(source, min, max) => source.sample(rng).clamp(*min, *max),
            Self::ClampedNormal {
                mean,
                deviation,
                min,
                max,
            } => {
                let normal = *mean + rng.next_gaussian() as f32 * *deviation;
                normal.clamp(*min as f32, *max as f32) as i32
            }
            Self::Weighted(entries) => {
                let total: i32 = entries.iter().map(|(_, weight)| weight).sum();
                if total <= 0 {
                    return 0;
                }
                let mut roll = rng.below(total);
                for (provider, weight) in entries {
                    roll -= weight;
                    if roll < 0 {
                        return provider.sample(rng);
                    }
                }
                0
            }
            Self::Trapezoid { min, max, plateau } => {
                if *plateau == 0 && *max == -*min {
                    let first = rng.below(max + 1);
                    return first - rng.below(max + 1);
                }
                let span = max - min;
                if *plateau == span {
                    return between(rng, *min, *max);
                }
                let half = (span - plateau) / 2;
                let rest = span - half;
                let low = min + between(rng, 0, rest);
                low + between(rng, 0, half)
            }
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum Anchor {
    Absolute(i32),
    AboveBottom(i32),
    BelowTop(i32),
}

impl Anchor {
    pub(crate) fn compile(value: &Value) -> Self {
        if let Some(n) = value.get("absolute").and_then(Value::as_i64) {
            Self::Absolute(n as i32)
        } else if let Some(n) = value.get("above_bottom").and_then(Value::as_i64) {
            Self::AboveBottom(n as i32)
        } else {
            Self::BelowTop(value.get("below_top").and_then(Value::as_i64).unwrap_or(0) as i32)
        }
    }

    pub(crate) fn resolve(self, min_y: i32, max_y: i32) -> i32 {
        match self {
            Self::Absolute(n) => n,
            Self::AboveBottom(n) => min_y + n,
            Self::BelowTop(n) => max_y - n,
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) enum Height {
    Constant(Anchor),
    Uniform(Anchor, Anchor),
    BiasedToBottom(Anchor, Anchor, i32),
    VeryBiasedToBottom(Anchor, Anchor, i32),
    Trapezoid(Anchor, Anchor, i32),
}

impl Height {
    pub(crate) fn compile(value: &Value) -> Self {
        let anchor = |key: &str| Anchor::compile(value.get(key).unwrap_or(&Value::Null));
        let (min, max) = (anchor("min_inclusive"), anchor("max_inclusive"));
        match short(value) {
            "constant" => {
                Self::Constant(Anchor::compile(value.get("value").unwrap_or(&Value::Null)))
            }
            "biased_to_bottom" => Self::BiasedToBottom(min, max, integer(value, "inner", 1)),
            "very_biased_to_bottom" => {
                Self::VeryBiasedToBottom(min, max, integer(value, "inner", 1))
            }
            "trapezoid" => Self::Trapezoid(min, max, integer(value, "plateau", 0)),
            _ => Self::Uniform(min, max),
        }
    }

    pub(crate) fn sample(&self, rng: &mut Xoroshiro, min_y: i32, max_y: i32) -> i32 {
        let resolve = |anchor: Anchor| anchor.resolve(min_y, max_y);
        match self {
            Self::Constant(anchor) => resolve(*anchor),
            Self::Uniform(min, max) => between(rng, resolve(*min), resolve(*max)),
            Self::BiasedToBottom(min, max, inner) => {
                let (lo, hi) = (resolve(*min), resolve(*max));
                if hi - lo - inner < 0 {
                    return lo;
                }
                let k = rng.below(hi - lo - inner + 1);
                lo + rng.below(k + inner)
            }
            Self::VeryBiasedToBottom(min, max, inner) => {
                let (lo, hi) = (resolve(*min), resolve(*max));
                if hi - lo - inner < 0 {
                    return lo;
                }
                let k = between(rng, lo + inner, hi);
                let l = between(rng, lo, k - 1);
                between(rng, lo, l - 1 + inner)
            }
            Self::Trapezoid(min, max, plateau) => {
                let (lo, hi) = (resolve(*min), resolve(*max));
                if lo > hi {
                    return lo;
                }
                let span = hi - lo;
                if *plateau >= span {
                    return between(rng, lo, hi);
                }
                let half = (span - plateau) / 2;
                let rest = span - half;
                let low = lo + between(rng, 0, rest);
                low + between(rng, 0, half)
            }
        }
    }
}

#[derive(Debug, Clone, Default)]
pub(crate) struct BlockSet(Vec<u16>);

impl BlockSet {
    pub(crate) fn compile(value: &Value) -> Self {
        let mut ids = Vec::new();
        let mut add_name = |name: &str| {
            if let Some(kind) = Kind::from_name(name) {
                ids.push(kind.id().0);
            }
        };
        match value {
            Value::String(name) => {
                if let Some(tag_name) = name.strip_prefix('#') {
                    ids.extend(
                        tag("minecraft:block", tag_name)
                            .unwrap_or(&[])
                            .iter()
                            .map(|id| *id as u16),
                    );
                } else {
                    add_name(name);
                }
            }
            Value::Array(items) => {
                for item in items {
                    if let Some(name) = item.as_str() {
                        add_name(name);
                    }
                }
            }
            _ => {}
        }
        ids.sort_unstable();
        ids.dedup();
        Self(ids)
    }

    pub(crate) fn ids(&self) -> &[u16] {
        &self.0
    }

    pub(crate) fn tag(name: &str) -> Self {
        let mut ids: Vec<u16> = tag("minecraft:block", name)
            .unwrap_or(&[])
            .iter()
            .map(|id| *id as u16)
            .collect();
        ids.sort_unstable();
        Self(ids)
    }

    pub(crate) fn contains(&self, state: BlockStateId) -> bool {
        state
            .kind()
            .is_some_and(|kind| self.0.binary_search(&kind.id().0).is_ok())
    }
}

pub(crate) fn is_water(state: BlockStateId) -> bool {
    state.kind() == Some(Kind::Water) || state.property("waterlogged") == Some("true")
}

pub(crate) fn is_lava(state: BlockStateId) -> bool {
    state.kind() == Some(Kind::Lava)
}

pub(crate) fn is_fluid(state: BlockStateId) -> bool {
    is_water(state) || is_lava(state)
}

pub(crate) fn is_replaceable(state: BlockStateId) -> bool {
    use std::sync::OnceLock;
    static SET: OnceLock<BlockSet> = OnceLock::new();
    SET.get_or_init(|| BlockSet::tag("minecraft:replaceable"))
        .contains(state)
}

pub(crate) fn is_solid(state: BlockStateId) -> bool {
    !state.is_air() && !is_fluid(state) && !state.collision_shape().is_empty()
}

pub(crate) fn survives(region: &mut Region, state: BlockStateId, pos: Pos) -> bool {
    use std::sync::OnceLock;
    struct Sets {
        vegetation: BlockSet,
        saplings: BlockSet,
        flowers: BlockSet,
        dry: BlockSet,
        cactus: BlockSet,
        sugar_cane: BlockSet,
        bamboo: BlockSet,
        mushroom: BlockSet,
        lily: BlockSet,
        cannot_hold_snow: BlockSet,
        snow_override: BlockSet,
        coral_plants: BlockSet,
        wall_corals: BlockSet,
    }
    static SETS: OnceLock<Sets> = OnceLock::new();
    let sets = SETS.get_or_init(|| Sets {
        vegetation: BlockSet::tag("minecraft:supports_vegetation"),
        saplings: BlockSet::tag("minecraft:saplings"),
        flowers: BlockSet::tag("minecraft:flowers"),
        dry: BlockSet::tag("minecraft:supports_dry_vegetation"),
        cactus: BlockSet::tag("minecraft:supports_cactus"),
        sugar_cane: BlockSet::tag("minecraft:supports_sugar_cane"),
        bamboo: BlockSet::tag("minecraft:supports_bamboo"),
        mushroom: BlockSet::tag("minecraft:mushroom_grow_block"),
        lily: BlockSet::tag("minecraft:supports_lily_pad"),
        cannot_hold_snow: BlockSet::tag("minecraft:cannot_support_snow_layer"),
        snow_override: BlockSet::tag("minecraft:support_override_snow_layer"),
        coral_plants: BlockSet::tag("minecraft:coral_plants"),
        wall_corals: BlockSet::tag("minecraft:wall_corals"),
    });
    let Some(kind) = state.kind() else {
        return true;
    };
    let (x, y, z) = pos;
    let below = region.get(x, y - 1, z);
    let name = kind.name();
    if sets.saplings.contains(state) || sets.flowers.contains(state) {
        return sets.vegetation.contains(below);
    }
    if sets.coral_plants.contains(state) || name.ends_with("_coral_fan") {
        return below.is_face_sturdy(Face::Up, Support::Full);
    }
    if sets.wall_corals.contains(state) {
        let facing = state.property("facing").unwrap_or("north");
        let (dx, dz) = match facing {
            "north" => (0, 1),
            "south" => (0, -1),
            "west" => (1, 0),
            _ => (-1, 0),
        };
        return region
            .get(x + dx, y, z + dz)
            .is_face_sturdy(face(facing), Support::Full);
    }
    if name == "minecraft:sea_pickle" {
        return below.is_face_sturdy(Face::Up, Support::Full)
            || below
                .collision_shape()
                .iter()
                .any(|shape| shape.max_y >= 1.0);
    }
    match name {
        "minecraft:short_grass"
        | "minecraft:fern"
        | "minecraft:tall_grass"
        | "minecraft:large_fern"
        | "minecraft:bush"
        | "minecraft:sweet_berry_bush"
        | "minecraft:firefly_bush" => sets.vegetation.contains(below),
        "minecraft:dead_bush" | "minecraft:short_dry_grass" | "minecraft:tall_dry_grass" => {
            sets.dry.contains(below)
        }
        "minecraft:cactus" => {
            sets.cactus.contains(below)
                && [(1, 0), (-1, 0), (0, 1), (0, -1)]
                    .iter()
                    .all(|(dx, dz)| !is_solid(region.get(x + dx, y, z + dz)))
        }
        "minecraft:sugar_cane" => {
            below.kind() == Some(Kind::SugarCane)
                || (sets.sugar_cane.contains(below)
                    && [(1, 0), (-1, 0), (0, 1), (0, -1)]
                        .iter()
                        .any(|(dx, dz)| is_water(region.get(x + dx, y - 1, z + dz))))
        }
        "minecraft:bamboo" | "minecraft:bamboo_sapling" => sets.bamboo.contains(below),
        "minecraft:brown_mushroom" | "minecraft:red_mushroom" => {
            sets.mushroom.contains(below) || below.is_face_sturdy(Face::Up, Support::Full)
        }
        "minecraft:lily_pad" => sets.lily.contains(below) && region.get(x, y + 1, z).is_air(),
        "minecraft:snow" => {
            !sets.cannot_hold_snow.contains(below)
                && (sets.snow_override.contains(below)
                    || below.is_face_sturdy(Face::Up, Support::Full))
        }
        _ => true,
    }
}

#[derive(Debug, Clone)]
pub(crate) enum Pred {
    True,
    AllOf(Vec<Pred>),
    AnyOf(Vec<Pred>),
    Not(Box<Pred>),
    Blocks(BlockSet, Pos),
    Fluids {
        water: bool,
        lava: bool,
        offset: Pos,
    },
    Survive(BlockStateId, Pos),
    Sturdy(Face, Pos),
    Solid(Pos),
    Replaceable(Pos),
    InsideWorld(Pos),
}

fn offset(value: &Value) -> Pos {
    value
        .get("offset")
        .and_then(Value::as_array)
        .map_or((0, 0, 0), |items| {
            let at = |i: usize| items.get(i).and_then(Value::as_i64).unwrap_or(0) as i32;
            (at(0), at(1), at(2))
        })
}

fn face(name: &str) -> Face {
    match name {
        "down" => Face::Down,
        "north" => Face::North,
        "south" => Face::South,
        "west" => Face::West,
        "east" => Face::East,
        _ => Face::Up,
    }
}

impl Pred {
    pub(crate) fn compile(value: &Value) -> Self {
        let list = |key: &str| -> Vec<Pred> {
            value
                .get(key)
                .and_then(Value::as_array)
                .map(|items| items.iter().map(Self::compile).collect())
                .unwrap_or_default()
        };
        match short(value) {
            "all_of" => Self::AllOf(list("predicates")),
            "any_of" => Self::AnyOf(list("predicates")),
            "not" => Self::Not(Box::new(Self::compile(
                value.get("predicate").unwrap_or(&Value::Null),
            ))),
            "matching_blocks" => Self::Blocks(
                BlockSet::compile(value.get("blocks").unwrap_or(&Value::Null)),
                offset(value),
            ),
            "matching_block_tag" => Self::Blocks(
                BlockSet::tag(text(value, "tag").trim_start_matches("#")),
                offset(value),
            ),
            "matching_fluids" => {
                let names: Vec<&str> = match value.get("fluids") {
                    Some(Value::String(name)) => vec![name.as_str()],
                    Some(Value::Array(items)) => items.iter().filter_map(Value::as_str).collect(),
                    _ => Vec::new(),
                };
                Self::Fluids {
                    water: names.iter().any(|n| n.ends_with("water")),
                    lava: names.iter().any(|n| n.ends_with("lava")),
                    offset: offset(value),
                }
            }
            "would_survive" => Self::Survive(
                value
                    .get("state")
                    .and_then(block_state)
                    .unwrap_or(BlockStateId::AIR),
                offset(value),
            ),
            "has_sturdy_face" => Self::Sturdy(face(text(value, "direction")), offset(value)),
            "solid" => Self::Solid(offset(value)),
            "replaceable" => Self::Replaceable(offset(value)),
            "inside_world_bounds" => Self::InsideWorld(offset(value)),
            _ => Self::True,
        }
    }

    pub(crate) fn test(&self, region: &mut Region, pos: Pos) -> bool {
        match self {
            Self::True => true,
            Self::AllOf(items) => items.iter().all(|item| item.test(region, pos)),
            Self::AnyOf(items) => items.iter().any(|item| item.test(region, pos)),
            Self::Not(inner) => !inner.test(region, pos),
            Self::Blocks(set, off) => {
                set.contains(region.get(pos.0 + off.0, pos.1 + off.1, pos.2 + off.2))
            }
            Self::Fluids {
                water,
                lava,
                offset,
            } => {
                let state = region.get(pos.0 + offset.0, pos.1 + offset.1, pos.2 + offset.2);
                (*water && is_water(state)) || (*lava && is_lava(state))
            }
            Self::Survive(state, off) => survives(
                region,
                *state,
                (pos.0 + off.0, pos.1 + off.1, pos.2 + off.2),
            ),
            Self::Sturdy(direction, off) => region
                .get(pos.0 + off.0, pos.1 + off.1, pos.2 + off.2)
                .is_face_sturdy(*direction, Support::Full),
            Self::Solid(off) => is_solid(region.get(pos.0 + off.0, pos.1 + off.1, pos.2 + off.2)),
            Self::Replaceable(off) => {
                is_replaceable(region.get(pos.0 + off.0, pos.1 + off.1, pos.2 + off.2))
            }
            Self::InsideWorld(off) => region.height().contains(pos.1 + off.1),
        }
    }
}

const DIRECTIONS: [&str; 6] = ["down", "up", "north", "south", "west", "east"];

#[derive(Debug, Clone)]
pub(crate) enum State {
    Fixed(BlockStateId),
    Weighted(Vec<(BlockStateId, i32)>),
    RuleBased {
        fallback: Option<Box<State>>,
        rules: Vec<(Pred, State)>,
    },
    RandomizedInt {
        source: Box<State>,
        property: String,
        values: Int,
    },
    Pick(Vec<BlockStateId>),
    Rotated {
        source: Box<State>,
        direction: Option<&'static str>,
    },
    Noise(NoiseBlocks),
    DualNoise {
        slow: Arc<NoiseStack>,
        slow_scale: f32,
        min: i32,
        max: i32,
        fast: NoiseBlocks,
    },
    Threshold {
        noise: NoiseBlocks,
        threshold: f32,
        chance: f32,
        default: BlockStateId,
        low: Vec<BlockStateId>,
        high: Vec<BlockStateId>,
    },
}

#[derive(Debug, Clone)]
pub(crate) struct NoiseBlocks {
    noise: Arc<NoiseStack>,
    scale: f64,
    states: Vec<BlockStateId>,
}

impl NoiseBlocks {
    fn compile(value: &Value, states: Vec<BlockStateId>) -> Self {
        let parameters = inline_noise_parameters(value.get("noise").unwrap_or(&Value::Null));
        let seed = value.get("seed").and_then(Value::as_i64).unwrap_or(0);
        Self {
            noise: Arc::new(NoiseStack::create_legacy(&parameters, seed)),
            scale: f64::from(number(value, "scale", 1.0) as f32),
            states,
        }
    }

    fn value(&self, (x, y, z): Pos) -> f32 {
        self.noise.get(
            f64::from(x) * self.scale,
            f64::from(y) * self.scale,
            f64::from(z) * self.scale,
        )
    }
}

fn pick_by_noise(states: &[BlockStateId], noise: f32) -> Option<BlockStateId> {
    let unit = ((noise + 1.0) / 2.0).clamp(0.0, 0.9999_f32);
    states.get((unit * states.len() as f32) as usize).copied()
}

impl State {
    pub(crate) fn compile(value: &Value) -> Self {
        if let Some(provider) = value.as_str().and_then(|name| {
            crate::datapack::worldgen()
                .get("providers")
                .and_then(|providers| providers.get(name))
        }) {
            return Self::compile(provider);
        }
        let states = |key: &str| -> Vec<BlockStateId> {
            value
                .get(key)
                .and_then(Value::as_array)
                .map(|items| items.iter().filter_map(block_state).collect())
                .unwrap_or_default()
        };
        match short(value) {
            "noise" => Self::Noise(NoiseBlocks::compile(value, states("states"))),
            "dual_noise" => {
                let range = value.get("variety").and_then(Value::as_array);
                let bound = |index: usize| {
                    range
                        .and_then(|items| items.get(index))
                        .and_then(Value::as_i64)
                        .unwrap_or(1) as i32
                };
                let slow = inline_noise_parameters(value.get("slow_noise").unwrap_or(&Value::Null));
                let seed = value.get("seed").and_then(Value::as_i64).unwrap_or(0);
                Self::DualNoise {
                    slow: Arc::new(NoiseStack::create_legacy(&slow, seed)),
                    slow_scale: number(value, "slow_scale", 1.0) as f32,
                    min: bound(0),
                    max: bound(1),
                    fast: NoiseBlocks::compile(value, states("states")),
                }
            }
            "noise_threshold" => Self::Threshold {
                noise: NoiseBlocks::compile(value, Vec::new()),
                threshold: number(value, "threshold", 0.0) as f32,
                chance: number(value, "high_chance", 0.0) as f32,
                default: value
                    .get("default_state")
                    .and_then(block_state)
                    .unwrap_or(BlockStateId::AIR),
                low: states("low_states"),
                high: states("high_states"),
            },
            "random_block" => {
                let set = BlockSet::compile(value.get("blocks").unwrap_or(&Value::Null));
                Self::Pick(
                    set.ids()
                        .iter()
                        .filter_map(|id| Kind::from_id(rspigot_blocks::BlockId(*id)))
                        .map(|kind| kind.default_state())
                        .collect(),
                )
            }
            "rotated" => Self::Rotated {
                source: Box::new(Self::compile(value.get("state").unwrap_or(&Value::Null))),
                direction: DIRECTIONS
                    .iter()
                    .find(|name| Some(**name) == value.get("direction").and_then(Value::as_str))
                    .copied(),
            },
            "weighted" => Self::Weighted(
                value
                    .get("entries")
                    .and_then(Value::as_array)
                    .map(|entries| {
                        entries
                            .iter()
                            .filter_map(|entry| {
                                Some((
                                    block_state(entry.get("data")?)?,
                                    integer(entry, "weight", 1),
                                ))
                            })
                            .collect()
                    })
                    .unwrap_or_default(),
            ),
            "rule_based" => Self::RuleBased {
                fallback: value.get("fallback").map(|f| Box::new(Self::compile(f))),
                rules: value
                    .get("rules")
                    .and_then(Value::as_array)
                    .map(|rules| {
                        rules
                            .iter()
                            .map(|rule| {
                                (
                                    Pred::compile(rule.get("if_true").unwrap_or(&Value::Null)),
                                    Self::compile(rule.get("then").unwrap_or(&Value::Null)),
                                )
                            })
                            .collect()
                    })
                    .unwrap_or_default(),
            },
            "randomized_int" => Self::RandomizedInt {
                source: Box::new(Self::compile(value.get("source").unwrap_or(&Value::Null))),
                property: text(value, "property").to_owned(),
                values: Int::compile(value.get("values").unwrap_or(&Value::Null)),
            },
            _ => Self::Fixed(block_state(value).unwrap_or(BlockStateId::AIR)),
        }
    }

    pub(crate) fn resolve(
        &self,
        region: &mut Region,
        rng: &mut Xoroshiro,
        pos: Pos,
    ) -> Option<BlockStateId> {
        match self {
            Self::Fixed(state) => Some(*state),
            Self::Pick(options) => {
                if options.is_empty() {
                    None
                } else {
                    Some(options[rng.below(options.len() as i32) as usize])
                }
            }
            Self::Rotated { source, direction } => {
                let name = match direction {
                    Some(name) => name,
                    None => DIRECTIONS[rng.below(6) as usize],
                };
                let state = source.resolve(region, rng, pos)?;
                let axis = match name {
                    "up" | "down" => "y",
                    "north" | "south" => "z",
                    _ => "x",
                };
                let state = state.with_property("axis", axis).unwrap_or(state);
                Some(state.with_property("facing", name).unwrap_or(state))
            }
            Self::Noise(blocks) => pick_by_noise(&blocks.states, blocks.value(pos)),
            Self::DualNoise {
                slow,
                slow_scale,
                min,
                max,
                fast,
            } => {
                let slow_at = |(x, y, z): Pos| {
                    slow.get(
                        f64::from(x as f32 * slow_scale),
                        f64::from(y as f32 * slow_scale),
                        f64::from(z as f32 * slow_scale),
                    )
                };
                let unit = ((f64::from(slow_at(pos)) + 1.0) / 2.0).clamp(0.0, 1.0);
                let low = f64::from(*min);
                let count = (low + unit * (f64::from(*max + 1) - low)) as i32;
                let chosen: Vec<BlockStateId> = (0..count)
                    .filter_map(|i| {
                        let at = (pos.0 + i * 0xd511, pos.1, pos.2 + i * 0x85ba);
                        pick_by_noise(&fast.states, slow_at(at))
                    })
                    .collect();
                pick_by_noise(&chosen, fast.value(pos))
            }
            Self::Threshold {
                noise,
                threshold,
                chance,
                default,
                low,
                high,
            } => {
                if noise.value(pos) < *threshold {
                    return low.get(rng.below(low.len() as i32) as usize).copied();
                }
                if rng.next_float() < *chance {
                    return high.get(rng.below(high.len() as i32) as usize).copied();
                }
                Some(*default)
            }
            Self::Weighted(entries) => {
                let total: i32 = entries.iter().map(|(_, weight)| weight).sum();
                if total <= 0 {
                    return None;
                }
                let mut roll = rng.below(total);
                for (state, weight) in entries {
                    roll -= weight;
                    if roll < 0 {
                        return Some(*state);
                    }
                }
                None
            }
            Self::RuleBased { fallback, rules } => {
                for (condition, state) in rules {
                    if condition.test(region, pos) {
                        return state.resolve(region, rng, pos);
                    }
                }
                fallback.as_ref().and_then(|f| f.resolve(region, rng, pos))
            }
            Self::RandomizedInt {
                source,
                property,
                values,
            } => {
                let state = source.resolve(region, rng, pos)?;
                let value = values.sample(rng).to_string();
                state.with_property(property, &value).or(Some(state))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::overworld::features::tests::Bench;

    fn names(provider: &str, rng_seed: i64) -> Vec<String> {
        let bench = Bench::new();
        let mut region = bench.floor(crate::resources::BlockKind::Stone, 64);
        let state = State::compile(&Value::String(format!("minecraft:{provider}")));
        let mut rng = Xoroshiro::from_seed(rng_seed);
        (0..24)
            .map(|i| {
                let pos = (i * 37 - 400, 64 + i % 3, i * -53 + 100);
                state
                    .resolve(&mut region, &mut rng, pos)
                    .and_then(|s| s.kind())
                    .map_or("none".to_owned(), |kind| {
                        kind.name().replace("minecraft:", "")
                    })
            })
            .collect()
    }

    #[test]
    fn flower_forest_noise_matches_vanilla() {
        let expected = "red_tulip azure_bluet orange_tulip azure_bluet white_tulip white_tulip orange_tulip red_tulip white_tulip white_tulip white_tulip orange_tulip oxeye_daisy orange_tulip red_tulip red_tulip azure_bluet azure_bluet white_tulip white_tulip orange_tulip white_tulip red_tulip white_tulip";
        assert_eq!(names("flower_flower_forest", 1).join(" "), expected);
    }

    #[test]
    fn meadow_dual_noise_matches_vanilla() {
        let expected = "cornflower cornflower cornflower cornflower cornflower cornflower oxeye_daisy cornflower dandelion cornflower azure_bluet oxeye_daisy oxeye_daisy oxeye_daisy poppy oxeye_daisy cornflower cornflower cornflower cornflower cornflower cornflower cornflower dandelion";
        assert_eq!(names("flower_meadow", 1).join(" "), expected);
    }

    #[test]
    fn plains_flowers_are_dandelions_or_the_high_set() {
        let allowed = [
            "dandelion",
            "poppy",
            "azure_bluet",
            "oxeye_daisy",
            "cornflower",
        ];
        let found = names("flower_plain", 3);
        assert!(found.iter().all(|name| allowed.contains(&name.as_str())));
        assert!(found.iter().filter(|name| *name == "dandelion").count() > 12);
    }
}

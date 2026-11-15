use std::collections::HashMap;
use std::sync::Arc;

use rspigot_domain::BlockStateId;
use serde_json::Value;

use super::place::{Modifier, Placed, Run};
use super::prov::{BlockSet, Int, Pos, Pred, State, flag, integer, number, short, survives, text};
use crate::datapack::block_state;
use crate::overworld::region::Heightmap;

#[derive(Debug, Clone)]
pub(crate) enum RuleTest {
    Always,
    Blocks(BlockSet),
    Chance(BlockSet, f32),
    Height(i32, i32),
    AllOf(Vec<RuleTest>),
    AnyOf(Vec<RuleTest>),
    Not(Box<RuleTest>),
}

impl RuleTest {
    fn compile(value: &Value) -> Self {
        let list = |key: &str| -> Vec<RuleTest> {
            value
                .get(key)
                .and_then(Value::as_array)
                .map(|items| items.iter().map(Self::compile).collect())
                .unwrap_or_default()
        };
        let kind = text(value, "predicate_type");
        match kind.strip_prefix("minecraft:").unwrap_or(kind) {
            "tag_match" => Self::Blocks(BlockSet::tag(text(value, "tag"))),
            "block_match" => Self::Blocks(BlockSet::compile(
                value.get("block").unwrap_or(&Value::Null),
            )),
            "random_block_match" => Self::Chance(
                BlockSet::compile(value.get("block").unwrap_or(&Value::Null)),
                number(value, "probability", 1.0) as f32,
            ),
            "height_match" => Self::Height(
                integer(value, "min_inclusive", i32::MIN),
                integer(value, "max_inclusive", i32::MAX),
            ),
            "all_of" => Self::AllOf(list("rules")),
            "any_of" => Self::AnyOf(list("rules")),
            "not" => Self::Not(Box::new(Self::compile(
                value.get("rule").unwrap_or(&Value::Null),
            ))),
            _ => Self::Always,
        }
    }

    fn test(&self, state: BlockStateId, y: i32, rng: &mut crate::climate::Xoroshiro) -> bool {
        match self {
            Self::Always => true,
            Self::Blocks(set) => set.contains(state),
            Self::Chance(set, chance) => set.contains(state) && rng.next_float() < *chance,
            Self::Height(min, max) => (*min..=*max).contains(&y),
            Self::AllOf(items) => items.iter().all(|item| item.test(state, y, rng)),
            Self::AnyOf(items) => items.iter().any(|item| item.test(state, y, rng)),
            Self::Not(inner) => !inner.test(state, y, rng),
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct Ore {
    size: i32,
    discard: f32,
    targets: Vec<(RuleTest, BlockStateId)>,
    scattered: bool,
}

#[derive(Debug, Clone)]
pub(crate) struct Layer {
    height: Int,
    state: State,
}

#[derive(Debug, Clone)]
pub(crate) struct Column {
    allowed: Pred,
    up: bool,
    layers: Vec<Layer>,
    prioritize_tip: bool,
}

#[derive(Debug, Clone)]
pub(crate) struct Disk {
    half_height: i32,
    radius: Int,
    state: State,
    target: Pred,
}

#[derive(Debug, Clone)]
pub(crate) struct Spring {
    state: BlockStateId,
    valid: BlockSet,
    requires_below: bool,
    rocks: i32,
    holes: i32,
}

#[derive(Debug, Clone)]
pub(crate) enum Feature {
    NoOp,
    Simple {
        state: State,
    },
    Ore(Ore),
    Column(Column),
    Disk(Disk),
    Spring(Spring),
    Tree(Box<super::tree::Tree>),
    RandomSelector {
        entries: Vec<(f32, Arc<Placed>)>,
        default: Arc<Placed>,
    },
    Simple2(Vec<Arc<Placed>>),
    Weighted(Vec<(Arc<Placed>, i32)>),
    Boolean(Arc<Placed>, Arc<Placed>),
    Sequence(Vec<Arc<Placed>>),
    Unsupported,
}

pub(crate) struct Compiler<'a> {
    pub(crate) placed: &'a serde_json::Map<String, Value>,
    pub(crate) configured: &'a serde_json::Map<String, Value>,
    placed_memo: HashMap<String, Arc<Placed>>,
    configured_memo: HashMap<String, Arc<Feature>>,
}

impl<'a> Compiler<'a> {
    pub(crate) fn new(
        placed: &'a serde_json::Map<String, Value>,
        configured: &'a serde_json::Map<String, Value>,
    ) -> Self {
        Self {
            placed,
            configured,
            placed_memo: HashMap::new(),
            configured_memo: HashMap::new(),
        }
    }

    pub(crate) fn placed_ref(&mut self, value: &Value) -> Arc<Placed> {
        if let Some(name) = value.as_str() {
            if let Some(found) = self.placed_memo.get(name) {
                return found.clone();
            }
            let result = if let Some(definition) = self.placed.get(name) {
                let definition = definition.clone();
                self.placed_inline(&definition)
            } else {
                let feature = self.configured_ref(value);
                Arc::new(Placed::new(feature, Vec::new()))
            };
            self.placed_memo.insert(name.to_owned(), result.clone());
            return result;
        }
        self.placed_inline(value)
    }

    fn placed_inline(&mut self, value: &Value) -> Arc<Placed> {
        let feature = self.configured_ref(value.get("feature").unwrap_or(&Value::Null));
        let modifiers = value
            .get("placement")
            .and_then(Value::as_array)
            .map(|items| items.iter().map(Modifier::compile).collect())
            .unwrap_or_default();
        Arc::new(Placed::new(feature, modifiers))
    }

    pub(crate) fn configured_ref(&mut self, value: &Value) -> Arc<Feature> {
        if let Some(name) = value.as_str() {
            if let Some(found) = self.configured_memo.get(name) {
                return found.clone();
            }
            let result = match self.configured.get(name) {
                Some(definition) => {
                    let definition = definition.clone();
                    Arc::new(self.feature(&definition))
                }
                None => Arc::new(Feature::Unsupported),
            };
            self.configured_memo.insert(name.to_owned(), result.clone());
            return result;
        }
        Arc::new(self.feature(value))
    }

    fn placed_list(&mut self, value: Option<&Value>) -> Vec<Arc<Placed>> {
        value
            .and_then(Value::as_array)
            .map(|items| items.iter().map(|item| self.placed_ref(item)).collect())
            .unwrap_or_default()
    }

    fn feature(&mut self, value: &Value) -> Feature {
        let kind = short(value);
        match kind {
            "no_op" => Feature::NoOp,
            "simple_block" => Feature::Simple {
                state: State::compile(value.get("to_place").unwrap_or(&Value::Null)),
            },
            "ore" | "scattered_ore" => Feature::Ore(Ore {
                size: integer(value, "size", 1),
                discard: number(value, "discard_chance_on_air_exposure", 0.0) as f32,
                targets: value
                    .get("targets")
                    .and_then(Value::as_array)
                    .map(|targets| {
                        targets
                            .iter()
                            .filter_map(|target| {
                                Some((
                                    RuleTest::compile(target.get("target")?),
                                    block_state(target.get("state")?)?,
                                ))
                            })
                            .collect()
                    })
                    .unwrap_or_default(),
                scattered: kind == "scattered_ore",
            }),
            "block_column" => Feature::Column(Column {
                allowed: Pred::compile(value.get("allowed_placement").unwrap_or(&Value::Null)),
                up: text(value, "direction") != "down",
                layers: value
                    .get("layers")
                    .and_then(Value::as_array)
                    .map(|layers| {
                        layers
                            .iter()
                            .map(|layer| Layer {
                                height: Int::compile(layer.get("height").unwrap_or(&Value::Null)),
                                state: State::compile(
                                    layer.get("provider").unwrap_or(&Value::Null),
                                ),
                            })
                            .collect()
                    })
                    .unwrap_or_default(),
                prioritize_tip: flag(value, "prioritize_tip", false),
            }),
            "disk" => Feature::Disk(Disk {
                half_height: integer(value, "half_height", 0),
                radius: Int::compile(value.get("radius").unwrap_or(&Value::Null)),
                state: State::compile(value.get("state_provider").unwrap_or(&Value::Null)),
                target: Pred::compile(value.get("target").unwrap_or(&Value::Null)),
            }),
            "spring_feature" => Feature::Spring(Spring {
                state: value
                    .get("state")
                    .and_then(block_state)
                    .unwrap_or(BlockStateId::AIR),
                valid: BlockSet::compile(value.get("valid_blocks").unwrap_or(&Value::Null)),
                requires_below: flag(value, "requires_block_below", true),
                rocks: integer(value, "rock_count", 4),
                holes: integer(value, "hole_count", 1),
            }),
            "tree" => match super::tree::compile(value) {
                Some(tree) => Feature::Tree(Box::new(tree)),
                None => Feature::Unsupported,
            },
            "random_selector" => {
                let entries = value
                    .get("features")
                    .and_then(Value::as_array)
                    .map(|items| {
                        items
                            .iter()
                            .map(|item| {
                                (
                                    number(item, "chance", 0.0) as f32,
                                    self.placed_ref(item.get("feature").unwrap_or(&Value::Null)),
                                )
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                Feature::RandomSelector {
                    entries,
                    default: self.placed_ref(value.get("default").unwrap_or(&Value::Null)),
                }
            }
            "simple_random_selector" => {
                let features = value
                    .get("features")
                    .and_then(Value::as_array)
                    .map(|items| items.iter().map(|item| self.placed_ref(item)).collect())
                    .unwrap_or_default();
                Feature::Simple2(features)
            }
            "weighted_random_selector" => Feature::Weighted(
                value
                    .get("features")
                    .and_then(Value::as_array)
                    .map(|items| {
                        items
                            .iter()
                            .map(|item| {
                                (
                                    self.placed_ref(item.get("data").unwrap_or(&Value::Null)),
                                    integer(item, "weight", 1),
                                )
                            })
                            .collect()
                    })
                    .unwrap_or_default(),
            ),
            "random_boolean_selector" => Feature::Boolean(
                self.placed_ref(value.get("feature_true").unwrap_or(&Value::Null)),
                self.placed_ref(value.get("feature_false").unwrap_or(&Value::Null)),
            ),
            "sequence" => Feature::Sequence(self.placed_list(value.get("features"))),
            _ => Feature::Unsupported,
        }
    }
}

fn double_plant_upper(state: BlockStateId) -> Option<BlockStateId> {
    if state.property("half") == Some("lower") {
        state.with_property("half", "upper")
    } else {
        None
    }
}

impl Feature {
    pub(crate) fn reach(&self) -> i32 {
        match self {
            Self::NoOp | Self::Simple { .. } | Self::Column(_) | Self::Spring(_) => 1,
            Self::Ore(_) => 9,
            Self::Tree(_) => 12,
            Self::Disk(_) => 8,
            Self::RandomSelector { entries, default } => entries
                .iter()
                .map(|(_, placed)| placed.margin)
                .chain([default.margin])
                .max()
                .unwrap_or(1),
            Self::Simple2(items) | Self::Sequence(items) => {
                items.iter().map(|p| p.margin).max().unwrap_or(1)
            }
            Self::Weighted(items) => items.iter().map(|(p, _)| p.margin).max().unwrap_or(1),
            Self::Boolean(a, b) => a.margin.max(b.margin),
            Self::Unsupported => 16,
        }
    }

    pub(crate) fn place(&self, run: &mut Run, pos: Pos) -> bool {
        match self {
            Self::NoOp | Self::Unsupported => false,
            Self::Simple { state } => place_simple(state, run, pos),
            Self::Ore(ore) => place_ore(ore, run, pos),
            Self::Column(column) => place_column(column, run, pos),
            Self::Disk(disk) => place_disk(disk, run, pos),
            Self::Spring(spring) => place_spring(spring, run, pos),
            Self::Tree(tree) => tree.place(run, pos),
            Self::RandomSelector { entries, default } => {
                for (chance, placed) in entries {
                    if run.rng.next_float() < *chance {
                        return placed.place(run, pos);
                    }
                }
                default.place(run, pos)
            }
            Self::Simple2(items) => {
                if items.is_empty() {
                    return false;
                }
                let pick = run.rng.below(items.len() as i32) as usize;
                items[pick].place(run, pos)
            }
            Self::Weighted(items) => {
                let total: i32 = items.iter().map(|(_, weight)| weight).sum();
                if total <= 0 {
                    return false;
                }
                let mut roll = run.rng.below(total);
                for (placed, weight) in items {
                    roll -= weight;
                    if roll < 0 {
                        return placed.place(run, pos);
                    }
                }
                false
            }
            Self::Boolean(yes, no) => {
                if run.rng.next_boolean() {
                    yes.place(run, pos)
                } else {
                    no.place(run, pos)
                }
            }
            Self::Sequence(items) => {
                let mut any = false;
                for placed in items {
                    any |= placed.place(run, pos);
                }
                any
            }
        }
    }
}

fn place_simple(state: &State, run: &mut Run, pos: Pos) -> bool {
    let Some(resolved) = state.resolve(run.region, run.rng, pos) else {
        return false;
    };
    if !survives(run.region, resolved, pos) {
        return false;
    }
    if let Some(upper) = double_plant_upper(resolved) {
        if !run.region.get(pos.0, pos.1 + 1, pos.2).is_air() {
            return false;
        }
        run.region.set(pos.0, pos.1, pos.2, resolved);
        run.region.set(pos.0, pos.1 + 1, pos.2, upper);
        return true;
    }
    run.region.set(pos.0, pos.1, pos.2, resolved)
}

fn adjacent_to_air(run: &mut Run, (x, y, z): Pos) -> bool {
    [
        (1, 0, 0),
        (-1, 0, 0),
        (0, 1, 0),
        (0, -1, 0),
        (0, 0, 1),
        (0, 0, -1),
    ]
    .iter()
    .any(|(dx, dy, dz)| run.region.get(x + dx, y + dy, z + dz).is_air())
}

fn place_ore(ore: &Ore, run: &mut Run, (ox, oy, oz): Pos) -> bool {
    if ore.scattered {
        return place_scattered(ore, run, (ox, oy, oz));
    }
    let size = ore.size;
    let angle = run.rng.next_float() * std::f32::consts::PI;
    let spread = size as f32 / 8.0;
    let half = (((size as f32 / 16.0 * 2.0 + 1.0) / 2.0).ceil()) as i32;
    let (sin, cos) = (f64::from(angle).sin(), f64::from(angle).cos());
    let (x1, x2) = (
        f64::from(ox) + sin * f64::from(spread),
        f64::from(ox) - sin * f64::from(spread),
    );
    let (z1, z2) = (
        f64::from(oz) + cos * f64::from(spread),
        f64::from(oz) - cos * f64::from(spread),
    );
    let y1 = f64::from(oy + run.rng.below(3) - 2);
    let y2 = f64::from(oy + run.rng.below(3) - 2);
    let spread_ceil = spread.ceil() as i32;
    let x0 = ox - spread_ceil - half;
    let y0 = oy - 2 - half;
    let z0 = oz - spread_ceil - half;
    let width = 2 * (spread_ceil + half);
    let height = 2 * (2 + half);
    let mut reachable = false;
    'scan: for px in x0..=x0 + width {
        for pz in z0..=z0 + width {
            if y0 <= run.region.height_at(Heightmap::OceanFloor, px, pz) {
                reachable = true;
                break 'scan;
            }
        }
    }
    if !reachable {
        return false;
    }
    let count = size as usize;
    let mut blobs = vec![0.0f64; count * 4];
    for i in 0..count {
        let t = i as f32 / size as f32;
        let td = f64::from(t);
        let radius = run.rng.next_double() * f64::from(size) / 16.0;
        let scale = (f64::from(f32::sin(std::f32::consts::PI * t) + 1.0) * radius + 1.0) / 2.0;
        blobs[i * 4] = x1 + (x2 - x1) * td;
        blobs[i * 4 + 1] = y1 + (y2 - y1) * td;
        blobs[i * 4 + 2] = z1 + (z2 - z1) * td;
        blobs[i * 4 + 3] = scale;
    }
    for i in 0..count.saturating_sub(1) {
        if blobs[i * 4 + 3] <= 0.0 {
            continue;
        }
        for k in (i + 1)..count {
            if blobs[k * 4 + 3] <= 0.0 {
                continue;
            }
            let dr = blobs[i * 4 + 3] - blobs[k * 4 + 3];
            let dx = blobs[i * 4] - blobs[k * 4];
            let dy = blobs[i * 4 + 1] - blobs[k * 4 + 1];
            let dz = blobs[i * 4 + 2] - blobs[k * 4 + 2];
            if dr * dr > dx * dx + dy * dy + dz * dz {
                if dr > 0.0 {
                    blobs[k * 4 + 3] = -1.0;
                } else {
                    blobs[i * 4 + 3] = -1.0;
                }
            }
        }
    }
    let mut done = vec![false; (width.max(1) * height.max(1) * width.max(1)) as usize + 1];
    let mut placed = false;
    for i in 0..count {
        let radius = blobs[i * 4 + 3];
        if radius < 0.0 {
            continue;
        }
        let (cx, cy, cz) = (blobs[i * 4], blobs[i * 4 + 1], blobs[i * 4 + 2]);
        let bx0 = ((cx - radius).floor() as i32).max(x0);
        let by0 = ((cy - radius).floor() as i32).max(y0);
        let bz0 = ((cz - radius).floor() as i32).max(z0);
        let bx1 = ((cx + radius).floor() as i32).max(bx0);
        let by1 = ((cy + radius).floor() as i32).max(by0);
        let bz1 = ((cz + radius).floor() as i32).max(bz0);
        for px in bx0..=bx1 {
            let dx = (f64::from(px) + 0.5 - cx) / radius;
            if dx * dx >= 1.0 {
                continue;
            }
            for py in by0..=by1 {
                let dy = (f64::from(py) + 0.5 - cy) / radius;
                if dx * dx + dy * dy >= 1.0 {
                    continue;
                }
                for pz in bz0..=bz1 {
                    let dz = (f64::from(pz) + 0.5 - cz) / radius;
                    if dx * dx + dy * dy + dz * dz >= 1.0 || !run.region.height().contains(py) {
                        continue;
                    }
                    let slot = (px - x0) + (py - y0) * width + (pz - z0) * width * height;
                    let Some(flag) = usize::try_from(slot).ok().and_then(|s| done.get_mut(s))
                    else {
                        continue;
                    };
                    if *flag {
                        continue;
                    }
                    *flag = true;
                    placed |= try_ore(ore, run, (px, py, pz));
                }
            }
        }
    }
    placed
}

fn try_ore(ore: &Ore, run: &mut Run, pos: Pos) -> bool {
    let state = run.region.get(pos.0, pos.1, pos.2);
    for (rule, result) in &ore.targets {
        if !rule.test(state, pos.1, run.rng) {
            continue;
        }
        let skip = if ore.discard <= 0.0 {
            true
        } else if ore.discard >= 1.0 {
            false
        } else {
            run.rng.next_float() >= ore.discard
        };
        if skip || !adjacent_to_air(run, pos) {
            run.region.set(pos.0, pos.1, pos.2, *result);
            return true;
        }
    }
    false
}

fn place_scattered(ore: &Ore, run: &mut Run, (ox, oy, oz): Pos) -> bool {
    let count = run.rng.below(ore.size + 1);
    let mut placed = false;
    for _ in 0..count {
        let offset = |run: &mut Run| {
            let bound = count.min(7);
            let spread = (run.rng.next_float() - run.rng.next_float()) * bound as f32;
            (spread + 0.5).floor() as i32
        };
        let dx = offset(run);
        let dy = offset(run);
        let dz = offset(run);
        placed |= try_ore(ore, run, (ox + dx, oy + dy, oz + dz));
    }
    placed
}

fn place_column(column: &Column, run: &mut Run, (x, y, z): Pos) -> bool {
    let step = if column.up { 1 } else { -1 };
    let mut heights: Vec<i32> = column
        .layers
        .iter()
        .map(|layer| layer.height.sample(run.rng))
        .collect();
    let total: i32 = heights.iter().sum();
    if total == 0 {
        return false;
    }
    let mut at = y;
    for k in 0..total {
        if !column.allowed.test(run.region, (x, at, z)) {
            let mut remove = total - k;
            let order: Vec<usize> = if column.prioritize_tip {
                (0..heights.len()).collect()
            } else {
                (0..heights.len()).rev().collect()
            };
            for index in order {
                let cut = heights[index].min(remove);
                remove -= cut;
                heights[index] -= cut;
            }
            break;
        }
        at += step;
    }
    let mut at = y;
    for (layer, count) in column.layers.iter().zip(&heights) {
        for _ in 0..*count {
            if let Some(state) = layer.state.resolve(run.region, run.rng, (x, at, z)) {
                run.region.set(x, at, z, state);
            }
            at += step;
        }
    }
    true
}

fn place_disk(disk: &Disk, run: &mut Run, (x, y, z): Pos) -> bool {
    let radius = disk.radius.sample(run.rng);
    let top = y + disk.half_height;
    let bottom = y - disk.half_height - 1;
    let mut placed = false;
    for px in (x - radius)..=(x + radius) {
        for pz in (z - radius)..=(z + radius) {
            let (dx, dz) = (px - x, pz - z);
            if dx * dx + dz * dz > radius * radius {
                continue;
            }
            for py in ((bottom + 1)..=top).rev() {
                if disk.target.test(run.region, (px, py, pz))
                    && let Some(state) = disk.state.resolve(run.region, run.rng, (px, py, pz))
                {
                    run.region.set(px, py, pz, state);
                    placed = true;
                }
            }
        }
    }
    placed
}

fn place_spring(spring: &Spring, run: &mut Run, (x, y, z): Pos) -> bool {
    let valid = |state: BlockStateId| spring.valid.contains(state);
    if !valid(run.region.get(x, y + 1, z)) {
        return false;
    }
    if spring.requires_below && !valid(run.region.get(x, y - 1, z)) {
        return false;
    }
    let here = run.region.get(x, y, z);
    if !here.is_air() && !valid(here) {
        return false;
    }
    let sides = [(-1, 0, 0), (1, 0, 0), (0, 0, -1), (0, 0, 1), (0, -1, 0)];
    let mut rocks = 0;
    let mut holes = 0;
    for (dx, dy, dz) in sides {
        let state = run.region.get(x + dx, y + dy, z + dz);
        if valid(state) {
            rocks += 1;
        }
        if state.is_air() {
            holes += 1;
        }
    }
    if rocks == spring.rocks && holes == spring.holes {
        run.region.set(x, y, z, spring.state);
        return true;
    }
    false
}

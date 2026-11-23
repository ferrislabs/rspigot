use std::collections::BTreeMap;

use rand::Rng;
use rspigot_blocks::{BlockKind, Face, StateFlags};
use rspigot_domain::{BlockPos, BlockStateId, ChunkPos, Vec3};
use rspigot_entities::SoundSource;
use rustc_hash::FxHashSet;

use crate::campfire;
use crate::loot::{self, LootContext};
use crate::plugins::events::{BlockFromTo, FluidLevelChange};
use crate::plugins::steps::Continuation;
use crate::plugins::{BlockState, EventPayload, Lane, block_state_of, data_of, face_of};
use crate::ports::ClientLink;
use crate::server::Server;

const WATER_DELAY: u64 = 5;
const LAVA_DELAY: u64 = 30;
const FAST_LAVA_DELAY: u64 = 10;
const NO_SLOPE: i32 = 1000;
const FALLING_LEVEL: u8 = 8;
const FALLING_SIDE_AMOUNT: i32 = 7;
const SOURCES_TO_FILL: i32 = 2;
const SOURCES_TO_LEAK: i32 = 3;
const FIZZ_SOUND: &str = "minecraft:block.lava.extinguish";
const HORIZONTAL: [Face; 4] = [Face::North, Face::South, Face::West, Face::East];
const LAVA_REACTS: [Face; 5] = [Face::Down, Face::South, Face::North, Face::East, Face::West];
const NEVER_HOLDS: [&str; 7] = [
    "door",
    "sign",
    "ladder",
    "sugar_cane",
    "bubble_column",
    "portal",
    "end_gateway",
];

#[derive(Debug, Default)]
pub struct ScheduledFluids {
    due: BTreeMap<u64, Vec<BlockPos>>,
    pending: FxHashSet<BlockPos>,
}

impl ScheduledFluids {
    pub(crate) fn entries(&self) -> impl Iterator<Item = (u64, BlockPos)> + '_ {
        self.due
            .iter()
            .flat_map(|(&due, positions)| positions.iter().map(move |&pos| (due, pos)))
    }

    pub(crate) fn remove_chunk(&mut self, chunk: ChunkPos) {
        let pending = &mut self.pending;
        self.due.retain(|_, positions| {
            positions.retain(|pos| {
                let keep = pos.chunk() != chunk;
                if !keep {
                    pending.remove(pos);
                }
                keep
            });
            !positions.is_empty()
        });
    }

    pub(crate) fn restore(&mut self, due: u64, pos: BlockPos) {
        if self.pending.insert(pos) {
            self.due.entry(due).or_default().push(pos);
        }
    }
}

pub(crate) fn saved_fluid_id(state: BlockStateId) -> Option<&'static str> {
    let flow = Flow::of(state)?;
    Some(match (flow.liquid, flow.is_source()) {
        (Liquid::Water, true) => "minecraft:water",
        (Liquid::Water, false) => "minecraft:flowing_water",
        (Liquid::Lava, true) => "minecraft:lava",
        (Liquid::Lava, false) => "minecraft:flowing_lava",
    })
}

pub(crate) fn is_fluid_id(id: &str) -> bool {
    matches!(
        id,
        "minecraft:water" | "minecraft:flowing_water" | "minecraft:lava" | "minecraft:flowing_lava"
    )
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Liquid {
    Water,
    Lava,
}

impl Liquid {
    fn of(state: BlockStateId) -> Option<Self> {
        match state.kind()? {
            BlockKind::Water => Some(Self::Water),
            BlockKind::Lava => Some(Self::Lava),
            _ => None,
        }
    }

    const fn kind(self) -> BlockKind {
        match self {
            Self::Water => BlockKind::Water,
            Self::Lava => BlockKind::Lava,
        }
    }

    const fn delay(self, fast: bool) -> u64 {
        match (self, fast) {
            (Self::Water, _) => WATER_DELAY,
            (Self::Lava, false) => LAVA_DELAY,
            (Self::Lava, true) => FAST_LAVA_DELAY,
        }
    }

    const fn drop_off(self, fast: bool) -> i32 {
        match (self, fast) {
            (Self::Lava, false) => 2,
            _ => 1,
        }
    }

    const fn slope_distance(self, fast: bool) -> i32 {
        match (self, fast) {
            (Self::Lava, false) => 2,
            (Self::Lava, true) => 4,
            (Self::Water, _) => 4,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Flow {
    liquid: Liquid,
    level: u8,
}

impl Flow {
    fn of(state: BlockStateId) -> Option<Self> {
        let Some(liquid) = Liquid::of(state) else {
            return (state.property("waterlogged") == Some("true"))
                .then(|| Self::source(Liquid::Water));
        };
        let level = state.property("level")?.parse().ok()?;
        Some(Self { liquid, level })
    }

    const fn source(liquid: Liquid) -> Self {
        Self { liquid, level: 0 }
    }

    fn flowing(liquid: Liquid, amount: i32, falling: bool) -> Self {
        let base = (8 - amount.clamp(1, 7)) as u8;
        let level = if falling { FALLING_LEVEL } else { base };
        Self { liquid, level }
    }

    const fn is_source(self) -> bool {
        self.level == 0
    }

    const fn is_falling(self) -> bool {
        self.level >= FALLING_LEVEL
    }

    fn amount(self) -> i32 {
        if self.is_source() || self.is_falling() {
            8
        } else {
            8 - i32::from(self.level)
        }
    }

    fn state(self) -> BlockStateId {
        let default = self.liquid.kind().default_state();
        default
            .with_property("level", &self.level.to_string())
            .unwrap_or(default)
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct FlowStep {
    from: BlockPos,
    face: Face,
    target: BlockPos,
    flow: Flow,
    leak: Option<Flow>,
}

impl FlowStep {
    pub(crate) fn is_same(&self, other: &Self) -> bool {
        self.from == other.from && self.face == other.face
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct LevelStep {
    pos: BlockPos,
    before: BlockStateId,
    state: BlockStateId,
    flow: Option<Flow>,
}

impl LevelStep {
    pub(crate) fn is_same(&self, other: &Self) -> bool {
        self.pos == other.pos
    }
}

#[derive(Debug, Clone, Copy)]
enum Cell {
    Empty,
    Fluid(Flow),
    Solid,
}

fn step(pos: BlockPos, face: Face) -> BlockPos {
    let (dx, dy, dz) = face.offset();
    pos.offset(dx, dy, dz)
}

fn holds_fluid(state: BlockStateId) -> bool {
    if state.is_air() {
        return true;
    }
    if state.property("waterlogged").is_some() {
        return false;
    }
    let open =
        state.flags().contains(StateFlags::REPLACEABLE) || state.collision_shape().is_empty();
    open && !state
        .kind()
        .is_some_and(|kind| NEVER_HOLDS.iter().any(|word| kind.name().contains(word)))
}

impl<L: ClientLink> Server<L> {
    fn cell(&self, pos: BlockPos) -> Cell {
        let Some(state) = self.world.chunks.block(pos) else {
            return Cell::Solid;
        };
        if let Some(flow) = Flow::of(state) {
            Cell::Fluid(flow)
        } else if holds_fluid(state) {
            Cell::Empty
        } else {
            Cell::Solid
        }
    }

    fn schedule_fluid(&mut self, pos: BlockPos, delay: u64) {
        if self.fluids.pending.insert(pos) {
            let due = self.current_tick() + delay;
            self.fluids.due.entry(due).or_default().push(pos);
        }
    }

    pub(crate) fn fluid_changed(&mut self, pos: BlockPos) {
        let Some(flow) = self.world.chunks.block(pos).and_then(Flow::of) else {
            return;
        };
        let delay = if self.lava_reaction(pos, flow).is_some() {
            1
        } else {
            flow.liquid.delay(self.fast_lava())
        };
        self.schedule_fluid(pos, delay);
    }

    pub(crate) fn tick_fluids(&mut self) {
        let now = self.current_tick();
        let due: Vec<BlockPos> = {
            let later = self.fluids.due.split_off(&(now + 1));
            let ready = std::mem::replace(&mut self.fluids.due, later);
            ready.into_values().flatten().collect()
        };
        for pos in due {
            self.fluids.pending.remove(&pos);
            self.fluid_tick(pos);
        }
    }

    fn fluid_tick(&mut self, pos: BlockPos) {
        let Some(flow) = self.world.chunks.block(pos).and_then(Flow::of) else {
            return;
        };
        if self.solidify_lava(pos, flow) {
            return;
        }
        if !flow.is_source() {
            let Some(before) = self.world.chunks.block(pos) else {
                return;
            };
            match self.new_liquid(pos, flow.liquid) {
                None => {
                    self.ask_level_change(LevelStep {
                        pos,
                        before,
                        state: BlockStateId::AIR,
                        flow: None,
                    });
                    return;
                }
                Some(new) if new != flow => {
                    self.ask_level_change(LevelStep {
                        pos,
                        before,
                        state: new.state(),
                        flow: Some(new),
                    });
                    return;
                }
                Some(_) => {}
            }
        }
        self.spread(pos, flow);
    }

    fn ask_level_change(&mut self, step: LevelStep) {
        let LevelStep { pos, state, .. } = step;
        self.fire(
            Lane::Block(pos),
            Continuation::FluidLevel(step),
            move |_| {
                Some(EventPayload::FluidLevelChange(FluidLevelChange {
                    block: pos,
                    new_data: data_of(state),
                }))
            },
        );
    }

    pub(crate) fn finish_level_change(
        &mut self,
        step: LevelStep,
        cancelled: bool,
        patched: Option<BlockState>,
    ) {
        if cancelled || self.world.chunks.block(step.pos) != Some(step.before) {
            return;
        }
        let state = patched
            .as_ref()
            .and_then(block_state_of)
            .unwrap_or(step.state);
        self.set_block(step.pos, state);
        if let Some(flow) = step.flow {
            self.schedule_fluid(step.pos, flow.liquid.delay(self.fast_lava()));
            self.spread(step.pos, flow);
        }
    }

    fn lava_reaction(&self, pos: BlockPos, flow: Flow) -> Option<BlockKind> {
        if flow.liquid != Liquid::Lava {
            return None;
        }
        let on_soul_soil = self
            .world
            .chunks
            .block(step(pos, Face::Down))
            .and_then(BlockStateId::kind)
            == Some(BlockKind::SoulSoil);
        LAVA_REACTS.into_iter().find_map(|face| {
            let next = step(pos, face);
            if matches!(self.cell(next), Cell::Fluid(other) if other.liquid == Liquid::Water) {
                Some(if flow.is_source() {
                    BlockKind::Obsidian
                } else {
                    BlockKind::Cobblestone
                })
            } else if on_soul_soil
                && self.world.chunks.block(next).and_then(BlockStateId::kind)
                    == Some(BlockKind::BlueIce)
            {
                Some(BlockKind::Basalt)
            } else {
                None
            }
        })
    }

    fn solidify_lava(&mut self, pos: BlockPos, flow: Flow) -> bool {
        let Some(result) = self.lava_reaction(pos, flow) else {
            return false;
        };
        self.set_block(pos, result.default_state());
        self.fizz(pos);
        true
    }

    fn fizz(&mut self, pos: BlockPos) {
        let pitch = 2.6 + (self.rng.random::<f32>() - self.rng.random::<f32>()) * 0.8;
        let at = Vec3::new(
            f64::from(pos.x) + 0.5,
            f64::from(pos.y) + 0.5,
            f64::from(pos.z) + 0.5,
        );
        self.play_sound(None, at, FIZZ_SOUND, SoundSource::Blocks, 0.5, pitch);
    }

    fn new_liquid(&self, pos: BlockPos, liquid: Liquid) -> Option<Flow> {
        let mut highest = 0;
        let mut sources = 0;
        for face in HORIZONTAL {
            if let Cell::Fluid(side) = self.cell(step(pos, face))
                && side.liquid == liquid
            {
                sources += i32::from(side.is_source());
                highest = highest.max(side.amount());
            }
        }
        let converts = match liquid {
            Liquid::Water => self.config.rules.water_source_conversion,
            Liquid::Lava => self.config.rules.lava_source_conversion,
        };
        if converts && sources >= SOURCES_TO_FILL {
            let supported = match self.cell(step(pos, Face::Down)) {
                Cell::Solid => true,
                Cell::Fluid(below) => below.liquid == liquid && below.is_source(),
                Cell::Empty => false,
            };
            if supported {
                return Some(Flow::source(liquid));
            }
        }
        if matches!(self.cell(step(pos, Face::Up)), Cell::Fluid(above) if above.liquid == liquid) {
            return Some(Flow::flowing(liquid, 8, true));
        }
        let amount = highest - liquid.drop_off(self.fast_lava());
        (amount > 0).then(|| Flow::flowing(liquid, amount, false))
    }

    fn spread(&mut self, pos: BlockPos, flow: Flow) {
        let liquid = flow.liquid;
        let below = step(pos, Face::Down);
        if self.can_spread_to(liquid, below, Face::Down) {
            let leak = (self.source_neighbours(pos, liquid) >= SOURCES_TO_LEAK).then_some(flow);
            self.ask_flow(FlowStep {
                from: pos,
                face: Face::Down,
                target: below,
                flow: Flow::flowing(liquid, 8, true),
                leak,
            });
        } else if flow.is_source() || !self.is_hole(below) {
            self.spread_to_sides(pos, flow);
        }
    }

    fn spread_to_sides(&mut self, pos: BlockPos, flow: Flow) {
        let liquid = flow.liquid;
        let amount = if flow.is_falling() {
            FALLING_SIDE_AMOUNT
        } else {
            flow.amount() - liquid.drop_off(self.fast_lava())
        };
        if amount <= 0 {
            return;
        }
        for face in self.spread_faces(pos, liquid) {
            let target = step(pos, face);
            if self.can_spread_to(liquid, target, face) {
                self.ask_flow(FlowStep {
                    from: pos,
                    face,
                    target,
                    flow: Flow::flowing(liquid, amount, false),
                    leak: None,
                });
            }
        }
    }

    fn ask_flow(&mut self, step: FlowStep) {
        let FlowStep {
            from, face, target, ..
        } = step;
        self.fire(Lane::Block(from), Continuation::Flow(step), move |_| {
            Some(EventPayload::BlockFromTo(BlockFromTo {
                block: from,
                face: face_of(face),
                to_block: target,
            }))
        });
    }

    pub(crate) fn finish_flow(&mut self, step: FlowStep, cancelled: bool) {
        if cancelled || !self.can_spread_to(step.flow.liquid, step.target, step.face) {
            return;
        }
        self.spread_to(step.target, step.flow, step.face);
        if let Some(source) = step.leak {
            self.spread_to_sides(step.from, source);
        }
    }

    fn spread_to(&mut self, target: BlockPos, flow: Flow, face: Face) {
        let Some(state) = self.world.chunks.block(target) else {
            return;
        };
        if flow.liquid == Liquid::Water
            && let Some(wet) = state
                .with_property("waterlogged", "true")
                .filter(|_| crate::bucket::can_waterlog(state))
        {
            self.set_block(target, wet);
            return;
        }
        if let Some(existing) = Flow::of(state) {
            if existing.liquid == Liquid::Water && flow.liquid == Liquid::Lava && face == Face::Down
            {
                self.set_block(target, BlockKind::Stone.default_state());
                self.fizz(target);
            }
            return;
        }
        if !state.is_air() {
            let context = LootContext::block(state, None, None);
            for stack in loot::block_drops(&context, &mut self.rng) {
                self.spawn_item(target, stack);
            }
        }
        self.set_block(target, flow.state());
        self.schedule_fluid(target, flow.liquid.delay(self.fast_lava()));
    }

    fn can_spread_to(&self, liquid: Liquid, target: BlockPos, face: Face) -> bool {
        match self.cell(target) {
            Cell::Empty => true,
            Cell::Fluid(existing) => {
                face == Face::Down && existing.liquid == Liquid::Water && liquid == Liquid::Lava
            }
            Cell::Solid => liquid == Liquid::Water && self.takes_water(target),
        }
    }

    fn takes_water(&self, pos: BlockPos) -> bool {
        self.world.chunks.block(pos).is_some_and(|state| {
            campfire::can_waterlog(state) || crate::bucket::can_waterlog(state)
        })
    }

    fn can_pass(&self, liquid: Liquid, pos: BlockPos) -> bool {
        match self.cell(pos) {
            Cell::Empty => true,
            Cell::Fluid(other) => !(other.liquid == liquid && other.is_source()),
            Cell::Solid => liquid == Liquid::Water && self.takes_water(pos),
        }
    }

    fn is_hole(&self, below: BlockPos) -> bool {
        !matches!(self.cell(below), Cell::Solid)
    }

    fn source_neighbours(&self, pos: BlockPos, liquid: Liquid) -> i32 {
        HORIZONTAL
            .into_iter()
            .filter(|face| {
                matches!(self.cell(step(pos, *face)),
                    Cell::Fluid(side) if side.liquid == liquid && side.is_source())
            })
            .count() as i32
    }

    fn spread_faces(&self, pos: BlockPos, liquid: Liquid) -> Vec<Face> {
        let mut best = NO_SLOPE;
        let mut faces = Vec::new();
        for face in HORIZONTAL {
            let next = step(pos, face);
            if !self.can_pass(liquid, next) {
                continue;
            }
            let distance = if self.is_hole(step(next, Face::Down)) {
                0
            } else {
                self.slope_distance(liquid, next, 1, face.opposite())
            };
            if distance < best {
                faces.clear();
            }
            if distance <= best {
                faces.push(face);
                best = distance;
            }
        }
        faces
    }

    fn slope_distance(&self, liquid: Liquid, pos: BlockPos, depth: i32, from: Face) -> i32 {
        let mut best = NO_SLOPE;
        for face in HORIZONTAL {
            if face == from {
                continue;
            }
            let next = step(pos, face);
            if !self.can_pass(liquid, next) {
                continue;
            }
            if self.is_hole(step(next, Face::Down)) {
                return depth;
            }
            if depth < liquid.slope_distance(self.fast_lava()) {
                best = best.min(self.slope_distance(liquid, next, depth + 1, face.opposite()));
            }
        }
        best
    }
}

#[cfg(test)]
mod fast_lava_tests {
    use super::*;

    #[test]
    fn lava_in_a_fast_lava_dimension_ticks_every_ten_loses_one_and_slopes_four() {
        assert_eq!(Liquid::Lava.delay(false), 30);
        assert_eq!(Liquid::Lava.delay(true), 10);
        assert_eq!(Liquid::Lava.drop_off(false), 2);
        assert_eq!(Liquid::Lava.drop_off(true), 1);
        assert_eq!(Liquid::Lava.slope_distance(false), 2);
        assert_eq!(Liquid::Lava.slope_distance(true), 4);
        assert_eq!(Liquid::Water.delay(true), 5);
        assert_eq!(Liquid::Water.drop_off(true), 1);
    }
}

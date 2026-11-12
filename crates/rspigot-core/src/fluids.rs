use std::collections::BTreeMap;

use rand::Rng;
use rspigot_blocks::{BlockKind, Face, StateFlags};
use rspigot_domain::{BlockPos, BlockStateId, Vec3};
use rspigot_entities::SoundSource;
use rustc_hash::FxHashSet;

use crate::loot::{self, LootContext};
use crate::ports::ClientLink;
use crate::server::Server;

const WATER_DELAY: u64 = 5;
const LAVA_DELAY: u64 = 30;
const NO_SLOPE: i32 = 1000;
const FALLING_LEVEL: u8 = 8;
const FALLING_SIDE_AMOUNT: i32 = 7;
const SOURCES_TO_FILL: i32 = 2;
const SOURCES_TO_LEAK: i32 = 3;
const FIZZ_SOUND: &str = "minecraft:block.lava.extinguish";
const HORIZONTAL: [Face; 4] = [Face::North, Face::South, Face::West, Face::East];
const LAVA_REACTS: [Face; 5] = [Face::Down, Face::North, Face::South, Face::West, Face::East];
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Liquid {
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

    const fn delay(self) -> u64 {
        match self {
            Self::Water => WATER_DELAY,
            Self::Lava => LAVA_DELAY,
        }
    }

    const fn drop_off(self) -> i32 {
        match self {
            Self::Water => 1,
            Self::Lava => 2,
        }
    }

    const fn slope_distance(self) -> i32 {
        match self {
            Self::Water => 4,
            Self::Lava => 2,
        }
    }

    const fn makes_sources(self) -> bool {
        matches!(self, Self::Water)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Flow {
    liquid: Liquid,
    level: u8,
}

impl Flow {
    fn of(state: BlockStateId) -> Option<Self> {
        let liquid = Liquid::of(state)?;
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
        let delay = if self.lava_meets_water(pos, flow) {
            1
        } else {
            flow.liquid.delay()
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
        let Some(mut flow) = self.world.chunks.block(pos).and_then(Flow::of) else {
            return;
        };
        if self.solidify_lava(pos, flow) {
            return;
        }
        if !flow.is_source() {
            match self.new_liquid(pos, flow.liquid) {
                None => {
                    self.set_block(pos, BlockStateId::AIR);
                    return;
                }
                Some(new) if new != flow => {
                    self.set_block(pos, new.state());
                    self.schedule_fluid(pos, new.liquid.delay());
                    flow = new;
                }
                Some(_) => {}
            }
        }
        self.spread(pos, flow);
    }

    fn lava_meets_water(&self, pos: BlockPos, flow: Flow) -> bool {
        flow.liquid == Liquid::Lava
            && LAVA_REACTS.into_iter().any(|face| {
                matches!(self.cell(step(pos, face)), Cell::Fluid(other) if other.liquid == Liquid::Water)
            })
    }

    fn solidify_lava(&mut self, pos: BlockPos, flow: Flow) -> bool {
        if !self.lava_meets_water(pos, flow) {
            return false;
        }
        let result = if flow.is_source() {
            BlockKind::Obsidian
        } else {
            BlockKind::Cobblestone
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
        if liquid.makes_sources() && sources >= SOURCES_TO_FILL {
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
        let amount = highest - liquid.drop_off();
        (amount > 0).then(|| Flow::flowing(liquid, amount, false))
    }

    fn spread(&mut self, pos: BlockPos, flow: Flow) {
        let liquid = flow.liquid;
        let below = step(pos, Face::Down);
        if self.can_spread_to(liquid, below, Face::Down) {
            self.spread_to(below, Flow::flowing(liquid, 8, true), Face::Down);
            if self.source_neighbours(pos, liquid) >= SOURCES_TO_LEAK {
                self.spread_to_sides(pos, flow);
            }
        } else if flow.is_source() || !self.is_hole(below) {
            self.spread_to_sides(pos, flow);
        }
    }

    fn spread_to_sides(&mut self, pos: BlockPos, flow: Flow) {
        let liquid = flow.liquid;
        let amount = if flow.is_falling() {
            FALLING_SIDE_AMOUNT
        } else {
            flow.amount() - liquid.drop_off()
        };
        if amount <= 0 {
            return;
        }
        for face in self.spread_faces(pos, liquid) {
            let target = step(pos, face);
            if self.can_spread_to(liquid, target, face) {
                self.spread_to(target, Flow::flowing(liquid, amount, false), face);
            }
        }
    }

    fn spread_to(&mut self, target: BlockPos, flow: Flow, face: Face) {
        let Some(state) = self.world.chunks.block(target) else {
            return;
        };
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
        self.schedule_fluid(target, flow.liquid.delay());
    }

    fn can_spread_to(&self, liquid: Liquid, target: BlockPos, face: Face) -> bool {
        match self.cell(target) {
            Cell::Empty => true,
            Cell::Fluid(existing) => {
                face == Face::Down && existing.liquid == Liquid::Water && liquid == Liquid::Lava
            }
            Cell::Solid => false,
        }
    }

    fn can_pass(&self, liquid: Liquid, pos: BlockPos) -> bool {
        match self.cell(pos) {
            Cell::Empty => true,
            Cell::Fluid(other) => !(other.liquid == liquid && other.is_source()),
            Cell::Solid => false,
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
            if depth < liquid.slope_distance() {
                best = best.min(self.slope_distance(liquid, next, depth + 1, face.opposite()));
            }
        }
        best
    }
}

use rand::Rng;
use rspigot_blocks::{BlockKind, Face, StateFlags};
use rspigot_domain::player::PlayerId;
use rspigot_domain::{BlockPos, BlockStateId, Rotation, Vec3};

use crate::plugins::events::{BlockFertilize, StructureGrow, TreeType};
use crate::plugins::steps::{FertilizeStep, StructureStep};
use crate::plugins::{Continuation, EventPayload, Lane, Location, snapshot_of};
use crate::ports::ClientLink;
use crate::server::Server;

const SAPLING_ODDS: f32 = 7.0 / 700.0;
const SAPLING_LIGHT: i32 = 9;
const BONE_MEAL_SAPLING_CHANCE: f32 = 0.45;
const MAX_LEAF_DISTANCE: i32 = 7;
const GRASS_ATTEMPTS: i32 = 128;
const GRASS_RANGE_DIVISOR: i32 = 16;
const TALL_GRASS_ODDS: i32 = 10;
const CONIFER_RADII: [i32; 10] = [0, 1, 2, 1, 2, 3, 2, 3, 3, 3];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Species {
    Oak,
    Birch,
    Spruce,
}

impl Species {
    fn of(kind: BlockKind) -> Option<Self> {
        match kind {
            BlockKind::OakSapling => Some(Self::Oak),
            BlockKind::BirchSapling => Some(Self::Birch),
            BlockKind::SpruceSapling => Some(Self::Spruce),
            _ => None,
        }
    }

    const fn log(self) -> BlockKind {
        match self {
            Self::Oak => BlockKind::OakLog,
            Self::Birch => BlockKind::BirchLog,
            Self::Spruce => BlockKind::SpruceLog,
        }
    }

    const fn leaves(self) -> BlockKind {
        match self {
            Self::Oak => BlockKind::OakLeaves,
            Self::Birch => BlockKind::BirchLeaves,
            Self::Spruce => BlockKind::SpruceLeaves,
        }
    }

    const fn tree_type(self) -> TreeType {
        match self {
            Self::Oak => TreeType::Tree,
            Self::Birch => TreeType::Birch,
            Self::Spruce => TreeType::Redwood,
        }
    }
}

fn at(pos: BlockPos, face: Face) -> BlockPos {
    let (dx, dy, dz) = face.offset();
    pos.offset(dx, dy, dz)
}

fn tagged(state: BlockStateId, tag: &str) -> bool {
    state.kind().is_some_and(|kind| {
        rspigot_data::tags::tag("minecraft:block", tag)
            .is_some_and(|ids| ids.binary_search(&u32::from(kind.id().0)).is_ok())
    })
}

pub(crate) fn is_sapling(state: BlockStateId) -> bool {
    state.property("stage").is_some() && tagged(state, "minecraft:saplings")
}

fn stage_of(state: BlockStateId) -> i32 {
    state
        .property("stage")
        .and_then(|stage| stage.parse().ok())
        .unwrap_or(0)
}

struct Crown {
    logs: Vec<BlockPos>,
    leaves: Vec<BlockPos>,
}

fn leaf_distances(crown: &Crown) -> Vec<i32> {
    let mut distances = vec![MAX_LEAF_DISTANCE; crown.leaves.len()];
    for _ in 0..MAX_LEAF_DISTANCE {
        for index in 0..crown.leaves.len() {
            let leaf = crown.leaves[index];
            let mut best = distances[index];
            for face in Face::ALL {
                let neighbour = at(leaf, face);
                if crown.logs.contains(&neighbour) {
                    best = best.min(1);
                } else if let Some(other) = crown.leaves.iter().position(|spot| *spot == neighbour)
                {
                    best = best.min(distances[other] + 1);
                }
            }
            distances[index] = best;
        }
    }
    distances
}

impl<L: ClientLink> Server<L> {
    fn tree_crown(&mut self, species: Species, origin: BlockPos) -> Crown {
        let trunk = match species {
            Species::Oak => 4 + self.rng.random_range(0..3),
            Species::Birch => 5 + self.rng.random_range(0..3),
            Species::Spruce => 6 + self.rng.random_range(0..4),
        };
        let top = origin.y + trunk - 1;
        let logs: Vec<BlockPos> = (origin.y..=top)
            .map(|y| BlockPos::new(origin.x, y, origin.z))
            .collect();
        let mut leaves = Vec::new();
        match species {
            Species::Oak | Species::Birch => {
                for dy in -2..=1i32 {
                    let radius: i32 = if dy <= -1 { 2 } else { 1 };
                    for dz in -radius..=radius {
                        for dx in -radius..=radius {
                            let corner = dx.abs() == radius && dz.abs() == radius;
                            if corner && (dy == 1 || (dy < 0 && self.rng.random_range(0..2) == 0)) {
                                continue;
                            }
                            if dx == 0 && dz == 0 && dy <= 0 {
                                continue;
                            }
                            leaves.push(BlockPos::new(origin.x + dx, top + dy, origin.z + dz));
                        }
                    }
                }
            }
            Species::Spruce => {
                for (level, y) in (origin.y + 2..=top + 1).rev().enumerate() {
                    let radius = CONIFER_RADII[level.min(CONIFER_RADII.len() - 1)];
                    let reach = radius + i32::from(radius >= 2);
                    for dz in -radius..=radius {
                        for dx in -radius..=radius {
                            if dx.abs() + dz.abs() > reach || (dx == 0 && dz == 0 && y <= top) {
                                continue;
                            }
                            leaves.push(BlockPos::new(origin.x + dx, y, origin.z + dz));
                        }
                    }
                }
            }
        }
        Crown { logs, leaves }
    }

    fn free_for_tree(&self, pos: BlockPos) -> bool {
        let state = self.block_or_air(pos);
        state.is_air() || tagged(state, "minecraft:replaceable_by_trees")
    }

    fn plan_tree(
        &mut self,
        origin: BlockPos,
        sapling: BlockStateId,
    ) -> Option<(Species, Vec<(BlockPos, BlockStateId)>)> {
        let species = Species::of(sapling.kind()?)?;
        let crown = self.tree_crown(species, origin);
        let top = crown.logs.last()?.y;
        if top + 1 >= self.world.dimension.height.max_y() {
            return None;
        }
        let blocked = crown
            .logs
            .iter()
            .any(|pos| *pos != origin && !self.free_for_tree(*pos));
        if blocked {
            return None;
        }
        let mut blocks: Vec<(BlockPos, BlockStateId)> = Vec::new();
        let below = at(origin, Face::Down);
        if matches!(
            self.block_or_air(below).kind(),
            Some(BlockKind::GrassBlock | BlockKind::Mycelium)
        ) {
            blocks.push((below, BlockKind::Dirt.default_state()));
        }
        let log = species.log().default_state();
        blocks.extend(crown.logs.iter().map(|pos| (*pos, log)));
        let distances = leaf_distances(&crown);
        for (pos, distance) in crown.leaves.iter().zip(distances) {
            if !self.free_for_tree(*pos) {
                continue;
            }
            let leaves = species
                .leaves()
                .state_with(&[
                    ("distance", &distance.to_string()),
                    ("persistent", "false"),
                    ("waterlogged", "false"),
                ])
                .unwrap_or_else(|| species.leaves().default_state());
            blocks.push((*pos, leaves));
        }
        Some((species, blocks))
    }

    pub(crate) fn sapling_random_tick(&mut self, pos: BlockPos, state: BlockStateId) {
        if self.raw_brightness(at(pos, Face::Up)) >= SAPLING_LIGHT
            && self.rng.random::<f32>() < SAPLING_ODDS
        {
            self.advance_tree(pos, state, None);
        }
    }

    fn advance_tree(&mut self, pos: BlockPos, state: BlockStateId, player: Option<PlayerId>) {
        if stage_of(state) == 0 {
            let Some(next) = state.with_property("stage", "1") else {
                return;
            };
            match player {
                Some(player) => self.fertilize(player, pos, vec![(pos, next)]),
                None => {
                    self.set_block(pos, next);
                }
            }
            return;
        }
        if let Some((species, blocks)) = self.plan_tree(pos, state) {
            self.structure_grow(player, pos, species.tree_type(), blocks);
        }
    }

    fn structure_grow(
        &mut self,
        player: Option<PlayerId>,
        origin: BlockPos,
        species: TreeType,
        blocks: Vec<(BlockPos, BlockStateId)>,
    ) {
        let step = StructureStep {
            player,
            origin,
            blocks: blocks.clone(),
        };
        self.fire(
            Lane::Block(origin),
            Continuation::Structure(Box::new(step)),
            move |server| {
                let world = server.world_name();
                Some(EventPayload::StructureGrow(StructureGrow {
                    blocks: blocks
                        .iter()
                        .map(|(pos, state)| snapshot_of(*pos, *state))
                        .collect(),
                    from_bonemeal: player.is_some(),
                    location: Location {
                        world: world.clone(),
                        position: Vec3::new(
                            f64::from(origin.x),
                            f64::from(origin.y),
                            f64::from(origin.z),
                        ),
                        rotation: Rotation {
                            yaw: 0.0,
                            pitch: 0.0,
                        },
                    },
                    player: player.and_then(|id| server.snapshot_of(id)),
                    species,
                    world,
                }))
            },
        );
    }

    pub(crate) fn finish_structure(&mut self, step: StructureStep, cancelled: bool) {
        let StructureStep {
            player,
            origin,
            blocks,
            ..
        } = step;
        match (player, cancelled) {
            (Some(player), true) => self.fertilize_refused(player, origin, blocks),
            (Some(player), false) => self.fertilize(player, origin, blocks),
            (None, false) => {
                for (pos, state) in blocks {
                    self.set_block(pos, state);
                }
            }
            (None, true) => {}
        }
    }

    fn fertilize_event(
        &self,
        player: PlayerId,
        origin: BlockPos,
        blocks: &[(BlockPos, BlockStateId)],
    ) -> Option<EventPayload> {
        Some(EventPayload::BlockFertilize(BlockFertilize {
            block: origin,
            blocks: blocks
                .iter()
                .map(|(pos, state)| snapshot_of(*pos, *state))
                .collect(),
            player: self.snapshot_of(player),
        }))
    }

    pub(crate) fn fertilize(
        &mut self,
        player: PlayerId,
        origin: BlockPos,
        blocks: Vec<(BlockPos, BlockStateId)>,
    ) {
        if blocks.is_empty() {
            return;
        }
        let listed = blocks.clone();
        self.fire(
            Lane::Player(player),
            Continuation::Fertilize(Box::new(FertilizeStep { blocks })),
            move |server| server.fertilize_event(player, origin, &listed),
        );
    }

    fn fertilize_refused(
        &mut self,
        player: PlayerId,
        origin: BlockPos,
        blocks: Vec<(BlockPos, BlockStateId)>,
    ) {
        let listed = blocks.clone();
        self.fire_refused(
            Lane::Player(player),
            Continuation::Fertilize(Box::new(FertilizeStep { blocks })),
            move |server| server.fertilize_event(player, origin, &listed),
        );
    }

    pub(crate) fn finish_fertilize(&mut self, step: FertilizeStep, cancelled: bool) {
        if cancelled {
            return;
        }
        for (pos, state) in step.blocks {
            self.set_block(pos, state);
        }
    }

    pub(crate) fn bone_meal_growth(&mut self, player: PlayerId, pos: BlockPos) -> bool {
        let Some(state) = self.world.chunks.block(pos) else {
            return false;
        };
        if is_sapling(state) {
            if self.rng.random::<f32>() < BONE_MEAL_SAPLING_CHANCE {
                self.advance_tree(pos, state, Some(player));
            }
            return true;
        }
        if state.kind() == Some(BlockKind::GrassBlock)
            && self.block_or_air(at(pos, Face::Up)).is_air()
        {
            let blocks = self.plan_grass(pos);
            self.fertilize(player, pos, blocks);
            return true;
        }
        false
    }

    fn plan_grass(&mut self, pos: BlockPos) -> Vec<(BlockPos, BlockStateId)> {
        let above = at(pos, Face::Up);
        let mut planned: Vec<(BlockPos, BlockStateId)> = Vec::new();
        'attempt: for attempt in 0..GRASS_ATTEMPTS {
            let mut spot = above;
            for _ in 0..attempt / GRASS_RANGE_DIVISOR {
                let dx = self.rng.random_range(0..3) - 1;
                let dy = (self.rng.random_range(0..3) - 1) * self.rng.random_range(0..3) / 2;
                let dz = self.rng.random_range(0..3) - 1;
                spot = spot.offset(dx, dy, dz);
                let floor = self.block_or_air(at(spot, Face::Down));
                let solid = self
                    .block_or_air(spot)
                    .flags()
                    .contains(StateFlags::FULL_COLLISION_CUBE);
                if floor.kind() != Some(BlockKind::GrassBlock) || solid {
                    continue 'attempt;
                }
            }
            let current = self.block_or_air(spot);
            if current.kind() == Some(BlockKind::ShortGrass)
                && self.rng.random_range(0..TALL_GRASS_ODDS) == 0
            {
                let top = at(spot, Face::Up);
                if self.block_or_air(top).is_air() {
                    for (place, half) in [(spot, "lower"), (top, "upper")] {
                        if let Some(state) = BlockKind::TallGrass.state_with(&[("half", half)]) {
                            planned.retain(|(known, _)| *known != place);
                            planned.push((place, state));
                        }
                    }
                }
            }
            if current.is_air() {
                planned.retain(|(known, _)| *known != spot);
                planned.push((spot, BlockKind::ShortGrass.default_state()));
            }
        }
        planned
    }
}

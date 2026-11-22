use std::sync::Arc;
use std::time::{Duration, Instant};

use crossbeam_channel::{Receiver, TryRecvError};
use rand::Rng;
use rspigot_blocks::{BlockKind, Face, StateFlags};
use rspigot_domain::player::PlayerId;
use rspigot_domain::{BlockPos, BlockStateId, Rotation, Vec3};

use crate::chunks::{QueryAnswer, WorldQuery};
use crate::grow_window::GrowWindow;
use crate::plugins::events::{BlockFertilize, StructureGrow, TreeType};
use crate::plugins::steps::{FertilizeStep, StructureStep};
use crate::plugins::{Continuation, EventPayload, Lane, Location, snapshot_of};
use crate::ports::{ClientLink, PlaceRequest};
use crate::server::Server;
use crate::tree_growers::{grower_of, pick, tree_type_of};
use crate::world_state::DimensionKey;

const SAPLING_ODDS: f32 = 1.0 / 7.0;
const SAPLING_LIGHT: i32 = 9;
const BONE_MEAL_SAPLING_CHANCE: f32 = 0.45;
const GRASS_ATTEMPTS: i32 = 128;
const GRASS_RANGE_DIVISOR: i32 = 16;
const TALL_GRASS_ODDS: i32 = 10;

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

const WINDOW_RADIUS: i32 = 16;
const WINDOW_BELOW: i32 = 8;
const WINDOW_ABOVE: i32 = 64;
const PENDING_LIFETIME: Duration = Duration::from_secs(60);

pub(crate) struct PendingTree {
    dimension: DimensionKey,
    player: Option<PlayerId>,
    origin: BlockPos,
    saplings: Vec<(BlockPos, BlockStateId)>,
    tree: Option<TreeType>,
    started: Instant,
    answer: Receiver<QueryAnswer>,
}

impl<L: ClientLink> Server<L> {
    pub(crate) fn sapling_random_tick(&mut self, pos: BlockPos, state: BlockStateId) {
        if state.kind() == Some(BlockKind::MangrovePropagule) {
            self.propagule_random_tick(pos, state);
            return;
        }
        if self.raw_brightness(at(pos, Face::Up)) >= SAPLING_LIGHT
            && self.rng.random::<f32>() < SAPLING_ODDS
        {
            self.advance_tree(pos, state, None);
        }
    }

    fn propagule_random_tick(&mut self, pos: BlockPos, state: BlockStateId) {
        if state.property("hanging") != Some("true") {
            if self.rng.random::<f32>() < SAPLING_ODDS {
                self.advance_tree(pos, state, None);
            }
            return;
        }
        let age: i32 = state
            .property("age")
            .and_then(|age| age.parse().ok())
            .unwrap_or(0);
        if age < 4
            && let Some(next) = state.with_property("age", &(age + 1).to_string())
        {
            self.set_block(pos, next);
        }
    }

    pub(crate) fn advance_tree(
        &mut self,
        pos: BlockPos,
        state: BlockStateId,
        player: Option<PlayerId>,
    ) {
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
        self.begin_tree_growth(pos, state, player);
    }

    fn has_flowers(&self, pos: BlockPos) -> bool {
        (-2..=2).any(|dx| {
            (-1..=1).any(|dy| {
                (-2..=2).any(|dz| {
                    tagged(
                        self.block_or_air(pos.offset(dx, dy, dz)),
                        "minecraft:flowers",
                    )
                })
            })
        })
    }

    fn two_by_two_of(&self, pos: BlockPos, state: BlockStateId) -> Option<BlockPos> {
        for dx in [0, -1] {
            for dz in [0, -1] {
                let corner = pos.offset(dx, 0, dz);
                let all = [(0, 0), (1, 0), (0, 1), (1, 1)].iter().all(|(ox, oz)| {
                    self.block_or_air(corner.offset(*ox, 0, *oz)).kind() == state.kind()
                });
                if all {
                    return Some(corner);
                }
            }
        }
        None
    }

    pub(crate) fn begin_tree_growth(
        &mut self,
        pos: BlockPos,
        state: BlockStateId,
        player: Option<PlayerId>,
    ) -> bool {
        let Some(grower) = state.kind().and_then(grower_of) else {
            return false;
        };
        let mega = pick(grower.megas, |total| self.rng.random_range(0..total));
        let mut chosen: Option<(&'static str, BlockPos, Vec<BlockPos>)> = None;
        if let Some(feature) = mega
            && let Some(corner) = self.two_by_two_of(pos, state)
        {
            let cells = [(0, 0), (1, 0), (0, 1), (1, 1)]
                .iter()
                .map(|(ox, oz)| corner.offset(*ox, 0, *oz))
                .collect();
            chosen = Some((feature, corner, cells));
        }
        if chosen.is_none() {
            let list = if self.has_flowers(pos) && !grower.flowers.is_empty() {
                grower.flowers
            } else {
                grower.trees
            };
            let feature = pick(list, |total| self.rng.random_range(0..total));
            chosen = feature.map(|feature| (feature, pos, vec![pos]));
        }
        let Some((feature, origin, cells)) = chosen else {
            return false;
        };
        self.begin_feature_growth(feature, origin, cells, player, Some(tree_type_of(feature)));
        true
    }

    pub(crate) fn begin_feature_growth(
        &mut self,
        feature: &str,
        origin: BlockPos,
        cells: Vec<BlockPos>,
        player: Option<PlayerId>,
        tree: Option<TreeType>,
    ) {
        let saplings: Vec<(BlockPos, BlockStateId)> = cells
            .iter()
            .map(|cell| (*cell, self.block_or_air(*cell)))
            .collect();
        let (min, size) = GrowWindow::around(origin, WINDOW_BELOW, WINDOW_ABOVE, WINDOW_RADIUS);
        let mut window = GrowWindow {
            min,
            size,
            states: Arc::from(Vec::new()),
        };
        let states: Vec<BlockStateId> = window
            .positions()
            .map(|at| self.world.chunks.block(at).unwrap_or(BlockStateId::AIR))
            .collect();
        window.states = states.into();
        let request = PlaceRequest::Grow {
            feature: feature.to_owned(),
            origin,
            clear: cells,
            seed: self.rng.random(),
            window,
        };
        let answer = self.workers.request_query(
            &self.world.dimension,
            origin.center(),
            WorldQuery::Place(request),
        );
        self.pending_trees.push(PendingTree {
            dimension: self.world.dimension.key.clone(),
            player,
            origin,
            saplings,
            tree,
            started: Instant::now(),
            answer,
        });
    }

    pub(crate) fn tick_tree_growth(&mut self) {
        if self.pending_trees.is_empty() {
            return;
        }
        let dimension = self.world.dimension.key.clone();
        let mut ready = Vec::new();
        let mut waiting = Vec::new();
        for pending in std::mem::take(&mut self.pending_trees) {
            if pending.dimension != dimension {
                if pending.started.elapsed() < PENDING_LIFETIME {
                    waiting.push(pending);
                }
                continue;
            }
            match pending.answer.try_recv() {
                Err(TryRecvError::Empty) if pending.started.elapsed() < PENDING_LIFETIME => {
                    waiting.push(pending);
                }
                Err(_) => {}
                Ok(answer) => ready.push((pending, answer)),
            }
        }
        self.pending_trees = waiting;
        for (pending, answer) in ready {
            let QueryAnswer::Placed(Ok(placed)) = answer else {
                continue;
            };
            let unchanged = pending
                .saplings
                .iter()
                .all(|(pos, state)| self.block_or_air(*pos) == *state);
            if !unchanged {
                continue;
            }
            let blocks = placed
                .into_iter()
                .map(|block| (block.pos, block.state))
                .collect();
            match pending.tree {
                Some(tree) => self.structure_grow(pending.player, pending.origin, tree, blocks),
                None => self.fertilized(pending.player, pending.origin, blocks),
            }
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

    pub(crate) fn bone_meal_growth(&mut self, who: Option<PlayerId>, pos: BlockPos) -> bool {
        let Some(state) = self.world.chunks.block(pos) else {
            return false;
        };
        if is_sapling(state) {
            if self.rng.random::<f32>() < BONE_MEAL_SAPLING_CHANCE {
                self.advance_tree(pos, state, who);
            }
            return true;
        }
        if state.kind() == Some(BlockKind::GrassBlock)
            && self.block_or_air(at(pos, Face::Up)).is_air()
        {
            let blocks = self.plan_grass(pos);
            self.fertilized(who, pos, blocks);
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

use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};
use rspigot_blocks::BlockKind;
use rspigot_domain::azalea_tree::{self, BONE_MEAL_SUCCESS};
use rspigot_domain::chorus;
use rspigot_domain::composter::BONE_MEAL_EVENT;
use rspigot_domain::player::PlayerId;
use rspigot_domain::{BlockPos, BlockStateId, Rotation, Vec3};
use rspigot_entities::SoundSource;

use crate::crops::BONE_MEAL_EVENT_DATA;
use crate::events::Hand;
use crate::mobs::ServerRandom;
use crate::plugins::events::{StructureGrow, TreeType};
use crate::plugins::steps::StructureStep;
use crate::plugins::{Continuation, EventPayload, Lane, Location, snapshot_of};
use crate::ports::ClientLink;
use crate::sea_pickle::is_sea_pickle;
use crate::server::Server;

const TILT_DOWN_SOUND: &str = "minecraft:block.big_dripleaf.tilt_down";
const TILT_UP_SOUND: &str = "minecraft:block.big_dripleaf.tilt_up";
const UNSTABLE_DELAY: u64 = 10;
const PARTIAL_DELAY: u64 = 10;
const FULL_DELAY: u64 = 100;
const LEAF_SURFACE: f64 = 0.6875;
const EDGE: f64 = 1.0e-5;
const PLAYER_HALF_WIDTH: f64 = 0.3;
const REPLACEABLE_BY_TREES: &str = "minecraft:replaceable_by_trees";
const DIRT: &str = "minecraft:dirt";

fn tagged(state: BlockStateId, tag: &str) -> bool {
    state.kind().is_some_and(|kind| {
        rspigot_data::tags::tag("minecraft:block", tag)
            .is_some_and(|ids| ids.binary_search(&u32::from(kind.id().0)).is_ok())
    })
}

fn tilt_of(state: BlockStateId) -> &'static str {
    match state.property("tilt") {
        Some("unstable") => "unstable",
        Some("partial") => "partial",
        Some("full") => "full",
        _ => "none",
    }
}

fn is_azalea(state: BlockStateId) -> bool {
    matches!(
        state.kind(),
        Some(BlockKind::Azalea | BlockKind::FloweringAzalea)
    )
}

fn leaf_can_replace(state: BlockStateId) -> bool {
    state.is_air()
        || matches!(
            state.kind(),
            Some(BlockKind::Water | BlockKind::SmallDripleaf)
        )
}

impl<L: ClientLink> Server<L> {
    pub(crate) fn tick_block_extras(&mut self) {
        self.tick_leaf_weights();
    }

    pub(crate) fn extra_block_tick(
        &mut self,
        pos: BlockPos,
        state: BlockStateId,
        kind: BlockKind,
    ) -> bool {
        match kind {
            BlockKind::Cauldron | BlockKind::WaterCauldron => {
                self.cauldron_drip_tick(pos, state);
                true
            }
            BlockKind::BigDripleaf => {
                self.leaf_tilt_tick(pos, state);
                true
            }
            _ => false,
        }
    }

    pub(crate) fn chorus_random_tick(&mut self, pos: BlockPos, state: BlockStateId) {
        let seed: u64 = self.rng.random();
        let changes = {
            let block = |at: BlockPos| self.block_or_air(at);
            let mut rng = StdRng::seed_from_u64(seed);
            chorus::plan(
                &block,
                pos,
                state,
                self.world.dimension.height.max_y(),
                &mut ServerRandom(&mut rng),
            )
        };
        for change in &changes {
            self.set_block(change.pos, change.state);
            if let Some(event) = change.event {
                self.level_event(event, change.pos, 0);
            }
        }
    }

    fn tick_leaf_weights(&mut self) {
        let mut standing: Vec<(Vec3, f64)> = self
            .players
            .values()
            .filter(|player| {
                player.on_ground
                    && !player.vitals.is_dead()
                    && player.game_mode != rspigot_domain::player::GameMode::Spectator
            })
            .map(|player| (player.position, PLAYER_HALF_WIDTH))
            .collect();
        standing.extend(
            self.entities
                .store
                .iter()
                .filter(|entity| entity.base.on_ground && !entity.base.is_removed())
                .map(|entity| {
                    let area = entity.base.bounding_box();
                    (entity.base.position, (area.max_x - area.min_x) / 2.0)
                }),
        );
        let mut pressed: Vec<BlockPos> = Vec::new();
        for (at, half) in standing {
            let low = |value: f64| (value - half + EDGE).floor() as i32;
            let high = |value: f64| (value + half - EDGE).floor() as i32;
            let y = at.y.floor() as i32;
            for x in low(at.x)..=high(at.x) {
                for z in low(at.z)..=high(at.z) {
                    let pos = BlockPos::new(x, y, z);
                    let tilts = self.world.chunks.block(pos).is_some_and(|state| {
                        state.kind() == Some(BlockKind::BigDripleaf)
                            && tilt_of(state) == "none"
                            && at.y > f64::from(y) + LEAF_SURFACE - EDGE
                    });
                    if tilts && !pressed.contains(&pos) {
                        pressed.push(pos);
                    }
                }
            }
        }
        for pos in pressed {
            if self.has_neighbor_signal(pos) {
                continue;
            }
            let Some(state) = self.world.chunks.block(pos) else {
                continue;
            };
            self.set_leaf_tilt(pos, state, "unstable", None, UNSTABLE_DELAY);
        }
    }

    fn set_leaf_tilt(
        &mut self,
        pos: BlockPos,
        state: BlockStateId,
        tilt: &str,
        sound: Option<&'static str>,
        delay: u64,
    ) {
        if let Some(next) = state.with_property("tilt", tilt) {
            self.set_block(pos, next);
        }
        if let Some(sound) = sound {
            self.play_sound(None, pos.center(), sound, SoundSource::Blocks, 1.0, 1.0);
        }
        if delay > 0 {
            self.schedule(pos, BlockKind::BigDripleaf, delay);
        }
    }

    fn reset_leaf_tilt(&mut self, pos: BlockPos, state: BlockStateId) {
        let tilted = tilt_of(state) != "none";
        self.set_leaf_tilt(pos, state, "none", None, 0);
        if tilted {
            self.play_sound(
                None,
                pos.center(),
                TILT_UP_SOUND,
                SoundSource::Blocks,
                1.0,
                1.0,
            );
        }
    }

    fn leaf_tilt_tick(&mut self, pos: BlockPos, state: BlockStateId) {
        if self.has_neighbor_signal(pos) {
            self.reset_leaf_tilt(pos, state);
            return;
        }
        match tilt_of(state) {
            "unstable" => {
                self.set_leaf_tilt(pos, state, "partial", Some(TILT_DOWN_SOUND), PARTIAL_DELAY)
            }
            "partial" => self.set_leaf_tilt(pos, state, "full", Some(TILT_DOWN_SOUND), FULL_DELAY),
            "full" => self.reset_leaf_tilt(pos, state),
            _ => {}
        }
    }

    pub(crate) fn use_plant_bone_meal(
        &mut self,
        id: PlayerId,
        hand: Hand,
        pos: BlockPos,
        sequence: i32,
    ) -> bool {
        let Some(state) = self.world.chunks.block(pos) else {
            return false;
        };
        let used = if is_azalea(state) {
            if self.rng.random::<f32>() < BONE_MEAL_SUCCESS {
                self.grow_azalea_tree(id, pos);
            }
            true
        } else if matches!(
            state.kind(),
            Some(BlockKind::BigDripleaf | BlockKind::BigDripleafStem)
        ) {
            self.grow_big_dripleaf(id, pos, state)
        } else if is_sea_pickle(state) {
            self.grow_sea_pickle(id, pos, state);
            true
        } else {
            false
        };
        if !used {
            return false;
        }
        self.consume_held(id, hand, 1);
        self.level_event(BONE_MEAL_EVENT, pos, BONE_MEAL_EVENT_DATA);
        self.player_swing(id, hand, false);
        self.refresh_hands(id);
        self.ack_block(id, sequence);
        true
    }

    fn grow_big_dripleaf(&mut self, player: PlayerId, pos: BlockPos, state: BlockStateId) -> bool {
        let mut top = pos;
        let mut current = state;
        while current.kind() == Some(BlockKind::BigDripleafStem) {
            top = top.offset(0, 1, 0);
            current = self.block_or_air(top);
        }
        if current.kind() != Some(BlockKind::BigDripleaf) {
            return false;
        }
        let above = top.offset(0, 1, 0);
        let above_state = self.block_or_air(above);
        if !leaf_can_replace(above_state) || above.y > self.world.dimension.height.max_y() {
            return false;
        }
        let facing = current.property("facing").unwrap_or("north");
        let wet = |state: BlockStateId| {
            if state.kind() == Some(BlockKind::Water)
                || state.property("waterlogged") == Some("true")
            {
                "true"
            } else {
                "false"
            }
        };
        let stem = BlockKind::BigDripleafStem
            .state_with(&[("facing", facing), ("waterlogged", wet(current))]);
        let leaf = BlockKind::BigDripleaf.state_with(&[
            ("facing", facing),
            ("tilt", "none"),
            ("waterlogged", wet(above_state)),
        ]);
        let (Some(stem), Some(leaf)) = (stem, leaf) else {
            return false;
        };
        self.fertilize(player, top, vec![(top, stem), (above, leaf)]);
        true
    }

    fn grow_azalea_tree(&mut self, player: PlayerId, origin: BlockPos) {
        let seed: u64 = self.rng.random();
        let tree = {
            let free = |at: BlockPos| {
                let state = self.block_or_air(at);
                state.is_air() || is_azalea(state) || tagged(state, REPLACEABLE_BY_TREES)
            };
            let dirt = tagged(self.block_or_air(origin.offset(0, -1, 0)), DIRT);
            let mut rng = StdRng::seed_from_u64(seed);
            azalea_tree::plan(
                origin,
                self.world.dimension.height.max_y(),
                &free,
                dirt,
                &mut ServerRandom(&mut rng),
            )
        };
        if let Some(tree) = tree {
            self.azalea_structure_grow(player, origin, tree.blocks);
        }
    }

    fn azalea_structure_grow(
        &mut self,
        player: PlayerId,
        origin: BlockPos,
        blocks: Vec<(BlockPos, BlockStateId)>,
    ) {
        let step = StructureStep {
            player: Some(player),
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
                    from_bonemeal: true,
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
                    player: server.snapshot_of(player),
                    species: TreeType::Azalea,
                    world,
                }))
            },
        );
    }
}

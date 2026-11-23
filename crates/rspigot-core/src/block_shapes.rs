use rand::Rng;
use rspigot_blocks::{BlockKind, Face};
use rspigot_domain::entity::Aabb;
use rspigot_domain::placement::{self, Shaped};
use rspigot_domain::player::{GameMode, PlayerId};
use rspigot_domain::{BlockPos, BlockStateId};

use crate::ports::ClientLink;
use crate::server::Server;

const PLAYER_WIDTH: f32 = 0.6;
const PLAYER_HEIGHT: f32 = 1.8;
const SNEAKING_HEIGHT: f32 = 1.5;
const TRAMPLE_OFFSET: f64 = 0.2;
const TRAMPLE_BASE: f32 = 0.5;

fn blocks_building(entity: &rspigot_domain::entity::Entity) -> bool {
    let name = entity.base.kind.name();
    entity.living().is_some()
        || name.contains("boat")
        || name.contains("raft")
        || name.contains("minecart")
        || name.ends_with("end_crystal")
}

fn placed_boxes(pos: BlockPos, shape: &[rspigot_blocks::Aabb]) -> Vec<Aabb> {
    let (x, y, z) = (f64::from(pos.x), f64::from(pos.y), f64::from(pos.z));
    shape
        .iter()
        .map(|b| {
            Aabb::new(
                x + b.min_x,
                y + b.min_y,
                z + b.min_z,
                x + b.max_x,
                y + b.max_y,
                z + b.max_z,
            )
        })
        .collect()
}

impl<L: ClientLink> Server<L> {
    pub(crate) fn shape_view(&self, pos: BlockPos) -> BlockStateId {
        match self.world.chunks.block(pos) {
            Some(state) => state,
            None if self.world.dimension.height.contains(pos.y)
                && !self.world.chunks.is_loaded(pos.chunk()) =>
            {
                BlockKind::Stone.default_state()
            }
            None => BlockStateId::AIR,
        }
    }

    pub(crate) fn apply_shape(
        &mut self,
        pos: BlockPos,
        state: BlockStateId,
        direction: Face,
    ) -> bool {
        let shaped = placement::update_shape(state, pos, direction, &|at| self.shape_view(at));
        match shaped {
            Shaped::Same => false,
            Shaped::Changed(next) => {
                self.set_block(pos, next);
                true
            }
            Shaped::Destroy => {
                self.destroy_block(pos, state);
                true
            }
            Shaped::Recheck => {
                if let Some(kind) = state.kind() {
                    self.schedule(pos, kind, 1);
                }
                true
            }
        }
    }

    pub(crate) fn survival_tick(&mut self, pos: BlockPos, state: BlockStateId) -> bool {
        if !placement::is_deferred(state)
            || placement::can_survive(state, pos, &|at| self.shape_view(at))
        {
            return false;
        }
        self.destroy_block(pos, state);
        true
    }

    pub(crate) fn rail_placed(&mut self, pos: BlockPos, old: BlockStateId, state: BlockStateId) {
        if old.kind() == state.kind() || !placement::is_rail(state) {
            return;
        }
        let powered = self.has_neighbor_signal(pos);
        let changes = placement::rail_placed(pos, state, powered, &|at| self.shape_view(at));
        for change in changes {
            if self.world.chunks.block(change.pos) != Some(change.state) {
                self.set_block(change.pos, change.state);
            }
        }
    }

    pub(crate) fn obstructed(&self, pos: BlockPos, state: BlockStateId) -> bool {
        let shape = state.collision_shape();
        if shape.is_empty() {
            return false;
        }
        let boxes = placed_boxes(pos, shape);
        let hits = |other: &Aabb| boxes.iter().any(|b| b.intersects(other));
        self.players_hit(&boxes)
            || self.entities.store.iter().any(|entity| {
                !entity.base.is_removed()
                    && blocks_building(entity)
                    && hits(&entity.base.bounding_box())
            })
    }

    fn players_hit(&self, boxes: &[Aabb]) -> bool {
        self.players.values().any(|p| {
            p.game_mode != GameMode::Spectator && {
                let height = if p.sneaking {
                    SNEAKING_HEIGHT
                } else {
                    PLAYER_HEIGHT
                };
                let body = Aabb::of_size(p.position, PLAYER_WIDTH, height);
                boxes.iter().any(|b| b.intersects(&body))
            }
        })
    }

    pub(crate) fn trample_farmland(&mut self, id: PlayerId, distance: f64) {
        let Some(player) = self.players.get(&id) else {
            return;
        };
        let at = player.position;
        let pos = BlockPos::new(
            at.x.floor() as i32,
            (at.y - TRAMPLE_OFFSET).floor() as i32,
            at.z.floor() as i32,
        );
        if self.world.chunks.block(pos).and_then(BlockStateId::kind) != Some(BlockKind::Farmland) {
            return;
        }
        if self.rng.random::<f32>() < distance as f32 - TRAMPLE_BASE {
            self.set_block(pos, BlockKind::Dirt.default_state());
        }
    }
}

pub(crate) fn left_behind(state: BlockStateId) -> BlockStateId {
    if state.property("waterlogged") == Some("true") {
        BlockKind::Water.default_state()
    } else {
        BlockStateId::AIR
    }
}

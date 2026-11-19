use rspigot_blocks::{BlockKind, Face, ShapeId};
use rspigot_domain::entity::raycast::{self, BlockHit};
use rspigot_domain::entity::{Aabb, BlockView};
use rspigot_domain::player::{GameMode, PlayerId};
use rspigot_domain::{BlockPos, BlockStateId, EntityId, Rotation, Vec3, mth};

use crate::chunks::ChunkManager;
use crate::events::Direction;
use crate::plugins::resolve::finite_not_negative;
use crate::plugins::{Answer, TargetHit};
use crate::ports::ClientLink;
use crate::server::Server;

pub const MAX_TARGET_DISTANCE: f64 = 120.0;
const PLAYER_WIDTH: f32 = 0.6;
const BLOCK_RANGE: f64 = 4.5;
const CREATIVE_BLOCK_RANGE: f64 = 5.0;
const ENTITY_RANGE: f64 = 3.0;
const CREATIVE_ENTITY_RANGE: f64 = 5.0;

pub(crate) enum Punched {
    Nothing,
    Block(BlockHit),
    Entity,
}

struct Sight {
    from: Vec3,
    to: Vec3,
    length: f64,
    block: Option<BlockHit>,
}

enum Struck {
    Player(PlayerId),
    Entity(EntityId),
    Npc(EntityId),
}

struct Sightline<'a> {
    chunks: &'a ChunkManager,
    eyes: BlockPos,
}

impl BlockView for Sightline<'_> {
    fn block(&self, pos: BlockPos) -> Option<BlockStateId> {
        let state = self.chunks.block(pos)?;
        if pos == self.eyes && state.collision_shape().is_empty() {
            return Some(BlockStateId::AIR);
        }
        Some(state)
    }
}

impl<L: ClientLink> Server<L> {
    pub(super) fn target_block(&self, id: PlayerId, max_distance: f64) -> Answer {
        self.aim(id, max_distance, |sight| {
            sight.block.and_then(|hit| self.block_target(hit))
        })
    }

    pub(super) fn target_entity(&self, id: PlayerId, max_distance: f64) -> Answer {
        self.aim(id, max_distance, |sight| self.entity_target(id, sight))
    }

    pub(crate) fn look_block(&self, id: PlayerId, rotation: Rotation) -> Option<BlockHit> {
        self.look_with(id, rotation, sight_shape)
    }

    pub(crate) fn look_source(&self, id: PlayerId, rotation: Rotation) -> Option<BlockHit> {
        self.look_with(id, rotation, source_shape)
    }

    fn look_with(
        &self,
        id: PlayerId,
        rotation: Rotation,
        shape: fn(BlockStateId) -> &'static [rspigot_blocks::Aabb],
    ) -> Option<BlockHit> {
        let player = self.players.get(&id)?;
        let reach = if player.game_mode == GameMode::Creative {
            CREATIVE_BLOCK_RANGE
        } else {
            BLOCK_RANGE
        };
        let from = player.eye_position();
        let sightline = Sightline {
            chunks: &self.world.chunks,
            eyes: from.block(),
        };
        raycast::clip_blocks_with(&sightline, from, ray_end(from, rotation, reach), shape)
    }

    pub(crate) fn punch_target(&self, id: PlayerId) -> Punched {
        let Some(player) = self.players.get(&id) else {
            return Punched::Nothing;
        };
        let reach = if player.game_mode == GameMode::Creative {
            CREATIVE_BLOCK_RANGE.max(CREATIVE_ENTITY_RANGE)
        } else {
            BLOCK_RANGE.max(ENTITY_RANGE)
        };
        let from = player.eye_position();
        let to = ray_end(from, player.rotation, reach);
        let sightline = Sightline {
            chunks: &self.world.chunks,
            eyes: from.block(),
        };
        let sight = Sight {
            from,
            to,
            length: reach,
            block: raycast::clip_blocks_with(&sightline, from, to, sight_shape),
        };
        if self.entity_target(id, &sight).is_some() {
            return Punched::Entity;
        }
        sight.block.map_or(Punched::Nothing, Punched::Block)
    }

    fn aim(
        &self,
        id: PlayerId,
        max_distance: f64,
        target: impl FnOnce(&Sight) -> Option<TargetHit>,
    ) -> Answer {
        let Some(player) = self.players.get(&id) else {
            return Answer::NotFound;
        };
        if let Err(reason) = finite_not_negative("max_distance", max_distance) {
            return Answer::Refused(reason);
        }
        let from = player.eye_position();
        let to = ray_end(from, player.rotation, max_distance);
        let sightline = Sightline {
            chunks: &self.world.chunks,
            eyes: from.block(),
        };
        let sight = Sight {
            from,
            to,
            length: max_distance.min(MAX_TARGET_DISTANCE),
            block: raycast::clip_blocks_with(&sightline, from, to, sight_shape),
        };
        Answer::Target(target(&sight))
    }

    fn block_target(&self, hit: BlockHit) -> Option<TargetHit> {
        let kind = self.world.chunks.block(hit.pos)?.kind()?;
        Some(TargetHit::Block {
            pos: hit.pos,
            face: direction_of(hit.face),
            kind: kind.name().to_owned(),
        })
    }

    fn entity_target(&self, viewer: PlayerId, sight: &Sight) -> Option<TargetHit> {
        let limit = sight
            .block
            .map_or(sight.length, |hit| (hit.location - sight.from).length());
        let players = self
            .players
            .values()
            .filter(|player| {
                player.id != viewer
                    && !player.vitals.is_dead()
                    && player.game_mode != GameMode::Spectator
            })
            .map(|player| {
                let bounds = Aabb::of_size(player.position, PLAYER_WIDTH, player.height() as f32);
                (bounds, Struck::Player(player.id))
            });
        let entities = self
            .entities
            .store
            .iter()
            .filter(|entity| !entity.base.is_removed())
            .map(|entity| (entity.base.bounding_box(), Struck::Entity(entity.base.id)));
        let npcs = self
            .entities
            .npcs
            .iter()
            .map(|npc| (crate::npc::bounds_of(npc), Struck::Npc(npc.id)));
        let (_, struck) = players
            .chain(entities)
            .chain(npcs)
            .filter_map(|(bounds, struck)| Some((sight.distance_to(&bounds)?, struck)))
            .filter(|(distance, _)| *distance < limit)
            .min_by(|a, b| a.0.total_cmp(&b.0))?;
        match struck {
            Struck::Player(id) => self.snapshot_of(id).map(TargetHit::Player),
            Struck::Entity(id) => self.entity_view(id).map(TargetHit::Entity),
            Struck::Npc(id) => self.npc_view(id).map(TargetHit::Npc),
        }
    }
}

impl Sight {
    fn distance_to(&self, bounds: &Aabb) -> Option<f64> {
        if raycast::contains(bounds, self.from) {
            return Some(0.0);
        }
        raycast::clip_box(bounds, self.from, self.to).map(|hit| (hit - self.from).length())
    }
}

pub(crate) fn ray_end(from: Vec3, rotation: Rotation, max_distance: f64) -> Vec3 {
    let radians = |angle: f32| f64::from(angle * mth::DEG_TO_RAD);
    let (yaw, pitch) = (radians(rotation.yaw), radians(rotation.pitch));
    let direction = Vec3::new(
        f64::from(-mth::sin(yaw) * mth::cos(pitch)),
        f64::from(-mth::sin(pitch)),
        f64::from(mth::cos(yaw) * mth::cos(pitch)),
    );
    from + direction.scale(max_distance.min(MAX_TARGET_DISTANCE))
}

fn sight_shape(state: BlockStateId) -> &'static [rspigot_blocks::Aabb] {
    let passed = matches!(
        state.kind(),
        Some(BlockKind::Light | BlockKind::Water | BlockKind::Lava | BlockKind::BubbleColumn)
    );
    if state.is_air() || passed {
        return &[];
    }
    match state.collision_shape() {
        [] => ShapeId::FULL.boxes(),
        shape => shape,
    }
}

fn source_shape(state: BlockStateId) -> &'static [rspigot_blocks::Aabb] {
    let source = matches!(state.kind(), Some(BlockKind::Water | BlockKind::Lava))
        && state.property("level") == Some("0");
    if source
        || state.kind() == Some(BlockKind::BubbleColumn)
        || state.property("waterlogged") == Some("true")
    {
        return ShapeId::FULL.boxes();
    }
    sight_shape(state)
}

pub(crate) const fn direction_of(face: Face) -> Direction {
    match face {
        Face::Down => Direction::Down,
        Face::Up => Direction::Up,
        Face::North => Direction::North,
        Face::South => Direction::South,
        Face::West => Direction::West,
        Face::East => Direction::East,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: Vec3, b: Vec3) -> bool {
        (a - b).length() < 1.0e-3
    }

    #[test]
    fn the_ray_is_capped_at_120_blocks() {
        assert!(close(
            ray_end(Vec3::ZERO, Rotation::new(0.0, 0.0), 500.0),
            Vec3::new(0.0, 0.0, 120.0)
        ));
        assert!(close(
            ray_end(Vec3::ZERO, Rotation::new(90.0, 0.0), 10.0),
            Vec3::new(-10.0, 0.0, 0.0)
        ));
    }
}

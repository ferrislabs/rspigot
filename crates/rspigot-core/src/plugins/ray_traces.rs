use rspigot_blocks::ShapeId;
use rspigot_domain::entity::{Aabb, raycast};
use rspigot_domain::player::GameMode;
use rspigot_domain::{BlockStateId, EntityId, Vec3};

use crate::plugins::targets::{Sightline, direction_of, sight_shape, source_shape};
use crate::plugins::{Answer, LookupAnswer, TargetHit};
use crate::ports::ClientLink;
use crate::server::Server;

const MAX_RAY_DISTANCE: f64 = 1024.0;
const MAX_RAY_SIZE: f64 = 16.0;
const PLAYER_WIDTH: f32 = 0.6;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FluidMode {
    Never,
    SourceOnly,
    Always,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RayTraceQuery {
    pub origin: Vec3,
    pub direction: Vec3,
    pub max_distance: f64,
    pub blocks: bool,
    pub entities: bool,
    pub fluid: FluidMode,
    pub ignore_passable: bool,
    pub ray_size: f64,
    pub ignore_entity: Option<EntityId>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RayHit {
    pub hit: Option<TargetHit>,
    pub position: Vec3,
    pub distance: f64,
}

fn finite(vec: Vec3) -> bool {
    vec.x.is_finite() && vec.y.is_finite() && vec.z.is_finite()
}

fn is_fluid(state: BlockStateId) -> bool {
    matches!(
        state.kind(),
        Some(
            rspigot_blocks::BlockKind::Water
                | rspigot_blocks::BlockKind::Lava
                | rspigot_blocks::BlockKind::BubbleColumn
        )
    )
}

fn collision_only(state: BlockStateId) -> &'static [rspigot_blocks::Aabb] {
    if state.is_air() || is_fluid(state) {
        return &[];
    }
    state.collision_shape()
}

fn collision_or_source(state: BlockStateId) -> &'static [rspigot_blocks::Aabb] {
    let source = matches!(
        state.kind(),
        Some(rspigot_blocks::BlockKind::Water | rspigot_blocks::BlockKind::Lava)
    ) && state.property("level") == Some("0");
    if source {
        return ShapeId::FULL.boxes();
    }
    collision_only(state)
}

fn collision_or_fluid(state: BlockStateId) -> &'static [rspigot_blocks::Aabb] {
    if is_fluid(state) {
        return ShapeId::FULL.boxes();
    }
    collision_only(state)
}

fn with_all_fluids(state: BlockStateId) -> &'static [rspigot_blocks::Aabb] {
    if is_fluid(state) || state.property("waterlogged") == Some("true") {
        return ShapeId::FULL.boxes();
    }
    sight_shape(state)
}

impl<L: ClientLink> Server<L> {
    pub(super) fn ray_trace(&self, query: &RayTraceQuery) -> Answer {
        if !finite(query.origin) || !finite(query.direction) {
            return Answer::Refused("the ray has a number that is not finite".to_owned());
        }
        let length = query.direction.length();
        if length < 1.0e-9 {
            return Answer::Refused("the ray has no direction".to_owned());
        }
        if !query.max_distance.is_finite()
            || query.max_distance < 0.0
            || !query.ray_size.is_finite()
            || query.ray_size < 0.0
        {
            return Answer::Refused("the ray has a negative or non-finite size".to_owned());
        }
        let max = query.max_distance.min(MAX_RAY_DISTANCE);
        let size = query.ray_size.min(MAX_RAY_SIZE);
        let from = query.origin;
        let to = from + query.direction.scale(max / length);
        let sightline = Sightline {
            chunks: &self.world.chunks,
            eyes: from.block(),
        };
        let block = query
            .blocks
            .then(|| match (query.ignore_passable, query.fluid) {
                (true, FluidMode::Never) => {
                    raycast::clip_blocks_with(&sightline, from, to, collision_only)
                }
                (true, FluidMode::SourceOnly) => {
                    raycast::clip_blocks_with(&sightline, from, to, collision_or_source)
                }
                (true, FluidMode::Always) => {
                    raycast::clip_blocks_with(&sightline, from, to, collision_or_fluid)
                }
                (false, FluidMode::Never) => {
                    raycast::clip_blocks_with(&sightline, from, to, sight_shape)
                }
                (false, FluidMode::SourceOnly) => {
                    raycast::clip_blocks_with(&sightline, from, to, source_shape)
                }
                (false, FluidMode::Always) => {
                    raycast::clip_blocks_with(&sightline, from, to, with_all_fluids)
                }
            })
            .flatten();
        let block_distance = block.map(|hit| (hit.location - from).length());
        let limit = block_distance.unwrap_or(max);
        let entity = query
            .entities
            .then(|| self.nearest_entity(from, to, size, limit, query.ignore_entity))
            .flatten();
        let answer = match (entity, block) {
            (Some((distance, point, hit)), _) => RayHit {
                hit: Some(hit),
                position: point,
                distance,
            },
            (None, Some(block)) => RayHit {
                hit: self.block_hit(block),
                position: block.location,
                distance: block_distance.unwrap_or(0.0),
            },
            (None, None) => RayHit {
                hit: None,
                position: to,
                distance: max,
            },
        };
        Answer::Lookup(LookupAnswer::RayHit(answer))
    }

    fn block_hit(&self, hit: raycast::BlockHit) -> Option<TargetHit> {
        let kind = self.world.chunks.block(hit.pos)?.kind()?;
        Some(TargetHit::Block {
            pos: hit.pos,
            face: direction_of(hit.face),
            kind: kind.name().to_owned(),
        })
    }

    fn nearest_entity(
        &self,
        from: Vec3,
        to: Vec3,
        size: f64,
        limit: f64,
        ignored: Option<EntityId>,
    ) -> Option<(f64, Vec3, TargetHit)> {
        let players = self
            .players
            .values()
            .filter(|player| {
                !player.vitals.is_dead()
                    && player.game_mode != GameMode::Spectator
                    && ignored != Some(player.entity_id)
            })
            .map(|player| {
                (
                    Aabb::of_size(player.position, PLAYER_WIDTH, player.height() as f32),
                    Struck::Player(player.id),
                )
            });
        let entities = self
            .entities
            .store
            .iter()
            .filter(|entity| !entity.base.is_removed() && ignored != Some(entity.base.id))
            .map(|entity| (entity.base.bounding_box(), Struck::Entity(entity.base.id)));
        let npcs = self
            .entities
            .npcs
            .iter()
            .filter(|npc| ignored != Some(npc.id))
            .map(|npc| (crate::npc::bounds_of(npc), Struck::Npc(npc.id)));
        let (distance, point, struck) = players
            .chain(entities)
            .chain(npcs)
            .filter_map(|(bounds, struck)| {
                let bounds = bounds.inflate(size);
                let point = if raycast::contains(&bounds, from) {
                    from
                } else {
                    raycast::clip_box(&bounds, from, to)?
                };
                Some(((point - from).length(), point, struck))
            })
            .filter(|(distance, _, _)| *distance <= limit)
            .min_by(|a, b| a.0.total_cmp(&b.0))?;
        let hit = match struck {
            Struck::Player(id) => self.snapshot_of(id).map(TargetHit::Player),
            Struck::Entity(id) => self.entity_view(id).map(TargetHit::Entity),
            Struck::Npc(id) => self.npc_view(id).map(TargetHit::Npc),
        }?;
        Some((distance, point, hit))
    }
}

enum Struck {
    Player(rspigot_domain::player::PlayerId),
    Entity(EntityId),
    Npc(EntityId),
}

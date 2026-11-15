use rspigot_blocks::{BlockKind, Face};
use rspigot_domain::entity::Aabb;
use rspigot_domain::entity::physics::collide_bounding_box;
use rspigot_domain::piston::motion::{
    self, Motion, pushed_by, search_area, slime_velocity, sticks, sticky_area,
};
use rspigot_domain::player::PlayerId;
use rspigot_domain::{BlockPos, BlockStateId, EntityId, Vec3};
use rspigot_entities::EntityKind;

use super::facing;
use super::piston::Moving;
use crate::events::ClientEvent;
use crate::ports::ClientLink;
use crate::server::{Audience, Server};

const PLAYER_WIDTH: f32 = 0.6;
const RESTRICTION_KEEP: u64 = 4;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum Mover {
    Entity(EntityId),
    Player(PlayerId),
}

#[derive(Debug, Clone, Copy)]
struct Body {
    mover: Mover,
    bounds: Aabb,
    position: Vec3,
    on_ground: bool,
}

fn ignores_pistons(kind: EntityKind) -> bool {
    matches!(
        kind,
        EntityKind::Marker
            | EntityKind::AreaEffectCloud
            | EntityKind::Interaction
            | EntityKind::BlockDisplay
            | EntityKind::ItemDisplay
            | EntityKind::TextDisplay
    )
}

fn shape_of(state: BlockStateId) -> Vec<Aabb> {
    state
        .collision_shape()
        .iter()
        .map(|part| {
            Aabb::new(
                part.min_x, part.min_y, part.min_z, part.max_x, part.max_y, part.max_z,
            )
        })
        .collect()
}

fn corner(pos: BlockPos) -> (i32, i32, i32) {
    (pos.x, pos.y, pos.z)
}

impl Moving {
    pub(crate) fn motion(&self) -> Motion {
        Motion {
            direction: self.direction,
            extending: self.extending,
            source: self.source,
            progress: self.progress,
        }
    }

    fn collision_state(&self) -> BlockStateId {
        let motion = self.motion();
        let is_base = matches!(
            self.state.kind(),
            Some(BlockKind::Piston | BlockKind::StickyPiston)
        );
        if !motion.is_retracting_source() || !is_base {
            return self.state;
        }
        let kind = if self.state.kind() == Some(BlockKind::StickyPiston) {
            "sticky"
        } else {
            "normal"
        };
        let short = if motion.shows_short_head() {
            "true"
        } else {
            "false"
        };
        let direction = facing(self.state).unwrap_or(self.direction);
        BlockKind::PistonHead
            .default_state()
            .with_property("facing", direction.name())
            .and_then(|state| state.with_property("type", kind))
            .and_then(|state| state.with_property("short", short))
            .unwrap_or(self.state)
    }
}

impl<L: ClientLink> Server<L> {
    pub(crate) fn move_collided_entities(
        &mut self,
        pos: BlockPos,
        moving: &Moving,
        new_progress: f32,
    ) {
        let motion = moving.motion();
        let delta = f64::from(new_progress - moving.progress);
        let shape = shape_of(moving.collision_state());
        let block = corner(pos);
        let Some(area) = search_area(&motion, block, &shape, delta) else {
            return;
        };
        let direction = motion.movement_direction();
        let slime = moving.state.kind() == Some(BlockKind::SlimeBlock);
        for body in self.bodies_in(&area) {
            if slime {
                self.bounce_off_slime(body.mover, direction);
            }
            let Some(distance) = pushed_by(&motion, block, &shape, delta, &body.bounds) else {
                continue;
            };
            self.nudge(body, direction, distance);
            if motion.is_retracting_source()
                && let Some(fix) = self
                    .refreshed(body.mover)
                    .and_then(|fresh| motion::base_fix(block, direction, &fresh.bounds, delta))
                && let Some(fresh) = self.refreshed(body.mover)
            {
                self.nudge(fresh, direction.opposite(), fix);
            }
        }
    }

    pub(crate) fn move_stuck_entities(
        &mut self,
        pos: BlockPos,
        moving: &Moving,
        new_progress: f32,
    ) {
        if moving.state.kind() != Some(BlockKind::HoneyBlock) {
            return;
        }
        let motion = moving.motion();
        let direction = motion.movement_direction();
        if matches!(direction, Face::Up | Face::Down) {
            return;
        }
        let top = shape_of(moving.state)
            .iter()
            .map(|part| part.max_y)
            .fold(0.0_f64, f64::max);
        let block = corner(pos);
        let area = sticky_area(&motion, block, top);
        let delta = f64::from(new_progress - moving.progress);
        for body in self.bodies_in(&area) {
            let supported = Self::supported_by(&body, pos);
            let sticks = sticks(
                &area,
                body.on_ground,
                supported,
                body.position.x,
                body.position.z,
            );
            if sticks {
                self.nudge(body, direction, delta);
            }
        }
    }

    fn supported_by(body: &Body, pos: BlockPos) -> bool {
        let below = Vec3::new(body.position.x, body.position.y - 1.0e-5, body.position.z).block();
        below == pos
    }

    fn bodies_in(&self, area: &Aabb) -> Vec<Body> {
        let mut found: Vec<Body> = self
            .entities
            .store
            .iter()
            .filter(|entity| entity.base.removal.is_none() && !ignores_pistons(entity.base.kind))
            .map(|entity| Body {
                mover: Mover::Entity(entity.base.id),
                bounds: entity.base.bounding_box(),
                position: entity.base.position,
                on_ground: entity.base.on_ground,
            })
            .filter(|body| body.bounds.intersects(area))
            .collect();
        found.extend(self.players.iter().filter_map(|(id, player)| {
            let bounds = Aabb::of_size(player.position, PLAYER_WIDTH, player.height() as f32);
            bounds.intersects(area).then_some(Body {
                mover: Mover::Player(*id),
                bounds,
                position: player.position,
                on_ground: player.on_ground,
            })
        }));
        found
    }

    fn refreshed(&self, mover: Mover) -> Option<Body> {
        match mover {
            Mover::Entity(id) => self.entities.store.get(id).map(|entity| Body {
                mover,
                bounds: entity.base.bounding_box(),
                position: entity.base.position,
                on_ground: entity.base.on_ground,
            }),
            Mover::Player(id) => self.players.get(&id).map(|player| Body {
                mover,
                bounds: Aabb::of_size(player.position, PLAYER_WIDTH, player.height() as f32),
                position: player.position,
                on_ground: player.on_ground,
            }),
        }
    }

    fn bounce_off_slime(&mut self, mover: Mover, direction: Face) {
        let Mover::Entity(id) = mover else {
            return;
        };
        if let Some(entity) = self.entities.store.get_mut(id) {
            entity.base.velocity = slime_velocity(entity.base.velocity, direction);
            entity.base.sync_velocity = true;
        }
    }

    fn nudge(&mut self, body: Body, direction: Face, distance: f64) {
        let (dx, dy, dz) = direction.offset();
        let wanted = Vec3::new(
            f64::from(dx) * distance,
            f64::from(dy) * distance,
            f64::from(dz) * distance,
        );
        let tick = self.current_tick();
        let restriction = self.falls.redstone.pushed.entry(body.mover).or_default();
        let allowed = restriction.limit(tick, wanted);
        if allowed.length_sqr() == 0.0 {
            return;
        }
        let moved = collide_bounding_box(&self.world.chunks, allowed, &body.bounds);
        if moved.length_sqr() == 0.0 {
            return;
        }
        match body.mover {
            Mover::Entity(id) => {
                if let Some(entity) = self.entities.store.get_mut(id) {
                    entity.base.position = entity.base.position + moved;
                    entity.base.needs_sync = true;
                }
                self.entities.store.reindex(id);
            }
            Mover::Player(id) => self.slide_player(id, moved),
        }
    }

    fn slide_player(&mut self, id: PlayerId, moved: Vec3) {
        let Some(player) = self.players.get_mut(&id) else {
            return;
        };
        let from = player.position;
        player.position = from + moved;
        let (entity, to, on_ground) = (player.entity_id, player.position, player.on_ground);
        self.push(
            Audience::Trackers(entity),
            ClientEvent::EntityMoved {
                entity,
                from,
                to,
                rotation: None,
                on_ground,
            },
        );
    }

    pub(crate) fn forget_old_pushes(&mut self) {
        let now = self.current_tick();
        self.falls
            .redstone
            .pushed
            .retain(|_, restriction| restriction.tick + RESTRICTION_KEEP >= now);
    }
}

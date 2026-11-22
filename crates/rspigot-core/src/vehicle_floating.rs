use rspigot_domain::entity::{Aabb, BlockView};
use rspigot_domain::movement::FLOATING_DY;
use rspigot_domain::player::PlayerId;
use rspigot_domain::text::Component;
use rspigot_domain::{BlockPos, EntityId, Vec3};
use rspigot_entities::EntityKind as Kind;

use crate::ports::ClientLink;
use crate::server::Server;

const FLOOR_INFLATE: f64 = 0.0625;
const FLOOR_PROBE: f64 = 0.55;
const DEFAULT_GRAVITY: f64 = 0.08;
const SLOW_GRAVITY: f64 = 0.04;
const MINIMUM_GRAVITY: f64 = 1.0e-5;
const BASE_FLYING_TICKS: f64 = 80.0;

pub(crate) fn nothing_around<V: BlockView>(view: &V, area: &Aabb) -> bool {
    let area = area
        .inflate(FLOOR_INFLATE)
        .expand_towards(Vec3::new(0.0, -FLOOR_PROBE, 0.0));
    let floor = |value: f64| value.floor() as i32;
    for y in floor(area.min_y)..=floor(area.max_y) {
        for z in floor(area.min_z)..=floor(area.max_z) {
            for x in floor(area.min_x)..=floor(area.max_x) {
                if view
                    .block(BlockPos::new(x, y, z))
                    .is_none_or(|state| !state.is_air())
                {
                    return false;
                }
            }
        }
    }
    true
}

pub(crate) fn max_flying_ticks(gravity: f64) -> i32 {
    if gravity < MINIMUM_GRAVITY {
        return i32::MAX;
    }
    ((DEFAULT_GRAVITY / gravity).max(1.0) * BASE_FLYING_TICKS).ceil() as i32
}

pub(crate) fn vehicle_gravity(kind: Kind) -> f64 {
    let name = kind.name().trim_start_matches("minecraft:");
    if name.ends_with("boat") || name.ends_with("raft") || name.ends_with("minecart") {
        SLOW_GRAVITY
    } else {
        DEFAULT_GRAVITY
    }
}

impl<L: ClientLink> Server<L> {
    pub(crate) fn clear_vehicle_floating(&mut self, id: PlayerId) {
        if let Some(player) = self.players.get_mut(&id) {
            player.motion.vehicle_floating = false;
            player.motion.vehicle_above_ground = 0;
        }
    }

    pub(crate) fn note_vehicle_floating(&mut self, id: PlayerId, vehicle: EntityId, dy: f64) {
        let Some(entity) = self.entities.store.get(vehicle) else {
            return;
        };
        let flying = entity.base.kind == Kind::HappyGhast;
        let floating = dy >= FLOATING_DY
            && !self.config.allow_flight
            && !flying
            && !entity.base.is_no_gravity()
            && nothing_around(&self.world.chunks, &entity.base.bounding_box());
        if let Some(player) = self.players.get_mut(&id) {
            player.motion.vehicle_floating = floating;
            if !floating {
                player.motion.vehicle_above_ground = 0;
            }
        }
    }

    pub(crate) fn tick_vehicle_floating(&mut self) {
        let ids: Vec<PlayerId> = self
            .players
            .iter()
            .filter(|(_, player)| player.motion.vehicle_floating)
            .map(|(id, _)| *id)
            .collect();
        let mut kicked = Vec::new();
        for id in ids {
            let Some(rider) = self.players.get(&id).map(|player| player.entity_id) else {
                continue;
            };
            let vehicle = self.entities.rides.root_vehicle(rider);
            let controlling = vehicle != rider && self.controlling_player(vehicle) == Some(id);
            let Some(kind) = self.kind_of(vehicle).filter(|_| controlling) else {
                self.clear_vehicle_floating(id);
                continue;
            };
            let Some(player) = self.players.get_mut(&id) else {
                continue;
            };
            player.motion.vehicle_above_ground += 1;
            if player.motion.vehicle_above_ground > max_flying_ticks(vehicle_gravity(kind)) {
                tracing::warn!(
                    "{} was kicked for floating a vehicle too long!",
                    player.profile.name
                );
                kicked.push(id);
            }
        }
        for id in kicked {
            self.disconnect_player(
                id,
                Component::translatable("multiplayer.disconnect.flying", Vec::new()),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn boats_and_minecarts_fall_slower_than_other_vehicles() {
        assert_eq!(vehicle_gravity(Kind::Strider), 0.08);
        assert_eq!(vehicle_gravity(Kind::OakBoat), 0.04);
        assert_eq!(vehicle_gravity(Kind::Minecart), 0.04);
        assert_eq!(vehicle_gravity(Kind::BambooRaft), 0.04);
    }

    #[test]
    fn the_maximum_flying_ticks_follow_the_vehicle_gravity() {
        assert_eq!(max_flying_ticks(0.08), 80);
        assert_eq!(max_flying_ticks(0.04), 160);
        assert_eq!(max_flying_ticks(0.16), 80);
        assert_eq!(max_flying_ticks(0.0), i32::MAX);
    }
}

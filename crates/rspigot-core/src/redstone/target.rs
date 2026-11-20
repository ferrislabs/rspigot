use rspigot_blocks::{BlockKind, Face};
use rspigot_domain::entity::raycast::BlockHit;
use rspigot_domain::{BlockPos, BlockStateId, EntityId, Vec3};
use rspigot_entities::EntityKind;

use super::MAX_POWER;
use crate::ports::ClientLink;
use crate::server::Server;
use crate::signal::level;

const ARROW_PULSE: u64 = 20;
const OTHER_PULSE: u64 = 8;

fn frac(value: f64) -> f64 {
    value - value.floor()
}

fn hit_strength(face: Face, at: Vec3) -> i32 {
    let x = (frac(at.x) - 0.5).abs();
    let y = (frac(at.y) - 0.5).abs();
    let z = (frac(at.z) - 0.5).abs();
    let off_centre = match face {
        Face::Up | Face::Down => x.max(z),
        Face::North | Face::South => x.max(y),
        Face::East | Face::West => y.max(z),
    };
    let closeness = ((0.5 - off_centre) / 0.5).clamp(0.0, 1.0);
    ((closeness * f64::from(MAX_POWER)).ceil() as i32).max(1)
}

fn is_arrow(kind: EntityKind) -> bool {
    matches!(
        kind,
        EntityKind::Arrow | EntityKind::SpectralArrow | EntityKind::Trident
    )
}

impl<L: ClientLink> Server<L> {
    pub(crate) fn projectile_hit_state(&mut self, projectile: EntityId, hit: BlockHit) {
        let state = self.block_or_air(hit.pos);
        if state.kind() == Some(BlockKind::Target) {
            self.target_hit(projectile, hit, state);
        }
    }

    fn target_hit(&mut self, projectile: EntityId, hit: BlockHit, state: BlockStateId) {
        self.count_target_hit(projectile);
        let pulse = match self.entities.store.get(projectile) {
            Some(entity) if is_arrow(entity.base.kind) => ARROW_PULSE,
            _ => OTHER_PULSE,
        };
        let strength = hit_strength(hit.face, hit.location);
        self.trigger_target_hit(projectile, hit.location, strength);
        if self.has_scheduled(hit.pos, BlockKind::Target) {
            return;
        }
        self.set_target_power(hit.pos, state, strength);
        self.schedule(hit.pos, BlockKind::Target, pulse);
    }

    fn set_target_power(&mut self, pos: BlockPos, state: BlockStateId, power: i32) {
        if let Some(next) = state.with_property("power", &power.to_string()) {
            self.set_block(pos, next);
        }
    }

    pub(crate) fn target_tick(&mut self, pos: BlockPos, state: BlockStateId) {
        if level(state, "power") != 0 {
            self.set_target_power(pos, state, 0);
        }
    }

    pub(crate) fn target_placed(&mut self, pos: BlockPos, state: BlockStateId) {
        if level(state, "power") > 0 && !self.has_scheduled(pos, BlockKind::Target) {
            self.set_block_quiet(pos, state.with_property("power", "0").unwrap_or(state));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_hit_at_the_centre_of_a_face_gives_fifteen() {
        assert_eq!(hit_strength(Face::Up, Vec3::new(0.5, 1.0, 0.5)), 15);
    }

    #[test]
    fn a_hit_on_the_edge_gives_one() {
        assert_eq!(hit_strength(Face::Up, Vec3::new(0.0, 1.0, 0.5)), 1);
        assert_eq!(hit_strength(Face::North, Vec3::new(0.5, 0.99, 0.0)), 1);
    }

    #[test]
    fn the_strength_falls_with_the_distance_to_the_centre_of_the_face() {
        assert_eq!(hit_strength(Face::East, Vec3::new(1.0, 0.75, 0.5)), 8);
        assert_eq!(hit_strength(Face::South, Vec3::new(0.25, 0.5, 1.0)), 8);
    }
}

use rand::Rng;
use rspigot_domain::EntityId;
use rspigot_entities::EntityKind as Kind;

use crate::mob_life::type_in_tag;
use crate::ports::ClientLink;
use crate::server::Server;

const DEFLECTS_PROJECTILES: &str = "minecraft:deflects_projectiles";
const REVERSE_SCALE: f64 = -0.1;
const REVERSE_BASE_YAW: f32 = 170.0;
const REVERSE_YAW_SPREAD: f32 = 20.0;

impl<L: ClientLink> Server<L> {
    pub(crate) fn deflected_on_hit(&mut self, projectile: EntityId, target: EntityId) -> bool {
        let Some(kind) = self.kind_of(target) else {
            return false;
        };
        if !type_in_tag(kind, DEFLECTS_PROJECTILES) {
            return false;
        }
        let Some(shot) = self.kind_of(projectile) else {
            return false;
        };
        if kind == Kind::Breeze && matches!(shot, Kind::WindCharge | Kind::BreezeWindCharge) {
            return false;
        }
        let roll = self.rng.random::<f32>();
        let Some(entity) = self.entities.store.get_mut(projectile) else {
            return false;
        };
        if entity.base.last_deflected_by != Some(target) {
            entity.base.velocity = entity.base.velocity.scale(REVERSE_SCALE);
            let yaw = entity.base.yaw + REVERSE_BASE_YAW + roll * REVERSE_YAW_SPREAD;
            entity.base.set_yaw(yaw);
            entity.base.last_deflected_by = Some(target);
            entity.base.needs_sync = true;
        }
        true
    }
}

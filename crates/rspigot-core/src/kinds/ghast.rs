use rspigot_ai::{GhastAction, MobView};
use rspigot_domain::entity::species::{Species, ghast};
use rspigot_domain::entity::{Entity, EntityState};
use rspigot_domain::{EntityId, Vec3};
use rspigot_entities::EntityKind as Kind;

use crate::health::DamageSource;
use crate::ports::ClientLink;
use crate::server::Server;

fn power_of(entity: &Entity) -> i32 {
    match &entity.state {
        EntityState::Living(living) => match living.mob.as_ref().map(|mob| mob.species) {
            Some(Species::Ghast(state)) => state.explosion_power,
            _ => ghast::DEFAULT_EXPLOSION_POWER,
        },
        _ => ghast::DEFAULT_EXPLOSION_POWER,
    }
}

impl<L: ClientLink> Server<L> {
    pub(crate) fn ghast_action(&mut self, id: EntityId, action: GhastAction) {
        match action {
            GhastAction::SetCharging(charging) => {
                if let Some(entity) = self.entities.store.get_mut(id) {
                    ghast::set_charging(&mut entity.base, charging);
                }
            }
            GhastAction::Fireball { direction, view } => self.ghast_shoot(id, direction, view),
        }
    }

    fn ghast_shoot(&mut self, id: EntityId, direction: Vec3, view: Vec3) {
        let Some(entity) = self.entities.store.get(id) else {
            return;
        };
        let base = &entity.base;
        let power = power_of(entity);
        let height_offset = f64::from(base.dimensions.height) * 0.5 + 0.5;
        let position = Vec3::new(
            base.position.x + view.x * ghast::FIREBALL_REACH,
            base.position.y + height_offset,
            base.position.z + view.z * ghast::FIREBALL_REACH,
        );
        self.launch_hurting(Kind::Fireball, id, direction, position, power);
    }

    pub(crate) fn ghast_view(&self, _entity: &Entity, _view: &mut MobView) {}

    pub(crate) fn ghast_pre_tick(&mut self, _id: EntityId) -> bool {
        true
    }

    pub(crate) fn ghast_hurt_amount(
        &mut self,
        _id: EntityId,
        source: &DamageSource,
        amount: f32,
    ) -> Option<f32> {
        let reflected = source
            .direct
            .as_ref()
            .is_some_and(|direct| direct.kind == Kind::Fireball)
            && source
                .causing
                .as_ref()
                .is_some_and(|causing| causing.kind == Kind::Player);
        Some(if reflected {
            ghast::REFLECTED_DAMAGE
        } else {
            amount
        })
    }
}

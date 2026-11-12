use rspigot_ai::{BreezeAction, breeze_firing_y_position};
use rspigot_domain::{EntityId, Vec3};
use rspigot_entities::EntityKind as Kind;

use crate::health::DamageSource;
use crate::ports::ClientLink;
use crate::server::Server;

const SHOOT_POWER: f32 = 0.7;
const BASE_INACCURACY: i32 = 5;
const INACCURACY_PER_DIFFICULTY: i32 = 4;

pub(crate) fn shoot_inaccuracy(difficulty: u8) -> f32 {
    (BASE_INACCURACY - i32::from(difficulty) * INACCURACY_PER_DIFFICULTY) as f32
}

impl<L: ClientLink> Server<L> {
    pub(crate) fn breeze_action(&mut self, id: EntityId, action: BreezeAction) {
        match action {
            BreezeAction::Shoot { direction } => self.breeze_shoot(id, direction),
        }
    }

    fn breeze_shoot(&mut self, id: EntityId, direction: Vec3) {
        let Some(entity) = self.entities.store.get(id) else {
            return;
        };
        let base = &entity.base;
        let origin = Vec3::new(
            base.position.x,
            breeze_firing_y_position(base.position.y, base.dimensions.height),
            base.position.z,
        );
        let uncertainty = shoot_inaccuracy(self.world.difficulty.id());
        self.launch_wind_charge(
            Kind::BreezeWindCharge,
            id,
            origin,
            direction,
            SHOOT_POWER,
            uncertainty,
        );
    }

    pub(crate) fn breeze_hurt_amount(
        &mut self,
        _id: EntityId,
        source: &DamageSource,
        amount: f32,
    ) -> Option<f32> {
        let by_breeze = source
            .causing
            .as_ref()
            .is_some_and(|causing| self.kind_of(causing.entity) == Some(Kind::Breeze));
        (!by_breeze).then_some(amount)
    }
}

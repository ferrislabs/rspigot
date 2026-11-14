use rspigot_ai::{BatAction, BatFacts, MobView};
use rspigot_domain::entity::Entity;
use rspigot_domain::entity::species::bat;
use rspigot_domain::random::RandomSource;
use rspigot_domain::{EntityId, Vec3};

use crate::health::DamageSource;
use crate::ports::ClientLink;
use crate::server::Server;

const AMBIENT_SILENCE_ODDS: i32 = 4;

impl<L: ClientLink> Server<L> {
    pub(crate) fn bat_view(&self, entity: &Entity, view: &mut MobView) {
        view.species.bat = BatFacts {
            resting: bat::is_resting(&entity.base),
        };
    }

    pub(crate) fn bat_action(&mut self, id: EntityId, action: BatAction) {
        let BatAction::ToggleSleep { awake } = action;
        let Some(entity) = self.entities.store.get_mut(id) else {
            return;
        };
        let silent = entity.base.is_silent();
        let pos = entity.base.position.block();
        bat::set_resting(&mut entity.base, !awake);
        if awake && !silent {
            self.level_event(bat::WAKE_EVENT, pos, 0);
        }
    }

    pub(crate) fn bat_post_tick(&mut self, id: EntityId) {
        let Some(entity) = self.entities.store.get_mut(id) else {
            return;
        };
        let base = &mut entity.base;
        if bat::is_resting(base) {
            base.velocity = Vec3::ZERO;
            let y = base.position.y.floor() + 1.0 - f64::from(base.dimensions.height);
            base.position = Vec3::new(base.position.x, y, base.position.z);
            base.needs_sync = true;
        } else {
            base.velocity = base.velocity.multiply(1.0, 0.6, 1.0);
        }
    }

    pub(crate) fn bat_hurt_amount(
        &mut self,
        id: EntityId,
        _source: &DamageSource,
        amount: f32,
    ) -> Option<f32> {
        if let Some(entity) = self.entities.store.get_mut(id) {
            bat::set_resting(&mut entity.base, false);
        }
        Some(amount)
    }

    pub(crate) fn bat_ambient_voice(&mut self, id: EntityId) {
        let Some(resting) = self
            .entities
            .store
            .get(id)
            .map(|entity| bat::is_resting(&entity.base))
        else {
            return;
        };
        if resting {
            let mut random = self.mob_random(id);
            let silent = random.next_int(AMBIENT_SILENCE_ODDS) != 0;
            self.store_mob_random(id, random);
            if silent {
                return;
            }
        }
        self.play_mob_voice(id, |_| Some("minecraft:entity.bat.ambient"));
    }
}

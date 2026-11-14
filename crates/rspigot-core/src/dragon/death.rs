use rspigot_domain::entity::species::ender_dragon;
use rspigot_domain::entity::{Entity, EntityState, RemovalReason};
use rspigot_domain::game_event::GameEvent;
use rspigot_domain::{EntityId, Vec3};

use super::ai::state_of;
use crate::game_events::GameEventContext;
use crate::plugins::events::EntityRemoveCause;
use crate::ports::ClientLink;
use crate::server::Server;

const DEATH_LIFT: f64 = 0.1;

impl<L: ClientLink> Server<L> {
    pub(super) fn dragon_tick_death(&mut self, id: EntityId) {
        self.dragon_update_fight(id);
        let mob_drops = self.config.rules.mob_drops;
        let Some(entity) = self.entities.store.get_mut(id) else {
            return;
        };
        let silent = entity.base.is_silent();
        let position = entity.base.position;
        let Some(state) = state_of(entity) else {
            return;
        };
        state.death_time += 1;
        let death_time = state.death_time;
        let exp = state.exp_to_drop;
        let burst = ender_dragon::death_experience(exp, death_time, mob_drops);
        if burst > 0 {
            self.award_experience(position, burst);
        }
        if death_time == 1 && !silent {
            self.global_level_event_near(position, ender_dragon::DEATH_SOUND_EVENT);
        }
        let lift = Vec3::new(0.0, DEATH_LIFT, 0.0);
        if let Some(entity) = self.entities.store.get_mut(id) {
            entity.base.position = entity.base.position + lift;
        }
        if let Some(runtime) = self.entities.dragons.get_mut(&id) {
            runtime.old_parts = runtime.parts;
            for part in &mut runtime.parts {
                *part = *part + lift;
            }
        }
        if death_time >= ender_dragon::DEATH_END {
            let at = self
                .entities
                .store
                .get(id)
                .map_or(position, |entity| entity.base.position);
            let last = ender_dragon::final_experience(exp);
            if last > 0 {
                self.award_experience(at, last);
            }
            self.dragon_finish_fight(id);
            self.remove_entity(id, RemovalReason::Killed, Some(EntityRemoveCause::Death));
            self.game_event(GameEvent::EntityDie, at, GameEventContext::of(Some(id)));
        }
    }

    pub(crate) fn dragon_kill(&mut self, id: EntityId) {
        let at = self
            .entities
            .store
            .get(id)
            .map(|entity| entity.base.position);
        self.dragon_update_fight(id);
        self.dragon_finish_fight(id);
        self.remove_entity(id, RemovalReason::Killed, Some(EntityRemoveCause::Death));
        if let Some(at) = at {
            self.game_event(GameEvent::EntityDie, at, GameEventContext::of(Some(id)));
        }
    }

    pub(crate) fn dragon_store_experience(&mut self, id: EntityId, amount: i32) {
        if let Some(Entity {
            state: EntityState::Living(_),
            ..
        }) = self.entities.store.get(id)
            && let Some(entity) = self.entities.store.get_mut(id)
            && let Some(state) = state_of(entity)
        {
            state.exp_to_drop = amount;
        }
    }

    pub(crate) fn dragon_experience_reward(&self, id: EntityId) -> i32 {
        let uuid = self.entities.store.get(id).map(|entity| entity.base.uuid);
        let previously = self
            .entities
            .fight
            .as_ref()
            .filter(|fight| fight.data.dragon_uuid == uuid)
            .map(|fight| fight.data.previously_killed);
        ender_dragon::experience_reward(previously, self.config.rules.mob_drops)
    }
}

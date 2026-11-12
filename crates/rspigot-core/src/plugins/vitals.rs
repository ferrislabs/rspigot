use rspigot_domain::EntityId;
use rspigot_domain::damage::java_clamp;
use rspigot_domain::entity::living;
use rspigot_domain::entity::{Entity, EntityState};
use rspigot_domain::player::PlayerId;
use rspigot_domain::world::Difficulty;
use rspigot_entities::EntityKind as Kind;

use crate::events::ClientEvent;
use crate::health::max_health;
use crate::plugins::EventPayload;
use crate::plugins::decisions::{Continuation, Lane};
use crate::plugins::events::{
    EntityExhaustion, EntityExhaustionPatch, EntityExhaustionReason, EntityRegainHealth,
    EntityRegainHealthPatch, EntityRegainHealthRegainReason, FoodLevelChange, FoodLevelChangePatch,
};
use crate::plugins::steps::{ExhaustStep, FoodStep, MobRegainStep, RegainStep};
use crate::ports::ClientLink;
use crate::server::Server;

impl<L: ClientLink> Server<L> {
    fn player_lane(&self, id: PlayerId) -> Option<Lane> {
        let entity = self.players.get(&id)?.entity_id;
        Some(self.lane_for(Some(id), entity))
    }

    pub(crate) fn heal_player(
        &mut self,
        id: PlayerId,
        amount: f32,
        reason: EntityRegainHealthRegainReason,
    ) {
        if self
            .players
            .get(&id)
            .is_none_or(|player| player.vitals.health <= 0.0)
        {
            return;
        }
        let Some(lane) = self.player_lane(id) else {
            return;
        };
        let step = RegainStep { player: id, amount };
        self.fire(lane, Continuation::Regain(step), move |server| {
            let entity = server.subject_view(server.players.get(&id)?.entity_id)?;
            Some(EventPayload::EntityRegainHealth(EntityRegainHealth {
                amount: f64::from(amount),
                entity,
                entity_type: Kind::Player.name().to_owned(),
                regain_reason: reason,
            }))
        });
    }

    pub(crate) fn heal_mob(
        &mut self,
        cause: Option<PlayerId>,
        id: EntityId,
        amount: f32,
        reason: EntityRegainHealthRegainReason,
    ) {
        let alive =
            self.entities.store.get(id).is_some_and(|entity| {
                entity.living().is_some() && living::health(&entity.base) > 0.0
            });
        if !alive {
            return;
        }
        let lane = self.lane_for(cause, id);
        let step = MobRegainStep { target: id, amount };
        self.fire(lane, Continuation::MobRegain(step), move |server| {
            let entity = server.entity_view(id)?;
            Some(EventPayload::EntityRegainHealth(EntityRegainHealth {
                amount: f64::from(amount),
                entity_type: entity.kind.clone(),
                entity,
                regain_reason: reason,
            }))
        });
    }

    pub(super) fn finish_mob_regain(
        &mut self,
        step: MobRegainStep,
        cancelled: bool,
        patch: EntityRegainHealthPatch,
    ) {
        if cancelled {
            return;
        }
        let amount = patch.amount.map_or(step.amount, |amount| amount as f32);
        if let Some(Entity {
            base,
            state: EntityState::Living(state),
            ..
        }) = self.entities.store.get_mut(step.target)
        {
            let health = living::health(base);
            if health > 0.0 {
                living::set_health(base, state, health + amount);
            }
        }
    }

    pub(super) fn finish_regain(
        &mut self,
        step: RegainStep,
        cancelled: bool,
        patch: EntityRegainHealthPatch,
    ) {
        if cancelled {
            return;
        }
        let Some(player) = self.players.get_mut(&step.player) else {
            return;
        };
        let amount = patch.amount.map_or(step.amount, |amount| amount as f32);
        let max = max_health(player);
        let vitals = &mut player.vitals;
        if vitals.health > 0.0 {
            vitals.health = java_clamp(vitals.health + amount, 0.0, max);
        }
    }

    pub(crate) fn exhaust(&mut self, id: PlayerId, amount: f32, reason: EntityExhaustionReason) {
        if self
            .players
            .get(&id)
            .is_none_or(|player| player.abilities().invulnerable)
        {
            return;
        }
        let Some(lane) = self.player_lane(id) else {
            return;
        };
        let step = ExhaustStep { player: id, amount };
        self.fire(lane, Continuation::Exhaust(step), move |server| {
            Some(EventPayload::EntityExhaustion(EntityExhaustion {
                entity: server.snapshot_of(id)?,
                entity_type: Kind::Player.name().to_owned(),
                exhaustion: amount,
                exhaustion_reason: reason,
            }))
        });
    }

    pub(super) fn finish_exhaust(
        &mut self,
        step: ExhaustStep,
        cancelled: bool,
        patch: EntityExhaustionPatch,
    ) {
        if cancelled {
            return;
        }
        if let Some(player) = self.players.get_mut(&step.player)
            && !player.abilities().invulnerable
        {
            player
                .vitals
                .food
                .add_exhaustion(patch.exhaustion.unwrap_or(step.amount));
        }
    }

    pub(crate) fn change_food(&mut self, id: PlayerId, level: i32, from: Option<i32>) {
        let Some(lane) = self.player_lane(id) else {
            return;
        };
        let step = FoodStep {
            player: id,
            level,
            from,
        };
        self.fire(lane, Continuation::Food(step), move |server| {
            Some(EventPayload::FoodLevelChange(FoodLevelChange {
                entity: server.snapshot_of(id)?,
                entity_type: Kind::Player.name().to_owned(),
                food_level: level,
                item: None,
            }))
        });
    }

    pub(super) fn finish_food(
        &mut self,
        step: FoodStep,
        cancelled: bool,
        patch: FoodLevelChangePatch,
    ) {
        let peaceful = self.world.difficulty == Difficulty::Peaceful;
        let Some(player) = self.players.get_mut(&step.player) else {
            return;
        };
        if player.vitals.is_dead() {
            return;
        }
        let food = &mut player.vitals.food;
        let level = patch.food_level.unwrap_or(step.level).max(0);
        let stale = step.from.is_none() && (peaceful || food.saturation > 0.0);
        if cancelled || stale {
            let event = ClientEvent::Health {
                health: player.vitals.health,
                food: food.food,
                saturation: food.saturation,
            };
            player.link.send(event);
            return;
        }
        match step.from {
            Some(old) => food.eat(level - old, 1.0),
            None => food.food = level,
        }
    }
}

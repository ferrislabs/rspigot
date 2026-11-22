use rspigot_domain::entity::decor::{self, PlayerAction};
use rspigot_domain::entity::{Entity, EntityState, RemovalReason};
use rspigot_domain::game_event::GameEvent;
use rspigot_domain::item::ItemStack;
use rspigot_domain::player::PlayerId;
use rspigot_domain::trial_spawner as trial;
use rspigot_domain::{EntityId, Vec3};
use rspigot_entities::{EntityKind as Kind, SoundSource};

use crate::events::Hand;
use crate::game_events::GameEventContext;
use crate::interact::Interaction;
use crate::items::PICKUP_DELAY;
use crate::mobs::ServerRandom;
use crate::plugins::events::{CreatureSpawnReason, EntityRemoveCause};
use crate::ports::ClientLink;
use crate::server::Server;

impl<L: ClientLink> Server<L> {
    pub(crate) fn spawn_ominous_item_spawner(&mut self, position: Vec3, stack: ItemStack) {
        let mut base = self.new_entity_base(Kind::OminousItemSpawner);
        base.position = position;
        base.old_position = position;
        let mut state = decor::define(&mut base);
        state.spawn_item_after_ticks = decor::delay(&mut ServerRandom(&mut self.rng));
        decor::set_item(&mut base, Some(stack));
        let entity = Entity::new(base, EntityState::Decor(Box::new(state)));
        self.add_entity(entity, CreatureSpawnReason::Default);
    }

    pub(crate) fn tick_decor(&mut self, id: EntityId) {
        let Some(entity) = self.entities.store.get_mut(id) else {
            return;
        };
        entity.base.tick_count = entity.base.tick_count.wrapping_add(1);
        if entity.base.kind != Kind::OminousItemSpawner {
            return;
        }
        let EntityState::Decor(state) = &entity.state else {
            return;
        };
        let position = entity.base.position;
        let about_to_spawn = decor::about_to_spawn(&entity.base, state);
        let due = decor::due(&entity.base, state);
        if about_to_spawn {
            self.play_sound(
                None,
                position,
                trial::ABOUT_TO_SPAWN_ITEM_SOUND,
                SoundSource::Neutral,
                1.0,
                1.0,
            );
        }
        if due {
            self.ominous_item_spawner_fires(id, position);
        }
    }

    fn ominous_item_spawner_fires(&mut self, id: EntityId, position: Vec3) {
        let stack = self.entities.store.get_mut(id).and_then(|entity| {
            let stack = decor::item(&entity.base);
            decor::set_item(&mut entity.base, None);
            stack
        });
        if let Some(stack) = stack {
            if let Some(shot) = crate::redstone::dispenser::shot_of(&stack) {
                self.launch_shot(shot, position, Vec3::new(0.0, -1.0, 0.0), &stack);
            } else {
                self.spawn_item_entity(position, Vec3::ZERO, PICKUP_DELAY, stack);
            }
            self.level_event(trial::ITEM_SPAWNED_EVENT, position.block(), 1);
            self.game_event(GameEvent::EntityPlace, position, GameEventContext::of(None));
        }
        self.remove_entity(id, RemovalReason::Killed, Some(EntityRemoveCause::Death));
    }

    pub(crate) fn interaction_attack(&mut self, player: PlayerId, target: EntityId) -> bool {
        if self.kind_of(target) != Some(Kind::Interaction) {
            return false;
        }
        let Some(uuid) = self.players.get(&player).map(|p| p.profile.id.0) else {
            return false;
        };
        let now = self.world.level.game_time;
        if let Some(Entity {
            state: EntityState::Decor(state),
            ..
        }) = self.entities.store.get_mut(target)
        {
            state.attack = Some(PlayerAction {
                player: uuid,
                timestamp: now,
            });
        }
        true
    }

    pub(crate) fn interaction_interact(
        &mut self,
        player: PlayerId,
        target: EntityId,
        _hand: Hand,
    ) -> Option<Interaction> {
        if self.kind_of(target) != Some(Kind::Interaction) {
            return None;
        }
        let uuid = self.players.get(&player).map(|p| p.profile.id.0)?;
        let now = self.world.level.game_time;
        let Some(Entity {
            state: EntityState::Decor(state),
            ..
        }) = self.entities.store.get_mut(target)
        else {
            return None;
        };
        state.interaction = Some(PlayerAction {
            player: uuid,
            timestamp: now,
        });
        Some(Interaction::Consume)
    }
}

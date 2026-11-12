use rspigot_ai::{MobAi, MobView};
use rspigot_domain::entity::species::{Species, rabbit};
use rspigot_domain::entity::{Entity, EntityState};
use rspigot_domain::item::ItemStack;
use rspigot_domain::player::PlayerId;
use rspigot_domain::random::RandomSource;
use rspigot_domain::{EntityId, Vec3};
use rspigot_entities::SoundSource;

use crate::events::Hand;
use crate::health::DamageSource;
use crate::interact::Interaction;
use crate::ports::ClientLink;
use crate::server::Server;

const JUMP_SOUND: &str = "minecraft:entity.rabbit.jump";
const ATTACK_SOUND: &str = "minecraft:entity.rabbit.attack";
const JUMP_EVENT: i8 = 1;

pub(super) fn sound_source(entity: &Entity) -> SoundSource {
    let evil = matches!(entity.state, EntityState::Living(_)) && rabbit::is_evil(&entity.base);
    if evil {
        SoundSource::Hostile
    } else {
        SoundSource::Neutral
    }
}

fn state_of(entity: &Entity) -> Option<rabbit::RabbitState> {
    match entity.living()?.mob.as_ref()?.species {
        Species::Rabbit(state) => Some(state),
        _ => None,
    }
}

impl<L: ClientLink> Server<L> {
    pub(crate) fn rabbit_view(&self, entity: &Entity, view: &mut MobView) {
        view.evil = rabbit::is_evil(&entity.base);
        view.more_carrot_ticks = state_of(entity).map_or(0, |state| state.more_carrot_ticks);
    }

    pub(super) fn rabbit_registered(&self, id: EntityId, ai: &mut MobAi) {
        if self
            .entities
            .store
            .get(id)
            .is_some_and(|entity| rabbit::is_evil(&entity.base))
        {
            ai.become_killer_bunny();
        }
    }

    pub(crate) fn rabbit_pre_tick(&mut self, _id: EntityId) -> bool {
        true
    }

    pub(crate) fn rabbit_post_tick(&mut self, id: EntityId) {
        let Some(entry) = self.entities.mobs.entries.get_mut(&id) else {
            return;
        };
        let Some(Entity {
            state: EntityState::Living(living),
            ..
        }) = self.entities.store.get_mut(id)
        else {
            return;
        };
        let brain = entry.ai.brain_mut();
        brain.rabbit_post_ai_step(living);
        let jumped = std::mem::take(&mut living.jumped);
        let sounds = brain.take_hop_sounds();
        if jumped {
            self.mob_entity_event(id, JUMP_EVENT);
        }
        for sound in sounds {
            self.rabbit_sound(id, sound.position, JUMP_SOUND, 1.0, sound.pitch);
        }
    }

    pub(crate) fn rabbit_custom_ai_step(&mut self, id: EntityId) {
        let target = self
            .entities
            .mobs
            .entries
            .get(&id)
            .and_then(|entry| entry.ai.target())
            .and_then(|target| self.entities.store.get(target))
            .map(|entity| entity.base.position);
        let Some(entry) = self.entities.mobs.entries.get_mut(&id) else {
            return;
        };
        let Some(Entity {
            base,
            state: EntityState::Living(living),
            ..
        }) = self.entities.store.get_mut(id)
        else {
            return;
        };
        let evil = rabbit::is_evil(base);
        let Some(Species::Rabbit(state)) = living.mob.as_mut().map(|mob| &mut mob.species) else {
            return;
        };
        let mut carrots = state.more_carrot_ticks;
        let mut copy = *state;
        entry
            .ai
            .brain_mut()
            .rabbit_ai_step(base, living, evil, target, &mut carrots);
        copy.more_carrot_ticks = carrots;
        if let Some(Species::Rabbit(state)) = living.mob.as_mut().map(|mob| &mut mob.species) {
            *state = copy;
        }
    }

    fn rabbit_sound(
        &mut self,
        id: EntityId,
        at: Vec3,
        sound: &'static str,
        volume: f32,
        pitch: f32,
    ) {
        let Some(entity) = self.entities.store.get(id) else {
            return;
        };
        if entity.base.is_silent() {
            return;
        }
        let source = sound_source(entity);
        self.play_sound(None, at, sound, source, volume, pitch);
    }

    pub(super) fn rabbit_attack_sound(&mut self, id: EntityId) {
        let Some(entity) = self.entities.store.get(id) else {
            return;
        };
        if !rabbit::is_evil(&entity.base) {
            return;
        }
        let at = entity.base.position;
        let mut random = self.mob_random(id);
        let first = random.next_float();
        let second = random.next_float();
        self.store_mob_random(id, random);
        self.rabbit_sound(id, at, ATTACK_SOUND, 1.0, (first - second) * 0.2 + 1.0);
    }

    pub(crate) fn rabbit_interact(
        &mut self,
        _player: PlayerId,
        _target: EntityId,
        _hand: Hand,
        _held: Option<&ItemStack>,
    ) -> Option<Interaction> {
        None
    }

    pub(crate) fn rabbit_hurt_amount(
        &mut self,
        _id: EntityId,
        _source: &DamageSource,
        amount: f32,
    ) -> Option<f32> {
        Some(amount)
    }

    pub(crate) fn rabbit_after_hurt(&mut self, _id: EntityId, _source: &DamageSource, _dead: bool) {
    }

    pub(crate) fn rabbit_ate_carrot(&mut self, id: EntityId) {
        if let Some(Species::Rabbit(state)) = self
            .entities
            .store
            .get_mut(id)
            .and_then(Entity::living_mut)
            .and_then(|living| living.mob.as_mut())
            .map(|mob| &mut mob.species)
        {
            state.more_carrot_ticks = rabbit::MORE_CARROT_TICKS;
        }
    }
}

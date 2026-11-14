use std::num::NonZeroU8;

use rspigot_ai::{GoatAction, MobView};
use rspigot_domain::attribute::ATTACK_DAMAGE;
use rspigot_domain::damage::DamageType;
use rspigot_domain::entity::species::Species;
use rspigot_domain::entity::species::goat::{self, GoatState};
use rspigot_domain::entity::{Entity, EntityState};
use rspigot_domain::item::ItemStack;
use rspigot_domain::random::RandomSource;
use rspigot_domain::{EntityId, Vec3};
use rspigot_items::ItemKind;

use crate::commands::Target;
use crate::health::DamageSource;
use crate::mobs::ServerRandom;
use crate::plugins::events::EntityKnockbackCause;
use crate::plugins::steps::{AfterHurt, RamFollow, Struck};
use crate::ports::ClientLink;
use crate::server::Server;

const HORN_BREAK_SOUND: &str = "minecraft:entity.goat.horn_break";
const HORN_PICKUP_DELAY: u16 = 10;
const HORN_SPREAD: f32 = 0.2;
const EAT_PITCH_MIN: f32 = 0.8;
const EAT_PITCH_MAX: f32 = 1.2;
const HORN_LIFT_MIN: f32 = 0.3;
const HORN_LIFT_MAX: f32 = 0.7;
const SOUNDS: [[&str; 2]; 5] = [
    [
        "minecraft:entity.goat.ambient",
        "minecraft:entity.goat.screaming.ambient",
    ],
    [
        "minecraft:entity.goat.hurt",
        "minecraft:entity.goat.screaming.hurt",
    ],
    [
        "minecraft:entity.goat.death",
        "minecraft:entity.goat.screaming.death",
    ],
    [
        "minecraft:entity.goat.milk",
        "minecraft:entity.goat.screaming.milk",
    ],
    [
        "minecraft:entity.goat.eat",
        "minecraft:entity.goat.screaming.eat",
    ],
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum GoatSound {
    Ambient,
    Hurt,
    Death,
    Milk,
    Eat,
}

pub(crate) fn sound(screaming: bool, which: GoatSound) -> &'static str {
    SOUNDS[which as usize][usize::from(screaming)]
}

pub(crate) fn voice(entity: &Entity, which: GoatSound) -> &'static str {
    sound(goat::is_screaming(&entity.base), which)
}

fn goat_state(entity: &mut Entity) -> Option<&mut GoatState> {
    match entity
        .living_mut()
        .and_then(|state| state.mob.as_mut())
        .map(|mob| &mut mob.species)
    {
        Some(Species::Goat(state)) => Some(state),
        _ => None,
    }
}

impl<L: ClientLink> Server<L> {
    pub(crate) fn goat_voice_sound(&mut self, id: EntityId, which: GoatSound) -> bool {
        self.play_mob_voice(id, move |entity| Some(voice(entity, which)));
        true
    }

    pub(crate) fn goat_eat_sound(&mut self, id: EntityId) {
        let Some(screaming) = self
            .entities
            .store
            .get(id)
            .map(|entity| goat::is_screaming(&entity.base))
        else {
            return;
        };
        let mut random = self.mob_random(id);
        let pitch = EAT_PITCH_MIN + random.next_float() * (EAT_PITCH_MAX - EAT_PITCH_MIN);
        self.store_mob_random(id, random);
        self.entity_sound(id, sound(screaming, GoatSound::Eat), 1.0, pitch);
    }

    pub(crate) fn goat_view(&self, entity: &Entity, view: &mut MobView) {
        view.species.goat.screaming = goat::is_screaming(&entity.base);
    }

    pub(crate) fn goat_registered(&mut self, id: EntityId, ai: &mut rspigot_ai::MobAi) {
        let pending = self
            .entities
            .store
            .get_mut(id)
            .and_then(goat_state)
            .and_then(|state| state.pending_memories.take());
        if let Some((long_jump, ram)) = pending {
            ai.init_goat_memories(long_jump, ram);
        }
    }

    pub(crate) fn goat_custom_ai_step(&mut self, id: EntityId) {
        let Some(entity) = self.entities.store.get_mut(id) else {
            return;
        };
        let baby = entity.is_baby();
        if let Some(living) = entity.living_mut() {
            goat::age_boundary_reached(&mut living.attributes, baby);
        }
    }

    pub(crate) fn goat_offspring(
        &mut self,
        child: &mut Entity,
        parent: EntityId,
        partner: EntityId,
        random: &mut impl RandomSource,
    ) {
        let screaming = |server: &Self, id: EntityId| {
            server
                .entities
                .store
                .get(id)
                .is_some_and(|entity| goat::is_screaming(&entity.base))
        };
        let own = screaming(self, parent);
        let other = screaming(self, partner);
        let memories = goat::sample_memories(&mut ServerRandom(&mut self.rng));
        let inherited = goat::offspring_screaming(own, other, random);
        goat::set_screaming(&mut child.base, inherited);
        if let EntityState::Living(living) = &mut child.state
            && let Some(Species::Goat(state)) = living.mob.as_mut().map(|mob| &mut mob.species)
        {
            state.pending_memories = Some(memories);
        }
    }

    pub(crate) fn goat_action(&mut self, id: EntityId, action: GoatAction) {
        match action {
            GoatAction::Ram {
                target,
                speed,
                force,
                direction,
            } => self.goat_ram(id, target, speed, force, direction),
            GoatAction::DropHorn => self.goat_drop_horn(id),
        }
    }

    fn target_of(&self, entity: EntityId) -> Option<Target> {
        self.players
            .iter()
            .find(|(_, player)| player.entity_id == entity)
            .map(|(player, _)| Target::Player(*player))
            .or_else(|| {
                self.entities
                    .store
                    .get(entity)
                    .is_some_and(|found| found.living().is_some())
                    .then_some(Target::Entity(entity))
            })
    }

    fn goat_ram(
        &mut self,
        id: EntityId,
        target: EntityId,
        speed: f32,
        force: f64,
        direction: Vec3,
    ) {
        let Some(target) = self.target_of(target) else {
            return;
        };
        let Some(source_entity) = self.mob_source(id) else {
            return;
        };
        let Some(damage) = self
            .entities
            .store
            .get_mut(id)
            .and_then(Entity::living_mut)
            .map(|state| state.attribute(ATTACK_DAMAGE) as f32)
        else {
            return;
        };
        let source = DamageSource::by(DamageType::MobAttackNoAggro, source_entity);
        let victim = match target {
            Target::Player(player) => Struck::Player(player),
            Target::Entity(entity) => Struck::Mob(entity),
        };
        let follow = RamFollow {
            goat: id,
            target,
            strength: f64::from(speed) * force,
            direction: (direction.x, direction.z),
        };
        self.strike(None, victim, &source, damage, AfterHurt::Ram(follow));
    }

    pub(crate) fn finish_ram(&mut self, follow: RamFollow) {
        let RamFollow {
            goat,
            target,
            strength,
            direction,
        } = follow;
        self.knockback_target(
            target,
            strength,
            direction,
            EntityKnockbackCause::EntityAttack,
            goat,
        );
    }

    fn goat_drop_horn(&mut self, id: EntityId) {
        let mut random = self.mob_random(id);
        let Some(entity) = self.entities.store.get_mut(id) else {
            return;
        };
        let baby = entity.is_baby();
        if !goat::drop_horn(&mut entity.base, baby, &mut random) {
            self.store_mob_random(id, random);
            return;
        }
        let position = entity.base.position;
        let instrument = goat::horn_instrument(entity.base.uuid, goat::is_screaming(&entity.base));
        let mut horn = ItemStack::new(ItemKind::GoatHorn.id(), NonZeroU8::MIN);
        horn.components.insert(goat::INSTRUMENT, instrument);
        let mut between = |min: f32, max: f32| min + random.next_float() * (max - min);
        let velocity = Vec3::new(
            f64::from(between(-HORN_SPREAD, HORN_SPREAD)),
            f64::from(between(HORN_LIFT_MIN, HORN_LIFT_MAX)),
            f64::from(between(-HORN_SPREAD, HORN_SPREAD)),
        );
        self.store_mob_random(id, random);
        self.spawn_item_entity(position, velocity, HORN_PICKUP_DELAY, horn);
        self.entity_sound(id, HORN_BREAK_SOUND, 1.0, 1.0);
    }
}

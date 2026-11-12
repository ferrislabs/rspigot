use std::num::NonZeroU8;

use rand::Rng;
use rspigot_ai::{CrossbowAction, Hand as AiHand};
use rspigot_domain::crossbow;
use rspigot_domain::entity::arrow::{self, ArrowState, Pickup};
use rspigot_domain::entity::living::slot_index;
use rspigot_domain::entity::species::{piglin, pillager};
use rspigot_domain::entity::{Entity, EntityState};
use rspigot_domain::item::ItemStack;
use rspigot_domain::random::RandomSource;
use rspigot_domain::{EntityId, Vec3};
use rspigot_entities::{EntityKind as Kind, SoundSource};
use rspigot_items::{EquipmentSlot, ItemKind};

use crate::entities::wire_slot;
use crate::events::{ClientEvent, Hand};
use crate::plugins::steps::{Draw, Shooter};
use crate::ports::ClientLink;
use crate::projectiles::EYE_OFFSET;
use crate::server::{Audience, Server};

const INTANGIBLE_PROJECTILE: &str = "minecraft:intangible_projectile";
const UNBREAKING: &str = "minecraft:unbreaking";
const INFINITY: &str = "minecraft:infinity";

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ChargingSoundFlags {
    pub start: bool,
    pub mid: bool,
}

pub(crate) const fn hand_of(slot: EquipmentSlot) -> Hand {
    match slot {
        EquipmentSlot::Offhand => Hand::Off,
        _ => Hand::Main,
    }
}

const fn slot_of(hand: AiHand) -> EquipmentSlot {
    match hand {
        AiHand::Main => EquipmentSlot::Mainhand,
        AiHand::Off => EquipmentSlot::Offhand,
    }
}

fn is_crossbow(stack: &ItemStack) -> bool {
    stack.item == ItemKind::Crossbow.id()
}

fn survival_odds(level: i32) -> f32 {
    level as f32 / (level + 1) as f32
}

impl<L: ClientLink> Server<L> {
    pub(crate) fn crossbow_action(&mut self, id: EntityId, action: CrossbowAction) {
        match action {
            CrossbowAction::SetCharging(charging) => {
                if let Some(entity) = self.entities.store.get_mut(id) {
                    if entity.base.kind == Kind::Piglin {
                        piglin::set_charging_crossbow(&mut entity.base, charging);
                    } else {
                        pillager::set_charging_crossbow(&mut entity.base, charging);
                    }
                }
            }
            CrossbowAction::ClearCharged => self.crossbow_clear_charged(id),
            CrossbowAction::Release { hand, ticks } => {
                self.crossbow_use_step(id, slot_of(hand), ticks);
            }
        }
    }

    pub(crate) fn crossbow_use_tick(&mut self, id: EntityId) {
        let using = self
            .entities
            .store
            .get(id)
            .and_then(|entity| entity.living())
            .and_then(|living| living.using_item);
        if let Some((slot, ticks)) = using {
            self.crossbow_use_step(id, slot, ticks);
        }
    }

    pub(crate) fn crossbow_view_charge(&self, entity: &Entity) -> Option<i32> {
        let living = entity.living()?;
        let (slot, _) = living.using_item?;
        living
            .equipment(slot)
            .filter(|stack| is_crossbow(stack))
            .map(crossbow::charge_ticks)
    }

    fn play_charging_sound(
        &mut self,
        position: Vec3,
        sound: &'static str,
        source: SoundSource,
        volume: f32,
        pitch: f32,
    ) {
        self.play_sound(None, position, sound, source, volume, pitch);
    }

    fn crossbow_use_step(&mut self, id: EntityId, slot: EquipmentSlot, ticks: i32) {
        let Some(entity) = self.entities.store.get(id) else {
            return;
        };
        let Some(weapon) = entity
            .living()
            .and_then(|living| living.equipment(slot))
            .filter(|stack| is_crossbow(stack))
            .cloned()
        else {
            return;
        };
        let position = entity.base.position;
        let source = entity
            .base
            .kind
            .sounds()
            .map_or(SoundSource::Neutral, |sounds| sounds.source);
        let progress = ticks as f32 / crossbow::charge_ticks(&weapon) as f32;
        let sounds = crossbow::charging_sounds(&weapon);
        let flags = &mut self.entities.mobs.charging_sounds;
        let mut start = false;
        let mut mid = false;
        if progress < crossbow::START_SOUND_PROGRESS {
            flags.start = false;
            flags.mid = false;
        } else if !flags.start {
            flags.start = true;
            start = true;
        }
        if progress >= crossbow::MID_SOUND_PROGRESS && !flags.mid {
            flags.mid = true;
            mid = true;
        }
        if start {
            self.play_charging_sound(
                position,
                sounds.start,
                SoundSource::Players,
                crossbow::CHARGING_SOUND_VOLUME,
                1.0,
            );
        }
        if mid {
            self.play_charging_sound(
                position,
                sounds.mid,
                SoundSource::Players,
                crossbow::CHARGING_SOUND_VOLUME,
                1.0,
            );
        }
        if progress >= 1.0 && !crossbow::is_charged(&weapon) && self.crossbow_try_load(id, slot) {
            let pitch =
                crossbow::charging_sound_pitch(&mut crate::mobs::ServerRandom(&mut self.rng));
            self.play_charging_sound(position, sounds.end, source, 1.0, pitch);
        }
    }

    fn held_projectile_slot(&self, id: EntityId) -> Option<EquipmentSlot> {
        let living = self.entities.store.get(id)?.living()?;
        [EquipmentSlot::Offhand, EquipmentSlot::Mainhand]
            .into_iter()
            .find(|slot| living.equipment(*slot).is_some_and(crossbow::is_arrow))
    }

    pub(crate) fn set_equipment_stack(
        &mut self,
        id: EntityId,
        slot: EquipmentSlot,
        stack: Option<ItemStack>,
    ) {
        let Some(living) = self
            .entities
            .store
            .get_mut(id)
            .and_then(|entity| entity.living_mut())
        else {
            return;
        };
        living.equipment[slot_index(slot)] = stack.clone();
        self.push(
            Audience::Trackers(id),
            ClientEvent::EntityEquipment {
                entity: id,
                slots: vec![(wire_slot(slot), stack)],
            },
        );
    }

    fn crossbow_try_load(&mut self, id: EntityId, slot: EquipmentSlot) -> bool {
        let Some(mut weapon) = self
            .entities
            .store
            .get(id)
            .and_then(|entity| entity.living())
            .and_then(|living| living.equipment(slot))
            .filter(|stack| is_crossbow(stack))
            .cloned()
        else {
            return false;
        };
        let held_slot = self.held_projectile_slot(id);
        let held = held_slot.and_then(|slot| {
            self.entities
                .store
                .get(id)?
                .living()?
                .equipment(slot)
                .cloned()
        });
        let ammo = held.clone().unwrap_or_else(arrow::default_item);
        let infinite = crossbow::level_of(&weapon, INFINITY) > 0;
        let mut left = ammo.count.get();
        let mut loaded: Vec<ItemStack> = Vec::new();
        for index in 0..crossbow::projectile_count(&weapon) {
            if index > 0 || (infinite && ammo.item == ItemKind::Arrow.id()) {
                loaded.push(intangible(&ammo));
            } else {
                left -= 1;
                loaded.push(ItemStack {
                    count: NonZeroU8::MIN,
                    ..ammo.clone()
                });
            }
        }
        if loaded.is_empty() {
            return false;
        }
        if let (Some(held_slot), Some(held)) = (held_slot, held)
            && left != held.count.get()
        {
            let remaining = NonZeroU8::new(left).map(|count| ItemStack { count, ..held });
            self.set_equipment_stack(id, held_slot, remaining);
        }
        crossbow::set_charged(&mut weapon, &loaded);
        self.set_equipment_stack(id, slot, Some(weapon));
        true
    }

    fn crossbow_clear_charged(&mut self, id: EntityId) {
        let Some(living) = self
            .entities
            .store
            .get(id)
            .and_then(|entity| entity.living())
        else {
            return;
        };
        let held = [EquipmentSlot::Mainhand, EquipmentSlot::Offhand]
            .into_iter()
            .find_map(|slot| {
                living
                    .equipment(slot)
                    .filter(|stack| is_crossbow(stack))
                    .map(|stack| (slot, stack.clone()))
            });
        if let Some((slot, mut weapon)) = held {
            crossbow::set_charged(&mut weapon, &[]);
            self.set_equipment_stack(id, slot, Some(weapon));
        }
    }

    pub(crate) fn crossbow_attack(&mut self, id: EntityId, target: EntityId) {
        let Some(living) = self
            .entities
            .store
            .get(id)
            .and_then(|entity| entity.living())
        else {
            return;
        };
        let slot = if living
            .equipment(EquipmentSlot::Mainhand)
            .is_some_and(is_crossbow)
        {
            EquipmentSlot::Mainhand
        } else {
            EquipmentSlot::Offhand
        };
        if let Some(mut weapon) = living.equipment(slot).filter(|s| is_crossbow(s)).cloned() {
            let projectiles = crossbow::charged_projectiles(&weapon);
            crossbow::set_charged(&mut weapon, &[]);
            self.set_equipment_stack(id, slot, Some(weapon.clone()));
            if !projectiles.is_empty() {
                self.crossbow_shoot(id, slot, &weapon, projectiles, target);
            }
        }
        if let Some(living) = self
            .entities
            .store
            .get_mut(id)
            .and_then(|entity| entity.living_mut())
        {
            living.no_action_time = 0;
        }
    }

    fn crossbow_shoot(
        &mut self,
        id: EntityId,
        slot: EquipmentSlot,
        weapon: &ItemStack,
        projectiles: Vec<ItemStack>,
        target: EntityId,
    ) {
        let Some(shooter) = self.entities.store.get(id) else {
            return;
        };
        let Some((target_position, target_height, _)) = self.entity_box(target) else {
            return;
        };
        let origin = shooter.base.position;
        let eye = shooter.base.eye_y();
        let owner = shooter.base.uuid;
        let up = crossbow::up_vector(shooter.base.pitch, shooter.base.yaw);
        let difficulty = self.world.difficulty.id();
        let uncertainty = crossbow::ranged_uncertainty(difficulty);
        let angles = crossbow::shot_angles(projectiles.len(), crossbow::projectile_spread(weapon));
        let pierce = crossbow::piercing_level(weapon);
        let power = crossbow::MOB_POWER;
        for (index, (stack, angle)) in projectiles.into_iter().zip(angles).enumerate() {
            let mut base = self.new_entity_base(Kind::Arrow);
            base.position = Vec3::new(origin.x, eye - EYE_OFFSET, origin.z);
            base.old_position = base.position;
            let seed = self.rng.random::<i64>();
            let mut state = ArrowState::new(&mut base, seed);
            state.set_owner(Some(owner), false);
            state.pickup = if stack.components.contains_key(INTANGIBLE_PROJECTILE) {
                Pickup::CreativeOnly
            } else {
                Pickup::Disallowed
            };
            state.item = ItemStack {
                count: NonZeroU8::MIN,
                ..stack.clone()
            };
            state.weapon = Some(weapon.clone());
            state.sound = crossbow::HIT_SOUND.to_owned();
            if pierce > 0 {
                arrow::set_pierce_level(&mut base, pierce.min(i32::from(i8::MAX)) as i8);
            }
            let aim = crossbow::aim_vector(origin, target_position, target_height, base.position.y);
            let direction = crossbow::shot_vector(aim, angle, up);
            arrow::shoot(&mut base, &mut state, direction, power, uncertainty);
            let mut random = self.mob_random(id);
            let pitch = crossbow::shot_pitch(&mut random, index);
            self.store_mob_random(id, random);
            self.crossbow_shot_sound(id, pitch);
            let arrow = Entity::new(base, EntityState::Arrow(Box::new(state)));
            let hand = hand_of(slot);
            let draw = Draw {
                weapon: Some(weapon.clone()),
                ammo: Some(stack.clone()),
                hand,
                force: power,
            };
            let shooter = Shooter::Crossbow {
                mob: id,
                hand,
                wear: crossbow::durability_use(&stack),
            };
            self.shoot_bow(shooter, draw, arrow);
        }
    }

    fn crossbow_shot_sound(&mut self, id: EntityId, pitch: f32) {
        let Some(entity) = self.entities.store.get(id) else {
            return;
        };
        let position = entity.base.position;
        let source = entity
            .base
            .kind
            .sounds()
            .map_or(SoundSource::Neutral, |sounds| sounds.source);
        self.play_sound(None, position, crossbow::SHOOT_SOUND, source, 1.0, pitch);
    }

    pub(crate) fn crossbow_wear(&mut self, id: EntityId, hand: Hand, amount: i32) {
        let slot = match hand {
            Hand::Main => EquipmentSlot::Mainhand,
            Hand::Off => EquipmentSlot::Offhand,
        };
        let level = self
            .entities
            .store
            .get(id)
            .and_then(|entity| entity.living())
            .and_then(|living| living.equipment(slot))
            .map_or(0, |stack| crossbow::level_of(stack, UNBREAKING));
        let mut remaining = amount;
        if level > 0 && amount > 0 {
            let odds = survival_odds(level);
            let mut random = crate::mobs::ServerRandom(&mut self.rng);
            for _ in 0..amount {
                if random.next_float() < odds {
                    remaining -= 1;
                }
            }
        }
        self.damage_mob_equipment(id, slot, remaining);
    }
}

fn intangible(stack: &ItemStack) -> ItemStack {
    let mut copy = ItemStack {
        count: NonZeroU8::MIN,
        ..stack.clone()
    };
    copy.components
        .insert(INTANGIBLE_PROJECTILE, rspigot_nbt::Compound::new());
    copy
}

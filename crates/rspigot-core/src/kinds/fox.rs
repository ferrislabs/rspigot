use std::num::NonZeroU8;

use rand::Rng;
use rspigot_ai::{FoxAction, FoxFacts, MobView, TrustedRef};
use rspigot_domain::entity::item;
use rspigot_domain::entity::living;
use rspigot_domain::entity::species::Species;
use rspigot_domain::entity::species::fox;
use rspigot_domain::entity::{Aabb, Entity, EntityState};
use rspigot_domain::item::ItemStack;
use rspigot_domain::mth;
use rspigot_domain::player::PlayerId;
use rspigot_domain::random::RandomSource;
use rspigot_domain::{BlockPos, EntityId, Vec3};
use rspigot_entities::EntityKind as Kind;
use rspigot_items::{EquipmentSlot, ItemKind};

use crate::events::{ClientEvent, EntityStatus, Hand};
use crate::health::DamageSource;
use crate::interact::Interaction;
use crate::plugins::events::EntityRemoveCause;
use crate::plugins::steps::{GatherStep, Grief};
use crate::plugins::{Continuation, EventPayload};
use crate::ports::ClientLink;
use crate::server::{Audience, Server};

const EAT_SOUND: &str = "minecraft:entity.fox.eat";
const SPIT_SOUND: &str = "minecraft:entity.fox.spit";
const AGGRO_SOUND: &str = "minecraft:entity.fox.aggro";
const AMBIENT_SOUND: &str = "minecraft:entity.fox.ambient";
const SLEEP_SOUND: &str = "minecraft:entity.fox.sleep";
const SCREECH_SOUND: &str = "minecraft:entity.fox.screech";
const GENERIC_EAT: &str = "minecraft:entity.generic.eat";
const BERRY_PICK: &str = "minecraft:block.sweet_berry_bush.pick_berries";
const GLOW_BERRY_PICK: &str = "minecraft:block.cave_vines.pick_berries";
const HARVEST_CAVE_VINE: &str = "minecraft:harvest/cave_vine";
const EAT_EVENT: i8 = 45;
const BLOCK_BREAK_EVENT: i32 = 2001;
const EAT_PARTICLE_AFTER: i32 = 560;
const SPIT_PICKUP_DELAY: u16 = 40;
const SCREECH_VOLUME: f32 = 2.0;
const SCREECH_CHANCE: f32 = 0.1;
const SCREECH_RADIUS: f64 = 16.0;
const AGGRO_CHANCE: f32 = 0.05;
const FAIL_FACEPLANT_EVENT_CHANCE: f32 = 0.2;
const GUARANTEED_DROP: f32 = 2.0;
const LOOT_REACH: f64 = 1.0;

fn look_angle(yaw: f32, pitch: f32) -> Vec3 {
    let f = pitch * mth::DEG_TO_RAD;
    let g = -yaw * mth::DEG_TO_RAD;
    let pi = std::f32::consts::PI;
    let h = mth::cos(f64::from(g - pi));
    let i = mth::sin(f64::from(g - pi));
    let j = -mth::cos(f64::from(-f));
    let k = mth::sin(f64::from(-f));
    Vec3::new(f64::from(i * j), f64::from(k), f64::from(h * j))
}

fn ambient_voice(
    first: Option<&'static str>,
    second: Option<&'static str>,
) -> Option<(&'static str, f32)> {
    if first == Some(SCREECH_SOUND) {
        first.map(|sound| (sound, SCREECH_VOLUME))
    } else {
        second.map(|sound| (sound, 1.0))
    }
}

impl<L: ClientLink> Server<L> {
    fn fox_state_mut(&mut self, id: EntityId) -> Option<&mut fox::FoxState> {
        match self
            .entities
            .store
            .get_mut(id)?
            .living_mut()?
            .mob
            .as_mut()
            .map(|mob| &mut mob.species)
        {
            Some(Species::Fox(state)) => Some(state),
            _ => None,
        }
    }

    pub(crate) fn fox_view(&self, entity: &Entity, view: &mut MobView) {
        let Some(living) = entity.living() else {
            return;
        };
        let Some(Species::Fox(state)) = living.mob.as_ref().map(|mob| mob.species) else {
            return;
        };
        let base = &entity.base;
        let world = self.mob_world(crate::mobs::sky_darken(&self.world.level));
        let trusted = fox::trusted(base).map(|uuid| {
            uuid.map(|uuid| TrustedRef {
                uuid,
                id: world.resolve(uuid),
            })
        });
        view.species.fox = FoxFacts {
            flags: fox::flags(base),
            fully_crouched: state.is_fully_crouched(),
            jumping: living.jumping,
            moving: living.xxa != 0.0 || living.yya != 0.0 || living.zza != 0.0,
            trusted,
        };
    }

    pub(crate) fn fox_pre_tick(&mut self, _id: EntityId) -> bool {
        true
    }

    pub(crate) fn fox_ai_step(&mut self, id: EntityId) {
        let target_present = self.mob_target(id).is_some();
        let target_alive = self
            .mob_target(id)
            .is_some_and(|target| self.living_entity_alive(target));
        let Some(Entity {
            base,
            state: EntityState::Living(living),
            ..
        }) = self.entities.store.get_mut(id)
        else {
            return;
        };
        let alive = !base.is_removed() && living::health(base) > 0.0;
        if alive && living::effective_ai(base, living) {
            let mut eat = false;
            let mut particles = false;
            let held = living.equipment(EquipmentSlot::Mainhand).cloned();
            let on_ground = base.on_ground;
            let sleeping = fox::is_sleeping(base);
            if let Some(Species::Fox(state)) = living.mob.as_mut().map(|mob| &mut mob.species) {
                state.ticks_since_eaten += 1;
                let can_eat = held.as_ref().is_some_and(fox::is_consumable_food)
                    && !target_present
                    && on_ground
                    && !sleeping;
                if can_eat {
                    if state.ticks_since_eaten > fox::TICKS_BEFORE_EAT {
                        eat = true;
                        state.ticks_since_eaten = 0;
                    } else if state.ticks_since_eaten > EAT_PARTICLE_AFTER {
                        particles = true;
                    }
                }
            }
            if eat && let Some(held) = held {
                self.fox_finish_eating(id, held);
            } else if particles {
                let mut random = self.mob_random(id);
                let chance = random.next_float() < 0.1;
                self.store_mob_random(id, random);
                if chance {
                    self.entity_sound(id, EAT_SOUND, 1.0, 1.0);
                    self.push(
                        Audience::Trackers(id),
                        ClientEvent::EntityStatus {
                            entity: id,
                            status: EntityStatus::Event(EAT_EVENT),
                        },
                    );
                }
            }
            if !(target_present && target_alive)
                && let Some(Entity { base, .. }) = self.entities.store.get_mut(id)
            {
                fox::set_crouching(base, false);
                fox::set_interested(base, false);
            }
        }
        if let Some(Entity {
            base,
            state: EntityState::Living(living),
            ..
        }) = self.entities.store.get_mut(id)
            && (fox::is_sleeping(base) || living::is_dead_or_dying(base, living))
        {
            living.jumping = false;
            living.xxa = 0.0;
            living.zza = 0.0;
        }
    }

    fn fox_finish_eating(&mut self, id: EntityId, held: ItemStack) {
        let mut random = self.mob_random(id);
        let loud = random.next_boolean();
        let triangle = 1.0 + 0.2 * (random.next_float() - random.next_float());
        let _ = random.next_float();
        self.store_mob_random(id, random);
        self.entity_sound(id, GENERIC_EAT, if loud { 0.5 } else { 1.0 }, triangle);
        let remaining = NonZeroU8::new(held.count.get() - 1).map(|count| ItemStack {
            count,
            ..held.clone()
        });
        let replacement = held
            .item
            .kind()
            .and_then(ItemKind::use_remainder)
            .map(|item| ItemStack::new(item, NonZeroU8::MIN));
        self.fox_set_mouth(id, remaining.or(replacement));
    }

    fn fox_set_mouth(&mut self, id: EntityId, stack: Option<ItemStack>) {
        let Some(living) = self
            .entities
            .store
            .get_mut(id)
            .and_then(|entity| entity.living_mut())
        else {
            return;
        };
        living.equipment[living::slot_index(EquipmentSlot::Mainhand)] = stack.clone();
        self.push(
            Audience::Trackers(id),
            ClientEvent::EntityEquipment {
                entity: id,
                slots: vec![(crate::entities::wire_slot(EquipmentSlot::Mainhand), stack)],
            },
        );
    }

    pub(crate) fn fox_post_tick(&mut self, id: EntityId) {
        self.fox_gather_loot(id);
        let mut random = self.mob_random(id);
        let aggro = self
            .entities
            .store
            .get(id)
            .is_some_and(|entity| fox::is_defending(&entity.base))
            && random.next_float() < AGGRO_CHANCE;
        self.store_mob_random(id, random);
        if aggro {
            self.entity_sound(id, AGGRO_SOUND, 1.0, 1.0);
        }
        let thundering = self.world.level.raining && self.world.level.thundering;
        let target = self.mob_target(id).is_some();
        let roll = self.rng.random::<f32>();
        let mut faceplant_event = None;
        let Some(Entity {
            base,
            state: EntityState::Living(living),
            ..
        }) = self.entities.store.get_mut(id)
        else {
            return;
        };
        if living::effective_ai(base, living) {
            let in_water = base.fluid.in_water;
            if in_water || target || thundering {
                fox::set_sleeping(base, false);
            }
            if in_water || fox::is_sleeping(base) {
                fox::set_sitting(base, false);
            }
            if fox::is_faceplanted(base) && roll < FAIL_FACEPLANT_EVENT_CHANCE {
                faceplant_event = Some(base.position.block());
            }
        }
        let crouching = fox::is_crouching(base);
        if let Some(Species::Fox(state)) = living.mob.as_mut().map(|mob| &mut mob.species) {
            state.tick_crouch(crouching);
        }
        if let Some(pos) = faceplant_event {
            let block = self
                .world
                .chunks
                .block(pos)
                .map_or(0, |state| i32::from(state.0));
            self.level_event(BLOCK_BREAK_EVENT, pos, block);
        }
    }

    fn fox_gather_loot(&mut self, id: EntityId) {
        let Some(entity) = self.entities.store.get(id) else {
            return;
        };
        let Some(living) = entity.living() else {
            return;
        };
        let can_pick = living.mob.as_ref().is_some_and(|mob| mob.can_pick_up_loot)
            && !entity.base.is_removed()
            && living::health(&entity.base) > 0.0
            && !living.dead
            && self.config.rules.mob_griefing;
        if !can_pick {
            return;
        }
        let Some(Species::Fox(state)) = living.mob.as_ref().map(|mob| mob.species) else {
            return;
        };
        let held = living.equipment(EquipmentSlot::Mainhand).cloned();
        let area = entity.base.bounding_box();
        let area = Aabb::new(
            area.min_x - LOOT_REACH,
            area.min_y,
            area.min_z - LOOT_REACH,
            area.max_x + LOOT_REACH,
            area.max_y,
            area.max_z + LOOT_REACH,
        );
        let found = self.entities.store.iter().find_map(|other| {
            let EntityState::Item(item_state) = &other.state else {
                return None;
            };
            let stack = item::stack(&other.base)?;
            (!other.base.is_removed()
                && item_state.pickup_delay <= 0
                && other.base.bounding_box().intersects(&area)
                && state.can_hold(held.as_ref(), stack))
            .then(|| (other.base.id, stack.clone()))
        });
        let Some((item_id, stack)) = found else {
            return;
        };
        let step = GatherStep {
            fox: id,
            item: item_id,
            stack: stack.clone(),
        };
        let describe = move |server: &Self| {
            Some(EventPayload::EntityPickupItem(
                crate::plugins::events::EntityPickupItem {
                    entity: server.subject_view(id)?,
                    entity_type: Kind::Fox.name().to_owned(),
                    item: server.entity_view(item_id)?,
                    remaining: i32::from(stack.count.get()) - 1,
                },
            ))
        };
        let lane = self.lane_for(None, id);
        self.fire(lane, Continuation::Gather(Box::new(step)), describe);
    }

    pub(crate) fn finish_gather(&mut self, step: GatherStep, cancelled: bool) {
        let GatherStep {
            fox: id,
            item,
            stack,
        } = step;
        if cancelled || self.item_stack_of(item).as_ref() != Some(&stack) {
            return;
        }
        let Some(fox_entity) = self.entities.store.get(id) else {
            return;
        };
        let position = fox_entity.base.position;
        let look = look_angle(fox_entity.base.yaw, fox_entity.base.pitch);
        let uuid = fox_entity.base.uuid;
        let mouth = fox_entity
            .living()
            .and_then(|living| living.equipment(EquipmentSlot::Mainhand).cloned());
        let count = stack.count.get();
        if count > 1 {
            let extra = ItemStack {
                count: NonZeroU8::new(count - 1).unwrap_or(NonZeroU8::MIN),
                ..stack.clone()
            };
            let velocity = self.item_velocity();
            let dropped = self.item_entity(position, velocity, 0, extra);
            self.add_entity(
                dropped,
                crate::plugins::events::CreatureSpawnReason::Default,
            );
        }
        if let Some(spat) = mouth {
            let at = Vec3::new(position.x + look.x, position.y + 1.0, position.z + look.z);
            let velocity = self.item_velocity();
            let mut dropped = self.item_entity(at, velocity, SPIT_PICKUP_DELAY, spat);
            if let EntityState::Item(state) = &mut dropped.state {
                state.thrower = Some(uuid);
            }
            self.entity_sound(id, SPIT_SOUND, 1.0, 1.0);
            self.add_entity(
                dropped,
                crate::plugins::events::CreatureSpawnReason::Default,
            );
        }
        let single = ItemStack {
            count: NonZeroU8::MIN,
            ..stack
        };
        if let Some(mob) = self
            .entities
            .store
            .get_mut(id)
            .and_then(|entity| entity.living_mut())
            .and_then(|living| living.mob.as_mut())
        {
            mob.drop_chances[living::slot_index(EquipmentSlot::Mainhand)] = GUARANTEED_DROP;
            if let Species::Fox(state) = &mut mob.species {
                state.ticks_since_eaten = 0;
            }
        }
        self.fox_set_mouth(id, Some(single));
        self.push(
            Audience::Trackers(id),
            ClientEvent::ItemPickup {
                collected: item,
                collector: id,
                amount: 0,
            },
        );
        self.remove_entity(
            item,
            rspigot_domain::entity::RemovalReason::Discarded,
            Some(EntityRemoveCause::Pickup),
        );
    }

    fn item_velocity(&mut self) -> Vec3 {
        Vec3::new(
            self.rng.random::<f64>() * 0.2 - 0.1,
            0.2,
            self.rng.random::<f64>() * 0.2 - 0.1,
        )
    }

    pub(crate) fn fox_breeding_trust(
        &self,
        parent: EntityId,
        partner: EntityId,
        child: &mut Entity,
    ) {
        if child.base.kind != Kind::Fox {
            return;
        }
        let cause = |id: EntityId| {
            let uuid = self
                .entities
                .store
                .get(id)?
                .living()?
                .mob
                .as_ref()?
                .ageable
                .as_ref()?
                .love
                .as_ref()?
                .cause?;
            self.players
                .values()
                .any(|player| player.id.0 == uuid)
                .then_some(uuid)
        };
        let first = cause(parent);
        let second = cause(partner);
        if let Some(uuid) = first {
            fox::add_trusted(&mut child.base, uuid);
        }
        if let Some(uuid) = second
            && Some(uuid) != first
        {
            fox::add_trusted(&mut child.base, uuid);
        }
    }

    pub(crate) fn fox_custom_ai_step(&mut self, _id: EntityId) {}

    pub(crate) fn fox_interact(
        &mut self,
        _player: PlayerId,
        _target: EntityId,
        _hand: Hand,
        _held: Option<&ItemStack>,
    ) -> Option<Interaction> {
        None
    }

    pub(crate) fn fox_hurt_amount(
        &mut self,
        _id: EntityId,
        _source: &DamageSource,
        amount: f32,
    ) -> Option<f32> {
        Some(amount)
    }

    pub(crate) fn fox_after_hurt(&mut self, _id: EntityId, _source: &DamageSource, _dead: bool) {}

    pub(crate) fn fox_death_item(&mut self, id: EntityId) -> Option<ItemStack> {
        let living = self.entities.store.get_mut(id)?.living_mut()?;
        let index = living::slot_index(EquipmentSlot::Mainhand);
        living.equipment[index].take()
    }

    pub(crate) fn fox_target_cleared(&mut self, id: EntityId) {
        if let Some(Entity { base, .. }) = self.entities.store.get_mut(id) {
            fox::set_defending(base, false);
        }
    }

    pub(crate) fn fox_ambient(&mut self, id: EntityId) {
        let first = self.fox_ambient_sound(id);
        let second = if first == Some(SCREECH_SOUND) {
            None
        } else {
            self.fox_ambient_sound(id)
        };
        let Some((sound, volume)) = ambient_voice(first, second) else {
            return;
        };
        let baby = self
            .entities
            .store
            .get(id)
            .is_some_and(rspigot_domain::entity::Entity::is_baby);
        let mut random = self.mob_random(id);
        let spread = (random.next_float() - random.next_float()) * 0.2;
        self.store_mob_random(id, random);
        let pitch = spread + if baby { 1.5 } else { 1.0 };
        self.entity_sound(id, sound, volume, pitch);
    }

    fn fox_ambient_sound(&mut self, id: EntityId) -> Option<&'static str> {
        let entity = self.entities.store.get(id)?;
        if fox::is_sleeping(&entity.base) {
            return Some(SLEEP_SOUND);
        }
        let position = entity.base.position;
        let bright =
            self.world.dimension.has_skylight && crate::mobs::sky_darken(&self.world.level) < 4;
        let mut random = self.mob_random(id);
        let screech = !bright && random.next_float() < SCREECH_CHANCE;
        self.store_mob_random(id, random);
        if screech {
            let near = self.players.values().any(|player| {
                player.game_mode != rspigot_domain::player::GameMode::Spectator
                    && (player.position.x - position.x).abs() <= SCREECH_RADIUS + 0.3
                    && (player.position.y - position.y).abs() <= SCREECH_RADIUS + 1.0
                    && (player.position.z - position.z).abs() <= SCREECH_RADIUS + 0.3
            });
            if !near {
                return Some(SCREECH_SOUND);
            }
        }
        Some(AMBIENT_SOUND)
    }

    pub(crate) fn fox_action(&mut self, id: EntityId, action: FoxAction) {
        match action {
            FoxAction::SetFlags(flags) => {
                if let Some(Entity { base, .. }) = self.entities.store.get_mut(id) {
                    fox::set_flags(base, flags);
                }
            }
            FoxAction::ClearStates(partner) => {
                if let Some(Entity { base, .. }) = self.entities.store.get_mut(partner) {
                    fox::clear_states(base);
                }
            }
            FoxAction::Sound(sound) => self.entity_sound(id, sound, 1.0, 1.0),
            FoxAction::ResetCrouch => {
                if let Some(state) = self.fox_state_mut(id) {
                    state.reset_crouch();
                }
            }
            FoxAction::PickBerries(pos) => self.fox_pick_berries(id, pos),
        }
    }

    fn fox_pick_berries(&mut self, id: EntityId, pos: BlockPos) {
        if !self.config.rules.mob_griefing {
            return;
        }
        let Some(state) = self.world.chunks.block(pos) else {
            return;
        };
        let Some(view) = self.entity_view(id) else {
            return;
        };
        let (to, glow) = if state.kind() == Some(rspigot_blocks::BlockKind::SweetBerryBush) {
            (state.with_property("age", "1"), false)
        } else if state.property("berries") == Some("true") {
            (state.with_property("berries", "false"), true)
        } else {
            return;
        };
        let Some(to) = to else {
            return;
        };
        self.change_block(view, pos, to, Grief::Berries { fox: id, pos, glow }, false);
    }

    pub(crate) fn finish_berries(&mut self, id: EntityId, pos: BlockPos, glow: bool) {
        let Some(state) = self.world.chunks.block(pos) else {
            return;
        };
        if glow {
            if state.property("berries") != Some("true") {
                return;
            }
            let Some(next) = state.with_property("berries", "false") else {
                return;
            };
            let drops = rspigot_data::loot::table(HARVEST_CAVE_VINE)
                .map(|table| {
                    crate::loot::table_drops(
                        table,
                        &crate::loot::LootContext::default(),
                        &mut self.rng,
                    )
                })
                .unwrap_or_default();
            for stack in drops {
                self.spawn_item(pos, stack);
            }
            let pitch = 0.8 + self.rng.random::<f32>() * 0.4;
            let at = Vec3::new(
                f64::from(pos.x) + 0.5,
                f64::from(pos.y) + 0.5,
                f64::from(pos.z) + 0.5,
            );
            self.play_sound(
                None,
                at,
                GLOW_BERRY_PICK,
                rspigot_entities::SoundSource::Blocks,
                1.0,
                pitch,
            );
            self.set_block(pos, next);
            return;
        }
        if state.kind() != Some(rspigot_blocks::BlockKind::SweetBerryBush) {
            return;
        }
        let age: u8 = state
            .property("age")
            .and_then(|age| age.parse().ok())
            .unwrap_or(0);
        let Some(next) = state.with_property("age", "1") else {
            return;
        };
        let mut count = 1 + self.rng.random_range(0..2) + i32::from(age == 3);
        let empty = self
            .entities
            .store
            .get(id)
            .and_then(|entity| entity.living())
            .is_some_and(|living| living.equipment(EquipmentSlot::Mainhand).is_none());
        if empty {
            let berry = ItemStack::new(ItemKind::SweetBerries.id(), NonZeroU8::MIN);
            self.fox_set_mouth(id, Some(berry));
            count -= 1;
        }
        if let Some(amount) = NonZeroU8::new(count.max(0) as u8) {
            self.spawn_item(pos, ItemStack::new(ItemKind::SweetBerries.id(), amount));
        }
        self.entity_sound(id, BERRY_PICK, 1.0, 1.0);
        self.set_block(pos, next);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_first_screech_is_loud() {
        assert_eq!(
            ambient_voice(Some(SCREECH_SOUND), None),
            Some((SCREECH_SOUND, SCREECH_VOLUME))
        );
        assert_eq!(
            ambient_voice(Some(AMBIENT_SOUND), Some(SCREECH_SOUND)),
            Some((SCREECH_SOUND, 1.0))
        );
        assert_eq!(
            ambient_voice(Some(AMBIENT_SOUND), Some(AMBIENT_SOUND)),
            Some((AMBIENT_SOUND, 1.0))
        );
        assert_eq!(ambient_voice(None, None), None);
    }
}

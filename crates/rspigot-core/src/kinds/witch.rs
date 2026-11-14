use std::borrow::Cow;

use rspigot_ai::{MobView, RaiderAction, TargetReason};
use rspigot_domain::effect::{AttributeModifier, AttributeOperation};
use rspigot_domain::entity::living::{self, MOVEMENT_SPEED};
use rspigot_domain::entity::species::{Species, raider, witch};
use rspigot_domain::entity::{Entity, fire};
use rspigot_domain::item::ItemStack;
use rspigot_domain::potion::{self, PotionContents};
use rspigot_domain::random::RandomSource;
use rspigot_domain::{EntityId, Vec3};
use rspigot_entities::EntityKind as Kind;
use rspigot_items::{EquipmentSlot, ItemKind};

use crate::entities::wire_slot;
use crate::events::{ClientEvent, EntityStatus};
use crate::plugins::events::EntityPotionEffectCause;
use crate::ports::ClientLink;
use crate::server::{Audience, Server};

#[derive(Debug, Clone, Copy)]
struct ThrowTarget {
    position: Vec3,
    eye_y: f64,
    motion: Vec3,
    health: f32,
    raider: bool,
    slowness: bool,
    poison: bool,
    weakness: bool,
}

impl<L: ClientLink> Server<L> {
    pub(crate) fn raider_action(&mut self, id: EntityId, action: RaiderAction) {
        match action {
            RaiderAction::StopPatrolling => {
                if let Some(raider) = self.raider_state_mut(id) {
                    raider.patrolling = false;
                }
            }
            RaiderAction::SetPatrolTarget { mob, target } => {
                if let Some(raider) = self.raider_state_mut(mob) {
                    raider.set_patrol_target(target);
                }
            }
            RaiderAction::ResetNoActionTime => {
                if let Some(living) = self.entities.store.get_mut(id).and_then(Entity::living_mut) {
                    living.no_action_time = 0;
                }
            }
            RaiderAction::SetCelebrating(celebrating) => {
                if let Some(entity) = self.entities.store.get_mut(id) {
                    raider::set_celebrating(&mut entity.base, celebrating);
                }
            }
            RaiderAction::CelebrateSound => self.raider_celebrate_sound(id),
            RaiderAction::Recruit => self.raider_recruit(id),
            RaiderAction::PickUpBanner(item) => self.raider_pick_up_banner(id, item),
            RaiderAction::ShoutTarget {
                mob,
                target,
                aggressive,
            } => {
                if let Some(view) = self.entity_view(mob) {
                    self.request_target(view, target, TargetReason::FollowLeader);
                }
                if aggressive && let Some(entity) = self.entities.store.get_mut(mob) {
                    living::set_aggressive(&mut entity.base, true);
                }
            }
        }
    }

    fn raider_state_mut(
        &mut self,
        id: EntityId,
    ) -> Option<&mut rspigot_domain::entity::species::raider::RaiderState> {
        self.entities
            .store
            .get_mut(id)?
            .living_mut()?
            .mob
            .as_mut()?
            .species
            .raider_mut()
    }

    pub(crate) fn witch_view(&self, entity: &Entity, view: &mut MobView) {
        if let Some(facts) = self.raider_view_facts(entity) {
            view.species.raider = facts;
        }
    }

    pub(crate) fn witch_ai_step(&mut self, id: EntityId) {
        let alive = self.entities.store.get(id).is_some_and(|entity| {
            entity
                .living()
                .is_some_and(|state| !living::is_dead_or_dying(&entity.base, state))
        });
        if !alive {
            return;
        }
        if let Some(entry) = self.entities.mobs.entries.get_mut(&id) {
            entry.ai.raider_ai_step();
        }
        let mut random = self.mob_random(id);
        let drinking = self
            .entities
            .store
            .get(id)
            .is_some_and(|entity| witch::is_drinking_potion(&entity.base));
        if drinking {
            if self.witch_countdown(id) {
                self.witch_finish_drinking(id);
            }
        } else if let Some(name) = self.witch_choice(id, &mut random) {
            self.witch_start_drinking(id, name, &mut random);
        }
        if random.next_float() < witch::PARTICLE_ODDS {
            self.push(
                Audience::Trackers(id),
                ClientEvent::EntityStatus {
                    entity: id,
                    status: EntityStatus::Event(witch::PARTICLE_EVENT),
                },
            );
        }
        self.store_mob_random(id, random);
    }

    fn witch_state_mut(&mut self, id: EntityId) -> Option<&mut witch::WitchState> {
        match &mut self
            .entities
            .store
            .get_mut(id)?
            .living_mut()?
            .mob
            .as_mut()?
            .species
        {
            Species::Witch(state) => Some(state),
            _ => None,
        }
    }

    fn witch_countdown(&mut self, id: EntityId) -> bool {
        let Some(state) = self.witch_state_mut(id) else {
            return false;
        };
        let before = state.using_time;
        state.using_time = before.wrapping_sub(1);
        before < 1
    }

    fn witch_choice(
        &mut self,
        id: EntityId,
        random: &mut impl RandomSource,
    ) -> Option<&'static str> {
        let entity = self.entities.store.get(id)?;
        let state = entity.living()?;
        let game_time = self.world.level.game_time;
        let target_distance = self
            .mob_target(id)
            .and_then(|target| self.entity_eye(target))
            .map(|(position, _)| position.distance_squared(entity.base.position));
        let mut copy = state.clone();
        let facts = witch::DrinkFacts {
            eye_in_water: entity.base.eye_in_water,
            has_water_breathing: state.has_effect("minecraft:water_breathing"),
            on_fire: fire::is_on_fire(&entity.base),
            recent_fire_damage: witch::recent_fire_damage(state.damage.last_source, game_time),
            has_fire_resistance: state.has_effect("minecraft:fire_resistance"),
            health: living::health(&entity.base),
            max_health: copy.max_health() as f32,
            target_distance_sqr: target_distance,
            has_speed: state.has_effect("minecraft:speed"),
        };
        witch::choose_drink(&facts, random)
    }

    fn set_mainhand(&mut self, id: EntityId, stack: Option<ItemStack>) {
        let Some(state) = self
            .entities
            .store
            .get_mut(id)
            .and_then(|entity| entity.living_mut())
        else {
            return;
        };
        state.equipment[living::slot_index(EquipmentSlot::Mainhand)] = stack.clone();
        self.push(
            Audience::Trackers(id),
            ClientEvent::EntityEquipment {
                entity: id,
                slots: vec![(wire_slot(EquipmentSlot::Mainhand), stack)],
            },
        );
    }

    fn witch_start_drinking(
        &mut self,
        id: EntityId,
        name: &'static str,
        random: &mut impl RandomSource,
    ) {
        self.set_mainhand(id, Some(potion::stack(ItemKind::Potion, name)));
        if let Some(state) = self.witch_state_mut(id) {
            state.using_time = witch::DRINK_USE_TICKS;
        }
        if let Some(entity) = self.entities.store.get_mut(id) {
            witch::set_using_item(&mut entity.base, true);
        }
        let pitch = random.next_float() * witch::DRINK_PITCH_SPREAD + witch::DRINK_PITCH_BASE;
        self.entity_sound(id, witch::DRINK_SOUND, 1.0, pitch);
        if let Some(state) = self
            .entities
            .store
            .get_mut(id)
            .and_then(|entity| entity.living_mut())
        {
            state.attributes.add_or_replace_modifier(
                MOVEMENT_SPEED,
                AttributeModifier {
                    id: Cow::Borrowed(witch::DRINK_MODIFIER),
                    amount: witch::DRINK_SPEED_PENALTY,
                    operation: AttributeOperation::AddValue,
                },
            );
        }
    }

    fn witch_finish_drinking(&mut self, id: EntityId) {
        if let Some(entity) = self.entities.store.get_mut(id) {
            witch::set_using_item(&mut entity.base, false);
        }
        let held = self
            .entities
            .store
            .get(id)
            .and_then(|entity| entity.living())
            .and_then(|state| state.equipment(EquipmentSlot::Mainhand).cloned());
        self.set_mainhand(id, None);
        if let Some(stack) = held
            && stack.item == ItemKind::Potion.id()
        {
            let scale = potion::duration_scale(&stack);
            for effect in PotionContents::of(&stack).scaled_effects(scale) {
                self.add_effect_to(id, effect, EntityPotionEffectCause::Attack);
            }
        }
        if let Some(state) = self
            .entities
            .store
            .get_mut(id)
            .and_then(|entity| entity.living_mut())
        {
            state
                .attributes
                .remove_modifier(MOVEMENT_SPEED, witch::DRINK_MODIFIER);
        }
    }

    fn throw_target(&self, target: EntityId) -> Option<ThrowTarget> {
        if let Some(player) = self
            .players
            .values()
            .find(|player| player.entity_id == target)
        {
            return Some(ThrowTarget {
                position: player.position,
                eye_y: player.position.y + player.eye_height(),
                motion: Vec3::ZERO,
                health: player.vitals.health,
                raider: false,
                slowness: player.effects.has("minecraft:slowness"),
                poison: player.effects.has("minecraft:poison"),
                weakness: player.effects.has("minecraft:weakness"),
            });
        }
        let entity = self.entities.store.get(target)?;
        let state = entity.living()?;
        Some(ThrowTarget {
            position: entity.base.position,
            eye_y: entity.base.eye_y(),
            motion: entity.base.velocity,
            health: living::health(&entity.base),
            raider: entity.base.kind.extends("Raider"),
            slowness: state.has_effect("minecraft:slowness"),
            poison: state.has_effect("minecraft:poison"),
            weakness: state.has_effect("minecraft:weakness"),
        })
    }

    pub(crate) fn witch_throw(&mut self, id: EntityId, target: EntityId) {
        let Some(witch_entity) = self.entities.store.get(id) else {
            return;
        };
        if witch::is_drinking_potion(&witch_entity.base) {
            return;
        }
        let origin = witch_entity.base.position;
        let Some(aimed) = self.throw_target(target) else {
            return;
        };
        let delta = (
            aimed.position.x + aimed.motion.x - origin.x,
            (aimed.eye_y - witch::TARGET_EYE_OFFSET) - origin.y,
            aimed.position.z + aimed.motion.z - origin.z,
        );
        let (direction, distance) = witch::throw_aim(delta);
        let mut random = self.mob_random(id);
        let throw = witch::choose_throw(
            &witch::ThrowFacts {
                target_is_raider: aimed.raider,
                target_health: aimed.health,
                target_has_slowness: aimed.slowness,
                target_has_poison: aimed.poison,
                target_has_weakness: aimed.weakness,
                distance,
            },
            &mut random,
        );
        if throw.clear_target {
            self.forget_target(id, TargetReason::Unknown);
        }
        let stack = potion::stack(ItemKind::SplashPotion, throw.potion);
        self.launch_thrown_item(
            Kind::SplashPotion,
            id,
            Some(&stack),
            move |_| direction,
            witch::throw_power(distance),
            witch::THROW_UNCERTAINTY,
        );
        let pitch = witch::THROW_PITCH_BASE + random.next_float() * witch::THROW_PITCH_SPREAD;
        self.store_mob_random(id, random);
        self.entity_sound(id, witch::THROW_SOUND, 1.0, pitch);
    }
}

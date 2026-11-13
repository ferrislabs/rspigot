use std::num::NonZeroU8;

use rspigot_ai::{
    AiWorld, MobBody, MobView, PandaAction, PandaFacts, TargetReason, TemptItems, inflate,
    look_at_entity,
};
use rspigot_domain::entity::living::{self, LivingState, slot_index};
use rspigot_domain::entity::species::Species;
use rspigot_domain::entity::species::panda;
use rspigot_domain::entity::{Entity, EntityBase, EntityState, ageable};
use rspigot_domain::item::ItemStack;
use rspigot_domain::player::PlayerId;
use rspigot_domain::random::{LegacyRandom, RandomSource};
use rspigot_domain::{EntityId, Vec3, mth};
use rspigot_entities::EntityKind as Kind;
use rspigot_items::EquipmentSlot;

use crate::entities::wire_slot;
use crate::events::{ClientEvent, Hand};
use crate::health::DamageSource;
use crate::interact::Interaction;
use crate::loot::{self, LootContext};
use crate::mobs::{ServerRandom, sky_darken};
use crate::plugins::events::EntityPickupItem;
use crate::plugins::steps::MobPickupStep;
use crate::plugins::{Continuation, EventPayload};
use crate::ports::ClientLink;
use crate::server::{Audience, Server};

const PANIC_ENVIRONMENTAL_CAUSES: &str = "minecraft:panic_environmental_causes";
const PANDA_FOOD: TemptItems = TemptItems::Tag("minecraft:panda_food");
const EATS_FROM_GROUND: TemptItems = TemptItems::Tag("minecraft:panda_eats_from_ground");
const PANDA_SNEEZE: &str = "minecraft:gameplay/panda_sneeze";
const CANT_BREED: &str = "minecraft:entity.panda.cant_breed";
const PRE_SNEEZE: &str = "minecraft:entity.panda.pre_sneeze";
const SNEEZE: &str = "minecraft:entity.panda.sneeze";
const EAT: &str = "minecraft:entity.panda.eat";
const BITE: &str = "minecraft:entity.panda.bite";
const DAMAGE_SOURCE_MEMORY: i64 = 40;
const SNEEZE_JUMP_RANGE: f64 = 10.0;
const PICKUP_REACH: (f64, f64, f64) = (1.0, 0.0, 1.0);
const UNHAPPY_SOUNDS: [i32; 2] = [29, 14];
const EAT_START_ROLL: i32 = 80;
const EAT_FINISH_ROLL: i32 = 20;
const EAT_MIN_COUNTER: i32 = 80;
const EAT_FROM_GROUND_COUNTER: i32 = 100;
const EATING_PARTICLES: i32 = 6;
const PARTICLE_RANDOMS: i32 = 6;
const ROLL_BOOST: f64 = 0.27;
const ROLL_SPEED_BABY: f32 = 0.1;
const ROLL_SPEED_ADULT: f32 = 0.2;
const ROLL_HOPS: [i32; 3] = [7, 15, 23];
const GUARANTEED_DROP: f32 = 2.0;
const LOOK_TURN: f32 = 90.0;

fn panda_state(entity: &Entity) -> Option<&panda::PandaState> {
    match &entity.living()?.mob.as_ref()?.species {
        Species::Panda(state) => Some(state),
        _ => None,
    }
}

fn panda_state_mut(entity: &mut Entity) -> Option<&mut panda::PandaState> {
    match &mut entity.living_mut()?.mob.as_mut()?.species {
        Species::Panda(state) => Some(state),
        _ => None,
    }
}

fn is_panda_food(stack: &ItemStack) -> bool {
    PANDA_FOOD.matches(Some(stack.item))
}

fn main_hand(entity: &Entity) -> Option<&ItemStack> {
    entity.living()?.equipment(EquipmentSlot::Mainhand)
}

impl<L: ClientLink> Server<L> {
    pub(crate) fn panda_thundering(&self) -> bool {
        self.world.dimension.has_skylight
            && !self.spawning.has_ceiling()
            && self.world.level.raining
            && self.world.level.thundering
    }

    pub(crate) fn panda_view(&self, entity: &Entity, view: &mut MobView) {
        let base = &entity.base;
        let Some(state) = panda_state(entity) else {
            return;
        };
        view.species.panda = PandaFacts {
            variant: panda::variant(base),
            unhappy_counter: panda::unhappy_counter(base),
            sitting: panda::is_sitting(base),
            on_back: panda::is_on_back(base),
            rolling: panda::is_rolling(base),
            eating: panda::is_eating(base),
            thundering: self.panda_thundering(),
            holding: main_hand(entity).is_some(),
            got_bamboo: state.got_bamboo,
            did_bite: state.did_bite,
        };
        view.panic = entity
            .living()
            .and_then(|living| living.damage.last_source)
            .is_some_and(|(kind, at)| {
                self.world.level.game_time - at <= DAMAGE_SOURCE_MEMORY
                    && kind.in_tag(PANIC_ENVIRONMENTAL_CAUSES)
            });
    }

    pub(crate) fn panda_pre_tick(&mut self, _id: EntityId) -> bool {
        true
    }

    pub(crate) fn panda_post_tick(&mut self, id: EntityId) {
        self.panda_pick_up_loot(id);
        self.panda_tick(id);
    }

    pub(crate) fn panda_custom_ai_step(&mut self, _id: EntityId) {}

    fn panda_tick(&mut self, id: EntityId) {
        let thundering = self.panda_thundering();
        let target = self.mob_target(id);
        let mut random = self.mob_random(id);
        {
            let Some(entity) = self.entities.store.get_mut(id) else {
                return;
            };
            let in_water = entity.base.fluid.in_water;
            let base = &mut entity.base;
            if panda::is_worried(base) {
                if thundering && !in_water {
                    panda::sit(base, true);
                    panda::eat(base, false);
                } else if !panda::is_eating(base) {
                    panda::sit(base, false);
                }
            }
            if target.is_none()
                && let Some(state) = panda_state_mut(entity)
            {
                state.got_bamboo = false;
                state.did_bite = false;
            }
        }
        self.panda_tick_unhappy(id, target);
        self.panda_tick_sneeze(id);
        self.panda_tick_roll(id);
        if let Some(entity) = self.entities.store.get_mut(id)
            && panda::is_sitting(&entity.base)
        {
            entity.base.set_pitch(0.0);
        }
        self.panda_handle_eating(id, thundering, &mut random);
        self.store_mob_random(id, random);
    }

    fn panda_tick_unhappy(&mut self, id: EntityId, target: Option<EntityId>) {
        let Some(counter) = self
            .entities
            .store
            .get(id)
            .map(|entity| panda::unhappy_counter(&entity.base))
            .filter(|counter| *counter > 0)
        else {
            return;
        };
        if let Some(target) = target {
            self.panda_look_at(id, target);
        }
        if UNHAPPY_SOUNDS.contains(&counter) {
            self.entity_sound(id, CANT_BREED, 1.0, 1.0);
        }
        if let Some(entity) = self.entities.store.get_mut(id) {
            panda::set_unhappy_counter(&mut entity.base, counter - 1);
        }
    }

    fn panda_look_at(&mut self, id: EntityId, target: EntityId) {
        let Some(view) = self.mob_view(id) else {
            return;
        };
        let mut body = MobBody::of(&view);
        {
            let world = self.mob_world(sky_darken(&self.world.level));
            let Some(target) = world.living(target) else {
                return;
            };
            look_at_entity(&view, &mut body, &target, LOOK_TURN, LOOK_TURN);
        }
        if let Some(entity) = self.entities.store.get_mut(id) {
            if body.yaw != view.yaw {
                entity.base.set_yaw(body.yaw);
            }
            if body.pitch != view.pitch {
                entity.base.set_pitch(body.pitch);
            }
        }
    }

    fn panda_tick_sneeze(&mut self, id: EntityId) {
        let Some(entity) = self.entities.store.get_mut(id) else {
            return;
        };
        if !panda::is_sneezing(&entity.base) {
            return;
        }
        let counter = panda::sneeze_counter(&entity.base) + 1;
        panda::set_sneeze_counter(&mut entity.base, counter);
        if counter > panda::SNEEZE_TICKS {
            panda::sneeze(&mut entity.base, false);
            self.panda_after_sneeze(id);
        } else if counter == 1 {
            self.entity_sound(id, PRE_SNEEZE, 1.0, 1.0);
        }
    }

    fn panda_after_sneeze(&mut self, id: EntityId) {
        self.entity_sound(id, SNEEZE, 1.0, 1.0);
        let Some(entity) = self.entities.store.get(id) else {
            return;
        };
        let area = entity.base.bounding_box().inflate(SNEEZE_JUMP_RANGE);
        let thundering = self.panda_thundering();
        let jumpers: Vec<EntityId> = self
            .entities
            .store
            .iter()
            .filter(|other| other.base.kind == Kind::Panda)
            .filter(|other| other.base.bounding_box().intersects(&area))
            .filter(|other| {
                !other.is_baby()
                    && other.base.on_ground
                    && !other.base.fluid.in_water
                    && panda::can_perform_action(&other.base, thundering)
            })
            .map(|other| other.base.id)
            .collect();
        for jumper in jumpers {
            if let Some(Entity {
                base,
                state: EntityState::Living(state),
                ..
            }) = self.entities.store.get_mut(jumper)
            {
                living::jump_from_ground(base, state, &self.world.chunks);
            }
        }
        if self.config.rules.mob_drops {
            self.drop_gift_loot(id, PANDA_SNEEZE);
        }
    }

    pub(crate) fn drop_gift_loot(&mut self, id: EntityId, table: &str) {
        let Some(entity) = self.entities.store.get(id) else {
            return;
        };
        let facts = crate::mobs::loot_facts_of(entity);
        let drops = rspigot_data::loot::table(table).map_or_else(Vec::new, |table| {
            let context = LootContext {
                this: Some(&facts),
                ..LootContext::default()
            };
            loot::table_drops(table, &context, &mut self.rng)
        });
        for stack in drops {
            self.drop_at_location(id, 0.0, stack, Vec3::ZERO);
        }
    }

    fn panda_tick_roll(&mut self, id: EntityId) {
        let Some(entity) = self.entities.store.get_mut(id) else {
            return;
        };
        if !panda::is_rolling(&entity.base) {
            if let Some(state) = panda_state_mut(entity) {
                state.roll_counter = 0;
            }
            return;
        }
        let baby = entity.is_baby();
        let base = &entity.base;
        let (yaw, velocity, on_ground) = (base.yaw, base.velocity, base.on_ground);
        let Some(state) = panda_state_mut(entity) else {
            return;
        };
        state.roll_counter += 1;
        let counter = state.roll_counter;
        if counter > panda::TOTAL_ROLL_STEPS {
            panda::roll(&mut entity.base, false);
            return;
        }
        let velocity = if counter == 1 {
            let radians = f64::from(yaw * mth::DEG_TO_RAD);
            let speed = if baby {
                ROLL_SPEED_BABY
            } else {
                ROLL_SPEED_ADULT
            };
            let delta = Vec3::new(
                velocity.x + f64::from(-mth::sin(radians) * speed),
                0.0,
                velocity.z + f64::from(mth::cos(radians) * speed),
            );
            state.set_roll_delta(delta);
            delta + Vec3::new(0.0, ROLL_BOOST, 0.0)
        } else {
            let (x, z) = state.roll_delta();
            if ROLL_HOPS.contains(&counter) {
                Vec3::new(0.0, if on_ground { ROLL_BOOST } else { velocity.y }, 0.0)
            } else {
                Vec3::new(x, velocity.y, z)
            }
        };
        entity.base.velocity = velocity;
        entity.base.needs_sync = true;
    }

    fn panda_handle_eating(&mut self, id: EntityId, thundering: bool, random: &mut LegacyRandom) {
        let Some(entity) = self.entities.store.get(id) else {
            return;
        };
        let holding = main_hand(entity).is_some();
        let base = &entity.base;
        let scared = panda::is_scared(base, thundering);
        let (eating, sitting) = (panda::is_eating(base), panda::is_sitting(base));
        let start =
            !eating && sitting && !scared && holding && random.next_int(EAT_START_ROLL) == 1;
        if let Some(entity) = self.entities.store.get_mut(id) {
            if start {
                panda::eat(&mut entity.base, true);
            } else if !holding || !sitting {
                panda::eat(&mut entity.base, false);
            }
        }
        let Some(entity) = self.entities.store.get(id) else {
            return;
        };
        if !panda::is_eating(&entity.base) {
            return;
        }
        let counter = panda::eat_counter(&entity.base);
        if counter % panda::EAT_TICK_INTERVAL == 0 {
            let volume = 0.5 + 0.5 * random.next_int(2) as f32;
            let pitch = (random.next_float() - random.next_float()) * 0.2 + 1.0;
            if holding {
                for _ in 0..EATING_PARTICLES * PARTICLE_RANDOMS {
                    random.next_float();
                }
            }
            self.entity_sound(id, EAT, volume, pitch);
        }
        if counter > EAT_MIN_COUNTER && random.next_int(EAT_FINISH_ROLL) == 1 {
            let eaten = counter > EAT_FROM_GROUND_COUNTER
                && self
                    .entities
                    .store
                    .get(id)
                    .and_then(main_hand)
                    .is_some_and(|stack| EATS_FROM_GROUND.matches(Some(stack.item)));
            if eaten {
                self.set_panda_main_hand(id, None);
                if let Some(entity) = self.entities.store.get_mut(id) {
                    panda::sit(&mut entity.base, false);
                }
            }
            if let Some(entity) = self.entities.store.get_mut(id) {
                panda::eat(&mut entity.base, false);
            }
            return;
        }
        if let Some(entity) = self.entities.store.get_mut(id) {
            panda::set_eat_counter(&mut entity.base, counter + 1);
        }
    }

    pub(crate) fn set_panda_main_hand(&mut self, id: EntityId, stack: Option<ItemStack>) {
        let Some(living) = self
            .entities
            .store
            .get_mut(id)
            .and_then(|entity| entity.living_mut())
        else {
            return;
        };
        living.equipment[slot_index(EquipmentSlot::Mainhand)] = stack.clone();
        self.push(
            Audience::Trackers(id),
            ClientEvent::EntityEquipment {
                entity: id,
                slots: vec![(wire_slot(EquipmentSlot::Mainhand), stack)],
            },
        );
    }

    fn panda_pick_up_loot(&mut self, id: EntityId) {
        let Some(entity) = self.entities.store.get(id) else {
            return;
        };
        let Some(living) = entity.living() else {
            return;
        };
        let can_pick_up = living.mob.as_ref().is_some_and(|mob| mob.can_pick_up_loot);
        if !can_pick_up
            || living::is_dead_or_dying(&entity.base, living)
            || entity.base.is_removed()
            || !self.config.rules.mob_griefing
        {
            return;
        }
        let area = inflate(
            &entity.base.bounding_box(),
            PICKUP_REACH.0,
            PICKUP_REACH.1,
            PICKUP_REACH.2,
        );
        let items: Vec<EntityId> = self
            .entities
            .store
            .iter()
            .filter(|other| {
                matches!(&other.state, EntityState::Item(item) if item.pickup_delay <= 0)
                    && !other.base.is_removed()
                    && other.base.bounding_box().intersects(&area)
            })
            .map(|other| other.base.id)
            .collect();
        for item in items {
            self.panda_pick_up_item(id, item);
        }
    }

    fn panda_pick_up_item(&mut self, id: EntityId, item: EntityId) {
        let Some(stack) = self.item_stack_of(item) else {
            return;
        };
        let holding = self.entities.store.get(id).and_then(main_hand).is_some();
        let edible = EATS_FROM_GROUND.matches(Some(stack.item))
            && self.entities.store.get(item).is_some_and(|entity| {
                !entity.base.is_removed()
                    && matches!(&entity.state, EntityState::Item(state) if state.pickup_delay <= 0)
            });
        let step = MobPickupStep {
            collector: id,
            item,
            stack,
        };
        let lane = self.lane_for(None, id);
        let describe = move |server: &Self| {
            let entity = server.subject_view(id)?;
            Some(EventPayload::EntityPickupItem(EntityPickupItem {
                entity_type: entity.kind.clone(),
                entity,
                item: server.entity_view(item)?,
                remaining: 0,
            }))
        };
        let then = Continuation::MobPickup(Box::new(step));
        if holding || !edible {
            self.fire_refused(lane, then, describe);
        } else {
            self.fire(lane, then, describe);
        }
    }

    pub(crate) fn finish_mob_pickup(&mut self, step: MobPickupStep, cancelled: bool) {
        if self.kind_of(step.collector) == Some(Kind::Allay) {
            self.finish_allay_pickup(step, cancelled);
            return;
        }
        let MobPickupStep {
            collector,
            item,
            stack,
        } = step;
        if cancelled || self.item_stack_of(item).as_ref() != Some(&stack) {
            return;
        }
        let alive = self.entities.store.get(collector).is_some_and(|entity| {
            entity
                .living()
                .is_some_and(|living| !living::is_dead_or_dying(&entity.base, living))
                && !entity.base.is_removed()
        });
        if !alive {
            return;
        }
        let amount = i32::from(stack.count.get());
        self.set_panda_main_hand(collector, Some(stack));
        if let Some(mob) = self
            .entities
            .store
            .get_mut(collector)
            .and_then(|entity| entity.living_mut())
            .and_then(|living| living.mob.as_mut())
        {
            mob.drop_chances[slot_index(EquipmentSlot::Mainhand)] = GUARANTEED_DROP;
        }
        self.push(
            Audience::Trackers(item),
            ClientEvent::ItemPickup {
                collected: item,
                collector,
                amount,
            },
        );
        self.remove_item(item);
    }

    pub(crate) fn panda_interact(
        &mut self,
        player: PlayerId,
        target: EntityId,
        hand: Hand,
        held: Option<&ItemStack>,
    ) -> Option<Interaction> {
        let thundering = self.panda_thundering();
        let entity = self.entities.store.get(target)?;
        let base = &entity.base;
        if panda::is_scared(base, thundering) {
            return Some(Interaction::Pass);
        }
        if panda::is_on_back(base) {
            let entity = self.entities.store.get_mut(target)?;
            panda::set_on_back(&mut entity.base, false);
            return Some(Interaction::Predicted);
        }
        let Some(stack) = held.filter(|stack| is_panda_food(stack)) else {
            return Some(self.panda_baby_interact(player, target, hand, held));
        };
        let has_target = self.mob_target(target).is_some();
        if has_target
            && let Some(state) = self
                .entities
                .store
                .get_mut(target)
                .and_then(panda_state_mut)
        {
            state.got_bamboo = true;
        }
        let entity = self.entities.store.get(target)?;
        let ageable = entity.living()?.mob.as_ref()?.ageable.as_ref()?;
        let age = ageable.age;
        let can_love = ageable.can_fall_in_love();
        let can_age_up = ageable::can_age_up(&entity.base, ageable);
        let baby = entity.is_baby();
        let (sitting, in_water) = (panda::is_sitting(&entity.base), entity.base.fluid.in_water);
        if can_age_up {
            self.consume_held(player, hand, 1);
            if let Some(Entity {
                base,
                state: EntityState::Living(state),
                ..
            }) = self.entities.store.get_mut(target)
                && let Some(ageable) = state.mob.as_mut().and_then(|mob| mob.ageable.as_mut())
            {
                let seconds = ageable::speed_up_seconds_when_feeding(-age);
                ageable::age_up(base, ageable, seconds, true);
            }
        } else {
            if baby {
                return Some(Interaction::Pass);
            }
            if age == 0 && can_love {
                self.consume_held(player, hand, 1);
                let uuid = self.players.get(&player).map(|p| p.profile.id.0);
                let item = self.held(player, Hand::Main);
                self.ask_love(player, target, uuid, item);
            } else {
                if sitting || in_water {
                    return Some(Interaction::Pass);
                }
                self.panda_try_to_sit(target);
                if let Some(entity) = self.entities.store.get_mut(target) {
                    panda::eat(&mut entity.base, true);
                }
                let previous = self.entities.store.get(target).and_then(main_hand).cloned();
                if let Some(previous) = previous
                    && !self.infinite_materials(player)
                {
                    self.drop_at_location(target, 0.0, previous, Vec3::ZERO);
                }
                let single = ItemStack::new(stack.item, NonZeroU8::MIN);
                self.set_panda_main_hand(target, Some(single));
                self.consume_held(player, hand, 1);
            }
        }
        Some(Interaction::ServerOnly)
    }

    fn panda_baby_interact(
        &mut self,
        player: PlayerId,
        target: EntityId,
        hand: Hand,
        held: Option<&ItemStack>,
    ) -> Interaction {
        let baby = self.entities.store.get(target).is_some_and(Entity::is_baby);
        let dandelion = rspigot_items::ItemKind::GoldenDandelion.id();
        let holding = [Hand::Main, Hand::Off]
            .into_iter()
            .any(|hand| self.held(player, hand).is_some_and(|s| s.item == dandelion));
        match held {
            Some(stack) if baby && holding => self.animal_interact(player, target, hand, stack),
            _ => Interaction::Pass,
        }
    }

    fn panda_try_to_sit(&mut self, id: EntityId) {
        let Some(Entity {
            base,
            state: EntityState::Living(state),
            ..
        }) = self.entities.store.get_mut(id)
        else {
            return;
        };
        if base.fluid.in_water {
            return;
        }
        state.zza = 0.0;
        panda::sit(base, true);
        if let Some(entry) = self.entities.mobs.entries.get_mut(&id) {
            entry.ai.brain_mut().navigation.stop();
        }
    }

    pub(crate) fn panda_stand_up(&mut self, id: EntityId) {
        if let Some(entity) = self.entities.store.get_mut(id)
            && entity.base.kind == Kind::Panda
        {
            panda::sit(&mut entity.base, false);
        }
    }

    pub(crate) fn panda_hurt_amount(
        &mut self,
        _id: EntityId,
        _source: &DamageSource,
        amount: f32,
    ) -> Option<f32> {
        Some(amount)
    }

    pub(crate) fn panda_after_hurt(&mut self, _id: EntityId, _source: &DamageSource, _dead: bool) {}

    pub(crate) fn panda_bite(&mut self, attacker: EntityId) {
        let Some(entity) = self.entities.store.get_mut(attacker) else {
            return;
        };
        if entity.base.kind != Kind::Panda || panda::is_aggressive(&entity.base) {
            return;
        }
        if let Some(state) = panda_state_mut(entity) {
            state.did_bite = true;
        }
    }

    pub(crate) fn panda_attack_sound(&mut self, attacker: EntityId) {
        if self.kind_of(attacker) == Some(Kind::Panda) {
            self.entity_sound(attacker, BITE, 1.0, 1.0);
        }
    }

    pub(crate) fn panda_offspring(
        &mut self,
        parent: EntityId,
        partner: EntityId,
        child: &mut Entity,
    ) {
        let genes = |server: &Self, id: EntityId| {
            server
                .entities
                .store
                .get(id)
                .map(|entity| panda::genes(&entity.base))
        };
        let Some(first) = genes(self, parent) else {
            return;
        };
        let second = genes(self, partner);
        panda::set_gene_from_parents(
            &mut child.base,
            first,
            second,
            &mut ServerRandom(&mut self.rng),
        );
        if let EntityState::Living(state) = &mut child.state {
            panda::set_attributes(&mut child.base, state);
        }
    }

    pub(crate) fn panda_action(&mut self, id: EntityId, action: PandaAction) {
        match action {
            PandaAction::Sit(value) => self.with_panda(id, |base, _| panda::sit(base, value)),
            PandaAction::OnBack(value) => {
                self.with_panda(id, |base, _| panda::set_on_back(base, value));
            }
            PandaAction::Roll(value) => self.with_panda(id, |base, _| panda::roll(base, value)),
            PandaAction::Sneeze(value) => {
                self.with_panda(id, |base, _| panda::sneeze(base, value));
            }
            PandaAction::Unhappy(value) => {
                self.with_panda(id, |base, _| panda::set_unhappy_counter(base, value));
            }
            PandaAction::TryToSit => self.panda_try_to_sit(id),
            PandaAction::DropHeld => {
                let stack = self.entities.store.get(id).and_then(main_hand).cloned();
                if let Some(stack) = stack {
                    self.drop_at_location(id, 0.0, stack, Vec3::ZERO);
                    self.set_panda_main_hand(id, None);
                }
            }
            PandaAction::Alert { mob, target } => {
                if let Some(view) = self.entity_view(mob) {
                    self.request_target(view, target, TargetReason::TargetAttackedEntity);
                }
            }
        }
    }

    fn with_panda(&mut self, id: EntityId, change: impl FnOnce(&mut EntityBase, &mut LivingState)) {
        if let Some(Entity {
            base,
            state: EntityState::Living(state),
            ..
        }) = self.entities.store.get_mut(id)
        {
            change(base, state);
        }
    }
}

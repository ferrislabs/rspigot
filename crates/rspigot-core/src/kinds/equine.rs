use std::num::NonZeroU8;

use rspigot_ai::{EquineAction, EquineFacts, MobView};
use rspigot_domain::attribute::equipment_modifiers;
use rspigot_domain::entity::living::{self, LivingState};
use rspigot_domain::entity::species::equine::{self, MountState};
use rspigot_domain::entity::{Entity, EntityBase, EntityState, ageable};
use rspigot_domain::game_event::GameEvent;
use rspigot_domain::item::ItemStack;
use rspigot_domain::player::PlayerId;
use rspigot_domain::random::RandomSource;
use rspigot_domain::spawn::SpawnReason;
use rspigot_domain::{EntityId, Vec3};
use rspigot_entities::EntityKind as Kind;
use rspigot_items::{EquipmentSlot, ItemKind};

use crate::entities::wire_slot;
use crate::events::{ClientEvent, Hand};
use crate::game_events::GameEventContext;
use crate::interact::{Interaction, item_tag};
use crate::mob_actions::new_mob;
use crate::mob_life::type_in_tag;
use crate::plugins::events::HorseJump;
use crate::plugins::events::{CreatureSpawnReason, EntityRegainHealthRegainReason};
use crate::plugins::steps::{Birth, Carried, Spawned};
use crate::plugins::{Continuation, EventPayload};
use crate::ports::ClientLink;
use crate::server::{Audience, Server};

const GUARANTEED_DROP: f32 = 2.0;
const SADDLE_SOUND: &str = "minecraft:entity.horse.saddle";
const ARMOR_SOUND: &str = "minecraft:entity.horse.armor";
const CHEST_EQUIP_SOUND_VOLUME: f32 = 1.0;
const BODY_ARMOR_MODIFIER: &str = "minecraft:armor.body";
const BODY_ARMOR_ATTRIBUTES: [&str; 3] = [
    living::ARMOR,
    "minecraft:armor_toughness",
    "minecraft:knockback_resistance",
];
const CAN_EQUIP_SADDLE: &str = "minecraft:can_equip_saddle";
const CAN_WEAR_HORSE_ARMOR: &str = "minecraft:can_wear_horse_armor";
const HORSE_FOOD: &str = "minecraft:horse_food";
const ZOMBIE_HORSE_FOOD: &str = "minecraft:zombie_horse_food";
const REGEN_ROLL: i32 = 900;
const EAT_GRASS_ROLL: i32 = 300;
const TAIL_ROLL: i32 = 200;
const EATING_TICKS: i32 = 50;
const HURT_STAND_ROLL: i32 = 3;
const BUCK_TEMPER: i32 = 5;
const BUCK_EVENT: i8 = 6;
const TAME_EVENT: i8 = 7;
const JUMP_VOLUME: f32 = 0.4;
const TRAP_PUSH_SPREAD: f64 = 1.1485;
const TRAP_INVULNERABLE_TICKS: i32 = 60;

#[derive(Debug, Clone, Copy)]
struct Feed {
    heal: f32,
    age_up: i32,
    temper: i32,
    love: bool,
}

fn feed_of(item: ItemKind) -> Option<Feed> {
    let plain = |heal, age_up, temper| Feed {
        heal,
        age_up,
        temper,
        love: false,
    };
    Some(match item {
        ItemKind::Wheat => plain(2.0, 20, 3),
        ItemKind::Sugar => plain(1.0, 30, 3),
        ItemKind::HayBlock => plain(20.0, 180, 0),
        ItemKind::Apple => plain(3.0, 60, 3),
        ItemKind::RedMushroom => plain(3.0, 0, 3),
        ItemKind::Carrot => plain(3.0, 60, 3),
        ItemKind::GoldenCarrot => Feed {
            love: true,
            ..plain(4.0, 60, 5)
        },
        ItemKind::GoldenApple | ItemKind::EnchantedGoldenApple => Feed {
            love: true,
            ..plain(10.0, 240, 10)
        },
        _ => return None,
    })
}

pub(crate) fn mount_of(entity: &Entity) -> Option<&MountState> {
    entity.living()?.mob.as_ref()?.mount.as_deref()
}

pub(crate) fn can_use_slot(entity: &Entity, slot: EquipmentSlot) -> bool {
    let kind = entity.base.kind;
    match slot {
        EquipmentSlot::Saddle => {
            matches!(kind, Kind::Horse | Kind::SkeletonHorse | Kind::ZombieHorse)
                || (entity
                    .living()
                    .is_some_and(|living| !living::is_dead_or_dying(&entity.base, living))
                    && !entity.is_baby()
                    && equine::is_tamed(&entity.base))
        }
        _ => true,
    }
}

pub(crate) fn is_equippable_in(entity: &Entity, stack: &ItemStack, slot: EquipmentSlot) -> bool {
    let Some(item) = stack.item.kind() else {
        return false;
    };
    if item.equippable() != Some(slot) || !can_use_slot(entity, slot) {
        return false;
    }
    let allowed = match slot {
        EquipmentSlot::Saddle => CAN_EQUIP_SADDLE,
        EquipmentSlot::Body => CAN_WEAR_HORSE_ARMOR,
        _ => return false,
    };
    type_in_tag(entity.base.kind, allowed)
}

fn voice(kind: Kind, baby: bool, adult: &'static str, young: &'static str) -> &'static str {
    if kind == Kind::Horse && baby {
        young
    } else {
        adult
    }
}

pub(crate) fn ambient_sound(entity: &Entity) -> Option<&'static str> {
    let baby = entity.is_baby();
    Some(match entity.base.kind {
        Kind::Horse => voice(
            Kind::Horse,
            baby,
            "minecraft:entity.horse.ambient",
            "minecraft:entity.baby_horse.ambient",
        ),
        Kind::Donkey => "minecraft:entity.donkey.ambient",
        Kind::Mule => "minecraft:entity.mule.ambient",
        Kind::SkeletonHorse => {
            if entity.base.eye_in_water {
                "minecraft:entity.skeleton_horse.ambient_water"
            } else {
                "minecraft:entity.skeleton_horse.ambient"
            }
        }
        Kind::ZombieHorse => "minecraft:entity.zombie_horse.ambient",
        _ => return None,
    })
}

fn angry_sound(entity: &Entity) -> Option<&'static str> {
    let baby = entity.is_baby();
    Some(match entity.base.kind {
        Kind::Horse => voice(
            Kind::Horse,
            baby,
            "minecraft:entity.horse.angry",
            "minecraft:entity.baby_horse.angry",
        ),
        Kind::Donkey => "minecraft:entity.donkey.angry",
        Kind::Mule => "minecraft:entity.mule.angry",
        Kind::ZombieHorse => "minecraft:entity.zombie_horse.angry",
        _ => return None,
    })
}

fn eating_sound(entity: &Entity) -> Option<&'static str> {
    let baby = entity.is_baby();
    Some(match entity.base.kind {
        Kind::Horse => voice(
            Kind::Horse,
            baby,
            "minecraft:entity.horse.eat",
            "minecraft:entity.baby_horse.eat",
        ),
        Kind::Donkey => "minecraft:entity.donkey.eat",
        Kind::Mule => "minecraft:entity.mule.eat",
        Kind::ZombieHorse => "minecraft:entity.zombie_horse.eat",
        _ => return None,
    })
}

pub(crate) fn food_tag(kind: Kind) -> Option<&'static str> {
    match kind {
        Kind::Horse | Kind::Donkey | Kind::Mule | Kind::SkeletonHorse => Some(HORSE_FOOD),
        Kind::ZombieHorse => Some(ZOMBIE_HORSE_FOOD),
        _ => None,
    }
}

fn equine_state(entity: &mut Entity) -> Option<(&mut EntityBase, &mut MountState)> {
    let Entity {
        base,
        state: EntityState::Living(state),
        ..
    } = entity
    else {
        return None;
    };
    let mount = state.mob.as_mut()?.mount.as_deref_mut()?;
    Some((base, mount))
}

impl<L: ClientLink> Server<L> {
    pub(crate) fn mount_facts(&self, entity: &Entity) -> EquineFacts {
        let base = &entity.base;
        let id = base.id;
        let Some(mount) = mount_of(entity) else {
            return EquineFacts::default();
        };
        let first = self.entities.rides.first_passenger(id);
        let player = first.filter(|passenger| {
            self.players
                .values()
                .any(|player| player.entity_id == *passenger)
        });
        let mob_controlled = base.kind == Kind::ZombieHorse
            && first.is_some_and(|passenger| {
                self.entities
                    .store
                    .get(passenger)
                    .is_some_and(|rider| rider.living().is_some_and(LivingState::is_mob))
            });
        EquineFacts {
            temper: mount.temper,
            max_temper: mount.max_temper(),
            standing: equine::is_standing(base),
            eating: equine::is_eating(base),
            saddled: entity
                .living()
                .is_some_and(|living| living.equipment(EquipmentSlot::Saddle).is_some()),
            vehicle: self.entities.rides.is_vehicle(id),
            passenger_player: player,
            has_passenger: first.is_some(),
            mob_controlled,
            trap: mount.trap,
            can_parent: self.can_parent(entity),
        }
    }

    pub(crate) fn can_parent(&self, entity: &Entity) -> bool {
        let base = &entity.base;
        let Some(living) = entity.living() else {
            return false;
        };
        let mut attributes = living.attributes.clone();
        let max = attributes.value(living::MAX_HEALTH, &living.effects) as f32;
        let in_love = living
            .mob
            .as_ref()
            .and_then(|mob| mob.ageable.as_ref())
            .is_some_and(ageable::AgeableState::in_love);
        !self.entities.rides.is_vehicle(base.id)
            && !self.entities.rides.is_passenger(base.id)
            && equine::is_tamed(base)
            && !entity.is_baby()
            && living::health(base) >= max
            && in_love
    }

    pub(crate) fn equine_view(&self, entity: &Entity, view: &mut MobView) {
        let facts = self.mount_facts(entity);
        view.tamed = equine::is_tamed(&entity.base);
        let world = self.mob_world(crate::mobs::sky_darken(&self.world.level));
        view.owner = mount_of(entity)
            .and_then(|mount| mount.owner)
            .and_then(|uuid| world.resolve(uuid));
        if facts.mob_controlled {
            view.panic = false;
            view.panic_environmental = false;
        }
        view.species.equine = facts;
    }

    pub(crate) fn equine_pre_tick(&mut self, id: EntityId) -> bool {
        if let Some((base, mount)) = self
            .entities
            .store
            .get_mut(id)
            .and_then(|entity| equine_state(entity))
        {
            mount.tick(base);
        }
        self.sync_body_armor_modifiers(id);
        true
    }

    fn sync_body_armor_modifiers(&mut self, id: EntityId) {
        let Some(state) = self
            .entities
            .store
            .get_mut(id)
            .and_then(|entity| entity.living_mut())
        else {
            return;
        };
        let body = state
            .equipment(EquipmentSlot::Body)
            .and_then(|stack| stack.item.kind().map(|kind| (kind, stack.item)));
        let wanted: Vec<_> = match body {
            Some((kind, item)) if kind.equippable() == Some(EquipmentSlot::Body) => {
                equipment_modifiers(EquipmentSlot::Body, item).collect()
            }
            _ => Vec::new(),
        };
        for attribute in BODY_ARMOR_ATTRIBUTES {
            let amount = wanted
                .iter()
                .find(|(name, _)| *name == attribute)
                .map(|(_, modifier)| modifier.amount);
            if state
                .attributes
                .modifier_amount(attribute, BODY_ARMOR_MODIFIER)
                == amount
            {
                continue;
            }
            state
                .attributes
                .remove_modifier(attribute, BODY_ARMOR_MODIFIER);
            if let Some((_, modifier)) = wanted.iter().find(|(name, _)| *name == attribute) {
                state.attributes.add_modifier(attribute, modifier.clone());
            }
        }
    }

    pub(crate) fn equine_ai_step(&mut self, id: EntityId) {
        let mut random = self.mob_random(id);
        let tail = random.next_int(TAIL_ROLL) == 0;
        let regen = random.next_int(REGEN_ROLL) == 0;
        let eat_roll = random.next_int(EAT_GRASS_ROLL) == 0;
        self.store_mob_random(id, random);
        let below_grass = self.entities.store.get(id).is_some_and(|entity| {
            self.world
                .chunks
                .block(entity.base.position.block().offset(0, -1, 0))
                .and_then(rspigot_blocks::StateId::kind)
                == Some(rspigot_blocks::BlockKind::GrassBlock)
        });
        let vehicle = self.entities.rides.is_vehicle(id);
        let Some(entity) = self.entities.store.get_mut(id) else {
            return;
        };
        let kind = entity.base.kind;
        let healthy = entity.living().is_some_and(|living| {
            !living::is_dead_or_dying(&entity.base, living) && living.death_time == 0
        });
        let alive = entity
            .living()
            .is_some_and(|living| !living::is_dead_or_dying(&entity.base, living));
        let persistent = entity
            .living()
            .and_then(|living| living.mob.as_ref())
            .is_some_and(|mob| mob.persistence_required);
        let mut expired = false;
        let dead = !alive;
        if let Some((base, mount)) = equine_state(entity) {
            if tail {
                mount.move_tail();
            }
            if dead {
                return;
            }
            if kind == Kind::SkeletonHorse && !persistent && mount.trap {
                let spent = mount.trap_time;
                mount.trap_time += 1;
                expired = spent >= equine::TRAP_LIFETIME;
            }
            if !vehicle && !equine::is_eating(base) && eat_roll && below_grass {
                equine::set_eating(base, true);
            }
            if equine::is_eating(base) {
                mount.eating_counter += 1;
                if mount.eating_counter > EATING_TICKS {
                    mount.eating_counter = 0;
                    equine::set_eating(base, false);
                }
            }
        }
        if expired {
            self.remove_entity(
                id,
                rspigot_domain::entity::RemovalReason::Discarded,
                Some(crate::plugins::events::EntityRemoveCause::Despawn),
            );
            return;
        }
        if regen && healthy {
            self.heal_mob(None, id, 1.0, EntityRegainHealthRegainReason::Regen);
        }
    }

    pub(crate) fn equine_is_immobile(&self, id: EntityId) -> bool {
        let Some(entity) = self.entities.store.get(id) else {
            return false;
        };
        let dead = entity
            .living()
            .is_some_and(|living| living::is_dead_or_dying(&entity.base, living));
        let saddled = entity
            .living()
            .is_some_and(|living| living.equipment(EquipmentSlot::Saddle).is_some());
        (dead && self.entities.rides.is_vehicle(id) && saddled)
            || equine::is_eating(&entity.base)
            || equine::is_standing(&entity.base)
    }

    pub(crate) fn equine_stand_if_possible(&mut self, id: EntityId) {
        let Some(entity) = self.entities.store.get_mut(id) else {
            return;
        };
        let effective = entity
            .living()
            .is_some_and(|living| living::effective_ai(&entity.base, living));
        if !effective || !equine::can_perform_rearing(entity.base.kind) {
            return;
        }
        if let Some((base, mount)) = equine_state(entity) {
            mount.mark_standing(base, equine::STAND_TICKS);
        }
    }

    pub(crate) fn equine_make_mad(&mut self, id: EntityId) {
        let Some(entity) = self.entities.store.get(id) else {
            return;
        };
        if equine::is_standing(&entity.base) {
            return;
        }
        let sound = angry_sound(entity);
        self.equine_stand_if_possible(id);
        if let Some(sound) = sound {
            self.mount_voice(id, sound);
        }
    }

    fn mount_voice(&mut self, id: EntityId, sound: &'static str) {
        self.play_mob_voice(id, move |_| Some(sound));
    }

    pub(crate) fn equine_eating(&mut self, id: EntityId) {
        if let Some((base, mount)) = self
            .entities
            .store
            .get_mut(id)
            .and_then(|entity| equine_state(entity))
        {
            mount.open_mouth(base);
        }
        let Some(entity) = self.entities.store.get(id) else {
            return;
        };
        let Some(sound) = eating_sound(entity) else {
            return;
        };
        let mut random = self.mob_random(id);
        let pitch = (random.next_float() - random.next_float()) * 0.2 + 1.0;
        self.store_mob_random(id, random);
        self.entity_sound(id, sound, 1.0, pitch);
    }

    pub(crate) fn equine_hurt(&mut self, id: EntityId) {
        let mut random = self.mob_random(id);
        let stand = random.next_int(HURT_STAND_ROLL) == 0;
        self.store_mob_random(id, random);
        if stand {
            self.equine_stand_if_possible(id);
        }
    }

    pub(crate) fn equine_action(&mut self, id: EntityId, action: EquineAction) {
        match action {
            EquineAction::Stand => {
                self.equine_stand_if_possible(id);
                self.play_ambient_sound(id);
            }
            EquineAction::Buck { raise_temper } => self.equine_buck(id, raise_temper),
            EquineAction::TryTame { player } => {
                if let Some(owner) = self
                    .players
                    .values()
                    .find(|candidate| candidate.entity_id == player)
                    .map(|candidate| candidate.id)
                {
                    self.ask_tame(owner, id);
                }
            }
            EquineAction::SpringTrap => self.spring_skeleton_trap(id),
        }
    }

    pub(crate) fn equine_buck(&mut self, id: EntityId, raise_temper: bool) {
        if raise_temper
            && let Some((_, mount)) = self
                .entities
                .store
                .get_mut(id)
                .and_then(|entity| equine_state(entity))
        {
            mount.modify_temper(BUCK_TEMPER);
        }
        self.eject_passengers(id);
        self.equine_make_mad(id);
        self.mob_entity_event(id, BUCK_EVENT);
    }

    pub(crate) fn equine_tame_with_name(&mut self, id: EntityId, player: PlayerId) {
        let Some(uuid) = self.players.get(&player).map(|player| player.profile.id.0) else {
            return;
        };
        if let Some((base, mount)) = self
            .entities
            .store
            .get_mut(id)
            .and_then(|entity| equine_state(entity))
        {
            mount.owner = Some(uuid);
            equine::set_tamed(base, true);
        }
        self.mob_entity_event(id, TAME_EVENT);
    }

    fn handle_eating(&mut self, player: PlayerId, target: EntityId, stack: &ItemStack) -> bool {
        let Some(feed) = stack.item.kind().and_then(feed_of) else {
            return false;
        };
        let Some(entity) = self.entities.store.get(target) else {
            return false;
        };
        let base = &entity.base;
        let Some(living) = entity.living() else {
            return false;
        };
        let mut attributes = living.attributes.clone();
        let max_health = attributes.value(living::MAX_HEALTH, &living.effects) as f32;
        let health = living::health(base);
        let tamed = equine::is_tamed(base);
        let baby = entity.is_baby();
        let age_locked = ageable::is_age_locked(base);
        let (age, in_love) = living
            .mob
            .as_ref()
            .and_then(|mob| mob.ageable.as_ref())
            .map_or((0, false), |state| (state.age, state.in_love()));
        let (temper, max_temper) = mount_of(entity).map_or((0, 0), |m| (m.temper, m.max_temper()));
        let mut fed = false;
        if feed.love && tamed && age == 0 && !in_love {
            fed = true;
            let uuid = self.players.get(&player).map(|p| p.profile.id.0);
            self.ask_love(player, target, uuid, None);
        }
        if health < max_health && feed.heal > 0.0 {
            self.heal_mob(
                Some(player),
                target,
                feed.heal,
                EntityRegainHealthRegainReason::Eating,
            );
            fed = true;
        }
        if baby && feed.age_up > 0 && !age_locked {
            if let Some(Entity {
                base,
                state: EntityState::Living(state),
                ..
            }) = self.entities.store.get_mut(target)
                && let Some(ageable) = state.mob.as_mut().and_then(|mob| mob.ageable.as_mut())
            {
                ageable::age_up(base, ageable, feed.age_up, false);
            }
            fed = true;
        }
        if feed.temper > 0 && (fed || !tamed) && temper < max_temper {
            if let Some((_, mount)) = self
                .entities
                .store
                .get_mut(target)
                .and_then(|entity| equine_state(entity))
            {
                mount.modify_temper(feed.temper);
            }
            fed = true;
        }
        if fed {
            self.equine_eating(target);
            if let Some(at) = self.entities.store.get(target).map(|e| e.base.position) {
                let rider = self.players.get(&player).map(|p| p.entity_id);
                self.game_event(GameEvent::Eat, at, GameEventContext::of(rider));
            }
        }
        fed
    }

    fn fed_food(
        &mut self,
        player: PlayerId,
        target: EntityId,
        hand: Hand,
        stack: &ItemStack,
    ) -> Interaction {
        if self.handle_eating(player, target, stack) {
            self.consume_held(player, hand, 1);
            Interaction::ServerOnly
        } else {
            Interaction::Pass
        }
    }

    fn mount_equip(&mut self, id: EntityId, slot: EquipmentSlot, stack: Option<ItemStack>) {
        let Some(living) = self
            .entities
            .store
            .get_mut(id)
            .and_then(|entity| entity.living_mut())
        else {
            return;
        };
        living.equipment[living::slot_index(slot)] = stack.clone();
        if stack.is_some()
            && let Some(mob) = living.mob.as_mut()
        {
            mob.drop_chances[living::slot_index(slot)] = GUARANTEED_DROP;
        }
        self.push(
            Audience::Trackers(id),
            ClientEvent::EntityEquipment {
                entity: id,
                slots: vec![(wire_slot(slot), stack)],
            },
        );
    }

    fn equip_one(
        &mut self,
        player: PlayerId,
        target: EntityId,
        hand: Hand,
        stack: &ItemStack,
        slot: EquipmentSlot,
        sound: &'static str,
    ) {
        let mut one = stack.clone();
        one.count = NonZeroU8::MIN;
        self.consume_held(player, hand, 1);
        self.mount_equip(target, slot, Some(one));
        self.entity_sound(target, sound, 1.0, 1.0);
        if let Some(at) = self.entities.store.get(target).map(|e| e.base.position) {
            let rider = self.players.get(&player).map(|p| p.entity_id);
            self.game_event(GameEvent::Equip, at, GameEventContext::of(rider));
        }
    }

    fn do_player_ride(&mut self, player: PlayerId, target: EntityId) {
        if let Some((base, mount)) = self
            .entities
            .store
            .get_mut(target)
            .and_then(|entity| equine_state(entity))
        {
            equine::set_eating(base, false);
            mount.clear_standing(base);
        }
        let Some((yaw, pitch)) = self
            .entities
            .store
            .get(target)
            .map(|entity| (entity.base.yaw, entity.base.pitch))
        else {
            return;
        };
        let Some(entity) = self.players.get_mut(&player).map(|p| {
            p.rotation.yaw = yaw;
            p.rotation.pitch = pitch;
            p.entity_id
        }) else {
            return;
        };
        self.start_riding(entity, target, false, true);
    }

    fn abstract_horse_interact(
        &mut self,
        player: PlayerId,
        target: EntityId,
        hand: Hand,
        held: Option<&ItemStack>,
    ) -> Interaction {
        let Some(entity) = self.entities.store.get(target) else {
            return Interaction::Pass;
        };
        let vehicle = self.entities.rides.is_vehicle(target);
        if vehicle || entity.is_baby() {
            return self.animal_fallback(player, target, hand, held);
        }
        let tamed = equine::is_tamed(&entity.base);
        let sneaking = self.players.get(&player).is_some_and(|p| p.sneaking);
        if tamed && sneaking {
            self.open_mount_inventory(player, target);
            return Interaction::Predicted;
        }
        if let Some(stack) = held {
            let entity = self.entities.store.get(target);
            let can_saddle = entity.is_some_and(|e| {
                stack.item == ItemKind::Saddle.id()
                    && is_equippable_in(e, stack, EquipmentSlot::Saddle)
                    && e.living()
                        .is_some_and(|l| l.equipment(EquipmentSlot::Saddle).is_none())
            });
            if can_saddle {
                self.equip_one(
                    player,
                    target,
                    hand,
                    stack,
                    EquipmentSlot::Saddle,
                    SADDLE_SOUND,
                );
                return Interaction::Predicted;
            }
            let armor = entity.is_some_and(|e| {
                is_equippable_in(e, stack, EquipmentSlot::Body)
                    && e.living()
                        .is_some_and(|l| l.equipment(EquipmentSlot::Body).is_none())
            });
            if armor {
                self.equip_one(
                    player,
                    target,
                    hand,
                    stack,
                    EquipmentSlot::Body,
                    ARMOR_SOUND,
                );
                return Interaction::Predicted;
            }
        }
        self.do_player_ride(player, target);
        Interaction::Predicted
    }

    fn animal_fallback(
        &mut self,
        player: PlayerId,
        target: EntityId,
        hand: Hand,
        held: Option<&ItemStack>,
    ) -> Interaction {
        match held {
            Some(stack) => self.animal_interact(player, target, hand, stack),
            None => Interaction::Pass,
        }
    }

    pub(crate) fn equine_interact(
        &mut self,
        player: PlayerId,
        target: EntityId,
        hand: Hand,
        held: Option<&ItemStack>,
    ) -> Option<Interaction> {
        let entity = self.entities.store.get(target)?;
        let kind = entity.base.kind;
        let tamed = equine::is_tamed(&entity.base);
        let baby = entity.is_baby();
        if kind == Kind::SkeletonHorse {
            if !tamed {
                return Some(Interaction::Pass);
            }
            return Some(self.abstract_horse_interact(player, target, hand, held));
        }
        let sneaking = self.players.get(&player).is_some_and(|p| p.sneaking);
        let adult_sneaking = !baby && tamed && sneaking;
        let vehicle = self.entities.rides.is_vehicle(target);
        if vehicle || adult_sneaking {
            return Some(self.abstract_horse_interact(player, target, hand, held));
        }
        if kind != Kind::ZombieHorse
            && baby
            && self
                .players
                .get(&player)
                .is_some_and(|p| self.holding(p, ItemKind::GoldenDandelion))
        {
            return Some(self.abstract_horse_interact(player, target, hand, held));
        }
        if let Some(stack) = held {
            let food = food_tag(kind).is_some_and(|tag| item_tag(stack.item, tag));
            if food {
                return Some(self.fed_food(player, target, hand, stack));
            }
            if !tamed {
                self.equine_make_mad(target);
                return Some(Interaction::Predicted);
            }
            if equine::is_chested(kind)
                && !self
                    .entities
                    .store
                    .get(target)
                    .is_some_and(|e| equine::has_chest(&e.base))
                && stack.item == ItemKind::Chest.id()
            {
                self.equip_chest(player, target, hand);
                return Some(Interaction::Predicted);
            }
        }
        Some(self.abstract_horse_interact(player, target, hand, held))
    }

    fn holding(&self, player: &crate::player::Player<L>, item: ItemKind) -> bool {
        [Hand::Main, Hand::Off].into_iter().any(|hand| {
            self.held(player.id, hand)
                .is_some_and(|s| s.item == item.id())
        })
    }

    fn equip_chest(&mut self, player: PlayerId, target: EntityId, hand: Hand) {
        if let Some(entity) = self.entities.store.get_mut(target) {
            equine::set_chest(&mut entity.base, true);
        }
        let mut random = self.mob_random(target);
        let pitch = (random.next_float() - random.next_float()) * 0.2 + 1.0;
        self.store_mob_random(target, random);
        let sound = if self.kind_of(target) == Some(Kind::Mule) {
            "minecraft:entity.mule.chest"
        } else {
            "minecraft:entity.donkey.chest"
        };
        self.entity_sound(target, sound, CHEST_EQUIP_SOUND_VOLUME, pitch);
        self.consume_held(player, hand, 1);
        self.recreate_mount_inventory(target);
    }

    pub(crate) fn recreate_mount_inventory(&mut self, id: EntityId) {
        if let Some(Entity {
            base,
            state: EntityState::Living(state),
            ..
        }) = self.entities.store.get_mut(id)
            && let Some(mount) = state.mob.as_mut().and_then(|mob| mob.mount.as_deref_mut())
        {
            mount.create_inventory(base);
        }
    }

    pub(crate) fn equine_position_offset(&self, id: EntityId) -> Vec3 {
        let Some(entity) = self.entities.store.get(id) else {
            return Vec3::ZERO;
        };
        let natural = entity.base.kind.dimensions().height;
        let scale = if natural > 0.0 {
            f64::from(entity.base.dimensions.height / natural)
        } else {
            1.0
        };
        mount_of(entity).map_or(Vec3::ZERO, |mount| {
            mount.passenger_offset(scale, entity.base.yaw)
        })
    }
}

impl<L: ClientLink> Server<L> {
    pub(crate) fn spring_skeleton_trap(&mut self, id: EntityId) {
        let Some(position) = self.entities.store.get(id).map(|e| e.base.position) else {
            return;
        };
        if let Some(Entity {
            base,
            state: EntityState::Living(state),
            ..
        }) = self.entities.store.get_mut(id)
            && let Some(mob) = state.mob.as_mut()
        {
            if let Some(mount) = mob.mount.as_deref_mut() {
                mount.trap = false;
            }
            equine::set_tamed(base, true);
            if let Some(age) = mob.ageable.as_mut() {
                ageable::set_age(base, age, 0);
            }
        }
        let Some(skeleton) = self.trap_skeleton(position) else {
            return;
        };
        self.spawn_entity(
            skeleton,
            Birth::of(CreatureSpawnReason::Trap).then(Spawned::Rides { vehicle: id }),
        );
        for _ in 0..3 {
            let mut random = self.mob_random(id);
            let push = Vec3::new(
                random.triangle(0.0, TRAP_PUSH_SPREAD),
                0.0,
                random.triangle(0.0, TRAP_PUSH_SPREAD),
            );
            self.store_mob_random(id, random);
            let Some(mut horse) = self.trap_horse(position) else {
                continue;
            };
            let Some(rider) = self.trap_skeleton(position) else {
                continue;
            };
            horse.base.velocity = horse.base.velocity + push;
            self.spawn_entity(
                horse,
                Birth::of(CreatureSpawnReason::Jockey).then(Spawned::Carries(Box::new(Carried {
                    rider,
                    reason: CreatureSpawnReason::Jockey,
                }))),
            );
        }
    }

    fn trap_horse(&mut self, position: Vec3) -> Option<Entity> {
        let mut horse = new_mob(self.new_entity_base(Kind::SkeletonHorse));
        horse.base.snap_to(position, 0.0, 0.0);
        self.finalize_part(&mut horse, SpawnReason::Triggered, None);
        horse.base.invulnerable_time = TRAP_INVULNERABLE_TICKS;
        equine::set_tamed(&mut horse.base, true);
        if let EntityState::Living(state) = &mut horse.state
            && let Some(mob) = state.mob.as_mut()
        {
            mob.persistence_required = true;
            if let Some(age) = mob.ageable.as_mut() {
                ageable::set_age(&mut horse.base, age, 0);
            }
        }
        Some(horse)
    }

    fn trap_skeleton(&mut self, position: Vec3) -> Option<Entity> {
        let mut skeleton = new_mob(self.new_entity_base(Kind::Skeleton));
        skeleton.base.snap_to(position, 0.0, 0.0);
        self.finalize_part(&mut skeleton, SpawnReason::Triggered, None);
        skeleton.base.invulnerable_time = TRAP_INVULNERABLE_TICKS;
        if let EntityState::Living(state) = &mut skeleton.state {
            if let Some(mob) = state.mob.as_mut() {
                mob.persistence_required = true;
            }
            if state.equipment(EquipmentSlot::Head).is_none() {
                rspigot_domain::entity::spawn::set_item_slot(
                    state,
                    EquipmentSlot::Head,
                    Some(ItemStack::new(ItemKind::IronHelmet.id(), NonZeroU8::MIN)),
                );
            }
        }
        Some(skeleton)
    }
}

impl<L: ClientLink> Server<L> {
    fn controlled_horse(&self, player: PlayerId) -> Option<EntityId> {
        let rider = self.players.get(&player)?.entity_id;
        let vehicle = self.entities.rides.vehicle_of(rider)?;
        let entity = self.entities.store.get(vehicle)?;
        (equine::is_equine(entity.base.kind) && self.controlling_player(vehicle) == Some(player))
            .then_some(vehicle)
    }

    pub(crate) fn start_riding_jump(&mut self, player: PlayerId, power: i32) {
        let Some(horse) = self.controlled_horse(player) else {
            return;
        };
        if power <= 0 {
            return;
        }
        if let Some((_, mount)) = self
            .entities
            .store
            .get_mut(horse)
            .and_then(|entity| equine_state(entity))
        {
            mount.player_jump_pending_scale = MountState::start_jump_scale(power);
        }
        let scale = MountState::start_jump_scale(power);
        let lane = self.lane_for(Some(player), horse);
        self.fire(lane, Continuation::HorseJump(horse), move |server| {
            let entity = server.entity_view(horse)?;
            Some(EventPayload::HorseJump(HorseJump {
                entity_type: entity.kind.clone(),
                entity,
                power: scale,
            }))
        });
    }

    pub(crate) fn finish_horse_jump(&mut self, horse: EntityId) {
        if let Some((_, mount)) = self
            .entities
            .store
            .get_mut(horse)
            .and_then(|entity| equine_state(entity))
        {
            mount.allow_stand_sliding = true;
        }
        self.equine_stand_if_possible(horse);
        self.play_jump_sound(horse);
    }

    fn play_jump_sound(&mut self, id: EntityId) {
        let Some(entity) = self.entities.store.get(id) else {
            return;
        };
        let sound = match entity.base.kind {
            Kind::Donkey => "minecraft:entity.donkey.jump",
            Kind::Mule => "minecraft:entity.mule.jump",
            Kind::SkeletonHorse if entity.base.fluid.in_water => {
                "minecraft:entity.skeleton_horse.jump_water"
            }
            _ => "minecraft:entity.horse.jump",
        };
        self.entity_sound(id, sound, JUMP_VOLUME, 1.0);
    }

    pub(crate) fn open_vehicle_inventory(&mut self, player: PlayerId) {
        let Some(rider) = self.players.get(&player).map(|p| p.entity_id) else {
            return;
        };
        let Some(vehicle) = self.entities.rides.vehicle_of(rider) else {
            return;
        };
        if self
            .kind_of(vehicle)
            .is_some_and(equine::is_equine)
        {
            self.open_mount_inventory(player, vehicle);
        }
    }
}

impl<L: ClientLink> Server<L> {
    pub(crate) fn equine_offspring(
        &mut self,
        parent: EntityId,
        partner: EntityId,
        random: &mut rspigot_domain::random::LegacyRandom,
    ) -> Option<Entity> {
        let parent_entity = self.entities.store.get(parent)?;
        let partner_entity = self.entities.store.get(partner)?;
        let kind = equine::offspring_kind(parent_entity.base.kind, partner_entity.base.kind)?;
        let own_variant = (equine::has_horse_variant(parent_entity.base.kind)).then(|| {
            (
                equine::variant_id(&parent_entity.base),
                equine::markings_id(&parent_entity.base),
            )
        });
        let other_variant = (equine::has_horse_variant(partner_entity.base.kind)).then(|| {
            (
                equine::variant_id(&partner_entity.base),
                equine::markings_id(&partner_entity.base),
            )
        });
        let mut own = parent_entity.living()?.attributes.clone();
        let mut other = partner_entity.living()?.attributes.clone();
        let mut child = new_mob(self.new_entity_base(kind));
        if kind == Kind::Horse
            && let (Some((own_variant, own_markings)), Some((other_variant, other_markings))) =
                (own_variant, other_variant)
        {
            let variant = equine::offspring_variant(own_variant, other_variant, random);
            let markings = equine::offspring_markings(own_markings, other_markings, random);
            equine::set_variant_and_markings(&mut child.base, variant, markings);
        }
        if let Entity {
            base,
            state: EntityState::Living(living),
            ..
        } = &mut child
        {
            equine::set_offspring_attributes(&mut own, &mut other, &mut living.attributes, random);
            equine::clamp_health(base, living);
        }
        Some(child)
    }
}

use std::num::NonZeroU8;

use rspigot_ai::{MobView, WolfAction};
use rspigot_domain::damage::DamageType;
use rspigot_domain::entity::living::{self, COOLDOWN_THRESHOLD};
use rspigot_domain::entity::reward::{damage_value, max_damage, set_damage_value};
use rspigot_domain::entity::species::wolf as wolf_state;
use rspigot_domain::entity::species::{self, Species};
use rspigot_domain::entity::tamable;
use rspigot_domain::entity::{Entity, EntityState};
use rspigot_domain::item::ItemStack;
use rspigot_domain::particle::{Particle, ParticleData};
use rspigot_domain::player::PlayerId;
use rspigot_domain::random::{LegacyRandom, RandomSource};
use rspigot_domain::{EntityId, Vec3};
use rspigot_entities::EntityKind as Kind;
use rspigot_items::{EquipmentSlot, ItemKind};

use crate::events::{ClientEvent, Hand};
use crate::health::DamageSource;
use crate::interact::{Interaction, dye_color};
use crate::mob_life::variant_sound;
use crate::plugins::events::{
    EntityRegainHealth, EntityRegainHealthPatch, EntityRegainHealthRegainReason,
};
use crate::plugins::steps::MobRegainStep;
use crate::plugins::{Continuation, EventPayload};
use crate::ports::ClientLink;
use crate::server::{Audience, Server};

const SHAKE_SOUND: &str = "minecraft:entity.wolf.shake";
const ARMOR_CRACK_SOUND: &str = "minecraft:item.wolf_armor.crack";
const ARMOR_REPAIR_SOUND: &str = "minecraft:item.wolf_armor.repair";
const ARMOR_EQUIP_SOUND: &str = "minecraft:item.armor.equip_wolf";
const ARMOR_UNEQUIP_SOUND: &str = "minecraft:item.armor.unequip_wolf";
const BYPASSES_WOLF_ARMOR: &str = "minecraft:bypasses_wolf_armor";
const WOLF_COLLAR_DYES: &str = "minecraft:wolf_collar_dyes";
const SHAKE_VOLUME: f32 = 0.4;
const ARMOR_REPAIR_UNIT: f32 = 0.125;
const WHINE_HEALTH: f32 = 20.0;
const TAME_ROLL: i32 = 3;
const AMBIENT_ROLL: i32 = 3;
const GUARANTEED_DROP: f32 = 2.0;
const FOOD_HEAL_FACTOR: f32 = 2.0;
const FOOD_DEFAULT_HEAL: f32 = 2.0;
const CRACK_LOW: f32 = 0.95;
const CRACK_MEDIUM: f32 = 0.69;
const CRACK_HIGH: f32 = 0.32;
const CRACK_PARTICLES: i32 = 20;

fn crack_level(damage: i32, max: i32) -> u8 {
    let fraction = (max - damage) as f32 / max as f32;
    if fraction < CRACK_HIGH {
        3
    } else if fraction < CRACK_MEDIUM {
        2
    } else if fraction < CRACK_LOW {
        1
    } else {
        0
    }
}

fn is_wolf_armor(stack: Option<&ItemStack>) -> bool {
    stack.is_some_and(|stack| stack.item == ItemKind::WolfArmor.id())
}

fn item_in_tag(item: rspigot_domain::ItemId, tag: &str) -> bool {
    rspigot_data::tags::tag("minecraft:item", tag)
        .is_some_and(|ids| ids.binary_search(&u32::from(item.0)).is_ok())
}

struct WolfFacts {
    tame: bool,
    owner: Option<uuid::Uuid>,
    baby: bool,
    health: f32,
    max_health: f32,
    sitting: bool,
    armor: Option<ItemStack>,
    collar: u8,
}

impl<L: ClientLink> Server<L> {
    pub(crate) fn wolf_view(&self, _entity: &Entity, _view: &mut MobView) {}

    pub(crate) fn wolf_pre_tick(&mut self, _id: EntityId) -> bool {
        true
    }

    fn wolf_facts(&self, id: EntityId) -> Option<WolfFacts> {
        let entity = self.entities.store.get(id)?;
        let state = entity.living()?;
        let base = &entity.base;
        let tame = tamable::is_tame(base);
        let mut attributes = state.attributes.clone();
        Some(WolfFacts {
            tame,
            owner: tame.then(|| tamable::owner(base)).flatten(),
            baby: entity.is_baby(),
            health: living::health(base),
            max_health: attributes.value(living::MAX_HEALTH, &state.effects) as f32,
            sitting: tamable::is_in_sitting_pose(base),
            armor: state.equipment(EquipmentSlot::Body).cloned(),
            collar: wolf_state::collar_color(base),
        })
    }

    fn wolf_angry(&self, id: EntityId) -> bool {
        let game_time = self.world.level.game_time;
        self.entities
            .store
            .get(id)
            .and_then(Entity::living)
            .and_then(|state| state.mob.as_ref())
            .and_then(|mob| mob.neutral)
            .is_some_and(|neutral| neutral.is_angry(game_time))
    }

    fn sync_body_armor(&mut self, id: EntityId) {
        let Some(state) = self
            .entities
            .store
            .get_mut(id)
            .and_then(|entity| entity.living_mut())
        else {
            return;
        };
        let worn = is_wolf_armor(state.equipment(EquipmentSlot::Body));
        let present = state
            .attributes
            .has_modifier(living::ARMOR, wolf_state::BODY_ARMOR_MODIFIER);
        if worn == present {
            return;
        }
        let item = ItemKind::WolfArmor.id();
        for (attribute, modifier) in
            rspigot_domain::attribute::equipment_modifiers(EquipmentSlot::Body, item)
        {
            if worn {
                state.attributes.add_modifier(attribute, modifier);
            } else {
                state.attributes.remove_modifier(attribute, &modifier.id);
            }
        }
    }

    pub(crate) fn wolf_post_tick(&mut self, id: EntityId) {
        self.sync_body_armor(id);
        let pathfinding = self
            .entities
            .mobs
            .entries
            .get(&id)
            .is_some_and(|entry| !entry.ai.brain().navigation.is_done());
        let raining = self
            .entities
            .store
            .get(id)
            .is_some_and(|entity| self.is_in_rain(&entity.base));
        let mut events = Vec::new();
        let mut shake = None;
        let mut alive = false;
        if let Some(Entity {
            base,
            state: EntityState::Living(state),
            ..
        }) = self.entities.store.get_mut(id)
        {
            alive = !living::is_dead_or_dying(base, state) && !base.is_removed();
            let wet_water = base.fluid.in_water || raining;
            let on_ground = base.on_ground;
            if let Some(Species::Wolf(wolf)) = state.mob.as_mut().map(|mob| &mut mob.species) {
                if wolf.wet && !wolf.shaking && !pathfinding && on_ground {
                    wolf.start_shaking();
                    events.push(wolf_state::SHAKE_START_EVENT);
                }
                if alive {
                    if wet_water {
                        wolf.wet = true;
                        if wolf.shaking {
                            events.push(wolf_state::SHAKE_CANCEL_EVENT);
                            wolf.cancel_shake();
                        }
                    } else if wolf.shaking {
                        shake = Some(wolf.advance_shake());
                    }
                }
            }
        }
        if alive && !self.entities.store.get(id).is_none() {
            self.update_persistent_anger(id, true);
        }
        for event in events {
            self.mob_entity_event(id, event);
        }
        if let Some(step) = shake {
            let mut random = self.mob_random(id);
            if step.first {
                let pitch = (random.next_float() - random.next_float()) * 0.2 + 1.0;
                self.entity_sound(id, SHAKE_SOUND, SHAKE_VOLUME, pitch);
            }
            for _ in 0..step.splashes {
                random.next_float();
                random.next_float();
            }
            self.store_mob_random(id, random);
        }
    }

    pub(crate) fn wolf_custom_ai_step(&mut self, _id: EntityId) {}

    pub(crate) fn wolf_hurt_amount(
        &mut self,
        _id: EntityId,
        _source: &DamageSource,
        amount: f32,
    ) -> Option<f32> {
        Some(amount)
    }

    pub(crate) fn wolf_absorbs_hurt(
        &mut self,
        id: EntityId,
        source: &DamageSource,
        amount: f32,
    ) -> bool {
        let Some(entity) = self.entities.store.get(id) else {
            return false;
        };
        let Some(state) = entity.living() else {
            return false;
        };
        let Some(armor) = state.equipment(EquipmentSlot::Body).cloned() else {
            return false;
        };
        if !is_wolf_armor(Some(&armor)) || source.kind.in_tag(BYPASSES_WOLF_ARMOR) {
            return false;
        }
        let cooling = state.damage.cooldown as f32 > COOLDOWN_THRESHOLD
            && !source
                .kind
                .is(rspigot_domain::damage::DamageTag::BypassesCooldown);
        if cooling && amount <= state.last_hurt {
            return false;
        }
        let kind: DamageType = source.kind;
        let final_damage = living::final_damage(&mut state.clone(), kind, amount);
        let position = entity.base.position;
        let before = (damage_value(&armor), max_damage(&armor).unwrap_or_default());
        self.damage_mob_equipment(id, EquipmentSlot::Body, final_damage.ceil() as i32);
        let after = self
            .entities
            .store
            .get(id)
            .and_then(Entity::living)
            .and_then(|state| state.equipment(EquipmentSlot::Body))
            .map_or(0, |stack| {
                crack_level(damage_value(stack), max_damage(stack).unwrap_or(1))
            });
        if crack_level(before.0, before.1.max(1)) != after {
            self.entity_sound(id, ARMOR_CRACK_SOUND, 1.0, 1.0);
            self.push(
                Audience::Trackers(id),
                ClientEvent::Particle(Particle {
                    kind: "minecraft:item",
                    data: ParticleData::Item(ItemKind::ArmadilloScute.id()),
                    position: Vec3::new(position.x, position.y + 1.0, position.z),
                    count: CRACK_PARTICLES,
                    spread: Vec3::new(0.2, 0.1, 0.2),
                    speed: 0.1,
                }),
            );
        }
        true
    }

    pub(crate) fn wolf_after_hurt(&mut self, id: EntityId, _source: &DamageSource, dead: bool) {
        if dead
            && let Some(Species::Wolf(wolf)) = self
                .entities
                .store
                .get_mut(id)
                .and_then(|entity| entity.living_mut())
                .and_then(|state| state.mob.as_mut())
                .map(|mob| &mut mob.species)
        {
            wolf.die();
        }
        self.set_ordered_to_sit(id, false);
    }

    pub(crate) fn wolf_action(&mut self, id: EntityId, action: WolfAction) {
        match action {
            WolfAction::SetInterested(interested) => {
                if let Some(entity) = self.entities.store.get_mut(id) {
                    wolf_state::set_interested(&mut entity.base, interested);
                }
            }
        }
    }

    pub(crate) fn wolf_tamed(&mut self, id: EntityId) {
        if let Some(Entity {
            base,
            state: EntityState::Living(state),
            ..
        }) = self.entities.store.get_mut(id)
        {
            wolf_state::apply_taming_side_effects(base, state);
        }
        if let Some(entry) = self.entities.mobs.entries.get_mut(&id) {
            entry.ai.brain_mut().navigation.stop();
        }
        self.forget_target(id, rspigot_ai::TargetReason::Unknown);
    }

    pub(crate) fn wolf_offspring(
        &mut self,
        child: &mut Entity,
        parent: EntityId,
        partner: EntityId,
        random: &mut LegacyRandom,
    ) {
        let read = |id: EntityId| {
            self.entities.store.get(id).map(|entity| {
                (
                    entity.base.kind,
                    species::variant(&entity.base, wolf_state::WOLF_FAMILY),
                    tamable::is_tame(&entity.base),
                    tamable::owner(&entity.base),
                    wolf_state::collar_color(&entity.base),
                )
            })
        };
        let (Some(mine), Some(theirs)) = (read(parent), read(partner)) else {
            return;
        };
        if theirs.0 != Kind::Wolf {
            return;
        }
        let variant = if random.next_boolean() {
            mine.1
        } else {
            theirs.1
        };
        species::set_variant(&mut child.base, wolf_state::WOLF_FAMILY, variant);
        if mine.2 {
            tamable::set_owner(&mut child.base, mine.3);
            tamable::set_tame(&mut child.base, true);
            if let EntityState::Living(state) = &mut child.state {
                wolf_state::apply_taming_side_effects(&mut child.base, state);
            }
            let mut level = crate::mobs::ServerRandom(&mut self.rng);
            let color = species::mixed_wool_color(mine.4, theirs.4, &mut level);
            wolf_state::set_collar_color(&mut child.base, color);
        }
        let sound = species::pick_sound_variant(wolf_state::WOLF_FAMILY, random);
        species::set_sound_variant(&mut child.base, wolf_state::WOLF_FAMILY, sound);
    }

    pub(crate) fn wolf_ambient_voice(&mut self, id: EntityId) {
        let Some(facts) = self.wolf_facts(id) else {
            return;
        };
        let angry = self.wolf_angry(id);
        let pick: fn(&Entity) -> Option<&'static str> = if angry {
            |entity| variant_sound(entity, |set| set.growl).flatten()
        } else {
            let mut random = self.mob_random(id);
            let roll = random.next_int(AMBIENT_ROLL);
            self.store_mob_random(id, random);
            if roll != 0 {
                |entity| variant_sound(entity, |set| set.ambient).flatten()
            } else if facts.tame && facts.health < WHINE_HEALTH {
                |entity| variant_sound(entity, |set| set.whine).flatten()
            } else {
                |entity| variant_sound(entity, |set| set.pant).flatten()
            }
        };
        self.play_mob_voice(id, pick);
    }

    pub(crate) fn heal_mob(
        &mut self,
        player: PlayerId,
        target: EntityId,
        amount: f32,
        reason: EntityRegainHealthRegainReason,
    ) {
        let alive = self
            .entities
            .store
            .get(target)
            .is_some_and(|entity| living::health(&entity.base) > 0.0);
        if !alive {
            return;
        }
        let lane = self.lane_for(Some(player), target);
        let step = MobRegainStep { target, amount };
        self.fire(lane, Continuation::MobRegain(step), move |server| {
            let entity = server.entity_view(target)?;
            Some(EventPayload::EntityRegainHealth(EntityRegainHealth {
                amount: f64::from(amount),
                entity_type: entity.kind.clone(),
                entity,
                regain_reason: reason,
            }))
        });
    }

    pub(crate) fn finish_mob_regain(
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

    fn use_player_item(&mut self, player: PlayerId, hand: Hand, stack: &ItemStack) {
        let count = stack.count.get();
        self.consume_held(player, hand, 1);
        let Some(remainder) = stack.item.kind().and_then(ItemKind::use_remainder) else {
            return;
        };
        if self.infinite_materials(player) {
            return;
        }
        let item = ItemStack::new(remainder, NonZeroU8::MIN);
        if count == 1 {
            self.set_held(player, hand, Some(item));
            return;
        }
        let added = self.players.get_mut(&player).is_some_and(|p| {
            p.inventory
                .insert(item.item, 1, item.item.max_stack_size())
                .remaining
                == 0
        });
        if !added && let Some(position) = self.players.get(&player).map(|p| p.eye_position()) {
            self.spawn_at_location(position, -0.3, item);
        }
    }

    fn set_body_armor(&mut self, id: EntityId, armor: Option<ItemStack>) {
        let Some(living) = self
            .entities
            .store
            .get_mut(id)
            .and_then(|entity| entity.living_mut())
        else {
            return;
        };
        living.equipment[living::slot_index(EquipmentSlot::Body)] = armor.clone();
        if armor.is_some()
            && let Some(mob) = living.mob.as_mut()
        {
            mob.drop_chances[living::slot_index(EquipmentSlot::Body)] = GUARANTEED_DROP;
        }
        self.push(
            Audience::Trackers(id),
            ClientEvent::EntityEquipment {
                entity: id,
                slots: vec![(crate::entities::wire_slot(EquipmentSlot::Body), armor)],
            },
        );
    }

    fn shear_wolf_armor(&mut self, player: PlayerId, target: EntityId, hand: Hand) {
        let Some(armor) = self
            .entities
            .store
            .get(target)
            .and_then(Entity::living)
            .and_then(|state| state.equipment(EquipmentSlot::Body))
            .cloned()
        else {
            return;
        };
        let height = self
            .entities
            .store
            .get(target)
            .map_or(0.0, |entity| f64::from(entity.base.dimensions.height));
        self.hurt_held(player, hand, 1);
        self.set_body_armor(target, None);
        self.drop_at_location(target, height, armor, Vec3::ZERO);
        self.entity_sound(target, ARMOR_UNEQUIP_SOUND, 1.0, 1.0);
    }

    fn try_to_tame(&mut self, player: PlayerId, target: EntityId) {
        let mut random = self.mob_random(target);
        let roll = random.next_int(TAME_ROLL);
        self.store_mob_random(target, random);
        if roll == 0 {
            self.ask_tame(player, target);
        } else {
            self.mob_entity_event(target, crate::tamable::TAME_FAILURE_EVENT);
        }
    }

    fn toggle_sit(&mut self, target: EntityId, ordered: bool) {
        self.set_ordered_to_sit(target, !ordered);
        if let Some(state) = self
            .entities
            .store
            .get_mut(target)
            .and_then(|entity| entity.living_mut())
        {
            state.jumping = false;
        }
        if let Some(entry) = self.entities.mobs.entries.get_mut(&target) {
            entry.ai.brain_mut().navigation.stop();
        }
        self.forget_target(target, rspigot_ai::TargetReason::ForgotTarget);
    }

    pub(crate) fn wolf_interact(
        &mut self,
        player: PlayerId,
        target: EntityId,
        hand: Hand,
        held: Option<&ItemStack>,
    ) -> Option<Interaction> {
        let facts = self.wolf_facts(target)?;
        let uuid = self.players.get(&player).map(|p| p.profile.id.0);
        let sneaking = self.players.get(&player).is_some_and(|p| p.sneaking);
        let owned = facts.tame && facts.owner.is_some() && facts.owner == uuid;
        let item = held.map(|stack| stack.item);
        let wearing = is_wolf_armor(facts.armor.as_ref());
        if item == Some(ItemKind::Shears.id()) && owned && !sneaking && wearing {
            self.shear_wolf_armor(player, target, hand);
            return Some(Interaction::Predicted);
        }
        let is_food = item.is_some_and(|item| item_in_tag(item, "minecraft:wolf_food"));
        if facts.tame {
            if is_food && facts.health < facts.max_health {
                let stack = held?.clone();
                let nutrition = stack
                    .item
                    .kind()
                    .and_then(ItemKind::food)
                    .map(|food| food.nutrition as f32);
                self.use_player_item(player, hand, &stack);
                let heal = nutrition.map_or(FOOD_DEFAULT_HEAL, |value| FOOD_HEAL_FACTOR * value);
                self.heal_mob(player, target, heal, EntityRegainHealthRegainReason::Eating);
                return Some(Interaction::Predicted);
            }
            let collar_dye = item.is_some_and(|item| item_in_tag(item, WOLF_COLLAR_DYES));
            if !(collar_dye && owned) {
                if item == Some(ItemKind::WolfArmor.id()) && !wearing && owned && !facts.baby {
                    let stack = held?.clone();
                    let mut single = stack.clone();
                    single.count = NonZeroU8::MIN;
                    let mut random = self.mob_random(target);
                    random.next_long();
                    self.store_mob_random(target, random);
                    self.set_body_armor(target, Some(single));
                    self.entity_sound(target, ARMOR_EQUIP_SOUND, 1.0, 1.0);
                    self.consume_held(player, hand, 1);
                    return Some(Interaction::Predicted);
                }
                if facts.sitting
                    && wearing
                    && owned
                    && let Some(armor) = &facts.armor
                    && damage_value(armor) > 0
                    && item.is_some_and(|item| {
                        ItemKind::WolfArmor
                            .repairable()
                            .is_some_and(|set| set.contains(item))
                    })
                {
                    self.shrink_held(player, hand, 1);
                    self.entity_sound(target, ARMOR_REPAIR_SOUND, 1.0, 1.0);
                    let mut repaired = armor.clone();
                    let unit =
                        (max_damage(armor).unwrap_or_default() as f32 * ARMOR_REPAIR_UNIT) as i32;
                    set_damage_value(&mut repaired, (damage_value(armor) - unit).max(0));
                    self.set_body_armor(target, Some(repaired));
                    return Some(Interaction::Predicted);
                }
                let result = match held {
                    Some(stack) => self.animal_interact(player, target, hand, stack),
                    None => Interaction::Pass,
                };
                if result != Interaction::Pass {
                    return Some(result);
                }
                if owned {
                    let ordered = self
                        .entities
                        .store
                        .get(target)
                        .and_then(Entity::living)
                        .and_then(|state| state.mob.as_ref())
                        .and_then(|mob| mob.tamable)
                        .is_some_and(|state| state.ordered_to_sit);
                    self.toggle_sit(target, ordered);
                    return Some(Interaction::Predicted);
                }
                return Some(result);
            }
            if let Some(color) = item.and_then(dye_color)
                && color != facts.collar
            {
                if let Some(entity) = self.entities.store.get_mut(target) {
                    wolf_state::set_collar_color(&mut entity.base, color);
                }
                self.consume_held(player, hand, 1);
                return Some(Interaction::Predicted);
            }
        } else if item == Some(ItemKind::Bone.id()) && !self.wolf_angry(target) {
            self.consume_held(player, hand, 1);
            self.try_to_tame(player, target);
            return Some(Interaction::ServerOnly);
        }
        let result = match held {
            Some(stack) => self.animal_interact(player, target, hand, stack),
            None => Interaction::Pass,
        };
        Some(result)
    }
}

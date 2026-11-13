use std::num::NonZeroU8;

use rspigot_ai::{IronGolemAction, MobView, TargetReason};
use rspigot_domain::EntityId;
use rspigot_domain::Vec3;
use rspigot_domain::attribute::{ATTACK_DAMAGE, KNOCKBACK_RESISTANCE};
use rspigot_domain::damage::DamageType;
use rspigot_domain::entity::living::{self, slot_index};
use rspigot_domain::entity::species::Species;
use rspigot_domain::entity::species::iron_golem::{
    self, ATTACK_ANIMATION_TICKS, Crackiness, EVENT_ATTACK, EVENT_OFFER_FLOWER,
    EVENT_STOP_OFFER_FLOWER, IRON_INGOT_HEAL_AMOUNT, IronGolemState, OFFER_FLOWER_TICKS,
};
use rspigot_domain::entity::{Entity, EntityState};
use rspigot_domain::item::ItemStack;
use rspigot_domain::player::PlayerId;
use rspigot_domain::random::RandomSource;
use rspigot_entities::EntityKind as Kind;
use rspigot_items::{EquipmentSlot, ItemKind};

use crate::commands::Target;
use crate::events::{ClientEvent, EquipmentSlot as WireSlot, Hand};
use crate::health::{DamageSource, attributes};
use crate::interact::Interaction;
use crate::plugins::events::EntityRegainHealthRegainReason;
use crate::plugins::steps::{AfterHurt, GolemRepair, MobAttackFollow, Struck};
use crate::ports::ClientLink;
use crate::server::{Audience, Server};

const ATTACK_SOUND: &str = "minecraft:entity.iron_golem.attack";
const DAMAGE_SOUND: &str = "minecraft:entity.iron_golem.damage";
const REPAIR_SOUND: &str = "minecraft:entity.iron_golem.repair";
const COLLISION_TARGET_ROLL: i32 = 20;
const REPAIR_PITCH_SPREAD: f32 = 0.2;
const TOSS_VELOCITY: f32 = 0.4;
const GUARANTEED_DROP: f32 = 2.0;

fn golem_state(entity: &mut Entity) -> Option<&mut IronGolemState> {
    match entity
        .living_mut()
        .and_then(|state| state.mob.as_mut())
        .map(|mob| &mut mob.species)
    {
        Some(Species::IronGolem(state)) => Some(state),
        _ => None,
    }
}

impl<L: ClientLink> Server<L> {
    fn with_golem_state(&mut self, id: EntityId, update: impl FnOnce(&mut IronGolemState)) {
        if let Some(state) = self.entities.store.get_mut(id).and_then(golem_state) {
            update(state);
        }
    }

    pub(crate) fn iron_golem_view(&self, entity: &Entity, view: &mut MobView) {
        view.player_created = iron_golem::is_player_created(&entity.base);
    }

    pub(crate) fn iron_golem_pre_tick(&mut self, _id: EntityId) -> bool {
        true
    }

    pub(crate) fn iron_golem_post_tick(&mut self, id: EntityId) {
        self.iron_golem_push_entities(id);
        self.with_golem_state(id, |state| {
            if state.attack_animation_tick > 0 {
                state.attack_animation_tick -= 1;
            }
            if state.offer_flower_tick > 0 {
                state.offer_flower_tick -= 1;
            }
        });
        self.update_persistent_anger(id, true);
    }

    fn iron_golem_push_entities(&mut self, id: EntityId) {
        let Some(golem) = self.entities.store.get(id) else {
            return;
        };
        let bounds = golem.base.bounding_box();
        let mut touched: Vec<EntityId> = self
            .entities
            .store
            .iter()
            .filter(|other| other.base.id != id)
            .filter(|other| {
                other.base.kind != Kind::Creeper
                    && rspigot_ai::is_enemy(other.base.kind)
                    && other.living().is_some_and(|state| {
                        !living::is_dead_or_dying(&other.base, state) && !other.base.is_removed()
                    })
                    && other.base.bounding_box().intersects(&bounds)
            })
            .map(|other| other.base.id)
            .collect();
        touched.sort_unstable();
        for enemy in touched {
            let mut random = self.mob_random(id);
            let hit = random.next_int(COLLISION_TARGET_ROLL) == 0;
            self.store_mob_random(id, random);
            if hit && let Some(view) = self.entity_view(id) {
                self.request_target(view, enemy, TargetReason::Collision);
            }
        }
    }

    pub(crate) fn iron_golem_custom_ai_step(&mut self, _id: EntityId) {}

    pub(crate) fn iron_golem_interact(
        &mut self,
        player: PlayerId,
        target: EntityId,
        hand: Hand,
        held: Option<&ItemStack>,
    ) -> Option<Interaction> {
        if held?.item != ItemKind::IronIngot.id() {
            return None;
        }
        let entity = self.entities.store.get_mut(target)?;
        let rspigot_domain::entity::Entity {
            base,
            state: EntityState::Living(state),
            ..
        } = entity
        else {
            return None;
        };
        let before = living::health(base);
        let full = before >= state.max_health() as f32;
        self.heal_mob_then(
            Some(player),
            target,
            IRON_INGOT_HEAL_AMOUNT,
            EntityRegainHealthRegainReason::Custom,
            Some(GolemRepair { player, hand }),
        );
        if full || before <= 0.0 {
            return None;
        }
        Some(Interaction::Predicted)
    }

    pub(crate) fn finish_golem_repair(&mut self, target: EntityId, repair: GolemRepair) {
        let mut random = self.mob_random(target);
        let pitch = 1.0 + (random.next_float() - random.next_float()) * REPAIR_PITCH_SPREAD;
        self.store_mob_random(target, random);
        self.entity_sound(target, REPAIR_SOUND, 1.0, pitch);
        self.consume_held(repair.player, repair.hand, 1);
    }

    fn golem_crackiness(&mut self, id: EntityId) -> Option<Crackiness> {
        let entity = self.entities.store.get_mut(id)?;
        let health = living::health(&entity.base);
        let max = entity.living_mut()?.max_health() as f32;
        Some(Crackiness::by_fraction(health / max))
    }

    pub(crate) fn iron_golem_hurt_amount(
        &mut self,
        id: EntityId,
        _source: &DamageSource,
        amount: f32,
    ) -> Option<f32> {
        let before = self.golem_crackiness(id);
        self.with_golem_state(id, |state| state.crack_before = before);
        Some(amount)
    }

    pub(crate) fn iron_golem_after_hurt(
        &mut self,
        id: EntityId,
        _source: &DamageSource,
        _dead: bool,
    ) {
        let mut before = None;
        self.with_golem_state(id, |state| before = state.crack_before.take());
        if before.is_some() && before != self.golem_crackiness(id) {
            self.entity_sound(id, DAMAGE_SOUND, 1.0, 1.0);
        }
    }

    pub(crate) fn iron_golem_action(&mut self, id: EntityId, action: IronGolemAction) {
        match action {
            IronGolemAction::OfferFlower(offer) => {
                let (ticks, event) = if offer {
                    (OFFER_FLOWER_TICKS, EVENT_OFFER_FLOWER)
                } else {
                    (0, EVENT_STOP_OFFER_FLOWER)
                };
                self.with_golem_state(id, |state| state.offer_flower_tick = ticks);
                self.mob_entity_event(id, event);
            }
            IronGolemAction::GiftPoppy(receiver) => self.gift_poppy(receiver),
        }
    }

    fn gift_poppy(&mut self, receiver: EntityId) {
        let Some(living) = self
            .entities
            .store
            .get_mut(receiver)
            .and_then(Entity::living_mut)
        else {
            return;
        };
        if living.equipment(EquipmentSlot::Saddle).is_some() {
            return;
        }
        let poppy = ItemStack::new(ItemKind::Poppy.id(), NonZeroU8::MIN);
        living.equipment[slot_index(EquipmentSlot::Saddle)] = Some(poppy.clone());
        if let Some(mob) = living.mob.as_mut() {
            mob.drop_chances[slot_index(EquipmentSlot::Saddle)] = GUARANTEED_DROP;
        }
        self.push(
            Audience::Trackers(receiver),
            ClientEvent::EntityEquipment {
                entity: receiver,
                slots: vec![(WireSlot::Saddle, Some(poppy))],
            },
        );
    }

    pub(crate) fn iron_golem_hurt_target(&mut self, attacker: EntityId, target: Target) -> bool {
        let Some(source_entity) = self.mob_source(attacker) else {
            return false;
        };
        self.with_golem_state(attacker, |state| {
            state.attack_animation_tick = ATTACK_ANIMATION_TICKS;
        });
        self.mob_entity_event(attacker, EVENT_ATTACK);
        let Some(attack) = self
            .entities
            .store
            .get_mut(attacker)
            .and_then(Entity::living_mut)
            .map(|state| state.attribute(ATTACK_DAMAGE) as f32)
        else {
            return false;
        };
        let mut random = self.mob_random(attacker);
        let whole = attack as i32;
        let damage = if whole > 0 {
            attack / 2.0 + random.next_int(whole) as f32
        } else {
            attack
        };
        self.store_mob_random(attacker, random);
        let source = DamageSource::by(DamageType::MobAttack, source_entity);
        let victim = match target {
            Target::Player(player) => Struck::Player(player),
            Target::Entity(entity) => Struck::Mob(entity),
        };
        let follow = MobAttackFollow {
            attacker,
            target,
            knockback: 0.0,
            yaw: 0.0,
            golem: true,
            hoglin: false,
            bee: false,
            puffer: None,
        };
        self.strike(None, victim, &source, damage, AfterHurt::MobAttack(follow))
    }

    pub(crate) fn iron_golem_finish_attack(
        &mut self,
        attacker: EntityId,
        target: Target,
        hurt: bool,
    ) {
        if hurt {
            let resistance = match target {
                Target::Player(player) => self
                    .players
                    .get(&player)
                    .map_or(0.0, |player| attributes(player).get(KNOCKBACK_RESISTANCE)),
                Target::Entity(entity) => self
                    .entities
                    .store
                    .get_mut(entity)
                    .and_then(Entity::living_mut)
                    .map_or(0.0, |state| state.attribute(KNOCKBACK_RESISTANCE)),
            };
            let toss = Vec3::new(
                0.0,
                f64::from(TOSS_VELOCITY) * (1.0 - resistance).max(0.0),
                0.0,
            );
            match target {
                Target::Player(player) => {
                    if let Some(player) = self.players.get_mut(&player) {
                        player.vitals.velocity = player.vitals.velocity + toss;
                        player.vitals.sync_velocity = true;
                    }
                }
                Target::Entity(entity) => {
                    if let Some(entity) = self.entities.store.get_mut(entity) {
                        entity.base.velocity = entity.base.velocity + toss;
                        entity.base.needs_sync = true;
                    }
                }
            }
        }
        self.entity_sound(attacker, ATTACK_SOUND, 1.0, 1.0);
    }
}

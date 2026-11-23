use std::num::NonZeroU8;

use rand::Rng;
use rspigot_domain::damage::DamageType;
use rspigot_domain::enchant;
use rspigot_domain::entity::arrow::{self, ArrowState, Launch, Pickup};
use rspigot_domain::entity::reward::{DAMAGE_COMPONENT, max_damage};
use rspigot_domain::entity::{Aabb, Entity, EntityState, RemovalReason, lightning};
use rspigot_domain::item::ItemStack;
use rspigot_domain::player::{GameMode, PlayerId};
use rspigot_domain::{BlockPos, EntityId, Vec3};
use rspigot_entities::{EntityKind as Kind, SoundSource};
use rspigot_items::ItemKind;

use crate::advancements::AdvancementEvent;
use crate::bow::{UsingItem, hand_slot};
use crate::enchant_effects;
use crate::events::Hand;
use crate::health::DamageSource;
use crate::plugins::events::{CreatureSpawnReason, EntityRemoveCause, PlayerRiptide};
use crate::plugins::steps::{Birth, Spawned};
use crate::plugins::{Continuation, EventKind, EventPayload, spec_of};
use crate::ports::ClientLink;
use crate::projectiles::EYE_OFFSET;
use crate::server::Server;

const MAX_LOYALTY: f32 = 127.0;
const SPIN_TICKS: i32 = 20;
const SPIN_DAMAGE: f32 = 8.0;
const SPIN_RECOIL: f64 = -0.2;
const RETURN_SOUND_VOLUME: f32 = 10.0;
const DROP_LIFT: f64 = 0.1;

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct SpinAttack {
    pub(crate) ticks: i32,
    pub(crate) damage: f32,
}

fn spin_strength(stack: &ItemStack) -> f32 {
    enchant::modify_trident_spin_attack_strength(stack, 0.0)
}

fn damage_of(stack: &ItemStack) -> i32 {
    stack.components.get_i32(DAMAGE_COMPONENT).unwrap_or(0)
}

fn next_damage_breaks(stack: &ItemStack) -> bool {
    max_damage(stack).is_some_and(|max| damage_of(stack) + 1 >= max)
}

fn damaged_without_breaking(stack: &ItemStack, amount: i32) -> ItemStack {
    let mut damaged = stack.clone();
    if let Some(max) = max_damage(stack) {
        let next = (damage_of(stack) + amount).min(max - 1);
        damaged.components.insert(DAMAGE_COMPONENT, next);
    }
    damaged
}

fn single(stack: &ItemStack) -> ItemStack {
    ItemStack {
        count: NonZeroU8::MIN,
        ..stack.clone()
    }
}

impl<L: ClientLink> Server<L> {
    pub(crate) fn starts_trident(&self, id: PlayerId, hand: Hand) -> bool {
        let Some(player) = self.players.get(&id) else {
            return false;
        };
        let Some(stack) = player
            .inventory
            .get(hand_slot(player, hand))
            .filter(|stack| stack.item == ItemKind::Trident.id())
        else {
            return false;
        };
        !next_damage_breaks(stack) && (spin_strength(stack) <= 0.0 || self.in_water_or_rain(id))
    }

    pub(crate) fn begin_trident(&mut self, id: PlayerId, hand: Hand) {
        let tick = self.current_tick();
        if let Some(player) = self.players.get_mut(&id) {
            player.using_item = Some(UsingItem { hand, since: tick });
        }
    }

    fn in_water_or_rain(&self, id: PlayerId) -> bool {
        self.players.get(&id).is_some_and(|player| {
            self.in_water(player.position) || self.raining_at(player.position.block())
        })
    }

    pub(crate) fn release_trident(&mut self, id: PlayerId) -> bool {
        let tick = self.current_tick();
        let Some(player) = self.players.get(&id) else {
            return false;
        };
        let Some(using) = player.using_item else {
            return false;
        };
        let Some(stack) = player
            .inventory
            .get(hand_slot(player, using.hand))
            .filter(|stack| stack.item == ItemKind::Trident.id())
            .cloned()
        else {
            return false;
        };
        if let Some(player) = self.players.get_mut(&id) {
            player.using_item = None;
        }
        let alive = self
            .players
            .get(&id)
            .is_some_and(|p| !p.vitals.is_dead() && p.game_mode != GameMode::Spectator);
        if !alive || tick.saturating_sub(using.since) < arrow::TRIDENT_MIN_CHARGE {
            return true;
        }
        let strength = spin_strength(&stack);
        let passenger = self
            .players
            .get(&id)
            .is_some_and(|p| self.entities.rides.is_passenger(p.entity_id));
        if strength > 0.0 && (!self.in_water_or_rain(id) || passenger) {
            return true;
        }
        if next_damage_breaks(&stack) {
            return true;
        }
        if strength > 0.0 {
            self.riptide(id, using.hand, stack, strength);
        } else {
            self.throw_trident(id, using.hand, stack);
        }
        true
    }

    fn throw_trident(&mut self, id: PlayerId, hand: Hand, stack: ItemStack) {
        let Some(player) = self.players.get(&id) else {
            return;
        };
        let (eye, rotation, movement, on_ground, owner, creative) = (
            player.eye_position(),
            player.rotation,
            player.known_movement,
            player.on_ground,
            player.profile.id.0,
            player.game_mode == GameMode::Creative,
        );
        let mut base = self.new_entity_base(Kind::Trident);
        base.position = Vec3::new(eye.x, eye.y - EYE_OFFSET, eye.z);
        base.old_position = base.position;
        let seed = self.rng.random::<i64>();
        let mut state = ArrowState::new_trident(&mut base, seed);
        state.set_owner(Some(owner), true);
        state.pickup = if creative {
            Pickup::CreativeOnly
        } else {
            Pickup::Allowed
        };
        let kept = if self.infinite_materials(id) {
            stack.clone()
        } else {
            damaged_without_breaking(&stack, 1)
        };
        state.item = single(&kept);
        state.weapon = Some(stack.clone());
        let acceleration = enchant_effects::trident_return_acceleration(&stack, &mut self.rng)
            .clamp(0.0, MAX_LOYALTY);
        arrow::set_loyalty(&mut base, acceleration as i32);
        arrow::set_foil(&mut base, !stack.enchantments().is_empty());
        arrow::shoot_from_rotation(
            &mut base,
            &mut state,
            Launch {
                pitch: rotation.pitch,
                yaw: rotation.yaw,
                roll: 0.0,
                power: arrow::TRIDENT_THROW_POWER,
                uncertainty: arrow::TRIDENT_THROW_UNCERTAINTY,
                shooter_movement: movement,
                shooter_on_ground: on_ground,
            },
        );
        let trident = Entity::new(base, EntityState::Arrow(Box::new(state)));
        self.spawn_entity(
            trident,
            Birth::of(CreatureSpawnReason::Default)
                .by(id)
                .then(Spawned::Trident {
                    player: id,
                    hand,
                    stack,
                }),
        );
    }

    pub(crate) fn finish_trident_throw(
        &mut self,
        id: PlayerId,
        hand: Hand,
        stack: &ItemStack,
        placed: Option<EntityId>,
    ) {
        let Some(thrown) = placed else {
            self.send_inventory(id);
            return;
        };
        if self.held(id, hand).as_ref() == Some(stack) {
            self.consume_held(id, hand, 1);
        }
        self.play_entity_sound(
            thrown,
            arrow::TRIDENT_THROW_SOUND,
            SoundSource::Players,
            1.0,
            1.0,
        );
        self.refresh_hands(id);
    }

    fn riptide(&mut self, id: PlayerId, hand: Hand, stack: ItemStack, strength: f32) {
        let Some(player) = self.players.get(&id) else {
            return;
        };
        let push = arrow::riptide_push(player.rotation.yaw, player.rotation.pitch, strength);
        let spec = spec_of(&stack);
        self.fire(
            id,
            Continuation::Notice(EventKind::PlayerRiptide),
            move |server| {
                Some(EventPayload::PlayerRiptide(PlayerRiptide {
                    item: spec,
                    player: server.snapshot_of(id)?,
                    velocity: push,
                }))
            },
        );
        if !self.infinite_materials(id) {
            let damaged = damaged_without_breaking(&stack, 1);
            self.set_held(id, hand, Some(damaged));
        }
        let sound = enchant::trident_sound(&stack).unwrap_or(arrow::TRIDENT_THROW_SOUND);
        let Some(player) = self.players.get_mut(&id) else {
            return;
        };
        player.vitals.velocity = player.vitals.velocity + push;
        player.vitals.sync_velocity = true;
        player.spin = Some(SpinAttack {
            ticks: SPIN_TICKS,
            damage: SPIN_DAMAGE,
        });
        let position = player.position;
        self.remove_entities_on_shoulder(id);
        self.play_sound(None, position, sound, SoundSource::Players, 1.0, 1.0);
        self.refresh_hands(id);
    }

    pub(crate) fn trident_channel(&mut self, trident: EntityId, target: EntityId, hurt: bool) {
        if !hurt {
            return;
        }
        let Some(Entity {
            base,
            state: EntityState::Arrow(state),
            ..
        }) = self.entities.store.get(trident)
        else {
            return;
        };
        if base.kind != Kind::Trident {
            return;
        }
        let Some(weapon) = state.weapon.clone() else {
            return;
        };
        let causing = self
            .owner_entity(state.owner)
            .and_then(|owner| self.source_for(owner));
        let source = DamageSource {
            kind: DamageType::Trident,
            causing,
            direct: self.mob_source(trident),
            position: None,
            knockback: None,
            direct_block: None,
        };
        self.run_weapon_post_attack(&weapon, &source, target);
    }

    pub fn trident_hit_block(&mut self, trident: EntityId, pos: BlockPos) {
        let Some(Entity {
            base,
            state: EntityState::Arrow(state),
            ..
        }) = self.entities.store.get(trident)
        else {
            return;
        };
        if base.kind != Kind::Trident {
            return;
        }
        let Some(weapon) = state.weapon.clone() else {
            return;
        };
        self.run_hit_block(&weapon, trident, pos);
    }

    pub(crate) fn trigger_channeled_lightning(&mut self, player: PlayerId, at: Vec3) {
        let radius = lightning::HIT_RADIUS;
        let area = Aabb::new(
            at.x - radius,
            at.y - radius,
            at.z - radius,
            at.x + radius,
            at.y + lightning::HIT_HEIGHT + radius,
            at.z + radius,
        );
        let from = self.players.get(&player).map_or(at, |p| p.position);
        let victims = self
            .living_in(&area)
            .into_iter()
            .filter_map(|victim| self.facts_of(victim, from))
            .collect();
        let mut event = AdvancementEvent::named("minecraft:channeled_lightning");
        event.victims = victims;
        self.trigger_event(player, event);
    }

    pub(crate) fn tick_spin_attacks(&mut self) {
        let spinning: Vec<PlayerId> = self
            .players
            .values()
            .filter(|player| player.spin.is_some())
            .map(|player| player.id)
            .collect();
        for id in spinning {
            self.tick_spin_attack(id);
        }
    }

    fn tick_spin_attack(&mut self, id: PlayerId) {
        let Some(player) = self.players.get_mut(&id) else {
            return;
        };
        let Some(mut spin) = player.spin else {
            return;
        };
        spin.ticks -= 1;
        let entity = player.entity_id;
        let reach = self
            .entity_box(entity)
            .map(|(_, _, bounds)| bounds.expand_towards(self.players[&id].known_movement));
        let target = reach.and_then(|area| {
            self.entities
                .store
                .iter()
                .find(|other| {
                    other.living().is_some()
                        && !other.base.is_removed()
                        && other.base.bounding_box().intersects(&area)
                })
                .map(|other| other.base.id)
        });
        let Some(player) = self.players.get_mut(&id) else {
            return;
        };
        if let Some(target) = target {
            player.vitals.velocity = player.vitals.velocity.scale(SPIN_RECOIL);
            player.vitals.sync_velocity = true;
            player.spin = None;
            if let Some(source) = self.source_for(entity) {
                let source = DamageSource::by(DamageType::PlayerAttack, source);
                self.hurt_entity(target, &source, spin.damage);
            }
        } else if spin.ticks <= 0 {
            player.spin = None;
        } else {
            player.spin = Some(spin);
        }
    }

    pub(crate) fn tick_trident_return(&mut self, id: EntityId) -> bool {
        let Some(Entity {
            base,
            state: EntityState::Arrow(state),
            ..
        }) = self.entities.store.get(id)
        else {
            return false;
        };
        let loyalty = arrow::loyalty(base);
        if base.kind != Kind::Trident || loyalty <= 0 || !(state.dealt_damage || base.no_physics) {
            return false;
        }
        let owner = self.owner_entity(state.owner);
        let position = base.position;
        let pickup = state.pickup;
        let item = state.item.clone();
        let returning = owner.and_then(|owner| self.returning_eye(owner));
        match (owner, returning) {
            (None, _) => false,
            (Some(_), Some((eye, width, player))) => {
                if !player && position.distance_squared(eye).sqrt() < width + 1.0 {
                    self.remove_entity(
                        id,
                        RemovalReason::Discarded,
                        Some(EntityRemoveCause::Despawn),
                    );
                    return true;
                }
                let delta = eye - position;
                let Some(Entity {
                    base,
                    state: EntityState::Arrow(state),
                    ..
                }) = self.entities.store.get_mut(id)
                else {
                    return false;
                };
                base.no_physics = true;
                let level = f64::from(loyalty);
                base.position.y += delta.y * arrow::TRIDENT_RETURN_LIFT * level;
                base.velocity = base.velocity.scale(arrow::TRIDENT_RETURN_DRAG)
                    + delta.normalize().scale(level * arrow::TRIDENT_RETURN_PULL);
                base.needs_sync = true;
                let first = state.return_ticks == 0;
                state.return_ticks += 1;
                if first {
                    self.play_sound(
                        None,
                        position,
                        arrow::TRIDENT_RETURN_SOUND,
                        SoundSource::Neutral,
                        RETURN_SOUND_VOLUME,
                        1.0,
                    );
                }
                false
            }
            (Some(_), None) => {
                if pickup == Pickup::Allowed {
                    let at = Vec3::new(position.x, position.y + DROP_LIFT, position.z);
                    let velocity = Vec3::ZERO;
                    let drop = self.item_entity(at, velocity, 10, item);
                    self.add_entity(drop, CreatureSpawnReason::Default);
                }
                self.remove_entity(id, RemovalReason::Discarded, Some(EntityRemoveCause::Drop));
                true
            }
        }
    }

    fn returning_eye(&self, owner: EntityId) -> Option<(Vec3, f64, bool)> {
        if let Some(player) = self.players.values().find(|p| p.entity_id == owner) {
            let alive = !player.vitals.is_dead() && player.game_mode != GameMode::Spectator;
            return alive.then(|| (player.eye_position(), 0.6, true));
        }
        let entity = self.entities.store.get(owner)?;
        let alive = entity.living().is_some() && !entity.base.is_removed();
        alive.then(|| {
            let eye = Vec3::new(
                entity.base.position.x,
                entity.base.eye_y(),
                entity.base.position.z,
            );
            (eye, f64::from(entity.base.dimensions.width), false)
        })
    }
}

use std::num::NonZeroU8;

use rand::Rng;
use rspigot_domain::damage::DamageTag;
use rspigot_domain::entity::armor_stand;
use rspigot_domain::entity::fire;
use rspigot_domain::entity::living::{EQUIPMENT_SLOTS, slot_index};
use rspigot_domain::entity::physics::block_colliders;
use rspigot_domain::entity::save::component_from_tag;
use rspigot_domain::entity::{Aabb, Entity, EntityState, RemovalReason};
use rspigot_domain::game_event::GameEvent;
use rspigot_domain::item::{ItemStack, OFFHAND_SLOT};
use rspigot_domain::player::{GameMode, PlayerId};
use rspigot_domain::{BlockPos, BlockStateId, EntityId, Vec3};
use rspigot_entities::{EntityKind as Kind, SoundSource};
use rspigot_items::{EquipmentSlot as ItemSlot, ItemKind};

use crate::entities::wire_slot;
use crate::events::{ClientEvent, Direction, EntityStatus, Hand};
use crate::game_events::GameEventContext;
use crate::health::DamageSource;
use crate::input::is_replaceable;
use crate::interact::Interaction;
use crate::plugins::events::{
    CreatureSpawnReason, EntityPlace, EntityRemoveCause, EquipmentSlot, PlayerArmorStandManipulate,
};
use crate::plugins::steps::{StandPlaceStep, StandSwapStep, Struck};
use crate::plugins::{Continuation, EventPayload, ItemSpec, face_of, spec_of};
use crate::ports::ClientLink;
use crate::server::{Audience, Server};

const CUSTOM_NAME: &str = "minecraft:custom_name";
const PLACE_SOUND: &str = "minecraft:entity.armor_stand.place";
const BREAK_SOUND: &str = "minecraft:entity.armor_stand.break";
const HIT_EVENT: i8 = 32;
const IGNITE_SECONDS: f32 = 5.0;
const BURN_DAMAGE: f32 = 4.0;
const FLAME_DAMAGE: f32 = 0.15;
const LOW_HEALTH: f32 = 0.5;
const DROP_PICKUP_DELAY: u16 = 10;

const fn plugin_slot(slot: ItemSlot) -> EquipmentSlot {
    match slot {
        ItemSlot::Mainhand => EquipmentSlot::Hand,
        ItemSlot::Offhand => EquipmentSlot::OffHand,
        ItemSlot::Feet => EquipmentSlot::Feet,
        ItemSlot::Legs => EquipmentSlot::Legs,
        ItemSlot::Chest => EquipmentSlot::Chest,
        ItemSlot::Head => EquipmentSlot::Head,
        ItemSlot::Body => EquipmentSlot::Body,
        ItemSlot::Saddle => EquipmentSlot::Saddle,
    }
}

fn air_spec() -> ItemSpec {
    ItemSpec::of("minecraft:air", 0)
}

fn spec_or_air(stack: Option<&ItemStack>) -> ItemSpec {
    stack.map_or_else(air_spec, spec_of)
}

fn stand_box(at: Vec3) -> Aabb {
    let dimensions = Kind::ArmorStand.dimensions();
    Aabb::of_size(at, dimensions.width, dimensions.height)
}

impl<L: ClientLink> Server<L> {
    pub(crate) fn use_armor_stand(
        &mut self,
        id: PlayerId,
        slot: usize,
        pos: BlockPos,
        face: Direction,
        clicked: BlockStateId,
    ) -> bool {
        let Some(player) = self.players.get(&id) else {
            return false;
        };
        let Some(held) = player.inventory.get(slot) else {
            return false;
        };
        if held.item != ItemKind::ArmorStand.id() {
            return false;
        }
        let name = held
            .components
            .get(CUSTOM_NAME)
            .and_then(component_from_tag);
        let game_mode = player.game_mode;
        let yaw = player.rotation.yaw;
        if face == Direction::Down || matches!(game_mode, GameMode::Adventure | GameMode::Spectator)
        {
            return true;
        }
        let (dx, dy, dz) = face.offset();
        let target = if is_replaceable(clicked) {
            pos
        } else {
            pos.offset(dx, dy, dz)
        };
        let at = Vec3::new(
            f64::from(target.x) + 0.5,
            f64::from(target.y),
            f64::from(target.z) + 0.5,
        );
        let area = stand_box(at);
        let blocked = !block_colliders(&self.world.chunks, &area).is_empty()
            || self.entities.store.iter().any(|entity| {
                !entity.base.is_removed() && entity.base.bounding_box().intersects(&area)
            })
            || self.intersects_player(target);
        if blocked
            || !self.world.dimension.height.contains(target.y)
            || !self.within_reach(id, target)
            || self.is_protected(id, target)
        {
            return true;
        }
        let mut base = self.new_entity_base(Kind::ArmorStand);
        let state = armor_stand::define(&mut base);
        if let Some(name) = name {
            base.set_custom_name(Some(name));
        }
        let snapped = rspigot_domain::mth::wrap_degrees(yaw - 180.0);
        let rotation = ((snapped + 22.5) / 45.0).floor() * 45.0;
        base.snap_to(at, rotation, 0.0);
        let entity = Entity::new(base, EntityState::Stand(Box::new(state)));
        let view = crate::plugins::view_of_entity(&entity);
        let step = StandPlaceStep {
            player: id,
            slot,
            entity,
        };
        let lane = self.lane_for(Some(id), view.id);
        self.fire(
            lane,
            Continuation::PlaceStand(Box::new(step)),
            move |server| {
                Some(EventPayload::EntityPlace(EntityPlace {
                    block: pos,
                    block_face: face_of(side_of(face)),
                    entity_type: view.kind.clone(),
                    entity: view,
                    hand: if slot == OFFHAND_SLOT {
                        EquipmentSlot::OffHand
                    } else {
                        EquipmentSlot::Hand
                    },
                    player: server.snapshot_of(id),
                }))
            },
        );
        true
    }

    pub(crate) fn finish_stand_place(&mut self, step: StandPlaceStep, cancelled: bool) {
        if cancelled {
            return;
        }
        let StandPlaceStep {
            player,
            slot,
            entity,
        } = step;
        let at = entity.base.position;
        self.add_entity(entity, CreatureSpawnReason::Default);
        self.play_sound(None, at, PLACE_SOUND, SoundSource::Blocks, 0.75, 0.8);
        let placer = self.players.get(&player).map(|p| p.entity_id);
        self.game_event(GameEvent::EntityPlace, at, GameEventContext::of(placer));
        self.consume_placed(player, slot);
    }

    pub(crate) fn tick_stand(&mut self, id: EntityId) {
        let Some(Entity {
            base,
            state: EntityState::Stand(_),
            ..
        }) = self.entities.store.get_mut(id)
        else {
            return;
        };
        armor_stand::tick(base, &self.world.chunks);
        let burn = if base.remaining_fire_ticks > 0 {
            let due = base.remaining_fire_ticks % fire::FIRE_DAMAGE_PERIOD == 0;
            base.remaining_fire_ticks -= 1;
            fire::set_on_fire_flag(base, base.remaining_fire_ticks > 0);
            due
        } else {
            false
        };
        self.apply_fire_inside(id);
        if burn {
            self.burn_damage(id);
        }
    }

    pub(crate) fn stand_interact(
        &mut self,
        id: PlayerId,
        target: EntityId,
        hand: Hand,
        location: Vec3,
    ) -> Option<Interaction> {
        let Some(Entity {
            base,
            state: EntityState::Stand(state),
            ..
        }) = self.entities.store.get(target)
        else {
            return None;
        };
        if base.is_removed() || armor_stand::is_marker(base) {
            return None;
        }
        let held = self.held(id, hand);
        if held
            .as_ref()
            .is_some_and(|stack| stack.item == ItemKind::NameTag.id())
        {
            return None;
        }
        match held {
            None => {
                let mut slot = armor_stand::clicked_slot(base, state, location.y);
                if armor_stand::is_disabled(base, state, slot) {
                    slot = ItemSlot::Mainhand;
                }
                if armor_stand::item_in(state, slot).is_some()
                    && self.ask_stand_swap(id, target, hand, slot)
                {
                    return Some(Interaction::ServerOnly);
                }
                None
            }
            Some(stack) => {
                let slot = armor_stand::slot_for_item(base, state, &stack);
                if armor_stand::is_disabled(base, state, slot) {
                    return Some(Interaction::Consume);
                }
                if armor_stand::is_hand(slot) && !armor_stand::shows_arms(base) {
                    return Some(Interaction::Consume);
                }
                if self.ask_stand_swap(id, target, hand, slot) {
                    Some(Interaction::ServerOnly)
                } else {
                    None
                }
            }
        }
    }

    fn ask_stand_swap(
        &mut self,
        id: PlayerId,
        target: EntityId,
        hand: Hand,
        slot: ItemSlot,
    ) -> bool {
        let Some(Entity {
            state: EntityState::Stand(state),
            ..
        }) = self.entities.store.get(target)
        else {
            return false;
        };
        let current = armor_stand::item_in(state, slot);
        if current.is_some() && armor_stand::slot_locked_for_taking(state, slot) {
            return false;
        }
        if current.is_none() && armor_stand::slot_locked_for_putting(state, slot) {
            return false;
        }
        let stand_item = spec_or_air(current);
        let player_item = spec_or_air(self.held(id, hand).as_ref());
        let step = StandSwapStep {
            player: id,
            target,
            hand,
            slot,
        };
        let lane = self.lane_for(Some(id), target);
        self.fire(
            lane,
            Continuation::SwapStand(Box::new(step)),
            move |server| {
                Some(EventPayload::PlayerArmorStandManipulate(
                    PlayerArmorStandManipulate {
                        armor_stand_item: stand_item,
                        hand: if hand == Hand::Off {
                            EquipmentSlot::OffHand
                        } else {
                            EquipmentSlot::Hand
                        },
                        player: server.snapshot_of(id)?,
                        player_item,
                        right_clicked: server.entity_view(target)?,
                        slot: plugin_slot(slot),
                    },
                ))
            },
        );
        true
    }

    pub(crate) fn finish_stand_swap(&mut self, step: StandSwapStep, cancelled: bool) {
        let StandSwapStep {
            player,
            target,
            hand,
            slot,
        } = step;
        if !cancelled
            && self
                .entities
                .store
                .get(target)
                .is_some_and(|entity| !entity.base.is_removed())
        {
            self.swap_stand_item(player, target, hand, slot);
        }
        self.refresh_hands(player);
    }

    fn swap_stand_item(&mut self, id: PlayerId, target: EntityId, hand: Hand, slot: ItemSlot) {
        let infinite = self.infinite_materials(id);
        let carried = self.held(id, hand);
        let Some(Entity {
            state: EntityState::Stand(state),
            ..
        }) = self.entities.store.get_mut(target)
        else {
            return;
        };
        let current = armor_stand::item_in(state, slot).cloned();
        let one = |stack: &ItemStack| ItemStack {
            count: NonZeroU8::MIN,
            ..stack.clone()
        };
        if infinite
            && current.is_none()
            && let Some(stack) = &carried
        {
            armor_stand::set_item(state, slot, Some(one(stack)));
        } else if let Some(stack) = &carried
            && stack.count.get() > 1
        {
            if current.is_some() {
                return;
            }
            armor_stand::set_item(state, slot, Some(one(stack)));
            self.shrink_held(id, hand, 1);
        } else {
            armor_stand::set_item(state, slot, carried);
            self.set_held(id, hand, current);
        }
        self.send_stand_slot(target, slot);
    }

    fn send_stand_slot(&mut self, id: EntityId, slot: ItemSlot) {
        let Some(Entity {
            state: EntityState::Stand(state),
            ..
        }) = self.entities.store.get(id)
        else {
            return;
        };
        let stack = armor_stand::item_in(state, slot).cloned();
        self.push(
            Audience::Trackers(id),
            ClientEvent::EntityEquipment {
                entity: id,
                slots: vec![(wire_slot(slot), stack)],
            },
        );
    }

    pub(crate) fn ask_hurt_stand(&self, id: EntityId, source: &DamageSource) -> bool {
        let Some(Entity {
            base,
            state: EntityState::Stand(_),
            ..
        }) = self.entities.store.get(id)
        else {
            return false;
        };
        if base.is_removed() {
            return false;
        }
        let mob_attacker = source
            .causing
            .as_ref()
            .is_some_and(|causing| causing.kind.extends("Mob"));
        if !self.config.rules.mob_griefing && mob_attacker {
            return false;
        }
        if source.kind.is(DamageTag::BypassesInvulnerability) {
            return true;
        }
        !base.invulnerable && !armor_stand::is_marker(base)
    }

    pub(crate) fn apply_hurt_stand(&mut self, id: EntityId, source: &DamageSource) -> bool {
        let Some(Entity {
            base,
            state: EntityState::Stand(state),
            ..
        }) = self.entities.store.get(id)
        else {
            return false;
        };
        let tick = base.tick_count;
        let last_hit = state.last_hit;
        let burning = fire::is_on_fire(base);
        let health = armor_stand::health(base);
        let remover = source.causing.as_ref().map(|causing| causing.entity);
        if source.kind.is(DamageTag::BypassesInvulnerability) {
            self.remove_stand(id, remover);
            return false;
        }
        if source.kind.in_tag("minecraft:is_explosion") {
            self.break_stand(id, remover, false);
            return false;
        }
        if source.kind.in_tag("minecraft:ignites_armor_stands") {
            if burning {
                self.damage_stand(id, source, FLAME_DAMAGE);
            } else if let Some(entity) = self.entities.store.get_mut(id) {
                fire::ignite_for_ticks(&mut entity.base, fire::seconds_to_ticks(IGNITE_SECONDS));
                fire::set_on_fire_flag(&mut entity.base, true);
            }
            return false;
        }
        if source.kind.in_tag("minecraft:burns_armor_stands") && health > LOW_HEALTH {
            self.damage_stand(id, source, BURN_DAMAGE);
            return false;
        }
        let breaks = source.kind.in_tag("minecraft:can_break_armor_stand");
        let kills = source.kind.in_tag("minecraft:always_kills_armor_stands");
        if !breaks && !kills {
            return false;
        }
        let attacker = source.attacker();
        if let Some(player) = attacker.and_then(|id| self.players.get(&id))
            && matches!(player.game_mode, GameMode::Adventure | GameMode::Spectator)
        {
            return false;
        }
        if source.creative_attacker() {
            self.break_stand(id, remover, false);
            return true;
        }
        if kills || last_hit.is_some_and(|hit| tick - hit < armor_stand::HIT_WINDOW + 1) {
            self.break_stand(id, remover, true);
            return true;
        }
        self.push(
            Audience::Trackers(id),
            ClientEvent::EntityStatus {
                entity: id,
                status: EntityStatus::Event(HIT_EVENT),
            },
        );
        let at = self.entities.store.get(id).map(|e| e.base.position);
        if let Some(at) = at {
            self.game_event(GameEvent::EntityDamage, at, GameEventContext::of(remover));
        }
        if let Some(Entity {
            state: EntityState::Stand(state),
            ..
        }) = self.entities.store.get_mut(id)
        {
            state.last_hit = Some(tick);
        }
        true
    }

    fn damage_stand(&mut self, id: EntityId, _source: &DamageSource, amount: f32) {
        let Some(Entity { base, .. }) = self.entities.store.get_mut(id) else {
            return;
        };
        let health = armor_stand::health(base);
        if health - amount > LOW_HEALTH {
            armor_stand::set_health(base, health - amount);
            let at = base.position;
            self.game_event(GameEvent::EntityDamage, at, GameEventContext::of(None));
        } else {
            self.break_stand(id, None, false);
        }
    }

    fn remove_stand(&mut self, id: EntityId, remover: Option<EntityId>) -> Option<Entity> {
        let entity =
            self.remove_entity(id, RemovalReason::Killed, Some(EntityRemoveCause::Death))?;
        self.game_event(
            GameEvent::EntityDie,
            entity.base.position,
            GameEventContext::of(remover.or(Some(id))),
        );
        Some(entity)
    }

    fn break_stand(&mut self, id: EntityId, remover: Option<EntityId>, with_stand: bool) {
        let at = self.entities.store.get(id).map(|e| e.base.position);
        if let Some(at) = at {
            self.play_sound(None, at, BREAK_SOUND, SoundSource::Neutral, 1.0, 1.0);
        }
        let Some(entity) = self.remove_stand(id, remover) else {
            return;
        };
        let mut drops = Vec::new();
        if with_stand {
            let mut stand = ItemStack::new(ItemKind::ArmorStand.id(), NonZeroU8::MIN);
            if let Some(name) = entity.base.custom_name() {
                stand.components.insert(
                    CUSTOM_NAME,
                    rspigot_domain::entity::save::component_to_tag(name),
                );
            }
            drops.push(stand);
        }
        if let EntityState::Stand(state) = &entity.state {
            for slot in EQUIPMENT_SLOTS {
                if let Some(stack) = state.equipment[slot_index(slot)].clone() {
                    drops.push(stack);
                }
            }
        }
        for stack in drops {
            let velocity = Vec3::new(
                self.rng.random::<f64>() * 0.2 - 0.1,
                0.2,
                self.rng.random::<f64>() * 0.2 - 0.1,
            );
            self.spawn_item_entity(entity.base.position, velocity, DROP_PICKUP_DELAY, stack);
        }
    }

    pub(crate) fn stand_attack_refused(&self, id: PlayerId, target: EntityId) -> bool {
        self.entities.store.get(target).is_some_and(|entity| {
            matches!(entity.state, EntityState::Stand(_))
                && self.is_protected(id, entity.base.position.block())
        })
    }

    pub(crate) fn stand_struck(&self, id: EntityId) -> Option<Struck> {
        self.entities
            .store
            .get(id)
            .filter(|entity| matches!(entity.state, EntityState::Stand(_)))
            .map(|_| Struck::Stand(id))
    }
}

const fn side_of(face: Direction) -> rspigot_blocks::Face {
    match face {
        Direction::Down => rspigot_blocks::Face::Down,
        Direction::Up => rspigot_blocks::Face::Up,
        Direction::North => rspigot_blocks::Face::North,
        Direction::South => rspigot_blocks::Face::South,
        Direction::West => rspigot_blocks::Face::West,
        Direction::East => rspigot_blocks::Face::East,
    }
}

use std::num::NonZeroU8;

use rand::Rng;
use rspigot_blocks::Face as Side;
use rspigot_domain::entity::hanging::{self, Neighbour};
use rspigot_domain::entity::save::component_to_tag;
use rspigot_domain::entity::{Entity, EntityState, RemovalReason};
use rspigot_domain::game_event::GameEvent;
use rspigot_domain::item::{ItemStack, OFFHAND_SLOT};
use rspigot_domain::player::{GameMode, PlayerId};
use rspigot_domain::{BlockPos, EntityId, Vec3};
use rspigot_entities::{EntityKind as Kind, SoundSource};
use rspigot_items::ItemKind;

use crate::events::{Direction, Hand};
use crate::game_events::GameEventContext;
use crate::health::DamageSource;
use crate::interact::Interaction;
use crate::plugins::events::{
    CreatureSpawnReason, EntityRemoveCause, EquipmentSlot, HangingBreak, HangingBreakByEntity,
    HangingBreakRemoveCause, HangingPlace,
};
use crate::plugins::steps::{Breaking, HangingBreakStep, HangingPlaceStep};
use crate::plugins::{Continuation, EventPayload, face_of, spec_of, view_of_entity};
use crate::ports::ClientLink;
use crate::server::Server;

const CUSTOM_NAME: &str = "minecraft:custom_name";
const WALL_PUSH: f64 = 0.15;
const DROP_PICKUP_DELAY: u16 = 10;
const EXPLOSION_TAG: &str = "minecraft:is_explosion";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum HangingSound {
    Place,
    Break,
    AddItem,
    RemoveItem,
    RotateItem,
}

pub(crate) const fn hanging_sound(kind: Kind, sound: HangingSound) -> &'static str {
    match (kind, sound) {
        (Kind::Painting, HangingSound::Break) => "minecraft:entity.painting.break",
        (Kind::Painting, _) => "minecraft:entity.painting.place",
        (Kind::GlowItemFrame, HangingSound::Place) => "minecraft:entity.glow_item_frame.place",
        (Kind::GlowItemFrame, HangingSound::Break) => "minecraft:entity.glow_item_frame.break",
        (Kind::GlowItemFrame, HangingSound::AddItem) => "minecraft:entity.glow_item_frame.add_item",
        (Kind::GlowItemFrame, HangingSound::RemoveItem) => {
            "minecraft:entity.glow_item_frame.remove_item"
        }
        (Kind::GlowItemFrame, HangingSound::RotateItem) => {
            "minecraft:entity.glow_item_frame.rotate_item"
        }
        (_, HangingSound::Place) => "minecraft:entity.item_frame.place",
        (_, HangingSound::Break) => "minecraft:entity.item_frame.break",
        (_, HangingSound::AddItem) => "minecraft:entity.item_frame.add_item",
        (_, HangingSound::RemoveItem) => "minecraft:entity.item_frame.remove_item",
        (_, HangingSound::RotateItem) => "minecraft:entity.item_frame.rotate_item",
    }
}

const fn side_of(face: Direction) -> Side {
    match face {
        Direction::Down => Side::Down,
        Direction::Up => Side::Up,
        Direction::North => Side::North,
        Direction::South => Side::South,
        Direction::West => Side::West,
        Direction::East => Side::East,
    }
}

fn hanging_item_kind(held: &ItemStack) -> Option<Kind> {
    match held.item.kind()? {
        ItemKind::ItemFrame => Some(Kind::ItemFrame),
        ItemKind::GlowItemFrame => Some(Kind::GlowItemFrame),
        ItemKind::Painting => Some(Kind::Painting),
        _ => None,
    }
}

fn frame_stack(kind: Kind, name: Option<&rspigot_domain::text::Component>) -> ItemStack {
    let item = match kind {
        Kind::GlowItemFrame => ItemKind::GlowItemFrame,
        Kind::Painting => ItemKind::Painting,
        _ => ItemKind::ItemFrame,
    };
    let mut stack = ItemStack::new(item.id(), NonZeroU8::MIN);
    if let Some(name) = name {
        stack.components.insert(CUSTOM_NAME, component_to_tag(name));
    }
    stack
}

impl<L: ClientLink> Server<L> {
    pub(crate) fn hanging_neighbours(&self) -> Vec<Neighbour> {
        self.entities
            .store
            .iter()
            .filter(|entity| !entity.base.is_removed())
            .filter(|entity| matches!(entity.state, EntityState::Hanging(_)))
            .map(|entity| Neighbour {
                id: entity.base.id,
                kind: entity.base.kind,
                facing: hanging::facing(&entity.base),
                bounds: entity.base.bounding_box(),
            })
            .collect()
    }

    fn hanging_holds(&self, id: EntityId) -> bool {
        let Some(Entity {
            base,
            state: EntityState::Hanging(state),
            ..
        }) = self.entities.store.get(id)
        else {
            return true;
        };
        hanging::survives(&self.world.chunks, base, state, &self.hanging_neighbours())
    }

    fn play_hanging_sound(&mut self, at: Vec3, kind: Kind, sound: HangingSound) {
        self.play_sound(
            None,
            at,
            hanging_sound(kind, sound),
            SoundSource::Neutral,
            1.0,
            1.0,
        );
    }

    fn new_hanging(&mut self, kind: Kind, pos: BlockPos, facing: Side) -> Option<Entity> {
        let mut base = self.new_entity_base(kind);
        let mut state = hanging::define(&mut base);
        hanging::place(&mut base, &mut state, pos, facing);
        if kind == Kind::Painting {
            let neighbours = self.hanging_neighbours();
            let mut fits = Vec::new();
            for variant in hanging::placeable_variants() {
                hanging::set_variant(&mut base, variant);
                hanging::place(&mut base, &mut state, pos, facing);
                if hanging::survives(&self.world.chunks, &base, &state, &neighbours) {
                    fits.push(variant);
                }
            }
            let largest = fits.iter().map(|id| hanging::variant_area(*id)).max()?;
            fits.retain(|id| hanging::variant_area(*id) >= largest);
            let chosen = fits[self.rng.random_range(0..fits.len())];
            hanging::set_variant(&mut base, chosen);
            hanging::place(&mut base, &mut state, pos, facing);
        }
        Some(Entity::new(base, EntityState::Hanging(state)))
    }

    pub(crate) fn use_hanging_item(
        &mut self,
        id: PlayerId,
        slot: usize,
        pos: BlockPos,
        face: Direction,
    ) -> bool {
        let Some(player) = self.players.get(&id) else {
            return false;
        };
        let Some(kind) = player.inventory.get(slot).and_then(hanging_item_kind) else {
            return false;
        };
        let held = player.inventory.get(slot).cloned();
        let game_mode = player.game_mode;
        let facing = side_of(face);
        let (dx, dy, dz) = face.offset();
        let target = pos.offset(dx, dy, dz);
        let vertical = !hanging::is_horizontal(facing);
        if (kind == Kind::Painting && vertical)
            || matches!(game_mode, GameMode::Adventure | GameMode::Spectator)
            || !self.world.dimension.height.contains(target.y)
            || !self.within_reach(id, target)
            || self.is_protected(id, target)
        {
            return true;
        }
        let Some(entity) = self.new_hanging(kind, target, facing) else {
            return true;
        };
        if !self.hanging_holds_entity(&entity) {
            return true;
        }
        let view = view_of_entity(&entity);
        let step = HangingPlaceStep {
            player: id,
            slot,
            entity,
        };
        let lane = self.lane_for(Some(id), view.id);
        self.fire(
            lane,
            Continuation::PlaceHanging(Box::new(step)),
            move |server| {
                Some(EventPayload::HangingPlace(HangingPlace {
                    block: pos,
                    block_face: face_of(facing),
                    entity: view,
                    hand: Some(if slot == OFFHAND_SLOT {
                        EquipmentSlot::OffHand
                    } else {
                        EquipmentSlot::Hand
                    }),
                    item_stack: held.as_ref().map(spec_of),
                    player: server.snapshot_of(id),
                }))
            },
        );
        true
    }

    fn hanging_holds_entity(&self, entity: &Entity) -> bool {
        let EntityState::Hanging(state) = &entity.state else {
            return false;
        };
        hanging::survives(
            &self.world.chunks,
            &entity.base,
            state,
            &self.hanging_neighbours(),
        )
    }

    pub(crate) fn finish_hanging_place(&mut self, step: HangingPlaceStep, cancelled: bool) {
        if cancelled {
            return;
        }
        let HangingPlaceStep {
            player,
            slot,
            entity,
        } = step;
        let kind = entity.base.kind;
        let at = entity.base.position;
        self.play_hanging_sound(at, kind, HangingSound::Place);
        let placer = self.players.get(&player).map(|p| p.entity_id);
        self.game_event(GameEvent::EntityPlace, at, GameEventContext::of(placer));
        self.add_entity(entity, CreatureSpawnReason::Default);
        self.consume_placed(player, slot);
    }

    pub(crate) fn consume_placed(&mut self, id: PlayerId, slot: usize) {
        if self.infinite_materials(id) {
            return;
        }
        let Some(player) = self.players.get_mut(&id) else {
            return;
        };
        let Some(held) = player.inventory.get(slot) else {
            return;
        };
        let left = NonZeroU8::new(held.count.get() - 1).map(|count| ItemStack {
            count,
            ..held.clone()
        });
        player.inventory.set(slot, left);
    }

    pub(crate) fn tick_hanging(&mut self, id: EntityId) {
        let Some(Entity {
            base,
            state: EntityState::Hanging(state),
            ..
        }) = self.entities.store.get_mut(id)
        else {
            return;
        };
        base.tick_count = base.tick_count.wrapping_add(1);
        let due = state.ticks_since_check >= hanging::CHECK_INTERVAL;
        state.ticks_since_check = if due { 0 } else { state.ticks_since_check + 1 };
        let here = base.position.block();
        if !due || self.hanging_holds(id) {
            return;
        }
        let cause = if self
            .world
            .chunks
            .block(here)
            .is_some_and(|state| state.is_air())
        {
            HangingBreakRemoveCause::Physics
        } else {
            HangingBreakRemoveCause::Obstruction
        };
        self.ask_hanging_break(id, Breaking::Survival, cause, None);
    }

    fn ask_hanging_break(
        &mut self,
        id: EntityId,
        breaking: Breaking,
        cause: HangingBreakRemoveCause,
        remover: Option<EntityId>,
    ) {
        let Some(view) = self.entity_view(id) else {
            return;
        };
        let remover_view = remover.and_then(|entity| self.subject_view(entity));
        let player = match &breaking {
            Breaking::Hurt { player, .. } => *player,
            Breaking::Survival => None,
        };
        let step = HangingBreakStep {
            id,
            breaking,
            by_entity: remover_view.is_some(),
        };
        let lane = self.lane_for(player, id);
        self.fire(
            lane,
            Continuation::BreakHanging(Box::new(step)),
            move |_| {
                Some(match remover_view {
                    Some(remover) => EventPayload::HangingBreakByEntity(HangingBreakByEntity {
                        cause,
                        entity: view,
                        remover: Some(remover),
                    }),
                    None => EventPayload::HangingBreak(HangingBreak {
                        cause,
                        entity: view,
                    }),
                })
            },
        );
    }

    pub(crate) fn finish_hanging_break(&mut self, step: HangingBreakStep, cancelled: bool) {
        let HangingBreakStep { id, breaking, .. } = step;
        if cancelled
            || self
                .entities
                .store
                .get(id)
                .is_none_or(|entity| entity.base.is_removed())
        {
            return;
        }
        match breaking {
            Breaking::Survival => {
                let Some(entity) =
                    self.remove_entity(id, RemovalReason::Discarded, Some(EntityRemoveCause::Drop))
                else {
                    return;
                };
                self.drop_hanging(&entity, None);
            }
            Breaking::Hurt { remover, player } => {
                let Some(entity) =
                    self.remove_entity(id, RemovalReason::Killed, Some(EntityRemoveCause::Death))
                else {
                    return;
                };
                self.game_event(
                    GameEvent::EntityDie,
                    entity.base.position,
                    GameEventContext::of(remover.or(Some(id))),
                );
                self.drop_hanging(&entity, player);
            }
        }
    }

    fn spawn_at_wall(&mut self, entity: &Entity, stack: ItemStack) {
        let (dx, _, dz) = hanging::facing(&entity.base).offset();
        let at = Vec3::new(
            entity.base.position.x + f64::from(dx) * WALL_PUSH,
            entity.base.position.y,
            entity.base.position.z + f64::from(dz) * WALL_PUSH,
        );
        let velocity = Vec3::new(
            self.rng.random::<f64>() * 0.2 - 0.1,
            0.2,
            self.rng.random::<f64>() * 0.2 - 0.1,
        );
        self.spawn_item_entity(at, velocity, DROP_PICKUP_DELAY, stack);
    }

    fn drop_hanging(&mut self, entity: &Entity, player: Option<PlayerId>) {
        let kind = entity.base.kind;
        let at = entity.base.position;
        self.play_hanging_sound(at, kind, HangingSound::Break);
        let creative = player.is_some_and(|id| self.infinite_materials(id));
        let EntityState::Hanging(state) = &entity.state else {
            return;
        };
        if kind == Kind::Painting {
            if !creative {
                let stack = frame_stack(kind, entity.base.custom_name());
                self.spawn_at_wall(entity, stack);
            }
            return;
        }
        if !state.fixed && !creative {
            let stack = frame_stack(kind, entity.base.custom_name());
            self.spawn_at_wall(entity, stack);
            if let Some(item) = hanging::item(&entity.base).cloned()
                && self.rng.random::<f32>() < state.drop_chance
            {
                self.spawn_at_wall(entity, item);
            }
        }
        let placer = player.and_then(|id| self.players.get(&id).map(|p| p.entity_id));
        self.game_event(GameEvent::BlockChange, at, GameEventContext::of(placer));
    }

    pub(crate) fn ask_hurt_hanging(&self, id: EntityId, source: &DamageSource) -> bool {
        let Some(Entity {
            base,
            state: EntityState::Hanging(state),
            ..
        }) = self.entities.store.get(id)
        else {
            return false;
        };
        if base.is_removed() {
            return false;
        }
        let bypasses = source
            .kind
            .is(rspigot_domain::damage::DamageTag::BypassesInvulnerability);
        if state.fixed {
            return bypasses || source.creative_attacker();
        }
        !base.invulnerable || bypasses
    }

    pub(crate) fn apply_hurt_hanging(&mut self, id: EntityId, source: &DamageSource) -> bool {
        let explosion = source.kind.in_tag(EXPLOSION_TAG);
        let Some(Entity {
            base,
            state: EntityState::Hanging(state),
            ..
        }) = self.entities.store.get(id)
        else {
            return false;
        };
        let framed = hanging::is_frame(base.kind) && hanging::item(base).is_some();
        let fixed = state.fixed;
        let player = source.attacker();
        if !fixed && !explosion && framed {
            self.pop_framed_item(id, player);
            return true;
        }
        let mob_attacker = source
            .causing
            .as_ref()
            .is_some_and(|causing| causing.kind.extends("Mob"));
        if !self.config.rules.mob_griefing && mob_attacker {
            return false;
        }
        let remover = source.causing.as_ref().map(|causing| causing.entity);
        let cause = match (remover.is_some(), explosion) {
            (true, true) | (false, true) => HangingBreakRemoveCause::Explosion,
            (true, false) => HangingBreakRemoveCause::Entity,
            (false, false) => HangingBreakRemoveCause::Default,
        };
        self.ask_hanging_break(id, Breaking::Hurt { remover, player }, cause, remover);
        true
    }

    fn pop_framed_item(&mut self, id: EntityId, player: Option<PlayerId>) {
        let creative = player.is_some_and(|id| self.infinite_materials(id));
        let Some(Entity {
            base,
            state: EntityState::Hanging(state),
            ..
        }) = self.entities.store.get_mut(id)
        else {
            return;
        };
        let drop_chance = state.drop_chance;
        let Some(item) = hanging::item(base).cloned() else {
            return;
        };
        let kind = base.kind;
        let at = base.position;
        hanging::set_item(base, state, None);
        let spawn = !creative && self.rng.random::<f32>() < drop_chance;
        if spawn && let Some(entity) = self.entities.store.get(id).cloned() {
            self.spawn_at_wall(&entity, item);
        }
        let placer = player.and_then(|id| self.players.get(&id).map(|p| p.entity_id));
        self.game_event(GameEvent::BlockChange, at, GameEventContext::of(placer));
        self.play_hanging_sound(at, kind, HangingSound::RemoveItem);
    }

    pub(crate) fn hanging_attack(&mut self, id: PlayerId, target: EntityId) -> bool {
        let Some(entity) = self.entities.store.get(target) else {
            return false;
        };
        if !matches!(entity.state, EntityState::Hanging(_)) {
            return false;
        }
        let pos = entity.base.position.block();
        if self.is_protected(id, pos) {
            return true;
        }
        let Some(source) = self.source_of(id) else {
            return true;
        };
        let source = DamageSource::by(rspigot_domain::damage::DamageType::PlayerAttack, source);
        self.strike(
            Some(id),
            crate::plugins::steps::Struck::Hanging(target),
            &source,
            0.0,
            crate::plugins::steps::AfterHurt::Nothing,
        );
        true
    }

    pub(crate) fn frame_interact(
        &mut self,
        id: PlayerId,
        target: EntityId,
        hand: Hand,
    ) -> Option<Interaction> {
        let entity = self.entities.store.get(target)?;
        let EntityState::Hanging(state) = &entity.state else {
            return None;
        };
        if !hanging::is_frame(entity.base.kind) || state.fixed || entity.base.is_removed() {
            return None;
        }
        let kind = entity.base.kind;
        let at = entity.base.position;
        let framed = hanging::item(&entity.base).is_some();
        let held = self.held(id, hand);
        let placer = self.players.get(&id).map(|p| p.entity_id);
        if framed {
            self.play_hanging_sound(at, kind, HangingSound::RotateItem);
            if let Some(Entity {
                base,
                state: EntityState::Hanging(_),
                ..
            }) = self.entities.store.get_mut(target)
            {
                let next = hanging::rotation(base) + 1;
                hanging::set_rotation(base, next);
            }
            self.game_event(GameEvent::BlockChange, at, GameEventContext::of(placer));
            return Some(Interaction::Predicted);
        }
        let stack = held?;
        if let Some(Entity {
            base,
            state: EntityState::Hanging(state),
            ..
        }) = self.entities.store.get_mut(target)
        {
            hanging::set_item(base, state, Some(stack));
        }
        self.play_hanging_sound(at, kind, HangingSound::AddItem);
        self.game_event(GameEvent::BlockChange, at, GameEventContext::of(placer));
        self.consume_held(id, hand, 1);
        Some(Interaction::Predicted)
    }
}

use rspigot_ai::{CopperGolemAction, TargetVerdict};
use rspigot_domain::entity::living::slot_index;
use rspigot_domain::entity::species::Species;
use rspigot_domain::entity::species::copper_golem::{
    self, CopperGolemState, GolemState, WeatherState,
};
use rspigot_domain::entity::{Entity, EntityBase, EntityState};
use rspigot_domain::item::{ItemStack, Slot};
use rspigot_domain::player::PlayerId;
use rspigot_domain::{BlockPos, EntityId, Vec3};
use rspigot_entities::EntityKind as Kind;
use rspigot_items::{EquipmentSlot, ItemKind};

use crate::containers::GolemOpening;
use crate::events::{ClientEvent, EquipmentSlot as WireSlot, Hand};
use crate::interact::{Interaction, item_tag};
use crate::mobs::ServerRandom;
use crate::plugins::Continuation;
use crate::plugins::EventPayload;
use crate::plugins::events::{EntityTargetBlock, EntityTargetBlockPatch};
use crate::ports::ClientLink;
use crate::server::{Audience, Server};

const SPAWN_SOUND: &str = "minecraft:entity.copper_golem.spawn";
const SHEAR_SOUND: &str = "minecraft:entity.copper_golem.shear";
const BECOME_STATUE_SOUND: &str = "minecraft:entity.copper_golem_become_statue";
const WAX_ON_SOUND: &str = "minecraft:item.honeycomb.wax_on";
const SCRAPE_SOUND: &str = "minecraft:item.axe.scrape";
const SHEARABLE: &str = "minecraft:shearable_from_copper_golem";
const GUARANTEED_DROP: f32 = 2.0;
const PRESERVED_CHANCE: f32 = 1.0;
const RECHECK_TICKS: i64 = 5;
const THROW_HEIGHT_OFFSET: f64 = 0.3;
const THROW_SPEED: f64 = 0.3;
const THROW_PICKUP_DELAY: u16 = 10;
const ANTENNA_DROP_OFFSET: f64 = 1.5;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum GolemVoice {
    Hurt,
    Death,
}

pub(crate) fn voice(weather: WeatherState, which: GolemVoice) -> &'static str {
    match (weather, which) {
        (WeatherState::Unaffected | WeatherState::Exposed, GolemVoice::Hurt) => {
            "minecraft:entity.copper_golem.hurt"
        }
        (WeatherState::Unaffected | WeatherState::Exposed, GolemVoice::Death) => {
            "minecraft:entity.copper_golem.death"
        }
        (WeatherState::Weathered, GolemVoice::Hurt) => {
            "minecraft:entity.copper_golem_weathered.hurt"
        }
        (WeatherState::Weathered, GolemVoice::Death) => {
            "minecraft:entity.copper_golem_weathered.death"
        }
        (WeatherState::Oxidized, GolemVoice::Hurt) => "minecraft:entity.copper_golem_oxidized.hurt",
        (WeatherState::Oxidized, GolemVoice::Death) => {
            "minecraft:entity.copper_golem_oxidized.death"
        }
    }
}

fn with_golem<R>(
    entity: &mut Entity,
    update: impl FnOnce(&mut EntityBase, &mut CopperGolemState) -> R,
) -> Option<R> {
    let Entity {
        base,
        state: EntityState::Living(living),
        ..
    } = entity
    else {
        return None;
    };
    let Some(Species::CopperGolem(state)) = living.mob.as_mut().map(|mob| &mut mob.species) else {
        return None;
    };
    Some(update(base, state))
}

pub(crate) fn golem_state_of(entity: &Entity) -> Option<CopperGolemState> {
    match entity
        .living()
        .and_then(|living| living.mob.as_ref())
        .map(|mob| mob.species)
    {
        Some(Species::CopperGolem(state)) => Some(state),
        _ => None,
    }
}

pub(crate) fn ready_for_shearing(entity: &Entity) -> bool {
    entity
        .living()
        .and_then(|living| living.equipment(EquipmentSlot::Saddle))
        .is_some_and(|stack| item_tag(stack.item, SHEARABLE))
}

impl<L: ClientLink> Server<L> {
    pub(crate) fn copper_golem_post_tick(&mut self, id: EntityId) {
        let game_time = self.world.level.game_time;
        let rng = &mut self.rng;
        let outcome = self.entities.store.get_mut(id).and_then(|entity| {
            with_golem(entity, |base, state| {
                let pending = std::mem::take(&mut state.spawn_sound_pending);
                let oxidized =
                    copper_golem::update_weathering(base, state, game_time, &mut ServerRandom(rng));
                (pending, oxidized)
            })
        });
        let Some((pending, oxidized)) = outcome else {
            return;
        };
        if pending {
            self.entity_sound(id, SPAWN_SOUND, 1.0, 1.0);
        }
        if oxidized && self.can_turn_to_statue(id) {
            self.turn_to_statue(id);
        }
    }

    fn can_turn_to_statue(&mut self, id: EntityId) -> bool {
        let Some(pos) = self
            .entities
            .store
            .get(id)
            .map(|entity| entity.base.position.block())
        else {
            return false;
        };
        self.world
            .chunks
            .block(pos)
            .is_some_and(rspigot_domain::BlockStateId::is_air)
            && copper_golem::statue_roll(&mut ServerRandom(&mut self.rng))
    }

    pub(crate) fn copper_golem_voice(&mut self, id: EntityId, which: GolemVoice) -> bool {
        let Some(weather) = self
            .entities
            .store
            .get(id)
            .map(|entity| copper_golem::weather_state(&entity.base))
        else {
            return false;
        };
        self.play_mob_voice(id, move |_| Some(voice(weather, which)));
        true
    }

    pub(crate) fn golem_targets_block(&mut self, id: EntityId, pos: BlockPos) {
        let Some(view) = self.entity_view(id) else {
            return;
        };
        let lane = self.lane_for(None, id);
        self.fire(
            lane,
            Continuation::TargetBlock { mob: id, pos },
            move |_| {
                Some(EventPayload::EntityTargetBlock(EntityTargetBlock {
                    entity_type: view.kind.clone(),
                    entity: view,
                    target: Some(pos),
                }))
            },
        );
    }

    pub(crate) fn finish_transport_target(
        &mut self,
        mob: EntityId,
        pos: BlockPos,
        cancelled: bool,
        patch: EntityTargetBlockPatch,
    ) {
        let verdict = match (cancelled, patch.target) {
            (true, _) | (false, Some(None)) => Some(TargetVerdict::Cancel(pos)),
            (false, Some(Some(to))) if to != pos => Some(TargetVerdict::Retarget { from: pos, to }),
            _ => None,
        };
        if let Some(verdict) = verdict
            && let Some(entry) = self.entities.mobs.entries.get_mut(&mob)
        {
            entry.ai.set_transport_verdict(verdict);
        }
    }

    pub(crate) fn copper_golem_after_hurt(&mut self, id: EntityId, hurt: bool) {
        if hurt && let Some(entity) = self.entities.store.get_mut(id) {
            copper_golem::set_golem_state(&mut entity.base, GolemState::Idle);
        }
    }

    pub(crate) fn copper_golem_action(&mut self, id: EntityId, action: CopperGolemAction) {
        match action {
            CopperGolemAction::SetState(state) => {
                if let Some(entity) = self.entities.store.get_mut(id) {
                    copper_golem::set_golem_state(&mut entity.base, state);
                }
            }
            CopperGolemAction::ClearOpenedChest => self.clear_opened_chest(id),
            CopperGolemAction::OpenContainer(pos) => self.golem_start_open(id, pos),
            CopperGolemAction::CloseContainer(pos) => self.golem_stop_open(id, pos),
            CopperGolemAction::SetContainer { pos, slots } => self.set_container_slots(pos, &slots),
            CopperGolemAction::SetMainHand { stack, guaranteed } => {
                self.golem_set_main_hand(id, stack, guaranteed);
            }
            CopperGolemAction::TargetBlock(pos) => self.golem_targets_block(id, pos),
        }
    }

    fn clear_opened_chest(&mut self, id: EntityId) {
        if let Some(entity) = self.entities.store.get_mut(id) {
            with_golem(entity, |_, state| state.opened_chest = None);
        }
    }

    fn golem_backing(&self, pos: BlockPos) -> Option<crate::containers::Backing> {
        let state = self.world.chunks.block(pos)?;
        self.opening_at(pos, state)
            .flatten()
            .map(|open| open.backing)
    }

    fn golem_start_open(&mut self, id: EntityId, pos: BlockPos) {
        let Some(backing) = self.golem_backing(pos) else {
            return;
        };
        let now = self.world.level.game_time;
        let opening = self
            .entities
            .mobs
            .golem_chests
            .entry(backing.pos)
            .or_insert(GolemOpening {
                backing,
                count: 0,
                recheck_at: now + RECHECK_TICKS,
            });
        opening.count += 1;
        if opening.count == 1 {
            opening.recheck_at = now + RECHECK_TICKS;
        }
        let count = self.viewer_count(backing.pos);
        self.viewers_changed(backing, count, true);
        if let Some(entity) = self.entities.store.get_mut(id) {
            with_golem(entity, |_, state| state.opened_chest = Some(pos));
        }
    }

    fn golem_stop_open(&mut self, id: EntityId, pos: BlockPos) {
        let Some(backing) = self.golem_backing(pos) else {
            return;
        };
        let key = backing.pos;
        if let Some(opening) = self.entities.mobs.golem_chests.get_mut(&key) {
            opening.count = (opening.count - 1).max(0);
            if opening.count == 0 {
                self.entities.mobs.golem_chests.remove(&key);
            }
        }
        let count = self.viewer_count(key);
        self.viewers_changed(backing, count, false);
        let _ = id;
    }

    pub(crate) fn tick_golem_chests(&mut self) {
        if self.entities.mobs.golem_chests.is_empty() {
            return;
        }
        let now = self.world.level.game_time;
        let due: Vec<BlockPos> = self
            .entities
            .mobs
            .golem_chests
            .iter()
            .filter(|(_, opening)| opening.recheck_at <= now)
            .map(|(pos, _)| *pos)
            .collect();
        for key in due {
            let Some(opening) = self.entities.mobs.golem_chests.get(&key).copied() else {
                continue;
            };
            let before = self.viewer_count(key);
            let actual = self.golems_with_open(&opening.backing);
            if actual == 0 {
                self.entities.mobs.golem_chests.remove(&key);
            } else if let Some(entry) = self.entities.mobs.golem_chests.get_mut(&key) {
                entry.count = actual;
            }
            let after = self.viewer_count(key);
            if before > 0 && after == 0 {
                self.viewers_changed(opening.backing, 0, false);
            }
            if after > 0
                && let Some(entry) = self.entities.mobs.golem_chests.get_mut(&key)
            {
                entry.recheck_at = now + RECHECK_TICKS;
            }
        }
    }

    fn golems_with_open(&self, backing: &crate::containers::Backing) -> i32 {
        let count = self
            .entities
            .store
            .iter()
            .filter(|entity| entity.base.kind == Kind::CopperGolem)
            .filter_map(golem_state_of)
            .filter_map(|state| state.opened_chest)
            .filter(|opened| backing.includes(*opened))
            .count();
        i32::try_from(count).unwrap_or(i32::MAX)
    }

    fn golem_set_main_hand(&mut self, id: EntityId, stack: Slot, guaranteed: bool) {
        let Some(living) = self
            .entities
            .store
            .get_mut(id)
            .and_then(|entity| entity.living_mut())
        else {
            return;
        };
        living.equipment[slot_index(EquipmentSlot::Mainhand)] = stack.clone();
        if guaranteed && let Some(mob) = living.mob.as_mut() {
            mob.drop_chances[slot_index(EquipmentSlot::Mainhand)] = GUARANTEED_DROP;
        }
        self.push(
            Audience::Trackers(id),
            ClientEvent::EntityEquipment {
                entity: id,
                slots: vec![(WireSlot::MainHand, stack)],
            },
        );
    }

    fn set_container_slots(&mut self, pos: BlockPos, slots: &[(usize, Slot)]) {
        let Some(backing) = self.golem_backing(pos) else {
            return;
        };
        let mut items = self.backing_items(backing);
        for (index, stack) in slots {
            if let Some(slot) = items.get_mut(*index) {
                *slot = stack.clone();
            }
        }
        self.write_backing_items(backing, &items);
    }

    pub(crate) fn copper_golem_interact(
        &mut self,
        player: PlayerId,
        target: EntityId,
        hand: Hand,
        held: Option<&ItemStack>,
    ) -> Option<Interaction> {
        let Some(stack) = held else {
            return self.golem_gives_back_item(player, target);
        };
        let waxed = self
            .entities
            .store
            .get(target)
            .and_then(golem_state_of)
            .is_some_and(|state| copper_golem::is_waxed(&state));
        let weather = self
            .entities
            .store
            .get(target)
            .map(|entity| copper_golem::weather_state(&entity.base))?;
        let shears = stack.item == ItemKind::Shears.id();
        if shears
            && self
                .entities
                .store
                .get(target)
                .is_some_and(ready_for_shearing)
        {
            return Some(self.ask_shear(player, target, hand));
        }
        if stack.item == ItemKind::Honeycomb.id() && !waxed {
            self.wax_golem(target);
            self.consume_held(player, hand, 1);
            return Some(Interaction::ServerOnly);
        }
        if item_tag(stack.item, "minecraft:axes") && (waxed || weather != WeatherState::Unaffected)
        {
            self.scrape_golem(target);
            self.hurt_held(player, hand, 1);
            return Some(Interaction::ServerOnly);
        }
        None
    }

    fn golem_gives_back_item(&mut self, player: PlayerId, target: EntityId) -> Option<Interaction> {
        let hand_item = self
            .entities
            .store
            .get(target)?
            .living()?
            .equipment(EquipmentSlot::Mainhand)
            .cloned()?;
        let to = self.players.get(&player)?.position;
        self.throw_stack(target, hand_item, to);
        self.golem_set_main_hand(target, None, false);
        Some(Interaction::Predicted)
    }

    fn throw_stack(&mut self, thrower: EntityId, stack: ItemStack, target: Vec3) {
        let Some(entity) = self.entities.store.get(thrower) else {
            return;
        };
        let position = entity.base.position;
        let eye = entity.base.eye_y();
        let origin = Vec3::new(position.x, eye - THROW_HEIGHT_OFFSET, position.z);
        let velocity =
            (target - position)
                .normalize()
                .multiply(THROW_SPEED, THROW_SPEED, THROW_SPEED);
        let Some(view) = self.entity_view(thrower) else {
            return;
        };
        let item = self.item_entity(origin, velocity, THROW_PICKUP_DELAY, stack);
        self.drop_from(view, item);
    }

    fn wax_golem(&mut self, target: EntityId) {
        let Some(pos) = self
            .entities
            .store
            .get(target)
            .map(|entity| entity.base.position.block())
        else {
            return;
        };
        self.level_event(copper_golem::WAX_ON_EVENT, pos, 0);
        self.play_sound(
            None,
            pos.center(),
            WAX_ON_SOUND,
            rspigot_entities::SoundSource::Blocks,
            1.0,
            1.0,
        );
        if let Some(entity) = self.entities.store.get_mut(target) {
            with_golem(entity, |_, state| copper_golem::wax(state));
        }
    }

    fn scrape_golem(&mut self, target: EntityId) {
        let Some(entity) = self.entities.store.get(target) else {
            return;
        };
        let pos = entity.base.position.block();
        self.entity_sound(target, SCRAPE_SOUND, 1.0, 1.0);
        let event = self
            .entities
            .store
            .get_mut(target)
            .and_then(|entity| with_golem(entity, copper_golem::scrape))
            .flatten();
        if let Some(event) = event {
            self.level_event(event, pos, 0);
        }
    }

    pub(crate) fn shear_copper_golem(&mut self, player: PlayerId, target: EntityId, hand: Hand) {
        self.entity_sound(target, SHEAR_SOUND, 1.0, 1.0);
        let Some(living) = self
            .entities
            .store
            .get_mut(target)
            .and_then(|entity| entity.living_mut())
        else {
            return;
        };
        let antenna = living.equipment[slot_index(EquipmentSlot::Saddle)].take();
        self.push(
            Audience::Trackers(target),
            ClientEvent::EntityEquipment {
                entity: target,
                slots: vec![(WireSlot::Saddle, None)],
            },
        );
        if let Some(stack) = antenna {
            self.drop_at_location(target, ANTENNA_DROP_OFFSET, stack, Vec3::ZERO);
        }
        if let Some(position) = self
            .entities
            .store
            .get(target)
            .map(|entity| entity.base.position)
        {
            self.game_event(
                rspigot_domain::game_event::GameEvent::Shear,
                position,
                crate::game_events::GameEventContext::of(self.player_entity(player)),
            );
        }
        self.hurt_held(player, hand, 1);
    }

    fn player_entity(&self, player: PlayerId) -> Option<EntityId> {
        self.players.get(&player).map(|player| player.entity_id)
    }

    pub(crate) fn drop_preserved_equipment(&mut self, id: EntityId) {
        let Some((position, preserved)) = self.entities.store.get_mut(id).and_then(|entity| {
            let position = entity.base.position;
            let living = entity.living_mut()?;
            let chances = living.mob.as_ref()?.drop_chances;
            let mut dropped = Vec::new();
            for slot in rspigot_domain::entity::living::EQUIPMENT_SLOTS {
                let index = slot_index(slot);
                if chances[index] > PRESERVED_CHANCE
                    && let Some(stack) = living.equipment[index].take()
                {
                    dropped.push(stack);
                }
            }
            Some((position, dropped))
        }) else {
            return;
        };
        for stack in preserved {
            self.spawn_at_location(position, 0.0, stack);
        }
    }

    pub(crate) fn play_become_statue_sound(&mut self, id: EntityId) {
        self.entity_sound(id, BECOME_STATUE_SOUND, 1.0, 1.0);
    }
}

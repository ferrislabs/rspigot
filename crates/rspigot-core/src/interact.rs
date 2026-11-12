use std::num::NonZeroU8;

use rspigot_domain::entity::ageable;
use rspigot_domain::entity::living::{self, slot_index};
use rspigot_domain::entity::reward::max_damage;
use rspigot_domain::entity::species;
use rspigot_domain::entity::{Aabb, Entity, EntityState, save};
use rspigot_domain::item::{HOTBAR_START, ItemStack, OFFHAND_SLOT, same_item_same_components};
use rspigot_domain::player::{GameMode, PlayerId};
use rspigot_domain::random::RandomSource;
use rspigot_domain::spawn::SpawnReason;
use rspigot_domain::text::Component;
use rspigot_domain::{BlockPos, BlockStateId, EntityId, ItemId, Vec3};
use rspigot_entities::{EntityKind as Kind, SoundSource};
use rspigot_items::{EquipmentSlot, ItemKind};

use crate::events::{Animation, ClientEvent, Direction, EquipmentSlot as WireSlot, Hand};
use crate::loot::{self, LootContext};
use crate::mob_actions::new_mob;
use crate::plugins::events::{
    CreatureSpawnReason, DyeColor, EntityEnterLoveMode, EquipmentSlot as Slot, PlayerShearEntity,
    SheepDyeWool, SheepDyeWoolPatch,
};
use crate::plugins::steps::{DyeStep, LoveStep, ShearStep};
use crate::plugins::{Birth, Continuation, EventPayload, Lane, Spawned, spec_of};
use crate::ports::ClientLink;
use crate::server::{Audience, Server};

const ENTITY_REACH: f64 = 3.0;
const CREATIVE_REACH_BONUS: f64 = 2.0;
const REACH_BUFFER: f64 = 3.0;
const CUSTOM_NAME: &str = "minecraft:custom_name";
const SHEAR_TABLE: &str = "minecraft:shearing/sheep";
const DYES: [&str; 16] = [
    "minecraft:white_dye",
    "minecraft:orange_dye",
    "minecraft:magenta_dye",
    "minecraft:light_blue_dye",
    "minecraft:yellow_dye",
    "minecraft:lime_dye",
    "minecraft:pink_dye",
    "minecraft:gray_dye",
    "minecraft:light_gray_dye",
    "minecraft:cyan_dye",
    "minecraft:purple_dye",
    "minecraft:blue_dye",
    "minecraft:brown_dye",
    "minecraft:green_dye",
    "minecraft:red_dye",
    "minecraft:black_dye",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Interaction {
    Pass,
    Predicted,
    ServerOnly,
    Consume,
}

impl Interaction {
    pub(crate) fn consumes(self) -> bool {
        self != Self::Pass
    }
}

pub(crate) fn item_tag(item: ItemId, tag: &str) -> bool {
    rspigot_data::tags::tag("minecraft:item", tag)
        .is_some_and(|ids| ids.binary_search(&u32::from(item.0)).is_ok())
}

fn food_tag(kind: Kind) -> Option<&'static str> {
    Some(match kind {
        Kind::Cow => "minecraft:cow_food",
        Kind::Pig => "minecraft:pig_food",
        Kind::Sheep => "minecraft:sheep_food",
        Kind::Chicken => "minecraft:chicken_food",
        Kind::Fox => "minecraft:fox_food",
        Kind::Wolf => "minecraft:wolf_food",
        Kind::Cat => "minecraft:cat_food",
        _ => return None,
    })
}

pub(crate) fn dye_color(item: ItemId) -> Option<u8> {
    let name = item.kind()?.name();
    DYES.iter()
        .position(|dye| *dye == name)
        .map(|index| index as u8)
}

const DYE_COLORS: [DyeColor; 16] = [
    DyeColor::White,
    DyeColor::Orange,
    DyeColor::Magenta,
    DyeColor::LightBlue,
    DyeColor::Yellow,
    DyeColor::Lime,
    DyeColor::Pink,
    DyeColor::Gray,
    DyeColor::LightGray,
    DyeColor::Cyan,
    DyeColor::Purple,
    DyeColor::Blue,
    DyeColor::Brown,
    DyeColor::Green,
    DyeColor::Red,
    DyeColor::Black,
];

fn dye_of(color: u8) -> DyeColor {
    DYE_COLORS
        .get(usize::from(color))
        .copied()
        .unwrap_or(DyeColor::White)
}

fn wool_of(color: DyeColor) -> u8 {
    DYE_COLORS
        .iter()
        .position(|dye| *dye == color)
        .and_then(|index| u8::try_from(index).ok())
        .unwrap_or(0)
}

fn hand_slot(hand: Hand, selected: u8) -> usize {
    match hand {
        Hand::Main => HOTBAR_START + usize::from(selected),
        Hand::Off => OFFHAND_SLOT,
    }
}

fn box_distance_squared(bounds: &Aabb, point: Vec3) -> f64 {
    let dx = (bounds.min_x - point.x)
        .max(point.x - bounds.max_x)
        .max(0.0);
    let dy = (bounds.min_y - point.y)
        .max(point.y - bounds.max_y)
        .max(0.0);
    let dz = (bounds.min_z - point.z)
        .max(point.z - bounds.max_z)
        .max(0.0);
    dx * dx + dy * dy + dz * dz
}

impl<L: ClientLink> Server<L> {
    pub(crate) fn handle_interact(
        &mut self,
        id: PlayerId,
        target: EntityId,
        hand: Hand,
        sneaking: bool,
        location: Vec3,
    ) {
        let Some(player) = self.players.get(&id) else {
            return;
        };
        if player.vitals.is_dead() || !player.vitals.client_loaded() {
            return;
        }
        if target == player.entity_id && player.game_mode != GameMode::Spectator {
            self.disconnect_player(id, Component::text("Cannot interact with self!"));
            return;
        }
        if player.sneaking != sneaking {
            if let Some(player) = self.players.get_mut(&id) {
                player.sneaking = sneaking;
            }
            self.broadcast_metadata(id);
        }
        if !self.within_entity_reach(id, target) {
            return;
        }
        self.ask_interact_entity(id, target, hand, location);
    }

    fn within_entity_reach(&self, id: PlayerId, target: EntityId) -> bool {
        let Some(bounds) = self
            .entities
            .store
            .get(target)
            .map(|e| e.base.bounding_box())
        else {
            return false;
        };
        let Some(player) = self.players.get(&id) else {
            return false;
        };
        let reach = ENTITY_REACH
            + if player.game_mode == GameMode::Creative {
                CREATIVE_REACH_BONUS
            } else {
                0.0
            }
            + REACH_BUFFER;
        box_distance_squared(&bounds, player.eye_position()) < reach * reach
    }

    pub(crate) fn may_interact_with(&self, id: PlayerId, target: EntityId) -> bool {
        let alive = self
            .players
            .get(&id)
            .is_some_and(|player| !player.vitals.is_dead());
        let present = self.entities.store.get(target).is_some_and(|entity| {
            !entity.base.is_removed()
                && entity
                    .living()
                    .is_none_or(|living| !living::is_dead_or_dying(&entity.base, living))
        });
        alive && present && self.within_entity_reach(id, target)
    }

    pub(crate) fn interact_now(&mut self, id: PlayerId, target: EntityId, hand: Hand) {
        let result = self.interact_on(id, target, hand);
        if matches!(result, Interaction::Predicted | Interaction::ServerOnly) {
            self.player_swing(id, hand, result == Interaction::ServerOnly);
        }
    }

    fn player_swing(&mut self, id: PlayerId, hand: Hand, to_self: bool) {
        let Some(player) = self.players.get_mut(&id) else {
            return;
        };
        player.vitals.attack_ticker = 0;
        let entity = player.entity_id;
        let animation = match hand {
            Hand::Main => Animation::SwingMainArm,
            Hand::Off => Animation::SwingOffHand,
        };
        let event = ClientEvent::EntityAnimation { entity, animation };
        if to_self {
            self.push(Audience::One(id), event.clone());
        }
        self.push(Audience::Trackers(entity), event);
    }

    pub(crate) fn held(&self, id: PlayerId, hand: Hand) -> Option<ItemStack> {
        let player = self.players.get(&id)?;
        player
            .inventory
            .get(hand_slot(hand, player.inventory.selected_hotbar))
            .cloned()
    }

    pub(crate) fn set_held(&mut self, id: PlayerId, hand: Hand, stack: Option<ItemStack>) {
        if let Some(player) = self.players.get_mut(&id) {
            let slot = hand_slot(hand, player.inventory.selected_hotbar);
            player.inventory.set(slot, stack);
        }
    }

    pub(crate) fn infinite_materials(&self, id: PlayerId) -> bool {
        self.players
            .get(&id)
            .is_some_and(|p| p.abilities().instant_build)
    }

    pub(crate) fn shrink_held(&mut self, id: PlayerId, hand: Hand, amount: u8) {
        let Some(stack) = self.held(id, hand) else {
            return;
        };
        let remaining = stack.count.get().saturating_sub(amount);
        self.set_held(
            id,
            hand,
            NonZeroU8::new(remaining).map(|count| ItemStack { count, ..stack }),
        );
    }

    pub(crate) fn consume_held(&mut self, id: PlayerId, hand: Hand, amount: u8) {
        if !self.infinite_materials(id) {
            self.shrink_held(id, hand, amount);
        }
    }

    pub(crate) fn hurt_held(&mut self, id: PlayerId, hand: Hand, amount: i32) {
        self.wear(id, hand, amount);
    }

    pub(crate) fn refresh_hands(&mut self, id: PlayerId) {
        self.sync_inventory(id);
        self.broadcast_held_item(id);
    }

    pub(crate) fn interact_on(
        &mut self,
        id: PlayerId,
        target: EntityId,
        hand: Hand,
    ) -> Interaction {
        let Some(player) = self.players.get(&id) else {
            return Interaction::Pass;
        };
        if player.game_mode == GameMode::Spectator {
            return Interaction::Pass;
        }
        let before = self.held(id, hand);
        let creative = self.infinite_materials(id);
        let result = self.mob_interact_entry(id, target, hand);
        if result.consumes() {
            if creative
                && let (Some(before), Some(now)) = (&before, self.held(id, hand))
                && now.item == before.item
                && now.count < before.count
            {
                self.set_held(id, hand, Some(before.clone()));
            }
            self.refresh_hands(id);
            return result;
        }
        let Some(stack) = before else {
            return Interaction::Pass;
        };
        let result = self.item_interact(id, target, hand, &stack);
        if result.consumes() {
            if creative {
                self.set_held(id, hand, Some(stack));
            }
            self.refresh_hands(id);
        }
        result
    }

    fn mob_interact_entry(&mut self, id: PlayerId, target: EntityId, hand: Hand) -> Interaction {
        let Some(entity) = self.entities.store.get(target) else {
            return Interaction::Pass;
        };
        let alive = entity
            .living()
            .is_some_and(|l| l.is_mob() && !living::is_dead_or_dying(&entity.base, l))
            && !entity.base.is_removed();
        if !alive {
            return Interaction::Pass;
        }
        let held = self.held(id, hand);
        if let Some(stack) = &held {
            if stack.item == ItemKind::NameTag.id() {
                let result = self.name_tag(id, target, hand, stack);
                if result.consumes() {
                    return result;
                }
            }
            if let Some(kind) = Kind::from_spawn_egg(stack.item.0) {
                let parent = self.entities.store.get(target).map(|e| e.base.kind);
                if Some(kind) != parent || !self.spawn_offspring_from_egg(id, target, hand, stack) {
                    return Interaction::Pass;
                }
                return Interaction::ServerOnly;
            }
            if stack.item == ItemKind::Shears.id()
                && !self.players.get(&id).is_some_and(|p| p.sneaking)
            {
                let saddled = self
                    .entities
                    .store
                    .get(target)
                    .and_then(|e| e.living())
                    .and_then(|l| l.equipment(EquipmentSlot::Saddle))
                    .is_some();
                if saddled {
                    self.shear_saddle(id, target, hand);
                    return Interaction::Predicted;
                }
            }
        }
        self.mob_interact(id, target, hand, held)
    }

    fn name_tag(
        &mut self,
        id: PlayerId,
        target: EntityId,
        hand: Hand,
        stack: &ItemStack,
    ) -> Interaction {
        let Some(name) = stack
            .components
            .get(CUSTOM_NAME)
            .and_then(save::component_from_tag)
        else {
            return Interaction::Pass;
        };
        let Some(entity) = self.entities.store.get_mut(target) else {
            return Interaction::Pass;
        };
        if !entity.base.kind.can_serialize() {
            return Interaction::Pass;
        }
        entity.base.set_custom_name(Some(name));
        if let Some(mob) = entity.living_mut().and_then(|l| l.mob.as_mut()) {
            mob.persistence_required = true;
        }
        self.shrink_held(id, hand, 1);
        Interaction::Predicted
    }

    fn spawn_offspring_from_egg(
        &mut self,
        id: PlayerId,
        parent: EntityId,
        hand: Hand,
        stack: &ItemStack,
    ) -> bool {
        let Some(entity) = self.entities.store.get(parent) else {
            return false;
        };
        let kind = entity.base.kind;
        let position = entity.base.position;
        let mut random = self.mob_random(parent);
        let child = if kind.extends("AgeableMob") {
            self.breed_offspring(parent, parent, &mut random)
        } else {
            Some(new_mob(self.new_entity_base(kind)))
        };
        self.store_mob_random(parent, random);
        let Some(mut child) = child else {
            return false;
        };
        if kind == Kind::Fox
            && let Some(player) = self.players.get(&id)
        {
            species::fox::add_trusted(&mut child.base, player.profile.id.0);
        }
        let EntityState::Living(state) = &mut child.state else {
            return false;
        };
        let baby = if let Some(ageable) = state.mob.as_mut().and_then(|m| m.ageable.as_mut()) {
            ageable::set_baby(&mut child.base, ageable, true);
            ageable::is_baby(&child.base, ageable)
        } else if species::is_zombie(kind) {
            species::set_zombie_baby(&mut child.base, &mut state.attributes, true);
            true
        } else {
            false
        };
        if !baby {
            return false;
        }
        child.base.snap_to(position, 0.0, 0.0);
        if let Some(name) = stack
            .components
            .get(CUSTOM_NAME)
            .and_then(save::component_from_tag)
        {
            child.base.set_custom_name(Some(name));
        }
        if species::is_zombie(kind) {
            let special = self.difficulty_at(position.block()).special_multiplier();
            let mut random = self.mob_random(parent);
            let loot = random.next_float() < 0.55 * special;
            self.store_mob_random(parent, random);
            if let Some(mob) = child.living_mut().and_then(|l| l.mob.as_mut()) {
                mob.can_pick_up_loot = loot;
            }
        }
        let Some(slot) = self
            .players
            .get(&id)
            .map(|player| hand_slot(hand, player.inventory.selected_hotbar))
        else {
            return false;
        };
        self.spawn_entity(
            child,
            Birth::of(CreatureSpawnReason::SpawnerEgg)
                .by(id)
                .then(Spawned::Hatched {
                    player: id,
                    slot,
                    item: stack.clone(),
                }),
        );
        true
    }

    pub(crate) fn spawn_from_egg_on_block(
        &mut self,
        id: PlayerId,
        slot: usize,
        stack: &ItemStack,
        pos: BlockPos,
        face: Direction,
        clicked: BlockStateId,
    ) -> bool {
        let Some(kind) = Kind::from_spawn_egg(stack.item.0) else {
            return false;
        };
        let passable = clicked.collision_shape().is_empty();
        let target = if passable {
            pos
        } else {
            let (dx, dy, dz) = face.offset();
            pos.offset(dx, dy, dz)
        };
        if !self.within_reach(id, pos) || self.is_protected(id, target) {
            return false;
        }
        let lift = if !passable && face == Direction::Up {
            let top = clicked
                .collision_shape()
                .iter()
                .map(|shape| shape.max_y)
                .fold(0.0, f64::max);
            (top - 1.0).min(0.0)
        } else {
            0.0
        };
        let position = Vec3::new(
            f64::from(target.x) + 0.5,
            f64::from(target.y) + lift,
            f64::from(target.z) + 0.5,
        );
        let Ok((mut entity, random)) =
            self.summon_entity(kind, position, None, SpawnReason::SpawnItemUse)
        else {
            return false;
        };
        if let Some(name) = stack
            .components
            .get(CUSTOM_NAME)
            .and_then(save::component_from_tag)
        {
            entity.base.set_custom_name(Some(name));
        }
        self.spawn_entity(
            entity,
            Birth::of(CreatureSpawnReason::SpawnerEgg)
                .with_random(random)
                .by(id)
                .then(Spawned::Hatched {
                    player: id,
                    slot,
                    item: stack.clone(),
                }),
        );
        true
    }

    pub(crate) fn use_spawn_egg(&mut self, id: PlayerId, slot: usize, egg: &ItemStack) {
        if self.infinite_materials(id) {
            return;
        }
        let Some(player) = self.players.get_mut(&id) else {
            return;
        };
        let Some(held) = player.inventory.get(slot) else {
            return;
        };
        if !same_item_same_components(held, egg) {
            return;
        }
        let remaining = held.count.get() - 1;
        let left = NonZeroU8::new(remaining).map(|count| ItemStack {
            count,
            ..held.clone()
        });
        player.inventory.set(slot, left);
    }

    fn shear_saddle(&mut self, id: PlayerId, target: EntityId, hand: Hand) {
        self.hurt_held(id, hand, 1);
        let Some(entity) = self.entities.store.get_mut(target) else {
            return;
        };
        let height = f64::from(entity.base.dimensions.height);
        let Some(living) = entity.living_mut() else {
            return;
        };
        let Some(saddle) = living.equipment[slot_index(EquipmentSlot::Saddle)].take() else {
            return;
        };
        self.push(
            Audience::Trackers(target),
            ClientEvent::EntityEquipment {
                entity: target,
                slots: vec![(WireSlot::Saddle, None)],
            },
        );
        self.drop_at_location(target, height, saddle, Vec3::ZERO);
        self.entity_sound(target, "minecraft:item.saddle.unequip", 1.0, 1.0);
    }

    fn mob_interact(
        &mut self,
        id: PlayerId,
        target: EntityId,
        hand: Hand,
        held: Option<ItemStack>,
    ) -> Interaction {
        let Some(kind) = self.entities.store.get(target).map(|e| e.base.kind) else {
            return Interaction::Pass;
        };
        if let Some(result) = self.kind_interact(id, target, hand, held.as_ref()) {
            return result;
        }
        let Some(stack) = held else {
            return Interaction::Pass;
        };
        if kind == Kind::Creeper && item_tag(stack.item, "minecraft:creeper_igniters") {
            return self.ignite_creeper(id, target, hand, &stack);
        }
        if kind == Kind::Cow && stack.item == ItemKind::Bucket.id() {
            let baby = self.entities.store.get(target).is_some_and(Entity::is_baby);
            if !baby {
                return self.milk_cow(id, target, hand);
            }
        }
        if kind == Kind::Sheep && stack.item == ItemKind::Shears.id() {
            return self.shear_sheep(id, target, hand);
        }
        let result = self.animal_interact(id, target, hand, &stack);
        if result.consumes() || kind != Kind::Pig {
            return result;
        }
        match self.held(id, hand) {
            Some(stack) if stack.item == ItemKind::Saddle.id() => self.saddle_pig(id, target, hand),
            _ => Interaction::Pass,
        }
    }

    fn ignite_creeper(
        &mut self,
        id: PlayerId,
        target: EntityId,
        hand: Hand,
        stack: &ItemStack,
    ) -> Interaction {
        let sound = if stack.item == ItemKind::FireCharge.id() {
            "minecraft:item.firecharge.use"
        } else {
            "minecraft:item.flintandsteel.use"
        };
        let mut random = self.mob_random(target);
        let pitch = random.next_float() * 0.4 + 0.8;
        self.store_mob_random(target, random);
        if let Some(position) = self.entity_position(target) {
            self.play_sound(Some(id), position, sound, SoundSource::Hostile, 1.0, pitch);
        }
        if let Some(entity) = self.entities.store.get_mut(target) {
            species::ignite(&mut entity.base);
        }
        if max_damage(stack).is_none() {
            self.shrink_held(id, hand, 1);
        } else {
            self.hurt_held(id, hand, 1);
        }
        Interaction::Predicted
    }

    fn milk_cow(&mut self, id: PlayerId, target: EntityId, hand: Hand) -> Interaction {
        let Some(pos) = self.entity_position(target).map(Vec3::block) else {
            return Interaction::Pass;
        };
        if self.is_protected(id, pos) {
            return Interaction::Pass;
        }
        if let Some(position) = self.players.get(&id).map(|p| p.position) {
            self.play_sound(
                Some(id),
                position,
                "minecraft:entity.cow.milk",
                SoundSource::Players,
                1.0,
                1.0,
            );
        }
        let milk = ItemStack::new(ItemKind::MilkBucket.id(), NonZeroU8::MIN);
        if self.infinite_materials(id) {
            let has = self.players.get(&id).is_some_and(|p| {
                p.inventory
                    .slots()
                    .iter()
                    .flatten()
                    .any(|s| s.item == milk.item)
            });
            if !has && let Some(player) = self.players.get_mut(&id) {
                player
                    .inventory
                    .insert(milk.item, 1, milk.item.max_stack_size());
            }
            return Interaction::Predicted;
        }
        self.shrink_held(id, hand, 1);
        if self.held(id, hand).is_none() {
            self.set_held(id, hand, Some(milk));
            return Interaction::Predicted;
        }
        let added = self.players.get_mut(&id).is_some_and(|p| {
            p.inventory
                .insert(milk.item, 1, milk.item.max_stack_size())
                .remaining
                == 0
        });
        if !added && let Some(position) = self.players.get(&id).map(|p| p.eye_position()) {
            self.spawn_at_location(position, -0.3, milk);
        }
        Interaction::Predicted
    }

    fn shear_sheep(&mut self, id: PlayerId, target: EntityId, hand: Hand) -> Interaction {
        let Some(entity) = self.entities.store.get(target) else {
            return Interaction::Pass;
        };
        if species::is_sheared(&entity.base) || entity.is_baby() {
            return Interaction::Consume;
        }
        let Some(stack) = self.held(id, hand) else {
            return Interaction::Pass;
        };
        let item = spec_of(&stack);
        let mut then = Continuation::Shear(Box::new(ShearStep {
            player: id,
            target,
            hand,
            stack,
            parked: false,
        }));
        let parked = self.parks(Lane::Player(id), &then);
        if let Continuation::Shear(step) = &mut then {
            step.parked = parked;
        }
        self.fire(id, then, move |server| {
            Some(EventPayload::PlayerShearEntity(PlayerShearEntity {
                entity: server.entity_view(target)?,
                hand: match hand {
                    Hand::Main => Slot::Hand,
                    Hand::Off => Slot::OffHand,
                },
                item,
                player: server.snapshot_of(id)?,
            }))
        });
        if parked {
            Interaction::Consume
        } else {
            Interaction::ServerOnly
        }
    }

    pub(crate) fn finish_shear(&mut self, step: ShearStep) {
        let ready = self.entities.store.get(step.target).is_some_and(|entity| {
            !entity.base.is_removed()
                && !species::is_sheared(&entity.base)
                && !entity.is_baby()
                && entity
                    .living()
                    .is_some_and(|living| !living::is_dead_or_dying(&entity.base, living))
        });
        let able = self
            .players
            .get(&step.player)
            .is_some_and(|player| !player.vitals.is_dead());
        if !ready || !able || self.held(step.player, step.hand).as_ref() != Some(&step.stack) {
            return;
        }
        self.shear_now(step.player, step.target, step.hand);
        if step.parked {
            self.player_swing(step.player, step.hand, true);
            self.refresh_hands(step.player);
        }
    }

    fn shear_now(&mut self, id: PlayerId, target: EntityId, hand: Hand) {
        let Some(entity) = self.entities.store.get(target) else {
            return;
        };
        let position = entity.base.position;
        let facts = crate::mobs::loot_facts_of(entity);
        self.play_sound(
            None,
            position,
            "minecraft:entity.sheep.shear",
            SoundSource::Players,
            1.0,
            1.0,
        );
        let drops = rspigot_data::loot::table(SHEAR_TABLE).map_or_else(Vec::new, |table| {
            let context = LootContext {
                this: Some(&facts),
                ..LootContext::default()
            };
            loot::table_drops(table, &context, &mut self.rng)
        });
        for stack in drops {
            for _ in 0..stack.count.get() {
                let single = ItemStack {
                    count: NonZeroU8::MIN,
                    ..stack.clone()
                };
                let mut random = self.mob_random(target);
                let push = Vec3::new(
                    f64::from((random.next_float() - random.next_float()) * 0.1),
                    f64::from(random.next_float() * 0.05),
                    f64::from((random.next_float() - random.next_float()) * 0.1),
                );
                self.store_mob_random(target, random);
                self.drop_at_location(target, 1.0, single, push);
            }
        }
        if let Some(entity) = self.entities.store.get_mut(target) {
            species::set_sheared(&mut entity.base, true);
        }
        self.hurt_held(id, hand, 1);
    }

    pub(crate) fn animal_interact(
        &mut self,
        id: PlayerId,
        target: EntityId,
        hand: Hand,
        stack: &ItemStack,
    ) -> Interaction {
        let Some(entity) = self.entities.store.get(target) else {
            return Interaction::Pass;
        };
        let kind = entity.base.kind;
        let Some(ageable) = entity
            .living()
            .and_then(|l| l.mob.as_ref())
            .and_then(|m| m.ageable.as_ref())
        else {
            return Interaction::Pass;
        };
        let age = ageable.age;
        let can_love = ageable.can_fall_in_love();
        let can_age_up = ageable::can_age_up(&entity.base, ageable);
        let is_food = food_tag(kind).is_some_and(|tag| item_tag(stack.item, tag));
        let uuid = self.players.get(&id).map(|p| p.profile.id.0);
        if is_food {
            if age == 0 && can_love {
                self.consume_held(id, hand, 1);
                let item = self.held(id, Hand::Main);
                self.ask_love(id, target, uuid, item);
                self.eating_sound(target);
                return Interaction::ServerOnly;
            }
            if can_age_up {
                self.consume_held(id, hand, 1);
                if let Some(Entity {
                    base,
                    state: EntityState::Living(state),
                    ..
                }) = self.entities.store.get_mut(target)
                    && let Some(ageable) = state.mob.as_mut().and_then(|m| m.ageable.as_mut())
                {
                    let seconds = ageable::speed_up_seconds_when_feeding(-age);
                    ageable::age_up(base, ageable, seconds, true);
                }
                self.eating_sound(target);
                return Interaction::Predicted;
            }
        }
        if stack.item == ItemKind::GoldenDandelion.id() {
            return self.golden_dandelion(id, target, hand);
        }
        Interaction::Pass
    }

    fn golden_dandelion(&mut self, id: PlayerId, target: EntityId, hand: Hand) -> Interaction {
        let Some(Entity {
            base,
            state: EntityState::Living(state),
            ..
        }) = self.entities.store.get_mut(target)
        else {
            return Interaction::Pass;
        };
        let Some(mob) = state.mob.as_mut() else {
            return Interaction::Pass;
        };
        let Some(ageable) = mob.ageable.as_mut() else {
            return Interaction::Pass;
        };
        if !ageable::is_baby(base, ageable)
            || ageable.age_lock_timer != 0
            || crate::mob_life::type_in_tag(base.kind, "minecraft:cannot_be_age_locked")
        {
            return Interaction::Pass;
        }
        let locked = ageable::toggle_age_lock(base, ageable);
        if locked {
            mob.persistence_required = true;
        }
        let pos = base.position.block();
        self.consume_held(id, hand, 1);
        let sound = if locked {
            "minecraft:item.golden_dandelion.use"
        } else {
            "minecraft:item.golden_dandelion.unuse"
        };
        let at = Vec3::new(
            f64::from(pos.x) + 0.5,
            f64::from(pos.y) + 0.5,
            f64::from(pos.z) + 0.5,
        );
        self.play_sound(None, at, sound, SoundSource::Players, 1.0, 1.0);
        Interaction::Predicted
    }

    fn eating_sound(&mut self, target: EntityId) {
        if self
            .entities
            .store
            .get(target)
            .is_some_and(|e| e.base.kind == Kind::Fox)
        {
            self.entity_sound(target, "minecraft:entity.fox.eat", 1.0, 1.0);
            return;
        }
        let is_pig = self
            .entities
            .store
            .get(target)
            .is_some_and(|e| e.base.kind == Kind::Pig);
        if is_pig {
            self.play_mob_voice(target, |entity| {
                crate::mob_life::variant_sound(entity, |set| set.eat).flatten()
            });
        }
    }

    fn saddle_pig(&mut self, id: PlayerId, target: EntityId, hand: Hand) -> Interaction {
        let Some(entity) = self.entities.store.get_mut(target) else {
            return Interaction::Pass;
        };
        let baby = entity.is_baby();
        let Some(living) = entity.living_mut() else {
            return Interaction::Pass;
        };
        if baby || living.equipment(EquipmentSlot::Saddle).is_some() {
            return Interaction::Pass;
        }
        let saddle = ItemStack::new(ItemKind::Saddle.id(), NonZeroU8::MIN);
        living.equipment[slot_index(EquipmentSlot::Saddle)] = Some(saddle.clone());
        if let Some(mob) = living.mob.as_mut() {
            mob.drop_chances[slot_index(EquipmentSlot::Saddle)] = 2.0;
        }
        self.shrink_held(id, hand, 1);
        self.push(
            Audience::Trackers(target),
            ClientEvent::EntityEquipment {
                entity: target,
                slots: vec![(WireSlot::Saddle, Some(saddle))],
            },
        );
        self.entity_sound(target, "minecraft:entity.pig.saddle", 1.0, 1.0);
        Interaction::Predicted
    }

    fn item_interact(
        &mut self,
        id: PlayerId,
        target: EntityId,
        hand: Hand,
        stack: &ItemStack,
    ) -> Interaction {
        let Some(color) = dye_color(stack.item) else {
            return Interaction::Pass;
        };
        let Some(entity) = self.entities.store.get_mut(target) else {
            return Interaction::Pass;
        };
        let alive = entity
            .living()
            .is_some_and(|l| !living::is_dead_or_dying(&entity.base, l));
        if entity.base.kind != Kind::Sheep
            || !alive
            || species::is_sheared(&entity.base)
            || species::wool_color(&entity.base) == color
        {
            return Interaction::Pass;
        }
        let mut then = Continuation::Dye(Box::new(DyeStep {
            player: id,
            target,
            hand,
            color,
            stack: stack.clone(),
            parked: false,
        }));
        let parked = self.parks(Lane::Player(id), &then);
        if let Continuation::Dye(step) = &mut then {
            step.parked = parked;
        }
        self.fire(id, then, move |server| {
            Some(EventPayload::SheepDyeWool(SheepDyeWool {
                color: dye_of(color),
                entity: server.entity_view(target)?,
                entity_type: Kind::Sheep.name().to_owned(),
                player: server.snapshot_of(id),
            }))
        });
        if parked {
            Interaction::Consume
        } else {
            Interaction::Predicted
        }
    }

    pub(crate) fn finish_dye(&mut self, step: DyeStep, cancelled: bool, patch: SheepDyeWoolPatch) {
        let DyeStep {
            player: id,
            target,
            hand,
            color,
            stack,
            parked,
        } = step;
        let ready = self.entities.store.get(target).is_some_and(|entity| {
            entity.base.kind == Kind::Sheep
                && !species::is_sheared(&entity.base)
                && entity
                    .living()
                    .is_some_and(|living| !living::is_dead_or_dying(&entity.base, living))
        });
        let holding = self
            .held(id, hand)
            .is_some_and(|held| held.item == stack.item);
        if cancelled || !ready || !holding {
            if parked {
                self.refresh_hands(id);
            }
            return;
        }
        let color = patch.color.map_or(color, wool_of);
        let Some(entity) = self.entities.store.get_mut(target) else {
            return;
        };
        species::set_wool_color(&mut entity.base, color);
        let position = entity.base.position;
        self.play_sound(
            Some(id),
            position,
            "minecraft:item.dye.use",
            SoundSource::Players,
            1.0,
            1.0,
        );
        if parked {
            self.consume_held(id, hand, 1);
            self.player_swing(id, hand, true);
        } else {
            self.shrink_held(id, hand, 1);
        }
        self.refresh_hands(id);
    }

    fn ask_love(
        &mut self,
        id: PlayerId,
        target: EntityId,
        cause: Option<uuid::Uuid>,
        item: Option<ItemStack>,
    ) {
        let step = LoveStep {
            target,
            cause,
            item,
        };
        self.fire(id, Continuation::Love(Box::new(step)), move |server| {
            Some(EventPayload::EntityEnterLoveMode(EntityEnterLoveMode {
                entity: server.entity_view(target)?,
                entity_type: server.entity_view(target)?.kind,
                human_entity: server.snapshot_of(id),
                ticks_in_love: rspigot_domain::entity::ageable::LOVE_TICKS,
            }))
        });
    }
}

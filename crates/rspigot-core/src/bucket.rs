use rspigot_blocks::{BlockKind, StateFlags};
use rspigot_domain::entity::ageable::{self, AgeableState};
use rspigot_domain::entity::save::{component_from_tag, item_stack_from_nbt, item_stack_to_nbt};
use rspigot_domain::entity::species::{self, Species, fish, sulfur_cube};
use rspigot_domain::entity::{Entity, EntityState, FLAG_GLOWING, living};
use rspigot_domain::item::ItemStack;
use rspigot_domain::player::{GameMode, PlayerId};
use rspigot_domain::spawn::SpawnReason;
use rspigot_domain::{BlockPos, EntityId, ItemId, Rotation, Vec3};
use rspigot_entities::{EntityKind as Kind, SoundSource};
use rspigot_items::ItemKind;
use rspigot_nbt::Compound;

use crate::events::Hand;
use crate::interact::Interaction;
use crate::plugins::events::{
    CreatureSpawnReason, EntityRemoveCause, EquipmentSlot as Slot, PlayerBucketEmpty,
    PlayerBucketEntity,
};
use crate::plugins::steps::{BucketEmptyStep, BucketEntityStep};
use crate::plugins::{
    Birth, Continuation, EventPayload, Lane, direction_of, plugin_face_of, spec_of,
};
use crate::ports::ClientLink;
use crate::server::Server;

const BUCKET_ENTITY_DATA: &str = "minecraft:bucket_entity_data";
const TROPICAL_PATTERN: &str = "minecraft:tropical_fish/pattern";
const TROPICAL_BASE_COLOR: &str = "minecraft:tropical_fish/base_color";
const TROPICAL_PATTERN_COLOR: &str = "minecraft:tropical_fish/pattern_color";
const SALMON_SIZE: &str = "minecraft:salmon/size";
const CUSTOM_NAME: &str = "minecraft:custom_name";
const AXOLOTL_VARIANT: &str = "minecraft:axolotl/variant";
const PICKUP_SOUND: &str = "minecraft:item.bucket.fill_fish";
const EMPTY_SOUND: &str = "minecraft:item.bucket.empty_fish";
const AXOLOTL_PICKUP_SOUND: &str = "minecraft:item.bucket.fill_axolotl";
const AXOLOTL_EMPTY_SOUND: &str = "minecraft:item.bucket.empty_axolotl";
const TADPOLE_PICKUP_SOUND: &str = "minecraft:item.bucket.fill_tadpole";
const TADPOLE_EMPTY_SOUND: &str = "minecraft:item.bucket.empty_tadpole";
const FIZZ_SOUND: &str = "minecraft:block.fire.extinguish";
const THE_NETHER: &str = "minecraft:the_nether";
const WATERLOGGED: &str = "waterlogged";

pub(crate) fn bucket_item_of(kind: Kind) -> Option<ItemKind> {
    match kind {
        Kind::Cod => Some(ItemKind::CodBucket),
        Kind::Salmon => Some(ItemKind::SalmonBucket),
        Kind::Pufferfish => Some(ItemKind::PufferfishBucket),
        Kind::TropicalFish => Some(ItemKind::TropicalFishBucket),
        Kind::Axolotl => Some(ItemKind::AxolotlBucket),
        Kind::Tadpole => Some(ItemKind::TadpoleBucket),
        Kind::SulfurCube => Some(ItemKind::SulfurCubeBucket),
        _ => None,
    }
}

pub(crate) fn mob_of_bucket(item: ItemId) -> Option<Kind> {
    match item.kind()? {
        ItemKind::CodBucket => Some(Kind::Cod),
        ItemKind::SalmonBucket => Some(Kind::Salmon),
        ItemKind::PufferfishBucket => Some(Kind::Pufferfish),
        ItemKind::TropicalFishBucket => Some(Kind::TropicalFish),
        ItemKind::AxolotlBucket => Some(Kind::Axolotl),
        ItemKind::TadpoleBucket => Some(Kind::Tadpole),
        _ => None,
    }
}

fn pickup_sound(kind: Kind) -> &'static str {
    match kind {
        Kind::Axolotl => AXOLOTL_PICKUP_SOUND,
        Kind::Tadpole => TADPOLE_PICKUP_SOUND,
        Kind::SulfurCube => sulfur_cube::BUCKET_FILL_SOUND,
        _ => PICKUP_SOUND,
    }
}

fn empty_sound(kind: Kind) -> &'static str {
    match kind {
        Kind::Axolotl => AXOLOTL_EMPTY_SOUND,
        Kind::Tadpole => TADPOLE_EMPTY_SOUND,
        _ => EMPTY_SOUND,
    }
}

fn slot_of(hand: Hand) -> Slot {
    match hand {
        Hand::Main => Slot::Hand,
        Hand::Off => Slot::OffHand,
    }
}

fn replaceable_by_water(state: rspigot_domain::BlockStateId) -> bool {
    let flags = state.flags();
    flags.contains(StateFlags::REPLACEABLE) || !flags.contains(StateFlags::LEGACY_SOLID)
}

fn can_waterlog(state: rspigot_domain::BlockStateId) -> bool {
    state.property(WATERLOGGED) == Some("false")
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Placement {
    Evaporate,
    Waterlog,
    Place { destroy: bool },
}

impl<L: ClientLink> Server<L> {
    fn fish_bucket_stack(&self, target: EntityId) -> Option<ItemStack> {
        let entity = self.entities.store.get(target)?;
        let base = &entity.base;
        let state = entity.living()?;
        let item = bucket_item_of(base.kind)?;
        let mut stack = ItemStack::new(item.id(), std::num::NonZeroU8::MIN);
        stack.set_custom_name(base.custom_name());
        let mut data = Compound::new();
        if living::is_no_ai(base) {
            data.insert("NoAI", true);
        }
        if base.is_silent() {
            data.insert("Silent", true);
        }
        if base.is_no_gravity() {
            data.insert("NoGravity", true);
        }
        if base.data.flag(
            rspigot_domain::entity::DATA_SHARED_FLAGS,
            rspigot_domain::entity::FLAG_GLOWING,
        ) {
            data.insert("Glowing", true);
        }
        if base.invulnerable {
            data.insert("Invulnerable", true);
        }
        if state
            .mob
            .as_ref()
            .is_some_and(|mob| mob.persistence_required)
        {
            data.insert("PersistenceRequired", true);
        }
        data.insert("Health", living::health(base));
        match base.kind {
            Kind::Axolotl => {
                if let Some(ageable) = state.mob.as_ref().and_then(|mob| mob.ageable.as_ref()) {
                    data.insert("Age", ageable.age);
                }
                data.insert("AgeLocked", ageable::is_age_locked(base));
                if let Some(ticks) = self.axolotl_hunting_cooldown(target) {
                    data.insert("HuntingCooldown", ticks);
                }
            }
            Kind::Tadpole => {
                if let Some(Species::Tadpole(tadpole)) = state.mob.as_ref().map(|mob| mob.species) {
                    data.insert("Age", tadpole.age);
                }
                data.insert("AgeLocked", species::tadpole::is_age_locked(base));
            }
            Kind::SulfurCube => {
                if let Some(ageable) = state.mob.as_ref().and_then(|mob| mob.ageable.as_ref()) {
                    data.insert("age", ageable.age);
                }
                data.insert("age_locked", ageable::is_age_locked(base));
                if let Some(content) = sulfur_cube::body_item(state) {
                    stack
                        .components
                        .insert(sulfur_cube::SULFUR_CUBE_CONTENT, item_stack_to_nbt(content));
                }
            }
            _ => {}
        }
        stack.components.insert(BUCKET_ENTITY_DATA, data);
        match base.kind {
            Kind::Axolotl => {
                stack
                    .components
                    .insert(AXOLOTL_VARIANT, species::axolotl::variant(base).name());
            }
            Kind::Salmon => {
                stack
                    .components
                    .insert(SALMON_SIZE, fish::salmon_variant(base).name());
            }
            Kind::TropicalFish => {
                let variant = fish::tropical_variant(base);
                stack
                    .components
                    .insert(TROPICAL_PATTERN, variant.pattern.name());
                stack.components.insert(
                    TROPICAL_BASE_COLOR,
                    fish::DYE_NAMES[usize::from(variant.base_color)],
                );
                stack.components.insert(
                    TROPICAL_PATTERN_COLOR,
                    fish::DYE_NAMES[usize::from(variant.pattern_color)],
                );
            }
            _ => {}
        }
        Some(stack)
    }

    pub(crate) fn fish_interact(
        &mut self,
        player: PlayerId,
        target: EntityId,
        hand: Hand,
        held: Option<&ItemStack>,
    ) -> Option<Interaction> {
        let stack = held?;
        let entity = self.entities.store.get(target)?;
        let required = if entity.base.kind == Kind::SulfurCube {
            ItemKind::Bucket
        } else {
            ItemKind::WaterBucket
        };
        if stack.item != required.id() {
            return None;
        }
        let alive = entity
            .living()
            .is_some_and(|state| !living::is_dead_or_dying(&entity.base, state));
        if !alive {
            return None;
        }
        let bucket = self.fish_bucket_stack(target)?;
        let step = BucketEntityStep {
            player,
            target,
            hand,
            stack: stack.clone(),
            bucket: bucket.clone(),
            parked: false,
        };
        let mut then = Continuation::BucketEntity(Box::new(step));
        let parked = self.parks(Lane::Player(player), &then);
        if let Continuation::BucketEntity(step) = &mut then {
            step.parked = parked;
        }
        let original = spec_of(stack);
        let filled = spec_of(&bucket);
        self.fire(player, then, move |server| {
            Some(EventPayload::PlayerBucketEntity(PlayerBucketEntity {
                entity: server.entity_view(target)?,
                entity_bucket: filled,
                hand: slot_of(hand),
                original_bucket: original,
                player: server.snapshot_of(player)?,
            }))
        });
        Some(if parked {
            Interaction::Consume
        } else {
            Interaction::Predicted
        })
    }

    pub(crate) fn finish_bucket_entity(&mut self, step: BucketEntityStep, cancelled: bool) {
        let BucketEntityStep {
            player,
            target,
            hand,
            stack,
            bucket,
            parked,
        } = step;
        if cancelled {
            self.send_inventory(player);
            return;
        }
        let alive = self.entities.store.get(target).is_some_and(|entity| {
            !entity.base.is_removed()
                && entity
                    .living()
                    .is_some_and(|state| !living::is_dead_or_dying(&entity.base, state))
        });
        if !alive || self.held(player, hand).as_ref() != Some(&stack) {
            return;
        }
        let sound = self
            .entities
            .store
            .get(target)
            .map_or(PICKUP_SOUND, |entity| pickup_sound(entity.base.kind));
        self.entity_sound(target, sound, 1.0, 1.0);
        self.fill_bucket_in_hand(player, hand, bucket);
        self.remove_entity(
            target,
            rspigot_domain::entity::RemovalReason::Discarded,
            Some(EntityRemoveCause::Pickup),
        );
        if parked {
            self.player_swing(player, hand, true);
            self.refresh_hands(player);
        }
    }

    fn fill_bucket_in_hand(&mut self, player: PlayerId, hand: Hand, filled: ItemStack) {
        self.consume_held(player, hand, 1);
        if self.held(player, hand).is_none() {
            self.set_held(player, hand, Some(filled));
        } else {
            self.give_or_drop(player, filled);
        }
    }

    fn empty_contents(
        &self,
        target: BlockPos,
        hit: Option<(BlockPos, rspigot_blocks::Face)>,
    ) -> Option<(BlockPos, Placement)> {
        let state = self.world.chunks.block(target)?;
        let ultra_warm = self.world.dimension.dimension_type.as_str() == THE_NETHER;
        let free = state.is_air() || replaceable_by_water(state) || can_waterlog(state);
        if !free {
            let (pos, face) = hit?;
            let (dx, dy, dz) = direction_of(face).offset();
            return self.empty_contents(pos.offset(dx, dy, dz), None);
        }
        if ultra_warm {
            return Some((target, Placement::Evaporate));
        }
        if can_waterlog(state) {
            return Some((target, Placement::Waterlog));
        }
        let liquid = state.flags().has_fluid();
        let destroy = replaceable_by_water(state) && !state.is_air() && !liquid;
        Some((target, Placement::Place { destroy }))
    }

    pub(crate) fn use_mob_bucket(&mut self, id: PlayerId, hand: Hand, rotation: Rotation) -> bool {
        let Some(stack) = self.held(id, hand) else {
            return false;
        };
        if stack.item == ItemKind::SulfurCubeBucket.id() {
            return self.use_sulfur_cube_bucket(id, hand, rotation, &stack);
        }
        let Some(kind) = mob_of_bucket(stack.item) else {
            return false;
        };
        let Some(hit) = self.look_block(id, rotation) else {
            return false;
        };
        let (dx, dy, dz) = direction_of(hit.face).offset();
        let relative = hit.pos.offset(dx, dy, dz);
        let Some(clicked) = self.world.chunks.block(hit.pos) else {
            return true;
        };
        let adventure = self
            .players
            .get(&id)
            .is_some_and(|player| player.game_mode == GameMode::Adventure);
        if self.is_protected(id, hit.pos) || self.is_protected(id, relative) || adventure {
            return true;
        }
        let target = if can_waterlog(clicked) {
            hit.pos
        } else {
            relative
        };
        let Some((block, placement)) = self.empty_contents(target, Some((hit.pos, hit.face)))
        else {
            return true;
        };
        let step = BucketEmptyStep {
            player: id,
            hand,
            stack: stack.clone(),
            kind,
            block,
            placement,
        };
        let spec = spec_of(&stack);
        let name = stack.item.kind().map_or("minecraft:air", ItemKind::name);
        let (clicked_pos, face) = (hit.pos, plugin_face_of(hit.face));
        self.fire(
            id,
            Continuation::BucketEmpty(Box::new(step)),
            move |server| {
                Some(EventPayload::PlayerBucketEmpty(PlayerBucketEmpty {
                    block,
                    block_clicked: clicked_pos,
                    block_face: face,
                    bucket: name.to_owned(),
                    hand: slot_of(hand),
                    item_stack: Some(spec),
                    player: server.snapshot_of(id)?,
                }))
            },
        );
        true
    }

    fn use_sulfur_cube_bucket(
        &mut self,
        id: PlayerId,
        hand: Hand,
        rotation: Rotation,
        stack: &ItemStack,
    ) -> bool {
        let Some(hit) = self.look_block(id, rotation) else {
            return false;
        };
        let (dx, dy, dz) = direction_of(hit.face).offset();
        let relative = hit.pos.offset(dx, dy, dz);
        let adventure = self
            .players
            .get(&id)
            .is_some_and(|player| player.game_mode == GameMode::Adventure);
        if self.is_protected(id, hit.pos) || self.is_protected(id, relative) || adventure {
            return true;
        }
        let position = Vec3::new(
            f64::from(relative.x) + 0.5,
            f64::from(relative.y),
            f64::from(relative.z) + 0.5,
        );
        self.play_sound(
            None,
            position,
            sulfur_cube::BUCKET_EMPTY_SOUND,
            SoundSource::Neutral,
            1.0,
            1.0,
        );
        self.spawn_bucketed_fish(id, Kind::SulfurCube, stack, relative);
        if !self.infinite_materials(id) {
            let empty = ItemStack::new(ItemKind::Bucket.id(), std::num::NonZeroU8::MIN);
            self.set_held(id, hand, Some(empty));
        }
        true
    }

    pub(crate) fn finish_bucket_empty(&mut self, step: BucketEmptyStep, cancelled: bool) {
        let BucketEmptyStep {
            player,
            hand,
            stack,
            kind,
            block,
            placement,
            ..
        } = step;
        if cancelled {
            self.send_inventory(player);
            if let Some(state) = self.world.chunks.block(block) {
                self.push(
                    crate::server::Audience::One(player),
                    crate::events::ClientEvent::BlockUpdate { pos: block, state },
                );
            }
            return;
        }
        if self.held(player, hand).as_ref() != Some(&stack) {
            return;
        }
        let position = Vec3::new(
            f64::from(block.x) + 0.5,
            f64::from(block.y),
            f64::from(block.z) + 0.5,
        );
        match placement {
            Placement::Evaporate => {
                self.play_sound(None, position, FIZZ_SOUND, SoundSource::Blocks, 0.5, 2.6);
            }
            Placement::Waterlog => {
                if let Some(state) = self
                    .world
                    .chunks
                    .block(block)
                    .and_then(|state| state.with_property(WATERLOGGED, "true"))
                {
                    self.set_block(block, state);
                }
                self.play_sound(
                    None,
                    position,
                    empty_sound(kind),
                    SoundSource::Neutral,
                    1.0,
                    1.0,
                );
            }
            Placement::Place { destroy } => {
                if destroy {
                    self.break_block_naturally(block);
                }
                self.set_block(block, BlockKind::Water.default_state());
                self.play_sound(
                    None,
                    position,
                    empty_sound(kind),
                    SoundSource::Neutral,
                    1.0,
                    1.0,
                );
            }
        }
        self.spawn_bucketed_fish(player, kind, &stack, block);
        if !self.infinite_materials(player) {
            let empty = ItemStack::new(ItemKind::Bucket.id(), std::num::NonZeroU8::MIN);
            self.set_held(player, hand, Some(empty));
        }
    }

    fn spawn_bucketed_fish(
        &mut self,
        player: PlayerId,
        kind: Kind,
        stack: &ItemStack,
        block: BlockPos,
    ) {
        let position = Vec3::new(
            f64::from(block.x) + 0.5,
            f64::from(block.y),
            f64::from(block.z) + 0.5,
        );
        let Ok((mut entity, random)) =
            self.summon_entity(kind, position, None, SpawnReason::Bucket)
        else {
            return;
        };
        apply_bucket_components(&mut entity, stack);
        self.spawn_entity(
            entity,
            Birth::of(CreatureSpawnReason::Bucket)
                .with_random(random)
                .by(player),
        );
    }
}

fn apply_bucket_components(entity: &mut Entity, stack: &ItemStack) {
    let Entity {
        base,
        state: EntityState::Living(living),
        ..
    } = entity
    else {
        return;
    };
    let components = &stack.components;
    if base.kind == Kind::SulfurCube
        && let Some(content) = components
            .get_compound(sulfur_cube::SULFUR_CUBE_CONTENT)
            .and_then(item_stack_from_nbt)
    {
        living.equipment[sulfur_cube::slot_index_of_body()] = Some(content);
        if let Some(mob) = living.mob.as_mut() {
            mob.drop_chances[sulfur_cube::slot_index_of_body()] = sulfur_cube::GUARANTEED_DROP;
        }
    }
    match base.kind {
        Kind::Salmon => {
            if let Some(variant) = components
                .get_str(SALMON_SIZE)
                .and_then(fish::Variant::from_name)
            {
                fish::set_salmon_variant(base, variant);
            }
        }
        Kind::TropicalFish => {
            let current = fish::tropical_variant(base);
            let pattern = components
                .get_str(TROPICAL_PATTERN)
                .and_then(|name| fish::Pattern::ALL.into_iter().find(|p| p.name() == name))
                .unwrap_or(current.pattern);
            let dye = |key: &str, fallback: u8| {
                components
                    .get_str(key)
                    .and_then(|name| fish::DYE_NAMES.iter().position(|dye| *dye == name))
                    .map_or(fallback, |index| index as u8)
            };
            fish::set_tropical_variant(
                base,
                fish::TropicalVariant {
                    pattern,
                    base_color: dye(TROPICAL_BASE_COLOR, current.base_color),
                    pattern_color: dye(TROPICAL_PATTERN_COLOR, current.pattern_color),
                },
            );
        }
        _ => {}
    }
    if let Some(data) = components.get_compound(BUCKET_ENTITY_DATA) {
        if let Some(name) = components.get(CUSTOM_NAME).and_then(component_from_tag) {
            base.set_custom_name(Some(name));
        }
        if let Some(no_ai) = data.get_bool("NoAI") {
            living::set_no_ai(base, no_ai);
        }
        if let Some(silent) = data.get_bool("Silent") {
            base.set_silent(silent);
        }
        if let Some(no_gravity) = data.get_bool("NoGravity") {
            base.set_no_gravity(no_gravity);
        }
        if let Some(glowing) = data.get_bool("Glowing") {
            base.set_shared_flag(FLAG_GLOWING, glowing);
        }
        if let Some(invulnerable) = data.get_bool("Invulnerable") {
            base.invulnerable = invulnerable;
        }
        if let (Some(persistent), Some(mob)) =
            (data.get_bool("PersistenceRequired"), living.mob.as_mut())
        {
            mob.persistence_required = persistent;
        }
        if let Some(health) = data.get_f64("Health") {
            living::set_health(base, living, health as f32);
        }
        match living.mob.as_mut() {
            Some(mob) if base.kind == Kind::Axolotl => {
                if let Some(ageable) = mob.ageable.as_mut() {
                    ageable::set_age(base, ageable, data.get_i32("Age").unwrap_or(0));
                }
                ageable::set_age_locked(base, data.get_bool("AgeLocked").unwrap_or(false));
                if let Species::Axolotl(state) = &mut mob.species {
                    state.pending_hunting_cooldown = data.get_i64("HuntingCooldown");
                }
            }
            Some(mob) if base.kind == Kind::SulfurCube => {
                if let Some(ageable) = mob.ageable.as_mut() {
                    ageable::set_age(base, ageable, data.get_i32("age").unwrap_or(0));
                }
                ageable::set_age_locked(base, data.get_bool("age_locked").unwrap_or(false));
            }
            Some(mob) if base.kind == Kind::Tadpole => {
                if let Species::Tadpole(state) = &mut mob.species {
                    state.age = data.get_i32("Age").unwrap_or(0);
                }
                species::tadpole::set_age_locked(base, data.get_bool("AgeLocked").unwrap_or(false));
            }
            _ => {}
        }
    }
    if base.kind == Kind::SulfurCube
        && living
            .mob
            .as_mut()
            .and_then(|mob| mob.ageable.as_mut())
            .is_some_and(AgeableState::take_grew_up)
    {
        sulfur_cube::age_boundary_reached(base, living);
    }
    match base.kind {
        Kind::Axolotl => {
            if let Some(variant) = components
                .get_str(AXOLOTL_VARIANT)
                .and_then(species::axolotl::Variant::from_name)
            {
                species::axolotl::set_variant(base, variant);
            }
            species::axolotl::set_from_bucket(base, true);
        }
        Kind::Tadpole => {}
        Kind::SulfurCube => {
            sulfur_cube::set_from_bucket(base, true);
        }
        _ => fish::set_from_bucket(base, true),
    }
}

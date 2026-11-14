use std::borrow::Cow;

use rspigot_data::sulfur_cube::{
    self as data, Archetype, ContactDamage, Explosion, Knockback, Operation, Sounds,
};
use rspigot_entities::EntityKind;
use rspigot_items::EquipmentSlot;

use crate::effect::{AttributeModifier, AttributeOperation};
use crate::entity::base::EntityBase;
use crate::entity::data::SyncedValue;
use crate::entity::living::{LivingState, slot_index};
use crate::entity::species::Species;
use crate::item::ItemStack;
use crate::random::RandomSource;

pub const DATA_MAX_FUSE: u8 = 19;
pub const DATA_FROM_BUCKET: u8 = 20;
pub const PICKUP_TIMER_DURATION: i32 = 100;
pub const SPLIT_COUNT: i32 = 2;
pub const ADULT_SIZE: i32 = 2;
pub const BABY_SIZE: i32 = 1;
pub const HEALTH_PER_SIZE: i32 = 4;
pub const NO_FUSE: i32 = -1;
pub const GUARANTEED_DROP: f32 = 2.0;
pub const PUSH_DISTANCE_THRESHOLD: f64 = 1.299_999_952_316_284_2;
pub const MAX_PLAYER_PUSH_SPEED: f64 = 0.5;
pub const PLAYER_PUSH_SPEED_SCALE: f64 = 0.3;
pub const VEHICLE_PUSH_SPEED_SCALE: f64 = 0.16;
pub const GROUND_LIFT: f64 = 0.300_000_011_920_928_96;
pub const PUSH_SPEED_FACTOR: f64 = 2.0;
pub const FLUID_JUMP_THRESHOLD: f64 = 0.2;
pub const BUOYANCY_WAVE_SPEED: f32 = 0.4;
pub const BUOYANCY_WAVE_AMPLITUDE: f32 = 0.2;
pub const BUOYANCY_LIFT: f64 = 0.04;
pub const EXPLOSION_LIFT: f64 = 0.0625;
pub const KEYS: &[&str] = &["pickup_timer", "from_bucket", "fuse"];
pub const ABSORB_SOUND: &str = "minecraft:entity.sulfur_cube.absorb";
pub const EJECT_SOUND: &str = "minecraft:entity.sulfur_cube.eject";
pub const EAT_SOUND: &str = "minecraft:entity.small_sulfur_cube.eat";
pub const BUCKET_FILL_SOUND: &str = "minecraft:item.bucket.fill_sulfur_cube";
pub const BUCKET_EMPTY_SOUND: &str = "minecraft:item.bucket.empty_sulfur_cube";
pub const PRIMED_SOUND: &str = "minecraft:entity.tnt.primed";
pub const SULFUR_CUBE_CONTENT: &str = "minecraft:sulfur_cube_content";
pub const TNT_DISABLED: &str = "block.minecraft.tnt.disabled";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SulfurState {
    pub pickup_timer: i32,
    pub push_sound_cooldown: i32,
    pub fuse: i32,
    pub applied: Option<u16>,
}

impl Default for SulfurState {
    fn default() -> Self {
        Self {
            pickup_timer: 0,
            push_sound_cooldown: 0,
            fuse: NO_FUSE,
            applied: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Profile {
    pub buoyant: bool,
    pub explosion: Option<Explosion>,
    pub contact_damage: Option<ContactDamage>,
    pub knockback: Knockback,
    pub sounds: Sounds,
}

impl Default for Profile {
    fn default() -> Self {
        Self {
            buoyant: false,
            explosion: None,
            contact_damage: None,
            knockback: data::DEFAULT_KNOCKBACK,
            sounds: data::DEFAULT_SOUNDS,
        }
    }
}

pub fn is_sulfur_cube(kind: EntityKind) -> bool {
    kind == EntityKind::SulfurCube
}

pub fn profile_of(item: Option<u16>) -> Profile {
    let mut profile = Profile::default();
    let Some(item) = item else {
        return profile;
    };
    for archetype in data::matching(item) {
        if archetype.buoyant {
            profile.buoyant = true;
        }
        if archetype.explosion.is_some() {
            profile.explosion = archetype.explosion;
        }
        if archetype.contact_damage.is_some() {
            profile.contact_damage = archetype.contact_damage;
        }
        profile.knockback = archetype.knockback_modifiers;
        profile.sounds = archetype.sound_settings;
    }
    profile
}

pub fn is_archetype_modifier(id: &str) -> bool {
    data::archetypes().iter().any(|archetype| {
        archetype
            .attribute_modifiers
            .iter()
            .any(|entry| entry.id == id)
    })
}

pub fn is_swallowable(stack: &ItemStack) -> bool {
    data::is_swallowable(stack.item.0)
}

pub fn is_food(stack: &ItemStack) -> bool {
    data::is_food(stack.item.0)
}

pub fn state(living: &LivingState) -> Option<&SulfurState> {
    match living.mob.as_ref().map(|mob| &mob.species) {
        Some(Species::Cube(cube)) => Some(&cube.sulfur),
        _ => None,
    }
}

pub fn state_mut(living: &mut LivingState) -> Option<&mut SulfurState> {
    match living.mob.as_mut().map(|mob| &mut mob.species) {
        Some(Species::Cube(cube)) => Some(&mut cube.sulfur),
        _ => None,
    }
}

pub fn body_item(living: &LivingState) -> Option<&ItemStack> {
    living.equipment(EquipmentSlot::Body)
}

pub fn has_body_item(base: &EntityBase, living: &LivingState) -> bool {
    is_sulfur_cube(base.kind) && body_item(living).is_some()
}

pub fn requires_persistence(base: &EntityBase, living: &LivingState) -> bool {
    is_sulfur_cube(base.kind) && (body_item(living).is_some() || from_bucket(base))
}

pub fn applied_profile(living: &LivingState) -> Profile {
    profile_of(state(living).and_then(|state| state.applied))
}

pub fn is_primed(living: &LivingState) -> bool {
    state(living).is_some_and(|state| state.fuse > NO_FUSE)
}

pub fn can_explode(living: &LivingState, alive: bool) -> bool {
    applied_profile(living).explosion.is_some() && alive && !is_primed(living)
}

pub fn from_bucket(base: &EntityBase) -> bool {
    base.data.boolean(DATA_FROM_BUCKET)
}

pub fn set_from_bucket(base: &mut EntityBase, value: bool) {
    base.data.set(DATA_FROM_BUCKET, SyncedValue::Boolean(value));
}

pub fn synced_fuse(base: &EntityBase) -> i32 {
    match base.data.get(DATA_MAX_FUSE) {
        Some(SyncedValue::Int(fuse)) => *fuse,
        _ => NO_FUSE,
    }
}

fn operation(operation: Operation) -> AttributeOperation {
    match operation {
        Operation::AddValue => AttributeOperation::AddValue,
        Operation::AddMultipliedBase => AttributeOperation::AddMultipliedBase,
        Operation::AddMultipliedTotal => AttributeOperation::AddMultipliedTotal,
    }
}

fn remove_modifiers(living: &mut LivingState, archetype: &Archetype) {
    for entry in archetype.attribute_modifiers {
        living.attributes.remove_modifier(entry.attribute, entry.id);
    }
}

fn add_modifiers(living: &mut LivingState, archetype: &Archetype) {
    for entry in archetype.attribute_modifiers {
        living.attributes.add_or_replace_modifier(
            entry.attribute,
            AttributeModifier {
                id: Cow::Borrowed(entry.id),
                amount: entry.amount,
                operation: operation(entry.operation),
            },
        );
    }
}

pub fn sync_equipment(base: &EntityBase, living: &mut LivingState) -> bool {
    if !is_sulfur_cube(base.kind) {
        return false;
    }
    let current = body_item(living).map(|stack| stack.item.0);
    let Some(previous) = state(living).map(|state| state.applied) else {
        return false;
    };
    if previous == current {
        return false;
    }
    if let Some(item) = previous {
        for archetype in data::matching(item) {
            remove_modifiers(living, archetype);
        }
    }
    if current.is_some() {
        living.speed = 0.0;
    }
    if let Some(state) = state_mut(living) {
        state.applied = current;
    }
    if let Some(item) = current {
        for archetype in data::matching(item) {
            add_modifiers(living, archetype);
        }
    }
    true
}

pub fn age_boundary_reached(base: &mut EntityBase, living: &mut LivingState) {
    let baby = living
        .mob
        .as_ref()
        .and_then(|mob| mob.ageable.as_ref())
        .is_some_and(|ageable| crate::entity::ageable::is_baby(base, ageable));
    if !baby {
        crate::entity::species::cube::set_size(base, living, ADULT_SIZE, true);
    }
}

pub fn can_hold_item(
    base: &EntityBase,
    living: &LivingState,
    stack: &ItemStack,
    baby: bool,
) -> bool {
    is_sulfur_cube(base.kind) && body_item(living).is_none() && is_swallowable(stack) && !baby
}

pub fn max_up_step_blocked(base: &EntityBase, living: &LivingState) -> bool {
    has_body_item(base, living)
}

pub fn jump_threshold(base: &EntityBase) -> f64 {
    f64::from(base.dimensions.height) * FLUID_JUMP_THRESHOLD
}

pub fn buoyancy(base: &EntityBase, living: &LivingState, tick_count: i32) -> f64 {
    if !has_body_item(base, living) || !applied_profile(living).buoyant {
        return 0.0;
    }
    let wave = crate::mth::sin(f64::from(tick_count as f32 * BUOYANCY_WAVE_SPEED))
        * BUOYANCY_WAVE_AMPLITUDE;
    let height = if base.fluid.in_water {
        base.fluid.water_height
    } else {
        base.fluid.lava_height
    };
    let lift = (height - jump_threshold(base)) + f64::from(wave);
    if lift > 0.0 {
        lift.min(1.0) * BUOYANCY_LIFT
    } else {
        0.0
    }
}

pub fn random_short_fuse(fuse: i32, random: &mut impl RandomSource) -> i32 {
    random.next_int((fuse / 4).max(1)) + fuse / 8
}

pub fn prime(
    base: &mut EntityBase,
    living: &mut LivingState,
    imminent: bool,
    random: &mut impl RandomSource,
) -> bool {
    let Some(explosion) = applied_profile(living).explosion else {
        return false;
    };
    let mut fuse = explosion.fuse;
    if imminent {
        fuse = random_short_fuse(fuse, random);
    }
    base.invulnerable = true;
    if let Some(state) = state_mut(living) {
        state.fuse = fuse;
    }
    base.data.set(DATA_MAX_FUSE, SyncedValue::Int(fuse));
    true
}

pub fn tick_fuse(living: &mut LivingState) -> bool {
    let explosive = applied_profile(living).explosion.is_some();
    let Some(state) = state_mut(living) else {
        return false;
    };
    if state.fuse > 0 {
        state.fuse -= 1;
    }
    explosive && state.fuse == 0
}

pub fn tick_timers(living: &mut LivingState) {
    if let Some(state) = state_mut(living) {
        if state.pickup_timer > 0 {
            state.pickup_timer -= 1;
        }
        if state.push_sound_cooldown > 0 {
            state.push_sound_cooldown -= 1;
        }
    }
}

pub fn read(nbt: &rspigot_nbt::Compound, base: &mut EntityBase, state: &mut SulfurState) {
    state.pickup_timer = nbt.get_i32("pickup_timer").unwrap_or(0);
    set_from_bucket(base, nbt.get_bool("from_bucket").unwrap_or(false));
    state.fuse = nbt.get_i32("fuse").unwrap_or(NO_FUSE);
    base.data.set(DATA_MAX_FUSE, SyncedValue::Int(state.fuse));
}

pub fn write(base: &EntityBase, state: &SulfurState, out: &mut rspigot_nbt::Compound) {
    out.insert("pickup_timer", state.pickup_timer);
    out.insert("from_bucket", from_bucket(base));
    out.insert("fuse", state.fuse);
}

pub fn slot_index_of_body() -> usize {
    slot_index(EquipmentSlot::Body)
}

pub fn jump_sound(tiny: bool) -> &'static str {
    if tiny {
        "minecraft:entity.small_sulfur_cube.jump"
    } else {
        "minecraft:entity.sulfur_cube.jump"
    }
}

pub fn squish_sound(tiny: bool, has_body_item: bool) -> &'static str {
    if tiny {
        "minecraft:entity.small_sulfur_cube.squish"
    } else if has_body_item {
        "minecraft:entity.sulfur_cube.bounce"
    } else {
        "minecraft:entity.sulfur_cube.squish"
    }
}

pub fn push_cooldown_ticks(sounds: &Sounds) -> i32 {
    (sounds.push_sound_cooldown * 20.0) as i32
}

pub fn push_sound_audible(push: crate::Vec3, sounds: &Sounds) -> bool {
    let threshold = sounds.push_sound_impulse_threshold;
    push.length_sqr() > f64::from(threshold * threshold)
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PlayerPush {
    pub delta: crate::Vec3,
}

#[allow(clippy::too_many_arguments)]
pub fn player_push(
    cube_position: crate::Vec3,
    cube_height: f32,
    cube_on_ground: bool,
    knockback_resistance: f64,
    pusher_position: crate::Vec3,
    pusher_height: f32,
    pusher_riding: bool,
    pusher_speed: f64,
) -> Option<PlayerPush> {
    let diff = cube_position - pusher_position;
    let cube_top = cube_position.y + f64::from(cube_height);
    let pusher_top = pusher_position.y + f64::from(pusher_height);
    let horizontal = (diff.x * diff.x + diff.z * diff.z).sqrt();
    if !(horizontal < PUSH_DISTANCE_THRESHOLD
        && pusher_position.y <= cube_top
        && pusher_top > cube_position.y)
    {
        return None;
    }
    let power = (1.0 - knockback_resistance).max(0.0);
    let direction = crate::Vec3::new(diff.x, 0.0, diff.z)
        .normalize()
        .scale(power);
    let scale = if pusher_riding {
        VEHICLE_PUSH_SPEED_SCALE
    } else {
        PLAYER_PUSH_SPEED_SCALE
    };
    let speed = (pusher_speed * PUSH_SPEED_FACTOR * f64::from(scale as f32))
        .clamp(0.0, MAX_PLAYER_PUSH_SPEED);
    let lift = if cube_on_ground {
        power * GROUND_LIFT
    } else {
        0.0
    };
    Some(PlayerPush {
        delta: crate::Vec3::new(direction.x, lift, direction.z).scale(speed),
    })
}

#[cfg(test)]
mod tests {
    use rspigot_nbt::Compound;
    use uuid::Uuid;

    use super::*;
    use crate::EntityId;
    use crate::entity::living::{MAX_HEALTH, MOVEMENT_SPEED};
    use crate::entity::save::{load_entity, save_entity};
    use crate::entity::species::cube;
    use crate::entity::{Entity, EntityState};
    use crate::random::LegacyRandom;

    fn cube_entity(nbt: Compound) -> Entity {
        load_entity(
            &nbt.with("id", "minecraft:sulfur_cube"),
            EntityId(1),
            Uuid::from_u128(1),
        )
        .unwrap()
    }

    fn block(name: &str) -> ItemStack {
        let kind = rspigot_items::ItemKind::from_name(name).unwrap();
        ItemStack::new(kind.id(), std::num::NonZeroU8::MIN)
    }

    fn swallow(entity: &mut Entity, name: &str) {
        let Entity {
            base,
            state: EntityState::Living(living),
            ..
        } = entity
        else {
            unreachable!()
        };
        living.equipment[slot_index_of_body()] = Some(block(name));
        assert!(sync_equipment(base, living));
    }

    fn living_of(entity: &mut Entity) -> (&mut EntityBase, &mut LivingState) {
        match &mut entity.state {
            EntityState::Living(living) => (&mut entity.base, living),
            _ => unreachable!(),
        }
    }

    #[test]
    fn the_fuse_and_the_bucket_flag_sit_after_the_size_in_the_synced_data() {
        let entity = cube_entity(Compound::new());
        assert_eq!(
            entity.base.data.get(cube::DATA_SIZE),
            Some(&SyncedValue::Int(1))
        );
        assert_eq!(
            entity.base.data.get(DATA_MAX_FUSE),
            Some(&SyncedValue::Int(-1))
        );
        assert_eq!(
            entity.base.data.get(DATA_FROM_BUCKET),
            Some(&SyncedValue::Boolean(false))
        );
    }

    #[test]
    fn persisted_state_round_trips_with_the_size_in_the_vanilla_layout() {
        let entity = cube_entity(
            Compound::new()
                .with("Size", 1)
                .with("pickup_timer", 40)
                .with("from_bucket", true)
                .with("fuse", 77),
        );
        let saved = save_entity(&entity, None).unwrap();
        assert_eq!(saved.get_i32("Size"), Some(1));
        assert_eq!(saved.get_i32("pickup_timer"), Some(40));
        assert_eq!(saved.get_bool("from_bucket"), Some(true));
        assert_eq!(saved.get_i32("fuse"), Some(77));
        assert_eq!(saved.get_bool("wasOnGround"), Some(false));
        let again = load_entity(&saved, EntityId(1), Uuid::from_u128(1)).unwrap();
        assert_eq!(save_entity(&again, None), Some(saved));
        assert_eq!(synced_fuse(&again.base), 77);
        assert!(from_bucket(&again.base));
    }

    #[test]
    fn missing_keys_default_to_no_timer_no_bucket_and_no_fuse() {
        let entity = cube_entity(Compound::new());
        let saved = save_entity(&entity, None).unwrap();
        assert_eq!(saved.get_i32("pickup_timer"), Some(0));
        assert_eq!(saved.get_bool("from_bucket"), Some(false));
        assert_eq!(saved.get_i32("fuse"), Some(-1));
    }

    #[test]
    fn the_health_is_four_per_size_and_there_is_no_attack_damage() {
        let mut entity = cube_entity(Compound::new().with("Size", 1));
        let (_, living) = living_of(&mut entity);
        assert_eq!(living.attribute(MAX_HEALTH), 8.0);
        assert_eq!(
            living.attribute(MOVEMENT_SPEED),
            f64::from(0.2f32 + 0.1f32 * 2.0)
        );
        let mut small = cube_entity(Compound::new().with("Size", 0));
        assert_eq!(living_of(&mut small).1.attribute(MAX_HEALTH), 4.0);
    }

    #[test]
    fn a_swallowed_block_applies_the_archetype_attributes_and_stops_the_cube() {
        let mut entity = cube_entity(Compound::new().with("Size", 1));
        let (_, living) = living_of(&mut entity);
        living.speed = 0.3;
        swallow(&mut entity, "minecraft:oak_planks");
        let (_, living) = living_of(&mut entity);
        assert_eq!(living.speed, 0.0);
        assert_eq!(living.attribute("minecraft:bounciness"), 0.9f32 as f64);
        assert_eq!(living.attribute("minecraft:knockback_resistance"), -2.0);
        assert!(applied_profile(living).buoyant);
        assert_eq!(
            applied_profile(living).sounds.hit_sound,
            "minecraft:entity.sulfur_cube.bouncy.hit"
        );
    }

    #[test]
    fn removing_the_block_restores_the_attributes_and_the_defaults() {
        let mut entity = cube_entity(Compound::new().with("Size", 1));
        let before = living_of(&mut entity).1.attribute("minecraft:bounciness");
        swallow(&mut entity, "minecraft:tnt");
        let (base, living) = living_of(&mut entity);
        living.equipment[slot_index_of_body()] = None;
        assert!(sync_equipment(base, living));
        assert_eq!(living.attribute("minecraft:bounciness"), before);
        assert_eq!(living.attribute("minecraft:knockback_resistance"), 0.0);
        assert_eq!(applied_profile(living), Profile::default());
        assert!(!sync_equipment(base, living));
    }

    #[test]
    fn swapping_one_block_for_another_replaces_the_modifiers() {
        let mut entity = cube_entity(Compound::new().with("Size", 1));
        swallow(&mut entity, "minecraft:oak_planks");
        swallow(&mut entity, "minecraft:honeycomb_block");
        let living = living_of(&mut entity).1;
        assert_eq!(living.attribute("minecraft:bounciness"), 0.0);
        assert_eq!(living.attribute("minecraft:friction_modifier"), 2.0);
        assert!(!applied_profile(living).buoyant);
    }

    #[test]
    fn the_archetype_modifiers_are_not_persisted() {
        let mut entity = cube_entity(Compound::new().with("Size", 1));
        swallow(&mut entity, "minecraft:oak_planks");
        let saved = save_entity(&entity, None).unwrap();
        let text = format!("{saved:?}");
        assert!(!text.contains("bouncy_add"));
        assert!(saved.get_compound("equipment").is_some());
        let mut again = load_entity(&saved, EntityId(1), Uuid::from_u128(1)).unwrap();
        let (base, living) = living_of(&mut again);
        assert!(sync_equipment(base, living));
        assert_eq!(living.attribute("minecraft:knockback_resistance"), -2.0);
    }

    #[test]
    fn only_an_adult_without_a_block_holds_a_swallowable_item() {
        let mut entity = cube_entity(Compound::new().with("Size", 1));
        let (base, living) = living_of(&mut entity);
        assert!(can_hold_item(base, living, &block("minecraft:dirt"), false));
        assert!(!can_hold_item(
            base,
            living,
            &block("minecraft:diamond"),
            false
        ));
        assert!(!can_hold_item(base, living, &block("minecraft:dirt"), true));
        living.equipment[slot_index_of_body()] = Some(block("minecraft:dirt"));
        assert!(!can_hold_item(
            base,
            living,
            &block("minecraft:dirt"),
            false
        ));
    }

    #[test]
    fn priming_stores_the_fuse_in_the_synced_data_and_a_short_one_when_imminent() {
        let mut entity = cube_entity(Compound::new().with("Size", 1));
        swallow(&mut entity, "minecraft:tnt");
        let (base, living) = living_of(&mut entity);
        let mut random = LegacyRandom::new(5);
        assert!(!is_primed(living));
        assert!(prime(base, living, false, &mut random));
        assert_eq!(state(living).unwrap().fuse, 120);
        assert_eq!(synced_fuse(base), 120);
        assert!(base.invulnerable);
        assert!(is_primed(living));
        assert!(!can_explode(living, true));
        let mut other = cube_entity(Compound::new().with("Size", 1));
        swallow(&mut other, "minecraft:tnt");
        let (base, living) = living_of(&mut other);
        assert!(prime(base, living, true, &mut random));
        let fuse = state(living).unwrap().fuse;
        assert!((15..=44).contains(&fuse), "{fuse}");
    }

    #[test]
    fn a_cube_without_an_explosive_block_cannot_be_primed() {
        let mut entity = cube_entity(Compound::new().with("Size", 1));
        swallow(&mut entity, "minecraft:dirt");
        let (base, living) = living_of(&mut entity);
        let mut random = LegacyRandom::new(5);
        assert!(!prime(base, living, false, &mut random));
        assert!(!can_explode(living, true));
    }

    #[test]
    fn the_fuse_counts_down_to_the_explosion() {
        let mut entity = cube_entity(Compound::new().with("Size", 1).with("fuse", 2));
        swallow(&mut entity, "minecraft:tnt");
        let living = living_of(&mut entity).1;
        assert!(!tick_fuse(living));
        assert_eq!(state(living).unwrap().fuse, 1);
        assert!(tick_fuse(living));
        assert_eq!(state(living).unwrap().fuse, 0);
    }

    #[test]
    fn an_unprimed_fuse_never_ticks() {
        let mut entity = cube_entity(Compound::new().with("Size", 1));
        swallow(&mut entity, "minecraft:tnt");
        let living = living_of(&mut entity).1;
        assert!(!tick_fuse(living));
        assert_eq!(state(living).unwrap().fuse, -1);
    }

    #[test]
    fn the_short_fuse_is_an_eighth_plus_a_random_quarter() {
        let mut random = LegacyRandom::new(9);
        for _ in 0..200 {
            let fuse = random_short_fuse(120, &mut random);
            assert!((15..45).contains(&fuse));
        }
    }

    #[test]
    fn the_timers_run_down_to_zero() {
        let mut entity = cube_entity(Compound::new().with("Size", 1).with("pickup_timer", 2));
        let living = living_of(&mut entity).1;
        state_mut(living).unwrap().push_sound_cooldown = 1;
        tick_timers(living);
        tick_timers(living);
        tick_timers(living);
        assert_eq!(state(living).unwrap().pickup_timer, 0);
        assert_eq!(state(living).unwrap().push_sound_cooldown, 0);
    }

    #[test]
    fn the_buoyancy_only_lifts_a_buoyant_cube_with_a_block_in_a_fluid() {
        let mut entity = cube_entity(Compound::new().with("Size", 1));
        {
            let (base, _) = living_of(&mut entity);
            base.fluid.in_water = true;
            base.fluid.water_height = 0.9;
        }
        let (base, living) = living_of(&mut entity);
        assert_eq!(buoyancy(base, living, 0), 0.0);
        swallow(&mut entity, "minecraft:oak_planks");
        let (base, living) = living_of(&mut entity);
        let lift = buoyancy(base, living, 0);
        assert!(lift > 0.0 && lift <= 0.04, "{lift}");
        swallow(&mut entity, "minecraft:honeycomb_block");
        let (base, living) = living_of(&mut entity);
        assert_eq!(buoyancy(base, living, 0), 0.0);
    }

    #[test]
    fn the_fluid_jump_threshold_is_a_fifth_of_the_height() {
        let entity = cube_entity(Compound::new().with("Size", 1));
        assert_eq!(
            jump_threshold(&entity.base),
            f64::from(entity.base.dimensions.height) * 0.2
        );
    }

    #[test]
    fn the_squish_and_jump_sounds_follow_the_size_and_the_body() {
        assert_eq!(
            squish_sound(true, true),
            "minecraft:entity.small_sulfur_cube.squish"
        );
        assert_eq!(
            squish_sound(false, true),
            "minecraft:entity.sulfur_cube.bounce"
        );
        assert_eq!(
            squish_sound(false, false),
            "minecraft:entity.sulfur_cube.squish"
        );
        assert_eq!(jump_sound(true), "minecraft:entity.small_sulfur_cube.jump");
        assert_eq!(jump_sound(false), "minecraft:entity.sulfur_cube.jump");
    }

    #[test]
    fn a_player_pushes_a_cube_with_a_block_away_with_a_ground_lift() {
        let push = player_push(
            crate::Vec3::new(1.0, 0.0, 0.0),
            0.98,
            true,
            0.0,
            crate::Vec3::new(0.0, 0.0, 0.0),
            1.8,
            false,
            0.2,
        )
        .unwrap();
        assert!(push.delta.x > 0.0);
        assert_eq!(push.delta.z, 0.0);
        assert!(push.delta.y > 0.0);
        let speed = 0.2 * 2.0 * f64::from(0.3f32);
        assert!((push.delta.x - speed).abs() < 1e-9);
        assert!((push.delta.y - 0.300_000_011_920_928_96 * speed).abs() < 1e-9);
    }

    #[test]
    fn a_distant_or_vertically_separate_pusher_does_nothing() {
        let near = crate::Vec3::new(0.0, 0.0, 0.0);
        assert!(
            player_push(
                crate::Vec3::new(1.4, 0.0, 0.0),
                1.0,
                true,
                0.0,
                near,
                1.8,
                false,
                0.2
            )
            .is_none()
        );
        assert!(
            player_push(
                crate::Vec3::new(1.0, 5.0, 0.0),
                1.0,
                true,
                0.0,
                near,
                1.8,
                false,
                0.2
            )
            .is_none()
        );
        assert!(
            player_push(
                crate::Vec3::new(1.0, -3.0, 0.0),
                1.0,
                true,
                0.0,
                near,
                1.8,
                false,
                0.2
            )
            .is_none()
        );
    }

    #[test]
    fn the_push_is_capped_scaled_down_in_a_vehicle_and_amplified_by_a_negative_resistance() {
        let cube = crate::Vec3::new(1.0, 0.0, 0.0);
        let player = crate::Vec3::ZERO;
        let fast = player_push(cube, 1.0, false, 0.0, player, 1.8, false, 5.0).unwrap();
        assert!((fast.delta.x - 0.5).abs() < 1e-9);
        assert_eq!(fast.delta.y, 0.0);
        let riding = player_push(cube, 1.0, false, 0.0, player, 1.8, true, 0.5).unwrap();
        assert!((riding.delta.x - 0.5 * 2.0 * f64::from(0.16f32)).abs() < 1e-9);
        let bouncy = player_push(cube, 1.0, false, -2.0, player, 1.8, false, 0.1).unwrap();
        assert!((bouncy.delta.x - 3.0 * 0.1 * 2.0 * f64::from(0.3f32)).abs() < 1e-9);
        let immune = player_push(cube, 1.0, false, 1.0, player, 1.8, false, 0.1).unwrap();
        assert_eq!(immune.delta.x, 0.0);
    }

    #[test]
    fn the_push_sound_needs_the_impulse_threshold_and_has_a_cooldown_in_ticks() {
        let sounds = data::archetype("minecraft:bouncy").unwrap().sound_settings;
        assert!(!push_sound_audible(
            crate::Vec3::new(0.29, 0.0, 0.0),
            &sounds
        ));
        assert!(push_sound_audible(
            crate::Vec3::new(0.31, 0.0, 0.0),
            &sounds
        ));
        assert_eq!(push_cooldown_ticks(&sounds), 14);
    }

    #[test]
    fn archetype_modifier_ids_are_recognised_as_transient() {
        assert!(is_archetype_modifier("minecraft:bouncy_add_bounciness"));
        assert!(!is_archetype_modifier("minecraft:baby"));
    }
}

use rspigot_entities::{EntityDimensions, EntityKind};
use rspigot_items::EquipmentSlot;
use rspigot_nbt::{Compound, Tag};

use super::base::EntityBase;
use super::data::SyncedValue;
use super::living::{DATA_HEALTH, DATA_LIVING_FLAGS, EQUIPMENT_SLOTS, slot_index};
use super::physics::{BlockView, MoveTraits, friction, move_entity};
use super::save::{item_stack_from_nbt, item_stack_to_nbt};
use crate::Vec3;
use crate::item::ItemStack;

pub const DATA_CLIENT_FLAGS: u8 = 15;
pub const DATA_HEAD_POSE: u8 = 16;
pub const DATA_BODY_POSE: u8 = 17;
pub const DATA_LEFT_ARM_POSE: u8 = 18;
pub const DATA_RIGHT_ARM_POSE: u8 = 19;
pub const DATA_LEFT_LEG_POSE: u8 = 20;
pub const DATA_RIGHT_LEG_POSE: u8 = 21;
pub const FLAG_SMALL: i8 = 1;
pub const FLAG_SHOW_ARMS: i8 = 4;
pub const FLAG_NO_BASE_PLATE: i8 = 8;
pub const FLAG_MARKER: i8 = 16;
pub const MAX_HEALTH: f32 = 20.0;
pub const HIT_WINDOW: i32 = 5;
pub const KEYS: &[&str] = &[
    "Health",
    "equipment",
    "ArmorItems",
    "HandItems",
    "Invisible",
    "Small",
    "ShowArms",
    "DisabledSlots",
    "NoBasePlate",
    "Marker",
    "Pose",
];

const GRAVITY: f64 = 0.08;
const AIR_DRAG: f64 = 0.98;
const GROUND_DRAG: f32 = 0.91;
const SMALL_SCALE: f32 = 0.5;
const SMALL_EYE_HEIGHT: f32 = 0.9875;

pub const DEFAULT_HEAD: (f32, f32, f32) = (0.0, 0.0, 0.0);
pub const DEFAULT_BODY: (f32, f32, f32) = (0.0, 0.0, 0.0);
pub const DEFAULT_LEFT_ARM: (f32, f32, f32) = (-10.0, 0.0, -10.0);
pub const DEFAULT_RIGHT_ARM: (f32, f32, f32) = (-15.0, 0.0, 10.0);
pub const DEFAULT_LEFT_LEG: (f32, f32, f32) = (-1.0, 0.0, -1.0);
pub const DEFAULT_RIGHT_LEG: (f32, f32, f32) = (1.0, 0.0, 1.0);

const POSES: [(&str, u8, (f32, f32, f32)); 6] = [
    ("Head", DATA_HEAD_POSE, DEFAULT_HEAD),
    ("Body", DATA_BODY_POSE, DEFAULT_BODY),
    ("LeftArm", DATA_LEFT_ARM_POSE, DEFAULT_LEFT_ARM),
    ("RightArm", DATA_RIGHT_ARM_POSE, DEFAULT_RIGHT_ARM),
    ("LeftLeg", DATA_LEFT_LEG_POSE, DEFAULT_LEFT_LEG),
    ("RightLeg", DATA_RIGHT_LEG_POSE, DEFAULT_RIGHT_LEG),
];

#[derive(Debug, Clone, PartialEq)]
pub struct StandState {
    pub equipment: [Option<ItemStack>; 8],
    pub disabled_slots: i32,
    pub last_hit: Option<i32>,
}

pub fn define(base: &mut EntityBase) -> StandState {
    base.data.define(DATA_LIVING_FLAGS, SyncedValue::Byte(0));
    base.data.define(DATA_HEALTH, SyncedValue::Float(1.0));
    base.data.set(DATA_HEALTH, SyncedValue::Float(MAX_HEALTH));
    base.data.define(DATA_CLIENT_FLAGS, SyncedValue::Byte(0));
    for (_, index, default) in POSES {
        base.data.define(
            index,
            SyncedValue::Rotations(default.0, default.1, default.2),
        );
    }
    StandState {
        equipment: Default::default(),
        disabled_slots: 0,
        last_hit: None,
    }
}

pub fn health(base: &EntityBase) -> f32 {
    super::living::health(base)
}

pub fn set_health(base: &mut EntityBase, health: f32) {
    base.data.set(
        DATA_HEALTH,
        SyncedValue::Float(health.clamp(0.0, MAX_HEALTH)),
    );
}

fn flag(base: &EntityBase, mask: i8) -> bool {
    base.data.byte(DATA_CLIENT_FLAGS) & mask != 0
}

fn set_flag(base: &mut EntityBase, mask: i8, value: bool) {
    let current = base.data.byte(DATA_CLIENT_FLAGS);
    let next = if value {
        current | mask
    } else {
        current & !mask
    };
    base.data.set(DATA_CLIENT_FLAGS, SyncedValue::Byte(next));
}

pub fn is_small(base: &EntityBase) -> bool {
    flag(base, FLAG_SMALL)
}

pub fn shows_arms(base: &EntityBase) -> bool {
    flag(base, FLAG_SHOW_ARMS)
}

pub fn shows_base_plate(base: &EntityBase) -> bool {
    !flag(base, FLAG_NO_BASE_PLATE)
}

pub fn is_marker(base: &EntityBase) -> bool {
    flag(base, FLAG_MARKER)
}

pub fn is_invisible(base: &EntityBase) -> bool {
    base.shared_flag(super::base::FLAG_INVISIBLE)
}

pub fn set_invisible(base: &mut EntityBase, invisible: bool) {
    base.set_shared_flag(super::base::FLAG_INVISIBLE, invisible);
}

pub fn refresh_dimensions(base: &mut EntityBase) {
    let normal = EntityKind::ArmorStand.dimensions();
    base.dimensions = if is_marker(base) {
        EntityDimensions {
            width: 0.0,
            height: 0.0,
            eye_height: 0.0,
        }
    } else if is_small(base) {
        EntityDimensions {
            width: normal.width * SMALL_SCALE,
            height: normal.height * SMALL_SCALE,
            eye_height: SMALL_EYE_HEIGHT,
        }
    } else {
        normal
    };
}

pub fn set_small(base: &mut EntityBase, small: bool) {
    set_flag(base, FLAG_SMALL, small);
    refresh_dimensions(base);
}

pub fn set_show_arms(base: &mut EntityBase, shows: bool) {
    set_flag(base, FLAG_SHOW_ARMS, shows);
}

pub fn set_no_base_plate(base: &mut EntityBase, none: bool) {
    set_flag(base, FLAG_NO_BASE_PLATE, none);
}

pub fn set_marker(base: &mut EntityBase, marker: bool) {
    set_flag(base, FLAG_MARKER, marker);
    refresh_dimensions(base);
    base.no_physics = !has_physics(base);
}

pub fn has_physics(base: &EntityBase) -> bool {
    !is_marker(base) && !base.is_no_gravity()
}

pub fn pose(base: &EntityBase, index: u8) -> (f32, f32, f32) {
    base.data.rotations(index)
}

pub fn set_pose(base: &mut EntityBase, index: u8, rotation: (f32, f32, f32)) {
    base.data.set(
        index,
        SyncedValue::Rotations(rotation.0, rotation.1, rotation.2),
    );
}

pub const fn filter_bit(slot: EquipmentSlot) -> i32 {
    match slot {
        EquipmentSlot::Mainhand | EquipmentSlot::Body | EquipmentSlot::Saddle => 0,
        EquipmentSlot::Feet => 1,
        EquipmentSlot::Legs => 2,
        EquipmentSlot::Chest => 3,
        EquipmentSlot::Head => 4,
        EquipmentSlot::Offhand => 5,
    }
}

pub const fn is_hand(slot: EquipmentSlot) -> bool {
    matches!(slot, EquipmentSlot::Mainhand | EquipmentSlot::Offhand)
}

pub fn is_disabled(base: &EntityBase, state: &StandState, slot: EquipmentSlot) -> bool {
    state.disabled_slots & (1 << filter_bit(slot)) != 0 || (is_hand(slot) && !shows_arms(base))
}

pub fn can_use_slot(base: &EntityBase, state: &StandState, slot: EquipmentSlot) -> bool {
    !matches!(slot, EquipmentSlot::Body | EquipmentSlot::Saddle) && !is_disabled(base, state, slot)
}

pub fn slot_locked_for_taking(state: &StandState, slot: EquipmentSlot) -> bool {
    state.disabled_slots & (1 << (filter_bit(slot) + 8)) != 0
}

pub fn slot_locked_for_putting(state: &StandState, slot: EquipmentSlot) -> bool {
    state.disabled_slots & (1 << (filter_bit(slot) + 16)) != 0
}

pub fn item_in(state: &StandState, slot: EquipmentSlot) -> Option<&ItemStack> {
    state.equipment[slot_index(slot)].as_ref()
}

pub fn set_item(state: &mut StandState, slot: EquipmentSlot, stack: Option<ItemStack>) {
    state.equipment[slot_index(slot)] = stack;
}

pub fn slot_for_item(base: &EntityBase, state: &StandState, stack: &ItemStack) -> EquipmentSlot {
    crate::stack_components::equippable(stack)
        .map(|found| found.slot)
        .filter(|slot| can_use_slot(base, state, *slot))
        .unwrap_or(EquipmentSlot::Mainhand)
}

pub fn clicked_slot(base: &EntityBase, state: &StandState, height: f64) -> EquipmentSlot {
    let small = is_small(base);
    let scale = if small { 0.5 } else { 1.0 };
    let y = height / scale;
    let has = |slot| item_in(state, slot).is_some();
    let pick = |low: f64, extent: f64| y >= low && y < low + extent;
    if pick(0.1, if small { 0.8 } else { 0.45 }) && has(EquipmentSlot::Feet) {
        EquipmentSlot::Feet
    } else if pick(
        0.9 + if small { 0.3 } else { 0.0 },
        if small { 1.0 } else { 0.7 },
    ) && has(EquipmentSlot::Chest)
    {
        EquipmentSlot::Chest
    } else if pick(0.4, if small { 1.0 } else { 0.8 }) && has(EquipmentSlot::Legs) {
        EquipmentSlot::Legs
    } else if y >= 1.6 && has(EquipmentSlot::Head) {
        EquipmentSlot::Head
    } else if !has(EquipmentSlot::Mainhand) && has(EquipmentSlot::Offhand) {
        EquipmentSlot::Offhand
    } else {
        EquipmentSlot::Mainhand
    }
}

pub fn tick<V: BlockView>(base: &mut EntityBase, view: &V) {
    base.old_position = base.position;
    base.tick_count = base.tick_count.wrapping_add(1);
    if !has_physics(base) {
        return;
    }
    base.velocity = base.velocity + Vec3::new(0.0, -GRAVITY, 0.0);
    let traits = MoveTraits::plain(GRAVITY);
    move_entity(base, view, base.velocity, &traits);
    let horizontal = if base.on_ground {
        friction(view.block_or_air(super::physics::ground_pos(base, view))) * GROUND_DRAG
    } else {
        GROUND_DRAG
    };
    base.velocity = base
        .velocity
        .multiply(f64::from(horizontal), AIR_DRAG, f64::from(horizontal));
    if base.on_ground && base.velocity.y < 0.0 {
        base.velocity.y = 0.0;
    }
}

pub fn read(nbt: &Compound, base: &mut EntityBase, state: &mut StandState) {
    set_invisible(base, nbt.get_bool("Invisible").unwrap_or(false));
    set_small(base, nbt.get_bool("Small").unwrap_or(false));
    set_show_arms(base, nbt.get_bool("ShowArms").unwrap_or(false));
    state.disabled_slots = nbt.get_i32("DisabledSlots").unwrap_or(0);
    set_no_base_plate(base, nbt.get_bool("NoBasePlate").unwrap_or(false));
    set_marker(base, nbt.get_bool("Marker").unwrap_or(false));
    let health = nbt.get_f64("Health").map_or(MAX_HEALTH, |v| v as f32);
    set_health(base, health);
    if let Some(hands) = nbt.get_list("HandItems") {
        for (slot, tag) in [EquipmentSlot::Mainhand, EquipmentSlot::Offhand]
            .into_iter()
            .zip(hands)
        {
            state.equipment[slot_index(slot)] = tag.as_compound().and_then(item_stack_from_nbt);
        }
    }
    if let Some(armor) = nbt.get_list("ArmorItems") {
        for (slot, tag) in [
            EquipmentSlot::Feet,
            EquipmentSlot::Legs,
            EquipmentSlot::Chest,
            EquipmentSlot::Head,
        ]
        .into_iter()
        .zip(armor)
        {
            state.equipment[slot_index(slot)] = tag.as_compound().and_then(item_stack_from_nbt);
        }
    }
    if let Some(equipment) = nbt.get_compound("equipment") {
        for slot in EQUIPMENT_SLOTS {
            state.equipment[slot_index(slot)] = equipment
                .get_compound(slot.name())
                .and_then(item_stack_from_nbt);
        }
    }
    if let Some(pose) = nbt.get_compound("Pose") {
        for (name, index, default) in POSES {
            let rotation = pose
                .get_list(name)
                .filter(|list| list.len() == 3)
                .and_then(|list| {
                    Some((
                        list[0].as_f64()? as f32,
                        list[1].as_f64()? as f32,
                        list[2].as_f64()? as f32,
                    ))
                })
                .unwrap_or(default);
            set_pose(base, index, rotation);
        }
    }
    base.no_physics = !has_physics(base);
}

pub fn write(base: &EntityBase, state: &StandState, out: &mut Compound) {
    out.insert("Health", health(base));
    out.insert("Invisible", is_invisible(base));
    out.insert("Small", is_small(base));
    out.insert("ShowArms", shows_arms(base));
    out.insert("DisabledSlots", state.disabled_slots);
    out.insert("NoBasePlate", !shows_base_plate(base));
    if is_marker(base) {
        out.insert("Marker", true);
    }
    let equipment: Compound = EQUIPMENT_SLOTS
        .iter()
        .filter_map(|slot| {
            state.equipment[slot_index(*slot)].as_ref().map(|stack| {
                (
                    slot.name().to_owned(),
                    Tag::Compound(item_stack_to_nbt(stack)),
                )
            })
        })
        .collect();
    if !equipment.is_empty() {
        out.insert("equipment", equipment);
    }
    let mut pose = Compound::new();
    for (name, index, default) in POSES {
        let rotation = self::pose(base, index);
        if rotation != default {
            pose.insert(
                name,
                Tag::List(vec![
                    Tag::Float(rotation.0),
                    Tag::Float(rotation.1),
                    Tag::Float(rotation.2),
                ]),
            );
        }
    }
    out.insert("Pose", pose);
}

#[cfg(test)]
mod tests {
    use uuid::Uuid;

    use super::*;
    use crate::BlockPos;
    use crate::EntityId;
    use crate::entity::save::{load_entity, save_entity};
    use crate::entity::{Entity, EntityState};

    struct Ground;

    impl BlockView for Ground {
        fn block(&self, pos: BlockPos) -> Option<crate::BlockStateId> {
            Some(if pos.y < 64 {
                rspigot_blocks::BlockKind::Stone.default_state()
            } else {
                crate::BlockStateId::AIR
            })
        }
    }

    fn stand(nbt: Compound) -> Entity {
        load_entity(
            &nbt.with("id", "minecraft:armor_stand").with(
                "Pos",
                Tag::List(vec![Tag::Double(0.5), Tag::Double(64.0), Tag::Double(0.5)]),
            ),
            EntityId(1),
            Uuid::from_u128(1),
        )
        .unwrap()
    }

    fn parts(entity: &mut Entity) -> (&mut EntityBase, &mut StandState) {
        match &mut entity.state {
            EntityState::Stand(state) => (&mut entity.base, state),
            other => panic!("expected an armor stand, got {other:?}"),
        }
    }

    fn stack(name: &str) -> Compound {
        Compound::new().with("id", name).with("count", 1)
    }

    #[test]
    fn a_fresh_stand_is_vanilla_sized_with_the_default_poses() {
        let mut entity = stand(Compound::new());
        let (base, state) = parts(&mut entity);
        assert_eq!(health(base), 20.0);
        assert!(!is_small(base) && !is_marker(base) && shows_base_plate(base));
        assert_eq!(base.dimensions.height, 1.975);
        assert_eq!(pose(base, DATA_LEFT_ARM_POSE), DEFAULT_LEFT_ARM);
        assert!(state.equipment.iter().all(Option::is_none));
    }

    #[test]
    fn small_and_marker_resize_the_stand() {
        let mut entity = stand(Compound::new().with("Small", true));
        let (base, _) = parts(&mut entity);
        assert_eq!(base.dimensions.height, 1.975 * 0.5);
        assert_eq!(base.dimensions.eye_height, 0.9875);
        set_marker(base, true);
        assert_eq!(base.dimensions.width, 0.0);
        assert!(base.no_physics);
    }

    #[test]
    fn the_nbt_round_trips_flags_poses_and_equipment() {
        let nbt = Compound::new()
            .with("Invisible", true)
            .with("Small", true)
            .with("ShowArms", true)
            .with("NoBasePlate", true)
            .with("DisabledSlots", 4144959)
            .with(
                "Pose",
                Compound::new().with(
                    "Head",
                    Tag::List(vec![Tag::Float(10.0), Tag::Float(20.0), Tag::Float(30.0)]),
                ),
            )
            .with(
                "equipment",
                Compound::new()
                    .with("head", stack("minecraft:iron_helmet"))
                    .with("mainhand", stack("minecraft:stick")),
            );
        let mut entity = stand(nbt);
        {
            let (base, state) = parts(&mut entity);
            assert!(is_invisible(base) && shows_arms(base) && !shows_base_plate(base));
            assert_eq!(pose(base, DATA_HEAD_POSE), (10.0, 20.0, 30.0));
            assert_eq!(state.disabled_slots, 4144959);
            assert!(item_in(state, EquipmentSlot::Head).is_some());
        }
        let saved = save_entity(&entity, None).unwrap();
        assert_eq!(saved.get_bool("Small"), Some(true));
        assert_eq!(saved.get_i32("DisabledSlots"), Some(4144959));
        assert!(saved.get_compound("Pose").unwrap().contains_key("Head"));
        assert!(!saved.get_compound("Pose").unwrap().contains_key("Body"));
        let again = load_entity(&saved, EntityId(1), Uuid::from_u128(1)).unwrap();
        assert_eq!(save_entity(&again, None), Some(saved));
    }

    #[test]
    fn the_legacy_hand_and_armor_lists_are_read() {
        let nbt = Compound::new()
            .with(
                "HandItems",
                Tag::List(vec![
                    Tag::Compound(stack("minecraft:stick")),
                    Tag::Compound(Compound::new()),
                ]),
            )
            .with(
                "ArmorItems",
                Tag::List(vec![
                    Tag::Compound(Compound::new()),
                    Tag::Compound(Compound::new()),
                    Tag::Compound(stack("minecraft:iron_chestplate")),
                    Tag::Compound(Compound::new()),
                ]),
            );
        let mut entity = stand(nbt);
        let (_, state) = parts(&mut entity);
        assert!(item_in(state, EquipmentSlot::Mainhand).is_some());
        assert!(item_in(state, EquipmentSlot::Chest).is_some());
        assert!(item_in(state, EquipmentSlot::Offhand).is_none());
    }

    #[test]
    fn disabled_slots_follow_the_vanilla_bit_layout() {
        let mut entity = stand(Compound::new().with("DisabledSlots", 1 << 4));
        let (base, state) = parts(&mut entity);
        assert!(is_disabled(base, state, EquipmentSlot::Head));
        assert!(!is_disabled(base, state, EquipmentSlot::Chest));
        assert!(is_disabled(base, state, EquipmentSlot::Mainhand));
        set_show_arms(base, true);
        assert!(!is_disabled(base, state, EquipmentSlot::Mainhand));
        assert!(!can_use_slot(base, state, EquipmentSlot::Body));
        state.disabled_slots = 1 << (3 + 8);
        assert!(slot_locked_for_taking(state, EquipmentSlot::Chest));
        state.disabled_slots = 1 << (3 + 16);
        assert!(slot_locked_for_putting(state, EquipmentSlot::Chest));
    }

    #[test]
    fn the_clicked_height_picks_the_worn_piece() {
        let mut entity = stand(
            Compound::new().with(
                "equipment",
                Compound::new()
                    .with("head", stack("minecraft:iron_helmet"))
                    .with("chest", stack("minecraft:iron_chestplate"))
                    .with("legs", stack("minecraft:iron_leggings"))
                    .with("feet", stack("minecraft:iron_boots")),
            ),
        );
        let (base, state) = parts(&mut entity);
        assert_eq!(clicked_slot(base, state, 1.7), EquipmentSlot::Head);
        assert_eq!(clicked_slot(base, state, 1.2), EquipmentSlot::Chest);
        assert_eq!(clicked_slot(base, state, 0.6), EquipmentSlot::Legs);
        assert_eq!(clicked_slot(base, state, 0.2), EquipmentSlot::Feet);
        assert_eq!(clicked_slot(base, state, 0.05), EquipmentSlot::Mainhand);
    }

    #[test]
    fn a_stand_falls_until_it_stands_on_the_ground() {
        let mut entity = stand(Compound::new());
        let (base, _) = parts(&mut entity);
        base.position = Vec3::new(0.5, 70.0, 0.5);
        for _ in 0..60 {
            tick(base, &Ground);
        }
        assert!(
            (base.position.y - 64.0).abs() < 1.0e-6,
            "{}",
            base.position.y
        );
        assert!(base.on_ground);
    }

    #[test]
    fn a_marker_does_not_fall() {
        let mut entity = stand(Compound::new().with("Marker", true));
        let (base, _) = parts(&mut entity);
        base.position = Vec3::new(0.5, 70.0, 0.5);
        for _ in 0..20 {
            tick(base, &Ground);
        }
        assert_eq!(base.position.y, 70.0);
    }

    fn stick_for(slot_name: &str) -> ItemStack {
        let mut stack = ItemStack::new(
            rspigot_items::ItemKind::Stick.id(),
            std::num::NonZeroU8::MIN,
        );
        stack.components.insert(
            "minecraft:equippable",
            rspigot_nbt::Compound::new().with("slot", slot_name),
        );
        stack
    }

    #[test]
    fn the_equippable_component_picks_the_armor_stand_slot() {
        let mut base = EntityBase::new(
            EntityId(1),
            Uuid::nil(),
            rspigot_entities::EntityKind::ArmorStand,
        );
        let state = define(&mut base);
        assert_eq!(
            slot_for_item(&base, &state, &stick_for("head")),
            EquipmentSlot::Head
        );
    }
}

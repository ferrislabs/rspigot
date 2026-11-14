use rspigot_blocks::{BlockKind, Face, StateFlags};
use rspigot_entities::EntityKind;
use rspigot_nbt::{Compound, Tag};

use super::aabb::Aabb;
use super::base::EntityBase;
use super::data::SyncedValue;
use super::physics::{BlockView, block_colliders};
use super::save::{item_stack_from_nbt, item_stack_to_nbt};
use crate::item::ItemStack;
use crate::{BlockPos, EntityId, Vec3};

pub const DATA_DIRECTION: u8 = 8;
pub const DATA_ITEM: u8 = 9;
pub const DATA_ROTATION: u8 = 10;
pub const DATA_VARIANT: u8 = 9;
pub const KEYS: &[&str] = &[
    "block_pos",
    "TileX",
    "TileY",
    "TileZ",
    "Facing",
    "facing",
    "Item",
    "ItemRotation",
    "ItemDropChance",
    "Invisible",
    "Fixed",
    "variant",
];
pub const CHECK_INTERVAL: i32 = 100;
pub const PAINTING_REGISTRY: &str = "minecraft:painting_variant";
pub const PLACEABLE_TAG: &str = "minecraft:placeable";

const OFFSET_FROM_WALL: f64 = 0.46875;
const THIN: f64 = 0.0625;
const SUPPORT_DEFLATE: f64 = 1.0e-7;
const MAP_ID: &str = "minecraft:map_id";

pub const PAINTINGS: &[(&str, i32, i32)] = &[
    ("alban", 1, 1),
    ("aztec", 1, 1),
    ("aztec2", 1, 1),
    ("backyard", 3, 4),
    ("baroque", 2, 2),
    ("bomb", 1, 1),
    ("bouquet", 3, 3),
    ("burning_skull", 4, 4),
    ("bust", 2, 2),
    ("cavebird", 3, 3),
    ("changing", 4, 2),
    ("cotan", 3, 3),
    ("courbet", 2, 1),
    ("creebet", 2, 1),
    ("dennis", 3, 3),
    ("donkey_kong", 4, 3),
    ("earth", 2, 2),
    ("endboss", 3, 3),
    ("fern", 3, 3),
    ("fighters", 4, 2),
    ("finding", 4, 2),
    ("fire", 2, 2),
    ("graham", 1, 2),
    ("humble", 2, 2),
    ("kebab", 1, 1),
    ("lowmist", 4, 2),
    ("match", 2, 2),
    ("meditative", 1, 1),
    ("orb", 4, 4),
    ("owlemons", 3, 3),
    ("passage", 4, 2),
    ("pigscene", 4, 4),
    ("plant", 1, 1),
    ("pointer", 4, 4),
    ("pond", 3, 4),
    ("pool", 2, 1),
    ("prairie_ride", 1, 2),
    ("sea", 2, 1),
    ("skeleton", 4, 3),
    ("skull_and_roses", 2, 2),
    ("stage", 2, 2),
    ("sunflowers", 3, 3),
    ("sunset", 2, 1),
    ("tides", 3, 3),
    ("unpacked", 4, 4),
    ("void", 2, 2),
    ("wanderer", 1, 2),
    ("wasteland", 1, 1),
    ("water", 2, 2),
    ("wind", 2, 2),
    ("wither", 2, 2),
];

#[derive(Debug, Clone, PartialEq)]
pub struct HangingState {
    pub pos: BlockPos,
    pub drop_chance: f32,
    pub fixed: bool,
    pub ticks_since_check: i32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Neighbour {
    pub id: EntityId,
    pub kind: EntityKind,
    pub facing: Face,
    pub bounds: Aabb,
}

pub const fn is_frame(kind: EntityKind) -> bool {
    matches!(kind, EntityKind::ItemFrame | EntityKind::GlowItemFrame)
}

pub const fn is_hanging(kind: EntityKind) -> bool {
    is_frame(kind) || matches!(kind, EntityKind::Painting)
}

pub fn define(base: &mut EntityBase) -> HangingState {
    base.data
        .define(DATA_DIRECTION, SyncedValue::Direction(Face::South));
    if is_frame(base.kind) {
        base.data.define(DATA_ITEM, SyncedValue::Item(None));
        base.data.define(DATA_ROTATION, SyncedValue::Int(0));
    } else {
        base.data
            .define(DATA_VARIANT, SyncedValue::PaintingVariant(0));
    }
    HangingState {
        pos: BlockPos::new(0, 0, 0),
        drop_chance: 1.0,
        fixed: false,
        ticks_since_check: 0,
    }
}

pub const fn data_3d(face: Face) -> i32 {
    match face {
        Face::Down => 0,
        Face::Up => 1,
        Face::North => 2,
        Face::South => 3,
        Face::West => 4,
        Face::East => 5,
    }
}

pub const fn from_3d(value: i32) -> Option<Face> {
    Some(match value {
        0 => Face::Down,
        1 => Face::Up,
        2 => Face::North,
        3 => Face::South,
        4 => Face::West,
        5 => Face::East,
        _ => return None,
    })
}

pub const fn data_2d(face: Face) -> i32 {
    match face {
        Face::South | Face::Down | Face::Up => 0,
        Face::West => 1,
        Face::North => 2,
        Face::East => 3,
    }
}

pub const fn from_2d(value: i32) -> Face {
    match value.rem_euclid(4) {
        0 => Face::South,
        1 => Face::West,
        2 => Face::North,
        _ => Face::East,
    }
}

pub const fn counter_clockwise(face: Face) -> Face {
    match face {
        Face::North => Face::West,
        Face::West => Face::South,
        Face::South => Face::East,
        Face::East => Face::North,
        other => other,
    }
}

pub const fn is_horizontal(face: Face) -> bool {
    !matches!(face, Face::Down | Face::Up)
}

pub fn facing(base: &EntityBase) -> Face {
    base.data.direction(DATA_DIRECTION)
}

pub fn variant(base: &EntityBase) -> u32 {
    base.data.painting_variant(DATA_VARIANT)
}

pub fn set_variant(base: &mut EntityBase, id: u32) {
    base.data
        .set(DATA_VARIANT, SyncedValue::PaintingVariant(id));
}

pub fn variant_name(id: u32) -> Option<&'static str> {
    PAINTINGS.get(id as usize).map(|(name, _, _)| *name)
}

pub fn variant_by_name(name: &str) -> Option<u32> {
    let name = name.strip_prefix("minecraft:").unwrap_or(name);
    PAINTINGS
        .iter()
        .position(|(candidate, _, _)| *candidate == name)
        .map(|index| index as u32)
}

pub fn variant_size(id: u32) -> (i32, i32) {
    PAINTINGS
        .get(id as usize)
        .map_or((1, 1), |(_, width, height)| (*width, *height))
}

pub fn variant_area(id: u32) -> i32 {
    let (width, height) = variant_size(id);
    width * height
}

pub fn placeable_variants() -> Vec<u32> {
    rspigot_data::tags::tag(PAINTING_REGISTRY, PLACEABLE_TAG)
        .map(|ids| ids.to_vec())
        .unwrap_or_default()
}

pub fn item(base: &EntityBase) -> Option<&ItemStack> {
    match base.data.get(DATA_ITEM) {
        Some(SyncedValue::Item(stack)) => stack.as_ref(),
        _ => None,
    }
}

pub fn has_map(base: &EntityBase) -> bool {
    item(base).is_some_and(|stack| stack.components.contains_key(MAP_ID))
}

pub fn rotation(base: &EntityBase) -> i32 {
    base.data.int(DATA_ROTATION)
}

pub fn set_rotation(base: &mut EntityBase, rotation: i32) {
    base.data
        .set(DATA_ROTATION, SyncedValue::Int(rotation.rem_euclid(8)));
}

pub fn is_invisible(base: &EntityBase) -> bool {
    base.shared_flag(super::base::FLAG_INVISIBLE)
}

pub fn set_invisible(base: &mut EntityBase, invisible: bool) {
    base.set_shared_flag(super::base::FLAG_INVISIBLE, invisible);
}

pub fn analog_output(base: &EntityBase) -> i32 {
    if item(base).is_none() {
        0
    } else {
        rotation(base) % 8 + 1
    }
}

fn center_of(pos: BlockPos) -> Vec3 {
    Vec3::new(
        f64::from(pos.x) + 0.5,
        f64::from(pos.y) + 0.5,
        f64::from(pos.z) + 0.5,
    )
}

fn against_wall(pos: BlockPos, facing: Face) -> Vec3 {
    let (dx, dy, dz) = facing.offset();
    let center = center_of(pos);
    Vec3::new(
        center.x + f64::from(dx) * -OFFSET_FROM_WALL,
        center.y + f64::from(dy) * -OFFSET_FROM_WALL,
        center.z + f64::from(dz) * -OFFSET_FROM_WALL,
    )
}

fn sized(center: Vec3, x: f64, y: f64, z: f64) -> Aabb {
    Aabb::new(
        center.x - x / 2.0,
        center.y - y / 2.0,
        center.z - z / 2.0,
        center.x + x / 2.0,
        center.y + y / 2.0,
        center.z + z / 2.0,
    )
}

pub fn frame_box(pos: BlockPos, facing: Face, map: bool) -> Aabb {
    let size = if map { 1.0 } else { 0.75 };
    let (x, y, z) = match facing {
        Face::East | Face::West => (THIN, size, size),
        Face::Up | Face::Down => (size, THIN, size),
        Face::North | Face::South => (size, size, THIN),
    };
    sized(against_wall(pos, facing), x, y, z)
}

fn painting_offset(size: i32) -> f64 {
    if size % 2 == 0 { 0.5 } else { 0.0 }
}

pub fn painting_box(pos: BlockPos, facing: Face, width: i32, height: i32) -> Aabb {
    let mut center = against_wall(pos, facing);
    let (dx, _, dz) = counter_clockwise(facing).offset();
    let along = painting_offset(width);
    center.x += f64::from(dx) * along;
    center.z += f64::from(dz) * along;
    center.y += painting_offset(height);
    let (x, z) = match facing {
        Face::East | Face::West => (THIN, f64::from(width)),
        _ => (f64::from(width), THIN),
    };
    sized(center, x, f64::from(height), z)
}

pub fn bounds_of(kind: EntityKind, pos: BlockPos, facing: Face, map: bool, variant: u32) -> Aabb {
    if is_frame(kind) {
        frame_box(pos, facing, map)
    } else {
        let (width, height) = variant_size(variant);
        painting_box(pos, facing, width, height)
    }
}

pub fn pop_box(base: &EntityBase, state: &HangingState) -> Aabb {
    if is_frame(base.kind) {
        frame_box(state.pos, facing(base), false)
    } else {
        base.bounding_box()
    }
}

pub fn orient(base: &mut EntityBase, facing: Face) {
    base.data
        .set(DATA_DIRECTION, SyncedValue::Direction(facing));
    if is_frame(base.kind) && !is_horizontal(facing) {
        let (_, dy, _) = facing.offset();
        base.set_pitch((dy * -90) as f32);
        base.set_yaw(0.0);
    } else {
        base.set_pitch(0.0);
        base.set_yaw((data_2d(facing) * 90) as f32);
    }
}

pub fn recalculate(base: &mut EntityBase, state: &HangingState) {
    let bounds = bounds_of(
        base.kind,
        state.pos,
        facing(base),
        has_map(base),
        variant(base),
    );
    let center = bounds.center();
    base.position = center;
    base.old_position = center;
    base.custom_box = Some(bounds.move_by(Vec3::new(-center.x, -center.y, -center.z)));
    base.needs_sync = true;
}

pub fn place(base: &mut EntityBase, state: &mut HangingState, pos: BlockPos, facing: Face) {
    state.pos = pos;
    orient(base, facing);
    recalculate(base, state);
}

pub fn set_item(base: &mut EntityBase, state: &HangingState, stack: Option<ItemStack>) {
    let stack = stack.map(|mut stack| {
        stack.count = std::num::NonZeroU8::MIN;
        stack
    });
    base.data.set(DATA_ITEM, SyncedValue::Item(stack));
    recalculate(base, state);
}

pub fn is_diode(state: crate::BlockStateId) -> bool {
    matches!(
        state.kind(),
        Some(BlockKind::Repeater | BlockKind::Comparator)
    )
}

fn supporting(state: crate::BlockStateId) -> bool {
    state.flags().contains(StateFlags::LEGACY_SOLID) || is_diode(state)
}

fn collides<V: BlockView>(view: &V, area: &Aabb) -> bool {
    !block_colliders(view, area).is_empty()
}

pub fn coexists(
    base: &EntityBase,
    others: &[Neighbour],
    area: &Aabb,
    allow_same_type: bool,
) -> bool {
    let facing = facing(base);
    !others.iter().any(|other| {
        other.id != base.id
            && other.bounds.intersects(area)
            && ((!allow_same_type && other.kind == base.kind) || other.facing == facing)
    })
}

pub fn survives<V: BlockView>(
    view: &V,
    base: &EntityBase,
    state: &HangingState,
    others: &[Neighbour],
) -> bool {
    if is_frame(base.kind) {
        return frame_survives(view, base, state, others);
    }
    let area = pop_box(base, state);
    if collides(view, &area) {
        return false;
    }
    let (dx, dy, dz) = facing(base).offset();
    let support = base
        .bounding_box()
        .move_by(Vec3::new(
            f64::from(dx) * -0.5,
            f64::from(dy) * -0.5,
            f64::from(dz) * -0.5,
        ))
        .deflate(SUPPORT_DEFLATE);
    let lows = (
        support.min_x.floor() as i32,
        support.min_y.floor() as i32,
        support.min_z.floor() as i32,
    );
    let highs = (
        support.max_x.floor() as i32,
        support.max_y.floor() as i32,
        support.max_z.floor() as i32,
    );
    for y in lows.1..=highs.1 {
        for z in lows.2..=highs.2 {
            for x in lows.0..=highs.0 {
                if !supporting(view.block_or_air(BlockPos::new(x, y, z))) {
                    return false;
                }
            }
        }
    }
    coexists(base, others, &area, false)
}

fn frame_survives<V: BlockView>(
    view: &V,
    base: &EntityBase,
    state: &HangingState,
    others: &[Neighbour],
) -> bool {
    if state.fixed {
        return true;
    }
    let area = pop_box(base, state);
    if collides(view, &area) {
        return false;
    }
    let face = facing(base);
    let (dx, dy, dz) = face.opposite().offset();
    let behind = view.block_or_air(state.pos.offset(dx, dy, dz));
    let held = behind.flags().contains(StateFlags::LEGACY_SOLID)
        || (is_horizontal(face) && is_diode(behind));
    held && coexists(base, others, &area, true)
}

pub fn read(nbt: &Compound, base: &mut EntityBase, state: &mut HangingState) {
    let from_position = base.position.block();
    state.pos = nbt
        .get_int_array("block_pos")
        .filter(|parts| parts.len() == 3)
        .map(|parts| BlockPos::new(parts[0], parts[1], parts[2]))
        .or_else(|| {
            Some(BlockPos::new(
                nbt.get_i32("TileX")?,
                nbt.get_i32("TileY")?,
                nbt.get_i32("TileZ")?,
            ))
        })
        .unwrap_or(from_position);
    if is_frame(base.kind) {
        let facing = nbt
            .get_i64("Facing")
            .and_then(|value| from_3d(value as i32))
            .unwrap_or(Face::Down);
        let stack = nbt.get_compound("Item").and_then(item_stack_from_nbt);
        base.data.set(DATA_ITEM, SyncedValue::Item(stack));
        set_rotation(
            base,
            nbt.get_i64("ItemRotation").map_or(0, |value| value as i32),
        );
        state.drop_chance = nbt
            .get_f64("ItemDropChance")
            .map_or(1.0, |value| value as f32);
        state.fixed = nbt.get_bool("Fixed").unwrap_or(false);
        set_invisible(base, nbt.get_bool("Invisible").unwrap_or(false));
        place(base, state, state.pos, facing);
    } else {
        let facing = nbt
            .get_i64("facing")
            .map_or(Face::South, |value| from_2d(value as i32));
        let id = nbt
            .get_str("variant")
            .and_then(variant_by_name)
            .unwrap_or(0);
        set_variant(base, id);
        place(base, state, state.pos, facing);
    }
}

pub fn write(base: &EntityBase, state: &HangingState, out: &mut Compound) {
    out.insert(
        "block_pos",
        Tag::IntArray(vec![state.pos.x, state.pos.y, state.pos.z]),
    );
    if is_frame(base.kind) {
        if let Some(stack) = item(base) {
            out.insert("Item", item_stack_to_nbt(stack));
        }
        out.insert("ItemRotation", rotation(base) as i8);
        out.insert("ItemDropChance", state.drop_chance);
        out.insert("Facing", data_3d(facing(base)) as i8);
        out.insert("Invisible", is_invisible(base));
        out.insert("Fixed", state.fixed);
    } else {
        out.insert("facing", data_2d(facing(base)) as i8);
        if let Some(name) = variant_name(variant(base)) {
            out.insert("variant", format!("minecraft:{name}"));
        }
    }
}

#[cfg(test)]
mod tests {
    use uuid::Uuid;

    use super::*;
    use crate::entity::save::{load_entity, save_entity};
    use crate::entity::{Entity, EntityState};

    struct Wall;

    impl BlockView for Wall {
        fn block(&self, pos: BlockPos) -> Option<crate::BlockStateId> {
            Some(if pos.z >= 1 {
                BlockKind::Stone.default_state()
            } else {
                crate::BlockStateId::AIR
            })
        }
    }

    fn load(id: &str, nbt: Compound) -> Entity {
        load_entity(&nbt.with("id", id), EntityId(1), Uuid::from_u128(1)).unwrap()
    }

    fn state_of(entity: &Entity) -> &HangingState {
        match &entity.state {
            EntityState::Hanging(state) => state,
            other => panic!("expected a hanging entity, got {other:?}"),
        }
    }

    #[test]
    fn the_variant_table_follows_the_synced_registry_order() {
        let names: Vec<&str> = PAINTINGS.iter().map(|(name, _, _)| *name).collect();
        let registry: Vec<&str> = rspigot_data::registries::synced_entries(PAINTING_REGISTRY)
            .unwrap()
            .iter()
            .map(|name| name.strip_prefix("minecraft:").unwrap())
            .collect();
        assert_eq!(names, registry);
    }

    #[test]
    fn the_placeable_tag_leaves_out_the_elemental_paintings() {
        let placeable = placeable_variants();
        let kebab = variant_by_name("kebab").unwrap();
        let earth = variant_by_name("earth").unwrap();
        assert!(placeable.contains(&kebab));
        assert!(!placeable.contains(&earth));
    }

    #[test]
    fn an_item_frame_box_hugs_the_wall_it_hangs_on() {
        let bounds = frame_box(BlockPos::new(0, 64, 0), Face::North, false);
        assert!((bounds.min_z - 0.9375).abs() < 1.0e-9);
        assert!((bounds.max_z - 1.0).abs() < 1.0e-9);
        assert!((bounds.max_x - bounds.min_x - 0.75).abs() < 1.0e-9);
        assert!((bounds.max_z - bounds.min_z - 0.0625).abs() < 1.0e-9);
        let map = frame_box(BlockPos::new(0, 64, 0), Face::North, true);
        assert!((map.max_x - map.min_x - 1.0).abs() < 1.0e-9);
    }

    #[test]
    fn a_two_by_one_painting_centers_on_the_block_to_its_left_edge() {
        let bounds = painting_box(BlockPos::new(0, 64, 0), Face::South, 2, 1);
        assert!((bounds.max_x - bounds.min_x - 2.0).abs() < 1.0e-9);
        assert!((bounds.max_y - bounds.min_y - 1.0).abs() < 1.0e-9);
        assert!((bounds.max_z - bounds.min_z - 0.0625).abs() < 1.0e-9);
        assert!((bounds.center().x - 1.0).abs() < 1.0e-9);
        assert!((bounds.center().y - 64.5).abs() < 1.0e-9);
    }

    #[test]
    fn an_item_frame_round_trips_its_nbt() {
        let nbt = Compound::new()
            .with("block_pos", Tag::IntArray(vec![3, 64, -2]))
            .with("Facing", 2i8)
            .with("ItemRotation", 5i8)
            .with("ItemDropChance", 0.5f32)
            .with("Invisible", true)
            .with("Fixed", true)
            .with(
                "Item",
                Compound::new()
                    .with("id", "minecraft:stone")
                    .with("count", 1),
            );
        let entity = load("minecraft:item_frame", nbt);
        let state = state_of(&entity);
        assert_eq!(state.pos, BlockPos::new(3, 64, -2));
        assert!(state.fixed);
        assert_eq!(state.drop_chance, 0.5);
        assert_eq!(facing(&entity.base), Face::North);
        assert_eq!(rotation(&entity.base), 5);
        assert!(is_invisible(&entity.base));
        assert!(item(&entity.base).is_some());
        let saved = save_entity(&entity, None).unwrap();
        assert_eq!(saved.get_i64("Facing"), Some(2));
        assert_eq!(saved.get_i64("ItemRotation"), Some(5));
        assert_eq!(saved.get_bool("Fixed"), Some(true));
        assert_eq!(saved.get_int_array("block_pos"), Some(&[3, 64, -2][..]));
        let again = load_entity(&saved, EntityId(1), Uuid::from_u128(1)).unwrap();
        assert_eq!(save_entity(&again, None), Some(saved));
    }

    #[test]
    fn a_legacy_frame_position_is_read_from_the_tile_keys() {
        let nbt = Compound::new()
            .with("TileX", 7)
            .with("TileY", 70)
            .with("TileZ", 9)
            .with("Facing", 3i8);
        let entity = load("minecraft:glow_item_frame", nbt);
        assert_eq!(state_of(&entity).pos, BlockPos::new(7, 70, 9));
        assert_eq!(facing(&entity.base), Face::South);
    }

    #[test]
    fn a_painting_round_trips_its_variant_and_facing() {
        let nbt = Compound::new()
            .with("block_pos", Tag::IntArray(vec![0, 64, 0]))
            .with("facing", 2i8)
            .with("variant", "minecraft:fighters");
        let entity = load("minecraft:painting", nbt);
        assert_eq!(variant_name(variant(&entity.base)), Some("fighters"));
        assert_eq!(facing(&entity.base), Face::North);
        let saved = save_entity(&entity, None).unwrap();
        assert_eq!(saved.get_str("variant"), Some("minecraft:fighters"));
        assert_eq!(saved.get_i64("facing"), Some(2));
    }

    #[test]
    fn a_frame_on_a_solid_wall_survives_and_one_in_the_open_does_not() {
        let held = load(
            "minecraft:item_frame",
            Compound::new()
                .with("block_pos", Tag::IntArray(vec![0, 64, 0]))
                .with("Facing", 2i8),
        );
        let loose = load(
            "minecraft:item_frame",
            Compound::new()
                .with("block_pos", Tag::IntArray(vec![0, 64, 0]))
                .with("Facing", 3i8),
        );
        assert!(survives(&Wall, &held.base, state_of(&held), &[]));
        assert!(!survives(&Wall, &loose.base, state_of(&loose), &[]));
    }

    #[test]
    fn a_frame_cannot_share_a_face_with_another_hanging_entity() {
        let held = load(
            "minecraft:item_frame",
            Compound::new()
                .with("block_pos", Tag::IntArray(vec![0, 64, 0]))
                .with("Facing", 2i8),
        );
        let other = Neighbour {
            id: EntityId(2),
            kind: EntityKind::Painting,
            facing: Face::North,
            bounds: held.base.bounding_box(),
        };
        assert!(!survives(&Wall, &held.base, state_of(&held), &[other]));
        let turned = Neighbour {
            facing: Face::East,
            ..other
        };
        assert!(survives(&Wall, &held.base, state_of(&held), &[turned]));
    }

    #[test]
    fn an_empty_painting_support_is_refused() {
        let painting = load(
            "minecraft:painting",
            Compound::new()
                .with("block_pos", Tag::IntArray(vec![0, 64, 0]))
                .with("facing", 2i8)
                .with("variant", "minecraft:kebab"),
        );
        assert!(survives(&Wall, &painting.base, state_of(&painting), &[]));
        let floating = load(
            "minecraft:painting",
            Compound::new()
                .with("block_pos", Tag::IntArray(vec![0, 64, 0]))
                .with("facing", 0i8)
                .with("variant", "minecraft:kebab"),
        );
        assert!(!survives(&Wall, &floating.base, state_of(&floating), &[]));
    }
}

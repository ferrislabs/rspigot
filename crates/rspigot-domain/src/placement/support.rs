use rspigot_blocks::{BlockKind, Face, StateFlags, Support};

use crate::{BlockPos, BlockStateId};

pub const BLOCK_TAGS: &str = "minecraft:block";

pub fn kind_in_tag(kind: BlockKind, tag: &str) -> bool {
    rspigot_data::tags::tag(BLOCK_TAGS, tag)
        .is_some_and(|ids| ids.binary_search(&u32::from(kind.id().0)).is_ok())
}

pub fn in_tag(state: BlockStateId, tag: &str) -> bool {
    state.kind().is_some_and(|kind| kind_in_tag(kind, tag))
}

pub fn sturdy(state: BlockStateId, face: Face) -> bool {
    state.is_face_sturdy(face, Support::Full)
}

pub fn center(state: BlockStateId, face: Face) -> bool {
    state.is_face_sturdy(face, Support::Center)
}

pub fn rigid(state: BlockStateId) -> bool {
    state.is_face_sturdy(Face::Up, Support::Rigid)
}

pub fn solid(state: BlockStateId) -> bool {
    state.flags().contains(StateFlags::LEGACY_SOLID)
}

pub fn step(pos: BlockPos, face: Face) -> BlockPos {
    let (dx, dy, dz) = face.offset();
    pos.offset(dx, dy, dz)
}

pub fn face_named(name: &str) -> Option<Face> {
    Face::ALL.into_iter().find(|face| face.name() == name)
}

pub fn facing_of(state: BlockStateId) -> Option<Face> {
    state.property("facing").and_then(face_named)
}

pub const fn clockwise(face: Face) -> Face {
    match face {
        Face::North => Face::East,
        Face::East => Face::South,
        Face::South => Face::West,
        Face::West => Face::North,
        other => other,
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

pub const fn same_axis(a: Face, b: Face) -> bool {
    matches!(
        (a, b),
        (Face::North | Face::South, Face::North | Face::South)
            | (Face::East | Face::West, Face::East | Face::West)
            | (Face::Up | Face::Down, Face::Up | Face::Down)
    )
}

pub fn with(state: BlockStateId, property: &str, value: &str) -> BlockStateId {
    state.with_property(property, value).unwrap_or(state)
}

pub fn flag(state: BlockStateId, property: &str) -> bool {
    state.property(property) == Some("true")
}

pub fn boolean(value: bool) -> &'static str {
    if value { "true" } else { "false" }
}

pub fn is_water_source(state: BlockStateId) -> bool {
    match state.kind() {
        Some(BlockKind::Water) => state.property("level") == Some("0"),
        _ => flag(state, "waterlogged"),
    }
}

pub fn is_kind(state: BlockStateId, kind: BlockKind) -> bool {
    state.kind() == Some(kind)
}

pub fn path(kind: BlockKind) -> &'static str {
    let name = kind.name();
    name.strip_prefix("minecraft:").unwrap_or(name)
}

pub fn name_of(state: BlockStateId) -> &'static str {
    state.kind().map_or("", path)
}

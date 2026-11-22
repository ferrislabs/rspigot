use rspigot_blocks::{BlockKind, Face};

use super::support::{
    clockwise, counter_clockwise, facing_of, flag, in_tag, kind_in_tag, name_of, same_axis, step,
    sturdy, with,
};
use crate::{BlockPos, BlockStateId};

const POST_STRIP: (f64, f64, f64, f64) = (0.4375, 0.4375, 0.5625, 0.5625);
const EPSILON: f64 = 1.0e-9;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Connecting {
    Fence { wooden: bool },
    Wall,
    Pane,
}

fn connecting(state: BlockStateId) -> Option<Connecting> {
    let kind = state.kind()?;
    if kind_in_tag(kind, "minecraft:fences") {
        return Some(Connecting::Fence {
            wooden: kind_in_tag(kind, "minecraft:wooden_fences"),
        });
    }
    if kind_in_tag(kind, "minecraft:walls") {
        return Some(Connecting::Wall);
    }
    is_pane(state).then_some(Connecting::Pane)
}

fn is_pane(state: BlockStateId) -> bool {
    let name = name_of(state);
    name.ends_with("glass_pane") || name.ends_with("_bars") || name == "iron_bars"
}

fn exception_for_connection(state: BlockStateId) -> bool {
    in_tag(state, "minecraft:leaves")
        || in_tag(state, "minecraft:shulker_boxes")
        || matches!(
            state.kind(),
            Some(
                BlockKind::Barrier
                    | BlockKind::CarvedPumpkin
                    | BlockKind::JackOLantern
                    | BlockKind::Melon
                    | BlockKind::Pumpkin
            )
        )
}

fn gate_connects(neighbour: BlockStateId, toward: Face) -> bool {
    in_tag(neighbour, "minecraft:fence_gates")
        && facing_of(neighbour)
            .is_some_and(|facing| same_axis(facing, clockwise(toward.opposite())))
}

fn connects(own: Connecting, neighbour: BlockStateId, toward: Face) -> bool {
    let wall = sturdy(neighbour, toward.opposite()) && !exception_for_connection(neighbour);
    match own {
        Connecting::Fence { wooden } => {
            wall || connecting(neighbour) == Some(Connecting::Fence { wooden })
                || gate_connects(neighbour, toward)
        }
        Connecting::Wall => {
            wall || in_tag(neighbour, "minecraft:walls") || gate_connects(neighbour, toward)
        }
        Connecting::Pane => wall || is_pane(neighbour) || in_tag(neighbour, "minecraft:walls"),
    }
}

fn down_face(above: BlockStateId) -> Vec<rspigot_blocks::Aabb> {
    above
        .collision_shape()
        .iter()
        .filter(|aabb| aabb.min_y <= EPSILON)
        .copied()
        .collect()
}

fn overlaps(face: &[rspigot_blocks::Aabb], strip: (f64, f64, f64, f64)) -> bool {
    let (x0, z0, x1, z1) = strip;
    face.iter().any(|b| {
        b.min_x < x1 - EPSILON
            && b.max_x > x0 + EPSILON
            && b.min_z < z1 - EPSILON
            && b.max_z > z0 + EPSILON
    })
}

fn strip_of(face: Face) -> (f64, f64, f64, f64) {
    match face {
        Face::North => (0.4375, 0.0, 0.5625, 0.5625),
        Face::South => (0.4375, 0.4375, 0.5625, 1.0),
        Face::East => (0.4375, 0.4375, 1.0, 0.5625),
        _ => (0.0, 0.4375, 0.5625, 0.5625),
    }
}

fn wall_with(
    state: BlockStateId,
    above: BlockStateId,
    forced: Option<(Face, bool)>,
) -> BlockStateId {
    let face = down_face(above);
    let mut sides = [""; 4];
    for (index, side) in Face::HORIZONTAL.into_iter().enumerate() {
        let connected = match forced {
            Some((forced, value)) if forced == side => value,
            _ => state
                .property(side.name())
                .is_some_and(|value| value != "none"),
        };
        sides[index] = match (connected, overlaps(&face, strip_of(side))) {
            (false, _) => "none",
            (true, true) => "tall",
            (true, false) => "low",
        };
    }
    let [north, east, south, west] = sides;
    let straight = north == south && east == west && ((north == "none") != (east == "none"));
    let raised = (is_wall_post(above))
        || in_tag(above, "minecraft:wall_post_override")
        || !straight
        || overlaps(&face, POST_STRIP);
    let mut next = state;
    for (side, value) in Face::HORIZONTAL.into_iter().zip(sides) {
        next = with(next, side.name(), value);
    }
    with(next, "up", if raised { "true" } else { "false" })
}

fn is_wall_post(above: BlockStateId) -> bool {
    in_tag(above, "minecraft:walls") && flag(above, "up")
}

pub fn connection_state(
    state: BlockStateId,
    pos: BlockPos,
    toward: Option<Face>,
    block: &impl Fn(BlockPos) -> BlockStateId,
) -> Option<BlockStateId> {
    let own = connecting(state)?;
    let side_connects = |face: Face| connects(own, block(step(pos, face)), face);
    match toward {
        None => {
            if own == Connecting::Wall {
                let mut next = state;
                for face in Face::HORIZONTAL {
                    next = with(
                        next,
                        face.name(),
                        if side_connects(face) { "low" } else { "none" },
                    );
                }
                let above = block(step(pos, Face::Up));
                return Some(wall_with(next, above, None));
            }
            let mut next = state;
            for face in Face::HORIZONTAL {
                next = with(
                    next,
                    face.name(),
                    if side_connects(face) { "true" } else { "false" },
                );
            }
            Some(next)
        }
        Some(Face::Down) => Some(state),
        Some(Face::Up) => Some(match own {
            Connecting::Wall => wall_with(state, block(step(pos, Face::Up)), None),
            _ => state,
        }),
        Some(face) => {
            let value = side_connects(face);
            Some(match own {
                Connecting::Wall => {
                    wall_with(state, block(step(pos, Face::Up)), Some((face, value)))
                }
                _ => with(state, face.name(), if value { "true" } else { "false" }),
            })
        }
    }
}

fn takes_shape(
    state: BlockStateId,
    pos: BlockPos,
    toward: Face,
    block: &impl Fn(BlockPos) -> BlockStateId,
) -> bool {
    let neighbour = block(step(pos, toward));
    !in_tag(neighbour, "minecraft:stairs")
        || neighbour.property("facing") != state.property("facing")
        || neighbour.property("half") != state.property("half")
}

pub fn stairs_shape(
    state: BlockStateId,
    pos: BlockPos,
    block: &impl Fn(BlockPos) -> BlockStateId,
) -> &'static str {
    let Some(facing) = facing_of(state) else {
        return "straight";
    };
    let half = state.property("half");
    let stair_at = |face: Face| {
        let neighbour = block(step(pos, face));
        (in_tag(neighbour, "minecraft:stairs") && neighbour.property("half") == half)
            .then_some(neighbour)
    };
    if let Some(front) = stair_at(facing)
        && let Some(front_facing) = facing_of(front)
        && !same_axis(front_facing, facing)
        && takes_shape(state, pos, front_facing.opposite(), block)
    {
        return if front_facing == counter_clockwise(facing) {
            "outer_left"
        } else {
            "outer_right"
        };
    }
    if let Some(back) = stair_at(facing.opposite())
        && let Some(back_facing) = facing_of(back)
        && !same_axis(back_facing, facing)
        && takes_shape(state, pos, back_facing, block)
    {
        return if back_facing == counter_clockwise(facing) {
            "inner_left"
        } else {
            "inner_right"
        };
    }
    "straight"
}

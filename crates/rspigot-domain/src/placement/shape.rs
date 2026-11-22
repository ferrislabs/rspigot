use rspigot_blocks::{BlockKind, Face};

use super::connect::{connection_state, stairs_shape};
use super::support::{in_tag, step, with};
use super::survive::{
    can_survive, is_deferred, is_door_or_plant, is_farmland, is_vine, vine_faces, vine_has_faces,
};
use crate::{BlockPos, BlockStateId};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shaped {
    Same,
    Changed(BlockStateId),
    Destroy,
    Recheck,
}

fn changed(state: BlockStateId, next: BlockStateId) -> Shaped {
    if next == state {
        Shaped::Same
    } else {
        Shaped::Changed(next)
    }
}

const COPIED_FROM_OTHER_HALF: [&str; 4] = ["facing", "open", "hinge", "powered"];

fn half_neighbour(
    state: BlockStateId,
    pos: BlockPos,
    direction: Face,
    block: &impl Fn(BlockPos) -> BlockStateId,
) -> Shaped {
    let lower = state.property("half") == Some("lower");
    let toward_other =
        matches!(direction, Face::Up | Face::Down) && lower == (direction == Face::Up);
    if toward_other {
        let neighbour = block(step(pos, direction));
        if neighbour.kind() != state.kind() || neighbour.property("half") == state.property("half")
        {
            return Shaped::Destroy;
        }
        let mut next = state;
        for property in COPIED_FROM_OTHER_HALF {
            if let Some(value) = neighbour.property(property) {
                next = with(next, property, value);
            }
        }
        return changed(state, next);
    }
    if lower && direction == Face::Down && !can_survive(state, pos, block) {
        Shaped::Destroy
    } else {
        Shaped::Same
    }
}

pub fn update_shape(
    state: BlockStateId,
    pos: BlockPos,
    direction: Face,
    block: &impl Fn(BlockPos) -> BlockStateId,
) -> Shaped {
    if is_door_or_plant(state) {
        return half_neighbour(state, pos, direction, block);
    }
    if is_vine(state) {
        if direction == Face::Down {
            return Shaped::Same;
        }
        let next = vine_faces(pos, state, block);
        return if vine_has_faces(next) {
            changed(state, next)
        } else {
            Shaped::Destroy
        };
    }
    if is_farmland(state) {
        return if direction == Face::Up && !can_survive(state, pos, block) {
            Shaped::Changed(BlockKind::Dirt.default_state())
        } else {
            Shaped::Same
        };
    }
    if let Some(next) = connection_state(state, pos, Some(direction), block) {
        return changed(state, next);
    }
    if in_tag(state, "minecraft:stairs") && direction.is_horizontal() {
        let next = with(state, "shape", stairs_shape(state, pos, block));
        return changed(state, next);
    }
    if can_survive(state, pos, block) {
        Shaped::Same
    } else if is_deferred(state) {
        Shaped::Recheck
    } else {
        Shaped::Destroy
    }
}

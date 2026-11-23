use std::sync::OnceLock;

use rspigot_blocks::{Aabb, BlockKind, StateFlags};
use rspigot_domain::BlockStateId;

pub(crate) const DIRECTIONS: [(i32, i32, i32); 6] = [
    (-1, 0, 0),
    (1, 0, 0),
    (0, 0, -1),
    (0, 0, 1),
    (0, -1, 0),
    (0, 1, 0),
];

const EPSILON: f64 = 1.0e-6;
const FULL: Face = [0xffff; 16];

type Face = [u16; 16];

const SHAPED_NAMES: [&str; 11] = [
    "farmland",
    "dirt_path",
    "snow",
    "piston",
    "sticky_piston",
    "piston_head",
    "enchanting_table",
    "end_portal_frame",
    "sculk_sensor",
    "calibrated_sculk_sensor",
    "sculk_shrieker",
];

const SHAPED_MORE: [&str; 3] = ["stonecutter", "daylight_detector", "lectern"];

fn shaped_kinds() -> &'static [bool] {
    static KINDS: OnceLock<Vec<bool>> = OnceLock::new();
    KINDS.get_or_init(|| {
        let mut kinds = vec![false; BlockKind::ALL.len()];
        for kind in BlockKind::ALL {
            let name = kind.name();
            let name = name.strip_prefix("minecraft:").unwrap_or(name);
            kinds[kind.id().0 as usize] = name.ends_with("_slab")
                || name.ends_with("_stairs")
                || name.ends_with("_shelf")
                || SHAPED_NAMES.contains(&name)
                || SHAPED_MORE.contains(&name);
        }
        kinds
    })
}

pub(crate) fn uses_shape(state: BlockStateId) -> bool {
    let Some(kind) = state.kind() else {
        return false;
    };
    if !shaped_kinds()[kind.id().0 as usize] || !state.flags().contains(StateFlags::CAN_OCCLUDE) {
        return false;
    }
    let name = kind.name();
    let name = name.strip_prefix("minecraft:").unwrap_or(name);
    !(name.ends_with("_slab") && state.property("type") == Some("double"))
}

fn boxes(state: BlockStateId) -> Vec<Aabb> {
    let is_snow = state.kind().is_some_and(|kind| {
        let name = kind.name();
        name.strip_prefix("minecraft:").unwrap_or(name) == "snow"
    });
    if is_snow {
        let layers = state
            .property("layers")
            .and_then(|value| value.parse::<f64>().ok())
            .unwrap_or(1.0);
        return vec![Aabb::new(0.0, 0.0, 0.0, 1.0, layers / 8.0, 1.0)];
    }
    state.collision_shape().to_vec()
}

fn axes(direction: usize) -> (usize, bool, usize, usize) {
    let axis = [0, 0, 2, 2, 1, 1][direction];
    let positive = direction % 2 == 1;
    let (u, v) = match axis {
        0 => (1, 2),
        1 => (0, 2),
        _ => (0, 1),
    };
    (axis, positive, u, v)
}

fn span(shape: &Aabb, axis: usize) -> (f64, f64) {
    match axis {
        0 => (shape.min_x, shape.max_x),
        1 => (shape.min_y, shape.max_y),
        _ => (shape.min_z, shape.max_z),
    }
}

fn cells(low: f64, high: f64) -> std::ops::Range<usize> {
    let from = (low * 16.0 + EPSILON).floor().clamp(0.0, 16.0) as usize;
    let to = (high * 16.0 - EPSILON).ceil().clamp(0.0, 16.0) as usize;
    from..to
}

fn face(state: BlockStateId, direction: usize) -> Face {
    let (axis, positive, u, v) = axes(direction);
    let mut face = [0u16; 16];
    for shape in boxes(state) {
        let (low, high) = span(&shape, axis);
        let touches = if positive {
            high >= 1.0 - EPSILON
        } else {
            low <= EPSILON
        };
        if !touches {
            continue;
        }
        let (u_low, u_high) = span(&shape, u);
        let (v_low, v_high) = span(&shape, v);
        for row in cells(v_low, v_high) {
            for column in cells(u_low, u_high) {
                face[row] |= 1 << column;
            }
        }
    }
    face
}

pub(crate) fn occludes(
    from: Option<BlockStateId>,
    to: Option<BlockStateId>,
    direction: usize,
) -> bool {
    let from = from.filter(|state| uses_shape(*state));
    let to = to.filter(|state| uses_shape(*state));
    if from.is_none() && to.is_none() {
        return false;
    }
    let mut union = [0u16; 16];
    for (state, side) in [(from, direction), (to, direction ^ 1)] {
        if let Some(state) = state {
            let face = face(state, side);
            for (row, bits) in union.iter_mut().zip(face) {
                *row |= bits;
            }
        }
    }
    union == FULL
}

#[cfg(test)]
mod tests {
    use super::*;

    fn slab(kind: &str) -> BlockStateId {
        BlockKind::OakSlab
            .state_with(&[("type", kind), ("waterlogged", "false")])
            .expect("a slab")
    }

    #[test]
    fn only_the_vanilla_shape_occluders_use_their_shape() {
        assert!(uses_shape(slab("bottom")));
        assert!(!uses_shape(slab("double")));
        assert!(!uses_shape(BlockStateId(1)));
        assert!(!uses_shape(BlockStateId::AIR));
        assert!(uses_shape(BlockKind::OakStairs.default_state()));
        assert!(uses_shape(BlockKind::Farmland.default_state()));
    }

    #[test]
    fn a_bottom_slab_facing_a_top_slab_blocks_light_but_not_two_bottoms() {
        let east = 1;
        assert!(occludes(Some(slab("bottom")), Some(slab("top")), east));
        assert!(!occludes(Some(slab("bottom")), Some(slab("bottom")), east));
        assert!(!occludes(Some(slab("bottom")), None, east));
        assert!(!occludes(
            Some(slab("bottom")),
            Some(BlockStateId::AIR),
            east
        ));
    }

    #[test]
    fn a_stair_is_solid_on_its_down_and_back_faces_only() {
        let stairs = BlockKind::OakStairs
            .state_with(&[
                ("facing", "east"),
                ("half", "bottom"),
                ("shape", "straight"),
                ("waterlogged", "false"),
            ])
            .expect("stairs");
        assert!(occludes(Some(stairs), None, 1));
        assert!(occludes(Some(stairs), None, 4));
        assert!(!occludes(Some(stairs), None, 0));
        assert!(!occludes(Some(stairs), None, 5));
        assert!(!occludes(Some(stairs), None, 2));
    }
}

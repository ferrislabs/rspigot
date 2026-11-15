use rspigot_blocks::{BlockKind as Kind, Face, StateFlags, Support};
use rspigot_domain::BlockStateId;

use super::place::Run;
use super::prov::Pos;
use crate::climate::Xoroshiro;

pub(super) fn moved((x, y, z): Pos, face: Face) -> Pos {
    let (dx, dy, dz) = face.offset();
    (x + dx, y + dy, z + dz)
}

pub(super) fn face_name(face: Face) -> &'static str {
    match face {
        Face::Down => "down",
        Face::Up => "up",
        Face::North => "north",
        Face::South => "south",
        Face::West => "west",
        Face::East => "east",
    }
}

pub(super) fn has_face(state: BlockStateId, face: Face) -> bool {
    state.property(face_name(face)) == Some("true")
}

pub(super) fn with_face(state: BlockStateId, face: Face, present: bool) -> BlockStateId {
    state
        .with_property(face_name(face), if present { "true" } else { "false" })
        .unwrap_or(state)
}

pub(super) fn has_any_face(state: BlockStateId) -> bool {
    Face::ALL.iter().any(|face| has_face(state, *face))
}

pub(super) fn water_source(state: BlockStateId) -> bool {
    match state.kind() {
        Some(Kind::Water) => state.property("level") == Some("0"),
        Some(
            Kind::Kelp | Kind::KelpPlant | Kind::Seagrass | Kind::TallSeagrass | Kind::BubbleColumn,
        ) => true,
        _ => state.property("waterlogged") == Some("true"),
    }
}

pub(super) fn replaceable(state: BlockStateId) -> bool {
    state.flags().contains(StateFlags::REPLACEABLE)
}

pub(super) fn shuffle<T>(rng: &mut Xoroshiro, items: &mut [T]) {
    for index in (2..=items.len()).rev() {
        let other = rng.below(index as i32) as usize;
        items.swap(index - 1, other);
    }
}

pub(super) fn shuffled_faces(run: &mut Run) -> [Face; 6] {
    let mut faces = Face::ALL;
    shuffle(run.rng, &mut faces);
    faces
}

pub(super) fn manhattan(a: Pos, b: Pos) -> i32 {
    (a.0 - b.0).abs() + (a.1 - b.1).abs() + (a.2 - b.2).abs()
}

pub(super) fn shape_face_full(state: BlockStateId, face: Face) -> bool {
    let boxes = state.collision_shape();
    if boxes.is_empty() {
        return false;
    }
    let near = 1.0e-7;
    let far = 0.999_999_9;
    let at = match face {
        Face::Down | Face::North | Face::West => near,
        _ => far,
    };
    let mut rects: Vec<(f64, f64, f64, f64)> = Vec::new();
    for shape in boxes {
        let (axis_min, axis_max, a0, a1, b0, b1) = match face {
            Face::Down | Face::Up => (
                shape.min_y,
                shape.max_y,
                shape.min_x,
                shape.max_x,
                shape.min_z,
                shape.max_z,
            ),
            Face::North | Face::South => (
                shape.min_z,
                shape.max_z,
                shape.min_x,
                shape.max_x,
                shape.min_y,
                shape.max_y,
            ),
            Face::West | Face::East => (
                shape.min_x,
                shape.max_x,
                shape.min_y,
                shape.max_y,
                shape.min_z,
                shape.max_z,
            ),
        };
        if axis_min <= at && at < axis_max {
            rects.push((a0, a1, b0, b1));
        }
    }
    covers_unit_square(&rects)
}

fn covers_unit_square(rects: &[(f64, f64, f64, f64)]) -> bool {
    let fuzz = 1.0e-7;
    let mut xs = vec![0.0, 1.0];
    let mut ys = vec![0.0, 1.0];
    for (a0, a1, b0, b1) in rects {
        xs.extend([*a0, *a1]);
        ys.extend([*b0, *b1]);
    }
    xs.retain(|v| (0.0..=1.0).contains(v));
    ys.retain(|v| (0.0..=1.0).contains(v));
    xs.sort_by(f64::total_cmp);
    ys.sort_by(f64::total_cmp);
    for xw in xs.windows(2) {
        if xw[1] - xw[0] < fuzz {
            continue;
        }
        for yw in ys.windows(2) {
            if yw[1] - yw[0] < fuzz {
                continue;
            }
            let (cx, cy) = ((xw[0] + xw[1]) / 2.0, (yw[0] + yw[1]) / 2.0);
            let covered = rects
                .iter()
                .any(|(a0, a1, b0, b1)| *a0 <= cx && cx <= *a1 && *b0 <= cy && cy <= *b1);
            if !covered {
                return false;
            }
        }
    }
    true
}

pub(super) fn full_face(state: BlockStateId, face: Face) -> bool {
    state.is_face_sturdy(face, Support::Full) || shape_face_full(state, face)
}

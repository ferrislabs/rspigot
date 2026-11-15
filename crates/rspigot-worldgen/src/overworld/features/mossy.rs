use rspigot_blocks::{BlockKind as Kind, Face};
use rspigot_domain::BlockStateId;

use super::cave::replaceable;
use super::multiface::can_attach_to;
use super::place::Run;
use super::prov::Pos;
use crate::overworld::region::Region;

fn side_name(face: Face) -> &'static str {
    match face {
        Face::North => "north",
        Face::East => "east",
        Face::South => "south",
        _ => "west",
    }
}

fn side(state: BlockStateId, face: Face) -> &'static str {
    state.property(side_name(face)).unwrap_or("none")
}

fn with_side(state: BlockStateId, face: Face, value: &str) -> BlockStateId {
    state.with_property(side_name(face), value).unwrap_or(state)
}

fn bottom(state: BlockStateId) -> bool {
    state.property("bottom") == Some("true")
}

fn carpet(state: BlockStateId) -> bool {
    state.kind() == Some(Kind::PaleMossCarpet)
}

fn updated_state(
    region: &mut Region,
    mut state: BlockStateId,
    pos: Pos,
    tall: bool,
) -> BlockStateId {
    let tall = tall || bottom(state);
    let mut above: Option<BlockStateId> = None;
    let mut below: Option<BlockStateId> = None;
    for face in Face::HORIZONTAL {
        let mut value = if can_attach_to(region, pos, face) {
            if tall { "low" } else { side(state, face) }
        } else {
            "none"
        };
        if value == "low" {
            let up = *above.get_or_insert_with(|| region.get(pos.0, pos.1 + 1, pos.2));
            if carpet(up) && side(up, face) != "none" && !bottom(up) {
                value = "tall";
            }
            if !bottom(state) {
                let down = *below.get_or_insert_with(|| region.get(pos.0, pos.1 - 1, pos.2));
                if carpet(down) && side(down, face) == "none" {
                    value = "none";
                }
            }
        }
        state = with_side(state, face, value);
    }
    state
}

fn has_faces(state: BlockStateId) -> bool {
    bottom(state)
        || Face::HORIZONTAL
            .iter()
            .any(|face| side(state, *face) != "none")
}

fn topper(region: &mut Region, pos: Pos) -> BlockStateId {
    let above_pos = (pos.0, pos.1 + 1, pos.2);
    let above = region.get(above_pos.0, above_pos.1, above_pos.2);
    let is_carpet = carpet(above);
    if (is_carpet && bottom(above)) || (!is_carpet && !replaceable(above)) {
        return BlockStateId::AIR;
    }
    let base = Kind::PaleMossCarpet.default_state();
    let base = base.with_property("bottom", "false").unwrap_or(base);
    let mut updated = updated_state(region, base, above_pos, true);
    for face in Face::HORIZONTAL {
        if side(updated, face) != "none" && !region.random().next_boolean() {
            updated = with_side(updated, face, "none");
        }
    }
    if has_faces(updated) && updated != above {
        updated
    } else {
        BlockStateId::AIR
    }
}

pub(super) fn place_at(run: &mut Run, pos: Pos) {
    let base = Kind::PaleMossCarpet.default_state();
    let placed = updated_state(run.region, base, pos, true);
    run.region.set(pos.0, pos.1, pos.2, placed);
    let upper = topper(run.region, pos);
    if !upper.is_air() {
        run.region.set(pos.0, pos.1 + 1, pos.2, upper);
        let again = updated_state(run.region, placed, pos, true);
        run.region.set(pos.0, pos.1, pos.2, again);
    }
}

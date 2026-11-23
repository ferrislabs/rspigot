use rspigot_blocks::{BlockKind, Face, StateFlags};

use super::connect::{connection_state, stairs_shape};
use super::support::{
    boolean, facing_of, flag, in_tag, is_kind, is_water_source, name_of, path, step, with,
};
use super::survive::{can_survive, vine_supports};
use crate::{BlockPos, BlockStateId};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Look {
    pub yaw: f32,
    pub pitch: f32,
}

impl Look {
    pub fn horizontal(self) -> Face {
        match ((f64::from(self.yaw) / 90.0 + 0.5).floor() as i64).rem_euclid(4) {
            0 => Face::South,
            1 => Face::West,
            2 => Face::North,
            _ => Face::East,
        }
    }

    pub fn nearest(self) -> [Face; 6] {
        let pitch = self.pitch * (std::f32::consts::PI / 180.0);
        let yaw = -self.yaw * (std::f32::consts::PI / 180.0);
        let (sin_pitch, cos_pitch) = (pitch.sin(), pitch.cos());
        let (sin_yaw, cos_yaw) = (yaw.sin(), yaw.cos());
        let east = sin_yaw > 0.0;
        let up = sin_pitch < 0.0;
        let south = cos_yaw > 0.0;
        let east_amount = if east { sin_yaw } else { -sin_yaw };
        let up_amount = if up { -sin_pitch } else { sin_pitch };
        let south_amount = if south { cos_yaw } else { -cos_yaw };
        let x_weight = east_amount * cos_pitch;
        let z_weight = south_amount * cos_pitch;
        let x = if east { Face::East } else { Face::West };
        let y = if up { Face::Up } else { Face::Down };
        let z = if south { Face::South } else { Face::North };
        let line = |a: Face, b: Face, c: Face| [a, b, c, c.opposite(), b.opposite(), a.opposite()];
        if east_amount > south_amount {
            if up_amount > x_weight {
                line(y, x, z)
            } else if z_weight > up_amount {
                line(x, z, y)
            } else {
                line(x, y, z)
            }
        } else if up_amount > z_weight {
            line(y, z, x)
        } else if x_weight > up_amount {
            line(z, x, y)
        } else {
            line(z, y, x)
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Click {
    pub face: Face,
    pub cursor_y: f64,
    pub look: Look,
    pub sneaking: bool,
    pub replacing_clicked: bool,
}

impl Click {
    fn directions(&self) -> [Face; 6] {
        let mut ordered = self.look.nearest();
        if self.replacing_clicked {
            return ordered;
        }
        let wanted = self.face.opposite();
        if let Some(index) = ordered.iter().position(|face| *face == wanted)
            && index > 0
        {
            ordered.copy_within(0..index, 1);
            ordered[0] = wanted;
        }
        ordered
    }
}

fn count(state: BlockStateId, property: &str) -> Option<u8> {
    state.property(property)?.parse().ok()
}

fn stacked(old: BlockStateId, property: &str, limit: u8) -> Option<BlockStateId> {
    let next = count(old, property)? + 1;
    (next <= limit).then(|| with(old, property, &next.to_string()))
}

fn stack_property(kind: BlockKind) -> Option<(&'static str, u8)> {
    let name = path(kind);
    Some(match name {
        "turtle_egg" => ("eggs", 4),
        "pink_petals" | "wildflowers" => ("flower_amount", 4),
        "leaf_litter" => ("segment_amount", 4),
        "snow" => ("layers", 8),
        _ if name.ends_with("candle") => ("candles", 4),
        _ => return None,
    })
}

fn is_multiface(kind: BlockKind) -> bool {
    matches!(path(kind), "glow_lichen" | "sculk_vein" | "resin_clump")
}

fn multiface_next(
    old: BlockStateId,
    kind: BlockKind,
    pos: BlockPos,
    click: &Click,
    block: &impl Fn(BlockPos) -> BlockStateId,
) -> Option<BlockStateId> {
    let same = is_kind(old, kind);
    click.directions().into_iter().find_map(|face| {
        let supported = super::support::sturdy(block(step(pos, face)), face.opposite());
        let free = !(same && flag(old, face.name()));
        (supported && free && kind.default_state().property(face.name()).is_some()).then(|| {
            let base = if same { old } else { kind.default_state() };
            with(base, face.name(), "true")
        })
    })
}

pub fn can_be_replaced(
    old: BlockStateId,
    pos: BlockPos,
    held: BlockKind,
    click: &Click,
    block: &impl Fn(BlockPos) -> BlockStateId,
) -> bool {
    let Some(kind) = old.kind() else {
        return false;
    };
    let same = kind == held;
    if in_tag(old, "minecraft:slabs") && old.property("type").is_some() {
        if !same || old.property("type") == Some("double") {
            return old.flags().contains(StateFlags::REPLACEABLE);
        }
        if !click.replacing_clicked {
            return true;
        }
        let upper = click.cursor_y > 0.5;
        let side = click.face.is_horizontal();
        return if old.property("type") == Some("bottom") {
            click.face == Face::Up || (upper && side)
        } else {
            click.face == Face::Down || (!upper && side)
        };
    }
    if kind == BlockKind::Snow {
        let layers = count(old, "layers").unwrap_or(1);
        if same && layers < 8 {
            return !click.replacing_clicked || click.face == Face::Up;
        }
        return layers == 1;
    }
    if let Some((property, limit)) = stack_property(kind) {
        let room = count(old, property).is_some_and(|amount| amount < limit);
        if same && !click.sneaking && room {
            return true;
        }
    }
    if same && is_multiface(kind) {
        return multiface_next(old, kind, pos, click, block).is_some();
    }
    old.flags().contains(StateFlags::REPLACEABLE)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Adjusted {
    pub state: BlockStateId,
    pub upper: Option<BlockStateId>,
}

fn alone(state: BlockStateId) -> Option<Adjusted> {
    Some(Adjusted { state, upper: None })
}

fn crafter_orientation(click: &Click) -> String {
    let front = click.directions()[0].opposite();
    let top = match front {
        Face::Down => click.look.horizontal().opposite(),
        Face::Up => click.look.horizontal(),
        _ => Face::Up,
    };
    if front.is_horizontal() {
        format!("{}_up", front.name())
    } else {
        format!("{}_{}", front.name(), top.name())
    }
}

fn first_surviving(
    click: &Click,
    pos: BlockPos,
    block: &impl Fn(BlockPos) -> BlockStateId,
    candidate: impl Fn(Face) -> Option<BlockStateId>,
) -> Option<BlockStateId> {
    click
        .directions()
        .into_iter()
        .filter_map(candidate)
        .find(|state| can_survive(*state, pos, block))
}

fn placed_by_kind(
    state: BlockStateId,
    kind: BlockKind,
    existing: BlockStateId,
    pos: BlockPos,
    click: &Click,
    block: &impl Fn(BlockPos) -> BlockStateId,
) -> Option<BlockStateId> {
    let name = path(kind);
    if in_tag(state, "minecraft:lanterns") {
        return first_surviving(click, pos, block, |face| {
            matches!(face, Face::Up | Face::Down)
                .then(|| with(state, "hanging", boolean(face == Face::Up)))
        });
    }
    if is_multiface(kind) {
        return multiface_next(existing, kind, pos, click, block);
    }
    match name {
        "crafter" => Some(with(state, "orientation", &crafter_orientation(click))),
        "calibrated_sculk_sensor" => Some(with(
            state,
            "facing",
            click.look.horizontal().opposite().name(),
        )),
        "grindstone" => first_surviving(click, pos, block, |face| {
            Some(if matches!(face, Face::Up | Face::Down) {
                with(
                    with(
                        state,
                        "face",
                        if face == Face::Up { "ceiling" } else { "floor" },
                    ),
                    "facing",
                    click.look.horizontal().name(),
                )
            } else {
                with(
                    with(state, "face", "wall"),
                    "facing",
                    face.opposite().name(),
                )
            })
        }),
        "cocoa" => first_surviving(click, pos, block, |face| {
            face.is_horizontal()
                .then(|| with(state, "facing", face.name()))
        }),
        "bell" => bell_state(state, pos, click, block),
        "big_dripleaf" => {
            let below = block(step(pos, Face::Down));
            let facing = if matches!(name_of(below), "big_dripleaf" | "big_dripleaf_stem") {
                facing_of(below).unwrap_or(Face::North)
            } else {
                click.look.horizontal().opposite()
            };
            Some(with(state, "facing", facing.name()))
        }
        "mangrove_propagule" => Some(with(state, "hanging", boolean(click.face == Face::Down))),
        "vine" => {
            let same = is_kind(existing, BlockKind::Vine);
            let base = if same { existing } else { state };
            click
                .directions()
                .into_iter()
                .filter(|face| *face != Face::Down)
                .find(|face| {
                    !(same && flag(existing, face.name())) && vine_supports(pos, *face, block)
                })
                .map(|face| with(base, face.name(), "true"))
                .or(same.then_some(existing))
        }
        _ => Some(state),
    }
}

fn bell_state(
    state: BlockStateId,
    pos: BlockPos,
    click: &Click,
    block: &impl Fn(BlockPos) -> BlockStateId,
) -> Option<BlockStateId> {
    let face = click.face;
    if !face.is_horizontal() {
        let placed = with(
            with(
                state,
                "attachment",
                if face == Face::Down {
                    "ceiling"
                } else {
                    "floor"
                },
            ),
            "facing",
            click.look.horizontal().name(),
        );
        return can_survive(placed, pos, block).then_some(placed);
    }
    let along = if matches!(face, Face::North | Face::South) {
        [Face::North, Face::South]
    } else {
        [Face::East, Face::West]
    };
    let both = along
        .into_iter()
        .all(|side| super::support::sturdy(block(step(pos, side)), side.opposite()));
    let placed = with(
        with(
            state,
            "attachment",
            if both { "double_wall" } else { "single_wall" },
        ),
        "facing",
        face.opposite().name(),
    );
    can_survive(placed, pos, block).then_some(placed)
}

pub fn adjust(
    state: BlockStateId,
    pos: BlockPos,
    click: &Click,
    block: &impl Fn(BlockPos) -> BlockStateId,
) -> Option<Adjusted> {
    let kind = state.kind()?;
    let existing = block(pos);
    if is_kind(existing, kind) {
        if in_tag(existing, "minecraft:slabs") && existing.property("type") != Some("double") {
            let merged = with(with(existing, "type", "double"), "waterlogged", "false");
            return alone(merged);
        }
        if let Some((property, limit)) = stack_property(kind)
            && let Some(merged) = stacked(existing, property, limit)
        {
            return alone(merged);
        }
    }
    let mut state = placed_by_kind(state, kind, existing, pos, click, block)?;
    if state.kind() == Some(BlockKind::Farmland) && !can_survive(state, pos, block) {
        state = BlockKind::Dirt.default_state();
    }
    if let Some(next) = connection_state(state, pos, None, block) {
        state = next;
    }
    if in_tag(state, "minecraft:stairs") {
        state = with(state, "shape", stairs_shape(state, pos, block));
    }
    if state.property("waterlogged").is_some() {
        state = with(state, "waterlogged", boolean(is_water_source(existing)));
    }
    let two_tall = state.kind().is_some_and(|k| {
        k.properties()
            .iter()
            .any(|p| p.name == "half" && p.values == ["upper", "lower"])
    });
    let mut upper = None;
    if two_tall {
        state = with(state, "half", "lower");
        let above = block(step(pos, Face::Up));
        if !above.flags().contains(StateFlags::REPLACEABLE) {
            return None;
        }
        let mut top = with(state, "half", "upper");
        if top.property("waterlogged").is_some() {
            top = with(top, "waterlogged", boolean(is_water_source(above)));
        }
        upper = Some(top);
    }
    if !can_survive(state, pos, block) {
        return None;
    }
    Some(Adjusted { state, upper })
}

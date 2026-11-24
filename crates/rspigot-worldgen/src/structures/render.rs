use rspigot_blocks::BlockKind;
use rspigot_core::ports::{PlaceFailure, TemplateMirror, TemplateRotation};
use rspigot_domain::BlockStateId;
use serde_json::Value;

use super::bbox::Pos;
use super::blueprint::Blueprint;
use super::jigsaw::{Ground, Heightmap, JigsawConfig, assemble};
use super::library::Library;
use super::pool::Element;
use super::processors::parse_state;
use super::world::Start;
use crate::legacy::JavaRandom;
use crate::overworld::features::structure::Rotation;

pub(crate) fn rotation_of(rotation: TemplateRotation) -> Rotation {
    match rotation {
        TemplateRotation::None => Rotation::None,
        TemplateRotation::Clockwise90 => Rotation::Clockwise90,
        TemplateRotation::Clockwise180 => Rotation::Clockwise180,
        TemplateRotation::Counterclockwise90 => Rotation::Counterclockwise90,
    }
}

fn mirrored(mirror: TemplateMirror, (x, y, z): Pos) -> Pos {
    match mirror {
        TemplateMirror::None => (x, y, z),
        TemplateMirror::LeftRight => (x, y, -z),
        TemplateMirror::FrontBack => (-x, y, z),
    }
}

fn mirrored_state(mirror: TemplateMirror, state: BlockStateId) -> BlockStateId {
    let swap = match (mirror, state.property("facing")) {
        (TemplateMirror::LeftRight, Some("north")) => Some("south"),
        (TemplateMirror::LeftRight, Some("south")) => Some("north"),
        (TemplateMirror::FrontBack, Some("east")) => Some("west"),
        (TemplateMirror::FrontBack, Some("west")) => Some("east"),
        _ => None,
    };
    swap.and_then(|facing| state.with_property("facing", facing))
        .unwrap_or(state)
}

pub(crate) fn blueprint_blocks(
    blueprint: &Blueprint,
    origin: Pos,
    rotation: Rotation,
    mirror: TemplateMirror,
    mut keep: impl FnMut() -> bool,
    out: &mut Vec<(Pos, BlockStateId)>,
) {
    for block in &blueprint.blocks {
        let mut state = block.state;
        match state.kind() {
            Some(BlockKind::StructureBlock | BlockKind::StructureVoid) => continue,
            Some(BlockKind::Jigsaw) => {
                let name = block
                    .nbt
                    .as_deref()
                    .and_then(|nbt| nbt.get("final_state"))
                    .and_then(Value::as_str)
                    .unwrap_or("minecraft:air");
                state = parse_state(name).unwrap_or(BlockStateId::AIR);
                if state.kind() == Some(BlockKind::StructureVoid) {
                    continue;
                }
            }
            _ => {}
        }
        if !keep() {
            continue;
        }
        let (x, y, z) = rotation.transform(mirrored(mirror, block.pos));
        let state = rotation.state(mirrored_state(mirror, state));
        out.push(((x + origin.0, y + origin.1, z + origin.2), state));
    }
}

pub(crate) fn template_blocks(
    blueprint: &Blueprint,
    origin: Pos,
    rotation: TemplateRotation,
    mirror: TemplateMirror,
    integrity: f32,
    seed: i32,
) -> Vec<(Pos, BlockStateId)> {
    let mut random = JavaRandom::new(i64::from(seed));
    let mut out = Vec::new();
    let keep = || integrity >= 1.0 || random.next_float() <= integrity;
    blueprint_blocks(
        blueprint,
        origin,
        rotation_of(rotation),
        mirror,
        keep,
        &mut out,
    );
    out
}

fn element_blocks(
    element: &Element,
    origin: Pos,
    rotation: Rotation,
    out: &mut Vec<(Pos, BlockStateId)>,
) {
    match element {
        Element::Empty | Element::Feature { .. } => {}
        Element::List { elements, .. } => {
            for inner in elements {
                element_blocks(inner, origin, rotation, out);
            }
        }
        Element::Single { .. } => {
            if let Some(blueprint) = element.blueprint() {
                blueprint_blocks(
                    blueprint,
                    origin,
                    rotation,
                    TemplateMirror::None,
                    || true,
                    out,
                );
            }
        }
    }
}

pub(crate) fn start_blocks(start: &Start) -> Vec<(Pos, BlockStateId)> {
    let mut out = Vec::new();
    for piece in &start.pieces {
        element_blocks(piece.element, piece.pos, piece.rotation, &mut out);
    }
    out
}

struct OpenGround {
    min_y: i32,
    max_y: i32,
}

impl Ground for OpenGround {
    fn first_free_height(&self, _x: i32, _z: i32, _heightmap: Heightmap) -> i32 {
        self.min_y
    }

    fn min_y(&self) -> i32 {
        self.min_y
    }

    fn max_y(&self) -> i32 {
        self.max_y
    }
}

pub(crate) fn jigsaw_blocks(
    library: &'static Library,
    pool: &str,
    target: &str,
    max_depth: i32,
    at: Pos,
    heights: (i32, i32),
) -> Result<Vec<(Pos, BlockStateId)>, PlaceFailure> {
    let pools = library.pools();
    if pools.get(pool).is_none() {
        return Err(PlaceFailure::Unknown);
    }
    let ground = OpenGround {
        min_y: heights.0,
        max_y: heights.1,
    };
    let no_aliases = super::alias::Lookup::new();
    let config = JigsawConfig {
        aliases: &no_aliases,
        start_pool: pool,
        start_jigsaw: (target != "minecraft:empty").then_some(target),
        size: max_depth,
        expansion_hack: false,
        project: None,
        max_horizontal: 128,
        max_vertical: 128,
        padding: (0, 0),
    };
    let mut random = JavaRandom::new(
        i64::from(at.0).wrapping_mul(341_873_128_712)
            ^ i64::from(at.2).wrapping_mul(132_897_987_541),
    );
    let assembly = assemble(pools, &ground, &config, &mut random, at, |_, _| true)
        .ok_or(PlaceFailure::Failed)?;
    let mut out = Vec::new();
    for piece in &assembly.pieces {
        element_blocks(piece.element, piece.pos, piece.rotation, &mut out);
    }
    if out.is_empty() {
        Err(PlaceFailure::Failed)
    } else {
        Ok(out)
    }
}

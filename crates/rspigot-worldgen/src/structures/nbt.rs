use rspigot_domain::chunk::BlockEntity;
use rspigot_domain::{BlockPos, Identifier};
use rspigot_nbt::{Compound, Tag};
use serde_json::Value;

use super::bbox::Bbox;
use super::pool::{Element, Processors, Projection};
use super::world::Start;
use crate::overworld::features::structure::Rotation;

fn typed(map: &serde_json::Map<String, Value>) -> Option<Tag> {
    let (key, value) = map.iter().next()?;
    let int = value.as_i64();
    let list = || -> Option<Vec<i64>> { value.as_array()?.iter().map(Value::as_i64).collect() };
    Some(match key.as_str() {
        "$b" => Tag::Byte(int? as i8),
        "$s" => Tag::Short(int? as i16),
        "$l" => Tag::Long(int?),
        "$f" => Tag::Float(value.as_f64()? as f32),
        "$ba" => Tag::ByteArray(list()?.into_iter().map(|n| n as u8).collect()),
        "$ia" => Tag::IntArray(list()?.into_iter().map(|n| n as i32).collect()),
        "$la" => Tag::LongArray(list()?),
        _ => return None,
    })
}

pub(crate) fn json_tag(value: &Value) -> Tag {
    match value {
        Value::Bool(flag) => Tag::Byte(i8::from(*flag)),
        Value::Number(number) => match number.as_i64() {
            Some(n) => match i32::try_from(n) {
                Ok(small) => Tag::Int(small),
                Err(_) => Tag::Long(n),
            },
            None => Tag::Double(number.as_f64().unwrap_or(0.0)),
        },
        Value::String(text) => Tag::String(text.clone()),
        Value::Array(items) => Tag::List(items.iter().map(json_tag).collect()),
        Value::Object(map) if map.len() == 1 && typed(map).is_some() => {
            typed(map).unwrap_or(Tag::Byte(0))
        }
        Value::Object(map) => {
            let mut compound = Compound::new();
            for (key, inner) in map {
                compound.insert(key.clone(), json_tag(inner));
            }
            Tag::Compound(compound)
        }
        Value::Null => Tag::Byte(0),
    }
}

pub(crate) fn block_entity(pos: (i32, i32, i32), nbt: &Value) -> Option<BlockEntity> {
    let Value::Object(map) = nbt else {
        return None;
    };
    let kind = Identifier::parse(map.get("id")?.as_str()?).ok()?;
    let mut data = Compound::new();
    for (key, inner) in map {
        if !matches!(key.as_str(), "id" | "x" | "y" | "z") {
            data.insert(key.clone(), json_tag(inner));
        }
    }
    Some(BlockEntity {
        pos: BlockPos::new(pos.0, pos.1, pos.2),
        kind,
        data,
    })
}

fn rotation_name(rotation: Rotation) -> &'static str {
    match rotation {
        Rotation::None => "none",
        Rotation::Clockwise90 => "clockwise_90",
        Rotation::Clockwise180 => "clockwise_180",
        Rotation::Counterclockwise90 => "counterclockwise_90",
    }
}

fn projection_name(projection: Projection) -> &'static str {
    match projection {
        Projection::Rigid => "rigid",
        Projection::TerrainMatching => "terrain_matching",
    }
}

fn element_tag(element: &Element) -> Compound {
    match element {
        Element::Empty => Compound::new().with("element_type", "minecraft:empty_pool_element"),
        Element::Single {
            template,
            processors,
            projection,
            legacy,
            ..
        } => {
            let kind = if *legacy {
                "minecraft:legacy_single_pool_element"
            } else {
                "minecraft:single_pool_element"
            };
            let processors = match processors {
                Processors::Named(name) => Tag::String(name.clone()),
                Processors::Inline(value) => json_tag(value),
            };
            Compound::new()
                .with("element_type", kind)
                .with("location", template.as_str())
                .with("processors", processors)
                .with("projection", projection_name(*projection))
        }
        Element::Feature {
            feature,
            projection,
        } => Compound::new()
            .with("element_type", "minecraft:feature_pool_element")
            .with("feature", feature.as_str())
            .with("projection", projection_name(*projection)),
        Element::List {
            elements,
            projection,
        } => Compound::new()
            .with("element_type", "minecraft:list_pool_element")
            .with(
                "elements",
                Tag::List(
                    elements
                        .iter()
                        .map(|e| Tag::Compound(element_tag(e)))
                        .collect(),
                ),
            )
            .with("projection", projection_name(*projection)),
    }
}

fn bounds_tag(bbox: &Bbox) -> Tag {
    Tag::IntArray(vec![
        bbox.min.0, bbox.min.1, bbox.min.2, bbox.max.0, bbox.max.1, bbox.max.2,
    ])
}

fn start_tag(start: &Start) -> Compound {
    let children: Vec<Tag> = start
        .pieces
        .iter()
        .map(|piece| {
            let junctions: Vec<Tag> = piece
                .junctions
                .iter()
                .map(|junction| {
                    Tag::Compound(
                        Compound::new()
                            .with("source_x", junction.source.0)
                            .with("source_ground_y", junction.source.1)
                            .with("source_z", junction.source.2)
                            .with("delta_y", junction.delta_y)
                            .with("dest_proj", projection_name(junction.destination)),
                    )
                })
                .collect();
            Tag::Compound(
                Compound::new()
                    .with("id", "minecraft:jigsaw")
                    .with("pos_x", piece.pos.0)
                    .with("pos_y", piece.pos.1)
                    .with("pos_z", piece.pos.2)
                    .with("ground_level_delta", piece.ground_level_delta)
                    .with("pool_element", Tag::Compound(element_tag(piece.element)))
                    .with("rotation", rotation_name(piece.rotation))
                    .with("BB", bounds_tag(&piece.bbox))
                    .with("junctions", Tag::List(junctions))
                    .with("liquid_settings", "apply_waterlogging"),
            )
        })
        .collect();
    Compound::new()
        .with("id", format!("minecraft:{}", start.name))
        .with("ChunkX", start.chunk.0)
        .with("ChunkZ", start.chunk.1)
        .with("references", 0)
        .with("Children", Tag::List(children))
}

pub(crate) fn structures_tag(
    own: &[std::sync::Arc<Start>],
    near: &[std::sync::Arc<Start>],
) -> Compound {
    let mut starts = Compound::new();
    for start in own {
        starts.insert(format!("minecraft:{}", start.name), start_tag(start));
    }
    let mut references: Vec<(String, Vec<i64>)> = Vec::new();
    for start in near {
        let key = format!("minecraft:{}", start.name);
        let packed = (i64::from(start.chunk.0) & 0xFFFF_FFFF)
            | ((i64::from(start.chunk.1) & 0xFFFF_FFFF) << 32);
        match references.iter_mut().find(|(name, _)| *name == key) {
            Some((_, list)) => list.push(packed),
            None => references.push((key, vec![packed])),
        }
    }
    let mut refs = Compound::new();
    for (name, list) in references {
        refs.insert(name, Tag::LongArray(list));
    }
    Compound::new()
        .with("starts", starts)
        .with("References", refs)
}

fn wrap_degrees(angle: f32) -> f32 {
    let mut wrapped = angle % 360.0;
    if wrapped >= 180.0 {
        wrapped -= 360.0;
    }
    if wrapped < -180.0 {
        wrapped += 360.0;
    }
    wrapped
}

fn entity_position(local: (f64, f64, f64), rotation: Rotation) -> (f64, f64, f64) {
    let (x, y, z) = local;
    match rotation {
        Rotation::None => (x, y, z),
        Rotation::Clockwise90 => (1.0 - z, y, x),
        Rotation::Clockwise180 => (1.0 - x, y, 1.0 - z),
        Rotation::Counterclockwise90 => (z, y, 1.0 - x),
    }
}

fn triple(value: &Value) -> Option<(f64, f64, f64)> {
    let items = value.as_array()?;
    Some((
        items.first()?.as_f64()?,
        items.get(1)?.as_f64()?,
        items.get(2)?.as_f64()?,
    ))
}

fn collect_entities(
    element: &Element,
    piece: &super::jigsaw::Piece<'static>,
    area: &Bbox,
    out: &mut Vec<Compound>,
) {
    match element {
        Element::List { elements, .. } => {
            for inner in elements {
                collect_entities(inner, piece, area, out);
            }
        }
        Element::Single {
            blueprint: Some(blueprint),
            ..
        } => {
            for entity in &blueprint.entities {
                let (Some(exact), Some(block), Some(nbt)) = (
                    entity.get("pos").and_then(triple),
                    entity.get("block_pos").and_then(triple),
                    entity.get("nbt"),
                ) else {
                    continue;
                };
                let (bx, by, bz) =
                    piece
                        .rotation
                        .transform((block.0 as i32, block.1 as i32, block.2 as i32));
                let at = (bx + piece.pos.0, by + piece.pos.1, bz + piece.pos.2);
                if !area.contains(at) {
                    continue;
                }
                let (rx, ry, rz) = entity_position(exact, piece.rotation);
                let Tag::Compound(mut compound) = json_tag(nbt) else {
                    continue;
                };
                compound.remove("UUID");
                compound.insert(
                    "Pos",
                    Tag::List(vec![
                        Tag::Double(rx + f64::from(piece.pos.0)),
                        Tag::Double(ry + f64::from(piece.pos.1)),
                        Tag::Double(rz + f64::from(piece.pos.2)),
                    ]),
                );
                let (yaw, pitch) = match compound.get_list("Rotation") {
                    Some([Tag::Float(yaw), Tag::Float(pitch)]) => (*yaw, *pitch),
                    _ => (0.0, 0.0),
                };
                let turned = match piece.rotation {
                    Rotation::None => 0.0,
                    Rotation::Clockwise90 => 90.0,
                    Rotation::Clockwise180 => 180.0,
                    Rotation::Counterclockwise90 => 270.0,
                };
                compound.insert(
                    "Rotation",
                    Tag::List(vec![
                        Tag::Float(wrap_degrees(wrap_degrees(yaw) + turned)),
                        Tag::Float(pitch),
                    ]),
                );
                out.push(compound);
            }
        }
        _ => {}
    }
}

pub(crate) fn chunk_entities(starts: &[std::sync::Arc<Start>], chunk: (i32, i32)) -> Vec<Compound> {
    let area = Bbox {
        min: (chunk.0 * 16, i32::MIN, chunk.1 * 16),
        max: (chunk.0 * 16 + 15, i32::MAX, chunk.1 * 16 + 15),
    };
    let mut out = Vec::new();
    for start in starts {
        for piece in &start.pieces {
            if piece.bbox.intersects(&area) {
                collect_entities(piece.element, piece, &area, &mut out);
            }
        }
    }
    out
}

use std::borrow::Cow;

use rspigot_domain::BlockStateId;
use serde_json::Value;

use super::bbox::{Bbox, Pos};
use super::jigsaw::Piece;
use super::pool::{Element, Projection};
use super::processors::{Processed, Step, Subject, steps_for};
use super::world::{Engine, Start};
use crate::overworld::features::place::Run;
use crate::overworld::features::structure::commit;

fn reference_of(start: &Start) -> Pos {
    start.pieces.first().map_or((0, 0, 0), |piece| {
        let bbox = &piece.bbox;
        (
            bbox.min.0 + (bbox.max.0 - bbox.min.0 + 1) / 2,
            bbox.min.1,
            bbox.min.2 + (bbox.max.2 - bbox.min.2 + 1) / 2,
        )
    })
}

pub(crate) fn place_start(engine: &Engine, run: &mut Run, start: &Start, write: &Bbox) {
    let reference = reference_of(start);
    for shape in &start.shapes {
        if shape.bbox().intersects(write) {
            shape.place(run, write);
        }
    }
    for piece in &start.pieces {
        if piece.bbox.intersects(write) {
            place_element(engine, run, piece.element, piece, write, reference);
        }
    }
}

fn place_element(
    engine: &Engine,
    run: &mut Run,
    element: &Element,
    piece: &Piece<'static>,
    write: &Bbox,
    reference: Pos,
) {
    match element {
        Element::Empty => {}
        Element::Feature { feature, .. } => {
            if let Some(placed) = engine.feature(feature) {
                placed.place(run, piece.pos);
            }
        }
        Element::List { elements, .. } => {
            for inner in elements {
                place_element(engine, run, inner, piece, write, reference);
            }
        }
        Element::Single {
            blueprint,
            processors,
            projection,
            ..
        } => {
            let Some(blueprint) = blueprint else {
                return;
            };
            let mut steps = vec![Step::IgnoreStructureBlock, Step::JigsawReplacement];
            steps.extend(steps_for(engine.library(), processors));
            if *projection == Projection::TerrainMatching {
                steps.push(Step::gravity());
            }
            let mut processed: Vec<Processed> = Vec::new();
            let mut dropped = false;
            for block in &blueprint.blocks {
                let (rx, ry, rz) = piece.rotation.transform(block.pos);
                let mut subject = Subject {
                    local: block.pos,
                    at: (rx + piece.pos.0, ry + piece.pos.1, rz + piece.pos.2),
                    reference,
                    state: block.state,
                    nbt: block.nbt.as_ref().map(Cow::Borrowed),
                };
                if !steps.iter().all(|step| step.process(run, &mut subject)) {
                    dropped = true;
                    continue;
                }
                processed.push(Processed {
                    local: block.pos,
                    at: subject.at,
                    state: subject.state,
                    nbt: subject.nbt.map(Cow::into_owned),
                });
            }
            if !dropped {
                for step in &steps {
                    step.finalize(run, piece.pos, reference, &mut processed);
                }
            }
            let mut placed: Vec<(Pos, BlockStateId)> = Vec::new();
            let mut entities: Vec<(Pos, Value)> = Vec::new();
            for item in processed {
                if !write.contains(item.at) {
                    continue;
                }
                placed.push((item.at, item.state));
                if let Some(nbt) = item.nbt {
                    entities.push((item.at, nbt));
                }
            }
            commit(run, placed, piece.rotation, false);
            for (at, mut nbt) in entities {
                if let Value::Object(map) = &mut nbt
                    && map.contains_key("LootTable")
                    && !map.contains_key("LootTableSeed")
                {
                    let seed = run.rng.next_long() as i64;
                    map.insert(
                        "LootTableSeed".to_owned(),
                        serde_json::json!({ "$l": seed }),
                    );
                }
                run.region.block_entities.push((at, nbt));
            }
        }
    }
}

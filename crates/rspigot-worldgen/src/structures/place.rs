use rspigot_domain::BlockStateId;
use serde_json::Value;

use super::bbox::{Bbox, Pos};
use super::jigsaw::Piece;
use super::pool::{Element, Projection};
use super::processors::{Step, Subject, steps_for};
use super::world::{Engine, Start};
use crate::overworld::features::place::Run;
use crate::overworld::features::structure::commit;

pub(crate) fn place_start(engine: &Engine, run: &mut Run, start: &Start, write: &Bbox) {
    for shape in &start.shapes {
        if shape.bbox().intersects(write) {
            shape.place(run, write);
        }
    }
    for piece in &start.pieces {
        if piece.bbox.intersects(write) {
            place_element(engine, run, piece.element, piece, write);
        }
    }
}

fn place_element(
    engine: &Engine,
    run: &mut Run,
    element: &Element,
    piece: &Piece<'static>,
    write: &Bbox,
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
                place_element(engine, run, inner, piece, write);
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
            let mut placed: Vec<(Pos, BlockStateId)> = Vec::new();
            let mut entities: Vec<(Pos, Value)> = Vec::new();
            for block in &blueprint.blocks {
                let (rx, ry, rz) = piece.rotation.transform(block.pos);
                let mut subject = Subject {
                    local: block.pos,
                    at: (rx + piece.pos.0, ry + piece.pos.1, rz + piece.pos.2),
                    state: block.state,
                    nbt: block.nbt.as_ref(),
                };
                if !steps.iter().all(|step| step.process(run, &mut subject)) {
                    continue;
                }
                if !write.contains(subject.at) {
                    continue;
                }
                placed.push((subject.at, subject.state));
                if let Some(nbt) = subject.nbt {
                    entities.push((subject.at, nbt.clone()));
                }
            }
            commit(run, placed, piece.rotation, false);
            for (at, mut nbt) in entities {
                if let Value::Object(map) = &mut nbt
                    && map.contains_key("LootTable")
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

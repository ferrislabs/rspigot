mod grid;
mod names;
mod placer;
#[cfg(test)]
mod tests;

use rspigot_blocks::BlockKind as Kind;
use rspigot_domain::BlockStateId;
use serde_json::json;

use super::frame::with;
use super::start;
use super::tpl::{Mirror, Tpl};
use crate::overworld::features::place::Run;
use crate::overworld::features::prov::is_fluid;
use crate::overworld::features::structure::Rotation;
use crate::structures::bbox::{Bbox, Pos};
use crate::structures::jigsaw::Ground;
use crate::structures::library::Library;
use crate::structures::placement::BiomeProbe;
use crate::structures::processors::Step;

const LOOT: &str = "minecraft:chests/woodland_mansion";

#[derive(Debug)]
pub(crate) struct Piece {
    pub(crate) tpl: Tpl,
    pub(crate) bbox: Bbox,
    pub(crate) floor: i32,
}

pub(crate) fn generate<G: Ground>(
    library: &'static Library,
    ground: &G,
    valid: &[u8],
    probe: &impl BiomeProbe,
    seed: i64,
    chunk: (i32, i32),
) -> Option<Vec<Piece>> {
    let mut random = start::random(seed, chunk);
    let (x, z) = (chunk.0 * 16 + 7, chunk.1 * 16 + 7);
    if !start::could_exist(ground, valid, probe, x, z) {
        return None;
    }
    let rotation = Rotation::ALL[random.next_int(4) as usize];
    let (width, depth) = match rotation {
        Rotation::None => (5, 5),
        Rotation::Clockwise90 => (-5, 5),
        Rotation::Clockwise180 => (-5, -5),
        Rotation::Counterclockwise90 => (5, -5),
    };
    let y = start::lowest_y(ground, x, width, z, depth);
    if y < 60 {
        return None;
    }
    let origin = (x, y, z);
    if !start::valid_at(valid, probe, origin) {
        return None;
    }
    let mut pieces = Vec::new();
    for spec in placer::mansion_pieces(origin, rotation, &mut random) {
        let tpl = Tpl::new(
            library,
            spec.name,
            spec.pos,
            spec.rotation,
            spec.mirror,
            (0, 0),
        )?;
        let bbox = tpl.bbox();
        pieces.push(Piece {
            tpl,
            bbox,
            floor: 0,
        });
    }
    let floor = pieces.iter().map(|piece| piece.bbox.min.1).min()?;
    for piece in &mut pieces {
        piece.floor = floor;
    }
    Some(pieces)
}

fn chest_facing(marker: &str, rotation: Rotation) -> Option<&'static str> {
    let face = match marker {
        "ChestWest" => grid::Face::West,
        "ChestEast" => grid::Face::East,
        "ChestSouth" => grid::Face::South,
        "ChestNorth" => grid::Face::North,
        _ => return None,
    };
    let turned = match rotation {
        Rotation::None => face,
        Rotation::Clockwise90 => face.clockwise(),
        Rotation::Clockwise180 => face.opposite(),
        Rotation::Counterclockwise90 => face.counterclockwise(),
    };
    Some(match turned {
        grid::Face::North => "north",
        grid::Face::East => "east",
        grid::Face::South => "south",
        grid::Face::West => "west",
    })
}

fn spawn(run: &mut Run, id: &str, at: Pos) {
    run.region.entities.push(json!({
        "id": id,
        "Pos": [f64::from(at.0) + 0.5, f64::from(at.1), f64::from(at.2) + 0.5],
        "Motion": [0.0, 0.0, 0.0],
        "Rotation": [0.0f32, 0.0f32],
        "PersistenceRequired": { "$b": 1 },
    }));
}

impl Piece {
    pub(crate) fn id(&self) -> &'static str {
        "WMP"
    }

    pub(crate) fn template(&self) -> &'static str {
        self.tpl.name.rsplit('/').next().unwrap_or(self.tpl.name)
    }

    pub(crate) fn mirror_name(&self) -> &'static str {
        match self.tpl.mirror {
            Mirror::None => "NONE",
            Mirror::LeftRight => "LEFT_RIGHT",
            Mirror::FrontBack => "FRONT_BACK",
        }
    }

    pub(crate) fn place(&self, run: &mut Run, write: &Bbox) {
        let markers = self.tpl.place(run, write, &[Step::IgnoreStructureBlock]);
        for marker in &markers {
            let at = marker.at;
            if marker.meta.starts_with("Chest") {
                self.chest(run, at, &marker.meta);
                continue;
            }
            match marker.meta.as_str() {
                "Mage" => spawn(run, "minecraft:evoker", at),
                "Warrior" => spawn(run, "minecraft:vindicator", at),
                "Group of Allays" => {
                    for _ in 0..run.rng.below(3) + 1 {
                        spawn(run, "minecraft:allay", at);
                    }
                }
                _ => continue,
            }
            run.region.set(at.0, at.1, at.2, BlockStateId::AIR);
        }
        self.foundation(run, write);
    }

    fn chest(&self, run: &mut Run, at: Pos, meta: &str) {
        if run.region.get(at.0, at.1, at.2).kind() == Some(Kind::Chest) {
            return;
        }
        let state = match chest_facing(meta, self.tpl.rotation) {
            Some(facing) => with(Kind::Chest, &[("facing", facing)]),
            None => Kind::Chest.default_state(),
        };
        run.region.set(at.0, at.1, at.2, state);
        let seed = run.rng.next_long() as i64;
        run.region.block_entities.push((
            at,
            json!({ "id": "minecraft:chest", "LootTable": LOOT, "LootTableSeed": { "$l": seed } }),
        ));
    }

    fn foundation(&self, run: &mut Run, write: &Bbox) {
        if self.bbox.min.1 != self.floor {
            return;
        }
        let low = run.region.height().min_y;
        for x in self.bbox.min.0.max(write.min.0)..=self.bbox.max.0.min(write.max.0) {
            for z in self.bbox.min.2.max(write.min.2)..=self.bbox.max.2.min(write.max.2) {
                if run.region.get(x, self.floor, z).is_air() {
                    continue;
                }
                let mut y = self.floor - 1;
                while y > low {
                    let state = run.region.get(x, y, z);
                    if !state.is_air() && !is_fluid(state) {
                        break;
                    }
                    run.region.set(x, y, z, Kind::Cobblestone.default_state());
                    y -= 1;
                }
            }
        }
    }
}

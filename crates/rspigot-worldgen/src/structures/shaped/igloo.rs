use rspigot_blocks::BlockKind as Kind;
use rspigot_domain::BlockStateId;

use super::frame::plain;
use super::start;
use super::tpl::{Mirror, Tpl, set_loot, transform};
use crate::legacy::JavaRandom;
use crate::overworld::features::place::Run;
use crate::overworld::features::structure::Rotation;
use crate::overworld::region::Heightmap;
use crate::structures::bbox::{Bbox, Pos};
use crate::structures::jigsaw::{Ground, Heightmap as Map};
use crate::structures::library::Library;
use crate::structures::placement::BiomeProbe;
use crate::structures::processors::Step;

const TOP: &str = "minecraft:igloo/top";
const LADDER: &str = "minecraft:igloo/middle";
const LABORATORY: &str = "minecraft:igloo/bottom";

fn pivot(name: &str) -> (i32, i32) {
    match name {
        TOP => (3, 5),
        LADDER => (1, 1),
        _ => (3, 7),
    }
}

fn offset(name: &str) -> Pos {
    match name {
        TOP => (0, 0, 0),
        LADDER => (2, -3, 4),
        _ => (0, -3, -2),
    }
}

#[derive(Debug)]
pub(crate) struct Piece {
    pub(crate) tpl: Tpl,
    pub(crate) bbox: Bbox,
}

fn piece(
    library: &'static Library,
    name: &'static str,
    at: Pos,
    rotation: Rotation,
    depth: i32,
) -> Option<Piece> {
    let shift = offset(name);
    let pos = (at.0 + shift.0, at.1 + shift.1 - depth, at.2 + shift.2);
    let tpl = Tpl::new(library, name, pos, rotation, Mirror::None, pivot(name))?;
    let bbox = tpl.bbox();
    Some(Piece { tpl, bbox })
}

pub(crate) fn generate<G: Ground>(
    library: &'static Library,
    ground: &G,
    valid: &[u8],
    probe: &impl BiomeProbe,
    seed: i64,
    chunk: (i32, i32),
) -> Option<Vec<Piece>> {
    let mut random: JavaRandom = start::random(seed, chunk);
    start::on_top(ground, valid, probe, Map::WorldSurfaceWg, chunk)?;
    let at = (chunk.0 * 16, 90, chunk.1 * 16);
    let rotation = Rotation::ALL[random.next_int(4) as usize];
    let mut pieces = Vec::new();
    if random.next_double() < 0.5 {
        let rungs = random.next_int(8) + 4;
        pieces.push(piece(library, LABORATORY, at, rotation, rungs * 3)?);
        for index in 0..rungs - 1 {
            pieces.push(piece(library, LADDER, at, rotation, index * 3)?);
        }
    }
    pieces.push(piece(library, TOP, at, rotation, 0)?);
    Some(pieces)
}

impl Piece {
    pub(crate) fn id(&self) -> &'static str {
        "Iglu"
    }

    pub(crate) fn place(&self, run: &mut Run, write: &Bbox) {
        let name = self.tpl.name;
        let relative = transform(
            (3 - offset(name).0, 0, -offset(name).2),
            Mirror::None,
            self.tpl.rotation,
            pivot(name),
        );
        let (x, z) = (self.tpl.pos.0 + relative.0, self.tpl.pos.2 + relative.2);
        let height = run.region.height_at(Heightmap::WorldSurface, x, z);
        let tpl = self
            .tpl
            .at((self.tpl.pos.0, self.tpl.pos.1 + height - 91, self.tpl.pos.2));
        let markers = tpl.place_with(run, write, &[Step::IgnoreStructureBlock], false, None);
        for marker in markers.iter().filter(|marker| marker.meta == "chest") {
            let at = marker.at;
            run.region.set(at.0, at.1, at.2, BlockStateId::AIR);
            set_loot(run, (at.0, at.1 - 1, at.2), "minecraft:chests/igloo_chest");
        }
        if name == TOP {
            let relative = transform((3, 0, 5), Mirror::None, tpl.rotation, pivot(name));
            let at = (
                tpl.pos.0 + relative.0,
                tpl.pos.1 + relative.1,
                tpl.pos.2 + relative.2,
            );
            let below = run.region.get(at.0, at.1 - 1, at.2);
            if !below.is_air() && below.kind() != Some(Kind::Ladder) {
                run.region.set(at.0, at.1, at.2, plain(Kind::SnowBlock));
            }
        }
    }
}

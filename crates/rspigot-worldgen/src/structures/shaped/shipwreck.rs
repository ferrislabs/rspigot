use super::start;
use super::tpl::{Mirror, Tpl, set_loot};
use crate::overworld::features::place::Run;
use crate::overworld::features::structure::Rotation;
use crate::overworld::region::Heightmap;
use crate::structures::bbox::Bbox;
use crate::structures::jigsaw::{Ground, Heightmap as Map};
use crate::structures::library::Library;
use crate::structures::placement::BiomeProbe;
use crate::structures::processors::Step;

const PIVOT: (i32, i32) = (4, 15);

const BEACHED: [&str; 11] = [
    "minecraft:shipwreck/with_mast",
    "minecraft:shipwreck/sideways_full",
    "minecraft:shipwreck/sideways_fronthalf",
    "minecraft:shipwreck/sideways_backhalf",
    "minecraft:shipwreck/rightsideup_full",
    "minecraft:shipwreck/rightsideup_fronthalf",
    "minecraft:shipwreck/rightsideup_backhalf",
    "minecraft:shipwreck/with_mast_degraded",
    "minecraft:shipwreck/rightsideup_full_degraded",
    "minecraft:shipwreck/rightsideup_fronthalf_degraded",
    "minecraft:shipwreck/rightsideup_backhalf_degraded",
];

const OCEAN: [&str; 20] = [
    "minecraft:shipwreck/with_mast",
    "minecraft:shipwreck/upsidedown_full",
    "minecraft:shipwreck/upsidedown_fronthalf",
    "minecraft:shipwreck/upsidedown_backhalf",
    "minecraft:shipwreck/sideways_full",
    "minecraft:shipwreck/sideways_fronthalf",
    "minecraft:shipwreck/sideways_backhalf",
    "minecraft:shipwreck/rightsideup_full",
    "minecraft:shipwreck/rightsideup_fronthalf",
    "minecraft:shipwreck/rightsideup_backhalf",
    "minecraft:shipwreck/with_mast_degraded",
    "minecraft:shipwreck/upsidedown_full_degraded",
    "minecraft:shipwreck/upsidedown_fronthalf_degraded",
    "minecraft:shipwreck/upsidedown_backhalf_degraded",
    "minecraft:shipwreck/sideways_full_degraded",
    "minecraft:shipwreck/sideways_fronthalf_degraded",
    "minecraft:shipwreck/sideways_backhalf_degraded",
    "minecraft:shipwreck/rightsideup_full_degraded",
    "minecraft:shipwreck/rightsideup_fronthalf_degraded",
    "minecraft:shipwreck/rightsideup_backhalf_degraded",
];

#[derive(Debug)]
pub(crate) struct Piece {
    pub(crate) tpl: Tpl,
    pub(crate) bbox: Bbox,
    beached: bool,
    drop: i32,
}

pub(crate) fn generate<G: Ground>(
    beached: bool,
    library: &'static Library,
    ground: &G,
    valid: &[u8],
    probe: &impl BiomeProbe,
    seed: i64,
    chunk: (i32, i32),
) -> Option<Piece> {
    let map = if beached {
        Map::WorldSurfaceWg
    } else {
        Map::OceanFloorWg
    };
    let mut random = start::random(seed, chunk);
    start::on_top(ground, valid, probe, map, chunk)?;
    let rotation = Rotation::ALL[random.next_int(4) as usize];
    let at = (chunk.0 * 16, 90, chunk.1 * 16);
    let names: &[&'static str] = if beached { &BEACHED } else { &OCEAN };
    let name = names[random.next_int(names.len() as i32) as usize];
    let tpl = Tpl::new(library, name, at, rotation, Mirror::None, PIVOT)?;
    let bbox = tpl.bbox();
    let drop = random.next_int(3);
    Some(Piece {
        tpl,
        bbox,
        beached,
        drop,
    })
}

fn marker_table(meta: &str) -> Option<&'static str> {
    Some(match meta {
        "map_chest" => "minecraft:chests/shipwreck_map",
        "treasure_chest" => "minecraft:chests/shipwreck_treasure",
        "supply_chest" => "minecraft:chests/shipwreck_supply",
        _ => return None,
    })
}

impl Piece {
    pub(crate) fn id(&self) -> &'static str {
        "Shipwreck"
    }

    fn height(&self, run: &mut Run) -> i32 {
        let map = if self.beached {
            Heightmap::WorldSurface
        } else {
            Heightmap::OceanFloor
        };
        let size = self.tpl.size();
        let at = self.tpl.pos;
        let mut lowest = run.region.height().max_y() + 1;
        let mut total = 0;
        for x in at.0..at.0 + size.0 {
            for z in at.2..at.2 + size.2 {
                let height = run.region.height_at(map, x, z);
                total += height;
                lowest = lowest.min(height);
            }
        }
        if self.beached {
            lowest - size.1 / 2 - self.drop
        } else {
            total / (size.0 * size.2)
        }
    }

    pub(crate) fn place(&self, run: &mut Run, write: &Bbox) {
        let y = self.height(run);
        let tpl = self.tpl.at((self.tpl.pos.0, y, self.tpl.pos.2));
        let markers = tpl.place(run, write, &[Step::IgnoreAir]);
        for marker in &markers {
            if let Some(table) = marker_table(&marker.meta) {
                set_loot(run, (marker.at.0, marker.at.1 - 1, marker.at.2), table);
            }
        }
    }
}

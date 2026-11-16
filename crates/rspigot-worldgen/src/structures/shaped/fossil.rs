use rspigot_blocks::BlockKind as Kind;

use super::start;
use super::tpl::{Mirror, Tpl, center};
use crate::legacy::JavaRandom;
use crate::overworld::features::place::Run;
use crate::overworld::features::structure::Rotation;
use crate::structures::bbox::Bbox;
use crate::structures::jigsaw::{Base, Ground};
use crate::structures::library::Library;
use crate::structures::placement::BiomeProbe;
use crate::structures::processors::{Step, position_seed};
use crate::structures::structure::HeightAnchor;

const FOSSILS: [&str; 14] = [
    "minecraft:nether_fossils/fossil_1",
    "minecraft:nether_fossils/fossil_2",
    "minecraft:nether_fossils/fossil_3",
    "minecraft:nether_fossils/fossil_4",
    "minecraft:nether_fossils/fossil_5",
    "minecraft:nether_fossils/fossil_6",
    "minecraft:nether_fossils/fossil_7",
    "minecraft:nether_fossils/fossil_8",
    "minecraft:nether_fossils/fossil_9",
    "minecraft:nether_fossils/fossil_10",
    "minecraft:nether_fossils/fossil_11",
    "minecraft:nether_fossils/fossil_12",
    "minecraft:nether_fossils/fossil_13",
    "minecraft:nether_fossils/fossil_14",
];

#[derive(Debug, Clone, Copy)]
pub(crate) struct Height {
    pub(crate) min: HeightAnchor,
    pub(crate) max: HeightAnchor,
}

#[derive(Debug)]
pub(crate) struct Piece {
    pub(crate) tpl: Tpl,
    pub(crate) bbox: Bbox,
}

pub(crate) fn generate<G: Ground>(
    height: Height,
    library: &'static Library,
    ground: &G,
    valid: &[u8],
    probe: &impl BiomeProbe,
    seed: i64,
    chunk: (i32, i32),
) -> Option<Piece> {
    let mut random = start::random(seed, chunk);
    let x = chunk.0 * 16 + random.next_int(16);
    let z = chunk.1 * 16 + random.next_int(16);
    let sea = ground.sea_level();
    let low = height.min.resolve(ground.min_y(), ground.gen_depth())?;
    let high = height.max.resolve(ground.min_y(), ground.gen_depth())?;
    let mut y = if low > high {
        low
    } else {
        random.between(low, high)
    };
    loop {
        if y <= sea {
            break;
        }
        let above = ground.base(x, y, z);
        y -= 1;
        let below = ground.base(x, y, z);
        if above == Base::Air && below == Base::Solid {
            break;
        }
    }
    if sea >= y || !start::valid_at(valid, probe, (x, y, z)) {
        return None;
    }
    let rotation = Rotation::ALL[random.next_int(4) as usize];
    let name = FOSSILS[random.next_int(14) as usize];
    let tpl = Tpl::new(library, name, (x, y, z), rotation, Mirror::None, (0, 0))?;
    let bbox = tpl.bbox();
    Some(Piece { tpl, bbox })
}

impl Piece {
    pub(crate) fn id(&self) -> &'static str {
        "NeFos"
    }

    pub(crate) fn place(&self, run: &mut Run, write: &Bbox) {
        let mut area = *write;
        area.encapsulate(&self.bbox);
        self.tpl.place(run, &area, &[Step::IgnoreAir]);
        self.dried_ghast(run, &area);
    }

    fn dried_ghast(&self, run: &mut Run, area: &Bbox) {
        let mut base = JavaRandom::new(run.seed);
        let factory = base.next_long();
        let middle = center(&self.bbox);
        let mut random = JavaRandom::new(position_seed(middle) ^ factory);
        if random.next_float() >= 0.5 {
            return;
        }
        let b = self.bbox;
        let x = b.min.0 + random.next_int(b.max.0 - b.min.0 + 1);
        let z = b.min.2 + random.next_int(b.max.2 - b.min.2 + 1);
        let at = (x, b.min.1, z);
        if !run.region.get(at.0, at.1, at.2).is_air() || !area.contains(at) {
            return;
        }
        let turn = Rotation::ALL[random.next_int(4) as usize];
        let state = turn.state(Kind::DriedGhast.default_state());
        run.region.set(at.0, at.1, at.2, state);
    }
}

use rspigot_domain::{BlockStateId, ChunkPos};

use crate::buffer::ChunkBuffer;
use crate::resources::{Biome, BlockKind, Palette, Tree};
use crate::rng::{Rng, hash2, mix, unit};

const TREE_ATTEMPTS: usize = 14;
const SALT_TREES: u64 = 0x7433_E5A1;
const SALT_PLANTS: u64 = 0x91A7_0CC3;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Ground {
    Grass,
    Sand,
    Bare,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct ColumnInfo {
    pub(crate) height: i32,
    pub(crate) ground: Ground,
    pub(crate) snow_cover: bool,
    pub(crate) biome: Biome,
}

fn tree_rule(biome: Biome, pick: f64) -> Option<(f64, Tree)> {
    match biome {
        Biome::Forest => Some((0.8, if pick < 0.2 { Tree::Birch } else { Tree::Oak })),
        Biome::BirchForest => Some((0.8, Tree::Birch)),
        Biome::Taiga => Some((0.8, Tree::Spruce)),
        Biome::SnowyTaiga => Some((0.7, Tree::Spruce)),
        Biome::Plains => Some((0.03, Tree::Oak)),
        Biome::SnowyPlains => Some((0.02, Tree::Spruce)),
        Biome::WindsweptHills => Some((0.2, Tree::Spruce)),
        _ => None,
    }
}

pub(crate) fn trees(
    buffer: &mut ChunkBuffer,
    palette: &Palette,
    seed: u64,
    pos: ChunkPos,
    columns: &[ColumnInfo; 256],
    sea_level: i32,
) {
    let mut rng = Rng::for_chunk(seed, pos.x, pos.z, SALT_TREES);
    let mut placed = [(0usize, 0usize); TREE_ATTEMPTS];
    let mut count = 0;
    for _ in 0..TREE_ATTEMPTS {
        let x = 3 + rng.below(10) as usize;
        let z = 3 + rng.below(10) as usize;
        let roll = rng.unit();
        let pick = rng.unit();
        let info = &columns[(z << 4) | x];
        let Some((chance, tree)) = tree_rule(info.biome, pick) else {
            continue;
        };
        let rooted =
            matches!(info.ground, Ground::Grass) && !buffer.get(x, info.height, z).is_air();
        let crowded = placed[..count]
            .iter()
            .any(|(px, pz)| px.abs_diff(x) < 3 && pz.abs_diff(z) < 3);
        if roll >= chance || !rooted || crowded || info.height < sea_level {
            continue;
        }
        if super::trees::plant(buffer, palette, tree, x, z, info.height + 1, &mut rng) {
            placed[count] = (x, z);
            count += 1;
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct PlantRates {
    tall_grass: f64,
    flower: f64,
    grass: f64,
    fern: bool,
}

fn plant_rates(biome: Biome) -> Option<PlantRates> {
    let rates = |tall_grass, flower, grass, fern| PlantRates {
        tall_grass,
        flower,
        grass,
        fern,
    };
    match biome {
        Biome::Plains => Some(rates(0.05, 0.035, 0.28, false)),
        Biome::Forest | Biome::BirchForest => Some(rates(0.03, 0.02, 0.16, false)),
        Biome::Taiga => Some(rates(0.0, 0.0, 0.18, true)),
        Biome::WindsweptHills => Some(rates(0.0, 0.0, 0.12, false)),
        Biome::River => Some(rates(0.0, 0.0, 0.08, false)),
        _ => None,
    }
}

pub(crate) fn ground_cover(
    buffer: &mut ChunkBuffer,
    palette: &Palette,
    seed: u64,
    pos: ChunkPos,
    columns: &[ColumnInfo; 256],
) {
    let flowers = [
        palette[BlockKind::Dandelion],
        palette[BlockKind::Poppy],
        palette[BlockKind::AzureBluet],
        palette[BlockKind::OxeyeDaisy],
        palette[BlockKind::Cornflower],
    ];
    for z in 0..16usize {
        for x in 0..16usize {
            let info = &columns[(z << 4) | x];
            let y = info.height + 1;
            if !buffer.get(x, y, z).is_air() || buffer.get(x, info.height, z).is_air() {
                continue;
            }
            let hash = hash2(
                seed ^ SALT_PLANTS,
                pos.x * 16 + x as i32,
                pos.z * 16 + z as i32,
            );
            let roll = unit(hash);
            let pick = unit(mix(hash));
            if info.snow_cover {
                buffer.set(x, y, z, palette[BlockKind::SnowLayer]);
                continue;
            }
            match info.ground {
                Ground::Grass => {
                    let Some(rates) = plant_rates(info.biome) else {
                        continue;
                    };
                    place_plant(buffer, palette, &flowers, &rates, (x, y, z), roll, pick);
                }
                Ground::Sand if info.biome == Biome::Desert && roll < 0.008 => {
                    buffer.set(x, y, z, palette[BlockKind::DeadBush]);
                }
                _ => {}
            }
        }
    }
}

fn place_plant(
    buffer: &mut ChunkBuffer,
    palette: &Palette,
    flowers: &[BlockStateId; 5],
    rates: &PlantRates,
    (x, y, z): (usize, i32, usize),
    roll: f64,
    pick: f64,
) {
    let tall_end = rates.tall_grass;
    let flower_end = tall_end + rates.flower;
    let grass_end = flower_end + rates.grass;
    if roll < tall_end {
        if buffer.get(x, y + 1, z).is_air() {
            buffer.set(x, y, z, palette[BlockKind::TallGrassLower]);
            buffer.set(x, y + 1, z, palette[BlockKind::TallGrassUpper]);
        }
    } else if roll < flower_end {
        let index = ((pick * flowers.len() as f64) as usize).min(flowers.len() - 1);
        buffer.set(x, y, z, flowers[index]);
    } else if roll < grass_end {
        let block = if rates.fern && pick < 0.4 {
            BlockKind::Fern
        } else {
            BlockKind::ShortGrass
        };
        buffer.set(x, y, z, palette[block]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rspigot_domain::world::WorldHeight;

    fn flat_columns(biome: Biome, ground: Ground, snow: bool) -> [ColumnInfo; 256] {
        [ColumnInfo {
            height: 70,
            ground,
            snow_cover: snow,
            biome,
        }; 256]
    }

    fn solid_ground(palette: &Palette) -> ChunkBuffer {
        let mut buffer = ChunkBuffer::new(WorldHeight::OVERWORLD);
        for y in -64..=70 {
            buffer.fill_layer(y, palette[BlockKind::GrassBlock]);
        }
        buffer
    }

    fn count(buffer: &ChunkBuffer, state: BlockStateId) -> usize {
        buffer.raw().iter().filter(|s| **s == state.0).count()
    }

    #[test]
    fn forests_grow_trees_and_plains_stay_open() {
        let palette = Palette::resolve();
        let mut forest_logs = 0;
        let mut plains_logs = 0;
        for cx in 0..20 {
            let mut forest = solid_ground(&palette);
            trees(
                &mut forest,
                &palette,
                7,
                ChunkPos::new(cx, 0),
                &flat_columns(Biome::BirchForest, Ground::Grass, false),
                63,
            );
            forest_logs += count(&forest, palette.log(Tree::Birch));
            let mut plains = solid_ground(&palette);
            trees(
                &mut plains,
                &palette,
                7,
                ChunkPos::new(cx, 0),
                &flat_columns(Biome::Plains, Ground::Grass, false),
                63,
            );
            plains_logs += count(&plains, palette.log(Tree::Oak));
        }
        assert!(forest_logs > 20 * 20, "{forest_logs}");
        assert!(plains_logs < forest_logs / 8, "{plains_logs}");
    }

    #[test]
    fn trees_stay_inside_the_chunk_and_off_sand_and_water() {
        let palette = Palette::resolve();
        let mut buffer = solid_ground(&palette);
        trees(
            &mut buffer,
            &palette,
            3,
            ChunkPos::new(-5, 9),
            &flat_columns(Biome::Taiga, Ground::Sand, false),
            63,
        );
        assert_eq!(count(&buffer, palette.log(Tree::Spruce)), 0);
        let mut buffer = solid_ground(&palette);
        let mut underwater = flat_columns(Biome::Taiga, Ground::Grass, false);
        for info in &mut underwater {
            info.height = 50;
        }
        trees(
            &mut buffer,
            &palette,
            3,
            ChunkPos::new(-5, 9),
            &underwater,
            63,
        );
        assert_eq!(count(&buffer, palette.log(Tree::Spruce)), 0);
    }

    #[test]
    fn snowy_ground_gets_snow_layers() {
        let palette = Palette::resolve();
        let mut buffer = solid_ground(&palette);
        ground_cover(
            &mut buffer,
            &palette,
            1,
            ChunkPos::new(0, 0),
            &flat_columns(Biome::SnowyPlains, Ground::Grass, true),
        );
        assert_eq!(count(&buffer, palette[BlockKind::SnowLayer]), 256);
    }

    #[test]
    fn plains_get_grass_and_flowers() {
        let palette = Palette::resolve();
        let mut buffer = solid_ground(&palette);
        for cx in 0..6 {
            ground_cover(
                &mut buffer,
                &palette,
                1,
                ChunkPos::new(cx, 0),
                &flat_columns(Biome::Plains, Ground::Grass, false),
            );
        }
        assert!(count(&buffer, palette[BlockKind::ShortGrass]) > 30);
        let flowers: usize = [
            BlockKind::Dandelion,
            BlockKind::Poppy,
            BlockKind::AzureBluet,
            BlockKind::OxeyeDaisy,
            BlockKind::Cornflower,
        ]
        .iter()
        .map(|b| count(&buffer, palette[*b]))
        .sum();
        assert!(flowers > 0);
        assert_eq!(
            count(&buffer, palette[BlockKind::TallGrassLower]),
            count(&buffer, palette[BlockKind::TallGrassUpper])
        );
    }

    #[test]
    fn deserts_only_get_dead_bushes_on_sand() {
        let palette = Palette::resolve();
        let mut buffer = ChunkBuffer::new(WorldHeight::OVERWORLD);
        for y in -64..=70 {
            buffer.fill_layer(y, palette[BlockKind::Sand]);
        }
        for cx in 0..30 {
            ground_cover(
                &mut buffer,
                &palette,
                5,
                ChunkPos::new(cx, 0),
                &flat_columns(Biome::Desert, Ground::Sand, false),
            );
        }
        assert!(count(&buffer, palette[BlockKind::DeadBush]) > 0);
        assert_eq!(count(&buffer, palette[BlockKind::ShortGrass]), 0);
    }

    #[test]
    fn nothing_grows_on_occupied_columns() {
        let palette = Palette::resolve();
        let mut buffer = solid_ground(&palette);
        for z in 0..16 {
            for x in 0..16 {
                buffer.set(x, 71, z, palette[BlockKind::Stone]);
            }
        }
        ground_cover(
            &mut buffer,
            &palette,
            1,
            ChunkPos::new(0, 0),
            &flat_columns(Biome::SnowyPlains, Ground::Grass, true),
        );
        assert_eq!(count(&buffer, palette[BlockKind::SnowLayer]), 0);
    }
}

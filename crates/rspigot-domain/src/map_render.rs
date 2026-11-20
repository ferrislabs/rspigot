use rspigot_blocks::{BlockKind, Face, Support};
use rspigot_data::map_colors::{self, Brightness, DIRT, NONE, STONE, WATER};

use crate::map_data::{MAP_SIZE, MapBanner, MapData};
use crate::player::PlayerId;
use crate::{BlockPos, BlockStateId};

const FIRE: u8 = 4;
const SIZE: i32 = MAP_SIZE as i32;

pub trait MapTerrain {
    fn has_ceiling(&self) -> bool;
    fn min_y(&self) -> i32;
    fn is_loaded(&self, x: i32, z: i32) -> bool;
    fn surface(&self, x: i32, z: i32) -> i32;
    fn state(&self, x: i32, y: i32, z: i32) -> BlockStateId;
    fn banner(&self, pos: BlockPos) -> Option<MapBanner>;
}

fn color_of(state: BlockStateId) -> u8 {
    map_colors::color_of_state(state.0)
}

fn has_fluid(state: BlockStateId) -> bool {
    state.flags().has_fluid()
}

fn fluid_color(state: BlockStateId) -> u8 {
    let flags = state.flags();
    if flags.contains(rspigot_blocks::StateFlags::WATER) {
        WATER
    } else {
        FIRE
    }
}

#[derive(Default)]
struct Tally(Vec<(u8, i32)>);

impl Tally {
    fn add(&mut self, color: u8, count: i32) {
        match self.0.iter_mut().find(|(c, _)| *c == color) {
            Some(entry) => entry.1 += count,
            None => self.0.push((color, count)),
        }
    }

    fn highest(&self) -> u8 {
        let mut best: Option<(u8, i32)> = None;
        for &(color, count) in &self.0 {
            if best.is_none_or(|(_, top)| count > top) {
                best = Some((color, count));
            }
        }
        best.map_or(NONE, |(color, _)| color)
    }
}

struct Sample {
    water_depth: i32,
    height: f64,
}

fn sample_block(terrain: &impl MapTerrain, x: i32, z: i32) -> (u8, i32, i32) {
    let min_y = terrain.min_y();
    let mut y = terrain.surface(x, z) + 1;
    if y <= min_y {
        return (color_of(BlockKind::Bedrock.default_state()), 0, y);
    }
    let mut state;
    loop {
        y -= 1;
        state = terrain.state(x, y, z);
        if color_of(state) != NONE || y <= min_y {
            break;
        }
    }
    let mut color = color_of(state);
    let mut depth = 0;
    if y > min_y && has_fluid(state) {
        let mut below = y - 1;
        loop {
            let at = below;
            below -= 1;
            let under = terrain.state(x, at, z);
            depth += 1;
            if !(below > min_y && has_fluid(under)) {
                break;
            }
        }
        if !state.is_face_sturdy(Face::Up, Support::Full) {
            color = fluid_color(state);
        }
    }
    (color, depth, y)
}

fn sample_column(terrain: &impl MapTerrain, x: i32, z: i32, scale: i32) -> (Tally, Sample) {
    let mut tally = Tally::default();
    if terrain.has_ceiling() {
        let mut hash = x.wrapping_add(z.wrapping_mul(231_871));
        hash = hash
            .wrapping_mul(hash)
            .wrapping_mul(31_287_121)
            .wrapping_add(hash.wrapping_mul(11));
        if (hash >> 20) & 1 == 0 {
            tally.add(DIRT, 10);
        } else {
            tally.add(STONE, 100);
        }
        return (
            tally,
            Sample {
                water_depth: 0,
                height: 100.0,
            },
        );
    }
    let mut depth = 0;
    let mut height = 0.0;
    for dx in 0..scale {
        for dz in 0..scale {
            let (color, water, y) = sample_block(terrain, x + dx, z + dz);
            depth += water;
            height += f64::from(y) / f64::from(scale * scale);
            tally.add(color, 1);
        }
    }
    (
        tally,
        Sample {
            water_depth: depth / (scale * scale),
            height,
        },
    )
}

pub fn update_map(map: &mut MapData, terrain: &impl MapTerrain, player: PlayerId, x: f64, z: f64) {
    let scale = 1 << map.scale;
    let (center_x, center_z) = (map.center_x, map.center_z);
    let px = crate::mth::floor(x - f64::from(center_x)) / scale + 64;
    let pz = crate::mth::floor(z - f64::from(center_z)) / scale + 64;
    let mut radius = SIZE / scale;
    if terrain.has_ceiling() {
        radius /= 2;
    }
    let step = map.step_of(player);
    let mut carry = false;
    for column in (px - radius + 1)..(px + radius) {
        if (column & 15) != (step & 15) && !carry {
            continue;
        }
        carry = false;
        let mut previous = 0.0;
        for row in (pz - radius - 1)..(pz + radius) {
            if column < 0 || row < -1 || column >= SIZE || row >= SIZE {
                continue;
            }
            let distance = (column - px).pow(2) + (row - pz).pow(2);
            let edge = distance > (radius - 2) * (radius - 2);
            let block_x = (center_x / scale + column - 64) * scale;
            let block_z = (center_z / scale + row - 64) * scale;
            if !terrain.is_loaded(block_x, block_z) {
                continue;
            }
            let (tally, sample) = sample_column(terrain, block_x, block_z, scale);
            for dx in 0..scale {
                for dz in 0..scale {
                    if !terrain.has_ceiling() {
                        map.check_banners(block_x + dx, block_z + dz, |pos| terrain.banner(pos));
                    }
                }
            }
            let color = tally.highest();
            let parity = f64::from((column + row) & 1);
            let brightness = if color == WATER {
                let shade = f64::from(sample.water_depth) * 0.1 + parity * 0.2;
                if shade < 0.5 {
                    Brightness::High
                } else if shade > 0.9 {
                    Brightness::Low
                } else {
                    Brightness::Normal
                }
            } else {
                let shade =
                    (sample.height - previous) * 4.0 / f64::from(scale + 4) + (parity - 0.5) * 0.4;
                if shade > 0.6 {
                    Brightness::High
                } else if shade < -0.6 {
                    Brightness::Low
                } else {
                    Brightness::Normal
                }
            };
            previous = sample.height;
            if row < 0 || distance >= radius * radius || (edge && (column + row) & 1 == 0) {
                continue;
            }
            carry |= map.update_color(column, row, map_colors::packed(color, brightness));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rspigot_blocks::BlockKind;
    use rspigot_data::map_colors::{Brightness, DIRT, STONE, WATER, packed};

    type Column = Box<dyn Fn(i32, i32) -> Vec<(i32, BlockKind)>>;

    struct Terrain {
        ceiling: bool,
        loaded: bool,
        columns: Column,
    }

    impl Terrain {
        fn flat(kind: BlockKind, top: i32) -> Self {
            Self {
                ceiling: false,
                loaded: true,
                columns: Box::new(move |_, _| vec![(top, kind)]),
            }
        }
    }

    impl MapTerrain for Terrain {
        fn has_ceiling(&self) -> bool {
            self.ceiling
        }

        fn min_y(&self) -> i32 {
            -64
        }

        fn is_loaded(&self, _: i32, _: i32) -> bool {
            self.loaded
        }

        fn surface(&self, x: i32, z: i32) -> i32 {
            (self.columns)(x, z)
                .iter()
                .map(|(y, _)| y + 1)
                .max()
                .unwrap_or(-64)
        }

        fn state(&self, x: i32, y: i32, z: i32) -> BlockStateId {
            (self.columns)(x, z)
                .iter()
                .find(|(top, _)| *top == y)
                .map_or(BlockKind::Air.default_state(), |(_, kind)| {
                    kind.default_state()
                })
        }

        fn banner(&self, _: BlockPos) -> Option<MapBanner> {
            None
        }
    }

    fn id() -> PlayerId {
        PlayerId::offline("mapper")
    }

    fn painted(map: &mut MapData, terrain: &Terrain, x: f64, z: f64) {
        for _ in 0..16 {
            update_map(map, terrain, id(), x, z);
        }
    }

    fn pixel(map: &MapData, x: usize, y: usize) -> u8 {
        map.colors()[x + y * 128]
    }

    #[test]
    fn flat_grass_paints_the_whole_map_in_the_normal_shade() {
        let terrain = Terrain::flat(BlockKind::GrassBlock, 63);
        let mut map = MapData::fresh(0.0, 0.0, 0, true, false, "minecraft:overworld");
        painted(&mut map, &terrain, 0.0, 0.0);
        assert_eq!(pixel(&map, 64, 64), 4 + 1);
        assert_eq!(pixel(&map, 0, 0), 4 + 1);
        assert_eq!(pixel(&map, 127, 127), 4 + 1);
        assert!(map.colors().iter().all(|c| *c == 4 + 1));
    }

    #[test]
    fn a_painted_map_that_did_not_change_sends_nothing_more() {
        let terrain = Terrain::flat(BlockKind::GrassBlock, 63);
        let mut map = MapData::fresh(0.0, 0.0, 0, true, false, "minecraft:overworld");
        painted(&mut map, &terrain, 0.0, 0.0);
        let mut who = crate::map_data::Carrier {
            id: id(),
            name: "mapper",
            x: 0.0,
            z: 0.0,
            yaw: 0.0,
            dimension: "minecraft:overworld",
            carries: true,
        };
        who.x = 0.0;
        let level = crate::map_data::LevelRef {
            nether: false,
            game_time: 0,
        };
        map.tick_carried_by(&who, None, &level, &[]);
        assert!(map.next_update(id(), 1).is_some());
        painted(&mut map, &terrain, 0.0, 0.0);
        let update = map.next_update(id(), 1);
        assert!(update.is_none_or(|u| u.patch.is_none()));
    }

    #[test]
    fn the_generated_colour_table_lines_up_with_the_block_states() {
        assert_eq!(map_colors::state_count(), rspigot_blocks::STATE_COUNT);
        assert_eq!(color_of(BlockKind::Air.default_state()), NONE);
        assert_eq!(color_of(BlockKind::GrassBlock.default_state()), 1);
        assert_eq!(color_of(BlockKind::Water.default_state()), WATER);
        assert_eq!(color_of(BlockKind::Stone.default_state()), STONE);
        assert_eq!(color_of(BlockKind::Dirt.default_state()), DIRT);
        assert_eq!(color_of(BlockKind::Lava.default_state()), FIRE);
    }

    #[test]
    fn water_is_shaded_by_its_depth() {
        let shallow = Terrain {
            ceiling: false,
            loaded: true,
            columns: Box::new(|_, _| vec![(62, BlockKind::Water), (61, BlockKind::Sand)]),
        };
        let mut map = MapData::fresh(0.0, 0.0, 0, true, false, "minecraft:overworld");
        painted(&mut map, &shallow, 0.0, 0.0);
        let value = pixel(&map, 64, 64);
        assert_eq!(value >> 2, WATER);
        assert_eq!(value & 3, Brightness::High as u8);
        let deep = Terrain {
            ceiling: false,
            loaded: true,
            columns: Box::new(|_, _| {
                let mut column = vec![(62, BlockKind::Water)];
                column.extend((40..62).map(|y| (y, BlockKind::Water)));
                column.push((39, BlockKind::Sand));
                column
            }),
        };
        let mut map = MapData::fresh(0.0, 0.0, 0, true, false, "minecraft:overworld");
        painted(&mut map, &deep, 0.0, 0.0);
        assert_eq!(pixel(&map, 64, 64) & 3, Brightness::Low as u8);
    }

    #[test]
    fn a_rise_towards_the_south_is_lit_and_a_fall_is_dark() {
        let rising = Terrain {
            ceiling: false,
            loaded: true,
            columns: Box::new(|_, z| vec![(60 + z.clamp(-20, 20) * 2, BlockKind::Stone)]),
        };
        let mut map = MapData::fresh(0.0, 0.0, 0, true, false, "minecraft:overworld");
        painted(&mut map, &rising, 0.0, 0.0);
        let lit = pixel(&map, 64, 70);
        assert_eq!(lit >> 2, STONE);
        assert_eq!(lit & 3, Brightness::High as u8);
        let falling = Terrain {
            ceiling: false,
            loaded: true,
            columns: Box::new(|_, z| vec![(60 - z.clamp(-20, 20) * 2, BlockKind::Stone)]),
        };
        let mut map = MapData::fresh(0.0, 0.0, 0, true, false, "minecraft:overworld");
        painted(&mut map, &falling, 0.0, 0.0);
        assert_eq!(pixel(&map, 64, 70) & 3, Brightness::Low as u8);
    }

    #[test]
    fn unloaded_chunks_are_left_blank() {
        let mut terrain = Terrain::flat(BlockKind::Stone, 63);
        terrain.loaded = false;
        let mut map = MapData::fresh(0.0, 0.0, 0, true, false, "minecraft:overworld");
        painted(&mut map, &terrain, 0.0, 0.0);
        assert!(map.colors().iter().all(|c| *c == 0));
    }

    #[test]
    fn a_bigger_scale_samples_a_square_and_keeps_the_commonest_colour() {
        let terrain = Terrain {
            ceiling: false,
            loaded: true,
            columns: Box::new(|x, _| {
                let kind = if x.rem_euclid(2) == 0 {
                    BlockKind::Stone
                } else {
                    BlockKind::Dirt
                };
                vec![(63, kind)]
            }),
        };
        let mut map = MapData::fresh(0.0, 0.0, 1, true, false, "minecraft:overworld");
        painted(&mut map, &terrain, 0.0, 0.0);
        let color = pixel(&map, 64, 64) >> 2;
        assert!(color == STONE || color == DIRT);
        assert_eq!(color, STONE);
    }

    #[test]
    fn a_ceiling_dimension_uses_the_hash_pattern_of_dirt_and_stone() {
        let terrain = Terrain {
            ceiling: true,
            loaded: true,
            columns: Box::new(|_, _| vec![(40, BlockKind::Netherrack)]),
        };
        let mut map = MapData::fresh(0.0, 0.0, 0, true, false, "minecraft:the_nether");
        painted(&mut map, &terrain, 0.0, 0.0);
        let colors: Vec<u8> = (0..128)
            .flat_map(|x| (0..128).map(move |y| (x, y)))
            .map(|(x, y)| pixel(&map, x, y) >> 2)
            .filter(|c| *c != 0)
            .collect();
        assert!(!colors.is_empty());
        assert!(colors.iter().all(|c| *c == DIRT || *c == STONE));
        assert!(colors.contains(&DIRT) && colors.contains(&STONE));
        let shade = pixel(&map, 64, 64) & 3;
        assert_eq!(packed(DIRT, Brightness::Normal) & 3, 1);
        assert!(shade <= 3);
    }

    #[test]
    fn a_banner_that_is_gone_leaves_the_map() {
        let terrain = Terrain::flat(BlockKind::Stone, 63);
        let mut map = MapData::fresh(0.0, 0.0, 0, true, false, "minecraft:overworld");
        let banner = MapBanner {
            pos: BlockPos::new(3, 64, 3),
            color: 0,
            name: None,
        };
        assert!(map.toggle_banner(banner, 3.5, 3.5));
        assert_eq!(map.decorations().count(), 1);
        painted(&mut map, &terrain, 0.0, 0.0);
        assert_eq!(map.decorations().count(), 0);
        assert!(map.banners().is_empty());
    }
}

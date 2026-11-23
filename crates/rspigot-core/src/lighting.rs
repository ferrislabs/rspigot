use std::collections::VecDeque;

use rspigot_domain::chunk::{ChunkColumn, LightArray};
use rspigot_domain::{BlockPos, BlockStateId, ChunkPos};
use rustc_hash::FxHashMap;

use crate::light_shapes::{self, DIRECTIONS};

const MAX_LIGHT: u8 = 15;

pub fn opacity(state: BlockStateId) -> u8 {
    state.light_dampening()
}

pub fn emission(state: BlockStateId) -> u8 {
    state.light_emission()
}

pub fn light_level(chunk: &ChunkColumn, pos: BlockPos, sky: bool) -> i32 {
    let layers = if sky {
        &chunk.light.sky
    } else {
        &chunk.light.block
    };
    let sections = layers.len().max(1) as i32;
    let index = ((pos.y >> 4) - chunk.height().min_section() + 1).clamp(0, sections - 1) as usize;
    layers
        .get(index)
        .and_then(Option::as_ref)
        .map_or(0, |layer| {
            i32::from(layer.get(
                (pos.x & 15) as usize,
                (pos.y & 15) as usize,
                (pos.z & 15) as usize,
            ))
        })
}

pub fn affects_light(old: BlockStateId, new: BlockStateId) -> bool {
    opacity(old) != opacity(new) || emission(old) != emission(new)
}

pub fn has_light(chunk: &ChunkColumn) -> bool {
    chunk
        .light
        .sky
        .iter()
        .chain(chunk.light.block.iter())
        .any(Option::is_some)
}

pub fn sky_is_consistent(chunk: &ChunkColumn) -> bool {
    let min_y = chunk.height().min_y;
    for z in 0..16 {
        for x in 0..16 {
            'column: for (index, section) in chunk.sections().iter().enumerate().rev() {
                if section.is_empty() {
                    continue;
                }
                for y in (0..16).rev() {
                    let state = section.block(x, y, z);
                    if opacity(state) != 0 {
                        break 'column;
                    }
                    let pos = BlockPos::new(x as i32, min_y + (index * 16 + y) as i32, z as i32);
                    if light_level(chunk, pos, true) != i32::from(MAX_LIGHT) {
                        return false;
                    }
                }
            }
        }
    }
    true
}

struct Volume {
    height: usize,
    levels: Vec<u8>,
}

impl Volume {
    fn new(height: usize) -> Self {
        Self {
            height,
            levels: vec![0; 256 * height],
        }
    }

    const fn index(x: usize, y: usize, z: usize) -> usize {
        (y << 8) | (z << 4) | x
    }
}

struct Cells<'a> {
    chunk: &'a ChunkColumn,
    opacity: Vec<u8>,
    shaped: FxHashMap<usize, BlockStateId>,
}

impl<'a> Cells<'a> {
    fn new(chunk: &'a ChunkColumn) -> Self {
        let height = chunk.height().height as usize;
        let mut opacity = vec![0u8; 256 * height];
        let mut shaped = FxHashMap::default();
        for (section_index, section) in chunk.sections().iter().enumerate() {
            if section.is_empty() {
                continue;
            }
            let has_shaped = section
                .blocks()
                .palette()
                .iter()
                .any(|state| light_shapes::uses_shape(*state));
            for y in 0..16 {
                for z in 0..16 {
                    for x in 0..16 {
                        let state = section.block(x, y, z);
                        let index = Volume::index(x, section_index * 16 + y, z);
                        opacity[index] = self::opacity(state);
                        if has_shaped && light_shapes::uses_shape(state) {
                            shaped.insert(index, state);
                        }
                    }
                }
            }
        }
        Self {
            chunk,
            opacity,
            shaped,
        }
    }

    fn blocked(&self, from: usize, to: usize, direction: usize) -> bool {
        if self.shaped.is_empty() {
            return false;
        }
        let (from, to) = (self.shaped.get(&from), self.shaped.get(&to));
        (from.is_some() || to.is_some())
            && light_shapes::occludes(from.copied(), to.copied(), direction)
    }
}

fn propagate(volume: &mut Volume, cells: &Cells<'_>, mut queue: VecDeque<(usize, usize, usize)>) {
    while let Some((x, y, z)) = queue.pop_front() {
        let level = volume.levels[Volume::index(x, y, z)];
        if level <= 1 {
            continue;
        }
        let from = Volume::index(x, y, z);
        let mut visit = |nx: usize, ny: usize, nz: usize, direction: usize| {
            let index = Volume::index(nx, ny, nz);
            let cost = cells.opacity[index].max(1);
            if cost >= MAX_LIGHT || cells.blocked(from, index, direction) {
                return;
            }
            let next = level.saturating_sub(cost);
            if next > volume.levels[index] {
                volume.levels[index] = next;
                queue.push_back((nx, ny, nz));
            }
        };
        if x > 0 {
            visit(x - 1, y, z, 0);
        }
        if x < 15 {
            visit(x + 1, y, z, 1);
        }
        if z > 0 {
            visit(x, y, z - 1, 2);
        }
        if z < 15 {
            visit(x, y, z + 1, 3);
        }
        if y > 0 {
            visit(x, y - 1, z, 4);
        }
        if y + 1 < volume.height {
            visit(x, y + 1, z, 5);
        }
    }
}

fn sky_volume(cells: &Cells<'_>, height: usize) -> Volume {
    let mut volume = Volume::new(height);
    for z in 0..16 {
        for x in 0..16 {
            let mut level = MAX_LIGHT;
            for y in (0..height).rev() {
                let cost = cells.opacity[Volume::index(x, y, z)];
                level = level.saturating_sub(cost);
                volume.levels[Volume::index(x, y, z)] = level;
                if level == 0 {
                    break;
                }
            }
        }
    }
    let mut queue = VecDeque::new();
    for y in 0..height {
        for z in 0..16 {
            for x in 0..16 {
                let level = volume.levels[Volume::index(x, y, z)];
                if level < 2 {
                    continue;
                }
                let neighbours_darker = [
                    (x.wrapping_sub(1), z),
                    (x + 1, z),
                    (x, z.wrapping_sub(1)),
                    (x, z + 1),
                ]
                .iter()
                .any(|(nx, nz)| {
                    *nx < 16 && *nz < 16 && volume.levels[Volume::index(*nx, y, *nz)] + 1 < level
                });
                let below_darker = y > 0 && volume.levels[Volume::index(x, y - 1, z)] + 1 < level;
                if neighbours_darker || below_darker {
                    queue.push_back((x, y, z));
                }
            }
        }
    }
    propagate(&mut volume, cells, queue);
    volume
}

fn block_volume(cells: &Cells<'_>, height: usize) -> Volume {
    let mut volume = Volume::new(height);
    let mut queue = VecDeque::new();
    for (section_index, section) in cells.chunk.sections().iter().enumerate() {
        if section.is_empty() {
            continue;
        }
        let emitting: Vec<bool> = section
            .blocks()
            .palette()
            .iter()
            .map(|s| emission(*s) > 0)
            .collect();
        if !emitting.iter().any(|e| *e) {
            continue;
        }
        for y in 0..16 {
            for z in 0..16 {
                for x in 0..16 {
                    let state = section.block(x, y, z);
                    let level = emission(state);
                    if level > 0 {
                        let wy = section_index * 16 + y;
                        volume.levels[Volume::index(x, wy, z)] = level;
                        queue.push_back((x, wy, z));
                    }
                }
            }
        }
    }
    propagate(&mut volume, cells, queue);
    volume
}

fn to_sections(
    volume: &Volume,
    sections: usize,
    above: Option<LightArray>,
) -> Vec<Option<LightArray>> {
    let mut out = Vec::with_capacity(sections + 2);
    out.push(None);
    for section in 0..sections {
        let base = section * 16;
        let slice = &volume.levels[Volume::index(0, base, 0)..Volume::index(0, base + 16, 0)];
        if slice.iter().all(|l| *l == 0) {
            out.push(None);
            continue;
        }
        let mut array = LightArray::filled(0);
        for y in 0..16 {
            for z in 0..16 {
                for x in 0..16 {
                    array.set(x, y, z, volume.levels[Volume::index(x, base + y, z)]);
                }
            }
        }
        out.push(Some(array));
    }
    out.push(above);
    out
}

pub fn compute(chunk: &mut ChunkColumn, has_skylight: bool) {
    let height = chunk.height().height as usize;
    let sections = chunk.height().section_count();
    let (sky, block) = {
        let cells = Cells::new(chunk);
        let sky = has_skylight.then(|| {
            to_sections(
                &sky_volume(&cells, height),
                sections,
                Some(LightArray::filled(MAX_LIGHT)),
            )
        });
        let block = to_sections(&block_volume(&cells, height), sections, None);
        (sky, block)
    };
    chunk.light.sky = sky.unwrap_or_else(|| vec![None; sections + 2]);
    chunk.light.block = block;
}

pub trait LightWorld {
    fn light_chunk(&self, pos: ChunkPos) -> Option<&ChunkColumn>;
    fn light_chunk_mut(&mut self, pos: ChunkPos) -> Option<&mut ChunkColumn>;
}

fn light_index(chunk: &ChunkColumn, y: i32) -> Option<usize> {
    let index = (y >> 4) - chunk.height().min_section() + 1;
    let sections = chunk.height().section_count() as i32;
    (1..=sections).contains(&index).then_some(index as usize)
}

fn read_light(chunk: &ChunkColumn, pos: BlockPos, sky: bool) -> u8 {
    let layers = if sky {
        &chunk.light.sky
    } else {
        &chunk.light.block
    };
    light_index(chunk, pos.y)
        .and_then(|index| layers.get(index))
        .and_then(Option::as_ref)
        .map_or(0, |layer| {
            layer.get(
                (pos.x & 15) as usize,
                (pos.y & 15) as usize,
                (pos.z & 15) as usize,
            )
        })
}

fn write_light(chunk: &mut ChunkColumn, pos: BlockPos, sky: bool, level: u8) {
    let Some(index) = light_index(chunk, pos.y) else {
        return;
    };
    let layers = if sky {
        &mut chunk.light.sky
    } else {
        &mut chunk.light.block
    };
    if index >= layers.len() {
        return;
    }
    layers[index]
        .get_or_insert_with(|| LightArray::filled(0))
        .set(
            (pos.x & 15) as usize,
            (pos.y & 15) as usize,
            (pos.z & 15) as usize,
            level,
        );
}

fn step_cost(from: Option<BlockStateId>, to: Option<BlockStateId>, direction: usize) -> Option<u8> {
    let cost = to.map_or(1, |state| opacity(state).max(1));
    if cost >= MAX_LIGHT || light_shapes::occludes(from, to, direction) {
        return None;
    }
    Some(cost)
}

fn spread<W: LightWorld>(
    world: &mut W,
    mut queue: VecDeque<BlockPos>,
    sky: bool,
    changed: &mut Vec<ChunkPos>,
) {
    while let Some(pos) = queue.pop_front() {
        let Some(chunk) = world.light_chunk(pos.chunk()) else {
            continue;
        };
        let level = read_light(chunk, pos, sky);
        if level <= 1 {
            continue;
        }
        let from = chunk.block(pos);
        for (direction, (dx, dy, dz)) in DIRECTIONS.iter().enumerate() {
            let next = BlockPos::new(pos.x + dx, pos.y + dy, pos.z + dz);
            let Some(target) = world.light_chunk(next.chunk()) else {
                continue;
            };
            let Some(state) = target.block(next) else {
                continue;
            };
            let Some(cost) = step_cost(from, Some(state), direction) else {
                continue;
            };
            let raised = level - cost.min(level);
            if raised <= read_light(target, next, sky) {
                continue;
            }
            let Some(target) = world.light_chunk_mut(next.chunk()) else {
                continue;
            };
            write_light(target, next, sky, raised);
            if !changed.contains(&next.chunk()) {
                changed.push(next.chunk());
            }
            queue.push_back(next);
        }
    }
}

fn border_seeds<W: LightWorld>(
    world: &W,
    pos: ChunkPos,
    sky: bool,
    seeds: &mut VecDeque<BlockPos>,
) {
    let Some(chunk) = world.light_chunk(pos) else {
        return;
    };
    let height = chunk.height();
    for (direction, (dx, _, dz)) in DIRECTIONS.iter().take(4).enumerate() {
        let neighbour = ChunkPos::new(pos.x + dx, pos.z + dz);
        let Some(other) = world.light_chunk(neighbour) else {
            continue;
        };
        for edge in 0..16 {
            let (inside, outside) = match direction {
                0 => ((0, edge), (15, edge)),
                1 => ((15, edge), (0, edge)),
                2 => ((edge, 0), (edge, 15)),
                _ => ((edge, 15), (edge, 0)),
            };
            for y in height.min_y..height.min_y + height.height as i32 {
                let here = BlockPos::new(
                    pos.min_block_x() + inside.0,
                    y,
                    pos.min_block_z() + inside.1,
                );
                let there = BlockPos::new(
                    neighbour.min_block_x() + outside.0,
                    y,
                    neighbour.min_block_z() + outside.1,
                );
                let (mine, theirs) = (read_light(chunk, here, sky), read_light(other, there, sky));
                if mine == theirs && (mine == 0 || mine == MAX_LIGHT) {
                    continue;
                }
                if mine > theirs + 1 {
                    seeds.push_back(here);
                } else if theirs > mine + 1 {
                    seeds.push_back(there);
                }
            }
        }
    }
}

pub fn stitch<W: LightWorld>(world: &mut W, pos: ChunkPos, has_skylight: bool) -> Vec<ChunkPos> {
    let mut changed = Vec::new();
    for sky in [false, true] {
        if sky && !has_skylight {
            continue;
        }
        let mut seeds = VecDeque::new();
        border_seeds(world, pos, sky, &mut seeds);
        spread(world, seeds, sky, &mut changed);
    }
    changed
}

#[cfg(test)]
mod tests {
    use rspigot_blocks::Block;
    use rspigot_domain::chunk::ChunkColumn;
    use rspigot_domain::world::WorldHeight;
    use rspigot_domain::{BiomeId, BlockPos, BlockStateId, ChunkPos};

    use super::*;

    fn stone() -> BlockStateId {
        BlockStateId(1)
    }

    #[test]
    fn open_sky_is_full_above_ground_and_dark_below() {
        let mut chunk = ChunkColumn::empty(ChunkPos::new(0, 0), WorldHeight::OVERWORLD, BiomeId(0));
        for x in 0..16 {
            for z in 0..16 {
                for y in -64..64 {
                    chunk.set_block(BlockPos::new(x, y, z), stone());
                }
            }
        }
        compute(&mut chunk, true);
        let above = chunk.light.sky[1 + 8].as_ref().map(|a| a.get(0, 0, 0));
        assert_eq!(above, Some(15));
        assert!(chunk.light.sky[1].is_none());
        assert_eq!(chunk.light.sky.len(), 26);
    }

    fn daisy_chunk(roof: bool) -> ChunkColumn {
        let mut chunk = ChunkColumn::empty(ChunkPos::new(0, 0), WorldHeight::OVERWORLD, BiomeId(0));
        for x in 0..16 {
            for z in 0..16 {
                chunk.set_block(BlockPos::new(x, 63, z), stone());
            }
        }
        chunk.set_block(
            BlockPos::new(4, 64, 4),
            rspigot_blocks::OxeyeDaisy::DEFAULT_STATE,
        );
        if roof {
            chunk.set_block(BlockPos::new(4, 70, 4), stone());
        }
        compute(&mut chunk, true);
        chunk
    }

    fn darken(chunk: &mut ChunkColumn, pos: BlockPos) {
        let index = ((pos.y >> 4) - chunk.height().min_section() + 1) as usize;
        if let Some(layer) = chunk.light.sky[index].as_mut() {
            layer.set(
                (pos.x & 15) as usize,
                (pos.y & 15) as usize,
                (pos.z & 15) as usize,
                0,
            );
        }
    }

    #[test]
    fn a_freshly_computed_chunk_has_consistent_sky_light() {
        assert!(sky_is_consistent(&daisy_chunk(false)));
        assert!(sky_is_consistent(&daisy_chunk(true)));
    }

    #[test]
    fn a_flower_stored_dark_under_open_sky_is_inconsistent() {
        let mut chunk = daisy_chunk(false);
        let daisy = BlockPos::new(4, 64, 4);
        assert_eq!(light_level(&chunk, daisy, true), 15);
        darken(&mut chunk, daisy);
        assert!(!sky_is_consistent(&chunk));
    }

    #[test]
    fn an_air_cell_left_dark_under_open_sky_is_inconsistent() {
        let mut chunk = daisy_chunk(false);
        let air = BlockPos::new(6, 64, 6);
        assert_eq!(light_level(&chunk, air, true), 15);
        darken(&mut chunk, air);
        assert!(!sky_is_consistent(&chunk));
    }

    #[test]
    fn a_flower_under_a_roof_may_be_dark() {
        let mut chunk = daisy_chunk(true);
        darken(&mut chunk, BlockPos::new(4, 64, 4));
        assert!(sky_is_consistent(&chunk));
    }

    #[test]
    fn light_level_reads_the_section_of_the_position() {
        let mut chunk = ChunkColumn::empty(ChunkPos::new(0, 0), WorldHeight::OVERWORLD, BiomeId(0));
        for x in 0..16 {
            for z in 0..16 {
                chunk.set_block(BlockPos::new(x, 63, z), stone());
            }
        }
        chunk.set_block(
            BlockPos::new(4, 64, 4),
            rspigot_blocks::Torch::DEFAULT_STATE,
        );
        compute(&mut chunk, true);
        assert_eq!(light_level(&chunk, BlockPos::new(2, 64, 2), true), 15);
        assert_eq!(light_level(&chunk, BlockPos::new(2, 40, 2), true), 0);
        assert_eq!(light_level(&chunk, BlockPos::new(5, 64, 4), false), 13);
        assert_eq!(light_level(&chunk, BlockPos::new(2, 400, 2), true), 15);
        assert!(affects_light(BlockStateId::AIR, stone()));
        assert!(!affects_light(BlockStateId::AIR, BlockStateId::AIR));
    }

    #[test]
    fn torch_lights_neighbourhood() {
        let mut chunk = ChunkColumn::empty(ChunkPos::new(0, 0), WorldHeight::OVERWORLD, BiomeId(0));
        let torch = rspigot_blocks::Torch::DEFAULT_STATE;
        chunk.set_block(BlockPos::new(8, 0, 8), torch);
        compute(&mut chunk, false);
        let section = chunk.light.block[1 + 4]
            .as_ref()
            .map(|a| (a.get(8, 0, 8), a.get(9, 0, 8), a.get(12, 0, 8)));
        assert_eq!(section, Some((14, 13, 10)));
    }

    struct Grid(std::collections::HashMap<ChunkPos, ChunkColumn>);

    impl LightWorld for Grid {
        fn light_chunk(&self, pos: ChunkPos) -> Option<&ChunkColumn> {
            self.0.get(&pos)
        }

        fn light_chunk_mut(&mut self, pos: ChunkPos) -> Option<&mut ChunkColumn> {
            self.0.get_mut(&pos)
        }
    }

    fn floor_chunk(pos: ChunkPos) -> ChunkColumn {
        let mut chunk = ChunkColumn::empty(pos, WorldHeight::OVERWORLD, BiomeId(0));
        for x in 0..16 {
            for z in 0..16 {
                chunk.set_block(
                    BlockPos::new(pos.min_block_x() + x, 63, pos.min_block_z() + z),
                    stone(),
                );
            }
        }
        chunk
    }

    fn grid(chunks: Vec<ChunkColumn>, sky: bool) -> Grid {
        let mut grid = Grid(std::collections::HashMap::new());
        for mut chunk in chunks {
            compute(&mut chunk, sky);
            grid.0.insert(chunk.pos(), chunk);
        }
        grid
    }

    fn level(grid: &Grid, pos: BlockPos, sky: bool) -> i32 {
        light_level(grid.0.get(&pos.chunk()).expect("loaded"), pos, sky)
    }

    #[test]
    fn a_torch_at_a_chunk_border_lights_the_neighbour() {
        let (west, east) = (ChunkPos::new(0, 0), ChunkPos::new(1, 0));
        let mut lit = floor_chunk(west);
        lit.set_block(
            BlockPos::new(15, 64, 8),
            rspigot_blocks::Torch::DEFAULT_STATE,
        );
        let mut world = grid(vec![lit, floor_chunk(east)], false);
        assert_eq!(level(&world, BlockPos::new(16, 64, 8), false), 0);
        let changed = stitch(&mut world, east, false);
        assert!(changed.contains(&east));
        assert_eq!(level(&world, BlockPos::new(15, 64, 8), false), 14);
        assert_eq!(level(&world, BlockPos::new(16, 64, 8), false), 13);
        assert_eq!(level(&world, BlockPos::new(20, 64, 8), false), 9);
        assert_eq!(level(&world, BlockPos::new(16, 64, 9), false), 12);
        assert_eq!(level(&world, BlockPos::new(31, 64, 8), false), 0);
    }

    #[test]
    fn stitching_from_the_lit_side_is_the_same_as_from_the_dark_side() {
        let (west, east) = (ChunkPos::new(0, 0), ChunkPos::new(1, 0));
        let mut lit = floor_chunk(west);
        lit.set_block(
            BlockPos::new(15, 64, 8),
            rspigot_blocks::Torch::DEFAULT_STATE,
        );
        let mut world = grid(vec![lit, floor_chunk(east)], false);
        stitch(&mut world, west, false);
        assert_eq!(level(&world, BlockPos::new(16, 64, 8), false), 13);
        assert!(stitch(&mut world, east, false).is_empty());
    }

    #[test]
    fn sky_light_flows_sideways_under_an_overhang_across_the_border() {
        let (open, roofed) = (ChunkPos::new(0, 0), ChunkPos::new(1, 0));
        let mut covered = floor_chunk(roofed);
        for x in 0..16 {
            for z in 0..16 {
                covered.set_block(BlockPos::new(16 + x, 66, z), stone());
            }
        }
        let mut world = grid(vec![floor_chunk(open), covered], true);
        assert_eq!(level(&world, BlockPos::new(16, 64, 8), true), 0);
        stitch(&mut world, open, true);
        assert_eq!(level(&world, BlockPos::new(16, 64, 8), true), 14);
        assert_eq!(level(&world, BlockPos::new(18, 64, 8), true), 12);
        assert_eq!(level(&world, BlockPos::new(15, 64, 8), true), 15);
    }

    #[test]
    fn a_bottom_slab_against_a_top_slab_blocks_light_inside_a_chunk() {
        let pos = ChunkPos::new(0, 0);
        let mut chunk = floor_chunk(pos);
        let slab = |kind: &str| {
            rspigot_blocks::BlockKind::OakSlab
                .state_with(&[("type", kind), ("waterlogged", "false")])
                .expect("a slab")
        };
        chunk.set_block(
            BlockPos::new(4, 64, 4),
            rspigot_blocks::Torch::DEFAULT_STATE,
        );
        chunk.set_block(BlockPos::new(5, 64, 4), slab("bottom"));
        chunk.set_block(BlockPos::new(6, 64, 4), slab("top"));
        compute(&mut chunk, false);
        assert_eq!(light_level(&chunk, BlockPos::new(5, 64, 4), false), 13);
        assert_eq!(light_level(&chunk, BlockPos::new(6, 64, 4), false), 10);
        assert_eq!(light_level(&chunk, BlockPos::new(7, 64, 4), false), 9);
    }

    #[test]
    fn two_bottom_slabs_do_not_block_light() {
        let pos = ChunkPos::new(0, 0);
        let mut chunk = floor_chunk(pos);
        let bottom = rspigot_blocks::BlockKind::OakSlab
            .state_with(&[("type", "bottom"), ("waterlogged", "false")])
            .expect("a slab");
        chunk.set_block(
            BlockPos::new(4, 64, 4),
            rspigot_blocks::Torch::DEFAULT_STATE,
        );
        chunk.set_block(BlockPos::new(5, 64, 4), bottom);
        chunk.set_block(BlockPos::new(6, 64, 4), bottom);
        compute(&mut chunk, false);
        assert_eq!(light_level(&chunk, BlockPos::new(6, 64, 4), false), 12);
    }
}

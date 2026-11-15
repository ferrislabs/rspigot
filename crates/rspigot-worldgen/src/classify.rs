use rspigot_data::{registries, tags};
use rspigot_domain::chunk::{Heightmap, HeightmapKind, Heightmaps};
use rspigot_domain::{BlockStateId, world::WorldHeight};

pub(crate) const WORLD_SURFACE: u8 = 1;
pub(crate) const OCEAN_FLOOR: u8 = 2;
pub(crate) const MOTION_BLOCKING: u8 = 4;
pub(crate) const MOTION_BLOCKING_NO_LEAVES: u8 = 8;
pub(crate) const FLUID: u8 = 16;
pub(crate) const LEAVES: u8 = 32;

const HEIGHTMAP_BITS: u8 =
    WORLD_SURFACE | OCEAN_FLOOR | MOTION_BLOCKING | MOTION_BLOCKING_NO_LEAVES;

#[derive(Debug, Clone)]
pub(crate) struct Classifier {
    flags: Box<[u8]>,
}

fn tag_ids(tag: &str) -> &'static [u32] {
    tags::TAGS
        .iter()
        .find(|registry| registry.registry == "minecraft:block")
        .and_then(|registry| registry.tags.iter().find(|(name, _)| *name == tag))
        .map_or(&[], |(_, ids)| *ids)
}

impl Classifier {
    pub(crate) fn new() -> Self {
        let names = registries::static_entries("minecraft:block").unwrap_or(&[]);
        let mut per_block =
            vec![
                WORLD_SURFACE | OCEAN_FLOOR | MOTION_BLOCKING | MOTION_BLOCKING_NO_LEAVES;
                names.len()
            ];
        for tag in [
            "minecraft:replaceable",
            "minecraft:flowers",
            "minecraft:saplings",
            "minecraft:crops",
        ] {
            for id in tag_ids(tag) {
                if let Some(flags) = per_block.get_mut(*id as usize) {
                    *flags = WORLD_SURFACE;
                }
            }
        }
        for id in tag_ids("minecraft:leaves") {
            if let Some(flags) = per_block.get_mut(*id as usize) {
                *flags = WORLD_SURFACE | OCEAN_FLOOR | MOTION_BLOCKING | LEAVES;
            }
        }
        for (id, name) in names.iter().enumerate() {
            per_block[id] = match *name {
                "minecraft:air" | "minecraft:void_air" | "minecraft:cave_air" => 0,
                "minecraft:water" | "minecraft:lava" => {
                    WORLD_SURFACE | MOTION_BLOCKING | MOTION_BLOCKING_NO_LEAVES | FLUID
                }
                "minecraft:snow" => {
                    WORLD_SURFACE | OCEAN_FLOOR | MOTION_BLOCKING | MOTION_BLOCKING_NO_LEAVES
                }
                _ => per_block[id],
            };
        }
        let flags = (0..rspigot_blocks::STATE_COUNT)
            .map(|state| {
                let Ok(state) = u16::try_from(state) else {
                    return 0;
                };
                let id = BlockStateId(state);
                let base = id
                    .kind()
                    .and_then(|kind| per_block.get(usize::from(kind.id().0)))
                    .copied()
                    .unwrap_or(0);
                let full =
                    WORLD_SURFACE | OCEAN_FLOOR | MOTION_BLOCKING | MOTION_BLOCKING_NO_LEAVES;
                if base == full
                    && id.collision_shape().is_empty()
                    && id.kind().map(rspigot_blocks::BlockKind::name) != Some("minecraft:snow")
                {
                    WORLD_SURFACE
                } else {
                    base
                }
            })
            .collect();
        Self { flags }
    }

    pub(crate) fn flags(&self, state: BlockStateId) -> u8 {
        self.flags.get(usize::from(state.0)).copied().unwrap_or(0)
    }

    pub(crate) fn blocks_motion(&self, state: BlockStateId) -> bool {
        self.flags(state) & OCEAN_FLOOR != 0
    }

    #[cfg(test)]
    pub(crate) fn is_fluid(&self, state: BlockStateId) -> bool {
        self.flags(state) & FLUID != 0
    }

    pub(crate) fn heightmaps(
        &self,
        blocks: &[u16],
        uniform: &[Option<u16>],
        height: WorldHeight,
    ) -> Heightmaps {
        let mut maps: [Heightmap; 4] = std::array::from_fn(|_| Heightmap::default());
        for z in 0..16 {
            for x in 0..16 {
                let found = self.column(blocks, uniform, height, x, z);
                for (map, value) in maps.iter_mut().zip(found) {
                    map.set(x, z, value);
                }
            }
        }
        let [surface, floor, motion, no_leaves] = maps;
        assemble(surface, floor, motion, no_leaves)
    }

    pub(crate) fn heightmaps_1d(&self, column: &[u16]) -> Heightmaps {
        let mut found = [0u16; 4];
        let mut remaining = HEIGHTMAP_BITS;
        for (y, state) in column.iter().enumerate().rev() {
            if remaining == 0 {
                break;
            }
            apply(
                self.flags(BlockStateId(*state)),
                (y + 1) as u16,
                &mut found,
                &mut remaining,
            );
        }
        let make = |value: u16| {
            let mut map = Heightmap::default();
            for z in 0..16 {
                for x in 0..16 {
                    map.set(x, z, value);
                }
            }
            map
        };
        assemble(
            make(found[0]),
            make(found[1]),
            make(found[2]),
            make(found[3]),
        )
    }

    fn column(
        &self,
        blocks: &[u16],
        uniform: &[Option<u16>],
        height: WorldHeight,
        x: usize,
        z: usize,
    ) -> [u16; 4] {
        let mut found = [0u16; 4];
        let mut remaining = HEIGHTMAP_BITS;
        let mut top = height.height as usize;
        while top > 0 && remaining != 0 {
            let section = (top - 1) >> 4;
            let bottom = section << 4;
            if let Some(Some(state)) = uniform.get(section) {
                apply(
                    self.flags(BlockStateId(*state)),
                    top as u16,
                    &mut found,
                    &mut remaining,
                );
            } else {
                for y in (bottom..top).rev() {
                    let state = blocks[(y << 8) | (z << 4) | x];
                    apply(
                        self.flags(BlockStateId(state)),
                        (y + 1) as u16,
                        &mut found,
                        &mut remaining,
                    );
                    if remaining == 0 {
                        break;
                    }
                }
            }
            top = bottom;
        }
        found
    }
}

fn apply(flags: u8, value: u16, found: &mut [u16; 4], remaining: &mut u8) {
    let hit = flags & *remaining & HEIGHTMAP_BITS;
    if hit == 0 {
        return;
    }
    for (bit, slot) in found.iter_mut().enumerate() {
        if hit & (1 << bit) != 0 {
            *slot = value;
        }
    }
    *remaining &= !hit;
}

fn assemble(
    surface: Heightmap,
    floor: Heightmap,
    motion: Heightmap,
    no_leaves: Heightmap,
) -> Heightmaps {
    let mut maps = Heightmaps::default();
    for (kind, source) in [
        (HeightmapKind::WorldSurfaceWg, &surface),
        (HeightmapKind::WorldSurface, &surface),
        (HeightmapKind::OceanFloorWg, &floor),
        (HeightmapKind::OceanFloor, &floor),
        (HeightmapKind::MotionBlocking, &motion),
        (HeightmapKind::MotionBlockingNoLeaves, &no_leaves),
    ] {
        *maps.get_or_insert(kind) = source.clone();
    }
    maps
}

#[cfg(test)]
mod tests {
    use super::*;
    use rspigot_blocks::BlockKind;

    fn state(name: &str) -> BlockStateId {
        BlockKind::from_name(name).unwrap().default_state()
    }

    #[test]
    fn classifies_common_blocks() {
        let c = Classifier::new();
        assert_eq!(c.flags(state("minecraft:air")), 0);
        assert_eq!(c.flags(state("minecraft:cave_air")), 0);
        assert_eq!(c.flags(state("minecraft:stone")), HEIGHTMAP_BITS);
        assert!(c.is_fluid(state("minecraft:water")));
        assert!(c.is_fluid(state("minecraft:lava")));
        assert!(!c.blocks_motion(state("minecraft:water")));
        assert!(!c.blocks_motion(state("minecraft:short_grass")));
        assert!(!c.blocks_motion(state("minecraft:dandelion")));
        assert_eq!(c.flags(state("minecraft:short_grass")), WORLD_SURFACE);
        assert!(c.blocks_motion(state("minecraft:snow")));
        assert!(c.blocks_motion(state("minecraft:oak_log")));
    }

    #[test]
    fn leaves_are_excluded_from_no_leaves_map() {
        let c = Classifier::new();
        let leaves = c.flags(state("minecraft:oak_leaves"));
        assert_ne!(leaves & LEAVES, 0);
        assert_ne!(leaves & MOTION_BLOCKING, 0);
        assert_eq!(leaves & MOTION_BLOCKING_NO_LEAVES, 0);
    }

    #[test]
    fn column_heights_follow_vanilla_rules() {
        let c = Classifier::new();
        let stone = state("minecraft:stone").0;
        let water = state("minecraft:water").0;
        let grass = state("minecraft:short_grass").0;
        let leaves = state("minecraft:oak_leaves").0;
        let mut column = vec![0u16; 32];
        column[..4].fill(stone);
        column[4..8].fill(water);
        column[8] = grass;
        column[9] = leaves;
        let maps = c.heightmaps_1d(&column);
        let get = |kind| maps.get(kind).unwrap().get(0, 0);
        assert_eq!(get(HeightmapKind::WorldSurface), 10);
        assert_eq!(get(HeightmapKind::WorldSurfaceWg), 10);
        assert_eq!(get(HeightmapKind::MotionBlocking), 10);
        assert_eq!(get(HeightmapKind::MotionBlockingNoLeaves), 8);
        assert_eq!(get(HeightmapKind::OceanFloor), 10);
        assert_eq!(get(HeightmapKind::OceanFloorWg), 10);
    }

    #[test]
    fn ocean_floor_ignores_water() {
        let c = Classifier::new();
        let stone = state("minecraft:stone").0;
        let water = state("minecraft:water").0;
        let mut column = vec![0u16; 32];
        column[..4].fill(stone);
        column[4..8].fill(water);
        let maps = c.heightmaps_1d(&column);
        let get = |kind| maps.get(kind).unwrap().get(3, 3);
        assert_eq!(get(HeightmapKind::OceanFloor), 4);
        assert_eq!(get(HeightmapKind::MotionBlocking), 8);
        assert_eq!(get(HeightmapKind::WorldSurface), 8);
    }

    #[test]
    fn empty_column_has_zero_height() {
        let c = Classifier::new();
        let maps = c.heightmaps_1d(&[0u16; 16]);
        for kind in HeightmapKind::ALL {
            assert_eq!(maps.get(kind).unwrap().get(5, 5), 0);
        }
    }

    #[test]
    fn buffer_scan_matches_column_scan() {
        let c = Classifier::new();
        let height = WorldHeight::new(0, 64).unwrap();
        let stone = state("minecraft:stone").0;
        let mut blocks = vec![0u16; 64 * 256];
        for y in 0..20 {
            blocks[y * 256 + 7 * 16 + 9] = stone;
        }
        let uniform = [None, None, Some(0), Some(0)];
        let maps = c.heightmaps(&blocks, &uniform, height);
        assert_eq!(
            maps.get(HeightmapKind::MotionBlocking).unwrap().get(9, 7),
            20
        );
        assert_eq!(
            maps.get(HeightmapKind::MotionBlocking).unwrap().get(0, 0),
            0
        );
        let solid = [Some(stone); 4];
        let full = vec![stone; 64 * 256];
        let maps = c.heightmaps(&full, &solid, height);
        assert_eq!(maps.get(HeightmapKind::OceanFloor).unwrap().get(4, 4), 64);
    }
}

use std::collections::BTreeSet;

use super::settings::{ArenaSettings, BlockPoint, Platform, Point};
use crate::domain::TeamId;

const CHUNK_SIZE: i32 = 16;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlacedBlock {
    pub x: i32,
    pub y: i32,
    pub z: i32,
    pub block: String,
}

#[derive(Debug, Clone)]
pub struct Layout {
    settings: ArenaSettings,
}

impl Layout {
    pub const fn new(settings: ArenaSettings) -> Self {
        Self { settings }
    }

    pub const fn settings(&self) -> &ArenaSettings {
        &self.settings
    }

    pub fn blocks(&self) -> Vec<PlacedBlock> {
        if self.settings.map.is_some() {
            return Vec::new();
        }
        platform_blocks(&self.settings.platform)
    }

    pub fn blocks_in_chunk(&self, chunk_x: i32, chunk_z: i32) -> Vec<PlacedBlock> {
        self.blocks()
            .into_iter()
            .filter(|block| chunk_of(block) == (chunk_x, chunk_z))
            .collect()
    }

    pub fn chunks(&self) -> BTreeSet<(i32, i32)> {
        self.blocks().iter().map(chunk_of).collect()
    }

    pub const fn spawn_of(&self, team: TeamId) -> Point {
        self.settings.site(team).spawn
    }

    pub const fn dispenser_of(&self, team: TeamId) -> BlockPoint {
        self.settings.site(team).dispenser
    }

    pub const fn lobby(&self) -> Point {
        self.settings.lobby
    }

    pub fn is_void_fall(&self, y: f64) -> bool {
        y < self.settings.void_fall_y
    }
}

fn chunk_of(block: &PlacedBlock) -> (i32, i32) {
    (
        block.x.div_euclid(CHUNK_SIZE),
        block.z.div_euclid(CHUNK_SIZE),
    )
}

fn platform_blocks(platform: &Platform) -> Vec<PlacedBlock> {
    let min_x = platform.min_x();
    let min_z = platform.min_z();
    let min_y = platform.min_y();
    let mut blocks = Vec::new();
    for y in min_y..=platform.center.y {
        for x in min_x..min_x + platform.width {
            for z in min_z..min_z + platform.depth {
                blocks.push(PlacedBlock {
                    x,
                    y,
                    z,
                    block: platform.block.clone(),
                });
            }
        }
    }
    blocks
}

#[cfg(test)]
mod tests {
    use super::*;

    fn layout() -> Layout {
        Layout::new(ArenaSettings::fallback_platform())
    }

    fn standing(layout: &Layout, point: Point) -> bool {
        let x = point.x.floor() as i32;
        let y = point.y.floor() as i32 - 1;
        let z = point.z.floor() as i32;
        layout
            .blocks()
            .iter()
            .any(|placed| (placed.x, placed.y, placed.z) == (x, y, z))
    }

    fn key(block: &PlacedBlock) -> (i32, i32, i32) {
        (block.x, block.y, block.z)
    }

    #[test]
    fn there_is_one_platform_and_nothing_else() {
        let layout = layout();
        let platform = &layout.settings().platform;
        let volume = (platform.width * platform.depth * platform.thickness) as usize;
        assert_eq!(layout.blocks().len(), volume);
        assert!(layout.blocks().iter().all(|b| b.block == platform.block));
    }

    #[test]
    fn blocks_have_no_duplicate() {
        let blocks = layout().blocks();
        let unique: BTreeSet<_> = blocks.iter().map(key).collect();
        assert_eq!(unique.len(), blocks.len());
    }

    #[test]
    fn the_lobby_stands_on_the_platform() {
        let layout = layout();
        assert!(standing(&layout, layout.lobby()));
    }

    #[test]
    fn spawns_stand_on_the_platform() {
        let layout = layout();
        for team in TeamId::ALL {
            assert!(standing(&layout, layout.spawn_of(team)));
        }
    }

    #[test]
    fn spawns_are_distinct() {
        let layout = layout();
        assert_ne!(layout.spawn_of(TeamId::Red), layout.spawn_of(TeamId::Blue));
    }

    #[test]
    fn spawns_are_symmetric() {
        let layout = layout();
        let red = layout.spawn_of(TeamId::Red);
        let blue = layout.spawn_of(TeamId::Blue);
        assert_eq!(red.x, blue.x);
        assert_eq!(red.y, blue.y);
        assert_eq!(red.z, -blue.z);
    }

    #[test]
    fn red_is_on_the_negative_end_and_blue_on_the_positive_end() {
        let layout = layout();
        assert!(layout.spawn_of(TeamId::Red).z < layout.lobby().z);
        assert!(layout.spawn_of(TeamId::Blue).z > layout.lobby().z);
    }

    #[test]
    fn dispensers_are_symmetric_distinct_and_in_front_of_their_spawn() {
        let layout = layout();
        let red = layout.dispenser_of(TeamId::Red);
        let blue = layout.dispenser_of(TeamId::Blue);
        assert_ne!(red, blue);
        assert_eq!((red.x, red.y), (blue.x, blue.y));
        assert_eq!(red.z, -1 - blue.z);
        assert!(f64::from(red.z) > layout.spawn_of(TeamId::Red).z);
        assert!(f64::from(blue.z) < layout.spawn_of(TeamId::Blue).z);
    }

    #[test]
    fn dispensers_sit_on_the_platform() {
        let layout = layout();
        for team in TeamId::ALL {
            let dispenser = layout.dispenser_of(team);
            let point = Point {
                x: f64::from(dispenser.x),
                y: f64::from(dispenser.y),
                z: f64::from(dispenser.z),
            };
            assert!(standing(&layout, point));
        }
    }

    #[test]
    fn void_fall_boundary() {
        let layout = layout();
        let threshold = layout.settings().void_fall_y;
        assert!(layout.is_void_fall(threshold - 0.01));
        assert!(!layout.is_void_fall(threshold));
        assert!(!layout.is_void_fall(threshold + 0.01));
    }

    #[test]
    fn lobby_is_above_the_void_line() {
        let layout = layout();
        assert!(!layout.is_void_fall(layout.lobby().y));
    }

    fn custom_layout() -> Layout {
        let mut settings = ArenaSettings::fallback_platform();
        settings.platform.center = BlockPoint {
            x: -20,
            y: 64,
            z: -1,
        };
        Layout::new(settings)
    }

    #[test]
    fn chunks_together_equal_blocks_without_loss_or_duplicate() {
        for layout in [layout(), custom_layout()] {
            let mut gathered: Vec<(i32, i32, i32)> = layout
                .chunks()
                .into_iter()
                .flat_map(|(x, z)| layout.blocks_in_chunk(x, z))
                .map(|block| key(&block))
                .collect();
            let mut expected: Vec<(i32, i32, i32)> = layout.blocks().iter().map(key).collect();
            gathered.sort_unstable();
            expected.sort_unstable();
            assert_eq!(gathered, expected);
            let unique: BTreeSet<_> = expected.iter().collect();
            assert_eq!(unique.len(), expected.len());
        }
    }

    #[test]
    fn blocks_land_in_their_own_chunk_negative_coordinates_included() {
        let layout = custom_layout();
        let mut seen_negative = false;
        for (chunk_x, chunk_z) in layout.chunks() {
            for block in layout.blocks_in_chunk(chunk_x, chunk_z) {
                assert_eq!(block.x.div_euclid(CHUNK_SIZE), chunk_x);
                assert_eq!(block.z.div_euclid(CHUNK_SIZE), chunk_z);
                seen_negative |= block.x < 0 || block.z < 0;
            }
        }
        assert!(seen_negative);
    }

    #[test]
    fn a_chunk_without_block_is_empty() {
        let layout = layout();
        assert!(layout.blocks_in_chunk(10_000, 10_000).is_empty());
        assert!(!layout.chunks().contains(&(10_000, 10_000)));
    }

    #[test]
    fn every_listed_chunk_has_blocks() {
        let layout = custom_layout();
        for (x, z) in layout.chunks() {
            assert!(!layout.blocks_in_chunk(x, z).is_empty());
        }
    }

    #[test]
    fn a_map_arena_places_no_generated_block() {
        let layout = Layout::new(ArenaSettings::default());
        assert!(layout.blocks().is_empty());
        assert!(layout.chunks().is_empty());
        assert!(layout.blocks_in_chunk(-3, 0).is_empty());
    }

    #[test]
    fn a_map_arena_keeps_its_spawns_and_lobby() {
        let layout = Layout::new(ArenaSettings::default());
        assert_eq!(layout.spawn_of(TeamId::Red).y, 132.0);
        assert_eq!(layout.lobby().y, 156.0);
        assert!(!layout.is_void_fall(0.0));
        assert!(layout.is_void_fall(-0.5));
    }
}

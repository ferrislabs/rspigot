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
        TeamId::ALL
            .into_iter()
            .flat_map(|team| platform_blocks(&self.settings.site(team).platform))
            .chain(platform_blocks(&self.settings.lobby_platform))
            .collect()
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
    let min_x = platform.center.x - platform.width / 2;
    let min_z = platform.center.z - platform.depth / 2;
    let min_y = platform.center.y - platform.thickness + 1;
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
        Layout::new(ArenaSettings::default())
    }

    fn solid_below(layout: &Layout, point: Point) -> bool {
        let x = point.x.floor() as i32;
        let y = point.y.floor() as i32 - 1;
        let z = point.z.floor() as i32;
        layout
            .blocks()
            .iter()
            .any(|placed| (placed.x, placed.y, placed.z) == (x, y, z))
    }

    #[test]
    fn block_count_matches_both_platforms_and_the_lobby() {
        let layout = layout();
        let volume =
            |platform: &Platform| (platform.width * platform.depth * platform.thickness) as usize;
        let settings = layout.settings();
        let expected = volume(&settings.red.platform)
            + volume(&settings.blue.platform)
            + volume(&settings.lobby_platform);
        assert_eq!(layout.blocks().len(), expected);
    }

    #[test]
    fn the_lobby_stands_on_its_platform() {
        let layout = layout();
        assert!(solid_below(&layout, layout.lobby()));
    }

    #[test]
    fn spawns_are_distinct() {
        let layout = layout();
        assert_ne!(layout.spawn_of(TeamId::Red), layout.spawn_of(TeamId::Blue));
    }

    #[test]
    fn spawns_stand_on_their_own_platform() {
        let layout = layout();
        for team in TeamId::ALL {
            assert!(solid_below(&layout, layout.spawn_of(team)));
        }
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
    fn dispensers_are_symmetric_and_distinct() {
        let layout = layout();
        let red = layout.dispenser_of(TeamId::Red);
        let blue = layout.dispenser_of(TeamId::Blue);
        assert_ne!(red, blue);
        assert_eq!((red.x, red.y), (blue.x, blue.y));
        assert_eq!(red.z, -1 - blue.z);
    }

    #[test]
    fn dispensers_sit_on_their_platform() {
        let layout = layout();
        for team in TeamId::ALL {
            let dispenser = layout.dispenser_of(team);
            let point = Point {
                x: f64::from(dispenser.x),
                y: f64::from(dispenser.y),
                z: f64::from(dispenser.z),
            };
            assert!(solid_below(&layout, point));
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
        let mut settings = ArenaSettings::default();
        settings.lobby_platform.center = BlockPoint {
            x: -20,
            y: 64,
            z: -1,
        };
        Layout::new(settings)
    }

    fn key(block: &PlacedBlock) -> (i32, i32, i32) {
        (block.x, block.y, block.z)
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
}

use super::settings::{ArenaSettings, BlockPoint, Platform, Point};
use crate::domain::TeamId;

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
            .collect()
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
    fn block_count_matches_both_platforms() {
        let layout = layout();
        let platform = &layout.settings().red.platform;
        let each = (platform.width * platform.depth * platform.thickness) as usize;
        assert_eq!(layout.blocks().len(), each * 2);
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
}

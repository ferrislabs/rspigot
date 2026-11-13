use rspigot_domain::{BlockPos, Vec3};

use crate::PathType;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Swimming {
    WaterBound { allow_breaching: bool },
    Amphibious { prefers_shallow: bool },
}

#[derive(Debug, Clone, PartialEq)]
pub struct MobProfile {
    pub width: f32,
    pub height: f32,
    pub step_height: f32,
    pub follow_range: f32,
    pub max_fall_distance: i32,
    pub can_open_doors: bool,
    pub can_pass_doors: bool,
    pub can_float: bool,
    pub can_walk_over_fences: bool,
    pub creaking_home: Option<BlockPos>,
    pub horizontal_step_cost: bool,
    pub flying: bool,
    pub swimming: Option<Swimming>,
    pub frog_nodes: bool,
    pub sea_level: i32,
    malus: [Option<f32>; PathType::COUNT],
}

impl MobProfile {
    pub const DEFAULT_STEP_HEIGHT: f32 = 0.6;
    pub const DEFAULT_FOLLOW_RANGE: f32 = 16.0;
    pub const DEFAULT_MAX_FALL_DISTANCE: i32 = 3;
    pub const DEFAULT_SEA_LEVEL: i32 = 63;
    pub const AMPHIBIOUS_WALKABLE_MALUS: f32 = 6.0;
    pub const AMPHIBIOUS_WATER_BORDER_MALUS: f32 = 4.0;
    pub const SHALLOW_SWIMMING_DEPTH: i32 = 10;

    pub fn new(width: f32, height: f32) -> Self {
        Self {
            width,
            height,
            step_height: Self::DEFAULT_STEP_HEIGHT,
            follow_range: Self::DEFAULT_FOLLOW_RANGE,
            max_fall_distance: Self::DEFAULT_MAX_FALL_DISTANCE,
            can_open_doors: false,
            can_pass_doors: true,
            can_float: false,
            can_walk_over_fences: false,
            creaking_home: None,
            horizontal_step_cost: false,
            flying: false,
            swimming: None,
            frog_nodes: false,
            sea_level: Self::DEFAULT_SEA_LEVEL,
            malus: [None; PathType::COUNT],
        }
    }

    pub fn water_bound(mut self, allow_breaching: bool) -> Self {
        self.swimming = Some(Swimming::WaterBound { allow_breaching });
        self.can_pass_doors = false;
        self
    }

    pub fn amphibious(mut self, prefers_shallow: bool) -> Self {
        self.swimming = Some(Swimming::Amphibious { prefers_shallow });
        self
    }

    pub fn frog_nodes(mut self) -> Self {
        self.frog_nodes = true;
        self
    }

    pub fn is_water_bound(&self) -> bool {
        matches!(self.swimming, Some(Swimming::WaterBound { .. }))
    }

    pub fn is_amphibious(&self) -> bool {
        matches!(self.swimming, Some(Swimming::Amphibious { .. }))
    }

    pub fn allows_breaching(&self) -> bool {
        matches!(
            self.swimming,
            Some(Swimming::WaterBound {
                allow_breaching: true
            })
        )
    }

    pub fn prefers_shallow_swimming(&self) -> bool {
        matches!(
            self.swimming,
            Some(Swimming::Amphibious {
                prefers_shallow: true
            })
        )
    }

    pub fn max_vertical_distance_to_waypoint(&self) -> f64 {
        if self.is_water_bound() { 0.5 } else { 1.0 }
    }

    pub(crate) fn prepared_for_search(&self) -> std::borrow::Cow<'_, Self> {
        if !self.is_amphibious() {
            return std::borrow::Cow::Borrowed(self);
        }
        let mut prepared = self.clone();
        prepared.set_malus(PathType::Water, 0.0);
        prepared.set_malus(PathType::Walkable, Self::AMPHIBIOUS_WALKABLE_MALUS);
        prepared.set_malus(PathType::WaterBorder, Self::AMPHIBIOUS_WATER_BORDER_MALUS);
        std::borrow::Cow::Owned(prepared)
    }

    pub fn flying(mut self, can_float: bool) -> Self {
        self.flying = true;
        self.can_pass_doors = true;
        self.can_open_doors = false;
        self.can_float = can_float;
        self
    }

    pub fn with_malus(mut self, kind: PathType, malus: f32) -> Self {
        self.set_malus(kind, malus);
        self
    }

    pub fn set_malus(&mut self, kind: PathType, malus: f32) {
        self.malus[usize::from(kind.id())] = Some(malus);
    }

    pub fn malus(&self, kind: PathType) -> f32 {
        self.malus[usize::from(kind.id())].unwrap_or(kind.malus())
    }

    pub(crate) fn cells(&self) -> (i32, i32) {
        (cell_count(self.width), cell_count(self.height))
    }

    pub(crate) fn jump_height(&self) -> f64 {
        1.125_f64.max(f64::from(self.step_height))
    }

    pub(crate) fn bounding_box(&self, position: Vec3) -> rspigot_blocks::Aabb {
        let half = f64::from(self.width / 2.0);
        rspigot_blocks::Aabb::new(
            position.x - half,
            position.y,
            position.z - half,
            position.x + half,
            position.y + f64::from(self.height),
            position.z + half,
        )
    }
}

fn cell_count(size: f32) -> i32 {
    (size + 1.0).floor() as i32
}

pub fn combat_fall_distance(health: f32, max_health: f32, difficulty: i32) -> i32 {
    let spare = (health - max_health * 0.33) as i32 - (3 - difficulty) * 4;
    comfortable_fall_distance(spare.max(0) as f32)
}

fn comfortable_fall_distance(extra: f32) -> i32 {
    (extra + 3.0).floor() as i32
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MobState {
    pub position: Vec3,
    pub on_ground: bool,
    pub in_floatable_fluid: bool,
}

impl MobState {
    pub fn block_pos(&self) -> BlockPos {
        self.position.block()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_follow_vanilla_mob() {
        let zombie = MobProfile::new(0.6, 1.95);
        assert_eq!(zombie.cells(), (1, 2));
        assert_eq!(zombie.jump_height(), 1.125);
        assert!(zombie.can_pass_doors);
        assert!(!zombie.can_open_doors);
        assert_eq!(zombie.malus(PathType::Water), 8.0);
        assert_eq!(MobProfile::new(1.4, 0.9).cells(), (2, 1));
        assert_eq!(MobProfile::new(0.9, 1.4).cells(), (1, 2));
    }

    #[test]
    fn malus_overrides_replace_defaults() {
        let profile = MobProfile::new(0.6, 1.95)
            .with_malus(PathType::Water, -1.0)
            .with_malus(PathType::DamagingInNeighbor, 0.0);
        assert_eq!(profile.malus(PathType::Water), -1.0);
        assert_eq!(profile.malus(PathType::Fire), 16.0);
    }

    #[test]
    fn fall_distance_with_target() {
        assert_eq!(comfortable_fall_distance(0.0), 3);
        assert_eq!(combat_fall_distance(20.0, 20.0, 3), 16);
        assert_eq!(combat_fall_distance(20.0, 20.0, 1), 8);
        assert_eq!(combat_fall_distance(4.0, 20.0, 2), 3);
    }
}

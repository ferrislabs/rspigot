use rspigot_nbt::Compound;

pub const MAX_SIZE: f64 = 59_999_968.0;
pub const MAX_CENTER: f64 = 29_999_984.0;
pub const ABSOLUTE_MAX_SIZE: i32 = 29_999_984;
const DEFAULT_DAMAGE_PER_BLOCK: f64 = 0.2;
const DEFAULT_SAFE_ZONE: f64 = 5.0;
const DEFAULT_WARNING_BLOCKS: i32 = 5;
const DEFAULT_WARNING_TIME: i32 = 300;

#[derive(Debug, Clone, Copy, PartialEq)]
enum Extent {
    Static(f64),
    Moving {
        from: f64,
        to: f64,
        start: i64,
        end: i64,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BorderStatus {
    Growing,
    Shrinking,
    Stationary,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BorderView {
    pub center_x: f64,
    pub center_z: f64,
    pub old_size: f64,
    pub new_size: f64,
    pub lerp_time: i64,
    pub absolute_max_size: i32,
    pub warning_blocks: i32,
    pub warning_time: i32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct WorldBorder {
    center_x: f64,
    center_z: f64,
    damage_per_block: f64,
    safe_zone: f64,
    warning_blocks: i32,
    warning_time: i32,
    extent: Extent,
}

impl Default for WorldBorder {
    fn default() -> Self {
        Self {
            center_x: 0.0,
            center_z: 0.0,
            damage_per_block: DEFAULT_DAMAGE_PER_BLOCK,
            safe_zone: DEFAULT_SAFE_ZONE,
            warning_blocks: DEFAULT_WARNING_BLOCKS,
            warning_time: DEFAULT_WARNING_TIME,
            extent: Extent::Static(MAX_SIZE),
        }
    }
}

impl WorldBorder {
    pub fn center(&self) -> (f64, f64) {
        (self.center_x, self.center_z)
    }

    pub fn set_center(&mut self, x: f64, z: f64) {
        self.center_x = x;
        self.center_z = z;
    }

    pub fn damage_per_block(&self) -> f64 {
        self.damage_per_block
    }

    pub fn set_damage_per_block(&mut self, damage: f64) {
        self.damage_per_block = damage;
    }

    pub fn safe_zone(&self) -> f64 {
        self.safe_zone
    }

    pub fn set_safe_zone(&mut self, distance: f64) {
        self.safe_zone = distance;
    }

    pub fn warning_blocks(&self) -> i32 {
        self.warning_blocks
    }

    pub fn set_warning_blocks(&mut self, blocks: i32) {
        self.warning_blocks = blocks;
    }

    pub fn warning_time(&self) -> i32 {
        self.warning_time
    }

    pub fn set_warning_time(&mut self, ticks: i32) {
        self.warning_time = ticks;
    }

    pub fn size(&self, now: i64) -> f64 {
        match self.extent {
            Extent::Static(size) => size,
            Extent::Moving {
                from,
                to,
                start,
                end,
            } => {
                let progress = ((now - start) as f64 / (end - start) as f64).clamp(0.0, 1.0);
                from + (to - from) * progress
            }
        }
    }

    pub fn lerp_target(&self) -> f64 {
        match self.extent {
            Extent::Static(size) => size,
            Extent::Moving { to, .. } => to,
        }
    }

    pub fn lerp_time(&self, now: i64) -> i64 {
        match self.extent {
            Extent::Static(_) => 0,
            Extent::Moving { end, .. } => (end - now).max(0),
        }
    }

    pub fn status(&self, now: i64) -> BorderStatus {
        match self.extent {
            Extent::Moving { from, to, end, .. } if now < end => {
                if to > from {
                    BorderStatus::Growing
                } else {
                    BorderStatus::Shrinking
                }
            }
            _ => BorderStatus::Stationary,
        }
    }

    pub fn set_size(&mut self, size: f64) {
        self.extent = Extent::Static(size);
    }

    pub fn lerp_size(&mut self, from: f64, to: f64, now: i64, ticks: i64) {
        self.extent = if from == to {
            Extent::Static(to)
        } else {
            Extent::Moving {
                from,
                to,
                start: now,
                end: now + ticks,
            }
        };
    }

    pub fn tick(&mut self, now: i64) -> bool {
        match self.extent {
            Extent::Moving { to, end, .. } if now >= end => {
                self.extent = Extent::Static(to);
                true
            }
            _ => false,
        }
    }

    fn bounds(&self, now: i64) -> (f64, f64, f64, f64) {
        let half = self.size(now) / 2.0;
        let limit = f64::from(ABSOLUTE_MAX_SIZE);
        (
            (self.center_x - half).max(-limit),
            (self.center_z - half).max(-limit),
            (self.center_x + half).min(limit),
            (self.center_z + half).min(limit),
        )
    }

    pub fn distance_to_border(&self, x: f64, z: f64, now: i64) -> f64 {
        let (min_x, min_z, max_x, max_z) = self.bounds(now);
        (x - min_x).min(max_x - x).min((z - min_z).min(max_z - z))
    }

    pub fn contains(&self, x: f64, z: f64, now: i64) -> bool {
        let (min_x, min_z, max_x, max_z) = self.bounds(now);
        x >= min_x && x < max_x && z >= min_z && z < max_z
    }

    pub fn contains_block(&self, x: i32, z: i32, now: i64) -> bool {
        let (min_x, min_z, max_x, max_z) = self.bounds(now);
        f64::from(x) >= min_x
            && f64::from(x) + 1.0 <= max_x
            && f64::from(z) >= min_z
            && f64::from(z) + 1.0 <= max_z
    }

    pub fn view(&self, now: i64) -> BorderView {
        BorderView {
            center_x: self.center_x,
            center_z: self.center_z,
            old_size: self.size(now),
            new_size: self.lerp_target(),
            lerp_time: self.lerp_time(now),
            absolute_max_size: ABSOLUTE_MAX_SIZE,
            warning_blocks: self.warning_blocks,
            warning_time: self.warning_time,
        }
    }

    pub fn to_nbt(&self, now: i64) -> Compound {
        Compound::new()
            .with("center_x", self.center_x)
            .with("center_z", self.center_z)
            .with("damage_per_block", self.damage_per_block)
            .with("safe_zone", self.safe_zone)
            .with("warning_blocks", self.warning_blocks)
            .with("warning_time", self.warning_time)
            .with("size", self.size(now))
            .with("lerp_time", self.lerp_time(now))
            .with("lerp_target", self.lerp_target())
    }

    pub fn from_nbt(nbt: &Compound, now: i64) -> Self {
        let defaults = Self::default();
        let size = nbt.get_f64("size").unwrap_or(MAX_SIZE);
        let target = nbt.get_f64("lerp_target").unwrap_or(size);
        let remaining = nbt.get_i64("lerp_time").unwrap_or(0);
        let mut border = Self {
            center_x: nbt.get_f64("center_x").unwrap_or(0.0),
            center_z: nbt.get_f64("center_z").unwrap_or(0.0),
            damage_per_block: nbt
                .get_f64("damage_per_block")
                .unwrap_or(defaults.damage_per_block),
            safe_zone: nbt.get_f64("safe_zone").unwrap_or(defaults.safe_zone),
            warning_blocks: nbt
                .get_i32("warning_blocks")
                .unwrap_or(defaults.warning_blocks),
            warning_time: nbt.get_i32("warning_time").unwrap_or(defaults.warning_time),
            extent: Extent::Static(size),
        };
        if remaining > 0 {
            border.lerp_size(size, target, now, remaining);
        }
        border
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_border_is_the_vanilla_one() {
        let border = WorldBorder::default();
        assert_eq!(border.size(0), MAX_SIZE);
        assert_eq!(border.center(), (0.0, 0.0));
        assert_eq!(border.damage_per_block(), 0.2);
        assert_eq!(border.safe_zone(), 5.0);
        assert_eq!(border.warning_blocks(), 5);
        assert_eq!(border.warning_time(), 300);
        assert_eq!(border.status(0), BorderStatus::Stationary);
    }

    #[test]
    fn a_lerp_moves_the_size_with_the_game_time_and_settles_at_the_target() {
        let mut border = WorldBorder::default();
        border.set_size(100.0);
        border.lerp_size(100.0, 200.0, 1_000, 100);
        assert_eq!(border.size(1_000), 100.0);
        assert_eq!(border.size(1_050), 150.0);
        assert_eq!(border.status(1_050), BorderStatus::Growing);
        assert_eq!(border.lerp_time(1_025), 75);
        assert!(!border.tick(1_099));
        assert!(border.tick(1_100));
        assert_eq!(border.size(1_100), 200.0);
        assert_eq!(border.lerp_time(1_100), 0);
        assert_eq!(border.status(1_100), BorderStatus::Stationary);
    }

    #[test]
    fn a_shrinking_lerp_reports_shrinking() {
        let mut border = WorldBorder::default();
        border.set_size(100.0);
        border.lerp_size(100.0, 10.0, 0, 20);
        assert_eq!(border.status(5), BorderStatus::Shrinking);
        assert_eq!(border.lerp_target(), 10.0);
    }

    #[test]
    fn the_distance_to_the_border_is_the_closest_edge_and_negative_outside() {
        let mut border = WorldBorder::default();
        border.set_size(100.0);
        border.set_center(10.0, 0.0);
        assert_eq!(border.distance_to_border(10.0, 0.0, 0), 50.0);
        assert_eq!(border.distance_to_border(55.0, 0.0, 0), 5.0);
        assert_eq!(border.distance_to_border(70.0, 0.0, 0), -10.0);
        assert!(border.contains(59.9, 0.0, 0));
        assert!(!border.contains(60.0, 0.0, 0));
        assert!(border.contains_block(59, 0, 0));
        assert!(!border.contains_block(60, 0, 0));
    }

    #[test]
    fn the_edges_never_pass_the_absolute_maximum() {
        let border = WorldBorder::default();
        assert!(border.contains(29_999_983.0, 0.0, 0));
        assert!(!border.contains(29_999_984.0, 0.0, 0));
    }

    #[test]
    fn the_settings_round_trip_through_nbt_and_resume_a_lerp() {
        let mut border = WorldBorder::default();
        border.set_center(3.5, -4.5);
        border.set_damage_per_block(1.5);
        border.set_safe_zone(2.0);
        border.set_warning_blocks(9);
        border.set_warning_time(40);
        border.set_size(100.0);
        border.lerp_size(100.0, 40.0, 500, 60);
        let saved = border.to_nbt(530);
        assert_eq!(saved.get_f64("size"), Some(70.0));
        assert_eq!(saved.get_i64("lerp_time"), Some(30));
        assert_eq!(saved.get_f64("lerp_target"), Some(40.0));
        let restored = WorldBorder::from_nbt(&saved, 0);
        assert_eq!(restored.center(), (3.5, -4.5));
        assert_eq!(restored.damage_per_block(), 1.5);
        assert_eq!(restored.warning_time(), 40);
        assert_eq!(restored.size(0), 70.0);
        assert_eq!(restored.size(30), 40.0);
    }

    #[test]
    fn the_client_view_carries_the_current_and_target_size() {
        let mut border = WorldBorder::default();
        border.set_size(100.0);
        border.lerp_size(100.0, 200.0, 0, 100);
        let view = border.view(50);
        assert_eq!(
            (view.old_size, view.new_size, view.lerp_time),
            (150.0, 200.0, 50)
        );
        assert_eq!(view.absolute_max_size, ABSOLUTE_MAX_SIZE);
    }
}

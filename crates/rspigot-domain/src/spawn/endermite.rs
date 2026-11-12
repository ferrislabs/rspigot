use rspigot_entities::EntityKind;

use super::SpawnReason;
use super::rules::{RandomLevel, check_mob_spawn_rules};
use crate::{BlockPos, Vec3};

pub const PLAYER_EXCLUSION_RADIUS: f64 = 5.0;

pub fn check_endermite_spawn_rules(
    level: &mut impl RandomLevel,
    kind: EntityKind,
    reason: SpawnReason,
    pos: BlockPos,
) -> bool {
    if !check_mob_spawn_rules(level, kind, reason, pos) {
        return false;
    }
    if reason.is_spawner() {
        return true;
    }
    let center = Vec3::new(
        f64::from(pos.x) + 0.5,
        f64::from(pos.y) + 0.5,
        f64::from(pos.z) + 0.5,
    );
    !level.survival_player_within(center, PLAYER_EXCLUSION_RADIUS)
}

#[cfg(test)]
mod tests {
    use rspigot_blocks::{Block, Stone};

    use super::*;
    use crate::spawn::rules::check_spawn_rules;
    use crate::spawn::testing::Level;

    #[test]
    fn endermites_avoid_players_within_five_blocks() {
        let pos = BlockPos::new(4, 64, 4);
        let mut level = Level::floor(Stone::DEFAULT_STATE, 63);
        assert!(check_spawn_rules(
            &mut level,
            EntityKind::Endermite,
            SpawnReason::Natural,
            pos
        ));
        level.players.push(Vec3::new(4.5, 64.5, 8.4));
        assert!(!check_spawn_rules(
            &mut level,
            EntityKind::Endermite,
            SpawnReason::Natural,
            pos
        ));
        level.players = vec![Vec3::new(4.5, 64.5, 9.6)];
        assert!(check_spawn_rules(
            &mut level,
            EntityKind::Endermite,
            SpawnReason::Natural,
            pos
        ));
    }

    #[test]
    fn spawners_ignore_nearby_players_but_not_the_floor() {
        let pos = BlockPos::new(4, 64, 4);
        let mut level = Level::floor(Stone::DEFAULT_STATE, 63);
        level.players.push(Vec3::new(4.5, 64.5, 4.5));
        assert!(check_spawn_rules(
            &mut level,
            EntityKind::Endermite,
            SpawnReason::Spawner,
            pos
        ));
    }

    #[test]
    fn endermites_ignore_light() {
        let pos = BlockPos::new(4, 64, 4);
        let mut level = Level::floor(Stone::DEFAULT_STATE, 63);
        level.sky = 15;
        assert!(check_spawn_rules(
            &mut level,
            EntityKind::Endermite,
            SpawnReason::Natural,
            pos
        ));
    }
}

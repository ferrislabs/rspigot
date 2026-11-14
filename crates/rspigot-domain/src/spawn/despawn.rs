use rspigot_entities::{EntityKind, MobClass};

use crate::entity::species::raider;
use crate::random::RandomSource;
use crate::world::Difficulty;

pub const NO_ACTION_DESPAWN_TIME: i32 = 600;
pub const RANDOM_DESPAWN_CHANCE: i32 = 800;
const OCELOT_PERSISTENT_TICKS: i32 = 2400;

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct FarAwayFacts {
    pub chicken_jockey: bool,
    pub trusting: bool,
    pub tame: bool,
    pub tick_count: i32,
    pub villager_progress: bool,
    pub patrolling: bool,
    pub distance_sqr: f64,
    pub from_bucket: bool,
    pub named: bool,
}

pub fn remove_when_far_away(kind: EntityKind, facts: FarAwayFacts) -> Option<bool> {
    Some(match kind.mob_spawn()?.remove_when_far_away {
        MobClass::Mob | MobClass::Hoglin => true,
        MobClass::Cat => !facts.tame && facts.tick_count > OCELOT_PERSISTENT_TICKS,
        MobClass::Ocelot => !facts.trusting && facts.tick_count > OCELOT_PERSISTENT_TICKS,
        MobClass::Animal
        | MobClass::AbstractGolem
        | MobClass::Villager
        | MobClass::WanderingTrader
        | MobClass::Allay
        | MobClass::Warden => false,
        MobClass::Chicken => facts.chicken_jockey,
        MobClass::ZombieVillager => !facts.villager_progress,
        MobClass::AbstractFish | MobClass::Axolotl => !(facts.from_bucket || facts.named),
        MobClass::AbstractNautilus | MobClass::ZombieHorse | MobClass::CamelHusk => true,
        MobClass::Raider => {
            !(facts.patrolling && facts.distance_sqr <= raider::PATROL_KEEP_DISTANCE_SQR)
        }
        _ => return None,
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DespawnFacts {
    pub kind: EntityKind,
    pub persistence_required: bool,
    pub requires_custom_persistence: bool,
    pub no_action_time: i32,
    pub chicken_jockey: bool,
    pub trusting: bool,
    pub tame: bool,
    pub tick_count: i32,
    pub villager_progress: bool,
    pub patrolling: bool,
    pub from_bucket: bool,
    pub named: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct DespawnOutcome {
    pub discard: bool,
    pub reset_no_action_time: bool,
}

pub fn check_despawn(
    facts: &DespawnFacts,
    difficulty: Difficulty,
    nearest_player_distance_sqr: Option<f64>,
    random: &mut impl RandomSource,
) -> DespawnOutcome {
    let mut outcome = DespawnOutcome::default();
    let kind = facts.kind;
    if difficulty == Difficulty::Peaceful && !kind.allowed_in_peaceful() {
        outcome.discard = true;
        return outcome;
    }
    if kind == EntityKind::Wither {
        outcome.reset_no_action_time = true;
        return outcome;
    }
    if kind == EntityKind::EnderDragon {
        return outcome;
    }
    let persistent = facts.persistence_required || facts.requires_custom_persistence;
    let Some(distance) = nearest_player_distance_sqr else {
        return outcome;
    };
    let category = kind.category();
    let far_facts = FarAwayFacts {
        chicken_jockey: facts.chicken_jockey,
        trusting: facts.trusting,
        tame: facts.tame,
        tick_count: facts.tick_count,
        villager_progress: facts.villager_progress,
        patrolling: facts.patrolling,
        distance_sqr: distance,
        from_bucket: facts.from_bucket,
        named: facts.named,
    };
    let far = || remove_when_far_away(kind, far_facts).unwrap_or(false);
    let despawn = i32::from(category.despawn_distance()).pow(2);
    if !persistent && distance > f64::from(despawn) && far() {
        outcome.discard = true;
    }
    let no_despawn = i32::from(category.no_despawn_distance()).pow(2);
    if !persistent
        && facts.no_action_time > NO_ACTION_DESPAWN_TIME
        && random.next_int(RANDOM_DESPAWN_CHANCE) == 0
        && distance > f64::from(no_despawn)
        && far()
    {
        outcome.discard = true;
    } else if distance < f64::from(no_despawn) {
        outcome.reset_no_action_time = true;
    }
    outcome
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::random::LegacyRandom;

    struct Fixed(i32);

    impl RandomSource for Fixed {
        fn next_float(&mut self) -> f32 {
            0.0
        }

        fn next_int(&mut self, _: i32) -> i32 {
            self.0
        }

        fn next_double(&mut self) -> f64 {
            0.0
        }

        fn next_boolean(&mut self) -> bool {
            false
        }

        fn next_gaussian(&mut self) -> f64 {
            0.0
        }
    }

    fn facts(kind: EntityKind) -> DespawnFacts {
        DespawnFacts {
            kind,
            persistence_required: false,
            requires_custom_persistence: false,
            no_action_time: 0,
            chicken_jockey: false,
            trusting: false,
            tame: false,
            tick_count: 0,
            villager_progress: false,
            patrolling: false,
            from_bucket: false,
            named: false,
        }
    }

    #[test]
    fn monsters_vanish_beyond_the_despawn_distance() {
        let zombie = facts(EntityKind::Zombie);
        let mut random = LegacyRandom::new(1);
        let far = check_despawn(
            &zombie,
            Difficulty::Normal,
            Some(128.5 * 128.5),
            &mut random,
        );
        assert!(far.discard);
        let edge = check_despawn(
            &zombie,
            Difficulty::Normal,
            Some(128.0 * 128.0),
            &mut random,
        );
        assert!(!edge.discard);
        let none = check_despawn(&zombie, Difficulty::Normal, None, &mut random);
        assert_eq!(none, DespawnOutcome::default());
    }

    #[test]
    fn the_ender_dragon_never_despawns_whatever_the_distance_or_the_difficulty() {
        let mut random = Fixed(0);
        let dragon = DespawnFacts {
            no_action_time: 700,
            ..facts(EntityKind::EnderDragon)
        };
        for difficulty in [Difficulty::Hard, Difficulty::Peaceful] {
            let far = check_despawn(&dragon, difficulty, Some(1e9), &mut random);
            assert!(!far.discard && !far.reset_no_action_time);
            let alone = check_despawn(&dragon, difficulty, None, &mut random);
            assert!(!alone.discard);
        }
    }

    #[test]
    fn the_wither_never_despawns_from_distance_and_resets_its_idle_time_except_in_peaceful() {
        let mut random = Fixed(0);
        let wither = DespawnFacts {
            no_action_time: 700,
            ..facts(EntityKind::Wither)
        };
        let far = check_despawn(&wither, Difficulty::Hard, Some(1e9), &mut random);
        assert!(!far.discard && far.reset_no_action_time);
        let alone = check_despawn(&wither, Difficulty::Hard, None, &mut random);
        assert!(!alone.discard && alone.reset_no_action_time);
        let peaceful = check_despawn(&wither, Difficulty::Peaceful, None, &mut random);
        assert!(peaceful.discard);
    }

    #[test]
    fn animals_and_persistent_mobs_stay() {
        let mut random = Fixed(0);
        let panda = DespawnFacts {
            no_action_time: 700,
            ..facts(EntityKind::Panda)
        };
        assert!(!check_despawn(&panda, Difficulty::Normal, Some(1e6), &mut random).discard);
        let cow = DespawnFacts {
            no_action_time: 700,
            ..facts(EntityKind::Cow)
        };
        assert!(!check_despawn(&cow, Difficulty::Normal, Some(1e6), &mut random).discard);
        let named = DespawnFacts {
            persistence_required: true,
            no_action_time: 700,
            ..facts(EntityKind::Zombie)
        };
        assert!(!check_despawn(&named, Difficulty::Normal, Some(1e6), &mut random).discard);
        let jockey = DespawnFacts {
            chicken_jockey: true,
            ..facts(EntityKind::Chicken)
        };
        assert!(check_despawn(&jockey, Difficulty::Normal, Some(1e6), &mut random).discard);
        assert!(
            !check_despawn(
                &facts(EntityKind::Chicken),
                Difficulty::Normal,
                Some(1e6),
                &mut random
            )
            .discard
        );
    }

    #[test]
    fn idle_monsters_roll_one_in_eight_hundred_beyond_thirty_two_blocks() {
        let idle = DespawnFacts {
            no_action_time: 601,
            ..facts(EntityKind::Skeleton)
        };
        assert!(check_despawn(&idle, Difficulty::Easy, Some(40.0 * 40.0), &mut Fixed(0)).discard);
        assert!(!check_despawn(&idle, Difficulty::Easy, Some(40.0 * 40.0), &mut Fixed(1)).discard);
        let fresh = DespawnFacts {
            no_action_time: 600,
            ..idle
        };
        assert!(!check_despawn(&fresh, Difficulty::Easy, Some(40.0 * 40.0), &mut Fixed(0)).discard);
        let near = check_despawn(&idle, Difficulty::Easy, Some(20.0 * 20.0), &mut Fixed(0));
        assert_eq!(
            near,
            DespawnOutcome {
                discard: false,
                reset_no_action_time: true
            }
        );
    }

    #[test]
    fn young_ocelots_stay_but_old_wild_ones_fade() {
        let mut random = Fixed(5);
        let young = DespawnFacts {
            tick_count: 2400,
            ..facts(EntityKind::Ocelot)
        };
        assert!(!check_despawn(&young, Difficulty::Normal, Some(1e6), &mut random).discard);
        let old = DespawnFacts {
            tick_count: 2401,
            ..facts(EntityKind::Ocelot)
        };
        assert!(check_despawn(&old, Difficulty::Normal, Some(1e6), &mut random).discard);
        let trusting = DespawnFacts {
            trusting: true,
            ..old
        };
        assert!(!check_despawn(&trusting, Difficulty::Normal, Some(1e6), &mut random).discard);
    }

    #[test]
    fn wild_cats_fade_after_two_minutes_but_tame_ones_stay() {
        let mut random = Fixed(5);
        let young = DespawnFacts {
            tick_count: 2400,
            ..facts(EntityKind::Cat)
        };
        assert!(!check_despawn(&young, Difficulty::Normal, Some(1e6), &mut random).discard);
        let old = DespawnFacts {
            tick_count: 2401,
            ..facts(EntityKind::Cat)
        };
        assert!(check_despawn(&old, Difficulty::Normal, Some(1e6), &mut random).discard);
        let tame = DespawnFacts { tame: true, ..old };
        assert!(!check_despawn(&tame, Difficulty::Normal, Some(1e6), &mut random).discard);
    }

    #[test]
    fn zombie_villagers_with_experience_or_a_cure_underway_never_fade() {
        let mut random = Fixed(5);
        let plain = facts(EntityKind::ZombieVillager);
        assert!(check_despawn(&plain, Difficulty::Normal, Some(1e6), &mut random).discard);
        let cured = DespawnFacts {
            villager_progress: true,
            ..plain
        };
        assert!(!check_despawn(&cured, Difficulty::Normal, Some(1e6), &mut random).discard);
    }

    #[test]
    fn witches_vanish_far_away_unless_they_patrol_within_one_hundred_and_twenty_eight_blocks() {
        let mut random = Fixed(5);
        let witch = facts(EntityKind::Witch);
        assert!(check_despawn(&witch, Difficulty::Normal, Some(1e6), &mut random).discard);
        let patrolling = DespawnFacts {
            patrolling: true,
            ..witch
        };
        let near = 128.0 * 128.0;
        assert!(!check_despawn(&patrolling, Difficulty::Normal, Some(near), &mut random).discard);
        assert!(
            check_despawn(
                &patrolling,
                Difficulty::Normal,
                Some(near + 1.0),
                &mut random
            )
            .discard
        );
    }

    #[test]
    fn polar_bears_never_fade_but_zombified_piglins_do() {
        let mut random = Fixed(5);
        let bear = facts(EntityKind::PolarBear);
        assert!(!check_despawn(&bear, Difficulty::Normal, Some(1e6), &mut random).discard);
        let piglin = facts(EntityKind::ZombifiedPiglin);
        assert!(check_despawn(&piglin, Difficulty::Normal, Some(1e6), &mut random).discard);
    }

    #[test]
    fn peaceful_removes_monsters_only() {
        let mut random = Fixed(5);
        assert!(
            check_despawn(
                &facts(EntityKind::Creeper),
                Difficulty::Peaceful,
                None,
                &mut random
            )
            .discard
        );
        assert!(
            !check_despawn(
                &facts(EntityKind::Sheep),
                Difficulty::Peaceful,
                None,
                &mut random
            )
            .discard
        );
    }
}

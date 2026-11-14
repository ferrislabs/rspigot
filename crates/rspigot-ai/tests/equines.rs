use std::cell::Cell;
use std::collections::HashMap;

use rspigot_ai::{
    AiWorld, EquineAction, EquineFacts, Facts, GoalSettings, Living, MobAction, MobAi, MobBody,
    MobGoal, MobView, TargetKind,
};
use rspigot_blocks::{BlockKind, StateId};
use rspigot_domain::entity::living::LivingState;
use rspigot_domain::entity::{Aabb, EntityBase};
use rspigot_domain::{BlockPos, EntityId, Vec3};
use rspigot_entities::EntityKind;
use uuid::Uuid;

struct World {
    players: Vec<Living>,
    others: Vec<Living>,
    blocks: HashMap<BlockPos, StateId>,
    griefing: bool,
    time: Cell<i64>,
}

impl World {
    fn new() -> Self {
        Self {
            players: Vec::new(),
            others: Vec::new(),
            blocks: HashMap::new(),
            griefing: true,
            time: Cell::new(0),
        }
    }
}

impl rspigot_ai::BlockView for World {
    fn state(&self, pos: BlockPos) -> Option<StateId> {
        if let Some(state) = self.blocks.get(&pos) {
            return Some(*state);
        }
        Some(if (60..64).contains(&pos.y) {
            BlockKind::Stone.default_state()
        } else {
            StateId::AIR
        })
    }
}

impl AiWorld for World {
    fn players(&self) -> &[Living] {
        &self.players
    }

    fn living(&self, id: EntityId) -> Option<Living> {
        self.players
            .iter()
            .chain(&self.others)
            .find(|living| living.id == id)
            .copied()
    }

    fn living_of(&self, kind: TargetKind, area: &Aabb) -> Vec<Living> {
        let pool = if kind == TargetKind::Player {
            &self.players
        } else {
            &self.others
        };
        pool.iter()
            .filter(|living| kind.matches(living.kind) && living.bounds.intersects(area))
            .copied()
            .collect()
    }

    fn difficulty(&self) -> i32 {
        2
    }

    fn game_time(&self) -> i64 {
        self.time.get()
    }

    fn raw_brightness(&self, _pos: BlockPos) -> i32 {
        15
    }

    fn mob_griefing(&self) -> bool {
        self.griefing
    }
}

fn living(id: i32, kind: EntityKind, position: Vec3) -> Living {
    let size = kind.dimensions();
    Living {
        id: EntityId(id),
        player: kind == EntityKind::Player,
        kind,
        position,
        eye_height: f64::from(size.eye_height),
        bounds: Aabb::of_size(position, size.width, size.height),
        alive: true,
        spectator: false,
        creative: false,
        invulnerable: false,
        discrete: false,
        invisible: false,
        armor_cover: 0.0,
        main_hand: None,
        off_hand: None,
        yaw: 0.0,
        pitch: 0.0,
        in_water: false,
        baby: false,
        age: 0,
        in_love: false,
        panicking: false,
        has_target: false,
        scared: false,
        facts: Facts::default(),
    }
}

fn player(position: Vec3) -> Living {
    living(100, EntityKind::Player, position)
}

fn mob(kind: EntityKind, position: Vec3) -> (EntityBase, LivingState) {
    let mut base = EntityBase::new(EntityId(1), Uuid::from_u128(1), kind);
    base.position = position;
    base.old_position = position;
    base.on_ground = true;
    base.tick_count = 1;
    let state = LivingState::new(&mut base, 0.0);
    (base, state)
}

fn step(
    ai: &mut MobAi,
    base: &mut EntityBase,
    state: &LivingState,
    world: &World,
    adjust: impl Fn(&mut MobView),
) -> Vec<MobAction> {
    base.tick_count += 1;
    world.time.set(world.time.get() + 1);
    let mut view = MobView::of(base, state);
    adjust(&mut view);
    let mut body = MobBody::of(&view);
    ai.think(&view, world, &mut body);
    ai.take_actions()
}

fn running(ai: &MobAi, matches: impl Fn(&MobGoal) -> bool) -> bool {
    ai.goals()
        .goals()
        .iter()
        .any(|goal| goal.is_running() && matches(goal.goal()))
}

fn summary(ai: &MobAi) -> Vec<(i32, &'static str)> {
    ai.goals()
        .goals()
        .iter()
        .map(|goal| {
            let label = match goal.goal() {
                MobGoal::Float(_) => "float",
                MobGoal::Panic(_) => "panic",
                MobGoal::Breed(_) => "breed",
                MobGoal::Tempt(_) => "tempt",
                MobGoal::FollowParent(_) => "follow_parent",
                MobGoal::Stroll(_) => "stroll",
                MobGoal::LookAtPlayer(_) => "look",
                MobGoal::RandomLookAround(_) => "look_around",
                MobGoal::Equine(rspigot_ai::EquineGoal::RunAround(_)) => "run_around",
                MobGoal::Equine(rspigot_ai::EquineGoal::RandomStand(_)) => "random_stand",
                MobGoal::Equine(rspigot_ai::EquineGoal::SkeletonTrap(_)) => "skeleton_trap",
                _ => "other",
            };
            (goal.priority(), label)
        })
        .collect()
}

const COMMON: [(i32, &str); 7] = [
    (1, "run_around"),
    (2, "breed"),
    (4, "follow_parent"),
    (6, "stroll"),
    (7, "look"),
    (8, "look_around"),
    (9, "random_stand"),
];

fn with_behaviour(extra: &[(i32, &'static str)]) -> Vec<(i32, &'static str)> {
    let mut goals = COMMON.to_vec();
    goals.extend_from_slice(extra);
    goals
}

#[test]
fn horses_donkeys_and_mules_register_the_vanilla_goals() {
    for kind in [EntityKind::Horse, EntityKind::Donkey, EntityKind::Mule] {
        let ai = MobAi::new(kind, 1, GoalSettings::default());
        assert_eq!(
            summary(&ai),
            with_behaviour(&[(0, "float"), (1, "panic"), (3, "tempt")]),
            "{kind:?}"
        );
        assert!(ai.targets().goals().is_empty());
    }
}

#[test]
fn zombie_horses_skip_the_panic_and_skeleton_horses_all_the_behaviour_goals() {
    let zombie = MobAi::new(EntityKind::ZombieHorse, 1, GoalSettings::default());
    assert_eq!(
        summary(&zombie),
        with_behaviour(&[(0, "float"), (3, "tempt")])
    );
    let skeleton = MobAi::new(EntityKind::SkeletonHorse, 1, GoalSettings::default());
    assert_eq!(summary(&skeleton), with_behaviour(&[(1, "skeleton_trap")]));
}

fn ridden(player: Option<i32>) -> impl Fn(&mut MobView) {
    move |view| {
        view.species.equine = EquineFacts {
            temper: 40,
            max_temper: 100,
            vehicle: true,
            has_passenger: true,
            passenger_player: player.map(EntityId),
            ..EquineFacts::default()
        };
    }
}

#[test]
fn an_untamed_ridden_horse_runs_around_and_bucks_or_tames_its_rider() {
    let mut tamed = 0;
    let mut bucked = 0;
    for seed in 0..40 {
        let (mut base, state) = mob(EntityKind::Horse, Vec3::new(0.5, 64.0, 0.5));
        let mut ai = MobAi::new(EntityKind::Horse, seed, GoalSettings::default());
        let world = World::new();
        let mut actions = Vec::new();
        for _ in 0..600 {
            actions.extend(step(&mut ai, &mut base, &state, &world, ridden(Some(100))));
        }
        assert!(
            running(&ai, |goal| matches!(
                goal,
                MobGoal::Equine(rspigot_ai::EquineGoal::RunAround(_))
            )) || !actions.is_empty()
        );
        for action in &actions {
            match action {
                MobAction::Equine(EquineAction::TryTame { player }) => {
                    assert_eq!(*player, EntityId(100));
                    tamed += 1;
                }
                MobAction::Equine(EquineAction::Buck { raise_temper }) => {
                    assert!(raise_temper);
                    bucked += 1;
                }
                _ => {}
            }
        }
    }
    assert!(tamed > 0 && bucked > 0);
}

#[test]
fn a_mob_rider_is_only_bucked_off_without_the_temper_gain() {
    let (mut base, state) = mob(EntityKind::Horse, Vec3::new(0.5, 64.0, 0.5));
    let mut ai = MobAi::new(EntityKind::Horse, 5, GoalSettings::default());
    let world = World::new();
    let mut raised = Vec::new();
    for _ in 0..800 {
        for action in step(&mut ai, &mut base, &state, &world, ridden(None)) {
            if let MobAction::Equine(EquineAction::Buck { raise_temper }) = action {
                raised.push(raise_temper);
            }
        }
    }
    assert!(!raised.is_empty());
    assert!(raised.iter().all(|raise| !raise));
}

#[test]
fn tamed_and_mob_controlled_horses_do_not_run_around() {
    for (tame, controlled) in [(true, false), (false, true)] {
        let (mut base, state) = mob(EntityKind::Horse, Vec3::new(0.5, 64.0, 0.5));
        let mut ai = MobAi::new(EntityKind::Horse, 5, GoalSettings::default());
        let world = World::new();
        for _ in 0..30 {
            step(&mut ai, &mut base, &state, &world, |view| {
                view.tamed = tame;
                view.species.equine = EquineFacts {
                    vehicle: true,
                    has_passenger: true,
                    mob_controlled: controlled,
                    ..EquineFacts::default()
                };
            });
            assert!(!running(&ai, |goal| matches!(
                goal,
                MobGoal::Equine(rspigot_ai::EquineGoal::RunAround(_))
            )));
        }
    }
}

#[test]
fn a_horse_stands_up_at_random_unless_it_is_eating_or_already_standing() {
    let count = |eating: bool| {
        let (mut base, state) = mob(EntityKind::Horse, Vec3::new(0.5, 64.0, 0.5));
        let mut ai = MobAi::new(EntityKind::Horse, 11, GoalSettings::default());
        let world = World::new();
        let mut stands = 0;
        for _ in 0..40_000 {
            for action in step(&mut ai, &mut base, &state, &world, |view| {
                view.species.equine.eating = eating;
            }) {
                if action == MobAction::Equine(EquineAction::Stand) {
                    stands += 1;
                }
            }
        }
        stands
    };
    assert!(count(false) > 5);
    assert_eq!(count(true), 0);
}

#[test]
fn a_trapped_skeleton_horse_springs_when_a_player_is_within_ten_blocks() {
    let spring = |distance: f64, trap: bool| {
        let (mut base, state) = mob(EntityKind::SkeletonHorse, Vec3::new(0.5, 64.0, 0.5));
        let mut ai = MobAi::new(EntityKind::SkeletonHorse, 2, GoalSettings::default());
        let mut world = World::new();
        world
            .players
            .push(player(Vec3::new(0.5 + distance, 64.0, 0.5)));
        let mut sprung = false;
        for _ in 0..10 {
            sprung |= step(&mut ai, &mut base, &state, &world, |view| {
                view.species.equine.trap = trap;
            })
            .contains(&MobAction::Equine(EquineAction::SpringTrap));
        }
        sprung
    };
    assert!(spring(9.0, true));
    assert!(!spring(11.0, true));
    assert!(!spring(3.0, false));
}

#[test]
fn horses_breed_with_donkeys_but_not_with_mules_or_unready_partners() {
    let breeds = |partner: EntityKind, partner_ready: bool, own_ready: bool| {
        let (mut base, state) = mob(EntityKind::Horse, Vec3::new(0.5, 64.0, 0.5));
        let mut ai = MobAi::new(EntityKind::Horse, 4, GoalSettings::default());
        let mut world = World::new();
        let mut other = living(2, partner, Vec3::new(2.5, 64.0, 0.5));
        other.in_love = true;
        other.facts.can_parent = partner_ready;
        world.others.push(other);
        let mut found = false;
        for _ in 0..4 {
            step(&mut ai, &mut base, &state, &world, |view| {
                view.in_love = true;
                view.species.equine.can_parent = own_ready;
            });
            found |= running(&ai, |goal| matches!(goal, MobGoal::Breed(_)));
        }
        found
    };
    assert!(breeds(EntityKind::Donkey, true, true));
    assert!(breeds(EntityKind::Horse, true, true));
    assert!(!breeds(EntityKind::Mule, true, true));
    assert!(!breeds(EntityKind::Donkey, false, true));
    assert!(!breeds(EntityKind::Donkey, true, false));
}

use std::cell::Cell;

use rspigot_ai::{
    AiWorld, Facts, GoalSettings, GuardianAction, GuardianGoal, Living, MobAction, MobAi, MobBody,
    MobGoal, MobView, MoveStyle, PathType, TargetKind,
};
use rspigot_blocks::{BlockKind, StateId};
use rspigot_domain::entity::living::LivingState;
use rspigot_domain::entity::{Aabb, EntityBase};
use rspigot_domain::{BlockPos, EntityId, Vec3};
use rspigot_entities::EntityKind;
use uuid::Uuid;

struct World {
    others: Vec<Living>,
    wall: bool,
    difficulty: i32,
    time: Cell<i64>,
}

impl World {
    fn sea(difficulty: i32) -> Self {
        Self {
            others: Vec::new(),
            wall: false,
            difficulty,
            time: Cell::new(100),
        }
    }
}

impl rspigot_ai::BlockView for World {
    fn state(&self, pos: BlockPos) -> Option<StateId> {
        Some(if (self.wall && pos.x == 3 && pos.y <= 70) || pos.y <= 60 {
            BlockKind::Stone.default_state()
        } else if pos.y <= 75 {
            BlockKind::Water.default_state()
        } else {
            StateId::AIR
        })
    }

    fn min_y(&self) -> i32 {
        0
    }
}

impl AiWorld for World {
    fn players(&self) -> &[Living] {
        &self.others
    }

    fn living(&self, id: EntityId) -> Option<Living> {
        self.others.iter().find(|living| living.id == id).copied()
    }

    fn living_of(&self, kind: TargetKind, area: &Aabb) -> Vec<Living> {
        self.others
            .iter()
            .filter(|living| kind.matches(living.kind) && living.bounds.intersects(area))
            .copied()
            .collect()
    }

    fn difficulty(&self) -> i32 {
        self.difficulty
    }

    fn game_time(&self) -> i64 {
        self.time.get()
    }

    fn raw_brightness(&self, _pos: BlockPos) -> i32 {
        15
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
        in_water: true,
        baby: false,
        age: 0,
        in_love: false,
        panicking: false,
        has_target: false,
        scared: false,
        facts: Facts::default(),
    }
}

struct Run {
    ai: MobAi,
    base: EntityBase,
    state: LivingState,
    world: World,
    attack_target: i32,
}

impl Run {
    fn new(kind: EntityKind, world: World) -> Self {
        let mut base = EntityBase::new(EntityId(1), Uuid::from_u128(1), kind);
        let position = Vec3::new(0.5, 65.0, 0.5);
        base.position = position;
        base.old_position = position;
        base.tick_count = 1;
        base.fluid.in_water = true;
        let state = LivingState::new(&mut base, 0.0);
        Self {
            ai: MobAi::new(kind, 7, GoalSettings::default()),
            base,
            state,
            world,
            attack_target: 0,
        }
    }

    fn step(&mut self) -> Vec<MobAction> {
        self.base.tick_count += 1;
        self.world.time.set(self.world.time.get() + 1);
        let mut view = MobView::of(&self.base, &self.state);
        view.species.guardian.attack_target = self.attack_target;
        let mut body = MobBody::of(&view);
        self.ai.think(&view, &self.world, &mut body);
        self.attack_target = body.guardian.attack_target;
        self.ai.take_actions()
    }

    fn run_until_beam(&mut self, ticks: usize) -> Vec<(usize, MobAction)> {
        let mut seen = Vec::new();
        for tick in 0..ticks {
            for action in self.step() {
                seen.push((tick, action));
            }
        }
        seen
    }
}

fn kind_of(goal: &MobGoal) -> &'static str {
    match goal {
        MobGoal::Guardian(GuardianGoal::Attack(_)) => "attack",
        MobGoal::Guardian(GuardianGoal::Restriction(_)) => "restriction",
        MobGoal::Guardian(GuardianGoal::Stroll(_)) => "stroll",
        MobGoal::LookAtPlayer(_) => "look",
        MobGoal::RandomLookAround(_) => "look_around",
        MobGoal::NearestAttackableTarget(_) => "nearest",
        _ => "other",
    }
}

type Listing = Vec<(i32, &'static str)>;

fn lists(kind: EntityKind) -> (Listing, Listing) {
    let ai = MobAi::new(kind, 1, GoalSettings::default());
    let map = |goals: &[rspigot_ai::WrappedGoal<MobGoal>]| {
        goals
            .iter()
            .map(|goal| (goal.priority(), kind_of(goal.goal())))
            .collect()
    };
    (map(ai.goals().goals()), map(ai.targets().goals()))
}

#[test]
fn guardians_register_the_vanilla_goals_and_the_single_far_target_goal() {
    for kind in [EntityKind::Guardian, EntityKind::ElderGuardian] {
        let (goals, targets) = lists(kind);
        assert_eq!(
            goals,
            vec![
                (4, "attack"),
                (5, "restriction"),
                (7, "stroll"),
                (8, "look"),
                (8, "look"),
                (9, "look_around"),
            ],
            "{kind:?}"
        );
        assert_eq!(targets, vec![(1, "nearest")]);
    }
}

#[test]
fn guardians_swim_water_bound_with_their_own_move_control() {
    for kind in [EntityKind::Guardian, EntityKind::ElderGuardian] {
        let ai = MobAi::new(kind, 1, GoalSettings::default());
        let brain = ai.brain();
        assert!(brain.profile.is_water_bound());
        assert!(!brain.profile.allows_breaching());
        assert_eq!(brain.profile.malus(PathType::Water), 0.0);
        assert_eq!(brain.movement.style(), MoveStyle::Guardian);
    }
}

fn beams(actions: &[(usize, MobAction)]) -> Vec<(usize, f32)> {
    actions
        .iter()
        .filter_map(|(tick, action)| match action {
            MobAction::Guardian(GuardianAction::Beam { damage, .. }) => Some((*tick, *damage)),
            _ => None,
        })
        .collect()
}

fn prepared(kind: EntityKind, difficulty: i32) -> Run {
    let mut world = World::sea(difficulty);
    world
        .others
        .push(living(2, EntityKind::Player, Vec3::new(6.5, 65.0, 0.5)));
    let mut run = Run::new(kind, world);
    run.ai.set_target(Some(EntityId(2)));
    run
}

#[test]
fn a_guardian_charges_for_ninety_ticks_then_zaps_one_point_and_forgets_its_target() {
    let mut run = prepared(EntityKind::Guardian, 2);
    let seen = run.run_until_beam(100);
    let events: Vec<usize> = seen
        .iter()
        .filter(|(_, action)| matches!(action, MobAction::EntityEvent(21)))
        .map(|(tick, _)| *tick)
        .collect();
    assert_eq!(events.len(), 1);
    let zaps = beams(&seen);
    assert_eq!(zaps.len(), 1);
    assert_eq!(zaps[0].1, 1.0);
    assert_eq!(zaps[0].0 - events[0], 80);
    assert!(
        seen.iter()
            .any(|(_, action)| matches!(action, MobAction::Attack(EntityId(2))))
    );
}

#[test]
fn the_beam_hurts_three_on_hard_and_the_elder_adds_two_more_in_sixty_ticks() {
    let mut hard = prepared(EntityKind::Guardian, 3);
    assert_eq!(beams(&hard.run_until_beam(120))[0].1, 3.0);
    let mut elder = prepared(EntityKind::ElderGuardian, 2);
    let seen = elder.run_until_beam(120);
    let events: Vec<usize> = seen
        .iter()
        .filter(|(_, action)| matches!(action, MobAction::EntityEvent(21)))
        .map(|(tick, _)| *tick)
        .collect();
    let zaps = beams(&seen);
    assert_eq!(zaps[0].1, 3.0);
    assert_eq!(zaps[0].0 - events[0], 60);
    let mut hard_elder = prepared(EntityKind::ElderGuardian, 3);
    assert_eq!(beams(&hard_elder.run_until_beam(120))[0].1, 5.0);
}

#[test]
fn the_beam_target_is_published_when_the_charge_starts_and_cleared_when_it_ends() {
    let mut run = prepared(EntityKind::Guardian, 2);
    let mut published = false;
    let mut zapped = false;
    let mut cleared = false;
    for _ in 0..140 {
        let actions = run.step();
        published |= run.attack_target == 2;
        zapped |= actions
            .iter()
            .any(|action| matches!(action, MobAction::Guardian(GuardianAction::Beam { .. })));
        if zapped && run.attack_target == 0 {
            cleared = true;
            break;
        }
    }
    assert!(published && cleared);
}

#[test]
fn a_guardian_loses_its_target_when_something_blocks_the_line_of_sight() {
    let mut run = prepared(EntityKind::Guardian, 2);
    run.world.wall = true;
    let seen = run.run_until_beam(60);
    assert!(beams(&seen).is_empty());
    assert!(run.ai.target().is_none());
    assert_eq!(run.attack_target, 0);
}

#[test]
fn a_non_elder_guardian_stops_charging_when_the_target_comes_within_three_blocks() {
    let mut run = prepared(EntityKind::Guardian, 2);
    run.run_until_beam(30);
    run.world.others[0].position = Vec3::new(2.5, 65.0, 0.5);
    run.world.others[0].bounds = Aabb::of_size(Vec3::new(2.5, 65.0, 0.5), 0.6, 1.8);
    let seen = run.run_until_beam(120);
    assert!(beams(&seen).is_empty());
    assert_eq!(run.attack_target, 0);
}

#[test]
fn an_elder_keeps_charging_even_when_the_target_is_close() {
    let mut run = prepared(EntityKind::ElderGuardian, 2);
    run.run_until_beam(30);
    run.world.others[0].position = Vec3::new(1.5, 65.0, 0.5);
    run.world.others[0].bounds = Aabb::of_size(Vec3::new(1.5, 65.0, 0.5), 0.6, 1.8);
    let seen = run.run_until_beam(120);
    assert_eq!(beams(&seen).len(), 1);
}

#[test]
fn the_attack_selector_takes_far_players_squids_and_axolotls_only() {
    let mut run = Run::new(EntityKind::Guardian, World::sea(2));
    let kinds = [
        (EntityKind::Player, Vec3::new(6.5, 65.0, 0.5), true),
        (EntityKind::Player, Vec3::new(1.5, 65.0, 0.5), false),
        (EntityKind::Squid, Vec3::new(5.5, 65.0, 0.5), true),
        (EntityKind::GlowSquid, Vec3::new(5.5, 65.0, 1.5), true),
        (EntityKind::Cow, Vec3::new(5.5, 65.0, 2.5), false),
        (EntityKind::Zombie, Vec3::new(5.5, 65.0, 3.5), false),
    ];
    for (index, (kind, position, accepted)) in kinds.into_iter().enumerate() {
        run.world.others = vec![living(2 + index as i32, kind, position)];
        run.ai = MobAi::new(EntityKind::Guardian, 7, GoalSettings::default());
        for _ in 0..30 {
            run.step();
        }
        assert_eq!(run.ai.target().is_some(), accepted, "{kind:?} {position:?}");
    }
}

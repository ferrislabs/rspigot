use std::cell::Cell;
use std::collections::HashMap;

use rspigot_ai::{
    AiWorld, Facts, GoalSettings, Living, MobAction, MobAi, MobBody, MobGoal, MobView,
    PolarBearAction, TargetKind,
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
    time: Cell<i64>,
    universal_anger: bool,
}

impl World {
    fn new() -> Self {
        Self {
            players: Vec::new(),
            others: Vec::new(),
            blocks: HashMap::new(),
            time: Cell::new(0),
            universal_anger: false,
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

    fn universal_anger(&self) -> bool {
        self.universal_anger
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

const PLAYER: EntityId = EntityId(100);

fn player_at(position: Vec3) -> Living {
    living(PLAYER.0, EntityKind::Player, position)
}

fn bear(position: Vec3) -> (EntityBase, LivingState) {
    let mut base = EntityBase::new(EntityId(1), Uuid::from_u128(1), EntityKind::PolarBear);
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

fn steps(
    count: usize,
    ai: &mut MobAi,
    base: &mut EntityBase,
    state: &LivingState,
    world: &World,
    adjust: impl Fn(&mut MobView),
) -> Vec<Vec<MobAction>> {
    (0..count)
        .map(|_| step(ai, base, state, world, &adjust))
        .collect()
}

fn adult(_: &mut MobView) {}

fn as_cub(view: &mut MobView) {
    view.baby = true;
}

fn bear_ai() -> MobAi {
    MobAi::new(EntityKind::PolarBear, 5, GoalSettings::default())
}

fn standing(actions: &[MobAction]) -> Vec<bool> {
    actions
        .iter()
        .filter_map(|action| match action {
            MobAction::PolarBear(PolarBearAction::SetStanding(value)) => Some(*value),
            _ => None,
        })
        .collect()
}

fn warns(actions: &[MobAction]) -> bool {
    actions.contains(&MobAction::PolarBear(PolarBearAction::Warn))
}

fn attacks(actions: &[MobAction]) -> bool {
    actions
        .iter()
        .any(|action| matches!(action, MobAction::Attack(_)))
}

type Listing = Vec<(i32, &'static str)>;

fn priorities(ai: &MobAi) -> (Listing, Listing) {
    fn name(goal: &MobGoal) -> &'static str {
        match goal {
            MobGoal::Float(_) => "float",
            MobGoal::MeleeAttack(_) => "melee",
            MobGoal::Panic(_) => "panic",
            MobGoal::FollowParent(_) => "follow_parent",
            MobGoal::Stroll(_) => "stroll",
            MobGoal::LookAtPlayer(_) => "look",
            MobGoal::RandomLookAround(_) => "look_around",
            MobGoal::HurtByTarget(_) => "hurt_by",
            MobGoal::PolarBear(_) => "attack_players",
            MobGoal::NearestAttackableTarget(_) => "nearest",
            MobGoal::ResetUniversalAnger(_) => "reset_anger",
            _ => "other",
        }
    }
    let list = |goals: &[rspigot_ai::WrappedGoal<MobGoal>]| {
        goals
            .iter()
            .map(|goal| (goal.priority(), name(goal.goal())))
            .collect::<Vec<_>>()
    };
    (list(ai.goals().goals()), list(ai.targets().goals()))
}

#[test]
fn the_goal_and_target_lists_match_the_vanilla_priorities() {
    let (goals, targets) = priorities(&bear_ai());
    assert_eq!(
        goals,
        vec![
            (0, "float"),
            (1, "melee"),
            (1, "panic"),
            (4, "follow_parent"),
            (5, "stroll"),
            (6, "look"),
            (7, "look_around"),
        ]
    );
    assert_eq!(
        targets,
        vec![
            (1, "hurt_by"),
            (2, "attack_players"),
            (3, "nearest"),
            (4, "nearest"),
            (5, "reset_anger"),
        ]
    );
}

fn melee_world(distance: f64) -> (World, EntityBase, LivingState, MobAi) {
    let (base, state) = bear(Vec3::new(0.5, 64.0, 0.5));
    let mut ai = bear_ai();
    ai.set_target(Some(PLAYER));
    let mut world = World::new();
    world.time.set(100);
    world.players = vec![player_at(Vec3::new(0.5 + distance, 64.0, 0.5))];
    (world, base, state, ai)
}

#[test]
fn a_bear_close_to_its_target_warns_ten_ticks_before_it_can_strike() {
    let (world, mut base, state, mut ai) = melee_world(3.0);
    let log = steps(40, &mut ai, &mut base, &state, &world, adult);
    let first = log
        .iter()
        .position(|actions| standing(actions) == vec![false])
        .expect("the goal starts");
    for (tick, actions) in log.iter().enumerate().take(first + 10).skip(first + 1) {
        assert!(standing(actions).is_empty(), "tick {tick}");
        assert!(!warns(actions), "tick {tick}");
    }
    assert_eq!(standing(&log[first + 10]), vec![true]);
    assert!(warns(&log[first + 10]));
    assert_eq!(standing(&log[first + 11]), vec![true]);
    assert!(log.iter().all(|actions| !attacks(actions)));
}

#[test]
fn a_bear_in_reach_strikes_then_drops_back_on_all_fours() {
    let (world, mut base, state, mut ai) = melee_world(1.0);
    let log = steps(20, &mut ai, &mut base, &state, &world, adult);
    let strike = log
        .iter()
        .position(|actions| attacks(actions))
        .expect("the bear strikes");
    assert_eq!(standing(&log[strike]), vec![false]);
    assert!(!warns(&log[strike]));
}

#[test]
fn a_bear_far_from_its_target_stays_down_and_keeps_its_cooldown() {
    let (world, mut base, state, mut ai) = melee_world(6.0);
    let log = steps(40, &mut ai, &mut base, &state, &world, adult);
    assert!(log.iter().any(|actions| standing(actions) == vec![false]));
    for actions in &log {
        assert!(!standing(actions).contains(&true));
        assert!(!warns(actions));
        assert!(!attacks(actions));
    }
}

#[test]
fn the_warning_distance_follows_the_width_of_the_target() {
    let near = |distance: f64| {
        let (world, mut base, state, mut ai) = melee_world(distance);
        let log = steps(40, &mut ai, &mut base, &state, &world, adult);
        log.iter().any(|actions| warns(actions))
    };
    assert!(near(3.5));
    assert!(!near(3.7));
}

#[test]
fn losing_the_target_puts_the_bear_back_down() {
    let (mut world, mut base, state, mut ai) = melee_world(3.0);
    steps(40, &mut ai, &mut base, &state, &world, adult);
    world.players[0].alive = false;
    let log = steps(4, &mut ai, &mut base, &state, &world, adult);
    assert!(log.iter().any(|actions| standing(actions) == vec![false]));
    assert!(
        !ai.goals()
            .goals()
            .iter()
            .any(|goal| { goal.is_running() && matches!(goal.goal(), MobGoal::MeleeAttack(_)) })
    );
}

fn bear_ids(world: &mut World, cubs: &[(i32, Vec3)], adults: &[(i32, Vec3)]) {
    for (id, at) in cubs {
        let mut other = living(*id, EntityKind::PolarBear, *at);
        other.baby = true;
        world.others.push(other);
    }
    for (id, at) in adults {
        world.others.push(living(*id, EntityKind::PolarBear, *at));
    }
}

fn targets_player(
    distance: f64,
    cub_at: Option<Vec3>,
    adjust: impl Fn(&mut MobView) + Copy,
) -> bool {
    let (mut base, state) = bear(Vec3::new(0.5, 64.0, 0.5));
    let mut ai = bear_ai();
    let mut world = World::new();
    world.players = vec![player_at(Vec3::new(0.5 + distance, 64.0, 0.5))];
    if let Some(at) = cub_at {
        bear_ids(&mut world, &[(9, at)], &[]);
    }
    for _ in 0..40 {
        step(&mut ai, &mut base, &state, &world, adjust);
        if ai.target().is_some() {
            return true;
        }
    }
    false
}

#[test]
fn an_adult_defends_a_cub_against_players_within_half_its_follow_range() {
    let cub = Some(Vec3::new(3.5, 64.0, 0.5));
    assert!(targets_player(9.0, cub, adult));
    assert!(!targets_player(11.0, cub, adult));
    assert!(!targets_player(9.0, None, adult));
    assert!(!targets_player(
        9.0,
        Some(Vec3::new(12.5, 64.0, 0.5)),
        adult
    ));
}

#[test]
fn a_cub_never_runs_the_defensive_goal() {
    let cub = Some(Vec3::new(3.5, 64.0, 0.5));
    assert!(!targets_player(5.0, cub, as_cub));
}

#[test]
fn an_angry_bear_hunts_the_player_it_is_angry_at_up_to_its_full_follow_range() {
    let angry = |view: &mut MobView| {
        view.angry = true;
        view.anger_target = Some(PLAYER);
    };
    assert!(targets_player(15.0, None, angry));
    assert!(!targets_player(15.0, None, adult));
}

fn fox_targeted(baby: bool) -> bool {
    let (mut base, state) = bear(Vec3::new(0.5, 64.0, 0.5));
    let mut ai = bear_ai();
    let mut world = World::new();
    world.others = vec![living(8, EntityKind::Fox, Vec3::new(6.5, 64.0, 0.5))];
    for _ in 0..40 {
        step(&mut ai, &mut base, &state, &world, |view| view.baby = baby);
        if ai.target() == Some(EntityId(8)) {
            return true;
        }
    }
    false
}

#[test]
fn adult_bears_hunt_foxes_and_cubs_do_not() {
    assert!(fox_targeted(false));
    assert!(!fox_targeted(true));
}

fn alerts(cub: bool, others: Vec<Living>) -> (Vec<EntityId>, Option<EntityId>) {
    let (mut base, state) = bear(Vec3::new(0.5, 64.0, 0.5));
    let mut ai = bear_ai();
    let mut world = World::new();
    world.others = others;
    world
        .others
        .push(living(7, EntityKind::Zombie, Vec3::new(2.5, 64.0, 0.5)));
    let log = steps(2, &mut ai, &mut base, &state, &world, |view| {
        view.baby = cub;
        view.last_hurt_by_mob = Some(EntityId(7));
        view.last_hurt_by_mob_at = 5;
    });
    let alerted = log
        .into_iter()
        .flatten()
        .filter_map(|action| match action {
            MobAction::Alert { mob, target } => {
                assert_eq!(target, EntityId(7));
                Some(mob)
            }
            _ => None,
        })
        .collect();
    (alerted, ai.target())
}

#[test]
fn a_hurt_cub_calls_the_adult_bears_around_and_drops_its_own_target() {
    let mut baby = living(3, EntityKind::PolarBear, Vec3::new(4.5, 64.0, 0.5));
    baby.baby = true;
    let mut busy = living(4, EntityKind::PolarBear, Vec3::new(5.5, 64.0, 0.5));
    busy.has_target = true;
    let far = living(5, EntityKind::PolarBear, Vec3::new(40.5, 64.0, 0.5));
    let wolf = living(6, EntityKind::Wolf, Vec3::new(4.5, 64.0, 1.5));
    let adult = living(2, EntityKind::PolarBear, Vec3::new(3.5, 64.0, 0.5));
    let (alerted, target) = alerts(true, vec![adult, baby, busy, far, wolf]);
    assert_eq!(alerted, vec![EntityId(2)]);
    assert_eq!(target, None);
}

#[test]
fn a_hurt_adult_fights_back_without_calling_anyone() {
    let adult = living(2, EntityKind::PolarBear, Vec3::new(3.5, 64.0, 0.5));
    let (alerted, target) = alerts(false, vec![adult]);
    assert!(alerted.is_empty());
    assert_eq!(target, Some(EntityId(7)));
}

#[test]
fn a_bear_runs_from_fire_and_for_the_causes_its_age_allows() {
    let panics = |panic: bool| {
        let (mut base, state) = bear(Vec3::new(0.5, 64.0, 0.5));
        let mut ai = bear_ai();
        let world = World::new();
        for _ in 0..4 {
            step(&mut ai, &mut base, &state, &world, |view| {
                view.panic = panic
            });
        }
        ai.goals()
            .goals()
            .iter()
            .any(|goal| goal.is_running() && matches!(goal.goal(), MobGoal::Panic(_)))
    };
    assert!(panics(true));
    assert!(!panics(false));
}

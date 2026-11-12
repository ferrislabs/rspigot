use std::cell::Cell;
use std::collections::HashMap;

use rspigot_ai::{
    AiWorld, Facts, GoalSettings, Living, MobAi, MobBody, MobGoal, MobView, TargetKind,
    TargetReason, TargetRequest, mob_can_attack,
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
    time: Cell<i64>,
    reputation: HashMap<(EntityId, EntityId), i32>,
}

impl World {
    fn new() -> Self {
        Self {
            players: Vec::new(),
            others: Vec::new(),
            time: Cell::new(0),
            reputation: HashMap::new(),
        }
    }
}

impl rspigot_ai::BlockView for World {
    fn state(&self, pos: BlockPos) -> Option<StateId> {
        Some(if (60..64).contains(&pos.y) {
            BlockKind::GrassBlock.default_state()
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

    fn villager_reputation(&self, villager: EntityId, player: EntityId) -> i32 {
        self.reputation
            .get(&(villager, player))
            .copied()
            .unwrap_or(0)
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
        facts: Facts {
            uuid: Uuid::from_u128(id as u128 + 1000),
            ..Facts::default()
        },
    }
}

fn golem_state() -> (EntityBase, LivingState) {
    let mut base = EntityBase::new(EntityId(1), Uuid::from_u128(1), EntityKind::IronGolem);
    let position = Vec3::new(0.5, 64.0, 0.5);
    base.position = position;
    base.old_position = position;
    base.on_ground = true;
    base.tick_count = 1;
    let state = LivingState::new(&mut base, 0.0);
    (base, state)
}

fn new_ai() -> MobAi {
    MobAi::new(EntityKind::IronGolem, 9, GoalSettings::default())
}

fn step(
    ai: &mut MobAi,
    base: &mut EntityBase,
    state: &LivingState,
    world: &World,
    adjust: &impl Fn(&mut MobView),
) {
    base.tick_count += 1;
    world.time.set(world.time.get() + 1);
    let mut view = MobView::of(base, state);
    adjust(&mut view);
    let mut body = MobBody::of(&view);
    ai.think(&view, world, &mut body);
}

fn run_until_targeted(world: &World, adjust: impl Fn(&mut MobView)) -> (MobAi, Vec<TargetRequest>) {
    let (mut base, state) = golem_state();
    let mut ai = new_ai();
    let mut requests = Vec::new();
    for _ in 0..400 {
        step(&mut ai, &mut base, &state, world, &adjust);
        requests.extend(ai.take_target_requests());
        if ai.target().is_some() {
            break;
        }
    }
    (ai, requests)
}

#[test]
fn goals_are_registered_with_the_vanilla_priorities() {
    let ai = new_ai();
    let goals: Vec<i32> = ai
        .goals()
        .goals()
        .iter()
        .map(rspigot_ai::WrappedGoal::priority)
        .collect();
    assert_eq!(goals, vec![1, 2, 2, 4, 5, 7, 8]);
    let targets: Vec<i32> = ai
        .targets()
        .goals()
        .iter()
        .map(rspigot_ai::WrappedGoal::priority)
        .collect();
    assert_eq!(targets, vec![1, 2, 3, 3, 4]);
}

#[test]
fn the_first_goal_is_a_melee_attack_that_follows_even_if_unseen() {
    let ai = new_ai();
    assert!(matches!(
        ai.goals().goals()[0].goal(),
        MobGoal::MeleeAttack(_)
    ));
    assert!(matches!(
        ai.goals().goals()[5].goal(),
        MobGoal::LookAtPlayer(_)
    ));
    assert!(matches!(
        ai.goals().goals()[6].goal(),
        MobGoal::RandomLookAround(_)
    ));
    assert!(matches!(
        ai.targets().goals()[1].goal(),
        MobGoal::HurtByTarget(_)
    ));
    assert!(matches!(
        ai.targets().goals()[4].goal(),
        MobGoal::ResetUniversalAnger(_)
    ));
}

#[test]
fn a_golem_attacks_a_nearby_hostile_mob() {
    let mut world = World::new();
    world
        .others
        .push(living(2, EntityKind::Zombie, Vec3::new(4.5, 64.0, 0.5)));
    let (ai, requests) = run_until_targeted(&world, |_| {});
    assert_eq!(ai.target(), Some(EntityId(2)));
    assert!(
        requests
            .iter()
            .any(|request| request.to == Some(EntityId(2))
                && request.reason == TargetReason::ClosestEntity)
    );
}

#[test]
fn a_golem_picks_the_nearest_hostile_mob() {
    let mut world = World::new();
    world
        .others
        .push(living(2, EntityKind::Zombie, Vec3::new(6.5, 64.0, 0.5)));
    world
        .others
        .push(living(3, EntityKind::Skeleton, Vec3::new(3.5, 64.0, 0.5)));
    let (ai, _) = run_until_targeted(&world, |_| {});
    assert_eq!(ai.target(), Some(EntityId(3)));
}

#[test]
fn a_golem_leaves_creepers_and_peaceful_mobs_alone() {
    let mut world = World::new();
    world
        .others
        .push(living(2, EntityKind::Creeper, Vec3::new(3.5, 64.0, 0.5)));
    world
        .others
        .push(living(3, EntityKind::Cow, Vec3::new(4.5, 64.0, 0.5)));
    world
        .others
        .push(living(4, EntityKind::Villager, Vec3::new(5.5, 64.0, 0.5)));
    let (ai, _) = run_until_targeted(&world, |_| {});
    assert_eq!(ai.target(), None);
}

#[test]
fn slimes_phantoms_and_hoglins_count_as_enemies() {
    for kind in [EntityKind::Slime, EntityKind::Hoglin, EntityKind::Phantom] {
        let mut world = World::new();
        world
            .others
            .push(living(2, kind, Vec3::new(3.5, 64.0, 0.5)));
        let (ai, _) = run_until_targeted(&world, |_| {});
        assert_eq!(ai.target(), Some(EntityId(2)), "{kind:?}");
    }
}

#[test]
fn a_golem_never_attacks_a_ghast_because_mobs_cannot_target_them() {
    let mut world = World::new();
    world
        .others
        .push(living(2, EntityKind::Ghast, Vec3::new(3.5, 64.0, 0.5)));
    let (ai, _) = run_until_targeted(&world, |_| {});
    assert_eq!(ai.target(), None);
}

#[test]
fn a_golem_does_not_look_for_hostile_mobs_beyond_its_follow_range() {
    let mut world = World::new();
    world
        .others
        .push(living(2, EntityKind::Zombie, Vec3::new(40.5, 64.0, 0.5)));
    let (ai, _) = run_until_targeted(&world, |_| {});
    assert_eq!(ai.target(), None);
}

#[test]
fn a_golem_ignores_players_it_is_not_angry_at() {
    let mut world = World::new();
    world
        .players
        .push(living(7, EntityKind::Player, Vec3::new(3.5, 64.0, 0.5)));
    let (ai, _) = run_until_targeted(&world, |_| {});
    assert_eq!(ai.target(), None);
}

#[test]
fn a_golem_attacks_the_player_it_is_angry_at() {
    let mut world = World::new();
    world
        .players
        .push(living(7, EntityKind::Player, Vec3::new(3.5, 64.0, 0.5)));
    let (ai, requests) = run_until_targeted(&world, |view| {
        view.angry = true;
        view.anger_target = Some(EntityId(7));
    });
    assert_eq!(ai.target(), Some(EntityId(7)));
    assert!(
        requests
            .iter()
            .any(|request| request.reason == TargetReason::ClosestPlayer)
    );
}

#[test]
fn a_golem_built_by_a_player_never_targets_players_even_when_angry() {
    let mut world = World::new();
    world
        .players
        .push(living(7, EntityKind::Player, Vec3::new(3.5, 64.0, 0.5)));
    let (ai, _) = run_until_targeted(&world, |view| {
        view.player_created = true;
        view.angry = true;
        view.anger_target = Some(EntityId(7));
    });
    assert_eq!(ai.target(), None);
}

#[test]
fn a_golem_built_by_a_player_still_attacks_hostile_mobs() {
    let mut world = World::new();
    world
        .others
        .push(living(2, EntityKind::Zombie, Vec3::new(4.5, 64.0, 0.5)));
    let (ai, _) = run_until_targeted(&world, |view| view.player_created = true);
    assert_eq!(ai.target(), Some(EntityId(2)));
}

#[test]
fn a_golem_retaliates_against_whoever_hurt_it() {
    let mut world = World::new();
    world
        .others
        .push(living(5, EntityKind::Cow, Vec3::new(3.5, 64.0, 0.5)));
    let (ai, requests) = run_until_targeted(&world, |view| {
        view.last_hurt_by_mob = Some(EntityId(5));
        view.last_hurt_by_mob_at = 10;
    });
    assert_eq!(ai.target(), Some(EntityId(5)));
    assert!(
        requests
            .iter()
            .any(|request| request.reason == TargetReason::TargetAttackedEntity)
    );
}

fn hated_player_world() -> World {
    let mut world = World::new();
    world
        .players
        .push(living(7, EntityKind::Player, Vec3::new(3.5, 64.0, 0.5)));
    world
        .others
        .push(living(8, EntityKind::Villager, Vec3::new(2.5, 64.0, 2.5)));
    world.reputation.insert((EntityId(8), EntityId(7)), -100);
    world
}

#[test]
fn a_golem_defends_villagers_against_a_hated_player_before_anything_else() {
    let mut world = hated_player_world();
    world
        .others
        .push(living(2, EntityKind::Zombie, Vec3::new(2.5, 64.0, -2.5)));
    let (ai, requests) = run_until_targeted(&world, |_| {});
    assert_eq!(ai.target(), Some(EntityId(7)));
    assert_eq!(
        requests.first().map(|request| request.reason),
        Some(TargetReason::DefendVillage)
    );
}

#[test]
fn a_golem_made_by_a_player_does_not_defend_against_players() {
    let world = hated_player_world();
    let (ai, _) = run_until_targeted(&world, |view| view.player_created = true);
    assert_eq!(ai.target(), None);
}

#[test]
fn the_golem_attack_rules_refuse_creepers_and_its_makers() {
    let (base, state) = golem_state();
    let wild = MobView::of(&base, &state);
    let mut made = wild;
    made.player_created = true;
    let world = World::new();
    let player = living(7, EntityKind::Player, Vec3::new(3.5, 64.0, 0.5));
    let creeper = living(2, EntityKind::Creeper, Vec3::new(3.5, 64.0, 0.5));
    let zombie = living(3, EntityKind::Zombie, Vec3::new(3.5, 64.0, 0.5));
    assert!(mob_can_attack(&wild, &world, &player));
    assert!(!mob_can_attack(&made, &world, &player));
    assert!(!mob_can_attack(&wild, &world, &creeper));
    assert!(!mob_can_attack(&made, &world, &creeper));
    assert!(mob_can_attack(&wild, &world, &zombie));
    assert!(mob_can_attack(&made, &world, &zombie));
}

#[test]
fn other_mobs_attack_creepers_and_players_as_before() {
    let mut base = EntityBase::new(EntityId(1), Uuid::from_u128(1), EntityKind::Zombie);
    base.position = Vec3::new(0.5, 64.0, 0.5);
    let state = LivingState::new(&mut base, 0.0);
    let mut view = MobView::of(&base, &state);
    view.player_created = true;
    let world = World::new();
    let creeper = living(2, EntityKind::Creeper, Vec3::new(3.5, 64.0, 0.5));
    let player = living(7, EntityKind::Player, Vec3::new(3.5, 64.0, 0.5));
    assert!(mob_can_attack(&view, &world, &creeper));
    assert!(mob_can_attack(&view, &world, &player));
}

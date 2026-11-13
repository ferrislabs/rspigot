use std::cell::Cell;
use std::collections::HashMap;

use rspigot_ai::{
    AiWorld, DolphinFacts, DroppedItem, Facts, FishFacts, GoalSettings, Living, MobAction, MobAi,
    MobBody, MobGoal, MobView, MoveStyle, PathType, SquidFacts, TargetKind, WaterAction, WaterGoal,
    motion_direction, thrown_item_velocity,
};
use rspigot_blocks::{BlockKind, StateId};
use rspigot_domain::entity::living::LivingState;
use rspigot_domain::entity::{Aabb, EntityBase};
use rspigot_domain::{BlockPos, EntityId, Vec3};
use rspigot_entities::EntityKind;
use rspigot_items::ItemKind;
use uuid::Uuid;

const WATER_TOP: i32 = 70;

struct World {
    players: Vec<Living>,
    others: Vec<Living>,
    items: Vec<DroppedItem>,
    blocks: HashMap<BlockPos, StateId>,
    water_top: i32,
    time: Cell<i64>,
}

impl World {
    fn new() -> Self {
        Self {
            players: Vec::new(),
            others: Vec::new(),
            items: Vec::new(),
            blocks: HashMap::new(),
            water_top: WATER_TOP,
            time: Cell::new(100),
        }
    }

    fn shallow() -> Self {
        Self {
            water_top: 63,
            ..Self::new()
        }
    }
}

impl rspigot_ai::BlockView for World {
    fn state(&self, pos: BlockPos) -> Option<StateId> {
        if let Some(state) = self.blocks.get(&pos) {
            return Some(*state);
        }
        Some(if pos.y == 60 {
            BlockKind::Stone.default_state()
        } else if (61..=self.water_top).contains(&pos.y) {
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
        self.players
            .iter()
            .chain(&self.others)
            .filter(|living| kind.matches(living.kind) && living.bounds.intersects(area))
            .copied()
            .collect()
    }

    fn items_in(&self, area: &Aabb) -> Vec<DroppedItem> {
        self.items
            .iter()
            .filter(|item| item.bounds.intersects(area))
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

fn school_member(
    id: i32,
    kind: EntityKind,
    position: Vec3,
    leader: Option<i32>,
    size: i32,
) -> Living {
    let mut member = living(id, kind, position);
    member.facts.school_leader = leader.map(EntityId);
    member.facts.school_size = size;
    member
}

fn swimmer(kind: EntityKind, position: Vec3) -> (EntityBase, LivingState) {
    let mut base = EntityBase::new(EntityId(1), Uuid::from_u128(1), kind);
    base.position = position;
    base.old_position = position;
    base.tick_count = 1;
    base.fluid.in_water = true;
    base.fluid.water_height = 1.0;
    let state = LivingState::new(&mut base, 0.0);
    (base, state)
}

fn view_of(base: &EntityBase, state: &LivingState, adjust: &dyn Fn(&mut MobView)) -> MobView {
    let mut view = MobView::of(base, state);
    adjust(&mut view);
    view
}

struct Run {
    ai: MobAi,
    base: EntityBase,
    state: LivingState,
    world: World,
}

impl Run {
    fn pair(&mut self, adjust: &dyn Fn(&mut MobView)) -> MobBody {
        self.step(adjust);
        self.step(adjust).1
    }

    fn new(kind: EntityKind, position: Vec3, world: World) -> Self {
        let (base, state) = swimmer(kind, position);
        Self {
            ai: MobAi::new(kind, 7, GoalSettings::default()),
            base,
            state,
            world,
        }
    }

    fn step(&mut self, adjust: &dyn Fn(&mut MobView)) -> (Vec<MobAction>, MobBody) {
        self.base.tick_count += 1;
        self.world.time.set(self.world.time.get() + 1);
        let view = view_of(&self.base, &self.state, adjust);
        let mut body = MobBody::of(&view);
        self.ai.think(&view, &self.world, &mut body);
        (self.ai.take_actions(), body)
    }
}

type Listing = Vec<(i32, &'static str)>;

fn name(goal: &MobGoal) -> &'static str {
    match goal {
        MobGoal::Panic(_) => "panic",
        MobGoal::AvoidEntity(_) => "avoid",
        MobGoal::MeleeAttack(_) => "melee",
        MobGoal::LookAtPlayer(_) => "look",
        MobGoal::RandomLookAround(_) => "look_around",
        MobGoal::HurtByTarget(_) => "hurt_by",
        MobGoal::Water(goal) => match goal {
            WaterGoal::RandomSwimming(_) => "swim",
            WaterGoal::FollowFlockLeader(_) => "flock",
            WaterGoal::PufferfishPuff(_) => "puff",
            WaterGoal::SquidRandomMovement(_) => "squid_move",
            WaterGoal::SquidFlee(_) => "squid_flee",
            WaterGoal::BreathAir(_) => "breath",
            WaterGoal::TryFindWater(_) => "find_water",
            WaterGoal::DolphinSwimToTreasure(_) => "treasure",
            WaterGoal::DolphinSwimWithPlayer(_) => "play_with_player",
            WaterGoal::DolphinJump(_) => "jump",
            WaterGoal::DolphinMoveToItem(_) => "move_to_item",
            WaterGoal::DolphinPlayWithItems(_) => "play_with_items",
        },
        _ => "other",
    }
}

fn listing(goals: &[rspigot_ai::WrappedGoal<MobGoal>]) -> Listing {
    goals
        .iter()
        .map(|goal| (goal.priority(), name(goal.goal())))
        .collect()
}

fn lists(kind: EntityKind) -> (Listing, Listing) {
    let ai = MobAi::new(kind, 1, GoalSettings::default());
    (listing(ai.goals().goals()), listing(ai.targets().goals()))
}

#[test]
fn the_fish_register_the_vanilla_goals_with_their_priorities() {
    for kind in [
        EntityKind::Cod,
        EntityKind::Salmon,
        EntityKind::TropicalFish,
    ] {
        let (goals, targets) = lists(kind);
        assert_eq!(
            goals,
            vec![(0, "panic"), (2, "avoid"), (4, "swim"), (5, "flock")],
            "{kind:?}"
        );
        assert!(targets.is_empty());
    }
    let (goals, targets) = lists(EntityKind::Pufferfish);
    assert_eq!(
        goals,
        vec![(0, "panic"), (2, "avoid"), (4, "swim"), (1, "puff")]
    );
    assert!(targets.is_empty());
}

#[test]
fn the_squid_and_the_glow_squid_register_the_random_movement_then_the_flee_goal() {
    for kind in [EntityKind::Squid, EntityKind::GlowSquid] {
        let (goals, targets) = lists(kind);
        assert_eq!(goals, vec![(0, "squid_move"), (1, "squid_flee")]);
        assert!(targets.is_empty());
    }
}

#[test]
fn the_dolphin_registers_the_vanilla_goals_and_targets() {
    let (goals, targets) = lists(EntityKind::Dolphin);
    assert_eq!(
        goals,
        vec![
            (0, "breath"),
            (0, "find_water"),
            (1, "treasure"),
            (2, "play_with_player"),
            (4, "swim"),
            (4, "look_around"),
            (5, "look"),
            (5, "jump"),
            (6, "melee"),
            (7, "move_to_item"),
            (8, "play_with_items"),
            (9, "avoid"),
        ]
    );
    assert_eq!(targets, vec![(1, "hurt_by")]);
}

#[test]
fn the_swimmers_get_a_water_bound_profile_with_free_water_and_their_controls() {
    for kind in [
        EntityKind::Cod,
        EntityKind::Salmon,
        EntityKind::Pufferfish,
        EntityKind::TropicalFish,
    ] {
        let ai = MobAi::new(kind, 1, GoalSettings::default());
        let brain = ai.brain();
        assert!(brain.profile.is_water_bound(), "{kind:?}");
        assert!(!brain.profile.allows_breaching());
        assert_eq!(brain.profile.malus(PathType::Water), 0.0);
        assert_eq!(brain.movement.style(), MoveStyle::Fish);
    }
    let ai = MobAi::new(EntityKind::Dolphin, 1, GoalSettings::default());
    let brain = ai.brain();
    assert!(brain.profile.allows_breaching());
    assert_eq!(brain.profile.malus(PathType::Water), 0.0);
    assert_eq!(brain.movement.style(), MoveStyle::SmoothSwimming);
    let smooth = brain.movement.smooth_swimming();
    assert_eq!(
        (smooth.max_turn_x, smooth.max_turn_y, smooth.apply_gravity),
        (85, 10, true)
    );
    assert_eq!(
        (
            smooth.in_water_speed_modifier,
            smooth.outside_water_speed_modifier
        ),
        (0.02, 0.1)
    );
    assert_eq!(brain.look.smooth_swimming(), Some(10));
}

fn flock(run: &mut Run, others: Vec<Living>, own: FishFacts, steps: usize) -> Vec<MobAction> {
    let mut pool = vec![school_member(
        1,
        run.base.kind,
        run.base.position,
        own.leader.map(|leader| leader.0),
        own.school_size,
    )];
    pool.extend(others);
    run.world.others = pool;
    let mut actions = Vec::new();
    for _ in 0..steps {
        let (taken, _) = run.step(&|view| view.species.fish = own);
        actions.extend(taken);
    }
    actions
}

fn recruits(actions: &[MobAction]) -> Vec<(i32, i32)> {
    actions
        .iter()
        .filter_map(|action| match action {
            MobAction::Water(WaterAction::Recruit { leader, follower }) => {
                Some((leader.0, follower.0))
            }
            _ => None,
        })
        .collect()
}

fn cod_at(id: i32, x: f64, leader: Option<i32>, size: i32) -> Living {
    school_member(id, EntityKind::Cod, Vec3::new(x, 65.0, 0.5), leader, size)
}

#[test]
fn a_loner_waits_about_two_hundred_ticks_then_recruits_its_neighbours() {
    let mut run = Run::new(EntityKind::Cod, Vec3::new(0.5, 65.0, 0.5), World::new());
    let own = FishFacts {
        school_size: 1,
        ..FishFacts::default()
    };
    let neighbours = vec![cod_at(2, 2.5, None, 1), cod_at(3, 3.5, None, 1)];
    let early = flock(&mut run, neighbours.clone(), own, 20);
    assert!(recruits(&early).is_empty());
    let all = flock(&mut run, neighbours, own, 300);
    let joined = recruits(&all);
    assert_eq!(joined, vec![(1, 2), (1, 3)]);
}

#[test]
fn the_first_free_fish_in_the_area_leads_and_the_others_follow_it() {
    let mut run = Run::new(EntityKind::Cod, Vec3::new(0.5, 65.0, 0.5), World::new());
    let own = FishFacts {
        school_size: 1,
        ..FishFacts::default()
    };
    let neighbours = vec![cod_at(2, 2.5, None, 1)];
    let actions = flock(&mut run, neighbours, own, 300);
    assert_eq!(recruits(&actions), vec![(1, 2)]);
}

#[test]
fn a_fish_listed_after_a_free_one_joins_it_and_keeps_following_while_the_leader_is_near() {
    let mut run = Run::new(EntityKind::Cod, Vec3::new(0.5, 65.0, 0.5), World::new());
    let own = FishFacts {
        school_size: 1,
        ..FishFacts::default()
    };
    run.world.others = vec![
        cod_at(2, 2.5, None, 1),
        school_member(1, EntityKind::Cod, Vec3::new(0.5, 65.0, 0.5), None, 1),
    ];
    let mut actions = Vec::new();
    for _ in 0..300 {
        actions.extend(run.step(&|view| view.species.fish = own).0);
    }
    assert_eq!(recruits(&actions), vec![(2, 1)]);
    let follower = FishFacts {
        leader: Some(EntityId(2)),
        school_size: 1,
        ..FishFacts::default()
    };
    run.world.others = vec![
        cod_at(2, 2.5, None, 2),
        school_member(1, EntityKind::Cod, Vec3::new(0.5, 65.0, 0.5), Some(2), 1),
    ];
    for _ in 0..4 {
        run.step(&|view| view.species.fish = follower);
    }
    let running = run
        .ai
        .goals()
        .goals()
        .iter()
        .any(|goal| name(goal.goal()) == "flock" && goal.is_running());
    assert!(running);
}

#[test]
fn a_school_leader_never_runs_the_follow_goal() {
    let mut run = Run::new(EntityKind::Cod, Vec3::new(0.5, 65.0, 0.5), World::new());
    let leader = FishFacts {
        school_size: 3,
        ..FishFacts::default()
    };
    let actions = flock(&mut run, vec![cod_at(2, 2.5, None, 1)], leader, 420);
    assert!(recruits(&actions).is_empty());
}

#[test]
fn recruiting_stops_when_the_school_is_full() {
    let mut run = Run::new(EntityKind::Salmon, Vec3::new(0.5, 65.0, 0.5), World::new());
    let own = FishFacts {
        school_size: 1,
        ..FishFacts::default()
    };
    let crowd: Vec<Living> = (2..9)
        .map(|id| {
            school_member(
                id,
                EntityKind::Salmon,
                Vec3::new(f64::from(id), 65.0, 0.5),
                None,
                1,
            )
        })
        .collect();
    let actions = flock(&mut run, crowd, own, 420);
    let joined = recruits(&actions);
    assert_eq!(joined, vec![(1, 2), (1, 3), (1, 4)]);
}

#[test]
fn a_follower_paths_to_its_leader_every_five_ticks_and_never_swims_on_its_own() {
    let mut run = Run::new(EntityKind::Cod, Vec3::new(0.5, 65.0, 0.5), World::new());
    run.world.others = vec![cod_at(2, 9.5, None, 2)];
    let own = FishFacts {
        leader: Some(EntityId(2)),
        school_size: 1,
        ..FishFacts::default()
    };
    let mut paths = 0;
    for _ in 0..12 {
        let (_, _) = run.step(&|view| view.species.fish = own);
        if run.ai.brain().navigation.path().is_some() {
            paths += 1;
        }
    }
    assert!(paths > 0);
    let target = run.ai.brain().navigation.target_pos();
    assert!(target.is_some_and(|pos| (pos.x - 9).abs() <= 1));
    let swim_runs = run
        .ai
        .goals()
        .goals()
        .iter()
        .filter(|goal| name(goal.goal()) == "swim")
        .any(rspigot_ai::WrappedGoal::is_running);
    assert!(!swim_runs);
}

#[test]
fn a_follower_stops_following_when_its_leader_is_dead_or_too_far() {
    let mut run = Run::new(EntityKind::Cod, Vec3::new(0.5, 65.0, 0.5), World::new());
    let own = FishFacts {
        leader: Some(EntityId(2)),
        school_size: 1,
        ..FishFacts::default()
    };
    run.world.others = vec![cod_at(2, 5.5, None, 2)];
    for _ in 0..4 {
        run.step(&|view| view.species.fish = own);
    }
    run.world.others = vec![cod_at(2, 20.5, None, 2)];
    let mut stopped = false;
    for _ in 0..4 {
        let (actions, _) = run.step(&|view| view.species.fish = own);
        stopped |= actions.contains(&MobAction::Water(WaterAction::StopFollowing));
    }
    assert!(stopped);
}

#[test]
fn a_fish_without_a_leader_swims_to_random_water_positions() {
    let mut run = Run::new(EntityKind::Cod, Vec3::new(0.5, 65.0, 0.5), World::new());
    let mut moved = false;
    for _ in 0..400 {
        run.step(&|_| {});
        if run.ai.brain().navigation.path().is_some() {
            moved = true;
            let end = run.ai.brain().navigation.target_pos().unwrap();
            assert!((61..=WATER_TOP).contains(&end.y), "{end:?}");
            break;
        }
    }
    assert!(moved);
}

#[test]
fn the_pufferfish_puffs_for_scary_neighbours_within_two_blocks_only() {
    let mut run = Run::new(
        EntityKind::Pufferfish,
        Vec3::new(0.5, 65.0, 0.5),
        World::new(),
    );
    run.world.others = vec![living(5, EntityKind::Zombie, Vec3::new(1.5, 64.5, 0.5))];
    let (_, body) = run.step(&|_| {});
    let (_, body2) = run.step(&|_| {});
    assert!(body.fish.inflate_counter == 1 || body2.fish.inflate_counter == 1);
    let mut calm = Run::new(
        EntityKind::Pufferfish,
        Vec3::new(0.5, 65.0, 0.5),
        World::new(),
    );
    calm.world.others = vec![living(5, EntityKind::Zombie, Vec3::new(5.5, 64.5, 0.5))];
    for _ in 0..4 {
        let (_, body) = calm.step(&|_| {});
        assert_eq!(body.fish.inflate_counter, 0);
    }
}

#[test]
fn the_pufferfish_ignores_the_kinds_of_the_not_scary_tag_and_creative_players() {
    for kind in [
        EntityKind::Turtle,
        EntityKind::Guardian,
        EntityKind::Cod,
        EntityKind::Dolphin,
        EntityKind::Squid,
        EntityKind::Pufferfish,
    ] {
        assert!(!rspigot_ai::is_scary_to_pufferfish(&living(
            5,
            kind,
            Vec3::ZERO
        )));
    }
    let mut creative = living(6, EntityKind::Player, Vec3::ZERO);
    creative.creative = true;
    assert!(!rspigot_ai::is_scary_to_pufferfish(&creative));
    creative.creative = false;
    assert!(rspigot_ai::is_scary_to_pufferfish(&creative));
}

#[test]
fn the_puff_goal_resets_the_deflate_timer_and_stops_with_the_counter() {
    let mut run = Run::new(
        EntityKind::Pufferfish,
        Vec3::new(0.5, 65.0, 0.5),
        World::new(),
    );
    run.world.others = vec![living(5, EntityKind::Zombie, Vec3::new(1.5, 64.5, 0.5))];
    let mut started = false;
    for _ in 0..4 {
        let (_, body) = run.step(&|view| view.species.fish.deflate_timer = 33);
        if body.fish.inflate_counter == 1 {
            assert_eq!(body.fish.deflate_timer, 0);
            started = true;
            break;
        }
    }
    assert!(started);
    run.world.others.clear();
    let mut stopped = false;
    for _ in 0..4 {
        let (_, body) = run.step(&|view| view.species.fish.inflate_counter = 9);
        stopped |= body.fish.inflate_counter == 0;
    }
    assert!(stopped);
}

fn squid() -> Run {
    Run::new(EntityKind::Squid, Vec3::new(0.5, 65.0, 0.5), World::new())
}

#[test]
fn an_idle_squid_picks_a_horizontal_circle_vector_with_a_small_vertical_drift() {
    let mut run = squid();
    let body = run.pair(&|_| {});
    let movement = body.squid.movement;
    let horizontal = (movement.x * movement.x + movement.z * movement.z).sqrt();
    assert!((horizontal - 0.2).abs() < 1.0e-6, "{movement:?}");
    assert!((-0.1..0.1).contains(&movement.y), "{movement:?}");
}

#[test]
fn a_squid_keeps_its_vector_most_of_the_time_while_wet_and_resets_it_after_a_hundred_idle_ticks() {
    let mut run = squid();
    let fixed = SquidFacts {
        movement: Vec3::new(0.1, 0.0, 0.1),
    };
    let mut kept = 0;
    for _ in 0..100 {
        let body = run.pair(&|view| view.species.squid = fixed);
        if body.squid.movement == fixed.movement {
            kept += 1;
        }
    }
    assert!(kept > 80, "{kept}");
    let body = run.pair(&|view| {
        view.species.squid = fixed;
        view.no_action_time = 101;
    });
    assert_eq!(body.squid.movement, Vec3::ZERO);
    let body = run.pair(&|view| {
        view.species.squid = fixed;
        view.no_action_time = 100;
    });
    assert!(body.squid.has_movement_vector());
}

#[test]
fn a_dry_squid_always_picks_a_new_vector() {
    let mut run = squid();
    let fixed = SquidFacts {
        movement: Vec3::new(0.1, 0.0, 0.1),
    };
    let body = run.pair(&|view| {
        view.species.squid = fixed;
        view.fluid.in_water = false;
    });
    assert_ne!(body.squid.movement, fixed.movement);
    assert!(body.squid.has_movement_vector());
}

#[test]
fn a_hurt_squid_flees_along_the_not_normalised_vector_scaled_and_divided_by_twenty() {
    let mut run = squid();
    run.world.others = vec![living(9, EntityKind::Zombie, Vec3::new(-1.5, 65.0, 0.5))];
    let hurt = |view: &mut MobView| view.last_hurt_by_mob = Some(EntityId(9));
    let body = run.pair(&hurt);
    let flee = body.squid.movement;
    assert!((flee.x - 2.0 * 3.0 / 20.0).abs() < 1.0e-9, "{flee:?}");
    assert_eq!((flee.y, flee.z), (0.0, 0.0));
}

#[test]
fn the_flee_push_falls_off_beyond_five_blocks() {
    let mut run = squid();
    run.world.others = vec![living(9, EntityKind::Zombie, Vec3::new(-7.5, 65.0, 0.5))];
    let hurt = |view: &mut MobView| view.last_hurt_by_mob = Some(EntityId(9));
    let flee = run.pair(&hurt).squid.movement;
    let push = 3.0 - (8.0 - 5.0) / 5.0;
    assert!((flee.x - 8.0 * push / 20.0).abs() < 1.0e-9, "{flee:?}");
}

#[test]
fn a_squid_fleeing_over_air_loses_the_vertical_part_and_one_far_away_does_not_flee() {
    let mut run = squid();
    run.world.water_top = 65;
    run.world.others = vec![living(9, EntityKind::Zombie, Vec3::new(-1.5, 64.0, 0.5))];
    run.base.position = Vec3::new(0.5, 65.0, 0.5);
    let hurt = |view: &mut MobView| view.last_hurt_by_mob = Some(EntityId(9));
    let flee = run.pair(&hurt).squid.movement;
    assert_eq!(flee.y, 0.0);
    let mut run = squid();
    run.world.others = vec![living(9, EntityKind::Zombie, Vec3::new(-20.5, 65.0, 0.5))];
    let body = run.pair(&hurt);
    assert!(body.squid.movement.x.abs() <= 0.2 + 1.0e-9);
}

fn dolphin(world: World) -> Run {
    Run::new(EntityKind::Dolphin, Vec3::new(0.5, 62.0, 0.5), world)
}

#[test]
fn the_dolphin_looks_for_air_below_a_hundred_and_forty_and_assists_each_tick() {
    let mut run = dolphin(World::shallow());
    run.step(&|view| view.air_supply = 140);
    run.step(&|view| view.air_supply = 140);
    let breathing = run
        .ai
        .goals()
        .goals()
        .iter()
        .any(|goal| name(goal.goal()) == "breath" && goal.is_running());
    assert!(!breathing);
    let mut assists = 0;
    for _ in 0..4 {
        let (actions, _) = run.step(&|view| view.air_supply = 139);
        assists += actions
            .iter()
            .filter(|action| **action == MobAction::Water(WaterAction::BreathAssist))
            .count();
    }
    assert!(assists >= 2, "{assists}");
    assert!(run.ai.brain().navigation.path().is_some() || run.ai.brain().navigation.is_done());
}

#[test]
fn the_dolphin_jumps_when_the_water_and_the_air_ahead_are_clear_and_pitches_with_its_velocity() {
    let mut world = World::shallow();
    world.water_top = 63;
    let mut run = dolphin(world);
    run.base.fluid.in_water = true;
    let mut jumped = false;
    for _ in 0..200 {
        let (_, body) = run.step(&|view| {
            view.position = Vec3::new(0.5, 63.0, 0.5);
            view.yaw = 0.0;
        });
        if body.velocity.y > 0.0 {
            jumped = true;
            assert_eq!(body.velocity.x, 0.0);
            assert!((body.velocity.z - 0.6).abs() < 1.0e-9);
            assert!((body.velocity.y - 0.7).abs() < 1.0e-9);
            break;
        }
    }
    assert!(jumped);
}

#[test]
fn the_motion_direction_follows_the_vanilla_yaw_quadrants() {
    assert_eq!(motion_direction(0.0), (0, 1));
    assert_eq!(motion_direction(90.0), (-1, 0));
    assert_eq!(motion_direction(180.0), (0, -1));
    assert_eq!(motion_direction(-90.0), (1, 0));
    assert_eq!(motion_direction(44.0), (0, 1));
    assert_eq!(motion_direction(46.0), (-1, 0));
    assert_eq!(motion_direction(360.0), (0, 1));
}

#[test]
fn a_dolphin_with_a_fish_and_no_structure_gives_up_and_loses_the_flag() {
    let mut run = dolphin(World::new());
    let mut cleared = false;
    for _ in 0..8 {
        let (_, body) = run.step(&|view| {
            view.species.dolphin = DolphinFacts {
                got_fish: true,
                treasure_pos: None,
            };
        });
        cleared |= !body.dolphin.got_fish;
    }
    assert!(cleared);
}

struct Structure(World, BlockPos);

impl rspigot_ai::BlockView for Structure {
    fn state(&self, pos: BlockPos) -> Option<StateId> {
        self.0.state(pos)
    }

    fn min_y(&self) -> i32 {
        0
    }
}

impl AiWorld for Structure {
    fn players(&self) -> &[Living] {
        self.0.players()
    }

    fn living(&self, id: EntityId) -> Option<Living> {
        self.0.living(id)
    }

    fn living_of(&self, kind: TargetKind, area: &Aabb) -> Vec<Living> {
        self.0.living_of(kind, area)
    }

    fn difficulty(&self) -> i32 {
        2
    }

    fn game_time(&self) -> i64 {
        self.0.game_time()
    }

    fn raw_brightness(&self, _pos: BlockPos) -> i32 {
        15
    }

    fn nearest_dolphin_structure(&self, _pos: BlockPos, radius: i32) -> Option<BlockPos> {
        assert_eq!(radius, 50);
        Some(self.1)
    }
}

#[test]
fn a_dolphin_with_a_fish_swims_toward_the_treasure_and_announces_it() {
    let world = Structure(World::new(), BlockPos::new(30, 40, 0));
    let (mut base, state) = swimmer(EntityKind::Dolphin, Vec3::new(0.5, 65.0, 0.5));
    let mut ai = MobAi::new(EntityKind::Dolphin, 7, GoalSettings::default());
    let mut announced = false;
    let mut treasure = None;
    let mut moving = false;
    for _ in 0..8 {
        base.tick_count += 1;
        world.0.time.set(world.0.time.get() + 1);
        let mut view = MobView::of(&base, &state);
        view.species.dolphin = DolphinFacts {
            got_fish: true,
            treasure_pos: treasure,
        };
        let mut body = MobBody::of(&view);
        ai.think(&view, &world, &mut body);
        announced |= ai.take_actions().contains(&MobAction::EntityEvent(38));
        treasure = body.dolphin.treasure_pos;
        moving |= ai.brain().navigation.path().is_some();
    }
    assert!(announced);
    assert_eq!(treasure, Some(BlockPos::new(30, 40, 0)));
    assert!(moving);
}

#[test]
fn a_swimming_player_nearby_gets_dolphins_grace_from_the_dolphin() {
    let mut world = World::new();
    let mut player = living(100, EntityKind::Player, Vec3::new(3.5, 65.0, 0.5));
    player.facts.swimming = true;
    world.players = vec![player];
    let mut run = dolphin(world);
    let mut graces = 0;
    for _ in 0..10 {
        let (actions, _) = run.step(&|_| {});
        graces += actions
            .iter()
            .filter(|action| {
                **action == MobAction::Water(WaterAction::GiveDolphinsGrace(EntityId(100)))
            })
            .count();
    }
    assert!(graces >= 1);
    let mut idle = dolphin(World::new());
    let mut standing = living(100, EntityKind::Player, Vec3::new(3.5, 65.0, 0.5));
    standing.facts.swimming = false;
    idle.world.players = vec![standing];
    for _ in 0..10 {
        let (actions, _) = idle.step(&|_| {});
        assert!(
            !actions.iter().any(|action| matches!(
                action,
                MobAction::Water(WaterAction::GiveDolphinsGrace(_))
            ))
        );
    }
}

#[test]
fn a_dolphin_holding_an_item_drops_it_and_one_nearby_swims_to_it() {
    let mut run = dolphin(World::new());
    let held = Some(ItemKind::Cod.id());
    let mut dropped = false;
    for _ in 0..4 {
        let (actions, _) = run.step(&|view| view.main_hand = held);
        dropped |= actions.contains(&MobAction::Water(WaterAction::DropHeldItem));
    }
    assert!(dropped);
    let mut run = dolphin(World::new());
    let item_position = Vec3::new(4.5, 63.0, 0.5);
    run.world.items = vec![DroppedItem {
        id: EntityId(50),
        position: item_position,
        item: ItemKind::Cod.id(),
        bounds: Aabb::of_size(item_position, 0.25, 0.25),
        has_pickup_delay: false,
        alive: true,
    }];
    let mut played = false;
    for _ in 0..8 {
        let (actions, _) = run.step(&|_| {});
        played |= actions.iter().any(|action| {
            matches!(
                action,
                MobAction::Sound { sound, .. } if *sound == "minecraft:entity.dolphin.play"
            )
        });
    }
    assert!(played);
    run.world.items[0].has_pickup_delay = true;
    let (actions, _) = run.step(&|_| {});
    assert!(
        !actions
            .iter()
            .any(|action| matches!(action, MobAction::Sound { .. }))
    );
}

#[test]
fn a_thrown_item_leaves_along_the_view_with_a_small_random_spread() {
    let velocity = thrown_item_velocity(0.0, 0.0, 0.0, 0.5);
    assert!((velocity.z - 0.3).abs() < 1.0e-6);
    assert!((velocity.x - 0.01).abs() < 1.0e-6);
    assert_eq!(velocity.y, 0.0);
    let up = thrown_item_velocity(0.0, -90.0, 0.0, 0.0);
    assert!((up.y + 0.3 * 1.5).abs() < 1.0e-6);
}

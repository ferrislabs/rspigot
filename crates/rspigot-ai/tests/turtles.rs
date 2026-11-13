use std::cell::Cell;
use std::collections::HashMap;

use rspigot_ai::{
    AiWorld, Facts, GoalSettings, Living, MobAction, MobAi, MobBody, MobGoal, MobView, MoveStyle,
    PathType, TargetKind, TurtleAction, TurtleFacts, TurtleGoal,
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
}

impl World {
    fn beach() -> Self {
        Self {
            players: Vec::new(),
            others: Vec::new(),
            blocks: HashMap::new(),
            time: Cell::new(100),
        }
    }
}

impl rspigot_ai::BlockView for World {
    fn state(&self, pos: BlockPos) -> Option<StateId> {
        if let Some(state) = self.blocks.get(&pos) {
            return Some(*state);
        }
        Some(if pos.y <= 63 && pos.x < 10 {
            BlockKind::Sand.default_state()
        } else if pos.y <= 61 {
            BlockKind::Stone.default_state()
        } else if pos.y <= 63 {
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

struct Run {
    ai: MobAi,
    base: EntityBase,
    state: LivingState,
    world: World,
    facts: TurtleFacts,
}

impl Run {
    fn new(position: Vec3, world: World) -> Self {
        let mut base = EntityBase::new(EntityId(1), Uuid::from_u128(1), EntityKind::Turtle);
        base.position = position;
        base.old_position = position;
        base.tick_count = 1;
        base.on_ground = true;
        let state = LivingState::new(&mut base, 0.0);
        Self {
            ai: MobAi::new(EntityKind::Turtle, 7, GoalSettings::default()),
            base,
            state,
            world,
            facts: TurtleFacts {
                home_pos: position.block(),
                ..TurtleFacts::default()
            },
        }
    }

    fn step(&mut self, adjust: &dyn Fn(&mut MobView)) -> (Vec<MobAction>, MobBody) {
        self.base.tick_count += 1;
        self.world.time.set(self.world.time.get() + 1);
        let mut view = MobView::of(&self.base, &self.state);
        view.species.turtle = self.facts;
        adjust(&mut view);
        let mut body = MobBody::of(&view);
        self.ai.think(&view, &self.world, &mut body);
        self.facts = body.turtle;
        (self.ai.take_actions(), body)
    }

    fn run(&mut self, steps: usize, adjust: &dyn Fn(&mut MobView)) -> Vec<MobAction> {
        let mut actions = Vec::new();
        for _ in 0..steps {
            actions.extend(self.step(adjust).0);
        }
        actions
    }
}

fn kind_of(goal: &MobGoal) -> &'static str {
    match goal {
        MobGoal::Turtle(goal) => match goal {
            TurtleGoal::Panic(_) => "panic",
            TurtleGoal::Breed(_) => "breed",
            TurtleGoal::LayEgg(_) => "lay_egg",
            TurtleGoal::GoToWater(_) => "go_to_water",
            TurtleGoal::GoHome(_) => "go_home",
            TurtleGoal::Travel(_) => "travel",
            TurtleGoal::RandomStroll(_) => "stroll",
        },
        MobGoal::Tempt(_) => "tempt",
        MobGoal::LookAtPlayer(_) => "look",
        _ => "other",
    }
}

#[test]
fn the_turtle_registers_the_vanilla_goals_with_their_priorities() {
    let ai = MobAi::new(EntityKind::Turtle, 1, GoalSettings::default());
    let goals: Vec<(i32, &str)> = ai
        .goals()
        .goals()
        .iter()
        .map(|goal| (goal.priority(), kind_of(goal.goal())))
        .collect();
    assert_eq!(
        goals,
        vec![
            (0, "panic"),
            (1, "breed"),
            (1, "lay_egg"),
            (2, "tempt"),
            (3, "go_to_water"),
            (4, "go_home"),
            (7, "travel"),
            (8, "look"),
            (9, "stroll"),
        ]
    );
    assert!(ai.targets().goals().is_empty());
}

#[test]
fn the_turtle_navigates_amphibiously_with_free_water_and_closed_doors() {
    let ai = MobAi::new(EntityKind::Turtle, 1, GoalSettings::default());
    let brain = ai.brain();
    assert!(brain.profile.is_amphibious());
    assert!(!brain.profile.prefers_shallow_swimming());
    assert_eq!(brain.profile.malus(PathType::Water), 0.0);
    assert_eq!(brain.profile.malus(PathType::DoorIronClosed), -1.0);
    assert_eq!(brain.profile.malus(PathType::DoorWoodClosed), -1.0);
    assert_eq!(brain.profile.malus(PathType::DoorOpen), -1.0);
    assert_eq!(brain.profile.malus(PathType::Fire), -1.0);
    assert_eq!(brain.movement.style(), MoveStyle::Turtle);
}

#[test]
fn a_turtle_with_an_egg_lays_it_on_sand_after_two_hundred_ticks_of_digging() {
    let mut run = Run::new(Vec3::new(5.5, 64.0, 5.5), World::beach());
    run.facts.has_egg = true;
    let mut actions = Vec::new();
    let mut laying_from = None;
    for tick in 0..400 {
        let (taken, body) = run.step(&|_| {});
        actions.extend(taken);
        if body.turtle.laying_egg && laying_from.is_none() {
            laying_from = Some(tick);
        }
        if actions
            .iter()
            .any(|action| matches!(action, MobAction::Turtle(TurtleAction::LayEgg { .. })))
        {
            break;
        }
    }
    let laid: Vec<BlockPos> = actions
        .iter()
        .filter_map(|action| match action {
            MobAction::Turtle(TurtleAction::LayEgg { target }) => Some(*target),
            _ => None,
        })
        .collect();
    assert_eq!(laid.len(), 1);
    assert_eq!(laid[0].y, 64);
    assert!(laying_from.is_some());
    assert!(!run.facts.has_egg);
    assert!(!run.facts.laying_egg);
    assert_eq!(run.facts.lay_egg_counter, 0);
}

#[test]
fn the_lay_egg_goal_needs_the_egg_and_a_home_within_nine_blocks() {
    let mut without = Run::new(Vec3::new(5.5, 64.0, 5.5), World::beach());
    let actions = without.run(60, &|_| {});
    assert!(
        !actions
            .iter()
            .any(|action| matches!(action, MobAction::Turtle(_)))
    );
    assert!(!without.facts.laying_egg);
    let mut far = Run::new(Vec3::new(5.5, 64.0, 5.5), World::beach());
    far.facts.has_egg = true;
    far.facts.home_pos = BlockPos::new(5, 64, 40);
    for _ in 0..80 {
        far.step(&|_| {});
        assert!(!far.facts.laying_egg);
    }
}

#[test]
fn a_turtle_with_an_egg_heads_home_and_a_far_walker_stops_within_seven_blocks() {
    let mut egg = Run::new(Vec3::new(5.5, 64.0, 5.5), World::beach());
    egg.facts.has_egg = true;
    egg.facts.home_pos = BlockPos::new(5, 64, 40);
    egg.run(3, &|_| {});
    assert!(egg.facts.going_home);
    let mut run = Run::new(Vec3::new(5.5, 64.0, 5.5), World::beach());
    run.facts.home_pos = BlockPos::new(5, 64, 300);
    for _ in 0..2000 {
        run.step(&|_| {});
        if run.facts.going_home {
            break;
        }
    }
    assert!(run.facts.going_home);
    run.facts.home_pos = BlockPos::new(5, 64, 8);
    run.run(4, &|_| {});
    assert!(!run.facts.going_home);
}

#[test]
fn an_adult_turtle_sometimes_goes_home_when_it_is_sixty_four_blocks_away() {
    let mut run = Run::new(Vec3::new(5.5, 64.0, 5.5), World::beach());
    run.facts.home_pos = BlockPos::new(5, 64, 300);
    let mut started = false;
    for _ in 0..2000 {
        run.step(&|_| {});
        started |= run.facts.going_home;
        if started {
            break;
        }
    }
    assert!(started);
    let mut near = Run::new(Vec3::new(5.5, 64.0, 5.5), World::beach());
    near.facts.home_pos = BlockPos::new(5, 64, 30);
    for _ in 0..2000 {
        near.step(&|_| {});
        assert!(!near.facts.going_home);
    }
}

#[test]
fn a_baby_turtle_never_goes_home() {
    let mut run = Run::new(Vec3::new(5.5, 64.0, 5.5), World::beach());
    run.facts.home_pos = BlockPos::new(5, 64, 300);
    run.facts.has_egg = true;
    for _ in 0..200 {
        run.step(&|view| view.baby = true);
        assert!(!run.facts.going_home);
    }
}

#[test]
fn a_turtle_swimming_in_the_sea_picks_a_far_travel_target_at_or_below_sea_level() {
    let mut world = World::beach();
    for x in 10..60 {
        for z in -40..40 {
            for y in 50..=63 {
                world.blocks.insert(
                    BlockPos::new(x, y, z),
                    if y == 50 {
                        BlockKind::Sand.default_state()
                    } else {
                        BlockKind::Water.default_state()
                    },
                );
            }
        }
    }
    let mut run = Run::new(Vec3::new(20.5, 60.0, 0.5), world);
    run.base.on_ground = false;
    run.base.fluid.in_water = true;
    run.base.fluid.water_height = 1.0;
    run.run(3, &|view| view.fluid.in_water = true);
    let target = run.facts.travel_pos.expect("a travel target");
    assert!(target.y <= 63);
    assert!((target.x - 20).abs() <= 512 && (target.z).abs() <= 512);
}

#[test]
fn the_travel_goal_does_not_run_with_an_egg_or_on_land() {
    let mut run = Run::new(Vec3::new(5.5, 64.0, 5.5), World::beach());
    run.run(20, &|_| {});
    assert!(run.facts.travel_pos.is_none());
}

#[test]
fn a_turtle_on_a_beach_heads_for_the_water_when_it_has_no_egg() {
    let mut run = Run::new(Vec3::new(8.5, 64.0, 0.5), World::beach());
    run.facts.home_pos = BlockPos::new(8, 64, 0);
    run.run(5, &|_| {});
    assert!(run.ai.brain().navigation.is_in_progress());
    let wanted = run
        .ai
        .brain()
        .navigation
        .path()
        .and_then(|path| path.end_node().map(|node| node.pos))
        .expect("a path");
    assert!(wanted.x >= 9 || wanted.y <= 63);
}

#[test]
fn a_turtle_with_an_egg_does_not_go_to_the_water() {
    let mut run = Run::new(Vec3::new(8.5, 64.0, 0.5), World::beach());
    run.facts.has_egg = true;
    run.facts.home_pos = BlockPos::new(8, 64, 0);
    for _ in 0..30 {
        let (_, body) = run.step(&|_| {});
        assert!(!body.turtle.travel_pos.is_some());
    }
}

#[test]
fn the_stroll_goal_waits_while_the_turtle_has_an_egg_or_is_going_home() {
    let mut run = Run::new(Vec3::new(5.5, 64.0, 5.5), World::beach());
    run.facts.has_egg = true;
    run.facts.home_pos = BlockPos::new(5, 64, 5);
    for _ in 0..400 {
        run.step(&|_| {});
        let running_stroll = run.ai.goals().goals().iter().any(|goal| {
            goal.is_running() && matches!(goal.goal(), MobGoal::Turtle(TurtleGoal::RandomStroll(_)))
        });
        assert!(!running_stroll);
    }
}

#[test]
fn a_panicking_turtle_runs_to_water_within_seven_blocks() {
    let mut run = Run::new(Vec3::new(8.5, 64.0, 0.5), World::beach());
    run.run(3, &|view| view.panic = true);
    let running = run.ai.goals().goals().iter().any(|goal| {
        goal.is_running() && matches!(goal.goal(), MobGoal::Turtle(TurtleGoal::Panic(_)))
    });
    assert!(running);
    assert!(run.ai.brain().navigation.is_in_progress());
}

#[test]
fn only_a_turtle_without_an_egg_breeds() {
    let mut run = Run::new(Vec3::new(5.5, 64.0, 5.5), World::beach());
    let mut mate = living(2, EntityKind::Turtle, Vec3::new(6.5, 64.0, 5.5));
    mate.in_love = true;
    run.world.others = vec![mate];
    run.facts.has_egg = true;
    for _ in 0..5 {
        let (actions, _) = run.step(&|view| view.in_love = true);
        assert!(
            !actions
                .iter()
                .any(|action| matches!(action, MobAction::Breed(_)))
        );
    }
    let mut free = Run::new(Vec3::new(5.5, 64.0, 5.5), World::beach());
    let mut mate = living(2, EntityKind::Turtle, Vec3::new(5.9, 64.0, 5.5));
    mate.in_love = true;
    free.world.others = vec![mate];
    let mut bred = false;
    for _ in 0..80 {
        let (actions, _) = free.step(&|view| view.in_love = true);
        bred |= actions
            .iter()
            .any(|action| matches!(action, MobAction::Breed(EntityId(2))));
    }
    assert!(bred);
}

#[test]
fn the_walk_target_prefers_water_when_not_going_home_and_sand_always() {
    let world = World::beach();
    let facts = TurtleFacts::default();
    let sea = BlockPos::new(20, 63, 0);
    let beach = BlockPos::new(5, 64, 0);
    let grass = BlockPos::new(20, 63 + 1, 0);
    assert_eq!(
        rspigot_ai::turtle_walk_target_value(&world, &facts, sea),
        10.0
    );
    assert_eq!(
        rspigot_ai::turtle_walk_target_value(&world, &facts, beach),
        10.0
    );
    let homing = TurtleFacts {
        going_home: true,
        ..facts
    };
    assert!(rspigot_ai::turtle_walk_target_value(&world, &homing, sea) < 10.0);
    assert_eq!(
        rspigot_ai::turtle_walk_target_value(&world, &homing, beach),
        10.0
    );
    assert!(rspigot_ai::turtle_walk_target_value(&world, &facts, grass) < 10.0);
}

#[test]
fn a_travelling_turtle_only_accepts_water_as_a_stable_destination() {
    let world = World::beach();
    let travelling = TurtleFacts {
        travel_pos: Some(BlockPos::new(30, 63, 0)),
        ..TurtleFacts::default()
    };
    let idle = TurtleFacts::default();
    assert!(rspigot_ai::turtle_is_stable_destination(
        &world,
        &travelling,
        BlockPos::new(30, 63, 0)
    ));
    assert!(!rspigot_ai::turtle_is_stable_destination(
        &world,
        &travelling,
        BlockPos::new(5, 64, 0)
    ));
    assert!(rspigot_ai::turtle_is_stable_destination(
        &world,
        &idle,
        BlockPos::new(5, 64, 0)
    ));
    assert!(!rspigot_ai::turtle_is_stable_destination(
        &world,
        &idle,
        BlockPos::new(30, 70, 0)
    ));
}

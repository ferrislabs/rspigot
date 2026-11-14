use std::cell::Cell;
use std::collections::HashMap;

use rspigot_ai::{
    AiWorld, DrownedFacts, DrownedGoal, Facts, GoalSettings, Living, MobAction, MobAi, MobBody,
    MobGoal, MobView, MoveStyle, PathType, TargetKind,
};
use rspigot_blocks::{BlockKind, StateId};
use rspigot_domain::entity::living::LivingState;
use rspigot_domain::entity::{Aabb, EntityBase};
use rspigot_domain::{BlockPos, EntityId, Vec3};
use rspigot_entities::EntityKind;
use rspigot_items::ItemKind;
use uuid::Uuid;

struct World {
    others: Vec<Living>,
    blocks: HashMap<BlockPos, StateId>,
    bright: bool,
    time: Cell<i64>,
}

impl World {
    fn shore(bright: bool) -> Self {
        Self {
            others: Vec::new(),
            blocks: HashMap::new(),
            bright,
            time: Cell::new(100),
        }
    }
}

impl rspigot_ai::BlockView for World {
    fn state(&self, pos: BlockPos) -> Option<StateId> {
        if let Some(state) = self.blocks.get(&pos) {
            return Some(*state);
        }
        Some(if pos.x >= 0 {
            if pos.y <= 63 {
                BlockKind::Sand.default_state()
            } else {
                StateId::AIR
            }
        } else if pos.y <= 40 {
            BlockKind::Stone.default_state()
        } else if pos.y <= 62 {
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
        2
    }

    fn game_time(&self) -> i64 {
        self.time.get()
    }

    fn raw_brightness(&self, _pos: BlockPos) -> i32 {
        15
    }

    fn bright_outside(&self) -> bool {
        self.bright
    }
}

fn living(id: i32, kind: EntityKind, position: Vec3, in_water: bool) -> Living {
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
        in_water,
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
    facts: DrownedFacts,
}

impl Run {
    fn new(position: Vec3, in_water: bool, world: World) -> Self {
        let mut base = EntityBase::new(EntityId(1), Uuid::from_u128(1), EntityKind::Drowned);
        base.position = position;
        base.old_position = position;
        base.tick_count = 1;
        base.fluid.in_water = in_water;
        base.on_ground = !in_water;
        let state = LivingState::new(&mut base, 0.0);
        Self {
            ai: MobAi::new(EntityKind::Drowned, 7, GoalSettings::default()),
            base,
            state,
            world,
            facts: DrownedFacts::default(),
        }
    }

    fn step(&mut self, adjust: &dyn Fn(&mut MobView)) -> (Vec<MobAction>, MobBody) {
        self.base.tick_count += 1;
        self.world.time.set(self.world.time.get() + 1);
        let mut view = MobView::of(&self.base, &self.state);
        view.species.drowned = self.facts;
        adjust(&mut view);
        let mut body = MobBody::of(&view);
        self.ai.think(&view, &self.world, &mut body);
        self.facts = body.drowned;
        (self.ai.take_actions(), body)
    }

    fn run(&mut self, steps: usize, adjust: &dyn Fn(&mut MobView)) -> Vec<MobBody> {
        (0..steps).map(|_| self.step(adjust).1).collect()
    }
}

fn kind_of(goal: &MobGoal) -> &'static str {
    match goal {
        MobGoal::Drowned(goal) => match goal {
            DrownedGoal::GoToWater(_) => "go_to_water",
            DrownedGoal::TridentAttack(_) => "trident",
            DrownedGoal::Attack(_) => "attack",
            DrownedGoal::GoToBeach(_) => "go_to_beach",
            DrownedGoal::SwimUp(_) => "swim_up",
        },
        MobGoal::RemoveTurtleEgg(_) => "turtle_egg",
        MobGoal::LookAtPlayer(_) => "look",
        MobGoal::RandomLookAround(_) => "look_around",
        MobGoal::Stroll(_) => "stroll",
        MobGoal::HurtByTarget(_) => "hurt_by",
        MobGoal::NearestAttackableTarget(_) => "nearest",
        _ => "other",
    }
}

type Listing = Vec<(i32, &'static str)>;

fn lists() -> (Listing, Listing) {
    let ai = MobAi::new(EntityKind::Drowned, 1, GoalSettings::default());
    let map = |goals: &[rspigot_ai::WrappedGoal<MobGoal>]| {
        goals
            .iter()
            .map(|goal| (goal.priority(), kind_of(goal.goal())))
            .collect()
    };
    (map(ai.goals().goals()), map(ai.targets().goals()))
}

#[test]
fn the_drowned_registers_the_vanilla_goals_and_targets() {
    let (goals, targets) = lists();
    assert_eq!(
        goals,
        vec![
            (4, "turtle_egg"),
            (8, "look"),
            (8, "look_around"),
            (1, "go_to_water"),
            (2, "trident"),
            (2, "attack"),
            (5, "go_to_beach"),
            (6, "swim_up"),
            (7, "stroll"),
        ]
    );
    assert_eq!(
        targets,
        vec![
            (1, "hurt_by"),
            (2, "nearest"),
            (3, "nearest"),
            (3, "nearest"),
            (3, "nearest"),
            (5, "nearest"),
        ]
    );
}

#[test]
fn the_drowned_navigates_amphibiously_with_free_water_and_its_own_control() {
    let ai = MobAi::new(EntityKind::Drowned, 1, GoalSettings::default());
    let brain = ai.brain();
    assert!(brain.profile.is_amphibious());
    assert!(!brain.profile.prefers_shallow_swimming());
    assert_eq!(brain.profile.malus(PathType::Water), 0.0);
    assert_eq!(brain.movement.style(), MoveStyle::Drowned);
}

fn running(run: &Run) -> Vec<&'static str> {
    run.ai
        .goals()
        .goals()
        .iter()
        .filter(|goal| goal.is_running())
        .map(|goal| kind_of(goal.goal()))
        .collect()
}

#[test]
fn in_daylight_a_drowned_on_land_heads_for_the_nearest_water() {
    let mut run = Run::new(Vec3::new(4.5, 64.0, 0.5), false, World::shore(true));
    run.run(6, &|_| {});
    assert!(running(&run).contains(&"go_to_water"));
    assert!(run.ai.brain().navigation.is_in_progress());
    let mut dark = Run::new(Vec3::new(4.5, 64.0, 0.5), false, World::shore(false));
    dark.run(6, &|_| {});
    assert!(!running(&dark).contains(&"go_to_water"));
}

#[test]
fn at_night_a_drowned_near_the_surface_heads_for_the_beach_and_stops_searching_for_land() {
    let mut run = Run::new(Vec3::new(-3.5, 62.0, 0.5), true, World::shore(false));
    run.run(6, &|view| view.fluid.in_water = true);
    assert!(running(&run).contains(&"go_to_beach"));
    assert!(!run.facts.searching_for_land);
}

#[test]
fn at_night_a_deep_drowned_swims_up_searching_for_land_and_stops_once_shallow() {
    let mut run = Run::new(Vec3::new(-6.5, 50.0, 0.5), true, World::shore(false));
    run.run(6, &|view| view.fluid.in_water = true);
    assert!(running(&run).contains(&"swim_up"));
    assert!(run.facts.searching_for_land);
    run.base.position = Vec3::new(-6.5, 62.0, 0.5);
    run.run(4, &|view| view.fluid.in_water = true);
    assert!(!running(&run).contains(&"swim_up"));
    assert!(!run.facts.searching_for_land);
}

#[test]
fn by_day_the_drowned_only_attacks_targets_that_are_in_water() {
    let mut dry = Run::new(Vec3::new(-6.5, 60.0, 0.5), true, World::shore(true));
    dry.world.others.push(living(
        2,
        EntityKind::Player,
        Vec3::new(4.5, 64.0, 0.5),
        false,
    ));
    for _ in 0..40 {
        dry.step(&|view| view.fluid.in_water = true);
    }
    assert!(dry.ai.target().is_none());
    let mut wet = Run::new(Vec3::new(-6.5, 60.0, 0.5), true, World::shore(true));
    wet.world.others.push(living(
        2,
        EntityKind::Player,
        Vec3::new(-12.5, 60.0, 0.5),
        true,
    ));
    for _ in 0..40 {
        wet.step(&|view| view.fluid.in_water = true);
    }
    assert_eq!(wet.ai.target(), Some(EntityId(2)));
    let mut night = Run::new(Vec3::new(4.5, 64.0, 0.5), false, World::shore(false));
    night.world.others.push(living(
        2,
        EntityKind::Player,
        Vec3::new(8.5, 64.0, 0.5),
        false,
    ));
    for _ in 0..40 {
        night.step(&|_| {});
    }
    assert_eq!(night.ai.target(), Some(EntityId(2)));
}

#[test]
fn the_trident_goal_needs_a_trident_in_hand_and_raises_it_while_attacking() {
    let mut run = Run::new(Vec3::new(-6.5, 60.0, 0.5), true, World::shore(false));
    run.world.others.push(living(
        2,
        EntityKind::Player,
        Vec3::new(-14.5, 60.0, 0.5),
        true,
    ));
    run.ai.set_target(Some(EntityId(2)));
    let (mut thrown, mut raised) = (0, false);
    for _ in 0..80 {
        let (actions, body) = run.step(&|view| {
            view.fluid.in_water = true;
            view.main_hand = Some(ItemKind::Trident.id());
        });
        thrown += actions
            .iter()
            .filter(|action| matches!(action, MobAction::RangedAttack { .. }))
            .count();
        raised |= body.aggressive && body.using_item.is_some();
    }
    assert!(running(&run).contains(&"trident"));
    assert!(raised);
    assert!(thrown >= 1, "{thrown}");
    let mut bare = Run::new(Vec3::new(-6.5, 60.0, 0.5), true, World::shore(false));
    bare.world.others.push(living(
        2,
        EntityKind::Player,
        Vec3::new(-14.5, 60.0, 0.5),
        true,
    ));
    bare.ai.set_target(Some(EntityId(2)));
    for _ in 0..80 {
        let (actions, _) = bare.step(&|view| view.fluid.in_water = true);
        assert!(
            !actions
                .iter()
                .any(|action| matches!(action, MobAction::RangedAttack { .. }))
        );
    }
    assert!(!running(&bare).contains(&"trident"));
}

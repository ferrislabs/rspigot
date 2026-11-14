use std::cell::Cell;

use rspigot_ai::{
    AiContext, AiWorld, Brain, CubeGoal, CubeSearchForItemsGoal, DroppedItem, Facts, Flag, Flags,
    Goal, GoalSettings, Living, MobAi, MobBody, MobGoal, MobView, TargetKind, TemptGoal,
    TemptItems,
};
use rspigot_blocks::{BlockKind, StateId};
use rspigot_domain::entity::living::LivingState;
use rspigot_domain::entity::species::cube;
use rspigot_domain::entity::{Aabb, EntityBase};
use rspigot_domain::{BlockPos, EntityId, Vec3};
use rspigot_entities::EntityKind;
use uuid::Uuid;

const PLAYER: EntityId = EntityId(100);
const ITEM: EntityId = EntityId(200);
const ORIGIN: Vec3 = Vec3::new(0.5, 64.0, 0.5);

struct World {
    players: Vec<Living>,
    others: Vec<Living>,
    items: Vec<DroppedItem>,
    time: Cell<i64>,
}

impl World {
    fn new() -> Self {
        Self {
            players: Vec::new(),
            others: Vec::new(),
            items: Vec::new(),
            time: Cell::new(0),
        }
    }
}

impl rspigot_ai::BlockView for World {
    fn state(&self, pos: BlockPos) -> Option<StateId> {
        Some(if pos.y < 64 {
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

    fn items_in(&self, area: &Aabb) -> Vec<DroppedItem> {
        self.items
            .iter()
            .filter(|item| item.bounds.intersects(area))
            .copied()
            .collect()
    }

    fn dropped_item(&self, id: EntityId) -> Option<DroppedItem> {
        self.items.iter().find(|item| item.id == id).copied()
    }
}

fn living(id: EntityId, kind: EntityKind, position: Vec3) -> Living {
    let size = kind.dimensions();
    Living {
        id,
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

fn cube_mob(size: i32) -> (EntityBase, LivingState) {
    let mut base = EntityBase::new(EntityId(1), Uuid::from_u128(1), EntityKind::SulfurCube);
    base.position = ORIGIN;
    base.old_position = ORIGIN;
    base.on_ground = true;
    base.tick_count = 1;
    let mut state = LivingState::new(&mut base, 0.0);
    cube::set_size(&mut base, &mut state, size, true);
    (base, state)
}

fn view_of(base: &EntityBase, state: &LivingState) -> MobView {
    MobView::of(base, state)
}

struct Rig {
    mob: MobView,
    brain: Brain,
    body: MobBody,
}

impl Rig {
    fn new(size: i32, seed: i64) -> Self {
        let (base, state) = cube_mob(size);
        let mob = view_of(&base, &state);
        let body = MobBody::of(&mob);
        Self {
            mob,
            brain: Brain::new(EntityKind::SulfurCube, seed),
            body,
        }
    }

    fn context<'a>(&'a mut self, world: &'a World) -> AiContext<'a, World> {
        AiContext {
            mob: &self.mob,
            world,
            brain: &mut self.brain,
            body: &mut self.body,
        }
    }
}

fn label(goal: &MobGoal) -> &'static str {
    match goal {
        MobGoal::Cube(CubeGoal::Float(_)) => "float",
        MobGoal::Cube(CubeGoal::RandomDirection(_)) => "random_direction",
        MobGoal::Cube(CubeGoal::KeepOnJumping(_)) => "keep_on_jumping",
        MobGoal::Cube(CubeGoal::SearchForItems(_)) => "search_for_items",
        MobGoal::Tempt(_) => "tempt",
        _ => "other",
    }
}

fn labels(ai: &MobAi) -> Vec<(i32, &'static str)> {
    ai.goals()
        .goals()
        .iter()
        .map(|goal| (goal.priority(), label(goal.goal())))
        .collect()
}

fn tempt() -> MobGoal {
    MobGoal::Tempt(TemptGoal::sulfur_cube(1.0, 1.0))
}

fn search() -> MobGoal {
    MobGoal::Cube(CubeGoal::SearchForItems(CubeSearchForItemsGoal::default()))
}

fn flags(goal: &MobGoal) -> Flags {
    Goal::<AiContext<'_, World>>::flags(goal)
}

fn item_named(name: &str) -> rspigot_domain::ItemId {
    rspigot_items::ItemKind::from_name(name).unwrap().id()
}

fn dropped(id: EntityId, name: &str, position: Vec3, pickup_delay: bool) -> DroppedItem {
    DroppedItem {
        id,
        position,
        item: item_named(name),
        bounds: Aabb::of_size(position, 0.25, 0.25),
        has_pickup_delay: pickup_delay,
        alive: true,
    }
}

fn player_holding(name: Option<&str>, position: Vec3) -> Living {
    let mut player = living(PLAYER, EntityKind::Player, position);
    player.main_hand = name.map(item_named);
    player
}

#[test]
fn the_sulfur_cube_registers_the_vanilla_goals_and_no_targets() {
    let ai = MobAi::new(EntityKind::SulfurCube, 1, GoalSettings::default());
    assert_eq!(
        labels(&ai),
        vec![
            (1, "float"),
            (2, "tempt"),
            (3, "search_for_items"),
            (4, "random_direction"),
            (5, "keep_on_jumping"),
        ]
    );
    assert!(ai.targets().goals().is_empty());
    assert!(ai.can_float());
    assert!(ai.has_ai());
}

#[test]
fn the_tempt_and_search_goals_only_claim_the_look_flag() {
    assert_eq!(flags(&tempt()), Flags::of(Flag::Look));
    assert_eq!(flags(&search()), Flags::of(Flag::Look));
    assert_eq!(
        flags(&MobGoal::Tempt(TemptGoal::new(1.0, TemptItems::SulfurCube))),
        Flags::of(Flag::Move).with(Flag::Look)
    );
}

#[test]
fn an_adult_follows_swallowable_blocks_and_a_baby_follows_slime_balls() {
    let mut world = World::new();
    let spot = Vec3::new(4.5, 64.0, 0.5);
    let mut adult = Rig::new(2, 5);
    let mut baby = Rig::new(1, 5);
    baby.mob.baby = true;
    let mut goal = tempt();
    for (holding, adult_wants, baby_wants) in [
        (Some("minecraft:oak_planks"), true, false),
        (Some("minecraft:tnt"), true, false),
        (Some("minecraft:slime_ball"), false, true),
        (Some("minecraft:diamond"), false, false),
        (None, false, false),
    ] {
        world.players = vec![player_holding(holding, spot)];
        assert_eq!(
            Goal::can_use(&mut goal, &mut adult.context(&world)),
            adult_wants,
            "{holding:?}"
        );
        assert_eq!(
            Goal::can_use(&mut tempt(), &mut baby.context(&world)),
            baby_wants,
            "{holding:?}"
        );
    }
}

#[test]
fn the_tempt_range_is_the_eight_block_attribute() {
    let mut world = World::new();
    let mut rig = Rig::new(2, 5);
    assert_eq!(rig.mob.tempt_range, 8.0);
    world.players = vec![player_holding(
        Some("minecraft:dirt"),
        Vec3::new(9.0, 64.0, 0.5),
    )];
    assert!(!Goal::can_use(&mut tempt(), &mut rig.context(&world)));
    world.players[0].position = Vec3::new(8.0, 64.0, 0.5);
    assert!(Goal::can_use(&mut tempt(), &mut rig.context(&world)));
}

#[test]
fn the_tempted_cube_stops_hopping_within_a_block_and_otherwise_walks_to_the_player() {
    let mut world = World::new();
    let mut rig = Rig::new(2, 5);
    world.players = vec![player_holding(
        Some("minecraft:dirt"),
        Vec3::new(0.5, 64.0, 0.9),
    )];
    let mut goal = tempt();
    assert!(Goal::can_use(&mut goal, &mut rig.context(&world)));
    Goal::start(&mut goal, &mut rig.context(&world));
    Goal::tick(&mut goal, &mut rig.context(&world));
    assert_eq!(rig.brain.movement.speed_modifier(), 0.0);
    assert!(rig.brain.movement.has_wanted());
    assert!(rig.brain.look.is_looking_at_target());
    world.players[0].position = Vec3::new(5.5, 64.0, 0.5);
    rig.brain.movement = Brain::new(EntityKind::SulfurCube, 5).movement;
    Goal::tick(&mut goal, &mut rig.context(&world));
    assert!(rig.brain.navigation.is_in_progress());
    assert_eq!(rig.brain.movement.speed_modifier(), 0.0);
    Goal::stop(&mut goal, &mut rig.context(&world));
    assert_eq!(rig.brain.movement.speed_modifier(), 0.0);
    assert!(!Goal::can_use(&mut goal, &mut rig.context(&world)));
}

#[test]
fn the_search_goal_picks_the_nearest_swallowable_item_without_a_pickup_delay() {
    let mut world = World::new();
    let mut rig = Rig::new(2, 5);
    let mut goal = search();
    assert!(!Goal::can_use(&mut goal, &mut rig.context(&world)));
    world.items = vec![
        dropped(
            EntityId(201),
            "minecraft:diamond",
            Vec3::new(1.5, 64.0, 0.5),
            false,
        ),
        dropped(
            EntityId(202),
            "minecraft:dirt",
            Vec3::new(2.5, 64.0, 0.5),
            true,
        ),
        dropped(
            EntityId(203),
            "minecraft:dirt",
            Vec3::new(6.5, 64.0, 0.5),
            false,
        ),
        dropped(
            EntityId(204),
            "minecraft:tnt",
            Vec3::new(4.5, 64.0, 0.5),
            false,
        ),
    ];
    assert!(Goal::can_use(&mut goal, &mut rig.context(&world)));
    let MobGoal::Cube(CubeGoal::SearchForItems(state)) = &goal else {
        panic!("not the search goal");
    };
    assert_eq!(state.target(), Some(EntityId(204)));
    world.items[3].position = Vec3::new(20.0, 64.0, 0.5);
    world.items[3].bounds = Aabb::of_size(world.items[3].position, 0.25, 0.25);
    assert!(Goal::can_use(&mut goal, &mut rig.context(&world)));
    let MobGoal::Cube(CubeGoal::SearchForItems(state)) = &goal else {
        panic!("not the search goal");
    };
    assert_eq!(state.target(), Some(EntityId(203)));
}

#[test]
fn the_search_goal_ignores_items_a_baby_or_a_recently_shorn_cube_cannot_take() {
    let mut world = World::new();
    world.items = vec![dropped(
        ITEM,
        "minecraft:dirt",
        Vec3::new(2.5, 64.0, 0.5),
        false,
    )];
    let mut baby = Rig::new(1, 5);
    baby.mob.baby = true;
    assert!(!Goal::can_use(&mut search(), &mut baby.context(&world)));
    let mut shorn = Rig::new(2, 5);
    shorn.mob.species.cube.pickup_timer = 1;
    assert!(!Goal::can_use(&mut search(), &mut shorn.context(&world)));
    shorn.mob.species.cube.pickup_timer = 0;
    assert!(Goal::can_use(&mut search(), &mut shorn.context(&world)));
}

#[test]
fn the_search_goal_faces_the_item_and_hops_aggressively_towards_it() {
    let mut world = World::new();
    world.items = vec![dropped(
        ITEM,
        "minecraft:dirt",
        Vec3::new(5.5, 64.0, 0.5),
        false,
    )];
    let mut rig = Rig::new(2, 5);
    let mut goal = search();
    assert!(Goal::can_use(&mut goal, &mut rig.context(&world)));
    Goal::tick(&mut goal, &mut rig.context(&world));
    assert_eq!(rig.body.yaw, -10.0);
    assert!(rig.brain.movement.cube().aggressive);
    assert_eq!(rig.brain.movement.cube().y_rot, rig.body.yaw);
}

#[test]
fn the_search_goal_continues_only_while_it_would_start() {
    let mut world = World::new();
    world.items = vec![dropped(
        ITEM,
        "minecraft:dirt",
        Vec3::new(2.5, 64.0, 0.5),
        false,
    )];
    let mut rig = Rig::new(2, 5);
    let mut goal = search();
    assert!(Goal::can_use(&mut goal, &mut rig.context(&world)));
    assert!(Goal::can_continue_to_use(
        &mut goal,
        &mut rig.context(&world)
    ));
    world.items.clear();
    assert!(!Goal::can_continue_to_use(
        &mut goal,
        &mut rig.context(&world)
    ));
}

fn think(
    ai: &mut MobAi,
    base: &EntityBase,
    state: &LivingState,
    has_body_item: bool,
    world: &World,
) {
    let mut mob = view_of(base, state);
    mob.species.cube.has_body_item = has_body_item;
    let mut body = MobBody::of(&mob);
    ai.think(&mob, world, &mut body);
}

#[test]
fn swallowing_a_block_removes_every_goal_and_dropping_it_registers_them_again() {
    let world = World::new();
    let (base, state) = cube_mob(2);
    let mut ai = MobAi::new(EntityKind::SulfurCube, 3, GoalSettings::default());
    think(&mut ai, &base, &state, false, &world);
    assert_eq!(ai.goals().goals().len(), 5);
    think(&mut ai, &base, &state, true, &world);
    assert!(ai.goals().goals().is_empty());
    think(&mut ai, &base, &state, true, &world);
    assert!(ai.goals().goals().is_empty());
    think(&mut ai, &base, &state, false, &world);
    assert_eq!(
        labels(&ai)
            .iter()
            .map(|(priority, _)| *priority)
            .collect::<Vec<_>>(),
        vec![1, 2, 3, 4, 5]
    );
}

#[test]
fn a_cube_loaded_with_a_block_loses_its_goals_on_the_first_tick() {
    let world = World::new();
    let (base, state) = cube_mob(2);
    let mut ai = MobAi::new(EntityKind::SulfurCube, 3, GoalSettings::default());
    assert_eq!(ai.goals().goals().len(), 5);
    think(&mut ai, &base, &state, true, &world);
    assert!(ai.goals().goals().is_empty());
}

#[test]
fn a_cube_with_a_block_ignores_its_move_control_and_snaps_its_yaw_to_a_quarter_turn() {
    let world = World::new();
    let (mut base, mut state) = cube_mob(2);
    let mut ai = MobAi::new(EntityKind::SulfurCube, 3, GoalSettings::default());
    think(&mut ai, &base, &state, true, &world);
    ai.brain_mut().movement.set_cube_direction(150.0, false);
    ai.brain_mut().movement.set_cube_wanted(1.0);
    base.set_yaw(100.0);
    ai.act(&mut base, &mut state, &world);
    assert_eq!(base.yaw, 90.0);
    assert_eq!(base.head_yaw, 90.0);
    assert_eq!(state.speed, 0.0);
    base.set_yaw(140.0);
    ai.act(&mut base, &mut state, &world);
    assert_eq!(base.yaw, 180.0);
}

#[test]
fn a_cube_without_a_block_turns_towards_its_direction_like_a_slime() {
    let world = World::new();
    let (mut base, mut state) = cube_mob(2);
    let mut ai = MobAi::new(EntityKind::SulfurCube, 3, GoalSettings::default());
    think(&mut ai, &base, &state, false, &world);
    ai.brain_mut().movement.set_cube_direction(150.0, false);
    ai.act(&mut base, &mut state, &world);
    assert_eq!(base.yaw, 90.0);
    ai.act(&mut base, &mut state, &world);
    assert_eq!(base.yaw, 150.0);
}

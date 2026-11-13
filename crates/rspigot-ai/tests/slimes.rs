use std::cell::Cell;

use rspigot_ai::{
    AiContext, AiWorld, Brain, CubeAttackGoal, CubeFloatGoal, CubeGoal, CubeKeepOnJumpingGoal,
    CubeRandomDirectionGoal, Facts, Flag, Flags, Goal, GoalSettings, Living, MobAi, MobBody,
    MobGoal, MobRandom, MobView, RandomSource, TargetKind,
};
use rspigot_blocks::{BlockKind, StateId};
use rspigot_domain::entity::living::LivingState;
use rspigot_domain::entity::species::cube;
use rspigot_domain::entity::{Aabb, EntityBase};
use rspigot_domain::{BlockPos, EntityId, Vec3};
use rspigot_entities::EntityKind;
use uuid::Uuid;

const PLAYER: EntityId = EntityId(100);
const ORIGIN: Vec3 = Vec3::new(0.5, 64.0, 0.5);

struct World {
    players: Vec<Living>,
    others: Vec<Living>,
    time: Cell<i64>,
}

impl World {
    fn new() -> Self {
        Self {
            players: Vec::new(),
            others: Vec::new(),
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

fn cube_mob(kind: EntityKind, size: i32) -> (EntityBase, LivingState) {
    let mut base = EntityBase::new(EntityId(1), Uuid::from_u128(1), kind);
    base.position = ORIGIN;
    base.old_position = ORIGIN;
    base.on_ground = true;
    base.tick_count = 1;
    let mut state = LivingState::new(&mut base, 0.0);
    cube::set_size(&mut base, &mut state, size, true);
    (base, state)
}

fn view_of(base: &EntityBase, state: &LivingState) -> MobView {
    let mut view = MobView::of(base, state);
    view.species.cube.can_deal_damage = cube::can_deal_damage(base, true);
    view
}

struct Rig {
    mob: MobView,
    brain: Brain,
    body: MobBody,
}

impl Rig {
    fn new(kind: EntityKind, size: i32, seed: i64) -> Self {
        let (base, state) = cube_mob(kind, size);
        let mob = view_of(&base, &state);
        let body = MobBody::of(&mob);
        Self {
            mob,
            brain: Brain::new(kind, seed),
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
        MobGoal::Cube(CubeGoal::Attack(_)) => "attack",
        MobGoal::Cube(CubeGoal::KeepOnJumping(_)) => "keep_on_jumping",
        MobGoal::NearestAttackableTarget(_) => "nearest_target",
        _ => "other",
    }
}

fn float() -> MobGoal {
    MobGoal::Cube(CubeGoal::Float(CubeFloatGoal))
}

fn direction() -> MobGoal {
    MobGoal::Cube(CubeGoal::RandomDirection(CubeRandomDirectionGoal::default()))
}

fn attack() -> MobGoal {
    MobGoal::Cube(CubeGoal::Attack(CubeAttackGoal::default()))
}

fn keep_jumping() -> MobGoal {
    MobGoal::Cube(CubeGoal::KeepOnJumping(CubeKeepOnJumpingGoal))
}

fn flags(goal: &MobGoal) -> Flags {
    Goal::<AiContext<'_, World>>::flags(goal)
}

fn every_tick(goal: &MobGoal) -> bool {
    Goal::<AiContext<'_, World>>::requires_update_every_tick(goal)
}

#[test]
fn slimes_and_magma_cubes_register_the_vanilla_goals_and_targets() {
    for kind in [EntityKind::Slime, EntityKind::MagmaCube] {
        let ai = MobAi::new(kind, 1, GoalSettings::default());
        let goals: Vec<_> = ai
            .goals()
            .goals()
            .iter()
            .map(|goal| (goal.priority(), label(goal.goal())))
            .collect();
        assert_eq!(
            goals,
            vec![
                (1, "float"),
                (4, "random_direction"),
                (5, "keep_on_jumping"),
                (2, "attack"),
            ],
            "{kind:?}"
        );
        let targets: Vec<_> = ai
            .targets()
            .goals()
            .iter()
            .map(|goal| (goal.priority(), label(goal.goal())))
            .collect();
        assert_eq!(targets, vec![(1, "nearest_target"), (3, "nearest_target")]);
        assert!(ai.can_float());
        assert!(ai.has_ai());
    }
}

#[test]
fn the_cube_goals_claim_the_vanilla_flags_and_update_cadence() {
    assert_eq!(flags(&float()), Flags::of(Flag::Jump).with(Flag::Move));
    assert_eq!(flags(&direction()), Flags::of(Flag::Look));
    assert_eq!(flags(&attack()), Flags::of(Flag::Look));
    assert_eq!(
        flags(&keep_jumping()),
        Flags::of(Flag::Jump).with(Flag::Move)
    );
    assert!(every_tick(&float()));
    assert!(!every_tick(&direction()));
    assert!(every_tick(&attack()));
    assert!(!every_tick(&keep_jumping()));
}

#[test]
fn the_float_goal_needs_a_liquid_and_jumps_eighty_percent_of_the_time_at_one_point_two() {
    let world = World::new();
    let mut rig = Rig::new(EntityKind::Slime, 2, 5);
    let mut goal = float();
    assert!(!Goal::can_use(&mut goal, &mut rig.context(&world)));
    rig.mob.fluid.in_water = true;
    rig.mob.fluid.water_height = 0.5;
    assert!(Goal::can_use(&mut goal, &mut rig.context(&world)));
    let mut jumps = 0;
    let mut expected = MobRandom::new(5);
    let mut rolls = 0;
    for _ in 0..200 {
        let before = rig.brain.jump.is_jumping();
        assert!(!before);
        rolls += 1;
        let roll = expected.next_float();
        Goal::tick(&mut goal, &mut rig.context(&world));
        let jumped = rig.brain.jump.take();
        assert_eq!(jumped, roll < 0.8);
        jumps += i32::from(jumped);
        assert_eq!(rig.brain.movement.speed_modifier(), 1.2);
        assert!(rig.brain.movement.has_wanted());
    }
    assert!(jumps > 120 && jumps < rolls);
}

#[test]
fn the_random_direction_goal_waits_for_ground_liquid_or_levitation_and_no_target() {
    let world = World::new();
    let mut rig = Rig::new(EntityKind::Slime, 2, 5);
    let mut goal = direction();
    assert!(Goal::can_use(&mut goal, &mut rig.context(&world)));
    rig.mob.on_ground = false;
    assert!(!Goal::can_use(&mut goal, &mut rig.context(&world)));
    rig.mob.levitating = true;
    assert!(Goal::can_use(&mut goal, &mut rig.context(&world)));
    rig.mob.levitating = false;
    rig.mob.fluid.in_water = true;
    rig.mob.fluid.water_height = 0.5;
    assert!(Goal::can_use(&mut goal, &mut rig.context(&world)));
    rig.mob.on_ground = true;
    rig.brain.target = Some(PLAYER);
    assert!(!Goal::can_use(&mut goal, &mut rig.context(&world)));
}

#[test]
fn the_random_direction_goal_rerolls_every_twenty_to_fifty_ticks_and_never_aggresses() {
    let world = World::new();
    let mut rig = Rig::new(EntityKind::Slime, 2, 9);
    let mut goal = direction();
    let mut expected = MobRandom::new(9);
    let interval = expected.next_int(60) + 40;
    let degrees = expected.next_int(360) as f32;
    Goal::tick(&mut goal, &mut rig.context(&world));
    let MobGoal::Cube(CubeGoal::RandomDirection(state)) = &goal else {
        panic!("not the direction goal");
    };
    assert_eq!(state.next_randomize_time(), (interval + 1) / 2);
    assert_eq!(state.chosen_degrees(), degrees);
    assert_eq!(rig.brain.movement.cube().y_rot, degrees);
    assert!(!rig.brain.movement.cube().aggressive);
    let waiting = state.next_randomize_time();
    for _ in 0..waiting - 1 {
        Goal::tick(&mut goal, &mut rig.context(&world));
    }
    let MobGoal::Cube(CubeGoal::RandomDirection(state)) = &goal else {
        panic!("not the direction goal");
    };
    assert_eq!(state.chosen_degrees(), degrees);
    assert_eq!(state.next_randomize_time(), 1);
    Goal::tick(&mut goal, &mut rig.context(&world));
    let MobGoal::Cube(CubeGoal::RandomDirection(state)) = &goal else {
        panic!("not the direction goal");
    };
    let interval = expected.next_int(60) + 40;
    assert_eq!(state.next_randomize_time(), (interval + 1) / 2);
    assert_eq!(state.chosen_degrees(), expected.next_int(360) as f32);
}

#[test]
fn the_attack_goal_needs_a_target_it_may_attack_and_tires_after_a_hundred_and_fifty_ticks() {
    let mut world = World::new();
    let mut rig = Rig::new(EntityKind::Slime, 2, 5);
    let mut goal = attack();
    assert!(!Goal::can_use(&mut goal, &mut rig.context(&world)));
    world.players.push(living(
        PLAYER,
        EntityKind::Player,
        Vec3::new(3.5, 64.0, 0.5),
    ));
    rig.brain.target = Some(PLAYER);
    assert!(Goal::can_use(&mut goal, &mut rig.context(&world)));
    Goal::start(&mut goal, &mut rig.context(&world));
    let mut ticks = 0;
    while Goal::can_continue_to_use(&mut goal, &mut rig.context(&world)) {
        ticks += 1;
        assert!(ticks < 400);
    }
    assert_eq!(ticks, 149);
    world.players[0].creative = true;
    assert!(!Goal::can_use(&mut goal, &mut rig.context(&world)));
}

#[test]
fn the_attack_goal_faces_the_target_and_marks_big_slimes_aggressive() {
    let mut world = World::new();
    world.players.push(living(
        PLAYER,
        EntityKind::Player,
        Vec3::new(5.5, 64.0, 0.5),
    ));
    let mut big = Rig::new(EntityKind::Slime, 2, 5);
    big.brain.target = Some(PLAYER);
    let mut goal = attack();
    Goal::tick(&mut goal, &mut big.context(&world));
    assert_eq!(big.body.yaw, -10.0);
    assert!(big.brain.movement.cube().aggressive);
    assert_eq!(big.brain.movement.cube().y_rot, big.body.yaw);
    let mut tiny = Rig::new(EntityKind::Slime, 1, 5);
    tiny.brain.target = Some(PLAYER);
    Goal::tick(&mut goal, &mut tiny.context(&world));
    assert!(!tiny.brain.movement.cube().aggressive);
    let mut magma = Rig::new(EntityKind::MagmaCube, 1, 5);
    magma.brain.target = Some(PLAYER);
    Goal::tick(&mut goal, &mut magma.context(&world));
    assert!(magma.brain.movement.cube().aggressive);
}

#[test]
fn the_keep_on_jumping_goal_runs_unless_riding_and_wants_speed_one() {
    let world = World::new();
    let mut rig = Rig::new(EntityKind::Slime, 2, 5);
    let mut goal = keep_jumping();
    assert!(Goal::can_use(&mut goal, &mut rig.context(&world)));
    rig.mob.passenger = true;
    assert!(!Goal::can_use(&mut goal, &mut rig.context(&world)));
    Goal::tick(&mut goal, &mut rig.context(&world));
    assert_eq!(rig.brain.movement.speed_modifier(), 1.0);
    assert!(rig.brain.movement.has_wanted());
}

fn mob_act(ai: &mut MobAi, base: &mut EntityBase, state: &mut LivingState, world: &World) {
    ai.act(base, state, world);
}

#[test]
fn the_move_control_turns_ninety_degrees_per_tick_and_aligns_head_and_body() {
    let world = World::new();
    let (mut base, mut state) = cube_mob(EntityKind::Slime, 2);
    let mut ai = MobAi::new(EntityKind::Slime, 3, GoalSettings::default());
    ai.brain_mut().movement.set_cube_direction(150.0, false);
    mob_act(&mut ai, &mut base, &mut state, &world);
    assert_eq!(base.yaw, 90.0);
    assert_eq!(base.head_yaw, 90.0);
    assert_eq!(state.body_yaw, 90.0);
    mob_act(&mut ai, &mut base, &mut state, &world);
    assert_eq!(base.yaw, 150.0);
    ai.brain_mut().movement.set_cube_direction(200.0, false);
    base.set_yaw(0.0);
    mob_act(&mut ai, &mut base, &mut state, &world);
    assert_eq!(base.yaw, 270.0);
    mob_act(&mut ai, &mut base, &mut state, &world);
    assert_eq!(base.yaw, 200.0);
}

#[test]
fn an_unwanted_movement_zeroes_the_forward_input() {
    let world = World::new();
    let (mut base, mut state) = cube_mob(EntityKind::Slime, 2);
    state.zza = 0.7;
    let mut ai = MobAi::new(EntityKind::Slime, 3, GoalSettings::default());
    mob_act(&mut ai, &mut base, &mut state, &world);
    assert_eq!(state.zza, 0.0);
}

#[test]
fn a_grounded_cube_jumps_when_the_delay_runs_out_then_waits() {
    let world = World::new();
    let (mut base, mut state) = cube_mob(EntityKind::Slime, 2);
    let mut ai = MobAi::new(EntityKind::Slime, 3, GoalSettings::default());
    let mut expected = MobRandom::new(3);
    let delay = cube::jump_delay(EntityKind::Slime, &mut expected);
    ai.brain_mut().movement.set_cube_wanted(1.0);
    mob_act(&mut ai, &mut base, &mut state, &world);
    assert!(state.jumping);
    assert_eq!(ai.brain().movement.cube().jump_delay, delay);
    let speed = (1.0 * state.attribute("minecraft:movement_speed")) as f32;
    assert_eq!(state.speed, speed);
    let sound = ai.take_cube_sound().expect("the jump sound");
    assert_eq!(sound.sound, "minecraft:entity.slime.jump");
    assert_eq!(sound.volume, 2.0 * 0.4);
    assert_eq!(ai.take_cube_sound(), None);
    for left in (1..delay).rev() {
        ai.brain_mut().movement.set_cube_wanted(1.0);
        state.xxa = 0.3;
        mob_act(&mut ai, &mut base, &mut state, &world);
        assert!(!state.jumping);
        assert_eq!(ai.brain().movement.cube().jump_delay, left);
        assert_eq!(state.speed, 0.0);
        assert_eq!(state.xxa, 0.0);
        assert_eq!(state.zza, 0.0);
    }
    ai.brain_mut().movement.set_cube_wanted(1.0);
    mob_act(&mut ai, &mut base, &mut state, &world);
    assert_eq!(ai.brain().movement.cube().jump_delay, 0);
    ai.brain_mut().movement.set_cube_wanted(1.0);
    mob_act(&mut ai, &mut base, &mut state, &world);
    assert!(state.jumping);
}

#[test]
fn an_aggressive_cube_jumps_three_times_as_often_and_a_magma_cube_four_times_less() {
    let world = World::new();
    let (mut base, mut state) = cube_mob(EntityKind::Slime, 2);
    let mut ai = MobAi::new(EntityKind::Slime, 3, GoalSettings::default());
    let mut expected = MobRandom::new(3);
    let delay = cube::jump_delay(EntityKind::Slime, &mut expected);
    ai.brain_mut().movement.set_cube_direction(0.0, true);
    ai.brain_mut().movement.set_cube_wanted(1.0);
    mob_act(&mut ai, &mut base, &mut state, &world);
    assert_eq!(ai.brain().movement.cube().jump_delay, delay / 3);
    let (mut magma_base, mut magma_state) = cube_mob(EntityKind::MagmaCube, 2);
    let mut magma = MobAi::new(EntityKind::MagmaCube, 3, GoalSettings::default());
    magma.brain_mut().movement.set_cube_wanted(1.0);
    mob_act(&mut magma, &mut magma_base, &mut magma_state, &world);
    assert_eq!(magma.brain().movement.cube().jump_delay, delay * 4);
    let sound = magma.take_cube_sound().expect("the jump sound");
    assert_eq!(sound.sound, "minecraft:entity.magma_cube.jump");
}

#[test]
fn a_cube_in_the_air_keeps_its_speed_without_counting_the_jump_delay() {
    let world = World::new();
    let (mut base, mut state) = cube_mob(EntityKind::Slime, 2);
    base.on_ground = false;
    let mut ai = MobAi::new(EntityKind::Slime, 3, GoalSettings::default());
    ai.brain_mut().movement.set_cube_wanted(1.2);
    mob_act(&mut ai, &mut base, &mut state, &world);
    assert!(!state.jumping);
    assert_eq!(ai.brain().movement.cube().jump_delay, 0);
    let speed = (1.2 * state.attribute("minecraft:movement_speed")) as f32;
    assert_eq!(state.speed, speed);
    assert_eq!(state.zza, speed);
}

#[test]
fn a_tiny_slime_jumps_with_the_small_sound_and_a_higher_pitch() {
    let world = World::new();
    let (mut base, mut state) = cube_mob(EntityKind::Slime, 1);
    let mut ai = MobAi::new(EntityKind::Slime, 3, GoalSettings::default());
    ai.brain_mut().movement.set_cube_wanted(1.0);
    mob_act(&mut ai, &mut base, &mut state, &world);
    let sound = ai.take_cube_sound().expect("the jump sound");
    assert_eq!(sound.sound, "minecraft:entity.slime.jump_small");
    assert!(sound.pitch >= 0.8 * 1.4 && sound.pitch <= 1.2 * 1.4);
}

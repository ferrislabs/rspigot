use std::collections::HashMap;

use rspigot_ai::{
    ALLOWED_ANGLES, Activity, AiContext, AiWorld, ArmadilloAction, ArmadilloFacts, BehaviorControl,
    BehaviorCx, BrainState, Facts, GoalSettings, GoatAction, JumpBody, JumpDimensions, Living,
    MemoryModuleType, MemoryValue, MobAction, MobAi, MobBody, MobControl, MobView,
    NearestVisibleLivingEntities, SensorKind, Status, TargetKind, calculate_jump_vector_for_angle,
};
use rspigot_blocks::{BlockKind, StateId};
use rspigot_domain::entity::living::LivingState;
use rspigot_domain::entity::{Aabb, EntityBase, Pose};
use rspigot_domain::{BlockPos, EntityId, Vec3};
use rspigot_entities::EntityKind;
use uuid::Uuid;

const NOW: i64 = 100;

#[derive(Default)]
struct Field {
    players: Vec<Living>,
    others: Vec<Living>,
    edits: HashMap<BlockPos, StateId>,
    floor: Option<(i32, StateId)>,
    time: i64,
    griefing: bool,
}

impl Field {
    fn flat() -> Self {
        Self {
            floor: Some((63, BlockKind::Stone.default_state())),
            griefing: true,
            time: NOW,
            ..Self::default()
        }
    }

    fn put(&mut self, x: i32, y: i32, z: i32, state: StateId) {
        self.edits.insert(BlockPos::new(x, y, z), state);
    }

    fn platform(&mut self, x: std::ops::RangeInclusive<i32>, z: std::ops::RangeInclusive<i32>) {
        for x in x {
            for z in z.clone() {
                for y in 55..=63 {
                    self.put(x, y, z, BlockKind::Stone.default_state());
                }
            }
        }
    }
}

impl rspigot_ai::BlockView for Field {
    fn state(&self, pos: BlockPos) -> Option<StateId> {
        if let Some(state) = self.edits.get(&pos) {
            return Some(*state);
        }
        Some(match self.floor {
            Some((top, state)) if pos.y <= top && pos.y >= 55 => state,
            _ => StateId::AIR,
        })
    }
}

impl AiWorld for Field {
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
        self.time
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
        facts: Facts {
            uuid: Uuid::from_u128(id as u128 + 900),
            ..Facts::default()
        },
    }
}

fn view(kind: EntityKind, position: Vec3) -> MobView {
    let mut base = EntityBase::new(EntityId(1), Uuid::from_u128(1), kind);
    base.position = position;
    base.old_position = position;
    base.on_ground = true;
    base.tick_count = 1;
    let state = LivingState::new(&mut base, 0.0);
    MobView::of(&base, &state)
}

struct Bench {
    mob: MobView,
    world: Field,
    controls: rspigot_ai::Brain,
    body: MobBody,
    state: BrainState,
}

impl Bench {
    fn new(kind: EntityKind, at: Vec3, world: Field, seed: i64) -> Self {
        let mob = view(kind, at);
        let mut state = BrainState::default();
        for ty in MemoryModuleType::ALL {
            state.memories.register(*ty);
        }
        Self {
            body: MobBody::of(&mob),
            mob,
            world,
            controls: rspigot_ai::Brain::new(kind, seed),
            state,
        }
    }

    fn with<R>(&mut self, run: impl FnOnce(&mut BehaviorCx<'_, '_, Field>) -> R) -> R {
        let mut ai = AiContext {
            mob: &self.mob,
            world: &self.world,
            brain: &mut self.controls,
            body: &mut self.body,
        };
        let mut cx = BehaviorCx {
            ai: &mut ai,
            brain: &mut self.state,
        };
        run(&mut cx)
    }

    fn start(&mut self, control: &mut MobControl, now: i64) -> bool {
        self.with(|cx| control.try_start(cx, now))
    }

    fn tick(&mut self, control: &mut MobControl, now: i64) {
        self.with(|cx| control.tick_or_stop(cx, now));
    }

    fn get(&self, ty: MemoryModuleType) -> Option<&MemoryValue> {
        self.state.memories.get(ty)
    }

    fn set(&mut self, ty: MemoryModuleType, value: MemoryValue) {
        self.state.memories.set(ty, value);
    }

    fn actions(&mut self) -> Vec<MobAction> {
        std::mem::take(&mut self.controls.actions)
    }

    fn see(&mut self, entities: &[Living]) {
        self.world.others.extend(entities.iter().copied());
        let ids = entities.iter().map(|living| living.id).collect();
        self.set(
            MemoryModuleType::NearestVisibleLivingEntities,
            MemoryValue::Visible(NearestVisibleLivingEntities::new(ids)),
        );
    }

    fn tick_sensor(&mut self, mut sensor: SensorKind) {
        use rspigot_ai::Sensor;
        for ty in sensor.requires() {
            self.state.memories.register(*ty);
        }
        let mut ai = AiContext {
            mob: &self.mob,
            world: &self.world,
            brain: &mut self.controls,
            body: &mut self.body,
        };
        sensor.do_tick(&mut ai, &mut self.state.memories);
    }
}

fn describe(control: &MobControl) -> String {
    format!("{control:?}")
}

fn behaviors(kind: EntityKind, activity: Activity) -> Vec<(i32, String)> {
    let ai = MobAi::new(kind, 1, GoalSettings::default());
    let brain = ai.behavior_brain().expect("a brain");
    brain
        .activity_behaviors(activity)
        .into_iter()
        .map(|(priority, control)| (priority, describe(control)))
        .collect()
}

fn leading(list: &[(i32, String)], known: &[&'static str]) -> Vec<(i32, &'static str)> {
    const WRAPPERS: [&str; 5] = ["OneShot", "Timed", "Gate", "Condition", "Sequence"];
    list.iter()
        .map(|(priority, text)| {
            let mut found: Vec<(usize, &'static str)> = known
                .iter()
                .filter(|name| !WRAPPERS.contains(name))
                .filter_map(|name| text.find(name).map(|at| (at, *name)))
                .collect();
            found.sort_by_key(|(at, _)| *at);
            (*priority, found.first().map_or("?", |(_, name)| *name))
        })
        .collect()
}

const KNOWN: [&str; 31] = [
    "ArmadilloPanic",
    "ArmadilloRollOut",
    "ArmadilloBallUp",
    "Swim",
    "AnimalPanic",
    "LookAtTargetSink",
    "MoveToTargetSink",
    "CountDownCooldownTicks",
    "SetEntityLookTargetSometimes",
    "AnimalMakeLove",
    "FollowTemptation",
    "BabyFollowAdult",
    "RandomStroll",
    "SetWalkTargetFromLookTarget",
    "DoNothing",
    "LongJumpMidJump",
    "LongJumpToRandomPos",
    "RamTarget",
    "PrepareRamNearestTarget",
    "RandomLookAround",
    "OneShot",
    "Timed",
    "Gate",
    "Condition",
    "Sequence",
    "SetEntityLookTarget",
    "StartAttacking",
    "MeleeAttack",
    "StopAttackingIfTargetInvalid",
    "EraseMemoryIf",
    "SetWalkTargetFromAttackTargetIfTargetOutOfReach",
];

fn entity(id: i32) -> MemoryValue {
    MemoryValue::Entity(EntityId(id))
}

fn int(memory: &MemoryValue) -> i32 {
    memory.as_int().expect("an int")
}

const GOAT_HOME: Vec3 = Vec3::new(0.5, 64.0, 0.5);

fn goat_bench(world: Field, seed: i64) -> Bench {
    Bench::new(EntityKind::Goat, GOAT_HOME, world, seed)
}

#[test]
fn goat_activities_follow_the_vanilla_priorities() {
    let kind = EntityKind::Goat;
    assert_eq!(
        leading(&behaviors(kind, Activity::Core), &KNOWN),
        vec![
            (0, "Swim"),
            (1, "AnimalPanic"),
            (2, "LookAtTargetSink"),
            (3, "MoveToTargetSink"),
            (4, "CountDownCooldownTicks"),
            (5, "CountDownCooldownTicks"),
            (6, "CountDownCooldownTicks"),
        ]
    );
    assert_eq!(
        leading(&behaviors(kind, Activity::Idle), &KNOWN),
        vec![
            (0, "SetEntityLookTargetSometimes"),
            (0, "AnimalMakeLove"),
            (1, "FollowTemptation"),
            (2, "BabyFollowAdult"),
            (3, "RandomStroll"),
        ]
    );
    assert_eq!(
        leading(&behaviors(kind, Activity::LongJump), &KNOWN),
        vec![(0, "LongJumpMidJump"), (1, "LongJumpToRandomPos")]
    );
    assert_eq!(
        leading(&behaviors(kind, Activity::Ram), &KNOWN),
        vec![(0, "RamTarget"), (1, "PrepareRamNearestTarget")]
    );
}

#[test]
fn the_core_cooldowns_of_a_goat_count_the_temptation_the_long_jump_and_the_ram() {
    let core = behaviors(EntityKind::Goat, Activity::Core);
    let counted: Vec<bool> = [
        MemoryModuleType::TemptationCooldownTicks,
        MemoryModuleType::LongJumpCooldownTicks,
        MemoryModuleType::RamCooldownTicks,
    ]
    .iter()
    .zip(&core[4..7])
    .map(|(ty, (_, text))| text.contains(&format!("{ty:?}")))
    .collect();
    assert_eq!(counted, vec![true; 3]);
    assert!(core[0].1.contains("chance: 0.8"));
    assert!(core[1].1.contains("speed_multiplier: 2.0"));
}

fn think(ai: &mut MobAi, mob: &MobView, world: &Field) {
    let mut body = MobBody::of(mob);
    ai.think(mob, world, &mut body);
}

fn active(ai: &MobAi) -> Option<Activity> {
    ai.behavior_brain()
        .and_then(|brain| brain.state().active_non_core_activity())
}

fn goat_ai(seed: i64) -> MobAi {
    MobAi::new(EntityKind::Goat, seed, GoalSettings::default())
}

#[test]
fn a_goat_with_both_cooldowns_running_idles_and_the_first_ready_one_picks_its_activity() {
    let mob = view(EntityKind::Goat, GOAT_HOME);
    let world = Field::default();
    let mut ai = goat_ai(3);
    ai.init_goat_memories(900, 3000);
    think(&mut ai, &mob, &world);
    assert_eq!(active(&ai), Some(Activity::Idle));
    let memories = ai.behavior_brain_mut().unwrap().memories_mut();
    memories.erase(MemoryModuleType::LongJumpCooldownTicks);
    memories.erase(MemoryModuleType::WalkTarget);
    think(&mut ai, &mob, &world);
    assert_eq!(active(&ai), Some(Activity::LongJump));
    let memories = ai.behavior_brain_mut().unwrap().memories_mut();
    memories.erase(MemoryModuleType::RamCooldownTicks);
    think(&mut ai, &mob, &world);
    assert_eq!(active(&ai), Some(Activity::Ram));
}

#[test]
fn a_tempted_or_breeding_goat_neither_rams_nor_long_jumps() {
    let mob = view(EntityKind::Goat, GOAT_HOME);
    let world = Field::flat();
    let mut ai = goat_ai(4);
    ai.init_goat_memories(900, 3000);
    let memories = ai.behavior_brain_mut().unwrap().memories_mut();
    memories.erase(MemoryModuleType::LongJumpCooldownTicks);
    memories.erase(MemoryModuleType::RamCooldownTicks);
    memories.set(MemoryModuleType::TemptingPlayer, entity(7));
    think(&mut ai, &mob, &world);
    assert_eq!(active(&ai), Some(Activity::Idle));
    let memories = ai.behavior_brain_mut().unwrap().memories_mut();
    memories.erase(MemoryModuleType::TemptingPlayer);
    memories.set(MemoryModuleType::BreedTarget, entity(8));
    think(&mut ai, &mob, &world);
    assert_eq!(active(&ai), Some(Activity::Idle));
}

#[test]
fn a_goat_never_long_jumps_while_it_has_a_walk_target() {
    let mob = view(EntityKind::Goat, GOAT_HOME);
    let world = Field::flat();
    let mut ai = goat_ai(5);
    ai.init_goat_memories(900, 3000);
    let memories = ai.behavior_brain_mut().unwrap().memories_mut();
    memories.erase(MemoryModuleType::LongJumpCooldownTicks);
    memories.set(
        MemoryModuleType::WalkTarget,
        MemoryValue::WalkTarget(rspigot_ai::WalkTarget::block(
            BlockPos::new(3, 64, 3),
            1.0,
            0,
        )),
    );
    think(&mut ai, &mob, &world);
    assert_ne!(active(&ai), Some(Activity::LongJump));
}

#[test]
fn the_first_two_cooldowns_of_a_new_goat_are_the_vanilla_ranges() {
    let (long_jump, ram) =
        rspigot_domain::entity::species::goat::sample_memories(&mut rspigot_ai::MobRandom::new(9));
    assert!((600..=1200).contains(&long_jump));
    assert!((600..=6000).contains(&ram));
    let mut ai = goat_ai(1);
    ai.init_goat_memories(long_jump, ram);
    let memories = ai.behavior_brain().unwrap().memories();
    assert_eq!(
        int(memories
            .get(MemoryModuleType::LongJumpCooldownTicks)
            .unwrap()),
        long_jump
    );
    assert_eq!(
        int(memories.get(MemoryModuleType::RamCooldownTicks).unwrap()),
        ram
    );
}

fn flat_jump(distance: f64, max_velocity: f32, angle: i32, check: bool) -> Option<Vec3> {
    let world = Field::flat();
    let mob = view(EntityKind::Goat, GOAT_HOME);
    calculate_jump_vector_for_angle(
        &world,
        JumpBody {
            position: mob.position,
            gravity: mob.gravity,
            dimensions: JumpDimensions::of(rspigot_domain::entity::ageable::dimensions_in_pose(
                EntityKind::Goat,
                false,
                Pose::LongJumping,
            )),
        },
        Vec3::new(GOAT_HOME.x + distance, 64.0, GOAT_HOME.z),
        max_velocity,
        angle,
        check,
    )
}

#[test]
fn the_jump_vector_of_a_flat_leap_follows_the_projectile_equation() {
    let vector = flat_jump(7.0, 3.0, 70, false).expect("reachable");
    let reach = 6.5_f64;
    let angle = f64::from(70.0_f32 * std::f32::consts::PI / 180.0);
    let gravity = 0.08;
    let speed = (reach * gravity / (2.0 * angle).sin()).sqrt();
    let expected = Vec3::new(
        speed * angle.cos() * 0.95_f32 as f64,
        speed * angle.sin() * 0.95_f32 as f64,
        0.0,
    );
    assert!((vector.x - expected.x).abs() < 1.0e-6, "{vector:?}");
    assert!((vector.y - expected.y).abs() < 1.0e-6, "{vector:?}");
    assert!(vector.z.abs() < 1.0e-9);
}

#[test]
fn a_jump_faster_than_the_goat_can_manage_is_refused() {
    assert!(flat_jump(7.0, 3.0, 70, false).is_some());
    assert!(flat_jump(7.0, 0.1, 70, false).is_none());
}

#[test]
fn a_wall_on_the_trajectory_blocks_the_collision_checked_jump_only() {
    let mut world = Field::flat();
    for y in 64..=140 {
        for z in -2..=2 {
            world.put(4, y, z, BlockKind::Stone.default_state());
        }
    }
    let mob = view(EntityKind::Goat, GOAT_HOME);
    let dimensions = JumpDimensions::of(rspigot_domain::entity::ageable::dimensions_in_pose(
        EntityKind::Goat,
        false,
        Pose::LongJumping,
    ));
    let target = Vec3::new(8.5, 64.0, 0.5);
    let attempt = |check: bool| {
        ALLOWED_ANGLES.iter().find_map(|angle| {
            calculate_jump_vector_for_angle(
                &world,
                JumpBody {
                    position: mob.position,
                    gravity: mob.gravity,
                    dimensions,
                },
                target,
                3.0,
                *angle,
                check,
            )
        })
    };
    assert!(attempt(false).is_some());
    assert!(attempt(true).is_none());
}

fn long_jump_control(kind: &str) -> MobControl {
    let ai = goat_ai(1);
    ai.behavior_brain()
        .unwrap()
        .activity_behaviors(Activity::LongJump)
        .into_iter()
        .map(|(_, control)| control.clone())
        .find(|control| describe(control).contains(kind))
        .expect("the behavior")
}

fn ram_control(kind: &str) -> MobControl {
    let ai = goat_ai(1);
    ai.behavior_brain()
        .unwrap()
        .activity_behaviors(Activity::Ram)
        .into_iter()
        .map(|(_, control)| control.clone())
        .find(|control| describe(control).contains(kind))
        .expect("the behavior")
}

#[test]
fn a_goat_mid_jump_flies_without_friction_in_the_long_jump_pose_until_it_lands() {
    let mut bench = goat_bench(Field::flat(), 2);
    bench.set(MemoryModuleType::LongJumpMidJump, MemoryValue::Bool(true));
    let mut control = long_jump_control("LongJumpMidJump");
    assert!(bench.start(&mut control, NOW));
    assert_eq!(bench.body.pose, Pose::LongJumping);
    assert!(bench.body.discard_friction);
    bench.mob.on_ground = false;
    bench.tick(&mut control, NOW + 1);
    assert_eq!(control.status(), Status::Running);
    bench.mob.on_ground = true;
    bench.body.velocity = Vec3::new(1.0, -0.5, 1.0);
    bench.tick(&mut control, NOW + 2);
    assert_eq!(control.status(), Status::Stopped);
    assert_eq!(bench.body.pose, Pose::Standing);
    assert!(!bench.body.discard_friction);
    assert!((bench.body.velocity.x - 0.1).abs() < 1.0e-12);
    assert_eq!(bench.body.velocity.y, -0.5);
    assert!((bench.body.velocity.z - 0.1).abs() < 1.0e-12);
    assert!(bench.get(MemoryModuleType::LongJumpMidJump).is_none());
    let cooldown = int(bench.get(MemoryModuleType::LongJumpCooldownTicks).unwrap());
    assert!((600..=1200).contains(&cooldown));
    let actions = bench.actions();
    assert!(actions.iter().any(|action| matches!(
        action,
        MobAction::Sound { sound: "minecraft:entity.goat.step", volume, .. } if *volume == 2.0
    )));
}

#[test]
fn a_goat_in_the_air_or_in_water_does_not_start_a_long_jump_and_rests_half_a_cooldown() {
    for (on_ground, in_water) in [(false, false), (true, true)] {
        let mut bench = goat_bench(Field::flat(), 2);
        bench.mob.on_ground = on_ground;
        bench.mob.fluid.in_water = in_water;
        let mut control = long_jump_control("LongJumpToRandomPos");
        assert!(!bench.start(&mut control, NOW));
        let cooldown = int(bench.get(MemoryModuleType::LongJumpCooldownTicks).unwrap());
        assert!((300..=600).contains(&cooldown), "{cooldown}");
    }
}

#[test]
fn a_goat_on_honey_does_not_start_a_long_jump() {
    let mut world = Field::flat();
    world.put(0, 64, 0, BlockKind::HoneyBlock.default_state());
    let mut bench = goat_bench(world, 2);
    let mut control = long_jump_control("LongJumpToRandomPos");
    assert!(!bench.start(&mut control, NOW));
    assert!(bench.get(MemoryModuleType::LongJumpCooldownTicks).is_some());
}

fn chasm_world() -> Field {
    let mut world = Field {
        griefing: true,
        time: NOW,
        ..Field::default()
    };
    world.platform(-1..=1, -1..=1);
    world.platform(5..=7, -1..=1);
    world
}

#[test]
fn a_goat_prepares_for_forty_ticks_then_leaps_across_a_gap_it_cannot_walk_over() {
    let mut bench = goat_bench(chasm_world(), 11);
    let mut control = long_jump_control("LongJumpToRandomPos");
    assert!(bench.start(&mut control, NOW));
    let mut chosen_at = None;
    for step in 1..=60 {
        bench.world.time = NOW + step;
        bench.tick(&mut control, NOW + step);
        if bench.get(MemoryModuleType::LongJumpMidJump).is_some() {
            chosen_at = Some(step);
            break;
        }
    }
    let leap = chosen_at.expect("the goat leaps");
    assert!(leap >= 41, "{leap}");
    assert!(bench.body.discard_friction);
    assert!(bench.body.velocity.y > 0.0);
    assert!(bench.body.velocity.x > 0.0);
    let landing = match bench.get(MemoryModuleType::LookTarget) {
        Some(MemoryValue::Tracker(tracker)) => tracker.current_position(&bench.world).unwrap(),
        other => panic!("{other:?}"),
    };
    assert!((4.0..=7.0).contains(&landing.x), "{landing:?}");
    let actions = bench.actions();
    assert!(actions.iter().any(|action| matches!(
        action,
        MobAction::Sound {
            sound: "minecraft:entity.goat.long_jump",
            ..
        }
    )));
}

#[test]
fn a_screaming_goat_leaps_with_its_own_voice() {
    let mut bench = goat_bench(chasm_world(), 11);
    bench.mob.species.goat.screaming = true;
    let mut control = long_jump_control("LongJumpToRandomPos");
    assert!(bench.start(&mut control, NOW));
    for step in 1..=60 {
        bench.world.time = NOW + step;
        bench.tick(&mut control, NOW + step);
    }
    assert!(bench.actions().iter().any(|action| matches!(
        action,
        MobAction::Sound {
            sound: "minecraft:entity.goat.screaming.long_jump",
            ..
        }
    )));
}

#[test]
fn a_goat_that_can_simply_walk_to_every_landing_spot_never_leaps() {
    let mut bench = goat_bench(Field::flat(), 11);
    let mut control = long_jump_control("LongJumpToRandomPos");
    assert!(bench.start(&mut control, NOW));
    for step in 1..=80 {
        bench.world.time = NOW + step;
        bench.tick(&mut control, NOW + step);
    }
    assert!(bench.get(MemoryModuleType::LongJumpMidJump).is_none());
    assert_eq!(control.status(), Status::Stopped);
    let cooldown = int(bench.get(MemoryModuleType::LongJumpCooldownTicks).unwrap());
    assert!((300..=600).contains(&cooldown), "{cooldown}");
}

#[test]
fn moving_off_the_spot_cancels_the_long_jump_preparation() {
    let mut bench = goat_bench(chasm_world(), 11);
    let mut control = long_jump_control("LongJumpToRandomPos");
    assert!(bench.start(&mut control, NOW));
    bench.tick(&mut control, NOW + 1);
    bench.mob.position = Vec3::new(1.5, 64.0, 0.5);
    bench.tick(&mut control, NOW + 2);
    assert_eq!(control.status(), Status::Stopped);
    assert!(bench.get(MemoryModuleType::LongJumpCooldownTicks).is_some());
}

#[test]
fn a_jump_boost_adds_to_the_launch_speed() {
    let mut plain = goat_bench(chasm_world(), 11);
    let mut boosted = goat_bench(chasm_world(), 11);
    boosted.mob.jump_boost_power = 0.2;
    let mut leaps = Vec::new();
    for bench in [&mut plain, &mut boosted] {
        let mut control = long_jump_control("LongJumpToRandomPos");
        assert!(bench.start(&mut control, NOW));
        for step in 1..=60 {
            bench.world.time = NOW + step;
            bench.tick(&mut control, NOW + step);
            if bench.get(MemoryModuleType::LongJumpMidJump).is_some() {
                break;
            }
        }
        leaps.push(bench.body.velocity.length());
    }
    assert!((leaps[1] - leaps[0] - 0.2).abs() < 1.0e-6, "{leaps:?}");
}

fn zombie(id: i32, at: Vec3) -> Living {
    living(id, EntityKind::Zombie, at)
}

fn ram_world() -> Field {
    let mut world = Field::flat();
    world.platform(-12..=12, -12..=12);
    world
}

#[test]
fn a_goat_picks_a_run_up_four_to_seven_blocks_from_the_nearest_visible_target() {
    let mut bench = goat_bench(ram_world(), 3);
    let target = zombie(9, Vec3::new(6.5, 64.0, 0.5));
    bench.see(&[target]);
    let mut control = ram_control("PrepareRamNearestTarget");
    assert!(bench.start(&mut control, NOW));
    bench.world.time = NOW + 1;
    bench.tick(&mut control, NOW + 1);
    let walk = match bench.get(MemoryModuleType::WalkTarget) {
        Some(MemoryValue::WalkTarget(walk)) => *walk,
        other => panic!("{other:?}"),
    };
    assert_eq!(walk.speed_modifier, 1.25);
    assert_eq!(walk.close_enough_dist, 0);
    let start = walk.target.current_position(&bench.world).unwrap().block();
    let distance = (start.x - 6).abs() + (start.z).abs();
    assert!((4..=7).contains(&distance), "{start:?}");
    assert!(matches!(
        bench.get(MemoryModuleType::LookTarget),
        Some(MemoryValue::Tracker(_))
    ));
}

#[test]
fn a_goat_charges_after_twenty_ticks_at_the_run_up_and_aims_at_the_edge_of_the_target_block() {
    let mut bench = goat_bench(ram_world(), 3);
    let target = zombie(9, Vec3::new(6.5, 64.0, 0.5));
    bench.see(&[target]);
    let mut control = ram_control("PrepareRamNearestTarget");
    assert!(bench.start(&mut control, NOW));
    bench.world.time = NOW + 1;
    bench.tick(&mut control, NOW + 1);
    let walk = match bench.get(MemoryModuleType::WalkTarget) {
        Some(MemoryValue::WalkTarget(walk)) => *walk,
        other => panic!("{other:?}"),
    };
    let spot = walk.target.current_position(&bench.world).unwrap();
    bench.mob.position = Vec3::new(spot.x, 64.0, spot.z);
    bench.actions();
    for step in 2..=30 {
        bench.world.time = NOW + step;
        bench.tick(&mut control, NOW + step);
        if bench.get(MemoryModuleType::RamTarget).is_some() {
            let aim = match bench.get(MemoryModuleType::RamTarget) {
                Some(MemoryValue::Position(position)) => *position,
                other => panic!("{other:?}"),
            };
            assert!(step >= 22, "{step}");
            let side = (f64::from(spot.block().x) - 6.0).signum();
            assert_eq!(aim.y, 64.0);
            assert_eq!(aim.x, 6.5 - side * 0.5);
            assert!(bench.actions().iter().any(|action| matches!(
                action,
                MobAction::Voice("minecraft:entity.goat.prepare_ram")
            )));
            return;
        }
    }
    panic!("the goat never charged");
}

#[test]
fn a_goat_that_finds_no_run_up_waits_its_minimum_cooldown_before_trying_again() {
    let mut bench = goat_bench(Field::default(), 3);
    let target = zombie(9, Vec3::new(2.5, 64.0, 0.5));
    bench.see(&[target]);
    let mut control = ram_control("PrepareRamNearestTarget");
    assert!(bench.start(&mut control, NOW));
    bench.tick(&mut control, NOW + 1);
    assert_eq!(control.status(), Status::Stopped);
    assert_eq!(
        int(bench.get(MemoryModuleType::RamCooldownTicks).unwrap()),
        600
    );
    assert!(
        bench
            .actions()
            .iter()
            .any(|action| matches!(action, MobAction::EntityEvent(59)))
    );
}

#[test]
fn a_screaming_goat_has_a_short_fail_cooldown() {
    let mut bench = goat_bench(Field::default(), 3);
    bench.mob.species.goat.screaming = true;
    bench.see(&[zombie(9, Vec3::new(2.5, 64.0, 0.5))]);
    let mut control = ram_control("PrepareRamNearestTarget");
    assert!(bench.start(&mut control, NOW));
    bench.tick(&mut control, NOW + 1);
    assert_eq!(
        int(bench.get(MemoryModuleType::RamCooldownTicks).unwrap()),
        100
    );
}

#[test]
fn another_goat_is_never_a_ram_target() {
    let mut bench = goat_bench(ram_world(), 3);
    bench.see(&[living(9, EntityKind::Goat, Vec3::new(6.5, 64.0, 0.5))]);
    let mut control = ram_control("PrepareRamNearestTarget");
    assert!(bench.start(&mut control, NOW));
    bench.tick(&mut control, NOW + 1);
    assert!(bench.get(MemoryModuleType::WalkTarget).is_none());
}

#[test]
fn an_armor_stand_is_only_rammed_when_mob_griefing_is_on() {
    for griefing in [true, false] {
        let mut world = ram_world();
        world.griefing = griefing;
        let mut bench = goat_bench(world, 3);
        bench.see(&[living(9, EntityKind::ArmorStand, Vec3::new(6.5, 64.0, 0.5))]);
        let mut control = ram_control("PrepareRamNearestTarget");
        assert!(bench.start(&mut control, NOW));
        bench.tick(&mut control, NOW + 1);
        assert_eq!(
            bench.get(MemoryModuleType::WalkTarget).is_some(),
            griefing,
            "{griefing}"
        );
    }
}

fn charging(speed: f32) -> Bench {
    let mut bench = goat_bench(ram_world(), 3);
    bench.mob.speed = speed;
    bench.set(
        MemoryModuleType::RamTarget,
        MemoryValue::Position(Vec3::new(6.5, 64.0, 0.5)),
    );
    bench
}

#[test]
fn a_charging_goat_walks_straight_at_the_ram_target_at_triple_speed() {
    let mut bench = charging(0.2);
    let mut control = ram_control("RamTarget");
    assert!(bench.start(&mut control, NOW));
    let walk = match bench.get(MemoryModuleType::WalkTarget) {
        Some(MemoryValue::WalkTarget(walk)) => *walk,
        other => panic!("{other:?}"),
    };
    assert_eq!(walk.speed_modifier, 3.0);
    assert_eq!(walk.close_enough_dist, 0);
}

#[test]
fn a_charging_goat_knocks_back_the_first_thing_it_touches_and_rests() {
    let mut bench = charging(0.2);
    let target = zombie(9, Vec3::new(0.5, 64.0, 0.5));
    bench.world.others.push(target);
    let mut control = ram_control("RamTarget");
    assert!(bench.start(&mut control, NOW));
    bench.actions();
    bench.mob.position = Vec3::new(5.5, 64.0, 0.5);
    bench.world.others[0] = zombie(9, Vec3::new(5.5, 64.0, 0.5));
    bench.tick(&mut control, NOW + 1);
    let actions = bench.actions();
    let ram = actions
        .iter()
        .find_map(|action| match action {
            MobAction::Goat(GoatAction::Ram {
                target,
                speed,
                force,
                direction,
            }) => Some((*target, *speed, *force, *direction)),
            _ => None,
        })
        .expect("a ram hit");
    assert_eq!(ram.0, EntityId(9));
    assert!(
        (ram.1 - 0.2 * 1.65).abs() < 1.0e-6 || ram.1 == 0.33,
        "{ram:?}"
    );
    assert_eq!(ram.2, 2.5);
    assert!(ram.3.x < 0.0 && ram.3.y == 0.0);
    assert!((ram.3.length() - 1.0).abs() < 1.0e-9);
    assert!(actions.contains(&MobAction::EntityEvent(59)));
    assert!(actions.iter().any(|action| matches!(
        action,
        MobAction::Sound {
            sound: "minecraft:entity.goat.ram_impact",
            ..
        }
    )));
    assert!(bench.get(MemoryModuleType::RamTarget).is_none());
    let cooldown = int(bench.get(MemoryModuleType::RamCooldownTicks).unwrap());
    assert!((600..=6000).contains(&cooldown), "{cooldown}");
}

#[test]
fn the_knockback_speed_is_clamped_and_shifted_by_the_speed_and_slowness_effects() {
    let speeds = |speed: f32, fast: i32, slow: i32| {
        let mut bench = charging(speed);
        bench.mob.speed_effect_level = fast;
        bench.mob.slowness_effect_level = slow;
        bench
            .world
            .others
            .push(zombie(9, Vec3::new(0.5, 64.0, 0.5)));
        let mut control = ram_control("RamTarget");
        assert!(bench.start(&mut control, NOW));
        bench.actions();
        bench.tick(&mut control, NOW + 1);
        bench
            .actions()
            .into_iter()
            .find_map(|action| match action {
                MobAction::Goat(GoatAction::Ram { speed, .. }) => Some(speed),
                _ => None,
            })
            .expect("a hit")
    };
    assert_eq!(speeds(0.01, 0, 0), 0.2);
    assert_eq!(speeds(5.0, 0, 0), 3.0);
    assert!((speeds(0.2, 2, 0) - (0.2 * 1.65 + 0.5)).abs() < 1.0e-6);
    assert!((speeds(0.2, 0, 1) - (0.2 * 1.65 - 0.25)).abs() < 1.0e-6);
}

#[test]
fn a_baby_goat_pushes_with_less_force() {
    let mut bench = charging(0.2);
    bench.mob.baby = true;
    bench
        .world
        .others
        .push(zombie(9, Vec3::new(0.5, 64.0, 0.5)));
    let mut control = ram_control("RamTarget");
    assert!(bench.start(&mut control, NOW));
    bench.actions();
    bench.tick(&mut control, NOW + 1);
    assert!(bench.actions().iter().any(|action| matches!(
        action,
        MobAction::Goat(GoatAction::Ram { force, .. }) if *force == 1.0
    )));
}

#[test]
fn a_goat_that_rams_a_horn_snapping_block_loses_a_horn_and_rests() {
    let mut world = ram_world();
    world.put(1, 64, 0, BlockKind::OakLog.default_state());
    let mut bench = Bench::new(EntityKind::Goat, Vec3::new(0.5, 64.0, 0.5), world, 3);
    bench.set(
        MemoryModuleType::RamTarget,
        MemoryValue::Position(Vec3::new(6.5, 64.0, 0.5)),
    );
    bench.mob.velocity = Vec3::new(0.3, 0.0, 0.0);
    let mut control = ram_control("RamTarget");
    assert!(bench.start(&mut control, NOW));
    bench.actions();
    bench.tick(&mut control, NOW + 1);
    let actions = bench.actions();
    assert!(actions.contains(&MobAction::Goat(GoatAction::DropHorn)));
    assert!(actions.iter().any(|action| matches!(
        action,
        MobAction::Sound {
            sound: "minecraft:entity.goat.ram_impact",
            ..
        }
    )));
    assert!(bench.get(MemoryModuleType::RamTarget).is_none());
}

#[test]
fn a_goat_that_reaches_the_ram_target_without_hitting_anything_rests_too() {
    let mut bench = charging(0.2);
    let mut control = ram_control("RamTarget");
    assert!(bench.start(&mut control, NOW));
    bench.tick(&mut control, NOW + 1);
    assert!(bench.get(MemoryModuleType::RamTarget).is_some());
    bench.set(
        MemoryModuleType::WalkTarget,
        MemoryValue::WalkTarget(rspigot_ai::WalkTarget::position(
            Vec3::new(6.5, 64.0, 0.5),
            3.0,
            0,
        )),
    );
    bench.tick(&mut control, NOW + 2);
    assert!(bench.get(MemoryModuleType::RamTarget).is_some());
    bench.set(
        MemoryModuleType::WalkTarget,
        MemoryValue::WalkTarget(rspigot_ai::WalkTarget::position(
            Vec3::new(6.5, 64.0, 0.5),
            3.0,
            0,
        )),
    );
    bench.world.others.clear();
    bench.set(
        MemoryModuleType::RamTarget,
        MemoryValue::Position(Vec3::new(6.5, 64.0, 0.5)),
    );
    bench.tick(&mut control, NOW + 3);
    assert!(bench.get(MemoryModuleType::RamTarget).is_some());
}

#[test]
fn the_goat_cooldowns_round_trip_through_the_brain_nbt_in_the_vanilla_layout() {
    let mut ai = goat_ai(1);
    ai.init_goat_memories(777, 4321);
    let saved = ai.save_behavior_brain().expect("a brain");
    let memories = saved.get_compound("memories").expect("memories");
    assert_eq!(
        memories
            .get_compound("minecraft:long_jump_cooling_down")
            .and_then(|entry| entry.get_i32("value")),
        Some(777)
    );
    assert_eq!(
        memories
            .get_compound("minecraft:ram_cooldown_ticks")
            .and_then(|entry| entry.get_i32("value")),
        Some(4321)
    );
    assert!(memories.get_compound("minecraft:walk_target").is_none());
    let mut loaded = goat_ai(2);
    loaded.load_behavior_brain(&saved);
    let memories = loaded.behavior_brain().unwrap().memories();
    assert_eq!(
        int(memories
            .get(MemoryModuleType::LongJumpCooldownTicks)
            .unwrap()),
        777
    );
    assert_eq!(
        int(memories.get(MemoryModuleType::RamCooldownTicks).unwrap()),
        4321
    );
}

#[test]
fn the_goat_temptation_sensor_picks_the_nearest_player_holding_goat_food() {
    use rspigot_ai::{TemptItems, TemptingSensor};
    use rspigot_items::ItemKind;
    let mut bench = goat_bench(Field::flat(), 1);
    let mut near = living(5, EntityKind::Player, Vec3::new(3.5, 64.0, 0.5));
    near.main_hand = Some(ItemKind::Wheat.id());
    let mut far = living(6, EntityKind::Player, Vec3::new(9.5, 64.0, 0.5));
    far.main_hand = Some(ItemKind::Wheat.id());
    let empty = living(7, EntityKind::Player, Vec3::new(1.5, 64.0, 0.5));
    bench.world.players = vec![empty, far, near];
    bench.tick_sensor(SensorKind::Tempting(TemptingSensor::new(TemptItems::Tag(
        "minecraft:goat_food",
    ))));
    assert_eq!(
        bench.get(MemoryModuleType::TemptingPlayer),
        Some(&entity(5))
    );
}

use rspigot_domain::entity::species::armadillo::State as ArmadilloState;

const ARMADILLO_HOME: Vec3 = Vec3::new(0.5, 64.0, 0.5);

fn armadillo_bench(world: Field) -> Bench {
    Bench::new(EntityKind::Armadillo, ARMADILLO_HOME, world, 6)
}

fn armadillo_control(activity: Activity, kind: &str) -> MobControl {
    let ai = MobAi::new(EntityKind::Armadillo, 1, GoalSettings::default());
    ai.behavior_brain()
        .unwrap()
        .activity_behaviors(activity)
        .into_iter()
        .map(|(_, control)| control.clone())
        .find(|control| describe(control).contains(kind))
        .expect("the behavior")
}

fn in_state(bench: &mut Bench, state: ArmadilloState, ticks: i32) {
    bench.body.armadillo = ArmadilloFacts {
        state,
        in_state_ticks: ticks,
    };
}

#[test]
fn armadillo_activities_follow_the_vanilla_priorities() {
    let kind = EntityKind::Armadillo;
    assert_eq!(
        leading(&behaviors(kind, Activity::Core), &KNOWN),
        vec![
            (0, "Swim"),
            (1, "ArmadilloPanic"),
            (2, "LookAtTargetSink"),
            (3, "MoveToTargetSink"),
            (4, "CountDownCooldownTicks"),
            (5, "CountDownCooldownTicks"),
            (6, "ArmadilloRollOut"),
        ]
    );
    assert_eq!(
        leading(&behaviors(kind, Activity::Idle), &KNOWN),
        vec![
            (0, "SetEntityLookTargetSometimes"),
            (1, "AnimalMakeLove"),
            (2, "FollowTemptation"),
            (3, "RandomLookAround"),
            (4, "RandomStroll"),
        ]
    );
    assert_eq!(
        leading(&behaviors(kind, Activity::Panic), &KNOWN),
        vec![(0, "ArmadilloBallUp")]
    );
    let core = behaviors(kind, Activity::Core);
    assert!(core[1].1.contains("speed_multiplier: 2.0"));
    assert!(core[4].1.contains("TemptationCooldownTicks"));
    assert!(core[5].1.contains("GazeCooldownTicks"));
}

#[test]
fn an_armadillo_idles_until_danger_is_detected_and_then_panics_unless_it_is_already_fleeing() {
    let mob = view(EntityKind::Armadillo, ARMADILLO_HOME);
    let world = Field::default();
    let mut ai = MobAi::new(EntityKind::Armadillo, 3, GoalSettings::default());
    think(&mut ai, &mob, &world);
    assert_eq!(active(&ai), Some(Activity::Idle));
    let memories = ai.behavior_brain_mut().unwrap().memories_mut();
    memories.set_with_expiry(
        MemoryModuleType::DangerDetectedRecently,
        MemoryValue::Bool(true),
        80,
    );
    think(&mut ai, &mob, &world);
    assert_eq!(active(&ai), Some(Activity::Panic));
    let memories = ai.behavior_brain_mut().unwrap().memories_mut();
    memories.set(MemoryModuleType::IsPanicking, MemoryValue::Bool(true));
    think(&mut ai, &mob, &world);
    assert_eq!(active(&ai), Some(Activity::Idle));
}

fn scare(bench: &mut Bench) -> Option<i64> {
    bench.tick_sensor(SensorKind::ArmadilloScareDetected(
        rspigot_ai::ArmadilloScareSensor,
    ));
    bench
        .get(MemoryModuleType::DangerDetectedRecently)
        .map(|_| {
            bench
                .state
                .memories
                .time_until_expiry(MemoryModuleType::DangerDetectedRecently)
                .unwrap()
        })
}

fn nearby(bench: &mut Bench, entities: &[Living]) {
    bench.world.others.extend(entities.iter().copied());
    let ids = entities.iter().map(|living| living.id).collect();
    bench.set(
        MemoryModuleType::NearestLivingEntities,
        MemoryValue::Entities(ids),
    );
}

#[test]
fn a_sprinting_player_close_by_scares_an_armadillo_for_eighty_ticks() {
    let mut bench = armadillo_bench(Field::flat());
    let mut player = living(5, EntityKind::Player, Vec3::new(5.5, 64.0, 0.5));
    player.facts.sprinting = true;
    bench.world.players.push(player);
    nearby(&mut bench, &[player]);
    assert_eq!(scare(&mut bench), Some(80));
}

#[test]
fn a_walking_player_a_spectator_or_a_far_sprinter_do_not_scare_it() {
    let mut bench = armadillo_bench(Field::flat());
    let walking = living(5, EntityKind::Player, Vec3::new(2.5, 64.0, 0.5));
    let mut spectator = living(6, EntityKind::Player, Vec3::new(2.5, 64.0, 0.5));
    spectator.facts.sprinting = true;
    spectator.spectator = true;
    let mut far = living(7, EntityKind::Player, Vec3::new(20.5, 64.0, 0.5));
    far.facts.sprinting = true;
    nearby(&mut bench, &[walking, spectator, far]);
    assert_eq!(scare(&mut bench), None);
}

#[test]
fn undead_mobs_and_whoever_hurt_it_scare_an_armadillo_without_sprinting() {
    let mut bench = armadillo_bench(Field::flat());
    nearby(
        &mut bench,
        &[living(8, EntityKind::Zombie, Vec3::new(4.5, 64.0, 0.5))],
    );
    assert_eq!(scare(&mut bench), Some(80));
    let mut bench = armadillo_bench(Field::flat());
    bench.mob.last_hurt_by_mob = Some(EntityId(9));
    nearby(
        &mut bench,
        &[living(9, EntityKind::Cow, Vec3::new(4.5, 64.0, 0.5))],
    );
    assert_eq!(scare(&mut bench), Some(80));
    let mut bench = armadillo_bench(Field::flat());
    nearby(
        &mut bench,
        &[living(10, EntityKind::Cow, Vec3::new(4.5, 64.0, 0.5))],
    );
    assert_eq!(scare(&mut bench), None);
}

#[test]
fn the_scare_sensor_forgets_the_danger_while_the_armadillo_cannot_stay_rolled_up() {
    let mut bench = armadillo_bench(Field::flat());
    bench.set(
        MemoryModuleType::DangerDetectedRecently,
        MemoryValue::Bool(true),
    );
    bench.mob.fluid.in_water = true;
    assert_eq!(scare(&mut bench), None);
    let mut bench = armadillo_bench(Field::flat());
    bench.set(
        MemoryModuleType::DangerDetectedRecently,
        MemoryValue::Bool(true),
    );
    bench.set(MemoryModuleType::IsPanicking, MemoryValue::Bool(true));
    assert_eq!(scare(&mut bench), None);
    let mut bench = armadillo_bench(Field::flat());
    bench.set(
        MemoryModuleType::DangerDetectedRecently,
        MemoryValue::Bool(true),
    );
    bench.mob.leashed = true;
    assert_eq!(scare(&mut bench), None);
}

#[test]
fn the_scare_sensor_runs_every_five_ticks_and_listens_to_the_nearby_entities() {
    use rspigot_ai::Sensor;
    let sensor = rspigot_ai::ArmadilloScareSensor;
    assert_eq!(sensor.scan_rate(), 5);
    assert_eq!(
        sensor.requires(),
        &[MemoryModuleType::NearestLivingEntities]
    );
}

#[test]
fn rolling_up_stops_the_armadillo_forgets_its_love_and_closes_the_shell() {
    let mut bench = armadillo_bench(Field::flat());
    bench.body.velocity = Vec3::new(0.4, -0.2, 0.4);
    let mut control = armadillo_control(Activity::Panic, "ArmadilloBallUp");
    assert!(bench.start(&mut control, NOW));
    assert_eq!(bench.body.armadillo.state, ArmadilloState::Rolling);
    assert_eq!(bench.body.armadillo.in_state_ticks, 0);
    assert_eq!(bench.body.velocity, Vec3::new(0.0, -0.2, 0.0));
    let actions = bench.actions();
    assert!(actions.contains(&MobAction::Armadillo(ArmadilloAction::ResetLove)));
    assert!(actions.contains(&MobAction::Voice("minecraft:entity.armadillo.roll")));
}

#[test]
fn an_armadillo_in_the_air_or_in_liquid_does_not_ball_up() {
    for (on_ground, in_water) in [(false, false), (true, true)] {
        let mut bench = armadillo_bench(Field::flat());
        bench.mob.on_ground = on_ground;
        bench.mob.fluid.in_water = in_water;
        let mut control = armadillo_control(Activity::Panic, "ArmadilloBallUp");
        assert!(!bench.start(&mut control, NOW));
    }
}

fn balled_up() -> (Bench, MobControl) {
    let mut bench = armadillo_bench(Field::flat());
    bench.set(
        MemoryModuleType::DangerDetectedRecently,
        MemoryValue::Bool(true),
    );
    let mut control = armadillo_control(Activity::Panic, "ArmadilloBallUp");
    assert!(bench.start(&mut control, NOW));
    bench.actions();
    (bench, control)
}

#[test]
fn a_rolling_armadillo_becomes_scared_after_ten_ticks_and_lands_with_a_thud() {
    let (mut bench, mut control) = balled_up();
    in_state(&mut bench, ArmadilloState::Rolling, 10);
    bench.tick(&mut control, NOW + 1);
    assert_eq!(bench.body.armadillo.state, ArmadilloState::Rolling);
    in_state(&mut bench, ArmadilloState::Rolling, 11);
    bench.tick(&mut control, NOW + 2);
    assert_eq!(bench.body.armadillo.state, ArmadilloState::Scared);
    assert!(
        bench
            .actions()
            .contains(&MobAction::Voice("minecraft:entity.armadillo.land"))
    );
}

fn set_ttl(bench: &mut Bench, ttl: i64) {
    bench.state.memories.set_with_expiry(
        MemoryModuleType::DangerDetectedRecently,
        MemoryValue::Bool(true),
        ttl,
    );
}

#[test]
fn a_scared_armadillo_starts_unrolling_when_the_danger_has_less_than_thirty_ticks_left() {
    let (mut bench, mut control) = balled_up();
    in_state(&mut bench, ArmadilloState::Scared, 20);
    set_ttl(&mut bench, 31);
    bench.tick(&mut control, NOW + 1);
    assert_eq!(bench.body.armadillo.state, ArmadilloState::Scared);
    set_ttl(&mut bench, 29);
    bench.tick(&mut control, NOW + 2);
    assert_eq!(bench.body.armadillo.state, ArmadilloState::Unrolling);
    assert!(
        bench
            .actions()
            .contains(&MobAction::Voice("minecraft:entity.armadillo.unroll_start"))
    );
}

#[test]
fn an_unrolling_armadillo_closes_up_again_when_the_danger_returns() {
    let (mut bench, mut control) = balled_up();
    in_state(&mut bench, ArmadilloState::Unrolling, 5);
    set_ttl(&mut bench, 29);
    bench.tick(&mut control, NOW + 1);
    assert_eq!(bench.body.armadillo.state, ArmadilloState::Unrolling);
    set_ttl(&mut bench, 80);
    bench.tick(&mut control, NOW + 2);
    assert_eq!(bench.body.armadillo.state, ArmadilloState::Scared);
}

#[test]
fn a_scared_armadillo_peeks_out_when_danger_lingers_and_its_timer_runs_out() {
    let (mut bench, mut control) = balled_up();
    in_state(&mut bench, ArmadilloState::Scared, 20);
    set_ttl(&mut bench, 80);
    bench.tick(&mut control, NOW + 1);
    assert!(!bench.actions().contains(&MobAction::EntityEvent(64)));
    let mut peeked = false;
    for step in 2..=500 {
        set_ttl(&mut bench, 80);
        bench.tick(&mut control, NOW + step);
        if bench.actions().contains(&MobAction::EntityEvent(64)) {
            assert!(step > 100, "{step}");
            peeked = true;
            break;
        }
    }
    assert!(peeked);
}

#[test]
fn the_ball_up_ends_with_the_threat_and_unrolls_when_it_cannot_stay_rolled_up() {
    let (mut bench, mut control) = balled_up();
    in_state(&mut bench, ArmadilloState::Scared, 20);
    bench
        .state
        .memories
        .erase(MemoryModuleType::DangerDetectedRecently);
    bench.set(MemoryModuleType::IsPanicking, MemoryValue::Bool(true));
    in_state(&mut bench, ArmadilloState::Idle, 20);
    bench.tick(&mut control, NOW + 1);
    assert_eq!(control.status(), Status::Stopped);
    in_state(&mut bench, ArmadilloState::Scared, 20);
    let (mut bench, mut control) = balled_up();
    in_state(&mut bench, ArmadilloState::Scared, 20);
    bench.set(MemoryModuleType::IsPanicking, MemoryValue::Bool(true));
    bench.with(|cx| control.do_stop(cx, NOW + 2));
    assert_eq!(bench.body.armadillo.state, ArmadilloState::Idle);
    assert!(bench.actions().contains(&MobAction::Voice(
        "minecraft:entity.armadillo.unroll_finish"
    )));
}

#[test]
fn the_ball_up_gives_up_after_five_minutes() {
    let (_, control) = balled_up();
    assert!(describe(&control).contains("max_duration: 6000"));
}

#[test]
fn the_armadillo_panic_unrolls_before_it_runs() {
    let mut bench = armadillo_bench(Field::flat());
    in_state(&mut bench, ArmadilloState::Scared, 20);
    bench
        .state
        .memories
        .set(MemoryModuleType::IsPanicking, MemoryValue::Bool(true));
    let mut control = armadillo_control(Activity::Core, "ArmadilloPanic");
    assert!(bench.start(&mut control, NOW));
    assert_eq!(bench.body.armadillo.state, ArmadilloState::Idle);
    assert!(bench.get(MemoryModuleType::IsPanicking).is_some());
}

#[test]
fn the_core_roll_out_only_acts_when_no_danger_remains_and_the_armadillo_is_scared() {
    let mut control = armadillo_control(Activity::Core, "ArmadilloRollOut");
    let mut bench = armadillo_bench(Field::flat());
    in_state(&mut bench, ArmadilloState::Scared, 20);
    bench.set(
        MemoryModuleType::DangerDetectedRecently,
        MemoryValue::Bool(true),
    );
    assert!(!bench.start(&mut control, NOW));
    assert_eq!(bench.body.armadillo.state, ArmadilloState::Scared);
    bench
        .state
        .memories
        .erase(MemoryModuleType::DangerDetectedRecently);
    assert!(bench.start(&mut control, NOW));
    assert_eq!(bench.body.armadillo.state, ArmadilloState::Idle);
    let mut calm = armadillo_bench(Field::flat());
    let mut again = armadillo_control(Activity::Core, "ArmadilloRollOut");
    assert!(!calm.start(&mut again, NOW));
}

#[test]
fn being_hurt_by_a_creature_starts_a_danger_and_rolls_the_armadillo_up_but_a_fire_unrolls_it() {
    let mob = view(EntityKind::Armadillo, ARMADILLO_HOME);
    let world = Field::flat();
    let mut ai = MobAi::new(EntityKind::Armadillo, 3, GoalSettings::default());
    let mut body = MobBody::of(&mob);
    ai.deliver(
        &mob,
        &world,
        &mut body,
        rspigot_ai::BrainMessage::ArmadilloHurt {
            by_living: true,
            environmental: false,
        },
    );
    assert_eq!(body.armadillo.state, ArmadilloState::Rolling);
    let brain = ai.behavior_brain().unwrap();
    assert_eq!(
        brain
            .memories()
            .time_until_expiry(MemoryModuleType::DangerDetectedRecently),
        Some(80)
    );
    let mut scared = view(EntityKind::Armadillo, ARMADILLO_HOME);
    scared.species.armadillo.state = ArmadilloState::Scared;
    let mut body = MobBody::of(&scared);
    ai.deliver(
        &scared,
        &world,
        &mut body,
        rspigot_ai::BrainMessage::ArmadilloHurt {
            by_living: false,
            environmental: true,
        },
    );
    assert_eq!(body.armadillo.state, ArmadilloState::Idle);
}

#[test]
fn a_leashed_armadillo_hit_by_a_creature_stays_unrolled_but_still_notes_the_danger() {
    let mut mob = view(EntityKind::Armadillo, ARMADILLO_HOME);
    mob.leashed = true;
    let world = Field::flat();
    let mut ai = MobAi::new(EntityKind::Armadillo, 3, GoalSettings::default());
    let mut body = MobBody::of(&mob);
    ai.deliver(
        &mob,
        &world,
        &mut body,
        rspigot_ai::BrainMessage::ArmadilloHurt {
            by_living: true,
            environmental: false,
        },
    );
    assert_eq!(body.armadillo.state, ArmadilloState::Idle);
    assert!(
        ai.behavior_brain()
            .unwrap()
            .memories()
            .has_value(MemoryModuleType::DangerDetectedRecently)
    );
}

#[test]
fn the_armadillo_temptation_sensor_wants_armadillo_food() {
    use rspigot_ai::{TemptItems, TemptingSensor};
    use rspigot_items::ItemKind;
    let mut bench = armadillo_bench(Field::flat());
    let mut near = living(5, EntityKind::Player, Vec3::new(3.5, 64.0, 0.5));
    near.main_hand = Some(ItemKind::SpiderEye.id());
    bench.world.players = vec![near];
    bench.tick_sensor(SensorKind::Tempting(TemptingSensor::new(TemptItems::Tag(
        "minecraft:armadillo_food",
    ))));
    assert_eq!(
        bench.get(MemoryModuleType::TemptingPlayer),
        Some(&entity(5))
    );
}

#[test]
fn the_armadillo_danger_round_trips_through_the_brain_nbt() {
    let mut ai = MobAi::new(EntityKind::Armadillo, 3, GoalSettings::default());
    ai.behavior_brain_mut()
        .unwrap()
        .memories_mut()
        .set_with_expiry(
            MemoryModuleType::DangerDetectedRecently,
            MemoryValue::Bool(true),
            200,
        );
    let saved = ai.save_behavior_brain().expect("a brain");
    let entry = saved
        .get_compound("memories")
        .and_then(|memories| memories.get_compound("minecraft:danger_detected_recently"))
        .expect("the danger");
    assert_eq!(entry.get_bool("value"), Some(true));
    assert_eq!(entry.get_i64("ttl"), Some(200));
    let mut loaded = MobAi::new(EntityKind::Armadillo, 4, GoalSettings::default());
    loaded.load_behavior_brain(&saved);
    let memories = loaded.behavior_brain().unwrap().memories();
    assert!(memories.has_value(MemoryModuleType::DangerDetectedRecently));
    assert_eq!(
        memories.time_until_expiry(MemoryModuleType::DangerDetectedRecently),
        Some(200)
    );
}

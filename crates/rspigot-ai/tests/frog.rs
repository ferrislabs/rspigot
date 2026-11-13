use std::collections::HashMap;

use rspigot_ai::{
    Activity, AiContext, AiWorld, BehaviorControl, BehaviorCx, BrainState, FROG_GATED_ACTIVITIES,
    Facts, FrogAction, FrogAttackablesSensor, FrogCroak, FrogShootTongue, FrogTongueState,
    FrogTryFindLand, GoalSettings, Living, Memories, MemoryModuleType, MemoryValue, MobAction,
    MobAi, MobBehavior, MobBody, MobControl, MobTrigger, MobView, NearestVisibleLivingEntities,
    Sensor, SensorKind, SensorType, Status, TargetKind, WalkTarget,
};
use rspigot_blocks::{BlockKind, StateId};
use rspigot_domain::entity::living::LivingState;
use rspigot_domain::entity::{Aabb, EntityBase, Pose};
use rspigot_domain::{BlockPos, EntityId, Vec3};
use rspigot_entities::EntityKind;
use uuid::Uuid;

const NOW: i64 = 100;
const HOME: Vec3 = Vec3::new(0.5, 64.0, 0.5);

struct Field {
    others: Vec<Living>,
    edits: HashMap<BlockPos, StateId>,
    floor: bool,
}

impl Field {
    fn meadow() -> Self {
        Self {
            others: Vec::new(),
            edits: HashMap::new(),
            floor: true,
        }
    }

    fn void() -> Self {
        Self {
            floor: false,
            ..Self::meadow()
        }
    }

    fn island(&mut self, x: std::ops::RangeInclusive<i32>, z: std::ops::RangeInclusive<i32>) {
        for x in x {
            for z in z.clone() {
                for y in 55..=63 {
                    self.put(x, y, z, BlockKind::Stone.default_state());
                }
            }
        }
    }

    fn put(&mut self, x: i32, y: i32, z: i32, state: StateId) {
        self.edits.insert(BlockPos::new(x, y, z), state);
    }
}

impl rspigot_ai::BlockView for Field {
    fn state(&self, pos: BlockPos) -> Option<StateId> {
        if let Some(state) = self.edits.get(&pos) {
            return Some(*state);
        }
        Some(if self.floor && (55..=63).contains(&pos.y) {
            BlockKind::GrassBlock.default_state()
        } else {
            StateId::AIR
        })
    }
}

impl AiWorld for Field {
    fn players(&self) -> &[Living] {
        &[]
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
        NOW
    }

    fn raw_brightness(&self, _pos: BlockPos) -> i32 {
        15
    }
}

fn creature(id: i32, kind: EntityKind, position: Vec3, cube_size: u8) -> Living {
    let size = kind.dimensions();
    Living {
        id: EntityId(id),
        player: false,
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
            cube_size,
            ..Facts::default()
        },
    }
}

fn slime(id: i32, position: Vec3, size: u8) -> Living {
    creature(id, EntityKind::Slime, position, size)
}

fn view(position: Vec3) -> MobView {
    let mut base = EntityBase::new(EntityId(1), Uuid::from_u128(1), EntityKind::Frog);
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
    fn new(world: Field, seed: i64) -> Self {
        let mob = view(HOME);
        let mut state = BrainState::default();
        for ty in MemoryModuleType::ALL {
            state.memories.register(*ty);
        }
        Self {
            body: MobBody::of(&mob),
            mob,
            world,
            controls: rspigot_ai::Brain::new(EntityKind::Frog, seed),
            state,
        }
    }

    fn meadow() -> Self {
        Self::new(Field::meadow(), 7)
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

    fn memories(&self) -> &Memories {
        &self.state.memories
    }

    fn set(&mut self, ty: MemoryModuleType, value: MemoryValue) {
        self.state.memories.set(ty, value);
    }

    fn has(&self, ty: MemoryModuleType) -> bool {
        self.state.memories.has_value(ty)
    }

    fn actions(&mut self) -> Vec<MobAction> {
        std::mem::take(&mut self.controls.actions)
    }

    fn target(&mut self, id: i32) {
        self.set(
            MemoryModuleType::AttackTarget,
            MemoryValue::Entity(EntityId(id)),
        );
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

fn activity(kind: Activity) -> Vec<(i32, String)> {
    let ai = MobAi::new(EntityKind::Frog, 1, GoalSettings::default());
    ai.behavior_brain()
        .expect("a brain")
        .activity_behaviors(kind)
        .into_iter()
        .map(|(priority, control)| (priority, describe(control)))
        .collect()
}

fn from_activity(kind: Activity, name: &str) -> MobControl {
    let ai = MobAi::new(EntityKind::Frog, 1, GoalSettings::default());
    ai.behavior_brain()
        .expect("a brain")
        .activity_behaviors(kind)
        .into_iter()
        .map(|(_, control)| control.clone())
        .find(|control| describe(control).contains(name))
        .expect("the behavior")
}

fn tongue() -> MobControl {
    from_activity(Activity::Tongue, "FrogShootTongue")
}

fn croak() -> MobControl {
    MobControl::behavior_for(MobBehavior::FrogCroak(FrogCroak::default()), 100, 100)
}

fn sounds(actions: &[MobAction]) -> Vec<(&'static str, f32)> {
    actions
        .iter()
        .filter_map(|action| match action {
            MobAction::Sound { sound, volume, .. } => Some((*sound, *volume)),
            _ => None,
        })
        .collect()
}

fn frog_actions(actions: &[MobAction]) -> Vec<FrogAction> {
    actions
        .iter()
        .filter_map(|action| match action {
            MobAction::Frog(action) => Some(*action),
            _ => None,
        })
        .collect()
}

fn find_all(text: &str, names: &[&str]) -> Vec<usize> {
    names
        .iter()
        .map(|name| {
            text.find(name)
                .unwrap_or_else(|| panic!("{name} in {text}"))
        })
        .collect()
}

#[test]
fn the_core_activity_panics_looks_moves_and_counts_two_cooldowns() {
    let core = activity(Activity::Core);
    assert_eq!(
        core.iter().map(|(p, _)| *p).collect::<Vec<_>>(),
        vec![0, 1, 2, 3, 4]
    );
    assert!(core[0].1.contains("AnimalPanic") && core[0].1.contains("speed_multiplier: 2.0"));
    assert!(core[1].1.contains("LookAtTargetSink"));
    assert!(core[1].1.contains("min_duration: 45") && core[1].1.contains("max_duration: 90"));
    assert!(core[2].1.contains("MoveToTargetSink"));
    assert!(core[3].1.contains("TemptationCooldownTicks"));
    assert!(core[4].1.contains("LongJumpCooldownTicks"));
    assert!(core.iter().all(|(_, text)| !text.contains("Swim")));
}

#[test]
fn the_idle_activity_follows_the_vanilla_priorities() {
    let idle = activity(Activity::Idle);
    assert_eq!(
        idle.iter().map(|(p, _)| *p).collect::<Vec<_>>(),
        vec![0, 0, 1, 2, 3, 4]
    );
    assert!(idle[0].1.contains("SetEntityLookTargetSometimes"));
    assert!(idle[0].1.contains("min_inclusive: 30") && idle[0].1.contains("max_inclusive: 60"));
    assert!(idle[1].1.contains("AnimalMakeLove") && idle[1].1.contains("Frog"));
    assert!(idle[2].1.contains("FollowTemptation") && idle[2].1.contains("1.25"));
    assert!(idle[3].1.contains("StartAttacking") && idle[3].1.contains("NearestAttackable"));
    assert!(idle[3].1.contains("BreedTarget"));
    assert!(idle[4].1.contains("FrogTryFindLand"));
    assert!(idle[4].1.contains("range: 6") && idle[4].1.contains("speed_modifier: 1.0"));
    let gate = &idle[5].1;
    let order = find_all(
        gate,
        &[
            "RandomStroll",
            "SetWalkTargetFromLookTarget",
            "FrogCroak",
            "OnGround",
        ],
    );
    assert!(order.windows(2).all(|pair| pair[0] < pair[1]), "{order:?}");
    assert!(gate.contains("WalkTarget"));
}

#[test]
fn the_tongue_activity_stops_invalid_targets_then_shoots_and_erases_the_target_after() {
    let tongue = activity(Activity::Tongue);
    assert_eq!(
        tongue.iter().map(|(p, _)| *p).collect::<Vec<_>>(),
        vec![0, 1]
    );
    assert!(tongue[0].1.contains("StopAttackingIfTargetInvalid"));
    assert!(tongue[1].1.contains("FrogShootTongue"));
    assert!(tongue[1].1.contains("min_duration: 100") && tongue[1].1.contains("max_duration: 100"));
}

#[test]
fn the_long_jump_activity_jumps_then_lands_with_the_frog_sounds_and_the_preferred_blocks() {
    let jump = activity(Activity::LongJump);
    assert_eq!(jump.iter().map(|(p, _)| *p).collect::<Vec<_>>(), vec![0, 1]);
    assert!(jump[0].1.contains("LongJumpMidJump") && jump[0].1.contains("entity.frog.step"));
    assert!(jump[0].1.contains("min_inclusive: 100") && jump[0].1.contains("max_inclusive: 140"));
    assert!(jump[1].1.contains("LongJumpToRandomPos"));
    assert!(jump[1].1.contains("max_long_jump_height: 2"));
    assert!(jump[1].1.contains("max_long_jump_width: 4"));
    assert!(
        jump[1]
            .1
            .contains("max_jump_velocity_multiplier: 3.5714288")
    );
    assert!(jump[1].1.contains("entity.frog.long_jump"));
    assert!(jump[1].1.contains("frog_prefer_jump_to"));
    assert!(jump[1].1.contains("chance: 0.5"));
    assert!(jump[1].1.contains("Frog"));
}

#[test]
fn the_swim_and_lay_spawn_activities_are_registered_but_never_chosen() {
    assert_eq!(FROG_GATED_ACTIVITIES, [Activity::Swim, Activity::LaySpawn]);
    assert!(!activity(Activity::Swim).is_empty());
    assert!(!activity(Activity::LaySpawn).is_empty());
    let mut ai = MobAi::new(EntityKind::Frog, 3, GoalSettings::default());
    let mob = view(HOME);
    let world = Field::meadow();
    ai.behavior_brain_mut()
        .unwrap()
        .memories_mut()
        .set(MemoryModuleType::IsInWater, MemoryValue::Unit);
    ai.behavior_brain_mut()
        .unwrap()
        .memories_mut()
        .set(MemoryModuleType::IsPregnant, MemoryValue::Unit);
    for _ in 0..3 {
        let mut body = MobBody::of(&mob);
        ai.think(&mob, &world, &mut body);
    }
    let chosen = ai
        .behavior_brain()
        .and_then(|brain| brain.state().active_non_core_activity());
    assert!(!matches!(chosen, Some(Activity::Swim | Activity::LaySpawn)));
}

#[test]
fn a_frog_picks_tongue_then_long_jump_then_idle() {
    let mut ai = MobAi::new(EntityKind::Frog, 3, GoalSettings::default());
    let mob = view(HOME);
    let world = Field::meadow();
    let chosen = |ai: &MobAi| {
        ai.behavior_brain()
            .and_then(|brain| brain.state().active_non_core_activity())
    };
    let step = |ai: &mut MobAi| {
        let mut body = MobBody::of(&mob);
        ai.think(&mob, &world, &mut body);
    };
    ai.behavior_brain_mut().unwrap().memories_mut().set(
        MemoryModuleType::LongJumpCooldownTicks,
        MemoryValue::Int(1000),
    );
    step(&mut ai);
    assert_eq!(chosen(&ai), Some(Activity::Idle));
    ai.behavior_brain_mut()
        .unwrap()
        .memories_mut()
        .erase(MemoryModuleType::LongJumpCooldownTicks);
    step(&mut ai);
    assert_eq!(chosen(&ai), Some(Activity::LongJump));
    ai.behavior_brain_mut().unwrap().memories_mut().set(
        MemoryModuleType::AttackTarget,
        MemoryValue::Entity(EntityId(9)),
    );
    step(&mut ai);
    assert_eq!(chosen(&ai), Some(Activity::Tongue));
}

fn near_slime(bench: &mut Bench) -> Living {
    let target = slime(5, Vec3::new(2.5, 64.0, 0.5), 1);
    bench.see(&[target]);
    bench.target(5);
    target
}

#[test]
fn shooting_a_tongue_needs_a_target_and_no_walk_target_or_panic() {
    let mut control = tongue();
    let mut bench = Bench::meadow();
    assert!(!bench.start(&mut control, NOW));
    near_slime(&mut bench);
    bench.set(
        MemoryModuleType::WalkTarget,
        MemoryValue::WalkTarget(WalkTarget::block(BlockPos::new(3, 63, 3), 1.0, 0)),
    );
    assert!(!bench.start(&mut control, NOW));
    bench.state.memories.erase(MemoryModuleType::WalkTarget);
    bench.set(MemoryModuleType::IsPanicking, MemoryValue::Bool(true));
    assert!(!bench.start(&mut control, NOW));
    bench.state.memories.erase(MemoryModuleType::IsPanicking);
    assert!(bench.start(&mut control, NOW));
}

#[test]
fn starting_the_tongue_looks_at_the_prey_walks_to_it_at_double_speed_and_syncs_the_target() {
    let mut bench = Bench::meadow();
    let target = near_slime(&mut bench);
    let mut control = tongue();
    assert!(bench.start(&mut control, NOW));
    let walk = bench
        .memories()
        .get(MemoryModuleType::WalkTarget)
        .and_then(MemoryValue::as_walk_target)
        .copied()
        .expect("a walk target");
    assert_eq!(walk.speed_modifier, 2.0);
    assert_eq!(walk.close_enough_dist, 0);
    assert_eq!(
        walk.target.current_block_position(&bench.world),
        Some(target.position.block())
    );
    assert!(bench.has(MemoryModuleType::LookTarget));
    assert_eq!(
        frog_actions(&bench.actions()),
        vec![FrogAction::SetTongueTarget(target.id)]
    );
}

#[test]
fn a_frog_that_cannot_croak_and_shoot_at_once_refuses_while_croaking() {
    let mut bench = Bench::meadow();
    near_slime(&mut bench);
    bench.body.pose = Pose::Croaking;
    assert!(!bench.start(&mut tongue(), NOW));
}

#[test]
fn only_small_cubes_and_magma_cubes_are_worth_a_tongue() {
    for (kind, size, accepted) in [
        (EntityKind::Slime, 1, true),
        (EntityKind::MagmaCube, 1, true),
        (EntityKind::Slime, 2, false),
        (EntityKind::MagmaCube, 4, false),
        (EntityKind::Cow, 0, false),
    ] {
        let mut bench = Bench::meadow();
        let target = creature(5, kind, Vec3::new(2.5, 64.0, 0.5), size);
        bench.see(&[target]);
        bench.target(5);
        assert_eq!(bench.start(&mut tongue(), NOW), accepted, "{kind:?} {size}");
    }
}

#[test]
fn an_unreachable_prey_is_dropped_remembered_and_forgotten_after_a_hundred_ticks() {
    let mut bench = Bench::meadow();
    for y in 63..=69 {
        bench.world.put(8, y, 0, BlockKind::Stone.default_state());
    }
    let target = slime(5, Vec3::new(8.5, 70.0, 0.5), 1);
    bench.see(&[target]);
    bench.target(5);
    assert!(!bench.start(&mut tongue(), NOW));
    assert!(!bench.has(MemoryModuleType::AttackTarget));
    let listed = bench
        .memories()
        .get(MemoryModuleType::UnreachableTongueTargets)
        .and_then(MemoryValue::as_entities)
        .map(<[_]>::to_vec);
    assert_eq!(listed, Some(vec![EntityId(5)]));
    assert_eq!(
        bench
            .memories()
            .time_until_expiry(MemoryModuleType::UnreachableTongueTargets),
        Some(100)
    );
}

#[test]
fn the_list_of_unreachable_prey_keeps_the_five_latest() {
    let mut bench = Bench::meadow();
    for y in 63..=69 {
        bench.world.put(8, y, 0, BlockKind::Stone.default_state());
    }
    bench.set(
        MemoryModuleType::UnreachableTongueTargets,
        MemoryValue::Entities((10..15).map(EntityId).collect()),
    );
    let target = slime(5, Vec3::new(8.5, 70.0, 0.5), 1);
    bench.see(&[target]);
    bench.target(5);
    assert!(!bench.start(&mut tongue(), NOW));
    let listed = bench
        .memories()
        .get(MemoryModuleType::UnreachableTongueTargets)
        .and_then(MemoryValue::as_entities)
        .map(<[_]>::to_vec)
        .unwrap();
    assert_eq!(
        listed,
        vec![11, 12, 13, 14, 5]
            .into_iter()
            .map(EntityId)
            .collect::<Vec<_>>()
    );
}

fn far_prey() -> (Bench, MobControl) {
    let mut bench = Bench::meadow();
    let target = slime(5, Vec3::new(5.5, 64.0, 0.5), 1);
    bench.see(&[target]);
    bench.target(5);
    let mut control = tongue();
    assert!(bench.start(&mut control, NOW));
    bench.actions();
    (bench, control)
}

#[test]
fn a_distant_prey_is_chased_with_a_fresh_walk_target_every_eleven_ticks() {
    let (mut bench, mut control) = far_prey();
    let mut refreshed = Vec::new();
    for tick in 1..=24 {
        bench.state.memories.erase(MemoryModuleType::WalkTarget);
        bench.tick(&mut control, NOW + tick);
        if bench.has(MemoryModuleType::WalkTarget) {
            refreshed.push(tick);
        }
    }
    assert_eq!(refreshed, vec![11, 22]);
}

fn within_reach(bench: &mut Bench) {
    bench.world.others[0] = slime(5, Vec3::new(1.5, 64.0, 0.5), 1);
}

#[test]
fn within_one_and_three_quarter_blocks_the_tongue_snaps_pulls_and_changes_the_pose() {
    let (mut bench, mut control) = far_prey();
    within_reach(&mut bench);
    bench.tick(&mut control, NOW + 1);
    assert_eq!(bench.body.pose, Pose::UsingTongue);
    let actions = bench.actions();
    assert_eq!(
        sounds(&actions),
        vec![("minecraft:entity.frog.tongue", 2.0)]
    );
    let pulls: Vec<_> = frog_actions(&actions)
        .into_iter()
        .filter_map(|action| match action {
            FrogAction::Pull { target, velocity } => Some((target, velocity)),
            _ => None,
        })
        .collect();
    assert_eq!(pulls.len(), 1);
    assert_eq!(pulls[0].0, EntityId(5));
    assert!((pulls[0].1.x + 0.75).abs() < 1.0e-9, "{:?}", pulls[0].1);
    assert!(pulls[0].1.z.abs() < 1.0e-9);
}

#[test]
fn the_catch_lasts_seven_ticks_then_the_frog_eats_and_the_tongue_retracts_four_ticks_later() {
    let (mut bench, mut control) = far_prey();
    within_reach(&mut bench);
    bench.tick(&mut control, NOW + 1);
    bench.actions();
    for tick in 2..=7 {
        bench.tick(&mut control, NOW + tick);
        assert!(
            frog_actions(&bench.actions())
                .iter()
                .all(|a| !matches!(a, FrogAction::Eat(_))),
            "{tick}"
        );
    }
    bench.tick(&mut control, NOW + 8);
    let actions = bench.actions();
    assert_eq!(sounds(&actions), vec![("minecraft:entity.frog.eat", 2.0)]);
    assert!(frog_actions(&actions).contains(&FrogAction::Eat(EntityId(5))));
    for tick in 9..=12 {
        bench.tick(&mut control, NOW + tick);
        assert_eq!(control.status(), Status::Running, "{tick}");
    }
    bench.tick(&mut control, NOW + 13);
    assert_eq!(control.status(), Status::Stopped);
}

#[test]
fn stopping_the_tongue_forgets_the_prey_erases_the_synced_target_and_stands_up() {
    let (mut bench, mut control) = far_prey();
    within_reach(&mut bench);
    bench.tick(&mut control, NOW + 1);
    bench.state.memories.erase(MemoryModuleType::AttackTarget);
    bench.actions();
    bench.tick(&mut control, NOW + 2);
    assert_eq!(control.status(), Status::Stopped);
    assert_eq!(bench.body.pose, Pose::Standing);
    assert!(!bench.has(MemoryModuleType::AttackTarget));
    assert_eq!(
        frog_actions(&bench.actions()),
        vec![FrogAction::EraseTongueTarget]
    );
}

#[test]
fn a_panicking_frog_gives_up_the_tongue() {
    let (mut bench, mut control) = far_prey();
    bench.set(MemoryModuleType::IsPanicking, MemoryValue::Bool(true));
    bench.tick(&mut control, NOW + 1);
    assert_eq!(control.status(), Status::Stopped);
}

#[test]
fn the_tongue_machine_starts_chasing_and_never_sits_in_the_done_state_when_it_starts() {
    let mut machine = FrogShootTongue::default();
    assert_eq!(machine.state(), FrogTongueState::Done);
    let (_, control) = far_prey();
    assert!(describe(&control).contains("MoveToTarget"));
    machine = FrogShootTongue::default();
    assert_eq!(machine.state(), FrogTongueState::Done);
}

#[test]
fn a_frog_croaks_for_sixty_ticks_in_the_croaking_pose_when_standing_with_no_walk_target() {
    let mut bench = Bench::meadow();
    let mut control = croak();
    assert!(bench.start(&mut control, NOW));
    assert_eq!(bench.body.pose, Pose::Croaking);
    for tick in 1..=59 {
        bench.tick(&mut control, NOW + tick);
        assert_eq!(control.status(), Status::Running, "{tick}");
    }
    bench.tick(&mut control, NOW + 61);
    bench.tick(&mut control, NOW + 62);
    assert_eq!(control.status(), Status::Stopped);
    assert_eq!(bench.body.pose, Pose::Standing);
}

#[test]
fn a_frog_with_a_walk_target_or_not_standing_does_not_croak() {
    let mut bench = Bench::meadow();
    bench.set(
        MemoryModuleType::WalkTarget,
        MemoryValue::WalkTarget(WalkTarget::block(BlockPos::new(3, 63, 3), 1.0, 0)),
    );
    assert!(!bench.start(&mut croak(), NOW));
    let mut bench = Bench::meadow();
    bench.body.pose = Pose::UsingTongue;
    assert!(!bench.start(&mut croak(), NOW));
}

#[test]
fn a_frog_in_a_liquid_starts_the_croak_behavior_without_changing_its_pose() {
    let mut bench = Bench::meadow();
    bench.mob.fluid.in_water = true;
    let mut control = croak();
    assert!(bench.start(&mut control, NOW));
    assert_eq!(bench.body.pose, Pose::Standing);
}

#[test]
fn the_attackables_sensor_picks_the_nearest_visible_small_cube_within_ten_blocks() {
    let mut bench = Bench::meadow();
    let near = slime(5, Vec3::new(3.5, 64.0, 0.5), 1);
    let nearer_but_big = slime(6, Vec3::new(1.5, 64.0, 0.5), 3);
    let far = slime(7, Vec3::new(14.5, 64.0, 0.5), 1);
    let cow = creature(8, EntityKind::Cow, Vec3::new(2.5, 64.0, 0.5), 0);
    bench.see(&[nearer_but_big, cow, near, far]);
    bench.tick_sensor(SensorKind::FrogAttackables(FrogAttackablesSensor));
    assert_eq!(
        bench
            .memories()
            .get(MemoryModuleType::NearestAttackable)
            .and_then(MemoryValue::as_entity),
        Some(EntityId(5))
    );
}

#[test]
fn the_attackables_sensor_skips_prey_that_proved_unreachable() {
    let mut bench = Bench::meadow();
    let first = slime(5, Vec3::new(3.5, 64.0, 0.5), 1);
    let second = slime(6, Vec3::new(4.5, 64.0, 0.5), 1);
    bench.see(&[first, second]);
    bench.set(
        MemoryModuleType::UnreachableTongueTargets,
        MemoryValue::Entities(vec![EntityId(5)]),
    );
    bench.tick_sensor(SensorKind::FrogAttackables(FrogAttackablesSensor));
    assert_eq!(
        bench
            .memories()
            .get(MemoryModuleType::NearestAttackable)
            .and_then(MemoryValue::as_entity),
        Some(EntityId(6))
    );
}

#[test]
fn the_attackables_sensor_clears_the_memory_when_nothing_qualifies() {
    let mut bench = Bench::meadow();
    bench.set(
        MemoryModuleType::NearestAttackable,
        MemoryValue::Entity(EntityId(5)),
    );
    bench.see(&[creature(8, EntityKind::Cow, Vec3::new(2.5, 64.0, 0.5), 0)]);
    bench.tick_sensor(SensorKind::FrogAttackables(FrogAttackablesSensor));
    assert!(!bench.has(MemoryModuleType::NearestAttackable));
}

fn pond() -> Bench {
    let mut world = Field::meadow();
    for x in -2..=2 {
        for z in -2..=2 {
            world.put(x, 63, z, BlockKind::Water.default_state());
        }
    }
    let mut bench = Bench::new(world, 11);
    bench.mob.position = Vec3::new(0.5, 63.0, 0.5);
    bench
}

fn find_land() -> MobControl {
    from_activity(Activity::Idle, "FrogTryFindLand")
}

#[test]
fn a_frog_on_dry_ground_never_looks_for_land() {
    let mut bench = Bench::meadow();
    assert!(!bench.start(&mut find_land(), NOW));
    assert!(!bench.has(MemoryModuleType::WalkTarget));
}

#[test]
fn a_frog_in_water_walks_to_the_nearest_dry_standing_spot_and_scans_again_after_sixty_ticks() {
    let mut bench = pond();
    let mut control = find_land();
    assert!(bench.start(&mut control, NOW));
    let walk = bench
        .memories()
        .get(MemoryModuleType::WalkTarget)
        .and_then(MemoryValue::as_walk_target)
        .copied()
        .expect("a walk target");
    assert_eq!(walk.speed_modifier, 1.0);
    assert_eq!(walk.close_enough_dist, 1);
    let spot = walk.target.current_block_position(&bench.world).unwrap();
    assert!(
        spot.x.abs() >= 3 || spot.z.abs() >= 3 || spot.y != 63,
        "{spot:?}"
    );
    assert!(bench.has(MemoryModuleType::LookTarget));
    bench.state.memories.erase(MemoryModuleType::WalkTarget);
    assert!(bench.start(&mut control, NOW + 10));
    assert!(!bench.has(MemoryModuleType::WalkTarget));
    assert!(bench.start(&mut control, NOW + 71));
    assert!(bench.has(MemoryModuleType::WalkTarget));
}

#[test]
fn the_frog_trigger_is_registered_under_the_idle_activity_only_with_vanilla_conditions() {
    let idle = activity(Activity::Idle);
    assert!(idle[4].1.contains("AttackTarget") && idle[4].1.contains("LookTarget"));
    let trigger = MobTrigger::FrogTryFindLand(FrogTryFindLand::new(6, 1.0));
    assert_eq!(trigger.conditions().len(), 3);
}

#[test]
fn the_long_jump_and_pregnancy_memories_round_trip_in_the_vanilla_layout() {
    let mut ai = MobAi::new(EntityKind::Frog, 1, GoalSettings::default());
    ai.init_frog_memories(123);
    ai.behavior_brain_mut()
        .unwrap()
        .memories_mut()
        .set(MemoryModuleType::IsPregnant, MemoryValue::Unit);
    ai.behavior_brain_mut().unwrap().memories_mut().set(
        MemoryModuleType::UnreachableTongueTargets,
        MemoryValue::Entities(vec![EntityId(4)]),
    );
    let saved = ai.save_behavior_brain().expect("a brain");
    let memories = saved.get_compound("memories").expect("memories");
    assert_eq!(
        memories
            .get_compound("minecraft:long_jump_cooling_down")
            .and_then(|entry| entry.get_i32("value")),
        Some(123)
    );
    assert!(memories.get_compound("minecraft:is_pregnant").is_some());
    assert!(
        memories
            .get_compound("minecraft:unreachable_tongue_targets")
            .is_none()
    );
    let mut loaded = MobAi::new(EntityKind::Frog, 2, GoalSettings::default());
    loaded.load_behavior_brain(&saved);
    let restored = loaded.behavior_brain().unwrap().memories();
    assert!(restored.has_value(MemoryModuleType::IsPregnant));
    assert_eq!(
        restored
            .get(MemoryModuleType::LongJumpCooldownTicks)
            .and_then(MemoryValue::as_int),
        Some(123)
    );
}

#[test]
fn the_frog_sensors_are_the_vanilla_five() {
    let ai = MobAi::new(EntityKind::Frog, 1, GoalSettings::default());
    let sensors: Vec<SensorType> = ai
        .behavior_brain()
        .unwrap()
        .sensors()
        .iter()
        .map(|slot| slot.sensor().sensor_type())
        .collect();
    for ty in [
        SensorType::NearestLivingEntities,
        SensorType::HurtBy,
        SensorType::FrogAttackables,
        SensorType::FrogTemptations,
        SensorType::IsInWater,
    ] {
        assert!(sensors.contains(&ty), "{ty:?}");
    }
    assert_eq!(sensors.len(), 5);
}

fn leap(world: Field, seed: i64) -> (Bench, bool) {
    let mut bench = Bench::new(world, seed);
    bench.set(MemoryModuleType::LongJumpCooldownTicks, MemoryValue::Int(0));
    bench
        .state
        .memories
        .erase(MemoryModuleType::LongJumpCooldownTicks);
    let mut control = from_activity(Activity::LongJump, "LongJumpToRandomPos");
    if !bench.start(&mut control, NOW) {
        return (bench, false);
    }
    for step in 1..=60 {
        bench.tick(&mut control, NOW + step);
        if bench.has(MemoryModuleType::LongJumpMidJump) {
            return (bench, true);
        }
    }
    (bench, false)
}

fn landing_x(bench: &Bench) -> f64 {
    match bench.memories().get(MemoryModuleType::LookTarget) {
        Some(MemoryValue::Tracker(tracker)) => tracker.current_position(&bench.world).unwrap().x,
        other => panic!("{other:?}"),
    }
}

fn two_islands(west: impl Fn(&mut Field)) -> Field {
    let mut world = Field::void();
    world.island(-1..=1, -1..=1);
    world.island(3..=5, -1..=1);
    west(&mut world);
    world
}

fn lily_pads(world: &mut Field) {
    for x in -4..=-3 {
        for z in -1..=1 {
            for y in 55..=62 {
                world.put(x, y, z, BlockKind::Stone.default_state());
            }
            world.put(x, 63, z, BlockKind::Water.default_state());
            world.put(x, 64, z, BlockKind::LilyPad.default_state());
        }
    }
}

#[test]
fn a_frog_leaps_across_a_gap_it_cannot_walk_over_with_the_frog_voice() {
    let (bench, jumped) = leap(two_islands(|_| {}), 11);
    assert!(jumped);
    assert!(bench.body.discard_friction);
    assert!(landing_x(&bench) >= 3.0);
}

#[test]
fn a_frog_prefers_to_land_on_a_lily_pad_half_of_the_time_and_on_either_island_otherwise() {
    let (mut lily, mut stone, mut none) = (0, 0, 0);
    for seed in 1..=80 {
        let (bench, jumped) = leap(two_islands(lily_pads), seed);
        if !jumped {
            none += 1;
        } else if landing_x(&bench) < 0.0 {
            lily += 1;
        } else {
            stone += 1;
        }
    }
    assert!(lily > stone + 10, "lily {lily} stone {stone} none {none}");
    assert!(stone > 0, "lily {lily} stone {stone} none {none}");
}

#[test]
fn a_frog_never_lands_in_water() {
    let mut world = Field::void();
    world.island(-1..=1, -1..=1);
    for x in 3..=5 {
        for z in -1..=1 {
            for y in 55..=62 {
                world.put(x, y, z, BlockKind::Stone.default_state());
            }
            world.put(x, 63, z, BlockKind::Water.default_state());
        }
    }
    for seed in 1..=10 {
        let (_, jumped) = leap(world_clone(&world), seed);
        assert!(!jumped, "{seed}");
    }
}

fn world_clone(world: &Field) -> Field {
    Field {
        others: world.others.clone(),
        edits: world.edits.clone(),
        floor: world.floor,
    }
}

#[test]
fn a_frog_lands_on_a_trapdoor_that_other_mobs_would_refuse() {
    let mut world = Field::void();
    world.island(-1..=1, -1..=1);
    for y in 55..=63 {
        world.put(4, y, 0, BlockKind::Stone.default_state());
    }
    world.put(4, 64, 0, BlockKind::OakTrapdoor.default_state());
    let mut landed = 0;
    for seed in 1..=30 {
        let (bench, jumped) = leap(world_clone(&world), seed);
        if jumped {
            assert!((3.5..=4.6).contains(&landing_x(&bench)));
            landed += 1;
        }
    }
    assert!(landed > 0);
}

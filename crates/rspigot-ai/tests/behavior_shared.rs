use rspigot_ai::{
    Activity, ActivityData, AdultSensor, AiContext, AiWorld, AnimalMakeLove, AwayFrom,
    BabyFollowAdult, BehaviorBrain, BehaviorControl, BehaviorCx, Brain, BrainState, DoNothing,
    DroppedItem, EntityFilter, EraseMemoryIf, Facts, FollowTemptation, Gate, GlobalPos,
    GoToTargetLocation, GoToWantedItem, Hand, InteractWith, LastDamage, Living, MeleeAttack,
    MemoryModuleType, MemoryValue, MobAction, MobBehavior, MobBody, MobControl, MobPredicate,
    MobRandom, MobTrigger, MobView, NearestLivingEntitySensor, NearestVisibleLivingEntities,
    PackedMemories, PositionTracker, RandomLookAround, RandomSource, RandomStroll, Sensor,
    SensorKind, SensorType, SetEntityLookTarget, SetEntityLookTargetSometimes, SetLookAndInteract,
    SetWalkTargetAwayFrom, SetWalkTargetFromAttackTargetIfTargetOutOfReach,
    SetWalkTargetFromLookTarget, StartAttacking, Status, StopAttackCondition,
    StopAttackingIfTargetInvalid, StopBeingAngryIfTargetDead, StrollAroundPoi, StrollToPoi,
    TargetErased, TargetFinder, TargetKind, TemptDistance, TemptItems, TemptingSensor, Timed,
    UniformInt, WalkTarget,
};
use rspigot_blocks::{BlockKind, StateId};
use rspigot_domain::damage::DamageType;
use rspigot_domain::entity::living::LivingState;
use rspigot_domain::entity::{Aabb, EntityBase};
use rspigot_domain::{BlockPos, EntityId, Vec3};
use rspigot_entities::EntityKind;
use rspigot_items::ItemKind;
use uuid::Uuid;

const HOME: Vec3 = Vec3::new(0.5, 64.0, 0.5);
const NOW: i64 = 100;

#[derive(Default)]
struct Field {
    players: Vec<Living>,
    others: Vec<Living>,
    items: Vec<DroppedItem>,
    walls: Vec<BlockPos>,
    water: Vec<BlockPos>,
    time: i64,
    dead_forgiven: bool,
}

impl rspigot_ai::BlockView for Field {
    fn state(&self, pos: BlockPos) -> Option<StateId> {
        Some(if self.water.contains(&pos) {
            BlockKind::Water.default_state()
        } else if self.walls.contains(&pos) || (60..64).contains(&pos.y) {
            BlockKind::Stone.default_state()
        } else {
            StateId::AIR
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

    fn within_world_border(&self, pos: BlockPos) -> bool {
        pos.x.abs() < 1000 && pos.z.abs() < 1000
    }

    fn living_by_uuid(&self, uuid: Uuid) -> Option<Living> {
        self.players
            .iter()
            .chain(&self.others)
            .find(|living| living.facts.uuid == uuid)
            .copied()
    }

    fn forgive_dead_players(&self) -> bool {
        self.dead_forgiven
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
            uuid: Uuid::from_u128(id as u128 + 500),
            ..Facts::default()
        },
    }
}

fn item(id: i32, position: Vec3) -> DroppedItem {
    DroppedItem {
        id: EntityId(id),
        position,
        bounds: Aabb::of_size(position, 0.25, 0.25),
        item: ItemKind::Bamboo.id(),
        alive: true,
        has_pickup_delay: false,
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
    controls: Brain,
    body: MobBody,
    state: BrainState,
}

impl Bench {
    fn new(kind: EntityKind, memories: &[MemoryModuleType], seed: i64) -> Self {
        let mob = view(kind, HOME);
        let mut state = BrainState::default();
        for ty in memories {
            state.memories.register(*ty);
        }
        Self {
            body: MobBody::of(&mob),
            mob,
            world: Field {
                time: NOW,
                ..Field::default()
            },
            controls: Brain::new(kind, seed),
            state,
        }
    }

    fn zombie(memories: &[MemoryModuleType]) -> Self {
        Self::new(EntityKind::Zombie, memories, 1)
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

    fn stop(&mut self, control: &mut MobControl, now: i64) {
        self.with(|cx| control.do_stop(cx, now));
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

    fn walk_target(&self) -> Option<WalkTarget> {
        self.get(MemoryModuleType::WalkTarget)
            .and_then(MemoryValue::as_walk_target)
            .copied()
    }

    fn look_target(&self) -> Option<PositionTracker> {
        self.get(MemoryModuleType::LookTarget)
            .and_then(MemoryValue::as_tracker)
            .copied()
    }

    fn see(&mut self, entities: &[Living]) {
        self.world.others.extend(entities.iter().copied());
        let ids = entities.iter().map(|living| living.id).collect();
        self.set(
            MemoryModuleType::NearestVisibleLivingEntities,
            MemoryValue::Visible(NearestVisibleLivingEntities::new(ids)),
        );
    }
}

fn seed_where(check: impl Fn(&mut MobRandom) -> bool) -> i64 {
    (0..100_000)
        .find(|seed| check(&mut MobRandom::new(*seed)))
        .expect("a matching seed exists")
}

fn shot(trigger: MobTrigger) -> MobControl {
    MobControl::one_shot(trigger)
}

fn timed(control: &MobControl) -> &Timed<MobBehavior> {
    match control {
        MobControl::Timed(timed) => timed,
        other => panic!("not a timed behavior: {other:?}"),
    }
}

fn entity(id: i32) -> MemoryValue {
    MemoryValue::Entity(EntityId(id))
}

fn block_target(x: i32, y: i32, z: i32) -> MemoryValue {
    MemoryValue::Tracker(PositionTracker::of_block(BlockPos::new(x, y, z)))
}

#[test]
fn swim_only_starts_in_a_fluid_deeper_than_the_jump_threshold_or_in_lava() {
    let mut bench = Bench::zombie(&[]);
    let mut swim = MobControl::swim(0.8);
    assert!(!bench.start(&mut swim, NOW));
    bench.mob.fluid.water_height = 0.3;
    assert!(!bench.start(&mut swim, NOW));
    bench.mob.fluid.water_height = 0.5;
    assert!(bench.start(&mut swim, NOW));
    assert_eq!(swim.status(), Status::Running);
}

#[test]
fn swim_starts_in_lava_and_stops_once_out_of_the_fluid() {
    let mut bench = Bench::zombie(&[]);
    let mut swim = MobControl::swim(0.8);
    bench.mob.fluid.lava_height = 0.1;
    assert!(bench.start(&mut swim, NOW));
    bench.mob.fluid.lava_height = 0.0;
    bench.tick(&mut swim, NOW + 1);
    assert_eq!(swim.status(), Status::Stopped);
}

#[test]
fn swim_jumps_with_the_configured_chance_each_tick() {
    let draw = |random: &mut MobRandom| {
        random.next_int(1);
        random.next_float()
    };
    let jumping = seed_where(|random| draw(random) < 0.8);
    let sinking = seed_where(|random| draw(random) >= 0.8);
    for (seed, expected) in [(jumping, true), (sinking, false)] {
        let mut bench = Bench::new(EntityKind::Zombie, &[], seed);
        bench.mob.fluid.water_height = 0.9;
        let mut swim = MobControl::swim(0.8);
        assert!(bench.start(&mut swim, NOW));
        bench.tick(&mut swim, NOW + 1);
        assert_eq!(bench.controls.jump.is_jumping(), expected);
    }
}

#[test]
fn swim_lasts_the_default_sixty_ticks() {
    let mut bench = Bench::zombie(&[]);
    bench.mob.fluid.water_height = 0.9;
    let mut swim = MobControl::swim(0.8);
    assert!(bench.start(&mut swim, NOW));
    assert_eq!(timed(&swim).end_timestamp(), NOW + 60);
    bench.tick(&mut swim, NOW + 60);
    assert_eq!(swim.status(), Status::Running);
    bench.tick(&mut swim, NOW + 61);
    assert_eq!(swim.status(), Status::Stopped);
}

#[test]
fn look_at_target_sink_needs_a_look_target_and_lasts_between_45_and_90_ticks() {
    let mut bench = Bench::zombie(&[MemoryModuleType::LookTarget]);
    let mut sink = MobControl::look_at_target_sink(45, 90);
    assert!(!bench.start(&mut sink, NOW));
    bench.set(MemoryModuleType::LookTarget, block_target(3, 65, 3));
    assert!(bench.start(&mut sink, NOW));
    let end = timed(&sink).end_timestamp();
    assert!((NOW + 45..=NOW + 90).contains(&end));
}

#[test]
fn look_at_target_sink_turns_the_head_towards_the_target_and_erases_it_on_stop() {
    let mut bench = Bench::zombie(&[MemoryModuleType::LookTarget]);
    let mut sink = MobControl::default_look_at_target_sink();
    bench.set(MemoryModuleType::LookTarget, block_target(3, 65, 3));
    assert!(bench.start(&mut sink, NOW));
    bench.tick(&mut sink, NOW + 1);
    assert_eq!(bench.controls.look.wanted(), Vec3::new(3.5, 65.5, 3.5));
    assert!(bench.controls.look.is_looking_at_target());
    bench.stop(&mut sink, NOW + 2);
    assert!(bench.get(MemoryModuleType::LookTarget).is_none());
}

#[test]
fn look_at_target_sink_stops_when_the_tracked_entity_is_no_longer_visible() {
    let mut bench = Bench::zombie(&[
        MemoryModuleType::LookTarget,
        MemoryModuleType::NearestVisibleLivingEntities,
    ]);
    let cow = living(7, EntityKind::Cow, Vec3::new(3.5, 64.0, 0.5));
    bench.see(&[cow]);
    bench.set(
        MemoryModuleType::LookTarget,
        MemoryValue::Tracker(PositionTracker::of_entity(EntityId(7), true)),
    );
    let mut sink = MobControl::default_look_at_target_sink();
    assert!(bench.start(&mut sink, NOW));
    bench.tick(&mut sink, NOW + 1);
    assert_eq!(sink.status(), Status::Running);
    bench.set(
        MemoryModuleType::NearestVisibleLivingEntities,
        MemoryValue::Visible(NearestVisibleLivingEntities::new(Vec::new())),
    );
    bench.tick(&mut sink, NOW + 2);
    assert_eq!(sink.status(), Status::Stopped);
    assert!(bench.get(MemoryModuleType::LookTarget).is_none());
}

const MOVE_MEMORIES: [MemoryModuleType; 3] = [
    MemoryModuleType::WalkTarget,
    MemoryModuleType::Path,
    MemoryModuleType::CantReachWalkTargetSince,
];

fn walk_to(bench: &mut Bench, x: i32, y: i32, z: i32, speed: f32, close: i32) {
    bench.set(
        MemoryModuleType::WalkTarget,
        MemoryValue::WalkTarget(WalkTarget::block(BlockPos::new(x, y, z), speed, close)),
    );
}

#[test]
fn move_to_target_sink_needs_a_walk_target_and_no_path() {
    let mut bench = Bench::zombie(&MOVE_MEMORIES);
    let mut sink = MobControl::move_to_target_sink();
    assert!(!bench.start(&mut sink, NOW));
    walk_to(&mut bench, 5, 64, 0, 1.0, 0);
    bench.set(
        MemoryModuleType::Path,
        MemoryValue::Path(rspigot_ai::Path::new(
            Vec::new(),
            BlockPos::new(0, 0, 0),
            true,
        )),
    );
    assert!(!bench.start(&mut sink, NOW));
}

#[test]
fn move_to_target_sink_computes_a_path_publishes_it_and_walks_it() {
    let mut bench = Bench::zombie(&MOVE_MEMORIES);
    walk_to(&mut bench, 5, 64, 0, 1.25, 0);
    let mut sink = MobControl::move_to_target_sink();
    assert!(bench.start(&mut sink, NOW));
    let path = bench
        .get(MemoryModuleType::Path)
        .and_then(MemoryValue::as_path);
    assert!(path.is_some_and(rspigot_ai::Path::can_reach));
    assert!(bench.controls.navigation.path().is_some());
    assert_eq!(bench.controls.navigation.speed_modifier(), 1.25);
    assert!(
        bench
            .get(MemoryModuleType::CantReachWalkTargetSince)
            .is_none()
    );
    assert!(bench.walk_target().is_some());
}

#[test]
fn move_to_target_sink_lasts_between_150_and_250_ticks() {
    let mut bench = Bench::zombie(&MOVE_MEMORIES);
    walk_to(&mut bench, 5, 64, 0, 1.0, 0);
    let mut sink = MobControl::move_to_target_sink();
    assert!(bench.start(&mut sink, NOW));
    assert!((NOW + 150..=NOW + 250).contains(&timed(&sink).end_timestamp()));
}

#[test]
fn move_to_target_sink_erases_a_walk_target_that_is_already_reached() {
    let mut bench = Bench::zombie(&MOVE_MEMORIES);
    bench.set(
        MemoryModuleType::CantReachWalkTargetSince,
        MemoryValue::Long(5),
    );
    walk_to(&mut bench, 2, 64, 0, 1.0, 2);
    let mut sink = MobControl::move_to_target_sink();
    assert!(!bench.start(&mut sink, NOW));
    assert!(bench.walk_target().is_none());
    assert!(
        bench
            .get(MemoryModuleType::CantReachWalkTargetSince)
            .is_none()
    );
}

#[test]
fn move_to_target_sink_remembers_since_when_the_target_cannot_be_reached() {
    let mut bench = Bench::zombie(&MOVE_MEMORIES);
    for (x, y, z) in [
        (9, 64, 10),
        (11, 64, 10),
        (10, 64, 9),
        (10, 64, 11),
        (10, 65, 10),
    ] {
        bench.world.walls.push(BlockPos::new(x, y, z));
    }
    walk_to(&mut bench, 10, 64, 10, 1.0, 0);
    let mut sink = MobControl::move_to_target_sink();
    assert!(bench.start(&mut sink, NOW));
    assert_eq!(
        bench
            .get(MemoryModuleType::CantReachWalkTargetSince)
            .and_then(MemoryValue::as_long),
        Some(NOW)
    );
    let path = bench
        .get(MemoryModuleType::Path)
        .and_then(MemoryValue::as_path);
    assert!(path.is_some_and(|path| !path.can_reach()));
}

#[test]
fn move_to_target_sink_stops_when_the_target_is_reached_and_clears_its_memories() {
    let mut bench = Bench::zombie(&MOVE_MEMORIES);
    walk_to(&mut bench, 5, 64, 0, 1.0, 0);
    let mut sink = MobControl::move_to_target_sink();
    assert!(bench.start(&mut sink, NOW));
    bench.mob.position = Vec3::new(5.5, 64.0, 0.5);
    bench.tick(&mut sink, NOW + 1);
    assert_eq!(sink.status(), Status::Stopped);
    assert!(bench.walk_target().is_none());
    assert!(bench.get(MemoryModuleType::Path).is_none());
    assert!(bench.controls.navigation.is_done());
}

#[test]
fn move_to_target_sink_recomputes_when_the_target_moved_more_than_two_blocks() {
    let mut bench = Bench::zombie(&MOVE_MEMORIES);
    walk_to(&mut bench, 5, 64, 0, 1.0, 0);
    let mut sink = MobControl::move_to_target_sink();
    assert!(bench.start(&mut sink, NOW));
    let first = bench.controls.navigation.target_pos();
    walk_to(&mut bench, 6, 64, 0, 1.0, 0);
    bench.tick(&mut sink, NOW + 1);
    assert_eq!(bench.controls.navigation.target_pos(), first);
    walk_to(&mut bench, 5, 64, 6, 1.0, 0);
    bench.tick(&mut sink, NOW + 2);
    assert_ne!(bench.controls.navigation.target_pos(), first);
}

fn hurt(kind: DamageType) -> MemoryValue {
    MemoryValue::HurtBy(LastDamage { kind, entity: None })
}

const PANIC_MEMORIES: [MemoryModuleType; 3] = [
    MemoryModuleType::IsPanicking,
    MemoryModuleType::HurtBy,
    MemoryModuleType::WalkTarget,
];

#[test]
fn animal_panic_starts_after_damage_tagged_as_a_panic_cause_or_while_panicking() {
    let mut bench = Bench::zombie(&PANIC_MEMORIES);
    let mut panic = MobControl::animal_panic(2.0);
    assert!(!bench.start(&mut panic, NOW));
    bench.set(MemoryModuleType::HurtBy, hurt(DamageType::Fall));
    assert!(!bench.start(&mut panic, NOW));
    bench.set(MemoryModuleType::HurtBy, hurt(DamageType::MobAttack));
    assert!(bench.start(&mut panic, NOW));
    let mut other = Bench::zombie(&PANIC_MEMORIES);
    other.set(MemoryModuleType::IsPanicking, MemoryValue::Bool(true));
    assert!(other.start(&mut MobControl::animal_panic(2.0), NOW));
}

#[test]
fn animal_panic_lasts_between_100_and_120_ticks_and_flags_the_brain() {
    let mut bench = Bench::zombie(&PANIC_MEMORIES);
    bench.set(MemoryModuleType::HurtBy, hurt(DamageType::MobAttack));
    bench.set(
        MemoryModuleType::WalkTarget,
        MemoryValue::WalkTarget(WalkTarget::block(BlockPos::new(9, 64, 9), 1.0, 0)),
    );
    let mut panic = MobControl::animal_panic(2.0);
    assert!(bench.start(&mut panic, NOW));
    assert!((NOW + 100..=NOW + 120).contains(&timed(&panic).end_timestamp()));
    assert_eq!(
        bench
            .get(MemoryModuleType::IsPanicking)
            .and_then(MemoryValue::as_bool),
        Some(true)
    );
    assert!(bench.walk_target().is_none());
}

#[test]
fn animal_panic_picks_a_flee_position_once_the_navigation_is_done_and_clears_on_stop() {
    let mut bench = Bench::zombie(&PANIC_MEMORIES);
    bench.set(MemoryModuleType::HurtBy, hurt(DamageType::MobAttack));
    let mut panic = MobControl::animal_panic(2.0);
    assert!(bench.start(&mut panic, NOW));
    bench.tick(&mut panic, NOW + 1);
    let target = bench.walk_target().expect("a flee target");
    assert_eq!(target.speed_modifier, 2.0);
    assert_eq!(target.close_enough_dist, 0);
    bench.tick(&mut panic, NOW + 120);
    bench.tick(&mut panic, NOW + 121);
    assert_eq!(panic.status(), Status::Stopped);
    assert!(bench.get(MemoryModuleType::IsPanicking).is_none());
}

#[test]
fn animal_panic_runs_to_water_when_on_fire() {
    let mut bench = Bench::zombie(&PANIC_MEMORIES);
    bench.mob.on_fire = true;
    bench.world.water.push(BlockPos::new(2, 64, 0));
    bench.set(MemoryModuleType::HurtBy, hurt(DamageType::OnFire));
    let mut panic = MobControl::animal_panic(1.0);
    assert!(bench.start(&mut panic, NOW));
    bench.tick(&mut panic, NOW + 1);
    let target = bench.walk_target().expect("a walk target");
    assert_eq!(
        target.target.current_block_position(&bench.world),
        Some(BlockPos::new(2, 64, 0))
    );
}

#[test]
fn random_look_around_needs_no_look_target_and_no_gaze_cooldown() {
    let memories = [
        MemoryModuleType::LookTarget,
        MemoryModuleType::GazeCooldownTicks,
    ];
    let mut bench = Bench::zombie(&memories);
    let look = RandomLookAround::new(UniformInt::of(150, 250), 30.0, 0.0, 0.0);
    let mut control = MobControl::random_look_around(look);
    bench.set(MemoryModuleType::GazeCooldownTicks, MemoryValue::Int(3));
    assert!(!bench.start(&mut control, NOW));
    bench
        .state
        .memories
        .erase(MemoryModuleType::GazeCooldownTicks);
    bench.set(MemoryModuleType::LookTarget, block_target(1, 1, 1));
    assert!(!bench.start(&mut control, NOW));
}

#[test]
fn random_look_around_looks_ahead_of_the_eyes_and_sets_the_gaze_cooldown() {
    let memories = [
        MemoryModuleType::LookTarget,
        MemoryModuleType::GazeCooldownTicks,
    ];
    let mut bench = Bench::zombie(&memories);
    let look = RandomLookAround::new(UniformInt::of(150, 250), 30.0, 0.0, 0.0);
    let mut control = MobControl::random_look_around(look);
    assert!(bench.start(&mut control, NOW));
    let tracker = bench.look_target().expect("a look target");
    let at = tracker.current_position(&bench.world).unwrap();
    let eyes = bench.mob.eye_position();
    assert!((at - eyes).length() > 0.99 && (at - eyes).length() < 1.01);
    assert!((at.y - eyes.y).abs() < 1e-6);
    let cooldown = bench
        .get(MemoryModuleType::GazeCooldownTicks)
        .and_then(MemoryValue::as_int)
        .unwrap();
    assert!((150..=250).contains(&cooldown));
    assert_eq!(timed(&control).end_timestamp(), NOW + 60);
}

#[test]
#[should_panic(expected = "Minimum pitch is larger than maximum pitch")]
fn random_look_around_rejects_an_inverted_pitch_range() {
    let _ = RandomLookAround::new(UniformInt::of(1, 2), 30.0, 10.0, 0.0);
}

fn in_front(bench: &mut Bench, id: i32, kind: EntityKind, x: f64) -> Living {
    let other = living(id, kind, Vec3::new(x, 64.0, 0.5));
    bench.see(&[other]);
    other
}

#[test]
fn set_walk_target_from_look_target_copies_the_look_target_into_the_walk_target() {
    let mut bench = Bench::zombie(&[MemoryModuleType::WalkTarget, MemoryModuleType::LookTarget]);
    let mut control = shot(MobTrigger::SetWalkTargetFromLookTarget(
        SetWalkTargetFromLookTarget::new(0.8, 3),
    ));
    assert!(!bench.start(&mut control, NOW));
    bench.set(MemoryModuleType::LookTarget, block_target(4, 64, 4));
    assert!(bench.start(&mut control, NOW));
    let target = bench.walk_target().unwrap();
    assert_eq!(target.target, bench.look_target().unwrap());
    assert_eq!((target.speed_modifier, target.close_enough_dist), (0.8, 3));
    bench.stop(&mut control, NOW);
    assert!(!bench.start(&mut control, NOW));
}

#[test]
fn random_stroll_sets_a_nearby_walk_target_only_without_one() {
    let mut bench = Bench::zombie(&[MemoryModuleType::WalkTarget]);
    let mut control = shot(MobTrigger::RandomStroll(RandomStroll::stroll(0.6)));
    assert!(bench.start(&mut control, NOW));
    let target = bench.walk_target().expect("a stroll target");
    assert_eq!((target.speed_modifier, target.close_enough_dist), (0.6, 0));
    let at = target.target.current_position(&bench.world).unwrap();
    assert!((at.x - HOME.x).abs() <= 11.0 && (at.z - HOME.z).abs() <= 11.0);
    assert!((at.y - HOME.y).abs() <= 7.0);
    bench.stop(&mut control, NOW);
    assert!(!bench.start(&mut control, NOW));
}

#[test]
fn random_stroll_can_refuse_to_leave_the_water() {
    let mut bench = Bench::zombie(&[MemoryModuleType::WalkTarget]);
    bench.mob.fluid.in_water = true;
    let mut refusing = shot(MobTrigger::RandomStroll(RandomStroll::stroll_from_water(
        0.6, false,
    )));
    assert!(!bench.start(&mut refusing, NOW));
    assert!(bench.walk_target().is_none());
    let mut allowing = shot(MobTrigger::RandomStroll(RandomStroll::stroll(0.6)));
    assert!(bench.start(&mut allowing, NOW));
}

#[test]
fn random_stroll_within_uses_the_given_bounds() {
    let mut bench = Bench::zombie(&[MemoryModuleType::WalkTarget]);
    let mut control = shot(MobTrigger::RandomStroll(RandomStroll::stroll_within(
        1.0, 2, 2,
    )));
    assert!(bench.start(&mut control, NOW));
    if let Some(target) = bench.walk_target() {
        let at = target.target.current_position(&bench.world).unwrap();
        assert!((at.x - HOME.x).abs() <= 3.0 && (at.z - HOME.z).abs() <= 3.0);
    }
}

const LOOK_MEMORIES: [MemoryModuleType; 2] = [
    MemoryModuleType::LookTarget,
    MemoryModuleType::NearestVisibleLivingEntities,
];

#[test]
fn set_entity_look_target_picks_the_closest_visible_match_within_range() {
    let mut bench = Bench::zombie(&LOOK_MEMORIES);
    let pig = living(5, EntityKind::Pig, Vec3::new(1.5, 64.0, 0.5));
    let far = living(6, EntityKind::Cow, Vec3::new(20.5, 64.0, 0.5));
    let near = living(7, EntityKind::Cow, Vec3::new(4.5, 64.0, 0.5));
    bench.see(&[pig, near, far]);
    let mut control = shot(MobTrigger::SetEntityLookTarget(SetEntityLookTarget::new(
        EntityFilter::Kind(EntityKind::Cow),
        8.0,
    )));
    assert!(bench.start(&mut control, NOW));
    assert_eq!(
        bench.look_target(),
        Some(PositionTracker::of_entity(EntityId(7), true))
    );
}

#[test]
fn set_entity_look_target_needs_a_free_look_slot_and_someone_in_range() {
    let mut bench = Bench::zombie(&LOOK_MEMORIES);
    let mut control = shot(MobTrigger::SetEntityLookTarget(SetEntityLookTarget::new(
        EntityFilter::Any,
        8.0,
    )));
    assert!(!bench.start(&mut control, NOW));
    in_front(&mut bench, 7, EntityKind::Cow, 12.5);
    assert!(!bench.start(&mut control, NOW));
    bench.world.others.clear();
    in_front(&mut bench, 7, EntityKind::Cow, 7.5);
    assert!(bench.start(&mut control, NOW));
    bench.stop(&mut control, NOW);
    assert!(!bench.start(&mut control, NOW));
}

#[test]
fn set_entity_look_target_sometimes_waits_out_a_ticker_drawn_from_the_interval() {
    let mut bench = Bench::zombie(&LOOK_MEMORIES);
    in_front(&mut bench, 7, EntityKind::Cow, 3.5);
    let mut control = shot(MobTrigger::SetEntityLookTargetSometimes(
        SetEntityLookTargetSometimes::new(EntityFilter::Any, 8.0, UniformInt::of(3, 3)),
    ));
    let mut fired = Vec::new();
    for tick in 0..8 {
        let started = bench.start(&mut control, NOW);
        bench.stop(&mut control, NOW);
        bench.state.memories.erase(MemoryModuleType::LookTarget);
        if started {
            fired.push(tick);
        }
    }
    assert_eq!(fired, vec![2, 5]);
}

#[test]
fn set_entity_look_target_sometimes_only_ticks_while_someone_is_in_range() {
    let mut bench = Bench::zombie(&LOOK_MEMORIES);
    let mut control = shot(MobTrigger::SetEntityLookTargetSometimes(
        SetEntityLookTargetSometimes::new(EntityFilter::Any, 8.0, UniformInt::of(2, 2)),
    ));
    bench.see(&[]);
    for _ in 0..5 {
        assert!(!bench.start(&mut control, NOW));
    }
    in_front(&mut bench, 7, EntityKind::Cow, 3.5);
    assert!(!bench.start(&mut control, NOW));
    assert!(bench.start(&mut control, NOW));
}

#[test]
#[should_panic(expected = "Interval min value must be greater than 1")]
fn set_entity_look_target_sometimes_rejects_a_tiny_interval() {
    let _ = SetEntityLookTargetSometimes::new(EntityFilter::Any, 8.0, UniformInt::of(1, 5));
}

const FOLLOW_MEMORIES: [MemoryModuleType; 3] = [
    MemoryModuleType::LookTarget,
    MemoryModuleType::WalkTarget,
    MemoryModuleType::NearestVisibleAdult,
];

fn follow(bench: &mut Bench, distance: f64, baby: bool) -> MobControl {
    bench.mob.baby = baby;
    let mut adult = living(9, EntityKind::Zombie, Vec3::new(distance, 64.0, 0.5));
    adult.baby = false;
    bench.world.others.push(adult);
    bench.set(MemoryModuleType::NearestVisibleAdult, entity(9));
    shot(MobTrigger::BabyFollowAdult(BabyFollowAdult::new(
        UniformInt::of(5, 16),
        1.25,
    )))
}

#[test]
fn baby_follow_adult_walks_a_baby_to_an_adult_between_the_follow_range_bounds() {
    let mut bench = Bench::zombie(&FOLLOW_MEMORIES);
    let mut control = follow(&mut bench, 10.5, true);
    assert!(bench.start(&mut control, NOW));
    assert_eq!(
        bench.look_target(),
        Some(PositionTracker::of_entity(EntityId(9), true))
    );
    let walk = bench.walk_target().unwrap();
    assert_eq!(walk.target, PositionTracker::of_entity(EntityId(9), false));
    assert_eq!((walk.speed_modifier, walk.close_enough_dist), (1.25, 4));
}

#[test]
fn baby_follow_adult_does_nothing_for_adults_or_adults_too_near_or_too_far() {
    for (distance, baby) in [(10.5, false), (3.5, true), (30.5, true)] {
        let mut bench = Bench::zombie(&FOLLOW_MEMORIES);
        let mut control = follow(&mut bench, distance, baby);
        assert!(!bench.start(&mut control, NOW));
        assert!(bench.walk_target().is_none());
    }
}

#[test]
fn baby_follow_adult_can_aim_for_the_eyes_and_follow_another_memory() {
    let mut bench = Bench::zombie(&[
        MemoryModuleType::LookTarget,
        MemoryModuleType::WalkTarget,
        MemoryModuleType::NearestVisibleAdultPiglin,
    ]);
    bench.mob.baby = true;
    bench
        .world
        .others
        .push(living(9, EntityKind::Piglin, Vec3::new(8.5, 64.0, 0.5)));
    bench.set(MemoryModuleType::NearestVisibleAdultPiglin, entity(9));
    let mut control = shot(MobTrigger::BabyFollowAdult(BabyFollowAdult::of(
        UniformInt::of(2, 16),
        0.6,
        MemoryModuleType::NearestVisibleAdultPiglin,
        true,
    )));
    assert!(bench.start(&mut control, NOW));
    assert_eq!(
        bench.look_target(),
        Some(PositionTracker::Entity {
            id: EntityId(9),
            track_eye_height: true,
            target_eye_height: true,
        })
    );
}

fn away_from_entity(bench: &mut Bench, interrupt: bool) -> MobControl {
    bench
        .world
        .others
        .push(living(4, EntityKind::Pig, Vec3::new(2.5, 64.0, 0.5)));
    bench.set(MemoryModuleType::AvoidTarget, entity(4));
    shot(MobTrigger::SetWalkTargetAwayFrom(
        SetWalkTargetAwayFrom::entity(MemoryModuleType::AvoidTarget, 1.3, 8, interrupt),
    ))
}

const AWAY_MEMORIES: [MemoryModuleType; 2] =
    [MemoryModuleType::WalkTarget, MemoryModuleType::AvoidTarget];

#[test]
fn set_walk_target_away_from_flees_a_nearby_entity_away_from_it() {
    let mut bench = Bench::zombie(&AWAY_MEMORIES);
    let mut control = away_from_entity(&mut bench, false);
    assert!(bench.start(&mut control, NOW));
    let target = bench.walk_target().expect("a flee target");
    assert_eq!((target.speed_modifier, target.close_enough_dist), (1.3, 0));
    let at = target.target.current_position(&bench.world).unwrap();
    assert!(at.x < HOME.x);
}

#[test]
fn set_walk_target_away_from_ignores_entities_beyond_the_desired_distance() {
    let mut bench = Bench::zombie(&AWAY_MEMORIES);
    let mut control = away_from_entity(&mut bench, false);
    bench.world.others[0].position = Vec3::new(20.5, 64.0, 0.5);
    assert!(!bench.start(&mut control, NOW));
    assert!(bench.walk_target().is_none());
}

#[test]
fn set_walk_target_away_from_respects_a_current_walk_unless_allowed_to_interrupt() {
    let mut bench = Bench::zombie(&AWAY_MEMORIES);
    let mut control = away_from_entity(&mut bench, false);
    walk_to(&mut bench, 5, 64, 5, 1.3, 0);
    assert!(!bench.start(&mut control, NOW));
    let mut interrupting = away_from_entity(&mut bench, true);
    assert!(bench.start(&mut interrupting, NOW));
}

#[test]
fn set_walk_target_away_from_keeps_a_walk_that_already_leads_away_at_the_same_speed() {
    let mut bench = Bench::zombie(&AWAY_MEMORIES);
    let mut control = away_from_entity(&mut bench, true);
    walk_to(&mut bench, -5, 64, 0, 1.3, 0);
    let before = bench.walk_target();
    assert!(!bench.start(&mut control, NOW));
    assert_eq!(bench.walk_target(), before);
    walk_to(&mut bench, 8, 64, 0, 1.3, 0);
    assert!(bench.start(&mut control, NOW));
    let at = bench
        .walk_target()
        .unwrap()
        .target
        .current_position(&bench.world)
        .unwrap();
    assert!(at.x < HOME.x);
}

#[test]
fn set_walk_target_away_from_a_block_uses_the_bottom_centre_of_the_position() {
    let mut bench = Bench::zombie(&[
        MemoryModuleType::WalkTarget,
        MemoryModuleType::NearestRepellent,
    ]);
    bench.set(
        MemoryModuleType::NearestRepellent,
        MemoryValue::Block(BlockPos::new(3, 64, 0)),
    );
    let mut control = shot(MobTrigger::SetWalkTargetAwayFrom(
        SetWalkTargetAwayFrom::pos(MemoryModuleType::NearestRepellent, 1.0, 8, true),
    ));
    assert!(bench.start(&mut control, NOW));
    let at = bench
        .walk_target()
        .unwrap()
        .target
        .current_position(&bench.world)
        .unwrap();
    assert!(at.x < HOME.x);
    assert_eq!(AwayFrom::Block, AwayFrom::Block);
}

const ITEM_MEMORIES: [MemoryModuleType; 4] = [
    MemoryModuleType::LookTarget,
    MemoryModuleType::WalkTarget,
    MemoryModuleType::NearestVisibleWantedItem,
    MemoryModuleType::ItemPickupCooldownTicks,
];

fn wanted(bench: &mut Bench, x: f64) {
    bench.world.items.push(item(30, Vec3::new(x, 64.0, 0.5)));
    bench.set(MemoryModuleType::NearestVisibleWantedItem, entity(30));
    bench.mob.can_pick_up_loot = true;
}

#[test]
fn go_to_wanted_item_looks_at_and_walks_to_a_close_item() {
    let mut bench = Bench::zombie(&ITEM_MEMORIES);
    wanted(&mut bench, 3.5);
    let mut control = shot(MobTrigger::GoToWantedItem(GoToWantedItem::new(
        0.5, false, 4,
    )));
    assert!(bench.start(&mut control, NOW));
    assert_eq!(
        bench.look_target(),
        Some(PositionTracker::of_entity(EntityId(30), true))
    );
    let walk = bench.walk_target().unwrap();
    assert_eq!(walk.target, PositionTracker::of_entity(EntityId(30), false));
    assert_eq!((walk.speed_modifier, walk.close_enough_dist), (0.5, 0));
    assert_eq!(
        walk.target.current_block_position(&bench.world),
        Some(BlockPos::new(3, 64, 0))
    );
}

#[test]
fn go_to_wanted_item_refuses_far_items_cooldowns_and_mobs_that_cannot_pick_up_loot() {
    let mut control = shot(MobTrigger::GoToWantedItem(GoToWantedItem::new(
        0.5, false, 4,
    )));
    let mut far = Bench::zombie(&ITEM_MEMORIES);
    wanted(&mut far, 9.5);
    assert!(!far.start(&mut control, NOW));
    let mut cooling = Bench::zombie(&ITEM_MEMORIES);
    wanted(&mut cooling, 3.5);
    cooling.set(
        MemoryModuleType::ItemPickupCooldownTicks,
        MemoryValue::Int(5),
    );
    assert!(!cooling.start(&mut control, NOW));
    let mut weak = Bench::zombie(&ITEM_MEMORIES);
    wanted(&mut weak, 3.5);
    weak.mob.can_pick_up_loot = false;
    assert!(!weak.start(&mut control, NOW));
}

#[test]
fn go_to_wanted_item_only_interrupts_a_walk_when_asked_to() {
    let mut keep = Bench::zombie(&ITEM_MEMORIES);
    wanted(&mut keep, 3.5);
    walk_to(&mut keep, 5, 64, 5, 1.0, 0);
    let mut polite = shot(MobTrigger::GoToWantedItem(GoToWantedItem::new(
        0.5, false, 4,
    )));
    assert!(!keep.start(&mut polite, NOW));
    let mut rude = shot(MobTrigger::GoToWantedItem(GoToWantedItem::new(
        0.5, true, 4,
    )));
    assert!(keep.start(&mut rude, NOW));
}

const LOCATION_MEMORIES: [MemoryModuleType; 4] = [
    MemoryModuleType::DisturbanceLocation,
    MemoryModuleType::AttackTarget,
    MemoryModuleType::WalkTarget,
    MemoryModuleType::LookTarget,
];

#[test]
fn go_to_target_location_walks_to_a_spot_next_to_a_far_location() {
    let mut bench = Bench::zombie(&LOCATION_MEMORIES);
    bench.set(
        MemoryModuleType::DisturbanceLocation,
        MemoryValue::Block(BlockPos::new(10, 64, 10)),
    );
    let mut control = shot(MobTrigger::GoToTargetLocation(GoToTargetLocation::new(
        MemoryModuleType::DisturbanceLocation,
        2,
        0.7,
    )));
    assert!(bench.start(&mut control, NOW));
    let walk = bench.walk_target().unwrap();
    assert_eq!((walk.speed_modifier, walk.close_enough_dist), (0.7, 2));
    let spot = walk.target.current_block_position(&bench.world).unwrap();
    assert!((9..=11).contains(&spot.x) && (9..=11).contains(&spot.z) && spot.y == 64);
    assert_eq!(bench.look_target(), Some(walk.target));
}

#[test]
fn go_to_target_location_leaves_everything_alone_when_close_enough() {
    let mut bench = Bench::zombie(&LOCATION_MEMORIES);
    bench.set(
        MemoryModuleType::DisturbanceLocation,
        MemoryValue::Block(BlockPos::new(1, 64, 0)),
    );
    let mut control = shot(MobTrigger::GoToTargetLocation(GoToTargetLocation::new(
        MemoryModuleType::DisturbanceLocation,
        2,
        0.7,
    )));
    assert!(bench.start(&mut control, NOW));
    assert!(bench.walk_target().is_none());
    bench.set(MemoryModuleType::AttackTarget, entity(3));
    bench.stop(&mut control, NOW);
    assert!(!bench.start(&mut control, NOW));
}

const ATTACK_MEMORIES: [MemoryModuleType; 5] = [
    MemoryModuleType::AttackTarget,
    MemoryModuleType::CantReachWalkTargetSince,
    MemoryModuleType::NearestAttackable,
    MemoryModuleType::LookTarget,
    MemoryModuleType::WalkTarget,
];

fn starter(predicate: MobPredicate) -> MobControl {
    shot(MobTrigger::StartAttacking(StartAttacking::when(
        predicate,
        TargetFinder::Memory(MemoryModuleType::NearestAttackable),
    )))
}

#[test]
fn start_attacking_picks_the_found_target_and_forgets_that_it_was_unreachable() {
    let mut bench = Bench::zombie(&ATTACK_MEMORIES);
    bench
        .world
        .others
        .push(living(3, EntityKind::Cow, Vec3::new(4.5, 64.0, 0.5)));
    bench.set(MemoryModuleType::NearestAttackable, entity(3));
    bench.set(
        MemoryModuleType::CantReachWalkTargetSince,
        MemoryValue::Long(2),
    );
    let mut control = starter(MobPredicate::Always);
    assert!(bench.start(&mut control, NOW));
    assert_eq!(bench.get(MemoryModuleType::AttackTarget), Some(&entity(3)));
    assert!(
        bench
            .get(MemoryModuleType::CantReachWalkTargetSince)
            .is_none()
    );
}

#[test]
fn start_attacking_needs_a_target_a_free_slot_a_passing_predicate_and_an_attackable_victim() {
    let mut bench = Bench::zombie(&ATTACK_MEMORIES);
    let mut control = starter(MobPredicate::Always);
    assert!(!bench.start(&mut control, NOW));
    let mut victim = living(3, EntityKind::Cow, Vec3::new(4.5, 64.0, 0.5));
    victim.invulnerable = true;
    bench.world.others.push(victim);
    bench.set(MemoryModuleType::NearestAttackable, entity(3));
    assert!(!bench.start(&mut control, NOW));
    bench.world.others[0].invulnerable = false;
    let mut refused = starter(MobPredicate::IsBaby);
    assert!(!bench.start(&mut refused, NOW));
    bench.set(MemoryModuleType::AttackTarget, entity(8));
    assert!(!bench.start(&mut control, NOW));
}

#[test]
fn start_attacking_never_picks_a_player_on_peaceful_or_a_ghast() {
    let mut bench = Bench::zombie(&ATTACK_MEMORIES);
    bench
        .world
        .others
        .push(living(3, EntityKind::Ghast, Vec3::new(4.5, 64.0, 0.5)));
    bench.set(MemoryModuleType::NearestAttackable, entity(3));
    let mut control = starter(MobPredicate::Always);
    assert!(!bench.start(&mut control, NOW));
}

fn stopper(can_grow_tired: bool) -> MobControl {
    shot(MobTrigger::StopAttackingIfTargetInvalid(
        StopAttackingIfTargetInvalid::new(
            StopAttackCondition::Never,
            TargetErased::Nothing,
            can_grow_tired,
        ),
    ))
}

#[test]
fn stop_attacking_keeps_a_living_attackable_target() {
    let mut bench = Bench::zombie(&ATTACK_MEMORIES);
    bench
        .world
        .others
        .push(living(3, EntityKind::Cow, Vec3::new(4.5, 64.0, 0.5)));
    bench.set(MemoryModuleType::AttackTarget, entity(3));
    let mut control = stopper(true);
    assert!(bench.start(&mut control, NOW));
    assert_eq!(bench.get(MemoryModuleType::AttackTarget), Some(&entity(3)));
}

#[test]
fn stop_attacking_forgets_dead_missing_or_unattackable_targets() {
    for case in 0..3 {
        let mut bench = Bench::zombie(&ATTACK_MEMORIES);
        let mut victim = living(3, EntityKind::Cow, Vec3::new(4.5, 64.0, 0.5));
        match case {
            0 => victim.alive = false,
            1 => victim.invulnerable = true,
            _ => {}
        }
        if case < 2 {
            bench.world.others.push(victim);
        }
        bench.set(MemoryModuleType::AttackTarget, entity(3));
        let mut control = stopper(true);
        assert!(bench.start(&mut control, NOW));
        assert!(
            bench.get(MemoryModuleType::AttackTarget).is_none(),
            "case {case}"
        );
    }
}

#[test]
fn stop_attacking_gives_up_on_a_target_unreachable_for_more_than_200_ticks() {
    let mut bench = Bench::zombie(&ATTACK_MEMORIES);
    bench
        .world
        .others
        .push(living(3, EntityKind::Cow, Vec3::new(4.5, 64.0, 0.5)));
    bench.set(MemoryModuleType::AttackTarget, entity(3));
    bench.set(
        MemoryModuleType::CantReachWalkTargetSince,
        MemoryValue::Long(NOW - 200),
    );
    assert!(bench.start(&mut stopper(true), NOW));
    assert!(bench.get(MemoryModuleType::AttackTarget).is_some());
    bench.set(
        MemoryModuleType::CantReachWalkTargetSince,
        MemoryValue::Long(NOW - 201),
    );
    assert!(bench.start(&mut stopper(false), NOW));
    assert!(bench.get(MemoryModuleType::AttackTarget).is_some());
    assert!(bench.start(&mut stopper(true), NOW));
    assert!(bench.get(MemoryModuleType::AttackTarget).is_none());
}

const MELEE_MEMORIES: [MemoryModuleType; 4] = [
    MemoryModuleType::LookTarget,
    MemoryModuleType::AttackTarget,
    MemoryModuleType::AttackCoolingDown,
    MemoryModuleType::NearestVisibleLivingEntities,
];

fn melee_bench(distance: f64) -> Bench {
    let mut bench = Bench::zombie(&MELEE_MEMORIES);
    in_front(&mut bench, 3, EntityKind::Cow, distance);
    bench.set(MemoryModuleType::AttackTarget, entity(3));
    bench
}

#[test]
fn melee_attack_swings_hits_looks_and_starts_the_cooldown() {
    let mut bench = melee_bench(1.5);
    let mut control = shot(MobTrigger::MeleeAttack(MeleeAttack::new(40)));
    assert!(bench.start(&mut control, NOW));
    assert_eq!(
        bench.actions(),
        vec![MobAction::Swing(Hand::Main), MobAction::Attack(EntityId(3))]
    );
    assert_eq!(
        bench.look_target(),
        Some(PositionTracker::of_entity(EntityId(3), true))
    );
    assert_eq!(
        bench
            .state
            .memories
            .time_until_expiry(MemoryModuleType::AttackCoolingDown),
        Some(40)
    );
    bench.stop(&mut control, NOW);
    assert!(!bench.start(&mut control, NOW));
}

#[test]
fn melee_attack_waits_for_the_target_to_be_in_reach_and_visible() {
    let mut bench = melee_bench(6.5);
    let mut control = shot(MobTrigger::MeleeAttack(MeleeAttack::new(20)));
    assert!(!bench.start(&mut control, NOW));
    assert!(bench.actions().is_empty());
    bench.world.others[0].position = Vec3::new(1.5, 64.0, 0.5);
    bench.world.others[0].bounds = Aabb::of_size(Vec3::new(1.5, 64.0, 0.5), 0.9, 1.4);
    bench.set(
        MemoryModuleType::NearestVisibleLivingEntities,
        MemoryValue::Visible(NearestVisibleLivingEntities::new(Vec::new())),
    );
    assert!(!bench.start(&mut control, NOW));
}

#[test]
fn melee_attack_honours_its_predicate() {
    let mut bench = melee_bench(1.5);
    let mut baby_only = shot(MobTrigger::MeleeAttack(MeleeAttack::when(
        MobPredicate::IsBaby,
        15,
    )));
    assert!(!bench.start(&mut baby_only, NOW));
    bench.mob.baby = true;
    assert!(bench.start(&mut baby_only, NOW));
    assert_eq!(
        bench
            .state
            .memories
            .time_until_expiry(MemoryModuleType::AttackCoolingDown),
        Some(15)
    );
}

const REACH_MEMORIES: [MemoryModuleType; 4] = [
    MemoryModuleType::WalkTarget,
    MemoryModuleType::LookTarget,
    MemoryModuleType::AttackTarget,
    MemoryModuleType::NearestVisibleLivingEntities,
];

fn reach_bench(distance: f64) -> Bench {
    let mut bench = Bench::zombie(&REACH_MEMORIES);
    in_front(&mut bench, 3, EntityKind::Cow, distance);
    bench.set(MemoryModuleType::AttackTarget, entity(3));
    bench
}

#[test]
fn walk_to_an_attack_target_out_of_reach_looks_and_walks_towards_it() {
    let mut bench = reach_bench(8.5);
    let mut control = shot(MobTrigger::SetWalkTargetFromAttackTargetIfTargetOutOfReach(
        SetWalkTargetFromAttackTargetIfTargetOutOfReach::new(1.2),
    ));
    assert!(bench.start(&mut control, NOW));
    assert_eq!(
        bench.look_target(),
        Some(PositionTracker::of_entity(EntityId(3), true))
    );
    let walk = bench.walk_target().unwrap();
    assert_eq!(walk.target, PositionTracker::of_entity(EntityId(3), false));
    assert_eq!((walk.speed_modifier, walk.close_enough_dist), (1.2, 0));
}

#[test]
fn walk_to_an_attack_target_in_reach_clears_the_walk_target() {
    let mut bench = reach_bench(1.5);
    walk_to(&mut bench, 8, 64, 8, 1.0, 0);
    let mut control = shot(MobTrigger::SetWalkTargetFromAttackTargetIfTargetOutOfReach(
        SetWalkTargetFromAttackTargetIfTargetOutOfReach::new(1.2),
    ));
    assert!(bench.start(&mut control, NOW));
    assert!(bench.walk_target().is_none());
}

#[test]
fn erase_memory_if_erases_the_memory_only_when_the_predicate_holds() {
    let mut bench = Bench::zombie(&[MemoryModuleType::AttackTarget]);
    let mut control = shot(MobTrigger::EraseMemoryIf(EraseMemoryIf::new(
        MobPredicate::IsBaby,
        MemoryModuleType::AttackTarget,
    )));
    assert!(!bench.start(&mut control, NOW));
    bench.set(MemoryModuleType::AttackTarget, entity(3));
    assert!(!bench.start(&mut control, NOW));
    assert!(bench.get(MemoryModuleType::AttackTarget).is_some());
    bench.mob.baby = true;
    assert!(bench.start(&mut control, NOW));
    assert!(bench.get(MemoryModuleType::AttackTarget).is_none());
}

#[test]
fn erase_memory_if_can_test_another_memory() {
    let mut bench = Bench::zombie(&[
        MemoryModuleType::AttackTarget,
        MemoryModuleType::BreedTarget,
    ]);
    bench.set(MemoryModuleType::AttackTarget, entity(3));
    let mut control = shot(MobTrigger::EraseMemoryIf(EraseMemoryIf::new(
        MobPredicate::HasMemory(MemoryModuleType::BreedTarget),
        MemoryModuleType::AttackTarget,
    )));
    assert!(!bench.start(&mut control, NOW));
    bench.set(MemoryModuleType::BreedTarget, entity(4));
    assert!(bench.start(&mut control, NOW));
    assert!(bench.get(MemoryModuleType::AttackTarget).is_none());
}

fn angry_at(bench: &mut Bench, victim: Living) -> MobControl {
    let uuid = victim.facts.uuid;
    if victim.player {
        bench.world.players.push(victim);
    } else {
        bench.world.others.push(victim);
    }
    bench.set(MemoryModuleType::AngryAt, MemoryValue::Uuid(uuid));
    shot(MobTrigger::StopBeingAngryIfTargetDead(
        StopBeingAngryIfTargetDead,
    ))
}

#[test]
fn stop_being_angry_forgets_a_dead_target_and_keeps_a_living_one() {
    let mut alive = Bench::zombie(&[MemoryModuleType::AngryAt]);
    let mut control = angry_at(&mut alive, living(3, EntityKind::Cow, HOME));
    assert!(alive.start(&mut control, NOW));
    assert!(alive.get(MemoryModuleType::AngryAt).is_some());
    let mut dead = Bench::zombie(&[MemoryModuleType::AngryAt]);
    let mut victim = living(3, EntityKind::Cow, HOME);
    victim.alive = false;
    let mut control = angry_at(&mut dead, victim);
    assert!(dead.start(&mut control, NOW));
    assert!(dead.get(MemoryModuleType::AngryAt).is_none());
}

#[test]
fn stop_being_angry_at_a_dead_player_follows_the_forgive_dead_players_rule() {
    for forgive in [false, true] {
        let mut bench = Bench::zombie(&[MemoryModuleType::AngryAt]);
        bench.world.dead_forgiven = forgive;
        let mut victim = living(3, EntityKind::Player, HOME);
        victim.alive = false;
        let mut control = angry_at(&mut bench, victim);
        assert!(bench.start(&mut control, NOW));
        assert_eq!(bench.get(MemoryModuleType::AngryAt).is_none(), forgive);
    }
}

#[test]
fn trigger_if_runs_the_wrapped_one_shot_only_when_the_predicate_holds() {
    let mut bench = Bench::zombie(&[MemoryModuleType::WalkTarget, MemoryModuleType::LookTarget]);
    bench.set(MemoryModuleType::LookTarget, block_target(4, 64, 4));
    let inner = MobControl::declared(MobTrigger::SetWalkTargetFromLookTarget(
        SetWalkTargetFromLookTarget::new(1.0, 3),
    ));
    let mut control = MobControl::trigger_if(MobPredicate::IsAdult.negate(), inner.clone());
    assert!(!bench.start(&mut control, NOW));
    assert!(bench.walk_target().is_none());
    let mut adult = MobControl::trigger_if(MobPredicate::IsAdult, inner);
    assert!(bench.start(&mut adult, NOW));
    assert!(bench.walk_target().is_some());
}

#[test]
fn trigger_one_shuffled_runs_a_single_one_shot() {
    let mut bench = Bench::zombie(&[MemoryModuleType::WalkTarget, MemoryModuleType::LookTarget]);
    bench.set(MemoryModuleType::LookTarget, block_target(4, 64, 4));
    let stroll = MobControl::declared(MobTrigger::RandomStroll(RandomStroll::stroll(0.4)));
    let look = MobControl::declared(MobTrigger::SetWalkTargetFromLookTarget(
        SetWalkTargetFromLookTarget::new(0.5, 3),
    ));
    let mut control = MobControl::trigger_one_shuffled(vec![(stroll, 1), (look, 1)]);
    assert!(bench.start(&mut control, NOW));
    let speed = bench.walk_target().map(|target| target.speed_modifier);
    assert!(matches!(speed, Some(0.4) | Some(0.5) | None));
}

fn love_bench(partner_in_love: bool) -> Bench {
    let mut bench = Bench::new(
        EntityKind::Cow,
        &[
            MemoryModuleType::NearestVisibleLivingEntities,
            MemoryModuleType::BreedTarget,
            MemoryModuleType::WalkTarget,
            MemoryModuleType::LookTarget,
            MemoryModuleType::IsPanicking,
        ],
        4,
    );
    bench.mob.in_love = true;
    let mut partner = living(8, EntityKind::Cow, Vec3::new(2.5, 64.0, 0.5));
    partner.in_love = partner_in_love;
    bench.see(&[partner]);
    bench
}

fn make_love() -> MobControl {
    MobControl::animal_make_love(AnimalMakeLove::new(EntityKind::Cow))
}

#[test]
fn animal_make_love_pairs_two_animals_in_love_and_locks_their_gaze() {
    let mut bench = love_bench(true);
    let mut control = make_love();
    assert!(bench.start(&mut control, NOW));
    assert_eq!(bench.get(MemoryModuleType::BreedTarget), Some(&entity(8)));
    assert_eq!(
        bench.look_target(),
        Some(PositionTracker::of_entity(EntityId(8), true))
    );
    let walk = bench.walk_target().unwrap();
    assert_eq!(walk.target, PositionTracker::of_entity(EntityId(8), false));
    assert_eq!((walk.speed_modifier, walk.close_enough_dist), (1.0, 2));
    assert_eq!(timed(&control).end_timestamp(), NOW + 110);
    let actions = bench.actions();
    assert!(actions.contains(&MobAction::SetMemory {
        mob: EntityId(8),
        memory: MemoryModuleType::BreedTarget,
        value: Some(entity(1)),
    }));
    assert!(actions.contains(&MobAction::SetMemory {
        mob: EntityId(8),
        memory: MemoryModuleType::WalkTarget,
        value: Some(MemoryValue::WalkTarget(WalkTarget {
            target: PositionTracker::of_entity(EntityId(1), false),
            speed_modifier: 1.0,
            close_enough_dist: 2,
        })),
    }));
    assert!(actions.contains(&MobAction::SetMemory {
        mob: EntityId(8),
        memory: MemoryModuleType::LookTarget,
        value: Some(MemoryValue::Tracker(PositionTracker::of_entity(
            EntityId(1),
            true
        ))),
    }));
}

#[test]
fn animal_make_love_needs_both_animals_in_love_and_the_partner_not_panicking() {
    let mut cold = love_bench(false);
    assert!(!cold.start(&mut make_love(), NOW));
    let mut single = love_bench(true);
    single.mob.in_love = false;
    assert!(!single.start(&mut make_love(), NOW));
    let mut scared = love_bench(true);
    scared.world.others[0].panicking = true;
    assert!(!scared.start(&mut make_love(), NOW));
    let mut hurried = love_bench(true);
    hurried.set(MemoryModuleType::IsPanicking, MemoryValue::Bool(true));
    assert!(!hurried.start(&mut make_love(), NOW));
    let mut busy = love_bench(true);
    busy.set(MemoryModuleType::BreedTarget, entity(8));
    assert!(!busy.start(&mut make_love(), NOW));
    let mut other_kind = love_bench(true);
    let mut wrong = MobControl::animal_make_love(AnimalMakeLove::new(EntityKind::Pig));
    assert!(!other_kind.start(&mut wrong, NOW));
}

#[test]
fn animal_make_love_spawns_the_child_between_sixty_and_one_hundred_nine_ticks_later() {
    let seed = 4;
    let mut random = MobRandom::new(seed);
    random.next_int(1);
    let delay = i64::from(random.next_int(50));
    let mut bench = love_bench(true);
    let mut control = make_love();
    assert!(bench.start(&mut control, NOW));
    bench.actions();
    bench.world.others[0].position = Vec3::new(1.5, 64.0, 0.5);
    let due = NOW + 60 + delay;
    bench.tick(&mut control, due - 1);
    assert!(
        !bench
            .actions()
            .iter()
            .any(|action| matches!(action, MobAction::Breed(_)))
    );
    bench.tick(&mut control, due);
    let actions = bench.actions();
    assert!(actions.contains(&MobAction::Breed(EntityId(8))));
    assert!(actions.contains(&MobAction::SetMemory {
        mob: EntityId(8),
        memory: MemoryModuleType::BreedTarget,
        value: None,
    }));
    assert!(bench.get(MemoryModuleType::BreedTarget).is_none());
}

#[test]
fn animal_make_love_does_not_breed_while_the_partner_is_three_blocks_away_or_more() {
    let mut bench = love_bench(true);
    let mut control = make_love();
    assert!(bench.start(&mut control, NOW));
    bench.actions();
    bench.world.others[0].position = Vec3::new(5.5, 64.0, 0.5);
    bench.tick(&mut control, NOW + 109);
    assert!(
        !bench
            .actions()
            .iter()
            .any(|action| matches!(action, MobAction::Breed(_)))
    );
}

#[test]
fn animal_make_love_stops_when_the_partner_dies_or_the_time_is_up_and_cleans_up() {
    let mut bench = love_bench(true);
    let mut control = make_love();
    assert!(bench.start(&mut control, NOW));
    bench.world.others[0].alive = false;
    bench.tick(&mut control, NOW + 1);
    assert_eq!(control.status(), Status::Stopped);
    assert!(bench.get(MemoryModuleType::BreedTarget).is_none());
    assert!(bench.get(MemoryModuleType::WalkTarget).is_none());
    assert!(bench.get(MemoryModuleType::LookTarget).is_none());
}

const TEMPT_MEMORIES: [MemoryModuleType; 6] = [
    MemoryModuleType::LookTarget,
    MemoryModuleType::WalkTarget,
    MemoryModuleType::TemptationCooldownTicks,
    MemoryModuleType::TemptingPlayer,
    MemoryModuleType::BreedTarget,
    MemoryModuleType::IsPanicking,
];

fn tempting(distance: f64) -> Bench {
    let mut bench = Bench::new(EntityKind::Cow, &TEMPT_MEMORIES, 2);
    bench.world.players.push(living(
        20,
        EntityKind::Player,
        Vec3::new(distance, 64.0, 0.5),
    ));
    bench.set(MemoryModuleType::TemptingPlayer, entity(20));
    bench
}

#[test]
fn follow_temptation_walks_towards_a_player_further_than_the_close_enough_distance() {
    let mut bench = tempting(6.5);
    let mut control = MobControl::follow_temptation(FollowTemptation::new(1.25));
    assert!(bench.start(&mut control, NOW));
    bench.tick(&mut control, NOW + 1);
    assert_eq!(
        bench.look_target(),
        Some(PositionTracker::of_entity(EntityId(20), true))
    );
    let walk = bench.walk_target().unwrap();
    assert_eq!(walk.target, PositionTracker::of_entity(EntityId(20), false));
    assert_eq!((walk.speed_modifier, walk.close_enough_dist), (1.25, 2));
}

#[test]
fn follow_temptation_stops_walking_once_within_two_and_a_half_blocks() {
    let mut bench = tempting(6.5);
    let mut control = MobControl::follow_temptation(FollowTemptation::new(1.25));
    assert!(bench.start(&mut control, NOW));
    bench.tick(&mut control, NOW + 1);
    bench.world.players[0].position = Vec3::new(2.5, 64.0, 0.5);
    bench.tick(&mut control, NOW + 2);
    assert!(bench.walk_target().is_none());
    assert!(bench.look_target().is_some());
}

#[test]
fn follow_temptation_can_aim_for_the_eyes_and_scale_its_distance_with_the_age() {
    let mut bench = tempting(3.0);
    bench.mob.baby = true;
    let follow = FollowTemptation::with_distance(
        1.0,
        TemptDistance::ByAge {
            baby: 2.5,
            adult: 3.5,
        },
        true,
    );
    let mut control = MobControl::follow_temptation(follow);
    assert!(bench.start(&mut control, NOW));
    bench.tick(&mut control, NOW + 1);
    assert_eq!(
        bench.walk_target().unwrap().target,
        PositionTracker::of_entity_targeting_eyes(EntityId(20), true)
    );
    bench.mob.baby = false;
    bench.tick(&mut control, NOW + 2);
    assert!(bench.walk_target().is_none());
}

#[test]
fn follow_temptation_never_times_out_but_yields_to_breeding_and_panic() {
    let mut bench = tempting(6.5);
    let mut control = MobControl::follow_temptation(FollowTemptation::new(1.0));
    assert!(bench.start(&mut control, NOW));
    bench.tick(&mut control, NOW + 100_000);
    assert_eq!(control.status(), Status::Running);
    bench.set(MemoryModuleType::BreedTarget, entity(5));
    bench.tick(&mut control, NOW + 100_001);
    assert_eq!(control.status(), Status::Stopped);
    assert_eq!(
        bench
            .get(MemoryModuleType::TemptationCooldownTicks)
            .and_then(MemoryValue::as_int),
        Some(100)
    );
    assert!(bench.walk_target().is_none());
    assert!(bench.look_target().is_none());
}

#[test]
fn follow_temptation_needs_a_tempting_player_and_no_cooldown_breeding_or_panic() {
    let mut control = MobControl::follow_temptation(FollowTemptation::new(1.0));
    let mut none = Bench::new(EntityKind::Cow, &TEMPT_MEMORIES, 2);
    assert!(!none.start(&mut control, NOW));
    for blocker in [
        (
            MemoryModuleType::TemptationCooldownTicks,
            MemoryValue::Int(5),
        ),
        (MemoryModuleType::BreedTarget, entity(5)),
        (MemoryModuleType::IsPanicking, MemoryValue::Bool(true)),
    ] {
        let mut bench = tempting(6.5);
        bench.set(blocker.0, blocker.1);
        assert!(!bench.start(&mut control, NOW), "{:?}", blocker.0);
    }
}

fn sense(bench: &mut Bench, sensor: &mut SensorKind) {
    let mut ai = AiContext {
        mob: &bench.mob,
        world: &bench.world,
        brain: &mut bench.controls,
        body: &mut bench.body,
    };
    sensor.do_tick(&mut ai, &mut bench.state.memories);
}

#[test]
fn the_adult_sensor_picks_the_closest_visible_adult_of_the_same_kind() {
    let mut bench = Bench::new(
        EntityKind::Cow,
        &[
            MemoryModuleType::NearestVisibleAdult,
            MemoryModuleType::NearestVisibleLivingEntities,
        ],
        1,
    );
    let mut baby = living(5, EntityKind::Cow, Vec3::new(1.5, 64.0, 0.5));
    baby.baby = true;
    let pig = living(6, EntityKind::Pig, Vec3::new(2.5, 64.0, 0.5));
    let adult = living(7, EntityKind::Cow, Vec3::new(5.5, 64.0, 0.5));
    let far = living(8, EntityKind::Cow, Vec3::new(9.5, 64.0, 0.5));
    bench.see(&[baby, pig, adult, far]);
    let mut sensor = SensorKind::NearestAdult(AdultSensor);
    assert_eq!(sensor.sensor_type(), SensorType::NearestAdult);
    sense(&mut bench, &mut sensor);
    assert_eq!(
        bench.get(MemoryModuleType::NearestVisibleAdult),
        Some(&entity(7))
    );
    bench
        .world
        .others
        .retain(|other| other.id != EntityId(7) && other.id != EntityId(8));
    bench.see(&[baby, pig]);
    sense(&mut bench, &mut sensor);
    assert!(bench.get(MemoryModuleType::NearestVisibleAdult).is_none());
}

#[test]
fn the_adult_sensor_leaves_the_memory_alone_without_visible_entities() {
    let mut bench = Bench::new(
        EntityKind::Cow,
        &[
            MemoryModuleType::NearestVisibleAdult,
            MemoryModuleType::NearestVisibleLivingEntities,
        ],
        1,
    );
    bench.set(MemoryModuleType::NearestVisibleAdult, entity(7));
    let mut sensor = SensorKind::NearestAdult(AdultSensor);
    sense(&mut bench, &mut sensor);
    assert_eq!(
        bench.get(MemoryModuleType::NearestVisibleAdult),
        Some(&entity(7))
    );
}

fn holder(id: i32, x: f64, item: Option<ItemKind>) -> Living {
    let mut player = living(id, EntityKind::Player, Vec3::new(x, 64.0, 0.5));
    player.main_hand = item.map(ItemKind::id);
    player
}

fn tempting_bench() -> Bench {
    let mut bench = Bench::new(EntityKind::Cow, &[MemoryModuleType::TemptingPlayer], 1);
    bench.mob.tempt_range = 10.0;
    bench
}

#[test]
fn the_tempting_sensor_finds_the_closest_player_holding_a_temptation_within_range() {
    let mut bench = tempting_bench();
    bench.world.players = vec![
        holder(20, 6.5, Some(ItemKind::Wheat)),
        holder(21, 3.5, Some(ItemKind::Wheat)),
        holder(22, 1.5, Some(ItemKind::Bamboo)),
        holder(23, 30.5, Some(ItemKind::Wheat)),
    ];
    let mut sensor = SensorKind::Tempting(TemptingSensor::new(TemptItems::Item(ItemKind::Wheat)));
    assert_eq!(sensor.sensor_type(), SensorType::FoodTemptations);
    sense(&mut bench, &mut sensor);
    assert_eq!(
        bench.get(MemoryModuleType::TemptingPlayer),
        Some(&entity(21))
    );
}

#[test]
fn the_tempting_sensor_erases_the_memory_when_nobody_tempts_and_skips_spectators() {
    let mut bench = tempting_bench();
    bench.set(MemoryModuleType::TemptingPlayer, entity(20));
    let mut spectator = holder(20, 3.5, Some(ItemKind::Wheat));
    spectator.spectator = true;
    let mut off_hand = holder(21, 5.5, None);
    off_hand.off_hand = Some(ItemKind::Wheat.id());
    bench.world.players = vec![spectator];
    let mut sensor = SensorKind::Tempting(TemptingSensor::new(TemptItems::Item(ItemKind::Wheat)));
    sense(&mut bench, &mut sensor);
    assert!(bench.get(MemoryModuleType::TemptingPlayer).is_none());
    bench.world.players.push(off_hand);
    sense(&mut bench, &mut sensor);
    assert_eq!(
        bench.get(MemoryModuleType::TemptingPlayer),
        Some(&entity(21))
    );
}

#[test]
fn the_tempting_sensor_can_report_the_frog_registry_type() {
    let sensor = SensorKind::Tempting(TemptingSensor::of(
        SensorType::FrogTemptations,
        TemptItems::Item(ItemKind::SlimeBall),
    ));
    assert_eq!(sensor.sensor_type(), SensorType::FrogTemptations);
}

#[test]
fn the_cooldown_countdown_and_one_shots_register_the_memories_they_declare() {
    let control = MobControl::count_down_cooldown_ticks(MemoryModuleType::LongJumpCooldownTicks);
    assert_eq!(
        control.required_memories(),
        vec![MemoryModuleType::LongJumpCooldownTicks]
    );
    let walk = shot(MobTrigger::SetWalkTargetFromLookTarget(
        SetWalkTargetFromLookTarget::new(1.0, 3),
    ));
    assert_eq!(
        walk.required_memories(),
        vec![MemoryModuleType::WalkTarget, MemoryModuleType::LookTarget]
    );
}

const INTERACT_MEMORIES: [MemoryModuleType; 4] = [
    MemoryModuleType::LookTarget,
    MemoryModuleType::InteractionTarget,
    MemoryModuleType::WalkTarget,
    MemoryModuleType::NearestVisibleLivingEntities,
];

#[test]
fn set_look_and_interact_picks_the_closest_visible_entity_of_the_type_in_range() {
    let mut bench = Bench::zombie(&INTERACT_MEMORIES);
    let pig = living(5, EntityKind::Pig, Vec3::new(2.5, 64.0, 0.5));
    let far = living(6, EntityKind::Cow, Vec3::new(9.5, 64.0, 0.5));
    let near = living(7, EntityKind::Cow, Vec3::new(4.5, 64.0, 0.5));
    bench.see(&[pig, near, far]);
    let mut control = shot(MobTrigger::SetLookAndInteract(SetLookAndInteract::new(
        EntityKind::Cow,
        6,
    )));
    assert!(bench.start(&mut control, NOW));
    assert_eq!(
        bench.get(MemoryModuleType::InteractionTarget),
        Some(&entity(7))
    );
    assert_eq!(
        bench.look_target(),
        Some(PositionTracker::of_entity(EntityId(7), true))
    );
    bench.stop(&mut control, NOW);
    assert!(!bench.start(&mut control, NOW));
}

#[test]
fn set_look_and_interact_needs_a_candidate_in_range() {
    let mut bench = Bench::zombie(&INTERACT_MEMORIES);
    bench.see(&[living(6, EntityKind::Cow, Vec3::new(9.5, 64.0, 0.5))]);
    let mut control = shot(MobTrigger::SetLookAndInteract(SetLookAndInteract::new(
        EntityKind::Cow,
        6,
    )));
    assert!(!bench.start(&mut control, NOW));
}

fn interact_with() -> MobControl {
    shot(MobTrigger::InteractWith(InteractWith::new(
        EntityKind::Cow,
        8,
        MemoryModuleType::InteractionTarget,
        0.6,
        2,
    )))
}

#[test]
fn interact_with_walks_to_the_closest_matching_entity_and_remembers_it() {
    let mut bench = Bench::zombie(&INTERACT_MEMORIES);
    bench.see(&[
        living(7, EntityKind::Cow, Vec3::new(4.5, 64.0, 0.5)),
        living(6, EntityKind::Cow, Vec3::new(7.5, 64.0, 0.5)),
    ]);
    assert!(bench.start(&mut interact_with(), NOW));
    assert_eq!(
        bench.get(MemoryModuleType::InteractionTarget),
        Some(&entity(7))
    );
    assert_eq!(
        bench.look_target(),
        Some(PositionTracker::of_entity(EntityId(7), true))
    );
    let walk = bench.walk_target().unwrap();
    assert_eq!(walk.target, PositionTracker::of_entity(EntityId(7), false));
    assert_eq!((walk.speed_modifier, walk.close_enough_dist), (0.6, 2));
}

#[test]
fn interact_with_succeeds_without_effect_when_the_candidates_are_all_out_of_range() {
    let mut bench = Bench::zombie(&INTERACT_MEMORIES);
    bench.see(&[living(6, EntityKind::Cow, Vec3::new(12.5, 64.0, 0.5))]);
    assert!(bench.start(&mut interact_with(), NOW));
    assert!(bench.walk_target().is_none());
    assert!(bench.get(MemoryModuleType::InteractionTarget).is_none());
}

#[test]
fn interact_with_needs_a_matching_visible_entity_and_no_walk_target() {
    let mut bench = Bench::zombie(&INTERACT_MEMORIES);
    bench.see(&[living(6, EntityKind::Pig, Vec3::new(3.5, 64.0, 0.5))]);
    assert!(!bench.start(&mut interact_with(), NOW));
    bench.see(&[living(7, EntityKind::Cow, Vec3::new(3.5, 64.0, 0.5))]);
    walk_to(&mut bench, 5, 64, 5, 1.0, 0);
    assert!(!bench.start(&mut interact_with(), NOW));
}

fn home(dimension: &str, x: i32, z: i32) -> MemoryValue {
    MemoryValue::GlobalPos(GlobalPos {
        dimension: dimension.to_owned(),
        pos: BlockPos::new(x, 64, z),
    })
}

#[test]
fn stroll_to_poi_walks_to_a_poi_in_range_and_retries_after_eighty_ticks() {
    let mut bench = Bench::zombie(&[MemoryModuleType::WalkTarget, MemoryModuleType::Home]);
    bench.set(MemoryModuleType::Home, home("minecraft:overworld", 6, 0));
    let mut control = shot(MobTrigger::StrollToPoi(StrollToPoi::new(
        MemoryModuleType::Home,
        0.6,
        2,
        100,
    )));
    assert!(bench.start(&mut control, NOW));
    let walk = bench.walk_target().unwrap();
    assert_eq!((walk.speed_modifier, walk.close_enough_dist), (0.6, 2));
    assert_eq!(
        walk.target.current_block_position(&bench.world),
        Some(BlockPos::new(6, 64, 0))
    );
    bench.state.memories.erase(MemoryModuleType::WalkTarget);
    bench.stop(&mut control, NOW);
    assert!(bench.start(&mut control, NOW + 80));
    assert!(bench.walk_target().is_none());
    bench.stop(&mut control, NOW + 80);
    assert!(bench.start(&mut control, NOW + 81));
    assert!(bench.walk_target().is_some());
}

#[test]
fn stroll_to_poi_ignores_a_poi_that_is_too_far_or_in_another_dimension() {
    let mut control = shot(MobTrigger::StrollToPoi(StrollToPoi::new(
        MemoryModuleType::Home,
        0.6,
        2,
        10,
    )));
    let mut far = Bench::zombie(&[MemoryModuleType::WalkTarget, MemoryModuleType::Home]);
    far.set(MemoryModuleType::Home, home("minecraft:overworld", 30, 0));
    assert!(!far.start(&mut control, NOW));
    let mut nether = Bench::zombie(&[MemoryModuleType::WalkTarget, MemoryModuleType::Home]);
    nether.set(MemoryModuleType::Home, home("minecraft:the_nether", 3, 0));
    assert!(!nether.start(&mut control, NOW));
    assert!(nether.walk_target().is_none());
}

#[test]
fn stroll_around_poi_picks_a_nearby_spot_and_waits_one_hundred_eighty_ticks() {
    let mut bench = Bench::zombie(&[MemoryModuleType::WalkTarget, MemoryModuleType::Home]);
    bench.set(MemoryModuleType::Home, home("minecraft:overworld", 3, 0));
    let mut control = shot(MobTrigger::StrollAroundPoi(StrollAroundPoi::new(
        MemoryModuleType::Home,
        0.6,
        5,
    )));
    assert!(bench.start(&mut control, NOW));
    let walk = bench.walk_target().expect("a stroll target");
    assert_eq!((walk.speed_modifier, walk.close_enough_dist), (0.6, 1));
    bench.state.memories.erase(MemoryModuleType::WalkTarget);
    bench.stop(&mut control, NOW);
    assert!(bench.start(&mut control, NOW + 180));
    assert!(bench.walk_target().is_none());
    let mut far = Bench::zombie(&[MemoryModuleType::WalkTarget, MemoryModuleType::Home]);
    far.set(MemoryModuleType::Home, home("minecraft:overworld", 30, 0));
    assert!(!far.start(&mut control, NOW));
}

fn hoglin_like_brain(random: &mut MobRandom) -> BehaviorBrain<MobControl> {
    let core = ActivityData::create(
        Activity::Core,
        0,
        vec![
            MobControl::default_look_at_target_sink(),
            MobControl::move_to_target_sink(),
        ],
    );
    let idle = ActivityData::create(
        Activity::Idle,
        10,
        vec![
            MobControl::one_shot(MobTrigger::StartAttacking(StartAttacking::new(
                TargetFinder::Memory(MemoryModuleType::NearestAttackable),
            ))),
            MobControl::Gate(Gate::run_one(vec![
                (
                    MobControl::one_shot(MobTrigger::RandomStroll(RandomStroll::stroll(0.4))),
                    2,
                ),
                (MobControl::DoNothing(DoNothing::new(30, 60)), 1),
            ])),
        ],
    );
    let fight = ActivityData::create_erasing(
        Activity::Fight,
        10,
        vec![
            MobControl::one_shot(MobTrigger::SetWalkTargetFromAttackTargetIfTargetOutOfReach(
                SetWalkTargetFromAttackTargetIfTargetOutOfReach::new(1.0),
            )),
            MobControl::trigger_if(
                MobPredicate::IsAdult,
                MobControl::declared(MobTrigger::MeleeAttack(MeleeAttack::new(40))),
            ),
            MobControl::one_shot(MobTrigger::StopAttackingIfTargetInvalid(
                StopAttackingIfTargetInvalid::default(),
            )),
        ],
        MemoryModuleType::AttackTarget,
    );
    BehaviorBrain::new(
        &[
            MemoryModuleType::NearestAttackable,
            MemoryModuleType::AttackTarget,
        ],
        vec![SensorKind::NearestLivingEntities(NearestLivingEntitySensor)],
        vec![core, idle, fight],
        &PackedMemories::default(),
        random,
    )
}

fn run_brain(
    bench: &mut Bench,
    brain: &mut BehaviorBrain<MobControl>,
    ticks: i64,
) -> Vec<MobAction> {
    let mut actions = Vec::new();
    for _ in 0..ticks {
        bench.world.time += 1;
        let mut ai = AiContext {
            mob: &bench.mob,
            world: &bench.world,
            brain: &mut bench.controls,
            body: &mut bench.body,
        };
        brain.tick(&mut ai);
        actions.append(&mut bench.controls.actions);
    }
    actions
}

#[test]
fn a_brain_built_from_the_shared_behaviors_strolls_while_idle() {
    let mut bench = Bench::zombie(&[]);
    let mut brain = hoglin_like_brain(&mut bench.controls.random);
    run_brain(&mut bench, &mut brain, 400);
    assert!(brain.memories().is_registered(MemoryModuleType::WalkTarget));
    assert!(brain.memories().is_registered(MemoryModuleType::Path));
    assert!(brain.state().is_active(Activity::Idle) && !brain.state().is_active(Activity::Fight));
}

#[test]
fn a_brain_built_from_the_shared_behaviors_fights_a_target_in_reach_on_a_cooldown() {
    let mut bench = Bench::zombie(&[]);
    let mut brain = hoglin_like_brain(&mut bench.controls.random);
    bench
        .world
        .others
        .push(living(3, EntityKind::Cow, Vec3::new(1.5, 64.0, 0.5)));
    brain.memories_mut().set(
        MemoryModuleType::AttackTarget,
        MemoryValue::Entity(EntityId(3)),
    );
    brain
        .state_mut()
        .set_active_activity_if_possible(Activity::Fight);
    let actions = run_brain(&mut bench, &mut brain, 130);
    let attacks = actions
        .iter()
        .filter(|action| matches!(action, MobAction::Attack(EntityId(3))))
        .count();
    assert!((2..=4).contains(&attacks), "attacked {attacks} times");
    assert!(brain.state().is_active(Activity::Fight));
    bench.world.others[0].alive = false;
    run_brain(&mut bench, &mut brain, 25);
    assert!(
        brain
            .memories()
            .get(MemoryModuleType::AttackTarget)
            .is_none()
    );
    brain
        .state_mut()
        .set_active_activity_if_possible(Activity::Fight);
    assert!(!brain.state().is_active(Activity::Fight));
}

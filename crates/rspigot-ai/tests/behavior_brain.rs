use std::cell::{Cell, RefCell};
use std::rc::Rc;

use rspigot_ai::{
    Activity, ActivityData, AiContext, AiWorld, BehaviorBrain, BehaviorControl, BehaviorCx, Brain,
    BrainState, CountDownCooldownTicks, DoNothing, DroppedItem, ExpirableValue, Facts, Gate,
    GlobalPos, HurtBySensor, IsInWaterSensor, ItemWish, LastDamage, Living, Memories,
    MemoryModuleType, MemoryStatus, MemoryValue, MobAi, MobBehavior, MobBody, MobControl, MobView,
    NO_EXPIRY, NearestItemSensor, NearestLivingEntitySensor, NearestVisibleLivingEntities, OneShot,
    OrderPolicy, PackedMemories, PlayerSensor, RunningPolicy, Schedule, SensorKind, SensorType,
    ShufflingList, Status, TargetKind, Timed, Trigger, TriggerGate, VILLAGER_PERIOD_TICKS,
    is_entity_targetable,
};
use rspigot_blocks::{BlockKind, StateId};
use rspigot_domain::damage::DamageType;
use rspigot_domain::entity::living::LivingState;
use rspigot_domain::entity::{Aabb, EntityBase};
use rspigot_domain::{BlockPos, EntityId, Vec3};
use rspigot_entities::EntityKind;
use rspigot_items::ItemKind;
use rspigot_nbt::{Compound, Tag};
use uuid::Uuid;

const HOME: Vec3 = Vec3::new(0.5, 64.0, 0.5);

#[derive(Default)]
struct Field {
    players: Vec<Living>,
    others: Vec<Living>,
    items: Vec<DroppedItem>,
    walls: Vec<BlockPos>,
    time: Cell<i64>,
    clock: Cell<i64>,
}

impl rspigot_ai::BlockView for Field {
    fn state(&self, pos: BlockPos) -> Option<StateId> {
        Some(if self.walls.contains(&pos) || (60..64).contains(&pos.y) {
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
        self.time.get()
    }

    fn clock_time(&self) -> i64 {
        self.clock.get()
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

struct Rig<C> {
    mob: MobView,
    world: Field,
    controls: Brain,
    body: MobBody,
    brain: BehaviorBrain<C>,
}

impl<C: BehaviorControl> Rig<C> {
    fn new(
        kind: EntityKind,
        sensors: Vec<SensorKind>,
        activities: Vec<ActivityData<C>>,
        memories: &[MemoryModuleType],
        seed: i64,
    ) -> Self {
        let mob = view(kind, HOME);
        let mut controls = Brain::new(kind, seed);
        let brain = BehaviorBrain::new(
            memories,
            sensors,
            activities,
            &PackedMemories::default(),
            &mut controls.random,
        );
        Self {
            body: MobBody::of(&mob),
            mob,
            world: Field::default(),
            controls,
            brain,
        }
    }

    fn tick(&mut self) {
        self.world.time.set(self.world.time.get() + 1);
        self.controls.sensing.tick();
        let mut context = AiContext {
            mob: &self.mob,
            world: &self.world,
            brain: &mut self.controls,
            body: &mut self.body,
        };
        self.brain.tick(&mut context);
    }

    fn tick_times(&mut self, count: usize) {
        for _ in 0..count {
            self.tick();
        }
    }

    fn memory(&self, ty: MemoryModuleType) -> Option<&MemoryValue> {
        self.brain.memories().get(ty)
    }
}

fn sensor_rig(sensor: SensorKind, kind: EntityKind) -> Rig<MobControl> {
    Rig::new(kind, vec![sensor], Vec::new(), &[], 3)
}

fn entities(rig: &Rig<MobControl>, ty: MemoryModuleType) -> Vec<i32> {
    rig.memory(ty)
        .and_then(MemoryValue::as_entities)
        .map(|ids| ids.iter().map(|id| id.0).collect())
        .unwrap_or_default()
}

fn fire_ticks(rig: &mut Rig<MobControl>, ticks: usize) -> Vec<usize> {
    let mut fired = Vec::new();
    for tick in 1..=ticks {
        rig.tick();
        if rig.brain.sensors()[0].time_to_tick() == 20 {
            fired.push(tick);
        }
    }
    fired
}

#[derive(Debug, Clone, PartialEq)]
struct Probe {
    id: i32,
    status: Status,
    accept: bool,
    stop_after: i32,
    runs: i32,
    requires: Vec<MemoryModuleType>,
    log: Rc<RefCell<Vec<String>>>,
}

impl Probe {
    fn new(id: i32, log: &Rc<RefCell<Vec<String>>>) -> Self {
        Self {
            id,
            status: Status::Stopped,
            accept: true,
            stop_after: i32::MAX,
            runs: 0,
            requires: Vec::new(),
            log: Rc::clone(log),
        }
    }

    fn refusing(mut self) -> Self {
        self.accept = false;
        self
    }

    fn stopping_after(mut self, runs: i32) -> Self {
        self.stop_after = runs;
        self
    }

    fn requiring(mut self, memory: MemoryModuleType) -> Self {
        self.requires.push(memory);
        self
    }
}

impl BehaviorControl for Probe {
    fn status(&self) -> Status {
        self.status
    }

    fn required_memories(&self) -> Vec<MemoryModuleType> {
        self.requires.clone()
    }

    fn try_start<W: AiWorld>(&mut self, _cx: &mut BehaviorCx<'_, '_, W>, _now: i64) -> bool {
        self.log.borrow_mut().push(format!("try {}", self.id));
        if self.accept {
            self.status = Status::Running;
            self.runs = 0;
        }
        self.accept
    }

    fn tick_or_stop<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, now: i64) {
        self.log.borrow_mut().push(format!("tick {}", self.id));
        self.runs += 1;
        if self.runs >= self.stop_after {
            self.do_stop(cx, now);
        }
    }

    fn do_stop<W: AiWorld>(&mut self, _cx: &mut BehaviorCx<'_, '_, W>, _now: i64) {
        self.log.borrow_mut().push(format!("stop {}", self.id));
        self.status = Status::Stopped;
    }
}

fn log() -> Rc<RefCell<Vec<String>>> {
    Rc::new(RefCell::new(Vec::new()))
}

fn entries(log: &Rc<RefCell<Vec<String>>>) -> Vec<String> {
    log.borrow().clone()
}

fn probe_rig(activities: Vec<ActivityData<Probe>>) -> Rig<Probe> {
    Rig::new(EntityKind::Zombie, Vec::new(), activities, &[], 1)
}

fn state_with(memories: &[MemoryModuleType]) -> BrainState {
    let mut state = BrainState::default();
    for ty in memories {
        state.memories.register(*ty);
    }
    state
}

fn with_cx<R>(
    state: &mut BrainState,
    seed: i64,
    world: &Field,
    run: impl FnOnce(&mut BehaviorCx<'_, '_, Field>) -> R,
) -> R {
    let mob = view(EntityKind::Zombie, HOME);
    let mut controls = Brain::new(EntityKind::Zombie, seed);
    let mut body = MobBody::of(&mob);
    let mut ai = AiContext {
        mob: &mob,
        world,
        brain: &mut controls,
        body: &mut body,
    };
    let mut cx = BehaviorCx {
        ai: &mut ai,
        brain: state,
    };
    run(&mut cx)
}

#[test]
fn registries_match_the_generated_vanilla_registries() {
    let names = |registry| rspigot_data::registries::static_entries(registry).unwrap();
    let memories: Vec<&str> = MemoryModuleType::ALL.iter().map(|ty| ty.name()).collect();
    assert_eq!(memories, names("minecraft:memory_module_type"));
    let sensors: Vec<&str> = SensorType::ALL.iter().map(|ty| ty.name()).collect();
    assert_eq!(sensors, names("minecraft:sensor_type"));
    let activities: Vec<&str> = Activity::ALL.iter().map(|ty| ty.name()).collect();
    assert_eq!(activities, names("minecraft:activity"));
}

#[test]
fn memory_set_get_and_erase_follow_the_brain_semantics() {
    let mut memories = Memories::default();
    memories.register(MemoryModuleType::LastSlept);
    assert!(memories.is_registered(MemoryModuleType::LastSlept));
    assert!(!memories.has_value(MemoryModuleType::LastSlept));
    memories.set(MemoryModuleType::LastSlept, MemoryValue::Long(5));
    assert_eq!(
        memories.get(MemoryModuleType::LastSlept),
        Some(&MemoryValue::Long(5))
    );
    assert!(memories.has_value(MemoryModuleType::LastSlept));
    assert!(memories.is_value(MemoryModuleType::LastSlept, &MemoryValue::Long(5)));
    assert!(!memories.is_value(MemoryModuleType::LastSlept, &MemoryValue::Long(6)));
    memories.erase(MemoryModuleType::LastSlept);
    assert_eq!(memories.get(MemoryModuleType::LastSlept), None);
    assert!(!memories.has_value(MemoryModuleType::LastSlept));
}

#[test]
fn unregistered_memories_ignore_writes_and_fail_every_status_check() {
    let mut memories = Memories::default();
    memories.set(MemoryModuleType::LastSlept, MemoryValue::Long(5));
    assert_eq!(memories.get(MemoryModuleType::LastSlept), None);
    assert_eq!(
        memories.time_until_expiry(MemoryModuleType::LastSlept),
        None
    );
    for status in [
        MemoryStatus::ValuePresent,
        MemoryStatus::ValueAbsent,
        MemoryStatus::Registered,
    ] {
        assert!(!memories.check(MemoryModuleType::LastSlept, status));
    }
}

#[test]
fn memory_status_checks_registered_present_and_absent() {
    let mut memories = Memories::default();
    memories.register(MemoryModuleType::LastSlept);
    let check = |memories: &Memories, status| memories.check(MemoryModuleType::LastSlept, status);
    assert!(check(&memories, MemoryStatus::Registered));
    assert!(check(&memories, MemoryStatus::ValueAbsent));
    assert!(!check(&memories, MemoryStatus::ValuePresent));
    memories.set(MemoryModuleType::LastSlept, MemoryValue::Long(1));
    assert!(check(&memories, MemoryStatus::Registered));
    assert!(!check(&memories, MemoryStatus::ValueAbsent));
    assert!(check(&memories, MemoryStatus::ValuePresent));
}

#[test]
fn an_expiring_memory_survives_n_ticks_and_clears_on_the_next() {
    let mut memories = Memories::default();
    memories.register(MemoryModuleType::LastWoken);
    memories.set_with_expiry(MemoryModuleType::LastWoken, MemoryValue::Long(9), 3);
    assert_eq!(
        memories.time_until_expiry(MemoryModuleType::LastWoken),
        Some(3)
    );
    for remaining in [2, 1, 0] {
        memories.forget_outdated();
        assert!(memories.has_value(MemoryModuleType::LastWoken));
        assert_eq!(
            memories.time_until_expiry(MemoryModuleType::LastWoken),
            Some(remaining)
        );
    }
    memories.forget_outdated();
    assert!(!memories.has_value(MemoryModuleType::LastWoken));
    assert_eq!(
        memories.time_until_expiry(MemoryModuleType::LastWoken),
        Some(NO_EXPIRY)
    );
}

#[test]
fn a_memory_without_expiry_never_ticks_down() {
    let mut memories = Memories::default();
    memories.register(MemoryModuleType::LastWoken);
    memories.set(MemoryModuleType::LastWoken, MemoryValue::Long(9));
    for _ in 0..1000 {
        memories.forget_outdated();
    }
    assert!(memories.has_value(MemoryModuleType::LastWoken));
    assert_eq!(
        memories.time_until_expiry(MemoryModuleType::LastWoken),
        Some(NO_EXPIRY)
    );
}

#[test]
fn setting_again_replaces_the_value_and_resets_the_expiry() {
    let mut memories = Memories::default();
    memories.register(MemoryModuleType::LastWoken);
    memories.set_with_expiry(MemoryModuleType::LastWoken, MemoryValue::Long(9), 3);
    memories.set(MemoryModuleType::LastWoken, MemoryValue::Long(10));
    assert_eq!(
        memories.time_until_expiry(MemoryModuleType::LastWoken),
        Some(NO_EXPIRY)
    );
    memories.set_with_expiry(MemoryModuleType::LastWoken, MemoryValue::Long(11), 8);
    memories.erase(MemoryModuleType::LastWoken);
    assert_eq!(
        memories.time_until_expiry(MemoryModuleType::LastWoken),
        Some(NO_EXPIRY)
    );
}

#[test]
fn setting_an_empty_collection_or_none_clears_the_memory() {
    let mut memories = Memories::default();
    memories.register(MemoryModuleType::NearestLivingEntities);
    memories.set(
        MemoryModuleType::NearestLivingEntities,
        MemoryValue::Entities(vec![EntityId(3)]),
    );
    assert!(memories.has_value(MemoryModuleType::NearestLivingEntities));
    memories.set(
        MemoryModuleType::NearestLivingEntities,
        MemoryValue::Entities(Vec::new()),
    );
    assert!(!memories.has_value(MemoryModuleType::NearestLivingEntities));
    memories.set_with_expiry(
        MemoryModuleType::NearestLivingEntities,
        MemoryValue::Entities(vec![EntityId(3)]),
        5,
    );
    memories.set_with_expiry(
        MemoryModuleType::NearestLivingEntities,
        MemoryValue::Entities(Vec::new()),
        5,
    );
    assert!(!memories.has_value(MemoryModuleType::NearestLivingEntities));
    memories.set(
        MemoryModuleType::NearestLivingEntities,
        MemoryValue::Entities(vec![EntityId(3)]),
    );
    memories.set_option(MemoryModuleType::NearestLivingEntities, None);
    assert!(!memories.has_value(MemoryModuleType::NearestLivingEntities));
}

#[test]
fn clear_all_empties_every_slot_but_keeps_them_registered() {
    let mut memories = Memories::default();
    memories.register(MemoryModuleType::LastWoken);
    memories.register(MemoryModuleType::LastSlept);
    memories.set(MemoryModuleType::LastWoken, MemoryValue::Long(1));
    memories.set(MemoryModuleType::LastSlept, MemoryValue::Long(2));
    memories.clear_all();
    assert!(memories.is_registered(MemoryModuleType::LastWoken));
    assert!(!memories.has_value(MemoryModuleType::LastWoken));
    assert!(!memories.has_value(MemoryModuleType::LastSlept));
}

#[test]
fn expirable_values_carry_an_optional_time_to_live() {
    let plain = ExpirableValue::of(MemoryValue::Int(1));
    assert_eq!(plain.time_to_live, None);
    let expiring = ExpirableValue::of_expiring(MemoryValue::Int(1), 40);
    assert_eq!(expiring.time_to_live, Some(40));
    let mut memories = Memories::default();
    memories.register(MemoryModuleType::PlayDeadTicks);
    memories.set_expirable(MemoryModuleType::PlayDeadTicks, expiring);
    assert_eq!(
        memories.time_until_expiry(MemoryModuleType::PlayDeadTicks),
        Some(40)
    );
    memories.set_expirable(MemoryModuleType::PlayDeadTicks, plain);
    assert_eq!(
        memories.time_until_expiry(MemoryModuleType::PlayDeadTicks),
        Some(NO_EXPIRY)
    );
}

#[test]
fn registering_twice_keeps_the_stored_value() {
    let mut memories = Memories::default();
    memories.register(MemoryModuleType::LastWoken);
    memories.set(MemoryModuleType::LastWoken, MemoryValue::Long(1));
    memories.register(MemoryModuleType::LastWoken);
    assert_eq!(
        memories.get(MemoryModuleType::LastWoken),
        Some(&MemoryValue::Long(1))
    );
}

#[test]
fn the_brain_forgets_outdated_memories_each_tick() {
    let mut rig = probe_rig(Vec::new());
    rig.brain
        .memories_mut()
        .register(MemoryModuleType::LastWoken);
    rig.brain
        .memories_mut()
        .set_with_expiry(MemoryModuleType::LastWoken, MemoryValue::Long(1), 2);
    rig.tick_times(2);
    assert!(rig.brain.memories().has_value(MemoryModuleType::LastWoken));
    rig.tick();
    assert!(!rig.brain.memories().has_value(MemoryModuleType::LastWoken));
}

#[test]
fn every_sensor_runs_at_its_scan_rate_after_a_random_initial_delay() {
    for seed in 0..6 {
        let mut rig = Rig::new(
            EntityKind::Zombie,
            vec![SensorKind::IsInWater(IsInWaterSensor)],
            Vec::new(),
            &[],
            seed,
        );
        let delay = rig.brain.sensors()[0].time_to_tick();
        assert!((0..20).contains(&delay));
        let first = delay.max(1) as usize;
        let fired = fire_ticks(&mut rig, 100);
        assert_eq!(fired[0], first, "seed {seed}");
        assert!(fired.windows(2).all(|pair| pair[1] - pair[0] == 20));
        assert_eq!(fired.len(), (100 - first) / 20 + 1);
    }
}

#[test]
fn the_sensor_kinds_report_their_registry_types() {
    let kinds = [
        (
            SensorKind::Dummy(rspigot_ai::DummySensor),
            SensorType::Dummy,
        ),
        (
            SensorKind::NearestItems(NearestItemSensor::default()),
            SensorType::NearestItems,
        ),
        (
            SensorKind::NearestLivingEntities(NearestLivingEntitySensor),
            SensorType::NearestLivingEntities,
        ),
        (
            SensorKind::NearestPlayers(PlayerSensor),
            SensorType::NearestPlayers,
        ),
        (SensorKind::HurtBy(HurtBySensor), SensorType::HurtBy),
        (
            SensorKind::IsInWater(IsInWaterSensor),
            SensorType::IsInWater,
        ),
    ];
    for (kind, ty) in kinds {
        assert_eq!(kind.sensor_type(), ty);
    }
}

#[test]
fn a_brain_registers_the_memories_its_sensors_require() {
    let rig = sensor_rig(
        SensorKind::NearestLivingEntities(NearestLivingEntitySensor),
        EntityKind::Zombie,
    );
    let memories = rig.brain.memories();
    assert!(memories.is_registered(MemoryModuleType::NearestLivingEntities));
    assert!(memories.is_registered(MemoryModuleType::NearestVisibleLivingEntities));
    assert!(!memories.is_registered(MemoryModuleType::NearestPlayers));
}

#[test]
fn the_living_entity_sensor_lists_the_closest_first_without_the_mob_or_the_dead() {
    let mut rig = sensor_rig(
        SensorKind::NearestLivingEntities(NearestLivingEntitySensor),
        EntityKind::Zombie,
    );
    let mut dead = living(5, EntityKind::Pig, Vec3::new(2.5, 64.0, 0.5));
    dead.alive = false;
    rig.world.others = vec![
        living(2, EntityKind::Cow, Vec3::new(9.5, 64.0, 0.5)),
        living(1, EntityKind::Pig, HOME),
        living(3, EntityKind::Pig, Vec3::new(3.5, 64.0, 0.5)),
        living(4, EntityKind::Cow, Vec3::new(500.5, 64.0, 0.5)),
        dead,
    ];
    rig.world.players = vec![living(7, EntityKind::Player, Vec3::new(5.5, 64.0, 0.5))];
    rig.tick_times(20);
    assert_eq!(
        entities(&rig, MemoryModuleType::NearestLivingEntities),
        vec![3, 7, 2]
    );
    let visible = rig
        .memory(MemoryModuleType::NearestVisibleLivingEntities)
        .and_then(MemoryValue::as_visible)
        .unwrap();
    let nearby: Vec<i32> = visible.nearby_entities().iter().map(|id| id.0).collect();
    assert_eq!(nearby, vec![3, 7, 2]);
}

#[test]
fn the_living_entity_sensor_clears_both_memories_when_nobody_is_around() {
    let mut rig = sensor_rig(
        SensorKind::NearestLivingEntities(NearestLivingEntitySensor),
        EntityKind::Zombie,
    );
    rig.world.others = vec![living(3, EntityKind::Pig, Vec3::new(3.5, 64.0, 0.5))];
    rig.tick_times(20);
    assert!(
        rig.brain
            .memories()
            .has_value(MemoryModuleType::NearestLivingEntities)
    );
    rig.world.others.clear();
    rig.tick_times(20);
    assert!(
        !rig.brain
            .memories()
            .has_value(MemoryModuleType::NearestLivingEntities)
    );
    assert_eq!(
        rig.memory(MemoryModuleType::NearestVisibleLivingEntities)
            .and_then(MemoryValue::as_visible)
            .map(|visible| visible.nearby_entities().len()),
        Some(0)
    );
}

fn visible_ids(rig: &mut Rig<MobControl>) -> Vec<i32> {
    let mut context = AiContext {
        mob: &rig.mob,
        world: &rig.world,
        brain: &mut rig.controls,
        body: &mut rig.body,
    };
    let memories = rig.brain.memories();
    memories
        .get(MemoryModuleType::NearestVisibleLivingEntities)
        .and_then(MemoryValue::as_visible)
        .map(|visible| {
            visible
                .find_all(&mut context, memories, |_| true)
                .iter()
                .map(|living| living.id.0)
                .collect()
        })
        .unwrap_or_default()
}

#[test]
fn visible_entities_need_a_clear_line_of_sight_and_cache_the_verdict() {
    let mut rig = sensor_rig(
        SensorKind::NearestLivingEntities(NearestLivingEntitySensor),
        EntityKind::Zombie,
    );
    rig.world.others = vec![
        living(2, EntityKind::Pig, Vec3::new(4.5, 64.0, 0.5)),
        living(3, EntityKind::Pig, Vec3::new(-4.5, 64.0, 0.5)),
    ];
    rig.world.walls = (64..67).map(|y| BlockPos::new(-2, y, 0)).collect();
    rig.tick_times(20);
    assert_eq!(visible_ids(&mut rig), vec![2]);
    rig.world.walls.clear();
    assert_eq!(visible_ids(&mut rig), vec![2]);
    rig.tick_times(20);
    assert_eq!(visible_ids(&mut rig), vec![2, 3]);
}

#[test]
fn find_closest_returns_the_first_visible_match_of_the_filter() {
    let mut rig = sensor_rig(
        SensorKind::NearestLivingEntities(NearestLivingEntitySensor),
        EntityKind::Zombie,
    );
    rig.world.others = vec![
        living(2, EntityKind::Pig, Vec3::new(2.5, 64.0, 0.5)),
        living(3, EntityKind::Cow, Vec3::new(4.5, 64.0, 0.5)),
        living(4, EntityKind::Cow, Vec3::new(6.5, 64.0, 0.5)),
    ];
    rig.tick_times(20);
    let mut context = AiContext {
        mob: &rig.mob,
        world: &rig.world,
        brain: &mut rig.controls,
        body: &mut rig.body,
    };
    let memories = rig.brain.memories();
    let visible = memories
        .get(MemoryModuleType::NearestVisibleLivingEntities)
        .and_then(MemoryValue::as_visible)
        .unwrap();
    let cow = visible.find_closest(&mut context, memories, |living| {
        living.kind == EntityKind::Cow
    });
    assert_eq!(cow.map(|living| living.id.0), Some(3));
    assert!(
        visible.contains(&mut context, memories, |living| living.kind
            == EntityKind::Pig)
    );
    assert!(
        !visible.contains(&mut context, memories, |living| living.kind
            == EntityKind::Sheep)
    );
    assert!(visible.contains_entity(&mut context, memories, EntityId(4)));
    assert!(!visible.contains_entity(&mut context, memories, EntityId(99)));
}

#[test]
fn an_attack_target_stays_targetable_while_invisible() {
    let mut rig = sensor_rig(
        SensorKind::NearestLivingEntities(NearestLivingEntitySensor),
        EntityKind::Zombie,
    );
    let mut ghost = living(2, EntityKind::Pig, Vec3::new(30.5, 64.0, 0.5));
    ghost.invisible = true;
    ghost.armor_cover = 0.0;
    rig.world.others = vec![ghost];
    let memories = rig.brain.memories_mut();
    memories.register(MemoryModuleType::AttackTarget);
    let mut context = AiContext {
        mob: &rig.mob,
        world: &rig.world,
        brain: &mut rig.controls,
        body: &mut rig.body,
    };
    assert!(!is_entity_targetable(
        &mut context,
        rig.brain.memories(),
        &ghost
    ));
    rig.brain.memories_mut().set(
        MemoryModuleType::AttackTarget,
        MemoryValue::Entity(EntityId(2)),
    );
    let mut context = AiContext {
        mob: &rig.mob,
        world: &rig.world,
        brain: &mut rig.controls,
        body: &mut rig.body,
    };
    assert!(is_entity_targetable(
        &mut context,
        rig.brain.memories(),
        &ghost
    ));
}

#[test]
fn the_player_sensor_fills_the_four_player_memories() {
    let mut rig = sensor_rig(SensorKind::NearestPlayers(PlayerSensor), EntityKind::Zombie);
    let mut spectator = living(10, EntityKind::Player, Vec3::new(1.5, 64.0, 0.5));
    spectator.spectator = true;
    let mut creative = living(11, EntityKind::Player, Vec3::new(3.5, 64.0, 0.5));
    creative.creative = true;
    creative.invulnerable = true;
    rig.world.players = vec![
        living(12, EntityKind::Player, Vec3::new(6.5, 64.0, 0.5)),
        creative,
        spectator,
        living(13, EntityKind::Player, Vec3::new(400.5, 64.0, 0.5)),
    ];
    rig.tick_times(20);
    assert_eq!(
        entities(&rig, MemoryModuleType::NearestPlayers),
        vec![11, 12]
    );
    assert_eq!(
        rig.memory(MemoryModuleType::NearestVisiblePlayer),
        Some(&MemoryValue::Entity(EntityId(11)))
    );
    assert_eq!(
        entities(&rig, MemoryModuleType::NearestVisibleAttackablePlayers),
        vec![12]
    );
    assert_eq!(
        rig.memory(MemoryModuleType::NearestVisibleAttackablePlayer),
        Some(&MemoryValue::Entity(EntityId(12)))
    );
}

#[test]
fn the_player_sensor_erases_the_single_player_memories_when_nobody_qualifies() {
    let mut rig = sensor_rig(SensorKind::NearestPlayers(PlayerSensor), EntityKind::Zombie);
    rig.world.players = vec![living(12, EntityKind::Player, Vec3::new(6.5, 64.0, 0.5))];
    rig.tick_times(20);
    assert!(
        rig.brain
            .memories()
            .has_value(MemoryModuleType::NearestVisiblePlayer)
    );
    rig.world.players.clear();
    rig.tick_times(20);
    for ty in [
        MemoryModuleType::NearestPlayers,
        MemoryModuleType::NearestVisiblePlayer,
        MemoryModuleType::NearestVisibleAttackablePlayer,
        MemoryModuleType::NearestVisibleAttackablePlayers,
    ] {
        assert!(!rig.brain.memories().has_value(ty), "{ty:?}");
    }
}

#[test]
fn the_hurt_by_sensor_mirrors_the_last_damage_and_forgets_dead_attackers() {
    let mut rig = sensor_rig(SensorKind::HurtBy(HurtBySensor), EntityKind::Zombie);
    rig.world.others = vec![living(8, EntityKind::Pig, Vec3::new(3.5, 64.0, 0.5))];
    rig.mob.last_damage = Some(LastDamage {
        kind: DamageType::MobAttack,
        entity: Some(EntityId(8)),
    });
    rig.tick_times(20);
    assert_eq!(
        rig.memory(MemoryModuleType::HurtBy),
        Some(&MemoryValue::HurtBy(LastDamage {
            kind: DamageType::MobAttack,
            entity: Some(EntityId(8)),
        }))
    );
    assert_eq!(
        rig.memory(MemoryModuleType::HurtByEntity),
        Some(&MemoryValue::Entity(EntityId(8)))
    );
    rig.world.others[0].alive = false;
    rig.tick_times(20);
    assert!(rig.brain.memories().has_value(MemoryModuleType::HurtBy));
    assert!(
        !rig.brain
            .memories()
            .has_value(MemoryModuleType::HurtByEntity)
    );
    rig.mob.last_damage = None;
    rig.tick_times(20);
    assert!(!rig.brain.memories().has_value(MemoryModuleType::HurtBy));
}

#[test]
fn the_hurt_by_sensor_keeps_hurt_by_without_an_attacker_entity() {
    let mut rig = sensor_rig(SensorKind::HurtBy(HurtBySensor), EntityKind::Zombie);
    rig.mob.last_damage = Some(LastDamage {
        kind: DamageType::Fall,
        entity: None,
    });
    rig.tick_times(20);
    assert!(rig.brain.memories().has_value(MemoryModuleType::HurtBy));
    assert!(
        !rig.brain
            .memories()
            .has_value(MemoryModuleType::HurtByEntity)
    );
}

#[test]
fn the_item_sensor_picks_the_closest_wanted_visible_item_within_32_blocks() {
    let sensor = SensorKind::NearestItems(NearestItemSensor::new(ItemWish::Any));
    let mut rig = sensor_rig(sensor, EntityKind::Zombie);
    let mut gone = item(5, Vec3::new(1.5, 64.0, 0.5));
    gone.alive = false;
    rig.world.items = vec![
        item(2, Vec3::new(20.5, 64.0, 0.5)),
        item(3, Vec3::new(5.5, 64.0, 0.5)),
        item(4, Vec3::new(40.5, 64.0, 0.5)),
        gone,
    ];
    rig.tick_times(20);
    assert_eq!(
        rig.memory(MemoryModuleType::NearestVisibleWantedItem),
        Some(&MemoryValue::Entity(EntityId(3)))
    );
    rig.world.items.remove(1);
    rig.tick_times(20);
    assert_eq!(
        rig.memory(MemoryModuleType::NearestVisibleWantedItem),
        Some(&MemoryValue::Entity(EntityId(2)))
    );
    rig.world.items.remove(0);
    rig.tick_times(20);
    assert!(
        !rig.brain
            .memories()
            .has_value(MemoryModuleType::NearestVisibleWantedItem)
    );
}

#[test]
fn the_item_sensor_skips_unwanted_and_obstructed_items() {
    let mut rig = sensor_rig(
        SensorKind::NearestItems(NearestItemSensor::new(ItemWish::Never)),
        EntityKind::Zombie,
    );
    rig.world.items = vec![item(3, Vec3::new(5.5, 64.0, 0.5))];
    rig.tick_times(20);
    assert!(
        !rig.brain
            .memories()
            .has_value(MemoryModuleType::NearestVisibleWantedItem)
    );
    let mut walled = sensor_rig(
        SensorKind::NearestItems(NearestItemSensor::new(ItemWish::Any)),
        EntityKind::Zombie,
    );
    walled.world.items = vec![item(3, Vec3::new(5.5, 64.0, 0.5))];
    walled.world.walls = (64..67).map(|y| BlockPos::new(3, y, 0)).collect();
    walled.tick_times(20);
    assert!(
        !walled
            .brain
            .memories()
            .has_value(MemoryModuleType::NearestVisibleWantedItem)
    );
}

#[test]
fn the_water_sensor_sets_and_erases_the_unit_memory() {
    let mut rig = sensor_rig(SensorKind::IsInWater(IsInWaterSensor), EntityKind::Zombie);
    rig.mob.fluid.in_water = true;
    rig.tick_times(20);
    assert_eq!(
        rig.memory(MemoryModuleType::IsInWater),
        Some(&MemoryValue::Unit)
    );
    rig.mob.fluid.in_water = false;
    rig.tick_times(20);
    assert!(!rig.brain.memories().has_value(MemoryModuleType::IsInWater));
}

#[test]
fn a_new_brain_runs_core_and_the_default_idle_activity() {
    let rig = probe_rig(Vec::new());
    let active = rig.brain.state().active_activities();
    assert!(active.contains(&Activity::Core));
    assert!(active.contains(&Activity::Idle));
    assert_eq!(active.len(), 2);
    assert_eq!(
        rig.brain.state().active_non_core_activity(),
        Some(Activity::Idle)
    );
    assert!(rig.brain.is_brain_dead());
}

#[test]
fn only_the_behaviors_of_active_activities_start_in_ascending_priority_order() {
    let log = log();
    let mut rig = probe_rig(vec![
        ActivityData::from_pairs(
            Activity::Idle,
            vec![(5, Probe::new(5, &log)), (1, Probe::new(1, &log))],
        ),
        ActivityData::from_pairs(Activity::Core, vec![(0, Probe::new(0, &log))]),
        ActivityData::from_pairs(Activity::Work, vec![(2, Probe::new(2, &log))]),
    ]);
    rig.tick();
    assert_eq!(
        entries(&log),
        vec!["try 0", "try 1", "try 5", "tick 0", "tick 1", "tick 5"]
    );
    assert_eq!(rig.brain.running_count(), 3);
}

#[test]
fn running_behaviors_keep_ticking_after_their_activity_goes_inactive() {
    let log = log();
    let mut rig = probe_rig(vec![
        ActivityData::from_pairs(Activity::Idle, vec![(1, Probe::new(1, &log))]),
        ActivityData::from_pairs(Activity::Work, vec![(2, Probe::new(2, &log))]),
    ]);
    rig.tick();
    log.borrow_mut().clear();
    rig.brain
        .state_mut()
        .set_active_activity_if_possible(Activity::Work);
    rig.tick();
    assert_eq!(entries(&log), vec!["try 2", "tick 1", "tick 2"]);
}

#[test]
fn a_refused_behavior_is_retried_every_tick_and_a_stopped_one_restarts() {
    let log = log();
    let mut rig = probe_rig(vec![ActivityData::from_pairs(
        Activity::Idle,
        vec![
            (1, Probe::new(1, &log).refusing()),
            (2, Probe::new(2, &log).stopping_after(1)),
        ],
    )]);
    rig.tick_times(2);
    assert_eq!(
        entries(&log),
        vec![
            "try 1", "try 2", "tick 2", "stop 2", "try 1", "try 2", "tick 2", "stop 2"
        ]
    );
}

#[test]
fn stop_all_stops_every_running_behavior() {
    let log = log();
    let mut rig = probe_rig(vec![ActivityData::from_pairs(
        Activity::Idle,
        vec![(1, Probe::new(1, &log)), (2, Probe::new(2, &log))],
    )]);
    rig.tick();
    log.borrow_mut().clear();
    let mut context = AiContext {
        mob: &rig.mob,
        world: &rig.world,
        brain: &mut rig.controls,
        body: &mut rig.body,
    };
    rig.brain.stop_all(&mut context);
    assert_eq!(entries(&log), vec!["stop 1", "stop 2"]);
    assert_eq!(rig.brain.running_count(), 0);
}

#[test]
fn adding_an_activity_registers_the_memories_its_behaviors_require() {
    let log = log();
    let rig = probe_rig(vec![ActivityData::from_pairs(
        Activity::Idle,
        vec![(1, Probe::new(1, &log).requiring(MemoryModuleType::Home))],
    )]);
    assert!(rig.brain.memories().is_registered(MemoryModuleType::Home));
    assert!(!rig.brain.is_brain_dead());
}

#[test]
fn activity_requirements_gate_switching_and_fall_back_to_the_default() {
    let log = log();
    let mut rig = probe_rig(vec![
        ActivityData::create_when(
            Activity::Work,
            1,
            vec![Probe::new(1, &log).requiring(MemoryModuleType::JobSite)],
            vec![(MemoryModuleType::JobSite, MemoryStatus::ValuePresent)],
        ),
        ActivityData::create(Activity::Rest, 1, vec![Probe::new(2, &log)]),
    ]);
    let state = rig.brain.state_mut();
    state.set_active_activity_if_possible(Activity::Work);
    assert!(!state.is_active(Activity::Work));
    assert!(state.is_active(Activity::Idle));
    state.memories.set(
        MemoryModuleType::JobSite,
        MemoryValue::GlobalPos(GlobalPos {
            dimension: "minecraft:overworld".to_owned(),
            pos: BlockPos::new(1, 2, 3),
        }),
    );
    state.set_active_activity_if_possible(Activity::Work);
    assert!(state.is_active(Activity::Work));
    assert!(state.is_active(Activity::Core));
    assert!(!state.is_active(Activity::Idle));
    state.set_active_activity_if_possible(Activity::Rest);
    assert!(state.is_active(Activity::Rest));
    assert!(!state.is_active(Activity::Work));
    state.set_active_activity_if_possible(Activity::Panic);
    assert!(state.is_active(Activity::Idle));
}

#[test]
fn switching_activity_erases_the_memories_of_the_activities_left() {
    let log = log();
    let mut rig = probe_rig(vec![
        ActivityData::create_erasing(
            Activity::Fight,
            1,
            vec![Probe::new(1, &log)],
            MemoryModuleType::AttackTarget,
        ),
        ActivityData::create(Activity::Idle, 1, vec![Probe::new(2, &log)]),
    ]);
    let state = rig.brain.state_mut();
    state.memories.register(MemoryModuleType::AttackTarget);
    state.memories.set(
        MemoryModuleType::AttackTarget,
        MemoryValue::Entity(EntityId(4)),
    );
    state.set_active_activity_if_possible(Activity::Fight);
    assert!(state.is_active(Activity::Fight));
    assert!(state.memories.has_value(MemoryModuleType::AttackTarget));
    state.set_active_activity_if_possible(Activity::Idle);
    assert!(state.is_active(Activity::Idle));
    assert!(!state.memories.has_value(MemoryModuleType::AttackTarget));
}

#[test]
fn switching_to_the_active_activity_changes_nothing() {
    let mut state = state_with(&[MemoryModuleType::AttackTarget]);
    state.set_core_activities([Activity::Core]);
    state.use_default_activity();
    state.memories.set(
        MemoryModuleType::AttackTarget,
        MemoryValue::Entity(EntityId(4)),
    );
    state.use_default_activity();
    assert!(state.memories.has_value(MemoryModuleType::AttackTarget));
}

#[test]
fn the_first_valid_activity_of_a_list_is_selected() {
    let log = log();
    let mut rig = probe_rig(vec![
        ActivityData::create_when(
            Activity::Fight,
            1,
            vec![Probe::new(1, &log)],
            vec![(MemoryModuleType::AttackTarget, MemoryStatus::ValuePresent)],
        ),
        ActivityData::create(Activity::Avoid, 1, vec![Probe::new(2, &log)]),
        ActivityData::create(Activity::Work, 1, vec![Probe::new(3, &log)]),
    ]);
    let state = rig.brain.state_mut();
    state.set_active_activity_to_first_valid(&[Activity::Fight, Activity::Avoid, Activity::Work]);
    assert!(state.is_active(Activity::Avoid));
    assert!(!state.is_active(Activity::Work));
    state.set_active_activity_to_first_valid(&[Activity::Panic]);
    assert!(state.is_active(Activity::Avoid));
}

#[test]
fn the_default_and_core_activities_can_be_replaced() {
    let log = log();
    let mut rig = probe_rig(vec![
        ActivityData::create(Activity::Swim, 1, vec![Probe::new(1, &log)]),
        ActivityData::create(Activity::Rest, 1, vec![Probe::new(2, &log)]),
    ]);
    let state = rig.brain.state_mut();
    state.set_default_activity(Activity::Rest);
    state.set_core_activities([Activity::Core, Activity::Swim]);
    state.use_default_activity();
    assert!(state.is_active(Activity::Rest));
    assert!(state.is_active(Activity::Swim));
    assert!(state.is_active(Activity::Core));
    assert_eq!(state.active_non_core_activity(), Some(Activity::Rest));
}

#[test]
fn the_villager_schedule_matches_the_timeline_keyframes() {
    let at = |ticks: i64| Schedule::VILLAGER.activity_at(ticks);
    assert_eq!(at(0), Activity::Rest);
    assert_eq!(at(9), Activity::Rest);
    assert_eq!(at(10), Activity::Idle);
    assert_eq!(at(1999), Activity::Idle);
    assert_eq!(at(2000), Activity::Work);
    assert_eq!(at(8999), Activity::Work);
    assert_eq!(at(9000), Activity::Meet);
    assert_eq!(at(10999), Activity::Meet);
    assert_eq!(at(11000), Activity::Idle);
    assert_eq!(at(11999), Activity::Idle);
    assert_eq!(at(12000), Activity::Rest);
    assert_eq!(at(23999), Activity::Rest);
    assert_eq!(at(VILLAGER_PERIOD_TICKS + 2000), Activity::Work);
    assert_eq!(at(-1), Activity::Rest);
}

#[test]
fn the_baby_villager_schedule_matches_the_timeline_keyframes() {
    let at = |ticks: i64| Schedule::BABY_VILLAGER.activity_at(ticks);
    assert_eq!(at(10), Activity::Idle);
    assert_eq!(at(3000), Activity::Play);
    assert_eq!(at(6000), Activity::Idle);
    assert_eq!(at(10000), Activity::Play);
    assert_eq!(at(12000), Activity::Rest);
    assert_eq!(at(5), Activity::Rest);
}

fn schedule_rig() -> Rig<Probe> {
    let log = log();
    let mut rig = probe_rig(
        [Activity::Work, Activity::Meet, Activity::Rest]
            .into_iter()
            .map(|activity| ActivityData::create(activity, 1, vec![Probe::new(1, &log)]))
            .collect(),
    );
    rig.brain.state_mut().set_schedule(Some(Schedule::VILLAGER));
    rig
}

fn schedule_update(rig: &mut Rig<Probe>, game_time: i64, clock: i64) {
    rig.world.time.set(game_time);
    rig.world.clock.set(clock);
    let context = AiContext {
        mob: &rig.mob,
        world: &rig.world,
        brain: &mut rig.controls,
        body: &mut rig.body,
    };
    rig.brain.update_activity_from_schedule(&context);
}

#[test]
fn the_schedule_switches_activity_at_its_keyframes() {
    let mut rig = schedule_rig();
    let active = |rig: &Rig<Probe>| rig.brain.state().active_non_core_activity();
    schedule_update(&mut rig, 100, 2000);
    assert_eq!(active(&rig), Some(Activity::Work));
    schedule_update(&mut rig, 200, 9000);
    assert_eq!(active(&rig), Some(Activity::Meet));
    schedule_update(&mut rig, 300, 11000);
    assert_eq!(active(&rig), Some(Activity::Idle));
    schedule_update(&mut rig, 400, 12000);
    assert_eq!(active(&rig), Some(Activity::Rest));
}

#[test]
fn the_schedule_is_only_read_once_the_interval_has_elapsed() {
    let mut rig = schedule_rig();
    schedule_update(&mut rig, 100, 2000);
    assert!(rig.brain.state().is_active(Activity::Work));
    schedule_update(&mut rig, 120, 9000);
    assert!(rig.brain.state().is_active(Activity::Work));
    schedule_update(&mut rig, 121, 9000);
    assert!(rig.brain.state().is_active(Activity::Meet));
}

#[test]
fn an_activity_the_schedule_wants_but_cannot_enter_falls_back_to_the_default() {
    let log = log();
    let mut rig = probe_rig(vec![ActivityData::create_when(
        Activity::Work,
        1,
        vec![Probe::new(1, &log)],
        vec![(MemoryModuleType::JobSite, MemoryStatus::ValuePresent)],
    )]);
    rig.brain.state_mut().set_schedule(Some(Schedule::VILLAGER));
    schedule_update(&mut rig, 100, 2000);
    assert!(rig.brain.state().is_active(Activity::Idle));
    assert!(!rig.brain.state().is_active(Activity::Work));
}

#[test]
fn without_a_schedule_the_brain_stays_on_idle() {
    let mut rig = probe_rig(Vec::new());
    rig.brain
        .state_mut()
        .set_active_activity_if_possible(Activity::Work);
    assert!(rig.brain.state().is_active(Activity::Idle));
    schedule_update(&mut rig, 100, 2000);
    assert!(rig.brain.state().is_active(Activity::Idle));
}

#[derive(Debug, Clone, PartialEq)]
struct Count {
    fires: Rc<Cell<i32>>,
    result: bool,
}

impl Trigger for Count {
    fn trigger<W: AiWorld>(&mut self, _cx: &mut BehaviorCx<'_, '_, W>, _now: i64) -> bool {
        self.fires.set(self.fires.get() + 1);
        self.result
    }
}

fn counter(result: bool) -> (Count, Rc<Cell<i32>>) {
    let fires = Rc::new(Cell::new(0));
    (
        Count {
            fires: Rc::clone(&fires),
            result,
        },
        fires,
    )
}

#[test]
fn a_one_shot_starts_only_when_its_memories_and_trigger_hold_and_stops_next_tick() {
    let world = Field::default();
    let mut state = state_with(&[MemoryModuleType::AttackTarget]);
    let (trigger, fires) = counter(true);
    let mut one_shot = OneShot::new(
        vec![(MemoryModuleType::AttackTarget, MemoryStatus::ValuePresent)],
        trigger,
    );
    with_cx(&mut state, 1, &world, |cx| {
        assert!(!one_shot.try_start(cx, 0));
        assert_eq!(fires.get(), 0);
        assert_eq!(one_shot.status(), Status::Stopped);
        cx.brain.memories.set(
            MemoryModuleType::AttackTarget,
            MemoryValue::Entity(EntityId(1)),
        );
        assert!(one_shot.try_start(cx, 0));
        assert_eq!(fires.get(), 1);
        assert_eq!(one_shot.status(), Status::Running);
        one_shot.tick_or_stop(cx, 1);
        assert_eq!(one_shot.status(), Status::Stopped);
        assert_eq!(fires.get(), 1);
    });
}

#[test]
fn a_one_shot_whose_trigger_declines_does_not_run() {
    let world = Field::default();
    let mut state = state_with(&[]);
    let (trigger, fires) = counter(false);
    let mut one_shot = OneShot::new(Vec::new(), trigger);
    with_cx(&mut state, 1, &world, |cx| {
        assert!(!one_shot.try_start(cx, 0));
        assert_eq!(fires.get(), 1);
        assert_eq!(one_shot.status(), Status::Stopped);
    });
}

#[test]
fn a_one_shot_requires_its_condition_memories_and_the_trigger_ones() {
    let (trigger, _) = counter(true);
    let one_shot = OneShot::new(
        vec![
            (MemoryModuleType::Home, MemoryStatus::ValuePresent),
            (MemoryModuleType::JobSite, MemoryStatus::ValueAbsent),
        ],
        trigger,
    );
    assert_eq!(
        BehaviorControl::required_memories(&one_shot),
        vec![MemoryModuleType::Home, MemoryModuleType::JobSite]
    );
}

#[test]
fn a_sequence_runs_the_second_one_shot_only_after_the_first_trigger_passes() {
    let world = Field::default();
    let mut state = state_with(&[MemoryModuleType::Home]);
    let (first_pass, first_fires) = counter(true);
    let (first_fail, fail_fires) = counter(false);
    let (second, second_fires) = counter(true);
    let build = |first: Count, second: Count| {
        OneShot::sequence(
            first,
            OneShot::new(
                vec![(MemoryModuleType::Home, MemoryStatus::ValuePresent)],
                second,
            ),
        )
    };
    let mut passing = build(first_pass, second.clone());
    let mut failing = build(first_fail, second);
    assert_eq!(
        BehaviorControl::required_memories(&passing),
        vec![MemoryModuleType::Home]
    );
    with_cx(&mut state, 1, &world, |cx| {
        assert!(!passing.try_start(cx, 0));
        assert_eq!(first_fires.get(), 1);
        assert_eq!(second_fires.get(), 0);
        cx.brain.memories.set(
            MemoryModuleType::Home,
            MemoryValue::GlobalPos(GlobalPos {
                dimension: "minecraft:overworld".to_owned(),
                pos: BlockPos::new(0, 0, 0),
            }),
        );
        assert!(!failing.try_start(cx, 0));
        assert_eq!(fail_fires.get(), 1);
        assert_eq!(second_fires.get(), 0);
        assert!(passing.try_start(cx, 0));
        assert_eq!(second_fires.get(), 1);
    });
}

#[test]
fn a_trigger_gate_always_succeeds_and_run_one_stops_after_the_first_success() {
    let world = Field::default();
    let mut state = state_with(&[]);
    let (a, a_fires) = counter(false);
    let (b, b_fires) = counter(true);
    let (c, c_fires) = counter(true);
    let mut gate = TriggerGate::one_shot(
        vec![(a, 1), (b, 1), (c, 1)],
        OrderPolicy::Ordered,
        RunningPolicy::RunOne,
    );
    with_cx(&mut state, 1, &world, |cx| {
        assert!(gate.try_start(cx, 0));
    });
    assert_eq!((a_fires.get(), b_fires.get(), c_fires.get()), (1, 1, 0));
}

#[test]
fn a_trigger_gate_try_all_runs_every_trigger_and_still_succeeds_when_none_pass() {
    let world = Field::default();
    let mut state = state_with(&[]);
    let (a, a_fires) = counter(true);
    let (b, b_fires) = counter(false);
    let mut gate = TriggerGate::one_shot(
        vec![(a, 1), (b, 1)],
        OrderPolicy::Ordered,
        RunningPolicy::TryAll,
    );
    with_cx(&mut state, 1, &world, |cx| {
        assert!(gate.try_start(cx, 0));
    });
    assert_eq!((a_fires.get(), b_fires.get()), (1, 1));
    let (never, never_fires) = counter(false);
    let mut gate = TriggerGate::one_shuffled(vec![(never, 1)]);
    with_cx(&mut state, 1, &world, |cx| {
        assert!(gate.try_start(cx, 0));
    });
    assert_eq!(never_fires.get(), 1);
}

#[test]
fn a_shuffling_list_orders_by_weighted_random_keys() {
    let mut heavy_first = 0;
    for seed in 0..400 {
        let mut random = rspigot_ai::MobRandom::new(seed);
        let mut list: ShufflingList<&str> = [("light", 1), ("heavy", 9)].into_iter().collect();
        list.shuffle(&mut random);
        if list.get(0) == Some(&"heavy") {
            heavy_first += 1;
        }
    }
    assert!((320..=395).contains(&heavy_first), "{heavy_first}");
    let mut list: ShufflingList<i32> = [(1, 1), (2, 1)].into_iter().collect();
    assert_eq!(list.len(), 2);
    assert_eq!(list.weight(1), Some(1));
    assert!(!list.is_empty());
    let mut random = rspigot_ai::MobRandom::new(5);
    let before: Vec<i32> = list.iter().copied().collect();
    list.shuffle(&mut random);
    let mut after: Vec<i32> = list.iter().copied().collect();
    after.sort_unstable();
    assert_eq!(after, before);
}

#[test]
fn a_run_one_gate_starts_a_single_stopped_behavior_in_shuffled_order() {
    let world = Field::default();
    let mut state = state_with(&[]);
    let log = log();
    let mut gate = Gate::run_one(vec![
        (Probe::new(1, &log).refusing(), 1),
        (Probe::new(2, &log), 1),
        (Probe::new(3, &log), 1),
    ]);
    with_cx(&mut state, 4, &world, |cx| {
        assert!(gate.try_start(cx, 0));
    });
    let tried = entries(&log);
    let started = tried
        .iter()
        .filter(|entry| entry.as_str() != "try 1")
        .count();
    assert_eq!(started, 1);
    assert_eq!(gate.status(), Status::Running);
    let running = gate
        .behaviors()
        .iter()
        .filter(|probe| probe.status() == Status::Running)
        .count();
    assert_eq!(running, 1);
}

#[test]
fn a_gate_stops_when_its_last_running_behavior_stops_and_erases_its_memories() {
    let world = Field::default();
    let mut state = state_with(&[MemoryModuleType::LastWoken]);
    let log = log();
    let mut gate = Gate::new(
        Vec::new(),
        vec![MemoryModuleType::LastWoken],
        OrderPolicy::Ordered,
        RunningPolicy::TryAll,
        vec![
            (Probe::new(1, &log).stopping_after(1), 1),
            (Probe::new(2, &log).stopping_after(2), 1),
        ],
    );
    with_cx(&mut state, 1, &world, |cx| {
        cx.brain
            .memories
            .set(MemoryModuleType::LastWoken, MemoryValue::Long(3));
        assert!(gate.try_start(cx, 0));
        gate.tick_or_stop(cx, 1);
        assert_eq!(gate.status(), Status::Running);
        assert!(cx.brain.memories.has_value(MemoryModuleType::LastWoken));
        gate.tick_or_stop(cx, 2);
        assert_eq!(gate.status(), Status::Stopped);
        assert!(!cx.brain.memories.has_value(MemoryModuleType::LastWoken));
    });
    assert_eq!(
        entries(&log),
        vec![
            "try 1", "try 2", "tick 1", "stop 1", "tick 2", "tick 2", "stop 2"
        ]
    );
}

#[test]
fn a_gate_do_stop_stops_the_running_behaviors() {
    let world = Field::default();
    let mut state = state_with(&[]);
    let log = log();
    let mut gate = Gate::new(
        Vec::new(),
        Vec::new(),
        OrderPolicy::Ordered,
        RunningPolicy::TryAll,
        vec![
            (Probe::new(1, &log), 1),
            (Probe::new(2, &log).refusing(), 1),
        ],
    );
    with_cx(&mut state, 1, &world, |cx| {
        gate.try_start(cx, 0);
        gate.do_stop(cx, 1);
    });
    assert_eq!(entries(&log), vec!["try 1", "try 2", "stop 1"]);
    assert_eq!(gate.status(), Status::Stopped);
}

#[test]
fn a_gate_needs_its_entry_conditions_and_requires_every_child_memory() {
    let world = Field::default();
    let mut state = state_with(&[MemoryModuleType::Home]);
    let log = log();
    let mut gate = Gate::run_one_when(
        vec![(MemoryModuleType::Home, MemoryStatus::ValuePresent)],
        vec![(Probe::new(1, &log).requiring(MemoryModuleType::JobSite), 1)],
    );
    assert_eq!(
        gate.required_memories(),
        vec![MemoryModuleType::Home, MemoryModuleType::JobSite]
    );
    with_cx(&mut state, 1, &world, |cx| {
        assert!(!gate.try_start(cx, 0));
    });
    assert_eq!(gate.status(), Status::Stopped);
    assert!(entries(&log).is_empty());
}

struct Counting {
    uses: i32,
    allow: bool,
    ended: Rc<RefCell<Vec<&'static str>>>,
}

impl rspigot_ai::Behavior for Counting {
    fn entry_condition(&self) -> &[(MemoryModuleType, MemoryStatus)] {
        &[]
    }

    fn check_extra_start_conditions<W: AiWorld>(
        &mut self,
        _cx: &mut BehaviorCx<'_, '_, W>,
    ) -> bool {
        self.allow
    }

    fn start<W: AiWorld>(&mut self, _cx: &mut BehaviorCx<'_, '_, W>, _now: i64) {
        self.ended.borrow_mut().push("start");
    }

    fn can_still_use<W: AiWorld>(&mut self, _cx: &mut BehaviorCx<'_, '_, W>, _now: i64) -> bool {
        self.uses > 0
    }

    fn tick<W: AiWorld>(&mut self, _cx: &mut BehaviorCx<'_, '_, W>, _now: i64) {
        self.uses -= 1;
        self.ended.borrow_mut().push("tick");
    }

    fn stop<W: AiWorld>(&mut self, _cx: &mut BehaviorCx<'_, '_, W>, _now: i64) {
        self.ended.borrow_mut().push("stop");
    }
}

fn counting(uses: i32, allow: bool) -> (Timed<Counting>, Rc<RefCell<Vec<&'static str>>>) {
    let ended = Rc::new(RefCell::new(Vec::new()));
    (
        Timed::with_duration(
            Counting {
                uses,
                allow,
                ended: Rc::clone(&ended),
            },
            10,
            10,
        ),
        ended,
    )
}

#[test]
fn a_timed_behavior_runs_until_it_cannot_continue_then_stops() {
    let world = Field::default();
    let mut state = state_with(&[]);
    let (mut timed, ended) = counting(2, true);
    with_cx(&mut state, 1, &world, |cx| {
        assert!(timed.try_start(cx, 100));
        assert_eq!(timed.end_timestamp(), 110);
        timed.tick_or_stop(cx, 101);
        timed.tick_or_stop(cx, 102);
        assert_eq!(timed.status(), Status::Running);
        timed.tick_or_stop(cx, 103);
        assert_eq!(timed.status(), Status::Stopped);
    });
    assert_eq!(*ended.borrow(), vec!["start", "tick", "tick", "stop"]);
}

#[test]
fn a_timed_behavior_stops_once_the_timestamp_passes_its_end() {
    let world = Field::default();
    let mut state = state_with(&[]);
    let (mut timed, ended) = counting(100, true);
    with_cx(&mut state, 1, &world, |cx| {
        assert!(timed.try_start(cx, 100));
        timed.tick_or_stop(cx, 110);
        assert_eq!(timed.status(), Status::Running);
        timed.tick_or_stop(cx, 111);
        assert_eq!(timed.status(), Status::Stopped);
    });
    assert_eq!(*ended.borrow(), vec!["start", "tick", "stop"]);
}

#[test]
fn a_timed_behavior_does_not_start_when_the_extra_conditions_fail() {
    let world = Field::default();
    let mut state = state_with(&[]);
    let (mut timed, ended) = counting(2, false);
    with_cx(&mut state, 1, &world, |cx| {
        assert!(!timed.try_start(cx, 100));
    });
    assert_eq!(timed.status(), Status::Stopped);
    assert!(ended.borrow().is_empty());
}

#[test]
fn a_timed_duration_is_drawn_between_min_and_max_inclusive() {
    let world = Field::default();
    let mut state = state_with(&[]);
    let mut seen = std::collections::BTreeSet::new();
    for seed in 0..200 {
        let ended = Rc::new(RefCell::new(Vec::new()));
        let mut timed = Timed::with_duration(
            Counting {
                uses: 1,
                allow: true,
                ended,
            },
            5,
            8,
        );
        with_cx(&mut state, seed * 2_654_435_761, &world, |cx| {
            timed.try_start(cx, 1000);
        });
        seen.insert(timed.end_timestamp() - 1000);
    }
    assert_eq!(seen.into_iter().collect::<Vec<_>>(), vec![5, 6, 7, 8]);
}

#[test]
fn a_timed_behavior_without_a_duration_lasts_sixty_ticks() {
    let world = Field::default();
    let mut state = state_with(&[]);
    let ended = Rc::new(RefCell::new(Vec::new()));
    let mut timed = Timed::new(Counting {
        uses: 1,
        allow: true,
        ended,
    });
    with_cx(&mut state, 3, &world, |cx| {
        timed.try_start(cx, 0);
    });
    assert_eq!(timed.end_timestamp(), 60);
}

#[test]
fn do_nothing_runs_for_its_drawn_duration_then_stops() {
    let world = Field::default();
    let mut state = state_with(&[]);
    let mut idle = DoNothing::new(3, 3);
    assert!(idle.required_memories().is_empty());
    with_cx(&mut state, 1, &world, |cx| {
        assert!(idle.try_start(cx, 10));
        idle.tick_or_stop(cx, 13);
        assert_eq!(idle.status(), Status::Running);
        idle.tick_or_stop(cx, 14);
        assert_eq!(idle.status(), Status::Stopped);
    });
}

fn count_down_brain(initial: i32, random: &mut rspigot_ai::MobRandom) -> BehaviorBrain<MobControl> {
    let control = MobControl::Timed(Timed::new(MobBehavior::CountDownCooldownTicks(
        CountDownCooldownTicks::new(MemoryModuleType::AttackTargetCooldown),
    )));
    let mut brain = BehaviorBrain::new(
        &[],
        Vec::new(),
        vec![ActivityData::create(Activity::Core, 0, vec![control])],
        &PackedMemories::default(),
        random,
    );
    brain.memories_mut().set(
        MemoryModuleType::AttackTargetCooldown,
        MemoryValue::Int(initial),
    );
    brain
}

fn count_down_rig(initial: i32) -> Rig<MobControl> {
    let mob = view(EntityKind::Zombie, HOME);
    let mut controls = Brain::new(EntityKind::Zombie, 1);
    let brain = count_down_brain(initial, &mut controls.random);
    Rig {
        body: MobBody::of(&mob),
        mob,
        world: Field::default(),
        controls,
        brain,
    }
}

#[test]
fn count_down_cooldown_ticks_decrements_the_memory_then_erases_it() {
    let mut rig = count_down_rig(3);
    let value = |rig: &Rig<MobControl>| {
        rig.memory(MemoryModuleType::AttackTargetCooldown)
            .and_then(MemoryValue::as_int)
    };
    assert!(
        rig.brain
            .memories()
            .is_registered(MemoryModuleType::AttackTargetCooldown)
    );
    rig.tick();
    assert_eq!(value(&rig), Some(2));
    rig.tick();
    assert_eq!(value(&rig), Some(1));
    rig.tick();
    assert_eq!(value(&rig), Some(0));
    rig.tick();
    assert_eq!(value(&rig), None);
    assert_eq!(rig.brain.running_count(), 0);
}

#[test]
fn count_down_cooldown_ticks_does_not_start_without_the_memory() {
    let mut rig = count_down_rig(3);
    rig.brain
        .memories_mut()
        .erase(MemoryModuleType::AttackTargetCooldown);
    rig.tick_times(3);
    assert_eq!(rig.brain.running_count(), 0);
}

fn global(x: i32, y: i32, z: i32) -> GlobalPos {
    GlobalPos {
        dimension: "minecraft:overworld".to_owned(),
        pos: BlockPos::new(x, y, z),
    }
}

fn persistent_memories() -> Memories {
    let mut memories = Memories::default();
    for ty in MemoryModuleType::ALL {
        memories.register(*ty);
    }
    memories
}

#[test]
fn persisted_memories_use_the_vanilla_nbt_layout() {
    let mut memories = persistent_memories();
    let uuid = Uuid::from_u128(0x0123_4567_89ab_cdef_fedc_ba98_7654_3210);
    memories.set(
        MemoryModuleType::Home,
        MemoryValue::GlobalPos(global(1, 2, -3)),
    );
    memories.set_with_expiry(MemoryModuleType::LastSlept, MemoryValue::Long(5000), 100);
    memories.set(MemoryModuleType::IsPanicking, MemoryValue::Bool(true));
    memories.set(MemoryModuleType::PlayDeadTicks, MemoryValue::Int(7));
    memories.set(MemoryModuleType::IsPregnant, MemoryValue::Unit);
    memories.set(MemoryModuleType::AngryAt, MemoryValue::Uuid(uuid));
    memories.set(
        MemoryModuleType::BreezeJumpTarget,
        MemoryValue::Block(BlockPos::new(4, 5, 6)),
    );
    memories.set(
        MemoryModuleType::VisitedBlockPositions,
        MemoryValue::GlobalPosList(vec![global(7, 8, 9)]),
    );
    let nbt = memories.pack().to_nbt();
    let stored = nbt.get_compound("memories").unwrap();
    let entry = |name: &str| stored.get_compound(name).unwrap();
    let home = entry("minecraft:home");
    assert_eq!(home.len(), 1);
    let pos = home.get_compound("value").unwrap();
    assert_eq!(pos.get_str("dimension"), Some("minecraft:overworld"));
    assert_eq!(pos.get_int_array("pos"), Some(&[1, 2, -3][..]));
    let slept = entry("minecraft:last_slept");
    assert_eq!(slept.get("value"), Some(&Tag::Long(5000)));
    assert_eq!(slept.get("ttl"), Some(&Tag::Long(100)));
    assert_eq!(
        entry("minecraft:is_panicking").get("value"),
        Some(&Tag::Byte(1))
    );
    assert_eq!(
        entry("minecraft:play_dead_ticks").get("value"),
        Some(&Tag::Int(7))
    );
    assert_eq!(
        entry("minecraft:is_pregnant").get("value"),
        Some(&Tag::Compound(Compound::new()))
    );
    assert_eq!(
        entry("minecraft:angry_at").get_int_array("value"),
        Some(
            &[
                0x0123_4567,
                0x89ab_cdefu32 as i32,
                0xfedc_ba98u32 as i32,
                0x7654_3210
            ][..]
        )
    );
    assert_eq!(
        entry("minecraft:breeze_jump_target").get_int_array("value"),
        Some(&[4, 5, 6][..])
    );
    let visited = entry("minecraft:visited_block_positions");
    let list = visited.get_list("value").unwrap();
    assert_eq!(list.len(), 1);
    assert_eq!(stored.len(), 8);
}

#[test]
fn persisted_memories_round_trip_through_nbt() {
    let mut memories = persistent_memories();
    memories.set(
        MemoryModuleType::Home,
        MemoryValue::GlobalPos(global(1, 2, -3)),
    );
    memories.set(
        MemoryModuleType::JobSite,
        MemoryValue::GlobalPos(global(0, 64, 0)),
    );
    memories.set_with_expiry(MemoryModuleType::LastWoken, MemoryValue::Long(-1), 42);
    memories.set(
        MemoryModuleType::GolemDetectedRecently,
        MemoryValue::Bool(false),
    );
    memories.set(
        MemoryModuleType::AttackTargetCooldown,
        MemoryValue::Int(i32::MIN),
    );
    memories.set(MemoryModuleType::IsSniffing, MemoryValue::Unit);
    memories.set(
        MemoryModuleType::LikedPlayer,
        MemoryValue::Uuid(Uuid::from_u128(u128::MAX - 5)),
    );
    memories.set(
        MemoryModuleType::SnifferExploredPositions,
        MemoryValue::GlobalPosList(vec![global(1, 1, 1), global(-2, -2, -2)]),
    );
    let nbt = memories.pack().to_nbt();
    let mut restored = persistent_memories();
    restored.load(&PackedMemories::from_nbt(&nbt));
    assert_eq!(restored, memories);
    assert_eq!(
        restored.time_until_expiry(MemoryModuleType::LastWoken),
        Some(42)
    );
}

#[test]
fn the_remaining_time_to_live_is_what_gets_persisted() {
    let mut memories = persistent_memories();
    memories.set_with_expiry(MemoryModuleType::LastWoken, MemoryValue::Long(1), 10);
    memories.forget_outdated();
    memories.forget_outdated();
    let nbt = memories.pack().to_nbt();
    let entry = nbt
        .get_compound("memories")
        .and_then(|stored| stored.get_compound("minecraft:last_woken"))
        .unwrap();
    assert_eq!(entry.get_i64("ttl"), Some(8));
}

#[test]
fn memories_vanilla_does_not_serialize_are_never_written() {
    let mut memories = persistent_memories();
    memories.set(
        MemoryModuleType::AttackTarget,
        MemoryValue::Entity(EntityId(3)),
    );
    memories.set(
        MemoryModuleType::NearestLivingEntities,
        MemoryValue::Entities(vec![EntityId(3)]),
    );
    memories.set(
        MemoryModuleType::NearestVisibleLivingEntities,
        MemoryValue::Visible(NearestVisibleLivingEntities::new(vec![EntityId(3)])),
    );
    let nbt = memories.pack().to_nbt();
    assert!(nbt.get_compound("memories").unwrap().is_empty());
    assert!(MemoryModuleType::Home.can_serialize());
    assert!(!MemoryModuleType::WalkTarget.can_serialize());
}

#[test]
fn loading_skips_unknown_transient_and_malformed_entries() {
    let value = |tag: Tag| Compound::new().with("value", tag);
    let memories = Compound::new()
        .with("minecraft:last_woken", value(Tag::Long(9)))
        .with("minecraft:nope", value(Tag::Long(1)))
        .with("minecraft:walk_target", value(Tag::Long(1)))
        .with("minecraft:last_slept", value(Tag::String("x".to_owned())))
        .with("minecraft:home", Compound::new())
        .with("minecraft:play_dead_ticks", Tag::Int(3))
        .with(
            "minecraft:breeze_jump_target",
            value(Tag::IntArray(vec![1, 2])),
        )
        .with(
            "minecraft:is_panicking",
            value(Tag::Byte(1)).with("ttl", Tag::String("soon".to_owned())),
        );
    let packed = PackedMemories::from_nbt(&Compound::new().with("memories", memories));
    let names: Vec<&str> = packed.entries.iter().map(|(ty, _)| ty.name()).collect();
    assert_eq!(
        names,
        vec!["minecraft:last_woken", "minecraft:is_panicking"]
    );
    assert_eq!(packed.entries[1].1.time_to_live, None);
    assert!(
        PackedMemories::from_nbt(&Compound::new())
            .entries
            .is_empty()
    );
}

#[test]
fn loading_only_fills_registered_memories() {
    let mut memories = Memories::default();
    memories.register(MemoryModuleType::LastWoken);
    let packed = PackedMemories {
        entries: vec![
            (
                MemoryModuleType::LastWoken,
                ExpirableValue::of(MemoryValue::Long(1)),
            ),
            (
                MemoryModuleType::LastSlept,
                ExpirableValue::of(MemoryValue::Long(2)),
            ),
        ],
    };
    memories.load(&packed);
    assert!(memories.has_value(MemoryModuleType::LastWoken));
    assert!(!memories.is_registered(MemoryModuleType::LastSlept));
}

#[test]
fn a_brain_saves_and_restores_its_memories_through_nbt() {
    let log = log();
    let mut original = probe_rig(vec![ActivityData::create(
        Activity::Idle,
        1,
        vec![Probe::new(1, &log).requiring(MemoryModuleType::Home)],
    )]);
    original
        .brain
        .memories_mut()
        .register(MemoryModuleType::LastWoken);
    original.brain.memories_mut().set(
        MemoryModuleType::Home,
        MemoryValue::GlobalPos(global(4, 70, -9)),
    );
    original.brain.memories_mut().set_with_expiry(
        MemoryModuleType::LastWoken,
        MemoryValue::Long(12),
        30,
    );
    let nbt = original.brain.save();
    let mut controls = Brain::new(EntityKind::Zombie, 2);
    let packed = PackedMemories::from_nbt(&nbt);
    let restored = BehaviorBrain::new(
        &[MemoryModuleType::Home, MemoryModuleType::LastWoken],
        Vec::new(),
        Vec::<ActivityData<Probe>>::new(),
        &packed,
        &mut controls.random,
    );
    assert_eq!(
        restored.memories().get(MemoryModuleType::Home),
        original.brain.memories().get(MemoryModuleType::Home)
    );
    assert_eq!(
        restored
            .memories()
            .time_until_expiry(MemoryModuleType::LastWoken),
        Some(30)
    );
    let mut reloaded = probe_rig(Vec::new());
    reloaded
        .brain
        .memories_mut()
        .register(MemoryModuleType::Home);
    reloaded.brain.load(&nbt);
    assert!(reloaded.brain.memories().has_value(MemoryModuleType::Home));
}

fn brain_mob(initial: i32) -> MobAi {
    let mut ai = MobAi::new(EntityKind::Zombie, 7, rspigot_ai::GoalSettings::default());
    let mut random = rspigot_ai::MobRandom::new(1);
    ai.set_behavior_brain(Some(count_down_brain(initial, &mut random)));
    ai
}

#[test]
fn the_mob_ai_step_ticks_the_brain_next_to_the_goal_selectors() {
    let mut ai = brain_mob(2);
    let world = Field::default();
    let mut mob = view(EntityKind::Zombie, HOME);
    mob.tick_count = 10;
    let mut body = MobBody::of(&mob);
    let cooldown = |ai: &MobAi| {
        ai.behavior_brain()
            .and_then(|brain| brain.memories().get(MemoryModuleType::AttackTargetCooldown))
            .and_then(MemoryValue::as_int)
    };
    assert_eq!(cooldown(&ai), Some(2));
    ai.think(&mob, &world, &mut body);
    assert_eq!(cooldown(&ai), Some(1));
    ai.think(&mob, &world, &mut body);
    assert_eq!(cooldown(&ai), Some(0));
    ai.think(&mob, &world, &mut body);
    assert_eq!(cooldown(&ai), None);
}

#[test]
fn a_mob_without_a_brain_keeps_running_its_goals() {
    let mut ai = MobAi::new(EntityKind::Zombie, 7, rspigot_ai::GoalSettings::default());
    assert!(ai.behavior_brain().is_none());
    assert!(ai.has_ai());
    assert!(ai.save_behavior_brain().is_none());
    let world = Field::default();
    let mob = view(EntityKind::Zombie, HOME);
    let mut body = MobBody::of(&mob);
    ai.think(&mob, &world, &mut body);
    assert!(!ai.goals().goals().is_empty());
}

#[test]
fn a_mob_with_a_brain_counts_as_having_an_ai_even_without_goals() {
    let mut bare = MobAi::new(
        EntityKind::ArmorStand,
        1,
        rspigot_ai::GoalSettings::default(),
    );
    assert!(!bare.has_ai());
    let ai = brain_mob(1);
    assert!(ai.has_ai());
    bare.set_behavior_brain(ai.behavior_brain().cloned());
    assert!(bare.has_ai());
    assert!(bare.goals().goals().is_empty());
    assert!(bare.targets().goals().is_empty());
}

#[test]
fn the_mob_ai_saves_and_loads_its_brain_memories() {
    let mut ai = brain_mob(5);
    let nbt = ai.save_behavior_brain().unwrap();
    assert!(nbt.get_compound("memories").is_some());
    ai.load_behavior_brain(&nbt);
    let mut empty = MobAi::new(EntityKind::Zombie, 1, rspigot_ai::GoalSettings::default());
    empty.load_behavior_brain(&nbt);
    assert!(empty.behavior_brain().is_none());
}

#[test]
fn stopping_the_mob_brain_stops_its_running_behaviors() {
    let mut ai = brain_mob(5);
    let world = Field::default();
    let mob = view(EntityKind::Zombie, HOME);
    let mut body = MobBody::of(&mob);
    ai.think(&mob, &world, &mut body);
    assert_eq!(ai.behavior_brain().unwrap().running_count(), 1);
    ai.stop_behavior_brain(&mob, &world, &mut body);
    assert_eq!(ai.behavior_brain().unwrap().running_count(), 0);
}

use std::collections::HashMap;

use rspigot_ai::{
    Activity, AiContext, AiWorld, BehaviorControl, BehaviorCx, BrainState, Facts, GoalSettings,
    Living, MemoryModuleType, MemoryValue, MobAction, MobAi, MobBody, MobControl, MobView,
    SensorKind, Status, TargetKind,
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

fn warden_ai(seed: i64) -> MobAi {
    MobAi::new(EntityKind::Warden, seed, GoalSettings::default())
}

fn listed(activity: Activity) -> Vec<(i32, String)> {
    let ai = warden_ai(1);
    ai.behavior_brain()
        .expect("a brain")
        .activity_behaviors(activity)
        .into_iter()
        .map(|(priority, control)| (priority, describe(control)))
        .collect()
}

fn control(activity: Activity, name: &str) -> MobControl {
    let ai = warden_ai(1);
    ai.behavior_brain()
        .unwrap()
        .activity_behaviors(activity)
        .into_iter()
        .map(|(_, control)| control.clone())
        .find(|control| describe(control).contains(&format!("{name}(")))
        .expect("the behavior")
}

fn names(activity: Activity, known: &[&'static str]) -> Vec<(i32, &'static str)> {
    listed(activity)
        .iter()
        .map(|(priority, text)| {
            let mut found: Vec<(usize, &'static str)> = known
                .iter()
                .filter_map(|name| text.find(&format!("{name}(")).map(|at| (at, *name)))
                .collect();
            found.sort_by_key(|(at, _)| *at);
            (*priority, found.first().map_or("?", |(_, name)| *name))
        })
        .collect()
}

const KNOWN: [&str; 17] = [
    "Swim",
    "WardenSetLookTarget",
    "LookAtTargetSink",
    "MoveToTargetSink",
    "WardenEmerging",
    "WardenForceUnmount",
    "WardenDigging",
    "WardenSetRoarTarget",
    "WardenTryToSniff",
    "Gate",
    "GoToTargetLocation",
    "WardenSniffing",
    "WardenRoar",
    "WardenDigCooldownSetter",
    "StopAttackingIfTargetInvalid",
    "WardenLookAtAttackTarget",
    "SetWalkTargetFromAttackTargetIfTargetOutOfReach",
];

const WARDEN: Vec3 = Vec3::new(0.5, 64.0, 0.5);

fn bench(world: Field) -> Bench {
    Bench::new(EntityKind::Warden, WARDEN, world, 3)
}

fn player(id: i32, at: Vec3) -> Living {
    living(id, EntityKind::Player, at)
}

fn world_with(players: Vec<Living>) -> Field {
    let mut world = Field::flat();
    world.players = players;
    world
}

fn entity(id: i32) -> MemoryValue {
    MemoryValue::Entity(EntityId(id))
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

#[test]
fn the_warden_activities_follow_the_vanilla_priorities() {
    assert_eq!(
        names(Activity::Core, &KNOWN),
        vec![
            (0, "Swim"),
            (1, "WardenSetLookTarget"),
            (2, "LookAtTargetSink"),
            (3, "MoveToTargetSink"),
        ]
    );
    assert_eq!(names(Activity::Emerge, &KNOWN), vec![(5, "WardenEmerging")]);
    assert_eq!(
        names(Activity::Dig, &KNOWN),
        vec![(0, "WardenForceUnmount"), (1, "WardenDigging")]
    );
    assert_eq!(
        names(Activity::Idle, &KNOWN),
        vec![
            (10, "WardenSetRoarTarget"),
            (11, "WardenTryToSniff"),
            (12, "Gate")
        ]
    );
    assert_eq!(
        names(Activity::Investigate, &KNOWN),
        vec![(5, "WardenSetRoarTarget"), (6, "GoToTargetLocation")]
    );
    assert_eq!(
        names(Activity::Sniff, &KNOWN),
        vec![(5, "WardenSetRoarTarget"), (6, "WardenSniffing")]
    );
    assert_eq!(names(Activity::Roar, &KNOWN), vec![(10, "WardenRoar")]);
    let fight = listed(Activity::Fight);
    assert_eq!(fight.len(), 6);
    assert_eq!(
        fight
            .iter()
            .map(|(priority, _)| *priority)
            .collect::<Vec<_>>(),
        vec![10, 11, 12, 13, 14, 15]
    );
    assert!(fight[0].1.contains("WardenDigCooldownSetter"));
    assert!(fight[1].1.contains("StopAttackingIfTargetInvalid"));
    assert!(fight[1].1.contains("WardenNotAngry"));
    assert!(fight[2].1.contains("WardenLookAtAttackTarget"));
    assert!(
        fight[3]
            .1
            .contains("SetWalkTargetFromAttackTargetIfTargetOutOfReach")
    );
    assert!(fight[4].1.contains("WardenSonicBoom"));
    assert!(fight[5].1.contains("MeleeAttack"));
}

#[test]
fn the_idle_gate_only_strolls_when_the_warden_is_not_sniffing() {
    let text = describe(&control(Activity::Idle, "Gate"));
    assert!(text.contains("IsSniffing"));
    assert!(text.contains("ValueAbsent"));
    assert!(text.contains("speed_modifier: 0.5"));
}

#[test]
fn the_vanilla_durations_drive_the_warden_behaviors() {
    assert!(describe(&control(Activity::Emerge, "WardenEmerging")).contains("min_duration: 134"));
    assert!(describe(&control(Activity::Dig, "WardenDigging")).contains("min_duration: 100"));
    assert!(describe(&control(Activity::Roar, "WardenRoar")).contains("min_duration: 84"));
    assert!(describe(&control(Activity::Sniff, "WardenSniffing")).contains("min_duration: 84"));
    assert!(describe(&control(Activity::Fight, "WardenSonicBoom")).contains("min_duration: 60"));
}

#[test]
fn the_warden_brain_registers_the_vanilla_sensors_and_memories() {
    let ai = warden_ai(1);
    let brain = ai.behavior_brain().expect("a brain");
    let kinds: Vec<_> = brain
        .sensors()
        .iter()
        .map(|slot| slot.sensor().sensor_type())
        .collect();
    assert_eq!(
        kinds,
        vec![
            rspigot_ai::SensorType::NearestPlayers,
            rspigot_ai::SensorType::WardenEntitySensor
        ]
    );
    for memory in rspigot_ai::WARDEN_MEMORIES {
        assert!(
            brain
                .memories()
                .check(memory, rspigot_ai::MemoryStatus::Registered)
        );
    }
}

#[test]
fn emerging_stands_the_warden_up_again_when_it_finishes() {
    let mut bench = bench(Field::flat());
    bench.set(MemoryModuleType::IsEmerging, MemoryValue::Unit);
    let mut emerging = control(Activity::Emerge, "WardenEmerging");
    assert!(bench.start(&mut emerging, NOW));
    assert_eq!(bench.body.pose, Pose::Emerging);
    assert_eq!(
        sounds(&bench.actions()),
        vec![("minecraft:entity.warden.emerge", 5.0)]
    );
    bench.tick(&mut emerging, NOW + 135);
    assert_eq!(bench.body.pose, Pose::Standing);
    assert_eq!(emerging.status(), Status::Stopped);
}

#[test]
fn emerging_needs_no_walk_target() {
    let mut bench = bench(Field::flat());
    bench.set(MemoryModuleType::IsEmerging, MemoryValue::Unit);
    bench.set(
        MemoryModuleType::WalkTarget,
        MemoryValue::WalkTarget(rspigot_ai::WalkTarget::position(Vec3::ZERO, 1.0, 0)),
    );
    let mut emerging = control(Activity::Emerge, "WardenEmerging");
    assert!(!bench.start(&mut emerging, NOW));
}

#[test]
fn digging_on_the_ground_starts_the_dig_pose_and_discards_the_warden_at_the_end() {
    let mut bench = bench(Field::flat());
    let mut digging = control(Activity::Dig, "WardenDigging");
    assert!(bench.start(&mut digging, NOW));
    assert_eq!(bench.body.pose, Pose::Digging);
    assert_eq!(
        sounds(&bench.actions()),
        vec![("minecraft:entity.warden.dig", 5.0)]
    );
    bench.tick(&mut digging, NOW + 101);
    assert_eq!(
        bench.actions(),
        vec![MobAction::Warden(rspigot_ai::WardenAction::Discard)]
    );
}

#[test]
fn digging_in_the_air_agitates_and_discards_at_once() {
    let mut bench = bench(Field::flat());
    bench.mob.on_ground = false;
    bench.mob.fluid.in_water = true;
    let mut digging = control(Activity::Dig, "WardenDigging");
    assert!(bench.start(&mut digging, NOW));
    let actions = bench.actions();
    assert_eq!(
        sounds(&actions),
        vec![("minecraft:entity.warden.agitated", 5.0)]
    );
    assert!(actions.contains(&MobAction::Warden(rspigot_ai::WardenAction::Discard)));
    assert_ne!(bench.body.pose, Pose::Digging);
}

#[test]
fn digging_needs_the_ground_or_a_liquid_and_no_target() {
    let mut bench = bench(Field::flat());
    bench.mob.on_ground = false;
    let mut digging = control(Activity::Dig, "WardenDigging");
    assert!(!bench.start(&mut digging, NOW));
    bench.mob.on_ground = true;
    bench.set(MemoryModuleType::AttackTarget, entity(9));
    assert!(!bench.start(&mut digging, NOW));
}

#[test]
fn the_roar_looks_at_its_target_raises_its_anger_and_ends_by_attacking_it() {
    let mut bench = bench(world_with(vec![player(5, Vec3::new(0.5, 64.0, 6.5))]));
    bench.set(MemoryModuleType::RoarTarget, entity(5));
    let mut roar = control(Activity::Roar, "WardenRoar");
    assert!(bench.start(&mut roar, NOW));
    assert_eq!(bench.body.pose, Pose::Roaring);
    assert_eq!(bench.controls.warden.anger.anger_of(EntityId(5)), 20);
    assert!(matches!(
        bench.get(MemoryModuleType::LookTarget),
        Some(MemoryValue::Tracker(_))
    ));
    assert!(bench.get(MemoryModuleType::RoarSoundDelay).is_some());
    for tick in 1..=30 {
        bench.state.memories.forget_outdated();
        bench.tick(&mut roar, NOW + tick);
    }
    assert!(bench.get(MemoryModuleType::RoarSoundCooldown).is_some());
    assert!(sounds(&bench.actions()).contains(&("minecraft:entity.warden.roar", 3.0)));
    bench.tick(&mut roar, NOW + 85);
    assert_eq!(bench.body.pose, Pose::Standing);
    assert_eq!(bench.get(MemoryModuleType::AttackTarget), Some(&entity(5)));
    assert!(bench.get(MemoryModuleType::RoarTarget).is_none());
    assert!(bench.get(MemoryModuleType::SonicBoomCooldown).is_some());
}

#[test]
fn sniffing_at_a_close_player_raises_the_anger_and_marks_a_disturbance() {
    let mut bench = bench(world_with(vec![player(5, Vec3::new(0.5, 64.0, 4.5))]));
    bench.set(MemoryModuleType::IsSniffing, MemoryValue::Unit);
    bench.set(MemoryModuleType::NearestAttackable, entity(5));
    let mut sniffing = control(Activity::Sniff, "WardenSniffing");
    assert!(bench.start(&mut sniffing, NOW));
    assert_eq!(
        sounds(&bench.actions()),
        vec![("minecraft:entity.warden.sniff", 5.0)]
    );
    bench.tick(&mut sniffing, NOW + 85);
    assert!(bench.get(MemoryModuleType::IsSniffing).is_none());
    assert_eq!(bench.controls.warden.anger.anger_of(EntityId(5)), 35);
    assert_eq!(
        bench.get(MemoryModuleType::DisturbanceLocation),
        Some(&MemoryValue::Block(BlockPos::new(0, 64, 4)))
    );
}

#[test]
fn sniffing_at_a_far_player_only_marks_the_disturbance() {
    let mut bench = bench(world_with(vec![player(5, Vec3::new(0.5, 64.0, 10.5))]));
    bench.set(MemoryModuleType::IsSniffing, MemoryValue::Unit);
    bench.set(MemoryModuleType::NearestAttackable, entity(5));
    let mut sniffing = control(Activity::Sniff, "WardenSniffing");
    assert!(bench.start(&mut sniffing, NOW));
    bench.tick(&mut sniffing, NOW + 85);
    assert_eq!(bench.controls.warden.anger.anger_of(EntityId(5)), 0);
    assert!(bench.get(MemoryModuleType::DisturbanceLocation).is_some());
}

#[test]
fn try_to_sniff_needs_an_attackable_and_no_disturbance_then_starts_a_sniff() {
    let mut bench = bench(Field::flat());
    let mut sniff = control(Activity::Idle, "WardenTryToSniff");
    assert!(!bench.start(&mut sniff, NOW));
    bench.set(MemoryModuleType::NearestAttackable, entity(5));
    assert!(bench.start(&mut sniff, NOW));
    assert!(bench.get(MemoryModuleType::IsSniffing).is_some());
    assert!(bench.get(MemoryModuleType::SniffCooldown).is_some());
    assert_eq!(bench.body.pose, Pose::Sniffing);
}

#[test]
fn set_roar_target_only_picks_an_angry_valid_suspect() {
    let target = player(5, Vec3::new(0.5, 64.0, 6.5));
    let mut bench = bench(world_with(vec![target]));
    let mut set = control(Activity::Idle, "WardenSetRoarTarget");
    assert!(!bench.start(&mut set, NOW));
    bench.controls.warden.anger.increase_anger(
        rspigot_domain::warden::Suspect {
            id: EntityId(5),
            uuid: Uuid::from_u128(905),
            player: true,
        },
        50,
    );
    assert!(!bench.start(&mut set, NOW));
    bench.controls.warden.anger.increase_anger(
        rspigot_domain::warden::Suspect {
            id: EntityId(5),
            uuid: Uuid::from_u128(905),
            player: true,
        },
        40,
    );
    assert!(bench.start(&mut set, NOW));
    assert_eq!(bench.get(MemoryModuleType::RoarTarget), Some(&entity(5)));
}

#[test]
fn the_look_target_follows_the_roar_target_then_the_disturbance() {
    let mut bench = bench(world_with(vec![player(5, Vec3::new(3.5, 64.0, 6.5))]));
    let mut look = control(Activity::Core, "WardenSetLookTarget");
    assert!(!bench.start(&mut look, NOW));
    bench.set(
        MemoryModuleType::DisturbanceLocation,
        MemoryValue::Block(BlockPos::new(9, 64, 9)),
    );
    assert!(bench.start(&mut look, NOW));
    let tracker = |bench: &Bench| match bench.get(MemoryModuleType::LookTarget) {
        Some(MemoryValue::Tracker(tracker)) => Some(format!("{tracker:?}")),
        _ => None,
    };
    assert!(tracker(&bench).is_some_and(|text| text.contains("x: 9")));
    bench.set(MemoryModuleType::RoarTarget, entity(5));
    assert!(bench.start(&mut look, NOW));
    assert!(tracker(&bench).is_some_and(|text| text.contains("x: 3")));
}

#[test]
fn a_boom_fires_after_the_charge_and_only_inside_fifteen_by_twenty_blocks() {
    let mut bench = bench(world_with(vec![player(5, Vec3::new(0.5, 64.0, 10.5))]));
    bench.set(MemoryModuleType::AttackTarget, entity(5));
    let mut boom = control(Activity::Fight, "WardenSonicBoom");
    assert!(bench.start(&mut boom, NOW));
    let started = bench.actions();
    assert!(started.contains(&MobAction::EntityEvent(62)));
    assert_eq!(
        sounds(&started),
        vec![("minecraft:entity.warden.sonic_charge", 3.0)]
    );
    for tick in 1..=40 {
        bench.state.memories.forget_outdated();
        bench.tick(&mut boom, NOW + tick);
        bench.world.time = NOW + tick;
    }
    let booms: Vec<_> = bench
        .actions()
        .into_iter()
        .filter_map(|action| match action {
            MobAction::Warden(rspigot_ai::WardenAction::SonicBoom { target, origin, .. }) => {
                Some((target, origin))
            }
            _ => None,
        })
        .collect();
    assert_eq!(booms.len(), 1);
    assert_eq!(booms[0].0, EntityId(5));
    assert_eq!(booms[0].1, Vec3::new(0.5, 65.6, 0.5));
}

#[test]
fn a_target_beyond_fifteen_blocks_cannot_be_boomed() {
    let mut bench = bench(world_with(vec![player(5, Vec3::new(0.5, 64.0, 18.5))]));
    bench.set(MemoryModuleType::AttackTarget, entity(5));
    let mut boom = control(Activity::Fight, "WardenSonicBoom");
    assert!(!bench.start(&mut boom, NOW));
}

#[test]
fn the_boom_cooldown_blocks_a_new_boom_and_follows_every_boom() {
    let mut bench = bench(world_with(vec![player(5, Vec3::new(0.5, 64.0, 10.5))]));
    bench.set(MemoryModuleType::AttackTarget, entity(5));
    let mut boom = control(Activity::Fight, "WardenSonicBoom");
    assert!(bench.start(&mut boom, NOW));
    bench.tick(&mut boom, NOW + 61);
    assert!(bench.get(MemoryModuleType::SonicBoomCooldown).is_some());
    assert_eq!(boom.status(), Status::Stopped);
    assert!(!bench.start(&mut boom, NOW + 62));
}

#[test]
fn the_dig_cooldown_setter_refreshes_the_cooldown_only_when_present() {
    let mut bench = bench(Field::flat());
    let mut setter = control(Activity::Fight, "WardenDigCooldownSetter");
    assert!(bench.start(&mut setter, NOW));
    assert!(bench.get(MemoryModuleType::DigCooldown).is_none());
    bench
        .state
        .memories
        .set_with_expiry(MemoryModuleType::DigCooldown, MemoryValue::Unit, 5);
    assert!(bench.start(&mut setter, NOW));
    bench.world.time = NOW + 100;
    bench.state.memories.forget_outdated();
    assert!(bench.get(MemoryModuleType::DigCooldown).is_some());
}

#[test]
fn creative_players_armor_stands_and_other_wardens_are_not_valid_targets() {
    let mut creative = player(5, Vec3::new(2.5, 64.0, 2.5));
    creative.creative = true;
    let stand = living(6, EntityKind::ArmorStand, Vec3::new(2.5, 64.0, 2.5));
    let warden = living(7, EntityKind::Warden, Vec3::new(2.5, 64.0, 2.5));
    let valid = player(8, Vec3::new(2.5, 64.0, 2.5));
    let mut dead = player(9, Vec3::new(2.5, 64.0, 2.5));
    dead.alive = false;
    let mut spectator = player(10, Vec3::new(2.5, 64.0, 2.5));
    spectator.spectator = true;
    let mut invulnerable = living(11, EntityKind::Zombie, Vec3::new(2.5, 64.0, 2.5));
    invulnerable.invulnerable = true;
    let bench = bench(Field::flat());
    let can =
        |target: &Living| rspigot_ai::warden_can_target_entity(&bench.mob, &bench.world, target);
    assert!(!can(&creative));
    assert!(!can(&stand));
    assert!(!can(&warden));
    assert!(!can(&dead));
    assert!(!can(&spectator));
    assert!(!can(&invulnerable));
    assert!(can(&valid));
    assert!(can(&living(
        12,
        EntityKind::Zombie,
        Vec3::new(2.5, 64.0, 2.5)
    )));
}

#[test]
fn increasing_the_anger_of_a_player_while_targeting_a_mob_drops_that_target() {
    let mut bench = bench(world_with(vec![player(5, Vec3::new(0.5, 64.0, 6.5))]));
    bench
        .world
        .others
        .push(living(20, EntityKind::Zombie, Vec3::new(1.5, 64.0, 1.5)));
    bench.set(MemoryModuleType::AttackTarget, entity(20));
    bench.with(|cx| rspigot_ai::warden_increase(cx, EntityId(5), 90, false));
    assert!(bench.get(MemoryModuleType::AttackTarget).is_none());
    assert_eq!(bench.controls.warden.anger.anger_of(EntityId(5)), 90);
}

#[test]
fn a_hit_makes_the_warden_angry_and_a_close_attacker_its_target() {
    let mut bench = bench(world_with(vec![player(5, Vec3::new(1.5, 64.0, 1.5))]));
    bench.with(|cx| rspigot_ai::warden_on_hurt(cx, EntityId(5), true));
    assert_eq!(bench.controls.warden.anger.anger_of(EntityId(5)), 100);
    assert_eq!(bench.get(MemoryModuleType::AttackTarget), Some(&entity(5)));
    assert!(bench.get(MemoryModuleType::SonicBoomCooldown).is_some());
}

#[test]
fn a_far_indirect_attacker_is_not_targeted_but_still_angers_the_warden() {
    let mut bench = bench(world_with(vec![player(5, Vec3::new(0.5, 64.0, 12.5))]));
    bench.with(|cx| rspigot_ai::warden_on_hurt(cx, EntityId(5), false));
    assert_eq!(bench.controls.warden.anger.anger_of(EntityId(5)), 100);
    assert!(bench.get(MemoryModuleType::AttackTarget).is_none());
}

#[test]
fn a_burrowing_warden_ignores_hits() {
    let mut bench = bench(world_with(vec![player(5, Vec3::new(1.5, 64.0, 1.5))]));
    bench.body.pose = Pose::Digging;
    bench.with(|cx| rspigot_ai::warden_on_hurt(cx, EntityId(5), true));
    assert_eq!(bench.controls.warden.anger.anger_of(EntityId(5)), 0);
}

#[test]
fn touching_a_warden_angers_it_once_per_second() {
    let mut bench = bench(world_with(vec![player(5, Vec3::new(1.0, 64.0, 0.5))]));
    bench.with(|cx| rspigot_ai::warden_touch(cx, EntityId(5)));
    assert_eq!(bench.controls.warden.anger.anger_of(EntityId(5)), 35);
    assert!(bench.get(MemoryModuleType::TouchCooldown).is_some());
    bench.with(|cx| rspigot_ai::warden_touch(cx, EntityId(5)));
    assert_eq!(bench.controls.warden.anger.anger_of(EntityId(5)), 35);
    assert_eq!(
        bench.get(MemoryModuleType::DisturbanceLocation),
        Some(&MemoryValue::Block(BlockPos::new(1, 64, 0)))
    );
}

#[test]
fn a_vibration_from_a_player_angers_the_warden_and_marks_the_spot() {
    let mut bench = bench(world_with(vec![player(5, Vec3::new(4.5, 64.0, 0.5))]));
    bench.with(|cx| {
        rspigot_ai::warden_on_vibration(cx, BlockPos::new(4, 64, 0), Some(EntityId(5)), None);
    });
    assert_eq!(bench.controls.warden.anger.anger_of(EntityId(5)), 35);
    assert!(bench.get(MemoryModuleType::VibrationCooldown).is_some());
    assert!(bench.get(MemoryModuleType::DisturbanceLocation).is_some());
    let actions = bench.actions();
    assert!(actions.contains(&MobAction::EntityEvent(61)));
    assert!(sounds(&actions).contains(&("minecraft:entity.warden.tendril_clicks", 5.0)));
}

#[test]
fn a_projectile_vibration_gives_ten_points_then_the_full_amount() {
    let mut bench = bench(world_with(vec![player(5, Vec3::new(4.5, 64.0, 0.5))]));
    bench.with(|cx| {
        rspigot_ai::warden_on_vibration(cx, BlockPos::new(8, 64, 0), None, Some(EntityId(5)));
    });
    assert_eq!(bench.controls.warden.anger.anger_of(EntityId(5)), 10);
    assert!(bench.get(MemoryModuleType::RecentProjectile).is_some());
    bench
        .state
        .memories
        .erase(MemoryModuleType::VibrationCooldown);
    bench.with(|cx| {
        rspigot_ai::warden_on_vibration(cx, BlockPos::new(8, 64, 0), None, Some(EntityId(5)));
    });
    assert_eq!(bench.controls.warden.anger.anger_of(EntityId(5)), 45);
    assert_eq!(
        bench.get(MemoryModuleType::DisturbanceLocation),
        Some(&MemoryValue::Block(BlockPos::new(4, 64, 0)))
    );
}

#[test]
fn a_warden_in_cooldown_or_burrowed_cannot_receive_a_vibration() {
    let ai = warden_ai(1);
    let mob = bench(Field::flat()).mob;
    let world = Field::flat();
    let pos = BlockPos::new(3, 64, 3);
    assert!(ai.warden_can_receive_vibration(&mob, &world, pos, None));
    let mut busy = warden_ai(1);
    busy.behavior_brain_mut()
        .unwrap()
        .memories_mut()
        .set(MemoryModuleType::VibrationCooldown, MemoryValue::Unit);
    assert!(!busy.warden_can_receive_vibration(&mob, &world, pos, None));
    let mut digging = mob;
    digging.pose = Pose::Digging;
    assert!(!ai.warden_can_receive_vibration(&digging, &world, pos, None));
    let creative = {
        let mut p = player(5, Vec3::new(1.5, 64.0, 1.5));
        p.creative = true;
        p
    };
    assert!(!ai.warden_can_receive_vibration(&mob, &world, pos, Some(&creative)));
}

#[test]
fn the_entity_sensor_prefers_the_closest_player_over_a_closer_mob() {
    let near_player = player(5, Vec3::new(0.5, 64.0, 8.5));
    let mut world = world_with(vec![near_player]);
    world
        .others
        .push(living(20, EntityKind::Zombie, Vec3::new(1.5, 64.0, 2.5)));
    let mut bench = bench(world);
    bench.tick_sensor(SensorKind::WardenEntity(rspigot_ai::WardenEntitySensor));
    assert_eq!(
        bench.get(MemoryModuleType::NearestAttackable),
        Some(&entity(5))
    );
    bench.world.players.clear();
    bench.tick_sensor(SensorKind::WardenEntity(rspigot_ai::WardenEntitySensor));
    assert_eq!(
        bench.get(MemoryModuleType::NearestAttackable),
        Some(&entity(20))
    );
}

#[test]
fn the_activity_is_chosen_in_the_vanilla_order() {
    let mut ai = warden_ai(1);
    ai.warden_spawned(false);
    let brain = ai.behavior_brain_mut().unwrap();
    rspigot_ai::warden_update_activity(brain);
    assert!(brain.state().is_active(Activity::Idle));
    brain.memories_mut().set(
        MemoryModuleType::DisturbanceLocation,
        MemoryValue::Block(BlockPos::new(1, 64, 1)),
    );
    rspigot_ai::warden_update_activity(brain);
    assert!(brain.state().is_active(Activity::Investigate));
    brain
        .memories_mut()
        .set(MemoryModuleType::IsSniffing, MemoryValue::Unit);
    rspigot_ai::warden_update_activity(brain);
    assert!(brain.state().is_active(Activity::Investigate));
    brain
        .memories_mut()
        .erase(MemoryModuleType::DisturbanceLocation);
    rspigot_ai::warden_update_activity(brain);
    assert!(brain.state().is_active(Activity::Sniff));
    brain.memories_mut().set(
        MemoryModuleType::AttackTarget,
        MemoryValue::Entity(EntityId(5)),
    );
    rspigot_ai::warden_update_activity(brain);
    assert!(brain.state().is_active(Activity::Fight));
    brain.memories_mut().set(
        MemoryModuleType::RoarTarget,
        MemoryValue::Entity(EntityId(5)),
    );
    rspigot_ai::warden_update_activity(brain);
    assert!(brain.state().is_active(Activity::Roar));
    brain
        .memories_mut()
        .set(MemoryModuleType::IsEmerging, MemoryValue::Unit);
    rspigot_ai::warden_update_activity(brain);
    assert!(brain.state().is_active(Activity::Emerge));
}

#[test]
fn the_dig_activity_needs_no_roar_target_and_an_expired_dig_cooldown() {
    let mut ai = warden_ai(1);
    let brain = ai.behavior_brain_mut().unwrap();
    brain
        .memories_mut()
        .set(MemoryModuleType::DigCooldown, MemoryValue::Unit);
    rspigot_ai::warden_update_activity(brain);
    assert!(!brain.state().is_active(Activity::Dig));
    brain.memories_mut().erase(MemoryModuleType::DigCooldown);
    rspigot_ai::warden_update_activity(brain);
    assert!(brain.state().is_active(Activity::Dig));
}

#[test]
fn a_fresh_warden_digs_in_after_a_minute_only_when_not_spawned_by_a_shrieker() {
    let mut ai = warden_ai(1);
    ai.warden_spawned(true);
    let brain = ai.behavior_brain().unwrap();
    assert!(brain.memories().has_value(MemoryModuleType::DigCooldown));
    assert!(brain.memories().has_value(MemoryModuleType::IsEmerging));
    let mut plain = warden_ai(1);
    plain.warden_spawned(false);
    let brain = plain.behavior_brain().unwrap();
    assert!(brain.memories().has_value(MemoryModuleType::DigCooldown));
    assert!(!brain.memories().has_value(MemoryModuleType::IsEmerging));
}

#[test]
fn the_anger_and_the_listener_round_trip_through_nbt() {
    let mut ai = warden_ai(1);
    ai.warden_state_mut().anger.increase_anger(
        rspigot_domain::warden::Suspect {
            id: EntityId(5),
            uuid: Uuid::from_u128(77),
            player: true,
        },
        60,
    );
    ai.warden_state_mut().vibration.travel_time_in_ticks = 7;
    let (anger, listener) = ai.warden_save();
    let mut loaded = warden_ai(2);
    loaded.warden_load(Some(&anger), Some(&listener));
    assert_eq!(
        loaded.warden_state().anger.pending(),
        &[(Uuid::from_u128(77), 60)]
    );
    assert_eq!(loaded.warden_state().vibration.travel_time_in_ticks, 7);
}

#[test]
fn the_persisted_warden_memories_round_trip() {
    let mut ai = warden_ai(1);
    ai.behavior_brain_mut()
        .unwrap()
        .memories_mut()
        .set_with_expiry(MemoryModuleType::IsEmerging, MemoryValue::Unit, 50);
    let saved = ai.save_behavior_brain().expect("a brain");
    let mut loaded = warden_ai(1);
    loaded.load_behavior_brain(&saved);
    assert!(
        loaded
            .behavior_brain()
            .unwrap()
            .memories()
            .has_value(MemoryModuleType::IsEmerging)
    );
}

#[test]
fn a_warden_flees_nothing_and_paths_with_the_vanilla_maluses() {
    let ai = warden_ai(1);
    assert!(ai.can_float());
    let profile = &ai.brain().profile;
    assert_eq!(profile.malus(rspigot_ai::PathType::Damaging), 8.0);
    assert_eq!(profile.malus(rspigot_ai::PathType::PowderSnow), 8.0);
    assert_eq!(profile.malus(rspigot_ai::PathType::Lava), 8.0);
    assert_eq!(profile.malus(rspigot_ai::PathType::Fire), 0.0);
    assert_eq!(profile.malus(rspigot_ai::PathType::FireInNeighbor), 0.0);
    assert_eq!(profile.malus(rspigot_ai::PathType::UnpassableRail), 0.0);
    assert!(profile.horizontal_step_cost);
}

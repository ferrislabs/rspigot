use std::collections::HashMap;

use rspigot_ai::{
    Activity, AiContext, AiWorld, BehaviorControl, BehaviorCx, BrainState, BreezeAction, Facts,
    GoalSettings, Living, MemoryModuleType, MemoryValue, MobAction, MobAi, MobBody, MobControl,
    MobView, NO_EXPIRY, SensorKind, Status, TargetKind,
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

const SHOOTER: Vec3 = Vec3::new(0.5, 64.0, 0.5);

fn entity(id: i32) -> MemoryValue {
    MemoryValue::Entity(EntityId(id))
}

fn named(text: &str, names: &[&'static str]) -> &'static str {
    let mut found: Vec<(usize, &'static str)> = names
        .iter()
        .filter_map(|name| text.find(&format!("{name}(")).map(|at| (at, *name)))
        .collect();
    found.sort_by_key(|(at, _)| *at);
    found.first().map_or("?", |(_, name)| *name)
}

const BREEZE_KNOWN: [&str; 15] = [
    "Swim",
    "LookAtTargetSink",
    "StartAttacking",
    "BreezeSlideToTarget",
    "BreezeShootWhenStuck",
    "BreezeShoot",
    "BreezeLongJump",
    "BreezeSlide",
    "StopAttackingIfTargetInvalid",
    "Gate",
    "DoNothing",
    "RandomStroll",
    "OneShot",
    "Timed",
    "Condition",
];

fn breeze_ai(seed: i64) -> MobAi {
    MobAi::new(EntityKind::Breeze, seed, GoalSettings::default())
}

fn breeze_behaviors(activity: Activity) -> Vec<(i32, String)> {
    let ai = breeze_ai(1);
    let brain = ai.behavior_brain().expect("a brain");
    brain
        .activity_behaviors(activity)
        .into_iter()
        .map(|(priority, control)| (priority, describe(control)))
        .collect()
}

fn breeze_control(activity: Activity, name: &str) -> MobControl {
    let ai = breeze_ai(1);
    ai.behavior_brain()
        .unwrap()
        .activity_behaviors(activity)
        .into_iter()
        .map(|(_, control)| control.clone())
        .find(|control| describe(control).contains(&format!("{name}(")))
        .expect("the behavior")
}

fn leading(list: &[(i32, String)]) -> Vec<(i32, &'static str)> {
    const WRAPPERS: [&str; 3] = ["OneShot", "Timed", "Condition"];
    let names: Vec<&'static str> = BREEZE_KNOWN
        .into_iter()
        .filter(|name| !WRAPPERS.contains(name))
        .collect();
    list.iter()
        .map(|(priority, text)| (*priority, named(text, &names)))
        .collect()
}

fn breeze_bench(world: Field, seed: i64) -> Bench {
    Bench::new(EntityKind::Breeze, SHOOTER, world, seed)
}

fn target_at(id: i32, position: Vec3) -> Living {
    living(id, EntityKind::Player, position)
}

fn world_with(players: Vec<Living>) -> Field {
    let mut world = Field::flat();
    world.players = players;
    world
}

#[test]
fn the_breeze_activities_follow_the_vanilla_priorities() {
    assert_eq!(
        leading(&breeze_behaviors(Activity::Core)),
        vec![(0, "Swim"), (1, "LookAtTargetSink")]
    );
    assert_eq!(
        leading(&breeze_behaviors(Activity::Idle)),
        vec![
            (0, "StartAttacking"),
            (1, "StartAttacking"),
            (2, "BreezeSlideToTarget"),
            (3, "Gate"),
        ]
    );
    assert_eq!(
        leading(&breeze_behaviors(Activity::Fight)),
        vec![
            (0, "StopAttackingIfTargetInvalid"),
            (1, "BreezeShoot"),
            (2, "BreezeLongJump"),
            (3, "BreezeShootWhenStuck"),
            (4, "BreezeSlide"),
        ]
    );
}

#[test]
fn the_breeze_behavior_parameters_follow_the_vanilla_constructors() {
    let core = breeze_behaviors(Activity::Core);
    assert!(core[0].1.contains("chance: 0.8"));
    assert!(core[1].1.contains("min_duration: 45"));
    assert!(core[1].1.contains("max_duration: 90"));
    let idle = breeze_behaviors(Activity::Idle);
    assert!(idle[2].1.contains("min_duration: 20"));
    assert!(idle[2].1.contains("max_duration: 40"));
    assert!(idle[3].1.contains("min_duration: 20"));
    assert!(idle[3].1.contains("max_duration: 100"));
    assert!(idle[3].1.contains("0.6"));
    let fight = breeze_behaviors(Activity::Fight);
    assert!(fight[1].1.contains("min_duration: 20"));
    assert!(fight[1].1.contains("max_duration: 20"));
    assert!(fight[2].1.contains("min_duration: 200"));
    assert!(fight[2].1.contains("max_duration: 200"));
    assert!(fight[3].1.contains("min_duration: 60"));
    assert!(fight[4].1.contains("min_duration: 60"));
}

#[test]
fn the_breeze_fight_activity_needs_an_attack_target_and_no_walk_target() {
    let ai = breeze_ai(1);
    let brain = ai.behavior_brain().unwrap();
    assert!(brain.state().is_active(Activity::Fight));
    assert!(brain.state().is_active(Activity::Core));
}

fn active(ai: &MobAi) -> Option<Activity> {
    ai.behavior_brain()
        .and_then(|brain| brain.state().active_non_core_activity())
}

fn think(ai: &mut MobAi, mob: &MobView, world: &Field) {
    let mut body = MobBody::of(mob);
    ai.think(mob, world, &mut body);
}

#[test]
fn a_breeze_fights_with_a_target_and_no_walk_target_and_idles_otherwise() {
    let mob = view(EntityKind::Breeze, SHOOTER);
    let world = world_with(vec![target_at(7, Vec3::new(5.5, 64.0, 0.5))]);
    let mut idle = breeze_ai(2);
    think(&mut idle, &mob, &world);
    assert_eq!(active(&idle), Some(Activity::Idle));
    let mut ai = breeze_ai(2);
    ai.set_target(Some(EntityId(7)));
    think(&mut ai, &mob, &world);
    assert_eq!(active(&ai), Some(Activity::Fight));
    ai.behavior_brain_mut().unwrap().memories_mut().set(
        MemoryModuleType::WalkTarget,
        MemoryValue::WalkTarget(rspigot_ai::WalkTarget::position(
            Vec3::new(3.5, 64.0, 3.5),
            0.6,
            1,
        )),
    );
    think(&mut ai, &mob, &world);
    assert_eq!(active(&ai), Some(Activity::Idle));
}

fn attackable_sensor() -> SensorKind {
    SensorKind::BreezeAttackEntity(rspigot_ai::BreezeAttackEntitySensor)
}

#[test]
fn the_breeze_sensor_picks_the_nearest_attackable_player_or_golem() {
    let mut world = world_with(vec![target_at(7, Vec3::new(9.5, 64.0, 0.5))]);
    world
        .others
        .push(living(8, EntityKind::Cow, Vec3::new(3.5, 64.0, 0.5)));
    world
        .others
        .push(living(9, EntityKind::IronGolem, Vec3::new(5.5, 64.0, 0.5)));
    let mut bench = breeze_bench(world, 1);
    bench.tick_sensor(attackable_sensor());
    assert_eq!(
        bench.get(MemoryModuleType::NearestAttackable),
        Some(&entity(9))
    );
    assert!(bench.get(MemoryModuleType::NearestLivingEntities).is_some());
    assert!(
        bench
            .get(MemoryModuleType::NearestVisibleLivingEntities)
            .is_some()
    );
}

#[test]
fn the_breeze_sensor_skips_creative_players_and_spectators_and_erases_when_alone() {
    let mut creative = target_at(7, Vec3::new(4.5, 64.0, 0.5));
    creative.creative = true;
    let mut spectator = target_at(8, Vec3::new(5.5, 64.0, 0.5));
    spectator.spectator = true;
    let mut bench = breeze_bench(world_with(vec![creative, spectator]), 1);
    bench.set(MemoryModuleType::NearestAttackable, entity(99));
    bench.tick_sensor(attackable_sensor());
    assert!(bench.get(MemoryModuleType::NearestAttackable).is_none());
    bench
        .world
        .players
        .push(target_at(10, Vec3::new(6.5, 64.0, 0.5)));
    bench.tick_sensor(attackable_sensor());
    assert_eq!(
        bench.get(MemoryModuleType::NearestAttackable),
        Some(&entity(10))
    );
}

#[test]
fn the_breeze_sensor_ignores_players_beyond_its_follow_range() {
    let mut bench = breeze_bench(
        world_with(vec![target_at(7, Vec3::new(40.5, 64.0, 0.5))]),
        1,
    );
    bench.tick_sensor(attackable_sensor());
    assert!(bench.get(MemoryModuleType::NearestAttackable).is_none());
}

#[test]
fn a_breeze_with_an_attack_target_attacks_nothing_else() {
    let mut bench = breeze_bench(
        world_with(vec![
            target_at(7, Vec3::new(4.5, 64.0, 0.5)),
            target_at(8, Vec3::new(6.5, 64.0, 0.5)),
        ]),
        1,
    );
    bench.mob.species.breeze.attack_target = Some(EntityId(8));
    let other = bench.world.players[0];
    let current = bench.world.players[1];
    assert!(!rspigot_ai::mob_can_attack(
        &bench.mob,
        &bench.world,
        &other
    ));
    assert!(rspigot_ai::mob_can_attack(
        &bench.mob,
        &bench.world,
        &current
    ));
    bench.mob.species.breeze.attack_target = None;
    assert!(rspigot_ai::mob_can_attack(&bench.mob, &bench.world, &other));
    let cow = living(9, EntityKind::Cow, Vec3::new(3.5, 64.0, 0.5));
    assert!(!rspigot_ai::mob_can_attack(&bench.mob, &bench.world, &cow));
}

#[test]
fn the_idle_breeze_starts_attacking_the_nearest_attackable() {
    let mut bench = breeze_bench(world_with(vec![target_at(7, Vec3::new(5.5, 64.0, 0.5))]), 1);
    let mut control = {
        let ai = breeze_ai(1);
        let list = ai
            .behavior_brain()
            .unwrap()
            .activity_behaviors(Activity::Idle);
        list[0].1.clone()
    };
    assert!(!bench.start(&mut control, NOW));
    bench.set(MemoryModuleType::NearestAttackable, entity(7));
    assert!(bench.start(&mut control, NOW));
    assert_eq!(bench.get(MemoryModuleType::AttackTarget), Some(&entity(7)));
}

#[test]
fn the_second_idle_attack_starter_follows_the_entity_that_hurt_the_breeze() {
    let mut bench = breeze_bench(world_with(vec![target_at(7, Vec3::new(5.5, 64.0, 0.5))]), 1);
    let mut control = {
        let ai = breeze_ai(1);
        let list = ai
            .behavior_brain()
            .unwrap()
            .activity_behaviors(Activity::Idle);
        list[1].1.clone()
    };
    assert!(!bench.start(&mut control, NOW));
    bench.set(
        MemoryModuleType::HurtBy,
        MemoryValue::HurtBy(rspigot_ai::LastDamage {
            kind: rspigot_domain::damage::DamageType::Arrow,
            entity: None,
        }),
    );
    assert!(!bench.start(&mut control, NOW));
    bench.set(
        MemoryModuleType::HurtBy,
        MemoryValue::HurtBy(rspigot_ai::LastDamage {
            kind: rspigot_domain::damage::DamageType::Arrow,
            entity: Some(EntityId(7)),
        }),
    );
    assert!(bench.start(&mut control, NOW));
    assert_eq!(bench.get(MemoryModuleType::AttackTarget), Some(&entity(7)));
}

#[test]
fn a_breeze_keeps_a_target_it_could_attack_within_the_last_hundred_ticks() {
    let player = target_at(7, Vec3::new(5.5, 64.0, 0.5));
    let mut bench = breeze_bench(world_with(vec![player]), 1);
    bench.set(MemoryModuleType::AttackTarget, entity(7));
    bench.mob.species.breeze.attack_target = Some(EntityId(7));
    let mut control = breeze_control(Activity::Fight, "StopAttackingIfTargetInvalid");
    assert!(bench.start(&mut control, NOW));
    bench.tick(&mut control, NOW);
    assert_eq!(bench.get(MemoryModuleType::AttackTarget), Some(&entity(7)));
    let far = Vec3::new(40.5, 64.0, 0.5);
    bench.world.players[0].position = far;
    bench.world.players[0].bounds = Aabb::of_size(far, 0.6, 1.8);
    for tick in 0..100 {
        assert!(bench.start(&mut control, NOW + tick));
        bench.tick(&mut control, NOW + tick);
        assert_eq!(
            bench.get(MemoryModuleType::AttackTarget),
            Some(&entity(7)),
            "tick {tick}"
        );
    }
    assert!(bench.start(&mut control, NOW + 100));
    assert!(bench.get(MemoryModuleType::AttackTarget).is_none());
}

#[test]
fn a_breeze_with_a_never_attackable_target_drops_it_at_once() {
    let player = target_at(7, Vec3::new(40.5, 64.0, 0.5));
    let mut bench = breeze_bench(world_with(vec![player]), 1);
    bench.set(MemoryModuleType::AttackTarget, entity(7));
    bench.mob.species.breeze.attack_target = Some(EntityId(7));
    let mut control = breeze_control(Activity::Fight, "StopAttackingIfTargetInvalid");
    assert!(bench.start(&mut control, NOW));
    assert!(bench.get(MemoryModuleType::AttackTarget).is_none());
}

fn shooter(distance: f64) -> Bench {
    let mut bench = breeze_bench(
        world_with(vec![target_at(7, Vec3::new(0.5 + distance, 64.0, 0.5))]),
        1,
    );
    bench.set(MemoryModuleType::AttackTarget, entity(7));
    bench.set(MemoryModuleType::BreezeShoot, MemoryValue::Unit);
    bench
}

#[test]
fn a_breeze_shoot_needs_the_standing_pose_and_a_target_within_sixteen_blocks() {
    let mut control = breeze_control(Activity::Fight, "BreezeShoot");
    let mut far = shooter(16.5);
    assert!(!far.start(&mut control, NOW));
    assert!(far.get(MemoryModuleType::BreezeShoot).is_none());
    let mut crouched = shooter(8.0);
    crouched.body.pose = Pose::Sliding;
    assert!(!crouched.start(&mut control, NOW));
    assert!(crouched.get(MemoryModuleType::BreezeShoot).is_some());
    let mut near = shooter(8.0);
    assert!(near.start(&mut control, NOW));
}

#[test]
fn a_breeze_shoot_waits_for_the_other_breeze_memories_to_clear() {
    for blocker in [
        MemoryModuleType::BreezeShootCooldown,
        MemoryModuleType::BreezeShootCharging,
        MemoryModuleType::BreezeShootRecovering,
        MemoryModuleType::WalkTarget,
        MemoryModuleType::BreezeJumpTarget,
    ] {
        let mut control = breeze_control(Activity::Fight, "BreezeShoot");
        let mut bench = shooter(8.0);
        bench.set(blocker, MemoryValue::Unit);
        assert!(!bench.start(&mut control, NOW), "{blocker:?}");
    }
    let mut control = breeze_control(Activity::Fight, "BreezeShoot");
    let mut idle = shooter(8.0);
    idle.state.memories.erase(MemoryModuleType::BreezeShoot);
    assert!(!idle.start(&mut control, NOW));
}

#[test]
fn a_breeze_inhales_for_fifteen_ticks_then_fires_once_and_recovers_for_four() {
    let mut bench = shooter(8.0);
    let mut control = breeze_control(Activity::Fight, "BreezeShoot");
    assert!(bench.start(&mut control, NOW));
    assert_eq!(bench.body.pose, Pose::Shooting);
    assert_eq!(
        bench
            .state
            .memories
            .time_until_expiry(MemoryModuleType::BreezeShootCharging),
        Some(15)
    );
    assert!(bench.actions().contains(&MobAction::Sound {
        sound: "minecraft:entity.breeze.inhale",
        volume: 1.0,
        pitch: 1.0,
    }));
    bench.tick(&mut control, NOW + 1);
    assert!(
        !bench
            .actions()
            .iter()
            .any(|action| matches!(action, MobAction::Breeze(_)))
    );
    assert!(bench.body.yaw.abs() > 1.0);
    bench
        .state
        .memories
        .erase(MemoryModuleType::BreezeShootCharging);
    bench.tick(&mut control, NOW + 16);
    let actions = bench.actions();
    let eyes_to_target =
        64.0 + 1.8 * 0.3 - (64.0 + f64::from(1.77f32 / 2.0) + 0.3f64 as f32 as f64);
    let direction = actions
        .iter()
        .find_map(|action| match action {
            MobAction::Breeze(BreezeAction::Shoot { direction }) => Some(*direction),
            _ => None,
        })
        .expect("a shot");
    assert_eq!(direction.x, 8.0);
    assert_eq!(direction.z, 0.0);
    assert!(
        (direction.y - eyes_to_target).abs() < 0.01,
        "{}",
        direction.y
    );
    assert!(actions.contains(&MobAction::Sound {
        sound: "minecraft:entity.breeze.shoot",
        volume: 1.5,
        pitch: 1.0,
    }));
    assert_eq!(
        bench
            .state
            .memories
            .time_until_expiry(MemoryModuleType::BreezeShootRecovering),
        Some(4)
    );
    bench.tick(&mut control, NOW + 17);
    assert!(
        !bench
            .actions()
            .iter()
            .any(|action| matches!(action, MobAction::Breeze(_)))
    );
}

#[test]
fn a_breeze_shoot_ends_after_twenty_ticks_with_a_cooldown_and_the_standing_pose() {
    let mut bench = shooter(8.0);
    let mut control = breeze_control(Activity::Fight, "BreezeShoot");
    assert!(bench.start(&mut control, NOW));
    bench.tick(&mut control, NOW + 20);
    assert_eq!(control.status(), Status::Running);
    bench.tick(&mut control, NOW + 21);
    assert_eq!(control.status(), Status::Stopped);
    assert_eq!(bench.body.pose, Pose::Standing);
    assert!(bench.get(MemoryModuleType::BreezeShoot).is_none());
    assert_eq!(
        bench
            .state
            .memories
            .time_until_expiry(MemoryModuleType::BreezeShootCooldown),
        Some(10)
    );
}

#[test]
fn a_breeze_shoot_stops_when_the_target_or_the_shoot_memory_is_lost() {
    let mut bench = shooter(8.0);
    let mut control = breeze_control(Activity::Fight, "BreezeShoot");
    assert!(bench.start(&mut control, NOW));
    bench.state.memories.erase(MemoryModuleType::AttackTarget);
    bench.tick(&mut control, NOW + 1);
    assert_eq!(control.status(), Status::Stopped);
}

#[test]
fn a_breeze_shoots_at_a_riding_target_higher_than_a_walking_one() {
    let mut walking = shooter(8.0);
    let mut riding = shooter(8.0);
    riding.world.players[0].facts.passenger = true;
    let mut control = breeze_control(Activity::Fight, "BreezeShoot");
    let mut aim = |bench: &mut Bench| {
        assert!(bench.start(&mut control, NOW));
        bench
            .state
            .memories
            .erase(MemoryModuleType::BreezeShootCharging);
        bench.tick(&mut control, NOW + 1);
        bench
            .actions()
            .iter()
            .find_map(|action| match action {
                MobAction::Breeze(BreezeAction::Shoot { direction }) => Some(direction.y),
                _ => None,
            })
            .expect("a shot")
    };
    let low = aim(&mut walking);
    let mut control = breeze_control(Activity::Fight, "BreezeShoot");
    let _ = &mut control;
    let high = {
        let mut control = breeze_control(Activity::Fight, "BreezeShoot");
        assert!(riding.start(&mut control, NOW));
        riding
            .state
            .memories
            .erase(MemoryModuleType::BreezeShootCharging);
        riding.tick(&mut control, NOW + 1);
        riding
            .actions()
            .iter()
            .find_map(|action| match action {
                MobAction::Breeze(BreezeAction::Shoot { direction }) => Some(direction.y),
                _ => None,
            })
            .expect("a shot")
    };
    assert!(high > low);
    assert!((high - low - 1.8 * 0.5).abs() < 0.01, "{high} {low}");
}

#[test]
fn a_breeze_in_water_or_levitating_shoots_when_stuck_for_sixty_ticks() {
    let mut control = breeze_control(Activity::Fight, "BreezeShootWhenStuck");
    let mut dry = shooter(8.0);
    dry.state.memories.erase(MemoryModuleType::BreezeShoot);
    assert!(!dry.start(&mut control, NOW));
    let mut wet = shooter(8.0);
    wet.state.memories.erase(MemoryModuleType::BreezeShoot);
    wet.mob.fluid.in_water = true;
    assert!(wet.start(&mut control, NOW));
    assert_eq!(
        wet.state
            .memories
            .time_until_expiry(MemoryModuleType::BreezeShoot),
        Some(60)
    );
    wet.tick(&mut control, NOW + 1);
    assert_eq!(control.status(), Status::Stopped);
    let mut floating = shooter(8.0);
    floating.state.memories.erase(MemoryModuleType::BreezeShoot);
    floating.mob.levitating = true;
    assert!(floating.start(&mut control, NOW));
    let mut blocked = shooter(8.0);
    blocked.mob.fluid.in_water = true;
    assert!(!blocked.start(&mut control, NOW));
}

fn slider(distance: f64) -> Bench {
    let mut bench = breeze_bench(
        world_with(vec![target_at(7, Vec3::new(0.5 + distance, 64.0, 0.5))]),
        4,
    );
    bench.set(MemoryModuleType::AttackTarget, entity(7));
    bench
}

fn walk_destination(bench: &Bench) -> (Vec3, f32, i32) {
    match bench.get(MemoryModuleType::WalkTarget) {
        Some(MemoryValue::WalkTarget(walk)) => (
            walk.target
                .current_position(&bench.world)
                .expect("a position"),
            walk.speed_modifier,
            walk.close_enough_dist,
        ),
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_far_target_makes_a_breeze_slide_to_a_point_around_it_at_slow_speed() {
    for seed in 0..20 {
        let mut bench = slider(10.0);
        bench.controls = rspigot_ai::Brain::new(EntityKind::Breeze, seed);
        let mut control = breeze_control(Activity::Fight, "BreezeSlide");
        assert!(bench.start(&mut control, NOW));
        let (destination, speed, close) = walk_destination(&bench);
        assert_eq!((speed, close), (0.6, 1));
        let target = Vec3::new(10.5, 64.0, 0.5);
        let distance = target.distance_squared(destination).sqrt();
        assert!(distance > 2.0 && distance < 12.5, "{seed}: {distance}");
    }
}

#[test]
fn a_close_target_makes_a_breeze_back_away_when_it_can() {
    let mut backed = 0;
    for seed in 0..30 {
        let mut bench = slider(2.0);
        bench.controls = rspigot_ai::Brain::new(EntityKind::Breeze, seed);
        let mut control = breeze_control(Activity::Fight, "BreezeSlide");
        assert!(bench.start(&mut control, NOW));
        let (destination, _, _) = walk_destination(&bench);
        let target = Vec3::new(2.5, 64.0, 0.5);
        if target.distance_squared(destination) > target.distance_squared(SHOOTER) {
            backed += 1;
        }
    }
    assert!(backed > 10, "{backed}");
}

#[test]
fn a_breeze_only_slides_standing_on_dry_ground_without_a_walk_target_or_jump_cooldown() {
    let mut control = breeze_control(Activity::Fight, "BreezeSlide");
    let mut airborne = slider(10.0);
    airborne.mob.on_ground = false;
    assert!(!airborne.start(&mut control, NOW));
    let mut wet = slider(10.0);
    wet.mob.fluid.in_water = true;
    assert!(!wet.start(&mut control, NOW));
    let mut shooting = slider(10.0);
    shooting.body.pose = Pose::Shooting;
    assert!(!shooting.start(&mut control, NOW));
    for blocker in [
        MemoryModuleType::WalkTarget,
        MemoryModuleType::BreezeJumpCooldown,
        MemoryModuleType::BreezeShoot,
    ] {
        let mut blocked = slider(10.0);
        blocked.set(blocker, MemoryValue::Unit);
        assert!(!blocked.start(&mut control, NOW), "{blocker:?}");
    }
}

#[test]
fn the_slide_sink_slides_in_the_sliding_pose_and_queues_a_shot_when_it_ends() {
    let mut bench = slider(10.0);
    bench.set(
        MemoryModuleType::WalkTarget,
        MemoryValue::WalkTarget(rspigot_ai::WalkTarget::position(
            Vec3::new(5.5, 64.0, 0.5),
            0.6,
            1,
        )),
    );
    let mut control = breeze_control(Activity::Idle, "BreezeSlideToTarget");
    assert!(bench.start(&mut control, NOW));
    assert_eq!(bench.body.pose, Pose::Sliding);
    assert!(bench.actions().contains(&MobAction::Sound {
        sound: "minecraft:entity.breeze.slide",
        volume: 1.0,
        pitch: 1.0,
    }));
    bench.with(|cx| control.do_stop(cx, NOW + 1));
    assert_eq!(bench.body.pose, Pose::Standing);
    assert_eq!(
        bench
            .state
            .memories
            .time_until_expiry(MemoryModuleType::BreezeShoot),
        Some(60)
    );
    assert!(bench.get(MemoryModuleType::WalkTarget).is_none());
}

#[test]
fn the_slide_sink_does_not_queue_a_shot_without_an_attack_target() {
    let mut bench = slider(10.0);
    bench.state.memories.erase(MemoryModuleType::AttackTarget);
    bench.set(
        MemoryModuleType::WalkTarget,
        MemoryValue::WalkTarget(rspigot_ai::WalkTarget::position(
            Vec3::new(5.5, 64.0, 0.5),
            0.6,
            1,
        )),
    );
    let mut control = breeze_control(Activity::Idle, "BreezeSlideToTarget");
    assert!(bench.start(&mut control, NOW));
    bench.with(|cx| control.do_stop(cx, NOW + 1));
    assert!(bench.get(MemoryModuleType::BreezeShoot).is_none());
}

fn jumper(distance: f64, seed: i64) -> Bench {
    let mut bench = breeze_bench(
        world_with(vec![target_at(7, Vec3::new(0.5 + distance, 64.0, 0.5))]),
        seed,
    );
    bench.set(MemoryModuleType::AttackTarget, entity(7));
    bench
}

fn ready_jumper() -> (Bench, MobControl) {
    for seed in 0..200 {
        let mut bench = jumper(12.0, seed);
        let mut control = breeze_control(Activity::Fight, "BreezeLongJump");
        if bench.start(&mut control, NOW) {
            return (bench, control);
        }
    }
    panic!("no seed lets the breeze pick a landing spot");
}

#[test]
fn a_breeze_picks_a_landing_spot_behind_its_target_and_inhales() {
    let (mut bench, _) = ready_jumper();
    let landing = bench
        .get(MemoryModuleType::BreezeJumpTarget)
        .and_then(MemoryValue::as_block)
        .expect("a landing spot");
    let target = Vec3::new(12.5, 64.0, 0.5);
    let distance = target.distance_squared(landing.center()).sqrt();
    assert!(distance > 2.0 && distance < 10.0, "{distance}");
    assert_eq!(bench.body.pose, Pose::Inhaling);
    assert_eq!(
        bench
            .state
            .memories
            .time_until_expiry(MemoryModuleType::BreezeJumpInhaling),
        Some(10)
    );
    assert!(bench.actions().contains(&MobAction::Sound {
        sound: "minecraft:entity.breeze.charge",
        volume: 1.0,
        pitch: 1.0,
    }));
}

#[test]
fn a_breeze_leaps_once_it_finished_inhaling_and_lands_with_cooldowns() {
    let (mut bench, mut control) = ready_jumper();
    bench.actions();
    bench.tick(&mut control, NOW + 5);
    assert_eq!(bench.body.pose, Pose::Inhaling);
    bench
        .state
        .memories
        .erase(MemoryModuleType::BreezeJumpInhaling);
    bench.tick(&mut control, NOW + 11);
    assert_eq!(bench.body.pose, Pose::LongJumping);
    assert!(bench.body.discard_friction);
    assert!(bench.body.velocity.length() > 0.1);
    assert!(bench.body.velocity.y > 0.0);
    assert_eq!(bench.body.yaw, bench.body.body_yaw);
    assert!(bench.actions().contains(&MobAction::Sound {
        sound: "minecraft:entity.breeze.jump",
        volume: 1.0,
        pitch: 1.0,
    }));
    bench.mob.on_ground = false;
    bench.tick(&mut control, NOW + 12);
    assert_eq!(bench.body.pose, Pose::LongJumping);
    bench.mob.on_ground = true;
    bench.tick(&mut control, NOW + 13);
    assert_eq!(bench.body.pose, Pose::Standing);
    assert!(!bench.body.discard_friction);
    assert_eq!(
        bench
            .state
            .memories
            .time_until_expiry(MemoryModuleType::BreezeJumpCooldown),
        Some(10)
    );
    assert_eq!(
        bench
            .state
            .memories
            .time_until_expiry(MemoryModuleType::BreezeShoot),
        Some(100)
    );
    assert_eq!(control.status(), Status::Running);
    bench.tick(&mut control, NOW + 14);
    assert_eq!(control.status(), Status::Stopped);
    assert!(bench.get(MemoryModuleType::BreezeJumpTarget).is_none());
}

#[test]
fn a_hurt_breeze_lands_with_a_two_tick_jump_cooldown() {
    let (mut bench, mut control) = ready_jumper();
    bench.set(
        MemoryModuleType::HurtBy,
        MemoryValue::HurtBy(rspigot_ai::LastDamage {
            kind: rspigot_domain::damage::DamageType::Arrow,
            entity: Some(EntityId(7)),
        }),
    );
    bench
        .state
        .memories
        .erase(MemoryModuleType::BreezeJumpInhaling);
    bench.tick(&mut control, NOW + 11);
    bench.tick(&mut control, NOW + 12);
    assert_eq!(
        bench
            .state
            .memories
            .time_until_expiry(MemoryModuleType::BreezeJumpCooldown),
        Some(2)
    );
}

#[test]
fn a_breeze_that_cannot_reach_its_landing_spot_stands_back_up() {
    let mut bench = jumper(12.0, 1);
    bench.set(
        MemoryModuleType::BreezeJumpTarget,
        MemoryValue::Block(BlockPos::new(80, 64, 0)),
    );
    let mut control = breeze_control(Activity::Fight, "BreezeLongJump");
    assert!(bench.start(&mut control, NOW));
    assert_eq!(bench.body.pose, Pose::Inhaling);
    bench
        .state
        .memories
        .erase(MemoryModuleType::BreezeJumpInhaling);
    bench.tick(&mut control, NOW + 11);
    assert_eq!(bench.body.pose, Pose::Standing);
    assert_eq!(bench.body.velocity, Vec3::ZERO);
}

fn leaping(boost: f32) -> Vec3 {
    let mut bench = jumper(12.0, 1);
    bench.mob.jump_boost_power = boost;
    bench.set(
        MemoryModuleType::BreezeJumpTarget,
        MemoryValue::Block(BlockPos::new(15, 64, 0)),
    );
    let mut control = breeze_control(Activity::Fight, "BreezeLongJump");
    assert!(bench.start(&mut control, NOW));
    bench
        .state
        .memories
        .erase(MemoryModuleType::BreezeJumpInhaling);
    bench.tick(&mut control, NOW + 11);
    assert_eq!(bench.body.pose, Pose::LongJumping);
    bench.body.velocity
}

#[test]
fn a_breeze_with_jump_boost_adds_the_boost_to_the_vertical_speed_of_the_leap() {
    let plain = leaping(0.0);
    let boosted = leaping(0.2);
    assert_eq!((boosted.x, boosted.z), (plain.x, plain.z));
    let expected = plain.y + plain.normalize().y * 0.2f32 as f64;
    assert!(
        (boosted.y - expected).abs() < 1.0e-9,
        "{} {}",
        boosted.y,
        expected
    );
}

#[test]
fn a_breeze_leap_uses_a_speed_capped_by_its_follow_range() {
    let plain = leaping(0.0);
    assert!(plain.length() <= 24.0 * 0.058_333_334 * 0.95 + 1.0e-6);
}

fn jump_world_blocker(configure: impl FnOnce(&mut Bench)) -> bool {
    let (mut bench, mut control) = {
        let mut bench = jumper(12.0, 1);
        configure(&mut bench);
        let control = breeze_control(Activity::Fight, "BreezeLongJump");
        (bench, control)
    };
    bench.start(&mut control, NOW)
}

#[test]
fn a_breeze_does_not_jump_when_it_cannot_or_should_not() {
    let target_close = {
        let mut bench = jumper(3.0, 1);
        let mut control = breeze_control(Activity::Fight, "BreezeLongJump");
        bench.start(&mut control, NOW)
    };
    assert!(!target_close);
    assert!(!jump_world_blocker(|bench| bench.mob.on_ground = false));
    assert!(!jump_world_blocker(|bench| bench.world.put(
        0,
        66,
        0,
        BlockKind::Stone.default_state()
    )));
    assert!(!jump_world_blocker(|bench| bench.world.put(
        0,
        64,
        0,
        BlockKind::HoneyBlock.default_state()
    )));
    assert!(!jump_world_blocker(
        |bench| bench.mob.fluid.lava_height = 0.5
    ));
    assert!(!jump_world_blocker(|bench| bench
        .state
        .memories
        .set(MemoryModuleType::BreezeJumpCooldown, MemoryValue::Unit)));
    assert!(!jump_world_blocker(|bench| bench
        .state
        .memories
        .set(MemoryModuleType::BreezeShoot, MemoryValue::Unit)));
    assert!(!jump_world_blocker(|bench| bench.state.memories.set(
        MemoryModuleType::WalkTarget,
        MemoryValue::WalkTarget(rspigot_ai::WalkTarget::position(
            Vec3::new(3.5, 64.0, 3.5),
            0.6,
            1
        ))
    )));
}

#[test]
fn a_breeze_forgets_a_target_beyond_its_follow_range_instead_of_jumping() {
    let mut bench = jumper(30.0, 1);
    let mut control = breeze_control(Activity::Fight, "BreezeLongJump");
    assert!(!bench.start(&mut control, NOW));
    assert!(bench.get(MemoryModuleType::AttackTarget).is_none());
}

#[test]
fn a_breeze_in_deep_water_does_not_jump_but_one_in_shallow_water_may() {
    let mut deep = jumper(12.0, 1);
    deep.mob.on_ground = false;
    deep.mob.fluid.in_water = true;
    deep.mob.fluid.water_height = 2.0;
    let mut control = breeze_control(Activity::Fight, "BreezeLongJump");
    assert!(!deep.start(&mut control, NOW));
    let mut shallow = jumper(12.0, 1);
    shallow.mob.on_ground = false;
    shallow.mob.fluid.in_water = true;
    shallow.mob.fluid.water_height = 0.5;
    shallow.set(
        MemoryModuleType::BreezeJumpTarget,
        MemoryValue::Block(BlockPos::new(15, 64, 0)),
    );
    let mut control = breeze_control(Activity::Fight, "BreezeLongJump");
    assert!(shallow.start(&mut control, NOW));
}

#[test]
fn the_breeze_memories_round_trip_through_the_brain_nbt() {
    let mut ai = breeze_ai(3);
    let memories = ai.behavior_brain_mut().unwrap().memories_mut();
    memories.set_with_expiry(MemoryModuleType::BreezeShoot, MemoryValue::Unit, 50);
    memories.set_with_expiry(
        MemoryModuleType::BreezeJumpTarget,
        MemoryValue::Block(BlockPos::new(3, 64, -2)),
        NO_EXPIRY,
    );
    memories.set(MemoryModuleType::WalkTarget, MemoryValue::Unit);
    let saved = ai.save_behavior_brain().expect("a brain");
    let stored = saved.get_compound("memories").expect("memories");
    assert!(stored.get_compound("minecraft:breeze_shoot").is_some());
    assert!(
        stored
            .get_compound("minecraft:breeze_jump_target")
            .is_some()
    );
    assert!(stored.get_compound("minecraft:walk_target").is_none());
    let mut loaded = breeze_ai(4);
    loaded.load_behavior_brain(&saved);
    let memories = loaded.behavior_brain().unwrap().memories();
    assert!(memories.has_value(MemoryModuleType::BreezeShoot));
    assert_eq!(
        memories.time_until_expiry(MemoryModuleType::BreezeShoot),
        Some(50)
    );
    assert_eq!(
        memories
            .get(MemoryModuleType::BreezeJumpTarget)
            .and_then(MemoryValue::as_block),
        Some(BlockPos::new(3, 64, -2))
    );
}

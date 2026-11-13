use std::collections::HashMap;

use rspigot_ai::{
    Activity, AiContext, AiWorld, AxolotlAction, AxolotlAttackablesSensor, AxolotlPlayDead,
    BehaviorControl, BehaviorCx, BrainMessage, BrainState, Facts, FrogAction, GoalSettings, Living,
    Memories, MemoryModuleType, MemoryValue, MobAction, MobAi, MobBehavior, MobBody, MobControl,
    MobTrigger, MobView, NautilusAction, NautilusChargeAttack, NearestVisibleLivingEntities,
    Sensor, SensorKind, Status, TargetKind,
};
use rspigot_blocks::{BlockKind, StateId};
use rspigot_domain::entity::living::LivingState;
use rspigot_domain::entity::{Aabb, EntityBase};
use rspigot_domain::{BlockPos, EntityId, Vec3};
use rspigot_entities::EntityKind;
use uuid::Uuid;

const NOW: i64 = 100;
const HOME: Vec3 = Vec3::new(0.5, 64.0, 0.5);

struct Field {
    others: Vec<Living>,
    edits: HashMap<BlockPos, StateId>,
}

impl Field {
    fn meadow() -> Self {
        Self {
            others: Vec::new(),
            edits: HashMap::new(),
        }
    }

    fn put(&mut self, x: i32, y: i32, z: i32, state: StateId) {
        self.edits.insert(BlockPos::new(x, y, z), state);
    }

    fn pond(&mut self, x: std::ops::RangeInclusive<i32>, z: std::ops::RangeInclusive<i32>) {
        for x in x {
            for z in z.clone() {
                for y in 62..=64 {
                    self.put(x, y, z, BlockKind::Water.default_state());
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
        Some(if (55..=63).contains(&pos.y) {
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

    fn living_by_uuid(&self, uuid: Uuid) -> Option<Living> {
        self.others
            .iter()
            .find(|living| living.facts.uuid == uuid)
            .copied()
    }
}

fn creature(id: i32, kind: EntityKind, position: Vec3, in_water: bool) -> Living {
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
        in_water,
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

fn view(kind: EntityKind, position: Vec3, in_water: bool, on_ground: bool) -> MobView {
    let mut base = EntityBase::new(EntityId(1), Uuid::from_u128(1), kind);
    base.position = position;
    base.old_position = position;
    base.on_ground = on_ground;
    base.fluid.in_water = in_water;
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
    fn new(kind: EntityKind, world: Field, in_water: bool, on_ground: bool) -> Self {
        let mob = view(kind, HOME, in_water, on_ground);
        let mut state = BrainState::default();
        for ty in MemoryModuleType::ALL {
            state.memories.register(*ty);
        }
        Self {
            body: MobBody::of(&mob),
            mob,
            world,
            controls: rspigot_ai::Brain::new(kind, 7),
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

    fn set(&mut self, ty: MemoryModuleType, value: MemoryValue) {
        self.state.memories.set(ty, value);
    }

    fn has(&self, ty: MemoryModuleType) -> bool {
        self.state.memories.has_value(ty)
    }

    fn actions(&mut self) -> Vec<MobAction> {
        std::mem::take(&mut self.controls.actions)
    }

    fn walk_speed(&self) -> Option<f32> {
        self.state
            .memories
            .get(MemoryModuleType::WalkTarget)
            .and_then(MemoryValue::as_walk_target)
            .map(|target| target.speed_modifier)
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

fn activity(kind: EntityKind, activity: Activity) -> Vec<(i32, String)> {
    let ai = MobAi::new(kind, 1, GoalSettings::default());
    ai.behavior_brain()
        .expect("a brain")
        .activity_behaviors(activity)
        .into_iter()
        .map(|(priority, control)| (priority, describe(control)))
        .collect()
}

fn pick(kind: EntityKind, activity: Activity, name: &str) -> MobControl {
    let ai = MobAi::new(kind, 1, GoalSettings::default());
    ai.behavior_brain()
        .expect("a brain")
        .activity_behaviors(activity)
        .into_iter()
        .map(|(_, control)| control.clone())
        .find(|control| describe(control).contains(name))
        .expect("the behavior")
}

fn priorities(entries: &[(i32, String)]) -> Vec<i32> {
    entries.iter().map(|(priority, _)| *priority).collect()
}

#[test]
fn the_frog_swim_activity_starts_with_a_swim_stroll_at_three_quarters() {
    let swim = activity(EntityKind::Frog, Activity::Swim);
    assert_eq!(priorities(&swim), vec![0, 1, 2, 3, 5]);
    let gate = &swim[4].1;
    let swim_at = gate.find("swimming: true").expect("a swim stroll");
    assert!(gate[..swim_at].contains("speed_modifier: 0.75"));
    assert!(gate.contains("speed_modifier: 1.0"));
    assert!(swim[3].1.contains("range: 8") && swim[3].1.contains("speed_modifier: 1.5"));
}

#[test]
fn the_frog_lay_spawn_activity_looks_for_land_then_lays_the_spawn() {
    let lay = activity(EntityKind::Frog, Activity::LaySpawn);
    assert_eq!(priorities(&lay), vec![0, 1, 2, 3, 4]);
    assert!(lay[2].1.contains("TryFindLandNearLiquid"));
    assert!(lay[2].1.contains("range: 8") && lay[2].1.contains("speed_modifier: 1.0"));
    assert!(lay[2].1.contains("minecraft:frog_tries_to_find_land_near"));
    assert!(lay[3].1.contains("TryLaySpawnOnFluidNearLand"));
}

#[test]
fn a_pregnant_frog_on_the_shore_lays_the_spawn_above_the_water_and_stops_being_pregnant() {
    let mut world = Field::meadow();
    world.pond(1..=1, 0..=0);
    world.put(1, 64, 0, StateId::AIR);
    let mut bench = Bench::new(EntityKind::Frog, world, false, true);
    bench.set(MemoryModuleType::IsPregnant, MemoryValue::Unit);
    bench.set(
        MemoryModuleType::WalkTarget,
        MemoryValue::WalkTarget(rspigot_ai::WalkTarget::block(
            BlockPos::new(1, 64, 0),
            1.0,
            0,
        )),
    );
    let mut lay = pick(
        EntityKind::Frog,
        Activity::LaySpawn,
        "TryLaySpawnOnFluidNearLand",
    );
    assert!(bench.start(&mut lay, NOW));
    let laid: Vec<FrogAction> = bench
        .actions()
        .into_iter()
        .filter_map(|action| match action {
            MobAction::Frog(action) => Some(action),
            _ => None,
        })
        .collect();
    assert_eq!(laid, vec![FrogAction::LaySpawn(BlockPos::new(1, 64, 0))]);
    assert!(!bench.has(MemoryModuleType::IsPregnant));
}

#[test]
fn a_frog_in_the_water_or_in_the_air_lays_nothing() {
    for (in_water, on_ground) in [(true, true), (false, false)] {
        let mut world = Field::meadow();
        world.pond(1..=1, 0..=0);
        let mut bench = Bench::new(EntityKind::Frog, world, in_water, on_ground);
        bench.set(MemoryModuleType::IsPregnant, MemoryValue::Unit);
        bench.set(
            MemoryModuleType::WalkTarget,
            MemoryValue::WalkTarget(rspigot_ai::WalkTarget::block(
                BlockPos::new(1, 64, 0),
                1.0,
                0,
            )),
        );
        let mut lay = pick(
            EntityKind::Frog,
            Activity::LaySpawn,
            "TryLaySpawnOnFluidNearLand",
        );
        assert!(!bench.start(&mut lay, NOW));
        assert!(bench.actions().is_empty());
        assert!(bench.has(MemoryModuleType::IsPregnant));
    }
}

#[test]
fn spawn_is_only_laid_on_water_beside_the_frog_with_air_above_it() {
    let mut world = Field::meadow();
    world.pond(1..=1, 0..=0);
    world.put(1, 64, 0, BlockKind::Stone.default_state());
    let mut bench = Bench::new(EntityKind::Frog, world, false, true);
    bench.set(MemoryModuleType::IsPregnant, MemoryValue::Unit);
    bench.set(
        MemoryModuleType::WalkTarget,
        MemoryValue::WalkTarget(rspigot_ai::WalkTarget::block(
            BlockPos::new(1, 64, 0),
            1.0,
            0,
        )),
    );
    let mut lay = pick(
        EntityKind::Frog,
        Activity::LaySpawn,
        "TryLaySpawnOnFluidNearLand",
    );
    assert!(bench.start(&mut lay, NOW));
    assert!(bench.actions().is_empty());
    assert!(bench.has(MemoryModuleType::IsPregnant));
}

#[test]
fn a_frog_far_from_the_water_walks_toward_land_next_to_it_and_waits_forty_ticks() {
    let mut world = Field::meadow();
    world.pond(3..=3, -1..=1);
    world.put(3, 64, 0, StateId::AIR);
    let mut bench = Bench::new(EntityKind::Frog, world, false, true);
    let mut find = pick(
        EntityKind::Frog,
        Activity::LaySpawn,
        "TryFindLandNearLiquid",
    );
    assert!(bench.start(&mut find, NOW));
    assert!(bench.has(MemoryModuleType::WalkTarget));
    assert_eq!(bench.walk_speed(), Some(1.0));
    assert!(bench.has(MemoryModuleType::LookTarget));
}

#[test]
fn a_frog_already_in_the_liquid_does_not_look_for_land_near_it() {
    let mut world = Field::meadow();
    world.pond(0..=0, 0..=0);
    let mut bench = Bench::new(EntityKind::Frog, world, true, false);
    let mut find = pick(
        EntityKind::Frog,
        Activity::LaySpawn,
        "TryFindLandNearLiquid",
    );
    assert!(!bench.start(&mut find, NOW));
    assert!(!bench.has(MemoryModuleType::WalkTarget));
}

#[test]
fn an_axolotl_out_of_the_water_finds_a_pond_and_swims_there_slowly() {
    let mut world = Field::meadow();
    world.pond(4..=5, 0..=0);
    let mut bench = Bench::new(EntityKind::Axolotl, world, false, true);
    let mut find = pick(EntityKind::Axolotl, Activity::Idle, "TryFindLiquid");
    assert!(bench.start(&mut find, NOW));
    assert_eq!(bench.walk_speed(), Some(0.15));
    assert!(bench.has(MemoryModuleType::LookTarget));
}

#[test]
fn an_axolotl_already_in_water_does_not_look_for_a_pond() {
    let mut world = Field::meadow();
    world.pond(0..=0, 0..=0);
    let mut bench = Bench::new(EntityKind::Axolotl, world, true, false);
    let mut find = pick(EntityKind::Axolotl, Activity::Idle, "TryFindLiquid");
    assert!(!bench.start(&mut find, NOW));
}

#[test]
fn the_axolotl_core_activity_looks_moves_validates_the_play_dead_and_counts_the_temptation() {
    let core = activity(EntityKind::Axolotl, Activity::Core);
    assert_eq!(priorities(&core), vec![0, 1, 2, 3]);
    assert!(core[0].1.contains("LookAtTargetSink") && core[0].1.contains("min_duration: 45"));
    assert!(core[1].1.contains("MoveToTargetSink"));
    assert!(core[2].1.contains("AxolotlValidatePlayDead"));
    assert!(core[3].1.contains("TemptationCooldownTicks"));
}

#[test]
fn the_axolotl_idle_activity_follows_the_vanilla_priorities_and_water_speeds() {
    let idle = activity(EntityKind::Axolotl, Activity::Idle);
    assert_eq!(priorities(&idle), vec![0, 1, 2, 3, 3, 4]);
    assert!(idle[0].1.contains("Player") && idle[0].1.contains("6.0"));
    assert!(idle[1].1.contains("AnimalMakeLove"));
    assert!(
        idle[1].1.contains("speed_modifier: 0.2") && idle[1].1.contains("close_enough_distance: 2")
    );
    assert!(idle[2].1.contains("in_water_speed: Some(0.5)"));
    assert!(idle[2].1.contains("in_water_speed: Some(0.6)"));
    assert!(idle[2].1.contains("min_inclusive: 5") && idle[2].1.contains("max_inclusive: 16"));
    assert!(idle[3].1.contains("StartAttacking"));
    assert!(idle[4].1.contains("TryFindLiquid") && idle[4].1.contains("range: 6"));
    assert!(idle[4].1.contains("speed_modifier: 0.15"));
    let wander = &idle[5].1;
    let order: Vec<usize> = [
        "swimming: true",
        "may_stroll_from_water: false",
        "SetWalkTargetFromLookTarget",
        "InWater",
        "OnGround",
    ]
    .iter()
    .map(|name| wander.find(name).unwrap_or_else(|| panic!("{name}")))
    .collect();
    assert!(order.windows(2).all(|pair| pair[0] < pair[1]));
}

#[test]
fn the_axolotl_fight_activity_stops_chases_bites_and_gives_up_when_breeding() {
    let fight = activity(EntityKind::Axolotl, Activity::Fight);
    assert_eq!(priorities(&fight), vec![0, 1, 2, 3]);
    assert!(fight[0].1.contains("StopAttackingIfTargetInvalid") && fight[0].1.contains("Axolotl"));
    assert!(
        fight[1].1.contains("in_water_speed: Some(0.6)")
            && fight[1].1.contains("speed_modifier: 0.15")
    );
    assert!(
        fight[2].1.contains("MeleeAttack") && fight[2].1.contains("cooldown_between_attacks: 20")
    );
    assert!(fight[3].1.contains("EraseMemoryIf") && fight[3].1.contains("BreedTarget"));
}

#[test]
fn the_axolotl_play_dead_activity_plays_dead_two_hundred_ticks_and_stops_when_breeding() {
    let dead = activity(EntityKind::Axolotl, Activity::PlayDead);
    assert_eq!(priorities(&dead), vec![0, 1]);
    assert!(dead[0].1.contains("AxolotlPlayDead"));
    assert!(dead[0].1.contains("min_duration: 200") && dead[0].1.contains("max_duration: 200"));
    assert!(dead[1].1.contains("PlayDeadTicks") && dead[1].1.contains("BreedTarget"));
}

#[test]
fn playing_dead_needs_water_and_forgets_the_walk_and_look_targets_and_regenerates() {
    let mut dry = Bench::new(EntityKind::Axolotl, Field::meadow(), false, true);
    dry.set(MemoryModuleType::PlayDeadTicks, MemoryValue::Int(200));
    dry.set(
        MemoryModuleType::HurtByEntity,
        MemoryValue::Entity(EntityId(5)),
    );
    let mut play =
        MobControl::behavior_for(MobBehavior::AxolotlPlayDead(AxolotlPlayDead), 200, 200);
    assert!(!dry.start(&mut play, NOW));
    let mut wet = Bench::new(EntityKind::Axolotl, Field::meadow(), true, false);
    wet.set(MemoryModuleType::PlayDeadTicks, MemoryValue::Int(200));
    wet.set(
        MemoryModuleType::HurtByEntity,
        MemoryValue::Entity(EntityId(5)),
    );
    wet.set(
        MemoryModuleType::WalkTarget,
        MemoryValue::WalkTarget(rspigot_ai::WalkTarget::block(
            BlockPos::new(3, 64, 0),
            1.0,
            0,
        )),
    );
    assert!(wet.start(&mut play, NOW));
    assert!(!wet.has(MemoryModuleType::WalkTarget));
    assert!(!wet.has(MemoryModuleType::LookTarget));
    assert!(
        wet.actions()
            .contains(&MobAction::Axolotl(AxolotlAction::PlayDeadStarted))
    );
    assert_eq!(play.status(), Status::Running);
}

#[test]
fn validating_the_play_dead_counts_down_and_resets_the_brain_at_zero() {
    let mut bench = Bench::new(EntityKind::Axolotl, Field::meadow(), true, false);
    bench.set(MemoryModuleType::PlayDeadTicks, MemoryValue::Int(2));
    bench.set(
        MemoryModuleType::HurtByEntity,
        MemoryValue::Entity(EntityId(5)),
    );
    let mut validate = MobControl::one_shot(MobTrigger::AxolotlValidatePlayDead(
        rspigot_ai::AxolotlValidatePlayDead,
    ));
    assert!(bench.start(&mut validate, NOW));
    let ticks = |bench: &Bench| {
        bench
            .state
            .memories
            .get(MemoryModuleType::PlayDeadTicks)
            .and_then(MemoryValue::as_int)
    };
    assert_eq!(ticks(&bench), Some(1));
    assert!(bench.start(&mut validate, NOW + 1));
    assert_eq!(ticks(&bench), Some(0));
    assert!(bench.start(&mut validate, NOW + 2));
    assert!(!bench.has(MemoryModuleType::PlayDeadTicks));
    assert!(!bench.has(MemoryModuleType::HurtByEntity));
}

fn axolotl_sensor_pick(bench: &mut Bench) -> Option<EntityId> {
    bench.tick_sensor(SensorKind::AxolotlAttackables(AxolotlAttackablesSensor));
    bench
        .state
        .memories
        .get(MemoryModuleType::NearestAttackable)
        .and_then(MemoryValue::as_entity)
}

#[test]
fn an_axolotl_hunts_nearby_fish_in_water_and_always_hostiles_even_on_cooldown() {
    let near = Vec3::new(2.5, 64.0, 0.5);
    let far = Vec3::new(9.5, 64.0, 0.5);
    let mut bench = Bench::new(EntityKind::Axolotl, Field::meadow(), true, false);
    bench.see(&[
        creature(2, EntityKind::Cod, far, true),
        creature(3, EntityKind::Cod, near, true),
        creature(4, EntityKind::Cod, Vec3::new(1.5, 64.0, 0.5), false),
    ]);
    assert_eq!(axolotl_sensor_pick(&mut bench), Some(EntityId(3)));
    let mut cooling = Bench::new(EntityKind::Axolotl, Field::meadow(), true, false);
    cooling.set(
        MemoryModuleType::HasHuntingCooldown,
        MemoryValue::Bool(true),
    );
    cooling.see(&[creature(3, EntityKind::Cod, near, true)]);
    assert_eq!(axolotl_sensor_pick(&mut cooling), None);
    let mut hostile = Bench::new(EntityKind::Axolotl, Field::meadow(), true, false);
    hostile.set(
        MemoryModuleType::HasHuntingCooldown,
        MemoryValue::Bool(true),
    );
    hostile.see(&[creature(5, EntityKind::Drowned, near, true)]);
    assert_eq!(axolotl_sensor_pick(&mut hostile), Some(EntityId(5)));
}

#[test]
fn leaving_the_fight_activity_starts_a_two_thousand_four_hundred_tick_hunting_cooldown() {
    let mut ai = MobAi::new(EntityKind::Axolotl, 3, GoalSettings::default());
    let mob = view(EntityKind::Axolotl, HOME, true, false);
    let world = Field::meadow();
    let brain = ai.behavior_brain_mut().unwrap();
    brain.memories_mut().set(
        MemoryModuleType::AttackTarget,
        MemoryValue::Entity(EntityId(2)),
    );
    let mut body = MobBody::of(&mob);
    ai.think(&mob, &world, &mut body);
    let active = |ai: &MobAi| {
        ai.behavior_brain()
            .and_then(|brain| brain.state().active_non_core_activity())
    };
    assert_eq!(active(&ai), Some(Activity::Fight));
    ai.behavior_brain_mut()
        .unwrap()
        .memories_mut()
        .erase(MemoryModuleType::AttackTarget);
    let mut body = MobBody::of(&mob);
    ai.think(&mob, &world, &mut body);
    assert_eq!(active(&ai), Some(Activity::Idle));
    let brain = ai.behavior_brain().unwrap();
    assert!(
        brain
            .memories()
            .has_value(MemoryModuleType::HasHuntingCooldown)
    );
    assert_eq!(
        brain
            .memories()
            .time_until_expiry(MemoryModuleType::HasHuntingCooldown),
        Some(2400)
    );
}

#[test]
fn a_hurt_axolotl_that_plays_dead_keeps_the_play_dead_activity_until_it_runs_out() {
    let mut ai = MobAi::new(EntityKind::Axolotl, 3, GoalSettings::default());
    let mob = view(EntityKind::Axolotl, HOME, true, false);
    let world = Field::meadow();
    let brain = ai.behavior_brain_mut().unwrap();
    brain
        .memories_mut()
        .set(MemoryModuleType::PlayDeadTicks, MemoryValue::Int(200));
    brain.memories_mut().set(
        MemoryModuleType::HurtByEntity,
        MemoryValue::Entity(EntityId(7)),
    );
    let mut body = MobBody::of(&mob);
    ai.think(&mob, &world, &mut body);
    let active = ai
        .behavior_brain()
        .and_then(|brain| brain.state().active_non_core_activity());
    assert_eq!(active, Some(Activity::PlayDead));
}

#[test]
fn the_tadpole_brain_panics_looks_moves_follows_frog_food_and_swims_about() {
    let core = activity(EntityKind::Tadpole, Activity::Core);
    assert_eq!(priorities(&core), vec![0, 1, 2, 3]);
    assert!(core[0].1.contains("AnimalPanic") && core[0].1.contains("speed_multiplier: 2.0"));
    assert!(core[1].1.contains("LookAtTargetSink"));
    assert!(core[3].1.contains("TemptationCooldownTicks"));
    let idle = activity(EntityKind::Tadpole, Activity::Idle);
    assert_eq!(priorities(&idle), vec![0, 1, 2]);
    assert!(idle[1].1.contains("speed_modifier: 1.25"));
    let wander = &idle[2].1;
    assert!(wander.contains("swimming: true") && wander.contains("speed_modifier: 0.5"));
    assert!(wander.contains("InWater"));
}

#[test]
fn the_nautilus_brain_has_a_charge_fight_activity_and_a_swimming_idle_activity() {
    let core = activity(EntityKind::Nautilus, Activity::Core);
    assert_eq!(priorities(&core), vec![0, 1, 2, 3, 4, 5]);
    assert!(core[0].1.contains("speed_multiplier: 1.6"));
    assert!(core[4].1.contains("ChargeCooldownTicks"));
    assert!(core[5].1.contains("AttackTargetCooldown"));
    let idle = activity(EntityKind::Nautilus, Activity::Idle);
    assert_eq!(priorities(&idle), vec![1, 2, 3, 4]);
    assert!(idle[0].1.contains("speed_modifier: 0.4"));
    assert!(idle[1].1.contains("speed_modifier: 1.3"));
    assert!(idle[1].1.contains("baby: 2.5") && idle[1].1.contains("adult: 3.5"));
    assert!(idle[3].1.contains("swimming: true"));
    let fight = activity(EntityKind::Nautilus, Activity::Fight);
    assert_eq!(priorities(&fight), vec![0]);
    assert!(fight[0].1.contains("time_between_attacks: 80"));
    assert!(fight[0].1.contains("speed: 0.6") && fight[0].1.contains("knockback_force: 2.0"));
    assert!(fight[0].1.contains("max_charge_distance: 12.0"));
    assert!(fight[0].1.contains("max_target_detection_distance: 11.0"));
    assert!(fight[0].1.contains("minecraft:entity.nautilus.dash"));
}

#[test]
fn the_zombie_nautilus_does_not_panic_or_breed_and_charges_a_little_slower() {
    let core = activity(EntityKind::ZombieNautilus, Activity::Core);
    assert_eq!(priorities(&core), vec![0, 1, 2, 3, 4]);
    assert!(core.iter().all(|(_, text)| !text.contains("AnimalPanic")));
    let idle = activity(EntityKind::ZombieNautilus, Activity::Idle);
    assert_eq!(priorities(&idle), vec![1, 2, 3]);
    assert!(idle[0].1.contains("speed_modifier: 0.9"));
    assert!(
        idle.iter()
            .all(|(_, text)| !text.contains("AnimalMakeLove"))
    );
    let fight = activity(EntityKind::ZombieNautilus, Activity::Fight);
    assert!(fight[0].1.contains("speed: 0.5"));
    assert!(fight[0].1.contains("minecraft:entity.zombie_nautilus.dash"));
}

#[test]
fn the_nautilus_fight_activity_needs_a_target_no_temptation_no_breeding_and_no_cooldown() {
    let ai = MobAi::new(EntityKind::Nautilus, 1, GoalSettings::default());
    let brain = ai.behavior_brain().unwrap();
    let memories = brain.memories();
    assert!(!memories.has_value(MemoryModuleType::ChargeCooldownTicks));
}

#[test]
fn an_angry_adult_nautilus_in_water_charges_the_one_that_hurt_it() {
    let mut bench = Bench::new(EntityKind::Nautilus, Field::meadow(), true, false);
    let victim = creature(8, EntityKind::Player, Vec3::new(2.5, 64.0, 0.5), true);
    bench.see(&[victim]);
    bench.set(
        MemoryModuleType::AngryAt,
        MemoryValue::Uuid(victim.facts.uuid),
    );
    let mut start = pick(EntityKind::Nautilus, Activity::Idle, "StartAttacking");
    assert!(bench.start(&mut start, NOW));
    assert!(bench.has(MemoryModuleType::AttackTarget));
}

#[test]
fn a_tame_baby_or_dry_nautilus_never_picks_a_target() {
    for (water, baby) in [(false, false), (true, true)] {
        let mut bench = Bench::new(EntityKind::Nautilus, Field::meadow(), water, false);
        bench.mob.baby = baby;
        let victim = creature(8, EntityKind::Player, Vec3::new(2.5, 64.0, 0.5), true);
        bench.see(&[victim]);
        bench.set(
            MemoryModuleType::AngryAt,
            MemoryValue::Uuid(victim.facts.uuid),
        );
        let mut start = pick(EntityKind::Nautilus, Activity::Idle, "StartAttacking");
        assert!(!bench.start(&mut start, NOW));
        assert!(!bench.has(MemoryModuleType::AttackTarget));
    }
    let mut tame = Bench::new(EntityKind::Nautilus, Field::meadow(), true, false);
    tame.mob.tamed = true;
    let victim = creature(8, EntityKind::Player, Vec3::new(2.5, 64.0, 0.5), true);
    tame.see(&[victim]);
    tame.set(
        MemoryModuleType::AngryAt,
        MemoryValue::Uuid(victim.facts.uuid),
    );
    let mut start = pick(EntityKind::Nautilus, Activity::Idle, "StartAttacking");
    assert!(!tame.start(&mut start, NOW));
    assert!(!tame.has(MemoryModuleType::AttackTarget));
}

#[test]
fn without_anger_the_nautilus_rolls_the_attack_cooldown_before_picking_a_hostile() {
    let mut bench = Bench::new(EntityKind::Nautilus, Field::meadow(), true, false);
    let mut start = pick(EntityKind::Nautilus, Activity::Idle, "StartAttacking");
    assert!(!bench.start(&mut start, NOW));
    let cooldown = bench
        .state
        .memories
        .get(MemoryModuleType::AttackTargetCooldown)
        .and_then(MemoryValue::as_int)
        .expect("a cooldown");
    assert!((2400..=3600).contains(&cooldown));
    assert!(!bench.has(MemoryModuleType::AttackTarget));
    let mut again = pick(EntityKind::Nautilus, Activity::Idle, "StartAttacking");
    assert!(!bench.start(&mut again, NOW + 1));
    assert_eq!(
        bench
            .state
            .memories
            .get(MemoryModuleType::AttackTargetCooldown)
            .and_then(MemoryValue::as_int),
        Some(cooldown)
    );
}

fn charge() -> MobControl {
    MobControl::behavior(MobBehavior::NautilusCharge(NautilusChargeAttack::new(
        80,
        0.6,
        2.0,
        12.0,
        11.0,
        "minecraft:entity.nautilus.dash",
    )))
}

#[test]
fn a_charging_nautilus_dashes_at_the_target_hits_it_once_and_rests_eighty_ticks() {
    let mut bench = Bench::new(EntityKind::Nautilus, Field::meadow(), true, false);
    let victim = creature(8, EntityKind::Cod, Vec3::new(0.9, 64.0, 0.5), true);
    bench.see(&[victim]);
    bench.set(
        MemoryModuleType::AttackTarget,
        MemoryValue::Entity(victim.id),
    );
    let mut charge = charge();
    assert!(bench.start(&mut charge, NOW));
    let sounds: Vec<_> = bench
        .actions()
        .into_iter()
        .filter(|action| matches!(action, MobAction::Sound { .. }))
        .collect();
    assert_eq!(sounds.len(), 1);
    bench.tick(&mut charge, NOW + 1);
    assert_eq!(bench.body.velocity, Vec3::new(f64::from(0.6_f32), 0.0, 0.0));
    let hits: Vec<_> = bench
        .actions()
        .into_iter()
        .filter_map(|action| match action {
            MobAction::Nautilus(NautilusAction::ChargeHit {
                target,
                speed,
                knockback,
                ..
            }) => Some((target, speed, knockback)),
            _ => None,
        })
        .collect();
    assert_eq!(hits, vec![(EntityId(8), 0.6, 2.0)]);
    assert_eq!(
        bench
            .state
            .memories
            .get(MemoryModuleType::ChargeCooldownTicks)
            .and_then(MemoryValue::as_int),
        Some(80)
    );
    assert!(!bench.has(MemoryModuleType::AttackTarget));
}

#[test]
fn a_charge_gives_up_when_the_target_is_too_far_or_the_nautilus_is_tame() {
    let mut far = Bench::new(EntityKind::Nautilus, Field::meadow(), true, false);
    let victim = creature(8, EntityKind::Cod, Vec3::new(20.5, 64.0, 0.5), true);
    far.see(&[victim]);
    far.set(
        MemoryModuleType::AttackTarget,
        MemoryValue::Entity(victim.id),
    );
    let mut charge_far = charge();
    assert!(far.start(&mut charge_far, NOW));
    assert!(
        far.actions()
            .iter()
            .all(|action| !matches!(action, MobAction::Sound { .. }))
    );
    let mut tame = Bench::new(EntityKind::Nautilus, Field::meadow(), true, false);
    tame.mob.tamed = true;
    let near = creature(9, EntityKind::Cod, Vec3::new(2.5, 64.0, 0.5), true);
    tame.see(&[near]);
    tame.set(MemoryModuleType::AttackTarget, MemoryValue::Entity(near.id));
    let mut charge_tame = charge();
    assert!(tame.start(&mut charge_tame, NOW));
    assert!(
        tame.actions()
            .iter()
            .all(|action| !matches!(action, MobAction::Sound { .. }))
    );
}

#[test]
fn being_hurt_angers_the_nautilus_at_the_attacker_for_four_hundred_ticks() {
    let mut ai = MobAi::new(EntityKind::Nautilus, 3, GoalSettings::default());
    let mob = view(EntityKind::Nautilus, HOME, true, false);
    let mut world = Field::meadow();
    let attacker = creature(6, EntityKind::Player, Vec3::new(2.5, 64.0, 0.5), true);
    world.others.push(attacker);
    let mut body = MobBody::of(&mob);
    ai.deliver(
        &mob,
        &world,
        &mut body,
        BrainMessage::NautilusAnger {
            attacker: attacker.id,
            uuid: attacker.facts.uuid,
        },
    );
    let memories: &Memories = ai.behavior_brain().unwrap().memories();
    assert_eq!(
        memories
            .get(MemoryModuleType::AngryAt)
            .and_then(MemoryValue::as_uuid),
        Some(attacker.facts.uuid)
    );
    assert_eq!(
        memories.time_until_expiry(MemoryModuleType::AngryAt),
        Some(400)
    );
}

#[test]
fn a_lost_attack_target_is_reported_to_the_axolotl_so_it_can_reward_the_killer() {
    let mut bench = Bench::new(EntityKind::Axolotl, Field::meadow(), true, false);
    bench.set(
        MemoryModuleType::AttackTarget,
        MemoryValue::Entity(EntityId(8)),
    );
    let mut stop = pick(
        EntityKind::Axolotl,
        Activity::Fight,
        "StopAttackingIfTargetInvalid",
    );
    assert!(bench.start(&mut stop, NOW));
    assert!(!bench.has(MemoryModuleType::AttackTarget));
    assert!(
        bench
            .actions()
            .contains(&MobAction::Axolotl(AxolotlAction::TargetErased(EntityId(
                8
            ))))
    );
}

#[test]
fn only_source_water_counts_for_the_fluid_tags_the_amphibians_look_for() {
    let source = BlockKind::Water.default_state();
    let flowing = BlockKind::Water
        .default_state()
        .with_property("level", "3")
        .expect("a level");
    assert!(rspigot_ai::fluid_in_tag(
        Some(source),
        "minecraft:axolotl_tries_to_find"
    ));
    assert!(!rspigot_ai::fluid_in_tag(
        Some(flowing),
        "minecraft:axolotl_tries_to_find"
    ));
    assert!(rspigot_ai::fluid_in_tag(Some(flowing), "minecraft:water"));
    assert!(!rspigot_ai::fluid_in_tag(
        Some(BlockKind::Stone.default_state()),
        "minecraft:water"
    ));
}

use std::collections::HashMap;

use rspigot_ai::{
    Activity, AiContext, AiWorld, BehaviorControl, BehaviorCx, BrainState, Facts, GoalSettings,
    Living, MemoryModuleType, MemoryValue, MobAction, MobAi, MobBody, MobControl, MobView,
    NearestVisibleLivingEntities, RaidFacts, SensorKind, StackFacts, Status, TargetKind, TradeView,
    VillagerAction, VillagerFacts, VillagerPeerFacts, WalkTarget,
};
use rspigot_blocks::{BlockKind, StateId};
use rspigot_domain::entity::living::LivingState;
use rspigot_domain::entity::species::villager::Profession;
use rspigot_domain::entity::{Aabb, EntityBase};
use rspigot_domain::poi::{Occupancy, PoiRecord, PoiType};
use rspigot_domain::{BlockPos, EntityId, Vec3};
use rspigot_entities::EntityKind;
use rspigot_items::ItemKind;
use uuid::Uuid;

const NOW: i64 = 1_000;

#[derive(Default)]
struct Field {
    others: Vec<Living>,
    players: Vec<Living>,
    edits: HashMap<BlockPos, StateId>,
    pois: Vec<PoiRecord>,
    raid: Option<RaidFacts>,
    trades: Vec<TradeView>,
    time: i64,
    clock: i64,
}

impl Field {
    fn flat() -> Self {
        Self {
            time: NOW,
            clock: NOW,
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
        Some(if (55..=63).contains(&pos.y) {
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

    fn clock_time(&self) -> i64 {
        self.clock
    }

    fn raw_brightness(&self, _pos: BlockPos) -> i32 {
        15
    }

    fn pois_in_range(&self, center: BlockPos, radius: i32, occupancy: Occupancy) -> Vec<PoiRecord> {
        self.pois
            .iter()
            .filter(|record| occupancy.test(record))
            .filter(|record| {
                let (dx, dy, dz) = (
                    i64::from(record.pos().x - center.x),
                    i64::from(record.pos().y - center.y),
                    i64::from(record.pos().z - center.z),
                );
                dx * dx + dy * dy + dz * dz <= i64::from(radius) * i64::from(radius)
            })
            .copied()
            .collect()
    }

    fn poi_type_at(&self, pos: BlockPos) -> Option<PoiType> {
        self.pois
            .iter()
            .find(|record| record.pos() == pos)
            .map(PoiRecord::kind)
    }

    fn raid_at(&self, _pos: BlockPos) -> Option<RaidFacts> {
        self.raid
    }

    fn villager_trades(&self, _villager: EntityId) -> Vec<TradeView> {
        self.trades.clone()
    }

    fn is_village(&self, _pos: BlockPos) -> bool {
        true
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

fn villager_at(id: i32, position: Vec3, facts: VillagerPeerFacts) -> Living {
    let mut found = living(id, EntityKind::Villager, position);
    found.facts.villager = facts;
    found
}

fn view(kind: EntityKind, position: Vec3, facts: VillagerFacts, baby: bool) -> MobView {
    let mut base = EntityBase::new(EntityId(1), Uuid::from_u128(1), kind);
    base.position = position;
    base.old_position = position;
    base.on_ground = true;
    base.tick_count = 1;
    let state = LivingState::new(&mut base, 0.0);
    let mut view = MobView::of(&base, &state);
    view.species.villager = facts;
    view.baby = baby;
    view
}

struct Bench {
    mob: MobView,
    world: Field,
    controls: rspigot_ai::Brain,
    body: MobBody,
    state: BrainState,
}

impl Bench {
    fn new(facts: VillagerFacts, world: Field, seed: i64) -> Self {
        Self::at(Vec3::new(0.5, 64.0, 0.5), facts, world, seed, false)
    }

    fn at(at: Vec3, facts: VillagerFacts, world: Field, seed: i64, baby: bool) -> Self {
        let mob = view(EntityKind::Villager, at, facts, baby);
        let mut state = BrainState::default();
        for ty in MemoryModuleType::ALL {
            state.memories.register(*ty);
        }
        Self {
            body: MobBody::of(&mob),
            mob,
            world,
            controls: rspigot_ai::Brain::new(EntityKind::Villager, seed),
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

    fn global(&self, pos: BlockPos) -> MemoryValue {
        MemoryValue::GlobalPos(rspigot_ai::GlobalPos {
            dimension: "minecraft:overworld".to_owned(),
            pos,
        })
    }

    fn actions(&mut self) -> Vec<MobAction> {
        std::mem::take(&mut self.controls.actions)
    }

    fn villager_actions(&mut self) -> Vec<VillagerAction> {
        self.actions()
            .into_iter()
            .filter_map(|action| match action {
                MobAction::Villager(action) => Some(action),
                _ => None,
            })
            .collect()
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

fn behaviors(profession: Profession, baby: bool, activity: Activity) -> Vec<(i32, String)> {
    let mut ai = MobAi::new(EntityKind::Villager, 1, GoalSettings::default());
    ai.rebuild_villager_brain(profession, baby);
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

const KNOWN: [&str; 53] = [
    "Swim",
    "InteractWithDoor",
    "LookAtTargetSink",
    "VillagerPanicTrigger",
    "WakeUp",
    "ReactToBell",
    "SetRaidStatus",
    "ValidateNearbyPoi",
    "MoveToTargetSink",
    "PoiCompetitorScan",
    "LookAndFollowTradingPlayerSink",
    "GoToWantedItem",
    "AcquirePoi",
    "GoToPotentialJobSite",
    "YieldJobSite",
    "AssignProfessionFromJobSite",
    "ResetProfession",
    "SetEntityLookTarget",
    "ShowTradesToPlayer",
    "SetLookAndInteract",
    "SetWalkTargetFromBlockMemory",
    "GiveGiftToHero",
    "UpdateActivityFromSchedule",
    "PlayTagWithOtherKids",
    "SleepInBed",
    "InteractWith",
    "VillageBoundRandomStroll",
    "SetWalkTargetFromLookTarget",
    "JumpOnBed",
    "DoNothing",
    "TradeWithVillager",
    "VillagerMakeLove",
    "VillagerCalmDown",
    "SetWalkTargetAwayFrom",
    "RingBell",
    "ResetRaidStatus",
    "LocateHidingPlace",
    "SetHiddenState",
    "StrollAroundPoi",
    "SocializeAtBell",
    "WorkAtPoi",
    "HarvestFarmland",
    "StrollToPoi",
    "MoveToSkySeeingSpot",
    "Gate",
    "SetClosestHomeAsWalkTarget",
    "InsideBrownianWalk",
    "GoToClosestVillage",
    "StrollToPoiList",
    "OneShot",
    "Timed",
    "Sequence",
    "Condition",
];

fn farmer() -> VillagerFacts {
    VillagerFacts {
        profession: Profession::Farmer,
        level: 1,
        ..VillagerFacts::default()
    }
}

fn kinds(list: Vec<(i32, String)>) -> Vec<i32> {
    list.into_iter().map(|(priority, _)| priority).collect()
}

#[test]
fn the_core_activity_follows_villager_goal_packages() {
    let core = behaviors(Profession::None, false, Activity::Core);
    assert_eq!(
        kinds(core.clone()),
        vec![
            0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 2, 3, 5, 6, 7, 8, 10, 10, 10, 10
        ]
    );
    let names = leading(&core, &KNOWN);
    assert_eq!(names[0], (0, "Swim"));
    assert_eq!(names[1], (0, "InteractWithDoor"));
    assert_eq!(names[2], (0, "LookAtTargetSink"));
    assert_eq!(names[3], (0, "VillagerPanicTrigger"));
    assert_eq!(names[9], (1, "MoveToTargetSink"));
    assert_eq!(names[11], (3, "LookAndFollowTradingPlayerSink"));
    assert_eq!(names[14], (7, "GoToPotentialJobSite"));
}

#[test]
fn adults_work_and_babies_play_with_the_same_other_activities() {
    assert!(!behaviors(Profession::Farmer, false, Activity::Work).is_empty());
    assert!(behaviors(Profession::Farmer, false, Activity::Play).is_empty());
    assert!(behaviors(Profession::Farmer, true, Activity::Work).is_empty());
    assert!(!behaviors(Profession::Farmer, true, Activity::Play).is_empty());
    for activity in [
        Activity::Meet,
        Activity::Rest,
        Activity::Idle,
        Activity::Panic,
        Activity::PreRaid,
        Activity::Raid,
        Activity::Hide,
    ] {
        assert!(
            !behaviors(Profession::None, false, activity).is_empty(),
            "{activity:?}"
        );
    }
}

#[test]
fn farmers_weight_the_farm_work_higher() {
    let work = |profession| {
        let list = behaviors(profession, false, Activity::Work);
        let (_, text) = list
            .iter()
            .find(|(_, text)| text.contains("WorkAtPoi"))
            .unwrap();
        text.clone()
    };
    let farmer = work(Profession::Farmer);
    let librarian = work(Profession::Librarian);
    assert!(farmer.contains("Composter"));
    assert!(!librarian.contains("Composter"));
    assert!(farmer.contains("HarvestFarmland"));
}

#[test]
fn the_idle_activity_runs_the_social_and_breeding_gates() {
    let idle = behaviors(Profession::None, false, Activity::Idle);
    assert_eq!(kinds(idle.clone()), vec![2, 3, 3, 3, 3, 3, 5, 99]);
    let names = leading(&idle, &KNOWN);
    assert_eq!(names[1], (3, "GiveGiftToHero"));
    assert_eq!(names[3], (3, "ShowTradesToPlayer"));
    assert!(idle[4].1.contains("TradeWithVillager"));
    assert!(idle[5].1.contains("VillagerMakeLove"));
    assert_eq!(names[7], (99, "UpdateActivityFromSchedule"));
}

#[test]
fn the_panic_and_hide_packages_flee_and_hide() {
    let panic = behaviors(Profession::None, false, Activity::Panic);
    assert_eq!(kinds(panic.clone()), vec![0, 1, 1, 3, 5]);
    assert!(panic[0].1.contains("VillagerCalmDown"));
    let hide = behaviors(Profession::None, false, Activity::Hide);
    assert_eq!(kinds(hide.clone()), vec![0, 1, 5]);
    assert!(hide[0].1.contains("SetHiddenState"));
    assert!(hide[1].1.contains("LocateHidingPlace"));
}

#[test]
fn the_sensors_are_the_vanilla_nine() {
    let ai = MobAi::new(EntityKind::Villager, 1, GoalSettings::default());
    let brain = ai.behavior_brain().expect("a brain");
    let names: Vec<&str> = brain
        .sensors()
        .iter()
        .map(|slot| slot.sensor().sensor_type().name())
        .collect();
    assert_eq!(
        names,
        vec![
            "minecraft:nearest_living_entities",
            "minecraft:nearest_players",
            "minecraft:nearest_items",
            "minecraft:nearest_bed",
            "minecraft:hurt_by",
            "minecraft:villager_hostiles",
            "minecraft:villager_babies",
            "minecraft:secondary_pois",
            "minecraft:golem_detected",
        ]
    );
}

#[test]
fn the_schedule_switches_adults_and_babies_between_their_timelines() {
    let mut ai = MobAi::new(EntityKind::Villager, 1, GoalSettings::default());
    ai.rebuild_villager_brain(Profession::None, false);
    let adult = ai.behavior_brain().unwrap().state().schedule().unwrap();
    assert_eq!(adult.activity_at(2_500), Activity::Work);
    assert_eq!(adult.activity_at(9_500), Activity::Meet);
    assert_eq!(adult.activity_at(12_500), Activity::Rest);
    ai.rebuild_villager_brain(Profession::None, true);
    let baby = ai.behavior_brain().unwrap().state().schedule().unwrap();
    assert_eq!(baby.activity_at(3_500), Activity::Play);
    assert_eq!(baby.activity_at(7_000), Activity::Idle);
}

#[test]
fn the_persisted_memories_round_trip_through_the_vanilla_layout() {
    let mut ai = MobAi::new(EntityKind::Villager, 1, GoalSettings::default());
    ai.rebuild_villager_brain(Profession::None, false);
    let pos = BlockPos::new(12, 64, 14);
    let brain = ai.behavior_brain_mut().unwrap();
    brain.memories_mut().set(
        MemoryModuleType::Home,
        MemoryValue::GlobalPos(rspigot_ai::GlobalPos {
            dimension: "minecraft:overworld".to_owned(),
            pos,
        }),
    );
    brain
        .memories_mut()
        .set(MemoryModuleType::LastSlept, MemoryValue::Long(500));
    brain
        .memories_mut()
        .set(MemoryModuleType::NearestBed, MemoryValue::Block(pos));
    let saved = ai.save_behavior_brain().unwrap();
    let memories = saved.get_compound("memories").unwrap();
    assert!(memories.contains_key("minecraft:home"));
    assert!(memories.contains_key("minecraft:last_slept"));
    assert!(!memories.contains_key("minecraft:nearest_bed"));
    let home = memories.get_compound("minecraft:home").unwrap();
    let value = home.get_compound("value").unwrap();
    assert_eq!(value.get_str("dimension"), Some("minecraft:overworld"));
    let mut again = MobAi::new(EntityKind::Villager, 2, GoalSettings::default());
    again.rebuild_villager_brain(Profession::None, false);
    again.load_behavior_brain(&saved);
    let restored = again.behavior_brain().unwrap().memories();
    assert_eq!(
        restored
            .get(MemoryModuleType::Home)
            .and_then(MemoryValue::as_global_pos)
            .map(|global| global.pos),
        Some(pos)
    );
    assert_eq!(
        restored
            .get(MemoryModuleType::LastSlept)
            .and_then(MemoryValue::as_long),
        Some(500)
    );
}

#[test]
fn rebuilding_the_brain_keeps_the_persisted_memories() {
    let mut ai = MobAi::new(EntityKind::Villager, 1, GoalSettings::default());
    let pos = BlockPos::new(1, 64, 1);
    ai.behavior_brain_mut().unwrap().memories_mut().set(
        MemoryModuleType::JobSite,
        MemoryValue::GlobalPos(rspigot_ai::GlobalPos {
            dimension: "minecraft:overworld".to_owned(),
            pos,
        }),
    );
    ai.rebuild_villager_brain(Profession::Librarian, false);
    assert_eq!(
        ai.behavior_brain()
            .unwrap()
            .memories()
            .get(MemoryModuleType::JobSite)
            .and_then(MemoryValue::as_global_pos)
            .map(|global| global.pos),
        Some(pos)
    );
}

#[test]
fn the_nearest_bed_sensor_only_runs_for_babies() {
    let mut world = Field::flat();
    world
        .pois
        .push(PoiRecord::new(BlockPos::new(3, 64, 0), PoiType::Home));
    let mut adult = Bench::new(VillagerFacts::default(), world, 3);
    adult.tick_sensor(SensorKind::NearestBed(
        rspigot_ai::NearestBedSensor::default(),
    ));
    assert!(adult.get(MemoryModuleType::NearestBed).is_none());
    let mut world = Field::flat();
    world
        .pois
        .push(PoiRecord::new(BlockPos::new(3, 64, 0), PoiType::Home));
    let mut baby = Bench::at(
        Vec3::new(0.5, 64.0, 0.5),
        VillagerFacts::default(),
        world,
        3,
        true,
    );
    baby.tick_sensor(SensorKind::NearestBed(
        rspigot_ai::NearestBedSensor::default(),
    ));
    assert_eq!(
        baby.get(MemoryModuleType::NearestBed)
            .and_then(MemoryValue::as_block),
        Some(BlockPos::new(3, 64, 0))
    );
}

#[test]
fn the_secondary_poi_sensor_lists_farmland_for_farmers_only() {
    let mut world = Field::flat();
    world.put(2, 63, 1, BlockKind::Farmland.default_state());
    let mut farmer_bench = Bench::new(farmer(), world, 3);
    farmer_bench.tick_sensor(SensorKind::SecondaryPois(rspigot_ai::SecondaryPoiSensor));
    let found = match farmer_bench.get(MemoryModuleType::SecondaryJobSite) {
        Some(MemoryValue::GlobalPosList(list)) => list.iter().map(|global| global.pos).collect(),
        _ => Vec::new(),
    };
    assert_eq!(found, vec![BlockPos::new(2, 63, 1)]);
    let mut world = Field::flat();
    world.put(2, 63, 1, BlockKind::Farmland.default_state());
    let mut other = Bench::new(VillagerFacts::default(), world, 3);
    other.tick_sensor(SensorKind::SecondaryPois(rspigot_ai::SecondaryPoiSensor));
    assert!(other.get(MemoryModuleType::SecondaryJobSite).is_none());
}

#[test]
fn the_hostile_sensor_uses_a_distance_per_monster() {
    let mut bench = Bench::new(VillagerFacts::default(), Field::flat(), 3);
    let near_zombie = living(7, EntityKind::Zombie, Vec3::new(5.5, 64.0, 0.5));
    let far_zombie = living(8, EntityKind::Zombie, Vec3::new(9.5, 64.0, 0.5));
    let pillager = living(9, EntityKind::Pillager, Vec3::new(14.5, 64.0, 0.5));
    bench.see(&[near_zombie, far_zombie, pillager]);
    bench.tick_sensor(SensorKind::VillagerHostiles(
        rspigot_ai::VillagerHostilesSensor,
    ));
    assert_eq!(
        bench
            .get(MemoryModuleType::NearestHostile)
            .and_then(MemoryValue::as_entity),
        Some(EntityId(7))
    );
    let mut bench = Bench::new(VillagerFacts::default(), Field::flat(), 3);
    bench.see(&[far_zombie]);
    bench.tick_sensor(SensorKind::VillagerHostiles(
        rspigot_ai::VillagerHostilesSensor,
    ));
    assert!(bench.get(MemoryModuleType::NearestHostile).is_none());
    let mut bench = Bench::new(VillagerFacts::default(), Field::flat(), 3);
    bench.see(&[pillager]);
    bench.tick_sensor(SensorKind::VillagerHostiles(
        rspigot_ai::VillagerHostilesSensor,
    ));
    assert_eq!(
        bench
            .get(MemoryModuleType::NearestHostile)
            .and_then(MemoryValue::as_entity),
        Some(EntityId(9))
    );
}

#[test]
fn the_golem_sensor_remembers_a_golem_for_599_ticks() {
    let mut bench = Bench::new(VillagerFacts::default(), Field::flat(), 3);
    let golem = living(5, EntityKind::IronGolem, Vec3::new(4.5, 64.0, 0.5));
    bench.world.others.push(golem);
    bench.set(
        MemoryModuleType::NearestLivingEntities,
        MemoryValue::Entities(vec![EntityId(5)]),
    );
    bench.tick_sensor(SensorKind::GolemDetected(rspigot_ai::GolemSensor));
    assert_eq!(
        bench
            .state
            .memories
            .time_until_expiry(MemoryModuleType::GolemDetectedRecently),
        Some(599)
    );
    assert!(
        bench
            .state
            .memories
            .has_value(MemoryModuleType::GolemDetectedRecently)
    );
}

#[test]
fn the_babies_sensor_lists_visible_baby_villagers() {
    let mut bench = Bench::new(VillagerFacts::default(), Field::flat(), 3);
    let mut baby = villager_at(4, Vec3::new(2.5, 64.0, 0.5), VillagerPeerFacts::default());
    baby.baby = true;
    let adult = villager_at(5, Vec3::new(3.5, 64.0, 0.5), VillagerPeerFacts::default());
    bench.see(&[baby, adult]);
    bench.tick_sensor(SensorKind::VillagerBabies(rspigot_ai::VillagerBabiesSensor));
    assert_eq!(
        bench
            .get(MemoryModuleType::VisibleVillagerBabies)
            .and_then(MemoryValue::as_entities),
        Some(&[EntityId(4)][..])
    );
}

fn trigger(trigger: rspigot_ai::MobTrigger) -> MobControl {
    MobControl::one_shot(trigger)
}

fn run_until(
    bench: &mut Bench,
    control: &mut MobControl,
    limit: i64,
    done: impl Fn(&Bench) -> bool,
) -> bool {
    for now in (NOW..).take(limit as usize) {
        bench.world.time = now;
        if bench.start(control, now) {
            bench.tick(control, now);
        }
        if done(bench) {
            return true;
        }
    }
    false
}

#[test]
fn acquiring_a_job_site_takes_the_nearest_free_poi_and_remembers_it() {
    let mut world = Field::flat();
    world
        .pois
        .push(PoiRecord::new(BlockPos::new(6, 64, 0), PoiType::Librarian));
    world
        .pois
        .push(PoiRecord::new(BlockPos::new(3, 64, 0), PoiType::Cleric));
    world
        .pois
        .push(PoiRecord::new(BlockPos::new(2, 64, 2), PoiType::Home));
    let mut bench = Bench::new(VillagerFacts::default(), world, 4);
    let mut control = trigger(rspigot_ai::MobTrigger::AcquirePoi(
        rspigot_ai::AcquirePoi::new(
            rspigot_ai::PoiRule::AcquirableJobSite,
            MemoryModuleType::JobSite,
            MemoryModuleType::PotentialJobSite,
            true,
            None,
        ),
    ));
    assert!(run_until(&mut bench, &mut control, 120, |bench| bench
        .get(MemoryModuleType::PotentialJobSite)
        .is_some()));
    let taken = bench
        .get(MemoryModuleType::PotentialJobSite)
        .and_then(MemoryValue::as_global_pos)
        .map(|global| global.pos);
    assert_eq!(taken, Some(BlockPos::new(3, 64, 0)));
    assert!(
        bench
            .villager_actions()
            .contains(&VillagerAction::TakePoi(BlockPos::new(3, 64, 0)))
    );
}

#[test]
fn babies_never_acquire_a_job_site_and_occupied_pois_are_skipped() {
    let mut world = Field::flat();
    world
        .pois
        .push(PoiRecord::new(BlockPos::new(3, 64, 0), PoiType::Cleric));
    let mut baby = Bench::at(
        Vec3::new(0.5, 64.0, 0.5),
        VillagerFacts::default(),
        world,
        4,
        true,
    );
    let mut control = trigger(rspigot_ai::MobTrigger::AcquirePoi(
        rspigot_ai::AcquirePoi::new(
            rspigot_ai::PoiRule::AcquirableJobSite,
            MemoryModuleType::JobSite,
            MemoryModuleType::PotentialJobSite,
            true,
            None,
        ),
    ));
    assert!(!run_until(&mut baby, &mut control, 60, |bench| bench
        .get(MemoryModuleType::PotentialJobSite)
        .is_some()));
}

#[test]
fn a_librarian_only_competes_for_its_own_job_site_type() {
    let mut world = Field::flat();
    world
        .pois
        .push(PoiRecord::new(BlockPos::new(3, 64, 0), PoiType::Cleric));
    let facts = VillagerFacts {
        profession: Profession::Librarian,
        ..VillagerFacts::default()
    };
    let mut bench = Bench::new(facts, world, 4);
    let mut control = trigger(rspigot_ai::MobTrigger::AcquirePoi(
        rspigot_ai::AcquirePoi::new(
            rspigot_ai::PoiRule::AcquirableJobSite,
            MemoryModuleType::JobSite,
            MemoryModuleType::PotentialJobSite,
            true,
            None,
        ),
    ));
    assert!(!run_until(&mut bench, &mut control, 60, |bench| bench
        .get(MemoryModuleType::PotentialJobSite)
        .is_some()));
}

#[test]
fn the_competitor_with_less_experience_loses_the_job_site() {
    let site = BlockPos::new(3, 64, 0);
    let mut world = Field::flat();
    world.pois.push(PoiRecord::new(site, PoiType::Librarian));
    let facts = VillagerFacts {
        profession: Profession::Librarian,
        xp: 10,
        ..VillagerFacts::default()
    };
    let mut bench = Bench::new(facts, world, 4);
    let global = bench.global(site);
    bench.set(MemoryModuleType::JobSite, global);
    bench.set(
        MemoryModuleType::NearestLivingEntities,
        MemoryValue::Entities(vec![EntityId(5)]),
    );
    let rival = villager_at(
        5,
        Vec3::new(2.5, 64.0, 0.5),
        VillagerPeerFacts {
            profession: Profession::Librarian,
            xp: 70,
            job_site: Some(site),
            ..VillagerPeerFacts::default()
        },
    );
    bench.world.others.push(rival);
    let mut control = trigger(rspigot_ai::MobTrigger::PoiCompetitorScan(
        rspigot_ai::PoiCompetitorScan,
    ));
    assert!(bench.start(&mut control, NOW));
    assert!(bench.get(MemoryModuleType::JobSite).is_none());
}

#[test]
fn the_competitor_with_more_experience_keeps_it_and_evicts_the_other() {
    let site = BlockPos::new(3, 64, 0);
    let mut world = Field::flat();
    world.pois.push(PoiRecord::new(site, PoiType::Librarian));
    let facts = VillagerFacts {
        profession: Profession::Librarian,
        xp: 70,
        ..VillagerFacts::default()
    };
    let mut bench = Bench::new(facts, world, 4);
    let global = bench.global(site);
    bench.set(MemoryModuleType::JobSite, global);
    bench.set(
        MemoryModuleType::NearestLivingEntities,
        MemoryValue::Entities(vec![EntityId(5)]),
    );
    let rival = villager_at(
        5,
        Vec3::new(2.5, 64.0, 0.5),
        VillagerPeerFacts {
            profession: Profession::Librarian,
            xp: 10,
            job_site: Some(site),
            ..VillagerPeerFacts::default()
        },
    );
    bench.world.others.push(rival);
    let mut control = trigger(rspigot_ai::MobTrigger::PoiCompetitorScan(
        rspigot_ai::PoiCompetitorScan,
    ));
    assert!(bench.start(&mut control, NOW));
    assert!(bench.get(MemoryModuleType::JobSite).is_some());
    assert!(bench.actions().iter().any(|action| matches!(
        action,
        MobAction::SetMemory { mob, memory: MemoryModuleType::JobSite, value: None } if *mob == EntityId(5)
    )));
}

#[test]
fn reaching_the_potential_job_site_assigns_the_profession_of_its_poi() {
    let site = BlockPos::new(1, 64, 0);
    let mut world = Field::flat();
    world.pois.push(PoiRecord::new(site, PoiType::Cleric));
    let mut bench = Bench::new(VillagerFacts::default(), world, 4);
    let global = bench.global(site);
    bench.set(MemoryModuleType::PotentialJobSite, global);
    let mut control = trigger(rspigot_ai::MobTrigger::AssignProfessionFromJobSite(
        rspigot_ai::AssignProfessionFromJobSite,
    ));
    assert!(bench.start(&mut control, NOW));
    assert!(bench.get(MemoryModuleType::PotentialJobSite).is_none());
    assert!(bench.get(MemoryModuleType::JobSite).is_some());
    let actions = bench.villager_actions();
    assert_eq!(
        actions,
        vec![VillagerAction::SetProfession(Profession::Cleric)]
    );
}

#[test]
fn a_far_potential_job_site_is_not_assigned_yet() {
    let site = BlockPos::new(6, 64, 0);
    let mut world = Field::flat();
    world.pois.push(PoiRecord::new(site, PoiType::Cleric));
    let mut bench = Bench::new(VillagerFacts::default(), world, 4);
    let global = bench.global(site);
    bench.set(MemoryModuleType::PotentialJobSite, global);
    let mut control = trigger(rspigot_ai::MobTrigger::AssignProfessionFromJobSite(
        rspigot_ai::AssignProfessionFromJobSite,
    ));
    assert!(!bench.start(&mut control, NOW));
    assert!(bench.get(MemoryModuleType::PotentialJobSite).is_some());
}

#[test]
fn an_unemployed_novice_without_a_job_site_gives_up_its_profession() {
    let facts = VillagerFacts {
        profession: Profession::Mason,
        level: 1,
        xp: 0,
        ..VillagerFacts::default()
    };
    let mut bench = Bench::new(facts, Field::flat(), 4);
    let mut control = trigger(rspigot_ai::MobTrigger::ResetProfession(
        rspigot_ai::ResetProfession,
    ));
    assert!(bench.start(&mut control, NOW));
    assert_eq!(
        bench.villager_actions(),
        vec![VillagerAction::SetProfession(Profession::None)]
    );
    let experienced = VillagerFacts { xp: 5, ..facts };
    let mut bench = Bench::new(experienced, Field::flat(), 4);
    let mut control = trigger(rspigot_ai::MobTrigger::ResetProfession(
        rspigot_ai::ResetProfession,
    ));
    assert!(bench.start(&mut control, NOW));
    assert!(bench.villager_actions().is_empty());
}

#[test]
fn a_poi_validator_forgets_beds_taken_by_someone_else_and_releases_the_ticket() {
    let bed = BlockPos::new(2, 64, 0);
    let mut world = Field::flat();
    world.pois.push(PoiRecord::new(bed, PoiType::Home));
    world.put(
        2,
        64,
        0,
        BlockKind::RedBed
            .default_state()
            .with_property("part", "head")
            .unwrap()
            .with_property("occupied", "true")
            .unwrap(),
    );
    let mut bench = Bench::new(VillagerFacts::default(), world, 4);
    let global = bench.global(bed);
    bench.set(MemoryModuleType::Home, global);
    let mut control = trigger(rspigot_ai::MobTrigger::ValidateNearbyPoi(
        rspigot_ai::ValidateNearbyPoi::new(rspigot_ai::PoiRule::Home, MemoryModuleType::Home),
    ));
    assert!(bench.start(&mut control, NOW));
    assert!(bench.get(MemoryModuleType::Home).is_none());
    assert_eq!(
        bench.villager_actions(),
        vec![VillagerAction::ReleasePoi(bed)]
    );
}

#[test]
fn a_poi_validator_forgets_a_vanished_poi_without_releasing_anything() {
    let mut bench = Bench::new(VillagerFacts::default(), Field::flat(), 4);
    let global = bench.global(BlockPos::new(2, 64, 0));
    bench.set(MemoryModuleType::Home, global);
    let mut control = trigger(rspigot_ai::MobTrigger::ValidateNearbyPoi(
        rspigot_ai::ValidateNearbyPoi::new(rspigot_ai::PoiRule::Home, MemoryModuleType::Home),
    ));
    assert!(bench.start(&mut control, NOW));
    assert!(bench.get(MemoryModuleType::Home).is_none());
    assert!(bench.villager_actions().is_empty());
}

#[test]
fn a_block_memory_walk_goes_to_a_near_home_and_gives_up_after_the_timeout() {
    let home = BlockPos::new(5, 64, 0);
    let mut bench = Bench::new(VillagerFacts::default(), Field::flat(), 4);
    let global = bench.global(home);
    bench.set(MemoryModuleType::Home, global);
    let mut control = trigger(rspigot_ai::MobTrigger::SetWalkTargetFromBlockMemory(
        rspigot_ai::SetWalkTargetFromBlockMemory::new(MemoryModuleType::Home, 0.5, 1, 150, 1200),
    ));
    assert!(bench.start(&mut control, NOW));
    let target: WalkTarget = *bench
        .get(MemoryModuleType::WalkTarget)
        .and_then(MemoryValue::as_walk_target)
        .unwrap();
    assert_eq!(target.close_enough_dist, 1);
    assert_eq!(target.speed_modifier, 0.5);
    bench.state.memories.erase(MemoryModuleType::WalkTarget);
    bench.set(
        MemoryModuleType::CantReachWalkTargetSince,
        MemoryValue::Long(NOW - 5_000),
    );
    let mut control = trigger(rspigot_ai::MobTrigger::SetWalkTargetFromBlockMemory(
        rspigot_ai::SetWalkTargetFromBlockMemory::new(MemoryModuleType::Home, 0.5, 1, 150, 1200),
    ));
    assert!(bench.start(&mut control, NOW));
    assert!(bench.get(MemoryModuleType::Home).is_none());
    assert_eq!(
        bench
            .get(MemoryModuleType::CantReachWalkTargetSince)
            .and_then(MemoryValue::as_long),
        Some(NOW)
    );
}

#[test]
fn a_villager_falls_asleep_in_a_free_bed_beside_it_and_wakes_when_resting_ends() {
    let bed = BlockPos::new(0, 64, 0);
    let mut world = Field::flat();
    world.put(
        0,
        64,
        0,
        BlockKind::RedBed
            .default_state()
            .with_property("part", "head")
            .unwrap(),
    );
    let mut bench = Bench::at(
        Vec3::new(0.5, 64.6, 0.5),
        VillagerFacts::default(),
        world,
        4,
        false,
    );
    let home = bench.global(bed);
    bench.set(MemoryModuleType::Home, home);
    bench.state.set_active_activity_if_possible(Activity::Idle);
    let mut sleep = MobControl::behavior(rspigot_ai::MobBehavior::SleepInBed(
        rspigot_ai::SleepInBed::new(),
    ));
    assert!(bench.start(&mut sleep, NOW));
    assert!(
        bench
            .villager_actions()
            .contains(&VillagerAction::StartSleeping(bed))
    );
    assert_eq!(
        bench
            .get(MemoryModuleType::LastSlept)
            .and_then(MemoryValue::as_long),
        Some(NOW)
    );
    let mut wake = trigger(rspigot_ai::MobTrigger::WakeUp(rspigot_ai::WakeUp));
    bench.mob.species.villager.sleeping = true;
    assert!(bench.start(&mut wake, NOW + 5));
    assert!(
        bench
            .villager_actions()
            .contains(&VillagerAction::StopSleeping)
    );
}

#[test]
fn a_villager_closes_the_doors_it_opened_before_going_to_sleep() {
    let bed = BlockPos::new(0, 64, 0);
    let door = BlockPos::new(1, 64, 1);
    let mut world = Field::flat();
    world.put(
        0,
        64,
        0,
        BlockKind::RedBed
            .default_state()
            .with_property("part", "head")
            .unwrap(),
    );
    world.put(
        1,
        64,
        1,
        BlockKind::OakDoor
            .default_state()
            .with_property("half", "lower")
            .unwrap()
            .with_property("open", "true")
            .unwrap(),
    );
    let mut bench = Bench::at(
        Vec3::new(0.5, 64.6, 0.5),
        VillagerFacts::default(),
        world,
        4,
        false,
    );
    let home = bench.global(bed);
    bench.set(MemoryModuleType::Home, home);
    let doors = bench.global(door);
    let MemoryValue::GlobalPos(doors) = doors else {
        unreachable!()
    };
    bench.set(
        MemoryModuleType::DoorsToClose,
        MemoryValue::GlobalPosList(vec![doors]),
    );
    bench.state.set_active_activity_if_possible(Activity::Idle);
    let mut sleep = MobControl::behavior(rspigot_ai::MobBehavior::SleepInBed(
        rspigot_ai::SleepInBed::new(),
    ));
    assert!(bench.start(&mut sleep, NOW));
    let actions = bench.actions();
    assert!(actions.contains(&MobAction::SetDoorOpen {
        pos: door,
        open: false,
        interact: false,
    }));
    assert!(actions.contains(&MobAction::Villager(VillagerAction::StartSleeping(bed))));
    assert!(bench.get(MemoryModuleType::DoorsToClose).is_none());
}

#[test]
fn a_villager_does_not_go_back_to_bed_right_after_waking() {
    let bed = BlockPos::new(0, 64, 0);
    let mut world = Field::flat();
    world.put(
        0,
        64,
        0,
        BlockKind::RedBed
            .default_state()
            .with_property("part", "head")
            .unwrap(),
    );
    let mut bench = Bench::at(
        Vec3::new(0.5, 64.6, 0.5),
        VillagerFacts::default(),
        world,
        4,
        false,
    );
    let home = bench.global(bed);
    bench.set(MemoryModuleType::Home, home);
    bench.set(MemoryModuleType::LastWoken, MemoryValue::Long(NOW - 50));
    let mut sleep = MobControl::behavior(rspigot_ai::MobBehavior::SleepInBed(
        rspigot_ai::SleepInBed::new(),
    ));
    assert!(!bench.start(&mut sleep, NOW));
}

#[test]
fn the_panic_trigger_forgets_what_the_villager_was_doing_when_a_hostile_shows_up() {
    let mut bench = Bench::new(VillagerFacts::default(), Field::flat(), 4);
    bench.set(
        MemoryModuleType::NearestHostile,
        MemoryValue::Entity(EntityId(9)),
    );
    bench.set(
        MemoryModuleType::WalkTarget,
        MemoryValue::WalkTarget(WalkTarget::block(BlockPos::new(1, 64, 1), 0.5, 1)),
    );
    bench.set(
        MemoryModuleType::InteractionTarget,
        MemoryValue::Entity(EntityId(4)),
    );
    let mut panic = MobControl::behavior(rspigot_ai::MobBehavior::VillagerPanicTrigger(
        rspigot_ai::VillagerPanicTrigger,
    ));
    assert!(bench.start(&mut panic, NOW));
    assert!(bench.get(MemoryModuleType::WalkTarget).is_none());
    assert!(bench.get(MemoryModuleType::InteractionTarget).is_none());
    bench.state.memories.erase(MemoryModuleType::NearestHostile);
    bench.tick(&mut panic, NOW + 1);
    assert_eq!(panic.status(), Status::Stopped);
}

#[test]
fn panicking_every_hundred_ticks_asks_for_a_golem() {
    let mut bench = Bench::new(VillagerFacts::default(), Field::flat(), 4);
    bench.set(
        MemoryModuleType::NearestHostile,
        MemoryValue::Entity(EntityId(9)),
    );
    let mut panic = MobControl::behavior(rspigot_ai::MobBehavior::VillagerPanicTrigger(
        rspigot_ai::VillagerPanicTrigger,
    ));
    bench.world.time = 1_100;
    assert!(bench.start(&mut panic, 1_100));
    bench.tick(&mut panic, 1_100);
    assert!(bench.villager_actions().iter().any(|action| matches!(
        action,
        VillagerAction::SpawnGolem {
            villagers_needed: 3,
            ..
        }
    )));
    bench.tick(&mut panic, 1_101);
    assert!(bench.villager_actions().is_empty());
}

#[test]
fn a_bell_without_a_raid_runs_the_hide_reaction() {
    let mut bench = Bench::new(VillagerFacts::default(), Field::flat(), 4);
    bench.state.set_core_activities([Activity::Core]);
    bench.set(MemoryModuleType::HeardBellTime, MemoryValue::Long(NOW));
    let mut react = trigger(rspigot_ai::MobTrigger::ReactToBell(rspigot_ai::ReactToBell));
    assert!(bench.start(&mut react, NOW));
    assert!(bench.get(MemoryModuleType::HeardBellTime).is_some());
}

#[test]
fn raid_status_without_a_raid_is_a_no_op_and_the_reset_goes_back_to_idle() {
    let mut bench = Bench::new(VillagerFacts::default(), Field::flat(), 4);
    let mut set = trigger(rspigot_ai::MobTrigger::SetRaidStatus(
        rspigot_ai::SetRaidStatus,
    ));
    let mut started = false;
    for tick in 0..200 {
        if bench.start(&mut set, NOW + tick) {
            started = true;
            bench.tick(&mut set, NOW + tick);
        }
    }
    assert!(started);
    let mut reset = trigger(rspigot_ai::MobTrigger::ResetRaidStatus(
        rspigot_ai::ResetRaidStatus,
    ));
    let mut ran = false;
    for tick in 0..200 {
        if bench.start(&mut reset, NOW + tick) {
            ran = true;
            bench.tick(&mut reset, NOW + tick);
        }
    }
    assert!(ran);
}

#[test]
fn two_breedable_villagers_share_food_and_the_baby_takes_a_free_bed() {
    let mut world = Field::flat();
    world
        .pois
        .push(PoiRecord::new(BlockPos::new(3, 64, 1), PoiType::Home));
    let facts = VillagerFacts {
        can_breed: true,
        ..VillagerFacts::default()
    };
    let mut bench = Bench::new(facts, world, 4);
    let partner = villager_at(
        2,
        Vec3::new(1.5, 64.0, 0.5),
        VillagerPeerFacts {
            can_breed: true,
            ..VillagerPeerFacts::default()
        },
    );
    bench.see(&[partner]);
    bench.set(
        MemoryModuleType::BreedTarget,
        MemoryValue::Entity(EntityId(2)),
    );
    let mut love = MobControl::behavior_for(
        rspigot_ai::MobBehavior::VillagerMakeLove(rspigot_ai::VillagerMakeLove::new()),
        350,
        350,
    );
    assert!(bench.start(&mut love, NOW));
    let events = bench.actions();
    assert!(
        events
            .iter()
            .any(|action| matches!(action, MobAction::EntityEvent(18)))
    );
    let mut breed = None;
    for tick in 1..340 {
        bench.world.time = NOW + tick;
        bench.tick(&mut love, NOW + tick);
        for action in bench.villager_actions() {
            if let VillagerAction::Breed { partner, bed } = action {
                breed = Some((partner, bed));
            }
        }
        if breed.is_some() {
            break;
        }
    }
    assert_eq!(breed, Some((EntityId(2), BlockPos::new(3, 64, 1))));
}

#[test]
fn breeding_without_a_free_bed_only_makes_the_pair_angry() {
    let facts = VillagerFacts {
        can_breed: true,
        ..VillagerFacts::default()
    };
    let mut bench = Bench::new(facts, Field::flat(), 4);
    let partner = villager_at(
        2,
        Vec3::new(1.5, 64.0, 0.5),
        VillagerPeerFacts {
            can_breed: true,
            ..VillagerPeerFacts::default()
        },
    );
    bench.see(&[partner]);
    bench.set(
        MemoryModuleType::BreedTarget,
        MemoryValue::Entity(EntityId(2)),
    );
    let mut love = MobControl::behavior_for(
        rspigot_ai::MobBehavior::VillagerMakeLove(rspigot_ai::VillagerMakeLove::new()),
        350,
        350,
    );
    assert!(bench.start(&mut love, NOW));
    let mut angry = false;
    for tick in 1..340 {
        bench.world.time = NOW + tick;
        bench.tick(&mut love, NOW + tick);
        let actions = bench.villager_actions();
        assert!(
            !actions
                .iter()
                .any(|action| matches!(action, VillagerAction::Breed { .. }))
        );
        angry |= actions
            .iter()
            .any(|action| matches!(action, VillagerAction::PartnerEvent { event: 13, .. }));
    }
    assert!(angry);
}

#[test]
fn a_villager_with_excess_bread_throws_half_a_stack_to_a_hungry_neighbour() {
    let mut facts = VillagerFacts {
        food_points: 160,
        ..VillagerFacts::default()
    };
    facts.inventory[0] = Some(StackFacts {
        item: ItemKind::Bread.id(),
        count: 40,
    });
    let mut bench = Bench::new(facts, Field::flat(), 4);
    let partner = villager_at(
        2,
        Vec3::new(1.5, 64.0, 0.5),
        VillagerPeerFacts {
            wants_more_food: true,
            ..VillagerPeerFacts::default()
        },
    );
    bench.see(&[partner]);
    bench.set(
        MemoryModuleType::InteractionTarget,
        MemoryValue::Entity(EntityId(2)),
    );
    let mut trade = MobControl::behavior(rspigot_ai::MobBehavior::TradeWithVillager(
        rspigot_ai::TradeWithVillager::new(),
    ));
    assert!(bench.start(&mut trade, NOW));
    bench.tick(&mut trade, NOW);
    let actions = bench.villager_actions();
    assert!(actions.contains(&VillagerAction::Gossip(EntityId(2))));
    assert!(actions.iter().any(|action| matches!(
        action,
        VillagerAction::ThrowSlot {
            slot: 0,
            amount: 20,
            ..
        }
    )));
    assert_eq!(trade.status(), Status::Running);
}

#[test]
fn a_farmer_gives_the_surplus_wheat_to_its_neighbours() {
    let mut facts = farmer();
    facts.wheat = 50;
    facts.inventory[2] = Some(StackFacts {
        item: ItemKind::Wheat.id(),
        count: 50,
    });
    let mut bench = Bench::new(facts, Field::flat(), 4);
    let partner = villager_at(2, Vec3::new(1.5, 64.0, 0.5), VillagerPeerFacts::default());
    bench.see(&[partner]);
    bench.set(
        MemoryModuleType::InteractionTarget,
        MemoryValue::Entity(EntityId(2)),
    );
    let mut trade = MobControl::behavior(rspigot_ai::MobBehavior::TradeWithVillager(
        rspigot_ai::TradeWithVillager::new(),
    ));
    assert!(bench.start(&mut trade, NOW));
    bench.tick(&mut trade, NOW);
    assert!(bench.villager_actions().iter().any(|action| matches!(
        action,
        VillagerAction::ThrowSlot {
            slot: 2,
            amount: 25,
            ..
        }
    )));
}

#[test]
fn working_at_the_job_site_plays_the_sound_restocks_and_looks_at_it() {
    let site = BlockPos::new(0, 64, 1);
    let mut world = Field::flat();
    world.pois.push(PoiRecord::new(site, PoiType::Librarian));
    let facts = VillagerFacts {
        profession: Profession::Librarian,
        ..VillagerFacts::default()
    };
    let mut bench = Bench::new(facts, world, 4);
    let global = bench.global(site);
    bench.set(MemoryModuleType::JobSite, global);
    let mut work = MobControl::behavior(rspigot_ai::MobBehavior::WorkAtPoi(
        rspigot_ai::WorkAtPoi::new(rspigot_ai::Workstation::Plain),
    ));
    let mut started = false;
    for tick in 0..2_000 {
        bench.world.time = NOW + tick;
        if bench.start(&mut work, NOW + tick) {
            started = true;
            break;
        }
    }
    assert!(started);
    let actions = bench.villager_actions();
    assert!(actions.contains(&VillagerAction::PlayWorkSound));
    assert!(actions.contains(&VillagerAction::Restock));
    assert!(bench.get(MemoryModuleType::LastWorkedAtPoi).is_some());
    assert!(bench.get(MemoryModuleType::LookTarget).is_some());
}

#[test]
fn a_farmer_harvests_ripe_wheat_and_replants_seeds() {
    let mut world = Field::flat();
    world.put(1, 63, 0, BlockKind::Farmland.default_state());
    world.put(
        1,
        64,
        0,
        BlockKind::Wheat
            .default_state()
            .with_property("age", "7")
            .unwrap(),
    );
    world.put(0, 63, 1, BlockKind::Farmland.default_state());
    let mut facts = farmer();
    facts.has_farm_seeds = true;
    facts.inventory[1] = Some(StackFacts {
        item: ItemKind::WheatSeeds.id(),
        count: 4,
    });
    let mut bench = Bench::at(Vec3::new(1.5, 64.0, 0.5), facts, world, 4, false);
    bench.set(
        MemoryModuleType::SecondaryJobSite,
        MemoryValue::GlobalPosList(vec![rspigot_ai::GlobalPos {
            dimension: "minecraft:overworld".to_owned(),
            pos: BlockPos::new(1, 63, 0),
        }]),
    );
    let mut harvest = MobControl::behavior(rspigot_ai::MobBehavior::HarvestFarmland(
        rspigot_ai::HarvestFarmland::new(),
    ));
    let mut harvested = false;
    let mut planted = false;
    for tick in 0..60 {
        bench.world.time = NOW + tick;
        if tick == 0 {
            bench.start(&mut harvest, NOW);
        }
        bench.tick(&mut harvest, NOW + tick);
        for action in bench.villager_actions() {
            match action {
                VillagerAction::HarvestCrop(pos) => {
                    assert_eq!(pos, BlockPos::new(1, 64, 0));
                    harvested = true;
                }
                VillagerAction::PlantSeed { slot: 1, .. } => planted = true,
                _ => {}
            }
        }
    }
    assert!(harvested);
    assert!(planted);
}

#[test]
fn showing_trades_displays_the_offers_the_player_can_pay_for_and_clears_afterwards() {
    let mut world = Field::flat();
    world.trades = vec![
        TradeView {
            cost_a: ItemKind::Emerald.id(),
            cost_b: None,
            out_of_stock: false,
        },
        TradeView {
            cost_a: ItemKind::Paper.id(),
            cost_b: None,
            out_of_stock: false,
        },
        TradeView {
            cost_a: ItemKind::Emerald.id(),
            cost_b: Some(ItemKind::Book.id()),
            out_of_stock: true,
        },
    ];
    let mut player = living(3, EntityKind::Player, Vec3::new(1.5, 64.0, 0.5));
    player.main_hand = Some(ItemKind::Emerald.id());
    world.players.push(player);
    let mut bench = Bench::new(VillagerFacts::default(), world, 4);
    bench.set(
        MemoryModuleType::InteractionTarget,
        MemoryValue::Entity(EntityId(3)),
    );
    let mut show = MobControl::behavior_for(
        rspigot_ai::MobBehavior::ShowTradesToPlayer(rspigot_ai::ShowTradesToPlayer::new()),
        400,
        1600,
    );
    assert!(bench.start(&mut show, NOW));
    bench.tick(&mut show, NOW);
    let shown = bench
        .villager_actions()
        .into_iter()
        .filter_map(|action| match action {
            VillagerAction::ShowTrade(index) => Some(index),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(shown, vec![Some(0)]);
}

#[test]
fn a_hero_receives_a_gift_after_a_short_walk_and_the_gift_timer_resets() {
    let mut world = Field::flat();
    let mut hero = living(3, EntityKind::Player, Vec3::new(2.5, 64.0, 0.5));
    hero.facts.hero_of_the_village = true;
    world.players.push(hero);
    let mut bench = Bench::new(VillagerFacts::default(), world, 4);
    bench.set(
        MemoryModuleType::NearestVisiblePlayer,
        MemoryValue::Entity(EntityId(3)),
    );
    let mut gift = MobControl::behavior_for(
        rspigot_ai::MobBehavior::GiveGiftToHero(rspigot_ai::GiveGiftToHero::new()),
        100,
        100,
    );
    let mut given = false;
    for tick in 0..700 {
        bench.world.time = NOW + tick;
        if bench.start(&mut gift, NOW + tick) {
            for step in 0..40 {
                bench.world.time = NOW + tick + step;
                bench.tick(&mut gift, NOW + tick + step);
                if bench
                    .villager_actions()
                    .contains(&VillagerAction::GiveGift(EntityId(3)))
                {
                    given = true;
                    break;
                }
            }
            break;
        }
    }
    assert!(given);
}

#[test]
fn a_villager_without_a_hero_in_sight_never_starts_the_gift() {
    let mut world = Field::flat();
    world
        .players
        .push(living(3, EntityKind::Player, Vec3::new(2.5, 64.0, 0.5)));
    let mut bench = Bench::new(VillagerFacts::default(), world, 4);
    bench.set(
        MemoryModuleType::NearestVisiblePlayer,
        MemoryValue::Entity(EntityId(3)),
    );
    let mut gift = MobControl::behavior_for(
        rspigot_ai::MobBehavior::GiveGiftToHero(rspigot_ai::GiveGiftToHero::new()),
        100,
        100,
    );
    for tick in 0..1_000 {
        assert!(!bench.start(&mut gift, NOW + tick));
    }
}

#[test]
fn the_trading_player_is_followed_and_looked_at() {
    let mut world = Field::flat();
    world
        .players
        .push(living(3, EntityKind::Player, Vec3::new(2.5, 64.0, 0.5)));
    let facts = VillagerFacts {
        trading_player: Some(EntityId(3)),
        ..VillagerFacts::default()
    };
    let mut bench = Bench::new(facts, world, 4);
    let mut follow = MobControl::behavior(rspigot_ai::MobBehavior::LookAndFollowTradingPlayerSink(
        rspigot_ai::LookAndFollowTradingPlayerSink::new(0.5),
    ));
    assert!(bench.start(&mut follow, NOW));
    let walk = *bench
        .get(MemoryModuleType::WalkTarget)
        .and_then(MemoryValue::as_walk_target)
        .unwrap();
    assert_eq!(walk.close_enough_dist, 2);
    assert!(bench.get(MemoryModuleType::LookTarget).is_some());
}

#[test]
fn villagers_hurt_recently_stop_following_the_trading_player() {
    let mut world = Field::flat();
    world
        .players
        .push(living(3, EntityKind::Player, Vec3::new(2.5, 64.0, 0.5)));
    let facts = VillagerFacts {
        trading_player: Some(EntityId(3)),
        hurt_recently: true,
        ..VillagerFacts::default()
    };
    let mut bench = Bench::new(facts, world, 4);
    let mut follow = MobControl::behavior(rspigot_ai::MobBehavior::LookAndFollowTradingPlayerSink(
        rspigot_ai::LookAndFollowTradingPlayerSink::new(0.5),
    ));
    assert!(!bench.start(&mut follow, NOW));
}

#[test]
fn only_one_potential_poi_is_never_claimed_twice_by_the_same_villager() {
    let mut world = Field::flat();
    world
        .pois
        .push(PoiRecord::new(BlockPos::new(3, 64, 0), PoiType::Cleric));
    let mut bench = Bench::new(VillagerFacts::default(), world, 4);
    let mut control = trigger(rspigot_ai::MobTrigger::AcquirePoi(
        rspigot_ai::AcquirePoi::new(
            rspigot_ai::PoiRule::AcquirableJobSite,
            MemoryModuleType::JobSite,
            MemoryModuleType::PotentialJobSite,
            true,
            None,
        ),
    ));
    run_until(&mut bench, &mut control, 120, |bench| {
        bench.get(MemoryModuleType::PotentialJobSite).is_some()
    });
    let takes = bench
        .villager_actions()
        .into_iter()
        .filter(|action| matches!(action, VillagerAction::TakePoi(_)))
        .count();
    assert_eq!(takes, 1);
}

fn wheat(age: &str) -> StateId {
    BlockKind::Wheat
        .default_state()
        .with_property("age", age)
        .unwrap()
}

fn bone_meal_facts(count: u8) -> VillagerFacts {
    let mut facts = farmer();
    facts.inventory[2] = Some(StackFacts {
        item: ItemKind::BoneMeal.id(),
        count,
    });
    facts
}

fn bonemeal() -> MobControl {
    MobControl::behavior(rspigot_ai::MobBehavior::UseBonemeal(
        rspigot_ai::UseBonemeal::new(),
    ))
}

fn crop_field() -> Field {
    let mut world = Field::flat();
    world.put(1, 63, 0, BlockKind::Farmland.default_state());
    world.put(1, 64, 0, wheat("3"));
    world
}

#[test]
fn the_work_package_weights_bone_meal_four_for_farmers_and_seven_for_the_others() {
    let weight = |profession, name: &str| {
        let list = behaviors(profession, false, Activity::Work);
        let (_, text) = list
            .iter()
            .find(|(_, text)| text.contains("UseBonemeal"))
            .unwrap();
        let at = text.find(name).unwrap();
        let rest = &text[at..];
        let weight = rest.find("weight: ").unwrap() + "weight: ".len();
        rest[weight..]
            .split(|c: char| !c.is_ascii_digit())
            .next()
            .unwrap()
            .parse::<i32>()
            .unwrap()
    };
    assert_eq!(weight(Profession::Farmer, "UseBonemeal(UseBonemeal"), 4);
    assert_eq!(
        weight(Profession::Farmer, "HarvestFarmland(HarvestFarmland"),
        2
    );
    assert_eq!(weight(Profession::Librarian, "UseBonemeal(UseBonemeal"), 7);
    assert_eq!(
        weight(Profession::Librarian, "HarvestFarmland(HarvestFarmland"),
        5
    );
}

#[test]
fn bone_meal_needs_a_ten_tick_boundary_bone_meal_and_a_growing_crop_around() {
    let mut bench = Bench::at(
        Vec3::new(1.5, 64.0, 0.5),
        bone_meal_facts(3),
        crop_field(),
        4,
        false,
    );
    let mut control = bonemeal();
    bench.mob.tick_count = 11;
    assert!(!bench.start(&mut control, NOW));
    bench.mob.tick_count = 20;
    assert!(bench.start(&mut control, NOW));

    let mut empty = Bench::at(Vec3::new(1.5, 64.0, 0.5), farmer(), crop_field(), 4, false);
    empty.mob.tick_count = 20;
    assert!(!empty.start(&mut bonemeal(), NOW));

    let mut grown = Field::flat();
    grown.put(1, 64, 0, wheat("7"));
    let mut mature = Bench::at(
        Vec3::new(1.5, 64.0, 0.5),
        bone_meal_facts(3),
        grown,
        4,
        false,
    );
    mature.mob.tick_count = 20;
    assert!(!mature.start(&mut bonemeal(), NOW));
}

#[test]
fn starting_holds_bone_meal_and_walks_to_the_crop_and_stopping_empties_the_hand() {
    let mut bench = Bench::at(
        Vec3::new(1.5, 64.0, 0.5),
        bone_meal_facts(3),
        crop_field(),
        4,
        false,
    );
    bench.mob.tick_count = 20;
    let mut control = bonemeal();
    assert!(bench.start(&mut control, NOW));
    assert!(
        bench
            .villager_actions()
            .contains(&VillagerAction::SetMainHand(Some(ItemKind::BoneMeal.id())))
    );
    let walk = bench
        .get(MemoryModuleType::WalkTarget)
        .and_then(MemoryValue::as_walk_target)
        .copied()
        .unwrap();
    assert_eq!(walk.speed_modifier, 0.5);
    assert_eq!(walk.close_enough_dist, 1);
    assert!(bench.get(MemoryModuleType::LookTarget).is_some());
    bench.state.memories.erase(MemoryModuleType::WalkTarget);
    bench.state.memories.erase(MemoryModuleType::LookTarget);
    bench.with(|cx| control.do_stop(cx, NOW + 5));
    assert!(
        bench
            .villager_actions()
            .contains(&VillagerAction::SetMainHand(None))
    );
}

#[test]
fn bone_meal_is_applied_when_close_then_the_next_cycle_waits_forty_ticks() {
    let mut bench = Bench::at(
        Vec3::new(1.5, 64.0, 0.5),
        bone_meal_facts(3),
        crop_field(),
        4,
        false,
    );
    bench.mob.tick_count = 20;
    let mut control = bonemeal();
    assert!(bench.start(&mut control, NOW));
    bench.villager_actions();
    bench.tick(&mut control, NOW);
    let actions = bench.actions();
    assert!(
        actions.contains(&MobAction::Villager(VillagerAction::UseBoneMeal(
            BlockPos::new(1, 64, 0)
        )))
    );
    assert!(actions.contains(&MobAction::LevelEvent {
        event: 1505,
        pos: BlockPos::new(1, 64, 0),
        data: 15,
    }));
    bench.tick(&mut control, NOW + 10);
    assert!(bench.actions().is_empty());
    bench.tick(&mut control, NOW + 40);
    assert!(!bench.actions().is_empty());
}

#[test]
fn bone_meal_waits_until_the_villager_is_within_a_block_of_the_crop() {
    let mut bench = Bench::at(
        Vec3::new(5.5, 64.0, 0.5),
        bone_meal_facts(3),
        crop_field(),
        4,
        false,
    );
    bench.mob.tick_count = 20;
    bench.mob.position = Vec3::new(1.5, 64.0, 0.5);
    let mut control = bonemeal();
    assert!(bench.start(&mut control, NOW));
    bench.villager_actions();
    bench.mob.position = Vec3::new(5.5, 64.0, 0.5);
    bench.tick(&mut control, NOW);
    assert!(bench.actions().is_empty());
}

#[test]
fn bone_meal_rests_a_hundred_sixty_ticks_after_a_session() {
    let mut bench = Bench::at(
        Vec3::new(1.5, 64.0, 0.5),
        bone_meal_facts(3),
        crop_field(),
        4,
        false,
    );
    bench.mob.tick_count = 20;
    let mut control = bonemeal();
    assert!(bench.start(&mut control, NOW));
    bench.state.memories.erase(MemoryModuleType::WalkTarget);
    bench.state.memories.erase(MemoryModuleType::LookTarget);
    bench.with(|cx| control.do_stop(cx, NOW + 5));
    bench.state.memories.erase(MemoryModuleType::WalkTarget);
    bench.state.memories.erase(MemoryModuleType::LookTarget);
    bench.mob.tick_count = 170;
    assert!(!bench.start(&mut control, NOW + 6));
    bench.mob.tick_count = 180;
    assert!(bench.start(&mut control, NOW + 7));
}

#[test]
fn working_at_a_composter_plays_the_sound_before_composting_and_restocking() {
    let site = BlockPos::new(0, 64, 1);
    let mut world = Field::flat();
    world.pois.push(PoiRecord::new(site, PoiType::Farmer));
    let mut bench = Bench::new(farmer(), world, 4);
    let global = bench.global(site);
    bench.set(MemoryModuleType::JobSite, global);
    let mut work = MobControl::behavior(rspigot_ai::MobBehavior::WorkAtPoi(
        rspigot_ai::WorkAtPoi::new(rspigot_ai::Workstation::Composter),
    ));
    let mut started = false;
    for tick in 0..2_000 {
        bench.world.time = NOW + tick;
        if bench.start(&mut work, NOW + tick) {
            started = true;
            break;
        }
    }
    assert!(started);
    assert_eq!(
        bench.villager_actions(),
        vec![
            VillagerAction::PlayWorkSound,
            VillagerAction::WorkAtComposter(site),
            VillagerAction::Restock,
        ]
    );
}

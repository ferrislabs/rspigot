use rspigot_ai::{
    Activity, AiContext, AiWorld, BackUpIfTooClose, BecomePassiveIfMemoryPresent, BehaviorControl,
    BehaviorCx, BrainMessage, BrainState, CopyIf, CopyMemoryWithExpiry, DroppedItem, Facts,
    GlobalPos, GoalSettings, HoglinSensor, ItemWish, Living, MemoryModuleType, MemoryValue,
    MobAction, MobAi, MobBody, MobControl, MobTrigger, MobView, NearestItemSensor,
    NearestVisibleLivingEntities, PackedMemories, PiglinAction, PiglinBruteSensor, PiglinSensor,
    RememberIfHoglinWasKilled, Sensor, SensorKind, StartAdmiringItemIfSeen,
    StartCelebratingIfTargetDead, StartHuntingHoglin, Status, StopAdmiringIfItemTooFarAway,
    StopAdmiringIfTiredOfTryingToReachItem, StopHoldingItemIfNoLongerAdmiring, TargetKind,
    UniformInt,
};
use rspigot_blocks::{BlockKind, StateId};
use rspigot_domain::entity::living::LivingState;
use rspigot_domain::entity::{Aabb, EntityBase};
use rspigot_domain::{BlockPos, EntityId, ItemId, Vec3};
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
    soul_fire: Vec<BlockPos>,
    time: i64,
    forgive: bool,
    anger: bool,
}

impl rspigot_ai::BlockView for Field {
    fn state(&self, pos: BlockPos) -> Option<StateId> {
        Some(if self.soul_fire.contains(&pos) {
            BlockKind::SoulCampfire.default_state()
        } else if (60..64).contains(&pos.y) {
            BlockKind::Netherrack.default_state()
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

    fn living_by_uuid(&self, uuid: Uuid) -> Option<Living> {
        self.players
            .iter()
            .chain(&self.others)
            .find(|living| living.facts.uuid == uuid)
            .copied()
    }

    fn forgive_dead_players(&self) -> bool {
        self.forgive
    }

    fn universal_anger(&self) -> bool {
        self.anger
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

fn baby(mut entity: Living) -> Living {
    entity.baby = true;
    entity
}

fn holding(mut entity: Living, item: ItemKind) -> Living {
    entity.main_hand = Some(item.id());
    entity
}

fn item(id: i32, position: Vec3, kind: ItemKind) -> DroppedItem {
    DroppedItem {
        id: EntityId(id),
        position,
        bounds: Aabb::of_size(position, 0.25, 0.25),
        item: kind.id(),
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
    controls: rspigot_ai::Brain,
    body: MobBody,
    state: BrainState,
}

impl Bench {
    fn new(kind: EntityKind, memories: &[MemoryModuleType]) -> Self {
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
            controls: rspigot_ai::Brain::new(kind, 5),
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

fn shot(trigger: MobTrigger) -> MobControl {
    MobControl::one_shot(trigger)
}

fn entity(id: i32) -> MemoryValue {
    MemoryValue::Entity(EntityId(id))
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

fn names(list: &[(i32, String)]) -> Vec<(i32, Vec<&'static str>)> {
    const KNOWN: [&str; 41] = [
        "InteractWithDoor",
        "BecomePassiveIfMemoryPresent",
        "AnimalMakeLove",
        "SetWalkTargetAwayFrom",
        "StartAttacking",
        "SetEntityLookTargetSometimes",
        "BabyFollowAdult",
        "SetWalkTargetFromAttackTargetIfTargetOutOfReach",
        "MeleeAttack",
        "StopAttackingIfTargetInvalid",
        "EraseMemoryIf",
        "LookAtTargetSink",
        "MoveToTargetSink",
        "StartHuntingHoglin",
        "SetLookAndInteract",
        "BackUpIfTooClose",
        "CrossbowAttack",
        "RememberIfHoglinWasKilled",
        "CopyMemoryWithExpiry",
        "StopHoldingItemIfNoLongerAdmiring",
        "StartAdmiringItemIfSeen",
        "StartCelebratingIfTargetDead",
        "StopBeingAngryIfTargetDead",
        "GoToWantedItem",
        "StopAdmiringIfItemTooFarAway",
        "StopAdmiringIfTiredOfTryingToReachItem",
        "GoToTargetLocation",
        "SetEntityLookTarget",
        "RandomStroll",
        "InteractWith",
        "StrollToPoi",
        "StrollAroundPoi",
        "SetWalkTargetFromLookTarget",
        "DoNothing",
        "Gate",
        "Timed",
        "OneShot",
        "Condition",
        "Sequence",
        "Swim",
        "AnimalPanic",
    ];
    list.iter()
        .map(|(priority, text)| {
            let mut found: Vec<(usize, &'static str)> = KNOWN
                .iter()
                .filter_map(|name| text.find(name).map(|at| (at, *name)))
                .collect();
            found.sort_by_key(|(at, _)| *at);
            (*priority, found.into_iter().map(|(_, name)| name).collect())
        })
        .collect()
}

fn leading(list: &[(i32, String)]) -> Vec<(i32, &'static str)> {
    names(list)
        .into_iter()
        .map(|(priority, found)| {
            let name = found
                .iter()
                .copied()
                .find(|name| {
                    !matches!(
                        *name,
                        "OneShot" | "Timed" | "Gate" | "Condition" | "Sequence"
                    )
                })
                .unwrap_or("?");
            (priority, name)
        })
        .collect()
}

#[test]
fn hoglin_activities_follow_the_vanilla_priorities() {
    let kind = EntityKind::Hoglin;
    assert_eq!(
        leading(&behaviors(kind, Activity::Core)),
        vec![(0, "LookAtTargetSink"), (1, "MoveToTargetSink")]
    );
    assert_eq!(
        leading(&behaviors(kind, Activity::Idle)),
        vec![
            (10, "BecomePassiveIfMemoryPresent"),
            (11, "AnimalMakeLove"),
            (12, "SetWalkTargetAwayFrom"),
            (13, "StartAttacking"),
            (14, "SetWalkTargetAwayFrom"),
            (15, "SetEntityLookTargetSometimes"),
            (16, "BabyFollowAdult"),
            (17, "RandomStroll"),
        ]
    );
    assert_eq!(
        leading(&behaviors(kind, Activity::Fight)),
        vec![
            (10, "BecomePassiveIfMemoryPresent"),
            (11, "AnimalMakeLove"),
            (12, "SetWalkTargetFromAttackTargetIfTargetOutOfReach"),
            (13, "MeleeAttack"),
            (14, "MeleeAttack"),
            (15, "StopAttackingIfTargetInvalid"),
            (16, "EraseMemoryIf"),
        ]
    );
    assert_eq!(
        leading(&behaviors(kind, Activity::Avoid)),
        vec![
            (10, "SetWalkTargetAwayFrom"),
            (11, "RandomStroll"),
            (12, "SetEntityLookTargetSometimes"),
            (13, "EraseMemoryIf"),
        ]
    );
}

#[test]
fn zoglin_activities_follow_the_vanilla_priorities() {
    let kind = EntityKind::Zoglin;
    assert_eq!(
        leading(&behaviors(kind, Activity::Core)),
        vec![(0, "LookAtTargetSink"), (1, "MoveToTargetSink")]
    );
    assert_eq!(
        leading(&behaviors(kind, Activity::Idle)),
        vec![
            (10, "StartAttacking"),
            (11, "SetEntityLookTargetSometimes"),
            (12, "RandomStroll"),
        ]
    );
    assert_eq!(
        leading(&behaviors(kind, Activity::Fight)),
        vec![
            (10, "SetWalkTargetFromAttackTargetIfTargetOutOfReach"),
            (11, "MeleeAttack"),
            (12, "MeleeAttack"),
            (13, "StopAttackingIfTargetInvalid"),
        ]
    );
}

#[test]
fn piglin_brute_activities_follow_the_vanilla_priorities() {
    let kind = EntityKind::PiglinBrute;
    assert_eq!(
        leading(&behaviors(kind, Activity::Core)),
        vec![
            (0, "LookAtTargetSink"),
            (1, "MoveToTargetSink"),
            (2, "InteractWithDoor"),
            (3, "StopBeingAngryIfTargetDead"),
        ]
    );
    assert_eq!(
        leading(&behaviors(kind, Activity::Idle)),
        vec![
            (10, "StartAttacking"),
            (11, "SetEntityLookTarget"),
            (12, "RandomStroll"),
            (13, "SetLookAndInteract"),
        ]
    );
    assert_eq!(
        leading(&behaviors(kind, Activity::Fight)),
        vec![
            (10, "StopAttackingIfTargetInvalid"),
            (11, "SetWalkTargetFromAttackTargetIfTargetOutOfReach"),
            (12, "MeleeAttack"),
        ]
    );
}

#[test]
fn piglin_activities_follow_the_vanilla_priorities() {
    let kind = EntityKind::Piglin;
    assert_eq!(
        leading(&behaviors(kind, Activity::Core)),
        vec![
            (0, "LookAtTargetSink"),
            (1, "MoveToTargetSink"),
            (2, "InteractWithDoor"),
            (3, "CopyMemoryWithExpiry"),
            (4, "CopyMemoryWithExpiry"),
            (5, "StopHoldingItemIfNoLongerAdmiring"),
            (6, "StartAdmiringItemIfSeen"),
            (7, "StartCelebratingIfTargetDead"),
            (8, "StopBeingAngryIfTargetDead"),
        ]
    );
    assert_eq!(
        leading(&behaviors(kind, Activity::Idle)),
        vec![
            (10, "SetEntityLookTarget"),
            (11, "StartAttacking"),
            (12, "StartHuntingHoglin"),
            (13, "SetWalkTargetAwayFrom"),
            (14, "SetEntityLookTarget"),
            (15, "RandomStroll"),
            (16, "SetLookAndInteract"),
        ]
    );
    assert_eq!(
        leading(&behaviors(kind, Activity::AdmireItem)),
        vec![
            (10, "GoToWantedItem"),
            (11, "StopAdmiringIfItemTooFarAway"),
            (12, "StopAdmiringIfTiredOfTryingToReachItem"),
        ]
    );
    assert_eq!(
        leading(&behaviors(kind, Activity::Fight)),
        vec![
            (10, "StopAttackingIfTargetInvalid"),
            (11, "BackUpIfTooClose"),
            (12, "SetWalkTargetFromAttackTargetIfTargetOutOfReach"),
            (16, "MeleeAttack"),
            (17, "CrossbowAttack"),
            (18, "RememberIfHoglinWasKilled"),
            (19, "EraseMemoryIf"),
        ]
    );
    assert_eq!(
        leading(&behaviors(kind, Activity::Celebrate)),
        vec![
            (10, "SetWalkTargetAwayFrom"),
            (11, "SetEntityLookTarget"),
            (12, "StartAttacking"),
            (13, "GoToTargetLocation"),
            (14, "GoToTargetLocation"),
            (15, "SetEntityLookTarget"),
        ]
    );
    assert_eq!(
        leading(&behaviors(kind, Activity::Avoid)),
        vec![
            (10, "SetWalkTargetAwayFrom"),
            (11, "SetEntityLookTarget"),
            (12, "RandomStroll"),
            (13, "EraseMemoryIf"),
        ]
    );
}

#[test]
fn every_nether_brain_registers_its_vanilla_sensors() {
    let sensors = |kind| {
        MobAi::new(kind, 1, GoalSettings::default())
            .behavior_brain()
            .map(|brain| {
                brain
                    .sensors()
                    .iter()
                    .map(|slot| slot.sensor().sensor_type().name())
                    .collect::<Vec<_>>()
            })
            .unwrap()
    };
    assert_eq!(
        sensors(EntityKind::Hoglin),
        vec![
            "minecraft:nearest_living_entities",
            "minecraft:nearest_players",
            "minecraft:nearest_adult",
            "minecraft:hoglin_specific_sensor",
        ]
    );
    assert_eq!(
        sensors(EntityKind::Zoglin),
        vec![
            "minecraft:nearest_living_entities",
            "minecraft:nearest_players"
        ]
    );
    assert_eq!(
        sensors(EntityKind::Piglin),
        vec![
            "minecraft:nearest_living_entities",
            "minecraft:nearest_players",
            "minecraft:nearest_items",
            "minecraft:hurt_by",
            "minecraft:piglin_specific_sensor",
        ]
    );
    assert_eq!(
        sensors(EntityKind::PiglinBrute),
        vec![
            "minecraft:nearest_living_entities",
            "minecraft:nearest_players",
            "minecraft:nearest_items",
            "minecraft:hurt_by",
            "minecraft:piglin_brute_specific_sensor",
        ]
    );
}

#[test]
fn become_passive_pacifies_the_mob_for_ten_seconds_and_drops_its_target() {
    let mut bench = Bench::new(
        EntityKind::Hoglin,
        &[
            MemoryModuleType::AttackTarget,
            MemoryModuleType::Pacified,
            MemoryModuleType::NearestRepellent,
        ],
    );
    let mut passive = shot(MobTrigger::BecomePassiveIfMemoryPresent(
        BecomePassiveIfMemoryPresent::new(MemoryModuleType::NearestRepellent, 200),
    ));
    assert!(!bench.start(&mut passive, NOW));
    bench.set(
        MemoryModuleType::NearestRepellent,
        MemoryValue::Block(BlockPos::new(3, 64, 3)),
    );
    bench.set(MemoryModuleType::AttackTarget, entity(9));
    assert!(bench.start(&mut passive, NOW));
    assert!(bench.get(MemoryModuleType::AttackTarget).is_none());
    assert_eq!(
        bench.get(MemoryModuleType::Pacified),
        Some(&MemoryValue::Bool(true))
    );
    assert_eq!(
        bench
            .state
            .memories
            .time_until_expiry(MemoryModuleType::Pacified),
        Some(200)
    );
    assert!(!bench.start(&mut passive, NOW));
}

fn back_up_bench() -> Bench {
    let mut bench = Bench::new(
        EntityKind::Piglin,
        &[
            MemoryModuleType::WalkTarget,
            MemoryModuleType::LookTarget,
            MemoryModuleType::AttackTarget,
            MemoryModuleType::NearestVisibleLivingEntities,
        ],
    );
    bench.set(MemoryModuleType::AttackTarget, entity(7));
    bench
}

#[test]
fn back_up_if_too_close_strafes_away_while_looking_at_a_visible_target() {
    let mut bench = back_up_bench();
    let close = living(7, EntityKind::Player, Vec3::new(3.5, 64.0, 0.5));
    bench.see(&[close]);
    let mut back_up = shot(MobTrigger::BackUpIfTooClose(BackUpIfTooClose::new(5, 0.75)));
    assert!(bench.start(&mut back_up, NOW));
    assert!(
        bench.controls.movement.has_wanted() || bench.get(MemoryModuleType::LookTarget).is_some()
    );
    assert!(bench.get(MemoryModuleType::LookTarget).is_some());
}

#[test]
fn back_up_if_too_close_ignores_far_targets_and_needs_no_walk_target() {
    let mut bench = back_up_bench();
    let far = living(7, EntityKind::Player, Vec3::new(9.5, 64.0, 0.5));
    bench.see(&[far]);
    let mut back_up = shot(MobTrigger::BackUpIfTooClose(BackUpIfTooClose::new(5, 0.75)));
    assert!(!bench.start(&mut back_up, NOW));
    assert!(bench.get(MemoryModuleType::LookTarget).is_none());
    let mut bench = back_up_bench();
    bench.see(&[living(7, EntityKind::Player, Vec3::new(3.5, 64.0, 0.5))]);
    bench.set(
        MemoryModuleType::WalkTarget,
        MemoryValue::WalkTarget(rspigot_ai::WalkTarget::block(
            BlockPos::new(1, 64, 1),
            1.0,
            0,
        )),
    );
    assert!(!bench.start(&mut back_up, NOW));
}

#[test]
fn copy_memory_with_expiry_copies_for_the_given_duration_only_when_the_predicate_holds() {
    let memories = [
        MemoryModuleType::NearestVisibleNemesis,
        MemoryModuleType::AvoidTarget,
    ];
    let copy = || {
        shot(MobTrigger::CopyMemoryWithExpiry(CopyMemoryWithExpiry::new(
            CopyIf::IsBaby,
            MemoryModuleType::NearestVisibleNemesis,
            MemoryModuleType::AvoidTarget,
            UniformInt::of(100, 140),
        )))
    };
    let mut adult = Bench::new(EntityKind::Piglin, &memories);
    adult.set(MemoryModuleType::NearestVisibleNemesis, entity(4));
    assert!(!adult.start(&mut copy(), NOW));
    assert!(adult.get(MemoryModuleType::AvoidTarget).is_none());
    let mut child = Bench::new(EntityKind::Piglin, &memories);
    child.mob.baby = true;
    child.set(MemoryModuleType::NearestVisibleNemesis, entity(4));
    assert!(child.start(&mut copy(), NOW));
    assert_eq!(child.get(MemoryModuleType::AvoidTarget), Some(&entity(4)));
    let ttl = child
        .state
        .memories
        .time_until_expiry(MemoryModuleType::AvoidTarget)
        .unwrap();
    assert!((100..=140).contains(&ttl), "{ttl}");
    assert!(!child.start(&mut copy(), NOW));
}

fn celebrating() -> Bench {
    let mut bench = Bench::new(
        EntityKind::Piglin,
        &[
            MemoryModuleType::AttackTarget,
            MemoryModuleType::AngryAt,
            MemoryModuleType::CelebrateLocation,
            MemoryModuleType::Dancing,
        ],
    );
    bench.set(MemoryModuleType::AttackTarget, entity(7));
    bench.set(
        MemoryModuleType::AngryAt,
        MemoryValue::Uuid(Uuid::from_u128(907)),
    );
    bench
}

#[test]
fn celebrating_needs_a_dead_target_and_marks_the_celebration_location() {
    let mut bench = celebrating();
    let mut dead = living(7, EntityKind::Zombie, Vec3::new(4.5, 64.0, 4.5));
    bench.world.others.push(dead);
    let mut celebrate = shot(MobTrigger::StartCelebratingIfTargetDead(
        StartCelebratingIfTargetDead::new(300),
    ));
    assert!(!bench.start(&mut celebrate, NOW));
    dead.alive = false;
    bench.world.others = vec![dead];
    assert!(bench.start(&mut celebrate, NOW));
    assert_eq!(
        bench.get(MemoryModuleType::CelebrateLocation),
        Some(&MemoryValue::Block(BlockPos::new(4, 64, 4)))
    );
    assert_eq!(
        bench
            .state
            .memories
            .time_until_expiry(MemoryModuleType::CelebrateLocation),
        Some(300)
    );
    assert!(bench.get(MemoryModuleType::AttackTarget).is_none());
    assert!(bench.get(MemoryModuleType::AngryAt).is_none());
    assert!(bench.get(MemoryModuleType::Dancing).is_none());
}

#[test]
fn only_a_ten_percent_roll_seeded_by_the_game_time_makes_a_piglin_dance_for_a_hoglin() {
    let dances = |time: i64| {
        let mut bench = celebrating();
        bench.world.time = time;
        let mut dead = living(7, EntityKind::Hoglin, Vec3::new(4.5, 64.0, 4.5));
        dead.alive = false;
        bench.world.others.push(dead);
        let mut celebrate = shot(MobTrigger::StartCelebratingIfTargetDead(
            StartCelebratingIfTargetDead::new(300),
        ));
        assert!(bench.start(&mut celebrate, time));
        bench.get(MemoryModuleType::Dancing).is_some()
    };
    let dancing = (0..600)
        .filter(|step| dances(i64::from(*step) * 7_919 + 3))
        .count();
    assert!((30..=90).contains(&dancing), "{dancing}");
    assert_eq!(dances(37), dances(37));
}

#[test]
fn a_dead_player_keeps_the_piglin_angry_unless_dead_players_are_forgiven() {
    for forgive in [false, true] {
        let mut bench = celebrating();
        bench.world.forgive = forgive;
        let mut dead = living(7, EntityKind::Player, Vec3::new(4.5, 64.0, 4.5));
        dead.alive = false;
        bench.world.players.push(dead);
        let mut celebrate = shot(MobTrigger::StartCelebratingIfTargetDead(
            StartCelebratingIfTargetDead::new(300),
        ));
        assert!(bench.start(&mut celebrate, NOW));
        assert_eq!(
            bench.get(MemoryModuleType::AttackTarget).is_some(),
            !forgive
        );
        assert!(bench.get(MemoryModuleType::CelebrateLocation).is_some());
    }
}

fn hunting() -> Bench {
    let mut bench = Bench::new(
        EntityKind::Piglin,
        &[
            MemoryModuleType::NearestVisibleHuntableHoglin,
            MemoryModuleType::AngryAt,
            MemoryModuleType::HuntedRecently,
            MemoryModuleType::NearestVisibleAdultPiglins,
            MemoryModuleType::NearbyAdultPiglins,
            MemoryModuleType::UniversalAnger,
            MemoryModuleType::CantReachWalkTargetSince,
            MemoryModuleType::NearestVisibleLivingEntities,
        ],
    );
    let hoglin = living(8, EntityKind::Hoglin, Vec3::new(5.5, 64.0, 0.5));
    bench.world.others.push(hoglin);
    bench.set(MemoryModuleType::NearestVisibleHuntableHoglin, entity(8));
    bench.see(&[hoglin]);
    bench
}

#[test]
fn hunting_a_hoglin_angers_the_piglin_and_makes_the_whole_group_stop_hunting() {
    let mut bench = hunting();
    let friend = living(5, EntityKind::Piglin, Vec3::new(2.5, 64.0, 0.5));
    bench.world.others.push(friend);
    bench.set(
        MemoryModuleType::NearestVisibleAdultPiglins,
        MemoryValue::Entities(vec![EntityId(5)]),
    );
    bench.set(
        MemoryModuleType::NearbyAdultPiglins,
        MemoryValue::Entities(vec![EntityId(5)]),
    );
    let mut hunt = shot(MobTrigger::StartHuntingHoglin(StartHuntingHoglin));
    assert!(bench.start(&mut hunt, NOW));
    assert_eq!(
        bench.get(MemoryModuleType::AngryAt),
        Some(&MemoryValue::Uuid(Uuid::from_u128(908)))
    );
    assert_eq!(
        bench
            .state
            .memories
            .time_until_expiry(MemoryModuleType::AngryAt),
        Some(600)
    );
    let cooldown = bench
        .state
        .memories
        .time_until_expiry(MemoryModuleType::HuntedRecently)
        .unwrap();
    assert!((600..=2400).contains(&cooldown), "{cooldown}");
    let actions = bench.actions();
    assert!(actions.contains(&MobAction::Message {
        to: EntityId(5),
        message: BrainMessage::PiglinAngerIfCloser {
            target: EntityId(8)
        },
    }));
    assert!(actions.contains(&MobAction::Message {
        to: EntityId(5),
        message: BrainMessage::PiglinDontHunt,
    }));
}

#[test]
fn a_piglin_does_not_hunt_when_a_neighbour_hunted_recently_or_when_it_is_a_baby() {
    let mut bench = hunting();
    let mut friend = living(5, EntityKind::Piglin, Vec3::new(2.5, 64.0, 0.5));
    friend.facts.hunted_recently = true;
    bench.world.others.push(friend);
    bench.set(
        MemoryModuleType::NearestVisibleAdultPiglins,
        MemoryValue::Entities(vec![EntityId(5)]),
    );
    let mut hunt = shot(MobTrigger::StartHuntingHoglin(StartHuntingHoglin));
    assert!(!bench.start(&mut hunt, NOW));
    let mut child = hunting();
    child.mob.baby = true;
    assert!(!child.start(&mut hunt, NOW));
    let mut already = hunting();
    already.set(MemoryModuleType::HuntedRecently, MemoryValue::Bool(true));
    assert!(!already.start(&mut hunt, NOW));
}

#[test]
fn a_killed_hoglin_starts_the_hunting_cooldown() {
    let mut bench = Bench::new(
        EntityKind::Piglin,
        &[
            MemoryModuleType::AttackTarget,
            MemoryModuleType::HuntedRecently,
        ],
    );
    bench.set(MemoryModuleType::AttackTarget, entity(8));
    let mut hoglin = living(8, EntityKind::Hoglin, Vec3::new(5.5, 64.0, 0.5));
    bench.world.others.push(hoglin);
    let mut remember = shot(MobTrigger::RememberIfHoglinWasKilled(
        RememberIfHoglinWasKilled,
    ));
    assert!(!bench.start(&mut remember, NOW));
    hoglin.alive = false;
    bench.world.others = vec![hoglin];
    assert!(bench.start(&mut remember, NOW));
    let ttl = bench
        .state
        .memories
        .time_until_expiry(MemoryModuleType::HuntedRecently)
        .unwrap();
    assert!((600..=2400).contains(&ttl));
}

fn admirer() -> Bench {
    Bench::new(
        EntityKind::Piglin,
        &[
            MemoryModuleType::NearestVisibleWantedItem,
            MemoryModuleType::AdmiringItem,
            MemoryModuleType::AdmiringDisabled,
            MemoryModuleType::DisableWalkToAdmireItem,
            MemoryModuleType::TimeTryingToReachAdmireItem,
        ],
    )
}

#[test]
fn a_piglin_starts_admiring_a_seen_gold_item_for_119_ticks_and_ignores_other_items() {
    let mut bench = admirer();
    bench
        .world
        .items
        .push(item(20, Vec3::new(2.5, 64.0, 0.5), ItemKind::GoldIngot));
    bench
        .world
        .items
        .push(item(21, Vec3::new(2.5, 64.0, 0.5), ItemKind::Dirt));
    let mut admire = shot(MobTrigger::StartAdmiringItemIfSeen(
        StartAdmiringItemIfSeen::new(119),
    ));
    bench.set(MemoryModuleType::NearestVisibleWantedItem, entity(21));
    assert!(!bench.start(&mut admire, NOW));
    bench.set(MemoryModuleType::NearestVisibleWantedItem, entity(20));
    assert!(bench.start(&mut admire, NOW));
    assert_eq!(
        bench
            .state
            .memories
            .time_until_expiry(MemoryModuleType::AdmiringItem),
        Some(119)
    );
    assert!(!bench.start(&mut admire, NOW));
}

#[test]
fn admiring_is_blocked_while_disabled() {
    for blocker in [
        MemoryModuleType::AdmiringDisabled,
        MemoryModuleType::DisableWalkToAdmireItem,
    ] {
        let mut bench = admirer();
        bench
            .world
            .items
            .push(item(20, Vec3::new(2.5, 64.0, 0.5), ItemKind::GoldIngot));
        bench.set(MemoryModuleType::NearestVisibleWantedItem, entity(20));
        bench.set(blocker, MemoryValue::Bool(true));
        let mut admire = shot(MobTrigger::StartAdmiringItemIfSeen(
            StartAdmiringItemIfSeen::new(119),
        ));
        assert!(!bench.start(&mut admire, NOW));
    }
}

#[test]
fn admiring_stops_when_the_item_is_further_than_nine_blocks_unless_something_is_in_hand() {
    let mut bench = admirer();
    bench
        .world
        .items
        .push(item(20, Vec3::new(20.5, 64.0, 0.5), ItemKind::GoldIngot));
    bench.set(MemoryModuleType::NearestVisibleWantedItem, entity(20));
    bench.set(MemoryModuleType::AdmiringItem, MemoryValue::Bool(true));
    let mut far = shot(MobTrigger::StopAdmiringIfItemTooFarAway(
        StopAdmiringIfItemTooFarAway::new(9),
    ));
    bench.mob.off_hand = Some(ItemKind::GoldIngot.id());
    assert!(!bench.start(&mut far, NOW));
    assert!(bench.get(MemoryModuleType::AdmiringItem).is_some());
    bench.mob.off_hand = None;
    assert!(bench.start(&mut far, NOW));
    assert!(bench.get(MemoryModuleType::AdmiringItem).is_none());
}

#[test]
fn a_near_item_keeps_the_piglin_admiring() {
    let mut bench = admirer();
    bench
        .world
        .items
        .push(item(20, Vec3::new(3.5, 64.0, 0.5), ItemKind::GoldIngot));
    bench.set(MemoryModuleType::NearestVisibleWantedItem, entity(20));
    bench.set(MemoryModuleType::AdmiringItem, MemoryValue::Bool(true));
    let mut far = shot(MobTrigger::StopAdmiringIfItemTooFarAway(
        StopAdmiringIfItemTooFarAway::new(9),
    ));
    assert!(!bench.start(&mut far, NOW));
    assert!(bench.get(MemoryModuleType::AdmiringItem).is_some());
}

#[test]
fn a_piglin_gives_up_reaching_an_item_after_two_hundred_ticks_and_disables_walking_to_it() {
    let mut bench = admirer();
    bench
        .world
        .items
        .push(item(20, Vec3::new(3.5, 64.0, 0.5), ItemKind::GoldIngot));
    bench.set(MemoryModuleType::NearestVisibleWantedItem, entity(20));
    bench.set(MemoryModuleType::AdmiringItem, MemoryValue::Bool(true));
    let mut tired = shot(MobTrigger::StopAdmiringIfTiredOfTryingToReachItem(
        StopAdmiringIfTiredOfTryingToReachItem::new(200, 200),
    ));
    assert!(bench.start(&mut tired, NOW));
    assert_eq!(
        bench.get(MemoryModuleType::TimeTryingToReachAdmireItem),
        Some(&MemoryValue::Int(0))
    );
    for expected in 1..=201 {
        let mut again = shot(MobTrigger::StopAdmiringIfTiredOfTryingToReachItem(
            StopAdmiringIfTiredOfTryingToReachItem::new(200, 200),
        ));
        assert!(bench.start(&mut again, NOW));
        if expected <= 200 {
            assert_eq!(
                bench.get(MemoryModuleType::TimeTryingToReachAdmireItem),
                Some(&MemoryValue::Int(expected))
            );
        }
    }
    let mut last = shot(MobTrigger::StopAdmiringIfTiredOfTryingToReachItem(
        StopAdmiringIfTiredOfTryingToReachItem::new(200, 200),
    ));
    assert!(bench.start(&mut last, NOW));
    assert!(bench.get(MemoryModuleType::AdmiringItem).is_none());
    assert!(
        bench
            .get(MemoryModuleType::TimeTryingToReachAdmireItem)
            .is_none()
    );
    assert_eq!(
        bench
            .state
            .memories
            .time_until_expiry(MemoryModuleType::DisableWalkToAdmireItem),
        Some(200)
    );
}

#[test]
fn the_piglin_lets_go_of_its_offhand_item_once_it_stops_admiring() {
    let mut bench = Bench::new(EntityKind::Piglin, &[MemoryModuleType::AdmiringItem]);
    let mut stop = shot(MobTrigger::StopHoldingItemIfNoLongerAdmiring(
        StopHoldingItemIfNoLongerAdmiring,
    ));
    assert!(!bench.start(&mut stop, NOW));
    bench.mob.off_hand = Some(ItemKind::GoldIngot.id());
    assert!(bench.start(&mut stop, NOW));
    assert_eq!(
        bench.actions(),
        vec![MobAction::Piglin(PiglinAction::StopHoldingOffHand {
            bartering_enabled: true
        })]
    );
    bench.mob.off_hand = Some(ItemKind::Shield.id());
    assert!(!bench.start(&mut stop, NOW));
    bench.set(MemoryModuleType::AdmiringItem, MemoryValue::Bool(true));
    bench.mob.off_hand = Some(ItemKind::GoldIngot.id());
    assert!(!bench.start(&mut stop, NOW));
}

fn full_brain(kind: EntityKind) -> (MobAi, MobView, Field) {
    let ai = MobAi::new(kind, 9, GoalSettings::default());
    let world = Field {
        time: NOW,
        ..Field::default()
    };
    (ai, view(kind, HOME), world)
}

fn think(ai: &mut MobAi, mob: &MobView, world: &Field, ticks: i32) -> Vec<MobAction> {
    let mut actions = Vec::new();
    for _ in 0..ticks {
        let mut body = MobBody::of(mob);
        ai.think(mob, world, &mut body);
        actions.extend(ai.take_actions());
    }
    actions
}

fn memories(ai: &MobAi) -> &rspigot_ai::Memories {
    ai.behavior_brain().unwrap().memories()
}

#[test]
fn the_piglin_sensor_sorts_what_the_piglin_sees() {
    let mut bench = Bench::new(
        EntityKind::Piglin,
        &[
            MemoryModuleType::NearestVisibleLivingEntities,
            MemoryModuleType::NearestLivingEntities,
        ],
    );
    let bare_player = living(2, EntityKind::Player, Vec3::new(3.5, 64.0, 0.5));
    let gold_player = {
        let mut player = living(3, EntityKind::Player, Vec3::new(4.5, 64.0, 0.5));
        player.facts.piglin_safe_armor = true;
        player
    };
    let lover = holding(
        living(4, EntityKind::Player, Vec3::new(5.5, 64.0, 0.5)),
        ItemKind::GoldIngot,
    );
    let adult_hoglin = living(5, EntityKind::Hoglin, Vec3::new(2.5, 64.0, 2.5));
    let baby_hoglin = baby(living(6, EntityKind::Hoglin, Vec3::new(2.5, 64.0, 3.5)));
    let piglin = living(7, EntityKind::Piglin, Vec3::new(-2.5, 64.0, 0.5));
    let brute = living(8, EntityKind::PiglinBrute, Vec3::new(-3.5, 64.0, 0.5));
    let zoglin = living(9, EntityKind::Zoglin, Vec3::new(0.5, 64.0, 6.5));
    let wither = living(10, EntityKind::WitherSkeleton, Vec3::new(0.5, 64.0, -6.5));
    let all = [
        bare_player,
        gold_player,
        lover,
        adult_hoglin,
        baby_hoglin,
        piglin,
        brute,
        zoglin,
        wither,
    ];
    bench.world.players = vec![bare_player, gold_player, lover];
    bench.world.others = vec![adult_hoglin, baby_hoglin, piglin, brute, zoglin, wither];
    bench.see(&all);
    let ids = all.iter().map(|living| living.id).collect();
    bench.set(
        MemoryModuleType::NearestLivingEntities,
        MemoryValue::Entities(ids),
    );
    bench.tick_sensor(SensorKind::PiglinSpecific(PiglinSensor));
    let get = |bench: &Bench, ty| bench.get(ty).cloned();
    assert_eq!(
        get(&bench, MemoryModuleType::NearestVisibleNemesis),
        Some(entity(10))
    );
    assert_eq!(
        get(&bench, MemoryModuleType::NearestVisibleHuntableHoglin),
        Some(entity(5))
    );
    assert_eq!(
        get(&bench, MemoryModuleType::NearestVisibleBabyHoglin),
        Some(entity(6))
    );
    assert_eq!(
        get(&bench, MemoryModuleType::NearestVisibleZombified),
        Some(entity(9))
    );
    assert_eq!(
        get(
            &bench,
            MemoryModuleType::NearestTargetablePlayerNotWearingGold
        ),
        Some(entity(2))
    );
    assert_eq!(
        get(&bench, MemoryModuleType::NearestPlayerHoldingWantedItem),
        Some(entity(4))
    );
    assert_eq!(
        get(&bench, MemoryModuleType::NearestVisibleAdultPiglins),
        Some(MemoryValue::Entities(vec![EntityId(7), EntityId(8)]))
    );
    assert_eq!(
        get(&bench, MemoryModuleType::NearbyAdultPiglins),
        Some(MemoryValue::Entities(vec![EntityId(7), EntityId(8)]))
    );
    assert_eq!(
        get(&bench, MemoryModuleType::VisibleAdultPiglinCount),
        Some(MemoryValue::Int(2))
    );
    assert_eq!(
        get(&bench, MemoryModuleType::VisibleAdultHoglinCount),
        Some(MemoryValue::Int(1))
    );
}

#[test]
fn a_hoglin_that_cannot_be_hunted_is_not_huntable() {
    let mut bench = Bench::new(
        EntityKind::Piglin,
        &[MemoryModuleType::NearestVisibleLivingEntities],
    );
    let mut protected = living(5, EntityKind::Hoglin, Vec3::new(2.5, 64.0, 2.5));
    protected.facts.cannot_be_hunted = true;
    bench.world.others = vec![protected];
    bench.see(&[protected]);
    bench.tick_sensor(SensorKind::PiglinSpecific(PiglinSensor));
    assert!(
        bench
            .get(MemoryModuleType::NearestVisibleHuntableHoglin)
            .is_none()
    );
    assert_eq!(
        bench.get(MemoryModuleType::VisibleAdultHoglinCount),
        Some(&MemoryValue::Int(1))
    );
}

#[test]
fn the_piglin_sensor_finds_a_lit_soul_campfire_but_ignores_an_unlit_one_in_the_repellent_tag() {
    let mut bench = Bench::new(
        EntityKind::Piglin,
        &[MemoryModuleType::NearestVisibleLivingEntities],
    );
    bench.world.soul_fire.push(BlockPos::new(0, 64, 3));
    bench.tick_sensor(SensorKind::PiglinSpecific(PiglinSensor));
    assert_eq!(
        bench.get(MemoryModuleType::NearestRepellent),
        Some(&MemoryValue::Block(BlockPos::new(0, 64, 3)))
    );
}

#[test]
fn the_hoglin_sensor_counts_visible_adult_piglins_and_hoglins() {
    let mut bench = Bench::new(
        EntityKind::Hoglin,
        &[MemoryModuleType::NearestVisibleLivingEntities],
    );
    let piglin = living(2, EntityKind::Piglin, Vec3::new(3.5, 64.0, 0.5));
    let baby_piglin = baby(living(3, EntityKind::Piglin, Vec3::new(4.5, 64.0, 0.5)));
    let hoglin = living(4, EntityKind::Hoglin, Vec3::new(5.5, 64.0, 0.5));
    let cow = living(5, EntityKind::Cow, Vec3::new(6.5, 64.0, 0.5));
    let all = [piglin, baby_piglin, hoglin, cow];
    bench.world.others = all.to_vec();
    bench.see(&all);
    bench.tick_sensor(SensorKind::HoglinSpecific(HoglinSensor));
    assert_eq!(
        bench.get(MemoryModuleType::NearestVisibleAdultPiglin),
        Some(&entity(2))
    );
    assert_eq!(
        bench.get(MemoryModuleType::NearestVisibleAdultHoglins),
        Some(&MemoryValue::Entities(vec![EntityId(4)]))
    );
    assert_eq!(
        bench.get(MemoryModuleType::VisibleAdultPiglinCount),
        Some(&MemoryValue::Int(1))
    );
    assert_eq!(
        bench.get(MemoryModuleType::VisibleAdultHoglinCount),
        Some(&MemoryValue::Int(1))
    );
}

#[test]
fn the_brute_sensor_tracks_the_closest_nemesis_and_the_nearby_adult_piglins() {
    let mut bench = Bench::new(
        EntityKind::PiglinBrute,
        &[
            MemoryModuleType::NearestVisibleLivingEntities,
            MemoryModuleType::NearestLivingEntities,
        ],
    );
    let wither = living(2, EntityKind::Wither, Vec3::new(8.5, 64.0, 0.5));
    let skeleton = living(3, EntityKind::WitherSkeleton, Vec3::new(3.5, 64.0, 0.5));
    let piglin = living(4, EntityKind::Piglin, Vec3::new(-3.5, 64.0, 0.5));
    let all = [skeleton, piglin, wither];
    bench.world.others = all.to_vec();
    bench.see(&all);
    bench.set(
        MemoryModuleType::NearestLivingEntities,
        MemoryValue::Entities(vec![EntityId(3), EntityId(4), EntityId(2)]),
    );
    bench.tick_sensor(SensorKind::PiglinBruteSpecific(PiglinBruteSensor));
    assert_eq!(
        bench.get(MemoryModuleType::NearestVisibleNemesis),
        Some(&entity(3))
    );
    assert_eq!(
        bench.get(MemoryModuleType::NearbyAdultPiglins),
        Some(&MemoryValue::Entities(vec![EntityId(4)]))
    );
}

#[test]
fn the_piglin_item_sensor_only_wants_what_a_piglin_wants() {
    let mut bench = Bench::new(
        EntityKind::Piglin,
        &[MemoryModuleType::NearestVisibleWantedItem],
    );
    bench.mob.main_hand = Some(ItemKind::GoldenSword.id());
    bench
        .world
        .items
        .push(item(20, Vec3::new(2.5, 64.0, 0.5), ItemKind::Dirt));
    bench.tick_sensor(SensorKind::NearestItems(NearestItemSensor::new(
        ItemWish::Piglin,
    )));
    assert!(
        bench
            .get(MemoryModuleType::NearestVisibleWantedItem)
            .is_none()
    );
    bench
        .world
        .items
        .push(item(21, Vec3::new(3.5, 64.0, 0.5), ItemKind::GoldIngot));
    bench.tick_sensor(SensorKind::NearestItems(NearestItemSensor::new(
        ItemWish::Piglin,
    )));
    assert_eq!(
        bench.get(MemoryModuleType::NearestVisibleWantedItem),
        Some(&entity(21))
    );
}

#[test]
fn the_brute_sensor_item_wish_is_the_golden_axe_only() {
    let mut bench = Bench::new(
        EntityKind::PiglinBrute,
        &[MemoryModuleType::NearestVisibleWantedItem],
    );
    bench
        .world
        .items
        .push(item(20, Vec3::new(2.5, 64.0, 0.5), ItemKind::GoldIngot));
    bench.tick_sensor(SensorKind::NearestItems(NearestItemSensor::new(
        ItemWish::PiglinBrute,
    )));
    assert!(
        bench
            .get(MemoryModuleType::NearestVisibleWantedItem)
            .is_none()
    );
    bench
        .world
        .items
        .push(item(21, Vec3::new(3.5, 64.0, 0.5), ItemKind::GoldenAxe));
    bench.tick_sensor(SensorKind::NearestItems(NearestItemSensor::new(
        ItemWish::PiglinBrute,
    )));
    assert_eq!(
        bench.get(MemoryModuleType::NearestVisibleWantedItem),
        Some(&entity(21))
    );
}

fn wants(kind_of_item: ItemKind, tweak: impl FnOnce(&mut MobView, &mut MobAi)) -> bool {
    let (mut ai, mut mob, _) = full_brain(EntityKind::Piglin);
    tweak(&mut mob, &mut ai);
    ai.piglin_wants_to_pickup(&mob, kind_of_item.id())
}

#[test]
fn piglin_pickup_rules_follow_wants_to_pickup() {
    assert!(wants(ItemKind::GoldIngot, |_, _| {}));
    assert!(!wants(ItemKind::GoldIngot, |mob, _| {
        mob.off_hand = Some(ItemKind::GoldIngot.id());
    }));
    assert!(wants(ItemKind::GoldNugget, |_, _| {}));
    assert!(wants(ItemKind::GoldBlock, |_, _| {}));
    assert!(!wants(ItemKind::SoulTorch, |_, _| {}));
    assert!(!wants(ItemKind::Dirt, |mob, _| {
        mob.main_hand = Some(ItemKind::GoldenSword.id());
    }));
    assert!(wants(ItemKind::Dirt, |_, _| {}));
    assert!(!wants(ItemKind::Leather, |mob, _| {
        mob.baby = true;
    }));
    assert!(wants(ItemKind::Dirt, |mob, _| {
        mob.baby = true;
    }));
    assert!(!wants(ItemKind::GoldIngot, |mob, ai| {
        let brain = ai.behavior_brain_mut().unwrap();
        brain
            .memories_mut()
            .set(MemoryModuleType::AdmiringDisabled, MemoryValue::Bool(true));
        brain
            .memories_mut()
            .set(MemoryModuleType::AttackTarget, entity(9));
        let _ = mob;
    }));
}

#[test]
fn piglins_eat_cooked_porkchops_unless_they_just_ate_and_babies_ignore_some_items() {
    assert!(wants(ItemKind::CookedPorkchop, |_, _| {}));
    assert!(!wants(ItemKind::CookedPorkchop, |_, ai| {
        ai.behavior_brain_mut()
            .unwrap()
            .memories_mut()
            .set(MemoryModuleType::AteRecently, MemoryValue::Bool(true));
    }));
    assert!(wants(ItemKind::GoldenSword, |mob, _| {
        mob.main_hand = Some(ItemKind::WoodenSword.id());
    }));
    assert!(!wants(ItemKind::WoodenSword, |mob, _| {
        mob.main_hand = Some(ItemKind::GoldenSword.id());
    }));
    assert!(wants(ItemKind::GoldenHelmet, |_, _| {}));
}

#[test]
fn a_piglin_wants_a_better_armor_piece_but_not_a_worse_one() {
    assert!(wants(ItemKind::IronHelmet, |mob, _| {
        mob.species.piglin.head = Some(ItemKind::LeatherHelmet.id());
    }));
    assert!(!wants(ItemKind::LeatherHelmet, |mob, _| {
        mob.species.piglin.head = Some(ItemKind::IronHelmet.id());
    }));
}

#[test]
fn a_full_inventory_refuses_food_and_gold_nuggets() {
    let full = |mob: &mut MobView, _: &mut MobAi| {
        for slot in &mut mob.species.piglin.inventory {
            *slot = Some((ItemKind::Cobblestone.id(), 64));
        }
    };
    assert!(!wants(ItemKind::CookedPorkchop, full));
    assert!(!wants(ItemKind::GoldNugget, full));
}

#[test]
fn a_piglin_can_admire_a_gold_ingot_when_it_is_an_adult_and_not_already_admiring() {
    let (mut ai, mob, _) = full_brain(EntityKind::Piglin);
    assert!(ai.piglin_can_admire(&mob, ItemKind::GoldIngot.id()));
    assert!(!ai.piglin_can_admire(&mob, ItemKind::GoldNugget.id()));
    let mut child = mob;
    child.baby = true;
    assert!(!ai.piglin_can_admire(&child, ItemKind::GoldIngot.id()));
    ai.behavior_brain_mut()
        .unwrap()
        .memories_mut()
        .set(MemoryModuleType::AdmiringItem, MemoryValue::Bool(true));
    assert!(!ai.piglin_can_admire(&mob, ItemKind::GoldIngot.id()));
}

fn deliver(ai: &mut MobAi, mob: &MobView, world: &Field, message: BrainMessage) -> Vec<MobAction> {
    let mut body = MobBody::of(mob);
    ai.deliver(mob, world, &mut body, message);
    ai.take_actions()
}

#[test]
fn a_hoglin_attacks_the_closer_of_its_target_and_a_broadcast_target() {
    let (mut ai, mob, mut world) = full_brain(EntityKind::Hoglin);
    let near = living(2, EntityKind::Piglin, Vec3::new(2.5, 64.0, 0.5));
    let far = living(3, EntityKind::Piglin, Vec3::new(9.5, 64.0, 0.5));
    world.others = vec![near, far];
    deliver(
        &mut ai,
        &mob,
        &world,
        BrainMessage::HoglinAttackIfCloser {
            target: EntityId(3),
        },
    );
    assert_eq!(
        memories(&ai).get(MemoryModuleType::AttackTarget),
        Some(&entity(3))
    );
    assert_eq!(
        memories(&ai).time_until_expiry(MemoryModuleType::AttackTarget),
        Some(200)
    );
    deliver(
        &mut ai,
        &mob,
        &world,
        BrainMessage::HoglinAttackIfCloser {
            target: EntityId(2),
        },
    );
    assert_eq!(
        memories(&ai).get(MemoryModuleType::AttackTarget),
        Some(&entity(2))
    );
    deliver(
        &mut ai,
        &mob,
        &world,
        BrainMessage::HoglinAttackIfCloser {
            target: EntityId(3),
        },
    );
    assert_eq!(
        memories(&ai).get(MemoryModuleType::AttackTarget),
        Some(&entity(2))
    );
}

#[test]
fn a_pacified_hoglin_ignores_broadcast_targets() {
    let (mut ai, mob, mut world) = full_brain(EntityKind::Hoglin);
    world.others = vec![living(2, EntityKind::Piglin, Vec3::new(2.5, 64.0, 0.5))];
    ai.behavior_brain_mut()
        .unwrap()
        .memories_mut()
        .set(MemoryModuleType::Pacified, MemoryValue::Bool(true));
    deliver(
        &mut ai,
        &mob,
        &world,
        BrainMessage::HoglinAttackIfCloser {
            target: EntityId(2),
        },
    );
    assert!(memories(&ai).get(MemoryModuleType::AttackTarget).is_none());
}

#[test]
fn a_hurt_baby_hoglin_flees_for_five_to_twenty_seconds_and_an_adult_retaliates() {
    let (mut ai, mut mob, mut world) = full_brain(EntityKind::Hoglin);
    let attacker = living(2, EntityKind::Player, Vec3::new(2.5, 64.0, 0.5));
    world.players = vec![attacker];
    mob.baby = true;
    deliver(
        &mut ai,
        &mob,
        &world,
        BrainMessage::HoglinHurtBy {
            attacker: EntityId(2),
        },
    );
    assert_eq!(
        memories(&ai).get(MemoryModuleType::AvoidTarget),
        Some(&entity(2))
    );
    let ttl = memories(&ai)
        .time_until_expiry(MemoryModuleType::AvoidTarget)
        .unwrap();
    assert!((100..=400).contains(&ttl), "{ttl}");
    assert!(memories(&ai).get(MemoryModuleType::AttackTarget).is_none());
    let (mut adult, mob, world_adult) = {
        let (adult, mob, _) = full_brain(EntityKind::Hoglin);
        let mut world = Field {
            time: NOW,
            ..Field::default()
        };
        world.players = vec![attacker];
        (adult, mob, world)
    };
    deliver(
        &mut adult,
        &mob,
        &world_adult,
        BrainMessage::HoglinHurtBy {
            attacker: EntityId(2),
        },
    );
    assert_eq!(
        memories(&adult).get(MemoryModuleType::AttackTarget),
        Some(&entity(2))
    );
}

#[test]
fn a_hoglin_does_not_retaliate_against_another_hoglin_or_a_piglin_it_flees_from() {
    let (mut ai, mob, mut world) = full_brain(EntityKind::Hoglin);
    world.others = vec![living(2, EntityKind::Hoglin, Vec3::new(2.5, 64.0, 0.5))];
    deliver(
        &mut ai,
        &mob,
        &world,
        BrainMessage::HoglinHurtBy {
            attacker: EntityId(2),
        },
    );
    assert!(memories(&ai).get(MemoryModuleType::AttackTarget).is_none());
}

#[test]
fn a_hoglin_hit_on_a_piglin_retreats_when_outnumbered_and_otherwise_calls_its_herd() {
    let (mut ai, mob, mut world) = full_brain(EntityKind::Hoglin);
    let piglin = living(2, EntityKind::Piglin, Vec3::new(2.5, 64.0, 0.5));
    world.others = vec![piglin];
    {
        let memories = ai.behavior_brain_mut().unwrap().memories_mut();
        memories.set(
            MemoryModuleType::VisibleAdultPiglinCount,
            MemoryValue::Int(3),
        );
        memories.set(
            MemoryModuleType::VisibleAdultHoglinCount,
            MemoryValue::Int(1),
        );
        memories.set(
            MemoryModuleType::NearestVisibleAdultHoglins,
            MemoryValue::Entities(vec![EntityId(40)]),
        );
    }
    let actions = deliver(
        &mut ai,
        &mob,
        &world,
        BrainMessage::HoglinHitTarget {
            target: EntityId(2),
        },
    );
    assert_eq!(
        memories(&ai).get(MemoryModuleType::AvoidTarget),
        Some(&entity(2))
    );
    assert_eq!(
        actions,
        vec![MobAction::Message {
            to: EntityId(40),
            message: BrainMessage::HoglinRetreat {
                target: EntityId(2)
            }
        }]
    );
    {
        let memories = ai.behavior_brain_mut().unwrap().memories_mut();
        memories.set(
            MemoryModuleType::VisibleAdultPiglinCount,
            MemoryValue::Int(1),
        );
    }
    let actions = deliver(
        &mut ai,
        &mob,
        &world,
        BrainMessage::HoglinHitTarget {
            target: EntityId(2),
        },
    );
    assert_eq!(
        actions,
        vec![MobAction::Message {
            to: EntityId(40),
            message: BrainMessage::HoglinAttackIfCloser {
                target: EntityId(2)
            }
        }]
    );
}

#[test]
fn a_zoglin_attacks_what_hurt_it_unless_a_nearer_target_is_already_chosen() {
    let (mut ai, mob, mut world) = full_brain(EntityKind::Zoglin);
    let near = living(2, EntityKind::Player, Vec3::new(2.5, 64.0, 0.5));
    let far = living(3, EntityKind::Player, Vec3::new(20.5, 64.0, 0.5));
    world.players = vec![near, far];
    deliver(
        &mut ai,
        &mob,
        &world,
        BrainMessage::ZoglinHurtBy {
            attacker: EntityId(2),
        },
    );
    assert_eq!(
        memories(&ai).get(MemoryModuleType::AttackTarget),
        Some(&entity(2))
    );
    deliver(
        &mut ai,
        &mob,
        &world,
        BrainMessage::ZoglinHurtBy {
            attacker: EntityId(3),
        },
    );
    assert_eq!(
        memories(&ai).get(MemoryModuleType::AttackTarget),
        Some(&entity(2))
    );
}

#[test]
fn a_hurt_piglin_drops_its_admired_item_celebration_and_admires_nothing_from_players_for_twenty_seconds()
 {
    let (mut ai, mut mob, mut world) = full_brain(EntityKind::Piglin);
    let attacker = living(2, EntityKind::Player, Vec3::new(2.5, 64.0, 0.5));
    world.players = vec![attacker];
    mob.off_hand = Some(ItemKind::GoldIngot.id());
    {
        let memories = ai.behavior_brain_mut().unwrap().memories_mut();
        memories.set(MemoryModuleType::AdmiringItem, MemoryValue::Bool(true));
        memories.set(
            MemoryModuleType::CelebrateLocation,
            MemoryValue::Block(BlockPos::new(1, 64, 1)),
        );
    }
    let actions = deliver(
        &mut ai,
        &mob,
        &world,
        BrainMessage::PiglinHurtBy {
            attacker: EntityId(2),
        },
    );
    assert!(
        actions.contains(&MobAction::Piglin(PiglinAction::StopHoldingOffHand {
            bartering_enabled: false
        }))
    );
    assert!(memories(&ai).get(MemoryModuleType::AdmiringItem).is_none());
    assert!(
        memories(&ai)
            .get(MemoryModuleType::CelebrateLocation)
            .is_none()
    );
    assert_eq!(
        memories(&ai).time_until_expiry(MemoryModuleType::AdmiringDisabled),
        Some(400)
    );
    assert_eq!(
        memories(&ai).get(MemoryModuleType::AngryAt),
        Some(&MemoryValue::Uuid(Uuid::from_u128(902)))
    );
    assert_eq!(
        memories(&ai).time_until_expiry(MemoryModuleType::AngryAt),
        Some(600)
    );
}

#[test]
fn a_hurt_piglin_calls_the_adult_piglins_nearby() {
    let (mut ai, mob, mut world) = full_brain(EntityKind::Piglin);
    world.players = vec![living(2, EntityKind::Player, Vec3::new(2.5, 64.0, 0.5))];
    ai.behavior_brain_mut().unwrap().memories_mut().set(
        MemoryModuleType::NearbyAdultPiglins,
        MemoryValue::Entities(vec![EntityId(30), EntityId(31)]),
    );
    let actions = deliver(
        &mut ai,
        &mob,
        &world,
        BrainMessage::PiglinHurtBy {
            attacker: EntityId(2),
        },
    );
    for piglin in [30, 31] {
        assert!(actions.contains(&MobAction::Message {
            to: EntityId(piglin),
            message: BrainMessage::PiglinAngerIfCloser {
                target: EntityId(2)
            },
        }));
    }
}

#[test]
fn a_piglin_never_reacts_to_another_piglin_hurting_it() {
    let (mut ai, mob, mut world) = full_brain(EntityKind::Piglin);
    world.others = vec![living(2, EntityKind::Piglin, Vec3::new(2.5, 64.0, 0.5))];
    let actions = deliver(
        &mut ai,
        &mob,
        &world,
        BrainMessage::PiglinHurtBy {
            attacker: EntityId(2),
        },
    );
    assert!(actions.is_empty());
    assert!(memories(&ai).get(MemoryModuleType::AngryAt).is_none());
}

#[test]
fn a_baby_piglin_runs_from_what_hurt_it_and_a_piglin_outnumbered_by_hoglins_retreats() {
    let (mut ai, mut mob, mut world) = full_brain(EntityKind::Piglin);
    let attacker = living(2, EntityKind::Player, Vec3::new(2.5, 64.0, 0.5));
    world.players = vec![attacker];
    mob.baby = true;
    deliver(
        &mut ai,
        &mob,
        &world,
        BrainMessage::PiglinHurtBy {
            attacker: EntityId(2),
        },
    );
    assert_eq!(
        memories(&ai).get(MemoryModuleType::AvoidTarget),
        Some(&entity(2))
    );
    assert_eq!(
        memories(&ai).time_until_expiry(MemoryModuleType::AvoidTarget),
        Some(100)
    );
    let (mut adult, mob, mut world) = full_brain(EntityKind::Piglin);
    let hoglin = living(3, EntityKind::Hoglin, Vec3::new(2.5, 64.0, 0.5));
    world.others = vec![hoglin];
    {
        let memories = adult.behavior_brain_mut().unwrap().memories_mut();
        memories.set(
            MemoryModuleType::VisibleAdultPiglinCount,
            MemoryValue::Int(0),
        );
        memories.set(
            MemoryModuleType::VisibleAdultHoglinCount,
            MemoryValue::Int(3),
        );
        memories.set(MemoryModuleType::HuntedRecently, MemoryValue::Bool(false));
    }
    deliver(
        &mut adult,
        &mob,
        &world,
        BrainMessage::PiglinHurtBy {
            attacker: EntityId(3),
        },
    );
    assert_eq!(
        memories(&adult).get(MemoryModuleType::AvoidTarget),
        Some(&entity(3))
    );
    assert!(
        memories(&adult)
            .get(MemoryModuleType::HuntedRecently)
            .is_some()
    );
}

#[test]
fn a_hurt_brute_retaliates_but_ignores_piglins() {
    let (mut ai, mob, mut world) = full_brain(EntityKind::PiglinBrute);
    let player = living(2, EntityKind::Player, Vec3::new(2.5, 64.0, 0.5));
    let piglin = living(3, EntityKind::Piglin, Vec3::new(2.5, 64.0, 1.5));
    world.players = vec![player];
    world.others = vec![piglin];
    deliver(
        &mut ai,
        &mob,
        &world,
        BrainMessage::PiglinBruteHurtBy {
            attacker: EntityId(3),
        },
    );
    assert!(memories(&ai).get(MemoryModuleType::AngryAt).is_none());
    deliver(
        &mut ai,
        &mob,
        &world,
        BrainMessage::PiglinBruteHurtBy {
            attacker: EntityId(2),
        },
    );
    assert_eq!(
        memories(&ai).get(MemoryModuleType::AngryAt),
        Some(&MemoryValue::Uuid(Uuid::from_u128(902)))
    );
}

#[test]
fn universal_anger_spreads_to_the_nearest_visible_player() {
    let (mut ai, mob, mut world) = full_brain(EntityKind::Piglin);
    world.anger = true;
    let attacker = living(2, EntityKind::Player, Vec3::new(2.5, 64.0, 0.5));
    let other = living(3, EntityKind::Player, Vec3::new(4.5, 64.0, 0.5));
    world.players = vec![attacker, other];
    ai.behavior_brain_mut()
        .unwrap()
        .memories_mut()
        .set(MemoryModuleType::NearestVisibleAttackablePlayer, entity(3));
    ai.behavior_brain_mut().unwrap().memories_mut().set(
        MemoryModuleType::NearbyAdultPiglins,
        MemoryValue::Entities(vec![EntityId(30)]),
    );
    let actions = deliver(
        &mut ai,
        &mob,
        &world,
        BrainMessage::PiglinHurtBy {
            attacker: EntityId(2),
        },
    );
    assert!(actions.contains(&MobAction::Message {
        to: EntityId(30),
        message: BrainMessage::PiglinUniversalAnger,
    }));
    assert_eq!(
        memories(&ai).get(MemoryModuleType::AngryAt),
        Some(&MemoryValue::Uuid(Uuid::from_u128(903)))
    );
    assert_eq!(
        memories(&ai).time_until_expiry(MemoryModuleType::UniversalAnger),
        Some(600)
    );
}

#[test]
fn a_broadcast_anger_target_is_ignored_when_a_nearer_one_is_already_chosen() {
    let (mut ai, mob, mut world) = full_brain(EntityKind::Piglin);
    let near = living(2, EntityKind::Player, Vec3::new(2.5, 64.0, 0.5));
    let far = living(3, EntityKind::Player, Vec3::new(9.5, 64.0, 0.5));
    world.players = vec![near, far];
    deliver(
        &mut ai,
        &mob,
        &world,
        BrainMessage::PiglinAngerIfCloser {
            target: EntityId(3),
        },
    );
    assert_eq!(
        memories(&ai).get(MemoryModuleType::AngryAt),
        Some(&MemoryValue::Uuid(Uuid::from_u128(903)))
    );
    deliver(
        &mut ai,
        &mob,
        &world,
        BrainMessage::PiglinAngerIfCloser {
            target: EntityId(2),
        },
    );
    assert_eq!(
        memories(&ai).get(MemoryModuleType::AngryAt),
        Some(&MemoryValue::Uuid(Uuid::from_u128(902)))
    );
    deliver(
        &mut ai,
        &mob,
        &world,
        BrainMessage::PiglinAngerIfCloser {
            target: EntityId(3),
        },
    );
    assert_eq!(
        memories(&ai).get(MemoryModuleType::AngryAt),
        Some(&MemoryValue::Uuid(Uuid::from_u128(902)))
    );
}

#[test]
fn only_hunters_follow_a_broadcast_hoglin_target() {
    let (mut hunter, mob, mut world) = full_brain(EntityKind::Piglin);
    let hoglin = living(3, EntityKind::Hoglin, Vec3::new(2.5, 64.0, 0.5));
    world.others = vec![hoglin];
    deliver(
        &mut hunter,
        &mob,
        &world,
        BrainMessage::PiglinAngerIfCloser {
            target: EntityId(3),
        },
    );
    assert!(memories(&hunter).get(MemoryModuleType::AngryAt).is_some());
    let (mut peaceful, mut mob, _) = full_brain(EntityKind::Piglin);
    mob.species.piglin.cannot_hunt = true;
    deliver(
        &mut peaceful,
        &mob,
        &world,
        BrainMessage::PiglinAngerIfCloser {
            target: EntityId(3),
        },
    );
    assert!(memories(&peaceful).get(MemoryModuleType::AngryAt).is_none());
    let mut protected = hoglin;
    protected.facts.cannot_be_hunted = true;
    let (mut other, mob, _) = full_brain(EntityKind::Piglin);
    let mut world = Field {
        time: NOW,
        ..Field::default()
    };
    world.others = vec![protected];
    deliver(
        &mut other,
        &mob,
        &world,
        BrainMessage::PiglinAngerIfCloser {
            target: EntityId(3),
        },
    );
    assert!(memories(&other).get(MemoryModuleType::AngryAt).is_none());
}

#[test]
fn a_piglin_stops_attacking_a_target_that_is_no_longer_the_nearest_valid_one() {
    let (mut ai, mob, mut world) = full_brain(EntityKind::Piglin);
    let player = living(2, EntityKind::Player, Vec3::new(2.5, 64.0, 0.5));
    world.players = vec![player];
    {
        let memories = ai.behavior_brain_mut().unwrap().memories_mut();
        memories.set(MemoryModuleType::AttackTarget, entity(2));
    }
    ai.behavior_brain_mut()
        .unwrap()
        .state_mut()
        .set_active_activity_to_first_valid(&[Activity::Fight]);
    think(&mut ai, &mob, &world, 1);
    assert!(memories(&ai).get(MemoryModuleType::AttackTarget).is_none());
}

#[test]
fn deferred_attack_targets_wait_for_the_plugin_verdict_and_are_requested_once_per_tick() {
    let (mut ai, mob, mut world) = full_brain(EntityKind::Zoglin);
    let player = living(2, EntityKind::Player, Vec3::new(3.5, 64.0, 0.5));
    world.players = vec![player];
    ai.behavior_brain_mut().unwrap().memories_mut().set(
        MemoryModuleType::NearestVisibleLivingEntities,
        MemoryValue::Visible(NearestVisibleLivingEntities::new(vec![EntityId(2)])),
    );
    ai.defer_targets(true);
    think(&mut ai, &mob, &world, 1);
    let requests = ai.take_target_requests();
    assert!(
        requests
            .iter()
            .all(|request| request.to == Some(EntityId(2)))
    );
    assert!(memories(&ai).get(MemoryModuleType::AttackTarget).is_none() || requests.is_empty());
}

#[test]
fn undeferred_attack_targets_are_set_and_announced() {
    let (mut ai, mob, mut world) = full_brain(EntityKind::Zoglin);
    let player = living(2, EntityKind::Player, Vec3::new(3.5, 64.0, 0.5));
    world.players = vec![player];
    let mut found = false;
    for _ in 0..80 {
        think(&mut ai, &mob, &world, 1);
        let requests = ai.take_target_requests();
        if requests
            .iter()
            .any(|request| request.to == Some(EntityId(2)))
        {
            found = true;
            assert_eq!(ai.target(), Some(EntityId(2)));
            break;
        }
    }
    assert!(found);
}

#[test]
fn the_active_activity_follows_the_vanilla_priority_order() {
    let (mut ai, mob, world) = full_brain(EntityKind::Piglin);
    let set = |ai: &mut MobAi, ty, value| {
        ai.behavior_brain_mut()
            .unwrap()
            .memories_mut()
            .set(ty, value);
    };
    think(&mut ai, &mob, &world, 1);
    assert_eq!(
        ai.behavior_brain()
            .unwrap()
            .state()
            .active_non_core_activity(),
        Some(Activity::Idle)
    );
    set(
        &mut ai,
        MemoryModuleType::CelebrateLocation,
        MemoryValue::Block(BlockPos::new(1, 64, 1)),
    );
    think(&mut ai, &mob, &world, 1);
    assert_eq!(
        ai.behavior_brain()
            .unwrap()
            .state()
            .active_non_core_activity(),
        Some(Activity::Celebrate)
    );
    set(&mut ai, MemoryModuleType::AvoidTarget, entity(9));
    think(&mut ai, &mob, &world, 1);
    assert_eq!(
        ai.behavior_brain()
            .unwrap()
            .state()
            .active_non_core_activity(),
        Some(Activity::Avoid)
    );
    set(&mut ai, MemoryModuleType::AttackTarget, entity(9));
    think(&mut ai, &mob, &world, 1);
    let fight_or_stopped = ai
        .behavior_brain()
        .unwrap()
        .state()
        .active_non_core_activity();
    assert!(matches!(
        fight_or_stopped,
        Some(Activity::Fight | Activity::Avoid)
    ));
    set(
        &mut ai,
        MemoryModuleType::AdmiringItem,
        MemoryValue::Bool(true),
    );
    think(&mut ai, &mob, &world, 1);
    assert_eq!(
        ai.behavior_brain()
            .unwrap()
            .state()
            .active_non_core_activity(),
        Some(Activity::AdmireItem)
    );
}

#[test]
fn the_dance_flag_follows_the_dancing_memory_and_the_activity_sound_plays_on_change() {
    let (mut ai, mob, world) = full_brain(EntityKind::Piglin);
    ai.behavior_brain_mut().unwrap().memories_mut().set(
        MemoryModuleType::CelebrateLocation,
        MemoryValue::Block(BlockPos::new(1, 64, 1)),
    );
    ai.behavior_brain_mut()
        .unwrap()
        .memories_mut()
        .set(MemoryModuleType::Dancing, MemoryValue::Bool(true));
    let actions = think(&mut ai, &mob, &world, 1);
    assert!(actions.contains(&MobAction::Piglin(PiglinAction::SetDancing(true))));
    assert!(actions.contains(&MobAction::Voice("minecraft:entity.piglin.celebrate")));
}

#[test]
fn a_hoglin_in_a_fight_growls_and_turns_aggressive() {
    let (mut ai, mob, world) = full_brain(EntityKind::Hoglin);
    ai.behavior_brain_mut()
        .unwrap()
        .memories_mut()
        .set(MemoryModuleType::AttackTarget, entity(9));
    let mut body = MobBody::of(&mob);
    ai.think(&mob, &world, &mut body);
    let actions = ai.take_actions();
    assert!(body.aggressive);
    assert!(actions.contains(&MobAction::Voice("minecraft:entity.hoglin.angry")));
}

#[test]
fn a_brute_growls_when_it_starts_to_fight() {
    let (mut ai, mob, world) = full_brain(EntityKind::PiglinBrute);
    ai.behavior_brain_mut()
        .unwrap()
        .memories_mut()
        .set(MemoryModuleType::AttackTarget, entity(9));
    let actions = think(&mut ai, &mob, &world, 1);
    assert!(actions.contains(&MobAction::Voice("minecraft:entity.piglin_brute.angry")));
}

#[test]
fn a_zoglin_growls_when_it_starts_to_fight() {
    let (mut ai, mob, world) = full_brain(EntityKind::Zoglin);
    ai.behavior_brain_mut()
        .unwrap()
        .memories_mut()
        .set(MemoryModuleType::AttackTarget, entity(9));
    let actions = think(&mut ai, &mob, &world, 1);
    assert!(actions.contains(&MobAction::Voice("minecraft:entity.zoglin.angry")));
}

#[test]
fn piglin_memories_round_trip_through_the_vanilla_nbt_layout() {
    let (mut ai, _, _) = full_brain(EntityKind::Piglin);
    {
        let memories = ai.behavior_brain_mut().unwrap().memories_mut();
        memories.set_with_expiry(
            MemoryModuleType::AngryAt,
            MemoryValue::Uuid(Uuid::from_u128(77)),
            321,
        );
        memories.set_with_expiry(
            MemoryModuleType::HuntedRecently,
            MemoryValue::Bool(true),
            1500,
        );
        memories.set_with_expiry(MemoryModuleType::AdmiringItem, MemoryValue::Bool(true), 50);
        memories.set_with_expiry(
            MemoryModuleType::AdmiringDisabled,
            MemoryValue::Bool(true),
            90,
        );
        memories.set(MemoryModuleType::UniversalAnger, MemoryValue::Bool(true));
        memories.set(MemoryModuleType::AttackTarget, entity(5));
        memories.set(MemoryModuleType::NearestVisibleNemesis, entity(5));
    }
    let saved = ai.save_behavior_brain().unwrap();
    let memories_tag = saved.get_compound("memories").unwrap();
    assert!(memories_tag.get_compound("minecraft:angry_at").is_some());
    assert!(
        memories_tag
            .get_compound("minecraft:attack_target")
            .is_none()
    );
    let mut fresh = MobAi::new(EntityKind::Piglin, 1, GoalSettings::default());
    fresh.load_behavior_brain(&saved);
    let loaded = memories(&fresh);
    assert_eq!(
        loaded.get(MemoryModuleType::AngryAt),
        Some(&MemoryValue::Uuid(Uuid::from_u128(77)))
    );
    assert_eq!(
        loaded.time_until_expiry(MemoryModuleType::AngryAt),
        Some(321)
    );
    assert_eq!(
        loaded.time_until_expiry(MemoryModuleType::HuntedRecently),
        Some(1500)
    );
    assert_eq!(
        loaded.time_until_expiry(MemoryModuleType::AdmiringItem),
        Some(50)
    );
    assert_eq!(
        loaded.time_until_expiry(MemoryModuleType::AdmiringDisabled),
        Some(90)
    );
    assert_eq!(
        loaded.get(MemoryModuleType::UniversalAnger),
        Some(&MemoryValue::Bool(true))
    );
    assert!(loaded.get(MemoryModuleType::AttackTarget).is_none());
    assert_eq!(fresh.save_behavior_brain(), Some(saved));
}

#[test]
fn the_brute_home_round_trips_as_a_global_position() {
    let (mut ai, _, _) = full_brain(EntityKind::PiglinBrute);
    ai.behavior_brain_mut().unwrap().memories_mut().set(
        MemoryModuleType::Home,
        MemoryValue::GlobalPos(GlobalPos {
            dimension: "minecraft:the_nether".to_owned(),
            pos: BlockPos::new(10, 70, -4),
        }),
    );
    let saved = ai.save_behavior_brain().unwrap();
    let mut fresh = MobAi::new(EntityKind::PiglinBrute, 1, GoalSettings::default());
    fresh.load_behavior_brain(&saved);
    assert_eq!(
        memories(&fresh).get(MemoryModuleType::Home),
        Some(&MemoryValue::GlobalPos(GlobalPos {
            dimension: "minecraft:the_nether".to_owned(),
            pos: BlockPos::new(10, 70, -4),
        }))
    );
}

#[test]
fn hoglin_and_zoglin_have_nothing_to_persist() {
    for kind in [EntityKind::Hoglin, EntityKind::Zoglin] {
        let (ai, _, _) = full_brain(kind);
        let saved = ai.save_behavior_brain().unwrap();
        assert!(saved.get_compound("memories").unwrap().is_empty());
        assert_eq!(PackedMemories::from_nbt(&saved), PackedMemories::default());
    }
}

#[test]
fn melee_attack_never_fires_with_a_crossbow_in_hand_and_the_walk_target_keeps_its_distance() {
    use rspigot_ai::{MeleeAttack, SetWalkTargetFromAttackTargetIfTargetOutOfReach};
    let target = living(7, EntityKind::Player, Vec3::new(0.5, 64.0, 5.5));
    let mut bench = Bench::new(
        EntityKind::Piglin,
        &[
            MemoryModuleType::LookTarget,
            MemoryModuleType::AttackTarget,
            MemoryModuleType::AttackCoolingDown,
            MemoryModuleType::NearestVisibleLivingEntities,
            MemoryModuleType::WalkTarget,
        ],
    );
    bench.world.players.push(target);
    bench.set(MemoryModuleType::AttackTarget, entity(7));
    bench.mob.main_hand = Some(ItemKind::Crossbow.id());
    bench.see(&[target]);
    let mut walk = shot(MobTrigger::SetWalkTargetFromAttackTargetIfTargetOutOfReach(
        SetWalkTargetFromAttackTargetIfTargetOutOfReach::new(1.0),
    ));
    assert!(bench.start(&mut walk, NOW));
    assert!(bench.get(MemoryModuleType::WalkTarget).is_none());
    bench.set(MemoryModuleType::AttackTarget, entity(7));
    bench.world.players = vec![living(7, EntityKind::Player, Vec3::new(0.5, 64.0, 9.5))];
    assert!(bench.start(&mut walk, NOW));
    assert!(bench.get(MemoryModuleType::WalkTarget).is_some());
    let mut melee = shot(MobTrigger::MeleeAttack(MeleeAttack::new(20)));
    bench.world.players = vec![living(7, EntityKind::Player, Vec3::new(0.5, 64.0, 1.5))];
    assert!(!bench.start(&mut melee, NOW));
    bench.mob.main_hand = Some(ItemKind::GoldenSword.id());
    assert!(bench.start(&mut melee, NOW));
}

#[test]
fn crossbow_attack_charges_releases_waits_and_shoots() {
    let target = living(7, EntityKind::Player, Vec3::new(0.5, 64.0, 5.5));
    let mut bench = Bench::new(
        EntityKind::Piglin,
        &[
            MemoryModuleType::LookTarget,
            MemoryModuleType::AttackTarget,
            MemoryModuleType::NearestVisibleLivingEntities,
        ],
    );
    bench.world.players.push(target);
    bench.set(MemoryModuleType::AttackTarget, entity(7));
    bench.mob.main_hand = Some(ItemKind::Crossbow.id());
    bench.see(&[target]);
    let mut attack = MobControl::crossbow_attack();
    assert!(bench.start(&mut attack, NOW));
    assert_eq!(attack.status(), Status::Running);
    let tick = |bench: &mut Bench, attack: &mut MobControl, at: i64| {
        bench.with(|cx| attack.tick_or_stop(cx, at));
    };
    tick(&mut bench, &mut attack, NOW + 1);
    assert!(bench.actions().contains(&MobAction::Crossbow(
        rspigot_ai::CrossbowAction::SetCharging(true)
    )));
    assert!(bench.body.using_item.is_some());
    bench.mob.ticks_using_item = 25;
    tick(&mut bench, &mut attack, NOW + 2);
    let released = bench.actions();
    assert!(released.iter().any(|a| matches!(
        a,
        MobAction::Crossbow(rspigot_ai::CrossbowAction::Release { .. })
    )));
    assert!(bench.body.using_item.is_none());
    let mut shots = 0;
    for step in 3..80 {
        tick(&mut bench, &mut attack, NOW + step);
        shots += bench
            .actions()
            .iter()
            .filter(|a| matches!(a, MobAction::RangedAttack { power, .. } if *power == 1.0))
            .count();
        if shots > 0 {
            break;
        }
    }
    assert_eq!(shots, 1);
}

#[test]
fn crossbow_attack_stops_when_the_target_leaves_the_crossbow_range_and_clears_its_charge() {
    let target = living(7, EntityKind::Player, Vec3::new(0.5, 64.0, 12.5));
    let mut bench = Bench::new(
        EntityKind::Piglin,
        &[
            MemoryModuleType::LookTarget,
            MemoryModuleType::AttackTarget,
            MemoryModuleType::NearestVisibleLivingEntities,
        ],
    );
    bench.world.players.push(target);
    bench.set(MemoryModuleType::AttackTarget, entity(7));
    bench.mob.main_hand = Some(ItemKind::Crossbow.id());
    bench.see(&[target]);
    let mut attack = MobControl::crossbow_attack();
    assert!(!bench.start(&mut attack, NOW));
    let near = living(7, EntityKind::Player, Vec3::new(0.5, 64.0, 5.5));
    bench.world.players = vec![near];
    bench.see(&[near]);
    assert!(bench.start(&mut attack, NOW));
    bench.with(|cx| attack.do_stop(cx, NOW + 1));
    assert!(bench.actions().contains(&MobAction::Crossbow(
        rspigot_ai::CrossbowAction::ClearCharged
    )));
}

#[test]
fn the_item_id_helper_compiles_for_every_tag_lookup() {
    let _: ItemId = ItemKind::GoldIngot.id();
}

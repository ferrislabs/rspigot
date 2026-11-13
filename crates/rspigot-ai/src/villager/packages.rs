use rspigot_domain::entity::species::villager::Profession;
use rspigot_entities::{EntityKind, MobCategory};

use crate::behavior::HurtBySensor;
use crate::behavior::{
    Activity, ActivityData, DoNothing, EntityFilter, Gate, GoToWantedItem, InteractWith,
    InteractWithDoor, ItemWish, MemoryModuleType, MemoryStatus, MobBehavior, MobControl,
    MobPredicate, MobTrigger, NearestItemSensor, NearestLivingEntitySensor, OneShot, OrderPolicy,
    PlayerSensor, RunningPolicy, SensorKind, SetEntityLookTarget, SetLookAndInteract,
    SetWalkTargetAwayFrom, SetWalkTargetFromLookTarget, StrollAroundPoi, StrollToPoi, TriggerGate,
};
use crate::villager::{
    AcquirePoi, AssignProfessionFromJobSite, GiveGiftToHero, GoToClosestVillage,
    GoToPotentialJobSite, GolemSensor, HOME_EVENT, HarvestFarmland, InsideBrownianWalk, JumpOnBed,
    LocateHidingPlace, LookAndFollowTradingPlayerSink, MoveToSkySeeingSpot, NearestBedSensor,
    PlayTagWithOtherKids, PoiCompetitorScan, PoiRule, ReactToBell, ResetProfession,
    ResetRaidStatus, RingBell, SecondaryPoiSensor, SetClosestHomeAsWalkTarget, SetHiddenState,
    SetRaidStatus, SetWalkTargetFromBlockMemory, ShowTradesToPlayer, SleepInBed, SocializeAtBell,
    StrollToPoiList, TradeWithVillager, UpdateActivityFromSchedule, ValidateNearbyPoi,
    VillageBoundRandomStroll, VillagerBabiesSensor, VillagerCalmDown, VillagerHostilesSensor,
    VillagerMakeLove, VillagerPanicTrigger, WakeUp, WorkAtPoi, Workstation, YieldJobSite,
};

pub const SPEED: f32 = 0.5;
const SWIM_CHANCE: f32 = 0.8;
const LOOK_RANGE: f32 = 8.0;

type Pairs = Vec<(i32, MobControl)>;

fn trigger(trigger: MobTrigger) -> MobControl {
    MobControl::one_shot(trigger)
}

fn look(filter: EntityFilter) -> MobTrigger {
    MobTrigger::SetEntityLookTarget(SetEntityLookTarget::new(filter, LOOK_RANGE))
}

fn interact_with(kind: EntityKind) -> MobControl {
    trigger(MobTrigger::InteractWith(InteractWith::new(
        kind,
        8,
        MemoryModuleType::InteractionTarget,
        SPEED,
        2,
    )))
}

fn interact_breeding() -> MobControl {
    trigger(MobTrigger::InteractWith(InteractWith::breeding_villagers(
        8,
        MemoryModuleType::BreedTarget,
        SPEED,
        2,
    )))
}

fn do_nothing(min: i32, max: i32) -> MobControl {
    MobControl::DoNothing(DoNothing::new(min, max))
}

fn run_one(weighted: Vec<(MobControl, i32)>) -> MobControl {
    MobControl::Gate(Gate::run_one(weighted))
}

fn minimal_look() -> (i32, MobControl) {
    (
        5,
        run_one(vec![
            (trigger(look(EntityFilter::Kind(EntityKind::Villager))), 2),
            (trigger(look(EntityFilter::Kind(EntityKind::Player))), 2),
            (do_nothing(30, 60), 8),
        ]),
    )
}

fn full_look() -> (i32, MobControl) {
    (
        5,
        run_one(vec![
            (trigger(look(EntityFilter::Kind(EntityKind::Cat))), 8),
            (trigger(look(EntityFilter::Kind(EntityKind::Villager))), 2),
            (trigger(look(EntityFilter::Kind(EntityKind::Player))), 2),
            (
                trigger(look(EntityFilter::Category(MobCategory::Creature))),
                1,
            ),
            (
                trigger(look(EntityFilter::Category(MobCategory::WaterCreature))),
                1,
            ),
            (
                trigger(look(EntityFilter::Category(MobCategory::Axolotls))),
                1,
            ),
            (
                trigger(look(EntityFilter::Category(
                    MobCategory::UndergroundWaterCreature,
                ))),
                1,
            ),
            (
                trigger(look(EntityFilter::Category(MobCategory::WaterAmbient))),
                1,
            ),
            (
                trigger(look(EntityFilter::Category(MobCategory::Monster))),
                1,
            ),
            (do_nothing(30, 60), 2),
        ]),
    )
}

fn update_schedule() -> (i32, MobControl) {
    (
        99,
        trigger(MobTrigger::UpdateActivityFromSchedule(
            UpdateActivityFromSchedule,
        )),
    )
}

fn village_stroll(speed: f32) -> MobControl {
    trigger(MobTrigger::VillageBoundRandomStroll(
        VillageBoundRandomStroll::new(speed),
    ))
}

fn walk_from_look() -> MobControl {
    trigger(MobTrigger::SetWalkTargetFromLookTarget(
        SetWalkTargetFromLookTarget::new(SPEED, 2),
    ))
}

fn declared_gate(weighted: Vec<(MobTrigger, i32)>) -> OneShot<MobTrigger> {
    let triggers = weighted
        .into_iter()
        .map(|(trigger, weight)| (MobControl::declared(trigger), weight))
        .collect();
    OneShot::new(
        Vec::new(),
        MobTrigger::Gate(TriggerGate::new(
            triggers,
            OrderPolicy::Shuffled,
            RunningPolicy::RunOne,
        )),
    )
}

fn trade_gate(exit: MemoryModuleType, control: MobControl) -> MobControl {
    MobControl::Gate(Gate::new(
        Vec::new(),
        vec![exit],
        OrderPolicy::Ordered,
        RunningPolicy::RunOne,
        vec![(control, 1)],
    ))
}

fn trade_with_villager() -> MobControl {
    MobControl::behavior(MobBehavior::TradeWithVillager(TradeWithVillager::new()))
}

fn make_love() -> MobControl {
    MobControl::behavior_for(
        MobBehavior::VillagerMakeLove(VillagerMakeLove::new()),
        VillagerMakeLove::DURATION,
        VillagerMakeLove::DURATION,
    )
}

fn show_trades() -> MobControl {
    MobControl::behavior_for(
        MobBehavior::ShowTradesToPlayer(ShowTradesToPlayer::new()),
        400,
        1600,
    )
}

fn give_gift() -> MobControl {
    MobControl::behavior_for(
        MobBehavior::GiveGiftToHero(GiveGiftToHero::new()),
        GiveGiftToHero::TIMEOUT,
        GiveGiftToHero::TIMEOUT,
    )
}

fn jump_on_bed() -> MobControl {
    MobControl::behavior(MobBehavior::JumpOnBed(JumpOnBed::new(SPEED)))
}

fn poi_validator(rule: PoiRule, memory: MemoryModuleType) -> MobControl {
    trigger(MobTrigger::ValidateNearbyPoi(ValidateNearbyPoi::new(
        rule, memory,
    )))
}

fn core_package(profession: Profession) -> Pairs {
    let _ = profession;
    vec![
        (0, MobControl::swim(SWIM_CHANCE)),
        (
            0,
            trigger(MobTrigger::InteractWithDoor(InteractWithDoor::default())),
        ),
        (0, MobControl::default_look_at_target_sink()),
        (
            0,
            MobControl::behavior(MobBehavior::VillagerPanicTrigger(VillagerPanicTrigger)),
        ),
        (0, trigger(MobTrigger::WakeUp(WakeUp))),
        (0, trigger(MobTrigger::ReactToBell(ReactToBell))),
        (0, trigger(MobTrigger::SetRaidStatus(SetRaidStatus))),
        (
            0,
            poi_validator(PoiRule::HeldJobSite, MemoryModuleType::JobSite),
        ),
        (
            0,
            poi_validator(
                PoiRule::AcquirableJobSite,
                MemoryModuleType::PotentialJobSite,
            ),
        ),
        (1, MobControl::move_to_target_sink()),
        (2, trigger(MobTrigger::PoiCompetitorScan(PoiCompetitorScan))),
        (
            3,
            MobControl::behavior(MobBehavior::LookAndFollowTradingPlayerSink(
                LookAndFollowTradingPlayerSink::new(SPEED),
            )),
        ),
        (
            5,
            trigger(MobTrigger::GoToWantedItem(GoToWantedItem::new(
                SPEED, false, 4,
            ))),
        ),
        (
            6,
            trigger(MobTrigger::AcquirePoi(AcquirePoi::new(
                PoiRule::AcquirableJobSite,
                MemoryModuleType::JobSite,
                MemoryModuleType::PotentialJobSite,
                true,
                None,
            ))),
        ),
        (
            7,
            MobControl::behavior_for(
                MobBehavior::GoToPotentialJobSite(GoToPotentialJobSite::new(SPEED)),
                GoToPotentialJobSite::TIMEOUT,
                GoToPotentialJobSite::TIMEOUT,
            ),
        ),
        (
            8,
            trigger(MobTrigger::YieldJobSite(YieldJobSite::new(SPEED))),
        ),
        (
            10,
            trigger(MobTrigger::AcquirePoi(AcquirePoi::new(
                PoiRule::Home,
                MemoryModuleType::Home,
                MemoryModuleType::Home,
                false,
                Some(HOME_EVENT),
            ))),
        ),
        (
            10,
            trigger(MobTrigger::AcquirePoi(AcquirePoi::new(
                PoiRule::Meeting,
                MemoryModuleType::MeetingPoint,
                MemoryModuleType::MeetingPoint,
                true,
                Some(HOME_EVENT),
            ))),
        ),
        (
            10,
            trigger(MobTrigger::AssignProfessionFromJobSite(
                AssignProfessionFromJobSite,
            )),
        ),
        (10, trigger(MobTrigger::ResetProfession(ResetProfession))),
    ]
}

fn work_package(profession: Profession) -> Pairs {
    let farmer = profession == Profession::Farmer;
    let workstation = if farmer {
        Workstation::Composter
    } else {
        Workstation::Plain
    };
    let choices = vec![
        (
            MobControl::behavior(MobBehavior::WorkAtPoi(WorkAtPoi::new(workstation))),
            7,
        ),
        (
            trigger(MobTrigger::StrollAroundPoi(StrollAroundPoi::new(
                MemoryModuleType::JobSite,
                0.4,
                4,
            ))),
            2,
        ),
        (
            trigger(MobTrigger::StrollToPoi(StrollToPoi::new(
                MemoryModuleType::JobSite,
                0.4,
                1,
                10,
            ))),
            5,
        ),
        (
            trigger(MobTrigger::StrollToPoiList(StrollToPoiList::new(
                MemoryModuleType::SecondaryJobSite,
                SPEED,
                1,
                6,
                MemoryModuleType::JobSite,
            ))),
            5,
        ),
        (
            MobControl::behavior(MobBehavior::HarvestFarmland(HarvestFarmland::new())),
            if farmer { 2 } else { 5 },
        ),
    ];
    vec![
        minimal_look(),
        (5, run_one(choices)),
        (10, show_trades()),
        (
            10,
            trigger(MobTrigger::SetLookAndInteract(SetLookAndInteract::new(
                EntityKind::Player,
                4,
            ))),
        ),
        (
            2,
            trigger(MobTrigger::SetWalkTargetFromBlockMemory(
                SetWalkTargetFromBlockMemory::new(MemoryModuleType::JobSite, SPEED, 9, 100, 1200),
            )),
        ),
        (3, give_gift()),
        update_schedule(),
    ]
}

fn play_package() -> Pairs {
    let idle_choices = vec![
        (interact_with(EntityKind::Villager), 2),
        (interact_with(EntityKind::Cat), 1),
        (village_stroll(SPEED), 1),
        (walk_from_look(), 1),
        (jump_on_bed(), 2),
        (do_nothing(20, 40), 2),
    ];
    vec![
        (0, MobControl::move_to_target_sink_for(80, 120)),
        full_look(),
        (
            5,
            trigger(MobTrigger::PlayTagWithOtherKids(PlayTagWithOtherKids)),
        ),
        (
            5,
            MobControl::Gate(Gate::run_one_when(
                vec![(
                    MemoryModuleType::VisibleVillagerBabies,
                    MemoryStatus::ValueAbsent,
                )],
                idle_choices,
            )),
        ),
        update_schedule(),
    ]
}

fn rest_package() -> Pairs {
    vec![
        (
            2,
            trigger(MobTrigger::SetWalkTargetFromBlockMemory(
                SetWalkTargetFromBlockMemory::new(MemoryModuleType::Home, SPEED, 1, 150, 1200),
            )),
        ),
        (3, poi_validator(PoiRule::Home, MemoryModuleType::Home)),
        (
            3,
            MobControl::behavior(MobBehavior::SleepInBed(SleepInBed::new())),
        ),
        (
            5,
            MobControl::Gate(Gate::run_one_when(
                vec![(MemoryModuleType::Home, MemoryStatus::ValueAbsent)],
                vec![
                    (
                        trigger(MobTrigger::SetClosestHomeAsWalkTarget(
                            SetClosestHomeAsWalkTarget::new(SPEED),
                        )),
                        1,
                    ),
                    (
                        trigger(MobTrigger::InsideBrownianWalk(InsideBrownianWalk::new(
                            SPEED,
                        ))),
                        4,
                    ),
                    (
                        trigger(MobTrigger::GoToClosestVillage(GoToClosestVillage::new(
                            SPEED, 4,
                        ))),
                        2,
                    ),
                    (do_nothing(20, 40), 2),
                ],
            )),
        ),
        minimal_look(),
        update_schedule(),
    ]
}

fn meet_package() -> Pairs {
    vec![
        (
            2,
            MobControl::trigger_one_shuffled(vec![
                (
                    MobControl::declared(MobTrigger::StrollAroundPoi(StrollAroundPoi::new(
                        MemoryModuleType::MeetingPoint,
                        0.4,
                        40,
                    ))),
                    2,
                ),
                (
                    MobControl::declared(MobTrigger::SocializeAtBell(SocializeAtBell)),
                    2,
                ),
            ]),
        ),
        (10, show_trades()),
        (
            10,
            trigger(MobTrigger::SetLookAndInteract(SetLookAndInteract::new(
                EntityKind::Player,
                4,
            ))),
        ),
        (
            2,
            trigger(MobTrigger::SetWalkTargetFromBlockMemory(
                SetWalkTargetFromBlockMemory::new(
                    MemoryModuleType::MeetingPoint,
                    SPEED,
                    6,
                    100,
                    200,
                ),
            )),
        ),
        (3, give_gift()),
        (
            3,
            poi_validator(PoiRule::Meeting, MemoryModuleType::MeetingPoint),
        ),
        (
            3,
            trade_gate(MemoryModuleType::InteractionTarget, trade_with_villager()),
        ),
        full_look(),
        update_schedule(),
    ]
}

fn idle_package() -> Pairs {
    vec![
        (
            2,
            run_one(vec![
                (interact_with(EntityKind::Villager), 2),
                (interact_breeding(), 1),
                (interact_with(EntityKind::Cat), 1),
                (village_stroll(SPEED), 1),
                (walk_from_look(), 1),
                (jump_on_bed(), 1),
                (do_nothing(30, 60), 1),
            ]),
        ),
        (3, give_gift()),
        (
            3,
            trigger(MobTrigger::SetLookAndInteract(SetLookAndInteract::new(
                EntityKind::Player,
                4,
            ))),
        ),
        (3, show_trades()),
        (
            3,
            trade_gate(MemoryModuleType::InteractionTarget, trade_with_villager()),
        ),
        (3, trade_gate(MemoryModuleType::BreedTarget, make_love())),
        full_look(),
        update_schedule(),
    ]
}

fn panic_package() -> Pairs {
    let fast = SPEED * 1.5;
    vec![
        (0, trigger(MobTrigger::VillagerCalmDown(VillagerCalmDown))),
        (
            1,
            trigger(MobTrigger::SetWalkTargetAwayFrom(
                SetWalkTargetAwayFrom::entity(MemoryModuleType::NearestHostile, fast, 6, false),
            )),
        ),
        (
            1,
            trigger(MobTrigger::SetWalkTargetAwayFrom(
                SetWalkTargetAwayFrom::entity(MemoryModuleType::HurtByEntity, fast, 6, false),
            )),
        ),
        (
            3,
            trigger(MobTrigger::VillageBoundRandomStroll(
                VillageBoundRandomStroll::with_range(fast, 2, 2),
            )),
        ),
        minimal_look(),
    ]
}

fn pre_raid_package() -> Pairs {
    vec![
        (0, trigger(MobTrigger::RingBell(RingBell))),
        (
            0,
            MobControl::trigger_one_shuffled(vec![
                (
                    MobControl::declared(MobTrigger::SetWalkTargetFromBlockMemory(
                        SetWalkTargetFromBlockMemory::new(
                            MemoryModuleType::MeetingPoint,
                            SPEED * 1.5,
                            2,
                            150,
                            200,
                        ),
                    )),
                    6,
                ),
                (
                    MobControl::declared(MobTrigger::VillageBoundRandomStroll(
                        VillageBoundRandomStroll::new(SPEED * 1.5),
                    )),
                    2,
                ),
            ]),
        ),
        minimal_look(),
        (99, trigger(MobTrigger::ResetRaidStatus(ResetRaidStatus))),
    ]
}

fn raid_package() -> Pairs {
    vec![
        (
            0,
            MobControl::trigger_if(
                MobPredicate::RaidNotVictory,
                declared_gate(vec![
                    (
                        MobTrigger::MoveToSkySeeingSpot(MoveToSkySeeingSpot::new(SPEED)),
                        5,
                    ),
                    (
                        MobTrigger::VillageBoundRandomStroll(VillageBoundRandomStroll::new(
                            SPEED * 1.1,
                        )),
                        2,
                    ),
                ]),
            ),
        ),
        (
            2,
            MobControl::trigger_if(
                MobPredicate::RaidActive,
                MobControl::declared(MobTrigger::LocateHidingPlace(LocateHidingPlace::new(
                    24,
                    SPEED * 1.4,
                    1,
                ))),
            ),
        ),
        minimal_look(),
        (99, trigger(MobTrigger::ResetRaidStatus(ResetRaidStatus))),
    ]
}

fn hide_package() -> Pairs {
    vec![
        (
            0,
            trigger(MobTrigger::SetHiddenState(SetHiddenState::new(15, 3))),
        ),
        (
            1,
            trigger(MobTrigger::LocateHidingPlace(LocateHidingPlace::new(
                32,
                SPEED * 1.25,
                2,
            ))),
        ),
        minimal_look(),
    ]
}

pub fn villager_sensors() -> Vec<SensorKind> {
    vec![
        SensorKind::NearestLivingEntities(NearestLivingEntitySensor),
        SensorKind::NearestPlayers(PlayerSensor),
        SensorKind::NearestItems(NearestItemSensor::new(ItemWish::Villager)),
        SensorKind::NearestBed(NearestBedSensor::default()),
        SensorKind::HurtBy(HurtBySensor),
        SensorKind::VillagerHostiles(VillagerHostilesSensor),
        SensorKind::VillagerBabies(VillagerBabiesSensor),
        SensorKind::SecondaryPois(SecondaryPoiSensor),
        SensorKind::GolemDetected(GolemSensor),
    ]
}

pub fn villager_activities(profession: Profession, baby: bool) -> Vec<ActivityData<MobControl>> {
    let mut activities = Vec::new();
    if baby {
        activities.push(ActivityData::from_pairs(Activity::Play, play_package()));
    } else {
        activities.push(ActivityData::with_conditions(
            Activity::Work,
            work_package(profession),
            vec![(MemoryModuleType::JobSite, MemoryStatus::ValuePresent)],
        ));
    }
    activities.push(ActivityData::from_pairs(
        Activity::Core,
        core_package(profession),
    ));
    activities.push(ActivityData::with_conditions(
        Activity::Meet,
        meet_package(),
        vec![(MemoryModuleType::MeetingPoint, MemoryStatus::ValuePresent)],
    ));
    activities.push(ActivityData::from_pairs(Activity::Rest, rest_package()));
    activities.push(ActivityData::from_pairs(Activity::Idle, idle_package()));
    activities.push(ActivityData::from_pairs(Activity::Panic, panic_package()));
    activities.push(ActivityData::from_pairs(
        Activity::PreRaid,
        pre_raid_package(),
    ));
    activities.push(ActivityData::from_pairs(Activity::Raid, raid_package()));
    activities.push(ActivityData::from_pairs(Activity::Hide, hide_package()));
    activities
}

use crate::armadillo::{ArmadilloBallUp, ArmadilloPanic, RollOutWithoutDanger};
use crate::behavior::attack::{
    EraseMemoryIf, MeleeAttack, SetWalkTargetFromAttackTargetIfTargetOutOfReach, StartAttacking,
    StopAttackingIfTargetInvalid, StopBeingAngryIfTargetDead,
};
use crate::behavior::control::{
    Behavior, BehaviorControl, BehaviorCx, DoNothing, Gate, OneShot, OrderPolicy, RunningPolicy,
    Sequence, Status, Timed, Trigger, TriggerGate,
};
use crate::behavior::long_jump::{LongJumpMidJump, LongJumpToRandomPos};
use crate::behavior::love::{AnimalMakeLove, FollowTemptation, MAKE_LOVE_DURATION};
use crate::behavior::memory::{Condition, MemoryModuleType, MemoryStatus, MemoryValue};
use crate::behavior::ram::{PrepareRamNearestTarget, RamTarget};
use crate::behavior::sinks::{
    AnimalPanic, LookAtTargetSink, MoveToTargetSink, RandomLookAround, Swim,
};
use crate::behavior::support::MobPredicate;
use crate::behavior::walk::{
    BabyFollowAdult, GoToTargetLocation, GoToWantedItem, InteractWith, RandomStroll,
    SetEntityLookTarget, SetEntityLookTargetSometimes, SetLookAndInteract, SetWalkTargetAwayFrom,
    SetWalkTargetFromLookTarget, StrollAroundPoi, StrollToPoi,
};
use crate::breeze::{
    LongJump as BreezeLongJump, Shoot as BreezeShoot, ShootWhenStuck as BreezeShootWhenStuck,
    Slide as BreezeSlide, SlideToTargetSink,
};
use crate::piglin::{
    BackUpIfTooClose, BecomePassiveIfMemoryPresent, CROSSBOW_ATTACK_DURATION, CopyMemoryWithExpiry,
    CrossbowAttack, RememberIfHoglinWasKilled, StartAdmiringItemIfSeen,
    StartCelebratingIfTargetDead, StartHuntingHoglin, StopAdmiringIfItemTooFarAway,
    StopAdmiringIfTiredOfTryingToReachItem, StopHoldingItemIfNoLongerAdmiring,
};
use crate::villager::{
    AcquirePoi, AssignProfessionFromJobSite, GiveGiftToHero, GoToClosestVillage,
    GoToPotentialJobSite, HarvestFarmland, InsideBrownianWalk, JumpOnBed, LocateHidingPlace,
    LookAndFollowTradingPlayerSink, MoveToSkySeeingSpot, PlayTagWithOtherKids, PoiCompetitorScan,
    ReactToBell, ResetProfession, ResetRaidStatus, RingBell, SetClosestHomeAsWalkTarget,
    SetHiddenState, SetRaidStatus, SetWalkTargetFromBlockMemory, ShowTradesToPlayer, SleepInBed,
    SocializeAtBell, StrollToPoiList, TradeWithVillager, UpdateActivityFromSchedule,
    ValidateNearbyPoi, VillageBoundRandomStroll, VillagerCalmDown, VillagerMakeLove,
    VillagerPanicTrigger, WakeUp, WorkAtPoi, YieldJobSite,
};
use crate::world::AiWorld;

const LOOK_SINK_MIN_DURATION: i32 = 45;
const LOOK_SINK_MAX_DURATION: i32 = 90;

#[derive(Debug, Clone, PartialEq)]
pub struct CountDownCooldownTicks {
    cooldown_ticks: MemoryModuleType,
    condition: [Condition; 1],
}

impl CountDownCooldownTicks {
    pub fn new(cooldown_ticks: MemoryModuleType) -> Self {
        Self {
            cooldown_ticks,
            condition: [(cooldown_ticks, MemoryStatus::ValuePresent)],
        }
    }

    fn cooldown<W: AiWorld>(&self, cx: &BehaviorCx<'_, '_, W>) -> Option<i32> {
        cx.brain
            .memories
            .get(self.cooldown_ticks)
            .and_then(MemoryValue::as_int)
    }
}

impl Behavior for CountDownCooldownTicks {
    fn entry_condition(&self) -> &[Condition] {
        &self.condition
    }

    fn timed_out(&self, _end_timestamp: i64, _now: i64) -> bool {
        false
    }

    fn can_still_use<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, _now: i64) -> bool {
        self.cooldown(cx).is_some_and(|ticks| 0 < ticks)
    }

    fn tick<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, _now: i64) {
        if let Some(ticks) = self.cooldown(cx) {
            cx.brain
                .memories
                .set(self.cooldown_ticks, MemoryValue::Int(ticks - 1));
        }
    }

    fn stop<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, _now: i64) {
        cx.brain.memories.erase(self.cooldown_ticks);
    }
}

macro_rules! mob_behaviors {
    ($($variant:ident($ty:ty)),* $(,)?) => {
        #[derive(Debug, Clone, PartialEq)]
        pub enum MobBehavior {
            $($variant($ty),)*
        }

        impl Behavior for MobBehavior {
            fn entry_condition(&self) -> &[Condition] {
                match self {
                    $(Self::$variant(behavior) => behavior.entry_condition(),)*
                }
            }

            fn check_extra_start_conditions<W: AiWorld>(
                &mut self,
                cx: &mut BehaviorCx<'_, '_, W>,
            ) -> bool {
                match self {
                    $(Self::$variant(behavior) => behavior.check_extra_start_conditions(cx),)*
                }
            }

            fn start<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, now: i64) {
                match self {
                    $(Self::$variant(behavior) => behavior.start(cx, now),)*
                }
            }

            fn can_still_use<W: AiWorld>(
                &mut self,
                cx: &mut BehaviorCx<'_, '_, W>,
                now: i64,
            ) -> bool {
                match self {
                    $(Self::$variant(behavior) => behavior.can_still_use(cx, now),)*
                }
            }

            fn tick<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, now: i64) {
                match self {
                    $(Self::$variant(behavior) => behavior.tick(cx, now),)*
                }
            }

            fn stop<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, now: i64) {
                match self {
                    $(Self::$variant(behavior) => behavior.stop(cx, now),)*
                }
            }

            fn timed_out(&self, end_timestamp: i64, now: i64) -> bool {
                match self {
                    $(Self::$variant(behavior) => behavior.timed_out(end_timestamp, now),)*
                }
            }
        }
    };
}

mob_behaviors! {
    CountDownCooldownTicks(CountDownCooldownTicks),
    Swim(Swim),
    LookAtTargetSink(LookAtTargetSink),
    MoveToTargetSink(MoveToTargetSink),
    AnimalPanic(AnimalPanic),
    RandomLookAround(RandomLookAround),
    AnimalMakeLove(AnimalMakeLove),
    FollowTemptation(FollowTemptation),
    CrossbowAttack(CrossbowAttack),
    LongJumpMidJump(LongJumpMidJump),
    LongJumpToRandomPos(LongJumpToRandomPos),
    RamTarget(RamTarget),
    PrepareRamNearestTarget(PrepareRamNearestTarget),
    ArmadilloBallUp(ArmadilloBallUp),
    ArmadilloPanic(ArmadilloPanic),
    BreezeShoot(BreezeShoot),
    BreezeLongJump(BreezeLongJump),
    BreezeShootWhenStuck(BreezeShootWhenStuck),
    BreezeSlide(BreezeSlide),
    BreezeSlideToTarget(SlideToTargetSink),
    GoToPotentialJobSite(GoToPotentialJobSite),
    SleepInBed(SleepInBed),
    JumpOnBed(JumpOnBed),
    VillagerPanicTrigger(VillagerPanicTrigger),
    LookAndFollowTradingPlayerSink(LookAndFollowTradingPlayerSink),
    VillagerMakeLove(VillagerMakeLove),
    TradeWithVillager(TradeWithVillager),
    WorkAtPoi(WorkAtPoi),
    ShowTradesToPlayer(ShowTradesToPlayer),
    GiveGiftToHero(GiveGiftToHero),
    HarvestFarmland(HarvestFarmland),
}

macro_rules! mob_triggers {
    ($($variant:ident($ty:ty)),* $(,)?) => {
        #[derive(Debug, Clone, PartialEq)]
        pub enum MobTrigger {
            $($variant($ty),)*
            Condition(MobPredicate),
            Sequence(Box<Sequence<MobTrigger, OneShot<MobTrigger>>>),
            Gate(TriggerGate<OneShot<MobTrigger>>),
        }

        impl MobTrigger {
            pub fn conditions(&self) -> Vec<Condition> {
                match self {
                    $(Self::$variant(trigger) => trigger.conditions(),)*
                    Self::Condition(_) | Self::Sequence(_) | Self::Gate(_) => Vec::new(),
                }
            }
        }

        impl Trigger for MobTrigger {
            fn trigger<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, now: i64) -> bool {
                match self {
                    $(Self::$variant(trigger) => trigger.trigger(cx, now),)*
                    Self::Condition(predicate) => predicate.test(cx),
                    Self::Sequence(sequence) => sequence.trigger(cx, now),
                    Self::Gate(gate) => gate.trigger(cx, now),
                }
            }

            fn required_memories(&self) -> Vec<MemoryModuleType> {
                match self {
                    Self::Sequence(sequence) => sequence.required_memories(),
                    Self::Gate(gate) => gate.required_memories(),
                    _ => Vec::new(),
                }
            }
        }
    };
}

mob_triggers! {
    SetWalkTargetFromLookTarget(SetWalkTargetFromLookTarget),
    SetEntityLookTarget(SetEntityLookTarget),
    SetEntityLookTargetSometimes(SetEntityLookTargetSometimes),
    RandomStroll(RandomStroll),
    BabyFollowAdult(BabyFollowAdult),
    SetWalkTargetAwayFrom(SetWalkTargetAwayFrom),
    GoToWantedItem(GoToWantedItem),
    GoToTargetLocation(GoToTargetLocation),
    SetLookAndInteract(SetLookAndInteract),
    InteractWith(InteractWith),
    StrollToPoi(StrollToPoi),
    StrollAroundPoi(StrollAroundPoi),
    StartAttacking(StartAttacking),
    StopAttackingIfTargetInvalid(StopAttackingIfTargetInvalid),
    MeleeAttack(MeleeAttack),
    SetWalkTargetFromAttackTargetIfTargetOutOfReach(SetWalkTargetFromAttackTargetIfTargetOutOfReach),
    EraseMemoryIf(EraseMemoryIf),
    StopBeingAngryIfTargetDead(StopBeingAngryIfTargetDead),
    BecomePassiveIfMemoryPresent(BecomePassiveIfMemoryPresent),
    BackUpIfTooClose(BackUpIfTooClose),
    CopyMemoryWithExpiry(CopyMemoryWithExpiry),
    StartCelebratingIfTargetDead(StartCelebratingIfTargetDead),
    StartHuntingHoglin(StartHuntingHoglin),
    RememberIfHoglinWasKilled(RememberIfHoglinWasKilled),
    StartAdmiringItemIfSeen(StartAdmiringItemIfSeen),
    StopAdmiringIfItemTooFarAway(StopAdmiringIfItemTooFarAway),
    StopAdmiringIfTiredOfTryingToReachItem(StopAdmiringIfTiredOfTryingToReachItem),
    StopHoldingItemIfNoLongerAdmiring(StopHoldingItemIfNoLongerAdmiring),
    ArmadilloRollOut(RollOutWithoutDanger),
    AcquirePoi(AcquirePoi),
    ValidateNearbyPoi(ValidateNearbyPoi),
    PoiCompetitorScan(PoiCompetitorScan),
    YieldJobSite(YieldJobSite),
    AssignProfessionFromJobSite(AssignProfessionFromJobSite),
    ResetProfession(ResetProfession),
    SetWalkTargetFromBlockMemory(SetWalkTargetFromBlockMemory),
    StrollToPoiList(StrollToPoiList),
    WakeUp(WakeUp),
    SetClosestHomeAsWalkTarget(SetClosestHomeAsWalkTarget),
    InsideBrownianWalk(InsideBrownianWalk),
    GoToClosestVillage(GoToClosestVillage),
    VillageBoundRandomStroll(VillageBoundRandomStroll),
    PlayTagWithOtherKids(PlayTagWithOtherKids),
    UpdateActivityFromSchedule(UpdateActivityFromSchedule),
    VillagerCalmDown(VillagerCalmDown),
    ReactToBell(ReactToBell),
    SetRaidStatus(SetRaidStatus),
    ResetRaidStatus(ResetRaidStatus),
    RingBell(RingBell),
    MoveToSkySeeingSpot(MoveToSkySeeingSpot),
    SetHiddenState(SetHiddenState),
    LocateHidingPlace(LocateHidingPlace),
    SocializeAtBell(SocializeAtBell),
}

#[derive(Debug, Clone, PartialEq)]
pub enum MobControl {
    DoNothing(DoNothing),
    Gate(Gate<MobControl>),
    Timed(Timed<MobBehavior>),
    OneShot(OneShot<MobTrigger>),
}

impl MobControl {
    pub fn behavior(behavior: MobBehavior) -> Self {
        Self::Timed(Timed::new(behavior))
    }

    pub fn behavior_for(behavior: MobBehavior, min_duration: i32, max_duration: i32) -> Self {
        Self::Timed(Timed::with_duration(behavior, min_duration, max_duration))
    }

    pub fn one_shot(trigger: MobTrigger) -> Self {
        Self::OneShot(Self::declared(trigger))
    }

    pub fn declared(trigger: MobTrigger) -> OneShot<MobTrigger> {
        OneShot::new(trigger.conditions(), trigger)
    }

    pub fn trigger_if(predicate: MobPredicate, behavior: OneShot<MobTrigger>) -> Self {
        let sequence = Sequence::new(MobTrigger::Condition(predicate), behavior);
        Self::OneShot(OneShot::new(
            Vec::new(),
            MobTrigger::Sequence(Box::new(sequence)),
        ))
    }

    pub fn trigger_one_shuffled(weighted: Vec<(OneShot<MobTrigger>, i32)>) -> Self {
        let gate = TriggerGate::new(weighted, OrderPolicy::Shuffled, RunningPolicy::RunOne);
        Self::OneShot(OneShot::new(Vec::new(), MobTrigger::Gate(gate)))
    }

    pub fn swim(chance: f32) -> Self {
        Self::behavior(MobBehavior::Swim(Swim::new(chance)))
    }

    pub fn creaking_swim(chance: f32) -> Self {
        Self::behavior(MobBehavior::Swim(Swim::creaking(chance)))
    }

    pub fn look_at_target_sink(min_duration: i32, max_duration: i32) -> Self {
        Self::behavior_for(
            MobBehavior::LookAtTargetSink(LookAtTargetSink),
            min_duration,
            max_duration,
        )
    }

    pub fn default_look_at_target_sink() -> Self {
        Self::look_at_target_sink(LOOK_SINK_MIN_DURATION, LOOK_SINK_MAX_DURATION)
    }

    pub fn move_to_target_sink() -> Self {
        Self::move_to_target_sink_for(
            MoveToTargetSink::min_timeout(),
            MoveToTargetSink::max_timeout(),
        )
    }

    pub fn move_to_target_sink_for(min_timeout: i32, max_timeout: i32) -> Self {
        Self::behavior_for(
            MobBehavior::MoveToTargetSink(MoveToTargetSink::new()),
            min_timeout,
            max_timeout,
        )
    }

    pub fn animal_panic(speed_multiplier: f32) -> Self {
        Self::behavior_for(
            MobBehavior::AnimalPanic(AnimalPanic::new(speed_multiplier)),
            AnimalPanic::min_duration(),
            AnimalPanic::max_duration(),
        )
    }

    pub fn random_look_around(behavior: RandomLookAround) -> Self {
        Self::behavior(MobBehavior::RandomLookAround(behavior))
    }

    pub fn animal_make_love(behavior: AnimalMakeLove) -> Self {
        Self::behavior_for(
            MobBehavior::AnimalMakeLove(behavior),
            MAKE_LOVE_DURATION,
            MAKE_LOVE_DURATION,
        )
    }

    pub fn follow_temptation(behavior: FollowTemptation) -> Self {
        Self::behavior(MobBehavior::FollowTemptation(behavior))
    }

    pub fn crossbow_attack() -> Self {
        Self::behavior_for(
            MobBehavior::CrossbowAttack(CrossbowAttack::new()),
            CROSSBOW_ATTACK_DURATION,
            CROSSBOW_ATTACK_DURATION,
        )
    }

    pub fn count_down_cooldown_ticks(cooldown_ticks: MemoryModuleType) -> Self {
        Self::behavior(MobBehavior::CountDownCooldownTicks(
            CountDownCooldownTicks::new(cooldown_ticks),
        ))
    }
}

macro_rules! delegate {
    ($self:ident, $control:ident => $call:expr) => {
        match $self {
            Self::DoNothing($control) => $call,
            Self::Gate($control) => $call,
            Self::Timed($control) => $call,
            Self::OneShot($control) => $call,
        }
    };
}

impl BehaviorControl for MobControl {
    fn status(&self) -> Status {
        delegate!(self, control => control.status())
    }

    fn required_memories(&self) -> Vec<MemoryModuleType> {
        delegate!(self, control => BehaviorControl::required_memories(control))
    }

    fn try_start<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, now: i64) -> bool {
        delegate!(self, control => control.try_start(cx, now))
    }

    fn tick_or_stop<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, now: i64) {
        delegate!(self, control => control.tick_or_stop(cx, now))
    }

    fn do_stop<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, now: i64) {
        delegate!(self, control => control.do_stop(cx, now))
    }
}

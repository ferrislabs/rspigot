mod facts;
mod home;
mod packages;
mod poi;
mod sensors;
mod social;
mod support;
mod trader;
mod work;

pub use facts::{
    INVENTORY_SLOTS, RaidFacts, StackFacts, TradeView, VillagerAction, VillagerFacts,
    VillagerPeerFacts,
};
pub use home::{
    GoToClosestVillage, InsideBrownianWalk, JumpOnBed, PlayTagWithOtherKids,
    SetClosestHomeAsWalkTarget, SleepInBed, UpdateActivityFromSchedule, VillageBoundRandomStroll,
    WakeUp,
};
pub use packages::{villager_activities, villager_sensors};
pub use poi::{
    AcquirePoi, AssignProfessionFromJobSite, GoToPotentialJobSite, HOME_EVENT, PoiCompetitorScan,
    PoiRule, ResetProfession, SetWalkTargetFromBlockMemory, StrollToPoiList, ValidateNearbyPoi,
    YieldJobSite, bed_is_occupied, is_free_bed,
};
pub use sensors::{
    GolemSensor, HOSTILE_DISTANCES, NearestBedSensor, SecondaryPoiSensor, VillagerBabiesSensor,
    VillagerHostilesSensor, check_for_nearby_golem, golem_detected, hostile_distance,
};
pub use social::{
    LocateHidingPlace, LookAndFollowTradingPlayerSink, MoveToSkySeeingSpot, ReactToBell,
    ResetRaidStatus, RingBell, SetHiddenState, SetRaidStatus, SocializeAtBell, TradeWithVillager,
    VillagerCalmDown, VillagerMakeLove, VillagerPanicTrigger,
};
pub use trader::{TraderFacts, TraderGoal, TraderItem};
pub use work::{
    GiveGiftToHero, HarvestFarmland, ShowTradesToPlayer, UseBonemeal, WorkAtPoi, Workstation,
};

use rspigot_domain::ItemId;
use rspigot_domain::entity::species::villager::{Profession, picks_up};

use crate::behavior::{BehaviorBrain, MobControl, PackedMemories, Schedule};
use crate::random::RandomSource;
use crate::world::MobView;

pub fn wants_to_pick_up(mob: &MobView, item: ItemId) -> bool {
    let facts = &mob.species.villager;
    item.kind()
        .is_some_and(|kind| picks_up(kind, facts.profession))
        && facts.can_add_item(item)
}

const MEMORIES: &[crate::behavior::MemoryModuleType] = &[
    crate::behavior::MemoryModuleType::DoorsToClose,
    crate::behavior::MemoryModuleType::LastWorkedAtPoi,
    crate::behavior::MemoryModuleType::LastSlept,
    crate::behavior::MemoryModuleType::LastWoken,
    crate::behavior::MemoryModuleType::Home,
    crate::behavior::MemoryModuleType::JobSite,
    crate::behavior::MemoryModuleType::PotentialJobSite,
    crate::behavior::MemoryModuleType::MeetingPoint,
    crate::behavior::MemoryModuleType::GolemDetectedRecently,
];

pub fn villager_brain(
    profession: Profession,
    baby: bool,
    packed: &PackedMemories,
    random: &mut impl RandomSource,
) -> BehaviorBrain<MobControl> {
    let mut brain = BehaviorBrain::new(
        MEMORIES,
        villager_sensors(),
        villager_activities(profession, baby),
        packed,
        random,
    );
    brain.state_mut().set_schedule(Some(if baby {
        Schedule::BABY_VILLAGER
    } else {
        Schedule::VILLAGER
    }));
    brain
}

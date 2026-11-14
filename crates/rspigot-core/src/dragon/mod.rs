pub(crate) mod ai;
mod cloud;
mod crystal;
mod death;
mod env;
mod fight;
mod hurt;
mod item;
pub(crate) mod phases;
mod walls;

pub(crate) use fight::DragonFight;

use rspigot_domain::Vec3;
use rspigot_domain::dragon::parts::PART_COUNT;
use rspigot_domain::dragon::phase::DragonPhases;
use rspigot_domain::dragon::{DragonNodes, FlightHistory};

#[derive(Debug, Default)]
pub(crate) struct DragonRuntime {
    pub(crate) phases: DragonPhases,
    pub(crate) nodes: DragonNodes,
    pub(crate) history: FlightHistory,
    pub(crate) parts: [Vec3; PART_COUNT],
    pub(crate) old_parts: [Vec3; PART_COUNT],
    pub(crate) pending_phase: bool,
    pub(crate) hit_part: Option<rspigot_domain::dragon::Part>,
    pub(crate) nearest_crystal: Option<rspigot_domain::EntityId>,
    pub(crate) health_before: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct CrystalFacts {
    pub(crate) id: rspigot_domain::EntityId,
    pub(crate) uuid: Option<uuid::Uuid>,
    pub(crate) pos: Option<rspigot_domain::BlockPos>,
}

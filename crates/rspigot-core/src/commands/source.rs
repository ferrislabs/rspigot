use rspigot_domain::chunk::HeightmapKind;
use rspigot_domain::{BlockPos, Identifier, Rotation, Vec3};

use super::dispatch::Target;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Anchor {
    #[default]
    Feet,
    Eyes,
}

impl Anchor {
    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "feet" => Some(Self::Feet),
            "eyes" => Some(Self::Eyes),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Relation {
    Owner,
    Leasher,
    Target,
    Attacker,
    Vehicle,
    Controller,
    Origin,
    Passengers,
}

impl Relation {
    pub fn parse(text: &str) -> Option<Self> {
        Some(match text {
            "owner" => Self::Owner,
            "leasher" => Self::Leasher,
            "target" => Self::Target,
            "attacker" => Self::Attacker,
            "vehicle" => Self::Vehicle,
            "controller" => Self::Controller,
            "origin" => Self::Origin,
            "passengers" => Self::Passengers,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Source {
    pub executor: Option<Target>,
    pub position: Vec3,
    pub rotation: Rotation,
    pub anchor: Anchor,
    pub dimension: Option<Identifier>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DataTarget {
    Entity(Target),
    Block(BlockPos),
    Storage(Identifier),
}

pub fn heightmap(name: &str) -> Option<HeightmapKind> {
    match name {
        "motion_blocking" => Some(HeightmapKind::MotionBlocking),
        "motion_blocking_no_leaves" => Some(HeightmapKind::MotionBlockingNoLeaves),
        "ocean_floor" => Some(HeightmapKind::OceanFloor),
        "world_surface" => Some(HeightmapKind::WorldSurface),
        _ => None,
    }
}

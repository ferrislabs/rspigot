use rspigot_plugin::Material;

use crate::domain::{PlayerId, SheepKind, TeamId};
use crate::sheep::sheep_wool;

pub const MAX_CONCURRENT_FLIGHTS: usize = 32;
pub const MAX_FLIGHT_TICKS: u32 = 20 * 15;
pub const LANDING_GRACE_TICKS: u32 = 2;
pub const EYE_HEIGHT: f64 = 1.62;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Flight {
    pub kind: SheepKind,
    pub team: TeamId,
    pub thrower: PlayerId,
    pub age: u32,
}

impl Flight {
    pub const fn new(kind: SheepKind, team: TeamId, thrower: PlayerId) -> Self {
        Self {
            kind,
            team,
            thrower,
            age: 0,
        }
    }

    pub const fn should_land(&self, on_ground: bool) -> bool {
        (on_ground && self.age >= LANDING_GRACE_TICKS) || self.age >= MAX_FLIGHT_TICKS
    }
}

pub fn kind_of(material: Material) -> Option<SheepKind> {
    SheepKind::ALL
        .into_iter()
        .find(|kind| sheep_wool(*kind) == material)
}

pub const fn can_launch(active: usize) -> bool {
    active < MAX_CONCURRENT_FLIGHTS
}

#[cfg(test)]
mod tests {
    use super::*;

    fn flight(age: u32) -> Flight {
        Flight {
            kind: SheepKind::Lightning,
            team: TeamId::Red,
            thrower: PlayerId(1),
            age,
        }
    }

    #[test]
    fn every_sheep_item_maps_back_to_its_kind() {
        for kind in SheepKind::ALL {
            assert_eq!(kind_of(sheep_wool(kind)), Some(kind));
        }
    }

    #[test]
    fn other_materials_are_not_sheep() {
        assert_eq!(kind_of(Material::Compass), None);
    }

    #[test]
    fn lands_on_the_ground_after_the_grace() {
        assert!(!flight(0).should_land(true));
        assert!(flight(LANDING_GRACE_TICKS).should_land(true));
        assert!(!flight(LANDING_GRACE_TICKS).should_land(false));
    }

    #[test]
    fn lands_when_the_lifetime_is_over() {
        assert!(flight(MAX_FLIGHT_TICKS).should_land(false));
    }

    #[test]
    fn flights_are_capped() {
        assert!(can_launch(MAX_CONCURRENT_FLIGHTS - 1));
        assert!(!can_launch(MAX_CONCURRENT_FLIGHTS));
    }
}

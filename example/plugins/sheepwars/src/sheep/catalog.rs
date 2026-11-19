use crate::domain::SheepKind;

pub const EXPLOSIVE_POWER: f32 = 3.0;
pub const EXPLOSIVE_BREAKS_BLOCKS: bool = true;

pub const INCENDIARY_RADIUS: i32 = 3;

pub const FROZEN_RADIUS: f64 = 5.0;
pub const FROZEN_SLOWNESS_TICKS: i32 = 200;
pub const FROZEN_SLOWNESS_AMPLIFIER: u8 = 2;
pub const FROZEN_ICE_RADIUS: i32 = 2;

pub const SEEKER_POWER: f32 = 2.0;
pub const SEEKER_SPEED: f64 = 0.8;
pub const SEEKER_RANGE: f64 = 20.0;

pub const HEALING_RADIUS: f64 = 5.0;
pub const HEALING_REGENERATION_TICKS: i32 = 100;
pub const HEALING_REGENERATION_AMPLIFIER: u8 = 1;

pub const QUAKE_RADIUS: f64 = 6.0;
pub const QUAKE_LIFT: f64 = 1.2;

pub const DARKNESS_RADIUS: f64 = 6.0;
pub const DARKNESS_BLINDNESS_TICKS: i32 = 160;
pub const DARKNESS_BLINDNESS_AMPLIFIER: u8 = 0;

pub const SPLIT_COUNT: u32 = 4;
pub const SPLIT_HORIZONTAL_SPEED: f64 = 0.5;
pub const SPLIT_VERTICAL_SPEED: f64 = 0.6;
pub const SPLIT_CHILD: SheepKind = SheepKind::Explosive;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum WoolColor {
    Red,
    Orange,
    LightBlue,
    Green,
    Yellow,
    Pink,
    Purple,
    Brown,
    Black,
    Gray,
}

pub const fn display_name(kind: SheepKind) -> &'static str {
    match kind {
        SheepKind::Explosive => "§cMouton explosif",
        SheepKind::Incendiary => "§6Mouton incendiaire",
        SheepKind::Frozen => "§bMouton glacé",
        SheepKind::Seeker => "§aMouton chercheur",
        SheepKind::Lightning => "§eMouton foudroyant",
        SheepKind::Healing => "§dMouton soigneur",
        SheepKind::Boarding => "§5Mouton d'abordage",
        SheepKind::Quake => "§6Mouton sismique",
        SheepKind::Darkness => "§8Mouton des ténèbres",
        SheepKind::Splitting => "§7Mouton fractionneur",
    }
}

pub const fn wool_color(kind: SheepKind) -> WoolColor {
    match kind {
        SheepKind::Explosive => WoolColor::Red,
        SheepKind::Incendiary => WoolColor::Orange,
        SheepKind::Frozen => WoolColor::LightBlue,
        SheepKind::Seeker => WoolColor::Green,
        SheepKind::Lightning => WoolColor::Yellow,
        SheepKind::Healing => WoolColor::Pink,
        SheepKind::Boarding => WoolColor::Purple,
        SheepKind::Quake => WoolColor::Brown,
        SheepKind::Darkness => WoolColor::Black,
        SheepKind::Splitting => WoolColor::Gray,
    }
}

pub const fn homes_in_flight(kind: SheepKind) -> bool {
    matches!(kind, SheepKind::Seeker)
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;

    #[test]
    fn every_kind_has_a_distinct_wool_and_name() {
        let colors: HashSet<_> = SheepKind::ALL.into_iter().map(wool_color).collect();
        let names: HashSet<_> = SheepKind::ALL.into_iter().map(display_name).collect();
        assert_eq!(colors.len(), SheepKind::ALL.len());
        assert_eq!(names.len(), SheepKind::ALL.len());
    }

    #[test]
    fn only_the_seeker_homes() {
        let homing: Vec<_> = SheepKind::ALL
            .into_iter()
            .filter(|kind| homes_in_flight(*kind))
            .collect();
        assert_eq!(homing, vec![SheepKind::Seeker]);
    }
}

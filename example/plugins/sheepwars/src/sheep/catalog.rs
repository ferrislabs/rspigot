use super::launch::DEFAULT_THROW_POWER;
use crate::domain::SheepKind;

pub const INCENDIARY_POWER: f32 = 6.0;
pub const INCENDIARY_BREAKS_BLOCKS: bool = true;

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
pub const HEALING_THROW_POWER: f64 = 0.5;

pub const QUAKE_RADIUS: f64 = 6.0;
pub const QUAKE_LIFT: f64 = 1.2;

pub const DARKNESS_RADIUS: f64 = 6.0;
pub const DARKNESS_BLINDNESS_TICKS: i32 = 160;
pub const DARKNESS_BLINDNESS_AMPLIFIER: u8 = 0;

pub const SPLIT_COUNT: u32 = 4;
pub const SPLIT_HORIZONTAL_SPEED: f64 = 0.5;
pub const SPLIT_VERTICAL_SPEED: f64 = 0.6;
pub const SPLIT_CHILD: SheepKind = SheepKind::Seeker;
pub const SPLIT_PARENT_POWER: f32 = 2.0;

pub const WARP_RADIUS: f64 = 8.0;
pub const WARP_PUSH: f64 = 2.0;
pub const WARP_LIFT: f64 = 0.8;
pub const WARP_BLAST_POWER: f32 = 2.5;
pub const WARP_BLAST_BREAKS_BLOCKS: bool = true;

pub const INTERGALACTIC_METEORS: u32 = 8;
pub const INTERGALACTIC_SPREAD: f64 = 8.0;
pub const INTERGALACTIC_POWER: f32 = 5.0;
pub const INTERGALACTIC_BREAKS_BLOCKS: bool = true;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum WoolColor {
    Orange,
    Magenta,
    LightBlue,
    Green,
    Yellow,
    Pink,
    Purple,
    Blue,
    Brown,
    Black,
    Gray,
}

pub const fn display_name(kind: SheepKind) -> &'static str {
    match kind {
        SheepKind::Lightning => "§eInvocateur de foudre",
        SheepKind::Incendiary => "§6Mouton incendiaire",
        SheepKind::Boarding => "§5Mouton d'abordage",
        SheepKind::Healing => "§dMouton soigneur",
        SheepKind::Seeker => "§aMouton chercheur",
        SheepKind::Splitting => "§7Mouton à fragmentation",
        SheepKind::Darkness => "§8Mouton ténébreux",
        SheepKind::Frozen => "§bMouton glacé",
        SheepKind::Quake => "§6Mouton tremblement de terre",
        SheepKind::Warp => "§dMouton à distorsion",
        SheepKind::Intergalactic => "§9Mouton intergalactique",
    }
}

pub const fn wool_color(kind: SheepKind) -> WoolColor {
    match kind {
        SheepKind::Lightning => WoolColor::Yellow,
        SheepKind::Incendiary => WoolColor::Orange,
        SheepKind::Boarding => WoolColor::Purple,
        SheepKind::Healing => WoolColor::Pink,
        SheepKind::Seeker => WoolColor::Green,
        SheepKind::Splitting => WoolColor::Gray,
        SheepKind::Darkness => WoolColor::Black,
        SheepKind::Frozen => WoolColor::LightBlue,
        SheepKind::Quake => WoolColor::Brown,
        SheepKind::Warp => WoolColor::Magenta,
        SheepKind::Intergalactic => WoolColor::Blue,
    }
}

pub const fn homes_in_flight(kind: SheepKind) -> bool {
    matches!(kind, SheepKind::Seeker)
}

pub const fn announces_launch(kind: SheepKind) -> bool {
    matches!(kind, SheepKind::Intergalactic)
}

pub const fn launch_power(kind: SheepKind) -> f64 {
    match kind {
        SheepKind::Healing => HEALING_THROW_POWER,
        _ => DEFAULT_THROW_POWER,
    }
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

    #[test]
    fn only_the_intergalactic_announces_its_launch() {
        let announcing: Vec<_> = SheepKind::ALL
            .into_iter()
            .filter(|kind| announces_launch(*kind))
            .collect();
        assert_eq!(announcing, vec![SheepKind::Intergalactic]);
    }

    #[test]
    fn healing_is_thrown_shorter_than_the_others() {
        for kind in SheepKind::ALL {
            if kind == SheepKind::Healing {
                assert!(launch_power(kind) < super::DEFAULT_THROW_POWER);
            } else {
                assert_eq!(launch_power(kind), super::DEFAULT_THROW_POWER);
            }
        }
    }

    #[test]
    fn there_are_eleven_kinds() {
        assert_eq!(SheepKind::ALL.len(), 11);
    }
}

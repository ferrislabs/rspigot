mod board;
mod criteria;
mod message;
pub(crate) mod persist;
mod team;

use crate::text::{Component, NamedColor, Style};

pub use board::{Objective, Score, Scoreboard, ScoreboardError};
pub use criteria::{Criteria, RenderType};
pub use message::{ObjectiveView, ScoreboardMessage, TeamOp, TeamView};
pub use team::{CollisionRule, Team, Visibility};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DisplaySlot {
    List,
    Sidebar,
    BelowName,
    Team(NamedColor),
}

pub const DISPLAY_SLOT_COUNT: usize = 19;

const COLORS: [NamedColor; 16] = [
    NamedColor::Black,
    NamedColor::DarkBlue,
    NamedColor::DarkGreen,
    NamedColor::DarkAqua,
    NamedColor::DarkRed,
    NamedColor::DarkPurple,
    NamedColor::Gold,
    NamedColor::Gray,
    NamedColor::DarkGray,
    NamedColor::Blue,
    NamedColor::Green,
    NamedColor::Aqua,
    NamedColor::Red,
    NamedColor::LightPurple,
    NamedColor::Yellow,
    NamedColor::White,
];

const TEAM_SLOT_PREFIX: &str = "sidebar.team.";

impl DisplaySlot {
    pub fn all() -> impl Iterator<Item = Self> {
        (0..DISPLAY_SLOT_COUNT).filter_map(Self::from_index)
    }

    pub const fn index(self) -> usize {
        match self {
            Self::List => 0,
            Self::Sidebar => 1,
            Self::BelowName => 2,
            Self::Team(color) => 3 + color as usize,
        }
    }

    pub fn from_index(index: usize) -> Option<Self> {
        match index {
            0 => Some(Self::List),
            1 => Some(Self::Sidebar),
            2 => Some(Self::BelowName),
            3..DISPLAY_SLOT_COUNT => COLORS.get(index - 3).copied().map(Self::Team),
            _ => None,
        }
    }

    pub fn name(self) -> String {
        match self {
            Self::List => "list".to_owned(),
            Self::Sidebar => "sidebar".to_owned(),
            Self::BelowName => "below_name".to_owned(),
            Self::Team(color) => format!("{TEAM_SLOT_PREFIX}{}", color.name()),
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "list" => Some(Self::List),
            "sidebar" => Some(Self::Sidebar),
            "below_name" => Some(Self::BelowName),
            _ => name
                .strip_prefix(TEAM_SLOT_PREFIX)
                .and_then(NamedColor::from_name)
                .map(Self::Team),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum NumberFormat {
    Blank,
    Fixed(Component),
    Styled(Style),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn there_are_nineteen_slots_with_stable_indices_and_names() {
        let slots: Vec<DisplaySlot> = DisplaySlot::all().collect();
        assert_eq!(slots.len(), DISPLAY_SLOT_COUNT);
        for (index, slot) in slots.iter().enumerate() {
            assert_eq!(slot.index(), index);
            assert_eq!(DisplaySlot::from_name(&slot.name()), Some(*slot));
        }
        assert_eq!(slots[0].name(), "list");
        assert_eq!(slots[3].name(), "sidebar.team.black");
        assert_eq!(slots[18].name(), "sidebar.team.white");
        assert_eq!(DisplaySlot::from_name("sidebar.team.pink"), None);
        assert_eq!(DisplaySlot::from_index(19), None);
    }
}

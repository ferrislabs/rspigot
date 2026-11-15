use crate::text::{Component, NamedColor};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Visibility {
    #[default]
    Always,
    Never,
    HideForOtherTeams,
    HideForOwnTeam,
}

impl Visibility {
    pub const ALL: [Self; 4] = [
        Self::Always,
        Self::Never,
        Self::HideForOtherTeams,
        Self::HideForOwnTeam,
    ];

    pub const fn name(self) -> &'static str {
        match self {
            Self::Always => "always",
            Self::Never => "never",
            Self::HideForOtherTeams => "hideForOtherTeams",
            Self::HideForOwnTeam => "hideForOwnTeam",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|value| value.name() == name)
    }

    pub const fn id(self) -> i32 {
        self as i32
    }

    pub fn translation_key(self) -> String {
        format!("team.visibility.{}", self.name())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CollisionRule {
    #[default]
    Always,
    Never,
    PushOtherTeams,
    PushOwnTeam,
}

impl CollisionRule {
    pub const ALL: [Self; 4] = [
        Self::Always,
        Self::Never,
        Self::PushOtherTeams,
        Self::PushOwnTeam,
    ];

    pub const fn name(self) -> &'static str {
        match self {
            Self::Always => "always",
            Self::Never => "never",
            Self::PushOtherTeams => "pushOtherTeams",
            Self::PushOwnTeam => "pushOwnTeam",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|value| value.name() == name)
    }

    pub const fn id(self) -> i32 {
        self as i32
    }

    pub fn translation_key(self) -> String {
        format!("team.collision.{}", self.name())
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Team {
    pub name: String,
    pub display_name: Component,
    pub color: Option<NamedColor>,
    pub friendly_fire: bool,
    pub see_friendly_invisibles: bool,
    pub prefix: Component,
    pub suffix: Component,
    pub nametag_visibility: Visibility,
    pub death_message_visibility: Visibility,
    pub collision_rule: CollisionRule,
}

impl Team {
    pub fn new(name: impl Into<String>, display_name: Component) -> Self {
        Self {
            name: name.into(),
            display_name,
            color: None,
            friendly_fire: true,
            see_friendly_invisibles: true,
            prefix: Component::text(""),
            suffix: Component::text(""),
            nametag_visibility: Visibility::Always,
            death_message_visibility: Visibility::Always,
            collision_rule: CollisionRule::Always,
        }
    }

    pub const fn options(&self) -> u8 {
        (self.friendly_fire as u8) | ((self.see_friendly_invisibles as u8) << 1)
    }

    pub fn format_name(&self, name: Component) -> Component {
        let mut formatted = Component::text("")
            .append(self.prefix.clone())
            .append(name)
            .append(self.suffix.clone());
        if let Some(color) = self.color {
            formatted = formatted.color(color);
        }
        formatted
    }

    pub fn formatted_display_name(&self) -> Component {
        let mut name = Component::text("[")
            .append(self.display_name.clone())
            .append(Component::text("]"));
        if let Some(color) = self.color {
            name = name.color(color);
        }
        name
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn options_pack_friendly_fire_and_invisibles() {
        let mut team = Team::new("red", Component::text("red"));
        assert_eq!(team.options(), 3);
        team.friendly_fire = false;
        assert_eq!(team.options(), 2);
        team.see_friendly_invisibles = false;
        assert_eq!(team.options(), 0);
        team.friendly_fire = true;
        assert_eq!(team.options(), 1);
    }

    #[test]
    fn names_wrap_in_prefix_suffix_and_color() {
        let mut team = Team::new("red", Component::text("red"));
        team.prefix = Component::text("[R] ");
        team.suffix = Component::text("!");
        team.color = Some(NamedColor::Red);
        let formatted = team.format_name(Component::text("Steve"));
        assert_eq!(formatted.plain_text(), "[R] Steve!");
        assert_eq!(
            formatted.style.color,
            Some(crate::text::Color::Named(NamedColor::Red))
        );
    }

    #[test]
    fn enums_round_trip_by_name() {
        for value in Visibility::ALL {
            assert_eq!(Visibility::from_name(value.name()), Some(value));
        }
        for value in CollisionRule::ALL {
            assert_eq!(CollisionRule::from_name(value.name()), Some(value));
        }
        assert_eq!(Visibility::HideForOwnTeam.id(), 3);
        assert_eq!(CollisionRule::PushOtherTeams.id(), 2);
    }
}

use crate::text::NamedColor;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RenderType {
    #[default]
    Integer,
    Hearts,
}

impl RenderType {
    pub const fn name(self) -> &'static str {
        match self {
            Self::Integer => "integer",
            Self::Hearts => "hearts",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "integer" => Some(Self::Integer),
            "hearts" => Some(Self::Hearts),
            _ => None,
        }
    }

    pub const fn id(self) -> i32 {
        self as i32
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Criteria {
    Dummy,
    Trigger,
    DeathCount,
    PlayerKillCount,
    TotalKillCount,
    Health,
    Food,
    Air,
    Armor,
    Xp,
    Level,
    TeamKill(NamedColor),
    KilledByTeam(NamedColor),
    Stat { kind: String, key: String },
}

const TEAM_KILL: &str = "teamkill.";
const KILLED_BY_TEAM: &str = "killedByTeam.";

fn dotted(id: &str) -> String {
    id.replacen(':', ".", 1)
}

fn colon(id: &str) -> String {
    match id.split_once('.') {
        Some((namespace, path)) => format!("{namespace}:{path}"),
        None => id.to_owned(),
    }
}

impl Criteria {
    pub fn parse(name: &str, is_stat: impl Fn(&str, &str) -> bool) -> Option<Self> {
        let simple = match name {
            "dummy" => Some(Self::Dummy),
            "trigger" => Some(Self::Trigger),
            "deathCount" => Some(Self::DeathCount),
            "playerKillCount" => Some(Self::PlayerKillCount),
            "totalKillCount" => Some(Self::TotalKillCount),
            "health" => Some(Self::Health),
            "food" => Some(Self::Food),
            "air" => Some(Self::Air),
            "armor" => Some(Self::Armor),
            "xp" => Some(Self::Xp),
            "level" => Some(Self::Level),
            _ => None,
        };
        if simple.is_some() {
            return simple;
        }
        if let Some(color) = name.strip_prefix(TEAM_KILL) {
            return NamedColor::from_name(color).map(Self::TeamKill);
        }
        if let Some(color) = name.strip_prefix(KILLED_BY_TEAM) {
            return NamedColor::from_name(color).map(Self::KilledByTeam);
        }
        let (kind, key) = name.split_once(':')?;
        let (kind, key) = (colon(kind), colon(key));
        is_stat(&kind, &key).then_some(Self::Stat { kind, key })
    }

    pub fn name(&self) -> String {
        match self {
            Self::Dummy => "dummy".to_owned(),
            Self::Trigger => "trigger".to_owned(),
            Self::DeathCount => "deathCount".to_owned(),
            Self::PlayerKillCount => "playerKillCount".to_owned(),
            Self::TotalKillCount => "totalKillCount".to_owned(),
            Self::Health => "health".to_owned(),
            Self::Food => "food".to_owned(),
            Self::Air => "air".to_owned(),
            Self::Armor => "armor".to_owned(),
            Self::Xp => "xp".to_owned(),
            Self::Level => "level".to_owned(),
            Self::TeamKill(color) => format!("{TEAM_KILL}{}", color.name()),
            Self::KilledByTeam(color) => format!("{KILLED_BY_TEAM}{}", color.name()),
            Self::Stat { kind, key } => format!("{}:{}", dotted(kind), dotted(key)),
        }
    }

    pub const fn read_only(&self) -> bool {
        matches!(
            self,
            Self::Health | Self::Food | Self::Air | Self::Armor | Self::Xp | Self::Level
        )
    }

    pub const fn render_type(&self) -> RenderType {
        match self {
            Self::Health => RenderType::Hearts,
            _ => RenderType::Integer,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn nothing(_: &str, _: &str) -> bool {
        false
    }

    #[test]
    fn names_round_trip_through_parse() {
        for name in [
            "dummy",
            "trigger",
            "deathCount",
            "playerKillCount",
            "totalKillCount",
            "health",
            "food",
            "air",
            "armor",
            "xp",
            "level",
            "teamkill.red",
            "killedByTeam.dark_blue",
        ] {
            let criteria = Criteria::parse(name, nothing).expect(name);
            assert_eq!(criteria.name(), name);
        }
    }

    #[test]
    fn stat_criteria_use_dotted_ids_and_defer_to_the_validator() {
        let known = |kind: &str, key: &str| kind == "minecraft:mined" && key == "minecraft:stone";
        let criteria = Criteria::parse("minecraft.mined:minecraft.stone", known).expect("stat");
        assert_eq!(
            criteria,
            Criteria::Stat {
                kind: "minecraft:mined".to_owned(),
                key: "minecraft:stone".to_owned()
            }
        );
        assert_eq!(criteria.name(), "minecraft.mined:minecraft.stone");
        assert!(Criteria::parse("minecraft.mined:minecraft.dirt", known).is_none());
        assert!(Criteria::parse("nonsense", known).is_none());
        assert!(Criteria::parse("teamkill.purple", known).is_none());
    }

    #[test]
    fn vitals_are_read_only_and_health_renders_hearts() {
        assert!(Criteria::Health.read_only());
        assert!(Criteria::Level.read_only());
        assert!(!Criteria::Dummy.read_only());
        assert!(!Criteria::Trigger.read_only());
        assert_eq!(Criteria::Health.render_type(), RenderType::Hearts);
        assert_eq!(Criteria::Food.render_type(), RenderType::Integer);
    }
}

use rspigot_plugin_protocol::v1::scoreboard_order::Order;
use rspigot_plugin_protocol::v1::team_option::Option as TeamChoice;
use rspigot_plugin_protocol::v1::{self, answer, from_plugin, question};

use crate::Error;
use crate::link::Link;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TeamColor {
    Black,
    DarkBlue,
    DarkGreen,
    DarkAqua,
    DarkRed,
    DarkPurple,
    Gold,
    Gray,
    DarkGray,
    Blue,
    Green,
    Aqua,
    Red,
    LightPurple,
    Yellow,
    White,
}

impl TeamColor {
    pub const ALL: [Self; 16] = [
        Self::Black,
        Self::DarkBlue,
        Self::DarkGreen,
        Self::DarkAqua,
        Self::DarkRed,
        Self::DarkPurple,
        Self::Gold,
        Self::Gray,
        Self::DarkGray,
        Self::Blue,
        Self::Green,
        Self::Aqua,
        Self::Red,
        Self::LightPurple,
        Self::Yellow,
        Self::White,
    ];

    pub const fn name(self) -> &'static str {
        match self {
            Self::Black => "black",
            Self::DarkBlue => "dark_blue",
            Self::DarkGreen => "dark_green",
            Self::DarkAqua => "dark_aqua",
            Self::DarkRed => "dark_red",
            Self::DarkPurple => "dark_purple",
            Self::Gold => "gold",
            Self::Gray => "gray",
            Self::DarkGray => "dark_gray",
            Self::Blue => "blue",
            Self::Green => "green",
            Self::Aqua => "aqua",
            Self::Red => "red",
            Self::LightPurple => "light_purple",
            Self::Yellow => "yellow",
            Self::White => "white",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|color| color.name() == name)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DisplaySlot {
    PlayerList,
    Sidebar,
    BelowName,
    SidebarTeam(TeamColor),
}

impl DisplaySlot {
    pub fn name(self) -> String {
        match self {
            Self::PlayerList => "list".to_owned(),
            Self::Sidebar => "sidebar".to_owned(),
            Self::BelowName => "below_name".to_owned(),
            Self::SidebarTeam(color) => format!("sidebar.team.{}", color.name()),
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "list" => Some(Self::PlayerList),
            "sidebar" => Some(Self::Sidebar),
            "below_name" => Some(Self::BelowName),
            _ => name
                .strip_prefix("sidebar.team.")
                .and_then(TeamColor::from_name)
                .map(Self::SidebarTeam),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum RenderType {
    #[default]
    Integer,
    Hearts,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Visibility {
    Always,
    Never,
    HideForOtherTeams,
    HideForOwnTeam,
}

impl Visibility {
    pub const fn name(self) -> &'static str {
        match self {
            Self::Always => "always",
            Self::Never => "never",
            Self::HideForOtherTeams => "hideForOtherTeams",
            Self::HideForOwnTeam => "hideForOwnTeam",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        [
            Self::Always,
            Self::Never,
            Self::HideForOtherTeams,
            Self::HideForOwnTeam,
        ]
        .into_iter()
        .find(|value| value.name() == name)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CollisionRule {
    Always,
    Never,
    PushOtherTeams,
    PushOwnTeam,
}

impl CollisionRule {
    pub const fn name(self) -> &'static str {
        match self {
            Self::Always => "always",
            Self::Never => "never",
            Self::PushOtherTeams => "pushOtherTeams",
            Self::PushOwnTeam => "pushOwnTeam",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        [
            Self::Always,
            Self::Never,
            Self::PushOtherTeams,
            Self::PushOwnTeam,
        ]
        .into_iter()
        .find(|value| value.name() == name)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Criteria {
    pub name: String,
}

impl Criteria {
    pub fn named(name: impl Into<String>) -> Self {
        Self { name: name.into() }
    }

    pub fn dummy() -> Self {
        Self::named("dummy")
    }

    pub fn trigger() -> Self {
        Self::named("trigger")
    }

    pub fn health() -> Self {
        Self::named("health")
    }

    pub fn food() -> Self {
        Self::named("food")
    }

    pub fn air() -> Self {
        Self::named("air")
    }

    pub fn armor() -> Self {
        Self::named("armor")
    }

    pub fn xp() -> Self {
        Self::named("xp")
    }

    pub fn level() -> Self {
        Self::named("level")
    }

    pub fn death_count() -> Self {
        Self::named("deathCount")
    }

    pub fn player_kill_count() -> Self {
        Self::named("playerKillCount")
    }

    pub fn total_kill_count() -> Self {
        Self::named("totalKillCount")
    }

    pub fn team_kill(color: TeamColor) -> Self {
        Self {
            name: format!("teamkill.{}", color.name()),
        }
    }

    pub fn killed_by_team(color: TeamColor) -> Self {
        Self {
            name: format!("killedByTeam.{}", color.name()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObjectiveState {
    pub name: String,
    pub criteria: Criteria,
    pub display_name: String,
    pub render_type: RenderType,
    pub read_only: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScoreEntry {
    pub entry: String,
    pub objective: String,
    pub value: i32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TeamState {
    pub name: String,
    pub display_name: String,
    pub prefix: String,
    pub suffix: String,
    pub color: Option<TeamColor>,
    pub friendly_fire: bool,
    pub see_friendly_invisibles: bool,
    pub nametag_visibility: Visibility,
    pub death_message_visibility: Visibility,
    pub collision_rule: CollisionRule,
    pub entries: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScoreboardState {
    pub objectives: Vec<ObjectiveState>,
    pub scores: Vec<ScoreEntry>,
    pub teams: Vec<TeamState>,
    pub slots: Vec<(DisplaySlot, String)>,
}

fn unknown<T>(what: &str, value: &str) -> Result<T, Error> {
    Err(Error::Malformed(format!("an unknown {what} {value}")))
}

impl ScoreboardState {
    fn from_wire(state: v1::ScoreboardState) -> Result<Self, Error> {
        let mut objectives = Vec::new();
        for objective in state.objectives {
            objectives.push(ObjectiveState {
                name: objective.name,
                criteria: Criteria::named(objective.criteria),
                display_name: objective.display_name,
                render_type: if objective.hearts {
                    RenderType::Hearts
                } else {
                    RenderType::Integer
                },
                read_only: objective.read_only,
            });
        }
        let mut teams = Vec::new();
        for team in state.teams {
            let color = match team.color.as_deref() {
                None => None,
                Some(name) => match TeamColor::from_name(name) {
                    Some(color) => Some(color),
                    None => return unknown("team color", name),
                },
            };
            let Some(nametag_visibility) = Visibility::from_name(&team.nametag_visibility) else {
                return unknown("visibility", &team.nametag_visibility);
            };
            let Some(death_message_visibility) =
                Visibility::from_name(&team.death_message_visibility)
            else {
                return unknown("visibility", &team.death_message_visibility);
            };
            let Some(collision_rule) = CollisionRule::from_name(&team.collision_rule) else {
                return unknown("collision rule", &team.collision_rule);
            };
            teams.push(TeamState {
                name: team.name,
                display_name: team.display_name,
                prefix: team.prefix,
                suffix: team.suffix,
                color,
                friendly_fire: team.friendly_fire,
                see_friendly_invisibles: team.see_friendly_invisibles,
                nametag_visibility,
                death_message_visibility,
                collision_rule,
                entries: team.entries,
            });
        }
        let mut slots = Vec::new();
        for slot in state.slots {
            let Some(parsed) = DisplaySlot::from_name(&slot.slot) else {
                return unknown("display slot", &slot.slot);
            };
            slots.push((parsed, slot.objective));
        }
        Ok(Self {
            objectives,
            scores: state
                .scores
                .into_iter()
                .map(|score| ScoreEntry {
                    entry: score.entry,
                    objective: score.objective,
                    value: score.value,
                })
                .collect(),
            teams,
            slots,
        })
    }
}

#[derive(Debug, Clone)]
pub struct ScoreboardManager {
    link: Link,
}

impl ScoreboardManager {
    pub(crate) fn new(link: Link) -> Self {
        Self { link }
    }

    pub fn main_scoreboard(&self) -> Scoreboard {
        Scoreboard::new(0, self.link.clone())
    }

    pub async fn new_scoreboard(&self) -> Result<Scoreboard, Error> {
        let board = self
            .link
            .ask(
                question::Question::NewScoreboard(v1::Empty {}),
                "a new scoreboard",
                |body| match body {
                    answer::Body::ScoreboardCreated(board) => Some(board),
                    _ => None,
                },
            )
            .await?;
        Ok(Scoreboard::new(board, self.link.clone()))
    }
}

#[derive(Debug, Clone)]
pub struct Scoreboard {
    pub id: u32,
    link: Link,
}

impl PartialEq for Scoreboard {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl Scoreboard {
    pub(crate) fn new(id: u32, link: Link) -> Self {
        Self { id, link }
    }

    pub const fn is_main(&self) -> bool {
        self.id == 0
    }

    pub async fn state(&self) -> Result<ScoreboardState, Error> {
        let state = self
            .link
            .ask(
                question::Question::Scoreboard(self.id),
                "a scoreboard",
                |body| match body {
                    answer::Body::Scoreboard(state) => Some(state),
                    _ => None,
                },
            )
            .await?;
        ScoreboardState::from_wire(state)
    }

    pub fn register_objective(
        &self,
        name: impl Into<String>,
        criteria: &Criteria,
        display_name: impl Into<String>,
    ) -> Objective {
        let name = name.into();
        self.order(Order::RegisterObjective(v1::RegisterObjective {
            name: name.clone(),
            criteria: criteria.name.clone(),
            display_name: display_name.into(),
        }));
        self.objective(name)
    }

    pub fn objective(&self, name: impl Into<String>) -> Objective {
        Objective {
            scoreboard: self.clone(),
            name: name.into(),
        }
    }

    pub fn set_display_slot(&self, slot: DisplaySlot, objective: Option<&Objective>) {
        self.order(Order::SetDisplaySlot(v1::DisplaySlotOrder {
            slot: slot.name(),
            objective: objective.map(|objective| objective.name.clone()),
        }));
    }

    pub fn register_team(&self, name: impl Into<String>) -> Team {
        let name = name.into();
        self.order(Order::RegisterTeam(name.clone()));
        self.team(name)
    }

    pub fn team(&self, name: impl Into<String>) -> Team {
        Team {
            scoreboard: self.clone(),
            name: name.into(),
        }
    }

    pub fn reset_scores(&self, entry: impl Into<String>) {
        self.order(Order::ResetScore(v1::ResetScoreOrder {
            entry: entry.into(),
            objective: None,
        }));
    }

    pub fn discard(&self) {
        self.order(Order::Discard(v1::Empty {}));
    }

    fn order(&self, order: Order) {
        self.link
            .order(from_plugin::Body::Scoreboard(v1::ScoreboardOrder {
                board: self.id,
                order: Some(order),
            }));
    }
}

#[derive(Debug, Clone)]
pub struct Objective {
    pub scoreboard: Scoreboard,
    pub name: String,
}

impl Objective {
    pub fn set_display_name(&self, text: impl Into<String>) {
        self.scoreboard
            .order(Order::SetObjectiveDisplayName(v1::ObjectiveText {
                objective: self.name.clone(),
                text: text.into(),
            }));
    }

    pub fn set_render_type(&self, render: RenderType) {
        self.scoreboard
            .order(Order::SetObjectiveRender(v1::ObjectiveRender {
                objective: self.name.clone(),
                hearts: render == RenderType::Hearts,
            }));
    }

    pub fn unregister(&self) {
        self.scoreboard
            .order(Order::UnregisterObjective(self.name.clone()));
    }

    pub fn score(&self, entry: impl Into<String>) -> Score {
        Score {
            objective: self.clone(),
            entry: entry.into(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Score {
    pub objective: Objective,
    pub entry: String,
}

impl Score {
    pub fn set(&self, value: i32) {
        self.objective
            .scoreboard
            .order(Order::SetScore(v1::ScoreOrder {
                entry: self.entry.clone(),
                objective: self.objective.name.clone(),
                value,
            }));
    }

    pub fn reset(&self) {
        self.objective
            .scoreboard
            .order(Order::ResetScore(v1::ResetScoreOrder {
                entry: self.entry.clone(),
                objective: Some(self.objective.name.clone()),
            }));
    }

    pub async fn get(&self) -> Result<i32, Error> {
        let state = self.objective.scoreboard.state().await?;
        Ok(state
            .scores
            .iter()
            .find(|score| score.entry == self.entry && score.objective == self.objective.name)
            .map_or(0, |score| score.value))
    }
}

#[derive(Debug, Clone)]
pub struct Team {
    pub scoreboard: Scoreboard,
    pub name: String,
}

impl Team {
    pub fn set_display_name(&self, text: impl Into<String>) {
        self.option(TeamChoice::DisplayName(text.into()));
    }

    pub fn set_prefix(&self, text: impl Into<String>) {
        self.option(TeamChoice::Prefix(text.into()));
    }

    pub fn set_suffix(&self, text: impl Into<String>) {
        self.option(TeamChoice::Suffix(text.into()));
    }

    pub fn set_color(&self, color: Option<TeamColor>) {
        self.option(TeamChoice::Color(
            color.map_or_else(String::new, |color| color.name().to_owned()),
        ));
    }

    pub fn set_friendly_fire(&self, allowed: bool) {
        self.option(TeamChoice::FriendlyFire(allowed));
    }

    pub fn set_see_friendly_invisibles(&self, allowed: bool) {
        self.option(TeamChoice::SeeFriendlyInvisibles(allowed));
    }

    pub fn set_nametag_visibility(&self, visibility: Visibility) {
        self.option(TeamChoice::NametagVisibility(visibility.name().to_owned()));
    }

    pub fn set_death_message_visibility(&self, visibility: Visibility) {
        self.option(TeamChoice::DeathMessageVisibility(
            visibility.name().to_owned(),
        ));
    }

    pub fn set_collision_rule(&self, rule: CollisionRule) {
        self.option(TeamChoice::CollisionRule(rule.name().to_owned()));
    }

    pub fn add_entry(&self, entry: impl Into<String>) {
        self.entry(entry.into(), true);
    }

    pub fn remove_entry(&self, entry: impl Into<String>) {
        self.entry(entry.into(), false);
    }

    pub fn unregister(&self) {
        self.scoreboard
            .order(Order::UnregisterTeam(self.name.clone()));
    }

    fn entry(&self, entry: String, join: bool) {
        self.scoreboard.order(Order::TeamEntry(v1::TeamEntry {
            team: self.name.clone(),
            entry,
            join,
        }));
    }

    fn option(&self, option: TeamChoice) {
        self.scoreboard.order(Order::SetTeamOption(v1::TeamOption {
            team: self.name.clone(),
            option: Some(option),
        }));
    }
}

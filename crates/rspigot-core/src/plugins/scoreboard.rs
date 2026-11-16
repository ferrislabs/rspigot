use rspigot_domain::player::PlayerId;
use rspigot_domain::scoreboard::{
    CollisionRule, DisplaySlot, Objective, RenderType, Scoreboard, Team, Visibility,
};
use rspigot_domain::text::NamedColor;

use crate::ports::ClientLink;
use crate::scoreboard::{MAIN_BOARD, parse_criteria};
use crate::server::Server;

use super::resolve;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TeamOptionSpec {
    DisplayName(String),
    Prefix(String),
    Suffix(String),
    Color(String),
    FriendlyFire(bool),
    SeeFriendlyInvisibles(bool),
    NametagVisibility(String),
    DeathMessageVisibility(String),
    CollisionRule(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScoreboardOrder {
    RegisterObjective {
        name: String,
        criteria: String,
        display_name: String,
    },
    UnregisterObjective(String),
    SetObjectiveDisplayName {
        objective: String,
        text: String,
    },
    SetObjectiveRender {
        objective: String,
        hearts: bool,
    },
    SetDisplaySlot {
        slot: String,
        objective: Option<String>,
    },
    SetScore {
        entry: String,
        objective: String,
        value: i32,
    },
    ResetScore {
        entry: String,
        objective: Option<String>,
    },
    RegisterTeam(String),
    UnregisterTeam(String),
    SetTeamOption {
        team: String,
        option: TeamOptionSpec,
    },
    TeamEntry {
        team: String,
        entry: String,
        join: bool,
    },
    Discard,
}

impl ScoreboardOrder {
    pub const fn name(&self) -> &'static str {
        match self {
            Self::RegisterObjective { .. } => "scoreboard.register_objective",
            Self::UnregisterObjective(_) => "scoreboard.unregister_objective",
            Self::SetObjectiveDisplayName { .. } => "scoreboard.set_objective_display_name",
            Self::SetObjectiveRender { .. } => "scoreboard.set_objective_render",
            Self::SetDisplaySlot { .. } => "scoreboard.set_display_slot",
            Self::SetScore { .. } => "scoreboard.set_score",
            Self::ResetScore { .. } => "scoreboard.reset_score",
            Self::RegisterTeam(_) => "scoreboard.register_team",
            Self::UnregisterTeam(_) => "scoreboard.unregister_team",
            Self::SetTeamOption { .. } => "scoreboard.set_team_option",
            Self::TeamEntry { .. } => "scoreboard.team_entry",
            Self::Discard => "scoreboard.discard",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObjectiveView {
    pub name: String,
    pub criteria: String,
    pub display_name: String,
    pub hearts: bool,
    pub read_only: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScoreView {
    pub entry: String,
    pub objective: String,
    pub value: i32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TeamView {
    pub name: String,
    pub display_name: String,
    pub prefix: String,
    pub suffix: String,
    pub color: Option<String>,
    pub friendly_fire: bool,
    pub see_friendly_invisibles: bool,
    pub nametag_visibility: String,
    pub death_message_visibility: String,
    pub collision_rule: String,
    pub entries: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ScoreboardView {
    pub objectives: Vec<ObjectiveView>,
    pub scores: Vec<ScoreView>,
    pub teams: Vec<TeamView>,
    pub slots: Vec<(String, String)>,
}

fn view_of(board: &Scoreboard) -> ScoreboardView {
    let objectives = board
        .objectives()
        .map(|objective| ObjectiveView {
            name: objective.name.clone(),
            criteria: objective.criteria.name(),
            display_name: objective.display_name.to_legacy(),
            hearts: objective.render_type == RenderType::Hearts,
            read_only: objective.criteria.read_only(),
        })
        .collect();
    let scores = board
        .holders()
        .into_iter()
        .flat_map(|holder| {
            board
                .holder_scores(holder)
                .into_iter()
                .map(move |(objective, score)| ScoreView {
                    entry: holder.to_owned(),
                    objective: objective.to_owned(),
                    value: score.value,
                })
        })
        .collect();
    let teams = board
        .teams()
        .map(|team| TeamView {
            name: team.name.clone(),
            display_name: team.display_name.to_legacy(),
            prefix: team.prefix.to_legacy(),
            suffix: team.suffix.to_legacy(),
            color: team.color.map(|color| color.name().to_owned()),
            friendly_fire: team.friendly_fire,
            see_friendly_invisibles: team.see_friendly_invisibles,
            nametag_visibility: team.nametag_visibility.name().to_owned(),
            death_message_visibility: team.death_message_visibility.name().to_owned(),
            collision_rule: team.collision_rule.name().to_owned(),
            entries: board
                .team_members(&team.name)
                .into_iter()
                .map(str::to_owned)
                .collect(),
        })
        .collect();
    let slots = DisplaySlot::all()
        .filter_map(|slot| {
            board
                .display(slot)
                .map(|objective| (slot.name(), objective.to_owned()))
        })
        .collect();
    ScoreboardView {
        objectives,
        scores,
        teams,
        slots,
    }
}

fn unknown_objective(name: &str) -> String {
    format!("there is no objective {name}")
}

fn unknown_team(name: &str) -> String {
    format!("there is no team {name}")
}

fn apply_team_option(team: &mut Team, option: TeamOptionSpec) -> Result<(), String> {
    match option {
        TeamOptionSpec::DisplayName(text) => team.display_name = resolve::text(&text),
        TeamOptionSpec::Prefix(text) => team.prefix = resolve::text(&text),
        TeamOptionSpec::Suffix(text) => team.suffix = resolve::text(&text),
        TeamOptionSpec::Color(name) => {
            team.color = if name.is_empty() || name == "reset" {
                None
            } else {
                Some(
                    NamedColor::from_name(&name)
                        .ok_or_else(|| format!("{name} is not a team color"))?,
                )
            };
        }
        TeamOptionSpec::FriendlyFire(value) => team.friendly_fire = value,
        TeamOptionSpec::SeeFriendlyInvisibles(value) => team.see_friendly_invisibles = value,
        TeamOptionSpec::NametagVisibility(name) => {
            team.nametag_visibility = Visibility::from_name(&name)
                .ok_or_else(|| format!("{name} is not a visibility"))?;
        }
        TeamOptionSpec::DeathMessageVisibility(name) => {
            team.death_message_visibility = Visibility::from_name(&name)
                .ok_or_else(|| format!("{name} is not a visibility"))?;
        }
        TeamOptionSpec::CollisionRule(name) => {
            team.collision_rule = CollisionRule::from_name(&name)
                .ok_or_else(|| format!("{name} is not a collision rule"))?;
        }
    }
    Ok(())
}

impl<L: ClientLink> Server<L> {
    pub(super) fn apply_scoreboard_order(
        &mut self,
        connection: u64,
        board: u32,
        order: ScoreboardOrder,
    ) {
        let name = order.name();
        if let Err(reason) = self.scoreboard_order(board, order) {
            self.warn_plugin(connection, name, reason);
        }
    }

    fn scoreboard_order(&mut self, board: u32, order: ScoreboardOrder) -> Result<(), String> {
        if matches!(order, ScoreboardOrder::Discard) {
            if board == MAIN_BOARD {
                return Err("the main scoreboard cannot be discarded".to_owned());
            }
            self.discard_board(board);
            return Ok(());
        }
        let holder_display = |server: &Self, entry: &str| server.holder_display_name(entry);
        let display = match &order {
            ScoreboardOrder::SetScore { entry, .. } => holder_display(self, entry),
            _ => None,
        };
        self.change_board(board, |scoreboard| {
            let outcome = scoreboard_change(scoreboard, order, display);
            match outcome {
                Ok(messages) => (Ok(()), messages),
                Err(reason) => (Err(reason), Vec::new()),
            }
        })
        .unwrap_or_else(|| Err(format!("there is no scoreboard {board}")))
    }

    pub(super) fn scoreboard_view(&self, board: u32) -> Option<ScoreboardView> {
        self.scoreboard.board_by_id(board).map(view_of)
    }

    pub(super) fn new_plugin_scoreboard(&mut self) -> u32 {
        self.scoreboard.create_board()
    }

    pub(super) fn set_player_board(&mut self, player: PlayerId, board: u32) -> Result<(), String> {
        if self.scoreboard.board_by_id(board).is_none() {
            return Err(format!("there is no scoreboard {board}"));
        }
        self.assign_scoreboard(player, board);
        Ok(())
    }
}

fn scoreboard_change(
    board: &mut Scoreboard,
    order: ScoreboardOrder,
    display: Option<rspigot_domain::text::Component>,
) -> Result<Vec<rspigot_domain::scoreboard::ScoreboardMessage>, String> {
    let known = |board: &Scoreboard, name: &str| {
        board
            .objective(name)
            .map(|_| ())
            .ok_or_else(|| unknown_objective(name))
    };
    let missing_team = |board: &Scoreboard, name: &str| {
        board
            .team(name)
            .map(|_| ())
            .ok_or_else(|| unknown_team(name))
    };
    match order {
        ScoreboardOrder::RegisterObjective {
            name,
            criteria,
            display_name,
        } => {
            let criteria =
                parse_criteria(&criteria).ok_or_else(|| format!("{criteria} is not a criteria"))?;
            let objective = Objective::new(name, criteria, resolve::text(&display_name));
            board.add_objective(objective).map_err(|e| e.to_string())?;
            Ok(Vec::new())
        }
        ScoreboardOrder::UnregisterObjective(name) => {
            known(board, &name)?;
            Ok(board.remove_objective(&name))
        }
        ScoreboardOrder::SetObjectiveDisplayName { objective, text } => {
            known(board, &objective)?;
            Ok(board.modify_objective(&objective, |o| o.display_name = resolve::text(&text)))
        }
        ScoreboardOrder::SetObjectiveRender { objective, hearts } => {
            known(board, &objective)?;
            let render = if hearts {
                RenderType::Hearts
            } else {
                RenderType::Integer
            };
            Ok(board.modify_objective(&objective, |o| o.render_type = render))
        }
        ScoreboardOrder::SetDisplaySlot { slot, objective } => {
            let slot = DisplaySlot::from_name(&slot)
                .ok_or_else(|| format!("{slot} is not a display slot"))?;
            board
                .set_display(slot, objective.as_deref())
                .map_err(|e| e.to_string())
        }
        ScoreboardOrder::SetScore {
            entry,
            objective,
            value,
        } => board
            .set_score(&entry, &objective, value, display)
            .map_err(|e| e.to_string()),
        ScoreboardOrder::ResetScore { entry, objective } => Ok(match objective {
            Some(objective) => board.reset_score(&entry, &objective),
            None => board.reset_holder(&entry),
        }),
        ScoreboardOrder::RegisterTeam(name) => board
            .add_team(Team::new(
                name.clone(),
                rspigot_domain::text::Component::text(name),
            ))
            .map_err(|e| e.to_string()),
        ScoreboardOrder::UnregisterTeam(name) => {
            missing_team(board, &name)?;
            Ok(board.remove_team(&name))
        }
        ScoreboardOrder::SetTeamOption { team, option } => {
            missing_team(board, &team)?;
            let mut failure = None;
            let messages = board.modify_team(&team, |team| {
                if let Err(reason) = apply_team_option(team, option) {
                    failure = Some(reason);
                }
            });
            match failure {
                Some(reason) => Err(reason),
                None => Ok(messages),
            }
        }
        ScoreboardOrder::TeamEntry { team, entry, join } => {
            missing_team(board, &team)?;
            if join {
                board.join_team(&entry, &team).map_err(|e| e.to_string())
            } else if board.team_of(&entry).is_some_and(|t| t.name == team) {
                Ok(board.leave_team(&entry))
            } else {
                Err(format!("{entry} is not on team {team}"))
            }
        }
        ScoreboardOrder::Discard => Ok(Vec::new()),
    }
}

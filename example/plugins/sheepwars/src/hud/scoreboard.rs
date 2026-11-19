use rspigot_plugin::{Criteria, DisplaySlot, Error, Objective, Scoreboard, ScoreboardState, Team};

use crate::domain::{PlayerId, Round, TeamId};

use super::view::{SIDEBAR_TITLE, sidebar_lines, team_color, team_name, team_prefix};

const OBJECTIVE_NAME: &str = "sheepwars";

const fn team_key(team: TeamId) -> &'static str {
    match team {
        TeamId::Red => "sw_red",
        TeamId::Blue => "sw_blue",
    }
}

#[derive(Debug)]
pub struct Hud {
    scoreboard: Scoreboard,
    objective: Objective,
    red: Team,
    blue: Team,
    shown: Vec<String>,
}

impl Hud {
    pub async fn open(scoreboard: Scoreboard) -> Result<Self, Error> {
        let reuse = Reuse::of(&scoreboard.state().await?);
        for entry in &reuse.stale_scores {
            scoreboard.reset_scores(entry.as_str());
        }
        let objective = if reuse.objective {
            scoreboard.objective(OBJECTIVE_NAME)
        } else {
            scoreboard.register_objective(OBJECTIVE_NAME, &Criteria::dummy(), SIDEBAR_TITLE)
        };
        objective.set_display_name(SIDEBAR_TITLE);
        scoreboard.set_display_slot(DisplaySlot::Sidebar, Some(&objective));
        let red = Self::open_team(&scoreboard, TeamId::Red, reuse.has_team(TeamId::Red));
        let blue = Self::open_team(&scoreboard, TeamId::Blue, reuse.has_team(TeamId::Blue));
        Ok(Self {
            scoreboard,
            objective,
            red,
            blue,
            shown: Vec::new(),
        })
    }

    pub fn scoreboard(&self) -> &Scoreboard {
        &self.scoreboard
    }

    pub fn add_player(&self, team: TeamId, name: &str) {
        self.team(team).add_entry(name);
    }

    pub fn remove_player(&self, team: TeamId, name: &str) {
        self.team(team).remove_entry(name);
    }

    pub fn render(&mut self, round: &Round, viewer: Option<PlayerId>) {
        let lines = sidebar_lines(round, viewer);
        for stale in self.shown.iter().filter(|line| !lines.contains(line)) {
            self.scoreboard.reset_scores(stale.as_str());
        }
        let top = i32::try_from(lines.len()).unwrap_or(i32::MAX);
        for (index, line) in lines.iter().enumerate() {
            let offset = i32::try_from(index).unwrap_or(i32::MAX);
            self.objective
                .score(line.as_str())
                .set(top.saturating_sub(offset));
        }
        self.shown = lines;
    }

    pub fn close(&self) {
        self.scoreboard.set_display_slot(DisplaySlot::Sidebar, None);
        self.objective.unregister();
        self.red.unregister();
        self.blue.unregister();
    }

    fn team(&self, id: TeamId) -> &Team {
        match id {
            TeamId::Red => &self.red,
            TeamId::Blue => &self.blue,
        }
    }

    fn open_team(scoreboard: &Scoreboard, id: TeamId, existing: bool) -> Team {
        let team = if existing {
            scoreboard.team(team_key(id))
        } else {
            scoreboard.register_team(team_key(id))
        };
        team.set_display_name(team_name(id));
        team.set_prefix(team_prefix(id));
        team.set_color(Some(team_color(id)));
        team.set_friendly_fire(false);
        team
    }
}

#[derive(Debug, Default, PartialEq, Eq)]
struct Reuse {
    objective: bool,
    red: bool,
    blue: bool,
    stale_scores: Vec<String>,
}

impl Reuse {
    fn of(state: &ScoreboardState) -> Self {
        Self {
            objective: state
                .objectives
                .iter()
                .any(|objective| objective.name == OBJECTIVE_NAME),
            red: has_team(state, TeamId::Red),
            blue: has_team(state, TeamId::Blue),
            stale_scores: state
                .scores
                .iter()
                .filter(|score| score.objective == OBJECTIVE_NAME)
                .map(|score| score.entry.clone())
                .collect(),
        }
    }

    const fn has_team(&self, id: TeamId) -> bool {
        match id {
            TeamId::Red => self.red,
            TeamId::Blue => self.blue,
        }
    }
}

fn has_team(state: &ScoreboardState, id: TeamId) -> bool {
    state.teams.iter().any(|team| team.name == team_key(id))
}

#[cfg(test)]
mod tests {
    use rspigot_plugin::{
        CollisionRule, ObjectiveState, RenderType, ScoreEntry, TeamState, Visibility,
    };

    use super::*;

    fn objective(name: &str) -> ObjectiveState {
        ObjectiveState {
            name: name.to_owned(),
            criteria: Criteria::dummy(),
            display_name: name.to_owned(),
            render_type: RenderType::Integer,
            read_only: false,
        }
    }

    fn team(name: &str) -> TeamState {
        TeamState {
            name: name.to_owned(),
            display_name: name.to_owned(),
            prefix: String::new(),
            suffix: String::new(),
            color: None,
            friendly_fire: true,
            see_friendly_invisibles: true,
            nametag_visibility: Visibility::Always,
            death_message_visibility: Visibility::Always,
            collision_rule: CollisionRule::Always,
            entries: Vec::new(),
        }
    }

    fn score(entry: &str, objective: &str) -> ScoreEntry {
        ScoreEntry {
            entry: entry.to_owned(),
            objective: objective.to_owned(),
            value: 1,
        }
    }

    fn state(
        objectives: Vec<ObjectiveState>,
        teams: Vec<TeamState>,
        scores: Vec<ScoreEntry>,
    ) -> ScoreboardState {
        ScoreboardState {
            objectives,
            scores,
            teams,
            slots: Vec::new(),
        }
    }

    #[test]
    fn a_clean_scoreboard_reuses_nothing() {
        assert_eq!(Reuse::of(&state(vec![], vec![], vec![])), Reuse::default());
    }

    #[test]
    fn leftovers_of_a_previous_run_are_reused_and_their_lines_cleared() {
        let leftovers = state(
            vec![objective(OBJECTIVE_NAME), objective("other")],
            vec![team("sw_red"), team("sw_blue"), team("unrelated")],
            vec![score("old line", OBJECTIVE_NAME), score("kept", "other")],
        );
        let reuse = Reuse::of(&leftovers);
        assert!(reuse.objective);
        assert!(reuse.has_team(TeamId::Red));
        assert!(reuse.has_team(TeamId::Blue));
        assert_eq!(reuse.stale_scores, vec!["old line".to_owned()]);
    }

    #[test]
    fn a_missing_team_is_not_reused() {
        let reuse = Reuse::of(&state(vec![], vec![team("sw_red")], vec![]));
        assert!(reuse.has_team(TeamId::Red));
        assert!(!reuse.has_team(TeamId::Blue));
    }
}

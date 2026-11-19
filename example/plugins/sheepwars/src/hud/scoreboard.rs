use rspigot_plugin::{Criteria, DisplaySlot, Objective, Scoreboard, Team};

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
    pub fn new(scoreboard: Scoreboard) -> Self {
        let objective =
            scoreboard.register_objective(OBJECTIVE_NAME, &Criteria::dummy(), SIDEBAR_TITLE);
        scoreboard.set_display_slot(DisplaySlot::Sidebar, Some(&objective));
        let red = Self::register_team(&scoreboard, TeamId::Red);
        let blue = Self::register_team(&scoreboard, TeamId::Blue);
        Self {
            scoreboard,
            objective,
            red,
            blue,
            shown: Vec::new(),
        }
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

    fn register_team(scoreboard: &Scoreboard, id: TeamId) -> Team {
        let team = scoreboard.register_team(team_key(id));
        team.set_display_name(team_name(id));
        team.set_prefix(team_prefix(id));
        team.set_color(Some(team_color(id)));
        team.set_friendly_fire(false);
        team
    }
}

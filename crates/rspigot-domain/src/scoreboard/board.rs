use std::collections::BTreeMap;

use crate::text::Component;

use super::message::{ObjectiveView, ScoreboardMessage, TeamOp, TeamView};
use super::{Criteria, DISPLAY_SLOT_COUNT, DisplaySlot, NumberFormat, RenderType, Team};

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ScoreboardError {
    #[error("an objective named {0} already exists")]
    ObjectiveExists(String),
    #[error("unknown objective {0}")]
    UnknownObjective(String),
    #[error("a team named {0} already exists")]
    TeamExists(String),
    #[error("unknown team {0}")]
    UnknownTeam(String),
}

type Messages = Vec<ScoreboardMessage>;

#[derive(Debug, Clone, PartialEq)]
pub struct Objective {
    pub name: String,
    pub criteria: Criteria,
    pub display_name: Component,
    pub render_type: RenderType,
    pub display_auto_update: bool,
    pub number_format: Option<NumberFormat>,
}

impl Objective {
    pub fn new(name: impl Into<String>, criteria: Criteria, display_name: Component) -> Self {
        Self {
            name: name.into(),
            render_type: criteria.render_type(),
            criteria,
            display_name,
            display_auto_update: false,
            number_format: None,
        }
    }

    fn view(&self) -> ObjectiveView {
        ObjectiveView {
            name: self.name.clone(),
            display_name: self.display_name.clone(),
            render_type: self.render_type,
            number_format: self.number_format.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Score {
    pub value: i32,
    pub locked: bool,
    pub display: Option<Component>,
    pub number_format: Option<NumberFormat>,
}

impl Default for Score {
    fn default() -> Self {
        Self {
            value: 0,
            locked: true,
            display: None,
            number_format: None,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct Scoreboard {
    pub(super) objectives: BTreeMap<String, Objective>,
    pub(super) scores: BTreeMap<String, BTreeMap<String, Score>>,
    pub(super) display: [Option<String>; DISPLAY_SLOT_COUNT],
    pub(super) teams: BTreeMap<String, Team>,
    pub(super) membership: BTreeMap<String, String>,
}

impl Scoreboard {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn objective(&self, name: &str) -> Option<&Objective> {
        self.objectives.get(name)
    }

    pub fn objectives(&self) -> impl Iterator<Item = &Objective> {
        self.objectives.values()
    }

    pub fn objectives_for(&self, criteria: &Criteria) -> Vec<String> {
        self.objectives
            .values()
            .filter(|objective| &objective.criteria == criteria)
            .map(|objective| objective.name.clone())
            .collect()
    }

    pub fn add_objective(&mut self, objective: Objective) -> Result<(), ScoreboardError> {
        if self.objectives.contains_key(&objective.name) {
            return Err(ScoreboardError::ObjectiveExists(objective.name));
        }
        self.objectives.insert(objective.name.clone(), objective);
        Ok(())
    }

    pub fn remove_objective(&mut self, name: &str) -> Messages {
        if !self.objectives.contains_key(name) {
            return Vec::new();
        }
        let mut out = Messages::new();
        for slot in DisplaySlot::all() {
            if self.display[slot.index()].as_deref() == Some(name) {
                out.extend(self.set_display(slot, None).unwrap_or_default());
            }
        }
        self.objectives.remove(name);
        for scores in self.scores.values_mut() {
            scores.remove(name);
        }
        self.scores.retain(|_, scores| !scores.is_empty());
        out
    }

    pub fn modify_objective(
        &mut self,
        name: &str,
        change: impl FnOnce(&mut Objective),
    ) -> Messages {
        let Some(objective) = self.objectives.get_mut(name) else {
            return Vec::new();
        };
        change(objective);
        let view = objective.view();
        if self.slots_showing(name) > 0 {
            vec![ScoreboardMessage::ObjectiveChange(view)]
        } else {
            Vec::new()
        }
    }

    pub fn display(&self, slot: DisplaySlot) -> Option<&str> {
        self.display[slot.index()].as_deref()
    }

    pub fn slots_showing(&self, objective: &str) -> usize {
        self.display
            .iter()
            .filter(|shown| shown.as_deref() == Some(objective))
            .count()
    }

    pub fn set_display(
        &mut self,
        slot: DisplaySlot,
        objective: Option<&str>,
    ) -> Result<Messages, ScoreboardError> {
        if let Some(name) = objective
            && !self.objectives.contains_key(name)
        {
            return Err(ScoreboardError::UnknownObjective(name.to_owned()));
        }
        let new = objective.map(str::to_owned);
        let was_tracked = new
            .as_deref()
            .is_some_and(|name| self.slots_showing(name) > 0);
        let old = std::mem::replace(&mut self.display[slot.index()], new.clone());
        let mut out = Messages::new();
        if old != new
            && let Some(old_name) = &old
        {
            if self.slots_showing(old_name) > 0 {
                out.push(ScoreboardMessage::DisplayObjective {
                    slot,
                    objective: new.clone(),
                });
            } else {
                out.push(ScoreboardMessage::ObjectiveRemove(old_name.clone()));
            }
        }
        if let Some(name) = new {
            if was_tracked {
                out.push(ScoreboardMessage::DisplayObjective {
                    slot,
                    objective: Some(name),
                });
            } else {
                out.extend(self.start_tracking(&name));
            }
        }
        Ok(out)
    }

    fn start_tracking(&self, name: &str) -> Messages {
        let Some(objective) = self.objectives.get(name) else {
            return Vec::new();
        };
        let mut out = vec![ScoreboardMessage::ObjectiveAdd(objective.view())];
        for slot in DisplaySlot::all() {
            if self.display[slot.index()].as_deref() == Some(name) {
                out.push(ScoreboardMessage::DisplayObjective {
                    slot,
                    objective: Some(name.to_owned()),
                });
            }
        }
        for (holder, scores) in &self.scores {
            if let Some(score) = scores.get(name) {
                out.push(score_message(holder, name, score));
            }
        }
        out
    }

    pub fn join_messages(&self) -> Messages {
        let mut out = Messages::new();
        for team in self.teams.values() {
            out.push(self.team_add_message(team));
        }
        let mut done: Vec<&str> = Vec::new();
        for slot in DisplaySlot::all() {
            if let Some(name) = self.display[slot.index()].as_deref()
                && !done.contains(&name)
            {
                out.extend(self.start_tracking(name));
                done.push(name);
            }
        }
        out
    }

    pub fn score(&self, holder: &str, objective: &str) -> Option<&Score> {
        self.scores.get(holder)?.get(objective)
    }

    pub fn holders(&self) -> Vec<&str> {
        self.scores.keys().map(String::as_str).collect()
    }

    pub fn holder_scores(&self, holder: &str) -> Vec<(&str, &Score)> {
        self.scores
            .get(holder)
            .map(|scores| {
                scores
                    .iter()
                    .map(|(objective, score)| (objective.as_str(), score))
                    .collect()
            })
            .unwrap_or_default()
    }

    fn score_entry(
        &mut self,
        holder: &str,
        objective: &str,
    ) -> Result<(&mut Score, bool), ScoreboardError> {
        if !self.objectives.contains_key(objective) {
            return Err(ScoreboardError::UnknownObjective(objective.to_owned()));
        }
        let scores = self.scores.entry(holder.to_owned()).or_default();
        let created = !scores.contains_key(objective);
        Ok((scores.entry(objective.to_owned()).or_default(), created))
    }

    fn broadcast_score(&self, holder: &str, objective: &str) -> Messages {
        match self.score(holder, objective) {
            Some(score) if self.slots_showing(objective) > 0 => {
                vec![score_message(holder, objective, score)]
            }
            _ => Vec::new(),
        }
    }

    pub fn set_score(
        &mut self,
        holder: &str,
        objective: &str,
        value: i32,
        holder_display: Option<Component>,
    ) -> Result<Messages, ScoreboardError> {
        let auto_update = self
            .objectives
            .get(objective)
            .is_some_and(|objective| objective.display_auto_update);
        let (score, created) = self.score_entry(holder, objective)?;
        let mut sync = created;
        if auto_update
            && let Some(display) = holder_display
            && score.display.as_ref() != Some(&display)
        {
            score.display = Some(display);
            sync = true;
        }
        if score.value != value {
            score.value = value;
            sync = true;
        }
        Ok(if sync {
            self.broadcast_score(holder, objective)
        } else {
            Vec::new()
        })
    }

    pub fn add_score(
        &mut self,
        holder: &str,
        objective: &str,
        amount: i32,
        holder_display: Option<Component>,
    ) -> Result<(i32, Messages), ScoreboardError> {
        let current = self.score(holder, objective).map_or(0, |score| score.value);
        let value = current.wrapping_add(amount);
        let messages = self.set_score(holder, objective, value, holder_display)?;
        Ok((value, messages))
    }

    pub fn set_locked(
        &mut self,
        holder: &str,
        objective: &str,
        locked: bool,
    ) -> Result<Messages, ScoreboardError> {
        let (score, created) = self.score_entry(holder, objective)?;
        score.locked = locked;
        Ok(if created {
            self.broadcast_score(holder, objective)
        } else {
            Vec::new()
        })
    }

    pub fn set_score_display(
        &mut self,
        holder: &str,
        objective: &str,
        display: Option<Component>,
    ) -> Result<Messages, ScoreboardError> {
        let (score, created) = self.score_entry(holder, objective)?;
        let changed = score.display != display;
        score.display = display;
        Ok(if created || changed {
            self.broadcast_score(holder, objective)
        } else {
            Vec::new()
        })
    }

    pub fn set_score_number_format(
        &mut self,
        holder: &str,
        objective: &str,
        format: Option<NumberFormat>,
    ) -> Result<Messages, ScoreboardError> {
        let (score, _) = self.score_entry(holder, objective)?;
        score.number_format = format;
        Ok(self.broadcast_score(holder, objective))
    }

    pub fn reset_score(&mut self, holder: &str, objective: &str) -> Messages {
        let Some(scores) = self.scores.get_mut(holder) else {
            return Vec::new();
        };
        let removed = scores.remove(objective).is_some();
        if scores.is_empty() {
            self.scores.remove(holder);
        }
        if removed && self.slots_showing(objective) > 0 {
            vec![ScoreboardMessage::ResetScore {
                holder: holder.to_owned(),
                objective: Some(objective.to_owned()),
            }]
        } else {
            Vec::new()
        }
    }

    pub fn reset_holder(&mut self, holder: &str) -> Messages {
        if self.scores.remove(holder).is_some() {
            vec![ScoreboardMessage::ResetScore {
                holder: holder.to_owned(),
                objective: None,
            }]
        } else {
            Vec::new()
        }
    }

    pub fn team(&self, name: &str) -> Option<&Team> {
        self.teams.get(name)
    }

    pub fn teams(&self) -> impl Iterator<Item = &Team> {
        self.teams.values()
    }

    pub fn team_of(&self, holder: &str) -> Option<&Team> {
        self.teams.get(self.membership.get(holder)?)
    }

    pub fn team_members(&self, team: &str) -> Vec<&str> {
        self.membership
            .iter()
            .filter(|(_, member_of)| member_of.as_str() == team)
            .map(|(holder, _)| holder.as_str())
            .collect()
    }

    fn team_add_message(&self, team: &Team) -> ScoreboardMessage {
        ScoreboardMessage::Team {
            name: team.name.clone(),
            op: TeamOp::Add {
                view: TeamView::of(team),
                members: self
                    .team_members(&team.name)
                    .into_iter()
                    .map(str::to_owned)
                    .collect(),
            },
        }
    }

    pub fn add_team(&mut self, team: Team) -> Result<Messages, ScoreboardError> {
        if self.teams.contains_key(&team.name) {
            return Err(ScoreboardError::TeamExists(team.name));
        }
        let message = self.team_add_message(&team);
        self.teams.insert(team.name.clone(), team);
        Ok(vec![message])
    }

    pub fn remove_team(&mut self, name: &str) -> Messages {
        if self.teams.remove(name).is_none() {
            return Vec::new();
        }
        self.membership.retain(|_, member_of| member_of != name);
        vec![ScoreboardMessage::Team {
            name: name.to_owned(),
            op: TeamOp::Remove,
        }]
    }

    pub fn modify_team(&mut self, name: &str, change: impl FnOnce(&mut Team)) -> Messages {
        let Some(team) = self.teams.get_mut(name) else {
            return Vec::new();
        };
        change(team);
        vec![ScoreboardMessage::Team {
            name: name.to_owned(),
            op: TeamOp::Modify(TeamView::of(team)),
        }]
    }

    pub fn join_team(&mut self, holder: &str, team: &str) -> Result<Messages, ScoreboardError> {
        if !self.teams.contains_key(team) {
            return Err(ScoreboardError::UnknownTeam(team.to_owned()));
        }
        let mut out = self.leave_team(holder);
        self.membership.insert(holder.to_owned(), team.to_owned());
        out.push(ScoreboardMessage::Team {
            name: team.to_owned(),
            op: TeamOp::Join(vec![holder.to_owned()]),
        });
        Ok(out)
    }

    pub fn leave_team(&mut self, holder: &str) -> Messages {
        match self.membership.remove(holder) {
            Some(team) => vec![ScoreboardMessage::Team {
                name: team,
                op: TeamOp::Leave(vec![holder.to_owned()]),
            }],
            None => Vec::new(),
        }
    }

    pub fn empty_team(&mut self, name: &str) -> (usize, Messages) {
        let members: Vec<String> = self
            .team_members(name)
            .into_iter()
            .map(str::to_owned)
            .collect();
        let mut out = Messages::new();
        for holder in &members {
            out.extend(self.leave_team(holder));
        }
        (members.len(), out)
    }
}

fn score_message(holder: &str, objective: &str, score: &Score) -> ScoreboardMessage {
    ScoreboardMessage::Score {
        holder: holder.to_owned(),
        objective: objective.to_owned(),
        value: score.value,
        display: score.display.clone(),
        number_format: score.number_format.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::text::NamedColor;

    fn board() -> Scoreboard {
        let mut board = Scoreboard::new();
        board
            .add_objective(Objective::new(
                "kills",
                Criteria::Dummy,
                Component::text("kills"),
            ))
            .unwrap();
        board
    }

    #[test]
    fn duplicate_objectives_are_rejected() {
        let mut board = board();
        let again = Objective::new("kills", Criteria::Trigger, Component::text("k"));
        assert_eq!(
            board.add_objective(again),
            Err(ScoreboardError::ObjectiveExists("kills".to_owned()))
        );
    }

    #[test]
    fn scores_stay_silent_until_the_objective_is_displayed() {
        let mut board = board();
        assert!(board.set_score("a", "kills", 3, None).unwrap().is_empty());
        let shown = board
            .set_display(DisplaySlot::Sidebar, Some("kills"))
            .unwrap();
        assert!(matches!(shown[0], ScoreboardMessage::ObjectiveAdd(_)));
        assert!(matches!(
            shown[1],
            ScoreboardMessage::DisplayObjective {
                slot: DisplaySlot::Sidebar,
                ..
            }
        ));
        assert!(matches!(
            shown[2],
            ScoreboardMessage::Score { value: 3, .. }
        ));
        let again = board.set_score("a", "kills", 3, None).unwrap();
        assert!(again.is_empty());
        let changed = board.set_score("a", "kills", 4, None).unwrap();
        assert_eq!(changed.len(), 1);
    }

    #[test]
    fn a_new_score_of_zero_still_reaches_clients() {
        let mut board = board();
        board.set_display(DisplaySlot::List, Some("kills")).unwrap();
        let created = board.set_score("a", "kills", 0, None).unwrap();
        assert_eq!(created.len(), 1);
        assert_eq!(board.score("a", "kills").map(|s| s.value), Some(0));
    }

    #[test]
    fn moving_an_objective_between_slots_follows_the_tracking_rules() {
        let mut board = board();
        board
            .set_display(DisplaySlot::Sidebar, Some("kills"))
            .unwrap();
        let second = board.set_display(DisplaySlot::List, Some("kills")).unwrap();
        assert_eq!(
            second,
            vec![ScoreboardMessage::DisplayObjective {
                slot: DisplaySlot::List,
                objective: Some("kills".to_owned())
            }]
        );
        let cleared = board.set_display(DisplaySlot::Sidebar, None).unwrap();
        assert_eq!(
            cleared,
            vec![ScoreboardMessage::DisplayObjective {
                slot: DisplaySlot::Sidebar,
                objective: None
            }]
        );
        let last = board.set_display(DisplaySlot::List, None).unwrap();
        assert_eq!(
            last,
            vec![ScoreboardMessage::ObjectiveRemove("kills".to_owned())]
        );
    }

    #[test]
    fn removing_an_objective_clears_slots_and_scores() {
        let mut board = board();
        board.set_score("a", "kills", 1, None).unwrap();
        board
            .set_display(DisplaySlot::Sidebar, Some("kills"))
            .unwrap();
        let removed = board.remove_objective("kills");
        assert_eq!(
            removed,
            vec![ScoreboardMessage::ObjectiveRemove("kills".to_owned())]
        );
        assert!(board.objective("kills").is_none());
        assert!(board.holders().is_empty());
        assert_eq!(board.display(DisplaySlot::Sidebar), None);
    }

    #[test]
    fn unknown_objectives_are_errors() {
        let mut board = board();
        assert_eq!(
            board.set_score("a", "nope", 1, None),
            Err(ScoreboardError::UnknownObjective("nope".to_owned()))
        );
        assert!(board.set_display(DisplaySlot::List, Some("nope")).is_err());
    }

    #[test]
    fn add_wraps_like_a_java_int() {
        let mut board = board();
        board.set_score("a", "kills", i32::MAX, None).unwrap();
        let (value, _) = board.add_score("a", "kills", 1, None).unwrap();
        assert_eq!(value, i32::MIN);
    }

    #[test]
    fn scores_start_locked_and_enable_unlocks() {
        let mut board = board();
        board.set_score("a", "kills", 1, None).unwrap();
        assert!(board.score("a", "kills").unwrap().locked);
        board.set_locked("a", "kills", false).unwrap();
        assert!(!board.score("a", "kills").unwrap().locked);
    }

    #[test]
    fn resets_drop_scores_and_only_announce_when_tracked() {
        let mut board = board();
        board.set_score("a", "kills", 1, None).unwrap();
        assert!(board.reset_score("a", "kills").is_empty());
        assert!(board.holders().is_empty());
        board.set_score("a", "kills", 1, None).unwrap();
        let all = board.reset_holder("a");
        assert_eq!(
            all,
            vec![ScoreboardMessage::ResetScore {
                holder: "a".to_owned(),
                objective: None
            }]
        );
        assert!(board.reset_holder("a").is_empty());
    }

    #[test]
    fn auto_update_copies_the_holder_display_into_the_score() {
        let mut board = board();
        board.modify_objective("kills", |o| o.display_auto_update = true);
        board
            .set_score("a", "kills", 1, Some(Component::text("[R] a")))
            .unwrap();
        let display = board.score("a", "kills").unwrap().display.clone();
        assert_eq!(display.map(|c| c.plain_text()), Some("[R] a".to_owned()));
    }

    #[test]
    fn teams_track_members_and_switch_on_join() {
        let mut board = Scoreboard::new();
        board
            .add_team(Team::new("red", Component::text("red")))
            .unwrap();
        board
            .add_team(Team::new("blue", Component::text("blue")))
            .unwrap();
        assert!(
            board
                .add_team(Team::new("red", Component::text("x")))
                .is_err()
        );
        board.join_team("a", "red").unwrap();
        let switched = board.join_team("a", "blue").unwrap();
        assert!(matches!(
            switched[0],
            ScoreboardMessage::Team {
                op: TeamOp::Leave(_),
                ..
            }
        ));
        assert!(matches!(
            switched[1],
            ScoreboardMessage::Team {
                op: TeamOp::Join(_),
                ..
            }
        ));
        assert_eq!(board.team_of("a").map(|t| t.name.as_str()), Some("blue"));
        assert!(board.team_members("red").is_empty());
        assert_eq!(
            board.join_team("a", "green"),
            Err(ScoreboardError::UnknownTeam("green".to_owned()))
        );
        let (count, messages) = board.empty_team("blue");
        assert_eq!(count, 1);
        assert_eq!(messages.len(), 1);
        assert!(board.team_of("a").is_none());
    }

    #[test]
    fn removing_a_team_drops_its_members() {
        let mut board = Scoreboard::new();
        board
            .add_team(Team::new("red", Component::text("red")))
            .unwrap();
        board.join_team("a", "red").unwrap();
        board.modify_team("red", |t| t.color = Some(NamedColor::Red));
        assert_eq!(
            board.team("red").and_then(|t| t.color),
            Some(NamedColor::Red)
        );
        let removed = board.remove_team("red");
        assert_eq!(removed.len(), 1);
        assert!(board.team_of("a").is_none());
        assert!(board.remove_team("red").is_empty());
    }

    #[test]
    fn joining_clients_get_teams_then_every_displayed_objective_once() {
        let mut board = board();
        board
            .add_team(Team::new("red", Component::text("red")))
            .unwrap();
        board.join_team("a", "red").unwrap();
        board.set_score("a", "kills", 2, None).unwrap();
        board
            .set_display(DisplaySlot::Sidebar, Some("kills"))
            .unwrap();
        board.set_display(DisplaySlot::List, Some("kills")).unwrap();
        let messages = board.join_messages();
        assert!(matches!(
            &messages[0],
            ScoreboardMessage::Team { op: TeamOp::Add { members, .. }, .. } if members == &["a"]
        ));
        let objectives = messages
            .iter()
            .filter(|m| matches!(m, ScoreboardMessage::ObjectiveAdd(_)))
            .count();
        let displays = messages
            .iter()
            .filter(|m| matches!(m, ScoreboardMessage::DisplayObjective { .. }))
            .count();
        let scores = messages
            .iter()
            .filter(|m| matches!(m, ScoreboardMessage::Score { .. }))
            .count();
        assert_eq!((objectives, displays, scores), (1, 2, 1));
    }
}

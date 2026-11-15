use std::collections::BTreeMap;
use std::sync::Arc;

use rspigot_domain::EntityId;
use rspigot_domain::player::PlayerId;
use rspigot_domain::scoreboard::{Criteria, Scoreboard, ScoreboardMessage, Visibility};
use rspigot_domain::text::Component;
use rustc_hash::FxHashMap;

use crate::chunks::{ChunkJob, Priority};
use crate::commands::Target;
use crate::events::ClientEvent;
use crate::health::attributes;
use crate::ports::ClientLink;
use crate::server::{Audience, Server};
use crate::stats::StatKind;
use rspigot_domain::attribute::ARMOR;

const SCOREBOARD_DATA: &str = "scoreboard";
const FULL_AIR: i32 = 300;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct RecordedVitals {
    health: Option<i32>,
    food: Option<i32>,
    air: Option<i32>,
    armor: Option<i32>,
    xp: Option<i32>,
    level: Option<i32>,
}

pub(crate) const MAIN_BOARD: u32 = 0;

#[derive(Default)]
pub(crate) struct ScoreboardState {
    pub(crate) board: Scoreboard,
    pub(crate) custom: BTreeMap<u32, Scoreboard>,
    created: u32,
    assigned: FxHashMap<PlayerId, u32>,
    recorded: FxHashMap<PlayerId, RecordedVitals>,
}

impl ScoreboardState {
    pub(crate) fn board_by_id(&self, id: u32) -> Option<&Scoreboard> {
        if id == MAIN_BOARD {
            Some(&self.board)
        } else {
            self.custom.get(&id)
        }
    }

    pub(crate) fn board_by_id_mut(&mut self, id: u32) -> Option<&mut Scoreboard> {
        if id == MAIN_BOARD {
            Some(&mut self.board)
        } else {
            self.custom.get_mut(&id)
        }
    }

    pub(crate) fn board_ids(&self) -> Vec<u32> {
        std::iter::once(MAIN_BOARD)
            .chain(self.custom.keys().copied())
            .collect()
    }

    pub(crate) fn create_board(&mut self) -> u32 {
        self.created += 1;
        self.custom.insert(self.created, Scoreboard::new());
        self.created
    }

    pub(crate) fn board_of(&self, player: PlayerId) -> u32 {
        self.assigned.get(&player).copied().unwrap_or(MAIN_BOARD)
    }
}

pub(crate) fn is_stat_criteria(kind: &str, key: &str) -> bool {
    StatKind::ALL
        .into_iter()
        .find(|candidate| candidate.registry_name() == kind)
        .is_some_and(|candidate| candidate.knows(key))
}

pub(crate) fn parse_criteria(name: &str) -> Option<Criteria> {
    Criteria::parse(name, is_stat_criteria)
}

impl<L: ClientLink> Server<L> {
    pub(crate) fn load_scoreboard(&mut self) {
        if let Some(nbt) = self.world.level.scoreboard.take() {
            self.scoreboard.board = Scoreboard::from_nbt(&nbt);
            self.world.level.scoreboard = Some(nbt);
        }
    }

    pub(crate) fn save_scoreboard(&mut self) {
        self.world.level.scoreboard = Some(self.scoreboard.board.to_nbt());
    }

    pub(crate) fn save_scoreboard_data(&mut self) {
        self.save_scoreboard();
        if let Some(data) = &self.world.level.scoreboard {
            self.workers.submit(
                Priority::BACKGROUND,
                ChunkJob::SaveData {
                    dimension: Arc::clone(&self.world.dimension),
                    name: SCOREBOARD_DATA,
                    data: Arc::new(data.clone()),
                },
            );
        }
    }

    pub(crate) fn send_scoreboard_messages(&mut self, messages: Vec<ScoreboardMessage>) {
        self.send_board_messages(MAIN_BOARD, messages);
    }

    pub(crate) fn send_board_messages(&mut self, board: u32, messages: Vec<ScoreboardMessage>) {
        if messages.is_empty() {
            return;
        }
        let viewers: Vec<PlayerId> = self
            .players
            .keys()
            .copied()
            .filter(|player| self.scoreboard.board_of(*player) == board)
            .collect();
        for viewer in viewers {
            for message in &messages {
                self.push(
                    Audience::One(viewer),
                    ClientEvent::Scoreboard(Box::new(message.clone())),
                );
            }
        }
    }

    pub(crate) fn change_board<R>(
        &mut self,
        board: u32,
        change: impl FnOnce(&mut Scoreboard) -> (R, Vec<ScoreboardMessage>),
    ) -> Option<R> {
        let (result, messages) = change(self.scoreboard.board_by_id_mut(board)?);
        self.send_board_messages(board, messages);
        Some(result)
    }

    pub(crate) fn assign_scoreboard(&mut self, player: PlayerId, board: u32) {
        let current = self.scoreboard.board_of(player);
        if current == board || self.scoreboard.board_by_id(board).is_none() {
            return;
        }
        let leaving = self
            .scoreboard
            .board_by_id(current)
            .map(Scoreboard::leave_messages)
            .unwrap_or_default();
        let joining = self
            .scoreboard
            .board_by_id(board)
            .map(Scoreboard::join_messages)
            .unwrap_or_default();
        if board == MAIN_BOARD {
            self.scoreboard.assigned.remove(&player);
        } else {
            self.scoreboard.assigned.insert(player, board);
        }
        for message in leaving.into_iter().chain(joining) {
            self.push(
                Audience::One(player),
                ClientEvent::Scoreboard(Box::new(message)),
            );
        }
    }

    pub(crate) fn discard_board(&mut self, board: u32) {
        if board == MAIN_BOARD || !self.scoreboard.custom.contains_key(&board) {
            return;
        }
        let viewers: Vec<PlayerId> = self
            .scoreboard
            .assigned
            .iter()
            .filter(|(_, assigned)| **assigned == board)
            .map(|(player, _)| *player)
            .collect();
        for viewer in viewers {
            self.assign_scoreboard(viewer, MAIN_BOARD);
        }
        self.scoreboard.custom.remove(&board);
    }

    pub(crate) fn scoreboard_join_events(&self) -> Vec<ClientEvent> {
        self.scoreboard
            .board
            .join_messages()
            .into_iter()
            .map(|message| ClientEvent::Scoreboard(Box::new(message)))
            .collect()
    }

    pub(crate) fn score_holder_of(&self, target: Target) -> Option<String> {
        match target {
            Target::Player(id) => self.players.get(&id).map(|p| p.profile.name.clone()),
            Target::Entity(id) => self
                .entities
                .store
                .get(id)
                .map(|entity| entity.base.uuid.to_string()),
        }
    }

    pub(crate) fn holder_display_name(&self, holder: &str) -> Option<Component> {
        self.holder_display_in(&self.scoreboard.board, holder)
    }

    fn holder_display_in(&self, board: &Scoreboard, holder: &str) -> Option<Component> {
        let player = self.players.values().find(|p| p.profile.name == holder)?;
        let name = Component::text(player.profile.name.clone());
        Some(match board.team_of(holder) {
            Some(team) => team.format_name(name),
            None => name,
        })
    }

    pub(crate) fn update_criteria(
        &mut self,
        holder: &str,
        criteria: &Criteria,
        change: impl Fn(i32) -> i32,
    ) {
        for id in self.scoreboard.board_ids() {
            let Some(board) = self.scoreboard.board_by_id(id) else {
                continue;
            };
            let objectives = board.objectives_for(criteria);
            if objectives.is_empty() {
                continue;
            }
            let display = self.holder_display_in(board, holder);
            for objective in objectives {
                let Some(board) = self.scoreboard.board_by_id_mut(id) else {
                    continue;
                };
                let current = board.score(holder, &objective).map_or(0, |s| s.value);
                if let Ok(messages) =
                    board.set_score(holder, &objective, change(current), display.clone())
                {
                    self.send_board_messages(id, messages);
                }
            }
        }
    }

    pub(crate) fn increment_criteria(&mut self, holder: &str, criteria: &Criteria) {
        self.update_criteria(holder, criteria, |value| value.wrapping_add(1));
    }

    pub(crate) fn stat_score_added(
        &mut self,
        player: PlayerId,
        kind: StatKind,
        key: &str,
        amount: i32,
    ) {
        let Some(holder) = self.players.get(&player).map(|p| p.profile.name.clone()) else {
            return;
        };
        let criteria = Criteria::Stat {
            kind: kind.registry_name().to_owned(),
            key: key.to_owned(),
        };
        self.update_criteria(&holder, &criteria, |value| value.saturating_add(amount));
    }

    pub(crate) fn stat_score_reset(&mut self, player: PlayerId, kind: StatKind, key: &str) {
        let Some(holder) = self.players.get(&player).map(|p| p.profile.name.clone()) else {
            return;
        };
        let criteria = Criteria::Stat {
            kind: kind.registry_name().to_owned(),
            key: key.to_owned(),
        };
        self.update_criteria(&holder, &criteria, |_| 0);
    }

    pub(crate) fn award_kill_score(
        &mut self,
        killer: PlayerId,
        victim: &str,
        victim_is_player: bool,
    ) {
        let Some(holder) = self.players.get(&killer).map(|p| p.profile.name.clone()) else {
            return;
        };
        self.increment_criteria(&holder, &Criteria::TotalKillCount);
        if victim_is_player {
            self.increment_criteria(&holder, &Criteria::PlayerKillCount);
        }
        for id in self.scoreboard.board_ids() {
            let Some(board) = self.scoreboard.board_by_id(id) else {
                continue;
            };
            let victim_color = board.team_of(victim).and_then(|team| team.color);
            let killer_color = board.team_of(&holder).and_then(|team| team.color);
            if let Some(color) = victim_color {
                self.increment_criteria_on(id, &holder, &Criteria::TeamKill(color));
            }
            if let Some(color) = killer_color {
                self.increment_criteria_on(id, victim, &Criteria::KilledByTeam(color));
            }
        }
    }

    fn increment_criteria_on(&mut self, id: u32, holder: &str, criteria: &Criteria) {
        let Some(board) = self.scoreboard.board_by_id(id) else {
            return;
        };
        let objectives = board.objectives_for(criteria);
        let display = self.holder_display_in(board, holder);
        for objective in objectives {
            let Some(board) = self.scoreboard.board_by_id_mut(id) else {
                continue;
            };
            let current = board.score(holder, &objective).map_or(0, |s| s.value);
            if let Ok(messages) =
                board.set_score(holder, &objective, current.wrapping_add(1), display.clone())
            {
                self.send_board_messages(id, messages);
            }
        }
    }

    pub(crate) fn award_death_score(&mut self, player: PlayerId) {
        if let Some(holder) = self.players.get(&player).map(|p| p.profile.name.clone()) {
            self.increment_criteria(&holder, &Criteria::DeathCount);
        }
    }

    pub(crate) fn award_entity_kill_score(&mut self, killer: PlayerId, victim: EntityId) {
        let Some(holder) = self.score_holder_of(Target::Entity(victim)) else {
            return;
        };
        self.award_kill_score(killer, &holder, false);
    }

    pub(crate) fn tick_scoreboard_criteria(&mut self) {
        let ids: Vec<PlayerId> = self.players.keys().copied().collect();
        for id in ids {
            let Some(player) = self.players.get(&id) else {
                continue;
            };
            let holder = player.profile.name.clone();
            let current = RecordedVitals {
                health: Some(player.vitals.health.ceil() as i32),
                food: Some(player.vitals.food.food),
                air: Some(FULL_AIR),
                armor: Some(attributes(player).get(ARMOR).floor() as i32),
                xp: Some(player.vitals.experience.total),
                level: Some(player.vitals.experience.level),
            };
            let recorded = self.scoreboard.recorded.entry(id).or_default();
            let before = *recorded;
            *recorded = current;
            let updates = [
                (Criteria::Health, before.health, current.health),
                (Criteria::Food, before.food, current.food),
                (Criteria::Air, before.air, current.air),
                (Criteria::Armor, before.armor, current.armor),
                (Criteria::Xp, before.xp, current.xp),
                (Criteria::Level, before.level, current.level),
            ];
            for (criteria, was, now) in updates {
                if let Some(value) = now.filter(|_| was != now) {
                    self.update_criteria(&holder, &criteria, |_| value);
                }
            }
        }
    }

    pub(crate) fn friendly_fire_blocked(&self, attacker: PlayerId, victim: PlayerId) -> bool {
        let board = &self.scoreboard.board;
        let name = |id: &PlayerId| self.players.get(id).map(|p| p.profile.name.as_str());
        let (Some(attacker), Some(victim)) = (name(&attacker), name(&victim)) else {
            return false;
        };
        match (board.team_of(attacker), board.team_of(victim)) {
            (Some(own), Some(other)) => own.name == other.name && !own.friendly_fire,
            _ => false,
        }
    }

    pub(crate) fn send_death_message(&mut self, victim: PlayerId, message: Component) {
        let team = self
            .players
            .get(&victim)
            .and_then(|player| self.scoreboard.board.team_of(&player.profile.name));
        let visibility = team.map(|team| (team.name.clone(), team.death_message_visibility));
        let recipients: Vec<PlayerId> = self
            .players
            .values()
            .filter(|player| {
                let on_team = |name: &str| {
                    self.scoreboard
                        .board
                        .team_of(&player.profile.name)
                        .is_some_and(|team| team.name == name)
                };
                match &visibility {
                    None | Some((_, Visibility::Always)) => true,
                    Some((_, Visibility::Never)) => false,
                    Some((team, Visibility::HideForOtherTeams)) => on_team(team),
                    Some((team, Visibility::HideForOwnTeam)) => !on_team(team),
                }
            })
            .map(|player| player.id)
            .collect();
        for recipient in recipients {
            self.push(
                Audience::One(recipient),
                ClientEvent::system(message.clone()),
            );
        }
    }

    pub(crate) fn forget_scoreboard_player(&mut self, id: PlayerId) {
        self.scoreboard.recorded.remove(&id);
        self.scoreboard.assigned.remove(&id);
    }
}

#[cfg(test)]
mod tests {
    use rspigot_domain::player::{ClientSettings, GameProfile};
    use rspigot_domain::scoreboard::{DisplaySlot, Objective, Team, TeamOp};
    use rspigot_domain::text::NamedColor;

    use super::*;
    use crate::config::{GameConfig, LevelData};
    use crate::end_tests::{Tap, arrive};
    use crate::events::Inbound;
    use crate::status::StatusBoard;
    use crate::world_state::tests::Quiet;

    fn messages(link: &Tap) -> Vec<ScoreboardMessage> {
        link.take()
            .into_iter()
            .filter_map(|event| match event {
                ClientEvent::Scoreboard(message) => Some(*message),
                _ => None,
            })
            .collect()
    }

    fn objective(server: &mut Server<Tap>, name: &str, criteria: Criteria) {
        let objective = Objective::new(name, criteria, Component::text(name));
        server.scoreboard.board.add_objective(objective).unwrap();
    }

    fn show(server: &mut Server<Tap>, slot: DisplaySlot, name: &str) {
        let sent = server
            .scoreboard
            .board
            .set_display(slot, Some(name))
            .unwrap();
        server.send_scoreboard_messages(sent);
        server.deliver();
    }

    fn score(server: &Server<Tap>, holder: &str, name: &str) -> Option<i32> {
        server.scoreboard.board.score(holder, name).map(|s| s.value)
    }

    #[test]
    fn changes_reach_players_while_the_objective_is_displayed() {
        let (mut server, _, link) = arrive();
        objective(&mut server, "kills", Criteria::Dummy);
        show(&mut server, DisplaySlot::Sidebar, "kills");
        let shown = messages(&link);
        assert!(matches!(shown[0], ScoreboardMessage::ObjectiveAdd(_)));
        assert!(matches!(
            shown[1],
            ScoreboardMessage::DisplayObjective { .. }
        ));
        let sent = server
            .scoreboard
            .board
            .set_score("a", "kills", 2, None)
            .unwrap();
        server.send_scoreboard_messages(sent);
        server.deliver();
        assert!(matches!(
            messages(&link)[0],
            ScoreboardMessage::Score { value: 2, .. }
        ));
    }

    #[test]
    fn a_joining_player_receives_the_teams_and_displayed_objectives() {
        let (mut server, _, _) = arrive();
        objective(&mut server, "kills", Criteria::Dummy);
        let sent = server
            .scoreboard
            .board
            .add_team(Team::new("red", Component::text("red")))
            .unwrap();
        server.send_scoreboard_messages(sent);
        server
            .scoreboard
            .board
            .set_score("a", "kills", 5, None)
            .unwrap();
        show(&mut server, DisplaySlot::List, "kills");
        let link = Tap::default();
        server.handle(Inbound::Join {
            profile: GameProfile {
                id: PlayerId::offline("Bob"),
                name: "Bob".to_owned(),
                properties: Vec::new(),
            },
            settings: ClientSettings::default(),
            link: link.clone(),
            address: [127, 0, 0, 2].into(),
            session: 2,
        });
        server.deliver();
        let received = messages(&link);
        assert!(matches!(
            &received[0],
            ScoreboardMessage::Team {
                op: TeamOp::Add { .. },
                ..
            }
        ));
        assert!(
            received
                .iter()
                .any(|m| matches!(m, ScoreboardMessage::ObjectiveAdd(_)))
        );
        assert!(
            received
                .iter()
                .any(|m| matches!(m, ScoreboardMessage::Score { value: 5, .. }))
        );
    }

    #[test]
    fn vitals_criteria_follow_health_food_level_and_experience() {
        let (mut server, id, _) = arrive();
        for (name, criteria) in [
            ("hp", Criteria::Health),
            ("food", Criteria::Food),
            ("lvl", Criteria::Level),
            ("xp", Criteria::Xp),
        ] {
            objective(&mut server, name, criteria);
        }
        let player = server.players.get_mut(&id).unwrap();
        player.vitals.health = 7.5;
        player.vitals.food.food = 11;
        player.vitals.experience.level = 4;
        player.vitals.experience.total = 37;
        server.tick_scoreboard_criteria();
        assert_eq!(score(&server, "Alice", "hp"), Some(8));
        assert_eq!(score(&server, "Alice", "food"), Some(11));
        assert_eq!(score(&server, "Alice", "lvl"), Some(4));
        assert_eq!(score(&server, "Alice", "xp"), Some(37));
        server.players.get_mut(&id).unwrap().vitals.health = 3.0;
        server.tick_scoreboard_criteria();
        assert_eq!(score(&server, "Alice", "hp"), Some(3));
    }

    #[test]
    fn deaths_and_kills_increment_their_criteria() {
        let (mut server, id, _) = arrive();
        objective(&mut server, "deaths", Criteria::DeathCount);
        objective(&mut server, "all", Criteria::TotalKillCount);
        objective(&mut server, "players", Criteria::PlayerKillCount);
        objective(&mut server, "vs_blue", Criteria::TeamKill(NamedColor::Blue));
        objective(
            &mut server,
            "by_red",
            Criteria::KilledByTeam(NamedColor::Red),
        );
        for (name, color) in [("red", NamedColor::Red), ("blue", NamedColor::Blue)] {
            let mut team = Team::new(name, Component::text(name));
            team.color = Some(color);
            server.scoreboard.board.add_team(team).unwrap();
        }
        server.scoreboard.board.join_team("Alice", "red").unwrap();
        server.scoreboard.board.join_team("Bob", "blue").unwrap();
        server.award_death_score(id);
        server.award_kill_score(id, "Bob", true);
        assert_eq!(score(&server, "Alice", "deaths"), Some(1));
        assert_eq!(score(&server, "Alice", "all"), Some(1));
        assert_eq!(score(&server, "Alice", "players"), Some(1));
        assert_eq!(score(&server, "Alice", "vs_blue"), Some(1));
        assert_eq!(score(&server, "Bob", "by_red"), Some(1));
        server.award_kill_score(id, "zombie", false);
        assert_eq!(score(&server, "Alice", "all"), Some(2));
        assert_eq!(score(&server, "Alice", "players"), Some(1));
    }

    fn two_teams(server: &mut Server<Tap>) {
        for name in ["red", "blue"] {
            server
                .scoreboard
                .board
                .add_team(Team::new(name, Component::text(name)))
                .unwrap();
        }
    }

    #[test]
    fn friendly_fire_only_blocks_teammates_when_disabled() {
        let (mut server, id, _) = arrive();
        let bob = PlayerId::offline("Bob");
        server.handle(Inbound::Join {
            profile: GameProfile {
                id: bob,
                name: "Bob".to_owned(),
                properties: Vec::new(),
            },
            settings: ClientSettings::default(),
            link: Tap::default(),
            address: [127, 0, 0, 3].into(),
            session: 3,
        });
        two_teams(&mut server);
        server.scoreboard.board.join_team("Alice", "red").unwrap();
        server.scoreboard.board.join_team("Bob", "red").unwrap();
        assert!(!server.friendly_fire_blocked(id, bob));
        server
            .scoreboard
            .board
            .modify_team("red", |team| team.friendly_fire = false);
        assert!(server.friendly_fire_blocked(id, bob));
        server.scoreboard.board.join_team("Bob", "blue").unwrap();
        assert!(!server.friendly_fire_blocked(id, bob));
    }

    #[test]
    fn death_messages_follow_the_team_visibility() {
        let (mut server, id, link) = arrive();
        two_teams(&mut server);
        server.scoreboard.board.join_team("Alice", "red").unwrap();
        link.take();
        for (visibility, heard) in [
            (Visibility::Always, true),
            (Visibility::Never, false),
            (Visibility::HideForOtherTeams, true),
            (Visibility::HideForOwnTeam, false),
        ] {
            server
                .scoreboard
                .board
                .modify_team("red", |team| team.death_message_visibility = visibility);
            server.send_death_message(id, Component::text("Alice died"));
            server.deliver();
            let received = link
                .take()
                .into_iter()
                .any(|event| matches!(event, ClientEvent::SystemChat { .. }));
            assert_eq!(received, heard, "{visibility:?}");
        }
    }

    #[test]
    fn statistic_criteria_count_awarded_stats() {
        let (mut server, id, _) = arrive();
        let criteria = parse_criteria("minecraft.custom:minecraft.jump").expect("jump");
        objective(&mut server, "jumps", criteria);
        server.award_custom(id, crate::stats::custom::JUMP, 3);
        assert_eq!(score(&server, "Alice", "jumps"), Some(3));
        assert!(parse_criteria("minecraft.custom:minecraft.nothing").is_none());
        assert!(parse_criteria("minecraft.mined:minecraft.stone").is_some());
    }

    #[test]
    fn the_scoreboard_survives_a_restart_through_the_level() {
        let (mut server, _, _) = arrive();
        objective(&mut server, "kills", Criteria::Dummy);
        server
            .scoreboard
            .board
            .set_score("a", "kills", 9, None)
            .unwrap();
        server
            .scoreboard
            .board
            .add_team(Team::new("red", Component::text("red")))
            .unwrap();
        server.save_scoreboard();
        let level = LevelData {
            scoreboard: server.world.level.scoreboard.clone(),
            ..LevelData::default()
        };
        let restored = Server::<Tap>::new(
            GameConfig {
                chunk_workers: 1,
                spawn_chunk_radius: 0,
                ..GameConfig::default()
            },
            level,
            std::sync::Arc::new(Quiet),
            std::sync::Arc::new(Quiet),
            StatusBoard::new(1),
        );
        assert_eq!(score(&restored, "a", "kills"), Some(9));
        assert!(restored.scoreboard.board.team("red").is_some());
    }
}

use std::collections::HashMap;

use thiserror::Error;

use super::capture::Capture;
use super::flight::Flight;
use super::intent::Intent;
use crate::arena::{Layout, Point};
use crate::domain::{
    Phase, PlayerId, Round, RoundError, RoundEvent, RoundRules, SheepKind, TeamId,
};
use crate::hud::{message, team_name, team_prefix};
use crate::sheep::{announces_launch, display_name};

pub const RESET_DELAY_TICKS: u32 = 20 * 10;
pub const RIDER_GRACE_TICKS: u32 = 20;
pub const FORCED_MIN_PLAYERS: usize = 1;
pub const SHEEP_STRATEGY: fn(u32) -> SheepKind = rotation;

const fn rotation(index: u32) -> SheepKind {
    SheepKind::ALL[index as usize % SheepKind::ALL.len()]
}

pub fn launch_announcement(kind: SheepKind, team: TeamId) -> Option<Intent> {
    announces_launch(kind).then(|| {
        Intent::Broadcast(format!(
            "§9L'équipe {}{} §9lance un {}§9 !",
            team_prefix(team),
            team_name(team),
            display_name(kind)
        ))
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum ForceStartError {
    #[error("the round has already started")]
    AlreadyStarted,
    #[error("nobody is in the round")]
    NoPlayers,
}

#[derive(Debug, Clone)]
pub struct Session {
    layout: Layout,
    rules: RoundRules,
    round: Round,
    reset_in: Option<u32>,
    shields: HashMap<PlayerId, u32>,
}

impl Session {
    pub fn new(layout: Layout, rules: RoundRules) -> Self {
        Self {
            layout,
            rules,
            round: Round::new(rules),
            reset_in: None,
            shields: HashMap::new(),
        }
    }

    pub const fn round(&self) -> &Round {
        &self.round
    }

    pub const fn layout(&self) -> &Layout {
        &self.layout
    }

    pub fn is_playing(&self) -> bool {
        matches!(self.round.phase(), Phase::Playing { .. })
    }

    pub fn standing(&self) -> Vec<(PlayerId, TeamId)> {
        TeamId::ALL
            .into_iter()
            .flat_map(|team| {
                let members = self.round.team(team);
                members
                    .members()
                    .filter(|player| members.lives_of(*player).unwrap_or(0) > 0)
                    .map(move |player| (player, team))
            })
            .collect()
    }

    pub fn join(&mut self, player: PlayerId) -> Vec<Intent> {
        match self.round.join(player) {
            Ok((team, events)) => {
                let mut intents = vec![
                    Intent::Enroll { player, team },
                    Intent::Teleport {
                        player,
                        to: self.layout.lobby(),
                    },
                ];
                intents.extend(self.absorb(&events));
                intents
            }
            Err(RoundError::AlreadyJoined(_)) => vec![Intent::Tell {
                player,
                text: "§cVous êtes déjà dans la partie.".to_owned(),
            }],
            Err(RoundError::AlreadyStarted) => self.sideline(
                player,
                "§eLa partie a déjà commencé, vous regardez en spectateur.",
            ),
            Err(RoundError::Full) => self.sideline(
                player,
                "§cLa partie est complète, vous regardez en spectateur.",
            ),
        }
    }

    pub fn leave(&mut self, player: PlayerId) -> Vec<Intent> {
        let Some(team) = self.round.team_of(player) else {
            return Vec::new();
        };
        let events: Vec<RoundEvent> = self
            .round
            .leave(player)
            .into_iter()
            .filter(|event| *event != RoundEvent::Eliminated(player))
            .collect();
        let mut intents = vec![Intent::Unenroll { player, team }];
        intents.extend(self.absorb(&events));
        intents
    }

    pub fn quit(&mut self, player: PlayerId) -> Vec<Intent> {
        if self.round.team_of(player).is_none() {
            return vec![Intent::Tell {
                player,
                text: "§cVous n'êtes pas dans la partie.".to_owned(),
            }];
        }
        let mut intents = self.leave(player);
        intents.extend(self.sideline(player, "§eVous avez quitté la partie."));
        intents
    }

    pub fn die(&mut self, player: PlayerId) -> Vec<Intent> {
        let events = self.round.die(player);
        self.absorb(&events)
    }

    pub fn tick(&mut self) -> Vec<Intent> {
        self.shields.retain(|_, remaining| {
            *remaining -= 1;
            *remaining > 0
        });
        let events = self.round.tick();
        let mut intents = self.absorb(&events);
        if let Some(remaining) = self.reset_in {
            if remaining <= 1 {
                intents.extend(self.reset("§eUne nouvelle partie se prépare."));
            } else {
                self.reset_in = Some(remaining - 1);
            }
        }
        intents
    }

    pub fn force_start(&mut self) -> Result<Vec<Intent>, ForceStartError> {
        let mut intents = Vec::new();
        match self.round.phase() {
            Phase::Waiting => {
                if self.round.player_count() == 0 {
                    return Err(ForceStartError::NoPlayers);
                }
                self.round = self.rebuilt_with_forced_minimum();
                intents.push(Intent::SyncTeams);
            }
            Phase::Countdown { .. } => {}
            Phase::Playing { .. } | Phase::Ended { .. } => {
                return Err(ForceStartError::AlreadyStarted);
            }
        }
        let mut events = Vec::new();
        while matches!(self.round.phase(), Phase::Countdown { .. }) {
            events.extend(self.round.tick());
        }
        intents.extend(self.absorb(&events));
        Ok(intents)
    }

    pub fn stop(&mut self) -> Vec<Intent> {
        self.reset("§cLa partie a été arrêtée.")
    }

    pub fn respawn_point(&self, player: PlayerId) -> Point {
        let alive = self
            .round
            .team_of(player)
            .filter(|team| self.round.team(*team).lives_of(player).unwrap_or(0) > 0);
        match (self.round.phase(), alive) {
            (Phase::Playing { .. }, Some(team)) => self.layout.spawn_of(team),
            _ => self.layout.lobby(),
        }
    }

    pub fn blocks_damage(&self) -> bool {
        !self.is_playing()
    }

    pub fn shield(&mut self, player: PlayerId) {
        self.shields.insert(player, RIDER_GRACE_TICKS);
    }

    pub fn is_shielded(&self, player: PlayerId) -> bool {
        self.shields.contains_key(&player)
    }

    pub fn blocks_damage_to(&self, player: PlayerId, shieldable: bool) -> bool {
        self.blocks_damage() || (shieldable && self.is_shielded(player))
    }

    pub fn capture(&self, attacker: PlayerId, flight: &Flight) -> Capture {
        let standing = self
            .standing()
            .iter()
            .any(|(member, _)| *member == attacker);
        if !self.is_playing() || !standing || flight.is_detonating() {
            Capture::Denied
        } else if flight.fragment {
            Capture::Fragment
        } else {
            Capture::Taken { kind: flight.kind }
        }
    }

    pub fn blocks_hit(&self, attacker: PlayerId, victim: PlayerId) -> bool {
        if !self.is_playing() {
            return true;
        }
        match (self.round.team_of(attacker), self.round.team_of(victim)) {
            (Some(a), Some(b)) => a == b,
            _ => false,
        }
    }

    pub fn void_verdict(&self, player: PlayerId, y: f64) -> VoidVerdict {
        if !self.layout.is_void_fall(y) {
            return VoidVerdict::Safe;
        }
        let fighting = self.is_playing() && self.round.team_of(player).is_some();
        if fighting {
            VoidVerdict::Kill
        } else {
            VoidVerdict::ToLobby
        }
    }

    fn rebuilt_with_forced_minimum(&self) -> Round {
        let mut forced = Round::new(RoundRules {
            min_players: FORCED_MIN_PLAYERS,
            ..self.rules
        });
        for team in TeamId::ALL {
            for player in self.round.team(team).members() {
                let _ = forced.join(player);
            }
        }
        forced
    }

    fn reset(&mut self, text: &str) -> Vec<Intent> {
        self.round = Round::new(self.rules);
        self.reset_in = None;
        self.shields.clear();
        vec![Intent::Broadcast(text.to_owned()), Intent::Reset]
    }

    fn sideline(&self, player: PlayerId, text: &str) -> Vec<Intent> {
        vec![
            Intent::Tell {
                player,
                text: text.to_owned(),
            },
            Intent::Spectate { player },
            Intent::Teleport {
                player,
                to: self.layout.lobby(),
            },
        ]
    }

    fn absorb(&mut self, events: &[RoundEvent]) -> Vec<Intent> {
        let mut intents = Vec::new();
        for event in events {
            if matches!(event, RoundEvent::Won(_) | RoundEvent::Draw) {
                self.reset_in = Some(RESET_DELAY_TICKS);
            }
            intents.extend(self.intents_for(event));
        }
        intents
    }

    fn intents_for(&self, event: &RoundEvent) -> Vec<Intent> {
        let mut intents = Vec::new();
        match *event {
            RoundEvent::Respawn(player) => {
                if let Some(text) = message(event) {
                    intents.push(Intent::Tell { player, text });
                }
                intents.push(Intent::Respawn { player });
                return intents;
            }
            RoundEvent::Eliminated(player) => {
                intents.extend(message(event).map(Intent::Broadcast));
                intents.push(Intent::Respawn { player });
                intents.push(Intent::Spectate { player });
                return intents;
            }
            _ => intents.extend(message(event).map(Intent::Broadcast)),
        }
        match *event {
            RoundEvent::Started => {
                for team in TeamId::ALL {
                    for player in self.round.team(team).members() {
                        intents.push(Intent::ClearInventory { player });
                        intents.push(Intent::Teleport {
                            player,
                            to: self.layout.spawn_of(team),
                        });
                    }
                }
            }
            RoundEvent::SheepDue(team) => intents.extend(self.sheep_for(team)),
            RoundEvent::Won(team) => intents.push(Intent::Title {
                text: "§6Victoire !".to_owned(),
                subtitle: format!("Équipe {}{}", team_prefix(team), team_name(team)),
            }),
            RoundEvent::Draw => intents.push(Intent::Title {
                text: "§6Égalité".to_owned(),
                subtitle: "Personne ne gagne".to_owned(),
            }),
            _ => {}
        }
        intents
    }

    fn sheep_for(&self, team: TeamId) -> Vec<Intent> {
        let Phase::Playing { elapsed } = self.round.phase() else {
            return Vec::new();
        };
        let interval = self.round.rules().sheep_interval_ticks.max(1);
        let kind = SHEEP_STRATEGY((elapsed / interval).saturating_sub(1));
        let members = self.round.team(team);
        members
            .members()
            .filter(|player| members.lives_of(*player).unwrap_or(0) > 0)
            .map(|player| Intent::Give { player, kind })
            .collect()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VoidVerdict {
    Safe,
    Kill,
    ToLobby,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::arena::ArenaSettings;
    use crate::domain::RespawnRule;
    use crate::game::flight::Stage;

    const A: PlayerId = PlayerId(1);
    const B: PlayerId = PlayerId(2);
    const C: PlayerId = PlayerId(3);

    fn rules() -> RoundRules {
        RoundRules {
            min_players: 2,
            max_players: 4,
            countdown_ticks: 3,
            sheep_interval_ticks: 5,
            respawn: RespawnRule::Lives(2),
        }
    }

    fn session() -> Session {
        Session::new(Layout::new(ArenaSettings::default()), rules())
    }

    fn started() -> Session {
        let mut session = session();
        session.join(A);
        session.join(B);
        for _ in 0..3 {
            session.tick();
        }
        assert!(session.is_playing());
        session
    }

    fn gives(intents: &[Intent]) -> Vec<(PlayerId, SheepKind)> {
        intents
            .iter()
            .filter_map(|intent| match intent {
                Intent::Give { player, kind } => Some((*player, *kind)),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn first_join_enrolls_and_sends_to_the_lobby() {
        let mut session = session();
        let intents = session.join(A);
        assert_eq!(
            intents,
            vec![
                Intent::Enroll {
                    player: A,
                    team: TeamId::Red
                },
                Intent::Teleport {
                    player: A,
                    to: session.layout().lobby()
                },
            ]
        );
    }

    #[test]
    fn second_join_starts_the_countdown() {
        let mut session = session();
        session.join(A);
        let intents = session.join(B);
        assert!(intents.contains(&Intent::Broadcast(
            message(&RoundEvent::CountdownStarted).expect("text")
        )));
        assert!(matches!(
            session.round().phase(),
            Phase::Countdown { remaining: 3 }
        ));
    }

    #[test]
    fn players_are_split_between_the_teams() {
        let mut session = session();
        session.join(A);
        session.join(B);
        session.join(C);
        assert_eq!(session.round().team_of(A), Some(TeamId::Red));
        assert_eq!(session.round().team_of(B), Some(TeamId::Blue));
        assert_eq!(session.round().team_of(C), Some(TeamId::Red));
    }

    #[test]
    fn joining_twice_only_tells_the_player() {
        let mut session = session();
        session.join(A);
        let intents = session.join(A);
        assert!(matches!(intents.as_slice(), [Intent::Tell { player, .. }] if *player == A));
    }

    #[test]
    fn start_teleports_each_team_to_its_spawn_and_clears_inventories() {
        let mut session = session();
        session.join(A);
        session.join(B);
        session.tick();
        session.tick();
        let intents = session.tick();
        let spawn = |team| session.layout().spawn_of(team);
        assert!(intents.contains(&Intent::Teleport {
            player: A,
            to: spawn(TeamId::Red)
        }));
        assert!(intents.contains(&Intent::Teleport {
            player: B,
            to: spawn(TeamId::Blue)
        }));
        assert!(intents.contains(&Intent::ClearInventory { player: A }));
        assert!(intents.contains(&Intent::ClearInventory { player: B }));
    }

    #[test]
    fn no_sheep_before_the_round_is_playing() {
        let mut session = session();
        session.join(A);
        session.join(B);
        for _ in 0..2 {
            assert!(gives(&session.tick()).is_empty());
        }
    }

    #[test]
    fn sheep_due_gives_one_item_to_every_standing_member() {
        let mut session = started();
        let mut given = Vec::new();
        for _ in 0..5 {
            given.extend(gives(&session.tick()));
        }
        assert_eq!(given.len(), 2);
        assert!(given.contains(&(A, SHEEP_STRATEGY(0))));
        assert!(given.contains(&(B, SHEEP_STRATEGY(0))));
    }

    #[test]
    fn sheep_kind_rotates_over_the_catalog() {
        let mut session = started();
        let mut kinds = Vec::new();
        for _ in 0..10 {
            kinds.extend(gives(&session.tick()).into_iter().map(|(_, kind)| kind));
        }
        assert_eq!(kinds[0], SheepKind::ALL[0]);
        assert_eq!(kinds[2], SheepKind::ALL[1]);
    }

    #[test]
    fn only_the_intergalactic_launch_is_announced_in_french() {
        for kind in SheepKind::ALL {
            let announcement = launch_announcement(kind, TeamId::Red);
            if kind == SheepKind::Intergalactic {
                assert!(matches!(
                    announcement,
                    Some(Intent::Broadcast(text)) if text.contains("intergalactique") && text.contains("lance")
                ));
            } else {
                assert_eq!(announcement, None);
            }
        }
    }

    #[test]
    fn rotation_wraps_around() {
        let len = u32::try_from(SheepKind::ALL.len()).expect("small");
        assert_eq!(SHEEP_STRATEGY(len), SHEEP_STRATEGY(0));
    }

    #[test]
    fn eliminated_players_get_no_sheep() {
        let mut session = session();
        session.join(A);
        session.join(B);
        session.join(C);
        for _ in 0..3 {
            session.tick();
        }
        session.die(C);
        session.die(C);
        let mut given = Vec::new();
        for _ in 0..5 {
            given.extend(gives(&session.tick()));
        }
        assert!(given.iter().all(|(player, _)| *player != C));
        assert_eq!(given.len(), 2);
    }

    #[test]
    fn death_with_lives_left_respawns() {
        let mut session = started();
        let intents = session.die(A);
        assert!(intents.contains(&Intent::Respawn { player: A }));
        assert!(!intents.contains(&Intent::Spectate { player: A }));
        assert_eq!(
            session.respawn_point(A),
            session.layout().spawn_of(TeamId::Red)
        );
    }

    #[test]
    fn last_life_eliminates_and_spectates() {
        let mut session = started();
        session.die(A);
        let intents = session.die(A);
        assert!(intents.contains(&Intent::Spectate { player: A }));
        assert_eq!(session.respawn_point(A), session.layout().lobby());
    }

    #[test]
    fn win_shows_a_title_then_resets_after_the_delay() {
        let mut session = started();
        session.die(A);
        let intents = session.die(A);
        assert!(intents.iter().any(|intent| matches!(
            intent,
            Intent::Title { text, .. } if text.contains("Victoire")
        )));
        assert!(matches!(
            session.round().phase(),
            Phase::Ended {
                winner: Some(TeamId::Blue)
            }
        ));
        for _ in 0..RESET_DELAY_TICKS - 1 {
            assert!(!session.tick().contains(&Intent::Reset));
        }
        assert!(session.tick().contains(&Intent::Reset));
        assert_eq!(session.round().phase(), Phase::Waiting);
        assert_eq!(session.round().player_count(), 0);
    }

    #[test]
    fn rejoining_after_the_reset_works() {
        let mut session = started();
        session.die(A);
        session.die(A);
        for _ in 0..RESET_DELAY_TICKS {
            session.tick();
        }
        session.join(A);
        session.join(B);
        assert!(matches!(session.round().phase(), Phase::Countdown { .. }));
    }

    #[test]
    fn leaving_removes_the_player_from_the_round() {
        let mut session = session();
        session.join(A);
        let intents = session.leave(A);
        assert_eq!(
            intents,
            vec![Intent::Unenroll {
                player: A,
                team: TeamId::Red
            }]
        );
        assert_eq!(session.round().team_of(A), None);
    }

    #[test]
    fn leaving_mid_round_does_not_spectate_the_leaver_but_can_end_the_round() {
        let mut session = started();
        let intents = session.leave(A);
        assert!(!intents.contains(&Intent::Spectate { player: A }));
        assert!(!intents.contains(&Intent::Respawn { player: A }));
        assert!(!session.is_playing());
    }

    #[test]
    fn leaving_when_not_in_the_round_does_nothing() {
        assert!(session().leave(A).is_empty());
    }

    #[test]
    fn quit_sends_the_player_to_the_lobby_as_a_spectator() {
        let mut session = session();
        session.join(A);
        let intents = session.quit(A);
        assert!(intents.contains(&Intent::Spectate { player: A }));
        assert_eq!(session.round().team_of(A), None);
    }

    #[test]
    fn joining_mid_round_makes_a_spectator() {
        let mut session = started();
        let intents = session.join(C);
        assert!(intents.contains(&Intent::Spectate { player: C }));
        assert_eq!(session.round().team_of(C), None);
    }

    #[test]
    fn force_start_skips_the_countdown() {
        let mut session = session();
        session.join(A);
        session.join(B);
        let intents = session.force_start().expect("starts");
        assert!(session.is_playing());
        assert!(intents.contains(&Intent::ClearInventory { player: A }));
    }

    #[test]
    fn force_start_works_with_a_single_player() {
        let mut session = session();
        session.join(A);
        session.force_start().expect("starts");
        assert!(session.is_playing());
    }

    #[test]
    fn force_start_refuses_an_empty_or_started_round() {
        assert_eq!(session().force_start(), Err(ForceStartError::NoPlayers));
        assert_eq!(
            started().force_start(),
            Err(ForceStartError::AlreadyStarted)
        );
    }

    #[test]
    fn stop_resets_the_round() {
        let mut session = started();
        let intents = session.stop();
        assert!(intents.contains(&Intent::Reset));
        assert_eq!(session.round().phase(), Phase::Waiting);
    }

    #[test]
    fn damage_is_blocked_outside_the_playing_phase() {
        assert!(session().blocks_damage());
        assert!(!started().blocks_damage());
    }

    fn live(kind: SheepKind) -> Flight {
        Flight::new(kind, TeamId::Red, A)
    }

    fn eliminated() -> Session {
        let mut session = started();
        let victim = session
            .standing()
            .into_iter()
            .map(|(player, _)| player)
            .next()
            .unwrap();
        session.die(victim);
        session.die(victim);
        assert!(!session.standing().iter().any(|(p, _)| *p == victim));
        session
    }

    fn standing_of(session: &Session, team: TeamId) -> PlayerId {
        session
            .standing()
            .into_iter()
            .find(|(_, member)| *member == team)
            .unwrap()
            .0
    }

    #[test]
    fn a_standing_player_of_either_team_captures_and_gets_the_kind() {
        let session = started();
        let flight = live(SheepKind::Frozen);
        for team in TeamId::ALL {
            assert_eq!(
                session.capture(standing_of(&session, team), &flight),
                Capture::Taken {
                    kind: SheepKind::Frozen
                }
            );
        }
    }

    #[test]
    fn the_thrower_can_capture_their_own_sheep() {
        let session = started();
        let thrower = standing_of(&session, TeamId::Red);
        let flight = Flight::new(SheepKind::Healing, TeamId::Red, thrower);
        assert!(matches!(
            session.capture(thrower, &flight),
            Capture::Taken { .. }
        ));
    }

    #[test]
    fn an_eliminated_player_cannot_capture() {
        let session = eliminated();
        let victim = [A, B]
            .into_iter()
            .find(|p| !session.standing().iter().any(|(s, _)| s == p))
            .unwrap();
        assert_eq!(
            session.capture(victim, &live(SheepKind::Frozen)),
            Capture::Denied
        );
    }

    #[test]
    fn a_spectator_or_outsider_cannot_capture() {
        let mut session = started();
        session.join(C);
        assert_eq!(
            session.capture(C, &live(SheepKind::Frozen)),
            Capture::Denied
        );
        assert_eq!(
            session.capture(PlayerId(99), &live(SheepKind::Frozen)),
            Capture::Denied
        );
    }

    #[test]
    fn nobody_captures_outside_the_playing_phase() {
        let session = session();
        assert_eq!(
            session.capture(A, &live(SheepKind::Frozen)),
            Capture::Denied
        );
    }

    #[test]
    fn a_splitting_fragment_is_destroyed_without_wool() {
        let session = started();
        let child = Flight::fragment(SheepKind::Seeker, TeamId::Red, A);
        assert_eq!(session.capture(A, &child), Capture::Fragment);
    }

    #[test]
    fn a_landed_sheep_with_a_running_fuse_can_be_captured() {
        let session = started();
        let mut flight = live(SheepKind::Frozen);
        flight.stage = Stage::Landed {
            ticks: flight.fuse - 1,
        };
        assert!(matches!(session.capture(A, &flight), Capture::Taken { .. }));
    }

    #[test]
    fn a_detonating_sheep_cannot_be_captured() {
        let session = started();
        let mut flight = live(SheepKind::Frozen);
        flight.stage = Stage::Landed { ticks: flight.fuse };
        assert_eq!(session.capture(A, &flight), Capture::Denied);
        let mut boarding = live(SheepKind::Boarding);
        boarding.stage = Stage::Landed { ticks: 0 };
        assert_eq!(session.capture(A, &boarding), Capture::Denied);
    }

    #[test]
    fn an_eligible_hit_is_not_denied_and_a_spectator_hit_is_denied() {
        let session = started();
        assert!(matches!(
            session.capture(A, &live(SheepKind::Frozen)),
            Capture::Taken { .. }
        ));
        assert_eq!(
            session.capture(C, &live(SheepKind::Frozen)),
            Capture::Denied
        );
    }

    #[test]
    fn friendly_fire_is_blocked_but_enemy_hits_pass() {
        let mut session = session();
        session.join(A);
        session.join(B);
        session.join(C);
        for _ in 0..3 {
            session.tick();
        }
        assert!(session.blocks_hit(A, C));
        assert!(!session.blocks_hit(A, B));
    }

    #[test]
    fn void_kills_fighters_and_rescues_everyone_else() {
        let session = started();
        let low = session.layout().settings().void_fall_y - 1.0;
        assert_eq!(session.void_verdict(A, low), VoidVerdict::Kill);
        assert_eq!(session.void_verdict(C, low), VoidVerdict::ToLobby);
        assert_eq!(session.void_verdict(A, 64.0), VoidVerdict::Safe);
    }

    #[test]
    fn a_shield_blocks_shieldable_damage_only() {
        let mut session = started();
        assert!(!session.blocks_damage_to(A, true));
        session.shield(A);
        assert!(session.blocks_damage_to(A, true));
        assert!(!session.blocks_damage_to(A, false));
        assert!(!session.blocks_damage_to(B, true));
    }

    #[test]
    fn a_shield_expires_after_the_grace() {
        let mut session = started();
        session.shield(A);
        for _ in 0..RIDER_GRACE_TICKS - 1 {
            session.tick();
            assert!(session.is_shielded(A));
        }
        session.tick();
        assert!(!session.is_shielded(A));
    }

    #[test]
    fn shielding_again_refreshes_the_grace() {
        let mut session = started();
        session.shield(A);
        for _ in 0..RIDER_GRACE_TICKS - 1 {
            session.tick();
        }
        session.shield(A);
        session.tick();
        assert!(session.is_shielded(A));
    }

    #[test]
    fn a_reset_drops_the_shields() {
        let mut session = started();
        session.shield(A);
        session.stop();
        assert!(!session.is_shielded(A));
    }
}

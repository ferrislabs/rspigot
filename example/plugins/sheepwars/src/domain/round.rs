use super::{PlayerId, RespawnRule, RoundError, RoundRules, Team, TeamId};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    Waiting,
    Countdown { remaining: u32 },
    Playing { elapsed: u32 },
    Ended { winner: Option<TeamId> },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RoundEvent {
    CountdownStarted,
    CountdownCancelled,
    Started,
    SheepDue(TeamId),
    Respawn(PlayerId),
    Eliminated(PlayerId),
    Won(TeamId),
    Draw,
}

#[derive(Debug, Clone)]
pub struct Round {
    rules: RoundRules,
    phase: Phase,
    red: Team,
    blue: Team,
}

impl Round {
    pub fn new(rules: RoundRules) -> Self {
        Self {
            rules,
            phase: Phase::Waiting,
            red: Team::new(TeamId::Red),
            blue: Team::new(TeamId::Blue),
        }
    }

    pub fn rules(&self) -> &RoundRules {
        &self.rules
    }

    pub fn phase(&self) -> Phase {
        self.phase
    }

    pub fn team(&self, id: TeamId) -> &Team {
        match id {
            TeamId::Red => &self.red,
            TeamId::Blue => &self.blue,
        }
    }

    pub fn team_of(&self, player: PlayerId) -> Option<TeamId> {
        TeamId::ALL
            .into_iter()
            .find(|id| self.team(*id).contains(player))
    }

    pub fn player_count(&self) -> usize {
        self.red.len() + self.blue.len()
    }

    pub fn join(&mut self, player: PlayerId) -> Result<(TeamId, Vec<RoundEvent>), RoundError> {
        if !matches!(self.phase, Phase::Waiting | Phase::Countdown { .. }) {
            return Err(RoundError::AlreadyStarted);
        }
        if self.team_of(player).is_some() {
            return Err(RoundError::AlreadyJoined(player));
        }
        if self.player_count() >= self.rules.max_players {
            return Err(RoundError::Full);
        }
        let id = if self.blue.len() < self.red.len() {
            TeamId::Blue
        } else {
            TeamId::Red
        };
        let lives = self.initial_lives();
        self.team_mut(id).add(player, lives);
        let mut events = Vec::new();
        if self.phase == Phase::Waiting && self.player_count() >= self.rules.min_players {
            self.phase = Phase::Countdown {
                remaining: self.rules.countdown_ticks,
            };
            events.push(RoundEvent::CountdownStarted);
        }
        Ok((id, events))
    }

    pub fn leave(&mut self, player: PlayerId) -> Vec<RoundEvent> {
        let Some(id) = self.team_of(player) else {
            return Vec::new();
        };
        match self.phase {
            Phase::Waiting | Phase::Countdown { .. } => {
                self.team_mut(id).remove(player);
                if matches!(self.phase, Phase::Countdown { .. })
                    && self.player_count() < self.rules.min_players
                {
                    self.phase = Phase::Waiting;
                    return vec![RoundEvent::CountdownCancelled];
                }
                Vec::new()
            }
            Phase::Playing { .. } => {
                self.team_mut(id).remove(player);
                let mut events = vec![RoundEvent::Eliminated(player)];
                events.extend(self.settle());
                events
            }
            Phase::Ended { .. } => {
                self.team_mut(id).remove(player);
                Vec::new()
            }
        }
    }

    pub fn die(&mut self, player: PlayerId) -> Vec<RoundEvent> {
        let Phase::Playing { .. } = self.phase else {
            return Vec::new();
        };
        let Some(id) = self.team_of(player) else {
            return Vec::new();
        };
        let Some(remaining) = self.team_mut(id).lose_life(player) else {
            return Vec::new();
        };
        let mut events = vec![if remaining > 0 {
            RoundEvent::Respawn(player)
        } else {
            RoundEvent::Eliminated(player)
        }];
        events.extend(self.settle());
        events
    }

    pub fn tick(&mut self) -> Vec<RoundEvent> {
        match self.phase {
            Phase::Countdown { remaining } if remaining <= 1 => {
                self.phase = Phase::Playing { elapsed: 0 };
                vec![RoundEvent::Started]
            }
            Phase::Countdown { remaining } => {
                self.phase = Phase::Countdown {
                    remaining: remaining - 1,
                };
                Vec::new()
            }
            Phase::Playing { elapsed } => {
                let elapsed = elapsed + 1;
                self.phase = Phase::Playing { elapsed };
                if self.rules.sheep_interval_ticks > 0
                    && elapsed % self.rules.sheep_interval_ticks == 0
                {
                    TeamId::ALL.into_iter().map(RoundEvent::SheepDue).collect()
                } else {
                    Vec::new()
                }
            }
            Phase::Waiting | Phase::Ended { .. } => Vec::new(),
        }
    }

    fn settle(&mut self) -> Vec<RoundEvent> {
        let red = self.red.standing();
        let blue = self.blue.standing();
        let (winner, event) = match (red, blue) {
            (0, 0) => (None, RoundEvent::Draw),
            (_, 0) => (Some(TeamId::Red), RoundEvent::Won(TeamId::Red)),
            (0, _) => (Some(TeamId::Blue), RoundEvent::Won(TeamId::Blue)),
            _ => return Vec::new(),
        };
        self.phase = Phase::Ended { winner };
        vec![event]
    }

    fn initial_lives(&self) -> u32 {
        match self.rules.respawn {
            RespawnRule::Lives(lives) => lives.max(1),
        }
    }

    fn team_mut(&mut self, id: TeamId) -> &mut Team {
        match id {
            TeamId::Red => &mut self.red,
            TeamId::Blue => &mut self.blue,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rules() -> RoundRules {
        RoundRules {
            min_players: 2,
            max_players: 4,
            countdown_ticks: 3,
            sheep_interval_ticks: 5,
            respawn: RespawnRule::Lives(2),
        }
    }

    fn player(n: u128) -> PlayerId {
        PlayerId(n)
    }

    fn playing() -> Round {
        let mut round = Round::new(rules());
        round.join(player(1)).unwrap();
        round.join(player(2)).unwrap();
        while !matches!(round.phase(), Phase::Playing { .. }) {
            round.tick();
        }
        round
    }

    #[test]
    fn countdown_starts_at_the_minimum_of_players() {
        let mut round = Round::new(rules());
        let (_, first) = round.join(player(1)).unwrap();
        assert!(first.is_empty());
        assert_eq!(round.phase(), Phase::Waiting);
        let (_, second) = round.join(player(2)).unwrap();
        assert_eq!(second, vec![RoundEvent::CountdownStarted]);
        assert_eq!(round.phase(), Phase::Countdown { remaining: 3 });
    }

    #[test]
    fn players_are_split_across_two_balanced_teams() {
        let mut round = Round::new(rules());
        let teams: Vec<_> = (1..=4).map(|n| round.join(player(n)).unwrap().0).collect();
        assert_eq!(
            teams,
            vec![TeamId::Red, TeamId::Blue, TeamId::Red, TeamId::Blue]
        );
    }

    #[test]
    fn a_player_cannot_join_twice_nor_past_the_maximum() {
        let mut round = Round::new(rules());
        round.join(player(1)).unwrap();
        assert_eq!(
            round.join(player(1)),
            Err(RoundError::AlreadyJoined(player(1)))
        );
        round.join(player(2)).unwrap();
        round.join(player(3)).unwrap();
        round.join(player(4)).unwrap();
        assert_eq!(round.join(player(5)), Err(RoundError::Full));
    }

    #[test]
    fn leaving_below_the_minimum_cancels_the_countdown() {
        let mut round = Round::new(rules());
        round.join(player(1)).unwrap();
        round.join(player(2)).unwrap();
        let events = round.leave(player(2));
        assert_eq!(events, vec![RoundEvent::CountdownCancelled]);
        assert_eq!(round.phase(), Phase::Waiting);
    }

    #[test]
    fn the_countdown_ends_in_play_and_closes_the_round_to_new_players() {
        let mut round = Round::new(rules());
        round.join(player(1)).unwrap();
        round.join(player(2)).unwrap();
        assert!(round.tick().is_empty());
        assert!(round.tick().is_empty());
        assert_eq!(round.tick(), vec![RoundEvent::Started]);
        assert_eq!(round.phase(), Phase::Playing { elapsed: 0 });
        assert_eq!(round.join(player(3)), Err(RoundError::AlreadyStarted));
    }

    #[test]
    fn each_team_is_due_a_sheep_at_every_interval_only_while_playing() {
        let mut waiting = Round::new(rules());
        for _ in 0..20 {
            assert!(waiting.tick().is_empty());
        }
        let mut round = playing();
        for _ in 0..4 {
            assert!(round.tick().is_empty());
        }
        assert_eq!(
            round.tick(),
            vec![
                RoundEvent::SheepDue(TeamId::Red),
                RoundEvent::SheepDue(TeamId::Blue)
            ]
        );
    }

    #[test]
    fn a_death_respawns_the_player_while_lives_remain() {
        let mut round = playing();
        assert_eq!(round.die(player(1)), vec![RoundEvent::Respawn(player(1))]);
        assert_eq!(round.team(TeamId::Red).lives_of(player(1)), Some(1));
    }

    #[test]
    fn the_last_team_standing_wins() {
        let mut round = playing();
        round.die(player(1));
        let events = round.die(player(1));
        assert_eq!(
            events,
            vec![
                RoundEvent::Eliminated(player(1)),
                RoundEvent::Won(TeamId::Blue)
            ]
        );
        assert_eq!(
            round.phase(),
            Phase::Ended {
                winner: Some(TeamId::Blue)
            }
        );
    }

    #[test]
    fn a_player_who_disconnects_while_playing_is_removed_from_their_team() {
        let mut round = playing();
        let events = round.leave(player(2));
        assert_eq!(
            events,
            vec![
                RoundEvent::Eliminated(player(2)),
                RoundEvent::Won(TeamId::Red)
            ]
        );
        assert_eq!(round.team_of(player(2)), None);
    }

    #[test]
    fn nothing_happens_after_the_round_ended() {
        let mut round = playing();
        round.leave(player(2));
        assert!(round.tick().is_empty());
        assert!(round.die(player(1)).is_empty());
    }
}

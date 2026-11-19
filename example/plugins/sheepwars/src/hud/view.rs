use rspigot_plugin::TeamColor;

use crate::domain::{Phase, PlayerId, Round, TeamId};

pub const TICKS_PER_SECOND: u32 = 20;
pub const SIDEBAR_TITLE: &str = "§e§lSheepWars";
pub const MAX_LINE_CHARS: usize = 40;

const SECONDS_PER_MINUTE: u32 = 60;

pub const fn team_color(team: TeamId) -> TeamColor {
    match team {
        TeamId::Red => TeamColor::Red,
        TeamId::Blue => TeamColor::Blue,
    }
}

pub const fn team_name(team: TeamId) -> &'static str {
    match team {
        TeamId::Red => "Rouge",
        TeamId::Blue => "Bleue",
    }
}

pub const fn team_prefix(team: TeamId) -> &'static str {
    match team {
        TeamId::Red => "§c",
        TeamId::Blue => "§9",
    }
}

pub fn clock(ticks: u32) -> String {
    let seconds = ticks / TICKS_PER_SECOND;
    format!(
        "{:02}:{:02}",
        seconds / SECONDS_PER_MINUTE,
        seconds % SECONDS_PER_MINUTE
    )
}

fn countdown_seconds(remaining: u32) -> u32 {
    remaining.div_ceil(TICKS_PER_SECOND)
}

fn phase_line(phase: Phase) -> String {
    match phase {
        Phase::Waiting => "§7En attente de joueurs".to_owned(),
        Phase::Countdown { remaining } => {
            format!("§eDébut dans §f{}s", countdown_seconds(remaining))
        }
        Phase::Playing { elapsed } => format!("§aTemps §f{}", clock(elapsed)),
        Phase::Ended { winner: Some(team) } => {
            format!("§6Victoire {}{}", team_prefix(team), team_name(team))
        }
        Phase::Ended { winner: None } => "§6Égalité".to_owned(),
    }
}

pub fn sidebar_lines(round: &Round, viewer: Option<PlayerId>) -> Vec<String> {
    let mut lines = vec![
        "§r".to_owned(),
        phase_line(round.phase()),
        "§r§r".to_owned(),
    ];
    for id in TeamId::ALL {
        lines.push(format!(
            "{}{} §f: {}",
            team_prefix(id),
            team_name(id),
            round.team(id).standing()
        ));
    }
    let lives = viewer.and_then(|player| {
        let team = round.team_of(player)?;
        round.team(team).lives_of(player)
    });
    if let Some(lives) = lives {
        lines.push("§r§r§r".to_owned());
        lines.push(format!("§7Vos vies §f: {lives}"));
    }
    lines
}

pub fn bossbar_title(round: &Round) -> String {
    match round.phase() {
        Phase::Waiting => "§eSheepWars §7- en attente de joueurs".to_owned(),
        Phase::Countdown { remaining } => {
            format!(
                "§eSheepWars §7- début dans §f{}s",
                countdown_seconds(remaining)
            )
        }
        Phase::Playing { elapsed } => format!("§eSheepWars §7- §f{}", clock(elapsed)),
        Phase::Ended { winner: Some(team) } => {
            format!(
                "§6Victoire de l'équipe {}{}",
                team_prefix(team),
                team_name(team)
            )
        }
        Phase::Ended { winner: None } => "§6Égalité".to_owned(),
    }
}

pub fn bossbar_progress(round: &Round) -> f64 {
    let rules = round.rules();
    let ratio = match round.phase() {
        Phase::Waiting => 0.0,
        Phase::Countdown { remaining } => {
            if rules.countdown_ticks == 0 {
                0.0
            } else {
                1.0 - f64::from(remaining) / f64::from(rules.countdown_ticks)
            }
        }
        Phase::Playing { elapsed } => {
            if rules.sheep_interval_ticks == 0 {
                0.0
            } else {
                f64::from(elapsed % rules.sheep_interval_ticks)
                    / f64::from(rules.sheep_interval_ticks)
            }
        }
        Phase::Ended { .. } => 1.0,
    };
    ratio.clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{RespawnRule, RoundRules};

    fn rules() -> RoundRules {
        RoundRules {
            min_players: 2,
            max_players: 4,
            countdown_ticks: 100,
            sheep_interval_ticks: 200,
            respawn: RespawnRule::Lives(2),
        }
    }

    fn player(n: u128) -> PlayerId {
        PlayerId(n)
    }

    fn waiting() -> Round {
        let mut round = Round::new(rules());
        round.join(player(1)).unwrap();
        round
    }

    fn countdown() -> Round {
        let mut round = waiting();
        round.join(player(2)).unwrap();
        round
    }

    fn playing() -> Round {
        let mut round = countdown();
        while !matches!(round.phase(), Phase::Playing { .. }) {
            round.tick();
        }
        round
    }

    fn has(lines: &[String], needle: &str) -> bool {
        lines.iter().any(|line| line.contains(needle))
    }

    #[test]
    fn colors_and_names_follow_the_team() {
        assert_eq!(team_color(TeamId::Red), TeamColor::Red);
        assert_eq!(team_color(TeamId::Blue), TeamColor::Blue);
        assert_eq!(team_name(TeamId::Red), "Rouge");
        assert_eq!(team_name(TeamId::Blue), "Bleue");
    }

    #[test]
    fn the_clock_formats_minutes_and_seconds() {
        assert_eq!(clock(0), "00:00");
        assert_eq!(clock(65 * TICKS_PER_SECOND), "01:05");
        assert_eq!(clock(600 * TICKS_PER_SECOND), "10:00");
    }

    #[test]
    fn waiting_sidebar_shows_both_teams_and_the_waiting_phase() {
        let lines = sidebar_lines(&waiting(), None);
        assert!(has(&lines, "En attente"));
        assert!(has(&lines, "Rouge §f: 1"));
        assert!(has(&lines, "Bleue §f: 0"));
    }

    #[test]
    fn countdown_sidebar_shows_the_seconds_left_rounded_up() {
        let mut round = countdown();
        assert!(has(&sidebar_lines(&round, None), "Début dans §f5s"));
        round.tick();
        assert!(has(&sidebar_lines(&round, None), "Début dans §f5s"));
        for _ in 0..TICKS_PER_SECOND {
            round.tick();
        }
        assert!(has(&sidebar_lines(&round, None), "Début dans §f4s"));
    }

    #[test]
    fn playing_sidebar_shows_the_elapsed_time_and_standing_counts() {
        let mut round = playing();
        for _ in 0..(65 * TICKS_PER_SECOND) {
            round.tick();
        }
        let lines = sidebar_lines(&round, None);
        assert!(has(&lines, "Temps §f01:05"));
        assert!(has(&lines, "Rouge §f: 1"));
        assert!(has(&lines, "Bleue §f: 1"));
    }

    #[test]
    fn standing_counts_drop_when_a_player_is_eliminated() {
        let mut round = playing();
        round.die(player(1));
        round.die(player(1));
        let lines = sidebar_lines(&round, None);
        assert!(has(&lines, "Rouge §f: 0"));
        assert!(has(&lines, "Victoire"));
    }

    #[test]
    fn a_departure_ending_the_round_announces_the_winner() {
        let mut round = playing();
        round.leave(player(1));
        assert!(has(&sidebar_lines(&round, None), "Victoire"));
    }

    #[test]
    fn the_viewer_sees_their_own_lives_only() {
        let mut round = playing();
        round.die(player(1));
        assert!(has(
            &sidebar_lines(&round, Some(player(1))),
            "Vos vies §f: 1"
        ));
        assert!(has(
            &sidebar_lines(&round, Some(player(2))),
            "Vos vies §f: 2"
        ));
        assert!(!has(&sidebar_lines(&round, None), "Vos vies"));
        assert!(!has(&sidebar_lines(&round, Some(player(9))), "Vos vies"));
    }

    #[test]
    fn sidebar_lines_are_unique_and_within_the_limit_in_every_phase() {
        let mut ended = playing();
        ended.leave(player(2));
        for round in [waiting(), countdown(), playing(), ended] {
            let lines = sidebar_lines(&round, Some(player(1)));
            for line in &lines {
                assert!(line.chars().count() <= MAX_LINE_CHARS, "{line}");
            }
            let mut sorted = lines.clone();
            sorted.sort();
            sorted.dedup();
            assert_eq!(sorted.len(), lines.len());
        }
    }

    #[test]
    fn the_bossbar_title_follows_the_phase() {
        assert!(bossbar_title(&waiting()).contains("attente"));
        assert!(bossbar_title(&countdown()).contains("5s"));
        assert!(bossbar_title(&playing()).contains("00:00"));
    }

    #[test]
    fn the_bossbar_progress_stays_between_zero_and_one() {
        assert_eq!(bossbar_progress(&waiting()), 0.0);
        let mut round = countdown();
        assert_eq!(bossbar_progress(&round), 0.0);
        round.tick();
        assert!((bossbar_progress(&round) - 0.01).abs() < 1e-9);
        let mut round = playing();
        for _ in 0..50 {
            round.tick();
        }
        assert!((bossbar_progress(&round) - 0.25).abs() < 1e-9);
        round.leave(player(2));
        assert_eq!(bossbar_progress(&round), 1.0);
    }

    #[test]
    fn a_zero_interval_does_not_divide_by_zero() {
        let mut zero = rules();
        zero.sheep_interval_ticks = 0;
        zero.countdown_ticks = 0;
        let mut round = Round::new(zero);
        round.join(player(1)).unwrap();
        round.join(player(2)).unwrap();
        assert_eq!(bossbar_progress(&round), 0.0);
        round.tick();
        assert_eq!(bossbar_progress(&round), 0.0);
    }
}

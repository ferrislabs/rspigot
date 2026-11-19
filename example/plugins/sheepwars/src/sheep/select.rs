use super::impact::Vec3;
use crate::domain::TeamId;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Candidate {
    pub team: TeamId,
    pub position: Vec3,
}

pub const fn targetable(standing_team: Option<TeamId>, dead: bool) -> Option<TeamId> {
    if dead { None } else { standing_team }
}

pub fn within(candidates: &[Candidate], center: Vec3, radius: f64, team: TeamId) -> Vec<usize> {
    candidates
        .iter()
        .enumerate()
        .filter(|(_, candidate)| {
            candidate.team == team
                && distance_squared(center, candidate.position) <= radius * radius
        })
        .map(|(index, _)| index)
        .collect()
}

pub fn nearest_enemy(
    candidates: &[Candidate],
    from: Vec3,
    range: f64,
    thrower_team: TeamId,
) -> Option<Vec3> {
    within(candidates, from, range, thrower_team.other())
        .into_iter()
        .map(|index| candidates[index].position)
        .min_by(|a, b| distance_squared(from, *a).total_cmp(&distance_squared(from, *b)))
}

pub fn distance_squared(a: Vec3, b: Vec3) -> f64 {
    (a.x - b.x).powi(2) + (a.y - b.y).powi(2) + (a.z - b.z).powi(2)
}

#[cfg(test)]
mod tests {
    use super::*;

    const fn at(x: f64, y: f64, z: f64) -> Vec3 {
        Vec3 { x, y, z }
    }

    const fn candidate(team: TeamId, position: Vec3) -> Candidate {
        Candidate { team, position }
    }

    #[test]
    fn the_seeker_picks_the_nearest_enemy_and_ignores_teammates() {
        let candidates = [
            candidate(TeamId::Red, at(1.0, 0.0, 0.0)),
            candidate(TeamId::Blue, at(9.0, 0.0, 0.0)),
            candidate(TeamId::Blue, at(4.0, 0.0, 0.0)),
        ];
        assert_eq!(
            nearest_enemy(&candidates, Vec3::ZERO, 20.0, TeamId::Red),
            Some(at(4.0, 0.0, 0.0))
        );
    }

    #[test]
    fn the_seeker_sees_nothing_beyond_its_range_or_without_enemies() {
        let far = [candidate(TeamId::Blue, at(30.0, 0.0, 0.0))];
        assert_eq!(nearest_enemy(&far, Vec3::ZERO, 20.0, TeamId::Red), None);
        let friends = [candidate(TeamId::Red, at(1.0, 0.0, 0.0))];
        assert_eq!(nearest_enemy(&friends, Vec3::ZERO, 20.0, TeamId::Red), None);
        assert_eq!(nearest_enemy(&[], Vec3::ZERO, 20.0, TeamId::Red), None);
    }

    #[test]
    fn blindness_reaches_every_enemy_within_the_radius_and_no_teammate() {
        let candidates = [
            candidate(TeamId::Blue, at(1.0, 0.0, 0.0)),
            candidate(TeamId::Red, at(1.0, 0.0, 1.0)),
            candidate(TeamId::Blue, at(0.0, 5.0, 0.0)),
            candidate(TeamId::Blue, at(0.0, 0.0, 7.0)),
            candidate(TeamId::Blue, at(0.0, 0.0, 6.0)),
        ];
        assert_eq!(
            within(&candidates, Vec3::ZERO, 6.0, TeamId::Blue),
            vec![0, 2, 4]
        );
    }

    #[test]
    fn regeneration_reaches_the_thrower_team_only() {
        let candidates = [
            candidate(TeamId::Blue, at(1.0, 0.0, 0.0)),
            candidate(TeamId::Red, at(2.0, 0.0, 0.0)),
        ];
        assert_eq!(within(&candidates, Vec3::ZERO, 5.0, TeamId::Red), vec![1]);
    }

    #[test]
    fn dead_and_spectating_players_are_not_targets() {
        assert_eq!(targetable(Some(TeamId::Red), false), Some(TeamId::Red));
        assert_eq!(targetable(Some(TeamId::Red), true), None);
        assert_eq!(targetable(None, false), None);
    }
}

use rspigot_core::plugins::{Order, ScoreboardOrder, ScoreboardView, TeamOptionSpec};
use rspigot_plugin_protocol::v1::{
    self, scoreboard_order::Order as Wire, team_option::Option as WireOption,
};

use super::Malformed;
use super::shared::required;

pub(super) fn order(order: v1::ScoreboardOrder) -> Result<Order, Malformed> {
    let board = order.board;
    let order = match required(order.order)? {
        Wire::RegisterObjective(v1::RegisterObjective {
            name,
            criteria,
            display_name,
        }) => ScoreboardOrder::RegisterObjective {
            name,
            criteria,
            display_name,
        },
        Wire::UnregisterObjective(name) => ScoreboardOrder::UnregisterObjective(name),
        Wire::SetObjectiveDisplayName(v1::ObjectiveText { objective, text }) => {
            ScoreboardOrder::SetObjectiveDisplayName { objective, text }
        }
        Wire::SetObjectiveRender(v1::ObjectiveRender { objective, hearts }) => {
            ScoreboardOrder::SetObjectiveRender { objective, hearts }
        }
        Wire::SetDisplaySlot(v1::DisplaySlotOrder { slot, objective }) => {
            ScoreboardOrder::SetDisplaySlot { slot, objective }
        }
        Wire::SetScore(v1::ScoreOrder {
            entry,
            objective,
            value,
        }) => ScoreboardOrder::SetScore {
            entry,
            objective,
            value,
        },
        Wire::ResetScore(v1::ResetScoreOrder { entry, objective }) => {
            ScoreboardOrder::ResetScore { entry, objective }
        }
        Wire::RegisterTeam(name) => ScoreboardOrder::RegisterTeam(name),
        Wire::UnregisterTeam(name) => ScoreboardOrder::UnregisterTeam(name),
        Wire::SetTeamOption(v1::TeamOption { team, option }) => ScoreboardOrder::SetTeamOption {
            team,
            option: match required(option)? {
                WireOption::DisplayName(text) => TeamOptionSpec::DisplayName(text),
                WireOption::Prefix(text) => TeamOptionSpec::Prefix(text),
                WireOption::Suffix(text) => TeamOptionSpec::Suffix(text),
                WireOption::Color(name) => TeamOptionSpec::Color(name),
                WireOption::FriendlyFire(value) => TeamOptionSpec::FriendlyFire(value),
                WireOption::SeeFriendlyInvisibles(value) => {
                    TeamOptionSpec::SeeFriendlyInvisibles(value)
                }
                WireOption::NametagVisibility(name) => TeamOptionSpec::NametagVisibility(name),
                WireOption::DeathMessageVisibility(name) => {
                    TeamOptionSpec::DeathMessageVisibility(name)
                }
                WireOption::CollisionRule(name) => TeamOptionSpec::CollisionRule(name),
            },
        },
        Wire::TeamEntry(v1::TeamEntry { team, entry, join }) => {
            ScoreboardOrder::TeamEntry { team, entry, join }
        }
        Wire::Discard(_) => ScoreboardOrder::Discard,
    };
    Ok(Order::Scoreboard { board, order })
}

pub(super) fn wire_state(view: ScoreboardView) -> v1::ScoreboardState {
    v1::ScoreboardState {
        objectives: view
            .objectives
            .into_iter()
            .map(|objective| v1::ObjectiveState {
                name: objective.name,
                criteria: objective.criteria,
                display_name: objective.display_name,
                hearts: objective.hearts,
                read_only: objective.read_only,
            })
            .collect(),
        scores: view
            .scores
            .into_iter()
            .map(|score| v1::ScoreState {
                entry: score.entry,
                objective: score.objective,
                value: score.value,
            })
            .collect(),
        teams: view
            .teams
            .into_iter()
            .map(|team| v1::TeamState {
                name: team.name,
                display_name: team.display_name,
                prefix: team.prefix,
                suffix: team.suffix,
                color: team.color,
                friendly_fire: team.friendly_fire,
                see_friendly_invisibles: team.see_friendly_invisibles,
                nametag_visibility: team.nametag_visibility,
                death_message_visibility: team.death_message_visibility,
                collision_rule: team.collision_rule,
                entries: team.entries,
            })
            .collect(),
        slots: view
            .slots
            .into_iter()
            .map(|(slot, objective)| v1::SlotState { slot, objective })
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use rspigot_core::plugins::{ObjectiveView, ScoreView, TeamView};

    use super::*;

    #[test]
    fn an_objective_registration_converts_with_its_board() {
        let wire = v1::ScoreboardOrder {
            board: 3,
            order: Some(Wire::RegisterObjective(v1::RegisterObjective {
                name: "kills".to_owned(),
                criteria: "dummy".to_owned(),
                display_name: "Kills".to_owned(),
            })),
        };
        assert_eq!(
            order(wire).unwrap(),
            Order::Scoreboard {
                board: 3,
                order: ScoreboardOrder::RegisterObjective {
                    name: "kills".to_owned(),
                    criteria: "dummy".to_owned(),
                    display_name: "Kills".to_owned(),
                }
            }
        );
    }

    #[test]
    fn team_options_convert_one_variant_each() {
        let option = |option| {
            order(v1::ScoreboardOrder {
                board: 0,
                order: Some(Wire::SetTeamOption(v1::TeamOption {
                    team: "red".to_owned(),
                    option: Some(option),
                })),
            })
            .unwrap()
        };
        assert_eq!(
            option(WireOption::FriendlyFire(false)),
            Order::Scoreboard {
                board: 0,
                order: ScoreboardOrder::SetTeamOption {
                    team: "red".to_owned(),
                    option: TeamOptionSpec::FriendlyFire(false),
                }
            }
        );
        assert!(matches!(
            option(WireOption::Color("red".to_owned())),
            Order::Scoreboard {
                order: ScoreboardOrder::SetTeamOption {
                    option: TeamOptionSpec::Color(_),
                    ..
                },
                ..
            }
        ));
    }

    #[test]
    fn an_empty_order_is_malformed() {
        let wire = v1::ScoreboardOrder {
            board: 0,
            order: None,
        };
        assert!(order(wire).is_err());
        let team = v1::ScoreboardOrder {
            board: 0,
            order: Some(Wire::SetTeamOption(v1::TeamOption {
                team: "x".to_owned(),
                option: None,
            })),
        };
        assert!(order(team).is_err());
    }

    #[test]
    fn the_state_view_converts_field_by_field() {
        let state = wire_state(ScoreboardView {
            objectives: vec![ObjectiveView {
                name: "a".to_owned(),
                criteria: "dummy".to_owned(),
                display_name: "A".to_owned(),
                hearts: true,
                read_only: false,
            }],
            scores: vec![ScoreView {
                entry: "x".to_owned(),
                objective: "a".to_owned(),
                value: 4,
            }],
            teams: vec![TeamView {
                name: "red".to_owned(),
                display_name: "Red".to_owned(),
                prefix: String::new(),
                suffix: String::new(),
                color: Some("red".to_owned()),
                friendly_fire: true,
                see_friendly_invisibles: false,
                nametag_visibility: "always".to_owned(),
                death_message_visibility: "never".to_owned(),
                collision_rule: "pushOwnTeam".to_owned(),
                entries: vec!["x".to_owned()],
            }],
            slots: vec![("sidebar".to_owned(), "a".to_owned())],
        });
        assert!(state.objectives[0].hearts);
        assert_eq!(state.scores[0].value, 4);
        assert_eq!(state.teams[0].color.as_deref(), Some("red"));
        assert_eq!(state.teams[0].entries, ["x"]);
        assert_eq!(state.slots[0].objective, "a");
    }
}

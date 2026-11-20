mod fake_host;

use fake_host::*;
use rspigot_plugin::{
    CollisionRule, Criteria, DisplaySlot, Error, RenderType, TeamColor, Visibility,
};
use rspigot_plugin_protocol::v1::{self, answer, from_plugin, question, scoreboard_order};

fn ordered(board: u32, order: scoreboard_order::Order) -> from_plugin::Body {
    from_plugin::Body::Scoreboard(v1::ScoreboardOrder {
        board,
        order: Some(order),
    })
}

fn team_option(option: v1::team_option::Option) -> from_plugin::Body {
    ordered(
        0,
        scoreboard_order::Order::SetTeamOption(v1::TeamOption {
            team: "red".to_owned(),
            option: Some(option),
        }),
    )
}

fn state() -> v1::ScoreboardState {
    v1::ScoreboardState {
        objectives: vec![v1::ObjectiveState {
            name: "kills".to_owned(),
            criteria: "dummy".to_owned(),
            display_name: "Kills".to_owned(),
            hearts: false,
            read_only: false,
        }],
        scores: vec![v1::ScoreState {
            entry: "Steve".to_owned(),
            objective: "kills".to_owned(),
            value: 7,
        }],
        teams: vec![v1::TeamState {
            name: "red".to_owned(),
            display_name: "Red".to_owned(),
            prefix: "[R] ".to_owned(),
            suffix: String::new(),
            color: Some("red".to_owned()),
            friendly_fire: false,
            see_friendly_invisibles: true,
            nametag_visibility: "hideForOtherTeams".to_owned(),
            death_message_visibility: "never".to_owned(),
            collision_rule: "pushOwnTeam".to_owned(),
            entries: vec!["Steve".to_owned()],
        }],
        slots: vec![v1::SlotState {
            slot: "sidebar.team.gold".to_owned(),
            objective: "kills".to_owned(),
        }],
    }
}

#[tokio::test]
async fn scoreboard_orders_go_on_the_wire() {
    let mut called = calling(|player| {
        Box::pin(async move {
            let board = player.server().scoreboard_manager.main_scoreboard();
            assert!(board.is_main());
            let kills = board.register_objective("kills", &Criteria::dummy(), "Kills");
            kills.set_display_name("Total kills");
            kills.set_render_type(RenderType::Hearts);
            board.set_display_slot(DisplaySlot::Sidebar, Some(&kills));
            board.set_display_slot(DisplaySlot::SidebarTeam(TeamColor::Gold), None);
            let score = kills.score("Steve");
            score.set(4);
            score.reset();
            board.reset_scores("Alex");
            kills.unregister();
            let team = board.register_team("red");
            team.set_display_name("Red");
            team.set_prefix("[R] ");
            team.set_suffix("!");
            team.set_color(Some(TeamColor::Red));
            team.set_color(None);
            team.set_friendly_fire(false);
            team.set_see_friendly_invisibles(false);
            team.set_nametag_visibility(Visibility::Never);
            team.set_death_message_visibility(Visibility::HideForOwnTeam);
            team.set_collision_rule(CollisionRule::PushOtherTeams);
            team.add_entry("Steve");
            team.remove_entry("Steve");
            team.unregister();
        })
    })
    .await;
    use scoreboard_order::Order;
    use v1::team_option::Option as Choice;
    let expected = vec![
        ordered(
            0,
            Order::RegisterObjective(v1::RegisterObjective {
                name: "kills".to_owned(),
                criteria: "dummy".to_owned(),
                display_name: "Kills".to_owned(),
            }),
        ),
        ordered(
            0,
            Order::SetObjectiveDisplayName(v1::ObjectiveText {
                objective: "kills".to_owned(),
                text: "Total kills".to_owned(),
            }),
        ),
        ordered(
            0,
            Order::SetObjectiveRender(v1::ObjectiveRender {
                objective: "kills".to_owned(),
                hearts: true,
            }),
        ),
        ordered(
            0,
            Order::SetDisplaySlot(v1::DisplaySlotOrder {
                slot: "sidebar".to_owned(),
                objective: Some("kills".to_owned()),
            }),
        ),
        ordered(
            0,
            Order::SetDisplaySlot(v1::DisplaySlotOrder {
                slot: "sidebar.team.gold".to_owned(),
                objective: None,
            }),
        ),
        ordered(
            0,
            Order::SetScore(v1::ScoreOrder {
                entry: "Steve".to_owned(),
                objective: "kills".to_owned(),
                value: 4,
            }),
        ),
        ordered(
            0,
            Order::ResetScore(v1::ResetScoreOrder {
                entry: "Steve".to_owned(),
                objective: Some("kills".to_owned()),
            }),
        ),
        ordered(
            0,
            Order::ResetScore(v1::ResetScoreOrder {
                entry: "Alex".to_owned(),
                objective: None,
            }),
        ),
        ordered(0, Order::UnregisterObjective("kills".to_owned())),
        ordered(0, Order::RegisterTeam("red".to_owned())),
        team_option(Choice::DisplayName("Red".to_owned())),
        team_option(Choice::Prefix("[R] ".to_owned())),
        team_option(Choice::Suffix("!".to_owned())),
        team_option(Choice::Color("red".to_owned())),
        team_option(Choice::Color(String::new())),
        team_option(Choice::FriendlyFire(false)),
        team_option(Choice::SeeFriendlyInvisibles(false)),
        team_option(Choice::NametagVisibility("never".to_owned())),
        team_option(Choice::DeathMessageVisibility("hideForOwnTeam".to_owned())),
        team_option(Choice::CollisionRule("pushOtherTeams".to_owned())),
        ordered(
            0,
            Order::TeamEntry(v1::TeamEntry {
                team: "red".to_owned(),
                entry: "Steve".to_owned(),
                join: true,
            }),
        ),
        ordered(
            0,
            Order::TeamEntry(v1::TeamEntry {
                team: "red".to_owned(),
                entry: "Steve".to_owned(),
                join: false,
            }),
        ),
        ordered(0, Order::UnregisterTeam("red".to_owned())),
    ];
    for order in expected {
        assert_eq!(called.session.hear().await, order);
    }
    called.finished().await;
}

#[tokio::test]
async fn a_new_scoreboard_has_its_own_id_and_can_be_shown_to_a_player() {
    let mut called = calling(|player| {
        Box::pin(async move {
            let manager = player.server().scoreboard_manager;
            let own = manager.new_scoreboard().await.unwrap();
            assert_eq!(own.id, 7);
            assert!(!own.is_main());
            own.register_team("blue");
            player.scoreboard.set(&own);
            player.scoreboard.set(&manager.main_scoreboard());
            own.discard();
        })
    })
    .await;
    let (request, asked) = called.session.asked().await;
    assert!(matches!(asked, question::Question::NewScoreboard(_)));
    called
        .session
        .say(answer(request, answer::Body::ScoreboardCreated(7)));
    assert_eq!(
        called.session.hear().await,
        ordered(7, scoreboard_order::Order::RegisterTeam("blue".to_owned()))
    );
    assert_eq!(
        called.session.hear().await,
        from_plugin::Body::Player(v1::PlayerOrder {
            player: STEVE.to_owned(),
            order: Some(v1::player_order::Order::SetScoreboard(7)),
        })
    );
    assert_eq!(
        called.session.hear().await,
        from_plugin::Body::Player(v1::PlayerOrder {
            player: STEVE.to_owned(),
            order: Some(v1::player_order::Order::SetScoreboard(0)),
        })
    );
    assert_eq!(
        called.session.hear().await,
        ordered(7, scoreboard_order::Order::Discard(v1::Empty {}))
    );
    called.finished().await;
}

#[tokio::test]
async fn the_state_of_a_scoreboard_is_read_back_as_typed_values() {
    let mut called = calling(|player| {
        Box::pin(async move {
            let board = player.server().scoreboard_manager.main_scoreboard();
            let read = board.state().await.unwrap();
            assert_eq!(read.objectives[0].name, "kills");
            assert_eq!(read.objectives[0].criteria, Criteria::dummy());
            assert_eq!(read.objectives[0].render_type, RenderType::Integer);
            assert_eq!(read.scores[0].value, 7);
            let team = &read.teams[0];
            assert_eq!(team.color, Some(TeamColor::Red));
            assert!(!team.friendly_fire);
            assert_eq!(team.nametag_visibility, Visibility::HideForOtherTeams);
            assert_eq!(team.death_message_visibility, Visibility::Never);
            assert_eq!(team.collision_rule, CollisionRule::PushOwnTeam);
            assert_eq!(team.entries, ["Steve"]);
            assert_eq!(
                read.slots,
                [(
                    DisplaySlot::SidebarTeam(TeamColor::Gold),
                    "kills".to_owned()
                )]
            );
            let kills = board.objective("kills");
            assert_eq!(kills.score("Steve").get().await.unwrap(), 7);
            assert_eq!(kills.score("Nobody").get().await.unwrap(), 0);
            assert!(matches!(board.state().await, Err(Error::NotFound)));
        })
    })
    .await;
    for _ in 0..3 {
        let (request, asked) = called.session.asked().await;
        assert_eq!(asked, question::Question::Scoreboard(0));
        called
            .session
            .say(answer(request, answer::Body::Scoreboard(state())));
    }
    let (request, _) = called.session.asked().await;
    called
        .session
        .say(answer(request, answer::Body::NotFound(v1::NotFound {})));
    called.finished().await;
}

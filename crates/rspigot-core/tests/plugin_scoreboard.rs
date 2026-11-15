mod common;

use common::*;
use rspigot_core::events::ClientEvent;
use rspigot_core::plugins::{
    Answer, Order, PlayerOrder, Question, ScoreboardOrder, ScoreboardView, TeamOptionSpec,
};
use rspigot_domain::scoreboard::{DisplaySlot, ScoreboardMessage, TeamOp};

fn board(order: ScoreboardOrder) -> Order {
    Order::Scoreboard { board: 0, order }
}

fn messages(player: &Player) -> Vec<ScoreboardMessage> {
    player
        .link
        .take()
        .into_iter()
        .filter_map(|event| match event {
            ClientEvent::Scoreboard(message) => Some(*message),
            _ => None,
        })
        .collect()
}

fn view(s: &mut Scene, board: u32) -> ScoreboardView {
    let Answer::Scoreboard(view) = s.ask(Question::Scoreboard(board)) else {
        panic!("expected a scoreboard");
    };
    view
}

fn register(name: &str, criteria: &str) -> Order {
    board(ScoreboardOrder::RegisterObjective {
        name: name.into(),
        criteria: criteria.into(),
        display_name: format!("§e{name}"),
    })
}

#[test]
fn a_plugin_builds_the_main_scoreboard_and_every_player_sees_it() {
    let mut s = scene(&["Alice", "Bob"]);
    s.act(register("kills", "dummy"));
    s.act(board(ScoreboardOrder::SetScore {
        entry: "Alice".into(),
        objective: "kills".into(),
        value: 3,
    }));
    for player in &s.players {
        assert!(messages(player).is_empty());
    }
    s.act(board(ScoreboardOrder::SetDisplaySlot {
        slot: "sidebar".into(),
        objective: Some("kills".into()),
    }));
    for player in &s.players {
        let seen = messages(player);
        assert!(matches!(seen[0], ScoreboardMessage::ObjectiveAdd(_)));
        assert!(matches!(
            seen[1],
            ScoreboardMessage::DisplayObjective {
                slot: DisplaySlot::Sidebar,
                ..
            }
        ));
        assert!(matches!(seen[2], ScoreboardMessage::Score { value: 3, .. }));
    }
    s.act(board(ScoreboardOrder::SetScore {
        entry: "Alice".into(),
        objective: "kills".into(),
        value: 4,
    }));
    for player in &s.players {
        assert!(matches!(
            messages(player)[0],
            ScoreboardMessage::Score { value: 4, .. }
        ));
    }
    let seen = view(&mut s, 0);
    assert_eq!(seen.objectives[0].name, "kills");
    assert_eq!(seen.objectives[0].display_name, "§ekills");
    assert_eq!(seen.scores[0].value, 4);
    assert_eq!(seen.slots, [("sidebar".to_owned(), "kills".to_owned())]);
    s.act(board(ScoreboardOrder::UnregisterObjective("kills".into())));
    assert!(view(&mut s, 0).objectives.is_empty());
    assert!(s.warnings().is_empty());
}

#[test]
fn teams_are_built_through_orders() {
    let mut s = scene(&["Alice"]);
    s.act(board(ScoreboardOrder::RegisterTeam("red".into())));
    s.act(board(ScoreboardOrder::SetTeamOption {
        team: "red".into(),
        option: TeamOptionSpec::Color("red".into()),
    }));
    s.act(board(ScoreboardOrder::SetTeamOption {
        team: "red".into(),
        option: TeamOptionSpec::FriendlyFire(false),
    }));
    s.act(board(ScoreboardOrder::SetTeamOption {
        team: "red".into(),
        option: TeamOptionSpec::Prefix("§c[R] ".into()),
    }));
    s.act(board(ScoreboardOrder::TeamEntry {
        team: "red".into(),
        entry: "Alice".into(),
        join: true,
    }));
    let seen = messages(&s.players[0]);
    assert!(matches!(
        seen[0],
        ScoreboardMessage::Team {
            op: TeamOp::Add { .. },
            ..
        }
    ));
    assert!(matches!(
        seen.last(),
        Some(ScoreboardMessage::Team {
            op: TeamOp::Join(_),
            ..
        })
    ));
    let team = view(&mut s, 0).teams.remove(0);
    assert_eq!(team.color.as_deref(), Some("red"));
    assert!(!team.friendly_fire);
    assert_eq!(team.prefix, "§c[R] ");
    assert_eq!(team.entries, ["Alice"]);
    s.act(board(ScoreboardOrder::TeamEntry {
        team: "red".into(),
        entry: "Alice".into(),
        join: false,
    }));
    s.act(board(ScoreboardOrder::UnregisterTeam("red".into())));
    assert!(view(&mut s, 0).teams.is_empty());
    assert!(s.warnings().is_empty());
}

#[test]
fn a_private_scoreboard_only_reaches_the_players_it_is_shown_to() {
    let mut s = scene(&["Alice", "Bob"]);
    s.act(register("shared", "dummy"));
    s.act(board(ScoreboardOrder::SetDisplaySlot {
        slot: "list".into(),
        objective: Some("shared".into()),
    }));
    for player in &s.players {
        messages(player);
    }
    let Answer::ScoreboardCreated(own) = s.ask(Question::NewScoreboard) else {
        panic!("expected a new scoreboard");
    };
    assert_ne!(own, 0);
    let private = |order| Order::Scoreboard { board: own, order };
    s.act(private(ScoreboardOrder::RegisterObjective {
        name: "mine".into(),
        criteria: "dummy".into(),
        display_name: "mine".into(),
    }));
    s.act(private(ScoreboardOrder::SetDisplaySlot {
        slot: "sidebar".into(),
        objective: Some("mine".into()),
    }));
    s.act(private(ScoreboardOrder::SetScore {
        entry: "x".into(),
        objective: "mine".into(),
        value: 9,
    }));
    assert!(messages(&s.players[0]).is_empty());
    s.act(Order::Player {
        player: s.players[0].id,
        order: PlayerOrder::SetScoreboard(own),
    });
    let alice = messages(&s.players[0]);
    assert_eq!(
        alice[0],
        ScoreboardMessage::ObjectiveRemove("shared".to_owned())
    );
    assert!(
        alice
            .iter()
            .any(|m| matches!(m, ScoreboardMessage::Score { value: 9, .. }))
    );
    assert!(messages(&s.players[1]).is_empty());
    s.act(private(ScoreboardOrder::SetScore {
        entry: "x".into(),
        objective: "mine".into(),
        value: 10,
    }));
    assert_eq!(messages(&s.players[0]).len(), 1);
    assert!(messages(&s.players[1]).is_empty());
    s.act(Order::Scoreboard {
        board: own,
        order: ScoreboardOrder::Discard,
    });
    let back = messages(&s.players[0]);
    assert!(matches!(back[0], ScoreboardMessage::ObjectiveRemove(ref name) if name == "mine"));
    assert!(
        back.iter()
            .any(|m| matches!(m, ScoreboardMessage::ObjectiveAdd(_)))
    );
    assert!(matches!(s.ask(Question::Scoreboard(own)), Answer::NotFound));
    assert!(s.warnings().is_empty());
}

#[test]
fn invalid_orders_warn_and_change_nothing() {
    let mut s = scene(&["Alice"]);
    s.act(register("kills", "nonsense"));
    s.act(board(ScoreboardOrder::UnregisterObjective("nope".into())));
    s.act(board(ScoreboardOrder::SetDisplaySlot {
        slot: "nowhere".into(),
        objective: None,
    }));
    s.act(board(ScoreboardOrder::SetTeamOption {
        team: "ghost".into(),
        option: TeamOptionSpec::FriendlyFire(true),
    }));
    s.act(Order::Scoreboard {
        board: 99,
        order: ScoreboardOrder::RegisterTeam("x".into()),
    });
    s.act(board(ScoreboardOrder::Discard));
    s.act(Order::Player {
        player: s.players[0].id,
        order: PlayerOrder::SetScoreboard(99),
    });
    assert_eq!(s.warnings().len(), 7);
    assert!(view(&mut s, 0).objectives.is_empty());
}

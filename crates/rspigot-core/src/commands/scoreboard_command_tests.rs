use rspigot_domain::player::PlayerId;
use rspigot_domain::scoreboard::{DisplaySlot, ScoreboardMessage};

use super::{CommandOutcome, CommandSender, execute};
use crate::end_tests::{Tap, arrive};
use crate::events::ClientEvent;
use crate::server::Server;

struct Rig {
    server: Server<Tap>,
    id: PlayerId,
    link: Tap,
}

fn rig() -> Rig {
    let (mut server, id, link) = arrive();
    if let Some(player) = server.players.get_mut(&id) {
        player.operator = true;
    }
    Rig { server, id, link }
}

impl Rig {
    fn run(&mut self, line: &str) -> CommandOutcome {
        let outcome = execute(&mut self.server, CommandSender::Player(self.id), line);
        self.server.deliver();
        outcome
    }

    fn ok(&mut self, line: &str) {
        assert_eq!(self.run(line), CommandOutcome::Success, "{line}");
    }

    fn texts(&self) -> Vec<String> {
        self.link
            .take()
            .into_iter()
            .filter_map(|event| match event {
                ClientEvent::SystemChat { message, .. } => Some(message.plain_text()),
                _ => None,
            })
            .collect()
    }

    fn score(&self, holder: &str, objective: &str) -> Option<i32> {
        self.server
            .scoreboard
            .board
            .score(holder, objective)
            .map(|score| score.value)
    }

    fn packets(&self) -> Vec<ScoreboardMessage> {
        self.link
            .take()
            .into_iter()
            .filter_map(|event| match event {
                ClientEvent::Scoreboard(message) => Some(*message),
                _ => None,
            })
            .collect()
    }
}

#[test]
fn objectives_are_added_listed_and_removed() {
    let mut rig = rig();
    rig.texts();
    rig.ok("scoreboard objectives list");
    assert_eq!(rig.texts(), ["commands.scoreboard.objectives.list.empty"]);
    rig.ok("scoreboard objectives add kills dummy");
    assert_eq!(
        rig.texts(),
        ["commands.scoreboard.objectives.add.success [kills]"]
    );
    rig.ok("scoreboard objectives add hp health {\"text\":\"Health\"}");
    assert_eq!(
        rig.run("scoreboard objectives add kills dummy"),
        CommandOutcome::Failure
    );
    assert_eq!(
        rig.texts().last().map(String::as_str),
        Some("commands.scoreboard.objectives.add.duplicate")
    );
    assert_eq!(
        rig.run("scoreboard objectives add x nonsense"),
        CommandOutcome::Failure
    );
    assert_eq!(
        rig.texts(),
        [
            "commands.scoreboard.objectives.add.success [hp]",
            "argument.criteria.invalid nonsense"
        ]
        .iter()
        .skip(1)
        .map(ToString::to_string)
        .collect::<Vec<_>>()
    );
    rig.ok("scoreboard objectives list");
    assert_eq!(
        rig.texts(),
        ["commands.scoreboard.objectives.list.success 2 [Health], [kills]"]
    );
    rig.ok("scoreboard objectives remove kills");
    assert!(rig.server.scoreboard.board.objective("kills").is_none());
    assert_eq!(
        rig.run("scoreboard objectives remove kills"),
        CommandOutcome::Failure
    );
}

#[test]
fn modify_changes_display_render_type_auto_update_and_format() {
    let mut rig = rig();
    rig.ok("scoreboard objectives add kills dummy");
    rig.ok("scoreboard objectives modify kills displayname {\"text\":\"Kills\"}");
    rig.ok("scoreboard objectives modify kills rendertype hearts");
    rig.ok("scoreboard objectives modify kills displayautoupdate true");
    rig.ok("scoreboard objectives modify kills numberformat fixed {\"text\":\"!\"}");
    let objective = rig.server.scoreboard.board.objective("kills").unwrap();
    assert_eq!(objective.display_name.plain_text(), "Kills");
    assert_eq!(objective.render_type.name(), "hearts");
    assert!(objective.display_auto_update);
    assert!(objective.number_format.is_some());
    rig.ok("scoreboard objectives modify kills numberformat");
    assert!(
        rig.server
            .scoreboard
            .board
            .objective("kills")
            .unwrap()
            .number_format
            .is_none()
    );
    rig.ok("scoreboard objectives modify kills numberformat styled {\"color\":\"red\"}");
    rig.ok("scoreboard objectives modify kills numberformat blank");
    assert_eq!(
        rig.run("scoreboard objectives modify kills rendertype nope"),
        CommandOutcome::Failure
    );
}

#[test]
fn setdisplay_shows_an_objective_and_tells_the_client() {
    let mut rig = rig();
    rig.ok("scoreboard objectives add kills dummy");
    rig.texts();
    rig.ok("scoreboard objectives setdisplay sidebar kills");
    let packets = rig.packets();
    assert!(matches!(packets[0], ScoreboardMessage::ObjectiveAdd(_)));
    assert!(matches!(
        packets[1],
        ScoreboardMessage::DisplayObjective {
            slot: DisplaySlot::Sidebar,
            ..
        }
    ));
    assert_eq!(
        rig.run("scoreboard objectives setdisplay sidebar kills"),
        CommandOutcome::Failure
    );
    rig.ok("scoreboard objectives setdisplay sidebar.team.red kills");
    rig.ok("scoreboard objectives setdisplay sidebar");
    assert_eq!(
        rig.server.scoreboard.board.display(DisplaySlot::Sidebar),
        None
    );
    assert_eq!(
        rig.run("scoreboard objectives setdisplay sidebar"),
        CommandOutcome::Failure
    );
    assert_eq!(
        rig.run("scoreboard objectives setdisplay nowhere kills"),
        CommandOutcome::Failure
    );
}

#[test]
fn players_set_add_remove_get_and_reset() {
    let mut rig = rig();
    rig.ok("scoreboard objectives add kills dummy");
    rig.texts();
    rig.ok("scoreboard players set Steve kills 5");
    assert_eq!(
        rig.texts(),
        ["commands.scoreboard.players.set.success.single [kills] Steve 5"]
    );
    rig.ok("scoreboard players add Steve kills 3");
    assert_eq!(
        rig.texts(),
        ["commands.scoreboard.players.add.success.single 3 [kills] Steve 8"]
    );
    rig.ok("scoreboard players remove Steve kills 2");
    assert_eq!(rig.score("Steve", "kills"), Some(6));
    rig.texts();
    rig.ok("scoreboard players get Steve kills");
    assert_eq!(
        rig.texts(),
        ["commands.scoreboard.players.get.success Steve 6 [kills]"]
    );
    assert_eq!(
        rig.run("scoreboard players get Alex kills"),
        CommandOutcome::Failure
    );
    assert_eq!(
        rig.texts(),
        ["commands.scoreboard.players.get.null [kills] Alex"]
    );
    assert_eq!(
        rig.run("scoreboard players add Steve kills -1"),
        CommandOutcome::Failure
    );
    assert_eq!(
        rig.run("scoreboard players set Steve kills x"),
        CommandOutcome::Failure
    );
    assert_eq!(
        rig.run("scoreboard players set Steve nothing 1"),
        CommandOutcome::Failure
    );
    rig.texts();
    rig.ok("scoreboard players set * kills 9");
    assert_eq!(
        rig.texts(),
        ["commands.scoreboard.players.set.success.single [kills] Steve 9"]
    );
    rig.ok("scoreboard players set @a kills 1");
    assert_eq!(rig.score("Alice", "kills"), Some(1));
    rig.ok("scoreboard players list");
    rig.ok("scoreboard players list Steve");
    rig.texts();
    rig.ok("scoreboard players reset Steve kills");
    assert_eq!(rig.score("Steve", "kills"), None);
    rig.ok("scoreboard players reset @s");
    assert_eq!(rig.score("Alice", "kills"), None);
    rig.texts();
    rig.ok("scoreboard players list");
    assert_eq!(rig.texts(), ["commands.scoreboard.players.list.empty"]);
}

#[test]
fn read_only_objectives_refuse_writes_but_accept_reads() {
    let mut rig = rig();
    rig.ok("scoreboard objectives add hp health");
    assert_eq!(
        rig.run("scoreboard players set Alice hp 3"),
        CommandOutcome::Failure
    );
    assert_eq!(
        rig.texts().last().map(String::as_str),
        Some("arguments.objective.readonly hp")
    );
    rig.server.tick_scoreboard_criteria();
    assert_eq!(rig.score("Alice", "hp"), Some(20));
    rig.ok("scoreboard players get Alice hp");
}

#[test]
fn operation_applies_every_operator() {
    let mut rig = rig();
    rig.ok("scoreboard objectives add a dummy");
    rig.ok("scoreboard objectives add b dummy");
    rig.ok("scoreboard players set x a 17");
    rig.ok("scoreboard players set y b 5");
    let cases = [
        ("=", 5),
        ("+=", 22),
        ("-=", 12),
        ("*=", 85),
        ("/=", 3),
        ("%=", 2),
        ("<", 5),
        (">", 17),
    ];
    for (operator, expected) in cases {
        rig.ok("scoreboard players set x a 17");
        rig.ok(&format!("scoreboard players operation x a {operator} y b"));
        assert_eq!(rig.score("x", "a"), Some(expected), "{operator}");
    }
    rig.ok("scoreboard players set x a 17");
    rig.ok("scoreboard players operation x a >< y b");
    assert_eq!(
        (rig.score("x", "a"), rig.score("y", "b")),
        (Some(5), Some(17))
    );
    rig.ok("scoreboard players set x a -7");
    rig.ok("scoreboard players set y b 2");
    rig.ok("scoreboard players operation x a /= y b");
    assert_eq!(rig.score("x", "a"), Some(-4));
    rig.ok("scoreboard players set x a -7");
    rig.ok("scoreboard players operation x a %= y b");
    assert_eq!(rig.score("x", "a"), Some(1));
    rig.ok("scoreboard players set y b 0");
    assert_eq!(
        rig.run("scoreboard players operation x a /= y b"),
        CommandOutcome::Failure
    );
    assert_eq!(
        rig.run("scoreboard players operation x a ?? y b"),
        CommandOutcome::Failure
    );
}

#[test]
fn per_score_display_name_and_number_format() {
    let mut rig = rig();
    rig.ok("scoreboard objectives add kills dummy");
    rig.ok("scoreboard players set x kills 1");
    rig.ok("scoreboard players display name x kills {\"text\":\"Ex\"}");
    let score = rig.server.scoreboard.board.score("x", "kills").unwrap();
    assert_eq!(
        score.display.as_ref().map(|c| c.plain_text()),
        Some("Ex".to_owned())
    );
    rig.ok("scoreboard players display name x kills");
    assert!(
        rig.server
            .scoreboard
            .board
            .score("x", "kills")
            .unwrap()
            .display
            .is_none()
    );
    rig.ok("scoreboard players display numberformat x kills blank");
    assert!(
        rig.server
            .scoreboard
            .board
            .score("x", "kills")
            .unwrap()
            .number_format
            .is_some()
    );
    rig.ok("scoreboard players display numberformat x kills");
    assert!(
        rig.server
            .scoreboard
            .board
            .score("x", "kills")
            .unwrap()
            .number_format
            .is_none()
    );
}

#[test]
fn trigger_needs_enable_then_locks_again() {
    let mut rig = rig();
    rig.ok("scoreboard objectives add t trigger");
    rig.ok("scoreboard objectives add plain dummy");
    rig.texts();
    assert_eq!(rig.run("trigger t"), CommandOutcome::Failure);
    assert_eq!(rig.texts(), ["commands.trigger.failed.unprimed"]);
    assert_eq!(rig.run("trigger plain"), CommandOutcome::Failure);
    assert_eq!(rig.texts(), ["commands.trigger.failed.invalid"]);
    assert_eq!(
        rig.run("scoreboard players enable Alice plain"),
        CommandOutcome::Failure
    );
    rig.ok("scoreboard players enable Alice t");
    assert_eq!(
        rig.texts().last().map(String::as_str),
        Some("commands.scoreboard.players.enable.success.single [t] Alice")
    );
    assert_eq!(
        rig.run("scoreboard players enable Alice t"),
        CommandOutcome::Failure
    );
    rig.ok("trigger t");
    assert_eq!(rig.score("Alice", "t"), Some(1));
    assert_eq!(rig.run("trigger t"), CommandOutcome::Failure);
    rig.ok("scoreboard players enable Alice t");
    rig.ok("trigger t add 4");
    assert_eq!(rig.score("Alice", "t"), Some(5));
    rig.ok("scoreboard players enable Alice t");
    rig.ok("trigger t set 2");
    assert_eq!(rig.score("Alice", "t"), Some(2));
    assert_eq!(rig.run("trigger nothing"), CommandOutcome::Failure);
}

#[test]
fn teams_are_created_modified_and_joined() {
    let mut rig = rig();
    rig.texts();
    rig.ok("team add red");
    assert_eq!(rig.texts(), ["commands.team.add.success [red]"]);
    assert_eq!(rig.run("team add red"), CommandOutcome::Failure);
    rig.ok("team modify red color red");
    rig.ok("team modify red prefix {\"text\":\"[R] \"}");
    rig.ok("team modify red friendlyFire false");
    rig.ok("team modify red seeFriendlyInvisibles false");
    rig.ok("team modify red nametagVisibility hideForOtherTeams");
    rig.ok("team modify red deathMessageVisibility never");
    rig.ok("team modify red collisionRule pushOwnTeam");
    rig.ok("team modify red displayName {\"text\":\"Reds\"}");
    let team = rig.server.scoreboard.board.team("red").unwrap().clone();
    assert!(!team.friendly_fire && !team.see_friendly_invisibles);
    assert_eq!(team.prefix.plain_text(), "[R] ");
    assert_eq!(team.display_name.plain_text(), "Reds");
    for line in [
        "team modify red color red",
        "team modify red friendlyFire false",
        "team modify red seeFriendlyInvisibles false",
        "team modify red nametagVisibility hideForOtherTeams",
        "team modify red deathMessageVisibility never",
        "team modify red collisionRule pushOwnTeam",
        "team modify red displayName {\"text\":\"Reds\"}",
    ] {
        assert_eq!(rig.run(line), CommandOutcome::Failure, "{line}");
    }
    assert_eq!(
        rig.run("team modify nope color red"),
        CommandOutcome::Failure
    );
    assert_eq!(
        rig.run("team modify red color mauve"),
        CommandOutcome::Failure
    );
    rig.ok("team modify red color reset");
    rig.ok("team join red");
    assert_eq!(
        rig.server
            .scoreboard
            .board
            .team_of("Alice")
            .map(|t| t.name.as_str()),
        Some("red")
    );
    rig.ok("team join red Bob");
    rig.ok("team join red Carol");
    rig.texts();
    rig.ok("team list red");
    assert_eq!(
        rig.texts(),
        ["commands.team.list.members.success [Reds] 3 Alice, Bob, Carol"]
    );
    rig.ok("team list");
    assert_eq!(rig.texts(), ["commands.team.list.teams.success 1 [Reds]"]);
    rig.ok("team leave Bob");
    assert!(rig.server.scoreboard.board.team_of("Bob").is_none());
    rig.ok("team empty red");
    assert_eq!(rig.run("team empty red"), CommandOutcome::Failure);
    rig.ok("team remove red");
    assert!(rig.server.scoreboard.board.team("red").is_none());
    assert_eq!(rig.run("team remove red"), CommandOutcome::Failure);
}

#[test]
fn team_changes_reach_the_client_as_team_packets() {
    let mut rig = rig();
    rig.packets();
    rig.ok("team add red");
    rig.ok("team join red Alice");
    rig.ok("team modify red color red");
    rig.ok("team leave Alice");
    rig.ok("team remove red");
    let kinds: Vec<&str> = rig
        .packets()
        .iter()
        .map(|message| match message {
            ScoreboardMessage::Team { op, .. } => match op {
                rspigot_domain::scoreboard::TeamOp::Add { .. } => "add",
                rspigot_domain::scoreboard::TeamOp::Modify(_) => "modify",
                rspigot_domain::scoreboard::TeamOp::Remove => "remove",
                rspigot_domain::scoreboard::TeamOp::Join(_) => "join",
                rspigot_domain::scoreboard::TeamOp::Leave(_) => "leave",
            },
            _ => "other",
        })
        .collect();
    assert_eq!(kinds, ["add", "join", "modify", "leave", "remove"]);
}

#[test]
fn teammsg_reaches_only_the_team() {
    let mut rig = rig();
    assert_eq!(rig.run("teammsg hello"), CommandOutcome::Failure);
    assert_eq!(
        rig.texts().last().map(String::as_str),
        Some("commands.teammsg.failed.noteam")
    );
    rig.ok("team add red");
    rig.ok("team join red");
    rig.texts();
    rig.ok("teammsg hello there");
    let heard: Vec<_> = rig
        .link
        .take()
        .into_iter()
        .filter_map(|event| match event {
            ClientEvent::ChatMessage {
                chat_type, message, ..
            } => Some((chat_type, message.plain_text())),
            _ => None,
        })
        .collect();
    assert_eq!(
        heard,
        [(
            crate::events::ChatKind::TeamOutgoing,
            "hello there".to_owned()
        )]
    );
    rig.ok("tm hi");
}

#[test]
fn selectors_filter_on_real_scores_and_teams() {
    let mut rig = rig();
    rig.ok("scoreboard objectives add kills dummy");
    rig.ok("scoreboard players set Alice kills 5");
    rig.ok("team add red");
    rig.texts();
    rig.ok("scoreboard players add @a[scores={kills=3..}] kills 1");
    assert_eq!(rig.score("Alice", "kills"), Some(6));
    assert_eq!(
        rig.run("scoreboard players add @a[scores={kills=..2}] kills 1"),
        CommandOutcome::Failure
    );
    assert_eq!(
        rig.run("scoreboard players add @a[scores={missing=1..}] kills 1"),
        CommandOutcome::Failure
    );
    assert_eq!(
        rig.run("scoreboard players add @a[team=red] kills 1"),
        CommandOutcome::Failure
    );
    rig.ok("scoreboard players add @a[team=] kills 1");
    assert_eq!(rig.score("Alice", "kills"), Some(7));
    rig.ok("team join red");
    rig.ok("scoreboard players add @a[team=red] kills 1");
    assert_eq!(rig.score("Alice", "kills"), Some(8));
    assert_eq!(
        rig.run("scoreboard players add @a[team=!red] kills 100"),
        CommandOutcome::Failure
    );
}

#[test]
fn execute_reads_and_writes_scores() {
    let mut rig = rig();
    rig.ok("scoreboard objectives add a dummy");
    rig.ok("scoreboard objectives add b dummy");
    rig.ok("scoreboard players set x a 4");
    rig.ok("scoreboard players set y b 9");
    rig.texts();
    rig.ok("execute if score x a matches 3..5 run scoreboard players set hit a 1");
    assert_eq!(rig.score("hit", "a"), Some(1));
    assert_eq!(
        rig.run("execute if score x a matches 10.. run scoreboard players set miss a 1"),
        CommandOutcome::Failure
    );
    assert_eq!(rig.score("miss", "a"), None);
    rig.ok("execute unless score x a matches 10.. run scoreboard players set miss a 2");
    assert_eq!(rig.score("miss", "a"), Some(2));
    rig.ok("execute if score x a < y b run scoreboard players set lt a 1");
    assert_eq!(rig.score("lt", "a"), Some(1));
    assert_eq!(
        rig.run("execute if score x a >= y b run scoreboard players set ge a 1"),
        CommandOutcome::Failure
    );
    assert_eq!(
        rig.run("execute if score nobody a = y b run scoreboard players set ne a 1"),
        CommandOutcome::Failure
    );
    rig.ok("execute store result score out a run scoreboard players get y b");
    assert_eq!(rig.score("out", "a"), Some(9));
    rig.ok("execute store success score ok b run scoreboard players get y b");
    assert_eq!(rig.score("ok", "b"), Some(1));
    rig.ok("execute store result score @s a if score x a matches 4");
    assert_eq!(rig.score("Alice", "a"), Some(1));
    assert_eq!(
        rig.run("execute if score x nothing matches 1 run say no"),
        CommandOutcome::Failure
    );
    assert_eq!(
        rig.texts().last().map(String::as_str),
        Some("arguments.objective.notFound nothing")
    );
    rig.ok("scoreboard objectives add hp health");
    rig.texts();
    rig.run("execute store result score x hp run say hi");
    assert_eq!(
        rig.texts().last().map(String::as_str),
        Some("arguments.objective.readonly hp")
    );
}

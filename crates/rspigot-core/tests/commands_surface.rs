mod common;

use std::sync::Arc;

use common::*;
use rspigot_core::commands::{CommandOutcome, CommandSender, execute};
use rspigot_core::events::{ClientEvent, Inbound};
use rspigot_core::pack_data::registry_overlay::{self, OverlayEntry, RegistryOverlay};

fn operator_scene() -> (Scene, Player) {
    let mut s = scene(&["Alice", "Bob"]);
    s.server.handle(Inbound::Console("op Alice".to_owned()));
    s.server.deliver();
    let alice = s.players[0].clone();
    alice.link.take();
    (s, alice)
}

fn run(s: &mut Scene, alice: &Player, line: &str) -> (CommandOutcome, Vec<ClientEvent>) {
    let outcome = execute(&mut s.server, CommandSender::Player(alice.id), line);
    s.server.deliver();
    (outcome, alice.link.take())
}

fn says(events: &[ClientEvent]) -> Vec<String> {
    events
        .iter()
        .filter_map(|event| match event {
            ClientEvent::SystemChat { message, .. } => Some(message.plain_text()),
            _ => None,
        })
        .collect()
}

fn publish_dialog(id: &str, json: &str) {
    let mut overlay = RegistryOverlay::default();
    overlay.insert(
        "minecraft:dialog",
        OverlayEntry {
            id: id.to_owned(),
            json: json.to_owned(),
            data: None,
        },
    );
    registry_overlay::publish(Some(Arc::new(overlay)));
}

#[test]
fn dialog_show_opens_datapack_and_built_in_dialogs_and_clear_closes_them() {
    publish_dialog(
        "t:welcome",
        r#"{"type":"minecraft:notice","title":"Welcome","body":{"type":"minecraft:plain_message","contents":"Hello"}}"#,
    );
    let (mut s, alice) = operator_scene();
    let (outcome, events) = run(&mut s, &alice, "dialog show Alice t:welcome");
    assert_eq!(outcome, CommandOutcome::Success);
    assert!(events.iter().any(|e| matches!(
        e,
        ClientEvent::ShowDialogInline(tag)
            if matches!(&**tag, rspigot_nbt::Tag::Compound(c) if c.get_str("type") == Some("minecraft:notice"))
    )));
    assert!(says(&events)[0].starts_with("commands.dialog.show.single"));
    let (outcome, events) = run(&mut s, &alice, "dialog show Alice minecraft:quick_actions");
    assert_eq!(outcome, CommandOutcome::Success);
    assert!(events.iter().any(
        |e| matches!(e, ClientEvent::ShowDialogRegistry(id) if id == "minecraft:quick_actions")
    ));
    let (outcome, events) = run(&mut s, &alice, "dialog clear Alice");
    assert_eq!(outcome, CommandOutcome::Success);
    assert!(events.iter().any(|e| matches!(e, ClientEvent::ClearDialog)));
    assert!(says(&events)[0].starts_with("commands.dialog.clear.single"));
}

#[test]
fn dialog_show_accepts_inline_dialogs_with_any_action_and_reports_many_targets() {
    let (mut s, alice) = operator_scene();
    let (outcome, events) = run(
        &mut s,
        &alice,
        r#"dialog show @a {type:"minecraft:multi_action",title:"Pick",actions:[{label:"Go",action:{type:"minecraft:run_command",command:"say hi"}}]}"#,
    );
    assert_eq!(outcome, CommandOutcome::Success);
    assert!(
        events
            .iter()
            .any(|e| matches!(e, ClientEvent::ShowDialogInline(_)))
    );
    assert_eq!(says(&events), vec!["commands.dialog.show.multiple 2"]);
    let (outcome, events) = run(
        &mut s,
        &alice,
        r#"dialog show Alice {type:"minecraft:bogus",title:"x"}"#,
    );
    assert_eq!(outcome, CommandOutcome::Failure);
    assert!(says(&events)[0].starts_with("argument.resource_or_id.failed_to_parse"));
}

#[test]
fn dialog_show_rejects_unknown_dialogs() {
    let (mut s, alice) = operator_scene();
    let (outcome, events) = run(&mut s, &alice, "dialog show Alice t:missing_dialog");
    assert_eq!(outcome, CommandOutcome::Failure);
    assert!(says(&events)[0].starts_with("argument.resource_or_id.no_such_element"));
}

#[test]
fn publish_and_unpublish_follow_the_dedicated_server_behaviour() {
    let (mut s, alice) = operator_scene();
    let (outcome, events) = run(&mut s, &alice, "unpublish");
    assert_eq!(outcome, CommandOutcome::Failure);
    assert_eq!(says(&events), vec!["commands.unpublish.notPublished"]);
    let (outcome, events) = run(&mut s, &alice, "publish true 25566");
    assert_eq!(outcome, CommandOutcome::Failure);
    assert_eq!(
        says(&events),
        vec!["commands.publish.alreadyPublished.lan 25565"]
    );
    let (_, events) = run(&mut s, &alice, "publish true 70000");
    assert!(says(&events)[0].starts_with("argument.integer.big"));
}

#[test]
fn jfr_is_honestly_unavailable_and_perf_runs_a_profile() {
    let (mut s, alice) = operator_scene();
    let (_, events) = run(&mut s, &alice, "jfr start");
    assert_eq!(says(&events), vec!["commands.jfr.start.failed"]);
    let (_, events) = run(&mut s, &alice, "perf stop");
    assert_eq!(says(&events), vec!["commands.perf.notRunning"]);
    let (outcome, events) = run(&mut s, &alice, "perf start");
    assert_eq!(outcome, CommandOutcome::Success);
    assert_eq!(says(&events), vec!["commands.perf.started"]);
    let (_, events) = run(&mut s, &alice, "perf start");
    assert_eq!(says(&events), vec!["commands.perf.alreadyRunning"]);
    let (outcome, events) = run(&mut s, &alice, "perf stop");
    assert_eq!(outcome, CommandOutcome::Success);
    assert!(says(&events)[0].starts_with("commands.perf.stopped"));
}

#[test]
fn owner_commands_need_the_owner_level() {
    let mut s = scene(&["Bob"]);
    let bob = s.players[0].clone();
    bob.link.take();
    let (outcome, _) = run(&mut s, &bob, "unpublish");
    assert_eq!(outcome, CommandOutcome::Denied);
}

#[test]
fn named_arguments_are_validated_when_the_command_runs() {
    let (mut s, alice) = operator_scene();
    run(&mut s, &alice, "team add t");
    let (outcome, events) = run(&mut s, &alice, "team modify t color nope");
    assert_eq!(outcome, CommandOutcome::Failure);
    assert_eq!(says(&events), vec!["argument.color.invalid nope"]);
    let (outcome, events) = run(&mut s, &alice, "execute anchored head run say hi");
    assert_eq!(outcome, CommandOutcome::Failure);
    assert_eq!(says(&events), vec!["argument.anchor.invalid head"]);
}

fn waypoint_packets(events: &[ClientEvent]) -> Vec<rspigot_core::waypoints::WaypointPacket> {
    events
        .iter()
        .filter_map(|event| match event {
            ClientEvent::TrackedWaypoint(packet) => Some((**packet).clone()),
            _ => None,
        })
        .collect()
}

#[test]
fn waypoints_are_tracked_listed_and_modified() {
    use rspigot_core::waypoints::WaypointOperation;
    let (mut s, alice) = operator_scene();
    s.tick(1);
    let tracked = waypoint_packets(&alice.link.take());
    assert!(
        tracked
            .iter()
            .any(|p| p.operation == WaypointOperation::Track && p.id == s.players[1].id.0)
    );
    let (outcome, events) = run(&mut s, &alice, "waypoint list");
    assert_eq!(outcome, CommandOutcome::Success);
    assert!(says(&events)[0].starts_with("commands.waypoint.list.success 2"));
    let (outcome, events) = run(&mut s, &alice, "waypoint modify Bob color red");
    assert_eq!(outcome, CommandOutcome::Success);
    assert!(says(&events)[0].starts_with("commands.waypoint.modify.color"));
    let updates = waypoint_packets(&events);
    assert!(updates.iter().any(|p| {
        p.operation == WaypointOperation::Update
            && p.id == s.players[1].id.0
            && p.icon.color == Some(0xFF5555)
    }));
    let (_, events) = run(
        &mut s,
        &alice,
        "waypoint modify Bob style set minecraft:bowtie",
    );
    assert_eq!(says(&events), vec!["commands.waypoint.modify.style"]);
    let (_, events) = run(&mut s, &alice, "waypoint modify Bob color reset");
    assert_eq!(says(&events), vec!["commands.waypoint.modify.color.reset"]);
}

#[test]
fn the_locator_bar_gamerule_untracks_waypoints() {
    use rspigot_core::waypoints::WaypointOperation;
    let (mut s, alice) = operator_scene();
    s.tick(1);
    alice.link.take();
    run(&mut s, &alice, "gamerule locator_bar false");
    s.tick(1);
    let events = alice.link.take();
    assert!(
        waypoint_packets(&events)
            .iter()
            .any(|p| p.operation == WaypointOperation::Untrack)
    );
}

#[test]
fn post_effects_are_added_listed_and_cleared() {
    let (mut s, alice) = operator_scene();
    let (outcome, events) = run(
        &mut s,
        &alice,
        "posteffect add Alice minecraft:transparency",
    );
    assert_eq!(outcome, CommandOutcome::Success);
    assert!(events.iter().any(|e| matches!(
        e,
        ClientEvent::PostEffects(list) if list.len() == 1 && list[0].to_string() == "minecraft:transparency"
    )));
    let (outcome, _) = run(
        &mut s,
        &alice,
        "posteffect add Alice minecraft:transparency",
    );
    assert_eq!(outcome, CommandOutcome::Failure);
    let (_, events) = run(&mut s, &alice, "posteffect list Alice");
    assert!(says(&events)[0].starts_with("commands.posteffect.list.success"));
    let (outcome, events) = run(&mut s, &alice, "posteffect clear Alice");
    assert_eq!(outcome, CommandOutcome::Success);
    assert!(
        events
            .iter()
            .any(|e| matches!(e, ClientEvent::PostEffects(list) if list.is_empty()))
    );
    let (outcome, _) = run(
        &mut s,
        &alice,
        "posteffect remove Alice minecraft:transparency",
    );
    assert_eq!(outcome, CommandOutcome::Failure);
}

#[test]
fn stateful_and_tail_named_arguments_are_validated_too() {
    let (mut s, alice) = operator_scene();
    for (line, key) in [
        ("team modify ghost color red", "team.notFound ghost"),
        (
            "scoreboard objectives add t bogus_criteria",
            "argument.criteria.invalid",
        ),
        ("particle nonsense", "particle.notFound nonsense"),
        (
            "scoreboard players set Alice ghost 1",
            "arguments.objective.notFound ghost",
        ),
        ("data get entity Alice a[", "arguments.nbtpath.node.invalid"),
        (
            "item replace entity Alice bogus.slot with stone",
            "slot.unknown",
        ),
    ] {
        let (outcome, events) = run(&mut s, &alice, line);
        assert_eq!(outcome, CommandOutcome::Failure, "{line}");
        assert!(says(&events)[0].starts_with(key), "{line}: {events:?}");
    }
    let (outcome, _) = run(&mut s, &alice, r#"tellraw Alice {"text":"#);
    assert_eq!(outcome, CommandOutcome::Failure);
}

#[test]
fn compute_evaluates_number_providers_with_scores_and_storage() {
    let (mut s, alice) = operator_scene();
    let cases = [
        (
            "compute default integer 7",
            "command.compute.result.unnamed.exact 7",
        ),
        (
            r#"compute default integer {type:"minecraft:mul",left:3,right:4}"#,
            "command.compute.result.unnamed.exact 12",
        ),
        (
            "compute default float 2.5 2",
            "command.compute.result.unnamed.rounded 2.5 5",
        ),
    ];
    for (line, expected) in cases {
        let (outcome, events) = run(&mut s, &alice, line);
        assert_eq!(outcome, CommandOutcome::Success, "{line}");
        assert_eq!(says(&events), vec![expected], "{line}");
    }
    run(&mut s, &alice, "scoreboard objectives add n dummy");
    run(&mut s, &alice, "scoreboard players set Alice n 9");
    run(&mut s, &alice, "data merge storage t:s {v:4}");
    let (_, events) = run(
        &mut s,
        &alice,
        r#"compute entity Alice integer {type:"minecraft:score",target:{type:"minecraft:context",target:"this"},score:"n"}"#,
    );
    assert_eq!(
        says(&events),
        vec!["command.compute.result.unnamed.exact 9"]
    );
    let (_, events) = run(
        &mut s,
        &alice,
        r#"compute default integer {type:"minecraft:storage",storage:"t:s",path:"v"}"#,
    );
    assert_eq!(
        says(&events),
        vec!["command.compute.result.unnamed.exact 4"]
    );
    let (outcome, events) = run(
        &mut s,
        &alice,
        r#"compute default integer {type:"minecraft:environment_attribute"}"#,
    );
    assert_eq!(outcome, CommandOutcome::Failure);
    assert!(says(&events)[0].starts_with("command.compute.result.unnamed.invalid"));
}

#[test]
fn test_runs_the_built_in_instance_and_is_honest_about_what_does_not_exist() {
    let (mut s, alice) = operator_scene();
    let (outcome, events) = run(&mut s, &alice, "test run minecraft:always_pass");
    assert_eq!(outcome, CommandOutcome::Success);
    assert_eq!(
        says(&events),
        vec![
            "commands.test.run.running 1",
            "commands.test.summary 1",
            "commands.test.summary.all_required_passed"
        ]
    );
    let (outcome, events) = run(&mut s, &alice, "test run t:missing");
    assert_eq!(outcome, CommandOutcome::Failure);
    assert!(says(&events)[0].starts_with("argument.resource_selector.not_found"));
    let (_, events) = run(&mut s, &alice, "test clearall");
    assert_eq!(says(&events), vec!["commands.test.clear.error.no_tests"]);
    let (_, events) = run(&mut s, &alice, "test create t:new 3 3 3");
    assert!(says(&events)[0].starts_with("commands.test.error.structure_not_found"));
    let (_, events) = run(&mut s, &alice, "test locate *");
    assert_eq!(
        says(&events),
        vec![
            "commands.test.locate.started",
            "commands.test.locate.done 0"
        ]
    );
}

#[test]
fn fetchprofile_resolves_online_players_and_reports_unknown_ones() {
    let (mut s, alice) = operator_scene();
    let bob = s.players[1].id.0;
    let (outcome, events) = run(&mut s, &alice, "fetchprofile name bob");
    assert_eq!(outcome, CommandOutcome::Success);
    assert!(says(&events)[0].starts_with("commands.fetchprofile.name.success bob"));
    let (outcome, events) = run(&mut s, &alice, &format!("fetchprofile id {bob}"));
    assert_eq!(outcome, CommandOutcome::Success);
    assert!(says(&events)[0].starts_with("commands.fetchprofile.id.success"));
    let (outcome, events) = run(&mut s, &alice, "fetchprofile entity Bob");
    assert_eq!(outcome, CommandOutcome::Success);
    assert!(says(&events)[0].starts_with("commands.fetchprofile.entity.success"));
    let (outcome, events) = run(&mut s, &alice, "fetchprofile name Ghost");
    assert_eq!(outcome, CommandOutcome::Failure);
    assert_eq!(
        says(&events),
        vec!["commands.fetchprofile.name.failure Ghost"]
    );
}

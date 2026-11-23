use rspigot_domain::player::{GameMode, PlayerId};

use super::{CommandContext, CommandOutcome, CommandSender, execute};
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
}

#[test]
fn defaultgamemode_changes_the_configured_mode() {
    let mut rig = rig();
    assert_eq!(rig.run("defaultgamemode creative"), CommandOutcome::Success);
    assert_eq!(rig.server.default_game_mode(), GameMode::Creative);
    assert_eq!(
        rig.texts(),
        ["commands.defaultgamemode.success gameMode.creative"]
    );
    assert_eq!(rig.run("defaultgamemode nope"), CommandOutcome::Failure);
}

#[test]
fn gamerule_queries_and_sets() {
    let mut rig = rig();
    rig.texts();
    assert_eq!(rig.run("gamerule keep_inventory"), CommandOutcome::Success);
    assert_eq!(
        rig.texts(),
        ["commands.gamerule.query keep_inventory false"]
    );
    assert_eq!(
        rig.run("gamerule keep_inventory true"),
        CommandOutcome::Success
    );
    assert!(rig.server.config.rules.keep_inventory);
    assert_eq!(rig.texts(), ["commands.gamerule.set keep_inventory true"]);
    assert_eq!(
        rig.run("gamerule random_tick_speed 7"),
        CommandOutcome::Success
    );
    assert_eq!(rig.server.config.rules.random_tick_speed, 7);
    assert_eq!(
        rig.run("gamerule random_tick_speed -2"),
        CommandOutcome::Failure
    );
    assert_eq!(
        rig.run("gamerule keep_inventory maybe"),
        CommandOutcome::Failure
    );
    assert_eq!(rig.run("gamerule nothing"), CommandOutcome::Failure);
    assert_eq!(
        rig.run("gamerule advance_time false"),
        CommandOutcome::Success
    );
    assert!(!rig.server.world.level.daylight_cycle);
}

#[test]
fn experience_adds_sets_and_queries() {
    let mut rig = rig();
    rig.texts();
    assert_eq!(
        rig.run("experience add @s 5 levels"),
        CommandOutcome::Success
    );
    assert_eq!(
        rig.texts(),
        ["commands.experience.add.levels.success.single 5 Alice"]
    );
    assert_eq!(rig.run("xp query @s levels"), CommandOutcome::Success);
    assert_eq!(rig.texts(), ["commands.experience.query.levels Alice 5"]);
    assert_eq!(
        rig.run("experience set @s 2 levels"),
        CommandOutcome::Success
    );
    assert_eq!(rig.server.experience_state(rig.id), Some((2, 0)));
    assert_eq!(
        rig.run("experience set @s 3 points"),
        CommandOutcome::Success
    );
    assert_eq!(rig.server.experience_state(rig.id), Some((2, 3)));
    assert_eq!(
        rig.run("experience set @s 9999 points"),
        CommandOutcome::Failure
    );
    assert_eq!(rig.run("experience add @s 10"), CommandOutcome::Success);
    assert_eq!(rig.run("experience set @s -1"), CommandOutcome::Failure);
}

#[test]
fn tags_apply_to_players_and_mobs_and_feed_selectors() {
    let mut rig = rig();
    rig.run("summon minecraft:cow 5 70 5");
    rig.texts();
    assert_eq!(
        rig.run("tag @e[type=cow] add herd"),
        CommandOutcome::Success
    );
    assert_eq!(
        rig.texts(),
        ["commands.tag.add.success.single herd entity.minecraft.cow"]
    );
    assert_eq!(
        rig.run("tag @e[type=cow] add herd"),
        CommandOutcome::Failure
    );
    assert_eq!(rig.texts(), ["commands.tag.add.failed"]);
    assert_eq!(rig.run("tag @s add vip"), CommandOutcome::Success);
    let count = |rig: &Rig, sel: &str| {
        rig.server
            .select_entities(CommandSender::Player(rig.id), sel)
            .expect("parses")
            .len()
    };
    assert_eq!(count(&rig, "@e[tag=herd]"), 1);
    assert_eq!(count(&rig, "@a[tag=vip]"), 1);
    rig.texts();
    assert_eq!(rig.run("tag @s list"), CommandOutcome::Success);
    assert_eq!(
        rig.texts(),
        ["commands.tag.list.single.success Alice 1 vip"]
    );
    assert_eq!(rig.run("tag @s remove vip"), CommandOutcome::Success);
    assert_eq!(rig.run("tag @s remove vip"), CommandOutcome::Failure);
    assert_eq!(count(&rig, "@a[tag=vip]"), 0);
}

#[test]
fn saving_can_be_toggled() {
    let mut rig = rig();
    rig.texts();
    assert_eq!(rig.run("save-on"), CommandOutcome::Failure);
    assert_eq!(rig.run("save-off"), CommandOutcome::Success);
    assert!(!rig.server.saving_enabled);
    assert_eq!(rig.run("save-off"), CommandOutcome::Failure);
    assert_eq!(rig.run("save-on"), CommandOutcome::Success);
    assert_eq!(
        rig.texts(),
        [
            "commands.save.alreadyOn",
            "commands.save.disabled",
            "commands.save.alreadyOff",
            "commands.save.enabled"
        ]
    );
}

#[test]
fn random_stays_in_range_and_rejects_bad_ranges() {
    let mut rig = rig();
    for _ in 0..20 {
        assert_eq!(rig.run("random value 3..5"), CommandOutcome::Success);
    }
    for shown in rig.texts() {
        let value: i32 = shown
            .strip_prefix("commands.random.sample.success ")
            .expect("a sample")
            .parse()
            .expect("a number");
        assert!((3..=5).contains(&value));
    }
    assert_eq!(rig.run("random value 4..4"), CommandOutcome::Failure);
    assert_eq!(rig.run("random roll 1..6"), CommandOutcome::Success);
    assert_eq!(
        rig.run("execute store result storage t:r v int 1 run random value 1..1000"),
        CommandOutcome::Success
    );
}

#[test]
fn spawnpoint_sets_the_respawn_config() {
    let mut rig = rig();
    rig.texts();
    assert_eq!(
        rig.run("spawnpoint @s 10 70 -5 90 12"),
        CommandOutcome::Success
    );
    let respawn = rig.server.players[&rig.id]
        .respawn
        .clone()
        .expect("a respawn point");
    assert_eq!((respawn.pos.x, respawn.pos.y, respawn.pos.z), (10, 70, -5));
    assert_eq!((respawn.yaw, respawn.pitch), (90.0, 12.0));
    assert!(respawn.forced);
    assert_eq!(
        rig.texts(),
        ["commands.spawnpoint.success.single 10 70 -5 90 minecraft:overworld Alice"]
    );
    assert_eq!(rig.run("spawnpoint"), CommandOutcome::Success);
}

#[test]
fn tellraw_and_title_reach_the_player() {
    let mut rig = rig();
    rig.texts();
    assert_eq!(
        rig.run(r#"tellraw @a {"text":"hello","color":"green"}"#),
        CommandOutcome::Success
    );
    assert_eq!(rig.texts(), ["hello"]);
    assert_eq!(rig.run("tellraw @a {oops"), CommandOutcome::Failure);
    rig.link.take();
    assert_eq!(rig.run(r#"title @s title "Big""#), CommandOutcome::Success);
    assert_eq!(
        rig.run(r#"title @s actionbar {"text":"bar"}"#),
        CommandOutcome::Success
    );
    assert_eq!(rig.run("title @s times 5 20 5"), CommandOutcome::Success);
    assert_eq!(rig.run("title @s clear"), CommandOutcome::Success);
    let events = rig.link.take();
    assert!(
        events
            .iter()
            .any(|e| matches!(e, ClientEvent::TitleText(t) if t.plain_text() == "Big"))
    );
    assert!(
        events
            .iter()
            .any(|e| matches!(e, ClientEvent::SystemChat { overlay: true, .. }))
    );
    assert!(events.iter().any(|e| matches!(
        e,
        ClientEvent::TitleTimes {
            fade_in: 5,
            stay: 20,
            fade_out: 5
        }
    )));
    assert!(
        events
            .iter()
            .any(|e| matches!(e, ClientEvent::ClearTitles { reset: false }))
    );
    assert_eq!(rig.run("title @s times 1 2"), CommandOutcome::Failure);
}

#[test]
fn save_all_reports_saving_then_success_and_accepts_flush() {
    let mut rig = rig();
    rig.texts();
    assert_eq!(rig.run("save-all"), CommandOutcome::Success);
    assert_eq!(rig.run("save-all flush"), CommandOutcome::Success);
    assert_eq!(rig.run("save-all later"), CommandOutcome::Failure);
    let texts = rig.texts();
    assert_eq!(
        texts[..2],
        ["commands.save.saving", "commands.save.success"]
    );
    assert_eq!(
        texts[2..4],
        ["commands.save.saving", "commands.save.success"]
    );
    assert!(texts[4].contains("/save-all [flush]"));
}

#[test]
fn every_registry_gamerule_can_be_queried_and_set_with_its_vanilla_type() {
    let mut rig = rig();
    for name in crate::game_rule_packet::RULE_NAMES {
        let query = format!("gamerule {name}");
        assert_eq!(rig.run(&query), CommandOutcome::Success, "{name}");
        let current = rig.texts().pop().expect("a reply");
        let value = current.rsplit(' ').next().expect("a value").to_owned();
        let set = format!("gamerule {name} {value}");
        assert_eq!(rig.run(&set), CommandOutcome::Success, "{name}");
        match crate::game_rule_packet::integer_bounds(name) {
            Some((min, max)) => {
                assert!(value.parse::<i32>().is_ok(), "{name}");
                if min > i32::MIN {
                    let below = format!("gamerule {name} {}", min - 1);
                    assert_eq!(rig.run(&below), CommandOutcome::Failure, "{name}");
                }
                if max < i32::MAX {
                    let above = format!("gamerule {name} {}", max + 1);
                    assert_eq!(rig.run(&above), CommandOutcome::Failure, "{name}");
                }
            }
            None => {
                assert!(value == "true" || value == "false", "{name}");
                let bad = format!("gamerule {name} 7");
                assert_eq!(rig.run(&bad), CommandOutcome::Failure, "{name}");
            }
        }
        rig.texts();
    }
    assert_eq!(crate::game_rule_packet::RULE_NAMES.len(), 59);
}

#[test]
fn gamerule_values_follow_the_vanilla_defaults() {
    let mut rig = rig();
    for (name, expected) in [
        ("respawn_radius", "10"),
        ("max_entity_cramming", "24"),
        ("players_nether_portal_default_delay", "80"),
        ("players_nether_portal_creative_delay", "0"),
        ("max_block_modifications", "32768"),
        ("max_command_forks", "65536"),
        ("max_minecart_speed", "8"),
        ("lava_source_conversion", "false"),
        ("tnt_explosion_drop_decay", "false"),
        ("water_source_conversion", "true"),
        ("pvp", "true"),
        ("spawn_mobs", "true"),
    ] {
        rig.run(&format!("gamerule {name}"));
        assert_eq!(
            rig.texts(),
            [format!("commands.gamerule.query {name} {expected}")],
            "{name}"
        );
    }
    rig.run("gamerule pvp false");
    assert!(!rig.server.config.pvp);
    rig.run("gamerule spawn_monsters false");
    assert!(!rig.server.world.level.spawn_monsters);
}

#[test]
fn the_command_feedback_rule_hides_successes_from_a_player_but_not_failures() {
    let mut rig = rig();
    assert_eq!(
        rig.run("gamerule send_command_feedback false"),
        CommandOutcome::Success
    );
    assert!(rig.texts().is_empty());
    assert_eq!(
        rig.run("gamerule keep_inventory true"),
        CommandOutcome::Success
    );
    assert!(rig.server.config.rules.keep_inventory);
    assert!(rig.texts().is_empty());
    assert_eq!(rig.run("gamerule nothing"), CommandOutcome::Failure);
    assert_eq!(rig.texts().len(), 1);
}

#[test]
fn the_unknown_command_message_replaces_the_vanilla_text() {
    let mut rig = rig();
    rig.texts();
    assert_eq!(rig.run("definitelynotacommand"), CommandOutcome::Unknown);
    assert_eq!(rig.texts(), ["Unknown command. Type \"/help\" for help."]);
    rig.server.config.messages.unknown_command = "Nope, not here.".to_owned();
    assert_eq!(rig.run("definitelynotacommand"), CommandOutcome::Unknown);
    assert_eq!(rig.texts(), ["Nope, not here."]);
}

#[test]
fn restart_stops_the_server_and_marks_it_as_restarting() {
    let mut rig = rig();
    assert_eq!(rig.run("restart"), CommandOutcome::Success);
    assert!(!rig.server.running);
    assert!(rig.server.restarting);
}

#[test]
fn restart_spigot_and_timings_need_an_owner() {
    let mut rig = rig();
    if let Some(player) = rig.server.players.get_mut(&rig.id) {
        player.op_level = 3;
    }
    for line in ["restart", "spigot reload", "timings on"] {
        assert_eq!(rig.run(line), CommandOutcome::Denied, "{line}");
    }
    assert!(rig.server.running);
}

#[test]
fn spigot_reload_asks_for_a_reload_and_says_so() {
    let mut rig = rig();
    rig.texts();
    assert_eq!(rig.run("spigot reload"), CommandOutcome::Failure);
    let (sink, requests) = crossbeam_channel::unbounded();
    rig.server =
        std::mem::replace(&mut rig.server, crate::end_tests::arrive().0).with_reload_sink(sink);
    rig.texts();
    assert_eq!(rig.run("spigot reload"), CommandOutcome::Success);
    let texts = rig.texts();
    assert_eq!(texts.last().map(String::as_str), Some("Reload complete."));
    assert!(!requests.try_recv().expect("a request").whitelist_only);
    assert_eq!(rig.run("spigot"), CommandOutcome::Failure);
}

#[test]
fn timings_replies_follow_bukkit_and_a_report_needs_timings_on() {
    let mut rig = rig();
    rig.texts();
    assert_eq!(rig.run("timings report"), CommandOutcome::Success);
    assert_eq!(rig.texts(), ["Please enable timings by typing /timings on"]);
    rig.run("timings on");
    assert_eq!(rig.texts(), ["Enabled Timings & Reset"]);
    assert!(rig.server.timings.enabled);
    rig.run("timings verbon");
    assert_eq!(rig.texts(), ["Enabled Verbose Timings"]);
    rig.run("timings verboff");
    assert_eq!(rig.texts(), ["Disabled Verbose Timings"]);
    rig.run("timings reset");
    assert_eq!(
        rig.texts(),
        ["Timings reset. Please wait 5-10 minutes before using /timings report."]
    );
    let (sink, reports) = crossbeam_channel::unbounded();
    rig.server.timings_sink = Some(sink);
    assert_eq!(rig.run("timings paste"), CommandOutcome::Success);
    let file = reports.try_recv().expect("a report").file;
    assert_eq!(
        rig.texts(),
        [format!(
            "Timings report written to timings/{file} (nothing is uploaded)"
        )]
    );
    rig.run("timings off");
    assert_eq!(rig.texts(), ["Disabled Timings"]);
    assert_eq!(rig.run("timings nonsense"), CommandOutcome::Failure);
}

#[test]
fn the_client_tree_lists_the_bukkit_commands_for_owners_only() {
    let names = |level: u8| -> Vec<String> {
        let tree = crate::commands::build_tree(level);
        tree.nodes[tree.root]
            .children
            .iter()
            .filter_map(|child| match &tree.nodes[*child].kind {
                crate::commands::CommandNodeKind::Literal(name) => Some(name.clone()),
                _ => None,
            })
            .collect()
    };
    for command in ["restart", "spigot", "timings"] {
        assert!(names(4).iter().any(|name| name == command), "{command}");
        assert!(!names(3).iter().any(|name| name == command), "{command}");
    }
}

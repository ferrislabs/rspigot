use std::sync::Arc;

use parking_lot::Mutex;
use rspigot_domain::player::PlayerId;
use rspigot_domain::{Identifier, Vec3};

use super::source::DataTarget;
use super::{CommandContext, CommandOutcome, CommandSender, execute};
use crate::end_tests::{Tap, arrive};
use crate::events::ClientEvent;
use crate::functions::MAX_NESTING;
use crate::ports::{DatapackError, DatapackSource, PackContents, PackFile, PackMeta};
use crate::server::Server;

#[derive(Default)]
struct MemoryPacks {
    packs: Mutex<Vec<PackContents>>,
}

impl DatapackSource for MemoryPacks {
    fn read_packs(&self) -> Result<Vec<PackContents>, DatapackError> {
        Ok(self.packs.lock().clone())
    }
}

fn file(id: &str, text: &str) -> PackFile {
    PackFile {
        id: Identifier::parse(id).unwrap(),
        text: text.to_owned(),
    }
}

fn pack(name: &str, functions: &[(&str, &str)], tags: &[(&str, &str)]) -> PackContents {
    PackContents {
        name: name.to_owned(),
        meta: PackMeta::default(),
        functions: functions.iter().map(|(id, text)| file(id, text)).collect(),
        function_tags: tags.iter().map(|(id, text)| file(id, text)).collect(),
        ..PackContents::default()
    }
}

struct Rig {
    server: Server<Tap>,
    id: PlayerId,
    link: Tap,
    source: Arc<MemoryPacks>,
}

fn rig(packs: Vec<PackContents>) -> Rig {
    let (mut server, id, link) = arrive();
    if let Some(player) = server.players.get_mut(&id) {
        player.operator = true;
    }
    let source = Arc::new(MemoryPacks {
        packs: Mutex::new(packs),
    });
    let mut server = server.with_datapacks(Arc::clone(&source));
    execute(
        &mut server,
        CommandSender::Player(id),
        "scoreboard objectives add n dummy",
    );
    server.start(std::time::Duration::from_secs(10));
    server.deliver();
    let rig = Rig {
        server,
        id,
        link,
        source,
    };
    rig.texts();
    rig
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

    fn score(&self, holder: &str) -> Option<i32> {
        self.server
            .scoreboard()
            .score(holder, "n")
            .map(|score| score.value)
    }

    fn log(&self) -> Vec<String> {
        let data = self
            .server
            .command_data(&DataTarget::Storage(Identifier::parse("t:log").unwrap()))
            .unwrap_or_default();
        data.get_list("items")
            .unwrap_or_default()
            .iter()
            .filter_map(|tag| tag.as_str().map(str::to_owned))
            .collect()
    }

    fn count(&self, selector: &str) -> usize {
        self.server
            .select_entities(CommandSender::Player(self.id), selector)
            .expect("the selector parses")
            .len()
    }

    fn write(&self, packs: Vec<PackContents>) {
        *self.source.packs.lock() = packs;
    }
}

const LOG: &str = "data modify storage t:log items append value";

#[test]
fn a_function_runs_its_commands_in_order_and_says_so_once() {
    let text = format!("# comment\n{LOG} \"one\"\n\n{LOG} \"two\"\n{LOG} \"three\"\n");
    let mut rig = rig(vec![pack("a", &[("t:order", &text)], &[])]);
    assert_eq!(rig.run("function t:order"), CommandOutcome::Success);
    assert_eq!(rig.log(), ["one", "two", "three"]);
    assert_eq!(rig.texts(), ["commands.function.scheduled.single t:order"]);
}

#[test]
fn unknown_functions_and_tags_are_refused() {
    let mut rig = rig(Vec::new());
    assert_eq!(rig.run("function t:nope"), CommandOutcome::Failure);
    assert_eq!(rig.run("function #t:nope"), CommandOutcome::Failure);
    assert_eq!(
        rig.texts(),
        [
            "arguments.function.unknown t:nope",
            "arguments.function.tag.unknown t:nope"
        ]
    );
    assert!(
        !rig.server
            .function_exists(&Identifier::parse("t:nope").unwrap())
    );
}

#[test]
fn execute_as_keeps_the_executor_and_the_position() {
    let functions = [
        ("t:mark", "tag @s add marked\nsummon minecraft:pig ~ ~ ~"),
        ("t:none", "say unused"),
    ];
    let mut rig = rig(vec![pack("a", &functions, &[])]);
    rig.run("summon minecraft:cow 5 70 5");
    rig.run("summon minecraft:cow 9 70 9");
    let outcome = rig.run("execute as @e[type=cow] at @s run function t:mark");
    assert_eq!(outcome, CommandOutcome::Success);
    assert_eq!(rig.count("@e[type=cow,tag=marked]"), 2);
    assert_eq!(rig.count("@e[type=pig,x=5,y=70,z=5,distance=..1]"), 1);
    assert_eq!(rig.count("@e[type=pig,x=9,y=70,z=9,distance=..1]"), 1);
}

#[test]
fn return_value_stops_the_function_and_feeds_execute_store() {
    let functions = [
        ("t:seven", "return 7\nscoreboard players set after n 1"),
        ("t:fail", "return fail\nscoreboard players set after n 1"),
        (
            "t:run",
            "scoreboard players set src n 42\nreturn run scoreboard players get src n\nscoreboard players set after n 1",
        ),
        (
            "t:conditional",
            "execute if score src n matches 1.. run return 5\nreturn 9",
        ),
        (
            "t:outer",
            "function t:seven\nscoreboard players set outer n 1",
        ),
    ];
    let mut rig = rig(vec![pack("a", &functions, &[])]);
    rig.run("execute store result score r n run function t:seven");
    assert_eq!(rig.score("r"), Some(7));
    assert_eq!(rig.score("after"), None);
    let outcome = rig.run("execute store success score s n run function t:fail");
    assert_eq!(outcome, CommandOutcome::Failure);
    assert_eq!(rig.score("s"), Some(0));
    rig.run("execute store result score r n run function t:run");
    assert_eq!(rig.score("r"), Some(42));
    assert_eq!(rig.score("after"), None);
    rig.run("execute store result score r n run function t:conditional");
    assert_eq!(rig.score("r"), Some(5));
    rig.run("scoreboard players set src n 0");
    rig.run("execute store result score r n run function t:conditional");
    assert_eq!(rig.score("r"), Some(9));
    rig.run("function t:outer");
    assert_eq!(rig.score("outer"), Some(1));
    rig.texts();
    rig.run("function t:seven");
    assert_eq!(
        rig.texts(),
        [
            "commands.function.scheduled.single t:seven",
            "commands.function.result t:seven 7"
        ]
    );
}

#[test]
fn execute_if_function_passes_on_a_nonzero_return() {
    let functions = [("t:yes", "return 1"), ("t:no", "return 0")];
    let mut rig = rig(vec![pack("a", &functions, &[])]);
    assert_eq!(
        rig.run("execute if function t:yes run scoreboard players set hit n 1"),
        CommandOutcome::Success
    );
    assert_eq!(rig.score("hit"), Some(1));
    assert_eq!(
        rig.run("execute if function t:no run scoreboard players set miss n 1"),
        CommandOutcome::Failure
    );
    assert_eq!(rig.score("miss"), None);
}

#[test]
fn recursion_stops_at_the_nesting_limit() {
    let functions = [("t:loop", "scoreboard players add c n 1\nfunction t:loop")];
    let mut rig = rig(vec![pack("a", &functions, &[])]);
    rig.run("function t:loop");
    assert_eq!(rig.score("c"), Some(MAX_NESTING as i32));
    rig.run("scoreboard players set c n 0");
    rig.run("function t:loop");
    assert_eq!(rig.score("c"), Some(MAX_NESTING as i32));
}

#[test]
fn the_command_chain_limit_stops_the_whole_run() {
    let body = "scoreboard players add c n 1\n".repeat(150);
    let functions = [("t:long", body.as_str())];
    let mut rig = rig(vec![pack("a", &functions, &[])]);
    rig.run("function t:long");
    assert_eq!(rig.score("c"), Some(150));
    assert_eq!(
        rig.run("gamerule max_command_sequence_length 100"),
        CommandOutcome::Success
    );
    rig.run("scoreboard players set c n 0");
    rig.run("function t:long");
    assert_eq!(rig.score("c"), Some(100));
    rig.run("scoreboard players set c n 0");
    rig.run("execute as @a run function t:long");
    assert_eq!(rig.score("c"), Some(100));
}

#[test]
fn functions_run_with_the_function_permission_cap() {
    let functions = [("t:stop", "stop")];
    let mut rig = rig(vec![pack("a", &functions, &[])]);
    rig.run("function t:stop");
    assert!(rig.server.running);
}

#[test]
fn tags_run_every_member_in_order() {
    let functions = [
        ("t:a", &*format!("{LOG} \"a\"")),
        ("t:b", &*format!("{LOG} \"b\"")),
    ];
    let tags = [("t:both", r#"{"values":["t:a","t:b"]}"#)];
    let mut rig = rig(vec![pack("a", &functions, &tags)]);
    assert_eq!(rig.run("function #t:both"), CommandOutcome::Success);
    assert_eq!(rig.log(), ["a", "b"]);
    assert_eq!(
        rig.texts(),
        ["commands.function.scheduled.multiple t:a, t:b"]
    );
}

#[test]
fn macros_take_a_compound_or_a_data_source() {
    let functions = [(
        "t:macro",
        "$data modify storage t:log items append value \"$(who)\"",
    )];
    let mut rig = rig(vec![pack("a", &functions, &[])]);
    assert_eq!(
        rig.run("function t:macro {who:\"Alex\"}"),
        CommandOutcome::Success
    );
    rig.run("data merge storage t:args {who:\"Sam\"}");
    assert_eq!(
        rig.run("function t:macro with storage t:args"),
        CommandOutcome::Success
    );
    assert_eq!(rig.log(), ["Alex", "Sam"]);
    rig.texts();
    assert_eq!(rig.run("function t:macro"), CommandOutcome::Failure);
    assert_eq!(
        rig.run("function t:macro {other:1}"),
        CommandOutcome::Failure
    );
    assert_eq!(
        rig.texts(),
        [
            "commands.function.scheduled.single t:macro",
            "commands.function.instantiationFailure t:macro commands.function.error.missing_arguments t:macro",
            "commands.function.scheduled.single t:macro",
            "commands.function.instantiationFailure t:macro commands.function.error.missing_argument t:macro who"
        ]
    );
}

#[test]
fn load_runs_at_startup_and_after_reload_and_tick_every_tick() {
    let functions = [
        ("t:boot", "scoreboard players add boots n 1"),
        ("t:beat", "scoreboard players add beats n 1"),
    ];
    let tags = [
        ("minecraft:load", r#"{"values":["t:boot"]}"#),
        ("minecraft:tick", r#"{"values":["t:beat"]}"#),
    ];
    let mut rig = rig(vec![pack("a", &functions, &tags)]);
    assert_eq!(rig.score("boots"), Some(1));
    assert_eq!(rig.score("beats"), None);
    rig.server.tick();
    rig.server.tick();
    rig.server.tick();
    assert_eq!(rig.score("beats"), Some(3));
    assert_eq!(rig.run("reload"), CommandOutcome::Success);
    assert_eq!(rig.score("boots"), Some(2));
    assert!(rig.texts().iter().any(|text| text == "Reloaded!"));
}

#[test]
fn reload_picks_up_a_new_function_and_keeps_scheduled_events() {
    let mut rig = rig(vec![pack("a", &[("t:old", "say old")], &[])]);
    assert!(
        !rig.server
            .function_exists(&Identifier::parse("t:fresh").unwrap())
    );
    rig.run("schedule function t:old 100t");
    rig.write(vec![pack(
        "a",
        &[("t:old", "say old"), ("t:fresh", "say fresh")],
        &[],
    )]);
    rig.run("reload");
    assert!(
        rig.server
            .function_exists(&Identifier::parse("t:fresh").unwrap())
    );
    assert_eq!(rig.server.functions.scheduled.len(), 1);
}

#[test]
fn schedule_runs_after_the_delay_and_replace_or_append_decide_duplicates() {
    let functions = [("t:later", "scoreboard players add later n 1")];
    let mut rig = rig(vec![pack("a", &functions, &[])]);
    assert_eq!(
        rig.run("schedule function t:later 3t"),
        CommandOutcome::Success
    );
    let now = rig.server.world.level.game_time;
    assert_eq!(
        rig.texts(),
        [format!(
            "commands.schedule.created.function t:later 3 {}",
            now + 3
        )]
    );
    rig.server.tick();
    rig.server.tick();
    assert_eq!(rig.score("later"), None);
    rig.server.tick();
    assert_eq!(rig.score("later"), Some(1));
    rig.server.tick();
    assert_eq!(rig.score("later"), Some(1));
    rig.run("schedule function t:later 2t");
    rig.run("schedule function t:later 4t replace");
    assert_eq!(rig.server.functions.scheduled.len(), 1);
    rig.run("schedule function t:later 2t append");
    assert_eq!(rig.server.functions.scheduled.len(), 2);
    for _ in 0..4 {
        rig.server.tick();
    }
    assert_eq!(rig.score("later"), Some(3));
}

#[test]
fn schedule_clear_and_its_refusals() {
    let functions = [("t:later", "say x"), ("t:macro", "$say $(a)")];
    let mut rig = rig(vec![pack("a", &functions, &[])]);
    rig.run("schedule function t:later 10t");
    rig.run("schedule function t:later 20t append");
    rig.texts();
    assert_eq!(rig.run("schedule clear t:later"), CommandOutcome::Success);
    assert_eq!(rig.texts(), ["commands.schedule.cleared.success 2 t:later"]);
    assert_eq!(rig.run("schedule clear t:later"), CommandOutcome::Failure);
    assert_eq!(
        rig.run("schedule function t:later 0t"),
        CommandOutcome::Failure
    );
    assert_eq!(
        rig.run("schedule function t:macro 5t"),
        CommandOutcome::Failure
    );
    assert_eq!(
        rig.texts(),
        [
            "commands.schedule.cleared.failure t:later",
            "commands.schedule.same_tick",
            "commands.schedule.macro"
        ]
    );
}

#[test]
fn a_scheduled_event_survives_a_save_round_trip() {
    let mut rig = rig(vec![pack("a", &[("t:later", "say x")], &[])]);
    rig.run("schedule function t:later 50t");
    rig.server.save_scheduled_events();
    let saved = rig.server.world.level.scheduled_events.clone().unwrap();
    let restored = crate::functions::ScheduledEvents::from_nbt(&saved);
    assert_eq!(restored.events(), rig.server.functions.scheduled.events());
}

#[test]
fn datapack_commands_change_the_registry_and_the_level_lists() {
    let packs = vec![
        pack("a", &[("t:one", "scoreboard players set one n 1")], &[]),
        pack("b", &[("t:two", "scoreboard players set two n 1")], &[]),
    ];
    let mut rig = rig(packs);
    let lists = |rig: &Rig| rig.server.world.level.data_packs.clone().unwrap();
    assert_eq!(lists(&rig).enabled, ["vanilla", "file/a", "file/b"]);
    assert_eq!(
        rig.run("datapack disable \"file/a\""),
        CommandOutcome::Success
    );
    assert!(
        !rig.server
            .function_exists(&Identifier::parse("t:one").unwrap())
    );
    assert!(
        rig.server
            .function_exists(&Identifier::parse("t:two").unwrap())
    );
    assert_eq!(lists(&rig).enabled, ["vanilla", "file/b"]);
    assert_eq!(lists(&rig).disabled, ["file/a"]);
    assert_eq!(rig.run("datapack disable file/a"), CommandOutcome::Failure);
    assert_eq!(rig.run("datapack enable file/zzz"), CommandOutcome::Failure);
    rig.texts();
    assert_eq!(
        rig.run("datapack enable \"file/a\" before \"file/b\""),
        CommandOutcome::Success
    );
    assert_eq!(lists(&rig).enabled, ["vanilla", "file/a", "file/b"]);
    assert_eq!(rig.run("datapack enable file/a"), CommandOutcome::Failure);
    rig.texts();
    rig.run("datapack list enabled");
    rig.run("datapack disable file/b");
    rig.run("datapack list available");
    let texts = rig.texts();
    assert!(
        texts
            .iter()
            .any(|t| t.starts_with("commands.datapack.list.enabled.success 3"))
    );
    assert!(
        texts
            .iter()
            .any(|t| t.starts_with("commands.datapack.list.available.success 1"))
    );
}

#[test]
fn tab_completion_offers_functions_and_packs() {
    use crate::events::{Inbound, PlayerInput};
    let packs = vec![pack(
        "a",
        &[("t:one", "say x")],
        &[("t:grp", r#"{"values":["t:one"]}"#)],
    )];
    let mut rig = rig(packs);
    let complete = |rig: &mut Rig, text: &str| {
        rig.server.handle(Inbound::Input {
            player: rig.id,
            input: PlayerInput::CommandSuggestion {
                id: 3,
                text: text.to_owned(),
            },
        });
        rig.server.deliver();
        rig.link
            .take()
            .into_iter()
            .find_map(|event| match event {
                ClientEvent::CommandSuggestions { matches, .. } => Some(matches),
                _ => None,
            })
            .unwrap_or_default()
    };
    assert_eq!(complete(&mut rig, "/function t:"), ["t:one"]);
    assert_eq!(complete(&mut rig, "/function #t"), ["#t:grp"]);
    assert_eq!(complete(&mut rig, "/execute if function t:o"), ["t:one"]);
    rig.run("datapack disable file/a");
    assert_eq!(complete(&mut rig, "/datapack enable "), ["\"file/a\""]);
}

#[test]
fn the_position_of_a_server_run_function_is_the_world_spawn() {
    let functions = [("t:where", "summon minecraft:pig ~ ~ ~")];
    let tags = [("minecraft:load", r#"{"values":["t:where"]}"#)];
    let rig = rig(vec![pack("a", &functions, &tags)]);
    let spawn: Vec3 = rig.server.world.spawn;
    let near = format!(
        "@e[type=pig,x={},y={},z={},distance=..1]",
        spawn.x, spawn.y, spawn.z
    );
    assert_eq!(rig.count(&near), 1);
}

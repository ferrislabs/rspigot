mod common;

use std::sync::{Arc, Mutex};
use std::time::Duration;

use common::{Link, Player, calm_server, join, texts};
use rspigot_core::events::Inbound;
use rspigot_core::ports::{DatapackError, DatapackSource, PackContents, PackFile, PackMeta};
use rspigot_core::server::Server;
use rspigot_domain::Identifier;

#[derive(Default)]
struct Packs(Mutex<Vec<PackContents>>);

impl DatapackSource for Packs {
    fn read_packs(&self) -> Result<Vec<PackContents>, DatapackError> {
        Ok(self.0.lock().expect("packs").clone())
    }
}

fn file(id: &str, text: &str) -> PackFile {
    PackFile {
        id: Identifier::parse(id).expect("id"),
        text: text.to_owned(),
    }
}

fn pack(functions: &[(&str, &str)], tags: &[(&str, &str)]) -> PackContents {
    PackContents {
        name: "main".to_owned(),
        meta: PackMeta::default(),
        functions: functions.iter().map(|(id, text)| file(id, text)).collect(),
        function_tags: tags.iter().map(|(id, text)| file(id, text)).collect(),
        ..PackContents::default()
    }
}

struct Scene {
    server: Server<Link>,
    packs: Arc<Packs>,
    alice: Player,
}

fn scene(initial: PackContents) -> Scene {
    let packs = Arc::new(Packs(Mutex::new(vec![initial])));
    let mut server = calm_server().with_datapacks(Arc::clone(&packs));
    server.handle(Inbound::Console(
        "scoreboard objectives add n dummy".to_owned(),
    ));
    server.start(Duration::from_secs(10));
    let alice = join(&mut server, "Alice");
    server.deliver();
    alice.link.take();
    Scene {
        server,
        packs,
        alice,
    }
}

impl Scene {
    fn console(&mut self, line: &str) {
        self.server.handle(Inbound::Console(line.to_owned()));
        self.server.deliver();
    }

    fn says_when(&mut self, score: &str, value: u32) -> bool {
        let line = format!(
            "execute if score {score} n matches {value} run tellraw Alice {{\"text\":\"yes\"}}"
        );
        self.console(&line);
        texts(&self.alice).iter().any(|(text, _)| text == "yes")
    }
}

const LOAD: &str = r#"{"values":["t:boot"]}"#;
const TICK: &str = r#"{"values":["t:beat"]}"#;

fn counting() -> PackContents {
    pack(
        &[
            ("t:boot", "scoreboard players add boots n 1"),
            ("t:beat", "scoreboard players add beats n 1"),
        ],
        &[("minecraft:load", LOAD), ("minecraft:tick", TICK)],
    )
}

#[test]
fn load_runs_at_startup_and_after_reload_and_tick_runs_every_tick() {
    let mut s = scene(counting());
    assert!(s.says_when("boots", 1));
    for _ in 0..3 {
        s.server.tick();
    }
    assert!(s.says_when("beats", 3));
    s.console("reload");
    assert!(s.says_when("boots", 2));
    s.server.tick();
    assert!(s.says_when("beats", 4));
}

#[test]
fn reload_reads_a_function_written_after_startup() {
    let mut s = scene(counting());
    s.console("function t:fresh");
    assert!(!s.says_when("fresh", 1));
    *s.packs.0.lock().expect("packs") = vec![pack(
        &[
            ("t:fresh", "scoreboard players set fresh n 1"),
            ("t:boot", "scoreboard players add boots n 1"),
        ],
        &[("minecraft:load", LOAD)],
    )];
    s.console("reload");
    s.console("function t:fresh");
    assert!(s.says_when("fresh", 1));
}

#[test]
fn schedule_runs_a_function_after_its_delay() {
    let mut s = scene(pack(
        &[("t:later", "scoreboard players set later n 1")],
        &[],
    ));
    s.console("schedule function t:later 5t");
    for _ in 0..4 {
        s.server.tick();
    }
    assert!(!s.says_when("later", 1));
    s.server.tick();
    assert!(s.says_when("later", 1));
}

#[test]
fn a_function_run_by_a_player_does_not_leak_its_inner_feedback() {
    let mut s = scene(pack(&[("t:quiet", "scoreboard players set q n 4")], &[]));
    s.console("op Alice");
    s.alice.link.take();
    s.server.handle(Inbound::Input {
        player: s.alice.id,
        input: rspigot_core::events::PlayerInput::Command("function t:quiet".to_owned()),
    });
    s.server.deliver();
    let heard = texts(&s.alice);
    assert!(
        heard
            .iter()
            .all(|(text, _)| !text.contains("commands.scoreboard")),
        "{heard:?}"
    );
}

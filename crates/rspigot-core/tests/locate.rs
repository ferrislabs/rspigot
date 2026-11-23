mod common;

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;

use common::{Floor, Link, NoStore, Player, join, leave_of};
use rspigot_core::config::{GameConfig, LevelData};
use rspigot_core::events::{ClientEvent, Inbound, PlayerInput};
use rspigot_core::ports::{ChunkGenerator, PlaceFailure, PlaceRequest, PlacedBlock};
use rspigot_core::server::Server;
use rspigot_core::status::StatusBoard;
use rspigot_domain::chunk::ChunkColumn;
use rspigot_domain::text::{ClickEvent, Component, Content};
use rspigot_domain::world::Dimension;
use rspigot_domain::{BlockPos, BlockStateId, ChunkPos, Vec3};

struct Gate {
    open: Mutex<bool>,
    signal: Condvar,
}

impl Gate {
    fn new(open: bool) -> Arc<Self> {
        Arc::new(Self {
            open: Mutex::new(open),
            signal: Condvar::new(),
        })
    }

    fn release(&self) {
        *self.open.lock().unwrap() = true;
        self.signal.notify_all();
    }

    fn wait(&self) {
        let mut open = self.open.lock().unwrap();
        while !*open {
            open = self.signal.wait(open).unwrap();
        }
    }
}

struct Oracle {
    gate: Arc<Gate>,
    structure: Option<(String, Vec3)>,
    biome: Option<(String, Vec3)>,
    asked: Arc<AtomicUsize>,
    asked_ids: Arc<Mutex<Vec<String>>>,
}

impl ChunkGenerator for Oracle {
    fn generate(&self, dimension: &Dimension, pos: ChunkPos) -> ChunkColumn {
        Floor.generate(dimension, pos)
    }

    fn spawn_point(&self, dimension: &Dimension) -> Vec3 {
        Floor.spawn_point(dimension)
    }

    fn nearest_structure(
        &self,
        _: &Dimension,
        _: Vec3,
        structure_id: &str,
    ) -> Option<(String, Vec3)> {
        self.asked.fetch_add(1, Ordering::SeqCst);
        self.asked_ids.lock().unwrap().push(structure_id.to_owned());
        self.gate.wait();
        self.structure.clone()
    }

    fn nearest_biome(&self, _: &Dimension, _: Vec3, _: &[String]) -> Option<(String, Vec3)> {
        self.asked.fetch_add(1, Ordering::SeqCst);
        self.gate.wait();
        self.biome.clone()
    }

    fn place(
        &self,
        _: &Dimension,
        request: &PlaceRequest,
    ) -> Result<Vec<PlacedBlock>, PlaceFailure> {
        self.gate.wait();
        match request {
            PlaceRequest::Template { id, pos, .. } if id == "t:hut" => Ok(vec![
                PlacedBlock {
                    pos: *pos,
                    state: BlockStateId(1),
                },
                PlacedBlock {
                    pos: BlockPos::new(pos.x + 1, pos.y, pos.z),
                    state: BlockStateId(1),
                },
            ]),
            PlaceRequest::Template { .. } => Err(PlaceFailure::Unknown),
            _ => Err(PlaceFailure::Failed),
        }
    }
}

struct Scene {
    server: Server<Link>,
    alice: Player,
    gate: Arc<Gate>,
    asked: Arc<AtomicUsize>,
    asked_ids: Arc<Mutex<Vec<String>>>,
}

fn scene(
    open: bool,
    structure: Option<(&str, Vec3)>,
    biome: Option<(&str, Vec3)>,
    operator: bool,
) -> Scene {
    let gate = Gate::new(open);
    let asked = Arc::new(AtomicUsize::new(0));
    let asked_ids = Arc::new(Mutex::new(Vec::new()));
    let oracle = Oracle {
        gate: Arc::clone(&gate),
        structure: structure.map(|(id, at)| (id.to_owned(), at)),
        biome: biome.map(|(id, at)| (id.to_owned(), at)),
        asked: Arc::clone(&asked),
        asked_ids: Arc::clone(&asked_ids),
    };
    let config = GameConfig {
        view_distance: 3,
        chunk_workers: 2,
        spawn_protection: 0,
        ..GameConfig::default()
    };
    let data = LevelData {
        spawn_mobs: false,
        day_time: 18_000,
        ..LevelData::default()
    };
    let mut server = Server::new(
        config,
        data,
        Arc::new(NoStore),
        Arc::new(oracle),
        StatusBoard::new(20),
    );
    server.wait_for_spawn(Duration::from_secs(10));
    server.start(Duration::from_secs(10));
    let alice = join(&mut server, "Alice");
    server.deliver();
    if operator {
        server.handle(Inbound::Console("op Alice".to_owned()));
        server.deliver();
    }
    alice.link.take();
    Scene {
        server,
        alice,
        gate,
        asked,
        asked_ids,
    }
}

impl Scene {
    fn command(&mut self, line: &str) {
        self.server.handle(Inbound::Input {
            player: self.alice.id,
            input: PlayerInput::Command(line.to_owned()),
        });
        self.server.deliver();
    }

    fn console(&mut self, line: &str) {
        self.server.handle(Inbound::Console(line.to_owned()));
        self.server.deliver();
    }

    fn messages(&self) -> Vec<Component> {
        self.alice
            .link
            .take()
            .into_iter()
            .filter_map(|event| match event {
                ClientEvent::SystemChat { message, .. } => Some(message),
                _ => None,
            })
            .collect()
    }

    fn tick_until(&mut self, mut done: impl FnMut(&mut Self) -> bool) -> bool {
        for _ in 0..400 {
            self.server.tick();
            self.server.deliver();
            if done(self) {
                return true;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        false
    }

    fn is_stone(&mut self, x: i32, y: i32, z: i32) -> bool {
        self.console(&format!(
            "execute if block {x} {y} {z} minecraft:stone run tellraw Alice {{\"text\":\"yes\"}}"
        ));
        self.messages().iter().any(|m| m.plain_text() == "yes")
    }
}

fn key_of(message: &Component) -> Option<&str> {
    match &message.content {
        Content::Translatable { key, .. } => Some(key),
        _ => None,
    }
}

fn args_of(message: &Component) -> &[Component] {
    match &message.content {
        Content::Translatable { args, .. } => args,
        _ => &[],
    }
}

fn found(messages: &[Component], key: &str) -> Option<Component> {
    messages.iter().find(|m| key_of(m) == Some(key)).cloned()
}

#[test]
fn locating_a_stronghold_never_blocks_the_tick_and_answers_on_a_later_one() {
    let mut s = scene(
        false,
        Some(("minecraft:stronghold", Vec3::new(1200.0, 0.0, -1600.0))),
        None,
        true,
    );
    let started = std::time::Instant::now();
    s.command("locate structure minecraft:stronghold");
    for _ in 0..5 {
        s.server.tick();
        s.server.deliver();
    }
    assert!(
        started.elapsed() < Duration::from_secs(5),
        "the tick waited"
    );
    assert!(found(&s.messages(), "commands.locate.structure.success").is_none());
    s.gate.release();
    let mut reply = None;
    assert!(s.tick_until(|scene| {
        reply = found(&scene.messages(), "commands.locate.structure.success");
        reply.is_some()
    }));
    let reply = reply.unwrap();
    let args = args_of(&reply);
    assert_eq!(args[0].plain_text(), "minecraft:stronghold");
    let coordinates = &args[1];
    assert_eq!(
        coordinates.style.click.as_deref(),
        Some(&ClickEvent::RunCommand("/tp @s 1200 ~ -1600".to_owned()))
    );
    assert!(coordinates.style.hover.is_some());
    let distance: i32 = args[2].plain_text().parse().unwrap();
    assert!((1900..=2100).contains(&distance), "{distance}");
}

#[test]
fn a_structure_tag_asks_the_generator_for_each_member() {
    let mut s = scene(true, None, None, true);
    s.command("locate structure #minecraft:village");
    assert!(s.tick_until(|scene| {
        found(&scene.messages(), "commands.locate.structure.not_found").is_some()
    }));
    let asked = s.asked_ids.lock().unwrap().clone();
    assert_eq!(asked.len(), 5);
    assert!(asked.contains(&"minecraft:village_taiga".to_owned()));
}

#[test]
fn a_missing_structure_reports_not_found() {
    let mut s = scene(true, None, None, true);
    s.command("locate structure minecraft:igloo");
    let mut reply = None;
    assert!(s.tick_until(|scene| {
        reply = found(&scene.messages(), "commands.locate.structure.not_found");
        reply.is_some()
    }));
    assert_eq!(args_of(&reply.unwrap())[0].plain_text(), "minecraft:igloo");
}

#[test]
fn locating_a_biome_shows_the_height_and_the_three_dimensional_distance() {
    let mut s = scene(
        true,
        None,
        Some(("minecraft:desert", Vec3::new(0.0, 64.0, 32.0))),
        true,
    );
    s.command("locate biome minecraft:desert");
    let mut reply = None;
    assert!(s.tick_until(|scene| {
        reply = found(&scene.messages(), "commands.locate.biome.success");
        reply.is_some()
    }));
    let reply = reply.unwrap();
    let args = args_of(&reply);
    assert_eq!(args[0].plain_text(), "minecraft:desert");
    assert_eq!(
        args[1].style.click.as_deref(),
        Some(&ClickEvent::RunCommand("/tp @s 0 64 32".to_owned()))
    );
    assert!(args[2].plain_text().parse::<i32>().unwrap() >= 30);
    s.command("locate biome minecraft:nowhere");
    assert!(found(&s.messages(), "argument.resource.not_found").is_some());
}

#[test]
fn a_biome_nobody_has_reports_not_found() {
    let mut s = scene(true, None, None, true);
    s.command("locate biome minecraft:plains");
    assert!(s.tick_until(|scene| {
        found(&scene.messages(), "commands.locate.biome.not_found").is_some()
    }));
}

#[test]
fn locating_a_poi_finds_a_loaded_point_of_interest() {
    let mut s = scene(true, None, None, true);
    s.console("setblock 3 64 3 minecraft:lectern");
    s.alice.link.take();
    s.command("locate poi minecraft:librarian");
    let reply = found(&s.messages(), "commands.locate.poi.success").expect("a poi");
    let args = args_of(&reply);
    assert_eq!(args[0].plain_text(), "minecraft:librarian");
    assert_eq!(
        args[1].style.click.as_deref(),
        Some(&ClickEvent::RunCommand("/tp @s 3 ~ 3".to_owned()))
    );
    s.command("locate poi minecraft:beehive");
    assert!(found(&s.messages(), "commands.locate.poi.not_found").is_some());
}

#[test]
fn placing_a_template_writes_the_returned_blocks_on_a_later_tick() {
    let mut s = scene(false, None, None, true);
    s.command("place template t:hut 2 70 2 none none 1 0");
    for _ in 0..3 {
        s.server.tick();
        s.server.deliver();
    }
    assert!(!s.is_stone(2, 70, 2));
    s.gate.release();
    let mut reply = None;
    assert!(s.tick_until(|scene| {
        reply = found(&scene.messages(), "commands.place.template.success");
        reply.is_some()
    }));
    assert_eq!(args_of(&reply.unwrap())[0].plain_text(), "t:hut");
    assert!(s.is_stone(2, 70, 2));
    assert!(s.is_stone(3, 70, 2));
    assert!(!s.is_stone(4, 70, 2));
}

#[test]
fn placing_reports_the_vanilla_failures() {
    let mut s = scene(true, None, None, true);
    s.command("place template t:nothing 2 70 2");
    assert!(s.tick_until(|scene| {
        found(&scene.messages(), "commands.place.template.invalid").is_some()
    }));
    s.command("place structure minecraft:village_plains 2 70 2");
    assert!(s.tick_until(|scene| {
        found(&scene.messages(), "commands.place.structure.failed").is_some()
    }));
    s.command("place feature minecraft:oak 2 70 2");
    assert!(s.tick_until(|scene| {
        found(&scene.messages(), "commands.place.feature.failed").is_some()
    }));
    s.command("place jigsaw minecraft:empty minecraft:empty 3 2 70 2");
    assert!(s.tick_until(|scene| {
        found(&scene.messages(), "commands.place.jigsaw.failed").is_some()
    }));
    s.command("place structure minecraft:nonsense 2 70 2");
    assert!(found(&s.messages(), "commands.place.structure.invalid").is_some());
    s.command("place template t:hut 2 400 2");
    assert!(found(&s.messages(), "argument.pos.outofworld").is_some());
}

#[test]
fn a_player_below_permission_level_two_cannot_locate_or_place() {
    let mut s = scene(
        true,
        Some(("minecraft:stronghold", Vec3::ZERO)),
        None,
        false,
    );
    s.command("locate structure minecraft:stronghold");
    s.command("place template t:hut 2 70 2");
    for _ in 0..10 {
        s.server.tick();
        s.server.deliver();
        std::thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(s.asked.load(Ordering::SeqCst), 0);
    let messages = s.messages();
    assert!(found(&messages, "commands.locate.structure.success").is_none());
    assert!(found(&messages, "commands.place.template.success").is_none());
    assert!(!s.is_stone(2, 70, 2));
}

#[test]
fn an_answer_for_a_source_that_left_is_dropped() {
    let mut s = scene(
        false,
        Some(("minecraft:stronghold", Vec3::new(64.0, 0.0, 64.0))),
        None,
        true,
    );
    s.command("locate structure minecraft:stronghold");
    s.server.handle(leave_of(&s.alice));
    s.server.deliver();
    s.alice.link.take();
    s.gate.release();
    for _ in 0..40 {
        s.server.tick();
        s.server.deliver();
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(found(&s.messages(), "commands.locate.structure.success").is_none());
}

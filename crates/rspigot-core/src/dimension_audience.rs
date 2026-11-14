use std::sync::{Arc, Mutex};
use std::time::Duration;

use rspigot_domain::Identifier;
use rspigot_domain::player::{ClientSettings, GameProfile, PlayerId};
use rspigot_domain::text::Component;

use crate::config::{GameConfig, LevelData};
use crate::events::{ClientEvent, Inbound};
use crate::ports::ClientLink;
use crate::server::{Audience, Server};
use crate::status::StatusBoard;
use crate::world_state::tests::Quiet;

#[derive(Clone, Default)]
struct Tap(Arc<Mutex<Vec<ClientEvent>>>);

impl Tap {
    fn take(&self) -> Vec<ClientEvent> {
        self.0
            .lock()
            .map(|mut events| std::mem::take(&mut *events))
            .unwrap_or_default()
    }
}

impl ClientLink for Tap {
    fn send(&self, event: ClientEvent) {
        if let Ok(mut events) = self.0.lock() {
            events.push(event);
        }
    }

    fn disconnect(&self, _: Component) {}

    fn is_connected(&self) -> bool {
        true
    }
}

fn server() -> Server<Tap> {
    let config = GameConfig {
        chunk_workers: 1,
        spawn_chunk_radius: 1,
        ..GameConfig::default()
    };
    let mut server = Server::new(
        config,
        LevelData::default(),
        Arc::new(Quiet),
        Arc::new(Quiet),
        StatusBoard::new(1),
    )
    .with_rng_seed(5);
    server.wait_for_spawn(Duration::from_secs(10));
    server
}

fn join(server: &mut Server<Tap>, name: &str) -> (PlayerId, Tap) {
    let link = Tap::default();
    let id = PlayerId::offline(name);
    server.handle(Inbound::Join {
        profile: GameProfile {
            id,
            name: name.to_owned(),
            properties: Vec::new(),
        },
        settings: ClientSettings::default(),
        link: link.clone(),
        address: [127, 0, 0, 1].into(),
        session: 1,
    });
    server.deliver();
    link.take();
    (id, link)
}

fn time() -> ClientEvent {
    ClientEvent::Time {
        game_time: 1,
        day_time: 2,
        advance_time: true,
    }
}

fn count(events: &[ClientEvent], wanted: fn(&ClientEvent) -> bool) -> usize {
    events.iter().filter(|event| wanted(event)).count()
}

fn is_time(event: &ClientEvent) -> bool {
    matches!(event, ClientEvent::Time { .. })
}

fn is_chat(event: &ClientEvent) -> bool {
    matches!(event, ClientEvent::SystemChat { .. })
}

#[test]
fn a_player_starts_in_the_active_dimension() {
    let mut server = server();
    let (id, _) = join(&mut server, "Alice");
    assert_eq!(server.players[&id].dim, server.world.dimension.key);
}

#[test]
fn a_dimension_event_skips_the_players_of_another_dimension() {
    let mut server = server();
    let (_, here) = join(&mut server, "Alice");
    let (bob, away) = join(&mut server, "Bob");
    if let Some(player) = server.players.get_mut(&bob) {
        player.dim = Identifier::minecraft("the_end");
    }
    let active = server.world.dimension.key.clone();
    server.push(Audience::Dimension(active), time());
    server.deliver();
    assert_eq!(count(&here.take(), is_time), 1);
    assert_eq!(count(&away.take(), is_time), 0);
}

#[test]
fn global_events_reach_the_players_of_every_dimension() {
    let mut server = server();
    let (_, here) = join(&mut server, "Alice");
    let (bob, away) = join(&mut server, "Bob");
    if let Some(player) = server.players.get_mut(&bob) {
        player.dim = Identifier::minecraft("the_end");
    }
    server.deliver();
    here.take();
    server.push(Audience::All, ClientEvent::system(Component::text("hi")));
    server.deliver();
    assert_eq!(count(&here.take(), is_chat), 1);
    assert_eq!(count(&away.take(), is_chat), 1);
}

#[test]
fn the_clock_only_reaches_the_active_dimension() {
    let mut server = server();
    let (_, here) = join(&mut server, "Alice");
    let (bob, away) = join(&mut server, "Bob");
    if let Some(player) = server.players.get_mut(&bob) {
        player.dim = Identifier::minecraft("the_end");
    }
    for _ in 0..20 {
        server.tick();
    }
    assert!(count(&here.take(), is_time) >= 1);
    assert_eq!(count(&away.take(), is_time), 0);
}

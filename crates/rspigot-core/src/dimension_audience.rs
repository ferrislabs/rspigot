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

#[test]
fn a_dimension_clock_reaches_only_its_own_players() {
    let mut server = server();
    server.load_dimension(
        Identifier::minecraft("the_end"),
        rspigot_domain::world::Dimension::end(),
        Arc::new(Quiet),
        Arc::new(Quiet),
    );
    let (_, here) = join(&mut server, "Alice");
    let (bob, away) = join(&mut server, "Bob");
    if let Some(player) = server.players.get_mut(&bob) {
        player.dim = Identifier::minecraft("the_end");
    }
    for _ in 0..20 {
        server.tick();
    }
    assert_eq!(count(&here.take(), is_time), 1);
    assert_eq!(count(&away.take(), is_time), 1);
}

fn the_end() -> Identifier {
    Identifier::minecraft("the_end")
}

fn with_end_player(server: &mut Server<Tap>) -> (PlayerId, Tap) {
    server.load_dimension(
        the_end(),
        rspigot_domain::world::Dimension::end(),
        Arc::new(Quiet),
        Arc::new(Quiet),
    );
    let (bob, link) = join(server, "Bob");
    if let Some(player) = server.players.get_mut(&bob) {
        player.dim = the_end();
    }
    (bob, link)
}

fn rain_packets(events: &[ClientEvent]) -> usize {
    count(events, |event| {
        matches!(
            event,
            ClientEvent::GameEvent(
                crate::events::GameEvent::StartRaining
                    | crate::events::GameEvent::RainLevelChange(_)
            )
        )
    })
}

#[test]
fn a_fixed_time_dimension_keeps_its_day_time_while_the_overworld_advances() {
    let mut server = server();
    join(&mut server, "Alice");
    with_end_player(&mut server);
    let start = server.world.level.day_time;
    for _ in 0..40 {
        server.tick();
    }
    assert_eq!(server.world.level.day_time, start + 40);
    let end = &server.dormant[&the_end()].world.level;
    assert_eq!(end.day_time, 0);
    assert!(end.game_time >= 40);
}

#[test]
fn a_fixed_time_dimension_never_announces_a_moving_day_time() {
    let mut server = server();
    join(&mut server, "Alice");
    let (_, away) = with_end_player(&mut server);
    for _ in 0..40 {
        server.tick();
    }
    let events = away.take();
    assert!(count(&events, is_time) >= 1);
    assert!(events.iter().all(|event| !matches!(
        event,
        ClientEvent::Time {
            advance_time: true,
            ..
        }
    )));
}

#[test]
fn the_end_gets_no_rain() {
    let mut server = server();
    join(&mut server, "Alice");
    let (_, away) = with_end_player(&mut server);
    away.take();
    server.in_dimension(&the_end(), |server| server.change_weather(true, true));
    for _ in 0..5 {
        server.tick();
    }
    let level = &server.dormant[&the_end()].world.level;
    assert!(!level.raining);
    assert!(!level.thundering);
    assert_eq!(rain_packets(&away.take()), 0);
}

#[test]
fn the_rain_of_the_overworld_does_not_reach_another_dimension() {
    let mut server = server();
    let (_, here) = join(&mut server, "Alice");
    let (_, away) = with_end_player(&mut server);
    here.take();
    away.take();
    server.change_weather(true, false);
    for _ in 0..5 {
        server.tick();
    }
    assert!(server.world.level.raining);
    assert!(!server.dormant[&the_end()].world.level.raining);
    assert!(rain_packets(&here.take()) > 0);
    assert_eq!(rain_packets(&away.take()), 0);
}

#[test]
fn a_new_dimension_does_not_inherit_the_weather_of_the_overworld() {
    let mut server = server();
    server.world.level.raining = true;
    server.world.level.thundering = true;
    server.world.level.day_time = 5000;
    with_end_player(&mut server);
    let end = &server.dormant[&the_end()].world.level;
    assert!(!end.raining);
    assert!(!end.thundering);
    assert_eq!(end.day_time, 0);
}

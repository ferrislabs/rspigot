use rspigot_domain::Vec3;
use rspigot_domain::player::PlayerId;
use rspigot_entities::SoundSource;

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
    server.deliver();
    link.take();
    Rig { server, id, link }
}

impl Rig {
    fn run(&mut self, line: &str) -> CommandOutcome {
        let outcome = execute(&mut self.server, CommandSender::Player(self.id), line);
        self.server.deliver();
        outcome
    }

    fn events(&self) -> Vec<ClientEvent> {
        self.link.take()
    }

    fn texts(events: &[ClientEvent]) -> Vec<String> {
        events
            .iter()
            .filter_map(|event| match event {
                ClientEvent::SystemChat { message, .. } => Some(message.plain_text()),
                _ => None,
            })
            .collect()
    }
}

#[test]
fn playsound_reaches_a_player_in_range_at_the_given_volume() {
    let mut rig = rig();
    assert_eq!(
        rig.run("playsound minecraft:entity.villager.no master @s 0 64 0 2 0.5"),
        CommandOutcome::Success
    );
    let events = rig.events();
    let heard = events.iter().find_map(|event| match event {
        ClientEvent::NamedSound {
            sound,
            source,
            volume,
            pitch,
            ..
        } => Some((sound.clone(), *source, *volume, *pitch)),
        _ => None,
    });
    assert_eq!(
        heard,
        Some((
            "minecraft:entity.villager.no".to_owned(),
            SoundSource::Master,
            2.0,
            0.5
        ))
    );
    assert_eq!(
        Rig::texts(&events),
        ["commands.playsound.success.single minecraft:entity.villager.no Alice"]
    );
}

#[test]
fn playsound_out_of_range_fails_without_a_minimum_volume() {
    let mut rig = rig();
    assert_eq!(
        rig.run("playsound entity.villager.no master @s 1000 64 0"),
        CommandOutcome::Failure
    );
    let events = rig.events();
    assert!(
        !events
            .iter()
            .any(|event| matches!(event, ClientEvent::NamedSound { .. }))
    );
    assert_eq!(Rig::texts(&events), ["commands.playsound.failed"]);
}

#[test]
fn playsound_out_of_range_plays_two_blocks_away_at_the_minimum_volume() {
    let mut rig = rig();
    let at = rig.server.player_position(rig.id).expect("a position");
    assert_eq!(
        rig.run("playsound entity.villager.no master @s 1000 64 0 1 1 0.5"),
        CommandOutcome::Success
    );
    let spot = rig
        .events()
        .into_iter()
        .find_map(|event| match event {
            ClientEvent::NamedSound {
                position, volume, ..
            } => Some((position, volume)),
            _ => None,
        })
        .expect("a sound");
    assert_eq!(spot.1, 0.5);
    let offset = Vec3::new(spot.0.x - at.x, spot.0.y - at.y, spot.0.z - at.z);
    let distance = (offset.x * offset.x + offset.y * offset.y + offset.z * offset.z).sqrt();
    assert!((distance - 2.0).abs() < 1.0e-6);
}

#[test]
fn playsound_rejects_an_unknown_source_and_a_pitch_above_two() {
    let mut rig = rig();
    assert_eq!(rig.run("playsound a:b nothing @s"), CommandOutcome::Failure);
    assert_eq!(
        rig.run("playsound a:b master @s 0 64 0 1 3"),
        CommandOutcome::Failure
    );
}

#[test]
fn stopsound_sends_the_flags_and_names_what_it_stops() {
    let mut rig = rig();
    assert_eq!(
        rig.run("stopsound @s record minecraft:music.creative"),
        CommandOutcome::Success
    );
    let events = rig.events();
    assert!(events.iter().any(|event| matches!(
        event,
        ClientEvent::StopSound { source: Some(SoundSource::Records), sound: Some(name) }
            if name == "minecraft:music.creative"
    )));
    assert_eq!(
        Rig::texts(&events),
        ["commands.stopsound.success.source.sound minecraft:music.creative record"]
    );
    assert_eq!(rig.run("stopsound @s"), CommandOutcome::Success);
    assert_eq!(
        Rig::texts(&rig.events()),
        ["commands.stopsound.success.sourceless.any"]
    );
    assert_eq!(rig.run("stopsound @s * a:b"), CommandOutcome::Success);
    assert_eq!(
        Rig::texts(&rig.events()),
        ["commands.stopsound.success.sourceless.sound a:b"]
    );
}

#[test]
fn particle_without_arguments_shows_one_at_the_source() {
    let mut rig = rig();
    assert_eq!(rig.run("particle minecraft:flame"), CommandOutcome::Success);
    let events = rig.events();
    let shown = events
        .iter()
        .find_map(|event| match event {
            ClientEvent::Particle(particle) => Some(*particle),
            _ => None,
        })
        .expect("a particle");
    assert_eq!(shown.kind, "minecraft:flame");
    assert_eq!(shown.count, 0);
    assert_eq!(
        Rig::texts(&events),
        ["commands.particle.success minecraft:flame"]
    );
}

#[test]
fn particle_carries_delta_speed_and_count() {
    let mut rig = rig();
    assert_eq!(
        rig.run("particle flame 1 70 2 0.5 0.25 0.5 0.1 12 force"),
        CommandOutcome::Success
    );
    let shown = rig
        .events()
        .into_iter()
        .find_map(|event| match event {
            ClientEvent::Particle(particle) => Some(particle),
            _ => None,
        })
        .expect("a particle");
    assert_eq!(shown.count, 12);
    assert_eq!(shown.spread, Vec3::new(0.5, 0.25, 0.5));
    assert!((shown.speed - 0.1).abs() < 1.0e-6);
    assert_eq!(shown.position, Vec3::new(1.5, 70.0, 2.5));
}

#[test]
fn particle_takes_block_data_and_refuses_what_it_cannot_build() {
    let mut rig = rig();
    assert_eq!(
        rig.run("particle block minecraft:stone"),
        CommandOutcome::Success
    );
    assert_eq!(rig.run("particle nothing"), CommandOutcome::Failure);
    assert_eq!(rig.run("particle block"), CommandOutcome::Failure);
}

#[test]
fn particle_too_far_from_every_viewer_fails_unless_forced() {
    let mut rig = rig();
    assert_eq!(
        rig.run("particle flame 100 70 0 0 0 0 0 1"),
        CommandOutcome::Failure
    );
    assert_eq!(
        rig.run("particle flame 100 70 0 0 0 0 0 1 force"),
        CommandOutcome::Success
    );
}

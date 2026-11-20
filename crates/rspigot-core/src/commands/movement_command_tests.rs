use rspigot_domain::player::PlayerId;
use rspigot_domain::{EntityId, Vec3};

use super::{CommandContext, CommandOutcome, CommandSender, Target, execute};
use crate::end_tests::{Tap, arrive};
use crate::events::{ClientEvent, Hand, SwingStyle};
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

    fn texts(&self) -> Vec<String> {
        self.events()
            .into_iter()
            .filter_map(|event| match event {
                ClientEvent::SystemChat { message, .. } => Some(message.plain_text()),
                _ => None,
            })
            .collect()
    }

    fn mob(&self, kind: &str) -> EntityId {
        let found = self
            .server
            .select_entities(CommandSender::Player(self.id), &format!("@e[type={kind}]"))
            .expect("parses");
        match found[0] {
            Target::Entity(id) => id,
            Target::Player(_) => panic!("a mob"),
        }
    }

    fn two_mobs(&mut self) -> (EntityId, EntityId) {
        self.run("summon minecraft:cow 5 70 5");
        self.run("summon minecraft:pig 6 70 5");
        self.texts();
        (self.mob("cow"), self.mob("pig"))
    }
}

#[test]
fn ride_mounts_and_dismounts_an_entity() {
    let mut rig = rig();
    let (cow, pig) = rig.two_mobs();
    assert_eq!(
        rig.run("ride @e[type=cow] mount @e[type=pig]"),
        CommandOutcome::Success
    );
    assert_eq!(rig.server.entities.rides.vehicle_of(cow), Some(pig));
    assert_eq!(
        rig.texts(),
        ["commands.ride.mount.success entity.minecraft.cow entity.minecraft.pig"]
    );
    assert_eq!(
        rig.run("ride @e[type=cow] dismount"),
        CommandOutcome::Success
    );
    assert_eq!(rig.server.entities.rides.vehicle_of(cow), None);
    assert_eq!(
        rig.texts(),
        ["commands.ride.dismount.success entity.minecraft.cow entity.minecraft.pig"]
    );
}

#[test]
fn ride_refuses_a_second_mount_a_loop_a_player_and_a_dismount_on_foot() {
    let mut rig = rig();
    rig.two_mobs();
    assert_eq!(
        rig.run("ride @e[type=cow] dismount"),
        CommandOutcome::Failure
    );
    assert_eq!(
        rig.texts(),
        ["commands.ride.not_riding entity.minecraft.cow"]
    );
    assert_eq!(
        rig.run("ride @e[type=cow] mount @s"),
        CommandOutcome::Failure
    );
    assert_eq!(
        rig.texts(),
        ["commands.ride.mount.failure.cant_ride_players"]
    );
    assert_eq!(
        rig.run("ride @e[type=cow] mount @e[type=cow]"),
        CommandOutcome::Failure
    );
    assert_eq!(rig.texts(), ["commands.ride.mount.failure.loop"]);
    rig.run("ride @e[type=cow] mount @e[type=pig]");
    rig.texts();
    assert_eq!(
        rig.run("ride @e[type=cow] mount @e[type=pig]"),
        CommandOutcome::Failure
    );
    assert_eq!(
        rig.texts(),
        ["commands.ride.already_riding entity.minecraft.cow entity.minecraft.pig"]
    );
    assert_eq!(
        rig.run("ride @e[type=pig] mount @e[type=cow]"),
        CommandOutcome::Failure
    );
    assert_eq!(rig.texts(), ["commands.ride.mount.failure.loop"]);
}

#[test]
fn rotate_turns_a_mob_and_a_player() {
    let mut rig = rig();
    let (cow, _) = rig.two_mobs();
    assert_eq!(
        rig.run("rotate @e[type=cow] 90 20"),
        CommandOutcome::Success
    );
    let base = &rig.server.entities.store.get(cow).expect("cow").base;
    assert_eq!((base.yaw, base.pitch), (90.0, 20.0));
    assert_eq!(
        rig.texts(),
        ["commands.rotate.success entity.minecraft.cow"]
    );
    assert_eq!(rig.run("rotate @s 45 -10"), CommandOutcome::Success);
    let turned = rig.events().into_iter().find_map(|event| match event {
        ClientEvent::PlayerRotation { yaw, pitch } => Some((yaw, pitch)),
        _ => None,
    });
    assert_eq!(turned, Some((45.0, -10.0)));
    let player = rig.server.players.get(&rig.id).expect("player");
    assert_eq!((player.rotation.yaw, player.rotation.pitch), (45.0, -10.0));
}

#[test]
fn rotate_facing_a_position_or_an_entity_aims_at_it() {
    let mut rig = rig();
    rig.two_mobs();
    let at = rig.server.player_position(rig.id).expect("position");
    assert_eq!(
        rig.run(&format!(
            "rotate @s facing {} {} {}",
            at.x + 10.0,
            at.y,
            at.z
        )),
        CommandOutcome::Success
    );
    let player = rig.server.players.get(&rig.id).expect("player");
    assert!((player.rotation.yaw + 90.0).abs() < 0.01);
    assert!(player.rotation.pitch.abs() < 0.01);
    assert_eq!(
        rig.run("rotate @e[type=cow] facing entity @s eyes"),
        CommandOutcome::Success
    );
    assert_eq!(rig.run("rotate @s facing entity"), CommandOutcome::Failure);
}

#[test]
fn swing_animates_living_targets_and_names_how() {
    let mut rig = rig();
    rig.two_mobs();
    assert_eq!(rig.run("swing @s offhand stab 10"), CommandOutcome::Success);
    let events = rig.events();
    assert!(events.iter().any(|event| matches!(
        event,
        ClientEvent::EntitySwing {
            hand: Hand::Off,
            style: SwingStyle::Stab,
            duration: 10,
            ..
        }
    )));
    assert_eq!(
        rig.run("swing @e[type=cow] mainhand"),
        CommandOutcome::Success
    );
    assert_eq!(
        rig.texts(),
        ["commands.swing.success.single entity.minecraft.cow"]
    );
    assert_eq!(
        rig.run("swing @e[type=!player] sideways"),
        CommandOutcome::Failure
    );
}

#[test]
fn spectate_needs_a_spectator_and_a_trackable_target() {
    let mut rig = rig();
    let (cow, _) = rig.two_mobs();
    assert_eq!(rig.run("spectate @e[type=cow]"), CommandOutcome::Failure);
    assert_eq!(rig.texts(), ["commands.spectate.not_spectator Alice"]);
    rig.run("gamemode spectator");
    rig.events();
    assert_eq!(rig.run("spectate @s"), CommandOutcome::Failure);
    assert_eq!(rig.texts(), ["commands.spectate.self"]);
    assert_eq!(rig.run("spectate @e[type=cow]"), CommandOutcome::Success);
    let events = rig.events();
    assert!(
        events
            .iter()
            .any(|event| matches!(event, ClientEvent::Camera(id) if *id == cow))
    );
    let at = rig
        .server
        .entities
        .store
        .get(cow)
        .expect("cow")
        .base
        .position;
    let player = rig.server.player_position(rig.id).expect("position");
    assert_eq!(Vec3::new(player.x, player.y, player.z), at);
    assert_eq!(rig.run("spectate"), CommandOutcome::Success);
    let own = rig.server.players.get(&rig.id).expect("player").entity_id;
    assert!(
        rig.events()
            .iter()
            .any(|event| matches!(event, ClientEvent::Camera(id) if *id == own))
    );
}

#[test]
fn tp_facing_a_position_sends_look_at_and_turns_the_player() {
    let mut rig = rig();
    rig.events();
    assert_eq!(
        rig.run("tp @s 5 70 5 facing 5 70 15"),
        CommandOutcome::Success
    );
    let events = rig.events();
    assert!(events.contains(&ClientEvent::LookAt {
        from: crate::events::LookAnchor::Feet,
        target: Vec3::new(5.5, 70.0, 15.5),
        entity: None,
    }));
    let rotation = rig.server.players.get(&rig.id).expect("player").rotation;
    assert!(rotation.yaw.abs() < 0.01);
    assert!(rotation.pitch.abs() < 0.01);
}

#[test]
fn tp_facing_an_entity_names_it_and_its_anchor() {
    let mut rig = rig();
    rig.events();
    assert_eq!(
        rig.run("tp @s 5 70 5 facing entity @s eyes"),
        CommandOutcome::Success
    );
    let entity = rig.server.players.get(&rig.id).expect("player").entity_id;
    let found = rig.events().into_iter().any(|event| {
        matches!(
            event,
            ClientEvent::LookAt {
                from: crate::events::LookAnchor::Feet,
                entity: Some((looked, crate::events::LookAnchor::Eyes)),
                ..
            } if looked == entity
        )
    });
    assert!(found);
}

#[test]
fn tp_with_an_unknown_trailing_word_is_refused_without_moving() {
    let mut rig = rig();
    rig.events();
    assert_eq!(rig.run("tp @s 5 70 5 sideways"), CommandOutcome::Failure);
    assert!(
        !rig.events()
            .iter()
            .any(|event| matches!(event, ClientEvent::LookAt { .. }))
    );
}

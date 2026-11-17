mod common;

use common::*;
use rspigot_blocks::BlockKind;
use rspigot_core::console::console_text;
use rspigot_core::events::{Abilities, ClientEvent, GameEvent, Inbound, PlayerInput, SoundSource};
use rspigot_core::plugins::{
    Answer, EffectSpec, ParticleDataSpec, ParticleSpec, PlayerOrder, PlayerState, Question,
    SoundSpec, TitleSpec,
};
use rspigot_core::ports::ClientLink;
use rspigot_domain::attribute::value;
use rspigot_domain::effect::{AttributeState, EffectId, EffectInstance};
use rspigot_domain::particle::{Particle, ParticleData};
use rspigot_domain::player::GameMode;
use rspigot_domain::text::{Color, Component, Content, NamedColor, Style};
use rspigot_domain::{Rotation, Vec3};

fn state(s: &mut Scene, player: &Player) -> PlayerState {
    let Answer::Player(state) = s.ask(Question::Player(player.id)) else {
        panic!("expected the state of the player");
    };
    state
}

fn speed(amplifier: i32) -> EffectSpec {
    EffectSpec {
        key: "minecraft:speed".into(),
        duration: 200,
        amplifier,
        ambient: false,
        particles: true,
        icon: true,
    }
}

fn particle(key: &str, data: ParticleDataSpec) -> ParticleSpec {
    ParticleSpec {
        key: key.into(),
        data,
        position: Vec3::new(0.5, 65.0, 0.5),
        count: 4,
        spread: Vec3::new(0.25, 0.25, 0.25),
        speed: 0.1,
    }
}

#[test]
fn send_message_parses_section_codes_only() {
    let mut s = scene(&["Alice", "Bob"]);
    let alice = s.players[0].clone();
    s.act(to(&alice, PlayerOrder::SendMessage("§6Gold".into())));
    let chats: Vec<ClientEvent> = alice
        .link
        .take()
        .into_iter()
        .filter(|event| matches!(event, ClientEvent::SystemChat { .. }))
        .collect();
    assert_eq!(
        chats,
        [ClientEvent::SystemChat {
            message: Component::from_section_codes("§6Gold"),
            overlay: false
        }]
    );
    let ClientEvent::SystemChat { message, .. } = &chats[0] else {
        unreachable!();
    };
    assert_eq!(message.style.color, Some(Color::Named(NamedColor::Gold)));

    s.act(to(&alice, PlayerOrder::SendMessage("R&D &aok".into())));
    let messages: Vec<Component> = alice
        .link
        .take()
        .into_iter()
        .filter_map(|event| match event {
            ClientEvent::SystemChat { message, .. } => Some(message),
            _ => None,
        })
        .collect();
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].plain_text(), "R&D &aok");
    assert_eq!(messages[0].style, Style::default());
}

#[test]
fn the_action_bar_is_an_overlay_message() {
    let mut s = scene(&["Alice", "Bob"]);
    let alice = s.players[0].clone();
    s.act(to(&alice, PlayerOrder::SendActionBar("§aGo".into())));
    let chats: Vec<ClientEvent> = alice
        .link
        .take()
        .into_iter()
        .filter(|event| matches!(event, ClientEvent::SystemChat { .. }))
        .collect();
    assert_eq!(
        chats,
        [ClientEvent::SystemChat {
            message: Component::from_section_codes("§aGo"),
            overlay: true
        }]
    );
}

#[test]
fn a_title_sends_times_then_subtitle_then_title() {
    let mut s = scene(&["Alice", "Bob"]);
    let alice = s.players[0].clone();
    let is_title = |event: &ClientEvent| {
        matches!(
            event,
            ClientEvent::TitleTimes { .. }
                | ClientEvent::SubtitleText(_)
                | ClientEvent::TitleText(_)
                | ClientEvent::ClearTitles { .. }
        )
    };
    s.act(to(
        &alice,
        PlayerOrder::SendTitle(TitleSpec {
            title: "§6Hello".into(),
            subtitle: Some("sub".into()),
            fade_in: 10,
            stay: 70,
            fade_out: 20,
        }),
    ));
    let titles: Vec<ClientEvent> = alice.link.take().into_iter().filter(is_title).collect();
    assert_eq!(
        titles,
        [
            ClientEvent::TitleTimes {
                fade_in: 10,
                stay: 70,
                fade_out: 20
            },
            ClientEvent::SubtitleText(Component::from_section_codes("sub")),
            ClientEvent::TitleText(Component::from_section_codes("§6Hello")),
        ]
    );

    s.act(to(
        &alice,
        PlayerOrder::SendTitle(TitleSpec {
            title: "Alone".into(),
            subtitle: None,
            fade_in: 0,
            stay: 5,
            fade_out: 0,
        }),
    ));
    let titles: Vec<ClientEvent> = alice.link.take().into_iter().filter(is_title).collect();
    assert_eq!(
        titles,
        [
            ClientEvent::TitleTimes {
                fade_in: 0,
                stay: 5,
                fade_out: 0
            },
            ClientEvent::TitleText(Component::from_section_codes("Alone")),
        ]
    );

    s.act(to(&alice, PlayerOrder::ResetTitle));
    let titles: Vec<ClientEvent> = alice.link.take().into_iter().filter(is_title).collect();
    assert_eq!(titles, [ClientEvent::ClearTitles { reset: true }]);
}

#[test]
fn the_tab_list_reaches_only_that_player() {
    let mut s = scene(&["Alice", "Bob"]);
    let (alice, bob) = (s.players[0].clone(), s.players[1].clone());
    s.act(to(
        &alice,
        PlayerOrder::SetTabList {
            header: "§6Welcome".into(),
            footer: "§7Have fun".into(),
        },
    ));
    let lists: Vec<ClientEvent> = alice
        .link
        .take()
        .into_iter()
        .filter(|event| matches!(event, ClientEvent::TabList { .. }))
        .collect();
    assert_eq!(
        lists,
        [ClientEvent::TabList {
            header: Component::from_section_codes("§6Welcome"),
            footer: Component::from_section_codes("§7Have fun"),
        }]
    );
    assert!(
        !bob.link
            .take()
            .iter()
            .any(|event| matches!(event, ClientEvent::TabList { .. }))
    );
}

#[test]
fn teleport_moves_the_player_and_facing_sets_the_rotation() {
    let mut s = scene(&["Alice", "Bob"]);
    let alice = s.players[0].clone();
    let teleports = |player: &Player| -> Vec<(Vec3, Rotation)> {
        player
            .link
            .take()
            .into_iter()
            .filter_map(|event| match event {
                ClientEvent::Teleport {
                    position, rotation, ..
                } => Some((position, rotation)),
                _ => None,
            })
            .collect()
    };
    let here = Vec3::new(10.5, 64.0, 10.5);
    s.act(to(&alice, PlayerOrder::Teleport(here)));
    assert_eq!(teleports(&alice), [(here, Rotation::new(0.0, 0.0))]);
    assert_eq!(s.server.player_location(alice.id), Some(here));

    let there = Vec3::new(-3.5, 70.0, 2.5);
    s.act(to(
        &alice,
        PlayerOrder::TeleportFacing(there, Rotation::new(90.0, 10.0)),
    ));
    assert_eq!(teleports(&alice), [(there, Rotation::new(90.0, 10.0))]);
    assert_eq!(s.server.player_location(alice.id), Some(there));
}

#[test]
fn game_mode_health_food_and_experience_reach_the_client() {
    let mut s = scene(&["Alice", "Bob"]);
    let alice = s.players[0].clone();
    s.act(to(&alice, PlayerOrder::SetGameMode(GameMode::Creative)));
    assert!(
        alice
            .link
            .take()
            .contains(&ClientEvent::GameEvent(GameEvent::ChangeGameMode(
                GameMode::Creative
            )))
    );

    s.act(to(&alice, PlayerOrder::SetHealth(10.0)));
    s.act(to(&alice, PlayerOrder::SetFoodLevel(5)));
    s.tick(1);
    let health: Vec<(f32, i32)> = alice
        .link
        .take()
        .into_iter()
        .filter_map(|event| match event {
            ClientEvent::Health { health, food, .. } => Some((health, food)),
            _ => None,
        })
        .collect();
    assert_eq!(health, [(10.0, 5)]);

    s.act(to(&alice, PlayerOrder::GiveLevels(3)));
    s.act(to(&alice, PlayerOrder::SetExp(0.5)));
    s.tick(1);
    let experience: Vec<(i32, f32)> = alice
        .link
        .take()
        .into_iter()
        .filter_map(|event| match event {
            ClientEvent::Experience {
                level, progress, ..
            } => Some((level, progress)),
            _ => None,
        })
        .collect();
    assert_eq!(experience, [(3, 0.5)]);

    s.act(to(&alice, PlayerOrder::SetHealth(0.0)));
    assert_eq!(s.server.player_is_dead(alice.id), Some(true));
    assert!(s.warnings().is_empty());
}

#[test]
fn effects_fire_and_velocity_reach_the_client() {
    let mut s = scene(&["Alice", "Bob"]);
    let alice = s.players[0].clone();
    let speed_id = EffectId::from_name("minecraft:speed").unwrap();
    s.act(to(&alice, PlayerOrder::AddEffect(speed(1))));
    let applied: Vec<EffectInstance> = alice
        .link
        .take()
        .into_iter()
        .filter_map(|event| match event {
            ClientEvent::EffectApplied { effect, .. } => Some(effect),
            _ => None,
        })
        .collect();
    assert_eq!(applied.len(), 1);
    assert_eq!(applied[0].effect, speed_id);
    assert_eq!(applied[0].amplifier, 1);

    s.act(to(
        &alice,
        PlayerOrder::RemoveEffect("minecraft:speed".into()),
    ));
    assert!(alice.link.take().iter().any(|event| matches!(
        event,
        ClientEvent::EffectRemoved { effect, .. } if *effect == speed_id
    )));

    s.act(to(&alice, PlayerOrder::SetFireTicks(100)));
    assert_eq!(s.server.player_fire_ticks(alice.id), Some(100));

    s.act(to(
        &alice,
        PlayerOrder::SetVelocity(Vec3::new(0.0, 1.0, 0.0)),
    ));
    alice.link.take();
    s.tick(1);
    let velocities: Vec<Vec3> = alice
        .link
        .take()
        .into_iter()
        .filter_map(|event| match event {
            ClientEvent::EntityVelocity { velocity, .. } => Some(velocity),
            _ => None,
        })
        .collect();
    assert_eq!(velocities, [Vec3::new(0.0, 1.0, 0.0)]);
    assert!(s.warnings().is_empty());
}

#[test]
fn flight_override_and_speeds_go_in_the_abilities_packet() {
    let mut s = scene(&["Alice", "Bob"]);
    let alice = s.players[0].clone();
    let abilities = |player: &Player| -> Vec<Abilities> {
        player
            .link
            .take()
            .into_iter()
            .filter_map(|event| match event {
                ClientEvent::Abilities(abilities) => Some(abilities),
                _ => None,
            })
            .collect()
    };
    s.act(to(&alice, PlayerOrder::SetAllowFlight(true)));
    let sent = abilities(&alice);
    assert_eq!(sent.len(), 1);
    assert!(sent[0].may_fly);

    s.act(to(&alice, PlayerOrder::SetFlying(true)));
    let sent = abilities(&alice);
    assert_eq!(sent.len(), 1);
    assert!(sent[0].flying);

    s.act(to(&alice, PlayerOrder::SetFlySpeed(0.2)));
    let sent = abilities(&alice);
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0].fly_speed, 0.1);

    s.act(to(&alice, PlayerOrder::SetWalkSpeed(0.4)));
    let sent = abilities(&alice);
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0].walk_speed, 0.2);

    s.act(to(&alice, PlayerOrder::SetGameMode(GameMode::Adventure)));
    let sent = abilities(&alice);
    assert_eq!(sent.len(), 1);
    assert!(!sent[0].may_fly);
    assert!(!sent[0].flying);
    assert_eq!(sent[0].fly_speed, 0.1);

    s.act(to(&alice, PlayerOrder::SetAllowFlight(true)));
    s.act(to(&alice, PlayerOrder::SetFlying(true)));
    s.act(to(&alice, PlayerOrder::SetAllowFlight(false)));
    let sent = abilities(&alice);
    assert_eq!(sent.len(), 3);
    assert!(!sent[2].may_fly);
    assert!(!sent[2].flying);
    assert!(s.warnings().is_empty());
}

#[test]
fn walk_speed_sets_the_movement_speed_base_that_speed_stacks_on() {
    let mut s = scene(&["Alice", "Bob"]);
    let (alice, bob) = (s.players[0].clone(), s.players[1].clone());
    let movement = |player: &Player| -> Vec<AttributeState> {
        player
            .link
            .take()
            .into_iter()
            .filter_map(|event| match event {
                ClientEvent::Attributes { attributes, .. } => Some(attributes),
                _ => None,
            })
            .flatten()
            .filter(|attribute| attribute.attribute == "minecraft:movement_speed")
            .collect()
    };
    let bob_sees_alice = (0..40).any(|_| {
        s.tick(1);
        bob.link.take().iter().any(
            |event| matches!(event, ClientEvent::SpawnEntity { uuid, .. } if *uuid == alice.id.0),
        )
    });
    assert!(bob_sees_alice);
    alice.link.take();
    s.act(to(&alice, PlayerOrder::SetWalkSpeed(0.4)));
    let base = f64::from(0.2f32);
    let slower = [AttributeState {
        attribute: "minecraft:movement_speed",
        base,
        modifiers: Vec::new(),
    }];
    assert_eq!(movement(&alice), slower);
    assert_eq!(movement(&bob), slower);

    s.act(to(&alice, PlayerOrder::AddEffect(speed(1))));
    let boosted = movement(&alice);
    assert_eq!(boosted.len(), 1);
    assert_eq!(boosted[0].base, base);
    assert_eq!(boosted[0].modifiers.len(), 1);
    assert_eq!(boosted[0].modifiers[0].id, "minecraft:effect.speed");
    assert!((value(&boosted[0]) - base * 1.4).abs() < 1e-6);
    assert!(s.warnings().is_empty());
}

#[test]
fn out_of_range_values_give_one_warning_and_change_nothing() {
    let mut s = scene(&["Alice", "Bob"]);
    let alice = s.players[0].clone();
    let home = s.server.player_location(alice.id);
    assert_eq!(PlayerOrder::SetHealth(1.0).name(), "player.set_health");
    let bad_orders = [
        PlayerOrder::SetHealth(25.0),
        PlayerOrder::SetHealth(-1.0),
        PlayerOrder::SetHealth(f32::NAN),
        PlayerOrder::SetAbsorption(-1.0),
        PlayerOrder::SetAbsorption(f32::NAN),
        PlayerOrder::SetFoodLevel(21),
        PlayerOrder::SetSaturation(-1.0),
        PlayerOrder::SetExp(1.5),
        PlayerOrder::SetLevel(-1),
        PlayerOrder::SetFlySpeed(2.0),
        PlayerOrder::SetWalkSpeed(-1.5),
        PlayerOrder::SetFlying(true),
        PlayerOrder::Teleport(Vec3::new(4.0e7, 64.0, 0.0)),
        PlayerOrder::AddEffect(speed(256)),
        PlayerOrder::AddEffect(EffectSpec {
            key: "minecraft:nope".into(),
            ..speed(0)
        }),
        PlayerOrder::RemoveEffect("minecraft:nope".into()),
        PlayerOrder::PlaySound(SoundSpec {
            key: "minecraft:nope".into(),
            volume: 1.0,
            pitch: 1.0,
        }),
        PlayerOrder::Respawn,
        PlayerOrder::SpawnParticle(particle("minecraft:entity_effect", ParticleDataSpec::None)),
        PlayerOrder::SpawnParticle(particle(
            "minecraft:dust",
            ParticleDataSpec::Dust {
                color: 0xFF0000,
                scale: 5.0,
            },
        )),
    ];
    for bad in bad_orders {
        let order = to(&alice, bad);
        let name = order.name();
        let before = s.warnings().len();
        s.act(order);
        let warnings = s.warnings();
        assert_eq!(warnings.len(), before + 1, "{name}");
        let last = warnings.last().unwrap();
        assert!(
            last.starts_with(&format!("{name} was not applied: ")),
            "{last}"
        );
    }
    assert!(
        !alice
            .link
            .take()
            .iter()
            .any(|event| matches!(event, ClientEvent::Abilities(_)))
    );
    let after = state(&mut s, &alice);
    assert_eq!(after.health, 20.0);
    assert_eq!(after.food, 20);
    assert_eq!(after.level, 0);
    assert_eq!(after.exp, 0.0);
    assert!(!after.allow_flight);
    assert!(!after.flying);
    assert_eq!(after.fly_speed, 0.1);
    assert_eq!(after.walk_speed, 0.2);
    assert!(after.effects.is_empty());
    assert_eq!(Some(after.player.position), home);
}

#[test]
fn an_order_for_a_player_who_left_is_dropped_silently() {
    let mut s = scene(&["Alice", "Bob"]);
    let alice = s.players[0].clone();
    s.server.handle(leave_of(&alice));
    s.server.deliver();
    s.act(to(&alice, PlayerOrder::SetHealth(5.0)));
    s.act(to(&alice, PlayerOrder::SetHealth(-1.0)));
    assert!(s.warnings().is_empty());
}

#[test]
fn sound_and_particles_reach_only_that_player() {
    let mut s = scene(&["Alice", "Bob"]);
    let (alice, bob) = (s.players[0].clone(), s.players[1].clone());
    let at = s.server.player_location(alice.id).unwrap();
    s.act(to(
        &alice,
        PlayerOrder::PlaySound(SoundSpec {
            key: "minecraft:entity.cow.hurt".into(),
            volume: 1.0,
            pitch: 1.0,
        }),
    ));
    let sounds: Vec<(&str, SoundSource, Vec3, f32, f32)> = alice
        .link
        .take()
        .into_iter()
        .filter_map(|event| match event {
            ClientEvent::Sound {
                sound,
                source,
                position,
                volume,
                pitch,
                ..
            } => Some((sound, source, position, volume, pitch)),
            _ => None,
        })
        .collect();
    assert_eq!(
        sounds,
        [(
            "minecraft:entity.cow.hurt",
            SoundSource::Master,
            at,
            1.0,
            1.0
        )]
    );

    let particles = |player: &Player| -> Vec<Particle> {
        player
            .link
            .take()
            .into_iter()
            .filter_map(|event| match event {
                ClientEvent::Particle(particle) => Some(particle),
                _ => None,
            })
            .collect()
    };
    s.act(to(
        &alice,
        PlayerOrder::SpawnParticle(particle("minecraft:happy_villager", ParticleDataSpec::None)),
    ));
    assert_eq!(
        particles(&alice),
        [Particle {
            kind: "minecraft:happy_villager",
            data: ParticleData::None,
            position: Vec3::new(0.5, 65.0, 0.5),
            count: 4,
            spread: Vec3::new(0.25, 0.25, 0.25),
            speed: 0.1,
        }]
    );

    s.act(to(
        &alice,
        PlayerOrder::SpawnParticle(particle(
            "minecraft:dust",
            ParticleDataSpec::Dust {
                color: 0xFF0000,
                scale: 1.0,
            },
        )),
    ));
    let dust = particles(&alice);
    assert_eq!(dust.len(), 1);
    assert_eq!(
        dust[0].data,
        ParticleData::Dust {
            color: 0xFF0000,
            scale: 1.0
        }
    );

    s.act(to(
        &alice,
        PlayerOrder::SpawnParticle(particle(
            "minecraft:block",
            ParticleDataSpec::Block("minecraft:stone".into()),
        )),
    ));
    let stone = BlockKind::from_name("minecraft:stone")
        .unwrap()
        .default_state();
    let block = particles(&alice);
    assert_eq!(block.len(), 1);
    assert_eq!(block[0].kind, "minecraft:block");
    assert_eq!(block[0].data, ParticleData::Block(stone));

    s.act(to(
        &alice,
        PlayerOrder::SpawnParticle(particle(
            "minecraft:block_crumble",
            ParticleDataSpec::Block("minecraft:stone".into()),
        )),
    ));
    let crumble = particles(&alice);
    assert_eq!(crumble.len(), 1);
    assert_eq!(crumble[0].kind, "minecraft:block_crumble");
    assert_eq!(crumble[0].data, ParticleData::Block(stone));

    s.act(to(
        &alice,
        PlayerOrder::SpawnParticle(particle(
            "minecraft:item",
            ParticleDataSpec::Item("minecraft:egg".into()),
        )),
    ));
    let item = particles(&alice);
    assert_eq!(item.len(), 1);
    assert!(matches!(item[0].data, ParticleData::Item(_)));

    assert!(
        !bob.link
            .take()
            .iter()
            .any(|event| matches!(event, ClientEvent::Sound { .. } | ClientEvent::Particle(_)))
    );
    assert!(s.warnings().is_empty());
}

#[test]
fn perform_command_runs_with_the_players_rights() {
    let mut s = scene(&["Alice", "Bob"]);
    let alice = s.players[0].clone();
    s.act(to(
        &alice,
        PlayerOrder::PerformCommand("time set 1000".into()),
    ));
    let replies = texts(&alice);
    assert_eq!(replies.len(), 2);
    assert!(replies[0].0.starts_with("command.unknown.command"));

    s.act(to(&alice, PlayerOrder::SetOp(true)));
    assert!(
        alice
            .link
            .take()
            .iter()
            .any(|event| matches!(event, ClientEvent::OperatorLevel { level: 4, .. }))
    );

    s.act(to(
        &alice,
        PlayerOrder::PerformCommand("time set 1000".into()),
    ));
    assert!(
        alice
            .link
            .take()
            .iter()
            .any(|event| matches!(event, ClientEvent::Time { day_time: 1000, .. }))
    );
}

#[test]
fn chat_makes_the_player_speak_or_run_a_command() {
    let mut s = scene(&["Alice", "Bob"]);
    let (alice, bob) = (s.players[0].clone(), s.players[1].clone());
    let said = |player: &Player| -> Vec<(rspigot_domain::player::PlayerId, String)> {
        player
            .link
            .take()
            .into_iter()
            .filter_map(|event| match event {
                ClientEvent::PlayerChat {
                    sender, message, ..
                } => Some((sender, message)),
                _ => None,
            })
            .collect()
    };
    s.act(to(&alice, PlayerOrder::Chat("hello".into())));
    assert_eq!(said(&bob), [(alice.id, "hello".to_owned())]);

    alice.link.take();
    s.act(to(&alice, PlayerOrder::Chat("/list".into())));
    let listed = alice.link.take().into_iter().any(|event| {
        matches!(
            event,
            ClientEvent::SystemChat { message, .. }
                if matches!(&message.content, Content::Translatable { key, .. } if key == "commands.list.players")
        )
    });
    assert!(listed);
    assert!(said(&bob).is_empty());

    let long = format!("§c{}", "x".repeat(300));
    s.act(to(&alice, PlayerOrder::Chat(long.clone())));
    assert!(alice.link.is_connected());
    assert_eq!(said(&bob), [(alice.id, long)]);
}

#[test]
fn kick_disconnects_with_the_reason() {
    let mut s = scene(&["Alice", "Bob"]);
    let alice = s.players[0].clone();
    s.act(to(&alice, PlayerOrder::Kick("§cBye".into())));
    let reason = alice.link.disconnected_with().unwrap();
    assert_eq!(console_text(&reason), "Bye");
    assert_eq!(reason, Component::from_section_codes("§cBye"));
    assert_eq!(s.server.players_online(), 1);
}

#[test]
fn respawn_brings_a_dead_player_back() {
    let mut s = scene(&["Alice", "Bob"]);
    let alice = s.players[0].clone();
    s.act(to(&alice, PlayerOrder::SetHealth(0.0)));
    assert_eq!(s.server.player_is_dead(alice.id), Some(true));
    alice.link.take();
    s.act(to(&alice, PlayerOrder::Respawn));
    assert!(
        alice
            .link
            .take()
            .iter()
            .any(|event| matches!(event, ClientEvent::Respawn { .. }))
    );
    assert_eq!(s.server.player_is_dead(alice.id), Some(false));
    assert!(s.warnings().is_empty());
}

#[test]
fn update_and_close_inventory() {
    let mut s = scene(&["Alice", "Bob"]);
    let alice = s.players[0].clone();
    s.act(to(&alice, PlayerOrder::UpdateInventory));
    assert!(
        alice
            .link
            .take()
            .iter()
            .any(|event| matches!(event, ClientEvent::InventoryContent { .. }))
    );
    s.act(to(&alice, PlayerOrder::CloseInventory));
    assert!(matches!(
        alice.link.take().as_slice(),
        [ClientEvent::ContainerClose(0)]
    ));
    assert!(s.warnings().is_empty());
}

#[test]
fn resolve_reports_the_full_state() {
    let mut s = scene(&["Alice", "Bob"]);
    let alice = s.players[0].clone();
    let joined = state(&mut s, &alice);
    assert!(!joined.on_ground);
    s.server.handle(Inbound::Input {
        player: alice.id,
        input: PlayerInput::Move {
            position: Some(joined.player.position),
            rotation: Some(joined.player.rotation),
            on_ground: true,
        },
    });
    s.server.deliver();
    for order in [
        PlayerOrder::SetHealth(10.0),
        PlayerOrder::SetFoodLevel(5),
        PlayerOrder::GiveLevels(3),
        PlayerOrder::SetExp(0.5),
        PlayerOrder::SetAllowFlight(true),
        PlayerOrder::SetFlySpeed(0.2),
        PlayerOrder::SetFireTicks(100),
        PlayerOrder::AddEffect(speed(1)),
    ] {
        s.act(to(&alice, order));
    }
    let resolved = state(&mut s, &alice);
    assert_eq!(resolved.player.id, alice.id);
    assert_eq!(resolved.player.rotation, Rotation::new(0.0, 0.0));
    assert_eq!(resolved.health, 10.0);
    assert_eq!(resolved.max_health, 20.0);
    assert_eq!(resolved.absorption, 0.0);
    assert_eq!(resolved.food, 5);
    assert_eq!(resolved.saturation, 5.0);
    assert_eq!(resolved.exhaustion, 0.0);
    assert_eq!(resolved.level, 3);
    assert_eq!(resolved.exp, 0.5);
    assert_eq!(resolved.total_exp, 0);
    assert!(!resolved.operator);
    assert!(resolved.allow_flight);
    assert!(!resolved.flying);
    assert_eq!(resolved.fly_speed, 0.2);
    assert_eq!(resolved.walk_speed, 0.2);
    assert!(!resolved.sneaking);
    assert!(!resolved.sprinting);
    assert!(resolved.on_ground);
    assert_eq!(resolved.fire_ticks, 100);
    assert!(!resolved.dead);
    assert_eq!(
        resolved.effects,
        [EffectSpec {
            key: "minecraft:speed".into(),
            amplifier: 1,
            ambient: false,
            particles: true,
            icon: true,
            duration: 200,
        }]
    );
    assert_eq!(resolved.ping, 0);
    assert_eq!(resolved.locale, "en_us");
}

#[test]
fn latency_is_smoothed_like_vanilla() {
    let mut s = scene(&["Alice", "Bob"]);
    let alice = s.players[0].clone();
    let mut pings = Vec::new();
    for _ in 0..2 {
        s.server.handle(Inbound::Input {
            player: alice.id,
            input: PlayerInput::Latency(100),
        });
        pings.push(state(&mut s, &alice).ping);
    }
    assert_eq!(pings, [25, 43]);
}

mod common;

use common::*;
use rspigot_core::config::GameConfig;
use rspigot_core::events::{
    ClientEvent, DigAction, Direction, Inbound, InputState, MetadataValue, MovementAction,
    PlayerInput, Pose, StateKind,
};
use rspigot_core::plugins::{BlockOrder, InventoryOrder, ItemSpec, Order, PlayerOrder};
use rspigot_domain::damage::DamageType;
use rspigot_domain::player::{ClientSettings, MainHand};
use rspigot_domain::text::Component;
use rspigot_domain::{BlockPos, EntityId, Vec3};

const SPAWN: Vec3 = Vec3::new(0.5, 64.0, 0.5);

fn console(s: &mut Scene, line: &str) {
    s.server.handle(Inbound::Console(line.to_owned()));
    s.server.deliver();
}

fn walk(s: &mut Scene, player: &Player, x: f64, y: f64, z: f64, on_ground: bool) {
    send_input(
        &mut s.server,
        player,
        PlayerInput::Move {
            position: Some(Vec3::new(x, y, z)),
            rotation: None,
            on_ground,
        },
    );
}

fn stand(s: &mut Scene, player: &Player) {
    send_input(
        &mut s.server,
        player,
        PlayerInput::Move {
            position: None,
            rotation: None,
            on_ground: true,
        },
    );
}

fn teleports(events: &[ClientEvent]) -> Vec<(i32, Vec3)> {
    events
        .iter()
        .filter_map(|event| match event {
            ClientEvent::Teleport { id, position, .. } => Some((*id, *position)),
            _ => None,
        })
        .collect()
}

fn at(s: &Scene, player: &Player) -> Vec3 {
    s.server.player_location(player.id).expect("online")
}

fn set_block(s: &mut Scene, pos: BlockPos, key: &str) {
    s.act(Order::Block {
        pos,
        order: BlockOrder::Set(key.to_owned()),
    });
}

fn entity_of(events: &[ClientEvent], player: &Player) -> Option<EntityId> {
    events.iter().find_map(|event| match event {
        ClientEvent::SpawnEntity { entity, uuid, .. } if *uuid == player.id.0 => Some(*entity),
        _ => None,
    })
}

fn metadata_of(events: &[ClientEvent], entity: EntityId) -> Vec<(u8, MetadataValue)> {
    events
        .iter()
        .filter_map(|event| match event {
            ClientEvent::EntityMetadata { entity: e, entries } if *e == entity => Some(entries),
            _ => None,
        })
        .flatten()
        .map(|entry| (entry.index, entry.value.clone()))
        .collect()
}

fn find(entries: &[(u8, MetadataValue)], index: u8) -> Option<MetadataValue> {
    entries
        .iter()
        .rev()
        .find(|(i, _)| *i == index)
        .map(|(_, value)| value.clone())
}

fn pair(s: &mut Scene) -> (Player, Player, EntityId, EntityId) {
    let alice = s.players[0].clone();
    let bob = join(&mut s.server, "Bob");
    s.tick(3);
    let alice_entity = entity_of(&bob.link.take(), &alice).expect("Bob tracks Alice");
    let bob_entity = entity_of(&alice.link.take(), &bob).expect("Alice tracks Bob");
    (alice, bob, alice_entity, bob_entity)
}

#[test]
fn w23_1_a_normal_stride_is_accepted_and_a_huge_jump_is_sent_back() {
    let mut s = scene(&["Alice"]);
    let alice = s.players[0].clone();
    walk(&mut s, &alice, 0.8, 64.0, 0.5, true);
    assert_eq!(at(&s, &alice), Vec3::new(0.8, 64.0, 0.5));
    assert!(teleports(&alice.link.take()).is_empty());
    walk(&mut s, &alice, 40.0, 64.0, 0.5, true);
    let sent = teleports(&alice.link.take());
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0].1, Vec3::new(0.8, 64.0, 0.5));
    assert_eq!(at(&s, &alice), Vec3::new(0.8, 64.0, 0.5));
}

#[test]
fn w23_1_a_flying_speed_player_may_cover_more_ground_than_a_walker() {
    let mut s = scene(&["Alice"]);
    let alice = s.players[0].clone();
    console(&mut s, "gamemode creative Alice");
    s.tick(1);
    alice.link.take();
    walk(&mut s, &alice, 8.5, 70.0, 0.5, false);
    assert!(teleports(&alice.link.take()).is_empty());
    assert_eq!(at(&s, &alice), Vec3::new(8.5, 70.0, 0.5));
}

#[test]
fn w23_1_walking_through_a_wall_is_sent_back() {
    let mut s = scene(&["Alice"]);
    let alice = s.players[0].clone();
    for y in 64..=65 {
        set_block(&mut s, BlockPos::new(1, y, 0), "minecraft:stone");
    }
    stream_chunk_of(&mut s, BlockPos::new(1, 64, 0));
    stand(&mut s, &alice);
    alice.link.take();
    walk(&mut s, &alice, 2.5, 64.0, 0.5, true);
    let sent = teleports(&alice.link.take());
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0].1, SPAWN);
    assert_eq!(at(&s, &alice), SPAWN);
}

#[test]
fn w23_1_stepping_up_a_slab_and_walking_along_a_wall_are_accepted() {
    let mut s = scene(&["Alice"]);
    let alice = s.players[0].clone();
    set_block(&mut s, BlockPos::new(1, 64, 0), "minecraft:stone_slab");
    set_block(&mut s, BlockPos::new(3, 64, 1), "minecraft:stone");
    stream_chunk_of(&mut s, BlockPos::new(1, 64, 0));
    stand(&mut s, &alice);
    alice.link.take();
    walk(&mut s, &alice, 1.5, 64.5, 0.5, true);
    s.tick(1);
    walk(&mut s, &alice, 2.5, 64.0, 0.5, true);
    s.tick(1);
    walk(&mut s, &alice, 3.5, 64.0, 0.5, true);
    assert!(teleports(&alice.link.take()).is_empty());
    assert_eq!(at(&s, &alice), Vec3::new(3.5, 64.0, 0.5));
}

#[test]
fn w23_1_hovering_for_too_long_is_a_kick_unless_flight_is_allowed() {
    let mut s = scene(&["Alice"]);
    let alice = s.players[0].clone();
    walk(&mut s, &alice, 0.5, 70.0, 0.5, false);
    s.tick(70);
    assert!(alice.link.disconnected_with().is_none());
    s.tick(20);
    assert_eq!(
        alice.link.disconnected_with(),
        Some(Component::translatable(
            "multiplayer.disconnect.flying",
            Vec::new()
        ))
    );

    let mut allowed = {
        let server = configured(|config: &mut GameConfig| config.allow_flight = true);
        scene_on(server, &["Alice"], Vec::new())
    };
    let alice = allowed.players[0].clone();
    walk(&mut allowed, &alice, 0.5, 70.0, 0.5, false);
    allowed.tick(100);
    assert!(alice.link.disconnected_with().is_none());
}

#[test]
fn w23_1_a_creative_player_never_floats_into_a_kick() {
    let mut s = scene(&["Alice"]);
    let alice = s.players[0].clone();
    console(&mut s, "gamemode creative Alice");
    s.tick(1);
    walk(&mut s, &alice, 0.5, 70.0, 0.5, false);
    s.tick(100);
    assert!(alice.link.disconnected_with().is_none());
}

#[test]
fn w23_1_an_unconfirmed_teleport_is_sent_again_after_twenty_ticks() {
    let mut s = scene(&["Alice"]);
    let alice = s.players[0].clone();
    console(&mut s, "tp Alice 10.5 64 0.5");
    let first = teleports(&alice.link.take());
    assert_eq!(first.len(), 1);
    s.tick(10);
    assert!(teleports(&alice.link.take()).is_empty());
    s.tick(15);
    let again = teleports(&alice.link.take());
    assert_eq!(again.len(), 1);
    assert_eq!(again[0].0, first[0].0);
    send_input(
        &mut s.server,
        &alice,
        PlayerInput::TeleportConfirm(first[0].0),
    );
    s.tick(30);
    assert!(teleports(&alice.link.take()).is_empty());
}

#[test]
fn w23_2_a_sprinting_swimmer_is_seen_swimming_by_the_others() {
    let mut s = scene(&["Alice"]);
    let (alice, bob, entity, _) = pair(&mut s);
    for y in 64..=66 {
        set_block(&mut s, BlockPos::new(0, y, 0), "minecraft:water");
    }
    stand(&mut s, &alice);
    send_input(
        &mut s.server,
        &alice,
        PlayerInput::Movement(MovementAction::StartSprinting),
    );
    s.tick(2);
    let seen = metadata_of(&bob.link.take(), entity);
    assert_eq!(find(&seen, 6), Some(MetadataValue::Pose(Pose::Swimming)));
    let Some(MetadataValue::Byte(flags)) = find(&seen, 0) else {
        panic!("flags are sent");
    };
    assert_ne!(flags & 0x10, 0);
}

#[test]
fn w23_2_a_player_under_a_one_block_ceiling_crawls() {
    let mut s = scene(&["Alice"]);
    let (alice, bob, entity, _) = pair(&mut s);
    set_block(&mut s, BlockPos::new(0, 65, 0), "minecraft:stone");
    stand(&mut s, &alice);
    s.tick(2);
    let seen = metadata_of(&bob.link.take(), entity);
    assert_eq!(find(&seen, 6), Some(MetadataValue::Pose(Pose::Swimming)));
    let flags = match find(&seen, 0) {
        Some(MetadataValue::Byte(flags)) => flags,
        _ => 0,
    };
    assert_eq!(flags & 0x10, 0);
}

#[test]
fn w23_3_skin_parts_and_main_hand_reach_the_other_players() {
    let mut s = scene(&["Alice"]);
    let bob = join_with(
        &mut s.server,
        "Bob",
        ClientSettings {
            skin_parts: 0x55,
            main_hand: MainHand::Left,
            ..ClientSettings::default()
        },
    );
    s.tick(3);
    let alice = s.players[0].clone();
    let events = alice.link.take();
    let entity = entity_of(&events, &bob).expect("Alice tracks Bob");
    let seen = metadata_of(&events, entity);
    assert_eq!(find(&seen, 16), Some(MetadataValue::Byte(0x55)));
    assert_eq!(
        find(&seen, 15),
        Some(MetadataValue::State(StateKind::HumanoidArm, 0))
    );
    send_input(
        &mut s.server,
        &bob,
        PlayerInput::Settings(ClientSettings {
            skin_parts: 0x7F,
            main_hand: MainHand::Right,
            ..ClientSettings::default()
        }),
    );
    s.tick(2);
    let later = metadata_of(&alice.link.take(), entity);
    assert_eq!(find(&later, 16), Some(MetadataValue::Byte(0x7F)));
    assert_eq!(
        find(&later, 15),
        Some(MetadataValue::State(StateKind::HumanoidArm, 1))
    );
}

fn spec(key: &str) -> ItemSpec {
    ItemSpec::of(key, 1)
}

#[test]
fn w23_4_a_new_viewer_gets_equipment_attributes_effects_and_particles() {
    let mut s = scene(&["Alice"]);
    let alice = s.players[0].clone();
    s.act(Order::Inventory {
        player: alice.id,
        order: InventoryOrder::Set {
            slot: 39,
            item: Some(spec("minecraft:iron_helmet")),
        },
    });
    s.act(Order::Inventory {
        player: alice.id,
        order: InventoryOrder::Set {
            slot: 40,
            item: Some(spec("minecraft:shield")),
        },
    });
    s.act(to(
        &alice,
        PlayerOrder::AddEffect(rspigot_core::plugins::EffectSpec {
            key: "minecraft:speed".to_owned(),
            duration: 200,
            amplifier: 0,
            ambient: false,
            particles: true,
            icon: true,
        }),
    ));
    console(&mut s, "attribute Alice minecraft:scale base set 2");
    s.tick(2);
    let bob = join(&mut s.server, "Bob");
    s.tick(3);
    let events = bob.link.take();
    let entity = entity_of(&events, &alice).expect("Bob tracks Alice");
    let slots: Vec<_> = events
        .iter()
        .filter_map(|event| match event {
            ClientEvent::EntityEquipment { entity: e, slots } if *e == entity => Some(slots),
            _ => None,
        })
        .flatten()
        .map(|(slot, stack)| (*slot, stack.is_some()))
        .collect();
    use rspigot_core::events::EquipmentSlot as Slot;
    assert!(slots.contains(&(Slot::Head, true)), "{slots:?}");
    assert!(slots.contains(&(Slot::OffHand, true)), "{slots:?}");
    let scale = events.iter().find_map(|event| match event {
        ClientEvent::Attributes {
            entity: e,
            attributes,
        } if *e == entity => attributes
            .iter()
            .find(|a| a.attribute == "minecraft:scale")
            .map(|a| a.base),
        _ => None,
    });
    assert_eq!(scale, Some(2.0));
    assert!(events.iter().any(|event| matches!(
        event,
        ClientEvent::EffectApplied { entity: e, .. } if *e == entity
    )));
    let seen = metadata_of(&events, entity);
    assert!(matches!(
        find(&seen, 10),
        Some(MetadataValue::Particles(list)) if list.len() == 1
    ));
    assert_eq!(find(&seen, 11), Some(MetadataValue::Boolean(false)));
}

#[test]
fn w23_4_effect_changes_update_the_particles_for_everyone() {
    let mut s = scene(&["Alice"]);
    let (alice, bob, entity, _) = pair(&mut s);
    s.act(to(
        &alice,
        PlayerOrder::AddEffect(rspigot_core::plugins::EffectSpec {
            key: "minecraft:speed".to_owned(),
            duration: 200,
            amplifier: 0,
            ambient: true,
            particles: true,
            icon: true,
        }),
    ));
    s.tick(2);
    let seen = metadata_of(&bob.link.take(), entity);
    assert!(matches!(
        find(&seen, 10),
        Some(MetadataValue::Particles(list)) if list.len() == 1
    ));
    assert_eq!(find(&seen, 11), Some(MetadataValue::Boolean(true)));
    s.act(to(&alice, PlayerOrder::ClearEffects));
    s.tick(2);
    let cleared = metadata_of(&bob.link.take(), entity);
    assert_eq!(
        find(&cleared, 10),
        Some(MetadataValue::Particles(Vec::new()))
    );
    assert_eq!(find(&cleared, 11), Some(MetadataValue::Boolean(false)));
}

#[test]
fn w23_4_a_mob_with_an_effect_shows_particles_to_its_viewers() {
    let mut s = scene(&["Alice"]);
    let alice = s.players[0].clone();
    stream_chunk_of(&mut s, BlockPos::new(1, 64, 1));
    console(&mut s, "summon minecraft:cow 3.5 64 0.5");
    s.tick(2);
    alice.link.take();
    console(
        &mut s,
        "effect give @e[type=minecraft:cow] minecraft:speed 100",
    );
    s.tick(3);
    let events = alice.link.take();
    let particles = events.iter().any(|event| match event {
        ClientEvent::EntityMetadata { entries, .. } => entries.iter().any(|e| {
            e.index == 10 && matches!(&e.value, MetadataValue::Particles(list) if list.len() == 1)
        }),
        _ => false,
    });
    assert!(particles);
}

fn exhaustion(s: &Scene, player: &Player) -> f32 {
    s.server.player_food(player.id).map_or(0.0, |food| food.2)
}

fn sprint(s: &mut Scene, player: &Player) {
    send_input(
        &mut s.server,
        player,
        PlayerInput::Movement(MovementAction::StartSprinting),
    );
}

#[test]
fn w23_5_sprinting_costs_a_tenth_of_exhaustion_per_block() {
    let mut s = scene(&["Alice"]);
    let alice = s.players[0].clone();
    stand(&mut s, &alice);
    sprint(&mut s, &alice);
    for step in 1..=5 {
        walk(&mut s, &alice, 0.5 + f64::from(step), 64.0, 0.5, true);
        s.tick(1);
    }
    assert!((exhaustion(&s, &alice) - 0.5).abs() < 1e-3);
}

#[test]
fn w23_5_walking_is_free_by_default_and_the_keys_scale_the_cost() {
    let mut s = scene(&["Alice"]);
    let alice = s.players[0].clone();
    stand(&mut s, &alice);
    for step in 1..=5 {
        walk(&mut s, &alice, 0.5 + f64::from(step), 64.0, 0.5, true);
        s.tick(1);
    }
    assert_eq!(exhaustion(&s, &alice), 0.0);

    let server = configured(|config: &mut GameConfig| {
        config.hunger.other_multiplier = 0.2;
        config.hunger.sprint_multiplier = 0.4;
    });
    let mut keyed = scene_on(server, &["Alice"], Vec::new());
    let alice = keyed.players[0].clone();
    stand(&mut keyed, &alice);
    for step in 1..=5 {
        walk(&mut keyed, &alice, 0.5 + f64::from(step), 64.0, 0.5, true);
        keyed.tick(1);
    }
    assert!((exhaustion(&keyed, &alice) - 1.0).abs() < 1e-3);
    sprint(&mut keyed, &alice);
    let before = exhaustion(&keyed, &alice);
    walk(&mut keyed, &alice, 6.5, 64.0, 0.5, true);
    walk(&mut keyed, &alice, 7.5, 64.0, 0.5, true);
    assert!((exhaustion(&keyed, &alice) - before - 0.8).abs() < 1e-3);
}

fn press_jump(s: &mut Scene, player: &Player) {
    send_input(
        &mut s.server,
        player,
        PlayerInput::Input(InputState {
            jump: true,
            ..InputState::default()
        }),
    );
}

#[test]
fn w23_5_jumping_costs_five_hundredths_and_a_sprint_jump_a_fifth() {
    let mut s = scene(&["Alice"]);
    let alice = s.players[0].clone();
    stand(&mut s, &alice);
    press_jump(&mut s, &alice);
    assert!((exhaustion(&s, &alice) - 0.05).abs() < 1e-4);
    send_input(
        &mut s.server,
        &alice,
        PlayerInput::Input(InputState::default()),
    );
    sprint(&mut s, &alice);
    press_jump(&mut s, &alice);
    assert!((exhaustion(&s, &alice) - 0.25).abs() < 1e-4);
}

#[test]
fn w23_5_swimming_costs_per_block_with_the_swim_multiplier() {
    let mut s = scene(&["Alice"]);
    let alice = s.players[0].clone();
    for x in 0..8 {
        for y in 64..=66 {
            set_block(&mut s, BlockPos::new(x, y, 0), "minecraft:water");
        }
    }
    stand(&mut s, &alice);
    sprint(&mut s, &alice);
    s.tick(2);
    let before = exhaustion(&s, &alice);
    for step in 1..=4 {
        walk(&mut s, &alice, 0.5 + f64::from(step), 64.0, 0.5, false);
        s.tick(1);
    }
    let cost = exhaustion(&s, &alice) - before;
    assert!((cost - 0.04).abs() < 1e-3, "{cost}");
}

#[test]
fn w23_5_an_attack_costs_the_combat_exhaustion_with_the_attack_reason() {
    use rspigot_core::plugins::events::EntityExhaustionReason;
    use rspigot_core::plugins::{EventKind, EventPayload};
    let mut s = scene_with(&["Alice"], vec![observe(1, EventKind::EntityExhaustion)]);
    let (alice, _bob, _, entity) = pair(&mut s);
    send_input(&mut s.server, &alice, PlayerInput::Attack(entity));
    let reasons: Vec<_> = fired(&s.plugin)
        .into_iter()
        .filter_map(|f| match f.payload {
            EventPayload::EntityExhaustion(event) => Some(event.exhaustion_reason),
            _ => None,
        })
        .collect();
    assert!(
        reasons.contains(&EntityExhaustionReason::Attack),
        "{reasons:?}"
    );
}

#[test]
fn w23_5_regeneration_costs_the_regen_exhaustion_key() {
    let server = configured(|config: &mut GameConfig| config.hunger.regen = 2.0);
    let mut s = scene_on(server, &["Alice"], Vec::new());
    let alice = s.players[0].clone();
    s.act(to(&alice, PlayerOrder::SetHealth(10.0)));
    s.act(to(&alice, PlayerOrder::SetSaturation(0.0)));
    s.act(to(&alice, PlayerOrder::SetFoodLevel(20)));
    s.act(to(&alice, PlayerOrder::SetExhaustion(0.0)));
    s.tick(85);
    let cost = exhaustion(&s, &alice);
    assert!((cost - 2.0).abs() < 1e-3, "{cost}");
}

fn inputs(shift: bool) -> PlayerInput {
    PlayerInput::Input(InputState {
        shift,
        ..InputState::default()
    })
}

fn cameras(events: &[ClientEvent]) -> Vec<EntityId> {
    events
        .iter()
        .filter_map(|event| match event {
            ClientEvent::Camera(entity) => Some(*entity),
            _ => None,
        })
        .collect()
}

#[test]
fn w23_6_a_spectator_follows_its_target_and_sneaking_lets_go() {
    let mut s = scene(&["Alice"]);
    let (alice, bob, alice_entity, bob_entity) = pair(&mut s);
    console(&mut s, "gamemode spectator Alice");
    s.tick(2);
    alice.link.take();
    send_input(
        &mut s.server,
        &alice,
        PlayerInput::SpectatorAction(Some(bob_entity)),
    );
    assert_eq!(cameras(&alice.link.take()), vec![bob_entity]);
    walk(&mut s, &bob, 3.5, 64.0, 0.5, true);
    s.tick(2);
    assert_eq!(at(&s, &alice), at(&s, &bob));
    send_input(&mut s.server, &alice, inputs(true));
    s.tick(2);
    assert_eq!(cameras(&alice.link.take()), vec![alice_entity]);
    walk(&mut s, &bob, 5.5, 64.0, 0.5, true);
    s.tick(2);
    assert_ne!(at(&s, &alice), at(&s, &bob));
}

#[test]
fn w23_6_the_camera_returns_when_the_target_dies() {
    let mut s = scene(&["Alice"]);
    let (alice, bob, _, bob_entity) = pair(&mut s);
    console(&mut s, "gamemode spectator Alice");
    s.tick(2);
    alice.link.take();
    send_input(
        &mut s.server,
        &alice,
        PlayerInput::SpectatorAction(Some(bob_entity)),
    );
    assert_eq!(cameras(&alice.link.take()), vec![bob_entity]);
    s.act(to(&bob, PlayerOrder::Kill));
    s.tick(3);
    assert_eq!(cameras(&alice.link.take()).len(), 1);
}

#[test]
fn w23_6_spectators_are_hidden_from_survival_players() {
    let mut s = scene(&["Alice"]);
    let alice = s.players[0].clone();
    console(&mut s, "gamemode spectator Alice");
    s.tick(2);
    let bob = join(&mut s.server, "Bob");
    s.tick(4);
    assert!(entity_of(&bob.link.take(), &alice).is_none());
    assert!(entity_of(&alice.link.take(), &bob).is_some());
}

#[test]
fn w23_6_flying_into_a_wall_with_elytra_hurts_as_fly_into_wall() {
    let mut s = scene(&["Alice"]);
    let alice = s.players[0].clone();
    s.act(Order::Inventory {
        player: alice.id,
        order: InventoryOrder::Set {
            slot: 38,
            item: Some(spec("minecraft:elytra")),
        },
    });
    for y in 64..=70 {
        set_block(&mut s, BlockPos::new(6, y, 0), "minecraft:stone");
    }
    stream_chunk_of(&mut s, BlockPos::new(6, 64, 0));
    walk(&mut s, &alice, 0.5, 66.0, 0.5, false);
    send_input(
        &mut s.server,
        &alice,
        PlayerInput::Movement(MovementAction::StartFallFlying),
    );
    s.tick(1);
    for x in [2.0, 3.5, 5.0] {
        walk(&mut s, &alice, x, 66.0, 0.5, false);
        s.tick(1);
    }
    alice.link.take();
    walk(&mut s, &alice, 5.7, 66.0, 0.5, false);
    let hit = alice.link.take().iter().any(|event| {
        matches!(
            event,
            ClientEvent::DamageEvent { kind, .. } if *kind == DamageType::FlyIntoWall
        )
    });
    assert!(hit);
}

fn dig(s: &mut Scene, player: &Player, action: DigAction, pos: BlockPos, face: Direction, n: i32) {
    send_input(
        &mut s.server,
        player,
        PlayerInput::Dig {
            action,
            pos,
            face,
            sequence: n,
        },
    );
}

fn destruction(events: &[ClientEvent]) -> Vec<(BlockPos, i8)> {
    events
        .iter()
        .filter_map(|event| match event {
            ClientEvent::BlockDestruction { pos, stage, .. } => Some((*pos, *stage)),
            _ => None,
        })
        .collect()
}

fn hits(events: &[ClientEvent]) -> Vec<(i32, i32)> {
    events
        .iter()
        .filter_map(|event| match event {
            ClientEvent::LevelEvent { event, data, .. } if matches!(*event, 2019 | 2020) => {
                Some((*event, *data))
            }
            _ => None,
        })
        .collect()
}

const DIRT: BlockPos = BlockPos::new(2, 63, 0);

fn mining_scene() -> (Scene, Player, Player) {
    let mut s = scene(&["Alice", "Bob"]);
    let (alice, bob) = (s.players[0].clone(), s.players[1].clone());
    set_block(&mut s, DIRT, "minecraft:dirt");
    stream_chunk_of(&mut s, DIRT);
    stand(&mut s, &alice);
    s.tick(3);
    alice.link.take();
    bob.link.take();
    (s, alice, bob)
}

#[test]
fn w23_7_other_players_see_the_cracks_grow_and_clear() {
    let (mut s, alice, bob) = mining_scene();
    dig(&mut s, &alice, DigAction::Start, DIRT, Direction::Up, 1);
    s.tick(6);
    let seen = destruction(&bob.link.take());
    assert!(seen.len() >= 2, "{seen:?}");
    assert!(seen.iter().all(|(pos, _)| *pos == DIRT));
    let stages: Vec<i8> = seen.iter().map(|(_, stage)| *stage).collect();
    assert!(
        stages.windows(2).all(|pair| pair[0] < pair[1]),
        "{stages:?}"
    );
    assert!(destruction(&alice.link.take()).is_empty());
    dig(&mut s, &alice, DigAction::Cancel, DIRT, Direction::Up, 2);
    assert_eq!(destruction(&bob.link.take()), vec![(DIRT, -1)]);
}

#[test]
fn w23_7_finishing_the_block_clears_the_cracks() {
    let (mut s, alice, bob) = mining_scene();
    dig(&mut s, &alice, DigAction::Start, DIRT, Direction::Up, 1);
    s.tick(10);
    bob.link.take();
    dig(&mut s, &alice, DigAction::Finish, DIRT, Direction::Up, 2);
    let seen = destruction(&bob.link.take());
    assert_eq!(seen.last(), Some(&(DIRT, -1)));
}

#[test]
fn w23_7_hit_level_events_carry_the_destroy_direction() {
    let (mut s, alice, bob) = mining_scene();
    dig(&mut s, &alice, DigAction::Start, DIRT, Direction::Up, 1);
    s.tick(9);
    let first = hits(&bob.link.take());
    assert_eq!(first.len(), 9);
    assert!(first.iter().all(|(_, data)| *data == Direction::Up as i32));
    let accents: Vec<usize> = first
        .iter()
        .enumerate()
        .filter(|(_, (event, _))| *event == 2020)
        .map(|(index, _)| index)
        .collect();
    assert!(accents.len() >= 2, "{accents:?}");
    assert!(accents.windows(2).all(|pair| pair[1] - pair[0] == 4));
    dig(
        &mut s,
        &alice,
        DigAction::ChangeDestroyDirection,
        DIRT,
        Direction::North,
        2,
    );
    s.tick(2);
    let later = hits(&bob.link.take());
    assert!(!later.is_empty());
    assert!(
        later
            .iter()
            .all(|(_, data)| *data == Direction::North as i32)
    );
    assert!(hits(&alice.link.take()).is_empty());
}

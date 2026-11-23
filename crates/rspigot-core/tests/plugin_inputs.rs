mod common;

use common::*;
use rspigot_core::events::{
    ClientEvent, DigAction, Direction, Hand, Inbound, InputState, MovementAction, PlayerInput,
};
use rspigot_core::plugins::events::{
    PlayerAnimation, PlayerAnimationType, PlayerGameModeChange, PlayerHideEntity,
    PlayerInput as Pressed, PlayerItemHeld, PlayerShowEntity, PlayerSwapHandItems,
    PlayerSwapHandItemsPatch, PlayerToggleFlight, PlayerToggleSneak, PlayerToggleSprint,
};
use rspigot_core::plugins::{
    Answer, EventKind, EventPatch, EventPayload, Input, ItemSpec, Phase, PlayerOrder, Question,
};
use rspigot_domain::BlockPos;
use rspigot_domain::player::GameMode;

fn shift(on: bool) -> PlayerInput {
    PlayerInput::Input(InputState {
        shift: on,
        ..InputState::default()
    })
}

fn sneaking(s: &mut Scene, player: &Player) -> bool {
    match s.ask(Question::Player(player.id)) {
        Answer::Player(state) => state.sneaking,
        other => panic!("expected the state, got {other:?}"),
    }
}

fn state(s: &mut Scene, player: &Player) -> rspigot_core::plugins::PlayerState {
    match s.ask(Question::Player(player.id)) {
        Answer::Player(state) => state,
        other => panic!("expected the state, got {other:?}"),
    }
}

#[test]
fn an_input_packet_fires_the_input_then_the_sneak_only_when_they_change() {
    let mut s = scene_with(
        &["Alice"],
        vec![
            observe(1, EventKind::PlayerInput),
            observe(2, EventKind::PlayerToggleSneak),
        ],
    );
    let alice = s.players[0].clone();
    send_input(&mut s.server, &alice, shift(true));
    send_input(&mut s.server, &alice, shift(true));
    let kinds = fired_kinds(&s.plugin);
    assert_eq!(
        kinds,
        [
            (Phase::Settled, EventKind::PlayerInput),
            (Phase::Settled, EventKind::PlayerToggleSneak),
        ]
    );
    let events = fired(&s.plugin);
    let EventPayload::PlayerInput(Pressed { input, .. }) = &events[0].payload else {
        panic!("expected the input");
    };
    assert_eq!(
        *input,
        Input {
            sneak: true,
            ..Input::default()
        }
    );
    let EventPayload::PlayerToggleSneak(PlayerToggleSneak { sneaking, .. }) = &events[1].payload
    else {
        panic!("expected the sneak");
    };
    assert!(*sneaking);
    assert!(self::sneaking(&mut s, &alice));
}

#[test]
fn a_cancelled_sneak_leaves_the_player_standing_and_the_next_packet_compares_at_the_head() {
    let mut s = scene_with(
        &["Alice"],
        vec![
            deciding(1, EventKind::PlayerToggleSneak),
            observe(2, EventKind::PlayerInput),
        ],
    );
    let alice = s.players[0].clone();
    send_input(&mut s.server, &alice, shift(true));
    send_input(&mut s.server, &alice, shift(false));
    assert_eq!(asked_for(&s.plugin, EventKind::PlayerToggleSneak).len(), 1);
    assert_eq!(settled_for(&s.plugin, EventKind::PlayerInput).len(), 1);
    let asked = last_asked(&s.plugin, EventKind::PlayerToggleSneak);
    rule_on(&mut s.server, 1, asked.id, true, None);
    assert!(!sneaking(&mut s, &alice));
    assert_eq!(asked_for(&s.plugin, EventKind::PlayerToggleSneak).len(), 1);
    assert_eq!(settled_for(&s.plugin, EventKind::PlayerInput).len(), 2);
}

#[test]
fn an_allowed_sneak_lets_the_next_release_ask_again() {
    let mut s = scene_with(&["Alice"], vec![deciding(1, EventKind::PlayerToggleSneak)]);
    let alice = s.players[0].clone();
    send_input(&mut s.server, &alice, shift(true));
    send_input(&mut s.server, &alice, shift(false));
    let asked = last_asked(&s.plugin, EventKind::PlayerToggleSneak);
    rule_on(&mut s.server, 1, asked.id, false, None);
    assert!(sneaking(&mut s, &alice));
    let release = asked_for(&s.plugin, EventKind::PlayerToggleSneak);
    assert_eq!(release.len(), 2);
    let EventPayload::PlayerToggleSneak(PlayerToggleSneak { sneaking, .. }) = &release[1].payload
    else {
        panic!("expected a sneak");
    };
    assert!(!*sneaking);
}

#[test]
fn every_sprint_packet_is_an_event_and_a_cancel_keeps_the_player_walking() {
    let mut s = scene_with(&["Alice"], vec![deciding(1, EventKind::PlayerToggleSprint)]);
    let alice = s.players[0].clone();
    send_input(
        &mut s.server,
        &alice,
        PlayerInput::Movement(MovementAction::StartSprinting),
    );
    let asked = last_asked(&s.plugin, EventKind::PlayerToggleSprint);
    let EventPayload::PlayerToggleSprint(PlayerToggleSprint { sprinting, .. }) = &asked.payload
    else {
        panic!("expected a sprint");
    };
    assert!(*sprinting);
    rule_on(&mut s.server, 1, asked.id, true, None);
    assert!(!state(&mut s, &alice).sprinting);
    send_input(
        &mut s.server,
        &alice,
        PlayerInput::Movement(MovementAction::StartSprinting),
    );
    let asked = last_asked(&s.plugin, EventKind::PlayerToggleSprint);
    rule_on(&mut s.server, 1, asked.id, false, None);
    assert!(state(&mut s, &alice).sprinting);
    send_input(
        &mut s.server,
        &alice,
        PlayerInput::Movement(MovementAction::StartSprinting),
    );
    assert_eq!(asked_for(&s.plugin, EventKind::PlayerToggleSprint).len(), 3);
}

fn abilities_sent(player: &Player) -> usize {
    player
        .link
        .take()
        .iter()
        .filter(|event| matches!(event, ClientEvent::Abilities(_)))
        .count()
}

#[test]
fn a_flight_toggle_fires_only_for_a_player_who_may_fly_and_a_cancel_resyncs() {
    let mut s = scene_with(&["Alice"], vec![deciding(1, EventKind::PlayerToggleFlight)]);
    let alice = s.players[0].clone();
    send_input(
        &mut s.server,
        &alice,
        PlayerInput::Abilities { flying: true },
    );
    assert!(asked_for(&s.plugin, EventKind::PlayerToggleFlight).is_empty());
    s.act(to(&alice, PlayerOrder::SetAllowFlight(true)));
    alice.link.take();
    send_input(
        &mut s.server,
        &alice,
        PlayerInput::Abilities { flying: true },
    );
    let asked = last_asked(&s.plugin, EventKind::PlayerToggleFlight);
    let EventPayload::PlayerToggleFlight(PlayerToggleFlight { flying, .. }) = &asked.payload else {
        panic!("expected a flight toggle");
    };
    assert!(*flying);
    rule_on(&mut s.server, 1, asked.id, true, None);
    assert!(!state(&mut s, &alice).flying);
    assert_eq!(abilities_sent(&alice), 1);
    send_input(
        &mut s.server,
        &alice,
        PlayerInput::Abilities { flying: true },
    );
    let asked = last_asked(&s.plugin, EventKind::PlayerToggleFlight);
    rule_on(&mut s.server, 1, asked.id, false, None);
    assert!(state(&mut s, &alice).flying);
}

fn game_mode(s: &mut Scene, player: &Player) -> GameMode {
    state(s, player).player.game_mode
}

#[test]
fn a_cancelled_game_mode_change_keeps_the_mode_and_sends_no_reply() {
    let mut s = scene_with(
        &["Alice"],
        vec![deciding(1, EventKind::PlayerGameModeChange)],
    );
    let alice = s.players[0].clone();
    s.server.handle(Inbound::Console("op Alice".to_owned()));
    s.tick(1);
    alice.link.take();
    send_input(
        &mut s.server,
        &alice,
        PlayerInput::Command("gamemode creative".to_owned()),
    );
    assert!(system_texts(&alice).is_empty());
    let asked = last_asked(&s.plugin, EventKind::PlayerGameModeChange);
    let EventPayload::PlayerGameModeChange(PlayerGameModeChange {
        new_game_mode,
        player,
    }) = &asked.payload
    else {
        panic!("expected a game mode change");
    };
    assert_eq!(*new_game_mode, GameMode::Creative);
    assert_eq!(player.game_mode, GameMode::Survival);
    rule_on(&mut s.server, 1, asked.id, true, None);
    assert_eq!(game_mode(&mut s, &alice), GameMode::Survival);
    assert!(system_texts(&alice).is_empty());
    send_input(
        &mut s.server,
        &alice,
        PlayerInput::Command("gamemode creative".to_owned()),
    );
    let asked = last_asked(&s.plugin, EventKind::PlayerGameModeChange);
    rule_on(&mut s.server, 1, asked.id, false, None);
    assert_eq!(game_mode(&mut s, &alice), GameMode::Creative);
    assert_eq!(system_texts(&alice), ["Set own game mode to Creative Mode"]);
}

#[test]
fn a_game_mode_change_reads_the_mode_at_the_head_of_the_lane() {
    let mut s = scene_with(
        &["Alice"],
        vec![deciding(1, EventKind::PlayerGameModeChange)],
    );
    let alice = s.players[0].clone();
    s.act(to(&alice, PlayerOrder::SetGameMode(GameMode::Creative)));
    s.act(to(&alice, PlayerOrder::SetGameMode(GameMode::Creative)));
    assert_eq!(
        asked_for(&s.plugin, EventKind::PlayerGameModeChange).len(),
        1
    );
    let asked = last_asked(&s.plugin, EventKind::PlayerGameModeChange);
    rule_on(&mut s.server, 1, asked.id, false, None);
    assert_eq!(game_mode(&mut s, &alice), GameMode::Creative);
    assert_eq!(
        asked_for(&s.plugin, EventKind::PlayerGameModeChange).len(),
        1
    );
}

#[test]
fn hiding_and_showing_fire_after_the_effect_for_the_first_and_last_plugin() {
    let mut s = scene_with(
        &["Alice", "Bob"],
        vec![
            observe(1, EventKind::PlayerHideEntity),
            observe(2, EventKind::PlayerShowEntity),
        ],
    );
    let (alice, bob) = (s.players[0].clone(), s.players[1].clone());
    s.act(to(&alice, PlayerOrder::HidePlayer(bob.id)));
    s.act(to(&alice, PlayerOrder::HidePlayer(bob.id)));
    s.act(to(&alice, PlayerOrder::ShowPlayer(bob.id)));
    let events = fired(&s.plugin);
    assert_eq!(events.len(), 2);
    let EventPayload::PlayerHideEntity(PlayerHideEntity { entity, player }) = &events[0].payload
    else {
        panic!("expected a hide");
    };
    assert_eq!(player.id, alice.id);
    assert_eq!(entity.uuid, bob.id.0);
    assert_eq!(entity.kind, "minecraft:player");
    let EventPayload::PlayerShowEntity(PlayerShowEntity { entity, .. }) = &events[1].payload else {
        panic!("expected a show");
    };
    assert_eq!(entity.uuid, bob.id.0);
}

fn held_slots(player: &Player) -> Vec<u8> {
    player
        .link
        .take()
        .into_iter()
        .filter_map(|event| match event {
            ClientEvent::HeldSlot(slot) => Some(slot),
            _ => None,
        })
        .collect()
}

fn selected(s: &mut Scene, player: &Player) -> u8 {
    match s.ask(Question::Inventory(player.id)) {
        Answer::Inventory(inventory) => inventory.held_slot,
        other => panic!("expected the inventory, got {other:?}"),
    }
}

#[test]
fn a_cancelled_held_slot_change_sends_the_old_slot_back() {
    let mut s = scene_with(&["Alice"], vec![deciding(1, EventKind::PlayerItemHeld)]);
    let alice = s.players[0].clone();
    send_input(&mut s.server, &alice, PlayerInput::HeldSlot(3));
    send_input(&mut s.server, &alice, PlayerInput::HeldSlot(5));
    let asked = asked_for(&s.plugin, EventKind::PlayerItemHeld);
    assert_eq!(asked.len(), 1);
    assert_eq!(
        asked[0].payload,
        EventPayload::PlayerItemHeld(PlayerItemHeld {
            new_slot: 3,
            player: held_player(&asked[0].payload),
            previous_slot: 0,
        })
    );
    rule_on(&mut s.server, 1, asked[0].id, false, None);
    assert_eq!(selected(&mut s, &alice), 3);
    let asked = asked_for(&s.plugin, EventKind::PlayerItemHeld);
    let EventPayload::PlayerItemHeld(PlayerItemHeld {
        new_slot,
        previous_slot,
        ..
    }) = &asked[1].payload
    else {
        panic!("expected a held slot");
    };
    assert_eq!((*previous_slot, *new_slot), (3, 5));
    alice.link.take();
    rule_on(&mut s.server, 1, asked[1].id, true, None);
    assert_eq!(selected(&mut s, &alice), 3);
    assert_eq!(held_slots(&alice), [3]);
}

fn held_player(payload: &EventPayload) -> rspigot_core::plugins::PlayerSnapshot {
    match payload {
        EventPayload::PlayerItemHeld(PlayerItemHeld { player, .. }) => player.clone(),
        other => panic!("expected a held slot, got {other:?}"),
    }
}

fn swings(player: &Player) -> usize {
    player
        .link
        .take()
        .iter()
        .filter(|event| matches!(event, ClientEvent::EntityAnimation { .. }))
        .count()
}

#[test]
fn a_cancelled_arm_swing_is_not_shown() {
    let mut s = scene_with(
        &["Alice", "Bob"],
        vec![deciding(1, EventKind::PlayerAnimation)],
    );
    let (alice, bob) = (s.players[0].clone(), s.players[1].clone());
    s.tick(30);
    bob.link.take();
    send_input(&mut s.server, &alice, PlayerInput::Swing(Hand::Main));
    let asked = last_asked(&s.plugin, EventKind::PlayerAnimation);
    let EventPayload::PlayerAnimation(PlayerAnimation { animation_type, .. }) = &asked.payload
    else {
        panic!("expected an animation");
    };
    assert_eq!(*animation_type, PlayerAnimationType::ArmSwing);
    rule_on(&mut s.server, 1, asked.id, true, None);
    assert_eq!(swings(&bob), 0);
    send_input(&mut s.server, &alice, PlayerInput::Swing(Hand::Main));
    let asked = last_asked(&s.plugin, EventKind::PlayerAnimation);
    rule_on(&mut s.server, 1, asked.id, false, None);
    assert_eq!(swings(&bob), 1);
}

#[test]
fn a_dead_player_does_not_swing() {
    let mut s = scene_with(&["Alice"], vec![observe(1, EventKind::PlayerAnimation)]);
    let alice = s.players[0].clone();
    s.act(to(&alice, PlayerOrder::SetHealth(0.0)));
    send_input(&mut s.server, &alice, PlayerInput::Swing(Hand::Main));
    assert!(settled_for(&s.plugin, EventKind::PlayerAnimation).is_empty());
}

fn swap(s: &mut Scene, player: &Player) {
    send_input(
        &mut s.server,
        player,
        PlayerInput::Dig {
            action: DigAction::SwapHands,
            pos: BlockPos::new(0, 0, 0),
            face: Direction::Down,
            sequence: 0,
        },
    );
}

fn slot(s: &mut Scene, player: &Player, index: usize) -> Option<ItemSpec> {
    match s.ask(Question::Inventory(player.id)) {
        Answer::Inventory(inventory) => inventory.slots[index].clone(),
        other => panic!("expected the inventory, got {other:?}"),
    }
}

#[test]
fn a_swap_carries_the_items_after_the_swap_and_its_patch_replaces_them() {
    let mut s = scene_with(
        &["Alice"],
        vec![deciding(1, EventKind::PlayerSwapHandItems)],
    );
    let alice = s.players[0].clone();
    s.server
        .handle(Inbound::Console("give Alice diamond 2".to_owned()));
    swap(&mut s, &alice);
    let asked = last_asked(&s.plugin, EventKind::PlayerSwapHandItems);
    let EventPayload::PlayerSwapHandItems(PlayerSwapHandItems {
        main_hand_item,
        off_hand_item,
        ..
    }) = &asked.payload
    else {
        panic!("expected a swap");
    };
    assert_eq!(*main_hand_item, None);
    assert_eq!(*off_hand_item, Some(ItemSpec::of("minecraft:diamond", 2)));
    rule_on(
        &mut s.server,
        1,
        asked.id,
        false,
        Some(EventPatch::PlayerSwapHandItems(PlayerSwapHandItemsPatch {
            main_hand_item: Some(Some(ItemSpec::of("minecraft:stick", 1))),
            off_hand_item: None,
        })),
    );
    assert_eq!(
        slot(&mut s, &alice, 0),
        Some(ItemSpec::of("minecraft:stick", 1))
    );
    assert_eq!(
        slot(&mut s, &alice, 40),
        Some(ItemSpec::of("minecraft:diamond", 2))
    );
}

#[test]
fn a_cancelled_or_outdated_swap_changes_nothing() {
    let mut s = scene_with(
        &["Alice"],
        vec![deciding(1, EventKind::PlayerSwapHandItems)],
    );
    let alice = s.players[0].clone();
    s.server
        .handle(Inbound::Console("give Alice diamond 2".to_owned()));
    swap(&mut s, &alice);
    let asked = last_asked(&s.plugin, EventKind::PlayerSwapHandItems);
    rule_on(&mut s.server, 1, asked.id, true, None);
    assert_eq!(
        slot(&mut s, &alice, 0),
        Some(ItemSpec::of("minecraft:diamond", 2))
    );
    assert_eq!(slot(&mut s, &alice, 40), None);
    swap(&mut s, &alice);
    let asked = last_asked(&s.plugin, EventKind::PlayerSwapHandItems);
    s.server.handle(Inbound::Console("clear Alice".to_owned()));
    rule_on(&mut s.server, 1, asked.id, false, None);
    assert_eq!(slot(&mut s, &alice, 0), None);
    assert_eq!(slot(&mut s, &alice, 40), None);
}

#[test]
fn a_held_slot_change_of_a_player_who_died_meanwhile_is_sent_back() {
    let mut s = scene_with(&["Alice"], vec![deciding(1, EventKind::PlayerItemHeld)]);
    let alice = s.players[0].clone();
    send_input(&mut s.server, &alice, PlayerInput::HeldSlot(4));
    let asked = last_asked(&s.plugin, EventKind::PlayerItemHeld);
    s.act(to(&alice, PlayerOrder::SetHealth(0.0)));
    alice.link.take();
    rule_on(&mut s.server, 1, asked.id, false, None);
    assert_eq!(selected(&mut s, &alice), 0);
    assert_eq!(held_slots(&alice), [0]);
}

#[test]
fn a_flight_toggle_settled_after_flight_was_taken_away_is_refused() {
    let mut s = scene_with(&["Alice"], vec![deciding(1, EventKind::PlayerToggleFlight)]);
    let alice = s.players[0].clone();
    s.act(to(&alice, PlayerOrder::SetAllowFlight(true)));
    send_input(
        &mut s.server,
        &alice,
        PlayerInput::Abilities { flying: true },
    );
    let asked = last_asked(&s.plugin, EventKind::PlayerToggleFlight);
    s.act(to(&alice, PlayerOrder::SetAllowFlight(false)));
    alice.link.take();
    rule_on(&mut s.server, 1, asked.id, false, None);
    let Answer::Player(state) = s.ask(Question::Player(alice.id)) else {
        panic!("expected the player");
    };
    assert!(!state.flying);
    assert!(
        alice
            .link
            .take()
            .iter()
            .any(|event| matches!(event, ClientEvent::Abilities(_)))
    );
}

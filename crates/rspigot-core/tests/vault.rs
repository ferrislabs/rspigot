mod common;
#[path = "common/screen.rs"]
mod screen;
#[path = "common/seeded.rs"]
mod seeded;

use common::*;
use rspigot_blocks::BlockKind;
use rspigot_core::events::{ClientEvent, Inbound};
use rspigot_core::plugins::{Answer, EventKind, EventPayload, Question};
use rspigot_domain::BlockPos;
use screen::use_block;
use seeded::{Seed, world};

const VAULT: BlockPos = BlockPos::new(2, 64, 2);

fn console(s: &mut Scene, line: &str) {
    s.server.handle(Inbound::Console(line.to_owned()));
    s.server.deliver();
}

fn scene_with_vault(
    seed: Seed,
    handlers: Vec<rspigot_core::plugins::HandlerSpec>,
    gift: &str,
) -> (Scene, Player) {
    let mut s = world(vec![seed], handlers);
    let alice = s.players[0].clone();
    console(&mut s, "gamemode survival Alice");
    if !gift.is_empty() {
        console(&mut s, &format!("give Alice {gift}"));
    }
    alice.link.take();
    (s, alice)
}

fn vault() -> Seed {
    Seed::block(VAULT, BlockKind::Vault)
}

fn prop(s: &mut Scene, name: &str) -> String {
    match s.ask(Question::Block(VAULT)) {
        Answer::Block(state) => state
            .properties
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.clone())
            .unwrap_or_default(),
        other => panic!("expected a block, got {other:?}"),
    }
}

fn held(s: &mut Scene, alice: &Player, key: &str) -> u32 {
    match s.ask(Question::Inventory(alice.id)) {
        Answer::Inventory(inventory) => inventory
            .slots
            .iter()
            .flatten()
            .filter(|item| item.key == key)
            .map(|item| item.count as u32)
            .sum(),
        other => panic!("expected the inventory, got {other:?}"),
    }
}

fn level_events(alice: &Player) -> Vec<(i32, i32)> {
    alice
        .link
        .take()
        .into_iter()
        .filter_map(|event| match event {
            ClientEvent::LevelEvent { event, data, .. } => Some((event, data)),
            _ => None,
        })
        .collect()
}

#[test]
fn a_vault_wakes_up_for_a_player_in_range_and_shows_an_item() {
    let (mut s, alice) = scene_with_vault(vault(), Vec::new(), "");
    assert_eq!(prop(&mut s, "vault_state"), "inactive");
    s.tick(25);
    assert_eq!(prop(&mut s, "vault_state"), "active");
    assert!(level_events(&alice).contains(&(3015, 0)));
    let data = s.server.block_entity_nbt(VAULT).expect("block entity");
    assert!(
        data.get_compound("shared_data")
            .and_then(|shared| shared.get_compound("display_item"))
            .is_some()
    );
}

#[test]
fn a_vault_out_of_range_stays_inactive() {
    let far = BlockPos::new(30, 64, 30);
    let (mut s, _) = scene_with_vault(Seed::block(far, BlockKind::Vault), Vec::new(), "");
    s.tick(60);
    match s.ask(Question::Block(far)) {
        Answer::Block(state) => assert!(
            state
                .properties
                .contains(&("vault_state".into(), "inactive".into()))
        ),
        other => panic!("expected a block, got {other:?}"),
    }
}

#[test]
fn a_trial_key_opens_the_vault_ejects_the_loot_and_is_spent() {
    let (mut s, alice) = scene_with_vault(vault(), Vec::new(), "minecraft:trial_key");
    s.tick(25);
    alice.link.take();
    use_block(&mut s, &alice, VAULT);
    s.tick(1);
    assert_eq!(held(&mut s, &alice, "minecraft:trial_key"), 0);
    assert_eq!(prop(&mut s, "vault_state"), "unlocking");
    let mut seen = Vec::new();
    for _ in 0..200 {
        s.tick(1);
        seen.extend(level_events(&alice));
    }
    assert!(seen.iter().any(|(event, _)| *event == 3017));
}

#[test]
fn a_player_who_already_won_is_refused_and_keeps_the_key() {
    let (mut s, alice) = scene_with_vault(vault(), Vec::new(), "minecraft:trial_key 2");
    s.tick(25);
    use_block(&mut s, &alice, VAULT);
    s.tick(200);
    assert_eq!(held(&mut s, &alice, "minecraft:trial_key"), 1);
    use_block(&mut s, &alice, VAULT);
    s.tick(60);
    assert_eq!(held(&mut s, &alice, "minecraft:trial_key"), 1);
    assert_eq!(prop(&mut s, "vault_state"), "inactive");
}

#[test]
fn another_item_does_not_open_the_vault() {
    let (mut s, alice) = scene_with_vault(vault(), Vec::new(), "minecraft:stick");
    s.tick(25);
    use_block(&mut s, &alice, VAULT);
    s.tick(5);
    assert_eq!(held(&mut s, &alice, "minecraft:stick"), 1);
    assert_eq!(prop(&mut s, "vault_state"), "active");
}

#[test]
fn the_ominous_key_does_not_open_a_normal_vault() {
    let (mut s, alice) = scene_with_vault(vault(), Vec::new(), "minecraft:ominous_trial_key");
    s.tick(25);
    use_block(&mut s, &alice, VAULT);
    s.tick(5);
    assert_eq!(held(&mut s, &alice, "minecraft:ominous_trial_key"), 1);
}

#[test]
fn opening_the_vault_asks_block_dispense_loot_with_the_player() {
    let (mut s, alice) = scene_with_vault(
        vault(),
        vec![observe(1, EventKind::BlockDispenseLoot)],
        "minecraft:trial_key",
    );
    s.tick(25);
    use_block(&mut s, &alice, VAULT);
    s.tick(2);
    let events = settled_for(&s.plugin, EventKind::BlockDispenseLoot);
    assert_eq!(events.len(), 1);
    let EventPayload::BlockDispenseLoot(loot) = &events[0].payload else {
        panic!("expected a dispense loot");
    };
    assert_eq!(loot.block, VAULT);
    assert!(loot.player.is_some());
    assert!(!loot.dispensed_loot.is_empty());
}

#[test]
fn cancelling_the_dispense_keeps_the_vault_shut_but_spends_the_key() {
    let (mut s, alice) = scene_with_vault(
        vault(),
        vec![deciding(1, EventKind::BlockDispenseLoot)],
        "minecraft:trial_key",
    );
    s.tick(25);
    use_block(&mut s, &alice, VAULT);
    s.tick(1);
    let asked = asked_for(&s.plugin, EventKind::BlockDispenseLoot);
    assert_eq!(asked.len(), 1);
    rule_on(&mut s.server, 1, asked[0].id, true, None);
    s.tick(100);
    assert_eq!(prop(&mut s, "vault_state"), "active");
    assert!(s.server.ground_items().is_empty());
}

#[test]
fn the_display_item_is_asked_and_can_be_cancelled() {
    let (mut s, _alice) =
        scene_with_vault(vault(), vec![observe(1, EventKind::VaultDisplayItem)], "");
    s.tick(25);
    let events = settled_for(&s.plugin, EventKind::VaultDisplayItem);
    assert!(!events.is_empty());
    let EventPayload::VaultDisplayItem(display) = &events[0].payload else {
        panic!("expected a display item");
    };
    assert_eq!(display.block, VAULT);
    assert!(display.display_item.is_some());
}

#[test]
fn a_cancelled_display_item_leaves_the_vault_empty_handed() {
    let (mut s, _alice) =
        scene_with_vault(vault(), vec![deciding(1, EventKind::VaultDisplayItem)], "");
    let mut ruled: Vec<u64> = Vec::new();
    for _ in 0..40 {
        s.tick(1);
        while let Some(asked) = asked_for(&s.plugin, EventKind::VaultDisplayItem)
            .into_iter()
            .find(|asked| !ruled.contains(&asked.id))
        {
            rule_on(&mut s.server, 1, asked.id, true, None);
            ruled.push(asked.id);
        }
    }
    assert!(!ruled.is_empty());
    let data = s.server.block_entity_nbt(VAULT).expect("block entity");
    assert!(
        data.get_compound("shared_data")
            .and_then(|shared| shared.get_compound("display_item"))
            .is_none()
    );
}

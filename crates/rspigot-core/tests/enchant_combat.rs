mod common;

use common::*;
use rspigot_core::events::PlayerInput;
use rspigot_core::plugins::{
    Answer, EntityView, InventoryOrder, ItemSpec, Order, PlayerOrder, Question,
};
use rspigot_domain::{EntityId, Vec3};

const HOTBAR_FIRST: i32 = 0;
const ARMOR_SLOTS: [(i32, &str); 4] = [
    (39, "minecraft:diamond_helmet"),
    (38, "minecraft:diamond_chestplate"),
    (37, "minecraft:diamond_leggings"),
    (36, "minecraft:diamond_boots"),
];

fn enchanted(key: &str, enchantments: &[(&str, i32)]) -> ItemSpec {
    let mut spec = ItemSpec::of(key, 1);
    spec.enchantments = enchantments
        .iter()
        .map(|(name, level)| ((*name).to_owned(), *level))
        .collect();
    spec
}

fn equip(s: &mut Scene, slot: i32, item: ItemSpec) {
    equip_for(s, 0, slot, item);
}

fn equip_for(s: &mut Scene, who: usize, slot: i32, item: ItemSpec) {
    let player = s.players[who].id;
    s.act(Order::Inventory {
        player,
        order: InventoryOrder::Set {
            slot,
            item: Some(item),
        },
    });
}

fn spawn(s: &mut Scene, kind: &str) -> EntityView {
    let Answer::Spawned(view) = s.ask(Question::Spawn {
        kind: kind.into(),
        position: Vec3::new(1.5, 64.0, 0.5),
    }) else {
        panic!("expected the spawned {kind}");
    };
    view
}

fn details(s: &mut Scene, id: EntityId) -> (f32, i32) {
    match s.ask(Question::Entity(id)) {
        Answer::Entity(details) => (details.health.unwrap_or(0.0), details.fire_ticks),
        _ => panic!("expected the entity {id:?}"),
    }
}

fn hit(weapon: &str, enchantments: &[(&str, i32)], victim: &str) -> (f32, i32) {
    let mut s = scene_with(&["Alice"], Vec::new());
    equip(&mut s, HOTBAR_FIRST, enchanted(weapon, enchantments));
    let target = spawn(&mut s, victim);
    s.tick(25);
    let alice = s.players[0].clone();
    send_input(&mut s.server, &alice, PlayerInput::Attack(target.id));
    details(&mut s, target.id)
}

fn damage_taken(armor: &[(&str, i32)], amount: f64) -> f32 {
    let mut s = scene_with(&["Alice"], Vec::new());
    for (slot, key) in ARMOR_SLOTS {
        equip(&mut s, slot, enchanted(key, armor));
    }
    let player = s.players[0].id;
    s.act(Order::Player {
        player,
        order: PlayerOrder::SetHealth(20.0),
    });
    s.act(Order::Player {
        player,
        order: PlayerOrder::Damage(amount),
    });
    match s.ask(Question::Player(player)) {
        Answer::Player(state) => 20.0 - state.health,
        _ => panic!("expected Alice"),
    }
}

#[test]
fn a_plain_sword_hit_is_the_baseline() {
    let (health, fire) = hit("minecraft:diamond_sword", &[], "minecraft:cow");
    assert_eq!(health, 3.0);
    assert_eq!(fire, 0);
}

#[test]
fn sharpness_adds_to_the_hit() {
    let (health, _) = hit(
        "minecraft:diamond_sword",
        &[("minecraft:sharpness", 3)],
        "minecraft:cow",
    );
    assert_eq!(health, 1.0);
}

#[test]
fn smite_only_adds_against_the_undead() {
    let (zombie, _) = hit(
        "minecraft:diamond_sword",
        &[("minecraft:smite", 2)],
        "minecraft:husk",
    );
    assert!((zombie - 8.0).abs() < 0.5, "{zombie}");
    let (cow, _) = hit(
        "minecraft:diamond_sword",
        &[("minecraft:smite", 2)],
        "minecraft:cow",
    );
    assert_eq!(cow, 3.0);
}

#[test]
fn bane_of_arthropods_adds_against_spiders() {
    let (spider, _) = hit(
        "minecraft:diamond_sword",
        &[("minecraft:bane_of_arthropods", 1)],
        "minecraft:spider",
    );
    assert_eq!(spider, 16.0 - 9.5);
}

#[test]
fn fire_aspect_sets_the_victim_on_fire() {
    let (_, fire) = hit(
        "minecraft:diamond_sword",
        &[("minecraft:fire_aspect", 2)],
        "minecraft:cow",
    );
    assert!(fire > 0);
}

#[test]
fn protection_cuts_the_damage_taken() {
    let plain = damage_taken(&[], 10.0);
    let protected = damage_taken(&[("minecraft:protection", 4)], 10.0);
    assert!(plain > 0.0);
    assert!(
        (protected - plain * 0.36).abs() < 0.01,
        "{protected} vs {plain}"
    );
}

#[test]
fn fire_protection_does_not_cut_generic_damage() {
    let plain = damage_taken(&[], 10.0);
    let protected = damage_taken(&[("minecraft:fire_protection", 4)], 10.0);
    assert!((protected - plain).abs() < 0.01, "{protected} vs {plain}");
}

fn push_after_hit(enchantments: &[(&str, i32)]) -> f64 {
    let mut s = scene_with(&["Alice"], Vec::new());
    equip(
        &mut s,
        HOTBAR_FIRST,
        enchanted("minecraft:diamond_sword", enchantments),
    );
    let target = spawn(&mut s, "minecraft:cow");
    s.tick(25);
    let alice = s.players[0].clone();
    send_input(&mut s.server, &alice, PlayerInput::Attack(target.id));
    match s.ask(Question::Entity(target.id)) {
        Answer::Entity(details) => details.velocity.x.hypot(details.velocity.z),
        _ => panic!("expected the cow"),
    }
}

#[test]
fn knockback_pushes_the_victim_further() {
    let plain = push_after_hit(&[]);
    let pushed = push_after_hit(&[("minecraft:knockback", 2)]);
    assert!(plain > 0.0);
    assert!(pushed > plain * 2.0, "{pushed} vs {plain}");
}

#[test]
fn thorns_hurt_the_player_who_hit_the_wearer() {
    let mut s = scene_with(&["Alice", "Bob"], Vec::new());
    equip_for(
        &mut s,
        1,
        38,
        enchanted("minecraft:diamond_chestplate", &[("minecraft:thorns", 10)]),
    );
    s.tick(25);
    let alice = s.players[0].clone();
    let bob_entity = match s.ask(Question::Player(s.players[1].id)) {
        Answer::Player(state) => state.player.entity_id,
        _ => panic!("expected Bob"),
    };
    send_input(
        &mut s.server,
        &alice,
        PlayerInput::Attack(EntityId(bob_entity)),
    );
    let alice_health = match s.ask(Question::Player(alice.id)) {
        Answer::Player(state) => state.health,
        _ => panic!("expected Alice"),
    };
    assert!(alice_health < 20.0, "{alice_health}");
}

fn swept_cow_health(enchantments: &[(&str, i32)]) -> f32 {
    let mut s = scene_with(&["Alice"], Vec::new());
    equip(
        &mut s,
        HOTBAR_FIRST,
        enchanted("minecraft:diamond_sword", enchantments),
    );
    let struck = spawn(&mut s, "minecraft:cow");
    let swept = match s.ask(Question::Spawn {
        kind: "minecraft:cow".into(),
        position: Vec3::new(1.5, 64.0, 1.5),
    }) {
        Answer::Spawned(view) => view,
        _ => panic!("expected the second cow"),
    };
    s.tick(30);
    let alice = s.players[0].clone();
    send_input(
        &mut s.server,
        &alice,
        PlayerInput::Move {
            position: Some(Vec3::new(0.5, 64.0, 0.5)),
            rotation: None,
            on_ground: true,
        },
    );
    send_input(&mut s.server, &alice, PlayerInput::Attack(struck.id));
    details(&mut s, swept.id).0
}

#[test]
fn sweeping_edge_hurts_the_neighbours_of_the_target() {
    let plain = swept_cow_health(&[]);
    let edged = swept_cow_health(&[("minecraft:sweeping_edge", 3)]);
    assert!((10.0 - plain - 1.0).abs() < 0.01, "{plain}");
    assert!(plain - edged > 3.0, "{edged} vs {plain}");
}

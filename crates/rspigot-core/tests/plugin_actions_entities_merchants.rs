mod common;

use common::*;
use rspigot_core::events::ClientEvent;
use rspigot_core::plugins::{
    Answer, EntityOrder, ItemSpec, ObjectAnswer, ObjectOrder, ObjectQuestion, Order,
    PlayerObjectOrder, PlayerOrder, Question, Trade,
};
use rspigot_domain::{EntityId, Vec3};

fn settled() -> Scene {
    let mut s = scene(&["Alice"]);
    s.tick(60);
    s
}

fn spawn(s: &mut Scene, kind: &str, position: Vec3) -> EntityId {
    let Answer::Spawned(view) = s.ask(Question::Spawn {
        kind: kind.into(),
        position,
    }) else {
        panic!("expected the spawned {kind}");
    };
    view.id
}

fn emerald_for_diamond() -> Trade {
    Trade {
        result: ItemSpec::of("minecraft:diamond", 2),
        ingredients: vec![ItemSpec::of("minecraft:emerald", 5)],
        uses: 1,
        max_uses: 9,
        experience_reward: false,
        villager_experience: 4,
        price_multiplier: 0.5,
        demand: 0,
        special_price: 0,
    }
}

fn set_trades(s: &mut Scene, merchant: EntityId, trades: Vec<Trade>) {
    s.act(Order::Entity {
        entity: merchant,
        order: EntityOrder::Object(ObjectOrder::SetTrades(trades)),
    });
}

fn trades(s: &mut Scene, merchant: EntityId) -> Vec<Trade> {
    match s.ask(Question::Object(ObjectQuestion::Trades(merchant))) {
        Answer::Object(ObjectAnswer::Trades(trades)) => trades,
        other => panic!("expected trades, got {other:?}"),
    }
}

#[test]
fn the_recipes_of_a_villager_are_replaced_and_read_back() {
    let mut s = settled();
    let villager = spawn(&mut s, "minecraft:villager", Vec3::new(2.5, 64.0, 2.5));

    set_trades(&mut s, villager, vec![emerald_for_diamond()]);

    assert!(s.warnings().is_empty());
    let held = trades(&mut s, villager);
    assert_eq!(held.len(), 1);
    assert_eq!(held[0].result.key, "minecraft:diamond");
    assert_eq!(held[0].result.count, 2);
    assert_eq!(held[0].ingredients[0].key, "minecraft:emerald");
    assert_eq!(held[0].ingredients[0].count, 5);
    assert_eq!(held[0].uses, 1);
    assert_eq!(held[0].max_uses, 9);
    assert_eq!(held[0].villager_experience, 4);
    assert!(!held[0].experience_reward);
}

#[test]
fn an_invalid_recipe_or_a_non_merchant_is_refused_and_changes_nothing() {
    let mut s = settled();
    let villager = spawn(&mut s, "minecraft:villager", Vec3::new(2.5, 64.0, 2.5));
    let cow = spawn(&mut s, "minecraft:cow", Vec3::new(4.5, 64.0, 2.5));
    set_trades(&mut s, villager, vec![emerald_for_diamond()]);

    let mut no_ingredient = emerald_for_diamond();
    no_ingredient.ingredients.clear();
    set_trades(&mut s, villager, vec![no_ingredient]);
    set_trades(&mut s, cow, vec![emerald_for_diamond()]);

    assert_eq!(s.warnings().len(), 2);
    assert_eq!(trades(&mut s, villager).len(), 1);
    assert!(matches!(
        s.ask(Question::Object(ObjectQuestion::Trades(cow))),
        Answer::Failed(_)
    ));
}

#[test]
fn opening_a_merchant_shows_the_trade_window_with_the_plugin_recipes() {
    let mut s = settled();
    let alice = s.players[0].clone();
    let villager = spawn(&mut s, "minecraft:villager", Vec3::new(1.5, 64.0, 1.5));
    set_trades(&mut s, villager, vec![emerald_for_diamond()]);
    alice.link.take();

    s.act(to(
        &alice,
        PlayerOrder::Object(PlayerObjectOrder::OpenMerchant(villager)),
    ));
    s.tick(2);

    assert!(s.warnings().is_empty());
    let events = alice.link.take();
    assert!(
        events
            .iter()
            .any(|event| matches!(event, ClientEvent::OpenScreen { .. }))
    );
    let offers = events
        .iter()
        .find_map(|event| match event {
            ClientEvent::MerchantOffers { offers, .. } => Some(offers),
            _ => None,
        })
        .expect("the merchant offers are sent");
    assert_eq!(offers.len(), 1);
    assert_eq!(offers[0].max_uses, 9);
}

#[test]
fn a_merchant_without_recipes_cannot_be_opened() {
    let mut s = settled();
    let alice = s.players[0].clone();
    let villager = spawn(&mut s, "minecraft:villager", Vec3::new(1.5, 64.0, 1.5));
    set_trades(&mut s, villager, Vec::new());
    alice.link.take();

    s.act(to(
        &alice,
        PlayerOrder::Object(PlayerObjectOrder::OpenMerchant(villager)),
    ));

    assert_eq!(s.warnings().len(), 1);
    assert!(
        !alice
            .link
            .take()
            .iter()
            .any(|event| matches!(event, ClientEvent::OpenScreen { .. }))
    );
}

mod common;
#[path = "common/screen.rs"]
mod screen;

use common::*;
use rspigot_core::config::PluginSettings;
use rspigot_core::events::{ClientEvent, Hand, Inbound, PlayerInput};
use rspigot_core::plugins::events::{
    EntityTransformReason, EventResult, TradeSelectPatch, VillagerAcquireTradePatch,
};
use rspigot_core::plugins::{EventKind, EventPatch, EventPayload};
use rspigot_domain::{BlockPos, EntityId, Vec3};
use rspigot_entities::EntityKind as Kind;
use screen::*;

const FARMER: &str = "{NoAI:1b,VillagerDataFinalized:1b,VillagerData:{type:\"minecraft:plains\",profession:\"minecraft:farmer\",level:1}}";

fn console(s: &mut Scene, line: &str) {
    s.server.handle(Inbound::Console(line.to_owned()));
    s.server.deliver();
}

fn ready(handlers: Vec<rspigot_core::plugins::HandlerSpec>) -> Scene {
    let mut s = scene_with(&["Alice"], handlers);
    stream_chunk_of(&mut s, BlockPos::new(1, 64, 1));
    s
}

fn patient(handlers: Vec<rspigot_core::plugins::HandlerSpec>) -> Scene {
    let mut server = server(PluginSettings {
        decision_ticks: 1_000,
        ..PluginSettings::default()
    });
    let plugin = connect_with(&mut server, 1, "tester", handlers);
    let alice = join(&mut server, "Alice");
    server.deliver();
    alice.link.take();
    let mut s = Scene {
        server,
        plugin,
        players: vec![alice],
    };
    stream_chunk_of(&mut s, BlockPos::new(1, 64, 1));
    s
}

fn summon(s: &mut Scene, kind: Kind, nbt: &str) -> EntityId {
    console(s, &format!("summon {} 2.5 64 0.5 {nbt}", kind.name()));
    s.server.entities_of(kind)[0].id
}

fn interact(s: &mut Scene, target: EntityId) {
    let alice = s.players[0].clone();
    send_input(
        &mut s.server,
        &alice,
        PlayerInput::Interact {
            target,
            hand: Hand::Main,
            sneaking: false,
            location: Vec3::new(0.0, 0.5, 0.0),
        },
    );
}

fn recipes(s: &Scene, id: EntityId) -> usize {
    s.server
        .saved_entity(id)
        .and_then(|nbt| nbt.get_compound("Offers").cloned())
        .and_then(|offers| offers.get_list("Recipes").map(<[_]>::len))
        .unwrap_or(0)
}

#[test]
fn every_offer_a_villager_acquires_is_announced_and_a_cancelled_one_is_dropped() {
    let mut s = ready(vec![deciding(1, EventKind::VillagerAcquireTrade)]);
    let villager = summon(&mut s, Kind::Villager, FARMER);
    interact(&mut s, villager);
    let asked = asked_for(&s.plugin, EventKind::VillagerAcquireTrade);
    assert_eq!(asked.len(), 2);
    let EventPayload::VillagerAcquireTrade(event) = &asked[0].payload else {
        panic!("an acquire event");
    };
    assert_eq!(event.entity.id, villager);
    assert!(event.recipe.max_uses >= 1);
    assert!(!event.recipe.ingredients.is_empty());
    rule_on(&mut s.server, 1, asked[0].id, true, None);
    rule_on(&mut s.server, 1, asked[1].id, false, None);
    assert_eq!(recipes(&s, villager), 1);
}

#[test]
fn a_plugin_can_replace_the_recipe_an_acquired_offer_becomes() {
    let mut s = ready(vec![deciding(1, EventKind::VillagerAcquireTrade)]);
    let villager = summon(&mut s, Kind::Villager, FARMER);
    interact(&mut s, villager);
    let asked = asked_for(&s.plugin, EventKind::VillagerAcquireTrade);
    let EventPayload::VillagerAcquireTrade(event) = asked[0].payload.clone() else {
        panic!("an acquire event");
    };
    let mut replaced = event.recipe;
    replaced.max_uses = 99;
    rule_on(
        &mut s.server,
        1,
        asked[0].id,
        false,
        Some(EventPatch::VillagerAcquireTrade(
            VillagerAcquireTradePatch {
                recipe: Some(replaced),
            },
        )),
    );
    rule_on(&mut s.server, 1, asked[1].id, true, None);
    let nbt = s.server.saved_entity(villager).unwrap();
    let uses = nbt
        .get_compound("Offers")
        .and_then(|offers| offers.get_list("Recipes"))
        .and_then(|recipes| recipes.first())
        .and_then(|recipe| recipe.as_compound())
        .and_then(|recipe| recipe.get_i32("maxUses"));
    assert_eq!(uses, Some(99));
}

#[test]
fn a_cancelled_trade_selection_leaves_the_payment_slots_alone() {
    let mut s = ready(vec![
        deciding(1, EventKind::TradeSelect),
        observe(2, EventKind::TradeSelect),
    ]);
    let alice = s.players[0].clone();
    let villager = summon(&mut s, Kind::Villager, FARMER);
    interact(&mut s, villager);
    let events = alice.link.take();
    let offers = events
        .iter()
        .rev()
        .find_map(|event| match event {
            ClientEvent::MerchantOffers { window, offers, .. } => Some((*window, offers.clone())),
            _ => None,
        })
        .expect("offers");
    let cost = offers.1[0].cost_a.item.kind().unwrap().name();
    give(&mut s, &alice, 20, cost, 40);
    alice.link.take();
    s.server.handle(Inbound::Input {
        player: alice.id,
        input: PlayerInput::SelectTrade(0),
    });
    s.server.deliver();
    let asked = last_asked(&s.plugin, EventKind::TradeSelect);
    let EventPayload::TradeSelect(event) = &asked.payload else {
        panic!("a trade select");
    };
    assert_eq!(event.index, 0);
    assert_eq!(event.who_clicked.id, alice.id);
    rule_on(
        &mut s.server,
        1,
        asked.id,
        false,
        Some(EventPatch::TradeSelect(TradeSelectPatch {
            result: Some(EventResult::Deny),
        })),
    );
    let mut view = View {
        window: offers.0,
        state: 0,
        slots: vec![None; 39],
        cursor: None,
        opened: true,
        data: Vec::new(),
    };
    view.apply(&alice.link.take());
    assert!(view.at(0).is_none());
    assert!(
        settled_for(&s.plugin, EventKind::TradeSelect)
            .last()
            .expect("settled")
            .cancelled
    );
}

#[test]
fn a_cure_announces_the_transformation_and_a_cancel_stops_the_cure() {
    let mut s = ready(vec![deciding(1, EventKind::EntityTransform)]);
    let zombie = summon(&mut s, Kind::ZombieVillager, "{NoAI:1b,ConversionTime:2}");
    s.tick(3);
    let asked = last_asked(&s.plugin, EventKind::EntityTransform);
    let EventPayload::EntityTransform(event) = &asked.payload else {
        panic!("a transform");
    };
    assert_eq!(event.transform_reason, EntityTransformReason::Cured);
    assert_eq!(event.entity.id, zombie);
    assert_eq!(event.transformed_entities.len(), 1);
    assert!(s.server.entities_of(Kind::Villager).is_empty());
    rule_on(&mut s.server, 1, asked.id, true, None);
    s.tick(2);
    assert_eq!(s.server.entities_of(Kind::ZombieVillager).len(), 1);
    assert!(s.server.entities_of(Kind::Villager).is_empty());
    assert_eq!(
        s.server
            .saved_entity(zombie)
            .unwrap()
            .get_i32("ConversionTime"),
        Some(-1)
    );
}

#[test]
fn an_allowed_transformation_swaps_the_zombie_villager_for_a_villager() {
    let mut s = ready(vec![deciding(1, EventKind::EntityTransform)]);
    summon(&mut s, Kind::ZombieVillager, "{NoAI:1b,ConversionTime:2}");
    s.tick(3);
    let asked = last_asked(&s.plugin, EventKind::EntityTransform);
    rule_on(&mut s.server, 1, asked.id, false, None);
    s.tick(2);
    assert!(s.server.entities_of(Kind::ZombieVillager).is_empty());
    assert_eq!(s.server.entities_of(Kind::Villager).len(), 1);
}

const SMITH: &str = "{VillagerDataFinalized:1b,VillagerData:{type:\"minecraft:plains\",profession:\"minecraft:weaponsmith\",level:1},\
Brain:{memories:{\"minecraft:job_site\":{value:{dimension:\"minecraft:overworld\",pos:[I;3,64,3]}}}},\
Offers:{Recipes:[{buy:{id:\"minecraft:emerald\",count:1},sell:{id:\"minecraft:bread\",count:1},uses:4,maxUses:4,xp:1,demand:0,priceMultiplier:0.05f,specialPrice:0,rewardExp:1b}]}}";

fn uses_of_first(s: &Scene, id: EntityId) -> Option<i32> {
    s.server
        .saved_entity(id)?
        .get_compound("Offers")?
        .get_list("Recipes")?
        .first()?
        .as_compound()?
        .get_i32("uses")
}

fn working_smith(mut s: Scene) -> (Scene, EntityId) {
    s.act(rspigot_core::plugins::Order::Block {
        pos: BlockPos::new(3, 64, 3),
        order: rspigot_core::plugins::BlockOrder::Set("minecraft:grindstone".to_owned()),
    });
    console(&mut s, "time set 4000");
    console(
        &mut s,
        &format!("summon minecraft:villager 3.5 64 4.5 {SMITH}"),
    );
    let smith = s.server.entities_of(Kind::Villager)[0].id;
    (s, smith)
}

#[test]
fn a_villager_that_works_restocks_and_each_offer_is_announced_to_plugins() {
    let (mut s, smith) = working_smith(ready(vec![observe(1, EventKind::VillagerReplenishTrade)]));
    assert_eq!(uses_of_first(&s, smith), Some(4));
    s.tick(400);
    assert_eq!(uses_of_first(&s, smith), Some(0));
    let announced = settled_for(&s.plugin, EventKind::VillagerReplenishTrade);
    assert!(!announced.is_empty());
    assert!(announced.iter().all(|event| !event.cancelled));
}

#[test]
fn a_cancelled_replenish_keeps_the_uses_of_the_offer() {
    let (mut s, smith) = working_smith(patient(vec![deciding(
        1,
        EventKind::VillagerReplenishTrade,
    )]));
    s.tick(400);
    let asked = asked_for(&s.plugin, EventKind::VillagerReplenishTrade);
    assert!(!asked.is_empty());
    for event in &asked {
        rule_on(&mut s.server, 1, event.id, true, None);
    }
    s.tick(1);
    assert_eq!(uses_of_first(&s, smith), Some(4));
}

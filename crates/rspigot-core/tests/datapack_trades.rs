mod common;
#[path = "common/screen.rs"]
mod screen;

use std::sync::{Arc, Mutex};

use common::*;
use rspigot_core::config::PluginSettings;
use rspigot_core::events::{ClientEvent, Hand, Inbound, PlayerInput};
use rspigot_core::ports::{
    DatapackError, DatapackSource, PackContents, PackExtras, PackFile, PackRegistryFile,
};
use rspigot_domain::{BlockPos, Identifier, Vec3};
use rspigot_entities::EntityKind as Kind;

#[derive(Default)]
struct Packs(Mutex<Vec<PackExtras>>);

impl DatapackSource for Packs {
    fn read_packs(&self) -> Result<Vec<PackContents>, DatapackError> {
        Ok(vec![PackContents {
            name: "main".to_owned(),
            ..PackContents::default()
        }])
    }

    fn read_extras(&self) -> Result<Vec<PackExtras>, DatapackError> {
        Ok(self.0.lock().expect("packs").clone())
    }
}

fn extras(files: &[(&str, &str, &str)]) -> Vec<PackExtras> {
    vec![PackExtras {
        name: "main".to_owned(),
        registries: files
            .iter()
            .map(|(registry, id, text)| PackRegistryFile {
                registry: (*registry).to_owned(),
                file: PackFile {
                    id: Identifier::parse(id).expect("id"),
                    text: (*text).to_owned(),
                },
            })
            .collect(),
        ..PackExtras::default()
    }]
}

const GEM: &str = r#"{"wants":{"id":"minecraft:emerald","count":3},"gives":{"id":"minecraft:diamond","count":2},"max_uses":7}"#;
const ARMORER: &str = "{NoAI:1b,VillagerDataFinalized:1b,VillagerData:{type:\"minecraft:plains\",profession:\"minecraft:armorer\",level:1}}";

#[test]
fn a_datapack_trade_set_replaces_what_a_villager_offers() {
    let packs = Arc::new(Packs(Mutex::new(extras(&[
        ("villager_trade", "t:gem", GEM),
        (
            "trade_set",
            "minecraft:armorer/level_1",
            r#"{"amount":1,"trades":["t:gem"]}"#,
        ),
    ]))));
    let server = server(PluginSettings::default()).with_datapacks(packs);
    let mut s = scene_on(server, &["Alice"], Vec::new());
    stream_chunk_of(&mut s, BlockPos::new(1, 64, 1));
    let known: Vec<_> = s.server.entities_of(Kind::Villager);
    assert!(known.is_empty());
    s.server.handle(Inbound::Console(format!(
        "summon minecraft:villager 2.5 64 0.5 {ARMORER}"
    )));
    s.server.deliver();
    let villager = s.server.entities_of(Kind::Villager)[0].id;
    let alice = s.players[0].clone();
    alice.link.take();
    common::send_input(
        &mut s.server,
        &alice,
        PlayerInput::Interact {
            target: villager,
            hand: Hand::Main,
            sneaking: false,
            location: Vec3::new(0.0, 0.5, 0.0),
        },
    );
    let offers = alice
        .link
        .take()
        .into_iter()
        .find_map(|event| match event {
            ClientEvent::MerchantOffers { offers, .. } => Some(offers),
            _ => None,
        })
        .expect("offers");
    assert_eq!(offers.len(), 1);
    assert_eq!(
        offers[0].cost_a.item.kind().unwrap().name(),
        "minecraft:emerald"
    );
    assert_eq!(offers[0].cost_a.count, 3);
    assert_eq!(
        offers[0].result.item.kind().unwrap().name(),
        "minecraft:diamond"
    );
    assert_eq!(offers[0].max_uses, 7);
}

#[test]
fn a_datapack_trial_spawner_config_overrides_and_adds_configs() {
    let packs = Arc::new(Packs(Mutex::new(extras(&[
        (
            "trial_spawner",
            "t:custom",
            r#"{"total_mobs":11.0,"spawn_potentials":[{"data":{"entity":{"id":"minecraft:husk"}},"weight":1}]}"#,
        ),
        (
            "trial_spawner",
            "minecraft:trial_chamber/breeze/ominous",
            r#"{"total_mobs":99.0}"#,
        ),
    ]))));
    let server = server(PluginSettings::default()).with_datapacks(packs);
    let _s = scene_on(server, &["Alice"], Vec::new());
    let custom = rspigot_data::trial_spawner::config("t:custom").expect("custom config");
    let resolved = rspigot_domain::trial_spawner::Config::of_registry(custom);
    assert_eq!(resolved.total_mobs, 11.0);
    assert_eq!(resolved.spawn_potentials.len(), 1);
    let overridden =
        rspigot_data::trial_spawner::config("trial_chamber/breeze/ominous").expect("config");
    assert_eq!(overridden.total_mobs, Some(99.0));
    assert!(overridden.spawn_potentials.is_empty());
}

const BOOK: &str = r##"{"wants":{"id":"minecraft:emerald","count":5},"additional_wants":{"id":"minecraft:book"},"gives":{"id":"minecraft:enchanted_book"},"given_item_modifier":[{"type":"minecraft:enchant_randomly","include_additional_cost_component":true,"only_compatible":false,"options":"#minecraft:tradeable"},{"type":"minecraft:filtered","item_filter":{"items":"minecraft:enchanted_book","predicates":{"minecraft:stored_enchantments":[{}]}},"on_fail":{"type":"minecraft:discard"}}]}"##;

#[test]
fn a_datapack_enchanted_book_trade_runs_its_modifiers() {
    let packs = Arc::new(Packs(Mutex::new(extras(&[
        ("villager_trade", "t:book", BOOK),
        (
            "trade_set",
            "minecraft:armorer/level_1",
            r#"{"amount":1,"trades":["t:book"]}"#,
        ),
    ]))));
    let server = server(PluginSettings::default()).with_datapacks(packs);
    let mut s = scene_on(server, &["Alice"], Vec::new());
    stream_chunk_of(&mut s, BlockPos::new(1, 64, 1));
    s.server.handle(Inbound::Console(format!(
        "summon minecraft:villager 2.5 64 0.5 {ARMORER}"
    )));
    s.server.deliver();
    let villager = s.server.entities_of(Kind::Villager)[0].id;
    let alice = s.players[0].clone();
    alice.link.take();
    common::send_input(
        &mut s.server,
        &alice,
        PlayerInput::Interact {
            target: villager,
            hand: Hand::Main,
            sneaking: false,
            location: Vec3::new(0.0, 0.5, 0.0),
        },
    );
    let offers = alice
        .link
        .take()
        .into_iter()
        .find_map(|event| match event {
            ClientEvent::MerchantOffers { offers, .. } => Some(offers),
            _ => None,
        })
        .expect("offers");
    assert_eq!(offers.len(), 1);
    let result = &offers[0].result;
    assert_eq!(
        result.item.kind().unwrap().name(),
        "minecraft:enchanted_book"
    );
    assert!(
        result
            .components
            .contains_key("minecraft:stored_enchantments")
    );
    assert!(offers[0].cost_b.is_some());
}

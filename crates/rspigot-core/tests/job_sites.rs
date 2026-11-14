mod common;

use common::*;
use rspigot_core::config::PluginSettings;
use rspigot_core::events::Inbound;
use rspigot_core::plugins::events::{VillagerCareerChangePatch, VillagerCareerChangeReason};
use rspigot_core::plugins::{BlockOrder, EventKind, EventPatch, EventPayload, Order};
use rspigot_domain::BlockPos;
use rspigot_entities::EntityKind as Kind;

fn console(s: &mut Scene, line: &str) {
    s.server.handle(Inbound::Console(line.to_owned()));
    s.server.deliver();
}

fn set(s: &mut Scene, pos: BlockPos, kind: &str, properties: &[(&str, &str)]) {
    s.act(Order::Block {
        pos,
        order: BlockOrder::SetWith {
            kind: kind.to_owned(),
            properties: properties
                .iter()
                .map(|(name, value)| ((*name).to_owned(), (*value).to_owned()))
                .collect(),
        },
    });
}

const LECTERN: BlockPos = BlockPos::new(8, 64, 8);
const LIBRARIAN: &str =
    "{VillagerData:{type:\"minecraft:plains\",profession:\"minecraft:librarian\",level:2},Xp:20}";

fn profession(s: &Scene, id: rspigot_domain::EntityId) -> String {
    s.server
        .saved_entity(id)
        .and_then(|nbt| nbt.get_compound("VillagerData").cloned())
        .and_then(|data| data.get_str("profession").map(str::to_owned))
        .expect("a profession")
}

fn cage(s: &mut Scene, at: BlockPos) {
    for x in -1..=1 {
        for z in -1..=1 {
            for y in 0..=2 {
                if (x, z) != (0, 0) || y == 2 {
                    set(s, at.offset(x, y, z), "minecraft:stone", &[]);
                }
            }
        }
    }
}

fn contest(caged: bool) -> (String, String) {
    let server = configured_level(
        |_| {},
        |level| {
            level.day_time = 4_000;
            level.daylight_cycle = false;
        },
    )
    .with_rng_seed(5);
    let mut s = scene_on(server, &["Alice"], Vec::new());
    stream_chunk_of(&mut s, BlockPos::new(1, 64, 1));
    set(&mut s, LECTERN, "minecraft:lectern", &[]);
    let peer_at = BlockPos::new(12, 64, 8);
    if caged {
        cage(&mut s, peer_at);
    }
    console(
        &mut s,
        &format!("summon minecraft:villager 12.5 64 8.5 {LIBRARIAN}"),
    );
    let peer = s
        .server
        .entities_of(Kind::Villager)
        .into_iter()
        .map(|entity| entity.id)
        .next_back()
        .expect("a librarian");
    console(&mut s, "summon minecraft:villager 7.5 64 8.5 {}");
    let jobless = s
        .server
        .entities_of(Kind::Villager)
        .into_iter()
        .map(|entity| entity.id)
        .find(|id| *id != peer)
        .expect("a villager");
    s.tick(1_200);
    (profession(&s, jobless), profession(&s, peer))
}

#[test]
fn a_jobless_villager_takes_the_lectern_when_the_librarian_peer_is_walled_in() {
    let (jobless, _) = contest(true);
    assert_eq!(jobless, "minecraft:librarian");
}

#[test]
fn a_jobless_villager_leaves_the_lectern_to_a_librarian_peer_that_can_walk_to_it() {
    let (jobless, peer) = contest(false);
    assert_eq!(jobless, "minecraft:none");
    assert_eq!(peer, "minecraft:librarian");
}

fn careers(handlers: Vec<rspigot_core::plugins::HandlerSpec>, lectern: bool) -> Scene {
    let server = configured_level(
        |config| {
            config.plugins = PluginSettings {
                decision_ticks: 1_000,
                ..PluginSettings::default()
            };
        },
        |level| {
            level.day_time = 4_000;
            level.daylight_cycle = false;
        },
    )
    .with_rng_seed(5);
    let mut s = scene_on(server, &["Alice"], handlers);
    stream_chunk_of(&mut s, BlockPos::new(1, 64, 1));
    if lectern {
        set(&mut s, LECTERN, "minecraft:lectern", &[]);
    }
    s
}

fn lone_villager(s: &mut Scene, nbt: &str) -> rspigot_domain::EntityId {
    console(s, &format!("summon minecraft:villager 7.5 64 8.5 {nbt}"));
    s.server.entities_of(Kind::Villager)[0].id
}

fn career_events(s: &Scene) -> Vec<rspigot_core::plugins::events::VillagerCareerChange> {
    fired(&s.plugin)
        .into_iter()
        .filter_map(|event| match event.payload {
            EventPayload::VillagerCareerChange(change) => Some(change),
            _ => None,
        })
        .collect()
}

#[test]
fn a_villager_taking_a_job_site_announces_its_new_profession_and_becomes_it() {
    let mut s = careers(vec![observe(1, EventKind::VillagerCareerChange)], true);
    let villager = lone_villager(&mut s, "{}");
    s.tick(400);
    assert_eq!(profession(&s, villager), "minecraft:librarian");
    let events = career_events(&s);
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].entity.id, villager);
    assert_eq!(events[0].profession, "minecraft:librarian");
    assert_eq!(events[0].reason, VillagerCareerChangeReason::Employed);
}

#[test]
fn a_cancelled_employment_keeps_the_villager_jobless() {
    let mut s = careers(vec![deciding(1, EventKind::VillagerCareerChange)], true);
    let villager = lone_villager(&mut s, "{}");
    s.tick(400);
    let asked = last_asked(&s.plugin, EventKind::VillagerCareerChange);
    rule_on(&mut s.server, 1, asked.id, true, None);
    s.tick(2);
    assert_eq!(profession(&s, villager), "minecraft:none");
}

#[test]
fn a_patched_profession_replaces_the_one_the_job_site_gives() {
    let mut s = careers(vec![deciding(1, EventKind::VillagerCareerChange)], true);
    let villager = lone_villager(&mut s, "{}");
    s.tick(400);
    let asked = last_asked(&s.plugin, EventKind::VillagerCareerChange);
    rule_on(
        &mut s.server,
        1,
        asked.id,
        false,
        Some(EventPatch::VillagerCareerChange(
            VillagerCareerChangePatch {
                profession: Some("minecraft:farmer".to_owned()),
            },
        )),
    );
    s.tick(2);
    assert_eq!(profession(&s, villager), "minecraft:farmer");
}

#[test]
fn an_unemployed_novice_losing_its_profession_announces_losing_job_with_none() {
    let mut s = careers(vec![deciding(1, EventKind::VillagerCareerChange)], false);
    let villager = lone_villager(
        &mut s,
        "{VillagerData:{type:\"minecraft:plains\",profession:\"minecraft:farmer\",level:1},Xp:0}",
    );
    s.tick(100);
    let asked = last_asked(&s.plugin, EventKind::VillagerCareerChange);
    let EventPayload::VillagerCareerChange(change) = &asked.payload else {
        panic!("a career change");
    };
    assert_eq!(change.reason, VillagerCareerChangeReason::LosingJob);
    assert_eq!(change.profession, "minecraft:none");
    rule_on(&mut s.server, 1, asked.id, true, None);
    s.tick(1);
    assert_eq!(profession(&s, villager), "minecraft:farmer");
}

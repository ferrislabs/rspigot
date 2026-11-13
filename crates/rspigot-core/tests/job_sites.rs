mod common;

use common::*;
use rspigot_core::events::Inbound;
use rspigot_core::plugins::{BlockOrder, Order};
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

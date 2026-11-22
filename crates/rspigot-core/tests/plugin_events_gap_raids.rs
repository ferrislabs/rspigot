mod common;

use common::*;
use rspigot_core::events::Inbound;
use rspigot_core::plugins::{BlockOrder, EventKind, EventPayload, Order};
use rspigot_domain::BlockPos;
use rspigot_domain::world::Difficulty;

fn console(s: &mut Scene, line: &str) {
    s.server.handle(Inbound::Console(line.to_owned()));
    s.server.deliver();
}

fn raid_scene() -> Scene {
    let server = configured_level(
        |config| config.difficulty = Difficulty::Easy,
        |level| {
            level.day_time = 4_000;
            level.daylight_cycle = false;
        },
    )
    .with_rng_seed(11);
    let mut s = scene_on(
        server,
        &["Alice"],
        vec![
            observe(2, EventKind::RaidSpawnWave),
            observe(5, EventKind::RaidTrigger),
        ],
    );
    stream_chunk_of(&mut s, BlockPos::new(1, 64, 1));
    s.act(Order::Block {
        pos: BlockPos::new(8, 64, 8),
        order: BlockOrder::SetWith {
            kind: "minecraft:lectern".to_owned(),
            properties: Vec::new(),
        },
    });
    console(
        &mut s,
        "summon minecraft:villager 7.5 64 8.5 {VillagerData:{type:\"minecraft:plains\",profession:\"minecraft:librarian\",level:2},Xp:20}",
    );
    s.tick(200);
    s
}

#[test]
fn raid_events_carry_the_raid_handle() {
    let mut s = raid_scene();
    console(&mut s, "effect give Alice minecraft:bad_omen 120 0");
    s.tick(650);
    for _ in 0..800 {
        if !settled_for(&s.plugin, EventKind::RaidSpawnWave).is_empty() {
            break;
        }
        s.tick(1);
    }
    let waves = settled_for(&s.plugin, EventKind::RaidSpawnWave);
    let EventPayload::RaidSpawnWave(wave) = &waves[0].payload else {
        panic!("a spawn wave");
    };
    let real = s.server.raids().iter().next().expect("a raid");
    assert_eq!(wave.raid.id, real.id);
    assert_eq!(wave.raid.center, real.center());
    assert_eq!(wave.raid.total_groups, real.num_groups());
    assert!(wave.raid.total_groups > 0);
    assert_eq!(wave.raid.world, "minecraft:overworld");
    let triggers = settled_for(&s.plugin, EventKind::RaidTrigger);
    let EventPayload::RaidTrigger(trigger) = &triggers[0].payload else {
        panic!("a trigger");
    };
    assert_eq!(trigger.raid.status, "ongoing");
}

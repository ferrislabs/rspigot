mod common;
#[path = "common/screen.rs"]
mod screen;
#[path = "common/seeded.rs"]
mod seeded;

use common::*;
use rspigot_blocks::BlockKind;
use rspigot_domain::BlockPos;
use rspigot_entities::EntityKind as Kind;
use rspigot_nbt::Compound;

const SPAWNER: BlockPos = BlockPos::new(4, 64, 4);

fn spawner() -> seeded::Seed {
    seeded::Seed::block(SPAWNER, BlockKind::Spawner)
        .carrying("Delay", 0i16)
        .carrying("SpawnCount", 20i16)
        .carrying("MaxNearbyEntities", 100i16)
        .carrying("MinSpawnDelay", 400i16)
        .carrying("MaxSpawnDelay", 400i16)
        .carrying(
            "SpawnData",
            Compound::new().with("entity", Compound::new().with("id", "minecraft:villager")),
        )
}

fn spawned(nerf: bool) -> Vec<bool> {
    let mut s = seeded::world_configured(vec![spawner()], Vec::new(), false, |config| {
        config.world_keys.nerf_spawner_mobs = nerf
    });
    s.tick(3);
    s.server
        .entities_of(Kind::Villager)
        .iter()
        .map(|entity| entity.no_ai)
        .collect()
}

#[test]
fn nerf_spawner_mobs_switches_the_ai_of_spawned_mobs_off() {
    let nerfed = spawned(true);
    assert!(!nerfed.is_empty());
    assert!(nerfed.iter().all(|no_ai| *no_ai));
}

#[test]
fn spawner_mobs_keep_their_ai_by_default() {
    let normal = spawned(false);
    assert!(!normal.is_empty());
    assert!(normal.iter().all(|no_ai| !*no_ai));
}

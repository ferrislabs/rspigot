mod common;
#[path = "common/screen.rs"]
mod screen;
#[path = "common/seeded.rs"]
mod seeded;

use common::*;
use rspigot_blocks::BlockKind;
use rspigot_core::events::Inbound;
use std::sync::Arc;

use rspigot_core::ports::{DatapackError, DatapackSource, PackContents, PackFile};
use rspigot_domain::{BlockPos, Identifier};
use screen::open_block;
use seeded::{Seed, world_with_datapacks};
use serde_json::json;

const CHEST: BlockPos = BlockPos::new(3, 64, 3);

fn table() -> serde_json::Value {
    let count_from_score = json!({
        "function": "minecraft:set_count",
        "count": {"type": "minecraft:score",
                  "target": {"type": "minecraft:context", "target": "this"},
                  "score": "n", "fallback": 1}
    });
    json!({"pools": [{
        "rolls": 1,
        "entries": [{"type": "minecraft:item", "name": "minecraft:apple",
                     "functions": [count_from_score]}]
    }]})
}

struct Packs(PackContents);

impl DatapackSource for Packs {
    fn read_packs(&self) -> Result<Vec<PackContents>, DatapackError> {
        Ok(vec![self.0.clone()])
    }
}

fn packs() -> Arc<Packs> {
    Arc::new(Packs(PackContents {
        name: "t".to_owned(),
        loot_tables: vec![PackFile {
            id: Identifier::parse("t:this").unwrap(),
            text: table().to_string(),
        }],
        ..PackContents::default()
    }))
}

fn console(s: &mut Scene, line: &str) {
    s.server.handle(Inbound::Console(line.to_owned()));
    s.server.deliver();
}

#[test]
fn a_chest_table_sees_the_opening_player_as_this() {
    let mut s = world_with_datapacks(
        vec![
            Seed::block(CHEST, BlockKind::Chest)
                .carrying("LootTable", "t:this")
                .carrying("LootTableSeed", 3i64),
        ],
        packs(),
    );
    console(&mut s, "scoreboard objectives add n dummy");
    console(&mut s, "scoreboard players set Alice n 7");
    let alice = s.players[0].clone();
    let view = open_block(&mut s, &alice, CHEST);
    let found: Vec<_> = (0..27).filter_map(|slot| view.at(slot)).collect();
    assert!(found.iter().all(|(name, _)| *name == "minecraft:apple"));
    assert_eq!(
        found
            .iter()
            .map(|(_, count)| u32::from(*count))
            .sum::<u32>(),
        7
    );
}

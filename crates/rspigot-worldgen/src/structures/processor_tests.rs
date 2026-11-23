use std::borrow::Cow;

use rspigot_blocks::BlockKind;
use serde_json::{Value, json};

use super::processors::{Processed, Step, Subject, compile_list};
use crate::overworld::features::tests::Bench;
use crate::resources::BlockKind as Slot;

fn steps(value: &Value) -> Vec<Step> {
    compile_list(value)
}

fn ramparts() -> Vec<Step> {
    steps(&json!({
        "processors": [{
            "processor_type": "minecraft:rule",
            "rules": [{
                "input_predicate": { "predicate_type": "minecraft:always_true" },
                "location_predicate": { "predicate_type": "minecraft:always_true" },
                "output_state": "minecraft:air",
                "position_predicate": {
                    "max_chance": 0.05,
                    "max_dist": 100,
                    "predicate_type": "minecraft:axis_aligned_linear_pos"
                }
            }]
        }]
    }))
}

fn replaced_at_distance(distance: i32) -> usize {
    let bench = Bench::new();
    let mut region = bench.floor(Slot::Stone, 64);
    let steps = ramparts();
    let stone = BlockKind::Stone.default_state();
    bench.with_run(&mut region, 1, |run| {
        (0..2000)
            .filter(|i| {
                let mut subject = Subject {
                    local: (0, 0, 0),
                    at: (i % 16, 64 + distance, i / 16),
                    reference: (0, 64, 0),
                    state: stone,
                    nbt: None,
                };
                steps.iter().all(|step| step.process(run, &mut subject)) && subject.state.is_air()
            })
            .count()
    })
}

#[test]
fn an_axis_aligned_position_predicate_fades_a_rule_in_with_distance() {
    assert_eq!(replaced_at_distance(0), 0);
    let far = replaced_at_distance(100);
    assert!(
        (40..=180).contains(&far),
        "{far} of 2000 at the maximum distance"
    );
    let middle = replaced_at_distance(50);
    assert!(middle > 0 && middle < far, "{middle} < {far}");
}

#[test]
fn a_linear_position_predicate_uses_the_manhattan_distance() {
    let list = steps(&json!({
        "processors": [{
            "processor_type": "minecraft:rule",
            "rules": [{
                "input_predicate": { "predicate_type": "minecraft:always_true" },
                "location_predicate": { "predicate_type": "minecraft:always_true" },
                "output_state": "minecraft:air",
                "position_predicate": {
                    "max_chance": 1.0,
                    "min_dist": 0,
                    "max_dist": 10,
                    "predicate_type": "minecraft:linear_pos"
                }
            }]
        }]
    }));
    let bench = Bench::new();
    let mut region = bench.floor(Slot::Stone, 64);
    let stone = BlockKind::Stone.default_state();
    let mut count = |at: (i32, i32, i32)| {
        bench.with_run(&mut region, 1, |run| {
            let mut subject = Subject {
                local: (0, 0, 0),
                at,
                reference: (0, 0, 0),
                state: stone,
                nbt: None,
            };
            list.iter().all(|step| step.process(run, &mut subject));
            subject.state.is_air()
        })
    };
    assert!(count((20, 20, 20)));
    assert!(!count((0, 0, 0)));
}

#[test]
fn append_loot_gives_the_block_a_loot_table_and_a_seed() {
    let list = steps(&json!({
        "processors": [{
            "processor_type": "minecraft:rule",
            "rules": [{
                "block_entity_modifier": {
                    "type": "minecraft:append_loot",
                    "loot_table": "minecraft:archaeology/desert_well"
                },
                "input_predicate": {
                    "block": "minecraft:suspicious_sand",
                    "predicate_type": "minecraft:block_match"
                },
                "location_predicate": { "predicate_type": "minecraft:always_true" },
                "output_state": "minecraft:suspicious_sand"
            }]
        }]
    }));
    let bench = Bench::new();
    let mut region = bench.floor(Slot::Stone, 64);
    let sand = BlockKind::SuspiciousSand.default_state();
    let nbt = bench.with_run(&mut region, 1, |run| {
        let mut subject = Subject {
            local: (0, 0, 0),
            at: (3, 65, 4),
            reference: (0, 0, 0),
            state: sand,
            nbt: None,
        };
        assert!(list.iter().all(|step| step.process(run, &mut subject)));
        subject.nbt.map(Cow::into_owned)
    });
    let nbt = nbt.expect("a block entity");
    assert_eq!(nbt["LootTable"], "minecraft:archaeology/desert_well");
    assert_eq!(nbt["id"], "minecraft:brushable_block");
    assert!(nbt["LootTableSeed"]["$l"].is_i64());
}

#[test]
fn a_capped_rule_changes_at_most_its_limit_of_blocks() {
    let list = steps(&json!({
        "processors": [{
            "processor_type": "minecraft:capped",
            "limit": 2,
            "delegate": {
                "processor_type": "minecraft:rule",
                "rules": [{
                    "block_entity_modifier": {
                        "type": "minecraft:append_loot",
                        "loot_table": "minecraft:archaeology/trail_ruins_common"
                    },
                    "input_predicate": {
                        "block": "minecraft:gravel",
                        "predicate_type": "minecraft:block_match"
                    },
                    "location_predicate": { "predicate_type": "minecraft:always_true" },
                    "output_state": "minecraft:suspicious_gravel"
                }]
            }
        }]
    }));
    let bench = Bench::new();
    let mut region = bench.floor(Slot::Stone, 64);
    let gravel = BlockKind::Gravel.default_state();
    let mut blocks: Vec<Processed> = (0..10)
        .map(|i| Processed {
            local: (i, 0, 0),
            at: (i, 70, 0),
            state: gravel,
            nbt: None,
        })
        .collect();
    bench.with_run(&mut region, 1, |run| {
        for step in &list {
            step.finalize(run, (0, 70, 0), (0, 70, 0), &mut blocks);
        }
    });
    let suspicious: Vec<&Processed> = blocks
        .iter()
        .filter(|block| block.state.kind() == Some(BlockKind::SuspiciousGravel))
        .collect();
    assert_eq!(suspicious.len(), 2);
    for block in suspicious {
        let nbt = block.nbt.as_ref().expect("loot");
        assert_eq!(nbt["LootTable"], "minecraft:archaeology/trail_ruins_common");
    }
    assert_eq!(blocks.iter().filter(|b| b.state == gravel).count(), 8);
}

#[test]
fn the_hard_coded_processors_compile_from_a_processor_list() {
    let list = steps(&json!({
        "processors": [
            { "processor_type": "minecraft:block_ignore",
              "blocks": [{ "Name": "minecraft:dirt" }] },
            { "processor_type": "minecraft:blackstone_replace" }
        ]
    }));
    assert_eq!(list.len(), 2);
    let bench = Bench::new();
    let mut region = bench.floor(Slot::Stone, 64);
    let outcome = bench.with_run(&mut region, 1, |run| {
        let mut run_on = |state| {
            let mut subject = Subject {
                local: (0, 0, 0),
                at: (1, 70, 1),
                reference: (0, 0, 0),
                state,
                nbt: None,
            };
            let kept = list.iter().all(|step| step.process(run, &mut subject));
            (kept, subject.state)
        };
        (
            run_on(BlockKind::Dirt.default_state()),
            run_on(BlockKind::Cobblestone.default_state()),
        )
    });
    assert!(!outcome.0.0);
    assert!(outcome.1.0);
    assert_eq!(outcome.1.1.kind(), Some(BlockKind::Blackstone));
}

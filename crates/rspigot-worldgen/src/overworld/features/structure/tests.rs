use rspigot_blocks::BlockKind as Kind;
use serde_json::Value;

use super::{Processor, Rotation, Template};
use crate::datapack::worldgen;

fn referenced() -> Vec<String> {
    let mut found = Vec::new();
    fn walk(node: &Value, found: &mut Vec<String>) {
        match node {
            Value::Object(map) => {
                if map.get("type").and_then(Value::as_str) == Some("minecraft:template") {
                    for entry in map["templates"].as_array().into_iter().flatten() {
                        found.push(entry["data"]["id"].as_str().unwrap_or("").to_owned());
                    }
                }
                if map.get("type").and_then(Value::as_str) == Some("minecraft:fossil") {
                    for key in ["fossil_structures", "overlay_structures"] {
                        for name in map[key].as_array().into_iter().flatten() {
                            found.push(name.as_str().unwrap_or("").to_owned());
                        }
                    }
                }
                map.values().for_each(|value| walk(value, found));
            }
            Value::Array(items) => items.iter().for_each(|value| walk(value, found)),
            _ => {}
        }
    }
    walk(&worldgen()["configured"], &mut found);
    found.sort();
    found.dedup();
    found
}

#[test]
fn every_template_the_features_reference_is_embedded() {
    let names = referenced();
    assert_eq!(names.len(), 28);
    for name in names {
        let template = Template::named(&name).unwrap_or_else(|| panic!("{name}"));
        assert!(!template.blocks().is_empty(), "{name}");
    }
}

#[test]
fn the_desert_well_template_has_its_recorded_size_and_block_count() {
    let well = Template::named("minecraft:desert_well/well").unwrap();
    assert_eq!(well.size(Rotation::None), (5, 7, 5));
    assert_eq!(well.blocks().len(), 127);
    let sand = Template::named("minecraft:desert_well/suspicious_sand").unwrap();
    assert_eq!(sand.size(Rotation::None), (1, 1, 1));
    assert_eq!(sand.blocks().len(), 1);
}

#[test]
fn blocks_stay_inside_the_template_size() {
    for name in referenced() {
        let template = Template::named(&name).unwrap();
        let (sx, sy, sz) = template.size(Rotation::None);
        for ((x, y, z), _) in template.blocks() {
            assert!(
                (0..sx).contains(x) && (0..sy).contains(y) && (0..sz).contains(z),
                "{name}"
            );
        }
    }
}

#[test]
fn full_cubes_come_first_then_the_other_blocks_each_sorted_bottom_up() {
    let well = Template::named("minecraft:desert_well/well").unwrap();
    let blocks = well.blocks();
    let first_other = blocks
        .iter()
        .position(|(_, state)| {
            !state
                .flags()
                .contains(rspigot_blocks::StateFlags::FULL_COLLISION_CUBE)
        })
        .unwrap();
    assert!(blocks[first_other..].iter().all(|(_, state)| {
        !state
            .flags()
            .contains(rspigot_blocks::StateFlags::FULL_COLLISION_CUBE)
    }));
    for run in [&blocks[..first_other], &blocks[first_other..]] {
        let keys: Vec<_> = run.iter().map(|((x, y, z), _)| (*y, *x, *z)).collect();
        assert!(keys.windows(2).all(|pair| pair[0] <= pair[1]));
    }
}

#[test]
fn rotated_templates_fill_exactly_their_rotated_bounds() {
    for name in referenced() {
        let template = Template::named(&name).unwrap();
        for rotation in Rotation::ALL {
            let zero = template.zero_position((100, 40, -60), rotation);
            let (low, high) = template.bounds(zero, rotation);
            let size = template.size(rotation);
            assert_eq!(
                (high.0 - low.0 + 1, high.1 - low.1 + 1, high.2 - low.2 + 1),
                size,
                "{name} {rotation:?}"
            );
            for (local, _) in template.blocks() {
                let (x, y, z) = rotation.transform(*local);
                let at = (zero.0 + x, zero.1 + y, zero.2 + z);
                assert!(
                    (low.0..=high.0).contains(&at.0)
                        && (low.1..=high.1).contains(&at.1)
                        && (low.2..=high.2).contains(&at.2),
                    "{name} {rotation:?} {at:?}"
                );
            }
        }
    }
}

#[test]
fn rotations_turn_axes_and_horizontal_facings() {
    let bone = Kind::BoneBlock
        .default_state()
        .with_property("axis", "x")
        .unwrap();
    assert_eq!(
        Rotation::Clockwise90.state(bone).property("axis"),
        Some("z")
    );
    assert_eq!(
        Rotation::Counterclockwise90.state(bone).property("axis"),
        Some("z")
    );
    assert_eq!(
        Rotation::Clockwise180.state(bone).property("axis"),
        Some("x")
    );
    let stairs = Kind::StoneStairs
        .default_state()
        .with_property("facing", "north")
        .unwrap();
    assert_eq!(
        Rotation::Clockwise90.state(stairs).property("facing"),
        Some("east")
    );
    assert_eq!(
        Rotation::Clockwise180.state(stairs).property("facing"),
        Some("south")
    );
    assert_eq!(
        Rotation::Counterclockwise90
            .state(stairs)
            .property("facing"),
        Some("west")
    );
}

#[test]
fn rotation_vectors_follow_the_vanilla_quarter_turns() {
    assert_eq!(Rotation::Clockwise90.vector((1, 0)), (0, 1));
    assert_eq!(Rotation::Counterclockwise90.vector((1, 0)), (0, -1));
    assert_eq!(Rotation::Clockwise180.vector((1, 2)), (-1, -2));
}

#[test]
fn the_fossil_processor_lists_compile() {
    let lists = &worldgen()["processor_lists"];
    for name in [
        "minecraft:fossil_rot",
        "minecraft:fossil_coal",
        "minecraft:fossil_diamonds",
    ] {
        let compiled = Processor::compile_list(&lists[name]).unwrap();
        assert!(matches!(compiled[0], Processor::BlockRot(_)), "{name}");
        assert!(
            matches!(compiled.last(), Some(Processor::Protected(_))),
            "{name}"
        );
    }
}

#[test]
fn unknown_processor_types_are_refused() {
    let list = serde_json::json!({"processors": [{"processor_type": "minecraft:gravity"}]});
    assert!(Processor::compile_list(&list).is_none());
}

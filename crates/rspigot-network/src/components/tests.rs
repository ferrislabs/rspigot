use rspigot_nbt::Tag;

use super::{decode_components, encode_components, encode_value, schema};

const GOLDEN: &[(&str, &str)] = &[
    ("family1", include_str!("golden/family1.txt")),
    ("family2", include_str!("golden/family2.txt")),
    ("family3", include_str!("golden/family3.txt")),
];

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn unhex(text: &str) -> Vec<u8> {
    (0..text.len())
        .step_by(2)
        .map(|at| u8::from_str_radix(&text[at..at + 2], 16).unwrap())
        .collect()
}

fn parse(snbt: &str) -> Tag {
    rspigot_nbt::parse_tag(snbt).unwrap_or_else(|error| panic!("{snbt}: {error:?}"))
}

#[test]
fn every_golden_value_encodes_to_the_vanilla_bytes_and_decodes_back() {
    let mut checked = 0;
    for (family, text) in GOLDEN {
        for line in text.lines().filter(|line| !line.trim().is_empty()) {
            let mut parts = line.splitn(3, " | ");
            let (name, snbt, expected) = (
                parts.next().unwrap(),
                parts.next().unwrap(),
                parts.next().unwrap(),
            );
            let label = format!("{family}: {name} {snbt}");
            let value = parse(snbt);
            let encoded = encode_value(name, &value).unwrap_or_else(|| panic!("{label}: encode"));
            assert_eq!(hex(&encoded.data), expected, "{label}");
            let node = schema::node(name).unwrap();
            let bytes = unhex(expected);
            let mut input = bytes.as_slice();
            let decoded = node
                .decode(&mut input)
                .unwrap_or_else(|| panic!("{label}: decode"));
            assert!(input.is_empty(), "{label}: trailing bytes");
            let again =
                encode_value(name, &decoded).unwrap_or_else(|| panic!("{label}: re-encode"));
            assert_eq!(hex(&again.data), expected, "{label}: round trip");
            checked += 1;
        }
    }
    assert!(checked > 0);
}

#[test]
fn a_component_patch_round_trips_through_the_wire_and_marks_removals() {
    let components = parse(
        r#"{"minecraft:custom_name":"X","minecraft:max_stack_size":3,"!minecraft:rarity":{}}"#,
    );
    let Tag::Compound(components) = components else {
        panic!("compound");
    };
    let patch = encode_components(&components);
    assert_eq!(patch.added.len(), 2);
    assert_eq!(patch.removed.len(), 1);
    assert!(patch.added[0].type_id < patch.added[1].type_id);
    let decoded = decode_components(&patch);
    assert_eq!(decoded, components);
}

#[test]
fn styled_text_travels_as_network_nbt() {
    let styled = parse(r#"{text:"Hi",color:"gold",bold:1b}"#);
    let encoded = encode_value("minecraft:custom_name", &styled).unwrap();
    let mut input = encoded.data.as_slice();
    let decoded = schema::node("minecraft:custom_name")
        .unwrap()
        .decode(&mut input)
        .unwrap();
    let compound = decoded.as_compound().unwrap();
    assert_eq!(compound.get_str("text"), Some("Hi"));
    assert_eq!(compound.get_str("color"), Some("gold"));
    assert_eq!(compound.get_bool("bold"), Some(true));
}

#[test]
fn a_hidden_effect_nests_inside_the_effect_details() {
    let effect = parse(
        r#"{custom_effects:[{id:"minecraft:speed",amplifier:2,duration:100,hidden_effect:{amplifier:1,duration:50}}]}"#,
    );
    let encoded = encode_value("minecraft:potion_contents", &effect).unwrap();
    let mut input = encoded.data.as_slice();
    let decoded = schema::node("minecraft:potion_contents")
        .unwrap()
        .decode(&mut input)
        .unwrap();
    let effects = decoded
        .as_compound()
        .unwrap()
        .get_list("custom_effects")
        .unwrap();
    let hidden = effects[0]
        .as_compound()
        .unwrap()
        .get_compound("hidden_effect")
        .unwrap();
    assert_eq!(hidden.get_i32("duration"), Some(50));
    assert_eq!(hidden.get_i32("amplifier"), Some(1));
}

#[test]
fn a_map_post_processing_value_is_its_ordinal() {
    let encoded = encode_value(
        "minecraft:map_post_processing",
        &Tag::String("scale".to_owned()),
    );
    assert_eq!(encoded.unwrap().data, vec![1]);
}

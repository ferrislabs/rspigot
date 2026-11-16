use super::{component_hash, crc32c};

const GOLDEN: &str = include_str!("golden.txt");

#[test]
fn crc32c_matches_the_standard_check_value() {
    assert_eq!(crc32c(b"123456789"), 0xE306_9283);
}

#[test]
fn every_component_hashes_to_what_vanilla_computes() {
    let mut checked = 0;
    let mut failures = Vec::new();
    for line in GOLDEN.lines().filter(|line| !line.trim().is_empty()) {
        let mut parts = line.splitn(3, " | ");
        let (name, snbt, expected) = (
            parts.next().unwrap(),
            parts.next().unwrap(),
            parts.next().unwrap(),
        );
        let value =
            rspigot_nbt::parse_tag(snbt).unwrap_or_else(|error| panic!("{snbt}: {error:?}"));
        if component_hash(name, &value) != Some(expected.parse::<i32>().unwrap()) {
            failures.push(format!("{name} {snbt}"));
        }
        checked += 1;
    }
    assert!(checked > 0);
    assert!(failures.is_empty(), "{failures:#?}");
}

use rspigot_nbt::{Compound, Tag};

use super::nbt::compound_mut;

struct Conversion {
    fallback: &'static str,
    biomes: &'static [(&'static str, &'static str)],
}

const MINESHAFT: Conversion = Conversion {
    fallback: "minecraft:mineshaft",
    biomes: &[
        ("minecraft:badlands", "minecraft:mineshaft_mesa"),
        ("minecraft:eroded_badlands", "minecraft:mineshaft_mesa"),
        ("minecraft:wooded_badlands", "minecraft:mineshaft_mesa"),
    ],
};

const SHIPWRECK: Conversion = Conversion {
    fallback: "minecraft:shipwreck",
    biomes: &[
        ("minecraft:beach", "minecraft:shipwreck_beached"),
        ("minecraft:snowy_beach", "minecraft:shipwreck_beached"),
    ],
};

const OCEAN_RUIN: Conversion = Conversion {
    fallback: "minecraft:ocean_ruin_cold",
    biomes: &[
        ("minecraft:warm_ocean", "minecraft:ocean_ruin_warm"),
        ("minecraft:lukewarm_ocean", "minecraft:ocean_ruin_warm"),
        ("minecraft:deep_lukewarm_ocean", "minecraft:ocean_ruin_warm"),
    ],
};

const VILLAGE: Conversion = Conversion {
    fallback: "minecraft:village_plains",
    biomes: &[
        ("minecraft:desert", "minecraft:village_desert"),
        ("minecraft:savanna", "minecraft:village_savanna"),
        ("minecraft:snowy_plains", "minecraft:village_snowy"),
        ("minecraft:taiga", "minecraft:village_taiga"),
    ],
};

const RUINED_PORTAL: Conversion = Conversion {
    fallback: "minecraft:ruined_portal",
    biomes: &[
        ("minecraft:desert", "minecraft:ruined_portal_desert"),
        ("minecraft:badlands", "minecraft:ruined_portal_mountain"),
        (
            "minecraft:eroded_badlands",
            "minecraft:ruined_portal_mountain",
        ),
        (
            "minecraft:wooded_badlands",
            "minecraft:ruined_portal_mountain",
        ),
        (
            "minecraft:windswept_hills",
            "minecraft:ruined_portal_mountain",
        ),
        (
            "minecraft:windswept_forest",
            "minecraft:ruined_portal_mountain",
        ),
        (
            "minecraft:windswept_gravelly_hills",
            "minecraft:ruined_portal_mountain",
        ),
        (
            "minecraft:savanna_plateau",
            "minecraft:ruined_portal_mountain",
        ),
        (
            "minecraft:windswept_savanna",
            "minecraft:ruined_portal_mountain",
        ),
        ("minecraft:stony_shore", "minecraft:ruined_portal_mountain"),
        ("minecraft:meadow", "minecraft:ruined_portal_mountain"),
        ("minecraft:frozen_peaks", "minecraft:ruined_portal_mountain"),
        ("minecraft:jagged_peaks", "minecraft:ruined_portal_mountain"),
        ("minecraft:stony_peaks", "minecraft:ruined_portal_mountain"),
        ("minecraft:snowy_slopes", "minecraft:ruined_portal_mountain"),
        ("minecraft:bamboo_jungle", "minecraft:ruined_portal_jungle"),
        ("minecraft:jungle", "minecraft:ruined_portal_jungle"),
        ("minecraft:sparse_jungle", "minecraft:ruined_portal_jungle"),
        (
            "minecraft:deep_frozen_ocean",
            "minecraft:ruined_portal_ocean",
        ),
        ("minecraft:deep_cold_ocean", "minecraft:ruined_portal_ocean"),
        ("minecraft:deep_ocean", "minecraft:ruined_portal_ocean"),
        (
            "minecraft:deep_lukewarm_ocean",
            "minecraft:ruined_portal_ocean",
        ),
        ("minecraft:frozen_ocean", "minecraft:ruined_portal_ocean"),
        ("minecraft:ocean", "minecraft:ruined_portal_ocean"),
        ("minecraft:cold_ocean", "minecraft:ruined_portal_ocean"),
        ("minecraft:lukewarm_ocean", "minecraft:ruined_portal_ocean"),
        ("minecraft:warm_ocean", "minecraft:ruined_portal_ocean"),
    ],
};

const fn trivial(id: &'static str) -> Conversion {
    Conversion {
        fallback: id,
        biomes: &[],
    }
}

fn conversion(old_key: &str) -> Option<Conversion> {
    Some(match old_key {
        "mineshaft" => MINESHAFT,
        "shipwreck" => SHIPWRECK,
        "ocean_ruin" => OCEAN_RUIN,
        "village" => VILLAGE,
        "ruined_portal" => RUINED_PORTAL,
        "pillager_outpost" => trivial("minecraft:pillager_outpost"),
        "mansion" => trivial("minecraft:mansion"),
        "jungle_pyramid" => trivial("minecraft:jungle_pyramid"),
        "desert_pyramid" => trivial("minecraft:desert_pyramid"),
        "igloo" => trivial("minecraft:igloo"),
        "swamp_hut" => trivial("minecraft:swamp_hut"),
        "stronghold" => trivial("minecraft:stronghold"),
        "monument" => trivial("minecraft:monument"),
        "fortress" => trivial("minecraft:fortress"),
        "endcity" => trivial("minecraft:end_city"),
        "buried_treasure" => trivial("minecraft:buried_treasure"),
        "nether_fossil" => trivial("minecraft:nether_fossil"),
        "bastion_remnant" => trivial("minecraft:bastion_remnant"),
        _ => return None,
    })
}

fn guess(chunk: &Compound, conversion: &Conversion) -> Option<&'static str> {
    let mut counts: Vec<(&'static str, i32)> = Vec::new();
    for section in chunk.get_list("sections").unwrap_or_default() {
        let Some(palette) = section
            .as_compound()
            .and_then(|section| section.get_compound("biomes"))
            .and_then(|biomes| biomes.get_list("palette"))
        else {
            continue;
        };
        for entry in palette {
            let Some(target) = entry.as_str().and_then(|biome| {
                conversion
                    .biomes
                    .iter()
                    .find(|(from, _)| *from == biome)
                    .map(|(_, to)| *to)
            }) else {
                continue;
            };
            match counts.iter_mut().find(|(known, _)| *known == target) {
                Some((_, count)) => *count += 1,
                None => counts.push((target, 1)),
            }
        }
    }
    let mut best: Option<(&'static str, i32)> = None;
    for (target, count) in counts {
        if best.is_none_or(|(_, top)| count > top) {
            best = Some((target, count));
        }
    }
    best.map(|(target, _)| target)
}

fn updated_type(chunk: &Compound, old_key: &str) -> Option<String> {
    let conversion = conversion(&old_key.to_lowercase())?;
    if conversion.biomes.is_empty() {
        return Some(conversion.fallback.to_owned());
    }
    Some(
        guess(chunk, &conversion)
            .unwrap_or(conversion.fallback)
            .to_owned(),
    )
}

fn long_values(tag: &Tag) -> Vec<i64> {
    match tag {
        Tag::LongArray(values) => values.clone(),
        Tag::List(items) => items
            .iter()
            .filter_map(|item| match item {
                Tag::Long(value) => Some(*value),
                _ => None,
            })
            .collect(),
        _ => Vec::new(),
    }
}

fn update_starts(chunk: &Compound, starts: &Compound) -> Compound {
    let mut updated = Compound::new();
    for (key, value) in starts.iter() {
        let Some(start) = value.as_compound() else {
            continue;
        };
        if start.get_str("id").unwrap_or("INVALID") == "INVALID" {
            continue;
        }
        let Some(new_key) = updated_type(chunk, key) else {
            tracing::warn!(structure = key, "unknown structure in the data fixer");
            continue;
        };
        if !updated.contains_key(&new_key) {
            let mut start = start.clone();
            start.insert("id", new_key.as_str());
            updated.insert(new_key, start);
        }
    }
    updated
}

fn update_references(chunk: &Compound, references: &Compound) -> Compound {
    let mut updated = Compound::new();
    for (key, value) in references.iter() {
        let values = long_values(value);
        if values.is_empty() {
            continue;
        }
        let Some(new_key) = updated_type(chunk, key) else {
            tracing::warn!(structure = key, "unknown structure in the data fixer");
            continue;
        };
        let merged = match updated.get(&new_key) {
            Some(old) => {
                let mut all = long_values(old);
                all.extend(values);
                all
            }
            None => values,
        };
        updated.insert(new_key, Tag::LongArray(merged));
    }
    updated
}

pub fn become_configured_2970(root: &mut Compound) {
    let snapshot = root.clone();
    let Some(structures) = compound_mut(root, "structures") else {
        return;
    };
    if let Some(starts) = structures.get_compound("starts") {
        let updated = update_starts(&snapshot, starts);
        structures.insert("starts", updated);
    }
    if let Some(references) = structures.get_compound("References") {
        let updated = update_references(&snapshot, references);
        structures.insert("References", updated);
    }
}

fn flatten_structures(settings: &Compound) -> Compound {
    let mut flat = Compound::new();
    if let Some(structures) = settings.get_compound("structures") {
        for (key, value) in structures.iter() {
            let tagged = match value.as_compound() {
                Some(compound) => {
                    let mut compound = compound.clone();
                    compound.insert("type", "minecraft:random_spread");
                    Tag::Compound(compound)
                }
                None => value.clone(),
            };
            flat.insert(key, tagged);
        }
    }
    if let Some(stronghold) = settings.get_compound("stronghold") {
        let mut stronghold = stronghold.clone();
        stronghold.insert("type", "minecraft:concentric_rings");
        flat.insert("minecraft:stronghold", stronghold);
    }
    flat
}

pub fn flatten_structure_settings_2967(root: &mut Compound) {
    let Some(dimensions) = compound_mut(root, "Data")
        .and_then(|data| compound_mut(data, "WorldGenSettings"))
        .and_then(|settings| compound_mut(settings, "dimensions"))
    else {
        return;
    };
    for dimension in dimensions.values_mut().filter_map(Tag::as_compound_mut) {
        let Some(settings) = compound_mut(dimension, "generator")
            .and_then(|generator| compound_mut(generator, "settings"))
        else {
            continue;
        };
        let Some(structures) = settings.get_compound("structures") else {
            continue;
        };
        let flat = flatten_structures(structures);
        settings.insert("structures", flat);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn section(biomes: &[&str]) -> Tag {
        Tag::Compound(Compound::new().with(
            "biomes",
            Compound::new().with(
                "palette",
                Tag::List(biomes.iter().map(|b| Tag::String((*b).into())).collect()),
            ),
        ))
    }

    fn chunk(sections: Vec<Tag>, starts: Compound, references: Compound) -> Compound {
        Compound::new().with("sections", Tag::List(sections)).with(
            "structures",
            Compound::new()
                .with("starts", starts)
                .with("References", references),
        )
    }

    fn start(id: &str) -> Compound {
        Compound::new().with("id", id).with("ChunkX", 1)
    }

    fn start_ids(root: &Compound) -> Vec<String> {
        let starts = root
            .get_compound("structures")
            .and_then(|s| s.get_compound("starts"))
            .unwrap();
        starts
            .iter()
            .map(|(key, value)| {
                assert_eq!(value.as_compound().unwrap().get_str("id"), Some(key));
                key.to_owned()
            })
            .collect()
    }

    #[test]
    fn trivial_structures_are_renamed_and_keep_their_data() {
        let mut root = chunk(
            Vec::new(),
            Compound::new()
                .with("Monument", start("Monument"))
                .with("EndCity", start("EndCity")),
            Compound::new(),
        );
        become_configured_2970(&mut root);
        assert_eq!(
            start_ids(&root),
            ["minecraft:monument", "minecraft:end_city"]
        );
        let starts = root
            .get_compound("structures")
            .unwrap()
            .get_compound("starts")
            .unwrap();
        assert_eq!(
            starts
                .get_compound("minecraft:monument")
                .unwrap()
                .get_i32("ChunkX"),
            Some(1)
        );
    }

    #[test]
    fn a_village_takes_the_variant_of_the_commonest_mapped_biome() {
        let mut root = chunk(
            vec![
                section(&["minecraft:desert", "minecraft:plains"]),
                section(&["minecraft:desert"]),
                section(&["minecraft:taiga"]),
            ],
            Compound::new().with("Village", start("Village")),
            Compound::new(),
        );
        become_configured_2970(&mut root);
        assert_eq!(start_ids(&root), ["minecraft:village_desert"]);
    }

    #[test]
    fn a_village_without_a_mapped_biome_falls_back_to_plains() {
        let mut root = chunk(
            vec![section(&["minecraft:plains"])],
            Compound::new().with("Village", start("Village")),
            Compound::new(),
        );
        become_configured_2970(&mut root);
        assert_eq!(start_ids(&root), ["minecraft:village_plains"]);
    }

    #[test]
    fn ocean_ruins_default_to_cold_and_ties_keep_the_first_biome() {
        let mut root = chunk(
            vec![section(&["minecraft:ocean"])],
            Compound::new().with("Ocean_Ruin", start("Ocean_Ruin")),
            Compound::new(),
        );
        become_configured_2970(&mut root);
        assert_eq!(start_ids(&root), ["minecraft:ocean_ruin_cold"]);
        let mut tied = chunk(
            vec![section(&["minecraft:taiga", "minecraft:desert"])],
            Compound::new().with("Village", start("Village")),
            Compound::new(),
        );
        become_configured_2970(&mut tied);
        assert_eq!(start_ids(&tied), ["minecraft:village_taiga"]);
    }

    #[test]
    fn invalid_and_unknown_starts_are_dropped() {
        let mut root = chunk(
            Vec::new(),
            Compound::new()
                .with("Fortress", start("INVALID"))
                .with("Castle", start("Castle"))
                .with("Igloo", start("Igloo")),
            Compound::new(),
        );
        become_configured_2970(&mut root);
        assert_eq!(start_ids(&root), ["minecraft:igloo"]);
    }

    #[test]
    fn references_merge_under_the_new_key_and_empty_ones_vanish() {
        let mut root = chunk(
            Vec::new(),
            Compound::new(),
            Compound::new()
                .with("Village", Tag::LongArray(vec![1, 2]))
                .with("village", Tag::LongArray(vec![3]))
                .with("Igloo", Tag::LongArray(Vec::new()))
                .with("Stronghold", Tag::LongArray(vec![9])),
        );
        become_configured_2970(&mut root);
        let references = root
            .get_compound("structures")
            .unwrap()
            .get_compound("References")
            .unwrap();
        assert_eq!(
            references.get_long_array("minecraft:village_plains"),
            Some(&[1, 2, 3][..])
        );
        assert_eq!(
            references.get_long_array("minecraft:stronghold"),
            Some(&[9][..])
        );
        assert!(!references.contains_key("minecraft:igloo"));
    }

    #[test]
    fn structure_settings_are_flattened_with_their_spread_types() {
        let settings = Compound::new().with(
            "structures",
            Compound::new()
                .with(
                    "structures",
                    Compound::new().with(
                        "minecraft:village",
                        Compound::new().with("spacing", 34).with("separation", 8),
                    ),
                )
                .with("stronghold", Compound::new().with("count", 128)),
        );
        let mut root = Compound::new().with(
            "Data",
            Compound::new().with(
                "WorldGenSettings",
                Compound::new().with(
                    "dimensions",
                    Compound::new().with(
                        "minecraft:overworld",
                        Compound::new().with(
                            "generator",
                            Compound::new()
                                .with("type", "minecraft:noise")
                                .with("settings", settings),
                        ),
                    ),
                ),
            ),
        );
        flatten_structure_settings_2967(&mut root);
        let flat = root
            .get_compound("Data")
            .and_then(|d| d.get_compound("WorldGenSettings"))
            .and_then(|w| w.get_compound("dimensions"))
            .and_then(|d| d.get_compound("minecraft:overworld"))
            .and_then(|d| d.get_compound("generator"))
            .and_then(|g| g.get_compound("settings"))
            .and_then(|s| s.get_compound("structures"))
            .unwrap();
        let village = flat.get_compound("minecraft:village").unwrap();
        assert_eq!(village.get_str("type"), Some("minecraft:random_spread"));
        assert_eq!(village.get_i32("spacing"), Some(34));
        let stronghold = flat.get_compound("minecraft:stronghold").unwrap();
        assert_eq!(
            stronghold.get_str("type"),
            Some("minecraft:concentric_rings")
        );
        assert_eq!(stronghold.get_i32("count"), Some(128));
        assert!(!flat.contains_key("stronghold"));
        assert!(!flat.contains_key("structures"));
    }

    #[test]
    fn world_gen_settings_with_a_preset_name_are_left_alone() {
        let mut root = Compound::new().with(
            "Data",
            Compound::new().with(
                "WorldGenSettings",
                Compound::new().with(
                    "dimensions",
                    Compound::new().with(
                        "minecraft:overworld",
                        Compound::new().with(
                            "generator",
                            Compound::new().with("settings", "minecraft:overworld"),
                        ),
                    ),
                ),
            ),
        );
        let before = root.clone();
        flatten_structure_settings_2967(&mut root);
        assert_eq!(root, before);
    }
}

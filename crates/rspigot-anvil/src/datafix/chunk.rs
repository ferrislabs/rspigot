use rspigot_nbt::{Compound, Tag};

use super::nbt::{
    compound_mut, ensure_namespaced, list_mut, rename_key, rename_string, walk_compounds,
};

const EMPTY_COMPONENT: &str = "{\"text\":\"\"}";
const BLOCK_STATE_KEYS: [&str; 5] = [
    "BlockState",
    "block_state",
    "carriedBlockState",
    "inBlockState",
    "DisplayState",
];
const SIGN_FIELDS_TO_DROP: [&str; 10] = [
    "Text1",
    "Text2",
    "Text3",
    "Text4",
    "FilteredText1",
    "FilteredText2",
    "FilteredText3",
    "FilteredText4",
    "Color",
    "GlowingText",
];

fn sections_mut(root: &mut Compound) -> impl Iterator<Item = &mut Compound> {
    list_mut(root, "sections")
        .into_iter()
        .flatten()
        .filter_map(Tag::as_compound_mut)
}

fn drop_section_light(root: &mut Compound) {
    for section in sections_mut(root) {
        section.remove("BlockLight");
        section.remove("SkyLight");
    }
}

pub fn delete_ignored_light_3077(root: &mut Compound) {
    if !root.get_bool("isLightOn").unwrap_or(false) {
        drop_section_light(root);
    }
}

pub fn delete_light(root: &mut Compound) {
    root.remove("isLightOn");
    drop_section_light(root);
}

fn block_entities_mut<'a>(
    root: &'a mut Compound,
    id: &'a [&str],
) -> impl Iterator<Item = &'a mut Compound> {
    list_mut(root, "block_entities")
        .into_iter()
        .flatten()
        .filter_map(Tag::as_compound_mut)
        .filter(move |entity| entity.get_str("id").is_some_and(|name| id.contains(&name)))
}

pub fn decorated_pot_sherds_3448(root: &mut Compound) {
    for pot in block_entities_mut(root, &["minecraft:decorated_pot"]) {
        rename_key(pot, "shards", "sherds");
    }
}

fn text_lines(sign: &Compound, prefix: &str) -> Vec<Option<String>> {
    (1..=4)
        .map(|index| sign.get_str(&format!("{prefix}{index}")).map(str::to_owned))
        .collect()
}

fn front_text(sign: &Compound) -> Compound {
    let messages: Vec<String> = text_lines(sign, "Text")
        .into_iter()
        .map(|line| line.unwrap_or_else(|| EMPTY_COMPONENT.to_owned()))
        .collect();
    let filtered = text_lines(sign, "FilteredText");
    let mut text = Compound::new().with(
        "messages",
        Tag::List(messages.iter().cloned().map(Tag::String).collect()),
    );
    text.insert("color", sign.get_str("Color").unwrap_or("black"));
    text.insert(
        "has_glowing_text",
        sign.get("GlowingText").cloned().unwrap_or(Tag::Byte(0)),
    );
    if filtered.iter().any(Option::is_some) {
        let lines = filtered
            .into_iter()
            .zip(&messages)
            .map(|(line, message)| Tag::String(line.unwrap_or_else(|| message.clone())))
            .collect();
        text.insert("filtered_messages", Tag::List(lines));
    }
    text
}

fn default_text() -> Compound {
    Compound::new()
        .with(
            "messages",
            Tag::List(vec![Tag::String(EMPTY_COMPONENT.to_owned()); 4]),
        )
        .with("color", "black")
        .with("has_glowing_text", false)
}

pub fn sign_double_sided_3439(root: &mut Compound) {
    for sign in block_entities_mut(root, &["minecraft:sign"]) {
        let front = front_text(sign);
        sign.insert("front_text", front);
        sign.insert("back_text", default_text());
        sign.insert("is_waxed", false);
        sign.insert("_filtered_correct", true);
        for field in SIGN_FIELDS_TO_DROP {
            sign.remove(field);
        }
    }
}

pub fn drop_invalid_sign_data_3564(root: &mut Compound) {
    for sign in block_entities_mut(root, &["minecraft:sign", "minecraft:hanging_sign"]) {
        sign.remove("_filtered_correct");
        for field in SIGN_FIELDS_TO_DROP {
            sign.remove(field);
        }
    }
}

fn remap_status(root: &mut Compound, rename: &impl Fn(&str) -> Option<String>) {
    rename_string(root, "Status", rename);
    if let Some(retrogen) = compound_mut(root, "below_zero_retrogen") {
        rename_string(retrogen, "target_status", rename);
    }
}

pub fn remap_status_3450(root: &mut Compound) {
    remap_status(root, &|name| match ensure_namespaced(name).as_str() {
        "minecraft:liquid_carvers" => Some("minecraft:carvers".to_owned()),
        "minecraft:heightmaps" => Some("minecraft:spawn".to_owned()),
        _ => None,
    });
}

pub fn merge_terrain_status_5013(root: &mut Compound) {
    remap_status(root, &|name| match ensure_namespaced(name).as_str() {
        "minecraft:carvers" => Some("minecraft:terrain".to_owned()),
        "minecraft:noise" | "minecraft:surface" => Some("minecraft:biomes".to_owned()),
        _ => None,
    });
}

pub fn furnace_burn_time_4181(root: &mut Compound) {
    for furnace in block_entities_mut(
        root,
        &[
            "minecraft:furnace",
            "minecraft:smoker",
            "minecraft:blast_furnace",
        ],
    ) {
        rename_key(furnace, "CookTime", "cooking_time_spent");
        rename_key(furnace, "CookTimeTotal", "cooking_total_time");
        rename_key(furnace, "BurnTime", "lit_time_remaining");
        if let Some(remaining) = furnace.get("lit_time_remaining").cloned() {
            furnace.insert("lit_total_time", remaining);
        }
    }
}

fn rename_state_fields(state: &mut Compound) {
    rename_key(state, "Name", "id");
    rename_key(state, "Properties", "properties");
}

pub fn block_state_field_names_5006(root: &mut Compound) {
    for section in sections_mut(root) {
        let Some(states) = compound_mut(section, "block_states") else {
            continue;
        };
        for entry in list_mut(states, "palette").into_iter().flatten() {
            if let Some(entry) = entry.as_compound_mut() {
                rename_state_fields(entry);
            }
        }
    }
    walk_compounds(root, &mut |compound| {
        for key in BLOCK_STATE_KEYS {
            if let Some(state) = compound_mut(compound, key) {
                rename_state_fields(state);
            }
        }
    });
}

pub fn pot_decorations_4996(root: &mut Compound) {
    for pot in block_entities_mut(root, &["minecraft:decorated_pot"]) {
        if let Some(Tag::List(items)) = pot.remove("sherds") {
            pot.insert("sherds", unpack_pot_decorations(&items));
        }
    }
}

pub fn unpack_pot_decorations(items: &[Tag]) -> Tag {
    let mut decorations = Compound::new();
    for (index, slot) in ["back", "left", "right", "front"].iter().enumerate() {
        let item = items
            .get(index)
            .and_then(Tag::as_str)
            .filter(|item| !item.is_empty())
            .unwrap_or("minecraft:brick");
        decorations.insert(*slot, Compound::new().with("id", item));
    }
    Tag::Compound(decorations)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chunk_with_sections() -> Compound {
        Compound::new().with("isLightOn", 1i8).with(
            "sections",
            Tag::List(vec![Tag::Compound(
                Compound::new()
                    .with("Y", 0i8)
                    .with("BlockLight", Tag::ByteArray(vec![0; 2048]))
                    .with("SkyLight", Tag::ByteArray(vec![0; 2048]))
                    .with(
                        "block_states",
                        Compound::new().with(
                            "palette",
                            Tag::List(vec![Tag::Compound(
                                Compound::new()
                                    .with("Name", "minecraft:oak_stairs")
                                    .with("Properties", Compound::new().with("half", "top")),
                            )]),
                        ),
                    ),
            )]),
        )
    }

    fn section(chunk: &Compound) -> &Compound {
        chunk.get_list("sections").unwrap()[0]
            .as_compound()
            .unwrap()
    }

    fn block_entity(id: &str) -> Compound {
        Compound::new().with("id", id)
    }

    fn with_block_entities(entities: Vec<Compound>) -> Compound {
        Compound::new().with(
            "block_entities",
            Tag::List(entities.into_iter().map(Tag::Compound).collect()),
        )
    }

    fn first_entity(chunk: &Compound) -> &Compound {
        chunk.get_list("block_entities").unwrap()[0]
            .as_compound()
            .unwrap()
    }

    #[test]
    fn light_is_deleted_everywhere() {
        let mut chunk = chunk_with_sections();
        delete_light(&mut chunk);
        assert!(!chunk.contains_key("isLightOn"));
        assert!(!section(&chunk).contains_key("BlockLight"));
        assert!(!section(&chunk).contains_key("SkyLight"));
        assert!(section(&chunk).contains_key("block_states"));
    }

    #[test]
    fn ignored_light_is_only_deleted_when_the_chunk_is_not_lit() {
        let mut lit = chunk_with_sections();
        delete_ignored_light_3077(&mut lit);
        assert!(section(&lit).contains_key("BlockLight"));
        assert!(lit.contains_key("isLightOn"));
        let mut dark = chunk_with_sections();
        dark.insert("isLightOn", 0i8);
        delete_ignored_light_3077(&mut dark);
        assert!(!section(&dark).contains_key("SkyLight"));
    }

    #[test]
    fn palette_entries_use_id_and_properties() {
        let mut chunk = chunk_with_sections();
        block_state_field_names_5006(&mut chunk);
        let palette = section(&chunk)
            .get_compound("block_states")
            .unwrap()
            .get_list("palette")
            .unwrap();
        let entry = palette[0].as_compound().unwrap();
        assert_eq!(entry.get_str("id"), Some("minecraft:oak_stairs"));
        assert_eq!(
            entry.get_compound("properties").unwrap().get_str("half"),
            Some("top")
        );
        assert!(!entry.contains_key("Name") && !entry.contains_key("Properties"));
    }

    #[test]
    fn statuses_are_remapped_in_two_steps() {
        let mut chunk = Compound::new()
            .with("Status", "minecraft:liquid_carvers")
            .with(
                "below_zero_retrogen",
                Compound::new().with("target_status", "minecraft:heightmaps"),
            );
        remap_status_3450(&mut chunk);
        assert_eq!(chunk.get_str("Status"), Some("minecraft:carvers"));
        assert_eq!(
            chunk
                .get_compound("below_zero_retrogen")
                .unwrap()
                .get_str("target_status"),
            Some("minecraft:spawn")
        );
        merge_terrain_status_5013(&mut chunk);
        assert_eq!(chunk.get_str("Status"), Some("minecraft:terrain"));
        let mut partial = Compound::new().with("Status", "minecraft:noise");
        merge_terrain_status_5013(&mut partial);
        assert_eq!(partial.get_str("Status"), Some("minecraft:biomes"));
        let mut full = Compound::new().with("Status", "minecraft:full");
        merge_terrain_status_5013(&mut full);
        assert_eq!(full.get_str("Status"), Some("minecraft:full"));
    }

    #[test]
    fn single_sided_signs_become_double_sided() {
        let mut chunk = with_block_entities(vec![
            block_entity("minecraft:sign")
                .with("Text1", "{\"text\":\"hi\"}")
                .with("Color", "red")
                .with("GlowingText", 1i8),
        ]);
        sign_double_sided_3439(&mut chunk);
        let sign = first_entity(&chunk);
        let front = sign.get_compound("front_text").unwrap();
        let messages = front.get_list("messages").unwrap();
        assert_eq!(messages[0].as_str(), Some("{\"text\":\"hi\"}"));
        assert_eq!(messages[1].as_str(), Some("{\"text\":\"\"}"));
        assert_eq!(front.get_str("color"), Some("red"));
        assert_eq!(front.get_bool("has_glowing_text"), Some(true));
        assert_eq!(
            sign.get_compound("back_text").unwrap().get_str("color"),
            Some("black")
        );
        assert_eq!(sign.get_bool("is_waxed"), Some(false));
        assert!(!sign.contains_key("Text1") && !sign.contains_key("Color"));
        drop_invalid_sign_data_3564(&mut chunk);
        assert!(!first_entity(&chunk).contains_key("_filtered_correct"));
    }

    #[test]
    fn furnaces_get_the_renamed_timers() {
        let mut chunk = with_block_entities(vec![
            block_entity("minecraft:furnace")
                .with("CookTime", 5i16)
                .with("CookTimeTotal", 200i16)
                .with("BurnTime", 30i16),
        ]);
        furnace_burn_time_4181(&mut chunk);
        let furnace = first_entity(&chunk);
        assert_eq!(furnace.get_i32("cooking_time_spent"), Some(5));
        assert_eq!(furnace.get_i32("cooking_total_time"), Some(200));
        assert_eq!(furnace.get_i32("lit_time_remaining"), Some(30));
        assert_eq!(furnace.get_i32("lit_total_time"), Some(30));
    }

    #[test]
    fn pot_sherds_are_renamed_then_unpacked_by_side() {
        let mut chunk = with_block_entities(vec![block_entity("minecraft:decorated_pot").with(
            "shards",
            Tag::List(vec![
                Tag::String("minecraft:brick".into()),
                Tag::String("minecraft:arms_up_pottery_sherd".into()),
            ]),
        )]);
        decorated_pot_sherds_3448(&mut chunk);
        assert!(first_entity(&chunk).contains_key("sherds"));
        pot_decorations_4996(&mut chunk);
        let sherds = first_entity(&chunk).get_compound("sherds").unwrap();
        assert_eq!(
            sherds.get_compound("left").unwrap().get_str("id"),
            Some("minecraft:arms_up_pottery_sherd")
        );
        assert_eq!(
            sherds.get_compound("front").unwrap().get_str("id"),
            Some("minecraft:brick")
        );
    }
}

use rspigot_nbt::{Compound, Tag};

use super::nbt::{
    compound_mut, ensure_namespaced, is_empty_stack, list_mut, rename_key, walk_compounds,
    walk_item_stacks,
};

const DEFAULT_DROP_CHANCE: f32 = 0.085;

const MOB_EFFECT_IDS: &[&str] = &[
    "minecraft:speed",
    "minecraft:slowness",
    "minecraft:haste",
    "minecraft:mining_fatigue",
    "minecraft:strength",
    "minecraft:instant_health",
    "minecraft:instant_damage",
    "minecraft:jump_boost",
    "minecraft:nausea",
    "minecraft:regeneration",
    "minecraft:resistance",
    "minecraft:fire_resistance",
    "minecraft:water_breathing",
    "minecraft:invisibility",
    "minecraft:blindness",
    "minecraft:night_vision",
    "minecraft:hunger",
    "minecraft:weakness",
    "minecraft:poison",
    "minecraft:wither",
    "minecraft:health_boost",
    "minecraft:absorption",
    "minecraft:saturation",
    "minecraft:glowing",
    "minecraft:levitation",
    "minecraft:luck",
    "minecraft:unluck",
    "minecraft:slow_falling",
    "minecraft:conduit_power",
    "minecraft:dolphins_grace",
    "minecraft:bad_omen",
    "minecraft:hero_of_the_village",
    "minecraft:darkness",
];

const MODIFIER_IDS_BY_UUID: &[(&str, &str)] = &[
    (
        "736565d2-e1a7-403d-a3f8-1aeb3e302542",
        "minecraft:creative_mode_block_range",
    ),
    (
        "98491ef6-97b1-4584-ae82-71a8cc85cf73",
        "minecraft:creative_mode_entity_range",
    ),
    (
        "91aeaa56-376b-4498-935b-2f7f68070635",
        "minecraft:effect.speed",
    ),
    (
        "7107de5e-7ce8-4030-940e-514c1f160890",
        "minecraft:effect.slowness",
    ),
    (
        "af8b6e3f-3328-4c0a-aa36-5ba2bb9dbef3",
        "minecraft:effect.haste",
    ),
    (
        "55fced67-e92a-486e-9800-b47f202c4386",
        "minecraft:effect.mining_fatigue",
    ),
    (
        "648d7064-6a60-4f59-8abe-c2c23a6dd7a9",
        "minecraft:effect.strength",
    ),
    (
        "c0105bf3-aef8-46b0-9ebc-92943757ccbe",
        "minecraft:effect.jump_boost",
    ),
    (
        "22653b89-116e-49dc-9b6b-9971489b5be5",
        "minecraft:effect.weakness",
    ),
    (
        "5d6f0ba2-1186-46ac-b896-c61c5cee99cc",
        "minecraft:effect.health_boost",
    ),
    (
        "eae29cf0-701e-4ed6-883a-96f798f3dab5",
        "minecraft:effect.absorption",
    ),
    (
        "03c3c89d-7037-4b42-869f-b146bcb64d2e",
        "minecraft:effect.luck",
    ),
    (
        "cc5af142-2bd2-4215-b636-2605aed11727",
        "minecraft:effect.unluck",
    ),
    ("6555be74-63b3-41f1-a245-77833b3c2562", "minecraft:evil"),
    (
        "1eaf83ff-7207-4596-b37a-d7a07b3ec4ce",
        "minecraft:powder_snow",
    ),
    (
        "662a6b8d-da3e-4c1c-8813-96ea6097278d",
        "minecraft:sprinting",
    ),
    (
        "020e0dfb-87ae-4653-9556-831010e291a0",
        "minecraft:attacking",
    ),
    ("766bfa64-11f3-11ea-8d71-362b9e155667", "minecraft:baby"),
    ("7e0292f2-9434-48d5-a29f-9583af7df27f", "minecraft:covered"),
    (
        "9e362924-01de-4ddd-a2b2-d0f7a405a174",
        "minecraft:suffocating",
    ),
    ("5cd17e52-a79a-43d3-a529-90fde04b181e", "minecraft:drinking"),
    ("b9766b59-9566-4402-bc1f-2ee2a276d836", "minecraft:baby"),
    (
        "49455a49-7ec5-45ba-b886-3b90b23a1718",
        "minecraft:attacking",
    ),
    (
        "845db27c-c624-495f-8c9f-6020a9a58b6b",
        "minecraft:armor.boots",
    ),
    (
        "d8499b04-0e66-4726-ab29-64469d734e0d",
        "minecraft:armor.leggings",
    ),
    (
        "9f3d476d-c118-4544-8365-64846904b48e",
        "minecraft:armor.chestplate",
    ),
    (
        "2ad3f246-fee1-4e67-b886-69fd380bb150",
        "minecraft:armor.helmet",
    ),
    (
        "c1c72771-8b8e-ba4a-ace0-81a93c8928b2",
        "minecraft:armor.body",
    ),
    (
        "b572ecd2-ac0c-4071-abde-9594af072a37",
        "minecraft:enchantment.fire_protection",
    ),
    (
        "40a9968f-5c66-4e2f-b7f4-2ec2f4b3e450",
        "minecraft:enchantment.blast_protection",
    ),
    (
        "07a65791-f64d-4e79-86c7-f83932f007ec",
        "minecraft:enchantment.respiration",
    ),
    (
        "60b1b7db-fffd-4ad0-817c-d6c6a93d8a45",
        "minecraft:enchantment.aqua_affinity",
    ),
    (
        "11dc269a-4476-46c0-aff3-9e17d7eb6801",
        "minecraft:enchantment.depth_strider",
    ),
    (
        "87f46a96-686f-4796-b035-22e16ee9e038",
        "minecraft:enchantment.soul_speed",
    ),
    (
        "b9716dbd-50df-4080-850e-70347d24e687",
        "minecraft:enchantment.soul_speed",
    ),
    (
        "92437d00-c3a7-4f2e-8f6c-1f21585d5dd0",
        "minecraft:enchantment.swift_sneak",
    ),
    (
        "5d3d087b-debe-4037-b53e-d84f3ff51f17",
        "minecraft:enchantment.sweeping_edge",
    ),
    (
        "3ceb37c0-db62-46b5-bd02-785457b01d96",
        "minecraft:enchantment.efficiency",
    ),
    (
        "cb3f55d3-645c-4f38-a497-9c13a33db5cf",
        "minecraft:base_attack_damage",
    ),
    (
        "fa233e1c-4180-4865-b01b-bcce9785aca3",
        "minecraft:base_attack_speed",
    ),
];

const MODIFIER_IDS_BY_NAME: &[(&str, &str)] = &[
    ("Random spawn bonus", "minecraft:random_spawn_bonus"),
    (
        "Random zombie-spawn bonus",
        "minecraft:zombie_random_spawn_bonus",
    ),
    ("Leader zombie bonus", "minecraft:leader_zombie_bonus"),
    (
        "Zombie reinforcement callee charge",
        "minecraft:reinforcement_callee_charge",
    ),
    (
        "Zombie reinforcement caller charge",
        "minecraft:reinforcement_caller_charge",
    ),
];

const BOAT_WOODS: &[&str] = &[
    "oak", "spruce", "birch", "jungle", "acacia", "cherry", "dark_oak", "mangrove", "bamboo",
];

const ARMOR_SLOTS: &[&str] = &["feet", "legs", "chest", "head"];
const HAND_SLOTS: &[&str] = &["mainhand", "offhand"];

fn entity_id(compound: &Compound) -> Option<&str> {
    compound.get_str("id")
}

fn walk_entities<F: FnMut(&mut Compound)>(root: &mut Compound, visit: &mut F) {
    walk_compounds(root, visit);
}

fn mob_effect_name(tag: &Tag) -> Option<&'static str> {
    let id = usize::try_from(tag.as_i64()?).ok()?;
    MOB_EFFECT_IDS.get(id.checked_sub(1)?).copied()
}

fn replace_effect_id(effect: &mut Compound, from: &str, to: &str) {
    if let Some(old) = effect.remove(from)
        && let Some(name) = mob_effect_name(&old)
    {
        effect.insert(to, name);
    }
}

fn fix_effect_instance(effect: &mut Compound) {
    replace_effect_id(effect, "Id", "id");
    rename_key(effect, "Ambient", "ambient");
    rename_key(effect, "Amplifier", "amplifier");
    rename_key(effect, "Duration", "duration");
    rename_key(effect, "ShowParticles", "show_particles");
    rename_key(effect, "ShowIcon", "show_icon");
    if let Some(mut hidden) = effect.remove("HiddenEffect") {
        if let Some(inner) = hidden.as_compound_mut() {
            fix_effect_instance(inner);
        }
        effect.insert("hidden_effect", hidden);
    }
}

fn fix_effect_list(root: &mut Compound, from: &str, to: &str) {
    let Some(Tag::List(mut effects)) = root.remove(from) else {
        return;
    };
    for effect in &mut effects {
        if let Some(effect) = effect.as_compound_mut() {
            fix_effect_instance(effect);
        }
    }
    root.insert(to, Tag::List(effects));
}

fn fix_stew_entry(entry: &mut Compound) {
    replace_effect_id(entry, "EffectId", "id");
    rename_key(entry, "EffectDuration", "duration");
}

fn fix_mooshroom(entity: &mut Compound) {
    let mut entry = Compound::new();
    if let Some(id) = entity.remove("EffectId") {
        entry.insert("EffectId", id);
    }
    if let Some(duration) = entity.remove("EffectDuration") {
        entry.insert("EffectDuration", duration);
    }
    fix_stew_entry(&mut entry);
    if !entry.is_empty() {
        entity.insert("stew_effects", Tag::List(vec![Tag::Compound(entry)]));
    }
}

pub fn mob_effect_ids_3568(root: &mut Compound) {
    walk_entities(root, &mut |compound| {
        fix_effect_list(compound, "ActiveEffects", "active_effects");
        match entity_id(compound) {
            Some("minecraft:mooshroom") => fix_mooshroom(compound),
            Some("minecraft:arrow") => {
                fix_effect_list(compound, "CustomPotionEffects", "custom_potion_effects");
            }
            Some("minecraft:area_effect_cloud") => fix_effect_list(compound, "Effects", "effects"),
            Some("minecraft:beacon") => {
                replace_effect_id(compound, "Primary", "primary_effect");
                replace_effect_id(compound, "Secondary", "secondary_effect");
            }
            _ => {}
        }
    });
    walk_item_stacks(root, &mut |stack| {
        let id = stack.get_str("id").map(str::to_owned);
        let Some(tag) = compound_mut(stack, "tag") else {
            return;
        };
        match id.as_deref() {
            Some("minecraft:suspicious_stew") => {
                if let Some(Tag::List(mut effects)) = tag.remove("Effects") {
                    for effect in &mut effects {
                        if let Some(effect) = effect.as_compound_mut() {
                            fix_stew_entry(effect);
                        }
                    }
                    tag.insert("effects", Tag::List(effects));
                }
            }
            Some(
                "minecraft:potion"
                | "minecraft:splash_potion"
                | "minecraft:lingering_potion"
                | "minecraft:tipped_arrow",
            ) => fix_effect_list(tag, "CustomPotionEffects", "custom_potion_effects"),
            _ => {}
        }
    });
}

fn rename_attribute_names(root: &mut Compound, from: &str, to: &str) {
    walk_entities(root, &mut |compound| {
        if let Some(attributes) = list_mut(compound, "Attributes") {
            for attribute in attributes {
                if let Some(attribute) = attribute.as_compound_mut()
                    && attribute.get_str("Name") == Some(from)
                {
                    attribute.insert("Name", to);
                }
            }
        }
    });
    walk_item_stacks(root, &mut |stack| {
        let Some(modifiers) =
            compound_mut(stack, "tag").and_then(|t| list_mut(t, "AttributeModifiers"))
        else {
            return;
        };
        for modifier in modifiers {
            if let Some(modifier) = modifier.as_compound_mut()
                && modifier.get_str("AttributeName") == Some(from)
            {
                modifier.insert("AttributeName", to);
            }
        }
    });
}

pub fn jump_strength_attribute_3814(root: &mut Compound) {
    rename_attribute_names(
        root,
        "minecraft:horse.jump_strength",
        "minecraft:generic.jump_strength",
    );
}

fn uuid_text(parts: &[i32]) -> Option<String> {
    let [a, b, c, d] = parts[..] else {
        return None;
    };
    let high = (u64::from(a as u32) << 32) | u64::from(b as u32);
    let low = (u64::from(c as u32) << 32) | u64::from(d as u32);
    Some(format!(
        "{:08x}-{:04x}-{:04x}-{:04x}-{:012x}",
        high >> 32,
        (high >> 16) & 0xffff,
        high & 0xffff,
        low >> 48,
        low & 0xffff_ffff_ffff
    ))
}

fn modifier_amount(modifier: &Compound) -> f64 {
    modifier.get_f64("amount").unwrap_or(0.0)
}

fn fix_modifiers(modifiers: Vec<Tag>) -> Vec<Tag> {
    let mut merged: Vec<(String, Compound)> = Vec::new();
    for tag in modifiers {
        let Tag::Compound(mut modifier) = tag else {
            continue;
        };
        let uuid = modifier.get_int_array("uuid").and_then(uuid_text);
        let name = modifier.get_str("name").unwrap_or("").to_owned();
        let by_uuid = uuid.as_deref().and_then(|text| {
            MODIFIER_IDS_BY_UUID
                .iter()
                .find(|(key, _)| *key == text)
                .map(|(_, id)| *id)
        });
        let by_name = MODIFIER_IDS_BY_NAME
            .iter()
            .find(|(key, _)| *key == name)
            .map(|(_, id)| *id);
        let existing_name = by_name.and_then(|id| merged.iter().position(|(key, _)| key == id));
        let (key, entry) = if let Some(id) = by_uuid {
            (id.to_owned(), None)
        } else if let (Some(id), Some(position)) = (by_name, existing_name) {
            let mut kept = merged[position].1.clone();
            let total = modifier_amount(&kept) + modifier_amount(&modifier);
            kept.insert("amount", total);
            (id.to_owned(), Some(kept))
        } else if let Some(id) = by_name {
            (id.to_owned(), None)
        } else {
            (
                format!("minecraft:{}", uuid.as_deref().unwrap_or("unknown")),
                None,
            )
        };
        let mut value = entry.unwrap_or_else(|| {
            modifier.insert("id", key.as_str());
            modifier.remove("uuid");
            modifier.remove("name");
            modifier
        });
        value.remove("uuid");
        value.remove("name");
        value.insert("id", key.as_str());
        match merged.iter().position(|(existing, _)| *existing == key) {
            Some(position) => merged[position].1 = value,
            None => merged.push((key, value)),
        }
    }
    merged
        .into_iter()
        .map(|(_, modifier)| Tag::Compound(modifier))
        .collect()
}

fn operation_name(tag: Option<&Tag>) -> &'static str {
    match tag.and_then(Tag::as_i64).unwrap_or(0) {
        0 => "add_value",
        1 => "add_multiplied_base",
        2 => "add_multiplied_total",
        _ => "invalid",
    }
}

fn convert_entity_modifier(modifier: &mut Compound) {
    rename_key(modifier, "UUID", "uuid");
    rename_key(modifier, "Name", "name");
    rename_key(modifier, "Amount", "amount");
    if let Some(operation) = modifier.remove("Operation") {
        modifier.insert("operation", operation_name(Some(&operation)));
    }
}

pub fn attribute_modifier_ids_3945(root: &mut Compound) {
    walk_entities(root, &mut |compound| {
        let Some(Tag::List(attributes)) = compound.remove("Attributes") else {
            return;
        };
        let mut fixed = Vec::with_capacity(attributes.len());
        for attribute in attributes {
            let Tag::Compound(mut attribute) = attribute else {
                continue;
            };
            rename_key(&mut attribute, "Name", "id");
            rename_key(&mut attribute, "Base", "base");
            if let Some(Tag::List(mut modifiers)) = attribute.remove("Modifiers") {
                for modifier in &mut modifiers {
                    if let Some(modifier) = modifier.as_compound_mut() {
                        convert_entity_modifier(modifier);
                    }
                }
                attribute.insert("modifiers", Tag::List(fix_modifiers(modifiers)));
            }
            fixed.push(Tag::Compound(attribute));
        }
        compound.insert("attributes", Tag::List(fixed));
    });
    walk_item_stacks(root, &mut |stack| {
        let Some(modifiers) = compound_mut(stack, "components")
            .and_then(|components| compound_mut(components, "minecraft:attribute_modifiers"))
        else {
            return;
        };
        if let Some(Tag::List(list)) = modifiers.remove("modifiers") {
            modifiers.insert("modifiers", Tag::List(fix_modifiers(list)));
        }
    });
}

fn strip_attribute_prefix(name: &str) -> Option<String> {
    let namespaced = ensure_namespaced(name);
    ["generic.", "horse.", "player.", "zombie."]
        .iter()
        .find_map(|prefix| {
            namespaced
                .strip_prefix("minecraft:")
                .and_then(|path| path.strip_prefix(prefix))
                .map(|rest| format!("minecraft:{rest}"))
        })
}

fn rename_attribute_ids(root: &mut Compound, rename: &impl Fn(&str) -> Option<String>) {
    walk_entities(root, &mut |compound| {
        if let Some(attributes) = list_mut(compound, "attributes") {
            for attribute in attributes {
                if let Some(attribute) = attribute.as_compound_mut() {
                    super::nbt::rename_string(attribute, "id", rename);
                }
            }
        }
    });
    walk_item_stacks(root, &mut |stack| {
        let Some(modifiers) = compound_mut(stack, "components")
            .and_then(|components| components.get_mut("minecraft:attribute_modifiers"))
        else {
            return;
        };
        let list = match modifiers {
            Tag::List(list) => Some(list),
            Tag::Compound(wrapper) => list_mut(wrapper, "modifiers"),
            _ => None,
        };
        for modifier in list.into_iter().flatten() {
            if let Some(modifier) = modifier.as_compound_mut() {
                super::nbt::rename_string(modifier, "type", rename);
            }
        }
    });
}

pub fn attribute_id_prefix_4055(root: &mut Compound) {
    rename_attribute_ids(root, &strip_attribute_prefix);
}

pub fn nameplate_attribute_4888(root: &mut Compound) {
    rename_attribute_ids(root, &|name| {
        (ensure_namespaced(name) == "minecraft:nameplate_distance")
            .then(|| "minecraft:name_tag_distance".to_owned())
    });
}

pub fn boat_split_4067(root: &mut Compound) {
    walk_entities(root, &mut |compound| {
        let chest = match entity_id(compound) {
            Some("minecraft:boat") => false,
            Some("minecraft:chest_boat") => true,
            _ => return,
        };
        let wood = compound
            .remove("Type")
            .and_then(|tag| tag.as_str().map(str::to_owned))
            .unwrap_or_default();
        let wood = BOAT_WOODS
            .iter()
            .copied()
            .find(|candidate| *candidate == wood)
            .unwrap_or("oak");
        let id = match (wood, chest) {
            ("bamboo", false) => "minecraft:bamboo_raft".to_owned(),
            ("bamboo", true) => "minecraft:bamboo_chest_raft".to_owned(),
            (wood, false) => format!("minecraft:{wood}_boat"),
            (wood, true) => format!("minecraft:{wood}_chest_boat"),
        };
        compound.insert("id", id);
    });
}

fn float_list(tag: Option<Tag>) -> Vec<f32> {
    tag.as_ref()
        .and_then(Tag::as_list)
        .map(|list| {
            list.iter()
                .map(|value| value.as_f64().map_or(DEFAULT_DROP_CHANCE, |v| v as f32))
                .collect()
        })
        .unwrap_or_default()
}

fn add_slot_chances(chances: &mut Compound, names: &[&str], values: &[f32]) {
    for (name, value) in names.iter().zip(values) {
        if *value != DEFAULT_DROP_CHANCE {
            chances.insert(*name, *value);
        }
    }
}

pub fn drop_chances_format_4293(root: &mut Compound) {
    walk_entities(root, &mut |compound| {
        if ![
            "ArmorDropChances",
            "HandDropChances",
            "body_armor_drop_chance",
        ]
        .iter()
        .any(|key| compound.contains_key(key))
        {
            return;
        }
        let armor = float_list(compound.remove("ArmorDropChances"));
        let hand = float_list(compound.remove("HandDropChances"));
        let body = compound
            .remove("body_armor_drop_chance")
            .and_then(|tag| tag.as_f64())
            .map_or(DEFAULT_DROP_CHANCE, |value| value as f32);
        let mut chances = Compound::new();
        add_slot_chances(&mut chances, ARMOR_SLOTS, &armor);
        add_slot_chances(&mut chances, HAND_SLOTS, &hand);
        if body != DEFAULT_DROP_CHANCE {
            chances.insert("body", body);
        }
        if !chances.is_empty() {
            compound.insert("drop_chances", chances);
        }
    });
}

fn set_saddle_drop_chance(compound: &mut Compound) {
    let mut chances = match compound.remove("drop_chances") {
        Some(Tag::Compound(chances)) => chances,
        _ => Compound::new(),
    };
    chances.insert("saddle", 2.0f32);
    compound.insert("drop_chances", chances);
}

pub fn saddle_equipment_slot_4300(root: &mut Compound) {
    walk_entities(root, &mut |compound| match entity_id(compound) {
        Some(
            "minecraft:horse"
            | "minecraft:skeleton_horse"
            | "minecraft:zombie_horse"
            | "minecraft:donkey"
            | "minecraft:mule"
            | "minecraft:camel"
            | "minecraft:llama"
            | "minecraft:trader_llama",
        ) => {
            if compound.contains_key("SaddleItem") {
                rename_key(compound, "SaddleItem", "saddle");
                set_saddle_drop_chance(compound);
            }
        }
        Some("minecraft:pig" | "minecraft:strider") => {
            let saddled = compound.get_bool("Saddle").unwrap_or(false);
            compound.remove("Saddle");
            if saddled {
                compound.insert(
                    "saddle",
                    Compound::new()
                        .with("id", "minecraft:saddle")
                        .with("count", 1),
                );
                set_saddle_drop_chance(compound);
            }
        }
        _ => {}
    });
}

fn present(tag: Option<&Tag>) -> Option<Tag> {
    tag.and_then(Tag::as_compound)
        .filter(|stack| !is_empty_stack(stack))
        .map(|stack| Tag::Compound(stack.clone()))
}

fn slot_from_list(list: Option<&Tag>, index: usize) -> Option<Tag> {
    present(
        list.and_then(Tag::as_list)
            .and_then(|items| items.get(index)),
    )
}

pub fn equipment_format_4301(root: &mut Compound) {
    walk_entities(root, &mut |compound| {
        let has_lists = compound.contains_key("ArmorItems") || compound.contains_key("HandItems");
        let has_body = compound.contains_key("body_armor_item");
        let saddle_stack = compound
            .get("saddle")
            .and_then(Tag::as_compound)
            .is_some_and(|stack| stack.contains_key("id") || stack.is_empty());
        if !has_lists && !has_body {
            return;
        }
        let armor = compound.remove("ArmorItems");
        let hand = compound.remove("HandItems");
        let body = compound.remove("body_armor_item");
        let saddle = if saddle_stack {
            compound.remove("saddle")
        } else {
            None
        };
        let mut equipment = Compound::new();
        for (index, slot) in HAND_SLOTS.iter().enumerate() {
            if let Some(stack) = slot_from_list(hand.as_ref(), index) {
                equipment.insert(*slot, stack);
            }
        }
        for (index, slot) in ARMOR_SLOTS.iter().enumerate() {
            if let Some(stack) = slot_from_list(armor.as_ref(), index) {
                equipment.insert(*slot, stack);
            }
        }
        if let Some(stack) = present(body.as_ref()) {
            equipment.insert("body", stack);
        }
        if let Some(stack) = present(saddle.as_ref()) {
            equipment.insert("saddle", stack);
        }
        if !equipment.is_empty() {
            compound.insert("equipment", equipment);
        }
    });
}

pub fn player_equipment_4312(root: &mut Compound) {
    if root.contains_key("equipment") {
        return;
    }
    let mut equipment = Compound::new();
    if let Some(Tag::List(inventory)) = root.remove("Inventory") {
        let mut kept = Vec::with_capacity(inventory.len());
        for tag in inventory {
            let slot = tag
                .as_compound()
                .and_then(|stack| stack.get_i64("Slot"))
                .and_then(|slot| match slot {
                    100 => Some("feet"),
                    101 => Some("legs"),
                    102 => Some("chest"),
                    103 => Some("head"),
                    -106 => Some("offhand"),
                    _ => None,
                });
            match (slot, tag) {
                (Some(name), Tag::Compound(mut stack)) => {
                    stack.remove("Slot");
                    equipment.insert(name, stack);
                }
                (_, tag) => kept.push(tag),
            }
        }
        root.insert("Inventory", Tag::List(kept));
    }
    root.insert("equipment", equipment);
}

pub fn area_effect_cloud_duration_scale_4296(root: &mut Compound) {
    walk_entities(root, &mut |compound| {
        if entity_id(compound) == Some("minecraft:area_effect_cloud") {
            compound.insert("potion_duration_scale", 0.25f32);
        }
    });
}

pub fn fall_distance_4303(root: &mut Compound) {
    walk_entities(root, &mut |compound| {
        if let Some(distance) = compound.remove("FallDistance") {
            compound.insert("fall_distance", distance.as_f64().unwrap_or(0.0));
        }
    });
}

pub fn thrown_potion_split_4306(root: &mut Compound) {
    walk_entities(root, &mut |compound| {
        if entity_id(compound) != Some("minecraft:potion") {
            return;
        }
        let Some(item) = compound.get_compound("Item") else {
            return;
        };
        let lingering = item.get_str("id") == Some("minecraft:lingering_potion");
        compound.insert(
            "id",
            if lingering {
                "minecraft:lingering_potion"
            } else {
                "minecraft:splash_potion"
            },
        );
    });
}

pub fn copper_golem_weather_state_4535(root: &mut Compound) {
    walk_entities(root, &mut |compound| {
        if entity_id(compound) != Some("minecraft:copper_golem") {
            return;
        }
        if let Some(state) = compound.get("weather_state").and_then(Tag::as_i64) {
            let name = match state {
                1 => "exposed",
                2 => "weathered",
                3 => "oxidized",
                _ => "unaffected",
            };
            compound.insert("weather_state", name);
        }
    });
}

fn fix_block_pos(compound: &mut Compound, from: &str, to: &str) {
    let Some(Tag::Compound(pos)) = compound.get(from) else {
        return;
    };
    let (Some(x), Some(y), Some(z)) = (pos.get_i32("X"), pos.get_i32("Y"), pos.get_i32("Z")) else {
        return;
    };
    compound.remove(from);
    compound.insert(to, Tag::IntArray(vec![x, y, z]));
}

pub fn block_pos_format_3813(root: &mut Compound) {
    walk_entities(root, &mut |compound| {
        let renames: &[(&str, &str)] = match entity_id(compound) {
            Some("minecraft:bee") => &[("HivePos", "hive_pos"), ("FlowerPos", "flower_pos")],
            Some("minecraft:end_crystal") => &[("BeamTarget", "beam_target")],
            Some("minecraft:wandering_trader") => &[("WanderTarget", "wander_target")],
            Some("minecraft:beehive") => &[("FlowerPos", "flower_pos")],
            Some("minecraft:end_gateway") => &[("ExitPortal", "exit_portal")],
            Some(
                "minecraft:witch"
                | "minecraft:ravager"
                | "minecraft:pillager"
                | "minecraft:illusioner"
                | "minecraft:evoker"
                | "minecraft:vindicator",
            ) => &[("PatrolTarget", "patrol_target")],
            _ => &[],
        };
        for (from, to) in renames {
            fix_block_pos(compound, from, to);
        }
        if let Some(Tag::Compound(leash)) = compound.remove("Leash") {
            match (leash.get_i32("X"), leash.get_i32("Y"), leash.get_i32("Z")) {
                (Some(x), Some(y), Some(z)) => {
                    compound.insert("leash", Tag::IntArray(vec![x, y, z]));
                }
                _ => {
                    compound.insert("leash", leash);
                }
            }
        }
    });
    walk_item_stacks(root, &mut |stack| {
        if stack.get_str("id") == Some("minecraft:compass")
            && let Some(tag) = compound_mut(stack, "tag")
        {
            fix_block_pos(tag, "LodestonePos", "LodestonePos");
        }
    });
}

fn fix_inline_pos(compound: &mut Compound, keys: [&str; 3], to: &str) {
    let (Some(x), Some(y), Some(z)) = (
        compound.get_i32(keys[0]),
        compound.get_i32(keys[1]),
        compound.get_i32(keys[2]),
    ) else {
        return;
    };
    for key in keys {
        compound.remove(key);
    }
    compound.insert(to, Tag::IntArray(vec![x, y, z]));
}

pub fn inline_block_pos_4314(root: &mut Compound) {
    walk_entities(root, &mut |compound| {
        fix_inline_pos(
            compound,
            ["SleepingX", "SleepingY", "SleepingZ"],
            "sleeping_pos",
        );
        match entity_id(compound) {
            Some("minecraft:vex") => {
                rename_key(compound, "LifeTicks", "life_ticks");
                fix_inline_pos(compound, ["BoundX", "BoundY", "BoundZ"], "bound_pos");
            }
            Some("minecraft:phantom") => {
                rename_key(compound, "Size", "size");
                fix_inline_pos(compound, ["AX", "AY", "AZ"], "anchor_pos");
            }
            Some("minecraft:turtle") => {
                for key in ["TravelPosX", "TravelPosY", "TravelPosZ"] {
                    compound.remove(key);
                }
                fix_inline_pos(compound, ["HomePosX", "HomePosY", "HomePosZ"], "home_pos");
                rename_key(compound, "HasEgg", "has_egg");
            }
            Some(
                "minecraft:item_frame"
                | "minecraft:glow_item_frame"
                | "minecraft:painting"
                | "minecraft:leash_knot",
            ) => fix_inline_pos(compound, ["TileX", "TileY", "TileZ"], "block_pos"),
            _ => {}
        }
    });
}

pub fn player_inline_positions_4314(root: &mut Compound) {
    if let (Some(x), Some(y), Some(z)) = (
        root.get_i32("SpawnX"),
        root.get_i32("SpawnY"),
        root.get_i32("SpawnZ"),
    ) {
        let mut respawn = Compound::new().with("pos", Tag::IntArray(vec![x, y, z]));
        for (from, to) in [
            ("SpawnAngle", "angle"),
            ("SpawnDimension", "dimension"),
            ("SpawnForced", "forced"),
        ] {
            if let Some(value) = root.get(from) {
                respawn.insert(to, value.clone());
            }
        }
        for key in [
            "SpawnX",
            "SpawnY",
            "SpawnZ",
            "SpawnAngle",
            "SpawnDimension",
            "SpawnForced",
        ] {
            root.remove(key);
        }
        root.insert("respawn", respawn);
    }
    if let Some(Tag::Compound(position)) = root.remove("enteredNetherPosition") {
        let coordinate = |key: &str| Tag::Double(position.get_f64(key).unwrap_or(0.0));
        root.insert(
            "entered_nether_pos",
            Tag::List(vec![coordinate("x"), coordinate("y"), coordinate("z")]),
        );
    }
}

pub fn villager_data_finalized_4763(root: &mut Compound) {
    walk_entities(root, &mut |compound| {
        if matches!(
            entity_id(compound),
            Some("minecraft:villager" | "minecraft:zombie_villager")
        ) {
            compound.insert("VillagerDataFinalized", true);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stack(id: &str) -> Compound {
        Compound::new().with("id", id).with("count", 1)
    }

    #[test]
    fn armor_and_hand_items_become_the_equipment_compound() {
        let mut zombie = Compound::new()
            .with("id", "minecraft:zombie")
            .with(
                "ArmorItems",
                Tag::List(vec![
                    Tag::Compound(Compound::new()),
                    Tag::Compound(Compound::new()),
                    Tag::Compound(stack("minecraft:iron_chestplate")),
                    Tag::Compound(stack("minecraft:iron_helmet")),
                ]),
            )
            .with(
                "HandItems",
                Tag::List(vec![
                    Tag::Compound(stack("minecraft:iron_sword")),
                    Tag::Compound(Compound::new()),
                ]),
            );
        equipment_format_4301(&mut zombie);
        assert!(!zombie.contains_key("ArmorItems") && !zombie.contains_key("HandItems"));
        let equipment = zombie.get_compound("equipment").unwrap();
        assert_eq!(
            equipment.get_compound("mainhand").unwrap().get_str("id"),
            Some("minecraft:iron_sword")
        );
        assert_eq!(
            equipment.get_compound("chest").unwrap().get_str("id"),
            Some("minecraft:iron_chestplate")
        );
        assert_eq!(
            equipment.get_compound("head").unwrap().get_str("id"),
            Some("minecraft:iron_helmet")
        );
        assert!(!equipment.contains_key("feet") && !equipment.contains_key("offhand"));
    }

    #[test]
    fn empty_equipment_is_dropped_entirely() {
        let mut cow = Compound::new()
            .with("id", "minecraft:cow")
            .with(
                "ArmorItems",
                Tag::List(vec![Tag::Compound(Compound::new()); 4]),
            )
            .with(
                "HandItems",
                Tag::List(vec![Tag::Compound(Compound::new()); 2]),
            );
        equipment_format_4301(&mut cow);
        assert!(!cow.contains_key("equipment") && !cow.contains_key("ArmorItems"));
    }

    #[test]
    fn body_armor_and_saddle_join_the_equipment() {
        let mut horse = Compound::new()
            .with("id", "minecraft:horse")
            .with("body_armor_item", stack("minecraft:iron_horse_armor"))
            .with("saddle", stack("minecraft:saddle"));
        equipment_format_4301(&mut horse);
        let equipment = horse.get_compound("equipment").unwrap();
        assert_eq!(
            equipment.get_compound("body").unwrap().get_str("id"),
            Some("minecraft:iron_horse_armor")
        );
        assert_eq!(
            equipment.get_compound("saddle").unwrap().get_str("id"),
            Some("minecraft:saddle")
        );
        assert!(!horse.contains_key("saddle") && !horse.contains_key("body_armor_item"));
    }

    #[test]
    fn drop_chances_collapse_into_a_compound_of_non_default_values() {
        let mut zombie = Compound::new()
            .with("ArmorDropChances", Tag::List(vec![Tag::Float(0.085); 4]))
            .with(
                "HandDropChances",
                Tag::List(vec![Tag::Float(1.0), Tag::Float(0.085)]),
            )
            .with("body_armor_drop_chance", 0.5f32);
        drop_chances_format_4293(&mut zombie);
        let chances = zombie.get_compound("drop_chances").unwrap();
        assert_eq!(chances.get_f64("mainhand"), Some(1.0));
        assert_eq!(chances.get_f64("body"), Some(0.5));
        assert_eq!(chances.len(), 2);
        assert!(!zombie.contains_key("HandDropChances"));
    }

    #[test]
    fn saddle_items_and_flags_move_to_the_saddle_slot() {
        let mut horse = Compound::new()
            .with("id", "minecraft:horse")
            .with("SaddleItem", stack("minecraft:saddle"));
        saddle_equipment_slot_4300(&mut horse);
        assert!(horse.contains_key("saddle") && !horse.contains_key("SaddleItem"));
        assert_eq!(
            horse
                .get_compound("drop_chances")
                .unwrap()
                .get_f64("saddle"),
            Some(2.0)
        );
        let mut pig = Compound::new()
            .with("id", "minecraft:pig")
            .with("Saddle", true);
        saddle_equipment_slot_4300(&mut pig);
        assert_eq!(
            pig.get_compound("saddle").unwrap().get_str("id"),
            Some("minecraft:saddle")
        );
        assert!(!pig.contains_key("Saddle"));
        let mut bare = Compound::new()
            .with("id", "minecraft:pig")
            .with("Saddle", false);
        saddle_equipment_slot_4300(&mut bare);
        assert!(!bare.contains_key("saddle") && !bare.contains_key("Saddle"));
    }

    #[test]
    fn player_armor_slots_move_out_of_the_inventory() {
        let mut player = Compound::new().with(
            "Inventory",
            Tag::List(vec![
                Tag::Compound(stack("minecraft:stone").with("Slot", 0i8)),
                Tag::Compound(stack("minecraft:iron_boots").with("Slot", 100i8)),
                Tag::Compound(stack("minecraft:shield").with("Slot", -106i8)),
            ]),
        );
        player_equipment_4312(&mut player);
        assert_eq!(player.get_list("Inventory").unwrap().len(), 1);
        let equipment = player.get_compound("equipment").unwrap();
        assert_eq!(
            equipment.get_compound("feet").unwrap().get_str("id"),
            Some("minecraft:iron_boots")
        );
        assert!(!equipment.get_compound("feet").unwrap().contains_key("Slot"));
        assert_eq!(
            equipment.get_compound("offhand").unwrap().get_str("id"),
            Some("minecraft:shield")
        );
    }

    #[test]
    fn boats_split_by_wood_and_chest() {
        let mut boat = Compound::new()
            .with("id", "minecraft:boat")
            .with("Type", "bamboo");
        boat_split_4067(&mut boat);
        assert_eq!(boat.get_str("id"), Some("minecraft:bamboo_raft"));
        assert!(!boat.contains_key("Type"));
        let mut chest = Compound::new()
            .with("id", "minecraft:chest_boat")
            .with("Type", "dark_oak");
        boat_split_4067(&mut chest);
        assert_eq!(chest.get_str("id"), Some("minecraft:dark_oak_chest_boat"));
        let mut plain = Compound::new().with("id", "minecraft:boat");
        boat_split_4067(&mut plain);
        assert_eq!(plain.get_str("id"), Some("minecraft:oak_boat"));
    }

    #[test]
    fn attribute_ids_lose_their_category_prefix() {
        let mut zombie = Compound::new().with(
            "attributes",
            Tag::List(vec![
                Tag::Compound(
                    Compound::new()
                        .with("id", "minecraft:generic.max_health")
                        .with("base", 20.0),
                ),
                Tag::Compound(
                    Compound::new()
                        .with("id", "zombie.spawn_reinforcements")
                        .with("base", 0.1),
                ),
                Tag::Compound(
                    Compound::new()
                        .with("id", "minecraft:scale")
                        .with("base", 1.0),
                ),
            ]),
        );
        attribute_id_prefix_4055(&mut zombie);
        let ids: Vec<_> = zombie
            .get_list("attributes")
            .unwrap()
            .iter()
            .map(|a| a.as_compound().unwrap().get_str("id").unwrap().to_owned())
            .collect();
        assert_eq!(
            ids,
            [
                "minecraft:max_health",
                "minecraft:spawn_reinforcements",
                "minecraft:scale"
            ]
        );
    }

    #[test]
    fn item_modifier_types_lose_their_prefix_in_both_component_shapes() {
        let modifier =
            Tag::Compound(Compound::new().with("type", "minecraft:generic.attack_damage"));
        let mut wrapped = Compound::new()
            .with("id", "minecraft:stick")
            .with("count", 1)
            .with(
                "components",
                Compound::new().with(
                    "minecraft:attribute_modifiers",
                    Compound::new().with("modifiers", Tag::List(vec![modifier.clone()])),
                ),
            );
        attribute_id_prefix_4055(&mut wrapped);
        let fixed = wrapped
            .get_compound("components")
            .unwrap()
            .get_compound("minecraft:attribute_modifiers")
            .unwrap()
            .get_list("modifiers")
            .unwrap()[0]
            .as_compound()
            .unwrap()
            .get_str("type")
            .unwrap()
            .to_owned();
        assert_eq!(fixed, "minecraft:attack_damage");
    }

    #[test]
    fn name_plate_distance_is_renamed() {
        let mut player = Compound::new().with(
            "attributes",
            Tag::List(vec![Tag::Compound(
                Compound::new().with("id", "minecraft:nameplate_distance"),
            )]),
        );
        nameplate_attribute_4888(&mut player);
        assert_eq!(
            player.get_list("attributes").unwrap()[0]
                .as_compound()
                .unwrap()
                .get_str("id"),
            Some("minecraft:name_tag_distance")
        );
    }

    #[test]
    fn legacy_attributes_get_ids_and_string_operations() {
        let mut zombie = Compound::new().with(
            "Attributes",
            Tag::List(vec![Tag::Compound(
                Compound::new()
                    .with("Name", "minecraft:generic.movement_speed")
                    .with("Base", 0.23)
                    .with(
                        "Modifiers",
                        Tag::List(vec![Tag::Compound(
                            Compound::new()
                                .with("Name", "Baby speed boost")
                                .with("Amount", 0.5)
                                .with("Operation", 1)
                                .with(
                                    "UUID",
                                    Tag::IntArray(vec![
                                        0x766bfa64,
                                        0x11f3_11eau32 as i32,
                                        0x8d71_362bu32 as i32,
                                        0x9e15_5667u32 as i32,
                                    ]),
                                ),
                        )]),
                    ),
            )]),
        );
        attribute_modifier_ids_3945(&mut zombie);
        assert!(!zombie.contains_key("Attributes"));
        let attribute = zombie.get_list("attributes").unwrap()[0]
            .as_compound()
            .unwrap();
        assert_eq!(
            attribute.get_str("id"),
            Some("minecraft:generic.movement_speed")
        );
        assert_eq!(attribute.get_f64("base"), Some(0.23));
        let modifier = attribute.get_list("modifiers").unwrap()[0]
            .as_compound()
            .unwrap();
        assert_eq!(modifier.get_str("id"), Some("minecraft:baby"));
        assert_eq!(modifier.get_str("operation"), Some("add_multiplied_base"));
        assert_eq!(modifier.get_f64("amount"), Some(0.5));
        assert!(!modifier.contains_key("uuid") && !modifier.contains_key("name"));
    }

    #[test]
    fn unknown_modifiers_are_keyed_by_their_uuid() {
        let modifiers = fix_modifiers(vec![Tag::Compound(
            Compound::new()
                .with("uuid", Tag::IntArray(vec![1, 2, 3, 4]))
                .with("amount", 1.0),
        )]);
        assert_eq!(
            modifiers[0].as_compound().unwrap().get_str("id"),
            Some("minecraft:00000001-0000-0002-0000-000300000004")
        );
    }

    #[test]
    fn modifiers_sharing_a_known_name_are_merged() {
        let named = |amount: f64| {
            Tag::Compound(
                Compound::new()
                    .with("name", "Random spawn bonus")
                    .with("amount", amount),
            )
        };
        let merged = fix_modifiers(vec![named(0.25), named(0.5)]);
        assert_eq!(merged.len(), 1);
        let only = merged[0].as_compound().unwrap();
        assert_eq!(only.get_str("id"), Some("minecraft:random_spawn_bonus"));
        assert_eq!(only.get_f64("amount"), Some(0.75));
    }

    #[test]
    fn numeric_effect_ids_become_names_and_fields_are_snake_case() {
        let mut zombie = Compound::new().with(
            "ActiveEffects",
            Tag::List(vec![Tag::Compound(
                Compound::new()
                    .with("Id", 1i8)
                    .with("Amplifier", 2i8)
                    .with("Duration", 100)
                    .with("ShowParticles", true),
            )]),
        );
        mob_effect_ids_3568(&mut zombie);
        let effect = zombie.get_list("active_effects").unwrap()[0]
            .as_compound()
            .unwrap();
        assert_eq!(effect.get_str("id"), Some("minecraft:speed"));
        assert_eq!(effect.get_i32("amplifier"), Some(2));
        assert_eq!(effect.get_i32("duration"), Some(100));
        assert_eq!(effect.get_bool("show_particles"), Some(true));
    }

    #[test]
    fn beacon_and_stew_effects_use_names() {
        let mut chunk = Compound::new().with(
            "block_entities",
            Tag::List(vec![Tag::Compound(
                Compound::new()
                    .with("id", "minecraft:beacon")
                    .with("Primary", 1)
                    .with("Secondary", 10),
            )]),
        );
        mob_effect_ids_3568(&mut chunk);
        let beacon = chunk.get_list("block_entities").unwrap()[0]
            .as_compound()
            .unwrap();
        assert_eq!(beacon.get_str("primary_effect"), Some("minecraft:speed"));
        assert_eq!(
            beacon.get_str("secondary_effect"),
            Some("minecraft:regeneration")
        );
        let mut stew = Compound::new()
            .with("id", "minecraft:suspicious_stew")
            .with("Count", 1i8)
            .with(
                "tag",
                Compound::new().with(
                    "Effects",
                    Tag::List(vec![Tag::Compound(
                        Compound::new()
                            .with("EffectId", 8i8)
                            .with("EffectDuration", 60),
                    )]),
                ),
            );
        mob_effect_ids_3568(&mut stew);
        let entry = stew
            .get_compound("tag")
            .unwrap()
            .get_list("effects")
            .unwrap()[0]
            .as_compound()
            .unwrap();
        assert_eq!(entry.get_str("id"), Some("minecraft:jump_boost"));
        assert_eq!(entry.get_i32("duration"), Some(60));
    }

    #[test]
    fn small_entity_fixes() {
        let mut cloud = Compound::new().with("id", "minecraft:area_effect_cloud");
        area_effect_cloud_duration_scale_4296(&mut cloud);
        assert_eq!(cloud.get_f64("potion_duration_scale"), Some(0.25));
        let mut fall = Compound::new().with("FallDistance", 2.5f32);
        fall_distance_4303(&mut fall);
        assert_eq!(fall.get("fall_distance"), Some(&Tag::Double(2.5)));
        let mut potion = Compound::new().with("id", "minecraft:potion").with(
            "Item",
            Compound::new().with("id", "minecraft:lingering_potion"),
        );
        thrown_potion_split_4306(&mut potion);
        assert_eq!(potion.get_str("id"), Some("minecraft:lingering_potion"));
        let mut splash = Compound::new()
            .with("id", "minecraft:potion")
            .with("Item", Compound::new().with("id", "minecraft:potion"));
        thrown_potion_split_4306(&mut splash);
        assert_eq!(splash.get_str("id"), Some("minecraft:splash_potion"));
        let mut item = Compound::new()
            .with("id", "minecraft:potion")
            .with("count", 1);
        thrown_potion_split_4306(&mut item);
        assert_eq!(item.get_str("id"), Some("minecraft:potion"));
        let mut golem = Compound::new()
            .with("id", "minecraft:copper_golem")
            .with("weather_state", 2);
        copper_golem_weather_state_4535(&mut golem);
        assert_eq!(golem.get_str("weather_state"), Some("weathered"));
        let mut villager = Compound::new().with("id", "minecraft:villager");
        villager_data_finalized_4763(&mut villager);
        assert_eq!(villager.get_bool("VillagerDataFinalized"), Some(true));
    }

    #[test]
    fn block_positions_become_int_arrays_with_snake_case_names() {
        let mut bee = Compound::new()
            .with("id", "minecraft:bee")
            .with(
                "HivePos",
                Compound::new().with("X", 1).with("Y", 2).with("Z", 3),
            )
            .with(
                "Leash",
                Compound::new().with("X", 4).with("Y", 5).with("Z", 6),
            );
        block_pos_format_3813(&mut bee);
        assert_eq!(bee.get_int_array("hive_pos"), Some(&[1, 2, 3][..]));
        assert_eq!(bee.get_int_array("leash"), Some(&[4, 5, 6][..]));
        assert!(!bee.contains_key("HivePos") && !bee.contains_key("Leash"));
        let mut tied = Compound::new().with("id", "minecraft:cow").with(
            "Leash",
            Compound::new().with("UUID", Tag::IntArray(vec![1, 2, 3, 4])),
        );
        block_pos_format_3813(&mut tied);
        assert!(tied.get_compound("leash").unwrap().contains_key("UUID"));
    }

    #[test]
    fn inline_coordinates_collapse_into_positions() {
        let mut turtle = Compound::new()
            .with("id", "minecraft:turtle")
            .with("HomePosX", 1)
            .with("HomePosY", 2)
            .with("HomePosZ", 3)
            .with("TravelPosX", 9)
            .with("HasEgg", true)
            .with("SleepingX", 7)
            .with("SleepingY", 8)
            .with("SleepingZ", 9);
        inline_block_pos_4314(&mut turtle);
        assert_eq!(turtle.get_int_array("home_pos"), Some(&[1, 2, 3][..]));
        assert_eq!(turtle.get_int_array("sleeping_pos"), Some(&[7, 8, 9][..]));
        assert_eq!(turtle.get_bool("has_egg"), Some(true));
        assert!(!turtle.contains_key("TravelPosX") && !turtle.contains_key("HomePosX"));
        let mut frame = Compound::new()
            .with("id", "minecraft:item_frame")
            .with("TileX", 1)
            .with("TileY", 2)
            .with("TileZ", 3);
        inline_block_pos_4314(&mut frame);
        assert_eq!(frame.get_int_array("block_pos"), Some(&[1, 2, 3][..]));
    }

    #[test]
    fn the_player_spawn_point_becomes_a_respawn_compound() {
        let mut player = Compound::new()
            .with("SpawnX", 1)
            .with("SpawnY", 2)
            .with("SpawnZ", 3)
            .with("SpawnAngle", 90.0f32)
            .with("SpawnDimension", "minecraft:the_nether")
            .with("SpawnForced", true)
            .with(
                "enteredNetherPosition",
                Compound::new()
                    .with("x", 1.5)
                    .with("y", 64.0)
                    .with("z", -2.5),
            );
        player_inline_positions_4314(&mut player);
        let respawn = player.get_compound("respawn").unwrap();
        assert_eq!(respawn.get_int_array("pos"), Some(&[1, 2, 3][..]));
        assert_eq!(respawn.get_str("dimension"), Some("minecraft:the_nether"));
        assert_eq!(respawn.get_bool("forced"), Some(true));
        assert!(!player.contains_key("SpawnX"));
        assert_eq!(player.get_list("entered_nether_pos").unwrap().len(), 3);
    }

    #[test]
    fn the_horse_jump_strength_attribute_name_changes_on_entities_and_items() {
        let mut horse = Compound::new().with(
            "Attributes",
            Tag::List(vec![Tag::Compound(
                Compound::new().with("Name", "minecraft:horse.jump_strength"),
            )]),
        );
        jump_strength_attribute_3814(&mut horse);
        assert_eq!(
            horse.get_list("Attributes").unwrap()[0]
                .as_compound()
                .unwrap()
                .get_str("Name"),
            Some("minecraft:generic.jump_strength")
        );
    }
}

mod chunk;
mod entity;
#[cfg(test)]
mod eras;
mod items;
mod level;
mod nbt;
mod renames;
mod text;

use rspigot_nbt::Compound;

use crate::error::AnvilError;

pub const DATA_VERSION: i32 = crate::chunk::DATA_VERSION;
pub const OLDEST_SUPPORTED: i32 = 2860;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DataKind {
    Chunk,
    Level,
    Player,
    Entity,
    Poi,
    SavedData,
}

#[derive(Clone, Copy)]
pub struct Fix {
    pub version: i32,
    pub kinds: &'static [DataKind],
    pub name: &'static str,
    pub apply: fn(&mut Compound),
}

const fn fix(
    version: i32,
    kinds: &'static [DataKind],
    name: &'static str,
    apply: fn(&mut Compound),
) -> Fix {
    Fix {
        version,
        kinds,
        name,
        apply,
    }
}

const CHUNK: &[DataKind] = &[DataKind::Chunk];
const ENTITY: &[DataKind] = &[DataKind::Entity];
const PLAYER: &[DataKind] = &[DataKind::Player];
const LEVEL: &[DataKind] = &[DataKind::Level];
const POI: &[DataKind] = &[DataKind::Poi];
const SAVED_DATA: &[DataKind] = &[DataKind::SavedData];
const ENTITY_HOLDERS: &[DataKind] = &[DataKind::Chunk, DataKind::Entity, DataKind::Player];
const ITEM_HOLDERS: &[DataKind] = &[DataKind::Chunk, DataKind::Entity, DataKind::Player];

static REGISTRY: &[Fix] = &[
    fix(
        3077,
        CHUNK,
        "delete ignored light data",
        chunk::delete_ignored_light_3077,
    ),
    fix(
        3097,
        POI,
        "remove unpopulated villager poi types",
        level::remove_unpopulated_poi_types_3097,
    ),
    fix(
        3438,
        CHUNK,
        "block entity suspicious_sand to brushable_block",
        renames::brushable_block_entity_3438,
    ),
    fix(
        3438,
        ITEM_HOLDERS,
        "pottery shard item names",
        renames::pottery_shard_items_3438,
    ),
    fix(
        3439,
        CHUNK,
        "double sided signs",
        chunk::sign_double_sided_3439,
    ),
    fix(
        3447,
        ITEM_HOLDERS,
        "pottery shard to sherd item names",
        renames::pottery_sherd_items_3447,
    ),
    fix(
        3448,
        CHUNK,
        "decorated pot sherds field",
        chunk::decorated_pot_sherds_3448,
    ),
    fix(
        3450,
        CHUNK,
        "remove liquid_carvers and heightmaps statuses",
        chunk::remap_status_3450,
    ),
    fix(3451, CHUNK, "delete light data", chunk::delete_light),
    fix(
        3459,
        LEVEL,
        "legacy dragon fight",
        level::legacy_dragon_fight_3459,
    ),
    fix(
        3564,
        CHUNK,
        "drop invalid sign data",
        chunk::drop_invalid_sign_data_3564,
    ),
    fix(
        3568,
        ENTITY_HOLDERS,
        "mob effect ids to names",
        entity::mob_effect_ids_3568,
    ),
    fix(
        3692,
        CHUNK,
        "grass block to short_grass",
        renames::grass_block_chunk_3692,
    ),
    fix(
        3692,
        ENTITY,
        "grass block state to short_grass",
        renames::grass_block_entity_3692,
    ),
    fix(
        3692,
        ITEM_HOLDERS,
        "grass item to short_grass",
        renames::grass_item_3692,
    ),
    fix(
        3800,
        ITEM_HOLDERS,
        "scute item to turtle_scute",
        renames::scute_item_3800,
    ),
    fix(
        3813,
        ENTITY_HOLDERS,
        "block position format and renames",
        entity::block_pos_format_3813,
    ),
    fix(
        3814,
        ENTITY_HOLDERS,
        "jump strength attribute name",
        entity::jump_strength_attribute_3814,
    ),
    fix(
        3818,
        ITEM_HOLDERS,
        "item stack componentization",
        items::componentize_3818,
    ),
    fix(
        3820,
        CHUNK,
        "skull block entity profile",
        items::skull_profile_3820,
    ),
    fix(
        3825,
        ITEM_HOLDERS,
        "ominous banner and explorer map names",
        items::ominous_banner_name_3825,
    ),
    fix(
        3945,
        ENTITY_HOLDERS,
        "attribute and modifier ids",
        entity::attribute_modifier_ids_3945,
    ),
    fix(
        4055,
        ENTITY_HOLDERS,
        "attribute id prefixes",
        entity::attribute_id_prefix_4055,
    ),
    fix(
        4059,
        ITEM_HOLDERS,
        "food to consumable",
        items::food_to_consumable_4059,
    ),
    fix(
        4064,
        ITEM_HOLDERS,
        "fire resistant to damage resistant",
        items::fire_resistant_to_damage_resistant_4064,
    ),
    fix(
        4067,
        ENTITY_HOLDERS,
        "boats split by wood",
        entity::boat_split_4067,
    ),
    fix(
        4068,
        ITEM_HOLDERS,
        "lock to predicate",
        items::lock_to_predicate_4068,
    ),
    fix(
        4175,
        ITEM_HOLDERS,
        "custom model data expansion",
        items::custom_model_data_expand_4175,
    ),
    fix(
        4175,
        ITEM_HOLDERS,
        "equippable asset rename",
        items::equippable_asset_rename_4175,
    ),
    fix(
        4176,
        ITEM_HOLDERS,
        "invalid lock removal",
        items::invalid_lock_4176,
    ),
    fix(4181, CHUNK, "furnace timers", chunk::furnace_burn_time_4181),
    fix(
        4290,
        ENTITY_HOLDERS,
        "text components as nbt",
        text::unflatten_text_components_4290,
    ),
    fix(
        4293,
        ENTITY_HOLDERS,
        "drop chances format",
        entity::drop_chances_format_4293,
    ),
    fix(
        4296,
        ENTITY_HOLDERS,
        "area effect cloud duration scale",
        entity::area_effect_cloud_duration_scale_4296,
    ),
    fix(
        4297,
        SAVED_DATA,
        "forced chunks to tickets",
        level::forced_chunks_to_tickets_4297,
    ),
    fix(
        4300,
        ENTITY_HOLDERS,
        "saddle equipment slot",
        entity::saddle_equipment_slot_4300,
    ),
    fix(
        4301,
        ENTITY_HOLDERS,
        "equipment format",
        entity::equipment_format_4301,
    ),
    fix(
        4303,
        ENTITY_HOLDERS,
        "fall distance as double",
        entity::fall_distance_4303,
    ),
    fix(
        4306,
        ENTITY_HOLDERS,
        "thrown potion split",
        entity::thrown_potion_split_4306,
    ),
    fix(
        4307,
        ITEM_HOLDERS,
        "tooltip display component",
        items::tooltip_display_4307,
    ),
    fix(
        4309,
        SAVED_DATA,
        "ticket chunk position unpacking",
        level::ticket_chunk_pos_4309,
    ),
    fix(
        4312,
        PLAYER,
        "player equipment",
        entity::player_equipment_4312,
    ),
    fix(
        4314,
        ENTITY_HOLDERS,
        "inline block position format",
        entity::inline_block_pos_4314,
    ),
    fix(
        4314,
        PLAYER,
        "player inline positions",
        entity::player_inline_positions_4314,
    ),
    fix(
        4535,
        ENTITY_HOLDERS,
        "copper golem weather state",
        entity::copper_golem_weather_state_4535,
    ),
    fix(4537, CHUNK, "delete light data", chunk::delete_light),
    fix(
        4541,
        CHUNK,
        "chain block to iron_chain",
        renames::chain_block_chunk_4541,
    ),
    fix(
        4541,
        ENTITY,
        "chain block state to iron_chain",
        renames::chain_block_entity_4541,
    ),
    fix(
        4541,
        ITEM_HOLDERS,
        "chain item to iron_chain",
        renames::chain_item_4541,
    ),
    fix(
        4544,
        LEVEL,
        "legacy world border",
        level::legacy_world_border_4544,
    ),
    fix(4548, LEVEL, "world spawn", level::world_spawn_4548),
    fix(4548, PLAYER, "player respawn", level::player_respawn_4548),
    fix(
        4657,
        LEVEL,
        "world border warning time",
        level::border_warning_time_4657,
    ),
    fix(
        4657,
        SAVED_DATA,
        "saved world border warning time",
        level::saved_border_warning_time_4657,
    ),
    fix(
        4658,
        LEVEL,
        "game rule registry",
        level::game_rule_registry_4658,
    ),
    fix(
        4763,
        ENTITY_HOLDERS,
        "villager data finalized",
        entity::villager_data_finalized_4763,
    ),
    fix(
        4766,
        LEVEL,
        "day time to clock",
        level::day_time_to_clock_4766,
    ),
    fix(
        4771,
        LEVEL,
        "difficulty settings",
        level::difficulty_settings_4771,
    ),
    fix(
        4771,
        LEVEL,
        "split level data",
        level::split_level_data_4771,
    ),
    fix(
        4888,
        ENTITY_HOLDERS,
        "nameplate attribute rename",
        entity::nameplate_attribute_4888,
    ),
    fix(
        4996,
        CHUNK,
        "pot decorations unflattening",
        chunk::pot_decorations_4996,
    ),
    fix(
        4996,
        ITEM_HOLDERS,
        "pot decorations component",
        items::pot_decorations_component_4996,
    ),
    fix(
        5006,
        ENTITY_HOLDERS,
        "block state field names",
        chunk::block_state_field_names_5006,
    ),
    fix(
        5007,
        ITEM_HOLDERS,
        "swing animation split",
        items::swing_animation_split_5007,
    ),
    fix(
        5008,
        ITEM_HOLDERS,
        "map color component removal",
        items::map_color_removal_5008,
    ),
    fix(
        5012,
        ITEM_HOLDERS,
        "explorer map item names",
        renames::explorer_map_items_5012,
    ),
    fix(
        5013,
        CHUNK,
        "merge terrain chunk statuses",
        chunk::merge_terrain_status_5013,
    ),
    fix(
        5014,
        ITEM_HOLDERS,
        "block transformer component removal",
        items::remove_block_transformer_5014,
    ),
    fix(
        5016,
        ITEM_HOLDERS,
        "unfilled buried treasure map name",
        items::unfilled_buried_treasure_map_5016,
    ),
];

pub fn upgrade(kind: DataKind, mut compound: Compound, from_version: i32) -> Compound {
    for fix in REGISTRY
        .iter()
        .filter(|fix| fix.kinds.contains(&kind) && fix.version > from_version)
    {
        tracing::trace!(version = fix.version, fix = fix.name, "applying data fixer");
        (fix.apply)(&mut compound);
    }
    compound
}

pub fn stored_version(kind: DataKind, root: &Compound) -> Option<i32> {
    match kind {
        DataKind::Level => root.get_compound("Data")?.get_i32("DataVersion"),
        _ => root.get_i32("DataVersion"),
    }
}

pub struct Upgraded {
    pub compound: Compound,
    pub from_version: i32,
}

impl Upgraded {
    pub fn changed(&self) -> bool {
        self.from_version < DATA_VERSION
    }
}

pub fn upgrade_checked(kind: DataKind, root: Compound) -> Result<Upgraded, AnvilError> {
    let from_version = stored_version(kind, &root).unwrap_or(OLDEST_SUPPORTED);
    if from_version > DATA_VERSION {
        return Err(AnvilError::NewerDataVersion {
            found: from_version,
            supported: DATA_VERSION,
        });
    }
    if from_version < OLDEST_SUPPORTED {
        return Err(AnvilError::TooOldDataVersion {
            found: from_version,
            oldest: OLDEST_SUPPORTED,
        });
    }
    let mut compound = upgrade(kind, root, from_version);
    if from_version < DATA_VERSION {
        match kind {
            DataKind::Level => {
                if let Some(data) = nbt::compound_mut(&mut compound, "Data") {
                    data.insert("DataVersion", DATA_VERSION);
                }
            }
            _ => {
                compound.insert("DataVersion", DATA_VERSION);
            }
        }
    }
    Ok(Upgraded {
        compound,
        from_version,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_registry_is_ordered_by_target_version() {
        assert!(
            REGISTRY
                .windows(2)
                .all(|pair| pair[0].version <= pair[1].version)
        );
        assert!(REGISTRY.iter().all(|fix| fix.version > OLDEST_SUPPORTED));
        assert!(REGISTRY.iter().all(|fix| fix.version <= DATA_VERSION));
    }

    #[test]
    fn newer_data_is_refused() {
        let root = Compound::new().with("DataVersion", DATA_VERSION + 1);
        assert!(matches!(
            upgrade_checked(DataKind::Chunk, root),
            Err(AnvilError::NewerDataVersion { .. })
        ));
    }

    #[test]
    fn data_older_than_the_oldest_supported_is_refused_with_a_clear_message() {
        let root = Compound::new().with("DataVersion", 2586);
        let error = upgrade_checked(DataKind::Chunk, root).err().unwrap();
        assert_eq!(
            error.to_string(),
            "world data version 2586 is older than the oldest supported version 2860; open it with a vanilla server first"
        );
    }

    #[test]
    fn a_missing_version_counts_as_the_oldest_supported() {
        let upgraded = upgrade_checked(DataKind::Chunk, Compound::new()).unwrap();
        assert_eq!(upgraded.from_version, OLDEST_SUPPORTED);
        assert!(upgraded.changed());
        assert_eq!(upgraded.compound.get_i32("DataVersion"), Some(DATA_VERSION));
    }

    #[test]
    fn current_data_is_returned_untouched() {
        let root = Compound::new()
            .with("DataVersion", DATA_VERSION)
            .with("a", 1);
        let upgraded = upgrade_checked(DataKind::Chunk, root.clone()).unwrap();
        assert!(!upgraded.changed());
        assert_eq!(upgraded.compound, root);
    }

    #[test]
    fn the_level_version_lives_in_the_data_compound() {
        let root = Compound::new().with("Data", Compound::new().with("DataVersion", 4000));
        assert_eq!(stored_version(DataKind::Level, &root), Some(4000));
        let upgraded = upgrade_checked(DataKind::Level, root).unwrap();
        assert_eq!(
            upgraded
                .compound
                .get_compound("Data")
                .unwrap()
                .get_i32("DataVersion"),
            Some(DATA_VERSION)
        );
    }
}

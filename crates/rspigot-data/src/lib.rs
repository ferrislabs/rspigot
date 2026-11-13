pub mod climate;
pub mod compostable;
pub mod entities;
#[rustfmt::skip]
mod generated;
pub mod loot;
pub mod registries;
pub mod spawns;
pub mod tags;
pub mod trading;
pub mod transformers;
pub mod variants;

pub const PROTOCOL_VERSION: i32 = 777;
pub const VERSION_NAME: &str = "26.3";
pub const WORLD_VERSION: i32 = 5023;
pub const KNOWN_PACK: KnownPack = KnownPack {
    namespace: "minecraft",
    id: "core",
    version: "26.3",
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KnownPack {
    pub namespace: &'static str,
    pub id: &'static str,
    pub version: &'static str,
}

#[cfg(test)]
mod tests {
    use super::*;

    const STONE_ITEM_ID: u16 = 1;
    const ITEM_COUNT: usize = 1658;
    const STONE_BLOCK_ID: u32 = 1;
    const PIG_ENTITY_ID: u16 = 101;

    #[test]
    fn version_constants() {
        assert_eq!(PROTOCOL_VERSION, 777);
        assert_eq!(VERSION_NAME, "26.3");
        assert_eq!(WORLD_VERSION, 5023);
        assert_eq!(KNOWN_PACK.namespace, "minecraft");
        assert_eq!(KNOWN_PACK.id, "core");
        assert_eq!(KNOWN_PACK.version, "26.3");
    }

    #[test]
    fn biomes() {
        let plains = registries::biome_id("minecraft:plains").unwrap();
        assert_eq!(registries::biome_name(plains), Some("minecraft:plains"));
        assert_eq!(registries::biome_id("minecraft:nope"), None);
        assert_eq!(
            registries::synced_index("minecraft:worldgen/biome", "minecraft:plains"),
            Some(u32::from(plains))
        );
    }

    #[test]
    fn dimension_types() {
        let entries = registries::synced_entries("minecraft:dimension_type").unwrap();
        assert_eq!(entries.len(), 4);
        for (index, name) in entries.iter().enumerate() {
            assert_eq!(
                registries::dimension_type_id(name),
                Some(u32::try_from(index).unwrap())
            );
        }
        assert_eq!(registries::dimension_type_id("minecraft:nope"), None);
    }

    #[test]
    fn synced_registries_follow_send_order() {
        let ids: Vec<&str> = registries::SYNCED.iter().map(|r| r.id).collect();
        assert_eq!(ids.len(), 32);
        assert_eq!(ids[0], "minecraft:worldgen/biome");
        assert_eq!(ids[1], "minecraft:chat_type");
        assert_eq!(ids[18], "minecraft:dimension_type");
        assert_eq!(ids[31], "minecraft:worldgen/block_state_provider");
        for registry in registries::SYNCED {
            assert!(registry.entries.windows(2).all(|w| w[0] < w[1]));
            assert_eq!(
                registries::synced_entries(registry.id),
                Some(registry.entries)
            );
            for (index, entry) in registry.entries.iter().enumerate() {
                assert_eq!(
                    registries::synced_index(registry.id, entry),
                    Some(u32::try_from(index).unwrap())
                );
            }
        }
        assert_eq!(registries::synced_entries("minecraft:block"), None);
        assert_eq!(
            registries::synced_index("minecraft:nope", "minecraft:x"),
            None
        );
    }

    #[test]
    fn static_registries_are_ordered_by_protocol_id() {
        assert_eq!(
            registries::static_index("minecraft:item", "minecraft:stone"),
            Some(u32::from(STONE_ITEM_ID))
        );
        assert_eq!(
            registries::static_index("minecraft:entity_type", "minecraft:pig"),
            Some(u32::from(PIG_ENTITY_ID))
        );
        assert_eq!(
            registries::static_index("minecraft:item", "minecraft:nope"),
            None
        );
        assert_eq!(
            registries::static_index("minecraft:nope", "minecraft:stone"),
            None
        );
        assert_eq!(registries::static_entries("minecraft:nope"), None);
        let items = registries::static_entries("minecraft:item").unwrap();
        assert_eq!(items.len(), ITEM_COUNT);
        assert_eq!(items[0], "minecraft:air");
        assert_eq!(items[1], "minecraft:stone");
        for (index, name) in items.iter().enumerate() {
            assert_eq!(
                registries::static_index("minecraft:item", name),
                Some(u32::try_from(index).unwrap())
            );
        }
    }

    #[test]
    fn entities_lookup() {
        assert_eq!(entities::id("minecraft:pig"), Some(PIG_ENTITY_ID));
        assert_eq!(entities::name(PIG_ENTITY_ID), Some("minecraft:pig"));
        assert_eq!(entities::id("minecraft:nope"), None);
        assert_eq!(entities::name(u16::MAX), None);
    }

    fn tag_ids(registry: &str, tag: &str) -> &'static [u32] {
        let registry = tags::TAGS.iter().find(|r| r.registry == registry).unwrap();
        registry
            .tags
            .iter()
            .find(|(name, _)| *name == tag)
            .map(|(_, ids)| *ids)
            .unwrap()
    }

    #[test]
    fn block_tags_contain_pickaxe_with_stone() {
        let ids = tag_ids("minecraft:block", "minecraft:mineable/pickaxe");
        assert!(ids.contains(&STONE_BLOCK_ID));
        assert!(ids.windows(2).all(|w| w[0] < w[1]));
    }

    #[test]
    fn tags_expand_nested_references() {
        let air = tag_ids("minecraft:block", "minecraft:air");
        let id = |name: &str| registries::static_index("minecraft:block", name).unwrap();
        assert_eq!(
            air,
            &[
                id("minecraft:air"),
                id("minecraft:void_air"),
                id("minecraft:cave_air")
            ]
        );
        let logs = tag_ids("minecraft:block", "minecraft:logs");
        assert!(logs.contains(&id("minecraft:oak_log")));
        assert!(logs.contains(&id("minecraft:crimson_stem")));
        assert!(!logs.contains(&id("minecraft:stone")));
    }

    #[test]
    fn tags_cover_static_and_synced_registries_only() {
        let registries: Vec<&str> = tags::TAGS.iter().map(|r| r.registry).collect();
        assert!(registries.contains(&"minecraft:block"));
        assert!(registries.contains(&"minecraft:item"));
        assert!(registries.contains(&"minecraft:worldgen/biome"));
        assert!(registries.contains(&"minecraft:damage_type"));
        assert!(!registries.contains(&"minecraft:villager_trade"));
        assert!(!registries.contains(&"minecraft:worldgen/structure"));
        assert!(registries.windows(2).all(|w| w[0] < w[1]));
        for registry in tags::TAGS {
            let size = registries::static_entries(registry.registry)
                .map(<[&str]>::len)
                .or_else(|| registries::synced_entries(registry.registry).map(<[&str]>::len))
                .unwrap();
            assert!(registry.tags.windows(2).all(|w| w[0].0 < w[1].0));
            for (_, ids) in registry.tags {
                assert!(ids.windows(2).all(|w| w[0] < w[1]));
                assert!(ids.iter().all(|id| (*id as usize) < size));
            }
        }
    }
}

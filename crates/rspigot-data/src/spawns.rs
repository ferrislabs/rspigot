use std::sync::OnceLock;

use crate::generated::spawns::{BIOME_SPAWNS, DIMENSION_SPAWNS};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpawnCount {
    Constant(i32),
    Uniform { min: i32, max: i32 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Spawner {
    pub entity: u16,
    pub weight: i32,
    pub count: SpawnCount,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SpawnCost {
    pub entity: u16,
    pub energy_budget: f64,
    pub charge: f64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BiomeSpawns {
    pub creature_probability: f32,
    pub monster: &'static [Spawner],
    pub creature: &'static [Spawner],
    pub ambient: &'static [Spawner],
    pub axolotls: &'static [Spawner],
    pub underground_water_creature: &'static [Spawner],
    pub water_creature: &'static [Spawner],
    pub water_ambient: &'static [Spawner],
    pub misc: &'static [Spawner],
    pub costs: &'static [SpawnCost],
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DimensionSpawns {
    pub monster_spawn_light: SpawnCount,
    pub monster_spawn_block_light_limit: i32,
    pub ambient_light: f32,
    pub has_ceiling: bool,
}

pub const EMPTY: BiomeSpawns = BiomeSpawns {
    creature_probability: 0.1,
    monster: &[],
    creature: &[],
    ambient: &[],
    axolotls: &[],
    underground_water_creature: &[],
    water_creature: &[],
    water_ambient: &[],
    misc: &[],
    costs: &[],
};

pub fn biome(id: u16) -> &'static BiomeSpawns {
    BIOME_SPAWNS.get(usize::from(id)).unwrap_or(&EMPTY)
}

impl BiomeSpawns {
    pub fn cost(&self, entity: u16) -> Option<&'static SpawnCost> {
        self.costs.iter().find(|cost| cost.entity == entity)
    }
}

pub fn has_spawn_cost(entity: u16) -> bool {
    static COSTED: OnceLock<Vec<u16>> = OnceLock::new();
    COSTED
        .get_or_init(|| {
            let mut entities: Vec<u16> = BIOME_SPAWNS
                .iter()
                .flat_map(|biome| biome.costs.iter().map(|cost| cost.entity))
                .collect();
            entities.sort_unstable();
            entities.dedup();
            entities
        })
        .binary_search(&entity)
        .is_ok()
}

pub fn dimension(name: &str) -> Option<&'static DimensionSpawns> {
    DIMENSION_SPAWNS
        .binary_search_by(|(candidate, _)| (*candidate).cmp(name))
        .ok()
        .map(|index| &DIMENSION_SPAWNS[index].1)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{entities, registries};

    #[test]
    fn plains_spawners_follow_the_datapack() {
        let plains = biome(registries::biome_id("minecraft:plains").unwrap());
        let sheep = entities::id("minecraft:sheep").unwrap();
        assert_eq!(
            plains.creature[0],
            Spawner {
                entity: sheep,
                weight: 12,
                count: SpawnCount::Constant(4)
            }
        );
        let horse = plains
            .creature
            .iter()
            .find(|s| s.entity == entities::id("minecraft:horse").unwrap());
        assert_eq!(
            horse.map(|s| s.count),
            Some(SpawnCount::Uniform { min: 2, max: 6 })
        );
        assert_eq!(plains.monster.len(), 9);
        assert_eq!(plains.creature_probability, 0.1);
        assert!(plains.costs.is_empty());
        let snowy = biome(registries::biome_id("minecraft:snowy_plains").unwrap());
        assert_eq!(snowy.creature_probability, 0.07);
    }

    #[test]
    fn soul_sand_valley_has_spawn_costs() {
        let valley = biome(registries::biome_id("minecraft:soul_sand_valley").unwrap());
        let skeleton = entities::id("minecraft:skeleton").unwrap();
        assert_eq!(
            valley.cost(skeleton),
            Some(&SpawnCost {
                entity: skeleton,
                energy_budget: 0.15,
                charge: 0.7
            })
        );
        assert!(has_spawn_cost(skeleton));
        assert!(!has_spawn_cost(entities::id("minecraft:cow").unwrap()));
    }

    #[test]
    fn overworld_monsters_need_darkness() {
        let overworld = dimension("minecraft:overworld").unwrap();
        assert_eq!(
            overworld.monster_spawn_light,
            SpawnCount::Uniform { min: 0, max: 7 }
        );
        assert_eq!(overworld.monster_spawn_block_light_limit, 0);
        let nether = dimension("minecraft:the_nether").unwrap();
        assert_eq!(nether.monster_spawn_light, SpawnCount::Constant(7));
        assert_eq!(nether.monster_spawn_block_light_limit, 15);
        assert!(dimension("minecraft:nope").is_none());
    }

    #[test]
    fn every_end_biome_spawns_endermen_and_the_end_dimension_allows_any_light() {
        let enderman = entities::id("minecraft:enderman").unwrap();
        for name in [
            "minecraft:the_end",
            "minecraft:end_highlands",
            "minecraft:end_midlands",
            "minecraft:small_end_islands",
            "minecraft:end_barrens",
        ] {
            let end = biome(registries::biome_id(name).unwrap());
            let found = end.monster.iter().find(|s| s.entity == enderman);
            assert_eq!(
                found.map(|s| (s.weight, s.count)),
                Some((10, SpawnCount::Constant(4))),
                "{name}"
            );
            assert_eq!(end.monster.len(), 1, "{name}");
            assert!(end.creature.is_empty(), "{name}");
        }
        let end = dimension("minecraft:the_end").expect("the_end dimension type");
        assert_eq!(end.monster_spawn_block_light_limit, 0);
        assert_eq!(end.monster_spawn_light, SpawnCount::Constant(15));
        assert!(!end.has_ceiling);
    }
}

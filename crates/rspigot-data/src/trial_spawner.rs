use crate::generated::trial_spawner::CONFIGS;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WeightedSpawn {
    pub entity: &'static str,
    pub equipment: Option<&'static str>,
    pub weight: i32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Config {
    pub name: &'static str,
    pub spawn_range: Option<i32>,
    pub total_mobs: Option<f32>,
    pub simultaneous_mobs: Option<f32>,
    pub total_mobs_added_per_player: Option<f32>,
    pub simultaneous_mobs_added_per_player: Option<f32>,
    pub ticks_between_spawn: Option<i32>,
    pub spawn_potentials: &'static [WeightedSpawn],
    pub loot_tables_to_eject: Option<&'static [(&'static str, i32)]>,
    pub items_to_drop_when_ominous: Option<&'static str>,
}

pub fn config(name: &str) -> Option<&'static Config> {
    let name = if name.contains(':') {
        name.to_owned()
    } else {
        format!("minecraft:{name}")
    };
    CONFIGS
        .binary_search_by(|config| config.name.cmp(name.as_str()))
        .ok()
        .map(|index| &CONFIGS[index])
}

pub fn names() -> impl Iterator<Item = &'static str> {
    CONFIGS.iter().map(|config| config.name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_ominous_breeze_config_is_loaded_with_its_overrides() {
        let config = config("trial_chamber/breeze/ominous").expect("config");
        assert_eq!(config.total_mobs, Some(4.0));
        assert_eq!(config.total_mobs_added_per_player, Some(1.0));
        assert_eq!(config.simultaneous_mobs_added_per_player, Some(0.5));
        assert_eq!(config.ticks_between_spawn, Some(20));
        assert_eq!(config.spawn_potentials.len(), 1);
        assert_eq!(
            config.spawn_potentials[0].entity,
            r#"{id:"minecraft:breeze"}"#
        );
        assert_eq!(
            config.loot_tables_to_eject,
            Some(
                &[
                    ("minecraft:spawners/ominous/trial_chamber/key", 3),
                    ("minecraft:spawners/ominous/trial_chamber/consumables", 7),
                ][..]
            )
        );
    }

    #[test]
    fn a_namespaced_key_finds_the_same_config() {
        assert_eq!(
            config("minecraft:trial_chamber/melee/zombie/normal"),
            config("trial_chamber/melee/zombie/normal")
        );
        assert!(config("trial_chamber/nothing").is_none());
    }

    #[test]
    fn the_slime_config_keeps_the_size_of_each_entry() {
        let config = config("trial_chamber/small_melee/slime/normal").expect("config");
        let sizes: Vec<&str> = config
            .spawn_potentials
            .iter()
            .map(|entry| entry.entity)
            .collect();
        assert_eq!(
            sizes,
            [
                r#"{Size:1,id:"minecraft:slime"}"#,
                r#"{Size:2,id:"minecraft:slime"}"#
            ]
        );
    }

    #[test]
    fn every_chamber_has_a_normal_and_an_ominous_config() {
        let normal = names().filter(|name| name.ends_with("/normal")).count();
        let ominous = names().filter(|name| name.ends_with("/ominous")).count();
        assert_eq!((normal, ominous), (14, 14));
    }
}

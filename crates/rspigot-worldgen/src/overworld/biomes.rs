use crate::resources::Biome;

pub(crate) fn surface_biome(name: &str) -> Biome {
    if let Some(biome) = Biome::from_name(name) {
        return biome;
    }
    let short = name.strip_prefix("minecraft:").unwrap_or(name);
    if short.ends_with("ocean") {
        return Biome::Ocean;
    }
    match short {
        "mangrove_swamp" | "swamp" | "jungle" | "sparse_jungle" | "bamboo_jungle"
        | "dark_forest" | "flower_forest" | "pale_garden" | "dappled_forest" | "cherry_grove" => {
            Biome::Forest
        }
        "old_growth_birch_forest" => Biome::BirchForest,
        "old_growth_pine_taiga" | "old_growth_spruce_taiga" => Biome::Taiga,
        "ice_spikes" | "grove" => Biome::SnowyPlains,
        "badlands" | "eroded_badlands" | "wooded_badlands" => Biome::Desert,
        "windswept_gravelly_hills" | "windswept_forest" | "windswept_savanna" => {
            Biome::WindsweptHills
        }
        "stony_shore" | "mushroom_fields" => Biome::Beach,
        _ => Biome::Plains,
    }
}

pub(crate) fn is_mountain(biome: Biome) -> bool {
    matches!(
        biome,
        Biome::WindsweptHills
            | Biome::SnowySlopes
            | Biome::JaggedPeaks
            | Biome::FrozenPeaks
            | Biome::StonyPeaks
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use rspigot_data::multi_noise::overworld_biomes;

    #[test]
    fn existing_biomes_keep_their_own_surface() {
        assert_eq!(surface_biome("minecraft:desert"), Biome::Desert);
        assert_eq!(surface_biome("minecraft:river"), Biome::River);
        assert_eq!(surface_biome("minecraft:stony_peaks"), Biome::StonyPeaks);
    }

    #[test]
    fn new_biomes_fall_back_on_a_close_surface() {
        assert_eq!(surface_biome("minecraft:jungle"), Biome::Forest);
        assert_eq!(surface_biome("minecraft:badlands"), Biome::Desert);
        assert_eq!(surface_biome("minecraft:savanna"), Biome::Plains);
        assert_eq!(surface_biome("minecraft:lush_caves"), Biome::Plains);
    }

    #[test]
    fn every_placed_biome_has_a_surface() {
        for name in overworld_biomes() {
            let _ = surface_biome(name);
        }
        assert!(is_mountain(Biome::JaggedPeaks));
        assert!(!is_mountain(Biome::Plains));
    }
}

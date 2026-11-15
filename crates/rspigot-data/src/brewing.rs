use crate::generated::brewing::MIXES;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Mix {
    pub input: &'static str,
    pub input_potion: &'static str,
    pub reagent: &'static str,
    pub output: &'static str,
    pub output_potion: &'static str,
}

pub fn all() -> &'static [Mix] {
    &MIXES
}

pub fn is_reagent(item: &str) -> bool {
    MIXES.iter().any(|mix| mix.reagent == item)
}

pub fn find(input: &str, potion: &str, reagent: &str) -> Option<&'static Mix> {
    MIXES
        .iter()
        .find(|mix| mix.input == input && mix.input_potion == potion && mix.reagent == reagent)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_datapack_has_its_279_mixes() {
        assert_eq!(all().len(), 279);
    }

    #[test]
    fn nether_wart_turns_water_into_awkward() {
        let mix = find(
            "minecraft:potion",
            "minecraft:water",
            "minecraft:nether_wart",
        )
        .unwrap();
        assert_eq!(mix.output_potion, "minecraft:awkward");
    }

    #[test]
    fn gunpowder_makes_a_potion_splash() {
        let mix = find(
            "minecraft:potion",
            "minecraft:strong_swiftness",
            "minecraft:gunpowder",
        )
        .unwrap();
        assert_eq!(mix.output, "minecraft:splash_potion");
        assert!(is_reagent("minecraft:gunpowder"));
        assert!(!is_reagent("minecraft:stick"));
    }
}

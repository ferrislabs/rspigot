use crate::random::RandomSource;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExperienceDrop {
    Uniform { min: i32, max: i32 },
    Spawner,
}

const SPAWNER_BASE: i32 = 15;
const SPAWNER_SPREAD: i32 = 15;

pub fn drop_of(block: &str) -> Option<ExperienceDrop> {
    let (min, max) = match block.trim_start_matches("minecraft:") {
        "coal_ore" | "deepslate_coal_ore" => (0, 2),
        "nether_gold_ore" => (0, 1),
        "diamond_ore" | "deepslate_diamond_ore" | "emerald_ore" | "deepslate_emerald_ore" => (3, 7),
        "lapis_ore" | "deepslate_lapis_ore" => (2, 5),
        "redstone_ore" | "deepslate_redstone_ore" => (1, 5),
        "nether_quartz_ore" => (2, 5),
        "sculk" => (1, 1),
        "sculk_sensor" | "calibrated_sculk_sensor" | "sculk_shrieker" | "sculk_catalyst" => (5, 5),
        "spawner" => return Some(ExperienceDrop::Spawner),
        _ => return None,
    };
    Some(ExperienceDrop::Uniform { min, max })
}

impl ExperienceDrop {
    pub fn sample(self, rng: &mut impl RandomSource) -> i32 {
        match self {
            Self::Uniform { min, max } => rng.next_int_between(min, max),
            Self::Spawner => {
                SPAWNER_BASE + rng.next_int(SPAWNER_SPREAD) + rng.next_int(SPAWNER_SPREAD)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::random::LegacyRandom;

    #[test]
    fn ores_drop_their_vanilla_ranges() {
        let mut rng = LegacyRandom::new(1);
        for _ in 0..200 {
            let diamond = drop_of("minecraft:deepslate_diamond_ore")
                .unwrap()
                .sample(&mut rng);
            assert!((3..=7).contains(&diamond));
            let coal = drop_of("minecraft:coal_ore").unwrap().sample(&mut rng);
            assert!((0..=2).contains(&coal));
            let spawner = drop_of("minecraft:spawner").unwrap().sample(&mut rng);
            assert!((15..=43).contains(&spawner));
        }
        assert!(drop_of("minecraft:stone").is_none());
    }
}

use rspigot_data::multi_noise::{ClimateVariant, overworld_graph, overworld_points};

use super::density::{ClimateFunctions, Scratch};
use super::rtree::Index;

pub(crate) type ClimateScratch = Scratch;

#[derive(Debug, Clone)]
pub(crate) struct BiomeSource {
    functions: ClimateFunctions,
    index: Index,
}

fn quantize(value: f32) -> i64 {
    (value * 10_000.0_f32) as i64
}

impl BiomeSource {
    pub(crate) fn new(seed: i64, variant: ClimateVariant) -> Self {
        Self {
            functions: ClimateFunctions::new(overworld_graph(variant), seed),
            index: Index::new(overworld_points()),
        }
    }

    pub(crate) fn scratch(&self) -> ClimateScratch {
        self.functions.scratch()
    }

    pub(crate) fn target(
        &self,
        scratch: &mut ClimateScratch,
        qx: i32,
        qy: i32,
        qz: i32,
    ) -> [i64; 6] {
        self.functions
            .sample(scratch, qx << 2, qy << 2, qz << 2)
            .map(quantize)
    }

    pub(crate) fn biome_index(
        &self,
        scratch: &mut ClimateScratch,
        qx: i32,
        qy: i32,
        qz: i32,
    ) -> u8 {
        let target = self.target(scratch, qx, qy, qz);
        self.index.find(&target)
    }

    pub(crate) fn biome_index_after(
        &self,
        scratch: &mut ClimateScratch,
        last: &mut Option<usize>,
        qx: i32,
        qy: i32,
        qz: i32,
    ) -> u8 {
        let target = self.target(scratch, qx, qy, qz);
        self.index.find_after(&target, last)
    }

    #[cfg(test)]
    pub(crate) fn biome(
        &self,
        scratch: &mut ClimateScratch,
        qx: i32,
        qy: i32,
        qz: i32,
    ) -> &'static str {
        rspigot_data::multi_noise::overworld_biomes()
            [usize::from(self.biome_index(scratch, qx, qy, qz))]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tree_search_agrees_with_brute_force() {
        let source = BiomeSource::new(11, ClimateVariant::Normal);
        let mut scratch = source.scratch();
        for i in -40..40 {
            let target = source.target(&mut scratch, i * 37, (i % 9) * 5 - 8, i * -53);
            let expected = source.index.find_brute_force(&target);
            assert_eq!(source.index.find(&target), expected, "{target:?}");
        }
    }
}

#[cfg(test)]
mod reference {
    use super::*;

    type Reference = (i64, i32, i32, i32, [i64; 6], &'static str);

    const VANILLA: [Reference; 51] = [
        (
            1,
            -300,
            -16,
            -211,
            [-3296, -674, 2407, 2558, 10291, -4833],
            "forest",
        ),
        (
            1,
            -17,
            0,
            3,
            [-391, -3231, -4950, -2592, 2861, 2019],
            "deep_ocean",
        ),
        (
            1,
            0,
            16,
            90,
            [1285, 246, -5053, -561, -2246, -1689],
            "deep_ocean",
        ),
        (
            1,
            5,
            60,
            -1000,
            [-2604, 1981, -890, -1184, -13763, -2259],
            "taiga",
        ),
        (
            1,
            64,
            -16,
            77,
            [428, 244, 316, -1205, 10187, 3588],
            "forest",
        ),
        (
            1,
            250,
            0,
            -211,
            [-43, -612, -5822, 532, 2740, 1638],
            "deep_ocean",
        ),
        (
            1,
            1000,
            16,
            3,
            [-4074, -193, 4990, -1487, 4622, -6643],
            "dripstone_caves",
        ),
        (
            -7,
            -300,
            -16,
            -211,
            [-3868, 496, 1699, -1391, 10890, 3763],
            "forest",
        ),
        (
            -7,
            -17,
            0,
            3,
            [-4594, 1208, -6848, 2400, 2740, -8228],
            "deep_frozen_ocean",
        ),
        (
            -7,
            0,
            16,
            90,
            [-4719, -2138, -7235, 6401, -2259, 4146],
            "deep_frozen_ocean",
        ),
        (
            -7,
            5,
            60,
            -1000,
            [778, -4482, -6068, -2633, -16009, 2697],
            "deep_ocean",
        ),
        (
            -7,
            64,
            -16,
            77,
            [-4627, -2326, -4935, 6044, 7883, 3950],
            "deep_frozen_ocean",
        ),
        (
            -7,
            250,
            0,
            -211,
            [-5784, -2892, 2667, 2753, 5033, -1514],
            "snowy_plains",
        ),
        (
            -7,
            1000,
            16,
            3,
            [10057, 1854, -5299, -891, -2259, 7845],
            "warm_ocean",
        ),
        (
            123456789012345,
            -300,
            -16,
            -211,
            [-3774, 1582, 93, -945, 9993, -2103],
            "taiga",
        ),
        (
            123456789012345,
            -17,
            0,
            3,
            [-5734, 946, -2205, -2973, 3762, 4757],
            "frozen_ocean",
        ),
        (
            123456789012345,
            0,
            16,
            90,
            [-5456, 2251, -3921, -5300, -1237, 3262],
            "frozen_ocean",
        ),
        (
            123456789012345,
            5,
            60,
            -1000,
            [7891, -717, -540, -3681, -13306, 2269],
            "desert",
        ),
        (
            123456789012345,
            64,
            -16,
            77,
            [-4922, 205, -4480, -3011, 8725, 5623],
            "frozen_ocean",
        ),
        (
            123456789012345,
            250,
            0,
            -211,
            [-185, 2282, 2739, 5252, 4381, 951],
            "old_growth_birch_forest",
        ),
        (
            123456789012345,
            1000,
            16,
            3,
            [708, -330, -2579, -8633, -1237, 2465],
            "ocean",
        ),
        (
            1,
            -513,
            -14,
            -1908,
            [1125, 310, 8606, 126, 9543, -3186],
            "forest",
        ),
        (
            1,
            1424,
            -11,
            -1494,
            [-2415, -790, -1794, -4718, 7203, -1471],
            "stony_shore",
        ),
        (
            1,
            -146,
            10,
            2691,
            [5136, 4693, 2934, 3034, 1937, 2094],
            "bamboo_jungle",
        ),
        (
            1,
            -2679,
            71,
            1831,
            [-3536, 454, -6837, -1233, -19447, 4506],
            "deep_cold_ocean",
        ),
        (
            1,
            747,
            52,
            2560,
            [5301, 1599, -3807, 654, -12487, 3319],
            "lukewarm_ocean",
        ),
        (
            1,
            2090,
            -9,
            3978,
            [-4930, 430, -694, 2612, 7586, -790],
            "snowy_plains",
        ),
        (
            1,
            2017,
            20,
            17,
            [2432, 2097, 3887, -2891, 1823, -2299],
            "jungle",
        ),
        (
            1,
            -835,
            76,
            -3192,
            [-758, 296, -3292, -1169, -19987, 3849],
            "ocean",
        ),
        (
            1,
            -1239,
            76,
            1402,
            [4330, 133, 5909, -1140, -17559, -3816],
            "forest",
        ),
        (
            1,
            2647,
            16,
            3715,
            [-8004, 3104, 5560, -2249, 2057, -2430],
            "taiga",
        ),
        (
            1,
            -1311,
            -3,
            1292,
            [2631, 3316, 1891, -743, 9796, 6070],
            "bamboo_jungle",
        ),
        (
            1,
            -2871,
            22,
            -2293,
            [-8836, -2207, 1350, -4026, -885, 925],
            "snowy_slopes",
        ),
        (
            1,
            -3054,
            -15,
            543,
            [-4902, -2847, 1919, -22, 8688, 26],
            "frozen_river",
        ),
        (
            1,
            -367,
            -5,
            -1116,
            [-811, -3087, -3482, -271, 5324, -4063],
            "ocean",
        ),
        (
            1,
            -2130,
            5,
            3357,
            [3174, 0, -1026, -453, 3425, -2442],
            "forest",
        ),
        (
            1,
            198,
            18,
            452,
            [1592, 2166, 3689, 3633, -562, -2077],
            "birch_forest",
        ),
        (
            1,
            -3947,
            -3,
            3595,
            [1395, -5681, -2114, -2138, 4699, -4191],
            "ocean",
        ),
        (
            1,
            3805,
            61,
            712,
            [1878, -648, -583, 2194, -14477, 532],
            "forest",
        ),
        (
            1,
            3503,
            19,
            1912,
            [-111, 3485, -3163, 2458, -2175, -4042],
            "ocean",
        ),
        (
            1,
            900,
            55,
            1947,
            [2219, 115, -597, -1758, -12070, -2819],
            "forest",
        ),
        (
            1,
            -3901,
            35,
            -2749,
            [2090, -2716, 424, -5645, -2034, 5160],
            "savanna_plateau",
        ),
        (
            1,
            1326,
            53,
            -3702,
            [-444, 3851, -636, -3141, -11774, -1216],
            "dark_forest",
        ),
        (
            1,
            3733,
            40,
            1697,
            [87, -760, -1831, -5590, -8737, -5668],
            "forest",
        ),
        (
            1,
            518,
            -12,
            -951,
            [3529, 1072, 2690, -799, 12880, 6328],
            "sparse_jungle",
        ),
        (
            1,
            -1342,
            51,
            -1970,
            [2254, 53, 5435, 1941, -11379, -342],
            "river",
        ),
        (
            1,
            3057,
            24,
            2769,
            [-1840, 3002, -2328, 1468, -3737, 3054],
            "cold_ocean",
        ),
        (
            1,
            3413,
            9,
            -2601,
            [-503, 1811, 6455, -79, 2223, 1909],
            "dripstone_caves",
        ),
        (
            1,
            -2927,
            15,
            -2316,
            [-8966, -3253, 959, -6724, 7251, 6046],
            "frozen_peaks",
        ),
        (
            1,
            3156,
            20,
            2091,
            [3486, -1269, 2608, -1786, -1451, -801],
            "savanna",
        ),
        (
            1,
            -1494,
            33,
            891,
            [134, 891, -1501, -1381, -4743, -6105],
            "forest",
        ),
    ];

    #[test]
    fn climate_and_biomes_match_the_decompiled_vanilla() {
        let mut sources: Vec<(i64, BiomeSource)> = Vec::new();
        for (seed, qx, qy, qz, expected, biome) in VANILLA {
            if !sources.iter().any(|(s, _)| *s == seed) {
                sources.push((seed, BiomeSource::new(seed, ClimateVariant::Normal)));
            }
            let source = &sources.iter().find(|(s, _)| *s == seed).unwrap().1;
            let mut scratch = source.scratch();
            assert_eq!(
                source.target(&mut scratch, qx, qy, qz),
                expected,
                "{seed} {qx} {qy} {qz}"
            );
            let name = source.biome(&mut scratch, qx, qy, qz);
            assert_eq!(
                name.trim_start_matches("minecraft:"),
                biome,
                "{seed} {qx} {qy} {qz}"
            );
        }
    }
}

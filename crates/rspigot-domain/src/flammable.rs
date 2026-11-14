use std::sync::OnceLock;

use rspigot_blocks::BlockKind;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Flammability {
    pub ignite: u8,
    pub burn: u8,
}

const GROUPS: &[(u8, u8, &[&str])] = &[
    (
        5,
        5,
        &[
            "minecraft:oak_log",
            "minecraft:spruce_log",
            "minecraft:birch_log",
            "minecraft:jungle_log",
            "minecraft:acacia_log",
            "minecraft:cherry_log",
            "minecraft:pale_oak_log",
            "minecraft:poplar_log",
            "minecraft:dark_oak_log",
            "minecraft:mangrove_log",
            "minecraft:bamboo_block",
            "minecraft:stripped_oak_log",
            "minecraft:stripped_spruce_log",
            "minecraft:stripped_birch_log",
            "minecraft:stripped_jungle_log",
            "minecraft:stripped_acacia_log",
            "minecraft:stripped_cherry_log",
            "minecraft:stripped_dark_oak_log",
            "minecraft:stripped_pale_oak_log",
            "minecraft:stripped_poplar_log",
            "minecraft:stripped_mangrove_log",
            "minecraft:stripped_bamboo_block",
            "minecraft:stripped_oak_wood",
            "minecraft:stripped_spruce_wood",
            "minecraft:stripped_birch_wood",
            "minecraft:stripped_jungle_wood",
            "minecraft:stripped_acacia_wood",
            "minecraft:stripped_cherry_wood",
            "minecraft:stripped_dark_oak_wood",
            "minecraft:stripped_pale_oak_wood",
            "minecraft:stripped_poplar_wood",
            "minecraft:stripped_mangrove_wood",
            "minecraft:oak_wood",
            "minecraft:spruce_wood",
            "minecraft:birch_wood",
            "minecraft:jungle_wood",
            "minecraft:acacia_wood",
            "minecraft:cherry_wood",
            "minecraft:pale_oak_wood",
            "minecraft:poplar_wood",
            "minecraft:dark_oak_wood",
            "minecraft:mangrove_wood",
            "minecraft:coal_block",
        ],
    ),
    (
        5,
        20,
        &[
            "minecraft:oak_planks",
            "minecraft:spruce_planks",
            "minecraft:birch_planks",
            "minecraft:jungle_planks",
            "minecraft:acacia_planks",
            "minecraft:cherry_planks",
            "minecraft:dark_oak_planks",
            "minecraft:pale_oak_planks",
            "minecraft:poplar_planks",
            "minecraft:mangrove_planks",
            "minecraft:bamboo_planks",
            "minecraft:bamboo_mosaic",
            "minecraft:oak_slab",
            "minecraft:spruce_slab",
            "minecraft:birch_slab",
            "minecraft:jungle_slab",
            "minecraft:acacia_slab",
            "minecraft:cherry_slab",
            "minecraft:dark_oak_slab",
            "minecraft:pale_oak_slab",
            "minecraft:poplar_slab",
            "minecraft:mangrove_slab",
            "minecraft:bamboo_slab",
            "minecraft:bamboo_mosaic_slab",
            "minecraft:oak_fence_gate",
            "minecraft:spruce_fence_gate",
            "minecraft:birch_fence_gate",
            "minecraft:jungle_fence_gate",
            "minecraft:acacia_fence_gate",
            "minecraft:cherry_fence_gate",
            "minecraft:dark_oak_fence_gate",
            "minecraft:pale_oak_fence_gate",
            "minecraft:poplar_fence_gate",
            "minecraft:mangrove_fence_gate",
            "minecraft:bamboo_fence_gate",
            "minecraft:oak_fence",
            "minecraft:spruce_fence",
            "minecraft:birch_fence",
            "minecraft:jungle_fence",
            "minecraft:acacia_fence",
            "minecraft:cherry_fence",
            "minecraft:dark_oak_fence",
            "minecraft:pale_oak_fence",
            "minecraft:poplar_fence",
            "minecraft:mangrove_fence",
            "minecraft:bamboo_fence",
            "minecraft:oak_stairs",
            "minecraft:birch_stairs",
            "minecraft:spruce_stairs",
            "minecraft:jungle_stairs",
            "minecraft:acacia_stairs",
            "minecraft:cherry_stairs",
            "minecraft:dark_oak_stairs",
            "minecraft:pale_oak_stairs",
            "minecraft:poplar_stairs",
            "minecraft:mangrove_stairs",
            "minecraft:bamboo_stairs",
            "minecraft:bamboo_mosaic_stairs",
            "minecraft:mangrove_roots",
            "minecraft:composter",
            "minecraft:beehive",
        ],
    ),
    (
        5,
        100,
        &[
            "minecraft:pale_moss_block",
            "minecraft:pale_moss_carpet",
            "minecraft:pale_hanging_moss",
        ],
    ),
    (15, 20, &["minecraft:target"]),
    (
        15,
        60,
        &["minecraft:cave_vines", "minecraft:cave_vines_plant"],
    ),
    (
        15,
        100,
        &["minecraft:tnt", "minecraft:vine", "minecraft:glow_lichen"],
    ),
    (
        30,
        20,
        &[
            "minecraft:bookshelf",
            "minecraft:lectern",
            "minecraft:bee_nest",
            "minecraft:acacia_shelf",
            "minecraft:bamboo_shelf",
            "minecraft:birch_shelf",
            "minecraft:cherry_shelf",
            "minecraft:dark_oak_shelf",
            "minecraft:jungle_shelf",
            "minecraft:mangrove_shelf",
            "minecraft:poplar_shelf",
            "minecraft:oak_shelf",
            "minecraft:pale_oak_shelf",
            "minecraft:spruce_shelf",
        ],
    ),
    (
        30,
        60,
        &[
            "minecraft:oak_leaves",
            "minecraft:spruce_leaves",
            "minecraft:birch_leaves",
            "minecraft:jungle_leaves",
            "minecraft:acacia_leaves",
            "minecraft:cherry_leaves",
            "minecraft:dark_oak_leaves",
            "minecraft:pale_oak_leaves",
            "minecraft:mangrove_leaves",
            "minecraft:red_poplar_leaves",
            "minecraft:orange_poplar_leaves",
            "minecraft:yellow_poplar_leaves",
            "minecraft:dried_kelp_block",
            "minecraft:azalea_leaves",
            "minecraft:flowering_azalea_leaves",
            "minecraft:azalea",
            "minecraft:flowering_azalea",
            "minecraft:hanging_roots",
            "minecraft:straw_bed",
        ],
    ),
    (60, 20, &["minecraft:hay_block"]),
    (60, 60, &["minecraft:bamboo", "minecraft:scaffolding"]),
    (
        60,
        100,
        &[
            "minecraft:short_grass",
            "minecraft:fern",
            "minecraft:dead_bush",
            "minecraft:short_dry_grass",
            "minecraft:tall_dry_grass",
            "minecraft:sunflower",
            "minecraft:lilac",
            "minecraft:rose_bush",
            "minecraft:peony",
            "minecraft:red_shrub",
            "minecraft:tall_grass",
            "minecraft:large_fern",
            "minecraft:dandelion",
            "minecraft:golden_dandelion",
            "minecraft:poppy",
            "minecraft:open_eyeblossom",
            "minecraft:closed_eyeblossom",
            "minecraft:blue_orchid",
            "minecraft:allium",
            "minecraft:azure_bluet",
            "minecraft:red_tulip",
            "minecraft:orange_tulip",
            "minecraft:white_tulip",
            "minecraft:pink_tulip",
            "minecraft:oxeye_daisy",
            "minecraft:cornflower",
            "minecraft:lily_of_the_valley",
            "minecraft:torchflower",
            "minecraft:pitcher_plant",
            "minecraft:wither_rose",
            "minecraft:pink_petals",
            "minecraft:wildflowers",
            "minecraft:leaf_litter",
            "minecraft:cactus_flower",
            "minecraft:sweet_berry_bush",
            "minecraft:spore_blossom",
            "minecraft:big_dripleaf",
            "minecraft:big_dripleaf_stem",
            "minecraft:small_dripleaf",
            "minecraft:firefly_bush",
            "minecraft:bush",
        ],
    ),
];

const TAGGED: &[(u8, u8, &str)] = &[
    (30, 60, "minecraft:wool"),
    (30, 60, "minecraft:wool_stairs"),
    (30, 60, "minecraft:wool_slabs"),
    (60, 20, "minecraft:wool_carpets"),
];

fn table() -> &'static [Flammability] {
    static TABLE: OnceLock<Vec<Flammability>> = OnceLock::new();
    TABLE.get_or_init(|| {
        let mut table = vec![Flammability::default(); 4096];
        let mut put = |id: usize, ignite: u8, burn: u8| {
            if let Some(slot) = table.get_mut(id) {
                *slot = Flammability { ignite, burn };
            }
        };
        for (ignite, burn, names) in GROUPS {
            for name in *names {
                if let Some(kind) = BlockKind::from_name(name) {
                    put(usize::from(kind.id().0), *ignite, *burn);
                }
            }
        }
        for (ignite, burn, tag) in TAGGED {
            for id in rspigot_data::tags::tag("minecraft:block", tag).unwrap_or(&[]) {
                put(*id as usize, *ignite, *burn);
            }
        }
        table
    })
}

pub fn flammability(kind: BlockKind) -> Flammability {
    table()
        .get(usize::from(kind.id().0))
        .copied()
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_listed_block_exists() {
        for (_, _, names) in GROUPS {
            for name in *names {
                assert!(BlockKind::from_name(name).is_some(), "{name}");
            }
        }
    }

    #[test]
    fn wood_burns_slowly_and_leaves_catch_fast() {
        assert_eq!(
            flammability(BlockKind::OakPlanks),
            Flammability {
                ignite: 5,
                burn: 20
            }
        );
        assert_eq!(
            flammability(BlockKind::OakLeaves),
            Flammability {
                ignite: 30,
                burn: 60
            }
        );
        assert_eq!(
            flammability(BlockKind::WhiteWool),
            Flammability {
                ignite: 30,
                burn: 60
            }
        );
        assert_eq!(flammability(BlockKind::Stone), Flammability::default());
    }
}

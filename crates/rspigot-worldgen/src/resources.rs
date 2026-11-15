use std::ops::Index;

use rspigot_data::registries;
use rspigot_domain::{BiomeId, BlockStateId};

use crate::classify::Classifier;

macro_rules! block_palette {
    ($($variant:ident = $name:literal $([$($key:literal = $value:literal),*])?;)*) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        pub(crate) enum BlockKind {
            $($variant),*
        }

        impl BlockKind {
            pub(crate) const COUNT: usize = [$(Self::$variant),*].len();
        }

        type BlockSpec = (&'static str, &'static [(&'static str, &'static str)]);

        const BLOCK_SPECS: [BlockSpec; BlockKind::COUNT] = [
            $(($name, &[$($(($key, $value)),*)?])),*
        ];
    };
}

block_palette! {
    Air = "minecraft:air";
    Stone = "minecraft:stone";
    Deepslate = "minecraft:deepslate";
    Bedrock = "minecraft:bedrock";
    Dirt = "minecraft:dirt";
    GrassBlock = "minecraft:grass_block";
    SnowyGrassBlock = "minecraft:grass_block" ["snowy" = "true"];
    Sand = "minecraft:sand";
    Sandstone = "minecraft:sandstone";
    Gravel = "minecraft:gravel";
    Clay = "minecraft:clay";
    Water = "minecraft:water";
    Lava = "minecraft:lava";
    SnowBlock = "minecraft:snow_block";
    SnowLayer = "minecraft:snow";
    Ice = "minecraft:ice";
    Netherrack = "minecraft:netherrack";
    EndStone = "minecraft:end_stone";
    MagmaBlock = "minecraft:magma_block";
    Granite = "minecraft:granite";
    Diorite = "minecraft:diorite";
    Andesite = "minecraft:andesite";
    Tuff = "minecraft:tuff";
    CoalOre = "minecraft:coal_ore";
    DeepslateCoalOre = "minecraft:deepslate_coal_ore";
    IronOre = "minecraft:iron_ore";
    DeepslateIronOre = "minecraft:deepslate_iron_ore";
    CopperOre = "minecraft:copper_ore";
    DeepslateCopperOre = "minecraft:deepslate_copper_ore";
    GoldOre = "minecraft:gold_ore";
    DeepslateGoldOre = "minecraft:deepslate_gold_ore";
    RedstoneOre = "minecraft:redstone_ore";
    DeepslateRedstoneOre = "minecraft:deepslate_redstone_ore";
    LapisOre = "minecraft:lapis_ore";
    DeepslateLapisOre = "minecraft:deepslate_lapis_ore";
    DiamondOre = "minecraft:diamond_ore";
    DeepslateDiamondOre = "minecraft:deepslate_diamond_ore";
    EmeraldOre = "minecraft:emerald_ore";
    DeepslateEmeraldOre = "minecraft:deepslate_emerald_ore";
    NetherQuartzOre = "minecraft:nether_quartz_ore";
    NetherGoldOre = "minecraft:nether_gold_ore";
    ShortGrass = "minecraft:short_grass";
    Fern = "minecraft:fern";
    TallGrassLower = "minecraft:tall_grass" ["half" = "lower"];
    TallGrassUpper = "minecraft:tall_grass" ["half" = "upper"];
    Dandelion = "minecraft:dandelion";
    Poppy = "minecraft:poppy";
    AzureBluet = "minecraft:azure_bluet";
    OxeyeDaisy = "minecraft:oxeye_daisy";
    Cornflower = "minecraft:cornflower";
    DeadBush = "minecraft:dead_bush";
    OakLog = "minecraft:oak_log";
    BirchLog = "minecraft:birch_log";
    SpruceLog = "minecraft:spruce_log";
}

macro_rules! biome_table {
    ($($variant:ident = $name:literal;)*) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        pub(crate) enum Biome {
            $($variant),*
        }

        impl Biome {
            pub(crate) const COUNT: usize = [$(Self::$variant),*].len();

            pub(crate) fn from_name(name: &str) -> Option<Self> {
                match name {
                    $($name => Some(Self::$variant),)*
                    _ => None,
                }
            }
        }

        const BIOME_NAMES: [&str; Biome::COUNT] = [$($name),*];
    };
}

biome_table! {
    Plains = "minecraft:plains";
    Forest = "minecraft:forest";
    BirchForest = "minecraft:birch_forest";
    Taiga = "minecraft:taiga";
    SnowyTaiga = "minecraft:snowy_taiga";
    SnowyPlains = "minecraft:snowy_plains";
    Desert = "minecraft:desert";
    Beach = "minecraft:beach";
    SnowyBeach = "minecraft:snowy_beach";
    River = "minecraft:river";
    FrozenRiver = "minecraft:frozen_river";
    Ocean = "minecraft:ocean";
    DeepOcean = "minecraft:deep_ocean";
    ColdOcean = "minecraft:cold_ocean";
    DeepColdOcean = "minecraft:deep_cold_ocean";
    FrozenOcean = "minecraft:frozen_ocean";
    DeepFrozenOcean = "minecraft:deep_frozen_ocean";
    LukewarmOcean = "minecraft:lukewarm_ocean";
    DeepLukewarmOcean = "minecraft:deep_lukewarm_ocean";
    WarmOcean = "minecraft:warm_ocean";
    WindsweptHills = "minecraft:windswept_hills";
    SnowySlopes = "minecraft:snowy_slopes";
    JaggedPeaks = "minecraft:jagged_peaks";
    FrozenPeaks = "minecraft:frozen_peaks";
    StonyPeaks = "minecraft:stony_peaks";
    TheVoid = "minecraft:the_void";
    NetherWastes = "minecraft:nether_wastes";
    TheEnd = "minecraft:the_end";
    EndHighlands = "minecraft:end_highlands";
    EndMidlands = "minecraft:end_midlands";
    EndBarrens = "minecraft:end_barrens";
    SmallEndIslands = "minecraft:small_end_islands";
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Tree {
    Oak,
    Birch,
    Spruce,
}

impl Tree {
    const ALL: [Self; 3] = [Self::Oak, Self::Birch, Self::Spruce];

    const fn leaf_name(self) -> &'static str {
        match self {
            Self::Oak => "minecraft:oak_leaves",
            Self::Birch => "minecraft:birch_leaves",
            Self::Spruce => "minecraft:spruce_leaves",
        }
    }
}

const DISTANCES: [&str; 7] = ["1", "2", "3", "4", "5", "6", "7"];

#[derive(Debug, Clone)]
pub(crate) struct Palette {
    ids: [BlockStateId; BlockKind::COUNT],
    leaves: [[BlockStateId; 7]; 3],
}

impl Palette {
    pub(crate) fn resolve() -> Self {
        Self {
            ids: std::array::from_fn(|i| {
                let (name, properties) = BLOCK_SPECS[i];
                state(name, properties)
            }),
            leaves: std::array::from_fn(|tree| {
                std::array::from_fn(|distance| {
                    state(
                        Tree::ALL[tree].leaf_name(),
                        &[("distance", DISTANCES[distance]), ("persistent", "false")],
                    )
                })
            }),
        }
    }

    pub(crate) fn leaves(&self, tree: Tree, distance: i32) -> BlockStateId {
        self.leaves[tree as usize][(distance.clamp(1, 7) - 1) as usize]
    }

    pub(crate) fn log(&self, tree: Tree) -> BlockStateId {
        match tree {
            Tree::Oak => self[BlockKind::OakLog],
            Tree::Birch => self[BlockKind::BirchLog],
            Tree::Spruce => self[BlockKind::SpruceLog],
        }
    }

    #[cfg(test)]
    pub(crate) fn unresolved(&self) -> Vec<usize> {
        let mut missing: Vec<usize> = (1..BlockKind::COUNT)
            .filter(|i| self.ids[*i].is_air())
            .collect();
        missing.extend(
            self.leaves
                .iter()
                .flatten()
                .filter(|s| s.is_air())
                .map(|_| usize::MAX),
        );
        missing
    }
}

impl Index<BlockKind> for Palette {
    type Output = BlockStateId;

    fn index(&self, block: BlockKind) -> &BlockStateId {
        &self.ids[block as usize]
    }
}

fn state(name: &str, properties: &[(&str, &str)]) -> BlockStateId {
    rspigot_blocks::BlockKind::from_name(name)
        .and_then(|kind| kind.state_with(properties))
        .unwrap_or(BlockStateId::AIR)
}

#[derive(Debug, Clone)]
pub(crate) struct BiomeTable {
    ids: [BiomeId; Biome::COUNT],
}

impl BiomeTable {
    pub(crate) fn resolve() -> Self {
        Self {
            ids: std::array::from_fn(|i| biome_id(BIOME_NAMES[i])),
        }
    }

    #[cfg(test)]
    pub(crate) fn unresolved() -> Vec<&'static str> {
        BIOME_NAMES
            .iter()
            .filter(|name| registries::biome_id(name).is_none())
            .copied()
            .collect()
    }
}

impl Index<Biome> for BiomeTable {
    type Output = BiomeId;

    fn index(&self, biome: Biome) -> &BiomeId {
        &self.ids[biome as usize]
    }
}

pub(crate) fn biome_id(name: &str) -> BiomeId {
    BiomeId(registries::biome_id(name).unwrap_or(0))
}

#[derive(Debug, Clone)]
pub(crate) struct Resources {
    pub(crate) blocks: Palette,
    pub(crate) biomes: BiomeTable,
    pub(crate) classifier: Classifier,
}

impl Resources {
    pub(crate) fn resolve() -> Self {
        Self {
            blocks: Palette::resolve(),
            biomes: BiomeTable::resolve(),
            classifier: Classifier::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_block_name_resolves() {
        let palette = Palette::resolve();
        assert_eq!(palette.unresolved(), Vec::<usize>::new());
        assert!(palette[BlockKind::Air].is_air());
    }

    #[test]
    fn every_biome_name_resolves() {
        assert_eq!(BiomeTable::unresolved(), Vec::<&str>::new());
    }

    #[test]
    fn property_variants_differ_from_defaults() {
        let palette = Palette::resolve();
        assert_ne!(
            palette[BlockKind::GrassBlock],
            palette[BlockKind::SnowyGrassBlock]
        );
        assert_ne!(
            palette[BlockKind::TallGrassLower],
            palette[BlockKind::TallGrassUpper]
        );
        assert_ne!(palette.leaves(Tree::Oak, 1), palette.leaves(Tree::Oak, 2));
        assert_ne!(palette.leaves(Tree::Oak, 1), palette.leaves(Tree::Birch, 1));
        assert_eq!(palette.leaves(Tree::Oak, 99), palette.leaves(Tree::Oak, 7));
    }

    #[test]
    fn blocks_have_expected_names() {
        let palette = Palette::resolve();
        assert_eq!(
            palette[BlockKind::Stone]
                .kind()
                .map(rspigot_blocks::BlockKind::name),
            Some("minecraft:stone")
        );
        assert_eq!(
            palette[BlockKind::SnowyGrassBlock].property("snowy"),
            Some("true")
        );
        assert_eq!(
            palette.leaves(Tree::Spruce, 4).property("distance"),
            Some("4")
        );
        assert_eq!(
            palette.leaves(Tree::Spruce, 4).property("persistent"),
            Some("false")
        );
        assert_eq!(
            palette
                .log(Tree::Birch)
                .kind()
                .map(rspigot_blocks::BlockKind::name),
            Some("minecraft:birch_log")
        );
    }
}

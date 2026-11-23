use rspigot_blocks::BlockKind;

use crate::plugins::events::TreeType;

#[derive(Debug, Clone, Copy)]
pub(crate) struct Entry(pub(crate) &'static str, pub(crate) u32);

#[derive(Debug)]
pub(crate) struct Grower {
    pub(crate) trees: &'static [Entry],
    pub(crate) megas: &'static [Entry],
    pub(crate) flowers: &'static [Entry],
}

const OAK: Grower = Grower {
    trees: &[Entry("oak", 9), Entry("fancy_oak", 1)],
    megas: &[],
    flowers: &[Entry("oak_bees_005", 9), Entry("fancy_oak_bees_005", 1)],
};
const SPRUCE: Grower = Grower {
    trees: &[Entry("spruce", 1)],
    megas: &[Entry("mega_spruce", 1), Entry("mega_pine", 1)],
    flowers: &[],
};
const MANGROVE: Grower = Grower {
    trees: &[Entry("mangrove", 15), Entry("tall_mangrove", 85)],
    megas: &[],
    flowers: &[],
};
const AZALEA: Grower = Grower {
    trees: &[Entry("azalea_tree", 1)],
    megas: &[],
    flowers: &[],
};
const BIRCH: Grower = Grower {
    trees: &[Entry("birch", 1)],
    megas: &[],
    flowers: &[Entry("birch_bees_005", 1)],
};
const JUNGLE: Grower = Grower {
    trees: &[Entry("jungle_tree_no_vine", 1)],
    megas: &[Entry("mega_jungle_tree", 1)],
    flowers: &[],
};
const ACACIA: Grower = Grower {
    trees: &[Entry("acacia", 1)],
    megas: &[],
    flowers: &[],
};
const CHERRY: Grower = Grower {
    trees: &[Entry("cherry", 1)],
    megas: &[],
    flowers: &[Entry("cherry_bees_005", 1)],
};
const DARK_OAK: Grower = Grower {
    trees: &[],
    megas: &[Entry("dark_oak", 1)],
    flowers: &[],
};
const PALE_OAK: Grower = Grower {
    trees: &[],
    megas: &[Entry("pale_oak_bonemeal", 1)],
    flowers: &[],
};
const POPLAR: Grower = Grower {
    trees: &[
        Entry("red_poplar", 1),
        Entry("orange_poplar", 1),
        Entry("yellow_poplar", 1),
    ],
    megas: &[],
    flowers: &[],
};

pub(crate) fn grower_of(kind: BlockKind) -> Option<&'static Grower> {
    match kind {
        BlockKind::OakSapling => Some(&OAK),
        BlockKind::SpruceSapling => Some(&SPRUCE),
        BlockKind::MangrovePropagule => Some(&MANGROVE),
        BlockKind::Azalea | BlockKind::FloweringAzalea => Some(&AZALEA),
        BlockKind::BirchSapling => Some(&BIRCH),
        BlockKind::JungleSapling => Some(&JUNGLE),
        BlockKind::AcaciaSapling => Some(&ACACIA),
        BlockKind::CherrySapling => Some(&CHERRY),
        BlockKind::DarkOakSapling => Some(&DARK_OAK),
        BlockKind::PaleOakSapling => Some(&PALE_OAK),
        BlockKind::PoplarSapling => Some(&POPLAR),
        _ => None,
    }
}

pub(crate) fn pick(list: &[Entry], mut roll: impl FnMut(u32) -> u32) -> Option<&'static str> {
    let total: u32 = list.iter().map(|entry| entry.1).sum();
    if total == 0 {
        return None;
    }
    let mut left = roll(total);
    for entry in list {
        if left < entry.1 {
            return Some(entry.0);
        }
        left -= entry.1;
    }
    None
}

pub(crate) fn tree_type_of(feature: &str) -> TreeType {
    match feature {
        "fancy_oak" | "fancy_oak_bees_005" => TreeType::BigTree,
        "birch" | "birch_bees_005" => TreeType::Birch,
        "spruce" => TreeType::Redwood,
        "mega_spruce" => TreeType::MegaRedwood,
        "mega_pine" => TreeType::MegaPine,
        "jungle_tree_no_vine" => TreeType::SmallJungle,
        "mega_jungle_tree" => TreeType::Jungle,
        "acacia" => TreeType::Acacia,
        "dark_oak" => TreeType::DarkOak,
        "mangrove" => TreeType::Mangrove,
        "tall_mangrove" => TreeType::TallMangrove,
        "azalea_tree" => TreeType::Azalea,
        "cherry" | "cherry_bees_005" => TreeType::Cherry,
        "pale_oak_bonemeal" => TreeType::PaleOak,
        "red_poplar" => TreeType::RedPoplar,
        "orange_poplar" => TreeType::OrangePoplar,
        "yellow_poplar" => TreeType::YellowPoplar,
        _ => TreeType::Tree,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_weighted_pick_walks_the_weights_in_order() {
        let list = [Entry("a", 9), Entry("b", 1)];
        assert_eq!(pick(&list, |_| 0), Some("a"));
        assert_eq!(pick(&list, |_| 8), Some("a"));
        assert_eq!(pick(&list, |_| 9), Some("b"));
        assert_eq!(pick(&[], |_| 0), None);
    }

    #[test]
    fn every_sapling_kind_has_a_grower_and_the_two_by_two_ones_have_no_single_tree() {
        for kind in [
            BlockKind::OakSapling,
            BlockKind::SpruceSapling,
            BlockKind::BirchSapling,
            BlockKind::JungleSapling,
            BlockKind::AcaciaSapling,
            BlockKind::CherrySapling,
            BlockKind::DarkOakSapling,
            BlockKind::PaleOakSapling,
            BlockKind::PoplarSapling,
            BlockKind::MangrovePropagule,
            BlockKind::Azalea,
            BlockKind::FloweringAzalea,
        ] {
            assert!(grower_of(kind).is_some(), "{kind:?}");
        }
        assert!(DARK_OAK.trees.is_empty() && PALE_OAK.trees.is_empty());
        assert_eq!(JUNGLE.megas[0].0, "mega_jungle_tree");
        assert_eq!(OAK.flowers.len(), 2);
    }

    #[test]
    fn feature_names_map_to_the_bukkit_tree_types() {
        assert_eq!(tree_type_of("fancy_oak"), TreeType::BigTree);
        assert_eq!(tree_type_of("mega_spruce"), TreeType::MegaRedwood);
        assert_eq!(tree_type_of("tall_mangrove"), TreeType::TallMangrove);
        assert_eq!(tree_type_of("oak"), TreeType::Tree);
    }
}

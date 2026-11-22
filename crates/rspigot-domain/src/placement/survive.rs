use std::sync::OnceLock;

use rspigot_blocks::{BlockKind, Face, StateFlags};

use super::rails::rail_removal_needed;
use super::support::{
    center, facing_of, flag, in_tag, is_kind, kind_in_tag, path, rigid, solid, step, sturdy,
};
use crate::{BlockPos, BlockStateId};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Rule {
    None,
    Floor(&'static str),
    FloorOrSame(&'static str),
    Mushroom,
    Cactus,
    SugarCane,
    Torch,
    WallTorch,
    SolidBelow,
    SolidBehind,
    HangingSign,
    Carpet,
    Plate,
    Wire,
    Candle,
    Rail,
    Door,
    DoublePlant(&'static str),
    Seagrass,
    Lantern,
    Cocoa,
    Bell,
    SeaPickle,
    Coral,
    Attached,
    SporeBlossom,
    SturdyBelow,
    Snow,
    Farmland,
    Propagule,
    Vine,
    Grindstone,
    BigDripleaf,
}

const VEGETATION: &str = "minecraft:supports_vegetation";

fn floor_tag(name: &str) -> Option<&'static str> {
    Some(match name {
        "azalea" | "flowering_azalea" => "minecraft:supports_azalea",
        "dead_bush" | "short_dry_grass" | "tall_dry_grass" => "minecraft:supports_dry_vegetation",
        "wither_rose" => "minecraft:supports_wither_rose",
        "crimson_fungus" => "minecraft:supports_crimson_fungus",
        "warped_fungus" => "minecraft:supports_warped_fungus",
        "crimson_roots" => "minecraft:supports_crimson_roots",
        "warped_roots" => "minecraft:supports_warped_roots",
        "nether_sprouts" => "minecraft:supports_nether_sprouts",
        "nether_wart" => "minecraft:supports_nether_wart",
        "wheat" | "carrots" | "potatoes" | "beetroots" | "torchflower_crop" | "pitcher_crop" => {
            "minecraft:supports_crops"
        }
        "pumpkin_stem" => "minecraft:supports_pumpkin_stem",
        "melon_stem" => "minecraft:supports_melon_stem",
        "bamboo_sapling" => "minecraft:supports_bamboo",
        "small_dripleaf" => "minecraft:supports_small_dripleaf",
        "short_grass" | "fern" | "bush" | "red_shrub" | "sweet_berry_bush" | "pink_petals"
        | "wildflowers" | "firefly_bush" | "open_eyeblossom" | "closed_eyeblossom"
        | "tall_grass" | "large_fern" | "sunflower" | "lilac" | "rose_bush" | "peony"
        | "pitcher_plant" => VEGETATION,
        _ => return None,
    })
}

fn is_double(kind: BlockKind) -> bool {
    kind.properties()
        .iter()
        .any(|property| property.name == "half" && property.values == ["upper", "lower"])
}

fn rule_of_kind(kind: BlockKind) -> Rule {
    let name = path(kind);
    if name == "mangrove_propagule" {
        return Rule::Propagule;
    }
    if name == "bamboo" {
        return Rule::FloorOrSame("minecraft:supports_bamboo");
    }
    if name == "seagrass" || name == "tall_seagrass" {
        return Rule::Seagrass;
    }
    if name.ends_with("_door") {
        return Rule::Door;
    }
    if is_double(kind) {
        return floor_tag(name).map_or(Rule::None, Rule::DoublePlant);
    }
    if let Some(tag) = floor_tag(name) {
        return Rule::Floor(tag);
    }
    if kind_in_tag(kind, "minecraft:saplings") || kind_in_tag(kind, "minecraft:small_flowers") {
        return Rule::Floor(VEGETATION);
    }
    if kind_in_tag(kind, "minecraft:rails") {
        return Rule::Rail;
    }
    if kind_in_tag(kind, "minecraft:pressure_plates") || matches!(name, "repeater" | "comparator") {
        return Rule::Plate;
    }
    if kind_in_tag(kind, "minecraft:lanterns") {
        return Rule::Lantern;
    }
    match name {
        "brown_mushroom" | "red_mushroom" => Rule::Mushroom,
        "cactus" => Rule::Cactus,
        "sugar_cane" => Rule::SugarCane,
        "torch" | "soul_torch" | "redstone_torch" | "copper_torch" => Rule::Torch,
        "wall_torch" | "soul_wall_torch" | "redstone_wall_torch" | "copper_wall_torch" => {
            Rule::WallTorch
        }
        "redstone_wire" => Rule::Wire,
        "cocoa" => Rule::Cocoa,
        "bell" => Rule::Bell,
        "sea_pickle" => Rule::SeaPickle,
        "spore_blossom" => Rule::SporeBlossom,
        "leaf_litter" => Rule::SturdyBelow,
        "snow" => Rule::Snow,
        "farmland" => Rule::Farmland,
        "vine" => Rule::Vine,
        "grindstone" => Rule::Grindstone,
        "big_dripleaf" => Rule::BigDripleaf,
        "cake" => Rule::SolidBelow,
        "ladder" => Rule::Attached,
        _ if name.ends_with("candle_cake") => Rule::SolidBelow,
        _ if name.ends_with("candle") => Rule::Candle,
        _ if name.ends_with("_carpet") => Rule::Carpet,
        _ if name.ends_with("_wall_sign") || name.ends_with("_wall_banner") => Rule::SolidBehind,
        _ if name.ends_with("_wall_hanging_sign") => Rule::None,
        _ if name.ends_with("_hanging_sign") => Rule::HangingSign,
        _ if name.ends_with("_sign") || name.ends_with("_banner") => Rule::SolidBelow,
        _ if name.ends_with("_coral_wall_fan") => Rule::Attached,
        _ if name.ends_with("_coral_fan") || name.ends_with("_coral") => Rule::Coral,
        _ if name.ends_with("_bud") || name == "amethyst_cluster" => Rule::Attached,
        _ => Rule::None,
    }
}

fn rule_of(state: BlockStateId) -> Rule {
    static RULES: OnceLock<Vec<Rule>> = OnceLock::new();
    let Some(kind) = state.kind() else {
        return Rule::None;
    };
    RULES.get_or_init(|| BlockKind::ALL.iter().map(|k| rule_of_kind(*k)).collect())
        [usize::from(kind.id().0)]
}

fn has_top_face(state: BlockStateId) -> bool {
    state
        .collision_shape()
        .iter()
        .any(|aabb| aabb.max_y >= 1.0 - 1.0e-9)
}

fn has_fluid(state: BlockStateId) -> bool {
    state.flags().has_fluid() || flag(state, "waterlogged")
}

pub(super) fn vine_supports(
    pos: BlockPos,
    face: Face,
    block: &impl Fn(BlockPos) -> BlockStateId,
) -> bool {
    if face == Face::Down {
        return false;
    }
    let neighbour = block(step(pos, face));
    if sturdy(neighbour, face.opposite()) {
        return true;
    }
    if face == Face::Up {
        return false;
    }
    let above = block(step(pos, Face::Up));
    is_kind(above, BlockKind::Vine) && flag(above, face.name())
}

pub fn vine_faces(
    pos: BlockPos,
    state: BlockStateId,
    block: &impl Fn(BlockPos) -> BlockStateId,
) -> BlockStateId {
    let mut next = state;
    for face in [Face::Up, Face::North, Face::South, Face::West, Face::East] {
        let held = flag(state, face.name()) && vine_supports(pos, face, block);
        next = super::support::with(next, face.name(), super::support::boolean(held));
    }
    next
}

pub fn vine_has_faces(state: BlockStateId) -> bool {
    [Face::Up, Face::North, Face::South, Face::West, Face::East]
        .into_iter()
        .any(|face| flag(state, face.name()))
}

fn bell_survives(
    state: BlockStateId,
    pos: BlockPos,
    block: &impl Fn(BlockPos) -> BlockStateId,
) -> bool {
    let facing = facing_of(state).unwrap_or(Face::North);
    match state.property("attachment") {
        Some("floor") => sturdy(block(step(pos, Face::Down)), Face::Up),
        Some("ceiling") => center(block(step(pos, Face::Up)), Face::Down),
        Some("double_wall") => {
            let along = if matches!(facing, Face::North | Face::South) {
                [Face::East, Face::West]
            } else {
                [Face::North, Face::South]
            };
            along
                .into_iter()
                .all(|side| sturdy(block(step(pos, side)), side.opposite()))
        }
        _ => sturdy(block(step(pos, facing.opposite())), facing),
    }
}

pub fn can_survive(
    state: BlockStateId,
    pos: BlockPos,
    block: &impl Fn(BlockPos) -> BlockStateId,
) -> bool {
    let below = || block(step(pos, Face::Down));
    let above = || block(step(pos, Face::Up));
    let behind = |facing: Face| block(step(pos, facing.opposite()));
    match rule_of(state) {
        Rule::None => true,
        Rule::Floor(tag) => in_tag(below(), tag),
        Rule::FloorOrSame(tag) => {
            let below = below();
            in_tag(below, tag) || below.kind() == state.kind()
        }
        Rule::Mushroom => {
            let below = below();
            in_tag(below, "minecraft:overrides_mushroom_light_requirement")
                || below.flags().contains(StateFlags::SOLID_RENDER)
        }
        Rule::Cactus => {
            let walled = Face::HORIZONTAL.into_iter().any(|side| {
                let neighbour = block(step(pos, side));
                solid(neighbour) || neighbour.flags().contains(StateFlags::LAVA)
            });
            let below = below();
            !walled
                && (in_tag(below, "minecraft:supports_cactus") || is_kind(below, BlockKind::Cactus))
                && !above().flags().has_fluid()
        }
        Rule::SugarCane => {
            let base = step(pos, Face::Down);
            let below = block(base);
            if below.kind() == state.kind() {
                return true;
            }
            in_tag(below, "minecraft:supports_sugar_cane")
                && Face::HORIZONTAL.into_iter().any(|side| {
                    let neighbour = block(step(base, side));
                    has_fluid(neighbour)
                        || in_tag(neighbour, "minecraft:supports_sugar_cane_adjacently")
                })
        }
        Rule::Torch => center(below(), Face::Up),
        Rule::WallTorch => facing_of(state).is_some_and(|facing| sturdy(behind(facing), facing)),
        Rule::SolidBelow => solid(below()),
        Rule::SolidBehind => facing_of(state).is_none_or(|facing| solid(behind(facing))),
        Rule::HangingSign => center(above(), Face::Down),
        Rule::Carpet => !below().is_air(),
        Rule::Plate => {
            let below = below();
            rigid(below) || center(below, Face::Up)
        }
        Rule::Wire => {
            let below = below();
            sturdy(below, Face::Up) || is_kind(below, BlockKind::Hopper)
        }
        Rule::Candle => center(below(), Face::Up),
        Rule::Rail => !rail_removal_needed(state, pos, block),
        Rule::Door => {
            let below = below();
            if state.property("half") == Some("upper") {
                below.kind() == state.kind()
            } else {
                sturdy(below, Face::Up)
            }
        }
        Rule::DoublePlant(tag) => {
            let below = below();
            if state.property("half") == Some("upper") {
                below.kind() == state.kind() && below.property("half") == Some("lower")
            } else {
                in_tag(below, tag)
            }
        }
        Rule::Seagrass => {
            let below = below();
            if state.property("half") == Some("upper") {
                below.kind() == state.kind() && below.property("half") == Some("lower")
            } else {
                sturdy(below, Face::Up) && !in_tag(below, "minecraft:cannot_support_seagrass")
            }
        }
        Rule::Lantern => {
            if flag(state, "hanging") {
                center(above(), Face::Down)
            } else {
                center(below(), Face::Up)
            }
        }
        Rule::Cocoa => facing_of(state)
            .is_some_and(|facing| in_tag(block(step(pos, facing)), "minecraft:supports_cocoa")),
        Rule::Bell => bell_survives(state, pos, block),
        Rule::SeaPickle => {
            let below = below();
            !below.collision_shape().is_empty() || has_top_face(below) || center(below, Face::Up)
        }
        Rule::Coral => sturdy(below(), Face::Up),
        Rule::Attached => facing_of(state).is_none_or(|facing| sturdy(behind(facing), facing)),
        Rule::SporeBlossom => center(above(), Face::Down),
        Rule::SturdyBelow => sturdy(below(), Face::Up),
        Rule::Snow => {
            let below = below();
            if in_tag(below, "minecraft:cannot_support_snow_layer") {
                return false;
            }
            in_tag(below, "minecraft:support_override_snow_layer")
                || sturdy(below, Face::Up)
                || (below.kind() == state.kind() && below.property("layers") == Some("8"))
        }
        Rule::Farmland => {
            let above = above();
            !solid(above) || in_tag(above, "minecraft:maintains_farmland")
        }
        Rule::Propagule => {
            if flag(state, "hanging") {
                in_tag(above(), "minecraft:supports_hanging_mangrove_propagule")
            } else {
                in_tag(below(), "minecraft:supports_mangrove_propagule")
            }
        }
        Rule::Vine => vine_has_faces(vine_faces(pos, state, block)),
        Rule::BigDripleaf => {
            let below = below();
            in_tag(below, "minecraft:supports_big_dripleaf")
                || matches!(below.kind(), Some(BlockKind::BigDripleafStem))
        }
        Rule::Grindstone => match state.property("face") {
            Some("floor") => sturdy(below(), Face::Up),
            Some("ceiling") => sturdy(above(), Face::Down),
            _ => facing_of(state).is_none_or(|facing| sturdy(behind(facing), facing)),
        },
    }
}

pub(super) fn is_door_or_plant(state: BlockStateId) -> bool {
    matches!(
        rule_of(state),
        Rule::Door | Rule::DoublePlant(_) | Rule::Seagrass
    )
}

pub(super) fn is_farmland(state: BlockStateId) -> bool {
    rule_of(state) == Rule::Farmland
}

pub(super) fn is_vine(state: BlockStateId) -> bool {
    rule_of(state) == Rule::Vine
}

pub fn is_deferred(state: BlockStateId) -> bool {
    matches!(
        rule_of(state),
        Rule::Cactus | Rule::SugarCane | Rule::FloorOrSame(_)
    )
}

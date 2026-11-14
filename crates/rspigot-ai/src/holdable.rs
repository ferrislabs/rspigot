use rspigot_blocks::{BlockKind, Face, StateFlags, StateId, Support};
use rspigot_domain::entity::physics::has_tag;
use rspigot_domain::entity::raycast;
use rspigot_domain::{BlockPos, Vec3};

use crate::BlockView;
use crate::world::AiWorld;

const SNOW: &str = "minecraft:snow";
const SMALL_FLOWERS: &str = "minecraft:small_flowers";
const SUPPORTS_VEGETATION: &str = "minecraft:supports_vegetation";
const SUPPORTS_CACTUS: &str = "minecraft:supports_cactus";
const SUPPORT_OVERRIDE_CACTUS_FLOWER: &str = "minecraft:support_override_cactus_flower";
const OVERRIDES_MUSHROOM_LIGHT: &str = "minecraft:overrides_mushroom_light_requirement";
const SUPPORTS_CRIMSON_FUNGUS: &str = "minecraft:supports_crimson_fungus";
const SUPPORTS_WARPED_FUNGUS: &str = "minecraft:supports_warped_fungus";
const SUPPORTS_CRIMSON_ROOTS: &str = "minecraft:supports_crimson_roots";
const SUPPORTS_WARPED_ROOTS: &str = "minecraft:supports_warped_roots";
const MUSHROOM_LIGHT_LIMIT: i32 = 13;
const UPDATE_SHAPE_ORDER: [Face; 6] = [
    Face::West,
    Face::East,
    Face::North,
    Face::South,
    Face::Down,
    Face::Up,
];

fn offset(pos: BlockPos, face: Face) -> BlockPos {
    let (dx, dy, dz) = face.offset();
    pos.offset(dx, dy, dz)
}

fn state_at(world: &impl BlockView, pos: BlockPos) -> StateId {
    world.state(pos).unwrap_or(StateId::AIR)
}

fn is_liquid_block(state: StateId) -> bool {
    matches!(state.kind(), Some(BlockKind::Water | BlockKind::Lava))
}

fn is_snowy_dirt(kind: BlockKind) -> bool {
    matches!(
        kind,
        BlockKind::GrassBlock | BlockKind::Mycelium | BlockKind::Podzol
    )
}

fn is_vegetation(state: StateId) -> bool {
    let Some(kind) = state.kind() else {
        return false;
    };
    matches!(
        kind,
        BlockKind::BrownMushroom
            | BlockKind::RedMushroom
            | BlockKind::CactusFlower
            | BlockKind::CrimsonFungus
            | BlockKind::WarpedFungus
            | BlockKind::CrimsonRoots
            | BlockKind::WarpedRoots
    ) || has_tag(state, SMALL_FLOWERS)
}

pub fn can_survive(world: &impl AiWorld, state: StateId, pos: BlockPos) -> bool {
    let Some(kind) = state.kind() else {
        return true;
    };
    let below_pos = pos.offset(0, -1, 0);
    let below = state_at(world, below_pos);
    match kind {
        BlockKind::BrownMushroom | BlockKind::RedMushroom => {
            has_tag(below, OVERRIDES_MUSHROOM_LIGHT)
                || (world.raw_brightness_undarkened(pos) < MUSHROOM_LIGHT_LIMIT
                    && below.flags().contains(StateFlags::SOLID_RENDER))
        }
        BlockKind::Cactus => {
            let sides_clear = [Face::North, Face::South, Face::West, Face::East]
                .into_iter()
                .all(|face| {
                    let side = state_at(world, offset(pos, face));
                    !side.flags().contains(StateFlags::LEGACY_SOLID)
                        && !side.flags().contains(StateFlags::LAVA)
                });
            sides_clear
                && (below.kind() == Some(BlockKind::Cactus) || has_tag(below, SUPPORTS_CACTUS))
                && !is_liquid_block(state_at(world, pos.offset(0, 1, 0)))
        }
        BlockKind::CactusFlower => {
            has_tag(below, SUPPORT_OVERRIDE_CACTUS_FLOWER)
                || below.is_face_sturdy(Face::Up, Support::Center)
        }
        BlockKind::CrimsonFungus => has_tag(below, SUPPORTS_CRIMSON_FUNGUS),
        BlockKind::WarpedFungus => has_tag(below, SUPPORTS_WARPED_FUNGUS),
        BlockKind::CrimsonRoots => has_tag(below, SUPPORTS_CRIMSON_ROOTS),
        BlockKind::WarpedRoots => has_tag(below, SUPPORTS_WARPED_ROOTS),
        _ if has_tag(state, SMALL_FLOWERS) => has_tag(below, SUPPORTS_VEGETATION),
        _ => true,
    }
}

pub fn update_from_neighbour_shapes(
    world: &impl AiWorld,
    state: StateId,
    pos: BlockPos,
) -> StateId {
    let mut current = state;
    for face in UPDATE_SHAPE_ORDER {
        let Some(kind) = current.kind() else {
            continue;
        };
        if is_snowy_dirt(kind) && face == Face::Up {
            let snowy = has_tag(state_at(world, offset(pos, face)), SNOW);
            current = current
                .with_property("snowy", if snowy { "true" } else { "false" })
                .unwrap_or(current);
        } else if is_vegetation(current) && !can_survive(world, current, pos) {
            current = StateId::AIR;
        }
    }
    current
}

struct Outline<'a, V>(&'a V);

impl<V: BlockView> rspigot_domain::entity::BlockView for Outline<'_, V> {
    fn block(&self, pos: BlockPos) -> Option<StateId> {
        self.0.state(pos)
    }
}

fn outline_shape(state: StateId) -> &'static [rspigot_blocks::Aabb] {
    let passed = matches!(
        state.kind(),
        Some(BlockKind::Light | BlockKind::Water | BlockKind::Lava | BlockKind::BubbleColumn)
    );
    if state.is_air() || passed {
        return &[];
    }
    match state.collision_shape() {
        [] => rspigot_blocks::ShapeId::FULL.boxes(),
        shape => shape,
    }
}

pub fn first_outline_hit(world: &impl BlockView, from: Vec3, to: Vec3) -> Option<BlockPos> {
    raycast::clip_blocks_with(&Outline(world), from, to, outline_shape).map(|hit| hit.pos)
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;
    use crate::scene::Scene;

    #[test]
    fn grass_picks_up_snow_from_above() {
        let mut scene = Scene::default();
        scene.put(
            BlockPos::new(0, 65, 0),
            BlockKind::SnowBlock.default_state(),
        );
        let placed = update_from_neighbour_shapes(
            &scene,
            BlockKind::GrassBlock.default_state(),
            BlockPos::new(0, 64, 0),
        );
        assert_eq!(placed.property("snowy"), Some("true"));
        let bare = update_from_neighbour_shapes(
            &Scene::default(),
            BlockKind::GrassBlock.default_state(),
            BlockPos::new(0, 64, 0),
        );
        assert_eq!(bare.property("snowy"), Some("false"));
    }

    #[test]
    fn flowers_need_vegetation_ground() {
        let mut scene = Scene::default();
        let pos = BlockPos::new(0, 64, 0);
        let poppy = BlockKind::Poppy.default_state();
        assert!(!can_survive(&scene, poppy, pos));
        assert_eq!(
            update_from_neighbour_shapes(&scene, poppy, pos),
            StateId::AIR
        );
        scene.put(pos.offset(0, -1, 0), BlockKind::GrassBlock.default_state());
        assert!(can_survive(&scene, poppy, pos));
        assert_eq!(update_from_neighbour_shapes(&scene, poppy, pos), poppy);
    }

    #[test]
    fn mushrooms_need_darkness_or_special_ground() {
        let mut scene = Scene::default();
        let pos = BlockPos::new(0, 64, 0);
        scene.put(pos.offset(0, -1, 0), BlockKind::Stone.default_state());
        let mushroom = BlockKind::RedMushroom.default_state();
        scene.undarkened = 15;
        assert!(!can_survive(&scene, mushroom, pos));
        scene.undarkened = 12;
        assert!(can_survive(&scene, mushroom, pos));
        scene.undarkened = 15;
        scene.put(pos.offset(0, -1, 0), BlockKind::Mycelium.default_state());
        assert!(can_survive(&scene, mushroom, pos));
    }

    #[test]
    fn cacti_need_sand_and_free_sides() {
        let mut scene = Scene::default();
        let pos = BlockPos::new(0, 64, 0);
        let cactus = BlockKind::Cactus.default_state();
        scene.put(pos.offset(0, -1, 0), BlockKind::Sand.default_state());
        assert!(can_survive(&scene, cactus, pos));
        scene.put(pos.offset(1, 0, 0), BlockKind::Stone.default_state());
        assert!(!can_survive(&scene, cactus, pos));
        let mut dirt = Scene::default();
        dirt.put(pos.offset(0, -1, 0), BlockKind::Dirt.default_state());
        assert!(!can_survive(&dirt, cactus, pos));
    }

    #[test]
    fn plain_holdables_always_survive() {
        let scene = Scene::default();
        for kind in [
            BlockKind::Pumpkin,
            BlockKind::Tnt,
            BlockKind::Dirt,
            BlockKind::Gravel,
        ] {
            assert!(can_survive(
                &scene,
                kind.default_state(),
                BlockPos::new(0, 64, 0)
            ));
        }
    }

    #[test]
    fn the_outline_ray_stops_at_the_first_solid_block() {
        let mut blocks = HashMap::new();
        blocks.insert(BlockPos::new(2, 64, 0), BlockKind::Stone.default_state());
        blocks.insert(BlockPos::new(4, 64, 0), BlockKind::Dirt.default_state());
        let scene = Scene::from(blocks);
        let from = Vec3::new(0.5, 64.5, 0.5);
        assert_eq!(
            first_outline_hit(&scene, from, Vec3::new(4.5, 64.5, 0.5)),
            Some(BlockPos::new(2, 64, 0))
        );
        assert_eq!(
            first_outline_hit(&scene, from, Vec3::new(0.5, 66.5, 0.5)),
            None
        );
    }
}

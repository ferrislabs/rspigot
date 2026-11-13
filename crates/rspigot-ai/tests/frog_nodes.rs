use std::collections::HashMap;

use rspigot_ai::{
    BlockView, MobProfile, MobState, PathFinder, PathType, can_cut_corner_for, mob_path_type,
};
use rspigot_blocks::{BlockKind, StateId};
use rspigot_domain::{BlockPos, Vec3};

struct World {
    blocks: HashMap<BlockPos, StateId>,
}

impl BlockView for World {
    fn state(&self, pos: BlockPos) -> Option<StateId> {
        Some(self.blocks.get(&pos).copied().unwrap_or(StateId::AIR))
    }

    fn min_y(&self) -> i32 {
        0
    }

    fn max_y(&self) -> i32 {
        256
    }
}

impl World {
    fn pond() -> Self {
        let mut blocks = HashMap::new();
        for x in -8..=12 {
            for z in -4..=4 {
                blocks.insert(BlockPos::new(x, 60, z), BlockKind::Stone.default_state());
                for y in 61..=62 {
                    blocks.insert(BlockPos::new(x, y, z), BlockKind::Water.default_state());
                }
            }
        }
        Self { blocks }
    }
}

fn frog() -> MobProfile {
    let mut profile = MobProfile::new(0.5, 0.5).amphibious(true).frog_nodes();
    profile.set_malus(PathType::Water, 4.0);
    profile.set_malus(PathType::Trapdoor, -1.0);
    profile
}

fn plain_amphibian() -> MobProfile {
    MobProfile::new(0.5, 0.5).amphibious(true)
}

fn swimming() -> MobState {
    MobState {
        position: Vec3::new(0.5, 61.7, 0.5),
        on_ground: false,
        in_floatable_fluid: true,
    }
}

fn first_y(profile: MobProfile) -> i32 {
    let world = World::pond();
    PathFinder::new(profile)
        .find_path(&world, &swimming(), &[BlockPos::new(5, 61, 0)], 0)
        .expect("a path")
        .node(0)
        .pos
        .y
}

#[test]
fn a_swimming_frog_starts_at_the_floored_bottom_of_its_box() {
    assert_eq!(first_y(frog()), 61);
    assert_eq!(first_y(plain_amphibian()), 62);
}

#[test]
fn a_block_that_frogs_prefer_to_jump_to_makes_the_space_above_it_open() {
    let mut world = World::pond();
    world
        .blocks
        .insert(BlockPos::new(2, 63, 0), BlockKind::LilyPad.default_state());
    let on_ground = MobState {
        position: Vec3::new(0.5, 64.0, 0.5),
        on_ground: true,
        in_floatable_fluid: false,
    };
    let above_lily = BlockPos::new(2, 64, 0);
    assert_eq!(
        mob_path_type(&world, &frog(), on_ground, above_lily),
        PathType::Open
    );
    assert_ne!(
        mob_path_type(&world, &plain_amphibian(), on_ground, above_lily),
        PathType::Open
    );
}

#[test]
fn a_frog_never_cuts_a_corner_past_the_water_border_and_other_mobs_do() {
    assert!(!can_cut_corner_for(&frog(), PathType::WaterBorder));
    assert!(can_cut_corner_for(
        &plain_amphibian(),
        PathType::WaterBorder
    ));
    assert!(can_cut_corner_for(&frog(), PathType::Walkable));
    assert!(!can_cut_corner_for(&frog(), PathType::WalkableDoor));
}

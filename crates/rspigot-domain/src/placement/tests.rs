use std::collections::HashMap;

use rspigot_blocks::{BlockKind, Face};

use super::*;
use crate::{BlockPos, BlockStateId};

struct Level(HashMap<BlockPos, BlockStateId>);

impl Level {
    fn new() -> Self {
        Self(HashMap::new())
    }

    fn put(mut self, x: i32, y: i32, z: i32, kind: BlockKind) -> Self {
        self.0.insert(BlockPos::new(x, y, z), kind.default_state());
        self
    }

    fn with(mut self, x: i32, y: i32, z: i32, state: BlockStateId) -> Self {
        self.0.insert(BlockPos::new(x, y, z), state);
        self
    }

    fn at(&self) -> impl Fn(BlockPos) -> BlockStateId + '_ {
        |pos| self.0.get(&pos).copied().unwrap_or(BlockStateId::AIR)
    }
}

fn state(kind: BlockKind, properties: &[(&str, &str)]) -> BlockStateId {
    kind.state_with(properties).expect("state exists")
}

fn origin() -> BlockPos {
    BlockPos::new(0, 1, 0)
}

fn click(face: Face) -> Click {
    Click {
        face,
        cursor_y: 0.5,
        look: Look {
            yaw: 0.0,
            pitch: 0.0,
        },
        sneaking: false,
        replacing_clicked: false,
    }
}

#[test]
fn a_fence_connects_to_a_fence_a_solid_block_and_a_gate_but_not_to_air() {
    let level = Level::new()
        .put(0, 0, 0, BlockKind::Stone)
        .put(1, 1, 0, BlockKind::OakFence)
        .put(0, 1, 1, BlockKind::Stone)
        .with(
            -1,
            1,
            0,
            state(BlockKind::OakFenceGate, &[("facing", "north")]),
        );
    let block = level.at();
    let fence = connection_state(BlockKind::OakFence.default_state(), origin(), None, &block)
        .expect("a fence connects");
    assert_eq!(fence.property("east"), Some("true"));
    assert_eq!(fence.property("south"), Some("true"));
    assert_eq!(fence.property("west"), Some("true"));
    assert_eq!(fence.property("north"), Some("false"));
}

#[test]
fn a_wooden_fence_does_not_join_a_nether_brick_fence() {
    let level = Level::new().put(1, 1, 0, BlockKind::NetherBrickFence);
    let fence = connection_state(
        BlockKind::OakFence.default_state(),
        origin(),
        None,
        &level.at(),
    )
    .expect("a fence");
    assert_eq!(fence.property("east"), Some("false"));
}

#[test]
fn a_neighbour_change_updates_only_the_side_it_touches() {
    let level = Level::new().put(0, 1, -1, BlockKind::OakFence);
    let shaped = update_shape(
        BlockKind::OakFence.default_state(),
        origin(),
        Face::North,
        &level.at(),
    );
    let Shaped::Changed(next) = shaped else {
        panic!("the fence should connect north");
    };
    assert_eq!(next.property("north"), Some("true"));
    assert_eq!(next.property("east"), Some("false"));
}

#[test]
fn a_glass_pane_and_iron_bars_connect_to_solid_blocks_and_each_other() {
    let level = Level::new()
        .put(1, 1, 0, BlockKind::Stone)
        .put(0, 1, 1, BlockKind::IronBars);
    let pane = connection_state(
        BlockKind::GlassPane.default_state(),
        origin(),
        None,
        &level.at(),
    )
    .expect("a pane");
    assert_eq!(pane.property("east"), Some("true"));
    assert_eq!(pane.property("south"), Some("true"));
    assert_eq!(pane.property("north"), Some("false"));
}

#[test]
fn a_wall_is_low_beside_a_wall_tall_under_a_block_and_posted_at_a_corner() {
    let level = Level::new()
        .put(1, 1, 0, BlockKind::CobblestoneWall)
        .put(0, 1, 1, BlockKind::CobblestoneWall)
        .put(0, 2, 0, BlockKind::Stone);
    let wall = connection_state(
        BlockKind::CobblestoneWall.default_state(),
        origin(),
        None,
        &level.at(),
    )
    .expect("a wall");
    assert_eq!(wall.property("east"), Some("tall"));
    assert_eq!(wall.property("south"), Some("tall"));
    assert_eq!(wall.property("north"), Some("none"));
    assert_eq!(wall.property("up"), Some("true"));
    let open = Level::new().put(1, 1, 0, BlockKind::CobblestoneWall).put(
        -1,
        1,
        0,
        BlockKind::CobblestoneWall,
    );
    let straight = connection_state(
        BlockKind::CobblestoneWall.default_state(),
        origin(),
        None,
        &open.at(),
    )
    .expect("a wall");
    assert_eq!(straight.property("east"), Some("low"));
    assert_eq!(straight.property("up"), Some("false"));
}

fn stairs(facing: &str) -> BlockStateId {
    state(
        BlockKind::OakStairs,
        &[("facing", facing), ("half", "bottom")],
    )
}

#[test]
fn stairs_turn_into_inner_and_outer_corners() {
    let outer = Level::new().with(0, 1, -1, stairs("east"));
    assert_eq!(
        stairs_shape(stairs("north"), origin(), &outer.at()),
        "outer_right"
    );
    let outer_left = Level::new().with(0, 1, -1, stairs("west"));
    assert_eq!(
        stairs_shape(stairs("north"), origin(), &outer_left.at()),
        "outer_left"
    );
    let inner = Level::new().with(0, 1, 1, stairs("east"));
    assert_eq!(
        stairs_shape(stairs("north"), origin(), &inner.at()),
        "inner_right"
    );
    let none = Level::new().with(0, 1, -1, stairs("north"));
    assert_eq!(
        stairs_shape(stairs("north"), origin(), &none.at()),
        "straight"
    );
}

fn rail(shape: &str) -> BlockStateId {
    state(
        BlockKind::Rail,
        &[("shape", shape), ("waterlogged", "false")],
    )
}

#[test]
fn a_rail_placed_beside_a_rail_turns_both_into_a_curve() {
    let level = Level::new()
        .put(0, 0, 0, BlockKind::Stone)
        .put(1, 0, 0, BlockKind::Stone)
        .put(0, 0, 1, BlockKind::Stone)
        .with(1, 1, 0, rail("east_west"))
        .with(0, 1, 1, rail("north_south"));
    let changes = rail_placed(
        origin(),
        BlockKind::Rail.default_state(),
        false,
        &level.at(),
    );
    let own = changes
        .iter()
        .find(|c| c.pos == origin())
        .expect("own change");
    assert_eq!(own.state.property("shape"), Some("south_east"));
}

#[test]
fn a_rail_climbs_toward_a_rail_one_block_higher() {
    let level = Level::new()
        .with(1, 2, 0, rail("east_west"))
        .with(-1, 1, 0, rail("east_west"));
    let changes = rail_placed(
        origin(),
        BlockKind::Rail.default_state(),
        false,
        &level.at(),
    );
    let own = changes
        .iter()
        .find(|c| c.pos == origin())
        .expect("own change");
    assert_eq!(own.state.property("shape"), Some("ascending_east"));
}

#[test]
fn a_lone_rail_stays_north_south() {
    let changes = rail_placed(
        origin(),
        BlockKind::Rail.default_state(),
        false,
        &Level::new().at(),
    );
    assert_eq!(changes.len(), 1);
    assert_eq!(changes[0].state.property("shape"), Some("north_south"));
}

#[test]
fn plants_torches_carpets_and_cacti_need_their_support() {
    let grass = Level::new().put(0, 0, 0, BlockKind::GrassBlock);
    let stone = Level::new().put(0, 0, 0, BlockKind::Stone);
    let poppy = BlockKind::Poppy.default_state();
    assert!(can_survive(poppy, origin(), &grass.at()));
    assert!(!can_survive(poppy, origin(), &stone.at()));
    assert!(!can_survive(poppy, origin(), &Level::new().at()));
    let torch = BlockKind::Torch.default_state();
    assert!(can_survive(torch, origin(), &stone.at()));
    assert!(!can_survive(torch, origin(), &Level::new().at()));
    let carpet = BlockKind::WhiteCarpet.default_state();
    assert!(can_survive(carpet, origin(), &stone.at()));
    assert!(!can_survive(carpet, origin(), &Level::new().at()));
    let sand = Level::new().put(0, 0, 0, BlockKind::Sand);
    let cactus = BlockKind::Cactus.default_state();
    assert!(can_survive(cactus, origin(), &sand.at()));
    let hemmed = Level::new()
        .put(0, 0, 0, BlockKind::Sand)
        .put(1, 1, 0, BlockKind::Stone);
    assert!(!can_survive(cactus, origin(), &hemmed.at()));
    let wall_torch = state(BlockKind::WallTorch, &[("facing", "north")]);
    let backed = Level::new().put(0, 1, 1, BlockKind::Stone);
    assert!(can_survive(wall_torch, origin(), &backed.at()));
    assert!(!can_survive(wall_torch, origin(), &stone.at()));
}

#[test]
fn sugar_cane_needs_water_beside_its_base() {
    let dry = Level::new().put(0, 0, 0, BlockKind::Sand);
    let wet = Level::new()
        .put(0, 0, 0, BlockKind::Sand)
        .put(1, 0, 0, BlockKind::Water);
    let cane = BlockKind::SugarCane.default_state();
    assert!(!can_survive(cane, origin(), &dry.at()));
    assert!(can_survive(cane, origin(), &wet.at()));
}

fn door(half: &str) -> BlockStateId {
    state(BlockKind::OakDoor, &[("half", half), ("facing", "north")])
}

#[test]
fn a_door_half_without_its_partner_or_floor_is_destroyed() {
    let lone_upper = Level::new().with(0, 1, 0, door("upper"));
    assert_eq!(
        update_shape(door("upper"), origin(), Face::Down, &lone_upper.at()),
        Shaped::Destroy
    );
    let floating_lower = Level::new()
        .with(0, 1, 0, door("lower"))
        .with(0, 2, 0, door("upper"));
    assert_eq!(
        update_shape(door("lower"), origin(), Face::Down, &floating_lower.at()),
        Shaped::Destroy
    );
    let standing = Level::new()
        .put(0, 0, 0, BlockKind::Stone)
        .with(0, 1, 0, door("lower"))
        .with(0, 2, 0, door("upper"));
    assert_eq!(
        update_shape(door("lower"), origin(), Face::Down, &standing.at()),
        Shaped::Same
    );
    assert_eq!(
        update_shape(
            door("upper"),
            origin().offset(0, 1, 0),
            Face::Down,
            &standing.at()
        ),
        Shaped::Same
    );
}

#[test]
fn a_tall_plant_loses_its_top_when_the_bottom_goes() {
    let sunflower = |half: &str| state(BlockKind::Sunflower, &[("half", half)]);
    let level = Level::new().with(0, 2, 0, sunflower("upper"));
    assert_eq!(
        update_shape(
            sunflower("upper"),
            BlockPos::new(0, 2, 0),
            Face::Down,
            &level.at()
        ),
        Shaped::Destroy
    );
}

fn slab(kind_type: &str) -> BlockStateId {
    state(
        BlockKind::StoneSlab,
        &[("type", kind_type), ("waterlogged", "false")],
    )
}

fn clicking(face: Face, cursor_y: f64, replacing_clicked: bool) -> Click {
    Click {
        face,
        cursor_y,
        replacing_clicked,
        ..click(face)
    }
}

#[test]
fn a_slab_is_replaced_by_the_same_slab_on_the_open_half() {
    let level = Level::new();
    let block = level.at();
    let held = BlockKind::StoneSlab;
    assert!(can_be_replaced(
        slab("bottom"),
        origin(),
        held,
        &clicking(Face::Up, 1.0, true),
        &block
    ));
    assert!(can_be_replaced(
        slab("bottom"),
        origin(),
        held,
        &clicking(Face::North, 0.75, true),
        &block
    ));
    assert!(!can_be_replaced(
        slab("bottom"),
        origin(),
        held,
        &clicking(Face::North, 0.25, true),
        &block
    ));
    assert!(can_be_replaced(
        slab("top"),
        origin(),
        held,
        &clicking(Face::Down, 0.0, true),
        &block
    ));
    assert!(!can_be_replaced(
        slab("double"),
        origin(),
        held,
        &clicking(Face::Up, 1.0, true),
        &block
    ));
    assert!(can_be_replaced(
        slab("bottom"),
        origin(),
        held,
        &clicking(Face::North, 0.25, false),
        &block
    ));
    assert!(!can_be_replaced(
        slab("bottom"),
        origin(),
        BlockKind::OakSlab,
        &clicking(Face::Up, 1.0, true),
        &block
    ));
}

#[test]
fn placing_a_slab_on_a_slab_makes_a_double_slab() {
    let level = Level::new().with(0, 1, 0, slab("bottom"));
    let placed = adjust(
        BlockKind::StoneSlab.default_state(),
        origin(),
        &clicking(Face::Up, 1.0, true),
        &level.at(),
    )
    .expect("a double slab");
    assert_eq!(placed.state.property("type"), Some("double"));
}

#[test]
fn candles_turtle_eggs_petals_and_snow_stack_up_to_their_limit() {
    let candles = |n: &str| state(BlockKind::Candle, &[("candles", n)]);
    let level = Level::new()
        .put(0, 0, 0, BlockKind::Stone)
        .with(0, 1, 0, candles("3"));
    let block = level.at();
    let merged = adjust(candles("1"), origin(), &click(Face::Up), &block).expect("merge");
    assert_eq!(merged.state.property("candles"), Some("4"));
    assert!(can_be_replaced(
        candles("3"),
        origin(),
        BlockKind::Candle,
        &click(Face::Up),
        &block
    ));
    assert!(!can_be_replaced(
        candles("4"),
        origin(),
        BlockKind::Candle,
        &click(Face::Up),
        &block
    ));
    let sneaking = Click {
        sneaking: true,
        ..click(Face::Up)
    };
    assert!(!can_be_replaced(
        candles("2"),
        origin(),
        BlockKind::Candle,
        &sneaking,
        &block
    ));
    let egg = |n: &str| state(BlockKind::TurtleEgg, &[("eggs", n), ("hatch", "0")]);
    assert!(can_be_replaced(
        egg("3"),
        origin(),
        BlockKind::TurtleEgg,
        &click(Face::Up),
        &block
    ));
    assert!(!can_be_replaced(
        egg("4"),
        origin(),
        BlockKind::TurtleEgg,
        &click(Face::Up),
        &block
    ));
    let snow = |n: &str| state(BlockKind::Snow, &[("layers", n)]);
    assert!(can_be_replaced(
        snow("7"),
        origin(),
        BlockKind::Snow,
        &clicking(Face::Up, 1.0, true),
        &block
    ));
    assert!(!can_be_replaced(
        snow("7"),
        origin(),
        BlockKind::Snow,
        &clicking(Face::North, 0.5, true),
        &block
    ));
    assert!(!can_be_replaced(
        snow("8"),
        origin(),
        BlockKind::Snow,
        &clicking(Face::Up, 1.0, true),
        &block
    ));
    assert!(can_be_replaced(
        snow("1"),
        origin(),
        BlockKind::Stone,
        &click(Face::Up),
        &block
    ));
    assert!(!can_be_replaced(
        snow("2"),
        origin(),
        BlockKind::Stone,
        &click(Face::Up),
        &block
    ));
    let stacked = adjust(
        snow("1"),
        origin(),
        &clicking(Face::Up, 1.0, true),
        &Level::new()
            .put(0, 0, 0, BlockKind::Stone)
            .with(0, 1, 0, snow("5"))
            .at(),
    )
    .expect("snow stacks");
    assert_eq!(stacked.state.property("layers"), Some("6"));
    let petals = |n: &str| {
        state(
            BlockKind::PinkPetals,
            &[("flower_amount", n), ("facing", "north")],
        )
    };
    assert!(can_be_replaced(
        petals("3"),
        origin(),
        BlockKind::PinkPetals,
        &click(Face::Up),
        &block
    ));
    assert!(!can_be_replaced(
        petals("4"),
        origin(),
        BlockKind::PinkPetals,
        &click(Face::Up),
        &block
    ));
    let litter = |n: &str| {
        state(
            BlockKind::LeafLitter,
            &[("segment_amount", n), ("facing", "north")],
        )
    };
    assert!(can_be_replaced(
        litter("2"),
        origin(),
        BlockKind::LeafLitter,
        &click(Face::Up),
        &block
    ));
}

#[test]
fn glow_lichen_gains_a_face_toward_the_block_it_is_placed_against() {
    let level = Level::new()
        .put(0, 0, 0, BlockKind::Stone)
        .put(0, 1, -1, BlockKind::Stone)
        .with(
            0,
            1,
            0,
            state(
                BlockKind::GlowLichen,
                &[("down", "true"), ("waterlogged", "false")],
            ),
        );
    let block = level.at();
    let existing = block(origin());
    let looking_north = Click {
        look: Look {
            yaw: 180.0,
            pitch: 0.0,
        },
        replacing_clicked: true,
        ..click(Face::North)
    };
    assert!(can_be_replaced(
        existing,
        origin(),
        BlockKind::GlowLichen,
        &looking_north,
        &block
    ));
    let placed = adjust(
        BlockKind::GlowLichen.default_state(),
        origin(),
        &looking_north,
        &block,
    )
    .expect("a new face");
    assert_eq!(placed.state.property("north"), Some("true"));
    assert_eq!(placed.state.property("down"), Some("true"));
    let walled_in = Level::new()
        .with(0, 1, 0, existing)
        .put(0, 0, 0, BlockKind::Stone);
    let nowhere = Click {
        replacing_clicked: true,
        ..click(Face::Up)
    };
    let result = adjust(
        BlockKind::GlowLichen.default_state(),
        origin(),
        &nowhere,
        &walled_in.at(),
    );
    assert!(result.is_none());
}

#[test]
fn a_lantern_hangs_when_looked_up_and_stands_when_looked_down() {
    let level = Level::new()
        .put(0, 0, 0, BlockKind::Stone)
        .put(0, 2, 0, BlockKind::Stone);
    let block = level.at();
    let down = Click {
        look: Look {
            yaw: 0.0,
            pitch: 80.0,
        },
        replacing_clicked: true,
        ..click(Face::Up)
    };
    let up = Click {
        look: Look {
            yaw: 0.0,
            pitch: -80.0,
        },
        replacing_clicked: true,
        ..click(Face::Up)
    };
    let standing =
        adjust(BlockKind::Lantern.default_state(), origin(), &down, &block).expect("stands");
    let hanging = adjust(BlockKind::Lantern.default_state(), origin(), &up, &block).expect("hangs");
    assert_eq!(standing.state.property("hanging"), Some("false"));
    assert_eq!(hanging.state.property("hanging"), Some("true"));
    assert!(
        adjust(
            BlockKind::Lantern.default_state(),
            origin(),
            &down,
            &Level::new().at()
        )
        .is_none()
    );
}

#[test]
fn a_crafter_a_sculk_sensor_a_grindstone_and_a_bell_take_their_orientation() {
    let level = Level::new()
        .put(0, 0, 0, BlockKind::Stone)
        .put(0, 1, 1, BlockKind::Stone);
    let block = level.at();
    let south = Click {
        look: Look {
            yaw: 0.0,
            pitch: 0.0,
        },
        replacing_clicked: true,
        ..click(Face::Up)
    };
    let crafter =
        adjust(BlockKind::Crafter.default_state(), origin(), &south, &block).expect("crafter");
    assert_eq!(crafter.state.property("orientation"), Some("north_up"));
    let down = Click {
        look: Look {
            yaw: 0.0,
            pitch: 80.0,
        },
        replacing_clicked: true,
        ..click(Face::Up)
    };
    let crafter =
        adjust(BlockKind::Crafter.default_state(), origin(), &down, &block).expect("crafter");
    assert_eq!(crafter.state.property("orientation"), Some("up_south"));
    let sensor = adjust(
        BlockKind::CalibratedSculkSensor.default_state(),
        origin(),
        &south,
        &block,
    )
    .expect("sensor");
    assert_eq!(sensor.state.property("facing"), Some("north"));
    let grindstone = adjust(
        BlockKind::Grindstone.default_state(),
        origin(),
        &down,
        &block,
    )
    .expect("grindstone");
    assert_eq!(grindstone.state.property("face"), Some("floor"));
    let wall = Click {
        look: Look {
            yaw: 0.0,
            pitch: 0.0,
        },
        replacing_clicked: false,
        ..click(Face::North)
    };
    let on_wall = adjust(
        BlockKind::Grindstone.default_state(),
        origin(),
        &wall,
        &block,
    )
    .expect("grindstone on a wall");
    assert_eq!(on_wall.state.property("face"), Some("wall"));
    assert_eq!(on_wall.state.property("facing"), Some("north"));
    let bell = adjust(
        BlockKind::Bell.default_state(),
        origin(),
        &click(Face::Up),
        &block,
    )
    .expect("bell");
    assert_eq!(bell.state.property("attachment"), Some("floor"));
    let hung = Level::new().put(0, 2, 0, BlockKind::Stone);
    let bell = adjust(
        BlockKind::Bell.default_state(),
        origin(),
        &click(Face::Down),
        &hung.at(),
    )
    .expect("bell");
    assert_eq!(bell.state.property("attachment"), Some("ceiling"));
}

#[test]
fn cocoa_faces_the_jungle_log_and_vines_attach_to_the_wall() {
    let level = Level::new().put(1, 1, 0, BlockKind::JungleLog);
    let east = Click {
        look: Look {
            yaw: -90.0,
            pitch: 0.0,
        },
        replacing_clicked: true,
        ..click(Face::North)
    };
    let cocoa = adjust(
        BlockKind::Cocoa.default_state(),
        origin(),
        &east,
        &level.at(),
    )
    .expect("cocoa");
    assert_eq!(cocoa.state.property("facing"), Some("east"));
    let vines = Level::new().put(0, 1, -1, BlockKind::Stone);
    let click_north = Click {
        replacing_clicked: false,
        ..click(Face::North)
    };
    let vine = adjust(
        BlockKind::Vine.default_state(),
        origin(),
        &click_north,
        &vines.at(),
    )
    .expect("vine");
    assert_eq!(vine.state.property("north"), Some("true"));
    assert!(
        adjust(
            BlockKind::Vine.default_state(),
            origin(),
            &click_north,
            &Level::new().at()
        )
        .is_none()
    );
}

#[test]
fn a_tall_plant_places_its_upper_half_and_needs_room() {
    let level = Level::new().put(0, 0, 0, BlockKind::GrassBlock);
    let placed = adjust(
        BlockKind::Sunflower.default_state(),
        origin(),
        &click(Face::Up),
        &level.at(),
    )
    .expect("a sunflower");
    assert_eq!(placed.state.property("half"), Some("lower"));
    assert_eq!(
        placed.upper.and_then(|upper| upper.property("half")),
        Some("upper")
    );
    let ceiling = Level::new()
        .put(0, 0, 0, BlockKind::GrassBlock)
        .put(0, 2, 0, BlockKind::Stone);
    assert!(
        adjust(
            BlockKind::Sunflower.default_state(),
            origin(),
            &click(Face::Up),
            &ceiling.at()
        )
        .is_none()
    );
}

#[test]
fn a_block_placed_in_water_is_waterlogged_and_a_block_without_support_is_refused() {
    let level = Level::new()
        .put(0, 0, 0, BlockKind::Stone)
        .put(0, 1, 0, BlockKind::Water);
    let placed = adjust(slab("bottom"), origin(), &click(Face::Up), &level.at()).expect("a slab");
    assert_eq!(placed.state.property("waterlogged"), Some("true"));
    let dry = Level::new().put(0, 0, 0, BlockKind::Stone);
    let placed = adjust(slab("bottom"), origin(), &click(Face::Up), &dry.at()).expect("a slab");
    assert_eq!(placed.state.property("waterlogged"), Some("false"));
    assert!(
        adjust(
            BlockKind::Torch.default_state(),
            origin(),
            &click(Face::Up),
            &Level::new().at()
        )
        .is_none()
    );
}

#[test]
fn farmland_under_a_solid_block_turns_back_to_dirt() {
    let level = Level::new().put(0, 2, 0, BlockKind::Stone);
    let farmland = BlockKind::Farmland.default_state();
    assert_eq!(
        update_shape(farmland, origin(), Face::Up, &level.at()),
        Shaped::Changed(BlockKind::Dirt.default_state())
    );
    assert_eq!(
        update_shape(farmland, origin(), Face::Up, &Level::new().at()),
        Shaped::Same
    );
    let placed = adjust(farmland, origin(), &click(Face::Up), &level.at()).expect("placed");
    assert_eq!(placed.state.kind(), Some(BlockKind::Dirt));
}

#[test]
fn look_ordering_puts_the_looked_at_direction_first() {
    let order = Look {
        yaw: 0.0,
        pitch: 0.0,
    }
    .nearest();
    assert_eq!(order[0], Face::South);
    assert_eq!(order[5], Face::North);
    let order = Look {
        yaw: 0.0,
        pitch: -90.0,
    }
    .nearest();
    assert_eq!(order[0], Face::Up);
    let order = Look {
        yaw: 90.0,
        pitch: 0.0,
    }
    .nearest();
    assert_eq!(order[0], Face::West);
}

#[test]
fn a_mangrove_propagule_hangs_when_clicked_from_below_and_a_dripleaf_follows_its_stem() {
    let leaves = Level::new().put(0, 2, 0, BlockKind::MangroveLeaves);
    let hanging = adjust(
        BlockKind::MangrovePropagule.default_state(),
        origin(),
        &click(Face::Down),
        &leaves.at(),
    )
    .expect("a hanging propagule");
    assert_eq!(hanging.state.property("hanging"), Some("true"));
    let level = Level::new().put(0, 0, 0, BlockKind::Clay);
    let leaf = adjust(
        BlockKind::BigDripleaf.default_state(),
        origin(),
        &click(Face::Up),
        &level.at(),
    )
    .expect("a dripleaf");
    assert_eq!(leaf.state.property("facing"), Some("north"));
    let stem = Level::new().with(
        0,
        0,
        0,
        state(BlockKind::BigDripleafStem, &[("facing", "east")]),
    );
    let top = adjust(
        BlockKind::BigDripleaf.default_state(),
        origin(),
        &click(Face::Up),
        &stem.at(),
    );
    assert!(top.is_some_and(|placed| placed.state.property("facing") == Some("east")));
}

#[test]
fn a_sea_pickle_can_stand_on_a_full_stack_of_pickles_in_water() {
    let pickles = state(
        BlockKind::SeaPickle,
        &[("pickles", "4"), ("waterlogged", "true")],
    );
    let level = Level::new()
        .with(0, 0, 0, pickles)
        .put(0, 1, 0, BlockKind::Water);
    let placed = adjust(
        BlockKind::SeaPickle.default_state(),
        origin(),
        &click(Face::Up),
        &level.at(),
    );
    assert!(placed.is_some());
}

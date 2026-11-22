use std::collections::HashMap;

use rspigot_blocks::Face;

use super::support::{in_tag, rigid, step};
use crate::{BlockPos, BlockStateId};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RailChange {
    pub pos: BlockPos,
    pub state: BlockStateId,
}

struct World<'a, F: Fn(BlockPos) -> BlockStateId> {
    base: &'a F,
    changed: HashMap<BlockPos, BlockStateId>,
    order: Vec<BlockPos>,
}

impl<F: Fn(BlockPos) -> BlockStateId> World<'_, F> {
    fn get(&self, pos: BlockPos) -> BlockStateId {
        self.changed
            .get(&pos)
            .copied()
            .unwrap_or_else(|| (self.base)(pos))
    }

    fn set(&mut self, pos: BlockPos, state: BlockStateId) {
        if self.changed.insert(pos, state).is_none() {
            self.order.push(pos);
        }
    }

    fn is_rail(&self, pos: BlockPos) -> bool {
        in_tag(self.get(pos), "minecraft:rails")
    }

    fn rail_at(&self, pos: BlockPos) -> Option<Rail> {
        [pos, up(pos), down(pos)]
            .into_iter()
            .find(|at| self.is_rail(*at))
            .map(|at| Rail::load(at, self.get(at)))
    }
}

fn up(pos: BlockPos) -> BlockPos {
    pos.offset(0, 1, 0)
}

fn down(pos: BlockPos) -> BlockPos {
    pos.offset(0, -1, 0)
}

fn north(pos: BlockPos) -> BlockPos {
    pos.offset(0, 0, -1)
}

fn south(pos: BlockPos) -> BlockPos {
    pos.offset(0, 0, 1)
}

fn west(pos: BlockPos) -> BlockPos {
    pos.offset(-1, 0, 0)
}

fn east(pos: BlockPos) -> BlockPos {
    pos.offset(1, 0, 0)
}

struct Rail {
    pos: BlockPos,
    state: BlockStateId,
    straight: bool,
    connections: Vec<BlockPos>,
}

fn shape_of(state: BlockStateId) -> &'static str {
    state.property("shape").unwrap_or("north_south")
}

fn is_straight(state: BlockStateId) -> bool {
    state.kind().is_some_and(|kind| {
        kind.properties()
            .iter()
            .any(|property| property.name == "shape" && property.values.len() == 6)
    })
}

impl Rail {
    fn load(pos: BlockPos, state: BlockStateId) -> Self {
        let mut rail = Self {
            pos,
            state,
            straight: is_straight(state),
            connections: Vec::new(),
        };
        rail.update_connections(shape_of(state));
        rail
    }

    fn update_connections(&mut self, shape: &str) {
        let p = self.pos;
        self.connections = match shape {
            "east_west" => vec![west(p), east(p)],
            "ascending_east" => vec![west(p), up(east(p))],
            "ascending_west" => vec![up(west(p)), east(p)],
            "ascending_north" => vec![up(north(p)), south(p)],
            "ascending_south" => vec![north(p), up(south(p))],
            "south_east" => vec![east(p), south(p)],
            "south_west" => vec![west(p), south(p)],
            "north_west" => vec![west(p), north(p)],
            "north_east" => vec![east(p), north(p)],
            _ => vec![north(p), south(p)],
        };
    }

    fn has_connection(&self, pos: BlockPos) -> bool {
        self.connections
            .iter()
            .any(|c| c.x == pos.x && c.z == pos.z)
    }

    fn connects_to(&self, other: &Self) -> bool {
        self.has_connection(other.pos)
    }

    fn can_connect_to(&self, other: &Self) -> bool {
        self.connects_to(other) || self.connections.len() != 2
    }

    fn remove_soft_connections<F: Fn(BlockPos) -> BlockStateId>(&mut self, world: &World<F>) {
        let mut kept = Vec::new();
        for connection in std::mem::take(&mut self.connections) {
            if let Some(rail) = world.rail_at(connection)
                && rail.connects_to(self)
            {
                kept.push(rail.pos);
            }
        }
        self.connections = kept;
    }

    fn has_neighbour_rail<F: Fn(BlockPos) -> BlockStateId>(
        &self,
        world: &World<F>,
        pos: BlockPos,
    ) -> bool {
        let Some(mut neighbour) = world.rail_at(pos) else {
            return false;
        };
        neighbour.remove_soft_connections(world);
        neighbour.can_connect_to(self)
    }

    fn connect_to<F: Fn(BlockPos) -> BlockStateId>(&mut self, world: &mut World<F>, other: &Self) {
        self.connections.push(other.pos);
        let (n, s, w, e) = (
            self.has_connection(north(self.pos)),
            self.has_connection(south(self.pos)),
            self.has_connection(west(self.pos)),
            self.has_connection(east(self.pos)),
        );
        let mut shape = None;
        if n || s {
            shape = Some("north_south");
        }
        if w || e {
            shape = Some("east_west");
        }
        if !self.straight {
            if s && e && !n && !w {
                shape = Some("south_east");
            }
            if s && w && !n && !e {
                shape = Some("south_west");
            }
            if n && w && !s && !e {
                shape = Some("north_west");
            }
            if n && e && !s && !w {
                shape = Some("north_east");
            }
        }
        if shape == Some("north_south") {
            if world.is_rail(up(north(self.pos))) {
                shape = Some("ascending_north");
            }
            if world.is_rail(up(south(self.pos))) {
                shape = Some("ascending_south");
            }
        }
        if shape == Some("east_west") {
            if world.is_rail(up(east(self.pos))) {
                shape = Some("ascending_east");
            }
            if world.is_rail(up(west(self.pos))) {
                shape = Some("ascending_west");
            }
        }
        let shape = shape.unwrap_or("north_south");
        self.state = self
            .state
            .with_property("shape", shape)
            .unwrap_or(self.state);
        world.set(self.pos, self.state);
    }

    fn place<F: Fn(BlockPos) -> BlockStateId>(
        &mut self,
        world: &mut World<F>,
        powered: bool,
        first: bool,
        current: &str,
    ) {
        let p = self.pos;
        let n = self.has_neighbour_rail(world, north(p));
        let s = self.has_neighbour_rail(world, south(p));
        let w = self.has_neighbour_rail(world, west(p));
        let e = self.has_neighbour_rail(world, east(p));
        let mut shape = None;
        if (n || s) && !w && !e {
            shape = Some("north_south");
        }
        if (w || e) && !n && !s {
            shape = Some("east_west");
        }
        if !self.straight {
            if s && e && !n && !w {
                shape = Some("south_east");
            }
            if s && w && !n && !e {
                shape = Some("south_west");
            }
            if n && e && !s && !w {
                shape = Some("north_east");
            }
            if n && w && !s && !e {
                shape = Some("north_west");
            }
        }
        if shape.is_none() {
            if n || s {
                shape = Some("north_south");
            }
            if w || e {
                shape = Some("east_west");
            }
            if !self.straight {
                if powered {
                    if s && e {
                        shape = Some("south_east");
                    }
                    if w && s {
                        shape = Some("south_west");
                    }
                    if e && n {
                        shape = Some("north_east");
                    }
                    if n && w {
                        shape = Some("north_west");
                    }
                } else {
                    if n && w {
                        shape = Some("north_west");
                    }
                    if e && n {
                        shape = Some("north_east");
                    }
                    if w && s {
                        shape = Some("south_west");
                    }
                    if s && e {
                        shape = Some("south_east");
                    }
                }
            }
        }
        if shape == Some("north_south") {
            if world.is_rail(up(north(p))) {
                shape = Some("ascending_north");
            }
            if world.is_rail(up(south(p))) {
                shape = Some("ascending_south");
            }
        }
        if shape == Some("east_west") {
            if world.is_rail(up(east(p))) {
                shape = Some("ascending_east");
            }
            if world.is_rail(up(west(p))) {
                shape = Some("ascending_west");
            }
        }
        let shape = shape.unwrap_or(current);
        self.update_connections(shape);
        self.state = self
            .state
            .with_property("shape", shape)
            .unwrap_or(self.state);
        if first || world.get(p) != self.state {
            world.set(p, self.state);
            for connection in self.connections.clone() {
                if let Some(mut neighbour) = world.rail_at(connection) {
                    neighbour.remove_soft_connections(world);
                    if neighbour.can_connect_to(self) {
                        neighbour.connect_to(world, self);
                    }
                }
            }
        }
    }
}

pub fn is_rail(state: BlockStateId) -> bool {
    in_tag(state, "minecraft:rails")
}

pub fn rail_placed(
    pos: BlockPos,
    state: BlockStateId,
    powered: bool,
    block: &impl Fn(BlockPos) -> BlockStateId,
) -> Vec<RailChange> {
    let mut world = World {
        base: block,
        changed: HashMap::new(),
        order: Vec::new(),
    };
    let current = shape_of(state);
    Rail::load(pos, state).place(&mut world, powered, true, current);
    world
        .order
        .iter()
        .filter(|at| world.changed[*at] != block(**at))
        .map(|at| RailChange {
            pos: *at,
            state: world.changed[at],
        })
        .collect()
}

pub fn rail_removal_needed(
    state: BlockStateId,
    pos: BlockPos,
    block: &impl Fn(BlockPos) -> BlockStateId,
) -> bool {
    if !rigid(block(step(pos, Face::Down))) {
        return true;
    }
    let raised = match shape_of(state) {
        "ascending_east" => Some(Face::East),
        "ascending_west" => Some(Face::West),
        "ascending_north" => Some(Face::North),
        "ascending_south" => Some(Face::South),
        _ => None,
    };
    raised.is_some_and(|face| !rigid(block(step(pos, face))))
}

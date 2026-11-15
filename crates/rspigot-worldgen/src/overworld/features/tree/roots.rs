use rspigot_domain::BlockStateId;
use serde_json::Value;

use super::{HORIZONTAL, Site};
use crate::climate::Xoroshiro;
use crate::overworld::features::prov::{
    BlockSet, Int, Pos, State, integer, is_water, number, short,
};

#[derive(Debug, Clone)]
struct Above {
    chance: f32,
    provider: State,
}

#[derive(Debug, Clone)]
struct Mangrove {
    through: BlockSet,
    max_length: usize,
    max_width: i32,
    muddy_in: BlockSet,
    muddy: State,
    skew: f32,
}

#[derive(Debug, Clone)]
pub(super) struct Root {
    offset: Int,
    provider: State,
    above: Option<Above>,
    mangrove: Mangrove,
}

impl Root {
    pub(super) fn compile(value: &Value) -> Option<Self> {
        if short(value) != "mangrove_root_placer" {
            return None;
        }
        let placement = value.get("mangrove_root_placement")?;
        let null = Value::Null;
        Some(Self {
            offset: Int::compile(value.get("trunk_offset_y").unwrap_or(&null)),
            provider: State::compile(value.get("root_provider").unwrap_or(&null)),
            above: value.get("above_root_placement").map(|above| Above {
                chance: number(above, "above_root_placement_chance", 0.0) as f32,
                provider: State::compile(above.get("above_root_provider").unwrap_or(&null)),
            }),
            mangrove: Mangrove {
                through: BlockSet::compile(placement.get("can_grow_through").unwrap_or(&null)),
                max_length: integer(placement, "max_root_length", 15).max(0) as usize,
                max_width: integer(placement, "max_root_width", 8),
                muddy_in: BlockSet::compile(placement.get("muddy_roots_in").unwrap_or(&null)),
                muddy: State::compile(placement.get("muddy_roots_provider").unwrap_or(&null)),
                skew: number(placement, "random_skew_chance", 0.0) as f32,
            },
        })
    }

    pub(super) fn offset(&self, rng: &mut Xoroshiro) -> i32 {
        self.offset.sample(rng)
    }
}

fn below((x, y, z): Pos) -> Pos {
    (x, y - 1, z)
}

fn relative((x, y, z): Pos, (dx, dz): (i32, i32)) -> Pos {
    (x + dx, y, z + dz)
}

fn manhattan(a: Pos, b: Pos) -> i32 {
    (a.0 - b.0).abs() + (a.1 - b.1).abs() + (a.2 - b.2).abs()
}

impl Site<'_, '_, '_> {
    fn root_config(&self) -> Option<&Root> {
        self.tree.root.as_ref()
    }

    fn can_place_root(&mut self, pos: Pos) -> bool {
        let state = self.run.region.get(pos.0, pos.1, pos.2);
        super::replaceable_by_trees(state)
            || self
                .root_config()
                .is_some_and(|root| root.mangrove.through.contains(state))
    }

    pub(super) fn place_roots(&mut self, origin: Pos, trunk_origin: Pos) -> bool {
        let mut cursor = origin;
        while cursor.1 < trunk_origin.1 {
            if !self.can_place_root(cursor) {
                return false;
            }
            cursor.1 += 1;
        }
        let mut positions = vec![below(trunk_origin)];
        for direction in HORIZONTAL {
            let start = relative(trunk_origin, direction);
            let mut found = Vec::new();
            if !self.simulate_roots(start, direction, trunk_origin, &mut found, 0) {
                return false;
            }
            positions.extend(found);
            positions.push(start);
        }
        for pos in positions {
            self.place_root(pos);
        }
        true
    }

    fn simulate_roots(
        &mut self,
        pos: Pos,
        direction: (i32, i32),
        origin: Pos,
        found: &mut Vec<Pos>,
        layer: usize,
    ) -> bool {
        let Some(max_length) = self.root_config().map(|root| root.mangrove.max_length) else {
            return false;
        };
        if layer == max_length || found.len() > max_length {
            return false;
        }
        for candidate in self.potential_roots(pos, direction, origin) {
            if self.can_place_root(candidate) {
                found.push(candidate);
                if !self.simulate_roots(candidate, direction, origin, found, layer + 1) {
                    return false;
                }
            }
        }
        true
    }

    fn potential_roots(&mut self, pos: Pos, direction: (i32, i32), origin: Pos) -> Vec<Pos> {
        let Some((width, skew)) = self
            .root_config()
            .map(|root| (root.mangrove.max_width, root.mangrove.skew))
        else {
            return Vec::new();
        };
        let down = below(pos);
        let side = relative(pos, direction);
        let distance = manhattan(pos, origin);
        if distance > width - 3 && distance <= width {
            if self.run.rng.next_float() < skew {
                vec![down, below(side)]
            } else {
                vec![down]
            }
        } else if distance > width || self.run.rng.next_float() < skew {
            vec![down]
        } else if self.run.rng.next_boolean() {
            vec![side]
        } else {
            vec![down]
        }
    }

    fn waterlogged(&mut self, pos: Pos, state: BlockStateId) -> BlockStateId {
        if state.property("waterlogged").is_none() {
            return state;
        }
        let wet = is_water(self.run.region.get(pos.0, pos.1, pos.2));
        state
            .with_property("waterlogged", if wet { "true" } else { "false" })
            .unwrap_or(state)
    }

    fn put_root(&mut self, pos: Pos, state: BlockStateId) {
        self.run.region.set(pos.0, pos.1, pos.2, state);
        self.roots.push(pos);
    }

    fn place_root(&mut self, pos: Pos) {
        let tree = self.tree;
        let Some(root) = tree.root.as_ref() else {
            return;
        };
        let here = self.run.region.get(pos.0, pos.1, pos.2);
        if root.mangrove.muddy_in.contains(here) {
            if let Some(state) = root
                .mangrove
                .muddy
                .resolve(self.run.region, self.run.rng, pos)
            {
                let state = self.waterlogged(pos, state);
                self.put_root(pos, state);
            }
            return;
        }
        if !self.can_place_root(pos) {
            return;
        }
        if let Some(state) = root.provider.resolve(self.run.region, self.run.rng, pos) {
            let state = self.waterlogged(pos, state);
            self.put_root(pos, state);
        }
        let Some(above) = &root.above else {
            return;
        };
        let up = (pos.0, pos.1 + 1, pos.2);
        if self.run.rng.next_float() < above.chance
            && self.run.region.get(up.0, up.1, up.2).is_air()
            && let Some(state) = above.provider.resolve(self.run.region, self.run.rng, up)
        {
            let state = self.waterlogged(up, state);
            self.put_root(up, state);
        }
    }
}

use std::sync::OnceLock;

use rspigot_domain::BlockStateId;
use serde_json::Value;

use super::place::Run;
use super::prov::{BlockSet, Pos, Pred, State, integer};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Red,
    Brown,
}

#[derive(Debug, Clone)]
pub(crate) struct Mushroom {
    kind: Kind,
    cap: State,
    stem: State,
    radius: i32,
    can_place_on: Pred,
}

fn replaceable(state: BlockStateId) -> bool {
    static SET: OnceLock<BlockSet> = OnceLock::new();
    state.is_air()
        || SET
            .get_or_init(|| BlockSet::tag("minecraft:replaceable_by_mushrooms"))
            .contains(state)
}

fn leaves(state: BlockStateId) -> bool {
    static SET: OnceLock<BlockSet> = OnceLock::new();
    SET.get_or_init(|| BlockSet::tag("minecraft:leaves"))
        .contains(state)
}

fn flag(value: bool) -> &'static str {
    if value { "true" } else { "false" }
}

impl Mushroom {
    pub(crate) fn compile(value: &Value, red: bool) -> Self {
        Self {
            kind: if red { Kind::Red } else { Kind::Brown },
            cap: State::compile(value.get("cap_provider").unwrap_or(&Value::Null)),
            stem: State::compile(value.get("stem_provider").unwrap_or(&Value::Null)),
            radius: integer(value, "foliage_radius", 2),
            can_place_on: Pred::compile(value.get("can_place_on").unwrap_or(&Value::Null)),
        }
    }

    pub(crate) fn reach(&self) -> i32 {
        self.radius
    }

    fn validation_radius(&self, yo: i32) -> i32 {
        match self.kind {
            Kind::Red => 0,
            Kind::Brown if yo < 4 => 0,
            Kind::Brown => self.radius,
        }
    }

    fn valid(&self, run: &mut Run, (x, y, z): Pos, height: i32) -> bool {
        let bounds = run.region.height();
        if y < bounds.min_y + 1 || y + height + 1 > bounds.max_y() {
            return false;
        }
        if !self.can_place_on.test(run.region, (x, y - 1, z)) {
            return false;
        }
        for yo in 0..=height {
            let radius = self.validation_radius(yo);
            for dx in -radius..=radius {
                for dz in -radius..=radius {
                    let state = run.region.get(x + dx, y + yo, z + dz);
                    if !state.is_air() && !leaves(state) {
                        return false;
                    }
                }
            }
        }
        true
    }

    fn put(run: &mut Run, (x, y, z): Pos, state: BlockStateId) {
        if replaceable(run.region.get(x, y, z)) {
            run.region.set(x, y, z, state);
        }
    }

    fn cap_state(&self, run: &mut Run, origin: Pos, sides: &[(&str, bool)]) -> BlockStateId {
        let state = self
            .cap
            .resolve(run.region, run.rng, origin)
            .unwrap_or(BlockStateId::AIR);
        if sides.iter().any(|(name, _)| state.property(name).is_none()) {
            return state;
        }
        sides.iter().fold(state, |state, (name, value)| {
            state.with_property(name, flag(*value)).unwrap_or(state)
        })
    }

    fn red_cap(&self, run: &mut Run, origin: Pos, height: i32) {
        let (x, y, z) = origin;
        let inner = self.radius - 2;
        for yo in (height - 3)..=height {
            let radius = if yo < height {
                self.radius
            } else {
                self.radius - 1
            };
            for dx in -radius..=radius {
                for dz in -radius..=radius {
                    let edge_x = dx == -radius || dx == radius;
                    let edge_z = dz == -radius || dz == radius;
                    if height > yo && edge_x == edge_z {
                        continue;
                    }
                    let state = self.cap_state(
                        run,
                        origin,
                        &[
                            ("up", height - 1 <= yo),
                            ("west", dx < -inner),
                            ("east", inner < dx),
                            ("north", dz < -inner),
                            ("south", inner < dz),
                        ],
                    );
                    Self::put(run, (x + dx, y + yo, z + dz), state);
                }
            }
        }
    }

    fn brown_cap(&self, run: &mut Run, origin: Pos, height: i32) {
        let (x, y, z) = origin;
        let radius = self.radius;
        for dx in -radius..=radius {
            for dz in -radius..=radius {
                let (west_edge, east_edge) = (dx == -radius, dx == radius);
                let (north_edge, south_edge) = (dz == -radius, dz == radius);
                let edge_x = west_edge || east_edge;
                let edge_z = north_edge || south_edge;
                if edge_x && edge_z {
                    continue;
                }
                let state = self.cap_state(
                    run,
                    origin,
                    &[
                        ("west", west_edge || (edge_z && dx == 1 - radius)),
                        ("east", east_edge || (edge_z && dx == radius - 1)),
                        ("north", north_edge || (edge_x && dz == 1 - radius)),
                        ("south", south_edge || (edge_x && dz == radius - 1)),
                    ],
                );
                Self::put(run, (x + dx, y + height, z + dz), state);
            }
        }
    }

    pub(crate) fn place(&self, run: &mut Run, origin: Pos) -> bool {
        let mut height = run.rng.below(3) + 4;
        if run.rng.below(12) == 0 {
            height *= 2;
        }
        if !self.valid(run, origin, height) {
            return false;
        }
        match self.kind {
            Kind::Red => self.red_cap(run, origin, height),
            Kind::Brown => self.brown_cap(run, origin, height),
        }
        for yo in 0..height {
            let at = (origin.0, origin.1 + yo, origin.2);
            let state = self
                .stem
                .resolve(run.region, run.rng, origin)
                .unwrap_or(BlockStateId::AIR);
            Self::put(run, at, state);
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::super::probe::{GRASS, fingerprint, world};
    use super::super::tests::Bench;

    const MYCELIUM: &str = "68:stone,69:mycelium";
    const STONE: &str = "69:stone";

    const GRASS_REFERENCE: [(&str, i64, usize, u64); 22] = [
        ("huge_red_mushroom", 1, 50, 0x2e74b06b0723bff5),
        ("huge_red_mushroom", 2, 50, 0x2e74b06b0723bff5),
        ("huge_red_mushroom", 3, 51, 0xa712b2102507f7e9),
        ("huge_red_mushroom", 4, 50, 0x2e74b06b0723bff5),
        ("huge_red_mushroom", 5, 50, 0x2e74b06b0723bff5),
        ("huge_red_mushroom", 6, 49, 0xa082d8709eb4cf6c),
        ("huge_red_mushroom", 7, 51, 0xa712b2102507f7e9),
        ("huge_red_mushroom", 8, 49, 0xa082d8709eb4cf6c),
        ("huge_red_mushroom", 16, 57, 0xd1ab30bfacbdef19),
        ("huge_red_mushroom", 28, 57, 0xd1ab30bfacbdef19),
        ("huge_red_mushroom", 78, 57, 0xd1ab30bfacbdef19),
        ("huge_brown_mushroom", 1, 50, 0xdc4f2c92d8288346),
        ("huge_brown_mushroom", 2, 50, 0xdc4f2c92d8288346),
        ("huge_brown_mushroom", 3, 51, 0x2c0b9c34d2bf2e88),
        ("huge_brown_mushroom", 4, 50, 0xdc4f2c92d8288346),
        ("huge_brown_mushroom", 5, 50, 0xdc4f2c92d8288346),
        ("huge_brown_mushroom", 6, 49, 0x8f764ec606b4a203),
        ("huge_brown_mushroom", 7, 51, 0x2c0b9c34d2bf2e88),
        ("huge_brown_mushroom", 8, 49, 0x8f764ec606b4a203),
        ("huge_brown_mushroom", 16, 57, 0xba7c394eda8be6d6),
        ("huge_brown_mushroom", 28, 57, 0xba7c394eda8be6d6),
        ("huge_brown_mushroom", 78, 57, 0xba7c394eda8be6d6),
    ];

    const MYCELIUM_REFERENCE: [(&str, i64, usize, u64); 6] = [
        ("huge_brown_mushroom", 1, 50, 0xdc4f2c92d8288346),
        ("huge_brown_mushroom", 2, 50, 0xdc4f2c92d8288346),
        ("huge_brown_mushroom", 16, 57, 0xba7c394eda8be6d6),
        ("huge_red_mushroom", 1, 50, 0x2e74b06b0723bff5),
        ("huge_red_mushroom", 2, 50, 0x2e74b06b0723bff5),
        ("huge_red_mushroom", 16, 57, 0xd1ab30bfacbdef19),
    ];

    #[test]
    fn huge_mushrooms_match_the_vanilla_feature_cell_for_cell() {
        let bench = Bench::new();
        for (spec, table) in [
            (GRASS, GRASS_REFERENCE.as_slice()),
            (MYCELIUM, MYCELIUM_REFERENCE.as_slice()),
        ] {
            for (feature, seed, count, hash) in table {
                assert_eq!(
                    fingerprint(&bench, spec, feature, (8, 70, 8), *seed, (40, 130)),
                    (*count, *hash),
                    "{feature} seed {seed}"
                );
            }
        }
    }

    #[test]
    fn huge_mushrooms_refuse_ground_outside_their_tag() {
        let bench = Bench::new();
        for feature in ["huge_red_mushroom", "huge_brown_mushroom"] {
            assert_eq!(
                fingerprint(&bench, STONE, feature, (8, 70, 8), 1, (40, 130)),
                (0, 0xcbf2_9ce4_8422_2325),
                "{feature}"
            );
        }
    }

    #[test]
    fn huge_mushrooms_need_room_above_the_ground() {
        let bench = Bench::new();
        let log = rspigot_blocks::BlockKind::from_name("minecraft:oak_log")
            .map_or(rspigot_domain::BlockStateId::AIR, |kind| {
                kind.default_state()
            });
        for (feature, at) in [
            ("huge_red_mushroom", (8, 73, 8)),
            ("huge_brown_mushroom", (10, 74, 9)),
        ] {
            let mut region = world(&bench, GRASS);
            region.set(at.0, at.1, at.2, log);
            assert!(
                !bench.place(&mut region, feature, (8, 70, 8), 1),
                "{feature}"
            );
        }
    }
}

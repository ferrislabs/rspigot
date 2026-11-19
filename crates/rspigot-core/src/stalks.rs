use rand::Rng;
use rspigot_blocks::{BlockKind, Face, Support};
use rspigot_domain::{BlockPos, BlockStateId};

use crate::plugins::Lane;
use crate::plugins::steps::Nature;
use crate::ports::ClientLink;
use crate::server::Server;

const REED_MAX_HEIGHT: i32 = 3;
const REED_MAX_AGE: i32 = 15;
const HEAD_MAX_AGE: i32 = 25;
const BAMBOO_MAX_HEIGHT: i32 = 16;
const BAMBOO_LIGHT: i32 = 9;
const BAMBOO_GROW_ODDS: i32 = 3;
const BAMBOO_TALL_START: i32 = 11;
const BAMBOO_TALL_CHANCE: f32 = 0.25;
const BERRY_CHANCE: f32 = 0.11;
const VINE_SPREAD_ODDS: i32 = 4;
const VINE_SPREAD_RANGE: i32 = 4;
const VINE_SPREAD_LIMIT: i32 = 5;
const VINE_TOP_CHANCE: f32 = 0.05;

#[derive(Clone, Copy)]
struct Plant {
    chance: f64,
    dy: i32,
    body: BlockKind,
    in_water: bool,
}

fn plant_of(kind: BlockKind) -> Option<Plant> {
    let (chance, dy, body, in_water) = match kind {
        BlockKind::Kelp => (0.14, 1, BlockKind::KelpPlant, true),
        BlockKind::TwistingVines => (0.1, 1, BlockKind::TwistingVinesPlant, false),
        BlockKind::WeepingVines => (0.1, -1, BlockKind::WeepingVinesPlant, false),
        BlockKind::CaveVines => (0.1, -1, BlockKind::CaveVinesPlant, false),
        _ => return None,
    };
    Some(Plant {
        chance,
        dy,
        body,
        in_water,
    })
}

fn number(state: BlockStateId, name: &str) -> i32 {
    state
        .property(name)
        .and_then(|value| value.parse().ok())
        .unwrap_or(0)
}

fn with_number(state: BlockStateId, name: &str, value: i32) -> BlockStateId {
    state
        .with_property(name, &value.to_string())
        .unwrap_or(state)
}

fn at(pos: BlockPos, face: Face) -> BlockPos {
    let (dx, dy, dz) = face.offset();
    pos.offset(dx, dy, dz)
}

fn side(face: Face) -> &'static str {
    match face {
        Face::North => "north",
        Face::East => "east",
        Face::South => "south",
        Face::West => "west",
        Face::Up => "up",
        Face::Down => "down",
    }
}

fn clockwise(face: Face) -> Face {
    match face {
        Face::North => Face::East,
        Face::East => Face::South,
        Face::South => Face::West,
        _ => Face::North,
    }
}

fn counter_clockwise(face: Face) -> Face {
    clockwise(clockwise(clockwise(face)))
}

fn has_side(state: BlockStateId, face: Face) -> bool {
    state.property(side(face)) == Some("true")
}

fn with_side(state: BlockStateId, face: Face) -> BlockStateId {
    state.with_property(side(face), "true").unwrap_or(state)
}

fn without_side(state: BlockStateId, face: Face) -> BlockStateId {
    state.with_property(side(face), "false").unwrap_or(state)
}

fn horizontal_connection(state: BlockStateId) -> bool {
    Face::HORIZONTAL.iter().any(|face| has_side(state, *face))
}

fn vine_on(face: Face) -> BlockStateId {
    with_side(BlockKind::Vine.default_state(), face)
}

impl<L: ClientLink> Server<L> {
    pub(crate) fn random_tick_stalk(&mut self, pos: BlockPos, state: BlockStateId) -> bool {
        let Some(kind) = state.kind() else {
            return false;
        };
        match kind {
            BlockKind::SugarCane | BlockKind::Cactus => self.reed_tick(pos, state, kind),
            BlockKind::Bamboo => self.bamboo_tick(pos, state),
            BlockKind::BambooSapling => self.bamboo_sapling_tick(pos),
            BlockKind::Vine => self.vine_tick(pos, state),
            _ => match plant_of(kind) {
                Some(plant) => self.plant_head_tick(pos, state, plant),
                None => return false,
            },
        }
        true
    }

    fn column_height(&self, pos: BlockPos, kind: BlockKind, limit: i32) -> i32 {
        let mut height = 1;
        while height < limit && self.block_or_air(pos.offset(0, -height, 0)).kind() == Some(kind) {
            height += 1;
        }
        height
    }

    fn reed_tick(&mut self, pos: BlockPos, state: BlockStateId, kind: BlockKind) {
        let above = at(pos, Face::Up);
        if !self.block_or_air(above).is_air() {
            return;
        }
        if self.column_height(pos, kind, REED_MAX_HEIGHT) >= REED_MAX_HEIGHT {
            return;
        }
        let age = number(state, "age");
        if age == REED_MAX_AGE {
            self.grow_block(Lane::Block(above), above, kind.default_state());
            self.set_block(pos, with_number(state, "age", 0));
        } else {
            self.set_block(pos, with_number(state, "age", age + 1));
        }
    }

    fn bamboo_tick(&mut self, pos: BlockPos, state: BlockStateId) {
        if number(state, "stage") != 0 || self.rng.random_range(0..BAMBOO_GROW_ODDS) != 0 {
            return;
        }
        let above = at(pos, Face::Up);
        if !self.block_or_air(above).is_air() || self.raw_brightness(above) < BAMBOO_LIGHT {
            return;
        }
        let height = self.column_height(pos, BlockKind::Bamboo, BAMBOO_MAX_HEIGHT);
        if height < BAMBOO_MAX_HEIGHT {
            self.grow_bamboo(pos, state, above, height);
        }
    }

    fn grow_bamboo(&mut self, pos: BlockPos, state: BlockStateId, above: BlockPos, height: i32) {
        let below_pos = at(pos, Face::Down);
        let below = self.block_or_air(below_pos);
        let far_pos = below_pos.offset(0, -1, 0);
        let far = self.block_or_air(far_pos);
        let below_leafy =
            below.kind() == Some(BlockKind::Bamboo) && below.property("leaves") != Some("none");
        let far_bamboo = far.kind() == Some(BlockKind::Bamboo);
        let leaves = if height < 1 {
            "none"
        } else if below_leafy {
            "large"
        } else {
            "small"
        };
        let age = i32::from(number(state, "age") == 1 || far_bamboo);
        let stage = i32::from(
            (height >= BAMBOO_TALL_START && self.rng.random::<f32>() < BAMBOO_TALL_CHANCE)
                || height == BAMBOO_MAX_HEIGHT - 1,
        );
        let grown = with_number(
            with_number(BlockKind::Bamboo.default_state(), "age", age),
            "stage",
            stage,
        )
        .with_property("leaves", leaves)
        .unwrap_or(state);
        let mut companions = Vec::new();
        if leaves == "large" && far_bamboo {
            if let Some(small) = below.with_property("leaves", "small") {
                companions.push((below_pos, small));
            }
            if let Some(none) = far.with_property("leaves", "none") {
                companions.push((far_pos, none));
            }
        }
        self.grow_block_with(Lane::Block(above), above, grown, companions);
    }

    fn bamboo_sapling_tick(&mut self, pos: BlockPos) {
        if self.rng.random_range(0..BAMBOO_GROW_ODDS) != 0 {
            return;
        }
        let above = at(pos, Face::Up);
        if self.block_or_air(above).is_air() && self.raw_brightness(above) >= BAMBOO_LIGHT {
            self.grow_block(Lane::Block(above), above, BlockKind::Bamboo.default_state());
        }
    }

    fn plant_head_tick(&mut self, pos: BlockPos, state: BlockStateId, plant: Plant) {
        let age = number(state, "age");
        if age >= HEAD_MAX_AGE || self.rng.random::<f64>() >= plant.chance {
            return;
        }
        let target = pos.offset(0, plant.dy, 0);
        let into = self.block_or_air(target);
        let free = if plant.in_water {
            into.kind() == Some(BlockKind::Water)
        } else {
            into.is_air()
        };
        if !free {
            return;
        }
        let mut grown = with_number(state, "age", age + 1);
        if state.property("berries").is_some() {
            let berries = if self.rng.random::<f32>() < BERRY_CHANCE {
                "true"
            } else {
                "false"
            };
            grown = grown.with_property("berries", berries).unwrap_or(grown);
        }
        let mut body = plant.body.default_state();
        if let Some(berries) = state.property("berries") {
            body = body.with_property("berries", berries).unwrap_or(body);
        }
        self.grow_block_with(Lane::Block(target), target, grown, vec![(pos, body)]);
    }

    fn vine_supports(&self, pos: BlockPos, face: Face) -> bool {
        if face == Face::Down {
            return false;
        }
        let neighbour = self.block_or_air(at(pos, face));
        if neighbour.is_face_sturdy(face.opposite(), Support::Full) {
            return true;
        }
        if face == Face::Up {
            return false;
        }
        let above = self.block_or_air(at(pos, Face::Up));
        above.kind() == Some(BlockKind::Vine) && has_side(above, face)
    }

    fn vine_can_spread(&self, pos: BlockPos) -> bool {
        let mut left = VINE_SPREAD_LIMIT;
        for dx in -VINE_SPREAD_RANGE..=VINE_SPREAD_RANGE {
            for dy in -1..=1 {
                for dz in -VINE_SPREAD_RANGE..=VINE_SPREAD_RANGE {
                    if self.block_or_air(pos.offset(dx, dy, dz)).kind() == Some(BlockKind::Vine) {
                        left -= 1;
                        if left <= 0 {
                            return false;
                        }
                    }
                }
            }
        }
        true
    }

    fn vine_place(&mut self, target: BlockPos, state: BlockStateId, source: BlockPos) {
        self.nature_change(Nature::Spread, target, state, source);
    }

    fn vine_tick(&mut self, pos: BlockPos, state: BlockStateId) {
        if self.rng.random_range(0..VINE_SPREAD_ODDS) != 0 {
            return;
        }
        let direction = Face::ALL[self.rng.random_range(0..Face::ALL.len())];
        if direction != Face::Up && direction != Face::Down && !has_side(state, direction) {
            self.vine_spread_sideways(pos, state, direction);
        } else {
            self.vine_spread_vertically(pos, state, direction);
        }
    }

    fn vine_spread_sideways(&mut self, pos: BlockPos, state: BlockStateId, direction: Face) {
        if !self.vine_can_spread(pos) {
            return;
        }
        let target = at(pos, direction);
        if !self.block_or_air(target).is_air() {
            if self.vine_supports(target, direction) {
                self.vine_place(pos, with_side(state, direction), pos);
            }
            return;
        }
        let cw = clockwise(direction);
        let ccw = counter_clockwise(direction);
        let has_cw = has_side(state, cw);
        let has_ccw = has_side(state, ccw);
        let cw_pos = at(target, cw);
        let ccw_pos = at(target, ccw);
        if has_cw && self.vine_supports(cw_pos, cw) {
            self.vine_place(target, vine_on(cw), pos);
        } else if has_ccw && self.vine_supports(ccw_pos, ccw) {
            self.vine_place(target, vine_on(ccw), pos);
        } else {
            let opposite = direction.opposite();
            if has_cw
                && self.block_or_air(cw_pos).is_air()
                && self.vine_supports(at(pos, cw), opposite)
            {
                self.vine_place(cw_pos, vine_on(opposite), pos);
            } else if has_ccw
                && self.block_or_air(ccw_pos).is_air()
                && self.vine_supports(at(pos, ccw), opposite)
            {
                self.vine_place(ccw_pos, vine_on(opposite), pos);
            } else if self.rng.random::<f32>() < VINE_TOP_CHANCE
                && self.vine_supports(at(target, Face::Up), Face::Up)
            {
                self.vine_place(target, vine_on(Face::Up), pos);
            }
        }
    }

    fn vine_spread_vertically(&mut self, pos: BlockPos, state: BlockStateId, direction: Face) {
        let height = self.world.dimension.height;
        let top = height.min_y + height.height as i32;
        if direction == Face::Up && pos.y < top - 1 {
            if self.vine_supports(pos, Face::Up) {
                if !has_side(state, Face::Up) {
                    self.vine_place(pos, with_side(state, Face::Up), pos);
                }
                return;
            }
            let above = at(pos, Face::Up);
            if self.block_or_air(above).is_air() {
                if !self.vine_can_spread(pos) {
                    return;
                }
                let mut grown = state;
                for face in Face::HORIZONTAL {
                    if self.rng.random::<bool>() || !self.vine_supports(at(above, face), face) {
                        grown = without_side(grown, face);
                    }
                }
                if horizontal_connection(grown) {
                    self.vine_place(above, grown, pos);
                }
                return;
            }
        }
        if pos.y > height.min_y {
            let below = at(pos, Face::Down);
            let below_state = self.block_or_air(below);
            if below_state.is_air() || below_state.kind() == Some(BlockKind::Vine) {
                let source = if below_state.is_air() {
                    BlockKind::Vine.default_state()
                } else {
                    below_state
                };
                let mut grown = source;
                for face in Face::HORIZONTAL {
                    if self.rng.random::<bool>() && has_side(state, face) {
                        grown = with_side(grown, face);
                    }
                }
                if source != grown && horizontal_connection(grown) {
                    self.vine_place(below, grown, pos);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::time::Duration;

    use rspigot_blocks::BlockKind;
    use rspigot_domain::{BlockPos, BlockStateId};

    use crate::config::{GameConfig, LevelData};
    use crate::server::Server;
    use crate::status::StatusBoard;
    use crate::world_state::tests::Quiet;

    fn server() -> Server<Quiet> {
        let config = GameConfig {
            chunk_workers: 1,
            spawn_chunk_radius: 1,
            ..GameConfig::default()
        };
        let mut server = Server::new(
            config,
            LevelData::default(),
            Arc::new(Quiet),
            Arc::new(Quiet),
            StatusBoard::new(1),
        )
        .with_rng_seed(5);
        server.wait_for_spawn(Duration::from_secs(10));
        server
    }

    fn state(kind: BlockKind, properties: &[(&str, &str)]) -> BlockStateId {
        kind.state_with(properties).expect("state")
    }

    fn at(server: &Server<Quiet>, pos: BlockPos) -> BlockStateId {
        server.world.chunks.block(pos).expect("loaded")
    }

    fn origin() -> BlockPos {
        BlockPos::new(2, 63, 2)
    }

    fn tick_until(
        server: &mut Server<Quiet>,
        pos: BlockPos,
        done: impl Fn(&Server<Quiet>) -> bool,
    ) -> bool {
        for _ in 0..4000 {
            let current = at(server, pos);
            server.random_tick_stalk(pos, current);
            if done(server) {
                return true;
            }
        }
        false
    }

    #[test]
    fn other_blocks_are_not_claimed() {
        let mut server = server();
        let stone = at(&server, BlockPos::new(2, 62, 2));
        assert!(!server.random_tick_stalk(BlockPos::new(2, 62, 2), stone));
    }

    #[test]
    fn cane_counts_its_age() {
        let mut server = server();
        let pos = origin();
        let cane = state(BlockKind::SugarCane, &[("age", "3")]);
        server.set_block(pos, cane);
        assert!(server.random_tick_stalk(pos, cane));
        assert_eq!(at(&server, pos).property("age"), Some("4"));
    }

    #[test]
    fn cane_at_full_age_grows_and_resets() {
        let mut server = server();
        let pos = origin();
        let cane = state(BlockKind::SugarCane, &[("age", "15")]);
        server.set_block(pos, cane);
        assert!(server.random_tick_stalk(pos, cane));
        assert_eq!(at(&server, pos).property("age"), Some("0"));
        assert_eq!(
            at(&server, pos.offset(0, 1, 0)).kind(),
            Some(BlockKind::SugarCane)
        );
    }

    #[test]
    fn cane_stops_at_three_high() {
        let mut server = server();
        let pos = origin();
        let plain = state(BlockKind::SugarCane, &[("age", "0")]);
        let top = state(BlockKind::SugarCane, &[("age", "15")]);
        server.set_block(pos, plain);
        server.set_block(pos.offset(0, 1, 0), plain);
        server.set_block(pos.offset(0, 2, 0), top);
        assert!(server.random_tick_stalk(pos.offset(0, 2, 0), top));
        assert!(at(&server, pos.offset(0, 3, 0)).is_air());
        assert_eq!(at(&server, pos.offset(0, 2, 0)).property("age"), Some("15"));
    }

    #[test]
    fn cane_with_blocked_top_does_not_age() {
        let mut server = server();
        let pos = origin();
        let cane = state(BlockKind::SugarCane, &[("age", "5")]);
        server.set_block(pos, cane);
        server.set_block(pos.offset(0, 1, 0), BlockKind::Stone.default_state());
        assert!(server.random_tick_stalk(pos, cane));
        assert_eq!(at(&server, pos).property("age"), Some("5"));
    }

    #[test]
    fn cactus_at_full_age_grows() {
        let mut server = server();
        let pos = origin();
        let cactus = state(BlockKind::Cactus, &[("age", "15")]);
        server.set_block(pos, cactus);
        assert!(server.random_tick_stalk(pos, cactus));
        assert_eq!(at(&server, pos).property("age"), Some("0"));
        assert_eq!(
            at(&server, pos.offset(0, 1, 0)).kind(),
            Some(BlockKind::Cactus)
        );
    }

    #[test]
    fn kelp_grows_into_water_and_head_becomes_body() {
        let mut server = server();
        let pos = origin();
        let water = BlockKind::Water.default_state();
        for dy in 0..4 {
            server.set_block(pos.offset(0, dy, 0), water);
        }
        server.set_block(pos, state(BlockKind::Kelp, &[("age", "0")]));
        let up = pos.offset(0, 1, 0);
        assert!(tick_until(&mut server, pos, |server| at(server, up).kind()
            == Some(BlockKind::Kelp)));
        assert_eq!(at(&server, pos).kind(), Some(BlockKind::KelpPlant));
        assert_eq!(at(&server, up).property("age"), Some("1"));
    }

    #[test]
    fn kelp_at_max_age_stays() {
        let mut server = server();
        let pos = origin();
        let water = BlockKind::Water.default_state();
        server.set_block(pos.offset(0, 1, 0), water);
        let kelp = state(BlockKind::Kelp, &[("age", "25")]);
        server.set_block(pos, kelp);
        assert!(!tick_until(&mut server, pos, |server| at(
            server,
            pos.offset(0, 1, 0)
        )
        .kind()
            == Some(BlockKind::Kelp)));
    }

    #[test]
    fn twisting_vines_grow_up_and_weeping_grow_down() {
        let mut server = server();
        let pos = origin();
        server.set_block(pos, state(BlockKind::TwistingVines, &[("age", "0")]));
        assert!(tick_until(&mut server, pos, |server| at(
            server,
            pos.offset(0, 1, 0)
        )
        .kind()
            == Some(BlockKind::TwistingVines)));
        assert_eq!(at(&server, pos).kind(), Some(BlockKind::TwistingVinesPlant));
        let hang = BlockPos::new(6, 70, 6);
        server.set_block(hang, state(BlockKind::WeepingVines, &[("age", "0")]));
        assert!(tick_until(&mut server, hang, |server| at(
            server,
            hang.offset(0, -1, 0)
        )
        .kind()
            == Some(BlockKind::WeepingVines)));
        assert_eq!(at(&server, hang).kind(), Some(BlockKind::WeepingVinesPlant));
    }

    #[test]
    fn cave_vines_keep_berries_on_the_body() {
        let mut server = server();
        let pos = BlockPos::new(6, 70, 6);
        server.set_block(
            pos,
            state(BlockKind::CaveVines, &[("age", "0"), ("berries", "true")]),
        );
        assert!(tick_until(&mut server, pos, |server| at(
            server,
            pos.offset(0, -1, 0)
        )
        .kind()
            == Some(BlockKind::CaveVines)));
        let body = at(&server, pos);
        assert_eq!(body.kind(), Some(BlockKind::CaveVinesPlant));
        assert_eq!(body.property("berries"), Some("true"));
    }

    #[test]
    fn bamboo_sapling_becomes_bamboo_in_light() {
        let mut server = server();
        let pos = origin();
        server.set_block(pos, BlockKind::BambooSapling.default_state());
        assert!(tick_until(&mut server, pos, |server| at(
            server,
            pos.offset(0, 1, 0)
        )
        .kind()
            == Some(BlockKind::Bamboo)));
    }

    #[test]
    fn bamboo_grows_a_leafy_stalk() {
        let mut server = server();
        let pos = origin();
        server.set_block(pos, BlockKind::Bamboo.default_state());
        let up = pos.offset(0, 1, 0);
        assert!(tick_until(&mut server, pos, |server| at(server, up).kind()
            == Some(BlockKind::Bamboo)));
        assert_eq!(at(&server, up).property("leaves"), Some("small"));
    }

    #[test]
    fn bamboo_stops_at_sixteen() {
        let mut server = server();
        let pos = origin();
        for dy in 0..16 {
            server.set_block(pos.offset(0, dy, 0), BlockKind::Bamboo.default_state());
        }
        let top = pos.offset(0, 15, 0);
        assert!(!tick_until(&mut server, top, |server| !at(
            server,
            top.offset(0, 1, 0)
        )
        .is_air()));
    }

    #[test]
    fn vine_spreads_around_a_wall() {
        let mut server = server();
        let pos = BlockPos::new(4, 66, 4);
        for dy in 0..3 {
            server.set_block(pos.offset(0, dy, -1), BlockKind::Stone.default_state());
        }
        let vine = state(BlockKind::Vine, &[("north", "true")]);
        server.set_block(pos, vine);
        assert!(tick_until(&mut server, pos, |server| at(
            server,
            pos.offset(0, -1, 0)
        )
        .kind()
            == Some(BlockKind::Vine)));
    }
}

use rand::Rng;
use rspigot_blocks::{BlockKind, Face, Support};
use rspigot_domain::flammable::flammability;
use rspigot_domain::{BlockPos, BlockStateId, Vec3};

use crate::explosion::fire_state_on;
use crate::plugins::steps::Nature;
use crate::ports::ClientLink;
use crate::server::Server;

const MAX_AGE: i32 = 15;
const TICK_DELAY_BASE: u64 = 30;
const TICK_DELAY_SPREAD: u64 = 10;
const HUMID_BURNOUT: i32 = -50;
const SIDE_BURN_CHANCE: i32 = 300;
const VERTICAL_BURN_CHANCE: i32 = 250;
const BURN_NEIGHBOURS: [(Face, i32); 6] = [
    (Face::East, SIDE_BURN_CHANCE),
    (Face::West, SIDE_BURN_CHANCE),
    (Face::Down, VERTICAL_BURN_CHANCE),
    (Face::Up, VERTICAL_BURN_CHANCE),
    (Face::North, SIDE_BURN_CHANCE),
    (Face::South, SIDE_BURN_CHANCE),
];
const HUMID_BIOMES: [&str; 5] = [
    "minecraft:swamp",
    "minecraft:mangrove_swamp",
    "minecraft:jungle",
    "minecraft:sparse_jungle",
    "minecraft:bamboo_jungle",
];
const SPREAD_BASE_RATE: i32 = 100;
const IGNITE_BASE: i32 = 40;
const DIFFICULTY_STEP: i32 = 7;
const AGE_BASE: i32 = 30;
const FIRE_SIDES: [(&str, Face); 5] = [
    ("north", Face::North),
    ("east", Face::East),
    ("south", Face::South),
    ("west", Face::West),
    ("up", Face::Up),
];

fn at(pos: BlockPos, face: Face) -> BlockPos {
    let (dx, dy, dz) = face.offset();
    pos.offset(dx, dy, dz)
}

fn fire_age(state: BlockStateId) -> i32 {
    state
        .property("age")
        .and_then(|age| age.parse().ok())
        .unwrap_or(0)
}

fn burnable(state: BlockStateId) -> Option<(i32, i32)> {
    if state.property("waterlogged") == Some("true") {
        return None;
    }
    let odds = flammability(state.kind()?);
    Some((i32::from(odds.ignite), i32::from(odds.burn)))
}

fn ignite_odds(state: BlockStateId) -> i32 {
    burnable(state).map_or(0, |(ignite, _)| ignite)
}

fn burn_odds(state: BlockStateId) -> i32 {
    burnable(state).map_or(0, |(_, burn)| burn)
}

fn can_burn(state: BlockStateId) -> bool {
    ignite_odds(state) > 0
}

impl<L: ClientLink> Server<L> {
    pub(crate) fn fire_placed(&mut self, pos: BlockPos) {
        let delay = TICK_DELAY_BASE + self.rng.random_range(0..TICK_DELAY_SPREAD);
        self.schedule(pos, BlockKind::Fire, delay);
    }

    fn infiniburn(&self, below: BlockStateId) -> bool {
        let tag = match self.world.dimension.key.as_str() {
            "minecraft:the_end" => "minecraft:infiniburn_end",
            "minecraft:the_nether" => "minecraft:infiniburn_nether",
            _ => "minecraft:infiniburn_overworld",
        };
        below.kind().is_some_and(|kind| {
            rspigot_data::tags::tag("minecraft:block", tag)
                .is_some_and(|ids| ids.binary_search(&u32::from(kind.id().0)).is_ok())
        })
    }

    pub(crate) fn can_spread_fire_around(&self, pos: BlockPos) -> bool {
        let radius = self.config.rules.fire_spread_radius;
        if radius < 0 {
            return true;
        }
        let centre = Vec3::new(
            f64::from(pos.x) + 0.5,
            f64::from(pos.y) + 0.5,
            f64::from(pos.z) + 0.5,
        );
        let limit = f64::from(radius) * f64::from(radius);
        self.players.values().any(|player| {
            let (dx, dy, dz) = (
                player.position.x - centre.x,
                player.position.y - centre.y,
                player.position.z - centre.z,
            );
            dx * dx + dy * dy + dz * dz < limit
        })
    }

    fn near_rain(&self, pos: BlockPos) -> bool {
        [
            pos,
            at(pos, Face::North),
            at(pos, Face::South),
            at(pos, Face::East),
            at(pos, Face::West),
        ]
        .into_iter()
        .any(|spot| self.raining_at(spot))
    }

    fn valid_fire_location(&self, pos: BlockPos) -> bool {
        Face::ALL
            .into_iter()
            .any(|face| can_burn(self.block_or_air(at(pos, face))))
    }

    pub(crate) fn fire_survives(&self, pos: BlockPos) -> bool {
        self.block_or_air(at(pos, Face::Down))
            .is_face_sturdy(Face::Up, Support::Full)
            || self.valid_fire_location(pos)
    }

    fn neighbour_ignite_odds(&self, pos: BlockPos) -> i32 {
        if !self.block_or_air(pos).is_air() {
            return 0;
        }
        Face::ALL
            .into_iter()
            .map(|face| ignite_odds(self.block_or_air(at(pos, face))))
            .max()
            .unwrap_or(0)
    }

    pub(crate) fn fire_state_with_age(&self, pos: BlockPos, age: i32) -> BlockStateId {
        let below_pos = at(pos, Face::Down);
        let below = self.block_or_air(below_pos);
        let base = fire_state_on(below);
        if base.kind() != Some(BlockKind::Fire) {
            return base;
        }
        let mut properties: Vec<(&str, String)> = vec![("age", age.to_string())];
        if !can_burn(below) && !below.is_face_sturdy(Face::Up, Support::Full) {
            for (name, face) in FIRE_SIDES {
                let lit = can_burn(self.block_or_air(at(pos, face)));
                properties.push((name, lit.to_string()));
            }
        }
        let pairs: Vec<(&str, &str)> = properties
            .iter()
            .map(|(name, value)| (*name, value.as_str()))
            .collect();
        BlockKind::Fire.state_with(&pairs).unwrap_or(base)
    }

    fn fire_extinguished(&mut self, pos: BlockPos) {
        self.nature_change(Nature::Fade, pos, BlockStateId::AIR, pos);
    }

    fn humid_at(&self, pos: BlockPos) -> bool {
        rspigot_data::registries::biome_name(self.biome_at(pos).0)
            .is_some_and(|name| HUMID_BIOMES.contains(&name))
    }

    pub(crate) fn fire_tick(&mut self, pos: BlockPos, state: BlockStateId) {
        let delay = TICK_DELAY_BASE + self.rng.random_range(0..TICK_DELAY_SPREAD);
        self.schedule(pos, BlockKind::Fire, delay);
        if !self.can_spread_fire_around(pos) {
            return;
        }
        if !self.fire_survives(pos) {
            self.fire_extinguished(pos);
            return;
        }
        let below = self.block_or_air(at(pos, Face::Down));
        let infiniburn = self.infiniburn(below);
        let age = fire_age(state);
        let raining = self.world.level.raining;
        if !infiniburn
            && raining
            && self.near_rain(pos)
            && self.rng.random::<f32>() < 0.2 + age as f32 * 0.03
        {
            self.fire_extinguished(pos);
            return;
        }
        let new_age = MAX_AGE.min(age + self.rng.random_range(0..3) / 2);
        if age != new_age
            && let Some(aged) = state.with_property("age", &new_age.to_string())
        {
            self.set_block(pos, aged);
        }
        if !infiniburn {
            if !self.valid_fire_location(pos) {
                let sturdy = below.is_face_sturdy(Face::Up, Support::Full);
                if !sturdy || age > 3 {
                    self.fire_extinguished(pos);
                }
                return;
            }
            if age == MAX_AGE && self.rng.random_range(0..4) == 0 && !can_burn(below) {
                self.fire_extinguished(pos);
                return;
            }
        }
        let humid = self.humid_at(pos);
        let extra = if humid { HUMID_BURNOUT } else { 0 };
        for (face, chance) in BURN_NEIGHBOURS {
            self.check_burn_out(at(pos, face), chance + extra, age, pos);
        }
        self.spread_fire(pos, age, humid);
    }

    fn check_burn_out(&mut self, pos: BlockPos, chance: i32, age: i32, source: BlockPos) {
        let odds = burn_odds(self.block_or_air(pos));
        if odds == 0 || self.rng.random_range(0..chance.max(1)) >= odds {
            return;
        }
        let stays = self.rng.random_range(0..age + 10) < 5 && !self.raining_at(pos);
        let replacement = if stays {
            let new_age = MAX_AGE.min(age + self.rng.random_range(0..5) / 4);
            self.fire_state_with_age(pos, new_age)
        } else {
            BlockStateId::AIR
        };
        self.nature_change(Nature::Burn, pos, replacement, source);
    }

    fn spread_fire(&mut self, pos: BlockPos, age: i32, humid: bool) {
        let difficulty = i32::from(self.world.difficulty.id());
        let raining = self.world.level.raining;
        for dx in -1..=1 {
            for dz in -1..=1 {
                for dy in -1..=4 {
                    if dx == 0 && dy == 0 && dz == 0 {
                        continue;
                    }
                    let rate = if dy > 1 {
                        SPREAD_BASE_RATE + (dy - 1) * 100
                    } else {
                        SPREAD_BASE_RATE
                    };
                    let target = pos.offset(dx, dy, dz);
                    let odds = self.neighbour_ignite_odds(target);
                    if odds <= 0 {
                        continue;
                    }
                    let mut scaled =
                        (odds + IGNITE_BASE + difficulty * DIFFICULTY_STEP) / (age + AGE_BASE);
                    if humid {
                        scaled /= 2;
                    }
                    if scaled <= 0
                        || self.rng.random_range(0..rate) > scaled
                        || (raining && self.near_rain(target))
                    {
                        continue;
                    }
                    let new_age = MAX_AGE.min(age + self.rng.random_range(0..5) / 4);
                    let fire = self.fire_state_with_age(target, new_age);
                    if self.block_or_air(target).kind() != Some(BlockKind::Fire) {
                        self.nature_change(Nature::Ignite, target, fire, pos);
                    }
                }
            }
        }
    }
}

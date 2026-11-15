use rspigot_blocks::{BlockKind as Kind, Face};
use rspigot_domain::BlockStateId;
use serde_json::Value;

use super::cave::{
    full_face, has_face, manhattan, moved, replaceable, shuffle, water_source, with_face,
};
use super::place::Run;
use super::prov::{BlockSet, Pos, flag, integer, is_fluid, number};
use crate::climate::Xoroshiro;
use crate::overworld::region::Region;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum SpreadType {
    SamePosition,
    SamePlane,
    WrapAround,
}

pub(super) const DEFAULT_ORDER: [SpreadType; 3] = [
    SpreadType::SamePosition,
    SpreadType::SamePlane,
    SpreadType::WrapAround,
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct SpreadPos {
    pub(super) pos: Pos,
    pub(super) face: Face,
}

impl SpreadType {
    fn spread_pos(self, pos: Pos, spread: Face, starting: Face) -> SpreadPos {
        match self {
            Self::SamePosition => SpreadPos { pos, face: spread },
            Self::SamePlane => SpreadPos {
                pos: moved(pos, spread),
                face: starting,
            },
            Self::WrapAround => SpreadPos {
                pos: moved(moved(pos, spread), starting),
                face: spread.opposite(),
            },
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub(super) enum Config {
    Plain,
    Sculk(&'static [SpreadType]),
}

#[derive(Debug, Clone, Copy)]
pub(super) struct Spreader {
    pub(super) block: Kind,
    pub(super) config: Config,
}

pub(super) const LICHEN_SPREADER: Spreader = Spreader {
    block: Kind::GlowLichen,
    config: Config::Plain,
};

pub(super) const VEIN_SPREADER: Spreader = Spreader {
    block: Kind::SculkVein,
    config: Config::Sculk(&DEFAULT_ORDER),
};

pub(super) const VEIN_SAME_SPACE: Spreader = Spreader {
    block: Kind::SculkVein,
    config: Config::Sculk(&[SpreadType::SamePosition]),
};

fn attachable(region: &mut Region, face: Face, pos: Pos) -> bool {
    full_face(region.get(pos.0, pos.1, pos.2), face.opposite())
}

pub(super) fn can_attach_to(region: &mut Region, pos: Pos, face: Face) -> bool {
    attachable(region, face, moved(pos, face))
}

impl Spreader {
    fn valid_for_placement(
        &self,
        region: &mut Region,
        old: BlockStateId,
        pos: Pos,
        face: Face,
    ) -> bool {
        if old.kind() == Some(self.block) && has_face(old, face) {
            return false;
        }
        can_attach_to(region, pos, face)
    }

    pub(super) fn state_for_placement(
        &self,
        region: &mut Region,
        old: BlockStateId,
        pos: Pos,
        face: Face,
    ) -> Option<BlockStateId> {
        if !self.valid_for_placement(region, old, pos, face) {
            return None;
        }
        let base = if old.kind() == Some(self.block) {
            old
        } else if water_source(old) {
            self.block
                .default_state()
                .with_property("waterlogged", "true")
                .unwrap_or(old)
        } else {
            self.block.default_state()
        };
        Some(with_face(base, face, true))
    }

    fn other_valid_as_source(&self, state: BlockStateId) -> bool {
        match self.config {
            Config::Plain => false,
            Config::Sculk(_) => state.kind() != Some(Kind::SculkVein),
        }
    }

    fn can_spread_from(&self, state: BlockStateId, face: Face) -> bool {
        self.other_valid_as_source(state) || has_face(state, face)
    }

    fn types(&self) -> &'static [SpreadType] {
        match self.config {
            Config::Plain => &DEFAULT_ORDER,
            Config::Sculk(types) => types,
        }
    }

    fn can_be_replaced(
        &self,
        region: &mut Region,
        pos: Pos,
        spread: Pos,
        face: Face,
        state: BlockStateId,
    ) -> bool {
        let plain = |state: BlockStateId| {
            state.is_air()
                || state.kind() == Some(self.block)
                || (state.kind() == Some(Kind::Water) && water_source(state))
        };
        match self.config {
            Config::Plain => plain(state),
            Config::Sculk(_) => {
                let beside = moved(spread, face);
                let beside = region.get(beside.0, beside.1, beside.2);
                if matches!(
                    beside.kind(),
                    Some(Kind::Sculk | Kind::SculkCatalyst | Kind::MovingPiston)
                ) {
                    return false;
                }
                if manhattan(pos, spread) == 2 {
                    let behind = moved(pos, face.opposite());
                    if region
                        .get(behind.0, behind.1, behind.2)
                        .is_face_sturdy(face, rspigot_blocks::Support::Full)
                    {
                        return false;
                    }
                }
                if is_fluid(state) && !water_source(state) {
                    return false;
                }
                if fire(state) {
                    return false;
                }
                replaceable(state) || plain(state)
            }
        }
    }

    fn can_spread_into(&self, region: &mut Region, pos: Pos, target: SpreadPos) -> bool {
        let state = region.get(target.pos.0, target.pos.1, target.pos.2);
        self.can_be_replaced(region, pos, target.pos, target.face, state)
            && self.valid_for_placement(region, state, target.pos, target.face)
    }

    fn spread_from_face_toward(
        &self,
        region: &mut Region,
        state: BlockStateId,
        pos: Pos,
        starting: Face,
        spread: Face,
    ) -> Option<SpreadPos> {
        if axis(spread) == axis(starting) {
            return None;
        }
        if !self.other_valid_as_source(state)
            && (!has_face(state, starting) || has_face(state, spread))
        {
            return None;
        }
        for kind in self.types() {
            let target = kind.spread_pos(pos, spread, starting);
            if self.can_spread_into(region, pos, target) {
                return self.spread_to_face(region, target);
            }
        }
        None
    }

    fn spread_to_face(&self, region: &mut Region, target: SpreadPos) -> Option<SpreadPos> {
        let state = region.get(target.pos.0, target.pos.1, target.pos.2);
        let placed = self.state_for_placement(region, state, target.pos, target.face)?;
        region.set(target.pos.0, target.pos.1, target.pos.2, placed);
        Some(target)
    }

    pub(super) fn spread_toward_random_direction(
        &self,
        region: &mut Region,
        rng: &mut Xoroshiro,
        state: BlockStateId,
        pos: Pos,
        starting: Face,
    ) -> Option<SpreadPos> {
        let mut faces = Face::ALL;
        shuffle(rng, &mut faces);
        faces
            .into_iter()
            .find_map(|spread| self.spread_from_face_toward(region, state, pos, starting, spread))
    }

    pub(super) fn spread_all(&self, region: &mut Region, state: BlockStateId, pos: Pos) -> i64 {
        let mut total = 0;
        for starting in Face::ALL {
            if !self.can_spread_from(state, starting) {
                continue;
            }
            for spread in Face::ALL {
                if self
                    .spread_from_face_toward(region, state, pos, starting, spread)
                    .is_some()
                {
                    total += 1;
                }
            }
        }
        total
    }
}

fn axis(face: Face) -> u8 {
    match face {
        Face::Down | Face::Up => 1,
        Face::North | Face::South => 2,
        Face::West | Face::East => 0,
    }
}

fn fire(state: BlockStateId) -> bool {
    matches!(state.kind(), Some(Kind::Fire | Kind::SoulFire))
}

#[derive(Debug, Clone)]
pub(crate) struct Growth {
    spreader: Spreader,
    search: i32,
    directions: Vec<Face>,
    spread_chance: f32,
    placeable: BlockSet,
}

fn air_or_water(state: BlockStateId) -> bool {
    state.is_air() || state.kind() == Some(Kind::Water)
}

impl Growth {
    pub(crate) fn compile(value: &Value) -> Option<Self> {
        let name = value.get("block").and_then(Value::as_str)?;
        let spreader = match Kind::from_name(name)? {
            Kind::GlowLichen => LICHEN_SPREADER,
            Kind::SculkVein => VEIN_SPREADER,
            _ => return None,
        };
        let mut directions = Vec::new();
        if flag(value, "can_place_on_ceiling", false) {
            directions.push(Face::Up);
        }
        if flag(value, "can_place_on_floor", false) {
            directions.push(Face::Down);
        }
        if flag(value, "can_place_on_wall", false) {
            directions.extend(Face::HORIZONTAL);
        }
        Some(Self {
            spreader,
            search: integer(value, "search_range", 10),
            directions,
            spread_chance: number(value, "chance_of_spreading", 0.5) as f32,
            placeable: BlockSet::compile(value.get("can_be_placed_on").unwrap_or(&Value::Null)),
        })
    }

    pub(crate) fn reach(&self) -> i32 {
        2
    }

    fn grow(&self, run: &mut Run, pos: Pos, old: BlockStateId, directions: &[Face]) -> bool {
        for face in directions {
            let base = moved(pos, *face);
            if !self
                .placeable
                .contains(run.region.get(base.0, base.1, base.2))
            {
                continue;
            }
            let Some(state) = self
                .spreader
                .state_for_placement(run.region, old, pos, *face)
            else {
                return false;
            };
            run.region.set(pos.0, pos.1, pos.2, state);
            if run.rng.next_float() < self.spread_chance {
                self.spreader
                    .spread_toward_random_direction(run.region, run.rng, state, pos, *face);
            }
            return true;
        }
        false
    }

    pub(crate) fn place(&self, run: &mut Run, origin: Pos) -> bool {
        if !air_or_water(run.region.get(origin.0, origin.1, origin.2)) {
            return false;
        }
        let mut shuffled = self.directions.clone();
        shuffle(run.rng, &mut shuffled);
        let old = run.region.get(origin.0, origin.1, origin.2);
        if self.grow(run, origin, old, &shuffled) {
            return true;
        }
        for face in &shuffled {
            let mut others: Vec<Face> = self
                .directions
                .iter()
                .copied()
                .filter(|other| *other != face.opposite())
                .collect();
            shuffle(run.rng, &mut others);
            if self.search <= 0 {
                continue;
            }
            let at = moved(origin, *face);
            let state = run.region.get(at.0, at.1, at.2);
            if !air_or_water(state) && state.kind() != Some(self.spreader.block) {
                continue;
            }
            if self.grow(run, at, state, &others) {
                return true;
            }
        }
        false
    }
}

#[cfg(test)]
mod tests;

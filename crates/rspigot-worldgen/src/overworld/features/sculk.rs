use std::sync::OnceLock;

use rspigot_blocks::{BlockKind as Kind, Face, StateFlags, Support};
use rspigot_domain::BlockStateId;
use serde_json::Value;

use super::cave::{has_any_face, has_face, manhattan, moved, shuffle, water_source, with_face};
use super::multiface::{VEIN_SAME_SPACE, VEIN_SPREADER, can_attach_to};
use super::place::Run;
use super::prov::{BlockSet, Pos, integer};
use crate::overworld::region::Region;

const GROWTH_COST: i32 = 50;
const NO_GROWTH_RADIUS: i32 = 1;
const DECAY_RATE: i32 = 5;
const ADDITIONAL_DECAY_RATE: i32 = 10;
const MAX_CURSORS: usize = 32;
const MAX_WORLDGEN_SPREAD: i32 = 144;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Behaviour {
    Plain,
    Sculk,
    Vein,
}

impl Behaviour {
    fn of(state: BlockStateId) -> Self {
        match state.kind() {
            Some(Kind::Sculk) => Self::Sculk,
            Some(Kind::SculkVein) => Self::Vein,
            _ => Self::Plain,
        }
    }

    fn is_sculk_block(self) -> bool {
        self != Self::Plain
    }

    fn changes_state_on_spread(self) -> bool {
        self != Self::Sculk
    }

    fn update_decay_delay(self, delay: i32) -> i32 {
        match self {
            Self::Plain => (delay - 1).max(0),
            _ => 1,
        }
    }
}

struct Sets {
    replaceable: BlockSet,
    replaceable_world_gen: BlockSet,
    inhibitors: BlockSet,
}

fn sets() -> &'static Sets {
    static SETS: OnceLock<Sets> = OnceLock::new();
    SETS.get_or_init(|| Sets {
        replaceable: BlockSet::tag("minecraft:sculk_replaceable"),
        replaceable_world_gen: BlockSet::tag("minecraft:sculk_replaceable_world_gen"),
        inhibitors: BlockSet::tag("minecraft:sculk_growth_inhibitors"),
    })
}

fn neighbours() -> &'static [Pos] {
    static OFFSETS: OnceLock<Vec<Pos>> = OnceLock::new();
    OFFSETS.get_or_init(|| {
        let mut offsets = Vec::with_capacity(18);
        for dz in -1..=1 {
            for dy in -1..=1 {
                for dx in -1..=1 {
                    if (dx == 0 || dy == 0 || dz == 0) && (dx, dy, dz) != (0, 0, 0) {
                        offsets.push((dx, dy, dz));
                    }
                }
            }
        }
        offsets
    })
}

fn available_faces(state: BlockStateId) -> Vec<Face> {
    if state.kind() != Some(Kind::SculkVein) {
        return Vec::new();
    }
    Face::ALL
        .into_iter()
        .filter(|face| has_face(state, *face))
        .collect()
}

#[derive(Debug, Clone)]
struct Cursor {
    pos: Pos,
    charge: i32,
    update_delay: i32,
    decay_delay: i32,
    facings: Option<Vec<Face>>,
}

fn at(region: &mut Region, (x, y, z): Pos) -> BlockStateId {
    region.get(x, y, z)
}

fn unobstructed(region: &mut Region, pos: Pos, face: Face) -> bool {
    let next = moved(pos, face);
    !at(region, next).is_face_sturdy(face.opposite(), Support::Full)
}

fn movement_unobstructed(region: &mut Region, from: Pos, to: Pos) -> bool {
    if manhattan(from, to) == 1 {
        return true;
    }
    let (dx, dy, dz) = (to.0 - from.0, to.1 - from.1, to.2 - from.2);
    let x = if dx < 0 { Face::West } else { Face::East };
    let y = if dy < 0 { Face::Down } else { Face::Up };
    let z = if dz < 0 { Face::North } else { Face::South };
    if dx == 0 {
        unobstructed(region, from, y) || unobstructed(region, from, z)
    } else if dy == 0 {
        unobstructed(region, from, x) || unobstructed(region, from, z)
    } else {
        unobstructed(region, from, x) || unobstructed(region, from, y)
    }
}

fn substrate_access(region: &mut Region, state: BlockStateId, pos: Pos) -> bool {
    if state.kind() != Some(Kind::SculkVein) {
        return false;
    }
    Face::ALL.into_iter().any(|face| {
        has_face(state, face) && sets().replaceable.contains(at(region, moved(pos, face)))
    })
}

fn can_move_to(origin: Pos, pos: Pos) -> bool {
    let dx = origin.0 - pos.0;
    let dz = origin.2 - pos.2;
    dx * dx + dz * dz <= MAX_WORLDGEN_SPREAD
}

fn regrow(region: &mut Region, pos: Pos, state: BlockStateId, facings: &[Face]) -> bool {
    let mut vein = Kind::SculkVein.default_state();
    let mut any = false;
    for face in facings {
        if can_attach_to(region, pos, *face) {
            vein = with_face(vein, *face, true);
            any = true;
        }
    }
    if !any {
        return false;
    }
    if super::prov::is_fluid(state) {
        vein = vein.with_property("waterlogged", "true").unwrap_or(vein);
    }
    region.set(pos.0, pos.1, pos.2, vein);
    true
}

fn discharge(region: &mut Region, behaviour: Behaviour, state: BlockStateId, pos: Pos) {
    if behaviour != Behaviour::Vein || state.kind() != Some(Kind::SculkVein) {
        return;
    }
    let mut state = state;
    for face in Face::ALL {
        if has_face(state, face) && at(region, moved(pos, face)).kind() == Some(Kind::Sculk) {
            state = with_face(state, face, false);
        }
    }
    if !has_any_face(state) {
        let here = at(region, pos);
        let fluid = super::prov::is_fluid(here);
        state = if fluid {
            Kind::Water.default_state()
        } else {
            BlockStateId::AIR
        };
    }
    region.set(pos.0, pos.1, pos.2, state);
}

fn place_sculk_from_vein(run: &mut Run, pos: Pos) -> bool {
    let state = at(run.region, pos);
    for face in super::cave::shuffled_faces(run) {
        if !has_face(state, face) {
            continue;
        }
        let target = moved(pos, face);
        let beside = at(run.region, target);
        if !sets().replaceable_world_gen.contains(beside) {
            continue;
        }
        let sculk = Kind::Sculk.default_state();
        run.region.set(target.0, target.1, target.2, sculk);
        VEIN_SPREADER.spread_all(run.region, sculk, target);
        let back = face.opposite();
        for other in Face::ALL {
            if other == back {
                continue;
            }
            let neighbour = moved(target, other);
            let neighbour_state = at(run.region, neighbour);
            if neighbour_state.kind() == Some(Kind::SculkVein) {
                discharge(run.region, Behaviour::Vein, neighbour_state, neighbour);
            }
        }
        return true;
    }
    false
}

fn growth_allowed(region: &mut Region, pos: Pos) -> bool {
    let above = at(region, (pos.0, pos.1 + 1, pos.2));
    if !(above.is_air() || (above.kind() == Some(Kind::Water) && water_source(above))) {
        return false;
    }
    let mut matched = 0;
    for dz in -4..=4 {
        for dy in 0..=2 {
            for dx in -4..=4 {
                if sets()
                    .inhibitors
                    .contains(at(region, (pos.0 + dx, pos.1 + dy, pos.2 + dz)))
                {
                    matched += 1;
                    if matched > 2 {
                        return false;
                    }
                }
            }
        }
    }
    true
}

fn decay_penalty(cursor: Pos, origin: Pos, charge: i32) -> i32 {
    let dx = f64::from(cursor.0 - origin.0);
    let dy = f64::from(cursor.1 - origin.1);
    let dz = f64::from(cursor.2 - origin.2);
    let distance = (dx * dx + dy * dy + dz * dz).sqrt() as f32;
    let outside = distance - NO_GROWTH_RADIUS as f32;
    let outside = outside * outside;
    let span = (24 - NO_GROWTH_RADIUS) * (24 - NO_GROWTH_RADIUS);
    let ratio = (outside / span as f32).min(1.0);
    1.max((charge as f32 * ratio * 0.5) as i32)
}

fn random_growth(run: &mut Run, pos: Pos) -> BlockStateId {
    let base = if run.rng.below(11) == 0 {
        let shrieker = Kind::SculkShrieker.default_state();
        shrieker
            .with_property("can_summon", "true")
            .unwrap_or(shrieker)
    } else {
        Kind::SculkSensor.default_state()
    };
    if base.property("waterlogged").is_some() && super::prov::is_fluid(at(run.region, pos)) {
        return base.with_property("waterlogged", "true").unwrap_or(base);
    }
    base
}

impl Cursor {
    fn new(pos: Pos, charge: i32) -> Self {
        Self {
            pos,
            charge,
            update_delay: 0,
            decay_delay: 1,
            facings: None,
        }
    }

    fn spread_vein(&self, region: &mut Region, behaviour: Behaviour, state: BlockStateId) -> bool {
        if behaviour == Behaviour::Plain {
            match &self.facings {
                None => {
                    let here = at(region, self.pos);
                    return VEIN_SAME_SPACE.spread_all(region, here, self.pos) > 0;
                }
                Some(faces) if !faces.is_empty() => {
                    if state.is_air() || water_source(state) {
                        return regrow(region, self.pos, state, faces);
                    }
                    return false;
                }
                Some(_) => {}
            }
        }
        VEIN_SPREADER.spread_all(region, state, self.pos) > 0
    }

    fn use_charge(&self, run: &mut Run, behaviour: Behaviour, origin: Pos, veins: bool) -> i32 {
        match behaviour {
            Behaviour::Plain => {
                if self.decay_delay > 0 {
                    self.charge
                } else {
                    0
                }
            }
            Behaviour::Vein => {
                if veins && place_sculk_from_vein(run, self.pos) {
                    return self.charge - 1;
                }
                if run.rng.below(DECAY_RATE) == 0 {
                    (self.charge as f32 * 0.5) as i32
                } else {
                    self.charge
                }
            }
            Behaviour::Sculk => self.use_charge_in_sculk(run, origin),
        }
    }

    fn use_charge_in_sculk(&self, run: &mut Run, origin: Pos) -> i32 {
        let charge = self.charge;
        if charge == 0 || run.rng.below(DECAY_RATE) != 0 {
            return charge;
        }
        let pos = self.pos;
        let close = {
            let (dx, dy, dz) = (pos.0 - origin.0, pos.1 - origin.1, pos.2 - origin.2);
            f64::from(dx * dx + dy * dy + dz * dz) < f64::from(NO_GROWTH_RADIUS * NO_GROWTH_RADIUS)
        };
        if close || !growth_allowed(run.region, pos) {
            if run.rng.below(ADDITIONAL_DECAY_RATE) != 0 {
                return charge;
            }
            return charge
                - if close {
                    1
                } else {
                    decay_penalty(pos, origin, charge)
                };
        }
        if run.rng.below(GROWTH_COST) < charge {
            let above = (pos.0, pos.1 + 1, pos.2);
            let state = random_growth(run, above);
            run.region.set(above.0, above.1, above.2, state);
        }
        0.max(charge - GROWTH_COST)
    }

    fn movement(&self, run: &mut Run, origin: Pos) -> Option<Pos> {
        let mut found = self.pos;
        let mut offsets = neighbours().to_vec();
        shuffle(run.rng, &mut offsets);
        for (dx, dy, dz) in offsets {
            let candidate = (self.pos.0 + dx, self.pos.1 + dy, self.pos.2 + dz);
            if !can_move_to(origin, candidate) {
                continue;
            }
            let state = at(run.region, candidate);
            if Behaviour::of(state).is_sculk_block()
                && movement_unobstructed(run.region, self.pos, candidate)
            {
                found = candidate;
                if substrate_access(run.region, state, candidate) {
                    break;
                }
            }
        }
        (found != self.pos).then_some(found)
    }

    fn update(&mut self, run: &mut Run, origin: Pos, veins: bool) {
        if self.charge <= 0 {
            return;
        }
        if self.update_delay > 0 {
            self.update_delay -= 1;
            return;
        }
        let mut state = at(run.region, self.pos);
        let mut behaviour = Behaviour::of(state);
        if veins
            && self.spread_vein(run.region, behaviour, state)
            && behaviour.changes_state_on_spread()
        {
            state = at(run.region, self.pos);
            behaviour = Behaviour::of(state);
        }
        self.charge = self.use_charge(run, behaviour, origin, veins);
        if self.charge <= 0 {
            discharge(run.region, behaviour, state, self.pos);
            return;
        }
        match self.movement(run, origin) {
            Some(next) => {
                discharge(run.region, behaviour, state, self.pos);
                self.pos = next;
                state = at(run.region, next);
            }
            None => {
                discharge(run.region, behaviour, state, self.pos);
                self.charge = 0;
                return;
            }
        }
        if Behaviour::of(state).is_sculk_block() {
            self.facings = Some(available_faces(state));
        }
        self.decay_delay = behaviour.update_decay_delay(self.decay_delay);
        self.update_delay = 1;
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct Patch {
    charges: i32,
    amount: i32,
    attempts: i32,
    growth_rounds: i32,
    spread_rounds: i32,
}

fn full_collision_cube(state: BlockStateId) -> bool {
    state.flags().contains(StateFlags::FULL_COLLISION_CUBE)
}

impl Patch {
    pub(crate) fn compile(value: &Value) -> Self {
        Self {
            charges: integer(value, "charge_count", 1),
            amount: integer(value, "amount_per_charge", 1),
            attempts: integer(value, "spread_attempts", 1),
            growth_rounds: integer(value, "growth_rounds", 0),
            spread_rounds: integer(value, "spread_rounds", 0),
        }
    }

    pub(crate) fn reach(&self) -> i32 {
        14
    }

    fn can_spread_from(&self, run: &mut Run, origin: Pos) -> bool {
        let state = at(run.region, origin);
        if Behaviour::of(state).is_sculk_block() {
            return true;
        }
        if !(state.is_air() || (state.kind() == Some(Kind::Water) && water_source(state))) {
            return false;
        }
        Face::ALL
            .into_iter()
            .any(|face| full_collision_cube(at(run.region, moved(origin, face))))
    }

    pub(crate) fn place(&self, run: &mut Run, origin: Pos) -> bool {
        if !self.can_spread_from(run, origin) {
            return false;
        }
        let rounds = self.spread_rounds + self.growth_rounds;
        for round in 0..rounds {
            let mut cursors: Vec<Cursor> = Vec::new();
            for _ in 0..self.charges {
                let mut left = self.amount;
                while left > 0 {
                    let charge = left.min(1000);
                    if cursors.len() < MAX_CURSORS {
                        cursors.push(Cursor::new(origin, charge));
                    }
                    left -= charge;
                }
            }
            let veins = round < self.spread_rounds;
            for _ in 0..self.attempts {
                if cursors.is_empty() {
                    break;
                }
                let mut kept = Vec::with_capacity(cursors.len());
                for mut cursor in cursors {
                    cursor.update(run, origin, veins);
                    if cursor.charge > 0 {
                        kept.push(cursor);
                    }
                }
                cursors = kept;
            }
        }
        true
    }
}

#[cfg(test)]
mod tests;

use rspigot_blocks::{BlockKind as Kind, Face};
use rspigot_domain::BlockStateId;
use serde_json::Value;

use super::cave::moved;
use super::floats::Float;
use super::place::{Run, max_abs};
use super::prov::{BlockSet, Int, Pos, integer, is_water, number};
use crate::climate::Xoroshiro;
use crate::overworld::region::Region;

pub(super) fn is_base(state: BlockStateId, base: Kind, replaceable: &BlockSet) -> bool {
    state.kind() == Some(base) || replaceable.contains(state)
}

pub(super) fn empty_or_water(state: BlockStateId) -> bool {
    state.is_air() || state.kind() == Some(Kind::Water)
}

pub(super) fn neither_empty_nor_water(state: BlockStateId) -> bool {
    !empty_or_water(state)
}

pub(super) fn empty_or_water_or_lava(state: BlockStateId) -> bool {
    empty_or_water(state) || state.kind() == Some(Kind::Lava)
}

pub(super) fn state_at(region: &mut Region, (x, y, z): Pos) -> BlockStateId {
    region.get(x, y, z)
}

pub(super) fn speleothem_height(
    radius_distance: f64,
    radius: f64,
    scale: f64,
    bluntness: f64,
) -> f64 {
    let distance = if radius_distance < bluntness {
        bluntness
    } else {
        radius_distance
    };
    let e = distance / radius * 0.384;
    let f = libm::pow(e, 1.333_333_333_333_333_3) * 0.75;
    let g = libm::pow(e, 0.666_666_666_666_666_6);
    let h = libm::log(e) * 0.333_333_333_333_333_3;
    let height = (scale * ((f - g) - h)).max(0.0);
    (height / 0.384) * radius
}

pub(super) fn circle_mostly_embedded(region: &mut Region, center: Pos, radius: i32) -> bool {
    if empty_or_water_or_lava(state_at(region, center)) {
        return false;
    }
    let step = 6.0_f32 / radius as f32;
    let mut angle = 0.0_f32;
    while angle < 6.283_185_5_f32 {
        let dx = (libm::cos(f64::from(angle)) as f32 * radius as f32) as i32;
        let dz = (libm::sin(f64::from(angle)) as f32 * radius as f32) as i32;
        let at = (center.0 + dx, center.1, center.2 + dz);
        if empty_or_water_or_lava(state_at(region, at)) {
            return false;
        }
        angle += step;
    }
    true
}

pub(super) fn place_base_block(
    region: &mut Region,
    pos: Pos,
    base: Kind,
    replaceable: &BlockSet,
) -> bool {
    if replaceable.contains(state_at(region, pos)) {
        region.set(pos.0, pos.1, pos.2, base.default_state());
        return true;
    }
    false
}

fn thicknesses(length: i32, merged: bool) -> Vec<&'static str> {
    let mut found = Vec::new();
    if length > 2 {
        found.push("base");
        found.extend(std::iter::repeat_n("middle", (length - 3) as usize));
    }
    if length > 1 {
        found.push("frustum");
    }
    if length > 0 {
        found.push(if merged { "tip_merge" } else { "tip" });
    }
    found
}

pub(super) struct Shape<'a> {
    pub(super) base: Kind,
    pub(super) pointed: Kind,
    pub(super) replaceable: &'a BlockSet,
}

pub(super) fn grow(
    region: &mut Region,
    start: Pos,
    tip: Face,
    height: i32,
    merged: bool,
    shape: &Shape,
) {
    let (base, pointed, replaceable) = (shape.base, shape.pointed, shape.replaceable);
    let behind = moved(start, tip.opposite());
    if !is_base(state_at(region, behind), base, replaceable) {
        return;
    }
    let direction = if tip == Face::Up { "up" } else { "down" };
    let mut at = start;
    for thickness in thicknesses(height, merged) {
        let mut state = pointed.default_state();
        state = state
            .with_property("vertical_direction", direction)
            .unwrap_or(state);
        state = state.with_property("thickness", thickness).unwrap_or(state);
        let wet = is_water(state_at(region, at));
        state = state
            .with_property("waterlogged", if wet { "true" } else { "false" })
            .unwrap_or(state);
        region.set(at.0, at.1, at.2, state);
        at = moved(at, tip);
    }
}

#[derive(Debug, Clone, Copy)]
pub(super) struct Column {
    pub(super) floor: Option<i32>,
    pub(super) ceiling: Option<i32>,
}

impl Column {
    pub(super) fn height(&self) -> Option<i32> {
        Some(self.ceiling? - self.floor? - 1)
    }
}

fn scan_direction(
    region: &mut Region,
    range: i32,
    inside: fn(BlockStateId) -> bool,
    edge: &dyn Fn(BlockStateId) -> bool,
    start: Pos,
    face: Face,
) -> Option<i32> {
    let mut at = start;
    let mut steps = 1;
    while steps < range && inside(state_at(region, at)) {
        at = moved(at, face);
        steps += 1;
    }
    edge(state_at(region, at)).then_some(at.1)
}

pub(super) fn scan(
    region: &mut Region,
    pos: Pos,
    range: i32,
    inside: fn(BlockStateId) -> bool,
    edge: &dyn Fn(BlockStateId) -> bool,
) -> Option<Column> {
    if !inside(state_at(region, pos)) {
        return None;
    }
    let ceiling = scan_direction(region, range, inside, edge, pos, Face::Up);
    let floor = scan_direction(region, range, inside, edge, pos, Face::Down);
    Some(Column { floor, ceiling })
}

fn lookup_kind(value: &Value, key: &str) -> Option<Kind> {
    Kind::from_name(value.get(key).and_then(Value::as_str)?)
}

#[derive(Debug, Clone)]
pub(crate) struct Speleothem {
    base: Kind,
    pointed: Kind,
    replaceable: BlockSet,
    taller: f32,
    directional: f32,
    radius2: f32,
    radius3: f32,
}

impl Speleothem {
    pub(crate) fn compile(value: &Value) -> Option<Self> {
        Some(Self {
            base: lookup_kind(value, "base_block")?,
            pointed: lookup_kind(value, "pointed_block")?,
            replaceable: BlockSet::compile(value.get("replaceable_blocks").unwrap_or(&Value::Null)),
            taller: number(value, "chance_of_taller_generation", 0.2) as f32,
            directional: number(value, "chance_of_directional_spread", 0.7) as f32,
            radius2: number(value, "chance_of_spread_radius2", 0.5) as f32,
            radius3: number(value, "chance_of_spread_radius3", 0.5) as f32,
        })
    }

    pub(crate) fn reach(&self) -> i32 {
        3
    }

    fn tip_direction(&self, run: &mut Run, pos: Pos) -> Option<Face> {
        let above = is_base(
            state_at(run.region, moved(pos, Face::Up)),
            self.base,
            &self.replaceable,
        );
        let below = is_base(
            state_at(run.region, moved(pos, Face::Down)),
            self.base,
            &self.replaceable,
        );
        if above && below {
            return Some(if run.rng.next_boolean() {
                Face::Down
            } else {
                Face::Up
            });
        }
        if above {
            return Some(Face::Down);
        }
        below.then_some(Face::Up)
    }

    fn patch(&self, run: &mut Run, pos: Pos) {
        place_base_block(run.region, pos, self.base, &self.replaceable);
        for face in Face::HORIZONTAL {
            if run.rng.next_float() > self.directional {
                continue;
            }
            let second = moved(pos, face);
            place_base_block(run.region, second, self.base, &self.replaceable);
            if run.rng.next_float() > self.radius2 {
                continue;
            }
            let third = moved(second, random_face(run.rng));
            place_base_block(run.region, third, self.base, &self.replaceable);
            if run.rng.next_float() > self.radius3 {
                continue;
            }
            let fourth = moved(third, random_face(run.rng));
            place_base_block(run.region, fourth, self.base, &self.replaceable);
        }
    }

    pub(crate) fn place(&self, run: &mut Run, origin: Pos) -> bool {
        let Some(tip) = self.tip_direction(run, origin) else {
            return false;
        };
        self.patch(run, moved(origin, tip.opposite()));
        let mut height = 1;
        if run.rng.next_float() < self.taller {
            let next = state_at(run.region, moved(origin, tip));
            if empty_or_water(next) {
                height = 2;
            }
        }
        grow(
            run.region,
            origin,
            tip,
            height,
            false,
            &Shape {
                base: self.base,
                pointed: self.pointed,
                replaceable: &self.replaceable,
            },
        );
        true
    }
}

pub(super) fn random_face(rng: &mut Xoroshiro) -> Face {
    Face::ALL[rng.below(6) as usize]
}

#[derive(Debug, Clone)]
pub(crate) struct Cluster {
    base: Kind,
    pointed: Kind,
    replaceable: BlockSet,
    search: i32,
    height: Int,
    radius: Int,
    height_difference: i32,
    deviation: i32,
    thickness: Int,
    density: Float,
    wetness: Float,
    edge_chance: f32,
    edge_distance: i32,
    bias_distance: i32,
}

fn clamped_map_float(value: f32, in_min: f32, in_max: f32, out_min: f32, out_max: f32) -> f32 {
    let t = (value - in_min) / (in_max - in_min);
    if t < 0.0 {
        out_min
    } else if t > 1.0 {
        out_max
    } else {
        out_min + t * (out_max - out_min)
    }
}

fn clamped_map_double(value: f64, in_min: f64, in_max: f64, out_min: f64, out_max: f64) -> f64 {
    let t = (value - in_min) / (in_max - in_min);
    if t < 0.0 {
        out_min
    } else if t > 1.0 {
        out_max
    } else {
        out_min + t * (out_max - out_min)
    }
}

impl Cluster {
    pub(crate) fn compile(value: &Value) -> Option<Self> {
        let int = |key: &str| Int::compile(value.get(key).unwrap_or(&Value::Null));
        let float = |key: &str| Float::compile(value.get(key).unwrap_or(&Value::Null));
        Some(Self {
            base: lookup_kind(value, "base_block")?,
            pointed: lookup_kind(value, "pointed_block")?,
            replaceable: BlockSet::compile(value.get("replaceable_blocks").unwrap_or(&Value::Null)),
            search: integer(value, "floor_to_ceiling_search_range", 12),
            height: int("height"),
            radius: int("radius"),
            height_difference: integer(value, "max_stalagmite_stalactite_height_diff", 1),
            deviation: integer(value, "height_deviation", 1),
            thickness: int("speleothem_block_layer_thickness"),
            density: float("density"),
            wetness: float("wetness"),
            edge_chance: number(
                value,
                "chance_of_speleothem_at_max_distance_from_center",
                0.1,
            ) as f32,
            edge_distance: integer(
                value,
                "max_distance_from_edge_affecting_chance_of_speleothem",
                1,
            ),
            bias_distance: integer(value, "max_distance_from_center_affecting_height_bias", 1),
        })
    }

    pub(crate) fn reach(&self) -> i32 {
        max_abs(&self.radius)
    }

    fn lava(region: &mut Region, pos: Pos) -> bool {
        state_at(region, pos).kind() == Some(Kind::Lava)
    }

    fn adjacent_to_water(region: &mut Region, pos: Pos) -> bool {
        use std::sync::OnceLock;
        static STONE: OnceLock<BlockSet> = OnceLock::new();
        let state = state_at(region, pos);
        STONE
            .get_or_init(|| BlockSet::tag("minecraft:base_stone_overworld"))
            .contains(state)
            || is_water(state)
    }

    fn can_place_pool(&self, region: &mut Region, pos: Pos) -> bool {
        let state = state_at(region, pos);
        if state.kind() == Some(Kind::Water)
            || state.kind() == Some(self.base)
            || state.kind() == Some(self.pointed)
        {
            return false;
        }
        if is_water(state_at(region, (pos.0, pos.1 + 1, pos.2))) {
            return false;
        }
        for face in Face::HORIZONTAL {
            if !Self::adjacent_to_water(region, moved(pos, face)) {
                return false;
            }
        }
        Self::adjacent_to_water(region, (pos.0, pos.1 - 1, pos.2))
    }

    fn replace_blocks(&self, region: &mut Region, first: Pos, count: i32, face: Face) {
        let mut at = first;
        for _ in 0..count {
            if !place_base_block(region, at, self.base, &self.replaceable) {
                break;
            }
            at = moved(at, face);
        }
    }

    fn chance_at(&self, rx: i32, rz: i32, dx: i32, dz: i32) -> f64 {
        let nearest = (rx - dx.abs()).min(rz - dz.abs());
        f64::from(clamped_map_float(
            nearest as f32,
            0.0,
            self.edge_distance as f32,
            self.edge_chance,
            1.0,
        ))
    }

    fn height_at(&self, rng: &mut Xoroshiro, dx: i32, dz: i32, density: f32, max: i32) -> i32 {
        if rng.next_float() > density {
            return 0;
        }
        let distance = dx.abs() + dz.abs();
        let mean = clamped_map_double(
            f64::from(distance),
            0.0,
            f64::from(self.bias_distance),
            f64::from(max) / 2.0,
            0.0,
        ) as f32;
        let sampled =
            (mean + rng.next_gaussian() as f32 * self.deviation as f32).clamp(0.0, max as f32);
        sampled as i32
    }

    #[allow(clippy::too_many_arguments)]
    fn column(
        &self,
        run: &mut Run,
        pos: Pos,
        dx: i32,
        dz: i32,
        wetness: f32,
        chance: f64,
        cluster_height: i32,
        density: f32,
    ) {
        let Some(found) = scan(
            run.region,
            pos,
            self.search,
            empty_or_water,
            &neither_empty_nor_water,
        ) else {
            return;
        };
        let ceiling = found.ceiling;
        if ceiling.is_none() && found.floor.is_none() {
            return;
        }
        let mut column = found;
        let pool = run.rng.next_float() < wetness;
        if pool
            && let Some(floor) = found.floor
            && self.can_place_pool(run.region, (pos.0, floor, pos.2))
        {
            column.floor = Some(floor - 1);
            run.region
                .set(pos.0, floor, pos.2, Kind::Water.default_state());
        }
        let floor = column.floor;
        let mut hanging = 0;
        let wants = run.rng.next_double() < chance;
        if let Some(top) = ceiling
            && wants
            && !Self::lava(run.region, (pos.0, top, pos.2))
        {
            let layers = self.thickness.sample(run.rng);
            self.replace_blocks(run.region, (pos.0, top, pos.2), layers, Face::Up);
            let max = match floor {
                Some(bottom) => cluster_height.min(top - bottom),
                None => cluster_height,
            };
            hanging = self.height_at(run.rng, dx, dz, density, max);
        }
        let mut standing = 0;
        let wants = run.rng.next_double() < chance;
        if let Some(bottom) = floor
            && wants
            && !Self::lava(run.region, (pos.0, bottom, pos.2))
        {
            let layers = self.thickness.sample(run.rng);
            self.replace_blocks(run.region, (pos.0, bottom, pos.2), layers, Face::Down);
            standing = if ceiling.is_some() {
                let low = -self.height_difference;
                let high = self.height_difference;
                (hanging + run.rng.below(high - low + 1) + low).max(0)
            } else {
                self.height_at(run.rng, dx, dz, density, cluster_height)
            };
        }
        if let (Some(top), Some(bottom)) = (ceiling, floor)
            && top - hanging <= bottom + standing
        {
            let lowest = (top - hanging).max(bottom + 1);
            let highest = (bottom + standing).min(top - 1);
            let meet = run.rng.below(highest + 1 - lowest + 1) + lowest;
            hanging = top - meet;
            standing = (meet - 1) - bottom;
        }
        let merged = run.rng.next_boolean()
            && hanging > 0
            && standing > 0
            && column
                .height()
                .is_some_and(|height| hanging + standing == height);
        if let Some(top) = ceiling {
            grow(
                run.region,
                (pos.0, top - 1, pos.2),
                Face::Down,
                hanging,
                merged,
                &Shape {
                    base: self.base,
                    pointed: self.pointed,
                    replaceable: &self.replaceable,
                },
            );
        }
        if let Some(bottom) = floor {
            grow(
                run.region,
                (pos.0, bottom + 1, pos.2),
                Face::Up,
                standing,
                merged,
                &Shape {
                    base: self.base,
                    pointed: self.pointed,
                    replaceable: &self.replaceable,
                },
            );
        }
    }

    pub(crate) fn place(&self, run: &mut Run, origin: Pos) -> bool {
        if !empty_or_water(state_at(run.region, origin)) {
            return false;
        }
        let cluster_height = self.height.sample(run.rng);
        let wetness = self.wetness.sample(run.rng);
        let density = self.density.sample(run.rng);
        let rx = self.radius.sample(run.rng);
        let rz = self.radius.sample(run.rng);
        for dx in -rx..=rx {
            for dz in -rz..=rz {
                let chance = self.chance_at(rx, rz, dx, dz);
                let pos = (origin.0 + dx, origin.1, origin.2 + dz);
                self.column(run, pos, dx, dz, wetness, chance, cluster_height, density);
            }
        }
        true
    }
}

#[cfg(test)]
mod tests;

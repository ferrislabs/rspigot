use rspigot_blocks::BlockKind as Kind;
use rspigot_domain::BlockStateId;
use serde_json::Value;

use super::place::Run;
use super::prov::{Pos, Pred};
use crate::datapack::block_state;

fn is(state: BlockStateId, kind: Kind) -> bool {
    state.kind() == Some(kind)
}

fn put(run: &mut Run, (x, y, z): Pos, state: BlockStateId) {
    run.region.set(x, y, z, state);
}

fn ceil(value: f32) -> i32 {
    let truncated = value as i32;
    if value > truncated as f32 {
        truncated + 1
    } else {
        truncated
    }
}

#[derive(Debug, Clone)]
pub(crate) struct Spike {
    state: BlockStateId,
    can_place_on: Pred,
    can_replace: Pred,
}

impl Spike {
    pub(crate) fn compile(value: &Value) -> Self {
        Self {
            state: value
                .get("state")
                .and_then(block_state)
                .unwrap_or(BlockStateId::AIR),
            can_place_on: Pred::compile(value.get("can_place_on").unwrap_or(&Value::Null)),
            can_replace: Pred::compile(value.get("can_replace").unwrap_or(&Value::Null)),
        }
    }

    pub(crate) fn reach(&self) -> i32 {
        3
    }

    fn replaceable(&self, run: &mut Run, pos: Pos) -> bool {
        run.region.get(pos.0, pos.1, pos.2).is_air() || self.can_replace.test(run.region, pos)
    }

    pub(crate) fn place(&self, run: &mut Run, origin: Pos) -> bool {
        let (x, mut y, z) = origin;
        let floor = run.region.height().min_y + 2;
        while run.region.get(x, y, z).is_air() && y > floor {
            y -= 1;
        }
        if !self.can_place_on.test(run.region, (x, y, z)) {
            return false;
        }
        y += run.rng.below(4);
        let height = run.rng.below(4) + 7;
        let width = height / 4 + run.rng.below(2);
        if width > 1 && run.rng.below(60) == 0 {
            y += 10 + run.rng.below(30);
        }
        for step in 0..height {
            let radius = (1.0 - step as f32 / height as f32) * width as f32;
            let reach = ceil(radius);
            for dx in -reach..=reach {
                let fx = dx.abs() as f32 - 0.25;
                for dz in -reach..=reach {
                    let fz = dz.abs() as f32 - 0.25;
                    if (dx != 0 || dz != 0) && fx * fx + fz * fz > radius * radius {
                        continue;
                    }
                    let edge = dx == -reach || dx == reach || dz == -reach || dz == reach;
                    if edge && run.rng.next_float() > 0.75 {
                        continue;
                    }
                    let up = (x + dx, y + step, z + dz);
                    if self.replaceable(run, up) {
                        put(run, up, self.state);
                    }
                    if step != 0 && reach > 1 {
                        let down = (x + dx, y - step, z + dz);
                        if self.replaceable(run, down) {
                            put(run, down, self.state);
                        }
                    }
                }
            }
        }
        let ring = (width - 1).clamp(0, 1);
        for dx in -ring..=ring {
            for dz in -ring..=ring {
                let mut at = (x + dx, y - 1, z + dz);
                let mut left = 50;
                if dx.abs() == 1 && dz.abs() == 1 {
                    left = run.rng.below(5);
                }
                while at.1 > 50 {
                    let here = run.region.get(at.0, at.1, at.2);
                    if !here.is_air()
                        && !self.can_replace.test(run.region, at)
                        && here != self.state
                    {
                        break;
                    }
                    put(run, at, self.state);
                    at.1 -= 1;
                    left -= 1;
                    if left <= 0 {
                        at.1 -= run.rng.below(5) + 1;
                        left = run.rng.below(5);
                    }
                }
            }
        }
        true
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct BlueIce;

impl BlueIce {
    pub(crate) fn reach(&self) -> i32 {
        2
    }

    fn absorbs(state: BlockStateId) -> bool {
        state.is_air()
            || is(state, Kind::Water)
            || is(state, Kind::PackedIce)
            || is(state, Kind::Ice)
    }

    pub(crate) fn place(&self, run: &mut Run, origin: Pos) -> bool {
        const FACES: [(i32, i32, i32); 6] = [
            (0, -1, 0),
            (0, 1, 0),
            (0, 0, -1),
            (0, 0, 1),
            (-1, 0, 0),
            (1, 0, 0),
        ];
        let (x, y, z) = origin;
        if y > run.region.sea_level() - 1 {
            return false;
        }
        if !is(run.region.get(x, y, z), Kind::Water)
            && !is(run.region.get(x, y - 1, z), Kind::Water)
        {
            return false;
        }
        let mut packed = false;
        for (dx, dy, dz) in FACES.into_iter().skip(1) {
            if is(run.region.get(x + dx, y + dy, z + dz), Kind::PackedIce) {
                packed = true;
            }
        }
        if !packed {
            return false;
        }
        let blue = Kind::BlueIce.default_state();
        put(run, origin, blue);
        for _ in 0..200 {
            let rise = run.rng.below(5) - run.rng.below(6);
            let spread = if rise < 2 { rise / 2 + 3 } else { 3 };
            if spread <= 0 {
                continue;
            }
            let a = run.rng.below(spread);
            let b = run.rng.below(spread);
            let c = run.rng.below(spread);
            let d = run.rng.below(spread);
            let at = (x + a - b, y + rise, z + c - d);
            if !Self::absorbs(run.region.get(at.0, at.1, at.2)) {
                continue;
            }
            let touches = FACES.iter().any(|(dx, dy, dz)| {
                is(
                    run.region.get(at.0 + dx, at.1 + dy, at.2 + dz),
                    Kind::BlueIce,
                )
            });
            if touches {
                put(run, at, blue);
            }
        }
        true
    }
}

#[derive(Debug, Clone)]
pub(crate) struct Iceberg {
    state: BlockStateId,
}

#[derive(Clone, Copy)]
struct Shape {
    ellipse: bool,
    snow_on_top: bool,
    angle: f64,
    a: i32,
    c: i32,
    main: BlockStateId,
}

#[derive(Clone, Copy)]
struct Cut {
    angle: f64,
    local: (i32, i32),
    a: i32,
    c: i32,
}

fn iceberg_state(state: BlockStateId) -> bool {
    is(state, Kind::PackedIce) || is(state, Kind::SnowBlock) || is(state, Kind::BlueIce)
}

fn distance_ellipse((xo, zo): (i32, i32), (ox, oz): (i32, i32), a: i32, c: i32, angle: f64) -> f64 {
    let dx = f64::from(xo - ox);
    let dz = f64::from(zo - oz);
    let (sin, cos) = (libm::sin(angle), libm::cos(angle));
    let along = (dx * cos - dz * sin) / f64::from(a);
    let across = (dx * sin + dz * cos) / f64::from(c);
    along * along + across * across - 1.0
}

fn distance_circle(run: &mut Run, xo: i32, zo: i32, radius: i32) -> f64 {
    let jitter = (10.0_f32 * run.rng.next_float().clamp(0.2, 0.8)) / radius as f32;
    let (x, z, r) = (f64::from(xo), f64::from(zo), f64::from(radius));
    f64::from(jitter) + x * x + z * z - r * r
}

fn radius_round(run: &mut Run, y: i32, height: i32, width: i32) -> i32 {
    let f = 3.5_f32 - run.rng.next_float();
    let mut value = (1.0_f32 - (y * y) as f32 / (height as f32 * f)) * width as f32;
    if height > 15 + run.rng.below(5) {
        let level = if y < 3 + run.rng.below(6) { y / 2 } else { y };
        value = (1.0_f32 - level as f32 / (height as f32 * f * 0.4_f32)) * width as f32;
    }
    ceil(value / 2.0)
}

fn radius_ellipse(y: i32, height: i32, width: i32) -> i32 {
    let value = (1.0_f32 - (y * y) as f32 / (height as f32 * 1.0_f32)) * width as f32;
    ceil(value / 2.0)
}

fn radius_steep(run: &mut Run, y: i32, height: i32, width: i32) -> i32 {
    let f = 1.0_f32 + run.rng.next_float() / 2.0;
    let value = (1.0_f32 - y as f32 / (height as f32 * f)) * width as f32;
    ceil(value / 2.0)
}

impl Iceberg {
    pub(crate) fn compile(value: &Value) -> Self {
        Self {
            state: value
                .get("state")
                .and_then(block_state)
                .unwrap_or(BlockStateId::AIR),
        }
    }

    pub(crate) fn reach(&self) -> i32 {
        11
    }

    fn ellipse_c(y: i32, height: i32, c: i32) -> i32 {
        if y > 0 && height - y <= 3 {
            c - (4 - (height - y))
        } else {
            c
        }
    }

    fn set_block(run: &mut Run, origin: Pos, height: i32, (xo, yo, zo): Pos, shape: Shape) {
        let here = (origin.0 + xo, origin.1 + yo, origin.2 + zo);
        let state = run.region.get(here.0, here.1, here.2);
        if !(state.is_air()
            || is(state, Kind::SnowBlock)
            || is(state, Kind::Ice)
            || is(state, Kind::Water))
        {
            return;
        }
        let fresh = !shape.ellipse || run.rng.next_double() > 0.05;
        let divisor = if shape.ellipse { 3 } else { 2 };
        if shape.snow_on_top && !is(state, Kind::Water) {
            let roll = run.rng.below(1.max(height / divisor));
            if f64::from(height - yo) <= f64::from(roll) + f64::from(height) * 0.6 && fresh {
                put(run, here, Kind::SnowBlock.default_state());
                return;
            }
        }
        put(run, here, shape.main);
    }

    fn block(
        run: &mut Run,
        origin: Pos,
        height: i32,
        (xo, yo, zo): Pos,
        radius: i32,
        a: i32,
        shape: Shape,
    ) {
        let distance = if shape.ellipse {
            let c = Self::ellipse_c(yo, height, shape.c);
            distance_ellipse((xo, zo), (0, 0), a, c, shape.angle)
        } else {
            distance_circle(run, xo, zo, radius)
        };
        if distance.is_nan() || distance >= 0.0 {
            return;
        }
        let threshold = if shape.ellipse {
            -0.5
        } else {
            f64::from(-6 - run.rng.below(3))
        };
        if distance > threshold && run.rng.next_double() > 0.9 {
            return;
        }
        Self::set_block(run, origin, height, (xo, yo, zo), shape);
    }

    fn below_is_air(run: &mut Run, (x, y, z): Pos) -> bool {
        run.region.get(x, y - 1, z).is_air()
    }

    fn smooth(run: &mut Run, origin: Pos, width: i32, height: i32, ellipse: bool, a: i32) {
        let reach = if ellipse { a } else { width / 2 };
        for xo in -reach..=reach {
            for zo in -reach..=reach {
                for yo in 0..=height {
                    let at = (origin.0 + xo, origin.1 + yo, origin.2 + zo);
                    let state = run.region.get(at.0, at.1, at.2);
                    if !(iceberg_state(state) || is(state, Kind::Snow)) {
                        continue;
                    }
                    if Self::below_is_air(run, at) {
                        put(run, at, BlockStateId::AIR);
                        put(run, (at.0, at.1 + 1, at.2), BlockStateId::AIR);
                    } else if iceberg_state(state) {
                        let open = [(-1, 0), (1, 0), (0, -1), (0, 1)]
                            .iter()
                            .filter(|(dx, dz)| {
                                !iceberg_state(run.region.get(at.0 + dx, at.1, at.2 + dz))
                            })
                            .count();
                        if open >= 3 {
                            put(run, at, BlockStateId::AIR);
                        }
                    }
                }
            }
        }
    }

    fn carve(run: &mut Run, radius: i32, yo: i32, origin: Pos, under_water: bool, cut: Cut) {
        let wide = radius + 1 + cut.a / 3;
        let tall = (radius - 3).min(3) + cut.c / 2 - 1;
        for xo in -wide..wide {
            for zo in -wide..wide {
                let inside = distance_ellipse((xo, zo), cut.local, wide, tall, cut.angle) < 0.0;
                if !inside {
                    continue;
                }
                let at = (origin.0 + xo, origin.1 + yo, origin.2 + zo);
                let state = run.region.get(at.0, at.1, at.2);
                if !(iceberg_state(state) || is(state, Kind::SnowBlock)) {
                    continue;
                }
                if under_water {
                    put(run, at, Kind::Water.default_state());
                } else {
                    put(run, at, BlockStateId::AIR);
                    let above = (at.0, at.1 + 1, at.2);
                    if is(run.region.get(above.0, above.1, above.2), Kind::Snow) {
                        put(run, above, BlockStateId::AIR);
                    }
                }
            }
        }
    }

    fn cut_out(run: &mut Run, origin: Pos, width: i32, height: i32, shape: Shape) {
        let sign_x = if run.rng.next_boolean() { -1 } else { 1 };
        let sign_z = if run.rng.next_boolean() { -1 } else { 1 };
        let half = width / 2;
        let mut x = run.rng.below(1.max(half - 2));
        if run.rng.next_boolean() {
            x = half + 1 - run.rng.below(1.max(width - half - 1));
        }
        let mut z = run.rng.below(1.max(half - 2));
        if run.rng.next_boolean() {
            z = half + 1 - run.rng.below(1.max(width - half - 1));
        }
        if shape.ellipse {
            x = run.rng.below(1.max(shape.a - 5));
            z = x;
        }
        let local = (sign_x * x, sign_z * z);
        let angle = if shape.ellipse {
            shape.angle + std::f64::consts::FRAC_PI_2
        } else {
            run.rng.next_double() * 2.0 * std::f64::consts::PI
        };
        let cut = Cut {
            angle,
            local,
            a: shape.a,
            c: shape.c,
        };
        for yo in 0..(height - 3) {
            let radius = radius_round(run, yo, height, width);
            Self::carve(run, radius, yo, origin, false, cut);
        }
        let mut yo = -1;
        while yo > -height + run.rng.below(5) {
            let radius = radius_steep(run, -yo, height, width);
            Self::carve(run, radius, yo, origin, true, cut);
            yo -= 1;
        }
    }

    pub(crate) fn place(&self, run: &mut Run, at: Pos) -> bool {
        let origin = (at.0, run.region.sea_level(), at.2);
        let snow_on_top = run.rng.next_double() > 0.7;
        let angle = run.rng.next_double() * 2.0 * std::f64::consts::PI;
        let a = 11 - run.rng.below(5);
        let c = 3 + run.rng.below(3);
        let ellipse = run.rng.next_double() > 0.7;
        let mut over = if ellipse {
            run.rng.below(6) + 6
        } else {
            run.rng.below(15) + 3
        };
        if !ellipse && run.rng.next_double() > 0.9 {
            over += run.rng.below(19) + 7;
        }
        let under = (over + run.rng.below(11)).min(18);
        let width = (over + run.rng.below(7) - run.rng.below(5)).min(11);
        let span = if ellipse { a } else { 11 };
        let shape = Shape {
            ellipse,
            snow_on_top,
            angle,
            a,
            c,
            main: self.state,
        };
        for xo in -span..span {
            for zo in -span..span {
                for yo in 0..over {
                    let radius = if ellipse {
                        radius_ellipse(yo, over, width)
                    } else {
                        radius_round(run, yo, over, width)
                    };
                    if ellipse || xo < radius {
                        Self::block(run, origin, over, (xo, yo, zo), radius, span, shape);
                    }
                }
            }
        }
        Self::smooth(run, origin, width, over, ellipse, a);
        for xo in -span..span {
            for zo in -span..span {
                let mut yo = -1;
                while yo > -under {
                    let wide = if ellipse {
                        ceil(span as f32 * (1.0_f32 - (yo * yo) as f32 / (under as f32 * 8.0_f32)))
                    } else {
                        span
                    };
                    let radius = radius_steep(run, -yo, under, width);
                    if xo < radius {
                        Self::block(run, origin, under, (xo, yo, zo), radius, wide, shape);
                    }
                    yo -= 1;
                }
            }
        }
        let cut = if ellipse {
            run.rng.next_double() > 0.1
        } else {
            run.rng.next_double() > 0.7
        };
        if cut {
            Self::cut_out(run, origin, width, over, shape);
        }
        true
    }
}

#[cfg(test)]
mod tests;

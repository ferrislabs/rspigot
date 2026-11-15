use std::collections::HashSet;
use std::sync::OnceLock;

use rspigot_domain::BlockStateId;
use serde_json::Value;

use super::place::Run;
use super::prov::{BlockSet, Int, Pos, State, flag, integer, short};
use super::treedeco::{self, Decorator};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Trunk {
    Straight,
    Forking,
    Giant,
    MegaJungle,
    DarkOak,
    Fancy,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Crown {
    Blob,
    Spruce,
    Pine,
    Acacia,
    Bush,
    Jungle,
    MegaPine,
    Fancy,
    DarkOak,
}

#[derive(Debug, Clone)]
enum Size {
    Two {
        limit: i32,
        lower: i32,
        upper: i32,
        clipped: Option<i32>,
    },
    Three {
        limit: i32,
        upper_limit: i32,
        lower: i32,
        middle: i32,
        upper: i32,
        clipped: Option<i32>,
    },
}

impl Size {
    fn at(&self, height: i32, y: i32) -> i32 {
        match self {
            Self::Two {
                limit,
                lower,
                upper,
                ..
            } => {
                if y < *limit {
                    *lower
                } else {
                    *upper
                }
            }
            Self::Three {
                limit,
                upper_limit,
                lower,
                middle,
                upper,
                ..
            } => {
                if y < *limit {
                    *lower
                } else if y >= height - upper_limit {
                    *upper
                } else {
                    *middle
                }
            }
        }
    }

    fn clipped(&self) -> Option<i32> {
        match self {
            Self::Two { clipped, .. } | Self::Three { clipped, .. } => *clipped,
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct Tree {
    trunk: Trunk,
    base_height: i32,
    rand_a: i32,
    rand_b: i32,
    crown: Crown,
    radius: Int,
    offset: Int,
    height: Int,
    trunk_height: Int,
    size: Size,
    log: State,
    leaves: State,
    below: State,
    ignore_vines: bool,
    decorators: Vec<Decorator>,
}

pub(crate) fn compile(value: &Value) -> Option<Tree> {
    let trunk_value = value.get("trunk_placer")?;
    let trunk = match short(trunk_value) {
        "straight_trunk_placer" => Trunk::Straight,
        "forking_trunk_placer" => Trunk::Forking,
        "giant_trunk_placer" => Trunk::Giant,
        "mega_jungle_trunk_placer" => Trunk::MegaJungle,
        "dark_oak_trunk_placer" => Trunk::DarkOak,
        "fancy_trunk_placer" => Trunk::Fancy,
        _ => return None,
    };
    if value.get("root_placer").is_some() {
        return None;
    }
    let crown_value = value.get("foliage_placer")?;
    let crown = match short(crown_value) {
        "blob_foliage_placer" => Crown::Blob,
        "spruce_foliage_placer" => Crown::Spruce,
        "pine_foliage_placer" => Crown::Pine,
        "acacia_foliage_placer" => Crown::Acacia,
        "bush_foliage_placer" => Crown::Bush,
        "jungle_foliage_placer" => Crown::Jungle,
        "mega_pine_foliage_placer" => Crown::MegaPine,
        "fancy_foliage_placer" => Crown::Fancy,
        "dark_oak_foliage_placer" => Crown::DarkOak,
        _ => return None,
    };
    let provider = |key: &str| Int::compile(crown_value.get(key).unwrap_or(&Value::Null));
    let size_value = value.get("minimum_size")?;
    let clipped = size_value
        .get("min_clipped_height")
        .and_then(Value::as_i64)
        .map(|n| n as i32);
    let size = if short(size_value) == "three_layers_feature_size" {
        Size::Three {
            limit: integer(size_value, "limit", 1),
            upper_limit: integer(size_value, "upper_limit", 1),
            lower: integer(size_value, "lower_size", 0),
            middle: integer(size_value, "middle_size", 1),
            upper: integer(size_value, "upper_size", 1),
            clipped,
        }
    } else {
        Size::Two {
            limit: integer(size_value, "limit", 1),
            lower: integer(size_value, "lower_size", 0),
            upper: integer(size_value, "upper_size", 1),
            clipped,
        }
    };
    Some(Tree {
        trunk,
        base_height: integer(trunk_value, "base_height", 0),
        rand_a: integer(trunk_value, "height_rand_a", 0),
        rand_b: integer(trunk_value, "height_rand_b", 0),
        crown,
        radius: provider("radius"),
        offset: provider("offset"),
        height: if crown == Crown::MegaPine {
            provider("crown_height")
        } else {
            provider("height")
        },
        trunk_height: provider("trunk_height"),
        size,
        log: State::compile(value.get("trunk_provider")?),
        leaves: State::compile(value.get("foliage_provider")?),
        below: State::compile(value.get("below_trunk_provider").unwrap_or(&Value::Null)),
        ignore_vines: flag(value, "ignore_vines", false),
        decorators: treedeco::compile_all(value),
    })
}

fn replaceable_by_trees(state: BlockStateId) -> bool {
    static SET: OnceLock<BlockSet> = OnceLock::new();
    state.is_air()
        || SET
            .get_or_init(|| BlockSet::tag("minecraft:replaceable_by_trees"))
            .contains(state)
}

#[derive(Clone, Copy)]
struct Attachment {
    pos: Pos,
    radius_offset: i32,
    double: bool,
}

struct Site<'a, 'b, 'r> {
    tree: &'a Tree,
    run: &'a mut Run<'b, 'r>,
    logs: Vec<Pos>,
    leaves: Vec<Pos>,
}

const HORIZONTAL: [(i32, i32); 4] = [(0, -1), (1, 0), (0, 1), (-1, 0)];

impl Site<'_, '_, '_> {
    fn valid(&mut self, pos: Pos) -> bool {
        replaceable_by_trees(self.run.region.get(pos.0, pos.1, pos.2))
    }

    fn put_log(&mut self, pos: Pos, axis: Option<&str>) -> bool {
        if !self.valid(pos) {
            return false;
        }
        let Some(mut state) = self.tree.log.resolve(self.run.region, self.run.rng, pos) else {
            return false;
        };
        if let Some(axis) = axis {
            state = state.with_property("axis", axis).unwrap_or(state);
        }
        self.run.region.set(pos.0, pos.1, pos.2, state);
        self.logs.push(pos);
        true
    }

    fn log(&mut self, pos: Pos) -> bool {
        self.put_log(pos, None)
    }

    fn log_if_free(&mut self, pos: Pos) {
        if self.valid(pos) {
            self.log(pos);
        }
    }

    fn dirt(&mut self, pos: Pos) {
        if let Some(state) = self.tree.below.resolve(self.run.region, self.run.rng, pos) {
            self.run.region.set(pos.0, pos.1, pos.2, state);
        }
    }

    fn dirt_square(&mut self, (x, y, z): Pos) {
        for (dx, dz) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
            self.dirt((x + dx, y - 1, z + dz));
        }
    }

    fn leaf(&mut self, pos: Pos) {
        if !self.valid(pos) {
            return;
        }
        let Some(mut state) = self.tree.leaves.resolve(self.run.region, self.run.rng, pos) else {
            return;
        };
        if state.property("waterlogged").is_some() {
            let wet = super::prov::is_water(self.run.region.get(pos.0, pos.1, pos.2));
            state = state
                .with_property("waterlogged", if wet { "true" } else { "false" })
                .unwrap_or(state);
        }
        self.run.region.set(pos.0, pos.1, pos.2, state);
        self.leaves.push(pos);
    }

    fn skip(&mut self, dx: i32, y: i32, dz: i32, range: i32, double: bool) -> bool {
        let (sx, sz) = if double {
            (dx.abs().min((dx - 1).abs()), dz.abs().min((dz - 1).abs()))
        } else {
            (dx.abs(), dz.abs())
        };
        if self.tree.crown == Crown::DarkOak
            && y == 0
            && double
            && (dx == -range || dx >= range)
            && (dz == -range || dz >= range)
        {
            return true;
        }
        match self.tree.crown {
            Crown::Blob => sx == range && sz == range && (self.run.rng.below(2) == 0 || y == 0),
            Crown::Bush => sx == range && sz == range && self.run.rng.below(2) == 0,
            Crown::Spruce | Crown::Pine => sx == range && sz == range && range > 0,
            Crown::Acacia => {
                if y == 0 {
                    (sx > 1 || sz > 1) && sx != 0 && sz != 0
                } else {
                    sx == range && sz == range && range > 0
                }
            }
            Crown::Jungle | Crown::MegaPine => {
                if sx + sz >= 7 {
                    true
                } else {
                    sx * sx + sz * sz > range * range
                }
            }
            Crown::Fancy => {
                let (a, b) = (sx as f32 + 0.5, sz as f32 + 0.5);
                a * a + b * b > (range * range) as f32
            }
            Crown::DarkOak => {
                if y == -1 && !double {
                    sx == range && sz == range
                } else if y == 1 {
                    sx + sz > range * 2 - 2
                } else {
                    false
                }
            }
        }
    }

    fn row(&mut self, origin: Pos, range: i32, y: i32, double: bool) {
        let extra = i32::from(double);
        for dx in -range..=(range + extra) {
            for dz in -range..=(range + extra) {
                if !self.skip(dx, y, dz, range, double) {
                    self.leaf((origin.0 + dx, origin.1 + y, origin.2 + dz));
                }
            }
        }
    }

    fn foliage(&mut self, attachment: Attachment, foliage_height: i32, radius: i32) {
        let offset = self.tree.offset.sample(self.run.rng);
        let Attachment {
            pos,
            radius_offset,
            double,
        } = attachment;
        match self.tree.crown {
            Crown::Blob | Crown::Bush | Crown::Fancy => {
                let mut i = offset;
                while i >= offset - foliage_height {
                    let range = match self.tree.crown {
                        Crown::Blob => (radius + radius_offset - 1 - i / 2).max(0),
                        Crown::Bush => radius + radius_offset - 1 - i,
                        _ => radius + i32::from(i != offset && i != offset - foliage_height),
                    };
                    self.row(pos, range, i, double);
                    i -= 1;
                }
            }
            Crown::Spruce => {
                let mut i = self.run.rng.below(2);
                let mut j = 1;
                let mut k = 0;
                let mut l = offset;
                while l >= -foliage_height {
                    self.row(pos, i, l, double);
                    if i >= j {
                        i = k;
                        k = 1;
                        j = (j + 1).min(radius + radius_offset);
                    } else {
                        i += 1;
                    }
                    l -= 1;
                }
            }
            Crown::Pine => {
                let mut i = 0;
                let mut j = offset;
                while j >= offset - foliage_height {
                    self.row(pos, i, j, double);
                    if i >= 1 && j == offset - foliage_height + 1 {
                        i -= 1;
                    } else if i < radius + radius_offset {
                        i += 1;
                    }
                    j -= 1;
                }
            }
            Crown::Acacia => {
                let at = (pos.0, pos.1 + offset, pos.2);
                self.row(at, radius + radius_offset, -1 - foliage_height, double);
                self.row(at, radius - 1, -foliage_height, double);
                self.row(at, radius + radius_offset - 1, 0, double);
            }
            Crown::Jungle => {
                let height = if double {
                    foliage_height
                } else {
                    1 + self.run.rng.below(2)
                };
                let mut j = offset;
                while j >= offset - height {
                    self.row(pos, radius + radius_offset + 1 - j, j, double);
                    j -= 1;
                }
            }
            Crown::MegaPine => {
                let base = (pos.0, pos.1 + offset, pos.2);
                let mut last = 0;
                let mut j = base.1 - foliage_height + offset;
                while j <= base.1 + offset {
                    let k = base.1 - j;
                    let l = radius
                        + radius_offset
                        + (k as f32 / foliage_height as f32 * 3.5).floor() as i32;
                    let width = if k > 0 && l == last && (j & 1) == 0 {
                        l + 1
                    } else {
                        l
                    };
                    self.row((base.0, j, base.2), width, 0, double);
                    last = l;
                    j += 1;
                }
            }
            Crown::DarkOak => {
                let at = (pos.0, pos.1 + offset, pos.2);
                if double {
                    self.row(at, radius + 2, -1, double);
                    self.row(at, radius + 3, 0, double);
                    self.row(at, radius + 2, 1, double);
                    if self.run.rng.next_boolean() {
                        self.row(at, radius, 2, double);
                    }
                } else {
                    self.row(at, radius + 2, -1, double);
                    self.row(at, radius + 1, 0, double);
                }
            }
        }
    }

    fn foliage_height(&mut self, tree_height: i32) -> i32 {
        match self.tree.crown {
            Crown::Blob
            | Crown::Bush
            | Crown::Fancy
            | Crown::Pine
            | Crown::Jungle
            | Crown::MegaPine => self.tree.height.sample(self.run.rng),
            Crown::Spruce => (tree_height - self.tree.trunk_height.sample(self.run.rng)).max(4),
            Crown::Acacia => 0,
            Crown::DarkOak => 4,
        }
    }

    fn random_horizontal(&mut self) -> (i32, i32) {
        HORIZONTAL[self.run.rng.below(4) as usize]
    }

    fn trunk(&mut self, height: i32, origin: Pos) -> Vec<Attachment> {
        match self.tree.trunk {
            Trunk::Straight => {
                self.dirt((origin.0, origin.1 - 1, origin.2));
                for i in 0..height {
                    self.log((origin.0, origin.1 + i, origin.2));
                }
                vec![Attachment {
                    pos: (origin.0, origin.1 + height, origin.2),
                    radius_offset: 0,
                    double: false,
                }]
            }
            Trunk::Forking => self.forking(height, origin),
            Trunk::Giant => self.giant(height, origin),
            Trunk::MegaJungle => self.mega_jungle(height, origin),
            Trunk::DarkOak => self.dark_oak(height, origin),
            Trunk::Fancy => self.fancy(height, origin),
        }
    }

    fn forking(&mut self, height: i32, origin: Pos) -> Vec<Attachment> {
        self.dirt((origin.0, origin.1 - 1, origin.2));
        let mut result = Vec::new();
        let direction = self.random_horizontal();
        let i = height - self.run.rng.below(4) - 1;
        let mut j = 3 - self.run.rng.below(3);
        let (mut x, mut z) = (origin.0, origin.2);
        let mut top = None;
        for step in 0..height {
            let y = origin.1 + step;
            if step >= i && j > 0 {
                x += direction.0;
                z += direction.1;
                j -= 1;
            }
            if self.log((x, y, z)) {
                top = Some(y + 1);
            }
        }
        if let Some(top) = top {
            result.push(Attachment {
                pos: (x, top, z),
                radius_offset: 1,
                double: false,
            });
        }
        let (mut x, mut z) = (origin.0, origin.2);
        let second = self.random_horizontal();
        if second != direction {
            let start = i - self.run.rng.below(2) - 1;
            let mut remaining = 1 + self.run.rng.below(3);
            let mut top = None;
            let mut step = start;
            while step < height && remaining > 0 {
                if step >= 1 {
                    let y = origin.1 + step;
                    x += second.0;
                    z += second.1;
                    if self.log((x, y, z)) {
                        top = Some(y + 1);
                    }
                }
                step += 1;
                remaining -= 1;
            }
            if let Some(top) = top {
                result.push(Attachment {
                    pos: (x, top, z),
                    radius_offset: 0,
                    double: false,
                });
            }
        }
        result
    }

    fn giant(&mut self, height: i32, origin: Pos) -> Vec<Attachment> {
        self.dirt_square(origin);
        for i in 0..height {
            self.log_if_free((origin.0, origin.1 + i, origin.2));
            if i < height - 1 {
                self.log_if_free((origin.0 + 1, origin.1 + i, origin.2));
                self.log_if_free((origin.0 + 1, origin.1 + i, origin.2 + 1));
                self.log_if_free((origin.0, origin.1 + i, origin.2 + 1));
            }
        }
        vec![Attachment {
            pos: (origin.0, origin.1 + height, origin.2),
            radius_offset: 0,
            double: true,
        }]
    }

    fn mega_jungle(&mut self, height: i32, origin: Pos) -> Vec<Attachment> {
        let mut result = self.giant(height, origin);
        let mut i = height - 2 - self.run.rng.below(4);
        while i > height / 2 {
            let angle = self.run.rng.next_float() * (std::f32::consts::PI * 2.0);
            let (mut j, mut k) = (0, 0);
            for l in 0..5 {
                j = (1.5 + angle.cos() * l as f32) as i32;
                k = (1.5 + angle.sin() * l as f32) as i32;
                self.log((origin.0 + j, origin.1 + i - 3 + l / 2, origin.2 + k));
            }
            result.push(Attachment {
                pos: (origin.0 + j, origin.1 + i, origin.2 + k),
                radius_offset: -2,
                double: false,
            });
            i -= 2 + self.run.rng.below(4);
        }
        result
    }

    fn dark_oak(&mut self, height: i32, origin: Pos) -> Vec<Attachment> {
        let mut result = Vec::new();
        self.dirt_square(origin);
        let direction = self.random_horizontal();
        let bend_at = height - self.run.rng.below(4);
        let mut bend = 2 - self.run.rng.below(3);
        let (x0, y0, z0) = origin;
        let (mut x, mut z) = (x0, z0);
        let top = y0 + height - 1;
        for i in 0..height {
            if i >= bend_at && bend > 0 {
                x += direction.0;
                z += direction.1;
                bend -= 1;
            }
            let y = y0 + i;
            let air = {
                let state = self.run.region.get(x, y, z);
                replaceable_by_trees(state)
            };
            if air {
                for (dx, dz) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                    self.log((x + dx, y, z + dz));
                }
            }
        }
        result.push(Attachment {
            pos: (x, top, z),
            radius_offset: 0,
            double: true,
        });
        for dx in -1..=2 {
            for dz in -1..=2 {
                if (!(0..=1).contains(&dx) || !(0..=1).contains(&dz)) && self.run.rng.below(3) <= 0
                {
                    let length = self.run.rng.below(3) + 2;
                    for k in 0..length {
                        self.log((x0 + dx, top - k - 1, z0 + dz));
                    }
                    result.push(Attachment {
                        pos: (x + dx, top, z + dz),
                        radius_offset: 0,
                        double: false,
                    });
                }
            }
        }
        result
    }

    fn limb(&mut self, start: Pos, end: Pos, place: bool) -> bool {
        if !place && start == end {
            return true;
        }
        let delta = (end.0 - start.0, end.1 - start.1, end.2 - start.2);
        let steps = delta.0.abs().max(delta.1.abs()).max(delta.2.abs());
        let (fx, fy, fz) = (
            delta.0 as f32 / steps as f32,
            delta.1 as f32 / steps as f32,
            delta.2 as f32 / steps as f32,
        );
        for j in 0..=steps {
            let at = (
                start.0 + (0.5 + j as f32 * fx).floor() as i32,
                start.1 + (0.5 + j as f32 * fy).floor() as i32,
                start.2 + (0.5 + j as f32 * fz).floor() as i32,
            );
            if place {
                let (ax, az) = ((at.0 - start.0).abs(), (at.2 - start.2).abs());
                let longest = ax.max(az);
                let axis = if longest > 0 {
                    if ax == longest { "x" } else { "z" }
                } else {
                    "y"
                };
                self.put_log(at, Some(axis));
            } else if !self.valid(at) {
                return false;
            }
        }
        true
    }

    fn fancy(&mut self, height: i32, origin: Pos) -> Vec<Attachment> {
        let total = height + 2;
        let j = (f64::from(total) * 0.618).floor() as i32;
        self.dirt((origin.0, origin.1 - 1, origin.2));
        let per_level = 1.min((1.382 + (f64::from(total) / 13.0).powi(2)).floor() as i32);
        let trunk_top = origin.1 + j;
        let mut level = total - 5;
        let mut coords: Vec<(Pos, i32)> = vec![((origin.0, origin.1 + level, origin.2), trunk_top)];
        while level >= 0 {
            let shape = tree_shape(total, level);
            if shape >= 0.0 {
                for _ in 0..per_level {
                    let reach = f64::from(shape) * (f64::from(self.run.rng.next_float()) + 0.328);
                    let angle = f64::from(self.run.rng.next_float() * 2.0) * std::f64::consts::PI;
                    let dx = reach * angle.sin() + 0.5;
                    let dz = reach * angle.cos() + 0.5;
                    let start = (
                        origin.0 + dx.floor() as i32,
                        origin.1 + level - 1,
                        origin.2 + dz.floor() as i32,
                    );
                    let end = (start.0, start.1 + 5, start.2);
                    if self.limb(start, end, false) {
                        let (ox, oz) = (origin.0 - start.0, origin.2 - start.2);
                        let slope =
                            f64::from(start.1) - f64::from(ox * ox + oz * oz).sqrt() * 0.381;
                        let base = if slope > f64::from(trunk_top) {
                            trunk_top
                        } else {
                            slope as i32
                        };
                        let low = (origin.0, base, origin.2);
                        if self.limb(low, start, false) {
                            coords.push((start, low.1));
                        }
                    }
                }
            }
            level -= 1;
        }
        self.limb(origin, (origin.0, origin.1 + j, origin.2), true);
        for (attachment, base) in &coords {
            let low = (origin.0, *base, origin.2);
            if low != *attachment && (*base - origin.1) as f32 >= total as f32 * 0.2 {
                self.limb(low, *attachment, true);
            }
        }
        coords
            .into_iter()
            .filter(|(_, base)| (*base - origin.1) as f32 >= total as f32 * 0.2)
            .map(|(pos, _)| Attachment {
                pos,
                radius_offset: 0,
                double: false,
            })
            .collect()
    }

    fn free_height(&mut self, max: i32, origin: Pos) -> i32 {
        for i in 0..=max + 1 {
            let size = self.tree.size.at(max, i);
            for dx in -size..=size {
                for dz in -size..=size {
                    let at = (origin.0 + dx, origin.1 + i, origin.2 + dz);
                    let blocked = !self.valid(at)
                        || (!self.tree.ignore_vines
                            && self.run.region.get(at.0, at.1, at.2).kind()
                                == Some(rspigot_blocks::BlockKind::Vine));
                    if blocked {
                        return i - 2;
                    }
                }
            }
        }
        max
    }
}

fn tree_shape(height: i32, y: i32) -> f32 {
    if (y as f32) < height as f32 * 0.3 {
        return -1.0;
    }
    let half = height as f32 / 2.0;
    let delta = half - y as f32;
    let mut shape = (half * half - delta * delta).sqrt();
    if delta == 0.0 {
        shape = half;
    } else if delta.abs() >= half {
        return 0.0;
    }
    shape * 0.5
}

impl Tree {
    pub(crate) fn place(&self, run: &mut Run, origin: Pos) -> bool {
        let mut site = Site {
            tree: self,
            run,
            logs: Vec::new(),
            leaves: Vec::new(),
        };
        let height = self.base_height
            + site.run.rng.below(self.rand_a + 1)
            + site.run.rng.below(self.rand_b + 1);
        let foliage_height = site.foliage_height(height);
        let radius = self.radius.sample(site.run.rng);
        let region_height = site.run.region.height();
        if origin.1 < region_height.min_y + 1 || origin.1 + height + 1 > region_height.max_y() + 1 {
            return false;
        }
        let free = site.free_height(height, origin);
        if free < height && self.size.clipped().is_none_or(|clipped| free < clipped) {
            return false;
        }
        let attachments = site.trunk(free, origin);
        for attachment in attachments {
            site.foliage(attachment, foliage_height, radius);
        }
        let logs = std::mem::take(&mut site.logs);
        let leaves = std::mem::take(&mut site.leaves);
        if logs.is_empty() && leaves.is_empty() {
            return false;
        }
        if !self.decorators.is_empty() {
            let mut context = treedeco::Context::new(site.run, &logs, &leaves, &[]);
            treedeco::run(&self.decorators, &mut context);
        }
        update_leaves(site.run, &logs);
        true
    }
}

fn update_leaves(run: &mut Run, logs: &[Pos]) {
    const SIDES: [(i32, i32, i32); 6] = [
        (1, 0, 0),
        (-1, 0, 0),
        (0, 1, 0),
        (0, -1, 0),
        (0, 0, 1),
        (0, 0, -1),
    ];
    let log_set: HashSet<Pos> = logs.iter().copied().collect();
    let mut layers: Vec<HashSet<Pos>> = vec![HashSet::new(); 7];
    for pos in logs {
        for (dx, dy, dz) in SIDES {
            let next = (pos.0 + dx, pos.1 + dy, pos.2 + dz);
            if log_set.contains(&next) {
                continue;
            }
            let state = run.region.get(next.0, next.1, next.2);
            if state.property("distance").is_some() {
                layers[0].insert(next);
                if let Some(updated) = state.with_property("distance", "1") {
                    run.region.set(next.0, next.1, next.2, updated);
                }
            }
        }
    }
    for level in 1..7usize {
        let current: Vec<Pos> = layers[level - 1].iter().copied().collect();
        for pos in current {
            for (dx, dy, dz) in SIDES {
                let next = (pos.0 + dx, pos.1 + dy, pos.2 + dz);
                if layers[level - 1].contains(&next) || layers[level].contains(&next) {
                    continue;
                }
                let state = run.region.get(next.0, next.1, next.2);
                let Some(distance) = state
                    .property("distance")
                    .and_then(|d| d.parse::<usize>().ok())
                else {
                    continue;
                };
                if distance > level + 1 {
                    if let Some(updated) = state.with_property("distance", &(level + 1).to_string())
                    {
                        run.region.set(next.0, next.1, next.2, updated);
                    }
                    layers[level].insert(next);
                }
            }
        }
    }
}

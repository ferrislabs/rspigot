use serde_json::Value;

use super::{Attachment, HORIZONTAL, Site};
use crate::overworld::features::prov::{Pos, integer, number};

#[derive(Debug, Clone, Default)]
pub(super) struct Config {
    corner_hole: f32,
    wide_bottom_hole: f32,
    hanging: f32,
    hanging_extension: f32,
    side_hole: f32,
    attempts: i32,
}

impl Config {
    pub(super) fn compile(value: &Value) -> Self {
        let chance = |key: &str| number(value, key, 0.0) as f32;
        Self {
            corner_hole: chance("corner_hole_chance"),
            wide_bottom_hole: chance("wide_bottom_layer_hole_chance"),
            hanging: chance("hanging_leaves_chance"),
            hanging_extension: chance("hanging_leaves_extension_chance"),
            side_hole: chance("side_hole_chance"),
            attempts: integer(value, "leaf_placement_attempts", 0),
        }
    }
}

fn manhattan(a: Pos, b: Pos) -> i32 {
    (a.0 - b.0).abs() + (a.1 - b.1).abs() + (a.2 - b.2).abs()
}

fn rhombus_cut(x: i32, z: i32, radius: i32, partial: bool, flip: bool) -> i32 {
    let corner = if flip {
        (x > 0 && z > 0) || (z < 0 && x < 0)
    } else {
        (x > 0 && z < 0) || (z > 0 && x < 0)
    };
    if corner {
        radius - 1
    } else if partial {
        radius + 1
    } else {
        radius
    }
}

impl Site<'_, '_, '_> {
    pub(super) fn cherry_skip(&mut self, sx: i32, y: i32, sz: i32, range: i32) -> bool {
        let config = &self.tree.crown_config;
        let (wide_bottom, corner_hole) = (config.wide_bottom_hole, config.corner_hole);
        if y == -1 && (sx == range || sz == range) && self.run.rng.next_float() < wide_bottom {
            return true;
        }
        let corner = sx == range && sz == range;
        if range > 2 {
            corner || (sx + sz > range * 2 - 2 && self.run.rng.next_float() < corner_hole)
        } else {
            corner && self.run.rng.next_float() < corner_hole
        }
    }

    pub(super) fn cherry_crown(
        &mut self,
        attachment: Attachment,
        foliage_height: i32,
        radius: i32,
        offset: i32,
    ) {
        let double = attachment.double;
        let center = (
            attachment.pos.0,
            attachment.pos.1 + offset,
            attachment.pos.2,
        );
        let range = radius + attachment.radius_offset - 1;
        let (hanging, extension) = (
            self.tree.crown_config.hanging,
            self.tree.crown_config.hanging_extension,
        );
        self.row(center, range - 2, foliage_height - 3, double);
        self.row(center, range - 1, foliage_height - 4, double);
        for y in (0..=foliage_height - 5).rev() {
            self.row(center, range, y, double);
        }
        self.row_hanging(center, (range, -1, double), (hanging, extension));
        self.row_hanging(center, (range - 1, -2, double), (hanging, extension));
    }

    fn row_hanging(
        &mut self,
        center: Pos,
        (range, y, double): (i32, i32, bool),
        (chance, extension): (f32, f32),
    ) {
        self.row(center, range, y, double);
        let extra = i32::from(double);
        let anchor = (center.0, center.1 - 1, center.2);
        for direction in HORIZONTAL {
            let clockwise = (-direction.1, direction.0);
            let reach = if clockwise.0 + clockwise.1 > 0 {
                range + extra
            } else {
                range
            };
            let mut at = (
                center.0 + clockwise.0 * reach - direction.0 * range,
                center.1 + y - 1,
                center.2 + clockwise.1 * reach - direction.1 * range,
            );

            for _ in -range..range + extra {
                let supported = self.leaves.contains(&(at.0, at.1 + 1, at.2));
                if supported && self.extend(chance, anchor, at) {
                    self.extend(extension, anchor, (at.0, at.1 - 1, at.2));
                }
                at = (at.0 + direction.0, at.1, at.2 + direction.1);
            }
        }
    }

    fn extend(&mut self, chance: f32, origin: Pos, pos: Pos) -> bool {
        if manhattan(pos, origin) >= 7 {
            return false;
        }
        if self.run.rng.next_float() > chance {
            return false;
        }
        self.leaf(pos)
    }

    pub(super) fn random_spread(&mut self, pos: Pos, foliage_height: i32, radius: i32) {
        for _ in 0..self.tree.crown_config.attempts {
            let dx = self.run.rng.below(radius) - self.run.rng.below(radius);
            let dy = self.run.rng.below(foliage_height) - self.run.rng.below(foliage_height);
            let dz = self.run.rng.below(radius) - self.run.rng.below(radius);
            self.leaf((pos.0 + dx, pos.1 + dy, pos.2 + dz));
        }
    }

    pub(super) fn poplar_crown(
        &mut self,
        attachment: Attachment,
        foliage_height: i32,
        radius: i32,
        offset: i32,
    ) {
        let double = attachment.double;
        let center = (
            attachment.pos.0,
            attachment.pos.1 + offset,
            attachment.pos.2,
        );
        let range = radius + attachment.radius_offset - 1;
        let flip = self.run.rng.next_boolean();
        let h = foliage_height;
        let shape = (h, flip, double);
        self.poplar_row(center, (range - 2, h - 1), shape);
        self.poplar_row(center, (range - 1, h - 2), shape);
        self.poplar_row(center, (range - 1, h - 3), shape);
        for y in (1..=h - 4).rev() {
            self.poplar_row(center, (range, y), shape);
        }
        self.poplar_logs(center, (range, h - 4), shape);
        self.poplar_row(center, (range - 1, 0), shape);
        self.poplar_row(center, ((range - 2).clamp(1, 2), -1), shape);
    }

    fn poplar_row(
        &mut self,
        center: Pos,
        (range, y): (i32, i32),
        (h, flip, double): (i32, bool, bool),
    ) {
        let extra = i32::from(double);
        for x in -range..=range + extra {
            for z in -range..=range + extra {
                if !self.poplar_skip((x, y, z), range, (h, flip)) {
                    self.leaf((center.0 + x, center.1 + y, center.2 + z));
                }
            }
        }
    }

    fn poplar_skip(&mut self, (x, y, z): Pos, range: i32, (h, flip): (i32, bool)) -> bool {
        let partial = h - 1 == y || h - 2 == y;
        let cut = rhombus_cut(x, z, range, partial, flip);
        let (ax, az) = (x.abs(), z.abs());
        let corner = ax == range || az == range;
        if partial && corner {
            return true;
        }
        let side = i32::from(self.run.rng.next_float() <= self.tree.crown_config.side_hole);
        ax + az > range * 2 - (cut + side)
    }

    fn poplar_logs(
        &mut self,
        center: Pos,
        (range, y): (i32, i32),
        (h, flip, double): (i32, bool, bool),
    ) {
        let extra = i32::from(double);
        let partial = h - 1 == y || h - 2 == y;
        for x in -range..=range + extra {
            for z in -range..=range + extra {
                let (ax, az) = (x.abs(), z.abs());
                let cut = rhombus_cut(x, z, range, partial, flip);
                if ax + az > range * 2 - (cut + 2) {
                    continue;
                }
                let along_x = az == 0 && range - ax >= 4;
                let along_z = ax == 0 && range - az >= 4;
                if !(along_x || along_z) {
                    continue;
                }
                let at = (center.0 + x, center.1 + y, center.2 + z);
                self.replace_leaf_with_log(at, if az == 0 { "x" } else { "z" });
            }
        }
    }

    fn replace_leaf_with_log(&mut self, pos: Pos, axis: &str) {
        let present = self.run.region.get(pos.0, pos.1, pos.2);
        let leaf = self.tree.leaves.resolve(self.run.region, self.run.rng, pos);
        if leaf != Some(present) {
            return;
        }
        let Some(log) = self.tree.log.resolve(self.run.region, self.run.rng, pos) else {
            return;
        };
        let log = log.with_property("axis", axis).unwrap_or(log);
        self.run.region.set(pos.0, pos.1, pos.2, log);
        self.leaves.push(pos);
    }
}

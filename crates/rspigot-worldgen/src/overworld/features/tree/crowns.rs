use serde_json::Value;

use super::{Attachment, HORIZONTAL, Site};
use crate::overworld::features::prov::{Pos, number};

#[derive(Debug, Clone, Default)]
pub(super) struct Config {
    corner_hole: f32,
    wide_bottom_hole: f32,
    hanging: f32,
    hanging_extension: f32,
}

impl Config {
    pub(super) fn compile(value: &Value) -> Self {
        let chance = |key: &str| number(value, key, 0.0) as f32;
        Self {
            corner_hole: chance("corner_hole_chance"),
            wide_bottom_hole: chance("wide_bottom_layer_hole_chance"),
            hanging: chance("hanging_leaves_chance"),
            hanging_extension: chance("hanging_leaves_extension_chance"),
        }
    }
}

fn manhattan(a: Pos, b: Pos) -> i32 {
    (a.0 - b.0).abs() + (a.1 - b.1).abs() + (a.2 - b.2).abs()
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
}

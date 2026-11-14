use crate::Vec3;
use crate::entity::Aabb;
use crate::mth;

use super::history::FlightHistory;

pub const PART_COUNT: usize = 8;
const DEG_TO_RAD: f32 = 0.017_453_292;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Part {
    Head,
    Neck,
    Body,
    Tail1,
    Tail2,
    Tail3,
    Wing1,
    Wing2,
}

impl Part {
    pub const ALL: [Self; PART_COUNT] = [
        Self::Head,
        Self::Neck,
        Self::Body,
        Self::Tail1,
        Self::Tail2,
        Self::Tail3,
        Self::Wing1,
        Self::Wing2,
    ];

    pub const fn index(self) -> usize {
        match self {
            Self::Head => 0,
            Self::Neck => 1,
            Self::Body => 2,
            Self::Tail1 => 3,
            Self::Tail2 => 4,
            Self::Tail3 => 5,
            Self::Wing1 => 6,
            Self::Wing2 => 7,
        }
    }

    pub fn from_index(index: usize) -> Option<Self> {
        Self::ALL.get(index).copied()
    }

    pub const fn name(self) -> &'static str {
        match self {
            Self::Head => "head",
            Self::Neck => "neck",
            Self::Body => "body",
            Self::Tail1 | Self::Tail2 | Self::Tail3 => "tail",
            Self::Wing1 | Self::Wing2 => "wing",
        }
    }

    pub const fn width(self) -> f32 {
        match self {
            Self::Head => 1.0,
            Self::Neck => 3.0,
            Self::Body => 5.0,
            Self::Tail1 | Self::Tail2 | Self::Tail3 => 2.0,
            Self::Wing1 | Self::Wing2 => 4.0,
        }
    }

    pub const fn height(self) -> f32 {
        match self {
            Self::Head => 1.0,
            Self::Neck => 3.0,
            Self::Body => 3.0,
            Self::Tail1 | Self::Tail2 | Self::Tail3 => 2.0,
            Self::Wing1 | Self::Wing2 => 2.0,
        }
    }

    pub const fn takes_full_damage(self) -> bool {
        matches!(self, Self::Head | Self::Neck)
    }
}

pub fn part_box(part: Part, position: Vec3) -> Aabb {
    Aabb::of_size(position, part.width(), part.height())
}

pub fn damage_through(part: Part, damage: f32) -> f32 {
    if part.takes_full_damage() {
        damage
    } else {
        damage / 4.0 + damage.min(1.0)
    }
}

pub fn head_y_offset(sitting: bool, history: &FlightHistory) -> f32 {
    if sitting {
        return -1.0;
    }
    (history.get(5).y - history.get(0).y) as f32
}

pub fn rot_wrap(value: f64) -> f32 {
    mth::wrap_degrees(value as f32)
}

pub fn offsets(
    history: &FlightHistory,
    yaw: f32,
    y_rot_a: f32,
    sitting: bool,
) -> [Vec3; PART_COUNT] {
    let tilt = ((history.get(5).y - history.get(10).y) as f32) * 10.0 * DEG_TO_RAD;
    let cos_tilt = mth::cos(f64::from(tilt));
    let sin_tilt = mth::sin(f64::from(tilt));
    let yaw_rad = yaw * DEG_TO_RAD;
    let yaw_sin = mth::sin(f64::from(yaw_rad));
    let yaw_cos = mth::cos(f64::from(yaw_rad));
    let mut out = [Vec3::ZERO; PART_COUNT];
    out[Part::Body.index()] = at(yaw_sin * 0.5, 0.0, -yaw_cos * 0.5);
    out[Part::Wing1.index()] = at(yaw_cos * 4.5, 2.0, yaw_sin * 4.5);
    out[Part::Wing2.index()] = at(yaw_cos * -4.5, 2.0, yaw_sin * -4.5);
    let swing = yaw_rad - y_rot_a * 0.01;
    let head_sin = mth::sin(f64::from(swing));
    let head_cos = mth::cos(f64::from(swing));
    let head_y = head_y_offset(sitting, history);
    out[Part::Head.index()] = at(
        head_sin * 6.5 * cos_tilt,
        head_y + sin_tilt * 6.5,
        -head_cos * 6.5 * cos_tilt,
    );
    out[Part::Neck.index()] = at(
        head_sin * 5.5 * cos_tilt,
        head_y + sin_tilt * 5.5,
        -head_cos * 5.5 * cos_tilt,
    );
    let reference = history.get(5);
    for (slot, part) in [Part::Tail1, Part::Tail2, Part::Tail3]
        .into_iter()
        .enumerate()
    {
        let sample = history.get(slot as i32 * 2 + 12);
        let turn = rot_wrap(f64::from(sample.y_rot - reference.y_rot));
        let angle = yaw * DEG_TO_RAD + turn * DEG_TO_RAD;
        let sin = mth::sin(f64::from(angle));
        let cos = mth::cos(f64::from(angle));
        let span = (slot as f32 + 1.0) * 2.0;
        out[part.index()] = Vec3::new(
            f64::from(-(yaw_sin * 1.5 + sin * span) * cos_tilt),
            (sample.y - reference.y) - f64::from((span + 1.5) * sin_tilt) + 1.5,
            f64::from((yaw_cos * 1.5 + cos * span) * cos_tilt),
        );
    }
    out
}

fn at(x: f32, y: f32, z: f32) -> Vec3 {
    Vec3::new(f64::from(x), f64::from(y), f64::from(z))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn flat() -> FlightHistory {
        let mut history = FlightHistory::default();
        history.record(70.0, 0.0);
        history
    }

    #[test]
    fn there_are_eight_parts_with_the_vanilla_sizes() {
        let sizes: Vec<(&str, f32, f32)> = Part::ALL
            .iter()
            .map(|part| (part.name(), part.width(), part.height()))
            .collect();
        assert_eq!(
            sizes,
            vec![
                ("head", 1.0, 1.0),
                ("neck", 3.0, 3.0),
                ("body", 5.0, 3.0),
                ("tail", 2.0, 2.0),
                ("tail", 2.0, 2.0),
                ("tail", 2.0, 2.0),
                ("wing", 4.0, 2.0),
                ("wing", 4.0, 2.0),
            ]
        );
    }

    #[test]
    fn only_the_head_and_neck_take_full_damage() {
        assert_eq!(damage_through(Part::Head, 10.0), 10.0);
        assert_eq!(damage_through(Part::Neck, 10.0), 10.0);
        assert_eq!(damage_through(Part::Body, 10.0), 3.5);
        assert_eq!(damage_through(Part::Wing1, 0.5), 0.625);
        assert_eq!(damage_through(Part::Tail2, 4.0), 2.0);
    }

    #[test]
    fn the_part_box_is_centered_on_the_part_position_from_the_ground_up() {
        let area = part_box(Part::Body, Vec3::new(10.0, 70.0, 20.0));
        assert_eq!(area, Aabb::new(7.5, 70.0, 17.5, 12.5, 73.0, 22.5));
    }

    #[test]
    fn a_dragon_facing_south_puts_its_head_in_front_and_wings_to_the_side() {
        let offsets = offsets(&flat(), 0.0, 0.0, false);
        let head = offsets[Part::Head.index()];
        assert!((head.z - -6.5).abs() < 1e-5);
        assert!(head.x.abs() < 1e-5);
        let wing = offsets[Part::Wing1.index()];
        assert!((wing.x - 4.5).abs() < 1e-5);
        assert_eq!(wing.y, 2.0);
        let other = offsets[Part::Wing2.index()];
        assert!((other.x + 4.5).abs() < 1e-5);
        let body = offsets[Part::Body.index()];
        assert!((body.z + 0.5).abs() < 1e-5);
    }

    #[test]
    fn the_tail_trails_behind_the_dragon() {
        let offsets = offsets(&flat(), 0.0, 0.0, false);
        let tail = [Part::Tail1, Part::Tail2, Part::Tail3].map(|part| offsets[part.index()]);
        assert!(tail[0].z > 1.5 && tail[1].z > tail[0].z && tail[2].z > tail[1].z);
        assert!((tail[0].y - (1.5 - 3.5 * 0.0)).abs() < 1e-5);
    }

    #[test]
    fn a_sitting_dragon_lowers_its_head_by_one_block() {
        assert_eq!(head_y_offset(true, &flat()), -1.0);
        let offsets = offsets(&flat(), 0.0, 0.0, true);
        assert!((offsets[Part::Head.index()].y + 1.0).abs() < 1e-5);
    }

    #[test]
    fn the_head_lags_behind_the_vertical_motion_of_the_body() {
        let mut climbing = FlightHistory::default();
        let mut diving = FlightHistory::default();
        for tick in 0..12 {
            climbing.record(70.0 + f64::from(tick) * 0.5, 0.0);
            diving.record(70.0 - f64::from(tick) * 0.5, 0.0);
        }
        assert_eq!(head_y_offset(false, &climbing), -2.5);
        assert_eq!(head_y_offset(false, &diving), 2.5);
    }

    #[test]
    fn the_head_swings_with_the_turn_rate() {
        let straight = offsets(&flat(), 0.0, 0.0, false)[Part::Head.index()];
        let turning = offsets(&flat(), 0.0, 100.0, false)[Part::Head.index()];
        assert!(turning.x.abs() > straight.x.abs());
    }
}

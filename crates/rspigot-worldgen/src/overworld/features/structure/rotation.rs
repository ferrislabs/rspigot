use rspigot_domain::BlockStateId;

use super::super::prov::Pos;
use crate::climate::Xoroshiro;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Rotation {
    None,
    Clockwise90,
    Clockwise180,
    Counterclockwise90,
}

const HORIZONTAL: [&str; 4] = ["north", "east", "south", "west"];

impl Rotation {
    pub(crate) const ALL: [Self; 4] = [
        Self::None,
        Self::Clockwise90,
        Self::Clockwise180,
        Self::Counterclockwise90,
    ];

    pub(crate) fn parse(name: &str) -> Option<Self> {
        match name.strip_prefix("minecraft:").unwrap_or(name) {
            "none" => Some(Self::None),
            "clockwise_90" => Some(Self::Clockwise90),
            "180" | "clockwise_180" => Some(Self::Clockwise180),
            "counterclockwise_90" => Some(Self::Counterclockwise90),
            _ => None,
        }
    }

    pub(crate) fn random(rng: &mut Xoroshiro) -> Self {
        Self::ALL[rng.below(4) as usize]
    }

    pub(crate) fn swaps_axes(self) -> bool {
        matches!(self, Self::Clockwise90 | Self::Counterclockwise90)
    }

    pub(crate) fn vector(self, (x, z): (i32, i32)) -> (i32, i32) {
        match self {
            Self::None => (x, z),
            Self::Clockwise90 => (-z, x),
            Self::Clockwise180 => (-x, -z),
            Self::Counterclockwise90 => (z, -x),
        }
    }

    pub(crate) fn transform(self, (x, y, z): Pos) -> Pos {
        let (rx, rz) = self.vector((x, z));
        (rx, y, rz)
    }

    fn quarter_turns(self) -> usize {
        match self {
            Self::None => 0,
            Self::Clockwise90 => 1,
            Self::Clockwise180 => 2,
            Self::Counterclockwise90 => 3,
        }
    }

    fn direction(self, name: &str) -> &'static str {
        HORIZONTAL
            .iter()
            .position(|candidate| *candidate == name)
            .map_or("north", |index| {
                HORIZONTAL[(index + self.quarter_turns()) % 4]
            })
    }

    pub(crate) fn state(self, state: BlockStateId) -> BlockStateId {
        if self == Self::None {
            return state;
        }
        let mut result = state;
        if self.swaps_axes() {
            let swapped = match state.property("axis") {
                Some("x") => Some("z"),
                Some("z") => Some("x"),
                _ => None,
            };
            if let Some(axis) = swapped {
                result = result.with_property("axis", axis).unwrap_or(result);
            }
        }
        if let Some(facing) = state.property("facing")
            && HORIZONTAL.contains(&facing)
        {
            result = result
                .with_property("facing", self.direction(facing))
                .unwrap_or(result);
        }
        let sides: Vec<(&str, Option<&str>)> = HORIZONTAL
            .iter()
            .map(|side| (*side, state.property(side)))
            .collect();
        if sides.iter().all(|(_, value)| value.is_some()) {
            for (side, value) in sides {
                let (Some(value), target) = (value, self.direction(side)) else {
                    continue;
                };
                result = result.with_property(target, value).unwrap_or(result);
            }
        }
        result
    }
}

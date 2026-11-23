use crate::generated::timelines::{DIMENSIONS, TIMELINES};
use crate::loot::AttributeValue;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Easing {
    Linear,
    Constant,
    CubicBezier([f32; 4]),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Modifier {
    Or,
    And,
    Maximum,
    Minimum,
    Multiply,
    Add,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Keyframe {
    pub ticks: i64,
    pub value: AttributeValue,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Track {
    pub attribute: &'static str,
    pub ease: Easing,
    pub modifier: Option<Modifier>,
    pub keyframes: &'static [Keyframe],
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TimelineData {
    pub id: &'static str,
    pub clock: &'static str,
    pub period_ticks: Option<i64>,
    pub tracks: &'static [Track],
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DimensionData {
    pub id: &'static str,
    pub timelines: &'static [&'static str],
    pub attributes: &'static [(&'static str, AttributeValue)],
}

pub fn timelines() -> &'static [TimelineData] {
    &TIMELINES
}

pub fn timeline(id: &str) -> Option<&'static TimelineData> {
    TIMELINES.iter().find(|timeline| timeline.id == id)
}

pub fn dimension(id: &str) -> Option<&'static DimensionData> {
    DIMENSIONS.iter().find(|dimension| dimension.id == id)
}

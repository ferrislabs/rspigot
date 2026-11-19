use rspigot_plugin::{Particle, Sound};

use crate::domain::SheepKind;

pub const CAPTURE_SOUND: Sound = Sound::EntityItemPickup;
pub const CAPTURE_PARTICLE: Particle = Particle::Cloud;
pub const CAPTURE_PARTICLES: i32 = 8;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Capture {
    Denied,
    Fragment,
    Taken { kind: SheepKind },
}

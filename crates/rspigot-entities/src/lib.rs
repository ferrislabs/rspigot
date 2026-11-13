#[rustfmt::skip]
pub mod attachments;
#[rustfmt::skip]
mod categories;
mod category;
pub mod family;
#[rustfmt::skip]
mod kind;
mod lookup;
mod mob_spawn;
#[rustfmt::skip]
mod spawns;
#[rustfmt::skip]
mod types;

pub use categories::MobCategory;
pub use family::EntityBehavior;
pub use kind::EntityKind;
pub use mob_spawn::{MobSpawn, SpawnHeightmap, SpawnPlacement, SpawnPlacementType};
pub use spawns::{MobClass, SpawnPredicate};
pub use types::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
pub struct EntityTypeId(pub u16);

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EntityDimensions {
    pub width: f32,
    pub height: f32,
    pub eye_height: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SoundSource {
    Master,
    Music,
    Records,
    Weather,
    Blocks,
    Hostile,
    Neutral,
    Players,
    Ambient,
    Voice,
    Ui,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VoicePitch {
    Standard,
    Fixed,
    Bat,
    ZombieVillager,
    Parrot,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SoundVolume {
    Fixed(f32),
    PerSize(f32),
    Baby { adult: f32, baby: f32 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CreatureSound {
    Fixed(Option<&'static str>),
    Varied(&'static [Option<&'static str>; 12]),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum WaterContact {
    #[default]
    Dry,
    Touching,
    Submerged,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SoundState {
    pub baby: bool,
    pub tiny: bool,
    pub water: WaterContact,
}

impl CreatureSound {
    pub const fn get(self, state: SoundState) -> Option<&'static str> {
        match self {
            Self::Fixed(sound) => sound,
            Self::Varied(table) => {
                let water = match state.water {
                    WaterContact::Dry => 0,
                    WaterContact::Touching => 1,
                    WaterContact::Submerged => 2,
                };
                table[(state.baby as usize) * 6 + (state.tiny as usize) * 3 + water]
            }
        }
    }
}

impl SoundVolume {
    pub fn get(self, baby: bool, size: i32) -> f32 {
        match self {
            Self::Fixed(volume) => volume,
            Self::PerSize(per_size) => per_size * size as f32,
            Self::Baby { adult, baby: young } => {
                if baby {
                    young
                } else {
                    adult
                }
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CreatureSounds {
    pub hurt: CreatureSound,
    pub death: CreatureSound,
    pub small_fall: &'static str,
    pub big_fall: &'static str,
    pub volume: SoundVolume,
    pub pitch: VoicePitch,
    pub source: SoundSource,
    pub can_be_baby: bool,
}

pub trait EntityType: 'static {
    const NAME: &'static str;
    const ID: EntityTypeId;
    const CLASS: &'static str;
    const CATEGORY: MobCategory;
    const WIDTH: f32;
    const HEIGHT: f32;
    const EYE_HEIGHT: f32;
    const FIRE_IMMUNE: bool;
    const SUMMONABLE: bool;
    const CAN_SERIALIZE: bool;
    const ALLOWED_IN_PEACEFUL: bool;
    const CAN_SPAWN_FAR_FROM_PLAYER: bool;
    const CLIENT_TRACKING_RANGE: u8;
    const UPDATE_INTERVAL: Option<u8>;
    const TRACK_DELTAS: bool;
    const SPAWN_EGG: Option<u16>;
    const LIVING: bool;
    const ANCESTORS: &'static [&'static str];
    const DEFAULT_ATTRIBUTES: &'static [(&'static str, f64)];
    const SOUNDS: Option<CreatureSounds>;
    const MOB_SPAWN: Option<MobSpawn>;
    type Behavior: EntityBehavior;
}

#[cfg(test)]
mod attachment_tests {
    use super::{EntityKind, attachments};

    #[test]
    fn passenger_and_vehicle_points_follow_the_vanilla_builders() {
        assert_eq!(
            attachments::passenger_points(EntityKind::Chicken),
            Some(&[[0.0, 0.7, -0.1]][..])
        );
        assert_eq!(
            attachments::vehicle_points(EntityKind::Zombie),
            Some(&[[0.0, 0.699999988079071, 0.0]][..])
        );
        assert_eq!(attachments::passenger_points(EntityKind::Strider), None);
        assert_eq!(
            attachments::passenger_points(EntityKind::HappyGhast).map(<[_]>::len),
            Some(4)
        );
    }
}

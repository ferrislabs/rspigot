use rspigot_nbt::Compound;

use crate::entity::base::EntityBase;
use crate::entity::living::LivingState;
use crate::entity::spawn::{SpawnEnvironment, mob_finalize};
use crate::random::RandomSource;
use crate::spawn::SpawnGroupData;

pub const MAX_LIFE: i32 = 2400;
pub const LIFETIME_KEY: &str = "Lifetime";
pub const KEYS: &[&str] = &[LIFETIME_KEY];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct EndermiteState {
    pub life: i32,
}

impl EndermiteState {
    pub fn tick(&mut self, persistence_required: bool) -> bool {
        if !persistence_required {
            self.life += 1;
        }
        self.life >= MAX_LIFE
    }
}

pub fn define(_base: &mut EntityBase) -> EndermiteState {
    EndermiteState::default()
}

pub fn read(nbt: &Compound, _base: &mut EntityBase, state: &mut EndermiteState) {
    state.life = nbt.get_i32(LIFETIME_KEY).unwrap_or(0);
}

pub fn write(_base: &EntityBase, state: &EndermiteState, out: &mut Compound) {
    out.insert(LIFETIME_KEY, state.life);
}

pub fn finalize(
    base: &mut EntityBase,
    living: &mut LivingState,
    _environment: &SpawnEnvironment,
    group: Option<SpawnGroupData>,
    level: &mut impl RandomSource,
) -> Option<SpawnGroupData> {
    mob_finalize(base, living, group, level)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn life_grows_until_the_despawn_limit_unless_persistent() {
        let mut state = EndermiteState::default();
        for _ in 0..MAX_LIFE - 1 {
            assert!(!state.tick(false));
        }
        assert!(state.tick(false));
        let mut kept = EndermiteState::default();
        for _ in 0..MAX_LIFE + 5 {
            assert!(!kept.tick(true));
        }
        assert_eq!(kept.life, 0);
    }

    #[test]
    fn lifetime_round_trips_through_nbt() {
        let mut base = EntityBase::new(
            crate::EntityId(1),
            uuid::Uuid::from_u128(1),
            rspigot_entities::EntityKind::Endermite,
        );
        let mut out = Compound::new();
        write(&base, &EndermiteState { life: 1234 }, &mut out);
        assert_eq!(out.get_i32(LIFETIME_KEY), Some(1234));
        let mut loaded = EndermiteState::default();
        read(&out, &mut base, &mut loaded);
        assert_eq!(loaded.life, 1234);
        read(&Compound::new(), &mut base, &mut loaded);
        assert_eq!(loaded.life, 0);
    }
}

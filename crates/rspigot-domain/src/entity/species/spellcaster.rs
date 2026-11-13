use rspigot_nbt::Compound;

use crate::entity::base::EntityBase;
use crate::entity::data::SyncedValue;

pub const DATA_SPELL_CASTING: u8 = 17;
pub const SPELL_TICKS_KEY: &str = "SpellTicks";
pub const KEYS: &[&str] = &[
    "Wave",
    "CanJoinRaid",
    "patrol_target",
    "PatrolLeader",
    "Patrolling",
    "RaidId",
    SPELL_TICKS_KEY,
];
pub const DEFAULT_WARMUP_TICKS: i32 = 20;
pub const SPELL_PARTICLE_ARM_DISTANCE: f64 = 0.6;
pub const SPELL_PARTICLE_HEIGHT: f64 = 1.8;
pub const SPELL_PARTICLE_SWAY_RATE: f32 = 0.6662;
pub const SPELL_PARTICLE_SWAY: f32 = 0.25;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum IllagerSpell {
    #[default]
    None,
    SummonVex,
    Fangs,
    Wololo,
    Disappear,
    Blindness,
}

impl IllagerSpell {
    pub const ALL: [Self; 6] = [
        Self::None,
        Self::SummonVex,
        Self::Fangs,
        Self::Wololo,
        Self::Disappear,
        Self::Blindness,
    ];

    pub const fn id(self) -> u8 {
        match self {
            Self::None => 0,
            Self::SummonVex => 1,
            Self::Fangs => 2,
            Self::Wololo => 3,
            Self::Disappear => 4,
            Self::Blindness => 5,
        }
    }

    pub fn by_id(id: i32) -> Self {
        usize::try_from(id)
            .ok()
            .and_then(|index| Self::ALL.get(index).copied())
            .unwrap_or(Self::None)
    }

    pub const fn color(self) -> [f64; 3] {
        match self {
            Self::None => [0.0, 0.0, 0.0],
            Self::SummonVex => [0.7, 0.7, 0.8],
            Self::Fangs => [0.4, 0.3, 0.35],
            Self::Wololo => [0.7, 0.5, 0.2],
            Self::Disappear => [0.3, 0.3, 0.8],
            Self::Blindness => [0.1, 0.1, 0.2],
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SpellcasterState {
    pub casting_ticks: i32,
    pub current: IllagerSpell,
}

impl SpellcasterState {
    pub fn is_casting(&self) -> bool {
        self.casting_ticks > 0
    }

    pub fn tick(&mut self) {
        if self.casting_ticks > 0 {
            self.casting_ticks -= 1;
        }
    }
}

pub fn define(base: &mut EntityBase) {
    base.data.define(DATA_SPELL_CASTING, SyncedValue::Byte(0));
}

pub fn synced_spell(base: &EntityBase) -> IllagerSpell {
    IllagerSpell::by_id(i32::from(base.data.byte(DATA_SPELL_CASTING)))
}

pub fn is_casting_synced(base: &EntityBase) -> bool {
    base.data.byte(DATA_SPELL_CASTING) > 0
}

pub fn set_casting(base: &mut EntityBase, state: &mut SpellcasterState, spell: IllagerSpell) {
    state.current = spell;
    base.data
        .set(DATA_SPELL_CASTING, SyncedValue::Byte(spell.id() as i8));
}

pub fn read(nbt: &Compound, state: &mut SpellcasterState) {
    state.casting_ticks = nbt.get_i32(SPELL_TICKS_KEY).unwrap_or(0);
}

pub fn write(state: &SpellcasterState, out: &mut Compound) {
    out.insert(SPELL_TICKS_KEY, state.casting_ticks);
}

pub fn particle_offsets(body_yaw: f32, tick_count: i32, scale: f32) -> [(f64, f64, f64); 2] {
    let sway = crate::mth::cos(f64::from(tick_count as f32 * SPELL_PARTICLE_SWAY_RATE));
    let angle = body_yaw * 0.017_453_292 + sway * SPELL_PARTICLE_SWAY;
    let cos = crate::mth::cos(f64::from(angle));
    let sin = crate::mth::sin(f64::from(angle));
    let reach = f64::from(scale) * SPELL_PARTICLE_ARM_DISTANCE;
    let height = f64::from(scale) * SPELL_PARTICLE_HEIGHT;
    [
        (f64::from(cos) * reach, height, f64::from(sin) * reach),
        (-f64::from(cos) * reach, height, -f64::from(sin) * reach),
    ]
}

#[cfg(test)]
mod tests {
    use rspigot_entities::EntityKind;
    use uuid::Uuid;

    use super::*;
    use crate::EntityId;

    fn base() -> EntityBase {
        let mut base = EntityBase::new(EntityId(1), Uuid::from_u128(1), EntityKind::Evoker);
        define(&mut base);
        base
    }

    #[test]
    fn the_spells_carry_their_vanilla_ids_and_colors() {
        let table = [
            (IllagerSpell::None, 0, [0.0, 0.0, 0.0]),
            (IllagerSpell::SummonVex, 1, [0.7, 0.7, 0.8]),
            (IllagerSpell::Fangs, 2, [0.4, 0.3, 0.35]),
            (IllagerSpell::Wololo, 3, [0.7, 0.5, 0.2]),
            (IllagerSpell::Disappear, 4, [0.3, 0.3, 0.8]),
            (IllagerSpell::Blindness, 5, [0.1, 0.1, 0.2]),
        ];
        for (spell, id, color) in table {
            assert_eq!(spell.id(), id);
            assert_eq!(spell.color(), color);
            assert_eq!(IllagerSpell::by_id(i32::from(id)), spell);
        }
    }

    #[test]
    fn an_unknown_spell_id_reads_as_none() {
        assert_eq!(IllagerSpell::by_id(6), IllagerSpell::None);
        assert_eq!(IllagerSpell::by_id(-1), IllagerSpell::None);
        assert_eq!(IllagerSpell::by_id(127), IllagerSpell::None);
    }

    #[test]
    fn the_synched_spell_byte_sits_at_index_seventeen_and_follows_the_current_spell() {
        let mut base = base();
        let mut state = SpellcasterState::default();
        assert_eq!(
            base.data.get(DATA_SPELL_CASTING),
            Some(&SyncedValue::Byte(0))
        );
        assert!(!is_casting_synced(&base));
        set_casting(&mut base, &mut state, IllagerSpell::Wololo);
        assert_eq!(state.current, IllagerSpell::Wololo);
        assert_eq!(
            base.data.get(DATA_SPELL_CASTING),
            Some(&SyncedValue::Byte(3))
        );
        assert!(is_casting_synced(&base));
        assert_eq!(synced_spell(&base), IllagerSpell::Wololo);
        set_casting(&mut base, &mut state, IllagerSpell::None);
        assert!(!is_casting_synced(&base));
    }

    #[test]
    fn the_casting_countdown_stops_at_zero() {
        let mut state = SpellcasterState {
            casting_ticks: 2,
            current: IllagerSpell::Fangs,
        };
        assert!(state.is_casting());
        state.tick();
        assert_eq!(state.casting_ticks, 1);
        state.tick();
        assert!(!state.is_casting());
        state.tick();
        assert_eq!(state.casting_ticks, 0);
    }

    #[test]
    fn the_spell_ticks_round_trip_and_are_written_even_at_zero() {
        let mut state = SpellcasterState::default();
        let mut out = Compound::new();
        write(&state, &mut out);
        assert_eq!(out.get_i32(SPELL_TICKS_KEY), Some(0));
        let nbt = Compound::new().with(SPELL_TICKS_KEY, 37);
        read(&nbt, &mut state);
        assert_eq!(state.casting_ticks, 37);
        let mut out = Compound::new();
        write(&state, &mut out);
        assert_eq!(out.get_i32(SPELL_TICKS_KEY), Some(37));
    }

    #[test]
    fn the_cast_particles_orbit_the_arms_at_sixty_percent_of_the_scale() {
        let [left, right] = particle_offsets(0.0, 0, 1.0);
        assert!((left.1 - 1.8).abs() < 1e-9);
        assert!((left.0.hypot(left.2) - 0.6).abs() < 1e-6);
        assert!((left.0 + right.0).abs() < 1e-12);
        assert!((left.2 + right.2).abs() < 1e-12);
        let scaled = particle_offsets(0.0, 0, 2.0);
        assert!((scaled[0].1 - 3.6).abs() < 1e-9);
    }
}

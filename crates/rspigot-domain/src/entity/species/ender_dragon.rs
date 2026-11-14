use rspigot_nbt::Compound;

use crate::BlockPos;
use crate::dragon::phase::DragonPhase;
use crate::entity::base::EntityBase;
use crate::entity::data::SyncedValue;

pub const DATA_PHASE: u8 = 16;
pub const KEYS: &[&str] = &[
    "DragonPhase",
    "DragonDeathTime",
    "sitting_damage_received",
    "Bukkit.expToDrop",
];
pub const GROWL_START: i32 = 100;
pub const GROWL_BASE: i32 = 200;
pub const GROWL_RANDOM: i32 = 200;
pub const DEATH_BLAST_START: i32 = 180;
pub const DEATH_BLAST_END: i32 = 200;
pub const DEATH_XP_START: i32 = 150;
pub const DEATH_XP_INTERVAL: i32 = 5;
pub const DEATH_END: i32 = 200;
pub const DEATH_SOUND_EVENT: i32 = 1028;
pub const FIRST_KILL_XP: i32 = 12_000;
pub const REPEAT_KILL_XP: i32 = 500;
pub const DEATH_XP_FRACTION: f32 = 0.08;
pub const DEATH_END_XP_FRACTION: f32 = 0.2;
pub const CRYSTAL_HEAL_INTERVAL: i32 = 10;
pub const CRYSTAL_HEAL_AMOUNT: f32 = 1.0;
pub const CRYSTAL_SCAN_ODDS: i32 = 10;
pub const CRYSTAL_SCAN_RADIUS: f64 = 32.0;
pub const CRYSTAL_DESTROY_RANGE: f64 = 64.0;
pub const CRYSTAL_DESTROY_DAMAGE: f32 = 10.0;
pub const SITTING_DAMAGE_RATIO: f32 = 0.25;
pub const HEAD_DAMAGE: f32 = 10.0;
pub const WING_DAMAGE: f32 = 5.0;
pub const WING_KNOCKBACK: f64 = 4.0;
pub const MIN_HURT_DAMAGE: f32 = 0.01;
pub const PUSH_UP: f64 = 0.2;
pub const PUSH_MIN_DISTANCE: f64 = 0.1;
pub const HURT_RECENTLY_TICKS: i32 = 2;
pub const XP_RANGE: &str = "dragon";
pub const MAX_HEALTH: f64 = 200.0;
pub const HOVER_ENTRY_PHASE: DragonPhase = DragonPhase::Hovering;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DragonState {
    pub phase: DragonPhase,
    pub death_time: i32,
    pub growl_time: i32,
    pub flap_time: f32,
    pub old_flap_time: f32,
    pub y_rot_a: f32,
    pub in_wall: bool,
    pub sitting_damage_received: f32,
    pub exp_to_drop: i32,
    pub fight_origin: BlockPos,
}

impl Default for DragonState {
    fn default() -> Self {
        Self {
            phase: DragonPhase::Hovering,
            death_time: 0,
            growl_time: GROWL_START,
            flap_time: 0.0,
            old_flap_time: 0.0,
            y_rot_a: 0.0,
            in_wall: false,
            sitting_damage_received: 0.0,
            exp_to_drop: 0,
            fight_origin: BlockPos::new(0, 0, 0),
        }
    }
}

pub fn define(base: &mut EntityBase) -> DragonState {
    base.data
        .define(DATA_PHASE, SyncedValue::Int(DragonPhase::Hovering.id()));
    DragonState::default()
}

pub fn set_phase(base: &mut EntityBase, state: &mut DragonState, phase: DragonPhase) {
    state.phase = phase;
    base.data.set(DATA_PHASE, SyncedValue::Int(phase.id()));
}

pub fn synced_phase(base: &EntityBase) -> DragonPhase {
    match base.data.get(DATA_PHASE) {
        Some(SyncedValue::Int(id)) => DragonPhase::from_id(*id).unwrap_or(DragonPhase::Hovering),
        _ => DragonPhase::Hovering,
    }
}

pub fn read(nbt: &Compound, base: &mut EntityBase, state: &mut DragonState) {
    if let Some(phase) = nbt.get_i32("DragonPhase").and_then(DragonPhase::from_id) {
        set_phase(base, state, phase);
    }
    state.death_time = nbt.get_i32("DragonDeathTime").unwrap_or(0);
    state.sitting_damage_received = nbt.get_f64("sitting_damage_received").unwrap_or(0.0) as f32;
    state.exp_to_drop = nbt.get_i32("Bukkit.expToDrop").unwrap_or(state.exp_to_drop);
}

pub fn write(_base: &EntityBase, state: &DragonState, out: &mut Compound) {
    out.insert("DragonPhase", state.phase.id());
    out.insert("DragonDeathTime", state.death_time);
    out.insert("sitting_damage_received", state.sitting_damage_received);
    out.insert("Bukkit.expToDrop", state.exp_to_drop);
}

pub fn experience_reward(previously_killed: Option<bool>, mob_drops: bool) -> i32 {
    if !mob_drops {
        return 0;
    }
    if previously_killed == Some(false) {
        FIRST_KILL_XP
    } else {
        REPEAT_KILL_XP
    }
}

pub fn death_experience(exp_to_drop: i32, death_time: i32, mob_drops: bool) -> i32 {
    if !mob_drops {
        return 0;
    }
    if death_time > DEATH_XP_START && death_time % DEATH_XP_INTERVAL == 0 {
        return (exp_to_drop as f32 * DEATH_XP_FRACTION).floor() as i32;
    }
    0
}

pub fn final_experience(exp_to_drop: i32) -> i32 {
    (exp_to_drop as f32 * DEATH_END_XP_FRACTION).floor() as i32
}

#[cfg(test)]
mod tests {
    use rspigot_entities::EntityKind;
    use uuid::Uuid;

    use super::*;
    use crate::EntityId;
    use crate::entity::save::{load_entity, save_entity};
    use crate::entity::species::Species;

    fn dragon(nbt: Compound) -> crate::entity::Entity {
        load_entity(
            &nbt.with("id", "minecraft:ender_dragon"),
            EntityId(1),
            Uuid::from_u128(1),
        )
        .unwrap()
    }

    #[test]
    fn a_fresh_dragon_hovers_with_the_phase_synced_at_index_sixteen() {
        let entity = dragon(Compound::new());
        assert_eq!(entity.base.kind, EntityKind::EnderDragon);
        assert_eq!(entity.base.data.get(16), Some(&SyncedValue::Int(10)));
        let Some(Species::EnderDragon(state)) = entity
            .living()
            .and_then(|living| living.mob.as_ref())
            .map(|mob| mob.species)
        else {
            panic!("no dragon state");
        };
        assert_eq!(state.phase, DragonPhase::Hovering);
        assert_eq!(state.growl_time, 100);
    }

    #[test]
    fn the_saved_state_keeps_the_phase_death_time_damage_and_experience() {
        let entity = dragon(
            Compound::new()
                .with("DragonPhase", 3)
                .with("DragonDeathTime", 42)
                .with("sitting_damage_received", 12.5f32)
                .with("Bukkit.expToDrop", 500),
        );
        assert_eq!(synced_phase(&entity.base), DragonPhase::Landing);
        let saved = save_entity(&entity, None).unwrap();
        assert_eq!(saved.get_i32("DragonPhase"), Some(3));
        assert_eq!(saved.get_i32("DragonDeathTime"), Some(42));
        assert_eq!(saved.get_f64("sitting_damage_received"), Some(12.5));
        assert_eq!(saved.get_i32("Bukkit.expToDrop"), Some(500));
        let again = load_entity(&saved, EntityId(1), Uuid::from_u128(1)).unwrap();
        assert_eq!(save_entity(&again, None), Some(saved));
    }

    #[test]
    fn an_unknown_phase_id_is_ignored() {
        let entity = dragon(Compound::new().with("DragonPhase", 99));
        assert_eq!(synced_phase(&entity.base), DragonPhase::Hovering);
    }

    #[test]
    fn the_experience_reward_depends_on_the_first_kill_and_the_mob_drops_rule() {
        assert_eq!(experience_reward(Some(false), true), 12_000);
        assert_eq!(experience_reward(Some(true), true), 500);
        assert_eq!(experience_reward(None, true), 500);
        assert_eq!(experience_reward(Some(false), false), 0);
    }

    #[test]
    fn the_death_experience_comes_in_eight_percent_bursts_every_five_ticks_after_one_fifty() {
        assert_eq!(death_experience(12_000, 150, true), 0);
        assert_eq!(death_experience(12_000, 151, true), 0);
        assert_eq!(death_experience(12_000, 155, true), 960);
        assert_eq!(death_experience(500, 155, true), 40);
        assert_eq!(death_experience(12_000, 155, false), 0);
        assert_eq!(final_experience(12_000), 2400);
        assert_eq!(final_experience(500), 100);
    }
}

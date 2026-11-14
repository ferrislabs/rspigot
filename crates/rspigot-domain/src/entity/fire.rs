use rspigot_blocks::{BlockKind, StateFlags};

use super::base::{EntityBase, FLAG_ON_FIRE};
use super::fluid::lava_height;
use super::physics::{BlockView, mth_floor, water_height};
use crate::{BlockPos, BlockStateId};

pub const FIRE_DAMAGE_PERIOD: i32 = 20;
pub const ON_FIRE_DAMAGE: f32 = 1.0;
pub const FIRE_BLOCK_DAMAGE: f32 = 1.0;
pub const SOUL_FIRE_BLOCK_DAMAGE: f32 = 2.0;
pub const LAVA_DAMAGE: f32 = 4.0;
pub const LAVA_SECONDS: f32 = 15.0;
pub const FIRE_SECONDS: f32 = 8.0;
pub const SUN_SECONDS: f32 = 8.0;
pub const PLAYER_FIRE_IMMUNE_TICKS: i32 = 20;
const CAULDRON_INSET: f64 = 0.125;
const INSIDE_DEFLATE: f64 = 9.999_999_747_378_752e-6;

pub fn is_on_fire(base: &EntityBase) -> bool {
    !base.kind.fire_immune() && base.remaining_fire_ticks > 0
}

pub fn clear_fire(base: &mut EntityBase) {
    base.remaining_fire_ticks = base.remaining_fire_ticks.min(0);
}

pub fn ignite_for_ticks(base: &mut EntityBase, ticks: i32) {
    if base.remaining_fire_ticks < ticks {
        base.remaining_fire_ticks = ticks;
    }
    base.set_ticks_frozen(0);
}

pub fn seconds_to_ticks(seconds: f32) -> i32 {
    (seconds * 20.0).floor() as i32
}

pub fn scaled_ticks(ticks: i32, burning_time: f64) -> i32 {
    (f64::from(ticks) * burning_time).ceil() as i32
}

pub fn set_on_fire_flag(base: &mut EntityBase, on_fire: bool) {
    let visual = base.has_visual_fire;
    base.set_shared_flag(FLAG_ON_FIRE, on_fire || visual);
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct FireTick {
    pub burn: bool,
}

pub fn base_tick(base: &mut EntityBase) -> FireTick {
    let mut tick = FireTick::default();
    if base.remaining_fire_ticks > 0 {
        if base.kind.fire_immune() {
            clear_fire(base);
        } else {
            tick.burn =
                base.remaining_fire_ticks % FIRE_DAMAGE_PERIOD == 0 && !base.fluid.in_lava();
            base.remaining_fire_ticks -= 1;
        }
    }
    if base.fluid.in_lava() {
        base.fall_distance *= 0.5;
    }
    tick
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum InsideEffect {
    Freeze,
    ClearFreeze,
    FireIgnite,
    InFire(f32),
    LavaIgnite(BlockPos),
    LavaHurt(BlockPos),
    MeltPowderSnow(BlockPos),
    Cauldron(BlockPos),
    Extinguish,
}

#[derive(Debug, Clone, PartialEq, Default)]
struct StepEffects {
    freeze: bool,
    clear_freeze: bool,
    fire_ignite: bool,
    in_fire: Vec<f32>,
    lava: Option<BlockPos>,
    lava_hurt: usize,
    melt: Vec<BlockPos>,
    cauldrons: Vec<BlockPos>,
    extinguish: bool,
}

impl StepEffects {
    fn flush(self) -> Vec<InsideEffect> {
        let mut out = Vec::new();
        if self.freeze {
            out.push(InsideEffect::Freeze);
        }
        if self.clear_freeze {
            out.push(InsideEffect::ClearFreeze);
        }
        if self.fire_ignite {
            out.push(InsideEffect::FireIgnite);
        }
        out.extend(self.in_fire.into_iter().map(InsideEffect::InFire));
        if let Some(lava) = self.lava {
            out.push(InsideEffect::LavaIgnite(lava));
            out.extend(std::iter::repeat_n(
                InsideEffect::LavaHurt(lava),
                self.lava_hurt,
            ));
        }
        out.extend(self.melt.into_iter().map(InsideEffect::MeltPowderSnow));
        out.extend(self.cauldrons.into_iter().map(InsideEffect::Cauldron));
        if self.extinguish {
            out.push(InsideEffect::Extinguish);
        }
        out
    }
}

fn cauldron_content_height(state: BlockStateId) -> Option<f64> {
    if !matches!(
        state.kind()?,
        BlockKind::WaterCauldron | BlockKind::PowderSnowCauldron
    ) {
        return None;
    }
    let level: f64 = state.property("level")?.parse().ok()?;
    Some((6.0 + 3.0 * level) / 16.0)
}

fn fire_damage(state: BlockStateId) -> Option<f32> {
    match state.kind()? {
        BlockKind::Fire => Some(FIRE_BLOCK_DAMAGE),
        BlockKind::SoulFire => Some(SOUL_FIRE_BLOCK_DAMAGE),
        _ => None,
    }
}

pub fn inside_effects(base: &mut EntityBase, view: &impl BlockView) -> Vec<InsideEffect> {
    base.in_powder_snow = false;
    if base.is_removed() || base.no_physics {
        return Vec::new();
    }
    let bb = base.bounding_box().deflate(INSIDE_DEFLATE);
    let mut step = StepEffects::default();
    let living = base.kind.living();
    let feet = view.block_or_air(base.position.block());
    for x in mth_floor(bb.min_x)..=mth_floor(bb.max_x) {
        for y in mth_floor(bb.min_y)..=mth_floor(bb.max_y) {
            for z in mth_floor(bb.min_z)..=mth_floor(bb.max_z) {
                let pos = BlockPos::new(x, y, z);
                let state = view.block_or_air(pos);
                if state.is_air() {
                    continue;
                }
                if let Some(damage) = fire_damage(state) {
                    step.clear_freeze = true;
                    step.fire_ignite = true;
                    step.in_fire.push(damage);
                }
                if state.kind() == Some(BlockKind::PowderSnow)
                    && (!living || feet.kind() == Some(BlockKind::PowderSnow))
                {
                    step.melt.push(pos);
                    step.freeze = true;
                    step.extinguish = true;
                }
                if state.flags().contains(StateFlags::WATER)
                    && water_height(view, pos).is_some_and(|h| bb.min_y < f64::from(y) + h)
                {
                    step.extinguish = true;
                }
                if is_on_fire(base)
                    && cauldron_content_height(state).is_some_and(|h| {
                        bb.min_y < f64::from(y) + h
                            && bb.max_x > f64::from(x) + CAULDRON_INSET
                            && bb.min_x < f64::from(x) + 1.0 - CAULDRON_INSET
                            && bb.max_z > f64::from(z) + CAULDRON_INSET
                            && bb.min_z < f64::from(z) + 1.0 - CAULDRON_INSET
                    })
                {
                    step.cauldrons.push(pos);
                }
                if lava_height(view, pos).is_some_and(|h| bb.min_y < f64::from(y) + h) {
                    step.clear_freeze = true;
                    step.lava = Some(pos);
                    step.lava_hurt += 1;
                }
            }
        }
    }
    step.flush()
}

pub fn fire_ignite(base: &mut EntityBase, player: bool, extra: impl FnOnce() -> i32) -> bool {
    if base.kind.fire_immune() {
        return false;
    }
    if base.remaining_fire_ticks < 0 {
        base.remaining_fire_ticks += 1;
    } else if player {
        base.remaining_fire_ticks += extra();
    }
    base.remaining_fire_ticks >= 0
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use rspigot_entities::EntityKind;
    use uuid::Uuid;

    use super::*;
    use crate::{EntityId, Vec3};

    struct Blocks(HashMap<BlockPos, BlockStateId>);

    impl BlockView for Blocks {
        fn block(&self, pos: BlockPos) -> Option<BlockStateId> {
            Some(self.0.get(&pos).copied().unwrap_or(BlockStateId::AIR))
        }
    }

    fn zombie() -> EntityBase {
        let mut base = EntityBase::new(EntityId(1), Uuid::from_u128(1), EntityKind::Zombie);
        base.position = Vec3::new(0.5, 64.0, 0.5);
        base
    }

    #[test]
    fn burning_hurts_every_twenty_ticks() {
        let mut base = zombie();
        base.remaining_fire_ticks = 41;
        assert!(!base_tick(&mut base).burn);
        assert!(base_tick(&mut base).burn);
        assert_eq!(base.remaining_fire_ticks, 39);
        ignite_for_ticks(&mut base, 10);
        assert_eq!(base.remaining_fire_ticks, 39);
        clear_fire(&mut base);
        assert_eq!(base.remaining_fire_ticks, 0);
    }

    #[test]
    fn fire_and_water_effects_follow_the_apply_order() {
        let mut base = zombie();
        let mut blocks = HashMap::new();
        blocks.insert(BlockPos::new(0, 64, 0), BlockKind::Fire.default_state());
        blocks.insert(BlockPos::new(0, 65, 0), BlockKind::Water.default_state());
        let effects = inside_effects(&mut base, &Blocks(blocks));
        assert_eq!(
            effects,
            vec![
                InsideEffect::ClearFreeze,
                InsideEffect::FireIgnite,
                InsideEffect::InFire(FIRE_BLOCK_DAMAGE),
                InsideEffect::Extinguish,
            ]
        );
    }

    #[test]
    fn fire_ignite_counts_up_from_negative_ticks() {
        let mut base = zombie();
        base.remaining_fire_ticks = -2;
        assert!(!fire_ignite(&mut base, false, || 1));
        assert!(fire_ignite(&mut base, false, || 1));
    }
}

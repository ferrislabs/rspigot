use rspigot_domain::entity::Pose;
use rspigot_domain::game_event::VibrationData;
use rspigot_domain::warden::{
    AngerLevel, AngerManagement, DEFAULT_ANGER_INCREASE, DIG_COOLDOWN_TICKS,
    PROJECTILE_ANGER_INCREASE, Suspect,
};
use rspigot_domain::{BlockPos, EntityId, SectionPos, Vec3};
use rspigot_entities::EntityKind;

use crate::action::MobAction;
use crate::behavior::{BehaviorCx, MemoryModuleType, MemoryValue, PositionTracker, look_at_entity};
use crate::random::RandomSource;
use crate::world::{AiWorld, Living, MobView};

pub const ANGER_TICK_INTERVAL: i32 = 20;
pub const DARKNESS_INTERVAL: i32 = 120;
pub const DARKNESS_RADIUS: i32 = 20;
pub const VIBRATION_COOLDOWN_TICKS: i64 = 40;
pub const TOUCH_COOLDOWN_TICKS: i64 = 20;
pub const RECENT_PROJECTILE_TICKS: i64 = 100;
pub const SONIC_BOOM_DAMAGE: f32 = 10.0;
pub const SONIC_BOOM_HORIZONTAL_KNOCKBACK: f64 = 2.5;
pub const SONIC_BOOM_VERTICAL_KNOCKBACK: f64 = 0.5;
pub const DISTURBANCE_TICKS: i64 = 100;
pub const PROJECTILE_RANGE: f64 = 30.0;
pub const HURT_TARGET_RANGE: f64 = 5.0;
pub const SONIC_BOOM_COOLDOWN_ON_TARGET: i64 = 200;
const TENDRIL_SOUND: &str = "minecraft:entity.warden.tendril_clicks";
const TENDRIL_VOLUME: f32 = 5.0;
const LISTENING_VOLUME: f32 = 10.0;
const VOICE_PITCH_SPREAD: f32 = 0.2;
const VOICE_PITCH_BASE: f32 = 1.0;
pub const TENDRIL_EVENT: i8 = 61;

#[derive(Debug, Clone, PartialEq)]
pub enum WardenAction {
    Discard,
    Unmount,
    SonicBoom {
        target: EntityId,
        origin: Vec3,
        direction: Vec3,
        length: f64,
    },
    ClientAngerLevel(i32),
    Darkness,
}

#[derive(Debug, Clone, PartialEq)]
pub struct WardenState {
    pub anger: AngerManagement,
    pub vibration: VibrationData,
    pub feet_section: Option<SectionPos>,
    pub listener_section: Option<SectionPos>,
}

impl Default for WardenState {
    fn default() -> Self {
        Self::new(0)
    }
}

impl WardenState {
    pub fn new(conversion_delay: i32) -> Self {
        Self {
            anger: AngerManagement::empty(conversion_delay),
            vibration: VibrationData::default(),
            feet_section: None,
            listener_section: None,
        }
    }
}

pub fn closer_than(from: Vec3, to: Vec3, distance: f64) -> bool {
    from.distance_squared(to) < distance * distance
}

pub fn closer_than_in_plane(from: Vec3, to: Vec3, horizontal: f64, vertical: f64) -> bool {
    let (dx, dy, dz) = (to.x - from.x, to.y - from.y, to.z - from.z);
    dx * dx + dz * dz < horizontal * horizontal && dy * dy < vertical * vertical
}

pub fn can_target_entity<W: AiWorld>(mob: &MobView, world: &W, target: &Living) -> bool {
    if target.id == mob.id || target.spectator || (target.player && target.creative) {
        return false;
    }
    if matches!(target.kind, EntityKind::ArmorStand | EntityKind::Warden)
        || target.invulnerable
        || !target.alive
    {
        return false;
    }
    let bounds = target.bounds;
    world.within_world_border(Vec3::new(bounds.min_x, bounds.min_y, bounds.min_z).block())
        && world.within_world_border(Vec3::new(bounds.max_x, bounds.max_y, bounds.max_z).block())
}

fn attack_target<W: AiWorld>(cx: &BehaviorCx<'_, '_, W>) -> Option<EntityId> {
    cx.brain
        .memories
        .get(MemoryModuleType::AttackTarget)
        .and_then(MemoryValue::as_entity)
}

pub fn active_anger<W: AiWorld>(cx: &BehaviorCx<'_, '_, W>) -> i32 {
    cx.ai.brain.warden.anger.active_anger(attack_target(cx))
}

pub fn anger_level<W: AiWorld>(cx: &BehaviorCx<'_, '_, W>) -> AngerLevel {
    AngerLevel::by_anger(active_anger(cx))
}

pub fn can_target_id<W: AiWorld>(cx: &BehaviorCx<'_, '_, W>, id: EntityId) -> bool {
    cx.ai
        .world
        .living(id)
        .is_some_and(|target| can_target_entity(cx.ai.mob, cx.ai.world, &target))
}

pub fn active_entity<W: AiWorld>(cx: &BehaviorCx<'_, '_, W>) -> Option<EntityId> {
    let mob = cx.ai.mob;
    let world = cx.ai.world;
    cx.ai.brain.warden.anger.active_entity(|id| {
        world
            .living(id)
            .is_some_and(|target| can_target_entity(mob, world, &target))
    })
}

pub fn entity_angry_at<W: AiWorld>(cx: &BehaviorCx<'_, '_, W>) -> Option<EntityId> {
    if anger_level(cx).is_angry() {
        active_entity(cx)
    } else {
        None
    }
}

pub fn voice_pitch<W: AiWorld>(cx: &mut BehaviorCx<'_, '_, W>) -> f32 {
    let random = &mut cx.ai.brain.random;
    (random.next_float() - random.next_float()) * VOICE_PITCH_SPREAD + VOICE_PITCH_BASE
}

pub fn sound<W: AiWorld>(
    cx: &mut BehaviorCx<'_, '_, W>,
    sound: &'static str,
    volume: f32,
    pitch: f32,
) {
    cx.ai.act(MobAction::Sound {
        sound,
        volume,
        pitch,
    });
}

pub fn voiced<W: AiWorld>(cx: &mut BehaviorCx<'_, '_, W>, name: &'static str, volume: f32) {
    let pitch = voice_pitch(cx);
    sound(cx, name, volume, pitch);
}

pub fn set_dig_cooldown<W: AiWorld>(cx: &mut BehaviorCx<'_, '_, W>) {
    if cx.brain.memories.has_value(MemoryModuleType::DigCooldown) {
        cx.brain.memories.set_with_expiry(
            MemoryModuleType::DigCooldown,
            MemoryValue::Unit,
            DIG_COOLDOWN_TICKS,
        );
    }
}

fn play_listening_sound<W: AiWorld>(cx: &mut BehaviorCx<'_, '_, W>) {
    if cx.ai.body.pose == Pose::Roaring {
        return;
    }
    let sound = anger_level(cx).listening_sound();
    voiced(cx, sound, LISTENING_VOLUME);
}

pub fn increase_anger_at<W: AiWorld>(
    cx: &mut BehaviorCx<'_, '_, W>,
    id: EntityId,
    amount: i32,
    play_sound: bool,
) {
    let Some(target) = cx.ai.world.living(id) else {
        return;
    };
    if !can_target_entity(cx.ai.mob, cx.ai.world, &target) {
        return;
    }
    set_dig_cooldown(cx);
    let targeting_player = attack_target(cx)
        .and_then(|current| cx.ai.world.living(current))
        .is_some_and(|current| current.player);
    let suspect = Suspect {
        id,
        uuid: target.facts.uuid,
        player: target.player,
    };
    let anger = cx.ai.brain.warden.anger.increase_anger(suspect, amount);
    if target.player && !targeting_player && AngerLevel::by_anger(anger).is_angry() {
        cx.brain.memories.erase(MemoryModuleType::AttackTarget);
    }
    if play_sound {
        play_listening_sound(cx);
    }
}

pub fn increase_anger_default<W: AiWorld>(cx: &mut BehaviorCx<'_, '_, W>, id: EntityId) {
    increase_anger_at(cx, id, DEFAULT_ANGER_INCREASE, true);
}

pub fn set_disturbance_location<W: AiWorld>(cx: &mut BehaviorCx<'_, '_, W>, pos: BlockPos) {
    if !cx.ai.world.within_world_border(pos) {
        return;
    }
    if entity_angry_at(cx).is_some() || attack_target(cx).is_some() {
        return;
    }
    set_dig_cooldown(cx);
    let memories = &mut cx.brain.memories;
    memories.set_with_expiry(
        MemoryModuleType::SniffCooldown,
        MemoryValue::Unit,
        DISTURBANCE_TICKS,
    );
    memories.set_with_expiry(
        MemoryModuleType::LookTarget,
        MemoryValue::Tracker(PositionTracker::of_block(pos)),
        DISTURBANCE_TICKS,
    );
    memories.set_with_expiry(
        MemoryModuleType::DisturbanceLocation,
        MemoryValue::Block(pos),
        DISTURBANCE_TICKS,
    );
    memories.erase(MemoryModuleType::WalkTarget);
}

pub fn set_attack_target<W: AiWorld>(cx: &mut BehaviorCx<'_, '_, W>, target: EntityId) {
    cx.brain.memories.erase(MemoryModuleType::RoarTarget);
    cx.brain
        .memories
        .set(MemoryModuleType::AttackTarget, MemoryValue::Entity(target));
    cx.brain
        .memories
        .erase(MemoryModuleType::CantReachWalkTargetSince);
    crate::warden::behaviors::set_sonic_boom_cooldown(cx, SONIC_BOOM_COOLDOWN_ON_TARGET);
}

pub fn on_target_invalid<W: AiWorld>(cx: &mut BehaviorCx<'_, '_, W>, target: EntityId) {
    if !can_target_id(cx, target) {
        cx.ai.brain.warden.anger.clear_anger(target);
    }
    set_dig_cooldown(cx);
}

pub fn on_hurt<W: AiWorld>(cx: &mut BehaviorCx<'_, '_, W>, attacker: EntityId, direct: bool) {
    if matches!(cx.ai.body.pose, Pose::Digging | Pose::Emerging) {
        return;
    }
    increase_anger_at(
        cx,
        attacker,
        AngerLevel::Angry.minimum_anger() + rspigot_domain::warden::HURT_ANGER_BONUS,
        false,
    );
    if cx.brain.memories.has_value(MemoryModuleType::AttackTarget) {
        return;
    }
    let Some(living) = cx.ai.world.living(attacker) else {
        return;
    };
    if !direct && !closer_than(cx.ai.mob.position, living.position, HURT_TARGET_RANGE) {
        return;
    }
    set_attack_target(cx, attacker);
}

pub fn touch<W: AiWorld>(cx: &mut BehaviorCx<'_, '_, W>, id: EntityId) {
    if cx.brain.memories.has_value(MemoryModuleType::TouchCooldown) {
        return;
    }
    cx.brain.memories.set_with_expiry(
        MemoryModuleType::TouchCooldown,
        MemoryValue::Unit,
        TOUCH_COOLDOWN_TICKS,
    );
    increase_anger_default(cx, id);
    if let Some(living) = cx.ai.world.living(id) {
        set_disturbance_location(cx, living.position.block());
    }
}

pub fn on_vibration_received<W: AiWorld>(
    cx: &mut BehaviorCx<'_, '_, W>,
    pos: BlockPos,
    source: Option<EntityId>,
    projectile_owner: Option<EntityId>,
) {
    if cx.ai.mob.health <= 0.0 {
        return;
    }
    let mut pos = pos;
    cx.brain.memories.set_with_expiry(
        MemoryModuleType::VibrationCooldown,
        MemoryValue::Unit,
        VIBRATION_COOLDOWN_TICKS,
    );
    cx.ai.act(MobAction::EntityEvent(TENDRIL_EVENT));
    let pitch = voice_pitch(cx);
    sound(cx, TENDRIL_SOUND, TENDRIL_VOLUME, pitch);
    match projectile_owner {
        None => {
            if let Some(source) = source {
                increase_anger_default(cx, source);
            }
        }
        Some(owner) => {
            let owner_living = cx.ai.world.living(owner);
            if owner_living.is_some_and(|living| {
                closer_than(cx.ai.mob.position, living.position, PROJECTILE_RANGE)
            }) {
                if cx
                    .brain
                    .memories
                    .has_value(MemoryModuleType::RecentProjectile)
                {
                    if can_target_id(cx, owner)
                        && let Some(living) = owner_living
                    {
                        pos = living.position.block();
                    }
                    increase_anger_default(cx, owner);
                } else {
                    increase_anger_at(cx, owner, PROJECTILE_ANGER_INCREASE, true);
                }
            }
            cx.brain.memories.set_with_expiry(
                MemoryModuleType::RecentProjectile,
                MemoryValue::Unit,
                RECENT_PROJECTILE_TICKS,
            );
        }
    }
    if anger_level(cx).is_angry() {
        return;
    }
    if projectile_owner.is_none()
        && let Some(active) = active_entity(cx)
        && Some(active) != source
    {
        return;
    }
    set_disturbance_location(cx, pos);
}

pub fn can_receive_vibration<W: AiWorld>(
    mob: &MobView,
    world: &W,
    memories: &crate::behavior::Memories,
    pos: BlockPos,
    source: Option<&Living>,
) -> bool {
    mob.health > 0.0
        && !memories.has_value(MemoryModuleType::VibrationCooldown)
        && !matches!(mob.pose, Pose::Digging | Pose::Emerging)
        && world.within_world_border(pos)
        && source.is_none_or(|source| can_target_entity(mob, world, source))
}

pub fn look_at_roar_target<W: AiWorld>(cx: &mut BehaviorCx<'_, '_, W>, target: EntityId) {
    look_at_entity(cx, target);
}

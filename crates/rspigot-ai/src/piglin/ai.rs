use rspigot_domain::{EntityId, ItemId};
use rspigot_entities::EntityKind;
use rspigot_items::{EquipmentSlot, ItemKind, ModifierOperation};

use crate::action::{BrainMessage, MobAction, PiglinAction};
use crate::behavior::{
    Activity, ActivityData, BehaviorBrain, BehaviorCx, DoNothing, EntityFilter, EraseMemoryIf,
    Gate, GoToTargetLocation, GoToWantedItem, InteractWith, InteractWithDoor, MeleeAttack,
    Memories, MemoryModuleType, MemoryValue, MobControl, MobPredicate, MobTrigger, RandomStroll,
    SensorKind, SetEntityLookTarget, SetLookAndInteract, SetWalkTargetAwayFrom,
    SetWalkTargetFromAttackTargetIfTargetOutOfReach, SetWalkTargetFromLookTarget, StartAttacking,
    StopAttackCondition, StopAttackingIfTargetInvalid, StopBeingAngryIfTargetDead, TargetFinder,
    UniformInt, is_entity_attackable_ignoring_line_of_sight,
    is_other_target_much_further_away_than_current_attack_target, living_from_uuid_memory,
    nearest_target,
};
use crate::goals::TemptItems;
use crate::piglin::behaviors::{
    BackUpIfTooClose, CopyIf, CopyMemoryWithExpiry, RememberIfHoglinWasKilled,
    StartAdmiringItemIfSeen, StartCelebratingIfTargetDead, StartHuntingHoglin,
    StopAdmiringIfItemTooFarAway, StopAdmiringIfTiredOfTryingToReachItem,
    StopHoldingItemIfNoLongerAdmiring,
};
use crate::piglin::sensors::PiglinSensor;
use crate::piglin::targets::{NetherFinder, is_near_zombified};
use crate::world::{AiWorld, Living, MobView};

pub const TIME_BETWEEN_HUNTS: UniformInt = UniformInt::of(600, 2400);
pub const RETREAT_DURATION: UniformInt = UniformInt::of(100, 400);
pub const AVOID_ZOMBIFIED_DURATION: UniformInt = UniformInt::of(100, 140);
pub const BABY_AVOID_NEMESIS_DURATION: UniformInt = UniformInt::of(100, 140);
pub const ADMIRE_DURATION: i32 = 119;
pub const CELEBRATION_TIME: i32 = 300;
pub const ANGER_TICKS: i64 = 600;
const ADMIRING_DISABLED_TICKS: i64 = 400;
const BABY_AVOID_TICKS: i64 = 100;
pub const ATE_RECENTLY_TICKS: i64 = 200;
const MUCH_FURTHER_AWAY: f64 = 4.0;
const NEAR_AVOID_TARGET_DISTANCE: f64 = 12.0;
const PIGLIN_LOVED: TemptItems = TemptItems::Tag("minecraft:piglin_loved");
const PIGLIN_FOOD: TemptItems = TemptItems::Tag("minecraft:piglin_food");
const PIGLIN_REPELLENTS: TemptItems = TemptItems::Tag("minecraft:piglin_repellents");
const PIGLIN_PREFERRED_WEAPONS: TemptItems = TemptItems::Tag("minecraft:piglin_preferred_weapons");
const IGNORED_BY_BABIES: TemptItems = TemptItems::Tag("minecraft:ignored_by_piglin_babies");

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PiglinCondition {
    CanHunt,
    NotHoldingLovedItemInOffHand,
    HasCrossbow,
    NearZombified,
    WantsToStopFleeing,
    Dancing,
    NotDancing,
}

impl PiglinCondition {
    pub fn test<W: AiWorld>(self, cx: &BehaviorCx<'_, '_, W>) -> bool {
        match self {
            Self::CanHunt => can_hunt(cx.ai.mob),
            Self::NotHoldingLovedItemInOffHand => is_not_holding_loved_item_in_off_hand(cx.ai.mob),
            Self::HasCrossbow => cx.ai.mob.is_holding(ItemKind::Crossbow.id()),
            Self::NearZombified => is_near_zombified(cx),
            Self::WantsToStopFleeing => wants_to_stop_fleeing(cx),
            Self::Dancing => cx.ai.mob.species.piglin.dancing,
            Self::NotDancing => !cx.ai.mob.species.piglin.dancing,
        }
    }
}

pub fn can_hunt(mob: &MobView) -> bool {
    mob.kind == EntityKind::Piglin && !mob.species.piglin.cannot_hunt
}

pub fn is_loved_item(item: ItemId) -> bool {
    PIGLIN_LOVED.matches(Some(item))
}

pub fn is_barter_currency(item: ItemId) -> bool {
    item == ItemKind::GoldIngot.id()
}

pub fn is_food(item: ItemId) -> bool {
    PIGLIN_FOOD.matches(Some(item))
}

pub fn is_not_holding_loved_item_in_off_hand(mob: &MobView) -> bool {
    mob.off_hand.is_none_or(|item| !is_loved_item(item))
}

fn is_holding_item_in_off_hand(mob: &MobView) -> bool {
    mob.off_hand.is_some()
}

fn memory_living<W: AiWorld>(
    cx: &BehaviorCx<'_, '_, W>,
    memory: MemoryModuleType,
) -> Option<Living> {
    cx.brain
        .memories
        .get(memory)
        .and_then(MemoryValue::as_entity)
        .and_then(|id| cx.ai.world.living(id))
}

fn ids<W: AiWorld>(cx: &BehaviorCx<'_, '_, W>, memory: MemoryModuleType) -> Vec<EntityId> {
    cx.brain
        .memories
        .get(memory)
        .and_then(MemoryValue::as_entities)
        .map(<[_]>::to_vec)
        .unwrap_or_default()
}

fn count<W: AiWorld>(cx: &BehaviorCx<'_, '_, W>, memory: MemoryModuleType) -> i32 {
    cx.brain
        .memories
        .get(memory)
        .and_then(MemoryValue::as_int)
        .unwrap_or(0)
}

fn hoglins_outnumber_piglins<W: AiWorld>(cx: &BehaviorCx<'_, '_, W>) -> bool {
    count(cx, MemoryModuleType::VisibleAdultPiglinCount) + 1
        < count(cx, MemoryModuleType::VisibleAdultHoglinCount)
}

fn piglins_equal_or_outnumber_hoglins<W: AiWorld>(cx: &BehaviorCx<'_, '_, W>) -> bool {
    !hoglins_outnumber_piglins(cx)
}

pub fn wants_to_stop_fleeing<W: AiWorld>(cx: &BehaviorCx<'_, '_, W>) -> bool {
    let Some(avoid) = memory_living(cx, MemoryModuleType::AvoidTarget) else {
        return true;
    };
    if avoid.kind == EntityKind::Hoglin {
        return piglins_equal_or_outnumber_hoglins(cx);
    }
    if crate::piglin::sensors::is_zombified(avoid.kind) {
        return !cx.brain.memories.is_value(
            MemoryModuleType::NearestVisibleZombified,
            &MemoryValue::Entity(avoid.id),
        );
    }
    false
}

pub fn dont_kill_any_more_hoglins_for_a_while<W: AiWorld>(cx: &mut BehaviorCx<'_, '_, W>) {
    let ticks = TIME_BETWEEN_HUNTS.sample(&mut cx.ai.brain.random);
    cx.brain.memories.set_with_expiry(
        MemoryModuleType::HuntedRecently,
        MemoryValue::Bool(true),
        i64::from(ticks),
    );
}

pub fn set_anger_target<W: AiWorld>(cx: &mut BehaviorCx<'_, '_, W>, target: &Living) {
    if cx.ai.mob.kind != EntityKind::PiglinBrute
        && !is_entity_attackable_ignoring_line_of_sight(cx.ai, &cx.brain.memories, target)
    {
        return;
    }
    cx.brain
        .memories
        .erase(MemoryModuleType::CantReachWalkTargetSince);
    cx.brain.memories.set_with_expiry(
        MemoryModuleType::AngryAt,
        MemoryValue::Uuid(target.facts.uuid),
        ANGER_TICKS,
    );
    if cx.ai.mob.kind == EntityKind::PiglinBrute {
        return;
    }
    if target.kind == EntityKind::Hoglin && can_hunt(cx.ai.mob) {
        dont_kill_any_more_hoglins_for_a_while(cx);
    }
    if target.player && cx.ai.world.universal_anger() {
        cx.brain.memories.set_with_expiry(
            MemoryModuleType::UniversalAnger,
            MemoryValue::Bool(true),
            ANGER_TICKS,
        );
    }
}

fn set_anger_target_to_nearest_targetable_player_if_found<W: AiWorld>(
    cx: &mut BehaviorCx<'_, '_, W>,
    target_if_no_player_found: &Living,
) {
    match memory_living(cx, MemoryModuleType::NearestVisibleAttackablePlayer) {
        Some(player) => set_anger_target(cx, &player),
        None => set_anger_target(cx, target_if_no_player_found),
    }
}

fn set_anger_target_if_closer_than_current<W: AiWorld>(
    cx: &mut BehaviorCx<'_, '_, W>,
    new_target: &Living,
) {
    let current = living_from_uuid_memory(cx, MemoryModuleType::AngryAt);
    let nearest = nearest_target(cx, current, *new_target);
    if current.is_some_and(|current| current.id == nearest.id) {
        return;
    }
    set_anger_target(cx, &nearest);
}

pub fn broadcast_anger_target<W: AiWorld>(cx: &mut BehaviorCx<'_, '_, W>, target: &Living) {
    for piglin in ids(cx, MemoryModuleType::NearbyAdultPiglins) {
        cx.ai.act(MobAction::Message {
            to: piglin,
            message: BrainMessage::PiglinAngerIfCloser { target: target.id },
        });
    }
}

fn broadcast_universal_anger<W: AiWorld>(cx: &mut BehaviorCx<'_, '_, W>) {
    for piglin in ids(cx, MemoryModuleType::NearbyAdultPiglins) {
        cx.ai.act(MobAction::Message {
            to: piglin,
            message: BrainMessage::PiglinUniversalAnger,
        });
    }
}

fn broadcast_retreat<W: AiWorld>(cx: &mut BehaviorCx<'_, '_, W>, target: &Living) {
    for piglin in ids(cx, MemoryModuleType::NearestVisibleAdultPiglins) {
        let is_piglin = cx
            .ai
            .world
            .living(piglin)
            .is_some_and(|other| other.kind == EntityKind::Piglin);
        if is_piglin {
            cx.ai.act(MobAction::Message {
                to: piglin,
                message: BrainMessage::PiglinRetreat { target: target.id },
            });
        }
    }
}

fn set_avoid_target_and_dont_hunt_for_a_while<W: AiWorld>(
    cx: &mut BehaviorCx<'_, '_, W>,
    target: &Living,
) {
    cx.brain.memories.erase(MemoryModuleType::AngryAt);
    cx.brain.memories.erase(MemoryModuleType::AttackTarget);
    cx.brain.memories.erase(MemoryModuleType::WalkTarget);
    let ticks = RETREAT_DURATION.sample(&mut cx.ai.brain.random);
    cx.brain.memories.set_with_expiry(
        MemoryModuleType::AvoidTarget,
        MemoryValue::Entity(target.id),
        i64::from(ticks),
    );
    dont_kill_any_more_hoglins_for_a_while(cx);
}

fn retreat_from_nearest_target<W: AiWorld>(cx: &mut BehaviorCx<'_, '_, W>, new_avoid: &Living) {
    let avoid = memory_living(cx, MemoryModuleType::AvoidTarget);
    let nearest = nearest_target(cx, avoid, *new_avoid);
    let attack = memory_living(cx, MemoryModuleType::AttackTarget);
    let nearest = nearest_target(cx, attack, nearest);
    set_avoid_target_and_dont_hunt_for_a_while(cx, &nearest);
}

pub fn maybe_retaliate<W: AiWorld>(cx: &mut BehaviorCx<'_, '_, W>, attacker: &Living) {
    if cx.brain.is_active(Activity::Avoid)
        || !is_entity_attackable_ignoring_line_of_sight(cx.ai, &cx.brain.memories, attacker)
        || is_other_target_much_further_away_than_current_attack_target(
            cx,
            attacker,
            MUCH_FURTHER_AWAY,
        )
    {
        return;
    }
    if attacker.player && cx.ai.world.universal_anger() {
        set_anger_target_to_nearest_targetable_player_if_found(cx, attacker);
        broadcast_universal_anger(cx);
        return;
    }
    set_anger_target(cx, attacker);
    broadcast_anger_target(cx, attacker);
}

pub fn was_hurt_by<W: AiWorld>(cx: &mut BehaviorCx<'_, '_, W>, attacker: &Living) {
    if attacker.kind == EntityKind::Piglin {
        return;
    }
    if is_holding_item_in_off_hand(cx.ai.mob) {
        cx.ai
            .act(MobAction::Piglin(PiglinAction::StopHoldingOffHand {
                bartering_enabled: false,
            }));
    }
    cx.brain.memories.erase(MemoryModuleType::CelebrateLocation);
    cx.brain.memories.erase(MemoryModuleType::Dancing);
    cx.brain.memories.erase(MemoryModuleType::AdmiringItem);
    if attacker.player {
        cx.brain.memories.set_with_expiry(
            MemoryModuleType::AdmiringDisabled,
            MemoryValue::Bool(true),
            ADMIRING_DISABLED_TICKS,
        );
    }
    if let Some(avoid) = memory_living(cx, MemoryModuleType::AvoidTarget)
        && avoid.kind != attacker.kind
    {
        cx.brain.memories.erase(MemoryModuleType::AvoidTarget);
    }
    if cx.ai.mob.baby {
        cx.brain.memories.set_with_expiry(
            MemoryModuleType::AvoidTarget,
            MemoryValue::Entity(attacker.id),
            BABY_AVOID_TICKS,
        );
        if is_entity_attackable_ignoring_line_of_sight(cx.ai, &cx.brain.memories, attacker) {
            broadcast_anger_target(cx, attacker);
        }
        return;
    }
    if attacker.kind == EntityKind::Hoglin && hoglins_outnumber_piglins(cx) {
        set_avoid_target_and_dont_hunt_for_a_while(cx, attacker);
        broadcast_retreat(cx, attacker);
        return;
    }
    maybe_retaliate(cx, attacker);
}

pub fn brute_was_hurt_by<W: AiWorld>(cx: &mut BehaviorCx<'_, '_, W>, attacker: &Living) {
    if matches!(attacker.kind, EntityKind::Piglin | EntityKind::PiglinBrute) {
        return;
    }
    maybe_retaliate(cx, attacker);
}

pub fn anger_from_player<W: AiWorld>(
    cx: &mut BehaviorCx<'_, '_, W>,
    player: &Living,
    only_if_sees: bool,
) {
    if cx.ai.mob.kind != EntityKind::Piglin || !cx.brain.is_active(Activity::Idle) {
        return;
    }
    if only_if_sees && !can_see(cx, player) {
        return;
    }
    if cx.ai.world.universal_anger() {
        set_anger_target_to_nearest_targetable_player_if_found(cx, player);
    } else {
        set_anger_target(cx, player);
    }
}

fn can_see<W: AiWorld>(cx: &mut BehaviorCx<'_, '_, W>, target: &Living) -> bool {
    cx.brain
        .memories
        .get(MemoryModuleType::NearestVisibleLivingEntities)
        .and_then(MemoryValue::as_visible)
        .is_some_and(|visible| visible.contains_entity(cx.ai, &cx.brain.memories, target.id))
}

pub fn deliver<W: AiWorld>(cx: &mut BehaviorCx<'_, '_, W>, message: BrainMessage) {
    match message {
        BrainMessage::PiglinAngerIfCloser { target } => {
            let Some(target) = cx.ai.world.living(target) else {
                return;
            };
            if target.kind == EntityKind::Hoglin
                && !(can_hunt(cx.ai.mob) && !target.baby && !target.facts.cannot_be_hunted)
            {
                return;
            }
            set_anger_target_if_closer_than_current(cx, &target);
        }
        BrainMessage::PiglinRetreat { target } => {
            if cx.ai.mob.kind != EntityKind::Piglin {
                return;
            }
            if let Some(target) = cx.ai.world.living(target) {
                retreat_from_nearest_target(cx, &target);
            }
        }
        BrainMessage::PiglinDontHunt => dont_kill_any_more_hoglins_for_a_while(cx),
        BrainMessage::PiglinUniversalAnger => {
            if let Some(player) =
                memory_living(cx, MemoryModuleType::NearestVisibleAttackablePlayer)
            {
                set_anger_target(cx, &player);
            }
        }
        BrainMessage::PiglinHurtBy { attacker } => {
            if let Some(attacker) = cx.ai.world.living(attacker) {
                was_hurt_by(cx, &attacker);
            }
        }
        BrainMessage::PiglinBruteHurtBy { attacker } => {
            if let Some(attacker) = cx.ai.world.living(attacker) {
                brute_was_hurt_by(cx, &attacker);
            }
        }
        BrainMessage::PiglinAngerFromPlayer {
            player,
            only_if_sees,
        } => {
            if let Some(player) = cx.ai.world.living(player) {
                anger_from_player(cx, &player, only_if_sees);
            }
        }
        _ => {}
    }
}

pub fn equipment_slot_for(item: ItemId) -> EquipmentSlot {
    item.kind()
        .and_then(ItemKind::equippable)
        .unwrap_or(EquipmentSlot::Mainhand)
}

fn is_armor_slot(slot: EquipmentSlot) -> bool {
    matches!(
        slot,
        EquipmentSlot::Feet | EquipmentSlot::Legs | EquipmentSlot::Chest | EquipmentSlot::Head
    )
}

fn approximate_attribute(item: ItemId, attribute: &str, slot: EquipmentSlot) -> f64 {
    let Some(kind) = item.kind() else {
        return 0.0;
    };
    let modifiers = kind
        .attribute_modifiers()
        .iter()
        .filter(|modifier| modifier.attribute == attribute && modifier.slot.contains(slot));
    let mut base = 0.0;
    let mut multiplied_base = 0.0;
    let mut multiplied_total = 1.0;
    for modifier in modifiers {
        match modifier.operation {
            ModifierOperation::AddValue => base += modifier.amount,
            ModifierOperation::AddMultipliedBase => multiplied_base += modifier.amount,
            ModifierOperation::AddMultipliedTotal => multiplied_total *= 1.0 + modifier.amount,
        }
    }
    let value = base + base * multiplied_base;
    value * multiplied_total
}

fn mob_can_replace_current_item(
    new: ItemId,
    current: Option<ItemId>,
    slot: EquipmentSlot,
    preferred: Option<TemptItems>,
) -> bool {
    let Some(current) = current else {
        return true;
    };
    if is_armor_slot(slot) {
        let (armor, current_armor) = (
            approximate_attribute(new, "minecraft:armor", slot),
            approximate_attribute(current, "minecraft:armor", slot),
        );
        if armor != current_armor {
            return armor > current_armor;
        }
        let (toughness, current_toughness) = (
            approximate_attribute(new, "minecraft:armor_toughness", slot),
            approximate_attribute(current, "minecraft:armor_toughness", slot),
        );
        return toughness > current_toughness;
    }
    if slot != EquipmentSlot::Mainhand {
        return false;
    }
    if let Some(tag) = preferred {
        if tag.matches(Some(current)) && !tag.matches(Some(new)) {
            return false;
        }
        if !tag.matches(Some(current)) && tag.matches(Some(new)) {
            return true;
        }
    }
    let (damage, current_damage) = (
        approximate_attribute(new, "minecraft:attack_damage", slot),
        approximate_attribute(current, "minecraft:attack_damage", slot),
    );
    damage > current_damage
}

fn item_in_slot(mob: &MobView, slot: EquipmentSlot) -> Option<ItemId> {
    match slot {
        EquipmentSlot::Mainhand => mob.main_hand,
        EquipmentSlot::Offhand => mob.off_hand,
        EquipmentSlot::Feet => mob.species.piglin.feet,
        EquipmentSlot::Legs => mob.species.piglin.legs,
        EquipmentSlot::Chest => mob.species.piglin.chest,
        EquipmentSlot::Head => mob.species.piglin.head,
        EquipmentSlot::Body | EquipmentSlot::Saddle => None,
    }
}

pub fn can_replace_current_item(mob: &MobView, item: ItemId) -> bool {
    let slot = equipment_slot_for(item);
    let current = item_in_slot(mob, slot);
    if mob.kind != EntityKind::Piglin {
        return mob_can_replace_current_item(item, current, slot, None);
    }
    let preferred = (!mob.baby).then_some(PIGLIN_PREFERRED_WEAPONS);
    let new_good = is_loved_item(item) || preferred.is_some_and(|tag| tag.matches(Some(item)));
    let current_good = current.is_some_and(|current| {
        is_loved_item(current) || preferred.is_some_and(|tag| tag.matches(Some(current)))
    });
    if new_good && !current_good {
        return true;
    }
    if !new_good && current_good {
        return false;
    }
    mob_can_replace_current_item(item, current, slot, preferred)
}

pub fn wants_to_pickup(mob: &MobView, memories: &Memories, item: ItemId) -> bool {
    if mob.baby && IGNORED_BY_BABIES.matches(Some(item)) {
        return false;
    }
    if PIGLIN_REPELLENTS.matches(Some(item)) {
        return false;
    }
    if memories.has_value(MemoryModuleType::AdmiringDisabled)
        && memories.has_value(MemoryModuleType::AttackTarget)
    {
        return false;
    }
    if is_barter_currency(item) {
        return is_not_holding_loved_item_in_off_hand(mob);
    }
    let can_add = mob.species.piglin.can_add_to_inventory(item);
    if item == ItemKind::GoldNugget.id() {
        return can_add;
    }
    if is_food(item) {
        return !memories.has_value(MemoryModuleType::AteRecently) && can_add;
    }
    if is_loved_item(item) {
        return is_not_holding_loved_item_in_off_hand(mob) && can_add;
    }
    can_replace_current_item(mob, item)
}

pub fn can_admire(mob: &MobView, memories: &Memories, held: ItemId) -> bool {
    !memories.has_value(MemoryModuleType::AdmiringDisabled)
        && !memories.has_value(MemoryModuleType::AdmiringItem)
        && !mob.baby
        && is_barter_currency(held)
}

pub fn sound_for_activity<W: AiWorld>(
    cx: &BehaviorCx<'_, '_, W>,
    activity: Activity,
) -> &'static str {
    if activity == Activity::Fight {
        return "minecraft:entity.piglin.angry";
    }
    if cx.ai.mob.species.piglin.converting {
        return "minecraft:entity.piglin.retreat";
    }
    if activity == Activity::Avoid
        && memory_living(cx, MemoryModuleType::AvoidTarget).is_some_and(|avoid| {
            cx.ai.mob.distance_to_sqr(avoid.position)
                < NEAR_AVOID_TARGET_DISTANCE * NEAR_AVOID_TARGET_DISTANCE
        })
    {
        return "minecraft:entity.piglin.retreat";
    }
    if activity == Activity::AdmireItem {
        return "minecraft:entity.piglin.admiring_item";
    }
    if activity == Activity::Celebrate {
        return "minecraft:entity.piglin.celebrate";
    }
    if cx
        .brain
        .memories
        .has_value(MemoryModuleType::NearestPlayerHoldingWantedItem)
    {
        return "minecraft:entity.piglin.jealous";
    }
    if cx
        .brain
        .memories
        .has_value(MemoryModuleType::NearestRepellent)
    {
        return "minecraft:entity.piglin.retreat";
    }
    "minecraft:entity.piglin.ambient"
}

pub fn update_activity<W: AiWorld>(
    brain: &mut BehaviorBrain<MobControl>,
    ai: &mut crate::brain::AiContext<'_, W>,
) {
    let before = brain.state().active_non_core_activity();
    brain.state_mut().set_active_activity_to_first_valid(&[
        Activity::AdmireItem,
        Activity::Fight,
        Activity::Avoid,
        Activity::Celebrate,
        Activity::Idle,
    ]);
    let after = brain.state().active_non_core_activity();
    if before != after
        && let Some(activity) = after
    {
        let cx = BehaviorCx {
            ai: &mut *ai,
            brain: brain.state_mut(),
        };
        let sound = sound_for_activity(&cx, activity);
        cx.ai.act(MobAction::Voice(sound));
    }
    let state = brain.state_mut();
    ai.body.aggressive = state.memories.has_value(MemoryModuleType::AttackTarget);
    if !state
        .memories
        .has_value(MemoryModuleType::CelebrateLocation)
    {
        state.memories.erase(MemoryModuleType::Dancing);
    }
    let dancing = state.memories.has_value(MemoryModuleType::Dancing);
    if dancing != ai.mob.species.piglin.dancing {
        ai.act(MobAction::Piglin(PiglinAction::SetDancing(dancing)));
    }
}

fn look_behaviors() -> Vec<(MobControl, i32)> {
    let look = |filter| {
        MobControl::one_shot(MobTrigger::SetEntityLookTarget(SetEntityLookTarget::new(
            filter, 8.0,
        )))
    };
    vec![
        (look(EntityFilter::Kind(EntityKind::Player)), 1),
        (look(EntityFilter::Kind(EntityKind::Piglin)), 1),
        (look(EntityFilter::Any), 1),
    ]
}

fn idle_look_behaviors() -> Gate<MobControl> {
    let mut behaviors = look_behaviors();
    behaviors.push((MobControl::DoNothing(DoNothing::new(30, 60)), 1));
    Gate::run_one(behaviors)
}

fn idle_movement_behaviors() -> Gate<MobControl> {
    Gate::run_one(vec![
        (
            MobControl::one_shot(MobTrigger::RandomStroll(RandomStroll::stroll(0.6))),
            2,
        ),
        (
            MobControl::one_shot(MobTrigger::InteractWith(InteractWith::new(
                EntityKind::Piglin,
                8,
                MemoryModuleType::InteractionTarget,
                0.6,
                2,
            ))),
            2,
        ),
        (
            MobControl::trigger_if(
                MobPredicate::Not(Box::new(MobPredicate::HasMemory(
                    MemoryModuleType::NearestPlayerHoldingWantedItem,
                ))),
                MobControl::declared(MobTrigger::SetWalkTargetFromLookTarget(
                    SetWalkTargetFromLookTarget::new(0.6, 3),
                )),
            ),
            2,
        ),
        (MobControl::DoNothing(DoNothing::new(30, 60)), 1),
    ])
}

fn avoid_repellent() -> MobControl {
    MobControl::one_shot(MobTrigger::SetWalkTargetAwayFrom(
        SetWalkTargetAwayFrom::pos(MemoryModuleType::NearestRepellent, 1.0, 8, false),
    ))
}

fn start_attacking() -> MobControl {
    MobControl::one_shot(MobTrigger::StartAttacking(StartAttacking::when(
        MobPredicate::IsAdult,
        TargetFinder::Nether(NetherFinder::Piglin),
    )))
}

fn look_at_unless_jealous() -> MobControl {
    MobControl::one_shot(MobTrigger::SetEntityLookTarget(SetEntityLookTarget::new(
        EntityFilter::SeesNoPlayerHoldingLovedItem,
        14.0,
    )))
}

pub fn piglin_activities() -> Vec<ActivityData<MobControl>> {
    let core = ActivityData::create(
        Activity::Core,
        0,
        vec![
            MobControl::default_look_at_target_sink(),
            MobControl::move_to_target_sink(),
            MobControl::one_shot(MobTrigger::InteractWithDoor(InteractWithDoor::default())),
            MobControl::one_shot(MobTrigger::CopyMemoryWithExpiry(CopyMemoryWithExpiry::new(
                CopyIf::IsBaby,
                MemoryModuleType::NearestVisibleNemesis,
                MemoryModuleType::AvoidTarget,
                BABY_AVOID_NEMESIS_DURATION,
            ))),
            MobControl::one_shot(MobTrigger::CopyMemoryWithExpiry(CopyMemoryWithExpiry::new(
                CopyIf::NearZombified,
                MemoryModuleType::NearestVisibleZombified,
                MemoryModuleType::AvoidTarget,
                AVOID_ZOMBIFIED_DURATION,
            ))),
            MobControl::one_shot(MobTrigger::StopHoldingItemIfNoLongerAdmiring(
                StopHoldingItemIfNoLongerAdmiring,
            )),
            MobControl::one_shot(MobTrigger::StartAdmiringItemIfSeen(
                StartAdmiringItemIfSeen::new(ADMIRE_DURATION),
            )),
            MobControl::one_shot(MobTrigger::StartCelebratingIfTargetDead(
                StartCelebratingIfTargetDead::new(CELEBRATION_TIME),
            )),
            MobControl::one_shot(MobTrigger::StopBeingAngryIfTargetDead(
                StopBeingAngryIfTargetDead,
            )),
        ],
    );
    let idle = ActivityData::create(
        Activity::Idle,
        10,
        vec![
            look_at_unless_jealous(),
            start_attacking(),
            MobControl::trigger_if(
                MobPredicate::Piglin(PiglinCondition::CanHunt),
                MobControl::declared(MobTrigger::StartHuntingHoglin(StartHuntingHoglin)),
            ),
            avoid_repellent(),
            MobControl::Gate(idle_look_behaviors()),
            MobControl::Gate(idle_movement_behaviors()),
            MobControl::one_shot(MobTrigger::SetLookAndInteract(SetLookAndInteract::new(
                EntityKind::Player,
                4,
            ))),
        ],
    );
    let admire = ActivityData::create_erasing(
        Activity::AdmireItem,
        10,
        vec![
            MobControl::one_shot(MobTrigger::GoToWantedItem(GoToWantedItem::when(
                MobPredicate::Piglin(PiglinCondition::NotHoldingLovedItemInOffHand),
                1.0,
                true,
                9,
            ))),
            MobControl::one_shot(MobTrigger::StopAdmiringIfItemTooFarAway(
                StopAdmiringIfItemTooFarAway::new(9),
            )),
            MobControl::one_shot(MobTrigger::StopAdmiringIfTiredOfTryingToReachItem(
                StopAdmiringIfTiredOfTryingToReachItem::new(200, 200),
            )),
        ],
        MemoryModuleType::AdmiringItem,
    );
    let fight = ActivityData::from_pairs_erasing(
        Activity::Fight,
        vec![
            (
                10,
                MobControl::one_shot(MobTrigger::StopAttackingIfTargetInvalid(
                    StopAttackingIfTargetInvalid::new(
                        StopAttackCondition::NotNearestValid(NetherFinder::Piglin),
                        crate::behavior::TargetErased::Nothing,
                        true,
                    ),
                )),
            ),
            (
                11,
                MobControl::trigger_if(
                    MobPredicate::Piglin(PiglinCondition::HasCrossbow),
                    MobControl::declared(MobTrigger::BackUpIfTooClose(BackUpIfTooClose::new(
                        5, 0.75,
                    ))),
                ),
            ),
            (
                12,
                MobControl::one_shot(MobTrigger::SetWalkTargetFromAttackTargetIfTargetOutOfReach(
                    SetWalkTargetFromAttackTargetIfTargetOutOfReach::new(1.0),
                )),
            ),
            (
                16,
                MobControl::one_shot(MobTrigger::MeleeAttack(MeleeAttack::new(20))),
            ),
            (17, MobControl::crossbow_attack()),
            (
                18,
                MobControl::one_shot(MobTrigger::RememberIfHoglinWasKilled(
                    RememberIfHoglinWasKilled,
                )),
            ),
            (
                19,
                MobControl::one_shot(MobTrigger::EraseMemoryIf(EraseMemoryIf::new(
                    MobPredicate::Piglin(PiglinCondition::NearZombified),
                    MemoryModuleType::AttackTarget,
                ))),
            ),
        ],
        MemoryModuleType::AttackTarget,
    );
    let celebrate = ActivityData::create_erasing(
        Activity::Celebrate,
        10,
        vec![
            avoid_repellent(),
            look_at_unless_jealous(),
            start_attacking(),
            MobControl::trigger_if(
                MobPredicate::Piglin(PiglinCondition::NotDancing),
                MobControl::declared(MobTrigger::GoToTargetLocation(GoToTargetLocation::new(
                    MemoryModuleType::CelebrateLocation,
                    2,
                    1.0,
                ))),
            ),
            MobControl::trigger_if(
                MobPredicate::Piglin(PiglinCondition::Dancing),
                MobControl::declared(MobTrigger::GoToTargetLocation(GoToTargetLocation::new(
                    MemoryModuleType::CelebrateLocation,
                    4,
                    0.6,
                ))),
            ),
            MobControl::Gate(Gate::run_one(vec![
                (
                    MobControl::one_shot(MobTrigger::SetEntityLookTarget(
                        SetEntityLookTarget::new(EntityFilter::Kind(EntityKind::Piglin), 8.0),
                    )),
                    1,
                ),
                (
                    MobControl::one_shot(MobTrigger::RandomStroll(RandomStroll::stroll_within(
                        0.6, 2, 1,
                    ))),
                    1,
                ),
                (MobControl::DoNothing(DoNothing::new(10, 20)), 1),
            ])),
        ],
        MemoryModuleType::CelebrateLocation,
    );
    let retreat = ActivityData::create_erasing(
        Activity::Avoid,
        10,
        vec![
            MobControl::one_shot(MobTrigger::SetWalkTargetAwayFrom(
                SetWalkTargetAwayFrom::entity(MemoryModuleType::AvoidTarget, 1.0, 12, true),
            )),
            MobControl::Gate(idle_look_behaviors()),
            MobControl::Gate(idle_movement_behaviors()),
            MobControl::one_shot(MobTrigger::EraseMemoryIf(EraseMemoryIf::new(
                MobPredicate::Piglin(PiglinCondition::WantsToStopFleeing),
                MemoryModuleType::AvoidTarget,
            ))),
        ],
        MemoryModuleType::AvoidTarget,
    );
    vec![core, idle, admire, fight, celebrate, retreat]
}

pub fn piglin_sensors() -> Vec<SensorKind> {
    use crate::behavior::{
        HurtBySensor, ItemWish, NearestItemSensor, NearestLivingEntitySensor, PlayerSensor,
    };
    vec![
        SensorKind::NearestLivingEntities(NearestLivingEntitySensor),
        SensorKind::NearestPlayers(PlayerSensor),
        SensorKind::NearestItems(NearestItemSensor::new(ItemWish::Piglin)),
        SensorKind::HurtBy(HurtBySensor),
        SensorKind::PiglinSpecific(PiglinSensor),
    ]
}

pub fn piglin_memories() -> &'static [MemoryModuleType] {
    &[
        MemoryModuleType::DoorsToClose,
        MemoryModuleType::UniversalAnger,
        MemoryModuleType::AteRecently,
        MemoryModuleType::SpearFleeingTime,
        MemoryModuleType::SpearFleeingPosition,
        MemoryModuleType::SpearChargePosition,
        MemoryModuleType::SpearEngageTime,
    ]
}

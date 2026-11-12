use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap};

use rspigot_domain::{BlockPos, EntityId, Vec3};
use uuid::Uuid;

use crate::behavior::nbt::PackedMemories;
use crate::behavior::sensor::is_entity_targetable;
use crate::behavior::tracker::PositionTracker;
use crate::brain::AiContext;
use crate::path::Path;
use crate::world::{AiWorld, LastDamage, Living};

pub const NO_EXPIRY: i64 = i64::MAX;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Codec {
    Transient,
    GlobalPos,
    Bool,
    Long,
    Int,
    Unit,
    Uuid,
    Block,
    GlobalPosList,
}

macro_rules! memory_types {
    ($($variant:ident = $name:literal, $codec:ident;)*) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
        pub enum MemoryModuleType {
            $($variant,)*
        }

        impl MemoryModuleType {
            pub const ALL: &'static [Self] = &[$(Self::$variant,)*];

            pub const fn name(self) -> &'static str {
                match self {
                    $(Self::$variant => $name,)*
                }
            }

            pub const fn codec(self) -> Codec {
                match self {
                    $(Self::$variant => Codec::$codec,)*
                }
            }
        }
    };
}

memory_types! {
    Dummy = "minecraft:dummy", Transient;
    Home = "minecraft:home", GlobalPos;
    JobSite = "minecraft:job_site", GlobalPos;
    PotentialJobSite = "minecraft:potential_job_site", GlobalPos;
    MeetingPoint = "minecraft:meeting_point", GlobalPos;
    SecondaryJobSite = "minecraft:secondary_job_site", Transient;
    NearestLivingEntities = "minecraft:mobs", Transient;
    NearestVisibleLivingEntities = "minecraft:visible_mobs", Transient;
    VisibleVillagerBabies = "minecraft:visible_villager_babies", Transient;
    NearestPlayers = "minecraft:nearest_players", Transient;
    NearestVisiblePlayer = "minecraft:nearest_visible_player", Transient;
    NearestVisibleAttackablePlayer = "minecraft:nearest_visible_targetable_player", Transient;
    NearestVisibleAttackablePlayers = "minecraft:nearest_visible_targetable_players", Transient;
    WalkTarget = "minecraft:walk_target", Transient;
    LookTarget = "minecraft:look_target", Transient;
    AttackTarget = "minecraft:attack_target", Transient;
    AttackCoolingDown = "minecraft:attack_cooling_down", Transient;
    InteractionTarget = "minecraft:interaction_target", Transient;
    BreedTarget = "minecraft:breed_target", Transient;
    RideTarget = "minecraft:ride_target", Transient;
    Path = "minecraft:path", Transient;
    DoorsToClose = "minecraft:doors_to_close", Transient;
    NearestBed = "minecraft:nearest_bed", Transient;
    HurtBy = "minecraft:hurt_by", Transient;
    HurtByEntity = "minecraft:hurt_by_entity", Transient;
    AvoidTarget = "minecraft:avoid_target", Transient;
    NearestHostile = "minecraft:nearest_hostile", Transient;
    NearestAttackable = "minecraft:nearest_attackable", Transient;
    HidingPlace = "minecraft:hiding_place", Transient;
    HeardBellTime = "minecraft:heard_bell_time", Transient;
    CantReachWalkTargetSince = "minecraft:cant_reach_walk_target_since", Transient;
    GolemDetectedRecently = "minecraft:golem_detected_recently", Bool;
    DangerDetectedRecently = "minecraft:danger_detected_recently", Bool;
    LastSlept = "minecraft:last_slept", Long;
    LastWoken = "minecraft:last_woken", Long;
    LastWorkedAtPoi = "minecraft:last_worked_at_poi", Long;
    NearestVisibleAdult = "minecraft:nearest_visible_adult", Transient;
    NearestVisibleWantedItem = "minecraft:nearest_visible_wanted_item", Transient;
    NearestVisibleNemesis = "minecraft:nearest_visible_nemesis", Transient;
    PlayDeadTicks = "minecraft:play_dead_ticks", Int;
    TemptingPlayer = "minecraft:tempting_player", Transient;
    TemptationCooldownTicks = "minecraft:temptation_cooldown_ticks", Int;
    GazeCooldownTicks = "minecraft:gaze_cooldown_ticks", Int;
    LongJumpCooldownTicks = "minecraft:long_jump_cooling_down", Int;
    LongJumpMidJump = "minecraft:long_jump_mid_jump", Transient;
    HasHuntingCooldown = "minecraft:has_hunting_cooldown", Bool;
    RamCooldownTicks = "minecraft:ram_cooldown_ticks", Int;
    RamTarget = "minecraft:ram_target", Transient;
    IsInWater = "minecraft:is_in_water", Unit;
    IsPregnant = "minecraft:is_pregnant", Unit;
    IsPanicking = "minecraft:is_panicking", Bool;
    UnreachableTongueTargets = "minecraft:unreachable_tongue_targets", Transient;
    VisitedBlockPositions = "minecraft:visited_block_positions", GlobalPosList;
    UnreachableTransportBlockPositions = "minecraft:unreachable_transport_block_positions", GlobalPosList;
    TransportItemsCooldownTicks = "minecraft:transport_items_cooldown_ticks", Transient;
    ChargeCooldownTicks = "minecraft:charge_cooldown_ticks", Int;
    AttackTargetCooldown = "minecraft:attack_target_cooldown", Int;
    SpearFleeingTime = "minecraft:spear_fleeing_time", Transient;
    SpearFleeingPosition = "minecraft:spear_fleeing_position", Transient;
    SpearChargePosition = "minecraft:spear_charge_position", Transient;
    SpearEngageTime = "minecraft:spear_engage_time", Transient;
    SpearStatus = "minecraft:spear_status", Transient;
    AngryAt = "minecraft:angry_at", Uuid;
    UniversalAnger = "minecraft:universal_anger", Bool;
    AdmiringItem = "minecraft:admiring_item", Bool;
    TimeTryingToReachAdmireItem = "minecraft:time_trying_to_reach_admire_item", Transient;
    DisableWalkToAdmireItem = "minecraft:disable_walk_to_admire_item", Transient;
    AdmiringDisabled = "minecraft:admiring_disabled", Bool;
    HuntedRecently = "minecraft:hunted_recently", Bool;
    CelebrateLocation = "minecraft:celebrate_location", Transient;
    Dancing = "minecraft:dancing", Transient;
    NearestVisibleHuntableHoglin = "minecraft:nearest_visible_huntable_hoglin", Transient;
    NearestVisibleBabyHoglin = "minecraft:nearest_visible_baby_hoglin", Transient;
    NearestTargetablePlayerNotWearingGold = "minecraft:nearest_targetable_player_not_wearing_gold", Transient;
    NearbyAdultPiglins = "minecraft:nearby_adult_piglins", Transient;
    NearestVisibleAdultPiglins = "minecraft:nearest_visible_adult_piglins", Transient;
    NearestVisibleAdultHoglins = "minecraft:nearest_visible_adult_hoglins", Transient;
    NearestVisibleAdultPiglin = "minecraft:nearest_visible_adult_piglin", Transient;
    NearestVisibleZombified = "minecraft:nearest_visible_zombified", Transient;
    VisibleAdultPiglinCount = "minecraft:visible_adult_piglin_count", Transient;
    VisibleAdultHoglinCount = "minecraft:visible_adult_hoglin_count", Transient;
    NearestPlayerHoldingWantedItem = "minecraft:nearest_player_holding_wanted_item", Transient;
    AteRecently = "minecraft:ate_recently", Transient;
    NearestRepellent = "minecraft:nearest_repellent", Transient;
    Pacified = "minecraft:pacified", Transient;
    RoarTarget = "minecraft:roar_target", Transient;
    DisturbanceLocation = "minecraft:disturbance_location", Transient;
    RecentProjectile = "minecraft:recent_projectile", Unit;
    IsSniffing = "minecraft:is_sniffing", Unit;
    IsEmerging = "minecraft:is_emerging", Unit;
    RoarSoundDelay = "minecraft:roar_sound_delay", Unit;
    DigCooldown = "minecraft:dig_cooldown", Unit;
    RoarSoundCooldown = "minecraft:roar_sound_cooldown", Unit;
    SniffCooldown = "minecraft:sniff_cooldown", Unit;
    TouchCooldown = "minecraft:touch_cooldown", Unit;
    VibrationCooldown = "minecraft:vibration_cooldown", Unit;
    SonicBoomCooldown = "minecraft:sonic_boom_cooldown", Unit;
    SonicBoomSoundCooldown = "minecraft:sonic_boom_sound_cooldown", Unit;
    SonicBoomSoundDelay = "minecraft:sonic_boom_sound_delay", Unit;
    LikedPlayer = "minecraft:liked_player", Uuid;
    LikedNoteblockPosition = "minecraft:liked_noteblock", GlobalPos;
    LikedNoteblockCooldownTicks = "minecraft:liked_noteblock_cooldown_ticks", Int;
    ItemPickupCooldownTicks = "minecraft:item_pickup_cooldown_ticks", Int;
    SnifferExploredPositions = "minecraft:sniffer_explored_positions", GlobalPosList;
    SnifferSniffingTarget = "minecraft:sniffer_sniffing_target", Transient;
    SnifferDigging = "minecraft:sniffer_digging", Transient;
    SnifferHappy = "minecraft:sniffer_happy", Transient;
    BreezeJumpCooldown = "minecraft:breeze_jump_cooldown", Unit;
    BreezeShoot = "minecraft:breeze_shoot", Unit;
    BreezeShootCharging = "minecraft:breeze_shoot_charging", Unit;
    BreezeShootRecovering = "minecraft:breeze_shoot_recover", Unit;
    BreezeShootCooldown = "minecraft:breeze_shoot_cooldown", Unit;
    BreezeJumpInhaling = "minecraft:breeze_jump_inhaling", Unit;
    BreezeJumpTarget = "minecraft:breeze_jump_target", Block;
    BreezeLeavingWater = "minecraft:breeze_leaving_water", Unit;
}

impl MemoryModuleType {
    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.iter().copied().find(|ty| ty.name() == name)
    }

    pub const fn can_serialize(self) -> bool {
        !matches!(self.codec(), Codec::Transient)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemoryStatus {
    ValuePresent,
    ValueAbsent,
    Registered,
}

pub type Condition = (MemoryModuleType, MemoryStatus);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GlobalPos {
    pub dimension: String,
    pub pos: BlockPos,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WalkTarget {
    pub target: PositionTracker,
    pub speed_modifier: f32,
    pub close_enough_dist: i32,
}

impl WalkTarget {
    pub fn block(pos: BlockPos, speed_modifier: f32, close_enough_dist: i32) -> Self {
        Self {
            target: PositionTracker::of_block(pos),
            speed_modifier,
            close_enough_dist,
        }
    }

    pub fn position(
        position: rspigot_domain::Vec3,
        speed_modifier: f32,
        close_enough_dist: i32,
    ) -> Self {
        Self {
            target: PositionTracker::of_block(position.block()),
            speed_modifier,
            close_enough_dist,
        }
    }

    pub fn entity(target: EntityId, speed_modifier: f32, close_enough_dist: i32) -> Self {
        Self {
            target: PositionTracker::of_entity(target, false),
            speed_modifier,
            close_enough_dist,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum MemoryValue {
    Unit,
    Bool(bool),
    Int(i32),
    Long(i64),
    GlobalPos(GlobalPos),
    GlobalPosList(Vec<GlobalPos>),
    Block(BlockPos),
    Position(Vec3),
    Uuid(Uuid),
    Entity(EntityId),
    Entities(Vec<EntityId>),
    Visible(NearestVisibleLivingEntities),
    WalkTarget(WalkTarget),
    Tracker(PositionTracker),
    HurtBy(LastDamage),
    Path(Path),
}

impl MemoryValue {
    fn is_empty_collection(&self) -> bool {
        match self {
            Self::Entities(entities) => entities.is_empty(),
            Self::GlobalPosList(positions) => positions.is_empty(),
            _ => false,
        }
    }

    pub fn as_entity(&self) -> Option<EntityId> {
        match self {
            Self::Entity(id) => Some(*id),
            _ => None,
        }
    }

    pub fn as_entities(&self) -> Option<&[EntityId]> {
        match self {
            Self::Entities(entities) => Some(entities),
            _ => None,
        }
    }

    pub fn as_int(&self) -> Option<i32> {
        match self {
            Self::Int(value) => Some(*value),
            _ => None,
        }
    }

    pub fn as_long(&self) -> Option<i64> {
        match self {
            Self::Long(value) => Some(*value),
            _ => None,
        }
    }

    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Self::Bool(value) => Some(*value),
            _ => None,
        }
    }

    pub fn as_visible(&self) -> Option<&NearestVisibleLivingEntities> {
        match self {
            Self::Visible(visible) => Some(visible),
            _ => None,
        }
    }

    pub fn as_hurt_by(&self) -> Option<LastDamage> {
        match self {
            Self::HurtBy(damage) => Some(*damage),
            _ => None,
        }
    }

    pub fn as_walk_target(&self) -> Option<&WalkTarget> {
        match self {
            Self::WalkTarget(target) => Some(target),
            _ => None,
        }
    }

    pub fn as_tracker(&self) -> Option<&PositionTracker> {
        match self {
            Self::Tracker(tracker) => Some(tracker),
            _ => None,
        }
    }

    pub fn as_uuid(&self) -> Option<Uuid> {
        match self {
            Self::Uuid(uuid) => Some(*uuid),
            _ => None,
        }
    }

    pub fn as_position(&self) -> Option<Vec3> {
        match self {
            Self::Position(position) => Some(*position),
            _ => None,
        }
    }

    pub fn as_block(&self) -> Option<BlockPos> {
        match self {
            Self::Block(pos) => Some(*pos),
            _ => None,
        }
    }

    pub fn as_path(&self) -> Option<&Path> {
        match self {
            Self::Path(path) => Some(path),
            _ => None,
        }
    }

    pub fn as_global_pos(&self) -> Option<&GlobalPos> {
        match self {
            Self::GlobalPos(pos) => Some(pos),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ExpirableValue {
    pub value: MemoryValue,
    pub time_to_live: Option<i64>,
}

impl ExpirableValue {
    pub fn of(value: MemoryValue) -> Self {
        Self {
            value,
            time_to_live: None,
        }
    }

    pub fn of_expiring(value: MemoryValue, ticks_until_expiry: i64) -> Self {
        Self {
            value,
            time_to_live: Some(ticks_until_expiry),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct MemorySlot {
    value: Option<MemoryValue>,
    time_to_live: i64,
}

impl MemorySlot {
    pub fn create() -> Self {
        Self {
            value: None,
            time_to_live: NO_EXPIRY,
        }
    }

    pub fn tick(&mut self) {
        if self.has_value() && self.can_expire() {
            if self.has_expired() {
                self.clear();
            } else {
                self.time_to_live -= 1;
            }
        }
    }

    pub fn set(&mut self, value: MemoryValue, time_to_live: i64) {
        self.value = Some(value);
        self.time_to_live = time_to_live;
    }

    pub fn clear(&mut self) {
        self.value = None;
        self.time_to_live = NO_EXPIRY;
    }

    pub fn has_value(&self) -> bool {
        self.value.is_some()
    }

    pub fn value(&self) -> Option<&MemoryValue> {
        self.value.as_ref()
    }

    pub fn can_expire(&self) -> bool {
        self.time_to_live != NO_EXPIRY
    }

    pub fn has_expired(&self) -> bool {
        self.time_to_live < 1
    }

    pub fn time_to_live(&self) -> i64 {
        self.time_to_live
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Memories {
    slots: BTreeMap<MemoryModuleType, MemorySlot>,
}

impl Memories {
    pub fn register(&mut self, ty: MemoryModuleType) {
        self.slots.entry(ty).or_insert_with(MemorySlot::create);
    }

    pub fn is_registered(&self, ty: MemoryModuleType) -> bool {
        self.slots.contains_key(&ty)
    }

    pub fn is_empty(&self) -> bool {
        self.slots.is_empty()
    }

    pub fn set(&mut self, ty: MemoryModuleType, value: MemoryValue) {
        self.set_option(ty, Some(value));
    }

    pub fn set_option(&mut self, ty: MemoryModuleType, value: Option<MemoryValue>) {
        let Some(slot) = self.slots.get_mut(&ty) else {
            return;
        };
        match value.filter(|value| !value.is_empty_collection()) {
            Some(value) => slot.set(value, NO_EXPIRY),
            None => slot.clear(),
        }
    }

    pub fn set_with_expiry(&mut self, ty: MemoryModuleType, value: MemoryValue, time_to_live: i64) {
        let Some(slot) = self.slots.get_mut(&ty) else {
            return;
        };
        if value.is_empty_collection() {
            slot.clear();
        } else {
            slot.set(value, time_to_live);
        }
    }

    pub fn set_expirable(&mut self, ty: MemoryModuleType, value: ExpirableValue) {
        match value.time_to_live {
            Some(time_to_live) => self.set_with_expiry(ty, value.value, time_to_live),
            None => self.set(ty, value.value),
        }
    }

    pub fn erase(&mut self, ty: MemoryModuleType) {
        if let Some(slot) = self.slots.get_mut(&ty) {
            slot.clear();
        }
    }

    pub fn get(&self, ty: MemoryModuleType) -> Option<&MemoryValue> {
        self.slots.get(&ty)?.value()
    }

    pub fn time_until_expiry(&self, ty: MemoryModuleType) -> Option<i64> {
        self.slots.get(&ty).map(MemorySlot::time_to_live)
    }

    pub fn has_value(&self, ty: MemoryModuleType) -> bool {
        self.check(ty, MemoryStatus::ValuePresent)
    }

    pub fn is_value(&self, ty: MemoryModuleType, value: &MemoryValue) -> bool {
        self.get(ty).is_some_and(|stored| stored == value)
    }

    pub fn check(&self, ty: MemoryModuleType, status: MemoryStatus) -> bool {
        let Some(slot) = self.slots.get(&ty) else {
            return false;
        };
        match status {
            MemoryStatus::Registered => true,
            MemoryStatus::ValuePresent => slot.has_value(),
            MemoryStatus::ValueAbsent => !slot.has_value(),
        }
    }

    pub fn clear_all(&mut self) {
        self.slots.values_mut().for_each(MemorySlot::clear);
    }

    pub fn forget_outdated(&mut self) {
        self.slots.values_mut().for_each(MemorySlot::tick);
    }

    pub fn pack(&self) -> PackedMemories {
        let entries = self
            .slots
            .iter()
            .filter(|(ty, _)| ty.can_serialize())
            .filter_map(|(ty, slot)| {
                let value = slot.value()?.clone();
                let expirable = if slot.can_expire() {
                    ExpirableValue::of_expiring(value, slot.time_to_live())
                } else {
                    ExpirableValue::of(value)
                };
                Some((*ty, expirable))
            })
            .collect();
        PackedMemories { entries }
    }

    pub fn load(&mut self, packed: &PackedMemories) {
        for (ty, value) in &packed.entries {
            self.set_expirable(*ty, value.clone());
        }
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct NearestVisibleLivingEntities {
    nearby: Vec<EntityId>,
    line_of_sight: RefCell<HashMap<EntityId, bool>>,
}

impl NearestVisibleLivingEntities {
    pub fn new(nearby: Vec<EntityId>) -> Self {
        let capacity = nearby.len();
        Self {
            nearby,
            line_of_sight: RefCell::new(HashMap::with_capacity(capacity)),
        }
    }

    pub fn nearby_entities(&self) -> &[EntityId] {
        &self.nearby
    }

    fn visible<W: AiWorld>(
        &self,
        cx: &mut AiContext<'_, W>,
        memories: &Memories,
        target: &Living,
    ) -> bool {
        if let Some(cached) = self.line_of_sight.borrow().get(&target.id) {
            return *cached;
        }
        let visible = is_entity_targetable(cx, memories, target);
        self.line_of_sight.borrow_mut().insert(target.id, visible);
        visible
    }

    pub fn find_closest<W: AiWorld>(
        &self,
        cx: &mut AiContext<'_, W>,
        memories: &Memories,
        filter: impl Fn(&Living) -> bool,
    ) -> Option<Living> {
        for id in &self.nearby {
            let Some(living) = cx.world.living(*id) else {
                continue;
            };
            if filter(&living) && self.visible(cx, memories, &living) {
                return Some(living);
            }
        }
        None
    }

    pub fn find_all<W: AiWorld>(
        &self,
        cx: &mut AiContext<'_, W>,
        memories: &Memories,
        filter: impl Fn(&Living) -> bool,
    ) -> Vec<Living> {
        let mut found = Vec::new();
        for id in &self.nearby {
            let Some(living) = cx.world.living(*id) else {
                continue;
            };
            if filter(&living) && self.visible(cx, memories, &living) {
                found.push(living);
            }
        }
        found
    }

    pub fn contains_entity<W: AiWorld>(
        &self,
        cx: &mut AiContext<'_, W>,
        memories: &Memories,
        target: EntityId,
    ) -> bool {
        if !self.nearby.contains(&target) {
            return false;
        }
        let Some(living) = cx.world.living(target) else {
            return false;
        };
        self.visible(cx, memories, &living)
    }

    pub fn contains<W: AiWorld>(
        &self,
        cx: &mut AiContext<'_, W>,
        memories: &Memories,
        filter: impl Fn(&Living) -> bool,
    ) -> bool {
        self.find_closest(cx, memories, filter).is_some()
    }
}

#[cfg(test)]
mod walk_target_tests {
    use rspigot_domain::{BlockPos, Vec3};

    use super::WalkTarget;
    use crate::mock::Mock;

    #[test]
    fn a_walk_target_built_from_a_vector_centres_on_the_containing_block() {
        let world = Mock::default();
        let target = WalkTarget::position(Vec3::new(1.3, 64.9, -2.7), 1.0, 2);
        assert_eq!(
            target.target.current_position(&world),
            Some(Vec3::new(1.5, 64.5, -2.5))
        );
        assert_eq!(
            target.target.current_block_position(&world),
            Some(BlockPos::new(1, 64, -3))
        );
        assert_eq!(target.close_enough_dist, 2);
    }

    #[test]
    fn a_position_tracker_built_from_a_vector_keeps_the_exact_vector() {
        let world = Mock::default();
        let tracker = crate::behavior::PositionTracker::of_position(Vec3::new(1.3, 64.9, -2.7));
        assert_eq!(
            tracker.current_position(&world),
            Some(Vec3::new(1.3, 64.9, -2.7))
        );
        assert_eq!(
            tracker.current_block_position(&world),
            Some(BlockPos::new(1, 64, -3))
        );
    }
}

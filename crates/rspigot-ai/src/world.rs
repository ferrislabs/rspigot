use rspigot_blocks::Face;
use rspigot_domain::damage::DamageType;
use rspigot_domain::entity::fluid::FluidContact;
use rspigot_domain::entity::living::{self, LivingState};
use rspigot_domain::entity::tamable;
use rspigot_domain::entity::{Aabb, EntityBase, Pose};
use rspigot_domain::item::{ItemStack, Slot};
use rspigot_domain::poi::{Occupancy, PoiRecord};
use rspigot_domain::{BlockPos, ChunkPos, EntityId, ItemId, Vec3};
use rspigot_entities::EntityKind;
use rspigot_items::{EquipmentSlot, ItemKind};
use uuid::Uuid;

use crate::BlockView;

pub const MOVEMENT_SPEED: &str = "minecraft:movement_speed";
pub const TEMPT_RANGE: &str = "minecraft:tempt_range";
const WOLF_SITTING_MAX_HEAD_X_ROT: f32 = 20.0;
const CROSSBOW_RANGE: i32 = 8;
const BOW_RANGE: i32 = 15;

fn is_kinetic_weapon(kind: ItemKind) -> bool {
    matches!(
        kind,
        ItemKind::WoodenSpear
            | ItemKind::StoneSpear
            | ItemKind::CopperSpear
            | ItemKind::IronSpear
            | ItemKind::GoldenSpear
            | ItemKind::DiamondSpear
            | ItemKind::NetheriteSpear
    )
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Facts {
    pub uuid: Uuid,
    pub tamed: bool,
    pub owner: Option<EntityId>,
    pub sleeping: bool,
    pub sleep_timer: i32,
    pub cat_relaxing: bool,
    pub last_hurt_by_mob: Option<EntityId>,
    pub last_hurt_by_mob_at: i32,
    pub last_hurt_mob: Option<EntityId>,
    pub last_hurt_mob_at: i32,
    pub retaliable: bool,
    pub tick_count: i32,
    pub sitting: bool,
    pub llama_strength: i32,
    pub gaze_disguise: bool,
    pub piglin_safe_armor: bool,
    pub cannot_be_hunted: bool,
    pub sees_player_holding_loved_item: bool,
    pub hunted_recently: bool,
    pub sprinting: bool,
    pub passenger: bool,
    pub hurt_by: bool,
    pub head_yaw: f32,
    pub villager: crate::villager::VillagerPeerFacts,
    pub hero_of_the_village: bool,
    pub cube_size: u8,
    pub sniffer_state: u8,
    pub school_leader: Option<EntityId>,
    pub school_size: i32,
    pub swimming: bool,
    pub wool_color: u8,
    pub can_parent: bool,
    pub caravan: crate::llama::CaravanFacts,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DroppedItem {
    pub id: EntityId,
    pub position: Vec3,
    pub item: ItemId,
    pub bounds: Aabb,
    pub has_pickup_delay: bool,
    pub alive: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ContainerView {
    pub slots: Vec<Slot>,
    pub compound: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LastDamage {
    pub kind: DamageType,
    pub entity: Option<EntityId>,
}

pub const NO_VILLAGE: i32 = rspigot_domain::poi::NO_VILLAGE;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Living {
    pub id: EntityId,
    pub player: bool,
    pub kind: EntityKind,
    pub position: Vec3,
    pub eye_height: f64,
    pub bounds: Aabb,
    pub alive: bool,
    pub spectator: bool,
    pub creative: bool,
    pub invulnerable: bool,
    pub discrete: bool,
    pub invisible: bool,
    pub armor_cover: f32,
    pub main_hand: Option<ItemId>,
    pub off_hand: Option<ItemId>,
    pub yaw: f32,
    pub pitch: f32,
    pub in_water: bool,
    pub baby: bool,
    pub age: i32,
    pub in_love: bool,
    pub panicking: bool,
    pub has_target: bool,
    pub scared: bool,
    pub facts: Facts,
}

impl Living {
    pub fn eye_y(&self) -> f64 {
        self.position.y + self.eye_height
    }

    pub fn can_be_seen_by_anyone(&self) -> bool {
        !self.spectator && self.alive
    }

    pub fn can_be_seen_as_enemy(&self) -> bool {
        !self.invulnerable && self.can_be_seen_by_anyone()
    }

    pub fn visibility_percent(&self) -> f64 {
        let mut visibility = 1.0;
        if self.discrete {
            visibility *= 0.8;
        }
        if self.invisible {
            let cover = self.armor_cover.max(0.1);
            visibility *= 0.7 * f64::from(cover);
        }
        visibility.clamp(0.0, 10.0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TargetKind {
    Player,
    AbstractVillager,
    IronGolem,
    Turtle,
    Class(&'static str),
}

impl TargetKind {
    pub fn matches(self, kind: EntityKind) -> bool {
        match self {
            Self::Player => kind == EntityKind::Player,
            Self::AbstractVillager => kind.extends("AbstractVillager"),
            Self::IronGolem => kind == EntityKind::IronGolem,
            Self::Turtle => kind == EntityKind::Turtle,
            Self::Class(name) => kind.extends(name),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Hand {
    Main,
    Off,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MobBody {
    pub velocity: Vec3,
    pub yaw: f32,
    pub body_yaw: f32,
    pub head_yaw: f32,
    pub pitch: f32,
    pub aggressive: bool,
    pub swell_dir: i32,
    pub using_item: Option<Hand>,
    pub fall_distance: f64,
    pub pose: Pose,
    pub discard_friction: bool,
    pub armadillo: crate::ArmadilloFacts,
    pub sniffer: crate::SnifferFacts,
    pub vex: crate::VexFacts,
    pub spell: crate::SpellFacts,
    pub phantom: crate::PhantomFacts,
    pub bee: crate::BeeFacts,
    pub fish: crate::FishFacts,
    pub squid: crate::SquidFacts,
    pub dolphin: crate::DolphinFacts,
    pub turtle: crate::TurtleFacts,
    pub guardian: crate::GuardianFacts,
    pub drowned: crate::DrownedFacts,
}

impl MobBody {
    pub fn of(mob: &MobView) -> Self {
        Self {
            velocity: mob.velocity,
            yaw: mob.yaw,
            body_yaw: mob.body_yaw,
            head_yaw: mob.head_yaw,
            pitch: mob.pitch,
            aggressive: mob.aggressive,
            swell_dir: mob.swell_dir,
            using_item: mob.using_item,
            fall_distance: mob.fall_distance,
            pose: mob.pose,
            discard_friction: mob.discard_friction,
            armadillo: mob.species.armadillo,
            sniffer: mob.species.sniffer,
            vex: mob.species.vex,
            spell: mob.species.spell,
            phantom: mob.species.phantom,
            bee: mob.species.bee,
            fish: mob.species.fish,
            squid: mob.species.squid,
            dolphin: mob.species.dolphin,
            turtle: mob.species.turtle,
            guardian: mob.species.guardian,
            drowned: mob.species.drowned,
        }
    }
}

pub trait AiWorld: BlockView {
    fn players(&self) -> &[Living];

    fn living(&self, id: EntityId) -> Option<Living>;

    fn living_of(&self, kind: TargetKind, area: &Aabb) -> Vec<Living>;

    fn difficulty(&self) -> i32;

    fn game_time(&self) -> i64;

    fn sea_level(&self) -> i32 {
        63
    }

    fn clock_time(&self) -> i64 {
        self.game_time()
    }

    fn raw_brightness(&self, pos: BlockPos) -> i32;

    fn ambient_light(&self) -> f32 {
        0.0
    }

    fn bright_outside(&self) -> bool {
        false
    }

    fn dark_outside(&self) -> bool {
        false
    }

    fn mob_griefing(&self) -> bool {
        true
    }

    fn raw_brightness_undarkened(&self, pos: BlockPos) -> i32 {
        self.raw_brightness(pos)
    }

    fn any_entity_in(&self, _area: &Aabb, _except: EntityId) -> bool {
        false
    }

    fn universal_anger(&self) -> bool {
        false
    }

    fn can_harm_player(&self, _attacker: EntityId, _victim: EntityId) -> bool {
        true
    }

    fn chest_open_count(&self, _pos: BlockPos) -> i32 {
        0
    }

    fn chests_in_chunk(&self, _chunk: ChunkPos) -> Option<Vec<BlockPos>> {
        None
    }

    fn chest_container(&self, _pos: BlockPos, _ignore_blocked: bool) -> Option<ContainerView> {
        None
    }

    fn container_locked(&self, _pos: BlockPos) -> bool {
        false
    }

    fn container_openers(&self, _pos: BlockPos) -> Vec<EntityId> {
        Vec::new()
    }

    fn main_hand_stack(&self, _id: EntityId) -> Option<ItemStack> {
        None
    }

    fn peaceful(&self) -> bool {
        self.difficulty() == 0
    }

    fn thundering(&self) -> bool {
        false
    }

    fn items_in(&self, _area: &Aabb) -> Vec<DroppedItem> {
        Vec::new()
    }

    fn is_ominous_banner(&self, _id: EntityId) -> bool {
        false
    }

    fn dropped_item(&self, _id: EntityId) -> Option<DroppedItem> {
        None
    }

    fn within_world_border(&self, _pos: BlockPos) -> bool {
        true
    }

    fn dimension(&self) -> &str {
        "minecraft:overworld"
    }

    fn living_by_uuid(&self, _uuid: Uuid) -> Option<Living> {
        None
    }

    fn forgive_dead_players(&self) -> bool {
        true
    }

    fn removal_keeps_anger(&self, _id: EntityId, _uuid: Uuid, player: bool) -> bool {
        player
    }

    fn sections_to_village(&self, _pos: BlockPos) -> i32 {
        NO_VILLAGE
    }

    fn pois_in_range(
        &self,
        _center: BlockPos,
        _radius: i32,
        _occupancy: Occupancy,
    ) -> Vec<PoiRecord> {
        Vec::new()
    }

    fn poi_type_at(&self, _pos: BlockPos) -> Option<rspigot_domain::poi::PoiType> {
        None
    }

    fn raid_at(&self, _pos: BlockPos) -> Option<crate::villager::RaidFacts> {
        None
    }

    fn path_door_nodes(&self, _mob: EntityId) -> Option<(BlockPos, BlockPos)> {
        None
    }

    fn villager_can_reach(&self, _villager: EntityId, _pos: BlockPos, _reach: i32) -> bool {
        true
    }

    fn villager_trades(&self, _villager: EntityId) -> Vec<crate::villager::TradeView> {
        Vec::new()
    }

    fn villagers_wanting_golem(&self, _area: &Aabb) -> Vec<Living> {
        Vec::new()
    }

    fn villager_reputation(&self, _villager: EntityId, _player: EntityId) -> i32 {
        0
    }

    fn is_close_to_village(&self, pos: BlockPos, sections: i32) -> bool {
        sections <= 6 && self.sections_to_village(pos) <= sections
    }

    fn is_village(&self, pos: BlockPos) -> bool {
        self.is_close_to_village(pos, 1)
    }

    fn motion_blocking_no_leaves_pos(&self, pos: BlockPos) -> BlockPos {
        let blocks = |state: rspigot_blocks::StateId| {
            !state.collision_shape().is_empty()
                || state.flags().contains(rspigot_blocks::StateFlags::WATER)
        };
        let mut y = self.max_y();
        while y >= self.min_y() {
            let at = BlockPos::new(pos.x, y, pos.z);
            let leaves = self.state(at).is_some_and(|state| {
                state
                    .kind()
                    .is_some_and(|kind| crate::animal::kind_in_block_tag(kind, "minecraft:leaves"))
            });
            if !leaves && self.state(at).is_some_and(blocks) {
                return BlockPos::new(pos.x, y + 1, pos.z);
            }
            y -= 1;
        }
        BlockPos::new(pos.x, self.min_y(), pos.z)
    }

    fn motion_blocking_height(&self, x: i32, z: i32) -> i32 {
        let blocks = |state: rspigot_blocks::StateId| {
            !state.collision_shape().is_empty()
                || state.flags().contains(rspigot_blocks::StateFlags::WATER)
        };
        let mut y = self.max_y();
        while y >= self.min_y() {
            if self.state(BlockPos::new(x, y, z)).is_some_and(blocks) {
                return y + 1;
            }
            y -= 1;
        }
        self.min_y()
    }

    fn aggressive_panda(&self, _id: EntityId) -> bool {
        false
    }

    fn beehive(&self, _pos: BlockPos) -> Option<crate::HiveView> {
        None
    }

    fn raining(&self) -> bool {
        false
    }

    fn nearest_dolphin_structure(&self, _pos: BlockPos, _radius: i32) -> Option<BlockPos> {
        None
    }

    fn bees_stay_in_hive(&self, _pos: BlockPos) -> bool {
        false
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ShulkerFacts {
    pub attach_face: Face,
    pub raw_peek: i32,
    pub can_stay: bool,
}

impl Default for ShulkerFacts {
    fn default() -> Self {
        Self {
            attach_face: Face::Down,
            raw_peek: 0,
            can_stay: true,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct KindFacts {
    pub piglin: crate::PiglinFacts,
    pub hoglin: crate::HoglinFacts,
    pub wolf: crate::WolfFacts,
    pub cat: crate::CatFacts,
    pub fox: crate::FoxFacts,
    pub panda: crate::PandaFacts,
    pub polar_bear: crate::PolarBearFacts,
    pub enderman: crate::EndermanFacts,
    pub silverfish: crate::SilverfishFacts,
    pub cube: crate::cube::CubeFacts,
    pub iron_golem: crate::IronGolemFacts,
    pub raider: crate::RaiderFacts,
    pub crossbow: crate::CrossbowFacts,
    pub shulker: ShulkerFacts,
    pub goat: crate::GoatFacts,
    pub armadillo: crate::ArmadilloFacts,
    pub sniffer: crate::SnifferFacts,
    pub breeze: crate::BreezeFacts,
    pub creaking: crate::CreakingFacts,
    pub vex: crate::VexFacts,
    pub spell: crate::SpellFacts,
    pub bat: crate::BatFacts,
    pub phantom: crate::PhantomFacts,
    pub bee: crate::BeeFacts,
    pub wither: crate::WitherFacts,
    pub allay: crate::AllayFacts,
    pub fish: crate::FishFacts,
    pub squid: crate::SquidFacts,
    pub dolphin: crate::DolphinFacts,
    pub turtle: crate::TurtleFacts,
    pub guardian: crate::GuardianFacts,
    pub drowned: crate::DrownedFacts,
    pub villager: crate::villager::VillagerFacts,
    pub trader: crate::villager::TraderFacts,
    pub equine: crate::EquineFacts,
    pub llama: crate::llama::LlamaFacts,
    pub camel: crate::camel::CamelFacts,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MobView {
    pub id: EntityId,
    pub kind: EntityKind,
    pub position: Vec3,
    pub old_position: Vec3,
    pub yaw: f32,
    pub head_yaw: f32,
    pub body_yaw: f32,
    pub silent: bool,
    pub on_ground: bool,
    pub fluid: FluidContact,
    pub width: f32,
    pub height: f32,
    pub eye_height: f32,
    pub tick_count: i32,
    pub no_action_time: i32,
    pub speed: f32,
    pub follow_range: f64,
    pub tempt_range: f64,
    pub step_height: f32,
    pub health: f32,
    pub max_health: f32,
    pub home_pos: BlockPos,
    pub home_radius: i32,
    pub pitch: f32,
    pub velocity: Vec3,
    pub fall_distance: f64,
    pub horizontal_collision: bool,
    pub age: i32,
    pub baby: bool,
    pub in_love: bool,
    pub last_hurt_by_mob: Option<EntityId>,
    pub last_hurt_by_mob_at: i32,
    pub last_damage: Option<LastDamage>,
    pub panic: bool,
    pub panic_environmental: bool,
    pub on_fire: bool,
    pub head_empty: bool,
    pub main_hand: Option<ItemId>,
    pub off_hand: Option<ItemId>,
    pub using_item: Option<Hand>,
    pub ticks_using_item: i32,
    pub aggressive: bool,
    pub swell_dir: i32,
    pub light: f32,
    pub swinging: bool,
    pub in_powder_snow: bool,
    pub was_in_powder_snow: bool,
    pub evil: bool,
    pub trusting: bool,
    pub more_carrot_ticks: i32,
    pub uuid: Uuid,
    pub tamed: bool,
    pub ordered_to_sit: bool,
    pub sitting: bool,
    pub owner: Option<EntityId>,
    pub angry: bool,
    pub anger_target: Option<EntityId>,
    pub leashed: bool,
    pub passenger: bool,
    pub vehicle: Option<EntityId>,
    pub player_created: bool,
    pub can_pick_up_loot: bool,
    pub team: bool,
    pub custom_box: Option<Aabb>,
    pub pose: Pose,
    pub discard_friction: bool,
    pub jump_strength: f64,
    pub gravity: f64,
    pub jump_boost_power: f32,
    pub speed_effect_level: i32,
    pub slowness_effect_level: i32,
    pub levitating: bool,
    pub air_supply: i32,
    pub species: KindFacts,
}

pub fn attribute(kind: EntityKind, living: &LivingState, name: &str) -> f64 {
    living.attributes.state(name, &living.effects).map_or_else(
        || kind.default_attribute(name).unwrap_or(0.0),
        |state| rspigot_domain::attribute::value(&state),
    )
}

const JUMP_BOOST_STEP: f32 = 0.1;
const SLOW_FALLING_GRAVITY: f64 = 0.01;

fn effect_level(living: &LivingState, name: &str) -> i32 {
    living
        .effect(name)
        .map_or(0, |effect| i32::from(effect.amplifier) + 1)
}

fn gravity(base: &EntityBase, living: &LivingState) -> f64 {
    if base.is_no_gravity() {
        return 0.0;
    }
    let gravity = attribute(base.kind, living, living::GRAVITY);
    if base.velocity.y > 0.0 || living.effect("minecraft:slow_falling").is_none() {
        gravity
    } else {
        gravity.min(SLOW_FALLING_GRAVITY)
    }
}

impl MobView {
    pub fn of(base: &EntityBase, living: &LivingState) -> Self {
        let dimensions = base.dimensions;
        let tamable_state = living.mob.as_ref().and_then(|mob| mob.tamable);
        let tame = tamable_state.is_some();
        let ordered_to_sit = tamable_state.is_some_and(|state| state.ordered_to_sit);
        let (home_pos, home_radius) = living
            .mob
            .as_ref()
            .map_or((BlockPos::new(0, 0, 0), -1), |mob| {
                (mob.home_pos, mob.home_radius)
            });
        Self {
            id: base.id,
            kind: base.kind,
            position: base.position,
            old_position: base.old_position,
            yaw: base.yaw,
            head_yaw: base.head_yaw,
            body_yaw: living.body_yaw,
            silent: base.is_silent(),
            on_ground: base.on_ground,
            fluid: base.fluid,
            width: dimensions.width,
            height: dimensions.height,
            eye_height: dimensions.eye_height,
            tick_count: base.tick_count,
            no_action_time: living.no_action_time,
            speed: living.speed,
            follow_range: attribute(base.kind, living, living::FOLLOW_RANGE),
            tempt_range: attribute(base.kind, living, TEMPT_RANGE),
            step_height: attribute(base.kind, living, living::STEP_HEIGHT) as f32,
            health: living::health(base),
            max_health: attribute(base.kind, living, living::MAX_HEALTH) as f32,
            home_pos,
            home_radius,
            pitch: base.pitch,
            velocity: base.velocity,
            fall_distance: base.fall_distance,
            horizontal_collision: base.horizontal_collision,
            age: 0,
            baby: false,
            in_love: false,
            last_hurt_by_mob: living.damage.last_hurt_by_mob,
            last_hurt_by_mob_at: living.damage.last_hurt_by_mob_at,
            last_damage: None,
            panic: false,
            panic_environmental: false,
            on_fire: false,
            head_empty: living.equipment(EquipmentSlot::Head).is_none(),
            main_hand: living.equipment(EquipmentSlot::Mainhand).map(|s| s.item),
            off_hand: living.equipment(EquipmentSlot::Offhand).map(|s| s.item),
            using_item: None,
            ticks_using_item: 0,
            aggressive: living::is_aggressive(base),
            swell_dir: 0,
            light: 0.0,
            swinging: false,
            in_powder_snow: base.in_powder_snow,
            was_in_powder_snow: base.was_in_powder_snow,
            evil: false,
            trusting: false,
            more_carrot_ticks: 0,
            uuid: base.uuid,
            tamed: tame && tamable::is_tame(base),
            ordered_to_sit,
            sitting: tame && tamable::is_in_sitting_pose(base),
            owner: None,
            angry: false,
            anger_target: None,
            leashed: false,
            passenger: false,
            vehicle: None,
            player_created: false,
            can_pick_up_loot: living.mob.as_ref().is_some_and(|mob| mob.can_pick_up_loot),
            team: false,
            custom_box: base.custom_box,
            pose: base.pose(),
            discard_friction: living.discard_friction,
            jump_strength: attribute(base.kind, living, living::JUMP_STRENGTH),
            gravity: gravity(base, living),
            jump_boost_power: living.effect("minecraft:jump_boost").map_or(0.0, |effect| {
                JUMP_BOOST_STEP * (effect.amplifier as f32 + 1.0)
            }),
            speed_effect_level: effect_level(living, "minecraft:speed"),
            slowness_effect_level: effect_level(living, "minecraft:slowness"),
            levitating: living.effect("minecraft:levitation").is_some(),
            air_supply: base.air_supply(),
            species: KindFacts::default(),
        }
    }

    pub fn max_head_x_rot(&self) -> f32 {
        if self.kind == EntityKind::Wolf && self.sitting {
            WOLF_SITTING_MAX_HEAD_X_ROT
        } else if self.kind == EntityKind::Dolphin {
            crate::swimmers::DOLPHIN_MAX_HEAD_ROT
        } else if self.kind.extends("Guardian") {
            rspigot_domain::entity::species::guardian::MAX_HEAD_X_ROT as f32
        } else if self.kind == EntityKind::Shulker {
            rspigot_domain::entity::species::shulker::MAX_HEAD_ROTATION
        } else {
            crate::control::MAX_HEAD_X_ROT
        }
    }

    pub fn is_holding(&self, item: ItemId) -> bool {
        self.main_hand == Some(item) || self.off_hand == Some(item)
    }

    pub fn can_use_non_melee_weapon(&self, item: ItemId) -> bool {
        self.kind == EntityKind::Piglin
            && (item == ItemKind::Crossbow.id() || item.kind().is_some_and(is_kinetic_weapon))
    }

    pub fn projectile_range(&self, item: ItemId) -> Option<i32> {
        if !self.can_use_non_melee_weapon(item) {
            return None;
        }
        match item.kind() {
            Some(ItemKind::Crossbow) => Some(CROSSBOW_RANGE),
            Some(ItemKind::Bow) => Some(BOW_RANGE),
            _ => None,
        }
    }

    pub fn eye_y(&self) -> f64 {
        self.position.y + f64::from(self.eye_height)
    }

    pub fn eye_position(&self) -> Vec3 {
        Vec3::new(self.position.x, self.eye_y(), self.position.z)
    }

    pub fn block_pos(&self) -> BlockPos {
        self.position.block()
    }

    pub fn bounds(&self) -> Aabb {
        match self.custom_box {
            Some(relative) => relative.move_by(self.position),
            None => Aabb::of_size(self.position, self.width, self.height),
        }
    }

    pub fn distance_to_sqr(&self, point: Vec3) -> f64 {
        self.position.distance_squared(point)
    }

    pub fn has_home(&self) -> bool {
        self.home_radius != -1
    }

    pub fn is_within_home(&self, pos: BlockPos) -> bool {
        if !self.has_home() {
            return true;
        }
        let home = self.home_pos;
        let dx = f64::from(home.x - pos.x);
        let dy = f64::from(home.y - pos.y);
        let dz = f64::from(home.z - pos.z);
        let radius = f64::from(self.home_radius.wrapping_mul(self.home_radius));
        dx * dx + dy * dy + dz * dz < radius
    }

    pub fn is_restricted(&self, range: f64) -> bool {
        if !self.has_home() {
            return false;
        }
        let home = self.home_pos;
        let dx = f64::from(home.x) + 0.5 - self.position.x;
        let dy = f64::from(home.y) + 0.5 - self.position.y;
        let dz = f64::from(home.z) + 0.5 - self.position.z;
        let reach = f64::from(self.home_radius) + range + 1.0;
        dx * dx + dy * dy + dz * dz < reach * reach
    }

    pub fn in_liquid(&self) -> bool {
        self.fluid.in_liquid()
    }
}

pub fn inflate(area: &Aabb, x: f64, y: f64, z: f64) -> Aabb {
    Aabb::new(
        area.min_x - x,
        area.min_y - y,
        area.min_z - z,
        area.max_x + x,
        area.max_y + y,
        area.max_z + z,
    )
}

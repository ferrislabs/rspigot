use rspigot_domain::entity::fluid::FluidContact;
use rspigot_domain::entity::living::{self, LivingState};
use rspigot_domain::entity::tamable;
use rspigot_domain::entity::{Aabb, EntityBase};
use rspigot_domain::{BlockPos, EntityId, ItemId, Vec3};
use rspigot_entities::EntityKind;
use rspigot_items::EquipmentSlot;
use uuid::Uuid;

use crate::BlockView;

pub const MOVEMENT_SPEED: &str = "minecraft:movement_speed";
pub const TEMPT_RANGE: &str = "minecraft:tempt_range";

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Facts {
    pub uuid: Uuid,
    pub tamed: bool,
    pub owner: Option<EntityId>,
    pub sleeping: bool,
    pub last_hurt_by_mob: Option<EntityId>,
    pub last_hurt_by_mob_at: i32,
    pub last_hurt_mob: Option<EntityId>,
    pub last_hurt_mob_at: i32,
    pub retaliable: bool,
}

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
    pub pitch: f32,
    pub aggressive: bool,
    pub swell_dir: i32,
    pub using_item: Option<Hand>,
    pub fall_distance: f64,
}

impl MobBody {
    pub fn of(mob: &MobView) -> Self {
        Self {
            velocity: mob.velocity,
            yaw: mob.yaw,
            pitch: mob.pitch,
            aggressive: mob.aggressive,
            swell_dir: mob.swell_dir,
            using_item: mob.using_item,
            fall_distance: mob.fall_distance,
        }
    }
}

pub trait AiWorld: BlockView {
    fn players(&self) -> &[Living];

    fn living(&self, id: EntityId) -> Option<Living>;

    fn living_of(&self, kind: TargetKind, area: &Aabb) -> Vec<Living>;

    fn difficulty(&self) -> i32;

    fn game_time(&self) -> i64;

    fn raw_brightness(&self, pos: BlockPos) -> i32;

    fn ambient_light(&self) -> f32 {
        0.0
    }

    fn bright_outside(&self) -> bool {
        false
    }

    fn mob_griefing(&self) -> bool {
        true
    }

    fn universal_anger(&self) -> bool {
        false
    }

    fn can_harm_player(&self, _attacker: EntityId, _victim: EntityId) -> bool {
        true
    }

    fn peaceful(&self) -> bool {
        self.difficulty() == 0
    }
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
    pub panic: bool,
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
    pub uuid: Uuid,
    pub tamed: bool,
    pub ordered_to_sit: bool,
    pub sitting: bool,
    pub owner: Option<EntityId>,
    pub angry: bool,
    pub anger_target: Option<EntityId>,
    pub leashed: bool,
    pub passenger: bool,
}

pub fn attribute(kind: EntityKind, living: &LivingState, name: &str) -> f64 {
    living.attributes.state(name, &living.effects).map_or_else(
        || kind.default_attribute(name).unwrap_or(0.0),
        |state| rspigot_domain::attribute::value(&state),
    )
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
            panic: false,
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
            uuid: base.uuid,
            tamed: tame && tamable::is_tame(base),
            ordered_to_sit,
            sitting: tame && tamable::is_in_sitting_pose(base),
            owner: None,
            angry: false,
            anger_target: None,
            leashed: false,
            passenger: false,
        }
    }

    pub fn is_holding(&self, item: ItemId) -> bool {
        self.main_hand == Some(item) || self.off_hand == Some(item)
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
        Aabb::of_size(self.position, self.width, self.height)
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

use rand::Rng;
use rspigot_blocks::{BlockKind, Face};
use rspigot_domain::entity::arrow::{self, ArrowState, Pickup};
use rspigot_domain::entity::firework::FireworkState;
use rspigot_domain::entity::hurting::{self, HurtingState};
use rspigot_domain::entity::thrown::{self, ThrownState};
use rspigot_domain::entity::{Aabb, Entity, EntityState, living};
use rspigot_domain::item::{ItemStack, Slot, with_count};
use rspigot_domain::{BlockPos, BlockStateId, EntityId, Vec3, mth};
use rspigot_entities::EntityKind as Kind;
use rspigot_items::{EquipmentSlot, ItemKind};

use super::hopper::Transfer;
use super::storage::{Holder, insert_stack};
use super::{facing, step};
use crate::items::PICKUP_DELAY;
use crate::mobs::ServerRandom;
use crate::plugins::events::{
    BlockDispense, BlockDispenseArmor, BlockShearEntity, CreatureSpawnReason, InventoryMoveItem,
    TNTPrimeCause,
};
use crate::plugins::steps::Birth;
use crate::plugins::{
    Continuation, EventKind, EventPayload, InventoryRef, Lane, spec_of, stack_of,
};
use crate::ports::ClientLink;
use crate::server::Server;
use crate::signal::flag;
use crate::tnt::TntPrimeStep;

const DISPENSE_DELAY: u64 = 4;
const SHULKER_BOXES: &str = "minecraft:shulker_boxes";
const CLICK_EVENT: i32 = 1000;
const FAIL_EVENT: i32 = 1001;
const LAUNCH_EVENT: i32 = 1002;
const SMOKE_EVENT: i32 = 2000;
const SHOT_POWER: f32 = 1.1;
const SHOT_UNCERTAINTY: f32 = 6.0;
const DROP_SPREAD: f64 = 0.0172275 * 6.0;
const SHOT_LIFT: f64 = 0.1;
const OFFSET: f64 = 0.7;
const FIREWORK_EVENT: i32 = 1004;
const FIRE_CHARGE_EVENT: i32 = 1018;
const WIND_CHARGE_EVENT: i32 = 1051;
const FIREWORK_LEAD: f64 = 0.500_009_999_999_747_4;
const HURTING_LEAD: f64 = 1.0;
const FIREWORK_POWER: f32 = 0.5;
const FIREWORK_UNCERTAINTY: f32 = 1.0;
const HURTING_UNCERTAINTY: f32 = 6.666_666_5;
const WAX_ON_EVENT: i32 = 3003;
const WAX_ON_SOUND: &str = "minecraft:item.honeycomb.wax_on";
const ANCHOR_CHARGE_SOUND: &str = "minecraft:block.respawn_anchor.charge";
const ANCHOR_FULL: i32 = 4;
const BRUSH_WEAR: i32 = 16;
const SKULL_SEGMENTS: i32 = 4;
const WAXED: &str = "minecraft:waxed_";
const FISH_EMPTY_SOUND: &str = "minecraft:item.bucket.empty_fish";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Shot {
    Arrow,
    Egg,
    Snowball,
    Bottle,
    Splash,
    Lingering,
    Firework,
    Fire,
    Wind,
}

impl Shot {
    const fn event(self) -> i32 {
        match self {
            Self::Firework => FIREWORK_EVENT,
            Self::Fire => FIRE_CHARGE_EVENT,
            Self::Wind => WIND_CHARGE_EVENT,
            _ => LAUNCH_EVENT,
        }
    }

    const fn lead(self) -> f64 {
        match self {
            Self::Firework => FIREWORK_LEAD,
            Self::Fire | Self::Wind => HURTING_LEAD,
            _ => OFFSET,
        }
    }

    const fn power(self) -> f32 {
        match self {
            Self::Firework => FIREWORK_POWER,
            _ => SHOT_POWER,
        }
    }

    const fn uncertainty(self) -> f32 {
        match self {
            Self::Firework => FIREWORK_UNCERTAINTY,
            Self::Fire | Self::Wind => HURTING_UNCERTAINTY,
            _ => SHOT_UNCERTAINTY,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Behavior {
    Drop,
    Equip(EquipmentSlot),
    Shoot(Shot),
    Pour { lava: bool },
    Scoop,
    Tnt,
    BoneMeal,
    Shear,
    Ignite,
    Mud,
    Bottle,
    PowderSnow,
    Cart(Kind),
    Boat(Kind),
    Stand,
    Egg(Kind),
    Wax,
    Charge,
    Box,
    Skull,
    Pumpkin,
    Brush,
    Chest,
    Release(Kind),
    ShearHive,
}

pub(crate) fn shot_of(stack: &ItemStack) -> Option<Shot> {
    match behavior_of(stack) {
        Behavior::Shoot(shot) => Some(shot),
        _ => None,
    }
}

fn behavior_of(stack: &ItemStack) -> Behavior {
    let Some(kind) = stack.item.kind() else {
        return Behavior::Drop;
    };
    match kind {
        ItemKind::Arrow | ItemKind::SpectralArrow | ItemKind::TippedArrow => {
            Behavior::Shoot(Shot::Arrow)
        }
        ItemKind::Egg | ItemKind::BlueEgg | ItemKind::BrownEgg => Behavior::Shoot(Shot::Egg),
        ItemKind::FireworkRocket => Behavior::Shoot(Shot::Firework),
        ItemKind::FireCharge => Behavior::Shoot(Shot::Fire),
        ItemKind::WindCharge => Behavior::Shoot(Shot::Wind),
        ItemKind::Honeycomb => Behavior::Wax,
        ItemKind::Glowstone => Behavior::Charge,
        ItemKind::Brush => Behavior::Brush,
        ItemKind::Chest => Behavior::Chest,
        ItemKind::WitherSkeletonSkull => Behavior::Skull,
        ItemKind::CarvedPumpkin => Behavior::Pumpkin,
        ItemKind::SalmonBucket
        | ItemKind::CodBucket
        | ItemKind::PufferfishBucket
        | ItemKind::TropicalFishBucket
        | ItemKind::AxolotlBucket
        | ItemKind::TadpoleBucket => {
            crate::bucket::mob_of_bucket(stack.item).map_or(Behavior::Drop, Behavior::Release)
        }
        _ if is_shulker_box(stack) => Behavior::Box,
        ItemKind::Snowball => Behavior::Shoot(Shot::Snowball),
        ItemKind::ExperienceBottle => Behavior::Shoot(Shot::Bottle),
        ItemKind::SplashPotion => Behavior::Shoot(Shot::Splash),
        ItemKind::LingeringPotion => Behavior::Shoot(Shot::Lingering),
        ItemKind::WaterBucket => Behavior::Pour { lava: false },
        ItemKind::LavaBucket => Behavior::Pour { lava: true },
        ItemKind::Bucket => Behavior::Scoop,
        ItemKind::Tnt => Behavior::Tnt,
        ItemKind::BoneMeal => Behavior::BoneMeal,
        ItemKind::Shears => Behavior::Shear,
        ItemKind::FlintAndSteel => Behavior::Ignite,
        ItemKind::Potion => Behavior::Mud,
        ItemKind::GlassBottle => Behavior::Bottle,
        ItemKind::PowderSnowBucket => Behavior::PowderSnow,
        ItemKind::ArmorStand => Behavior::Stand,
        _ if kind.class() == "MinecartItem" => {
            Kind::from_name(kind.name()).map_or(Behavior::Drop, Behavior::Cart)
        }
        _ if kind.class() == "BoatItem" => {
            Kind::from_name(kind.name()).map_or(Behavior::Drop, Behavior::Boat)
        }
        _ if Kind::from_spawn_egg(stack.item.0).is_some() => {
            Kind::from_spawn_egg(stack.item.0).map_or(Behavior::Drop, Behavior::Egg)
        }
        _ => equip_or_drop(kind),
    }
}

fn is_shulker_box(stack: &ItemStack) -> bool {
    rspigot_data::tags::tag("minecraft:item", SHULKER_BOXES)
        .is_some_and(|ids| ids.binary_search(&u32::from(stack.item.0)).is_ok())
}

fn wears(entity: &Entity, stack: &ItemStack, slot: EquipmentSlot) -> bool {
    match slot {
        EquipmentSlot::Saddle => crate::kinds::mount_accepts(entity, stack, slot),
        EquipmentSlot::Body => {
            crate::kinds::mount_accepts(entity, stack, slot)
                || (entity.base.kind == Kind::Wolf && stack.item == ItemKind::WolfArmor.id())
        }
        _ => true,
    }
}

fn equip_or_drop(kind: ItemKind) -> Behavior {
    match kind.equippable() {
        Some(
            slot @ (EquipmentSlot::Head
            | EquipmentSlot::Chest
            | EquipmentSlot::Legs
            | EquipmentSlot::Feet
            | EquipmentSlot::Saddle
            | EquipmentSlot::Body),
        ) => Behavior::Equip(slot),
        _ => Behavior::Drop,
    }
}

fn waxed_of(state: BlockStateId) -> Option<BlockStateId> {
    let name = state.kind()?.name();
    if name.starts_with(WAXED) {
        return None;
    }
    let waxed = BlockKind::from_name(&format!("{WAXED}{}", name.trim_start_matches("minecraft:")))?;
    let properties: Vec<(&str, &str)> = state.properties().collect();
    waxed.state_with(&properties)
}

fn lights_up(state: BlockStateId) -> bool {
    let has = |tag| rspigot_domain::entity::physics::has_tag(state, tag);
    let unlit = state.property("lit") == Some("false");
    unlit
        && (has("minecraft:candles")
            || has("minecraft:candle_cakes")
            || (has("minecraft:campfires") && state.property("waterlogged") != Some("true")))
}

fn skull_rotation(direction: Face) -> i32 {
    let turn = match direction.opposite() {
        Face::South => 0,
        Face::West => 1,
        Face::North => 2,
        Face::East => 3,
        _ => return 0,
    };
    turn * SKULL_SEGMENTS
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Move {
    Item(Box<Transfer>),
    None,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct DispenseStep {
    pub(crate) pos: BlockPos,
    pub(crate) slot: usize,
    pub(crate) stack: ItemStack,
    pub(crate) behavior: Behavior,
    pub(crate) velocity: Vec3,
    pub(crate) target: Option<EntityId>,
    pub(crate) transfer: Move,
}

impl DispenseStep {
    pub(crate) fn kind(&self) -> EventKind {
        if matches!(self.transfer, Move::Item(_)) {
            return EventKind::InventoryMoveItem;
        }
        match self.behavior {
            Behavior::Equip(_) | Behavior::Brush | Behavior::Chest => EventKind::BlockDispenseArmor,
            Behavior::Shear => EventKind::BlockShearEntity,
            _ => EventKind::BlockDispense,
        }
    }
}

const HONEY_FULL: i32 = 5;

fn yaw_of(face: Face) -> f32 {
    match face {
        Face::South => 0.0,
        Face::West => 90.0,
        Face::North => 180.0,
        Face::East => 270.0,
        _ => 0.0,
    }
}

fn has_water(state: BlockStateId) -> bool {
    state.flags().contains(rspigot_blocks::StateFlags::WATER)
}

fn stack_is_water(stack: &ItemStack) -> bool {
    rspigot_domain::potion::PotionContents::of(stack).is(rspigot_domain::potion::WATER)
}

fn vector_of(face: Face) -> Vec3 {
    let (dx, dy, dz) = face.offset();
    Vec3::new(f64::from(dx), f64::from(dy), f64::from(dz))
}

fn direction_code(face: Face) -> i32 {
    match face {
        Face::Down => 0,
        Face::Up => 1,
        Face::North => 2,
        Face::South => 3,
        Face::West => 4,
        Face::East => 5,
    }
}

fn dispense_position(pos: BlockPos, face: Face) -> Vec3 {
    let center = Vec3::new(
        f64::from(pos.x) + 0.5,
        f64::from(pos.y) + 0.5,
        f64::from(pos.z) + 0.5,
    );
    let toward = vector_of(face);
    Vec3::new(
        center.x + OFFSET * toward.x,
        center.y + OFFSET * toward.y,
        center.z + OFFSET * toward.z,
    )
}

impl<L: ClientLink> Server<L> {
    pub(crate) fn dispenser_neighbor_changed(&mut self, pos: BlockPos, state: BlockStateId) {
        let Some(kind) = state.kind() else {
            return;
        };
        let powered =
            self.has_neighbor_signal(pos) || self.has_neighbor_signal(step(pos, Face::Up));
        let triggered = flag(state, "triggered");
        if powered && !triggered {
            self.schedule(pos, kind, DISPENSE_DELAY);
            if let Some(next) = state.with_property("triggered", "true") {
                self.set_block_quiet(pos, next);
            }
        } else if !powered
            && triggered
            && let Some(next) = state.with_property("triggered", "false")
        {
            self.set_block_quiet(pos, next);
        }
    }

    pub(crate) fn dispenser_tick(&mut self, pos: BlockPos, state: BlockStateId) {
        let Some(me) = self.holder_at(pos) else {
            return;
        };
        let Some(direction) = facing(state) else {
            return;
        };
        let items = self.holder_items(&me);
        let mut chosen: Option<usize> = None;
        let mut seen = 1;
        for (slot, stack) in items.iter().enumerate() {
            if stack.is_some() {
                if self.rng.random_range(0..seen) == 0 {
                    chosen = Some(slot);
                }
                seen += 1;
            }
        }
        let Some((slot, stack)) = chosen.and_then(|slot| {
            items
                .get(slot)
                .and_then(Option::as_ref)
                .and_then(|stack| with_count(stack, 1))
                .map(|one| (slot, one))
        }) else {
            self.level_event(FAIL_EVENT, pos, 0);
            return;
        };
        let front = step(pos, direction);
        let dropper = state.kind() == Some(BlockKind::Dropper);
        if dropper && let Some(destination) = self.container_at(front) {
            let transfer = Transfer {
                source: me,
                destination,
                slot,
                stack: stack.clone(),
            };
            self.ask_dispense(DispenseStep {
                pos,
                slot,
                stack,
                behavior: Behavior::Drop,
                velocity: Vec3::ZERO,
                target: None,
                transfer: Move::Item(Box::new(transfer)),
            });
            return;
        }
        let behavior = if dropper {
            Behavior::Drop
        } else {
            behavior_of(&stack)
        };
        let Some((behavior, target, velocity)) =
            self.plan_dispense(pos, direction, behavior, &stack)
        else {
            self.level_event(FAIL_EVENT, pos, 0);
            return;
        };
        self.ask_dispense(DispenseStep {
            pos,
            slot,
            stack,
            behavior,
            velocity,
            target,
            transfer: Move::None,
        });
    }

    fn triangle(&mut self, mode: f64, spread: f64) -> f64 {
        mode + spread * (self.rng.random::<f64>() - self.rng.random::<f64>())
    }

    fn mobs_in_block(&self, pos: BlockPos) -> Vec<EntityId> {
        let area = Aabb::block(pos);
        let mut found: Vec<EntityId> = self
            .entities
            .store
            .iter()
            .filter(|entity| {
                entity.base.removal.is_none()
                    && entity
                        .living()
                        .is_some_and(|state| !living::is_dead_or_dying(&entity.base, state))
                    && entity.base.bounding_box().intersects(&area)
            })
            .map(|entity| entity.base.id)
            .collect();
        found.sort_by_key(|id| id.0);
        found
    }

    fn plan_dispense(
        &mut self,
        pos: BlockPos,
        direction: Face,
        behavior: Behavior,
        stack: &ItemStack,
    ) -> Option<(Behavior, Option<EntityId>, Vec3)> {
        let front = step(pos, direction);
        let toward = vector_of(direction);
        match behavior {
            Behavior::Drop => {
                let power = self.rng.random::<f64>() * 0.1 + 0.2;
                let velocity = Vec3::new(
                    self.triangle(toward.x * power, DROP_SPREAD),
                    self.triangle(0.2, DROP_SPREAD),
                    self.triangle(toward.z * power, DROP_SPREAD),
                );
                Some((Behavior::Drop, None, velocity))
            }
            Behavior::Shoot(_) => Some((
                behavior,
                None,
                Vec3::new(toward.x, toward.y + SHOT_LIFT, toward.z),
            )),
            Behavior::Equip(slot) => {
                let target = self.mobs_in_block(front).into_iter().find(|id| {
                    self.entities.store.get(*id).is_some_and(|entity| {
                        entity.living().is_some_and(|state| {
                            state.mob.is_some() && state.equipment(slot).is_none()
                        }) && wears(entity, stack, slot)
                    })
                });
                match target {
                    Some(target) => Some((behavior, Some(target), Vec3::ZERO)),
                    None => self.plan_dispense(pos, direction, Behavior::Drop, stack),
                }
            }
            Behavior::Pour { lava } => {
                if self.empty_contents(front, None, lava).is_some() {
                    Some((behavior, None, Vec3::ZERO))
                } else {
                    self.plan_dispense(pos, direction, Behavior::Drop, stack)
                }
            }
            Behavior::Scoop => {
                if Self::scooped(self.block_or_air(front)).is_some() {
                    Some((behavior, None, Vec3::ZERO))
                } else {
                    self.plan_dispense(pos, direction, Behavior::Drop, stack)
                }
            }
            Behavior::Tnt => Some((
                behavior,
                None,
                Vec3::new(
                    f64::from(front.x) + 0.5,
                    f64::from(front.y),
                    f64::from(front.z) + 0.5,
                ),
            )),
            Behavior::BoneMeal => Some((behavior, None, Vec3::ZERO)),
            Behavior::Shear => {
                let hive = self.block_or_air(front);
                if crate::beehive::is_hive_block(hive)
                    && crate::beehive::honey_level(hive) >= HONEY_FULL
                {
                    return Some((Behavior::ShearHive, None, Vec3::ZERO));
                }
                let sheep = self.mobs_in_block(front).into_iter().find(|id| {
                    self.entities.store.get(*id).is_some_and(|entity| {
                        matches!(
                            entity.base.kind,
                            Kind::Sheep | Kind::Mooshroom | Kind::SnowGolem
                        ) && crate::kinds::readiness_for_shearing(entity)
                    })
                });
                sheep.map(|target| (behavior, Some(target), Vec3::ZERO))
            }
            Behavior::Ignite => Some((behavior, None, Vec3::ZERO)),
            Behavior::Mud => {
                let state = self.block_or_air(front);
                if rspigot_domain::entity::physics::has_tag(state, "minecraft:convertible_to_mud")
                    && stack_is_water(stack)
                {
                    Some((behavior, None, Vec3::ZERO))
                } else {
                    self.plan_dispense(pos, direction, Behavior::Drop, stack)
                }
            }
            Behavior::Bottle => {
                let state = self.block_or_air(front);
                let honey = crate::beehive::is_hive_block(state)
                    && crate::beehive::honey_level(state) >= HONEY_FULL;
                if honey || has_water(state) {
                    Some((behavior, None, Vec3::ZERO))
                } else {
                    self.plan_dispense(pos, direction, Behavior::Drop, stack)
                }
            }
            Behavior::PowderSnow => {
                if self.block_or_air(front).is_air()
                    || crate::bucket::replaceable_by_water(self.block_or_air(front))
                {
                    Some((behavior, None, Vec3::ZERO))
                } else {
                    self.plan_dispense(pos, direction, Behavior::Drop, stack)
                }
            }
            Behavior::Cart(_) => match self.cart_position(pos, direction) {
                Some(at) => Some((behavior, None, at)),
                None => self.plan_dispense(pos, direction, Behavior::Drop, stack),
            },
            Behavior::Boat(kind) => match self.boat_position(pos, direction, kind) {
                Some(at) => Some((behavior, None, at)),
                None => self.plan_dispense(pos, direction, Behavior::Drop, stack),
            },
            Behavior::Stand | Behavior::Egg(_) | Behavior::ShearHive => {
                Some((behavior, None, Vec3::ZERO))
            }
            Behavior::Wax => {
                if waxed_of(self.block_or_air(front)).is_some() {
                    Some((behavior, None, Vec3::ZERO))
                } else {
                    self.plan_dispense(pos, direction, Behavior::Drop, stack)
                }
            }
            Behavior::Charge => {
                let anchor = self.block_or_air(front);
                if anchor.kind() != Some(BlockKind::RespawnAnchor) {
                    return self.plan_dispense(pos, direction, Behavior::Drop, stack);
                }
                let charges: i32 = anchor
                    .property("charges")
                    .and_then(|value| value.parse().ok())
                    .unwrap_or(ANCHOR_FULL);
                (charges < ANCHOR_FULL).then_some((behavior, None, Vec3::ZERO))
            }
            Behavior::Box => {
                let target = Vec3::new(f64::from(front.x), f64::from(front.y), f64::from(front.z));
                crate::input::is_replaceable(self.block_or_air(front))
                    .then_some((behavior, None, target))
            }
            Behavior::Skull => {
                let target = Vec3::new(f64::from(front.x), f64::from(front.y), f64::from(front.z));
                if self.block_or_air(front).is_air() && self.wither_base_stands(front) {
                    Some((behavior, None, target))
                } else {
                    let kind = stack.item.kind()?;
                    self.plan_dispense(pos, direction, equip_or_drop(kind), stack)
                }
            }
            Behavior::Pumpkin => {
                let target = Vec3::new(f64::from(front.x), f64::from(front.y), f64::from(front.z));
                if self.block_or_air(front).is_air() && self.can_spawn_golem_at(front) {
                    Some((behavior, None, target))
                } else {
                    let kind = stack.item.kind()?;
                    self.plan_dispense(pos, direction, equip_or_drop(kind), stack)
                }
            }
            Behavior::Brush => self
                .mobs_in_block(front)
                .into_iter()
                .find(|id| self.kind_of(*id) == Some(Kind::Armadillo))
                .map(|target| (behavior, Some(target), Vec3::ZERO)),
            Behavior::Chest => {
                let target = self.mobs_in_block(front).into_iter().find(|id| {
                    self.entities.store.get(*id).is_some_and(|entity| {
                        rspigot_domain::entity::species::equine::is_chested(entity.base.kind)
                            && !rspigot_domain::entity::species::equine::has_chest(&entity.base)
                            && rspigot_domain::entity::species::equine::is_tamed(&entity.base)
                    })
                });
                match target {
                    Some(target) => Some((behavior, Some(target), Vec3::ZERO)),
                    None => self.plan_dispense(pos, direction, Behavior::Drop, stack),
                }
            }
            Behavior::Release(_) => {
                if self.empty_contents(front, None, false).is_some() {
                    Some((behavior, None, Vec3::ZERO))
                } else {
                    self.plan_dispense(pos, direction, Behavior::Drop, stack)
                }
            }
        }
    }

    fn cart_position(&self, pos: BlockPos, direction: Face) -> Option<Vec3> {
        let toward = vector_of(direction);
        let front = step(pos, direction);
        let state = self.block_or_air(front);
        let slope = |rail: BlockStateId| {
            rail.property("shape")
                .and_then(rspigot_domain::entity::minecart::exits_of)
                .is_some_and(|exits| exits.slope)
        };
        let lift = if rspigot_domain::entity::minecart::is_rail(state) {
            if slope(state) { 0.6 } else { 0.1 }
        } else if state.is_air() {
            let below = self.block_or_air(step(front, Face::Down));
            if !rspigot_domain::entity::minecart::is_rail(below) {
                return None;
            }
            if direction != Face::Down && slope(below) {
                -0.4
            } else {
                -0.9
            }
        } else {
            return None;
        };
        Some(Vec3::new(
            f64::from(pos.x) + 0.5 + toward.x * 1.125,
            f64::from(pos.y) + toward.y + lift,
            f64::from(pos.z) + 0.5 + toward.z * 1.125,
        ))
    }

    fn boat_position(&mut self, pos: BlockPos, direction: Face, kind: Kind) -> Option<Vec3> {
        let toward = vector_of(direction);
        let width = f64::from(self.new_entity_base(kind).dimensions.width);
        let reach = width / 2.0 + 0.5625;
        let front = step(pos, direction);
        let state = self.block_or_air(front);
        let lift = if has_water(state) {
            1.0
        } else if state.is_air() && has_water(self.block_or_air(step(front, Face::Down))) {
            0.0
        } else {
            return None;
        };
        Some(Vec3::new(
            f64::from(pos.x) + 0.5 + toward.x * reach,
            f64::from(pos.y) + 0.5 + toward.y * 1.125 + lift,
            f64::from(pos.z) + 0.5 + toward.z * reach,
        ))
    }

    pub(crate) fn ask_dispense(&mut self, dispense: DispenseStep) {
        let pos = dispense.pos;
        let item = spec_of(&dispense.stack);
        let velocity = dispense.velocity;
        let target = dispense.target;
        let behavior = dispense.behavior;
        let transfer = dispense.transfer.clone();
        self.fire(
            Lane::Block(pos),
            Continuation::Dispense(Box::new(dispense)),
            move |server| {
                if let Move::Item(transfer) = transfer {
                    return Some(EventPayload::InventoryMoveItem(InventoryMoveItem {
                        destination: Self::dispense_ref(&transfer.destination),
                        initiator: Self::dispense_ref(&transfer.source),
                        item,
                        source: Self::dispense_ref(&transfer.source),
                    }));
                }
                match behavior {
                    Behavior::Equip(_) | Behavior::Brush | Behavior::Chest => {
                        Some(EventPayload::BlockDispenseArmor(BlockDispenseArmor {
                            block: pos,
                            item,
                            target_entity: server.entity_view(target?)?,
                            velocity,
                        }))
                    }
                    Behavior::Shear => Some(EventPayload::BlockShearEntity(BlockShearEntity {
                        block: pos,
                        entity: server.entity_view(target?)?,
                        tool: item,
                    })),
                    _ => Some(EventPayload::BlockDispense(BlockDispense {
                        block: pos,
                        item,
                        velocity,
                    })),
                }
            },
        );
    }

    fn dispense_ref(holder: &Holder) -> InventoryRef {
        InventoryRef {
            owner: holder.owner(),
            size: i32::try_from(holder.size).unwrap_or(i32::MAX),
            kind: holder.kind_name().to_owned(),
            id: 0,
        }
    }

    pub(crate) fn finish_dispense(
        &mut self,
        dispense: DispenseStep,
        cancelled: bool,
        item: Option<crate::plugins::ItemSpec>,
        velocity: Option<Vec3>,
        moved: Option<crate::plugins::ItemSpec>,
    ) {
        if cancelled {
            return;
        }
        let state = self.block_or_air(dispense.pos);
        let Some(direction) = facing(state).filter(|_| {
            matches!(
                state.kind(),
                Some(BlockKind::Dispenser | BlockKind::Dropper)
            )
        }) else {
            return;
        };
        let Some(me) = self.holder_at(dispense.pos) else {
            return;
        };
        let mut items = self.holder_items(&me);
        let present = items
            .get(dispense.slot)
            .and_then(Option::as_ref)
            .is_some_and(|stack| {
                stack.item == dispense.stack.item && stack.components == dispense.stack.components
            });
        if !present {
            return;
        }
        if let Move::Item(transfer) = &dispense.transfer {
            self.finish_dropper_move(&me, items, transfer, direction, moved);
            return;
        }
        let mut stack = dispense.stack.clone();
        let mut behavior = dispense.behavior;
        let mut velocity = velocity.unwrap_or(dispense.velocity);
        let mut target = dispense.target;
        if let Some(Ok(Some(replaced))) = item.as_ref().map(stack_of) {
            if replaced.item != stack.item {
                behavior = behavior_of(&replaced);
                match self.plan_dispense(dispense.pos, direction, behavior, &replaced) {
                    Some((planned, planned_target, planned_velocity)) => {
                        behavior = planned;
                        target = planned_target;
                        velocity = velocity_if_unset(velocity, planned_velocity, &dispense);
                    }
                    None => return,
                }
            }
            stack = replaced;
        }
        let Some(outcome) =
            self.perform_dispense(dispense.pos, direction, behavior, &stack, velocity, target)
        else {
            self.level_event(FAIL_EVENT, dispense.pos, 0);
            return;
        };
        let sound = match behavior {
            Behavior::Shoot(shot) => shot.event(),
            _ => CLICK_EVENT,
        };
        self.level_event(sound, dispense.pos, 0);
        self.level_event(SMOKE_EVENT, dispense.pos, direction_code(direction));
        let wear = outcome.wear;
        if let Some(cell) = items.get_mut(dispense.slot) {
            let remaining = cell.as_ref().and_then(|present| {
                if wear > 0 {
                    let mut worn_stack = present.clone();
                    let worn = rspigot_domain::entity::reward::damage_value(&worn_stack) + wear;
                    rspigot_domain::entity::reward::set_damage_value(&mut worn_stack, worn);
                    let broken = rspigot_domain::entity::reward::max_damage(&worn_stack)
                        .is_some_and(|max| worn >= max);
                    return (!broken).then_some(worn_stack);
                }
                with_count(present, u32::from(present.count.get()) - 1)
            });
            *cell = remaining;
        }
        let mut rest: Option<ItemStack> = outcome.remainder;
        if let Some(remainder) = rest.take() {
            let empty_slot = items.get(dispense.slot).is_some_and(Option::is_none);
            if empty_slot {
                items[dispense.slot] = Some(remainder);
            } else {
                let order: Vec<usize> = (0..items.len()).collect();
                rest = insert_stack(&mut items, &order, remainder, |_, _| true);
            }
        }
        self.store_holder(&me, &items);
        if let Some(leftover) = rest {
            let position = dispense_position(dispense.pos, direction);
            self.spawn_item_entity(position, Vec3::ZERO, PICKUP_DELAY, leftover);
        }
    }

    fn finish_dropper_move(
        &mut self,
        me: &Holder,
        mut items: Vec<Slot>,
        transfer: &Transfer,
        direction: Face,
        moved: Option<crate::plugins::ItemSpec>,
    ) {
        let destination = transfer.destination;
        let mut into = self.holder_items(&destination);
        let stack = match moved.as_ref().map(stack_of) {
            Some(Ok(Some(replaced))) => replaced,
            _ => transfer.stack.clone(),
        };
        let toward = direction.opposite();
        let slots = destination.slots_toward(toward);
        let before = into.clone();
        let leftover = insert_stack(&mut into, &slots, stack, |at, candidate| {
            destination.may_place(at, candidate, toward, &before)
        });
        if leftover.is_some() {
            return;
        }
        if let Some(cell) = items.get_mut(transfer.slot) {
            let remaining = cell
                .as_ref()
                .and_then(|present| with_count(present, u32::from(present.count.get()) - 1));
            *cell = remaining;
        }
        self.store_holder(me, &items);
        self.store_holder(&destination, &into);
    }

    fn perform_dispense(
        &mut self,
        pos: BlockPos,
        direction: Face,
        behavior: Behavior,
        stack: &ItemStack,
        velocity: Vec3,
        target: Option<EntityId>,
    ) -> Option<Outcome> {
        let front = step(pos, direction);
        let at = dispense_position(pos, direction);
        let plain = Outcome {
            remainder: None,
            wear: 0,
        };
        match behavior {
            Behavior::Drop => {
                let mut spot = at;
                spot.y -= if direction.is_horizontal() {
                    0.15625
                } else {
                    0.125
                };
                self.spawn_item_entity(spot, velocity, PICKUP_DELAY, stack.clone());
                Some(plain)
            }
            Behavior::Shoot(shot) => {
                let center = pos.center();
                let toward = vector_of(direction);
                let lead = shot.lead();
                let spot = Vec3::new(
                    center.x + lead * toward.x,
                    center.y + lead * toward.y,
                    center.z + lead * toward.z,
                );
                self.launch_shot(
                    shot,
                    if lead == OFFSET { at } else { spot },
                    velocity,
                    stack,
                );
                Some(plain)
            }
            Behavior::Equip(slot) => {
                let target = target.filter(|id| self.mobs_in_block(front).contains(id))?;
                self.set_equipment_stack(target, slot, Some(stack.clone()));
                Some(plain)
            }
            Behavior::Pour { lava } => {
                let (block, placement) = self.empty_contents(front, None, lava)?;
                self.pour_into(
                    block,
                    placement,
                    if lava {
                        "minecraft:item.bucket.empty_lava"
                    } else {
                        "minecraft:item.bucket.empty"
                    },
                );
                Some(Outcome {
                    remainder: Some(ItemStack::new(
                        ItemKind::Bucket.id(),
                        std::num::NonZeroU8::MIN,
                    )),
                    wear: 0,
                })
            }
            Behavior::Scoop => {
                let (bucket, after) = Self::scooped(self.block_or_air(front))?;
                self.set_block(front, after);
                Some(Outcome {
                    remainder: Some(ItemStack::new(bucket.id(), std::num::NonZeroU8::MIN)),
                    wear: 0,
                })
            }
            Behavior::Tnt => {
                let spot = Vec3::new(
                    f64::from(front.x) + 0.5,
                    f64::from(front.y),
                    f64::from(front.z) + 0.5,
                );
                self.spawn_primed_tnt(spot, rspigot_domain::entity::tnt::DEFAULT_FUSE, None);
                self.play_sound(
                    None,
                    spot,
                    "minecraft:entity.tnt.primed",
                    rspigot_entities::SoundSource::Blocks,
                    1.0,
                    1.0,
                );
                Some(plain)
            }
            Behavior::BoneMeal => (self.grow_crop(Lane::Block(pos), None, front)
                || self.bone_meal_apply(None, front)
                || self.bone_meal_growth(None, front))
            .then_some(plain),
            Behavior::Shear => {
                let target = target?;
                if self
                    .entities
                    .store
                    .get(target)
                    .is_some_and(|entity| entity.base.kind == Kind::SnowGolem)
                {
                    self.shear_snow_golem_pumpkin(target);
                } else {
                    self.shear_sheep_wool(target);
                }
                Some(Outcome {
                    remainder: None,
                    wear: 1,
                })
            }
            Behavior::Ignite => {
                let lit = self.block_or_air(front);
                if lit.kind() == Some(BlockKind::Tnt) {
                    self.prime_tnt(TntPrimeStep::of(front, TNTPrimeCause::Dispenser).by_block(pos));
                } else if lights_up(lit) {
                    if let Some(next) = lit.with_property("lit", "true") {
                        self.set_block(front, next);
                    }
                } else if lit.is_air() {
                    let below = self.block_or_air(step(front, Face::Down));
                    self.set_block(front, crate::explosion::fire_state_on(below));
                } else {
                    return None;
                }
                Some(Outcome {
                    remainder: None,
                    wear: i32::from(stack.item == ItemKind::FlintAndSteel.id()),
                })
            }
            Behavior::Mud => {
                self.set_block(front, BlockKind::Mud.default_state());
                self.play_sound(
                    None,
                    pos.center(),
                    "minecraft:item.bottle.empty",
                    rspigot_entities::SoundSource::Blocks,
                    1.0,
                    1.0,
                );
                Some(Outcome {
                    remainder: Some(ItemStack::new(
                        ItemKind::GlassBottle.id(),
                        std::num::NonZeroU8::MIN,
                    )),
                    wear: 0,
                })
            }
            Behavior::Bottle => {
                let state = self.block_or_air(front);
                let filled = if crate::beehive::is_hive_block(state)
                    && crate::beehive::honey_level(state) >= HONEY_FULL
                {
                    self.release_honey(front, state);
                    ItemStack::new(ItemKind::HoneyBottle.id(), std::num::NonZeroU8::MIN)
                } else if has_water(state) {
                    rspigot_domain::potion::stack(ItemKind::Potion, rspigot_domain::potion::WATER)
                } else {
                    return None;
                };
                Some(Outcome {
                    remainder: Some(filled),
                    wear: 0,
                })
            }
            Behavior::PowderSnow => {
                self.set_block(front, BlockKind::PowderSnow.default_state());
                self.play_sound(
                    None,
                    front.center(),
                    "minecraft:item.bucket.empty_powder_snow",
                    rspigot_entities::SoundSource::Blocks,
                    1.0,
                    1.0,
                );
                Some(Outcome {
                    remainder: Some(ItemStack::new(
                        ItemKind::Bucket.id(),
                        std::num::NonZeroU8::MIN,
                    )),
                    wear: 0,
                })
            }
            Behavior::Cart(kind) => {
                let mut base = self.new_entity_base(kind);
                let state = rspigot_domain::entity::minecart::define(&mut base);
                rspigot_domain::entity::minecart::set_initial_position(&mut base, velocity);
                self.name_from(&mut base, stack);
                self.spawn_entity(
                    Entity::new(base, EntityState::Minecart(Box::new(state))),
                    Birth::of(CreatureSpawnReason::Default),
                );
                Some(plain)
            }
            Behavior::Boat(kind) => {
                let mut base = self.new_entity_base(kind);
                let state = rspigot_domain::entity::boat::define(&mut base);
                rspigot_domain::entity::boat::set_initial_position(&mut base, velocity);
                base.set_yaw(yaw_of(direction));
                self.name_from(&mut base, stack);
                self.spawn_entity(
                    Entity::new(base, EntityState::Boat(Box::new(state))),
                    Birth::of(CreatureSpawnReason::Default),
                );
                Some(plain)
            }
            Behavior::Stand => {
                let mut base = self.new_entity_base(Kind::ArmorStand);
                let state = rspigot_domain::entity::armor_stand::define(&mut base);
                self.name_from(&mut base, stack);
                let at = Vec3::new(
                    f64::from(front.x) + 0.5,
                    f64::from(front.y),
                    f64::from(front.z) + 0.5,
                );
                base.snap_to(at, yaw_of(direction), 0.0);
                self.spawn_entity(
                    Entity::new(base, EntityState::Stand(Box::new(state))),
                    Birth::of(CreatureSpawnReason::Default),
                );
                Some(plain)
            }
            Behavior::ShearHive => {
                let hive = self.block_or_air(front);
                self.play_sound(
                    None,
                    front.center(),
                    rspigot_domain::beehive::SHEAR_SOUND,
                    rspigot_entities::SoundSource::Blocks,
                    1.0,
                    1.0,
                );
                self.drop_honeycomb(front);
                self.release_honey(front, hive);
                Some(Outcome {
                    remainder: None,
                    wear: 1,
                })
            }
            Behavior::Wax => {
                let waxed = waxed_of(self.block_or_air(front))?;
                self.set_block(front, waxed);
                self.level_event(WAX_ON_EVENT, front, 0);
                self.play_sound(
                    None,
                    front.center(),
                    WAX_ON_SOUND,
                    rspigot_entities::SoundSource::Blocks,
                    1.0,
                    1.0,
                );
                Some(plain)
            }
            Behavior::Charge => {
                let anchor = self.block_or_air(front);
                let charges: i32 = anchor.property("charges")?.parse().ok()?;
                let next = anchor.with_property("charges", &(charges + 1).to_string())?;
                self.set_block(front, next);
                self.play_sound(
                    None,
                    front.center(),
                    ANCHOR_CHARGE_SOUND,
                    rspigot_entities::SoundSource::Blocks,
                    1.0,
                    1.0,
                );
                Some(plain)
            }
            Behavior::Box => {
                if !crate::input::is_replaceable(self.block_or_air(front)) {
                    return None;
                }
                let below_empty = self.block_or_air(step(front, Face::Down)).is_air();
                let facing = if below_empty { direction } else { Face::Up };
                let kind = BlockKind::from_item(stack.item.0)?;
                let state = kind.default_state();
                let state = state
                    .with_property("facing", facing.name())
                    .unwrap_or(state);
                self.set_block(front, state);
                Some(plain)
            }
            Behavior::Skull => {
                if !self.block_or_air(front).is_air() || !self.wither_base_stands(front) {
                    return None;
                }
                let skull = BlockKind::WitherSkeletonSkull.default_state();
                let rotation = skull_rotation(direction).to_string();
                let state = skull.with_property("rotation", &rotation).unwrap_or(skull);
                self.set_block(front, state);
                self.check_wither_summon(front);
                Some(plain)
            }
            Behavior::Pumpkin => {
                if !self.block_or_air(front).is_air() || !self.can_spawn_golem_at(front) {
                    return None;
                }
                self.set_block(front, BlockKind::CarvedPumpkin.default_state());
                Some(plain)
            }
            Behavior::Brush => {
                let target = target.filter(|id| self.mobs_in_block(front).contains(id))?;
                self.brush_off_scute(target).then_some(Outcome {
                    remainder: None,
                    wear: BRUSH_WEAR,
                })
            }
            Behavior::Chest => {
                let target = target.filter(|id| self.mobs_in_block(front).contains(id))?;
                self.fit_chest(target);
                Some(plain)
            }
            Behavior::Release(kind) => {
                let (block, placement) = self.empty_contents(front, None, false)?;
                self.pour_into(block, placement, FISH_EMPTY_SOUND);
                self.spawn_bucketed_fish(None, kind, stack, block);
                Some(Outcome {
                    remainder: Some(ItemStack::new(
                        ItemKind::Bucket.id(),
                        std::num::NonZeroU8::MIN,
                    )),
                    wear: 0,
                })
            }
            Behavior::Egg(kind) => {
                let at = Vec3::new(
                    f64::from(front.x) + 0.5,
                    f64::from(front.y),
                    f64::from(front.z) + 0.5,
                );
                let (mut entity, random) = self
                    .summon_entity(
                        kind,
                        at,
                        None,
                        rspigot_domain::spawn::SpawnReason::SpawnItemUse,
                    )
                    .ok()?;
                if let Some(name) = stack
                    .components
                    .get("minecraft:custom_name")
                    .and_then(rspigot_domain::entity::save::component_from_tag)
                {
                    entity.base.set_custom_name(Some(name));
                }
                self.spawn_entity(
                    entity,
                    Birth::of(CreatureSpawnReason::DispenseEgg).with_random(random),
                );
                Some(plain)
            }
        }
    }

    fn name_from(&self, base: &mut rspigot_domain::entity::EntityBase, stack: &ItemStack) {
        if let Some(name) = stack
            .components
            .get("minecraft:custom_name")
            .and_then(rspigot_domain::entity::save::component_from_tag)
        {
            base.set_custom_name(Some(name));
        }
    }

    fn scooped(state: BlockStateId) -> Option<(ItemKind, BlockStateId)> {
        crate::liquid_bucket::pickup_of(state).map(|pickup| (pickup.bucket, pickup.after))
    }

    fn pour_into(
        &mut self,
        block: BlockPos,
        placement: crate::bucket::Placement,
        sound: &'static str,
    ) {
        use crate::bucket::Placement;
        let spot = block.center();
        match placement {
            Placement::Evaporate => {
                self.play_sound(
                    None,
                    spot,
                    "minecraft:block.fire.extinguish",
                    rspigot_entities::SoundSource::Blocks,
                    0.5,
                    2.6,
                );
            }
            Placement::Waterlog => {
                if let Some(next) = self
                    .block_or_air(block)
                    .with_property("waterlogged", "true")
                {
                    self.set_block(block, next);
                }
            }
            Placement::Place { destroy } => {
                if destroy {
                    self.break_block_naturally(block);
                }
                let liquid = if sound.ends_with("lava") {
                    BlockKind::Lava
                } else {
                    BlockKind::Water
                };
                self.set_block(block, liquid.default_state());
                self.play_sound(
                    None,
                    spot,
                    sound,
                    rspigot_entities::SoundSource::Blocks,
                    1.0,
                    1.0,
                );
            }
        }
    }

    fn launch_charge(
        &mut self,
        shot: Shot,
        at: Vec3,
        velocity: Vec3,
        stack: &ItemStack,
        seed: i64,
    ) {
        let kind = match shot {
            Shot::Firework => Kind::FireworkRocket,
            Shot::Fire => Kind::SmallFireball,
            _ => Kind::WindCharge,
        };
        let mut base = self.new_entity_base(kind);
        base.position = at;
        base.old_position = at;
        let state = match shot {
            Shot::Firework => {
                EntityState::Firework(Box::new(FireworkState::new(&mut base, seed, stack)))
            }
            _ => {
                let state = HurtingState::new(&mut base);
                if shot == Shot::Fire {
                    hurting::set_item(&mut base, stack);
                }
                EntityState::Hurting(Box::new(state))
            }
        };
        let movement = thrown::movement_to_shoot(
            &mut ServerRandom(&mut self.rng),
            velocity,
            shot.power(),
            shot.uncertainty(),
        );
        base.velocity = movement;
        base.needs_sync = true;
        let horizontal = (movement.x * movement.x + movement.z * movement.z).sqrt();
        base.set_yaw((mth::atan2(movement.x, movement.z) * mth::RAD_TO_DEG) as f32);
        base.set_pitch((mth::atan2(movement.y, horizontal) * mth::RAD_TO_DEG) as f32);
        self.spawn_entity(
            Entity::new(base, state),
            Birth::of(CreatureSpawnReason::Default),
        );
    }

    pub(crate) fn launch_shot(&mut self, shot: Shot, at: Vec3, velocity: Vec3, stack: &ItemStack) {
        let seed = self.rng.random::<i64>();
        if matches!(shot, Shot::Firework | Shot::Fire | Shot::Wind) {
            self.launch_charge(shot, at, velocity, stack, seed);
            return;
        }
        let kind = match shot {
            Shot::Arrow => Kind::Arrow,
            Shot::Egg => Kind::Egg,
            Shot::Snowball => Kind::Snowball,
            Shot::Bottle => Kind::ExperienceBottle,
            Shot::Splash => Kind::SplashPotion,
            Shot::Lingering => Kind::LingeringPotion,
            Shot::Firework | Shot::Fire | Shot::Wind => return,
        };
        let mut base = self.new_entity_base(kind);
        base.position = at;
        base.old_position = at;
        let entity = if shot == Shot::Arrow {
            let mut state = ArrowState::new(&mut base, seed);
            state.pickup = Pickup::Allowed;
            state.item = stack.clone();
            arrow::shoot(
                &mut base,
                &mut state,
                velocity,
                SHOT_POWER,
                SHOT_UNCERTAINTY,
            );
            Entity::new(base, EntityState::Arrow(Box::new(state)))
        } else {
            let mut state = ThrownState::new(&mut base, seed);
            thrown::set_item(&mut base, stack);
            thrown::shoot(
                &mut base,
                &mut state,
                velocity,
                SHOT_POWER,
                SHOT_UNCERTAINTY,
            );
            Entity::new(base, EntityState::Thrown(Box::new(state)))
        };
        self.spawn_entity(entity, Birth::of(CreatureSpawnReason::Default));
    }
}

#[derive(Debug, Clone, PartialEq)]
struct Outcome {
    remainder: Option<ItemStack>,
    wear: i32,
}

fn velocity_if_unset(current: Vec3, planned: Vec3, original: &DispenseStep) -> Vec3 {
    if current == original.velocity {
        planned
    } else {
        current
    }
}

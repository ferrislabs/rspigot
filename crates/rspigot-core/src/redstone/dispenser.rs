use rand::Rng;
use rspigot_blocks::{BlockKind, Face};
use rspigot_domain::entity::arrow::{self, ArrowState, Pickup};
use rspigot_domain::entity::thrown::{self, ThrownState};
use rspigot_domain::entity::{Aabb, Entity, EntityState, living};
use rspigot_domain::item::{ItemStack, Slot, with_count};
use rspigot_domain::{BlockPos, BlockStateId, EntityId, Vec3};
use rspigot_entities::EntityKind as Kind;
use rspigot_items::{EquipmentSlot, ItemKind};

use super::hopper::Transfer;
use super::storage::{Holder, insert_stack};
use super::{facing, step};
use crate::items::PICKUP_DELAY;
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
const CLICK_EVENT: i32 = 1000;
const FAIL_EVENT: i32 = 1001;
const LAUNCH_EVENT: i32 = 1002;
const SMOKE_EVENT: i32 = 2000;
const SHOT_POWER: f32 = 1.1;
const SHOT_UNCERTAINTY: f32 = 6.0;
const DROP_SPREAD: f64 = 0.0172275 * 6.0;
const SHOT_LIFT: f64 = 0.1;
const OFFSET: f64 = 0.7;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Shot {
    Arrow,
    Egg,
    Snowball,
    Bottle,
    Splash,
    Lingering,
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
}

fn behavior_of(stack: &ItemStack) -> Behavior {
    let Some(kind) = stack.item.kind() else {
        return Behavior::Drop;
    };
    match kind {
        ItemKind::Arrow => Behavior::Shoot(Shot::Arrow),
        ItemKind::Egg => Behavior::Shoot(Shot::Egg),
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
        ItemKind::FlintAndSteel | ItemKind::FireCharge => Behavior::Ignite,
        _ => match kind.equippable() {
            Some(
                slot @ (EquipmentSlot::Head
                | EquipmentSlot::Chest
                | EquipmentSlot::Legs
                | EquipmentSlot::Feet),
            ) => Behavior::Equip(slot),
            _ => Behavior::Drop,
        },
    }
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
            Behavior::Equip(_) => EventKind::BlockDispenseArmor,
            Behavior::Shear => EventKind::BlockShearEntity,
            _ => EventKind::BlockDispense,
        }
    }
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
        let Some((behavior, target, velocity)) = self.plan_dispense(pos, direction, behavior)
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
                    self.entities
                        .store
                        .get(*id)
                        .and_then(Entity::living)
                        .is_some_and(|state| state.mob.is_some() && state.equipment(slot).is_none())
                });
                match target {
                    Some(target) => Some((behavior, Some(target), Vec3::ZERO)),
                    None => self.plan_dispense(pos, direction, Behavior::Drop),
                }
            }
            Behavior::Pour { lava } => {
                if self.empty_contents(front, None, lava).is_some() {
                    Some((behavior, None, Vec3::ZERO))
                } else {
                    self.plan_dispense(pos, direction, Behavior::Drop)
                }
            }
            Behavior::Scoop => {
                if Self::scooped(self.block_or_air(front)).is_some() {
                    Some((behavior, None, Vec3::ZERO))
                } else {
                    self.plan_dispense(pos, direction, Behavior::Drop)
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
                let sheep = self.mobs_in_block(front).into_iter().find(|id| {
                    self.entities.store.get(*id).is_some_and(|entity| {
                        matches!(entity.base.kind, Kind::Sheep | Kind::Mooshroom)
                            && crate::kinds::readiness_for_shearing(entity)
                    })
                });
                sheep.map(|target| (behavior, Some(target), Vec3::ZERO))
            }
            Behavior::Ignite => Some((behavior, None, Vec3::ZERO)),
        }
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
                    Behavior::Equip(_) => {
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
                match self.plan_dispense(dispense.pos, direction, behavior) {
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
        let launch = matches!(behavior, Behavior::Shoot(_));
        self.level_event(
            if launch { LAUNCH_EVENT } else { CLICK_EVENT },
            dispense.pos,
            0,
        );
        self.level_event(SMOKE_EVENT, dispense.pos, direction_code(direction));
        let wear = outcome.wear;
        if let Some(cell) = items.get_mut(dispense.slot) {
            let remaining = cell.as_ref().and_then(|present| {
                let mut left = with_count(present, u32::from(present.count.get()) - 1);
                if let (Some(rest), true) = (left.as_mut(), wear) {
                    let worn = rspigot_domain::entity::reward::damage_value(rest) + 1;
                    rspigot_domain::entity::reward::set_damage_value(rest, worn);
                    if rspigot_domain::entity::reward::max_damage(rest)
                        .is_some_and(|max| worn >= max)
                    {
                        left = None;
                    }
                }
                left
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
            wear: false,
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
                self.launch_shot(shot, at, velocity, stack);
                Some(plain)
            }
            Behavior::Equip(slot) => {
                let target = target.filter(|id| self.mobs_in_block(front).contains(id))?;
                self.set_equipment_stack(target, slot, Some(stack.clone()));
                Some(plain)
            }
            Behavior::Pour { lava } => {
                let (block, placement) = self.empty_contents(front, None, lava)?;
                self.pour_into(block, placement, lava);
                Some(Outcome {
                    remainder: Some(ItemStack::new(
                        ItemKind::Bucket.id(),
                        std::num::NonZeroU8::MIN,
                    )),
                    wear: false,
                })
            }
            Behavior::Scoop => {
                let (bucket, after) = Self::scooped(self.block_or_air(front))?;
                self.set_block(front, after);
                Some(Outcome {
                    remainder: Some(ItemStack::new(bucket.id(), std::num::NonZeroU8::MIN)),
                    wear: false,
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
            Behavior::BoneMeal => self
                .grow_crop(Lane::Block(pos), None, front)
                .then_some(plain),
            Behavior::Shear => {
                let target = target?;
                self.shear_sheep_wool(target);
                Some(Outcome {
                    remainder: None,
                    wear: true,
                })
            }
            Behavior::Ignite => {
                let lit = self.block_or_air(front);
                if lit.kind() == Some(BlockKind::Tnt) {
                    self.prime_tnt(TntPrimeStep::of(front, TNTPrimeCause::Dispenser).by_block(pos));
                } else if lit.is_air() {
                    let below = self.block_or_air(step(front, Face::Down));
                    self.set_block(front, crate::explosion::fire_state_on(below));
                } else {
                    return None;
                }
                Some(Outcome {
                    remainder: None,
                    wear: stack.item == ItemKind::FlintAndSteel.id(),
                })
            }
        }
    }

    fn scooped(state: BlockStateId) -> Option<(ItemKind, BlockStateId)> {
        crate::liquid_bucket::pickup_of(state).map(|pickup| (pickup.bucket, pickup.after))
    }

    fn pour_into(&mut self, block: BlockPos, placement: crate::bucket::Placement, lava: bool) {
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
                let liquid = if lava {
                    BlockKind::Lava
                } else {
                    BlockKind::Water
                };
                self.set_block(block, liquid.default_state());
                self.play_sound(
                    None,
                    spot,
                    if lava {
                        "minecraft:item.bucket.empty_lava"
                    } else {
                        "minecraft:item.bucket.empty"
                    },
                    rspigot_entities::SoundSource::Blocks,
                    1.0,
                    1.0,
                );
            }
        }
    }

    fn launch_shot(&mut self, shot: Shot, at: Vec3, velocity: Vec3, stack: &ItemStack) {
        let seed = self.rng.random::<i64>();
        let kind = match shot {
            Shot::Arrow => Kind::Arrow,
            Shot::Egg => Kind::Egg,
            Shot::Snowball => Kind::Snowball,
            Shot::Bottle => Kind::ExperienceBottle,
            Shot::Splash => Kind::SplashPotion,
            Shot::Lingering => Kind::LingeringPotion,
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
    wear: bool,
}

fn velocity_if_unset(current: Vec3, planned: Vec3, original: &DispenseStep) -> Vec3 {
    if current == original.velocity {
        planned
    } else {
        current
    }
}

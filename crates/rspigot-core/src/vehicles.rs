use rspigot_blocks::BlockKind;
use rspigot_domain::container_menu::{ContainerMenu, MenuKind};
use rspigot_domain::damage::DamageTag;
use rspigot_domain::entity::save::component_to_tag;
use rspigot_domain::entity::vehicle;
use rspigot_domain::entity::{Entity, EntityState, RemovalReason};
use rspigot_domain::game_event::GameEvent;
use rspigot_domain::item::ItemStack;
use rspigot_domain::player::PlayerId;
use rspigot_domain::{EntityId, Vec3};
use rspigot_entities::EntityKind as Kind;
use rspigot_items::ItemKind;
use std::num::NonZeroU8;

use crate::containers::{Backing, Lid, OpenMenu};
use crate::events::Hand;
use crate::game_events::GameEventContext;
use crate::health::DamageSource;
use crate::plugins::events::{
    CreatureSpawnReason, EntityPlace, EntityRemoveCause, VehicleDamage, VehicleDestroy,
    VehicleEntityCollision,
};
use crate::plugins::steps::{
    AfterHurt, Outcome, VehicleDestroyStep, VehicleHurtStep, VehiclePlaceStep, VehiclePushStep,
};
use crate::plugins::{Birth, Continuation, EventPayload, Spawned, slot_of};
use crate::ports::ClientLink;
use crate::server::Server;

const CUSTOM_NAME: &str = "minecraft:custom_name";
const PUSH_STRENGTH: f64 = 0.05;
const PUSH_THRESHOLD: f64 = 0.01;

pub(crate) fn drop_item_of(kind: Kind) -> Option<ItemKind> {
    if kind.extends("AbstractMinecart") {
        return Some(match kind {
            Kind::SpawnerMinecart | Kind::CommandBlockMinecart => ItemKind::Minecart,
            other => ItemKind::from_name(other.name())?,
        });
    }
    ItemKind::from_name(kind.name())
}

pub(crate) fn is_vehicle_state(state: &EntityState) -> bool {
    matches!(state, EntityState::Boat(_) | EntityState::Minecart(_))
}

pub(crate) fn cargo_of(entity: &Entity) -> Option<&vehicle::Cargo> {
    match &entity.state {
        EntityState::Boat(state) => state.cargo.as_ref(),
        EntityState::Minecart(state) => state.cargo.as_ref(),
        _ => None,
    }
}

pub(crate) fn cargo_mut(entity: &mut Entity) -> Option<&mut vehicle::Cargo> {
    match &mut entity.state {
        EntityState::Boat(state) => state.cargo.as_mut(),
        EntityState::Minecart(state) => state.cargo.as_mut(),
        _ => None,
    }
}

pub(crate) fn cargo_menu(entity: &Entity) -> MenuKind {
    if entity.base.kind == Kind::HopperMinecart {
        MenuKind::Hopper
    } else {
        MenuKind::Generic { rows: 3 }
    }
}

pub(crate) fn cargo_title(entity: &Entity) -> &'static str {
    match entity.base.kind {
        Kind::OakChestBoat => "entity.minecraft.oak_chest_boat",
        Kind::SpruceChestBoat => "entity.minecraft.spruce_chest_boat",
        Kind::BirchChestBoat => "entity.minecraft.birch_chest_boat",
        Kind::JungleChestBoat => "entity.minecraft.jungle_chest_boat",
        Kind::AcaciaChestBoat => "entity.minecraft.acacia_chest_boat",
        Kind::DarkOakChestBoat => "entity.minecraft.dark_oak_chest_boat",
        Kind::PaleOakChestBoat => "entity.minecraft.pale_oak_chest_boat",
        Kind::MangroveChestBoat => "entity.minecraft.mangrove_chest_boat",
        Kind::CherryChestBoat => "entity.minecraft.cherry_chest_boat",
        Kind::BambooChestRaft => "entity.minecraft.bamboo_chest_raft",
        Kind::ChestMinecart => "entity.minecraft.chest_minecart",
        Kind::HopperMinecart => "entity.minecraft.hopper_minecart",
        _ => "container.chest",
    }
}

impl<L: ClientLink> Server<L> {
    pub(crate) fn is_vehicle_entity(&self, id: EntityId) -> bool {
        self.entities
            .store
            .get(id)
            .is_some_and(|entity| is_vehicle_state(&entity.state))
    }

    pub(crate) fn strike_vehicle(
        &mut self,
        id: EntityId,
        source: &DamageSource,
        amount: f32,
        then: AfterHurt,
    ) -> bool {
        let Some(base) = self.entities.store.get(id).map(|entity| &entity.base) else {
            self.after_hurt(then, Outcome::Missed);
            return false;
        };
        let invulnerable = base.invulnerable
            && !source.kind.is(DamageTag::BypassesInvulnerability)
            && !source.creative_attacker();
        if base.is_removed() || invulnerable {
            self.after_hurt(then, Outcome::Missed);
            return false;
        }
        let attacker = source.causing.as_ref().map(|causing| causing.entity);
        self.tnt_cart_struck(id, source);
        let lane = self.lane_for(source.attacker(), id);
        let step = VehicleHurtStep {
            vehicle: id,
            source: source.clone(),
            amount,
            then,
        };
        self.impacts.last = None;
        self.fire(
            lane,
            Continuation::HurtVehicle(Box::new(step)),
            move |server| {
                Some(EventPayload::VehicleDamage(VehicleDamage {
                    attacker: attacker.and_then(|attacker| server.subject_view(attacker)),
                    damage: f64::from(amount),
                    vehicle: server.subject_view(id)?,
                }))
            },
        );
        self.impacts.last.take().unwrap_or(true)
    }

    pub(crate) fn finish_vehicle_hurt(
        &mut self,
        step: VehicleHurtStep,
        cancelled: bool,
        damage: Option<f64>,
    ) {
        let VehicleHurtStep {
            vehicle: id,
            source,
            amount,
            then,
        } = step;
        let Some(entity) = self.entities.store.get_mut(id) else {
            self.after_hurt(then, Outcome::Missed);
            self.impacts.last = Some(false);
            return;
        };
        if cancelled {
            self.after_hurt(then, Outcome::Cancelled);
            self.impacts.last = Some(false);
            return;
        }
        let amount = damage.map_or(amount, |damage| damage as f32);
        vehicle::wobble(&mut entity.base, amount);
        let total = vehicle::damage(&entity.base);
        let at = entity.base.position;
        let attacker = source.causing.as_ref().map(|causing| causing.entity);
        let creative = source.creative_attacker();
        self.game_event(GameEvent::EntityDamage, at, GameEventContext::of(attacker));
        let destroys = total > vehicle::BREAK_DAMAGE;
        self.after_hurt(then, Outcome::Hurt);
        self.impacts.last = Some(true);
        if destroys && !creative && self.tnt_cart_breaks(id, &source, attacker) {
            return;
        }
        if destroys || creative {
            let lane = self.lane_for(source.attacker(), id);
            let step = VehicleDestroyStep {
                vehicle: id,
                attacker,
                creative,
            };
            self.fire(lane, Continuation::DestroyVehicle(step), move |server| {
                Some(EventPayload::VehicleDestroy(VehicleDestroy {
                    attacker: attacker.and_then(|attacker| server.subject_view(attacker)),
                    vehicle: server.subject_view(id)?,
                }))
            });
        }
    }

    pub(crate) fn finish_vehicle_destroy(&mut self, step: VehicleDestroyStep, cancelled: bool) {
        let VehicleDestroyStep {
            vehicle: id,
            attacker,
            creative,
        } = step;
        if self.entities.store.get(id).is_none() {
            return;
        }
        if cancelled {
            if let Some(entity) = self.entities.store.get_mut(id) {
                vehicle::set_damage(&mut entity.base, vehicle::BREAK_DAMAGE);
            }
            return;
        }
        if creative {
            self.discard_vehicle(id);
        } else {
            self.destroy_vehicle(id, attacker);
        }
    }

    fn discard_vehicle(&mut self, id: EntityId) {
        let Some(view) = self.subject_view(id) else {
            return;
        };
        if let Some(entity) =
            self.remove_entity(id, RemovalReason::Discarded, Some(EntityRemoveCause::Death))
        {
            self.drop_cargo(&view, &entity);
        }
    }

    pub(crate) fn destroy_vehicle(&mut self, id: EntityId, killer: Option<EntityId>) {
        let Some(view) = self.subject_view(id) else {
            return;
        };
        let Some(entity) =
            self.remove_entity(id, RemovalReason::Killed, Some(EntityRemoveCause::Death))
        else {
            return;
        };
        self.game_event(
            GameEvent::EntityDie,
            entity.base.position,
            GameEventContext::of(killer.or(Some(id))),
        );
        if !self.config.rules.entity_drops {
            return;
        }
        self.drop_cargo(&view, &entity);
        let Some(item) = drop_item_of(entity.base.kind) else {
            return;
        };
        let mut stack = ItemStack::new(item.id(), NonZeroU8::MIN);
        if let Some(name) = entity.base.custom_name() {
            stack.components.insert(CUSTOM_NAME, component_to_tag(name));
        }
        let drop = self.item_at_location(view.position, 0.0, stack);
        self.drop_from(view, drop);
    }

    fn drop_cargo(&mut self, view: &crate::plugins::EntityView, entity: &Entity) {
        let Some(cargo) = cargo_of(entity) else {
            return;
        };
        let stacks: Vec<ItemStack> = cargo.items.iter().flatten().cloned().collect();
        self.drop_contents(view.position.block(), stacks);
    }

    pub(crate) fn open_cargo(&mut self, player: PlayerId, vehicle: EntityId) -> bool {
        if self.unpack_cargo_loot(player, vehicle) {
            return true;
        }
        let Some(entity) = self.entities.store.get(vehicle) else {
            return false;
        };
        if cargo_of(entity).is_none() {
            return false;
        }
        let menu = cargo_menu(entity);
        let position = entity.base.position.block();
        self.close_open_menu(player, true);
        let inventory = self.next_inventory_id();
        let open = OpenMenu {
            window: 0,
            inventory,
            title: "",
            menu: ContainerMenu::new(menu),
            backing: Backing {
                pos: position,
                second: None,
                entity: None,
                ender: false,
                block: BlockKind::Air,
                lid: Lid::None,
            },
            state_id: 0,
            local: Vec::new(),
            data: Vec::new(),
            trader: None,
            mount: Some(vehicle),
        };
        self.ask_open(player, position, open);
        true
    }

    pub(crate) fn place_vehicle(
        &mut self,
        id: PlayerId,
        hand: Hand,
        entity: Entity,
        hit: (rspigot_domain::BlockPos, crate::plugins::events::Face),
    ) {
        let view = crate::plugins::view_of_entity(&entity);
        let step = VehiclePlaceStep {
            player: id,
            hand,
            entity,
        };
        let lane = self.lane_for(Some(id), view.id);
        self.fire(
            lane,
            Continuation::PlaceVehicle(Box::new(step)),
            move |server| {
                Some(EventPayload::EntityPlace(EntityPlace {
                    block: hit.0,
                    block_face: hit.1,
                    entity_type: view.kind.clone(),
                    entity: view,
                    hand: slot_of(hand),
                    player: server.snapshot_of(id),
                }))
            },
        );
    }

    pub(crate) fn finish_vehicle_place(&mut self, step: VehiclePlaceStep, cancelled: bool) {
        if cancelled {
            return;
        }
        let VehiclePlaceStep {
            player,
            hand,
            entity,
        } = step;
        self.spawn_entity(
            entity,
            Birth::of(CreatureSpawnReason::Default)
                .by(player)
                .then(Spawned::Placed { player, hand }),
        );
    }

    pub(crate) fn finish_vehicle_placed(&mut self, player: PlayerId, hand: Hand, id: EntityId) {
        let at = self
            .entities
            .store
            .get(id)
            .map(|entity| entity.base.position);
        let placer = self.players.get(&player).map(|p| p.entity_id);
        if let Some(at) = at {
            self.game_event(GameEvent::EntityPlace, at, GameEventContext::of(placer));
        }
        self.consume_held(player, hand, 1);
    }

    pub(crate) fn push_vehicle(&mut self, vehicle: EntityId, other: EntityId, ride: bool) {
        let lane = self.lane_for(None, vehicle);
        self.fire(
            lane,
            Continuation::PushVehicle(VehiclePushStep {
                vehicle,
                other,
                ride,
            }),
            move |server| {
                Some(EventPayload::VehicleEntityCollision(
                    VehicleEntityCollision {
                        collision_cancelled: false,
                        entity: server.subject_view(other)?,
                        pickup_cancelled: false,
                        vehicle: server.subject_view(vehicle)?,
                    },
                ))
            },
        );
    }

    pub(crate) fn finish_vehicle_push(&mut self, step: VehiclePushStep, cancelled: bool) {
        if cancelled {
            return;
        }
        let VehiclePushStep {
            vehicle,
            other,
            ride,
        } = step;
        if ride {
            self.start_riding(other, vehicle, false, true);
        } else {
            self.shove_apart(vehicle, other);
        }
    }

    fn shove_apart(&mut self, vehicle: EntityId, other: EntityId) {
        if self
            .entities
            .rides
            .is_passenger_of_same_vehicle(vehicle, other)
        {
            return;
        }
        let (Some(first), Some(second)) = (
            self.entities.store.get(vehicle).map(|e| e.base.position),
            self.entities.store.get(other).map(|e| e.base.position),
        ) else {
            return;
        };
        let mut dx = second.x - first.x;
        let mut dz = second.z - first.z;
        let mut dd = dx.abs().max(dz.abs());
        if dd < PUSH_THRESHOLD {
            return;
        }
        dd = dd.sqrt();
        dx /= dd;
        dz /= dd;
        let falloff = (1.0 / dd).min(1.0);
        dx *= falloff * PUSH_STRENGTH;
        dz *= falloff * PUSH_STRENGTH;
        if !self.is_vehicle(vehicle)
            && let Some(entity) = self.entities.store.get_mut(vehicle)
        {
            entity.base.velocity = entity.base.velocity + Vec3::new(-dx, 0.0, -dz);
            entity.base.sync_velocity = true;
        }
        if !self.is_vehicle(other)
            && let Some(entity) = self.entities.store.get_mut(other)
        {
            entity.base.velocity = entity.base.velocity + Vec3::new(dx, 0.0, dz);
            entity.base.sync_velocity = true;
        }
    }
}

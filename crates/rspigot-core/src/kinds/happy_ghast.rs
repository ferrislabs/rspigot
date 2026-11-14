use std::num::NonZeroU8;

use rspigot_ai::{HappyGhastFacts, MobView};
use rspigot_domain::EntityId;
use rspigot_domain::entity::species::Species;
use rspigot_domain::entity::species::happy_ghast::{self, HappyGhastState};
use rspigot_domain::entity::{Entity, EntityBase, EntityState};
use rspigot_domain::game_event::GameEvent;
use rspigot_domain::item::ItemStack;
use rspigot_domain::player::PlayerId;
use rspigot_entities::EntityKind as Kind;
use rspigot_items::EquipmentSlot;

use crate::events::Hand;
use crate::game_events::GameEventContext;
use crate::interact::Interaction;
use crate::mob_life::type_in_tag;
use crate::plugins::events::EntityRegainHealthRegainReason;
use crate::ports::ClientLink;
use crate::server::Server;
use crate::spawning::LEASH;

const CAN_EQUIP_HARNESS: &str = "minecraft:can_equip_harness";
const HARNESS_SUFFIX: &str = "_harness";
const HEAL_AMOUNT: f32 = 1.0;

pub(crate) fn ghast_state(entity: &mut Entity) -> Option<(&mut EntityBase, &mut HappyGhastState)> {
    let EntityState::Living(living) = &mut entity.state else {
        return None;
    };
    match living.mob.as_mut().map(|mob| &mut mob.species) {
        Some(Species::HappyGhast(state)) => Some((&mut entity.base, state)),
        _ => None,
    }
}

fn state_of(entity: &Entity) -> Option<&HappyGhastState> {
    match &entity.living()?.mob.as_ref()?.species {
        Species::HappyGhast(state) => Some(state),
        _ => None,
    }
}

pub(crate) fn is_on_still_timeout(entity: &Entity) -> bool {
    state_of(entity).is_some_and(|state| happy_ghast::is_on_still_timeout(&entity.base, state))
}

pub(crate) fn wears_harness(entity: &Entity) -> bool {
    entity
        .living()
        .is_some_and(|living| living.equipment(EquipmentSlot::Body).is_some())
}

fn is_harness(stack: &ItemStack) -> bool {
    stack.item.kind().is_some_and(|item| {
        item.equippable() == Some(EquipmentSlot::Body) && item.name().ends_with(HARNESS_SUFFIX)
    })
}

impl<L: ClientLink> Server<L> {
    pub(crate) fn happy_ghast_view(&self, entity: &Entity, view: &mut MobView) {
        view.species.happy_ghast = HappyGhastFacts {
            wearing_harness: wears_harness(entity),
            vehicle: self.entities.rides.is_vehicle(entity.base.id),
            on_still_timeout: is_on_still_timeout(entity),
        };
    }

    pub(crate) fn happy_ghast_set_still_timeout(&mut self, id: EntityId, ticks: i32) {
        if let Some((base, state)) = self.entities.store.get_mut(id).and_then(ghast_state) {
            happy_ghast::set_server_still_timeout(base, state, ticks);
        }
    }

    fn happy_ghast_scans_player(&self, id: EntityId) -> bool {
        let Some(entity) = self.entities.store.get(id) else {
            return false;
        };
        let area = happy_ghast::player_scan_area(&entity.base.bounding_box());
        self.players.values().any(|player| {
            if player.game_mode == rspigot_domain::player::GameMode::Spectator {
                return false;
            }
            let root = self.entities.rides.root_vehicle(player.entity_id);
            let rides_ghast =
                root != player.entity_id && self.kind_of(root) == Some(Kind::HappyGhast);
            !rides_ghast && happy_ghast::contains(&area, player.position)
        })
    }

    pub(crate) fn happy_ghast_post_tick(&mut self, id: EntityId) {
        if let Some((base, state)) = self.entities.store.get_mut(id).and_then(ghast_state) {
            if state.leash_holder_time > 0 {
                state.leash_holder_time -= 1;
            }
            happy_ghast::set_leash_holder(base, state.leash_holder_time > 0);
            if state.server_still_timeout > 0 {
                if base.tick_count > happy_ghast::STILL_TIMEOUT_MIN_AGE {
                    state.server_still_timeout -= 1;
                }
                let ticks = state.server_still_timeout;
                happy_ghast::set_server_still_timeout(base, state, ticks);
            }
        }
        if self.happy_ghast_scans_player(id) {
            self.happy_ghast_set_still_timeout(id, happy_ghast::STILL_TIMEOUT_TICKS);
        }
    }

    pub(crate) fn happy_ghast_ai_step(&mut self, id: EntityId) {
        let Some(entity) = self.entities.store.get(id) else {
            return;
        };
        let alive = entity.living().is_some_and(|living| {
            !rspigot_domain::entity::living::is_dead_or_dying(&entity.base, living)
        });
        let health = f64::from(rspigot_domain::entity::living::health(&entity.base));
        let Some(max) = self
            .entities
            .store
            .get_mut(id)
            .and_then(|entity| entity.living_mut())
            .map(|living| living.max_health())
        else {
            return;
        };
        if !alive || health >= max {
            return;
        }
        let Some(entity) = self.entities.store.get(id) else {
            return;
        };
        let base = &entity.base;
        let pos = base.position.block();
        let in_clouds = happy_ghast::is_in_clouds(
            self.world.dimension.key == rspigot_domain::world::Dimension::overworld().key,
            base.position.y,
            base.dimensions.height,
        );
        let precipitation = rspigot_data::climate::climate(self.biome_at(pos).0)
            .is_some_and(|climate| climate.has_precipitation);
        let interval = happy_ghast::heal_interval(in_clouds || precipitation);
        if base.tick_count % interval == 0 {
            self.heal_mob(
                None,
                id,
                HEAL_AMOUNT,
                EntityRegainHealthRegainReason::Custom,
            );
        }
    }

    pub(crate) fn happy_ghast_custom_ai_step(&mut self, id: EntityId) {
        let vehicle = self.entities.rides.is_vehicle(id);
        let Some(entity) = self.entities.store.get_mut(id) else {
            return;
        };
        let baby = entity.is_baby();
        let leashed = entity.extra.contains_key(LEASH);
        let harnessed = wears_harness(entity);
        let position = entity.base.position.block();
        if let Some((base, state)) = ghast_state(entity)
            && state.baby_seen != baby
        {
            state.baby_seen = baby;
            if baby {
                happy_ghast::set_server_still_timeout(base, state, 0);
            }
        }
        if leashed || vehicle {
            return;
        }
        let radius = happy_ghast::restriction_radius(baby, harnessed);
        let Some(mob) = entity.living_mut().and_then(|living| living.mob.as_mut()) else {
            return;
        };
        let home = (mob.home_radius != -1).then_some((mob.home_pos, mob.home_radius));
        if happy_ghast::home_needs_reset(home, position, radius) {
            mob.home_pos = position;
            mob.home_radius = radius;
        }
    }

    pub(crate) fn happy_ghast_passenger_added(&mut self, vehicle: EntityId, was_vehicle: bool) {
        if !was_vehicle {
            self.entity_sound(vehicle, happy_ghast::GOGGLES_DOWN_SOUND, 1.0, 1.0);
        }
        if self.happy_ghast_scans_player(vehicle) {
            let still = self
                .entities
                .store
                .get(vehicle)
                .and_then(state_of)
                .map_or(0, |state| state.server_still_timeout);
            if still > happy_ghast::STILL_TIMEOUT_TICKS {
                self.happy_ghast_set_still_timeout(vehicle, happy_ghast::STILL_TIMEOUT_TICKS);
            }
        } else {
            self.happy_ghast_set_still_timeout(vehicle, 0);
        }
    }

    pub(crate) fn happy_ghast_passenger_removed(&mut self, vehicle: EntityId) {
        self.happy_ghast_set_still_timeout(vehicle, happy_ghast::STILL_TIMEOUT_TICKS);
        if !self.entities.rides.is_vehicle(vehicle) {
            if let Some(mob) = self
                .entities
                .store
                .get_mut(vehicle)
                .and_then(|entity| entity.living_mut())
                .and_then(|living| living.mob.as_mut())
            {
                mob.home_radius = -1;
            }
            self.entity_sound(vehicle, happy_ghast::GOGGLES_UP_SOUND, 1.0, 1.0);
        }
    }

    pub(crate) fn happy_ghast_on_leash_pull(&mut self, id: EntityId) {
        if let Some(entry) = self.entities.mobs.entries.get_mut(&id) {
            entry.ai.brain_mut().movement.set_wait();
        }
    }

    pub(crate) fn happy_ghast_notify_leash_holder(&mut self, holder: EntityId) {
        if let Some((_, state)) = self.entities.store.get_mut(holder).and_then(ghast_state) {
            state.leash_holder_time = happy_ghast::LEASH_HOLDER_TICKS;
        }
    }

    pub(crate) fn happy_ghast_interact(
        &mut self,
        player: PlayerId,
        target: EntityId,
        hand: Hand,
        held: Option<&ItemStack>,
    ) -> Option<Interaction> {
        let entity = self.entities.store.get(target)?;
        if entity.is_baby() {
            return None;
        }
        let alive = entity.living().is_some_and(|living| {
            !rspigot_domain::entity::living::is_dead_or_dying(&entity.base, living)
        });
        if let Some(stack) = held.filter(|stack| is_harness(stack))
            && alive
            && type_in_tag(entity.base.kind, CAN_EQUIP_HARNESS)
            && !wears_harness(entity)
        {
            let mut one = stack.clone();
            one.count = NonZeroU8::MIN;
            self.consume_held(player, hand, 1);
            self.mount_equip(target, EquipmentSlot::Body, Some(one));
            self.entity_sound(target, happy_ghast::EQUIP_SOUND, 1.0, 1.0);
            if let Some(at) = self.entities.store.get(target).map(|e| e.base.position) {
                let rider = self.players.get(&player).map(|p| p.entity_id);
                self.game_event(GameEvent::Equip, at, GameEventContext::of(rider));
            }
            return Some(Interaction::Predicted);
        }
        let sneaking = self.players.get(&player).is_some_and(|p| p.sneaking);
        if wears_harness(entity) && !sneaking {
            let rider = self.players.get(&player).map(|p| p.entity_id)?;
            self.start_riding(rider, target, false, true);
            return Some(Interaction::Predicted);
        }
        None
    }

    pub(crate) fn happy_ghast_ridden_rotation(&mut self, vehicle: EntityId, yaw: f32, pitch: f32) {
        let Some(entity) = self.entities.store.get_mut(vehicle) else {
            return;
        };
        let (pitch, yaw) = {
            let (pitch, yaw) = happy_ghast::ridden_rotation(pitch, yaw);
            (pitch, happy_ghast::ridden_yaw(entity.base.yaw, yaw))
        };
        entity.base.set_rotation(yaw, pitch);
        entity.base.head_yaw = yaw;
        if let Some(living) = entity.living_mut() {
            living.body_yaw = yaw;
        }
    }
}

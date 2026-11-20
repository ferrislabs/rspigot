use rspigot_ai::TargetReason;
use rspigot_domain::attribute;
use rspigot_domain::entity::living::{self, EQUIPMENT_SLOTS, LivingState};
use rspigot_domain::entity::{Entity, EntityBase, EntityState, ageable, tamable};
use rspigot_domain::entity::{FLAG_INVISIBLE, FLAG_ON_FIRE};
use rspigot_domain::{EntityId, Vec3};
use rspigot_items::EquipmentSlot;

use crate::plugins::edits::{EntityEdit, EquipmentView, LivingDetails};
use crate::plugins::{resolve, spec_of, stack_of};
use crate::ports::ClientLink;
use crate::server::Server;

const MAX_TAGS: usize = 1024;

fn living_of(entity: &mut Entity) -> Result<(&mut EntityBase, &mut LivingState), String> {
    let Entity { base, state, .. } = entity;
    match state {
        EntityState::Living(living) => Ok((base, living)),
        _ => Err("the entity is not a living entity".to_owned()),
    }
}

fn mob_of(entity: &mut Entity) -> Result<(&mut EntityBase, &mut LivingState), String> {
    let (base, living) = living_of(entity)?;
    if living.mob.is_some() {
        Ok((base, living))
    } else {
        Err("the entity is not a mob".to_owned())
    }
}

fn slot_of(name: &str) -> Result<EquipmentSlot, String> {
    EQUIPMENT_SLOTS
        .into_iter()
        .find(|slot| slot.name() == name)
        .ok_or_else(|| format!("{name} is not an equipment slot"))
}

fn not_nan(what: &str, value: f32) -> Result<f32, String> {
    if value.is_finite() {
        Ok(value)
    } else {
        Err(format!("{what} {value} is not a number"))
    }
}

impl<L: ClientLink> Server<L> {
    pub(super) fn edit_entity(&mut self, id: EntityId, edit: EntityEdit) -> Result<(), String> {
        match edit {
            EntityEdit::Mount(vehicle) => {
                if self.entities.store.get(vehicle).is_none()
                    && !self
                        .players
                        .values()
                        .any(|player| player.entity_id == vehicle)
                {
                    return Err("the vehicle is gone".to_owned());
                }
                if self.start_riding(id, vehicle, false, true) {
                    Ok(())
                } else {
                    Err("the entity cannot ride that vehicle".to_owned())
                }
            }
            EntityEdit::Dismount => {
                if !self.is_passenger(id) {
                    return Err("the entity is not riding".to_owned());
                }
                self.stop_riding(id);
                Ok(())
            }
            EntityEdit::EjectPassengers => {
                self.eject_passengers(id);
                Ok(())
            }
            EntityEdit::SetLeashHolder(holder) => self.tie_leash(id, holder),
            EntityEdit::ClearLeash => {
                self.remove_leash(id);
                Ok(())
            }
            EntityEdit::SetTarget(target) => {
                let mob = self.entity_view(id).ok_or("the entity is gone")?;
                let alive = self.living_entity_alive(target);
                if !alive {
                    return Err("the target is not a living entity".to_owned());
                }
                let entity = self
                    .entities
                    .store
                    .get_mut(id)
                    .ok_or("the entity is gone")?;
                mob_of(entity)?;
                self.request_target(mob, target, TargetReason::Unknown);
                Ok(())
            }
            EntityEdit::ClearTarget => {
                let entity = self
                    .entities
                    .store
                    .get_mut(id)
                    .ok_or("the entity is gone")?;
                mob_of(entity)?;
                self.forget_target(id, TargetReason::Unknown);
                Ok(())
            }
            EntityEdit::SetOwner(owner) => {
                let entity = self
                    .entities
                    .store
                    .get_mut(id)
                    .ok_or("the entity is gone")?;
                let (base, living) = mob_of(entity)?;
                if living.mob.as_ref().is_some_and(|mob| mob.tamable.is_none()) {
                    return Err("the entity cannot be tamed".to_owned());
                }
                tamable::tame(base, owner);
                self.species_tamed(id);
                Ok(())
            }
            EntityEdit::ClearOwner => {
                let entity = self
                    .entities
                    .store
                    .get_mut(id)
                    .ok_or("the entity is gone")?;
                let (base, living) = mob_of(entity)?;
                if living.mob.as_ref().is_some_and(|mob| mob.tamable.is_none()) {
                    return Err("the entity cannot be tamed".to_owned());
                }
                tamable::set_owner(base, None);
                Ok(())
            }
            EntityEdit::SetTamed(tamed) => {
                let entity = self
                    .entities
                    .store
                    .get_mut(id)
                    .ok_or("the entity is gone")?;
                let (base, living) = mob_of(entity)?;
                if living.mob.as_ref().is_some_and(|mob| mob.tamable.is_none()) {
                    return Err("the entity cannot be tamed".to_owned());
                }
                tamable::set_tame(base, tamed);
                if tamed {
                    self.species_tamed(id);
                } else {
                    tamable::set_owner(base, None);
                }
                Ok(())
            }
            EntityEdit::SetSitting(sitting) => {
                let entity = self
                    .entities
                    .store
                    .get_mut(id)
                    .ok_or("the entity is gone")?;
                let (_, living) = mob_of(entity)?;
                if living.mob.as_ref().is_some_and(|mob| mob.tamable.is_none()) {
                    return Err("the entity cannot sit".to_owned());
                }
                self.set_sitting_pose(id, sitting);
                self.set_ordered_to_sit(id, sitting);
                Ok(())
            }
            EntityEdit::SetEquipment { slot, item } => {
                let slot = slot_of(&slot)?;
                let stack = match item {
                    Some(spec) => stack_of(&spec)?,
                    None => None,
                };
                let entity = self
                    .entities
                    .store
                    .get_mut(id)
                    .ok_or("the entity is gone")?;
                living_of(entity)?;
                self.set_equipment_stack(id, slot, stack);
                Ok(())
            }
            other => {
                let entity = self
                    .entities
                    .store
                    .get_mut(id)
                    .ok_or("the entity is gone")?;
                Self::edit_state(entity, other)
            }
        }
    }

    fn edit_state(entity: &mut Entity, edit: EntityEdit) -> Result<(), String> {
        match edit {
            EntityEdit::SetCustomName(name) => {
                entity.base.set_custom_name(Some(resolve::text(&name)));
            }
            EntityEdit::ClearCustomName => entity.base.set_custom_name(None),
            EntityEdit::SetCustomNameVisible(visible) => {
                entity.base.set_custom_name_visible(visible);
            }
            EntityEdit::SetGlowing(glowing) => entity.base.set_glowing_tag(glowing),
            EntityEdit::SetInvisible(invisible) => {
                entity.base.persistent_invisibility = invisible;
                entity.base.set_shared_flag(FLAG_INVISIBLE, invisible);
            }
            EntityEdit::SetSilent(silent) => entity.base.set_silent(silent),
            EntityEdit::SetGravity(gravity) => entity.base.set_no_gravity(!gravity),
            EntityEdit::SetInvulnerable(invulnerable) => {
                entity.base.invulnerable = invulnerable;
            }
            EntityEdit::SetPersistent(persistent) => {
                let (_, living) = mob_of(entity)?;
                if let Some(mob) = living.mob.as_mut() {
                    mob.persistence_required = persistent;
                }
            }
            EntityEdit::SetFireTicks(ticks) => {
                let ticks = ticks.max(0);
                entity.base.remaining_fire_ticks = ticks;
                entity.base.set_shared_flag(FLAG_ON_FIRE, ticks > 0);
            }
            EntityEdit::SetFreezeTicks(ticks) => {
                resolve::not_negative("freeze ticks", ticks)?;
                entity.base.set_ticks_frozen(ticks);
            }
            EntityEdit::SetAir(air) => entity.base.set_air_supply(air),
            EntityEdit::SetFallDistance(distance) => {
                entity.base.fall_distance = f64::from(not_nan("fall distance", distance)?);
            }
            EntityEdit::SetRotation { yaw, pitch } => {
                let rotation = resolve::rotation_of(rspigot_domain::Rotation { yaw, pitch })?;
                Self::face(&mut entity.base, rotation.yaw, rotation.pitch);
            }
            EntityEdit::LookAt(target) => {
                let target = resolve::position_of(target)?;
                let base = &mut entity.base;
                let eye = Vec3::new(base.position.x, base.eye_y(), base.position.z);
                let (dx, dy, dz) = (target.x - eye.x, target.y - eye.y, target.z - eye.z);
                let yaw = (-dx).atan2(dz).to_degrees() as f32;
                let pitch = (-dy.atan2(dx.hypot(dz))).to_degrees() as f32;
                Self::face(base, yaw, pitch);
            }
            EntityEdit::SetTicksLived(ticks) => {
                if ticks < 1 {
                    return Err(format!("{ticks} ticks lived is not above 0"));
                }
                entity.base.tick_count = ticks;
            }
            EntityEdit::AddTag(tag) => {
                if !entity.base.tags.contains(&tag) {
                    if entity.base.tags.len() >= MAX_TAGS {
                        return Err(format!("an entity holds at most {MAX_TAGS} tags"));
                    }
                    entity.base.tags.push(tag);
                }
            }
            EntityEdit::RemoveTag(tag) => entity.base.tags.retain(|held| *held != tag),
            EntityEdit::SetHealth(health) => {
                let health = not_nan("health", health)?;
                let (base, living) = living_of(entity)?;
                let max = living.max_health() as f32;
                if !(0.0..=max).contains(&health) {
                    return Err(format!("health {health} is outside 0..={max}"));
                }
                living::set_health(base, living, health);
            }
            EntityEdit::SetAbsorption(absorption) => {
                let absorption = not_nan("absorption", absorption)?;
                if absorption < 0.0 {
                    return Err(format!("absorption {absorption} is negative"));
                }
                living_of(entity)?.1.absorption = absorption;
            }
            EntityEdit::SetAi(ai) => {
                let (base, _) = mob_of(entity)?;
                living::set_no_ai(base, !ai);
            }
            EntityEdit::SetCanPickUpLoot(can) => {
                if let Some(mob) = mob_of(entity)?.1.mob.as_mut() {
                    mob.can_pick_up_loot = can;
                }
            }
            EntityEdit::SetLeftHanded(left) => {
                let (base, _) = mob_of(entity)?;
                living::set_left_handed(base, left);
            }
            EntityEdit::SetDropChance { slot, chance } => {
                let slot = slot_of(&slot)?;
                let chance = not_nan("drop chance", chance)?;
                if chance < 0.0 {
                    return Err(format!("drop chance {chance} is negative"));
                }
                if let Some(mob) = mob_of(entity)?.1.mob.as_mut() {
                    mob.drop_chances[living::slot_index(slot)] = chance;
                }
            }
            EntityEdit::SetAge(age) => {
                let (base, living) = mob_of(entity)?;
                let ageable = living
                    .mob
                    .as_mut()
                    .and_then(|mob| mob.ageable.as_mut())
                    .ok_or("the entity does not age")?;
                ageable::set_age(base, ageable, age);
            }
            EntityEdit::SetAgeLocked(locked) => {
                let (base, living) = mob_of(entity)?;
                if living.mob.as_ref().is_none_or(|mob| mob.ageable.is_none()) {
                    return Err("the entity does not age".to_owned());
                }
                ageable::set_age_locked(base, locked);
            }
            EntityEdit::SetBreeding(breeding) => {
                let (_, living) = mob_of(entity)?;
                let ageable = living
                    .mob
                    .as_mut()
                    .and_then(|mob| mob.ageable.as_mut())
                    .filter(|ageable| ageable.love.is_some())
                    .ok_or("the entity cannot breed")?;
                if breeding {
                    ageable.set_in_love(None);
                } else {
                    ageable.reset_love();
                }
            }
            _ => return Err("this edit needs the server".to_owned()),
        }
        Ok(())
    }

    fn face(base: &mut EntityBase, yaw: f32, pitch: f32) {
        base.set_rotation(yaw, pitch);
        base.head_yaw = base.yaw;
        base.needs_sync = true;
        base.sync_position = true;
    }

    pub(super) fn living_details(&self, id: EntityId) -> Option<Box<LivingDetails>> {
        let entity = self.entities.store.get(id)?;
        let living = entity.living()?;
        let base = &entity.base;
        let mob = living.mob.as_ref();
        let ageable = mob.and_then(|mob| mob.ageable.as_ref());
        let tame = mob.is_some_and(|mob| mob.tamable.is_some());
        let max_health = living
            .attributes
            .state(living::MAX_HEALTH, &living.effects)
            .map_or(0.0, |state| attribute::value(&state)) as f32;
        Some(Box::new(LivingDetails {
            max_health,
            absorption: living.absorption,
            ai: !living::is_no_ai(base),
            can_pick_up_loot: mob.is_some_and(|mob| mob.can_pick_up_loot),
            left_handed: living::is_left_handed(base),
            equipment: EQUIPMENT_SLOTS
                .into_iter()
                .map(|slot| EquipmentView {
                    slot: slot.name(),
                    item: living.equipment(slot).map(spec_of),
                    drop_chance: mob.map_or(living::DEFAULT_DROP_CHANCE, |mob| {
                        mob.drop_chances[living::slot_index(slot)]
                    }),
                })
                .collect(),
            leash_holder: self.leash_holder_of(entity),
            owner: tame.then(|| tamable::owner(base)).flatten(),
            tamed: tame && tamable::is_tame(base),
            sitting: tame && tamable::is_in_sitting_pose(base),
            target: self.mob_target(id),
            age: ageable.map_or(0, |ageable| ageable.age),
            age_locked: ageable.is_some() && ageable::is_age_locked(base),
            baby: entity.is_baby(),
            in_love: ageable.is_some_and(ageable::AgeableState::in_love),
        }))
    }
}

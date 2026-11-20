use rspigot_core::plugins::{EntityDetails, EntityEdit, LivingDetails};
use rspigot_domain::EntityId;
use rspigot_plugin_protocol::v1::{self, entity_edit::Edit as Wire};

use super::Malformed;
use super::shared::{self, required};

pub(super) fn edit(edit: v1::EntityEdit) -> Result<EntityEdit, Malformed> {
    Ok(match required(edit.edit)? {
        Wire::SetCustomName(name) => EntityEdit::SetCustomName(name),
        Wire::ClearCustomName(_) => EntityEdit::ClearCustomName,
        Wire::SetCustomNameVisible(visible) => EntityEdit::SetCustomNameVisible(visible),
        Wire::SetGlowing(flag) => EntityEdit::SetGlowing(flag),
        Wire::SetInvisible(flag) => EntityEdit::SetInvisible(flag),
        Wire::SetSilent(flag) => EntityEdit::SetSilent(flag),
        Wire::SetGravity(flag) => EntityEdit::SetGravity(flag),
        Wire::SetInvulnerable(flag) => EntityEdit::SetInvulnerable(flag),
        Wire::SetPersistent(flag) => EntityEdit::SetPersistent(flag),
        Wire::SetFireTicks(ticks) => EntityEdit::SetFireTicks(ticks),
        Wire::SetFreezeTicks(ticks) => EntityEdit::SetFreezeTicks(ticks),
        Wire::SetAir(air) => EntityEdit::SetAir(air),
        Wire::SetFallDistance(distance) => EntityEdit::SetFallDistance(distance),
        Wire::SetRotation(rotation) => EntityEdit::SetRotation {
            yaw: rotation.yaw,
            pitch: rotation.pitch,
        },
        Wire::LookAt(target) => EntityEdit::LookAt(shared::vec3(target)),
        Wire::SetTicksLived(ticks) => EntityEdit::SetTicksLived(ticks),
        Wire::AddTag(tag) => EntityEdit::AddTag(tag),
        Wire::RemoveTag(tag) => EntityEdit::RemoveTag(tag),
        Wire::Mount(vehicle) => EntityEdit::Mount(EntityId(vehicle)),
        Wire::Dismount(_) => EntityEdit::Dismount,
        Wire::EjectPassengers(_) => EntityEdit::EjectPassengers,
        Wire::SetHealth(health) => EntityEdit::SetHealth(health),
        Wire::SetAbsorption(absorption) => EntityEdit::SetAbsorption(absorption),
        Wire::SetAi(flag) => EntityEdit::SetAi(flag),
        Wire::SetCanPickUpLoot(flag) => EntityEdit::SetCanPickUpLoot(flag),
        Wire::SetLeftHanded(flag) => EntityEdit::SetLeftHanded(flag),
        Wire::SetEquipment(equipment) => EntityEdit::SetEquipment {
            slot: equipment.slot,
            item: equipment.item.map(shared::item),
        },
        Wire::SetDropChance(drop) => EntityEdit::SetDropChance {
            slot: drop.slot,
            chance: drop.chance,
        },
        Wire::SetLeashHolder(holder) => EntityEdit::SetLeashHolder(EntityId(holder)),
        Wire::ClearLeash(_) => EntityEdit::ClearLeash,
        Wire::SetOwner(owner) => EntityEdit::SetOwner(shared::uuid(owner)?),
        Wire::ClearOwner(_) => EntityEdit::ClearOwner,
        Wire::SetTamed(flag) => EntityEdit::SetTamed(flag),
        Wire::SetSitting(flag) => EntityEdit::SetSitting(flag),
        Wire::SetTarget(target) => EntityEdit::SetTarget(EntityId(target)),
        Wire::ClearTarget(_) => EntityEdit::ClearTarget,
        Wire::SetAge(age) => EntityEdit::SetAge(age),
        Wire::SetAgeLocked(flag) => EntityEdit::SetAgeLocked(flag),
        Wire::SetBreeding(flag) => EntityEdit::SetBreeding(flag),
    })
}

fn wire_living(living: LivingDetails) -> v1::LivingExtra {
    v1::LivingExtra {
        max_health: living.max_health,
        absorption: living.absorption,
        ai: living.ai,
        can_pick_up_loot: living.can_pick_up_loot,
        left_handed: living.left_handed,
        equipment: living
            .equipment
            .into_iter()
            .map(|view| v1::EquipmentState {
                slot: view.slot.to_owned(),
                item: view.item.map(shared::wire_item),
                drop_chance: view.drop_chance,
            })
            .collect(),
        leash_holder: living.leash_holder.map(|holder| holder.0),
        owner: living.owner.map(|owner| owner.hyphenated().to_string()),
        tamed: living.tamed,
        sitting: living.sitting,
        target: living.target.map(|target| target.0),
        age: living.age,
        age_locked: living.age_locked,
        baby: living.baby,
        in_love: living.in_love,
    }
}

pub(super) fn wire_details(details: EntityDetails) -> v1::EntityState {
    let flags = details.flags;
    v1::EntityState {
        entity: Some(super::entity::wire_view(details.entity)),
        health: details.health,
        velocity: Some(shared::wire_vec(details.velocity)),
        on_ground: details.on_ground,
        custom_name: details.custom_name,
        fire_ticks: details.fire_ticks,
        custom_name_visible: flags.custom_name_visible,
        glowing: flags.glowing,
        invisible: flags.invisible,
        silent: flags.silent,
        gravity: flags.gravity,
        invulnerable: flags.invulnerable,
        persistent: flags.persistent,
        freeze_ticks: details.freeze_ticks,
        air: details.air,
        fall_distance: details.fall_distance,
        rotation: Some(shared::wire_rotation(details.rotation)),
        ticks_lived: details.ticks_lived,
        tags: details.tags,
        passengers: details.passengers.into_iter().map(|id| id.0).collect(),
        vehicle: details.vehicle.map(|id| id.0),
        living: details.living.map(|living| wire_living(*living)),
    }
}

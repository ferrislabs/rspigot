use rspigot_plugin_protocol::v1::{
    self, answer, entity_edit::Edit, entity_order, from_plugin, question,
};
use uuid::Uuid;

use crate::link::Link;
use crate::{Entity, EntityState, EquipmentSlot, Error, ItemStack, Player};

const SLOTS: [(EquipmentSlot, &str); 8] = [
    (EquipmentSlot::Hand, "mainhand"),
    (EquipmentSlot::OffHand, "offhand"),
    (EquipmentSlot::Head, "head"),
    (EquipmentSlot::Chest, "chest"),
    (EquipmentSlot::Legs, "legs"),
    (EquipmentSlot::Feet, "feet"),
    (EquipmentSlot::Body, "body"),
    (EquipmentSlot::Saddle, "saddle"),
];

fn slot_key(slot: EquipmentSlot) -> &'static str {
    SLOTS
        .iter()
        .find(|(known, _)| *known == slot)
        .map_or("mainhand", |(_, key)| key)
}

fn slot_of(key: &str) -> Option<EquipmentSlot> {
    SLOTS
        .iter()
        .find(|(_, known)| *known == key)
        .map(|(slot, _)| *slot)
}

pub(crate) async fn fetch(link: &Link, entity: i32) -> Result<EntityState, Error> {
    let state = link
        .ask(
            question::Question::Entity(entity),
            "an entity state",
            |body| match body {
                answer::Body::Entity(state) => Some(state),
                _ => None,
            },
        )
        .await?;
    EntityState::from_wire(state, link)
}

async fn entity_at(link: &Link, entity: i32) -> Result<Entity, Error> {
    Ok(fetch(link, entity).await?.entity)
}

macro_rules! groups {
    ($($(#[$meta:meta])* $name:ident),+ $(,)?) => {
        $(
            $(#[$meta])*
            #[derive(Debug, Clone)]
            pub struct $name {
                entity_id: i32,
                link: Link,
            }

            impl $name {
                pub(crate) const fn new(entity_id: i32, link: Link) -> Self {
                    Self { entity_id, link }
                }

                fn edit(&self, edit: Edit) {
                    self.link.order(from_plugin::Body::Entity(v1::EntityOrder {
                        entity: self.entity_id,
                        order: Some(entity_order::Order::Edit(v1::EntityEdit {
                            edit: Some(edit),
                        })),
                    }));
                }

                async fn state(&self) -> Result<EntityState, Error> {
                    fetch(&self.link, self.entity_id).await
                }
            }
        )+
    };
}

groups!(
    EntityNameHandle,
    EntityFlagsHandle,
    EntityVitalsHandle,
    EntityTagsHandle,
    EntityPassengersHandle,
    EntityEquipmentHandle,
    EntityAiHandle,
    EntityLeashHandle,
    EntityOwnerHandle,
    EntityAgeHandle,
);

impl EntityNameHandle {
    pub fn set(&self, text: impl Into<String>) {
        self.edit(Edit::SetCustomName(text.into()));
    }

    pub fn clear(&self) {
        self.edit(Edit::ClearCustomName(v1::Empty {}));
    }

    pub fn set_visible(&self, visible: bool) {
        self.edit(Edit::SetCustomNameVisible(visible));
    }

    pub async fn get(&self) -> Result<Option<String>, Error> {
        Ok(self.state().await?.custom_name)
    }

    pub async fn get_visible(&self) -> Result<bool, Error> {
        Ok(self.state().await?.custom_name_visible)
    }
}

impl EntityFlagsHandle {
    pub fn set_glowing(&self, glowing: bool) {
        self.edit(Edit::SetGlowing(glowing));
    }

    pub fn set_invisible(&self, invisible: bool) {
        self.edit(Edit::SetInvisible(invisible));
    }

    pub fn set_silent(&self, silent: bool) {
        self.edit(Edit::SetSilent(silent));
    }

    pub fn set_gravity(&self, gravity: bool) {
        self.edit(Edit::SetGravity(gravity));
    }

    pub fn set_invulnerable(&self, invulnerable: bool) {
        self.edit(Edit::SetInvulnerable(invulnerable));
    }

    pub fn set_persistent(&self, persistent: bool) {
        self.edit(Edit::SetPersistent(persistent));
    }

    pub async fn get_glowing(&self) -> Result<bool, Error> {
        Ok(self.state().await?.glowing)
    }

    pub async fn get_invisible(&self) -> Result<bool, Error> {
        Ok(self.state().await?.invisible)
    }

    pub async fn get_silent(&self) -> Result<bool, Error> {
        Ok(self.state().await?.silent)
    }

    pub async fn get_gravity(&self) -> Result<bool, Error> {
        Ok(self.state().await?.gravity)
    }

    pub async fn get_invulnerable(&self) -> Result<bool, Error> {
        Ok(self.state().await?.invulnerable)
    }

    pub async fn get_persistent(&self) -> Result<bool, Error> {
        Ok(self.state().await?.persistent)
    }
}

impl EntityVitalsHandle {
    pub fn set_health(&self, health: f32) {
        self.edit(Edit::SetHealth(health));
    }

    pub fn set_absorption(&self, absorption: f32) {
        self.edit(Edit::SetAbsorption(absorption));
    }

    pub fn set_fire_ticks(&self, ticks: i32) {
        self.edit(Edit::SetFireTicks(ticks));
    }

    pub fn set_freeze_ticks(&self, ticks: i32) {
        self.edit(Edit::SetFreezeTicks(ticks));
    }

    pub fn set_air(&self, ticks: i32) {
        self.edit(Edit::SetAir(ticks));
    }

    pub fn set_fall_distance(&self, distance: f32) {
        self.edit(Edit::SetFallDistance(distance));
    }

    pub async fn get_health(&self) -> Result<Option<f32>, Error> {
        Ok(self.state().await?.health)
    }

    pub async fn get_fire_ticks(&self) -> Result<i32, Error> {
        Ok(self.state().await?.fire_ticks)
    }

    pub async fn get_freeze_ticks(&self) -> Result<i32, Error> {
        Ok(self.state().await?.freeze_ticks)
    }

    pub async fn get_air(&self) -> Result<i32, Error> {
        Ok(self.state().await?.air)
    }

    pub async fn get_fall_distance(&self) -> Result<f32, Error> {
        Ok(self.state().await?.fall_distance)
    }
}

impl EntityTagsHandle {
    pub fn add(&self, tag: impl Into<String>) {
        self.edit(Edit::AddTag(tag.into()));
    }

    pub fn remove(&self, tag: impl Into<String>) {
        self.edit(Edit::RemoveTag(tag.into()));
    }

    pub async fn get_all(&self) -> Result<Vec<String>, Error> {
        Ok(self.state().await?.tags)
    }
}

impl EntityPassengersHandle {
    pub fn mount(&self, vehicle: &Entity) {
        self.edit(Edit::Mount(vehicle.entity_id));
    }

    pub fn add(&self, passenger: &Entity) {
        self.link.order(from_plugin::Body::Entity(v1::EntityOrder {
            entity: passenger.entity_id,
            order: Some(entity_order::Order::Edit(v1::EntityEdit {
                edit: Some(Edit::Mount(self.entity_id)),
            })),
        }));
    }

    pub fn dismount(&self) {
        self.edit(Edit::Dismount(v1::Empty {}));
    }

    pub fn eject(&self) {
        self.edit(Edit::EjectPassengers(v1::Empty {}));
    }

    pub async fn get_passengers(&self) -> Result<Vec<Entity>, Error> {
        let state = self.state().await?;
        let mut found = Vec::with_capacity(state.passengers.len());
        for id in state.passengers {
            found.push(entity_at(&self.link, id).await?);
        }
        Ok(found)
    }

    pub async fn get_vehicle(&self) -> Result<Option<Entity>, Error> {
        match self.state().await?.vehicle {
            Some(id) => entity_at(&self.link, id).await.map(Some),
            None => Ok(None),
        }
    }
}

impl EntityEquipmentHandle {
    pub fn set(&self, slot: EquipmentSlot, item: &ItemStack) {
        self.edit(Edit::SetEquipment(v1::EquipmentEdit {
            slot: slot_key(slot).to_owned(),
            item: Some(item.to_wire()),
        }));
    }

    pub fn clear(&self, slot: EquipmentSlot) {
        self.edit(Edit::SetEquipment(v1::EquipmentEdit {
            slot: slot_key(slot).to_owned(),
            item: None,
        }));
    }

    pub fn set_drop_chance(&self, slot: EquipmentSlot, chance: f32) {
        self.edit(Edit::SetDropChance(v1::DropChanceEdit {
            slot: slot_key(slot).to_owned(),
            chance,
        }));
    }

    pub async fn get(&self, slot: EquipmentSlot) -> Result<Option<ItemStack>, Error> {
        let living = self.state().await?.living;
        Ok(living.and_then(|living| {
            living
                .equipment
                .into_iter()
                .find(|held| held.slot == slot)
                .and_then(|held| held.item)
        }))
    }

    pub async fn get_drop_chance(&self, slot: EquipmentSlot) -> Result<Option<f32>, Error> {
        let living = self.state().await?.living;
        Ok(living.and_then(|living| {
            living
                .equipment
                .into_iter()
                .find(|held| held.slot == slot)
                .map(|held| held.drop_chance)
        }))
    }
}

impl EntityAiHandle {
    pub fn set(&self, enabled: bool) {
        self.edit(Edit::SetAi(enabled));
    }

    pub fn set_can_pick_up_loot(&self, can: bool) {
        self.edit(Edit::SetCanPickUpLoot(can));
    }

    pub fn set_left_handed(&self, left: bool) {
        self.edit(Edit::SetLeftHanded(left));
    }

    pub fn set_target(&self, target: &Entity) {
        self.edit(Edit::SetTarget(target.entity_id));
    }

    pub fn set_target_player(&self, target: &Player) {
        self.edit(Edit::SetTarget(target.entity_id));
    }

    pub fn clear_target(&self) {
        self.edit(Edit::ClearTarget(v1::Empty {}));
    }

    pub async fn get_target(&self) -> Result<Option<Entity>, Error> {
        let target = self.state().await?.living.and_then(|living| living.target);
        match target {
            Some(id) => entity_at(&self.link, id).await.map(Some),
            None => Ok(None),
        }
    }
}

impl EntityLeashHandle {
    pub fn set_holder(&self, holder: &Entity) {
        self.edit(Edit::SetLeashHolder(holder.entity_id));
    }

    pub fn set_holder_player(&self, holder: &Player) {
        self.edit(Edit::SetLeashHolder(holder.entity_id));
    }

    pub fn clear(&self) {
        self.edit(Edit::ClearLeash(v1::Empty {}));
    }

    pub async fn get_holder(&self) -> Result<Option<Entity>, Error> {
        let holder = self
            .state()
            .await?
            .living
            .and_then(|living| living.leash_holder);
        match holder {
            Some(id) => entity_at(&self.link, id).await.map(Some),
            None => Ok(None),
        }
    }
}

impl EntityOwnerHandle {
    pub fn set(&self, owner: &Player) {
        self.set_uuid(owner.uuid);
    }

    pub fn set_uuid(&self, owner: Uuid) {
        self.edit(Edit::SetOwner(owner.hyphenated().to_string()));
    }

    pub fn clear(&self) {
        self.edit(Edit::ClearOwner(v1::Empty {}));
    }

    pub fn set_tamed(&self, tamed: bool) {
        self.edit(Edit::SetTamed(tamed));
    }

    pub fn set_sitting(&self, sitting: bool) {
        self.edit(Edit::SetSitting(sitting));
    }

    pub async fn get_owner(&self) -> Result<Option<Uuid>, Error> {
        Ok(self.state().await?.living.and_then(|living| living.owner))
    }

    pub async fn is_tamed(&self) -> Result<bool, Error> {
        Ok(self
            .state()
            .await?
            .living
            .is_some_and(|living| living.tamed))
    }
}

impl EntityAgeHandle {
    pub fn set(&self, ticks: i32) {
        self.edit(Edit::SetAge(ticks));
    }

    pub fn set_locked(&self, locked: bool) {
        self.edit(Edit::SetAgeLocked(locked));
    }

    pub fn set_breeding(&self, breeding: bool) {
        self.edit(Edit::SetBreeding(breeding));
    }

    pub async fn get(&self) -> Result<Option<i32>, Error> {
        Ok(self.state().await?.living.map(|living| living.age))
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct EquipmentState {
    pub slot: EquipmentSlot,
    pub item: Option<ItemStack>,
    pub drop_chance: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LivingState {
    pub max_health: f32,
    pub absorption: f32,
    pub ai: bool,
    pub can_pick_up_loot: bool,
    pub left_handed: bool,
    pub equipment: Vec<EquipmentState>,
    pub leash_holder: Option<i32>,
    pub owner: Option<Uuid>,
    pub tamed: bool,
    pub sitting: bool,
    pub target: Option<i32>,
    pub age: i32,
    pub age_locked: bool,
    pub baby: bool,
    pub in_love: bool,
}

impl LivingState {
    pub(crate) fn from_wire(living: v1::LivingExtra) -> Result<Self, Error> {
        Ok(Self {
            max_health: living.max_health,
            absorption: living.absorption,
            ai: living.ai,
            can_pick_up_loot: living.can_pick_up_loot,
            left_handed: living.left_handed,
            equipment: living
                .equipment
                .into_iter()
                .map(|held| {
                    let slot = slot_of(&held.slot).ok_or_else(|| {
                        Error::Malformed(format!("an unknown equipment slot `{}`", held.slot))
                    })?;
                    Ok(EquipmentState {
                        slot,
                        item: held.item.map(ItemStack::from_wire).transpose()?,
                        drop_chance: held.drop_chance,
                    })
                })
                .collect::<Result<_, Error>>()?,
            leash_holder: living.leash_holder,
            owner: living
                .owner
                .map(|owner| crate::wire::parse_uuid(&owner, "an owner"))
                .transpose()?,
            tamed: living.tamed,
            sitting: living.sitting,
            target: living.target,
            age: living.age,
            age_locked: living.age_locked,
            baby: living.baby,
            in_love: living.in_love,
        })
    }
}

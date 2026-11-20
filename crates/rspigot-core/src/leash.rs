use rspigot_domain::entity::BlockView;
use rspigot_domain::entity::leash::{self, Body};
use rspigot_domain::entity::living;
use rspigot_domain::entity::physics;
use rspigot_domain::entity::save::{read_uuid, uuid_tag};
use rspigot_domain::item::ItemStack;
use rspigot_domain::player::PlayerId;
use rspigot_domain::{EntityId, Vec3};
use rspigot_entities::SoundSource;
use rspigot_items::ItemKind;
use rspigot_nbt::{Compound, Tag};
use uuid::Uuid;

use crate::bucket::slot_of;
use crate::events::{ClientEvent, Hand};
use crate::interact::Interaction;
use crate::plugins::events::{
    EntityUnleash, EntityUnleashReason, PlayerLeashEntity, PlayerUnleashEntity,
};
use crate::plugins::steps::LeashStep;
use crate::plugins::{Continuation, EventKind, EventPayload, Lane};
use crate::ports::ClientLink;
use crate::server::{Audience, Server};
use crate::spawning::LEASH;

const LEAD_TIED_SOUND: &str = "minecraft:item.lead.tied";
const LEAD_UNTIED_SOUND: &str = "minecraft:item.lead.untied";
const LEAD_BREAK_SOUND: &str = "minecraft:item.lead.break";
const PLAYER_WIDTH: f32 = 0.6;
const FALL_DISTANCE_CAP: f64 = 1.0;
const FALL_MOVEMENT_FLOOR: f64 = -0.5;

struct Holder {
    id: EntityId,
    center: Vec3,
    position: Vec3,
    yaw: f32,
    width: f32,
    height: f32,
    movement: Vec3,
    alive: bool,
    kind: rspigot_entities::EntityKind,
}

impl Holder {
    fn body(&self) -> Body {
        Body {
            position: self.position,
            yaw: self.yaw,
            width: self.width,
            height: self.height,
        }
    }
}

fn holder_uuid(extra: &Compound) -> Option<Uuid> {
    match extra.get(LEASH)? {
        Tag::Compound(holder) => read_uuid(holder.get("UUID")?),
        _ => None,
    }
}

impl<L: ClientLink> Server<L> {
    pub(crate) fn leash_holder_of(
        &self,
        entity: &rspigot_domain::entity::Entity,
    ) -> Option<EntityId> {
        let uuid = holder_uuid(&entity.extra)?;
        self.entities.store.by_uuid(uuid).or_else(|| {
            self.players
                .get(&PlayerId(uuid))
                .map(|player| player.entity_id)
        })
    }

    fn resolve_holder(&self, uuid: Uuid) -> Option<Holder> {
        if let Some(id) = self.entities.store.by_uuid(uuid) {
            let other = self.entities.store.get(id)?;
            let alive = other
                .living()
                .is_none_or(|state| !living::is_dead_or_dying(&other.base, state));
            let movement = if other
                .living()
                .is_some_and(|_| living::is_no_ai(&other.base))
            {
                Vec3::ZERO
            } else {
                other.base.velocity
            };
            return Some(Holder {
                id,
                center: other.base.bounding_box().center(),
                position: other.base.position,
                yaw: other.base.yaw,
                width: other.base.dimensions.width,
                height: other.base.dimensions.height,
                movement,
                alive,
                kind: other.base.kind,
            });
        }
        let player = self.players.get(&PlayerId(uuid))?;
        let height = player.height();
        Some(Holder {
            id: player.entity_id,
            center: Vec3::new(
                player.position.x,
                player.position.y + height / 2.0,
                player.position.z,
            ),
            position: player.position,
            yaw: player.rotation.yaw,
            width: PLAYER_WIDTH,
            height: height as f32,
            movement: player.known_movement,
            alive: !player.vitals.is_dead(),
            kind: rspigot_entities::EntityKind::Player,
        })
    }

    pub(crate) fn leashed_to_block(&self, entity: &rspigot_domain::entity::Entity) -> bool {
        matches!(entity.extra.get(LEASH), Some(Tag::IntArray(pos)) if pos.len() == 3)
    }

    pub(crate) fn remove_leash(&mut self, id: EntityId) {
        if let Some(entity) = self.entities.store.get_mut(id) {
            entity.extra.remove(LEASH);
        }
        self.entities.leash_spin.remove(&id);
        if self.entities.leash_links.remove(&id).is_some() {
            self.push(
                Audience::Trackers(id),
                ClientEvent::EntityLink {
                    entity: id,
                    holder: None,
                },
            );
        }
    }

    fn announce_leash(&mut self, id: EntityId, holder: EntityId) {
        if self.entities.leash_links.get(&id) == Some(&holder) {
            return;
        }
        self.entities.leash_links.insert(id, holder);
        self.push(
            Audience::Trackers(id),
            ClientEvent::EntityLink {
                entity: id,
                holder: Some(holder),
            },
        );
    }

    fn notify_unleash(&mut self, id: EntityId, reason: EntityUnleashReason) {
        let lane = self.lane_for(None, id);
        self.fire(
            lane,
            Continuation::Notice(EventKind::EntityUnleash),
            move |server: &Self| {
                let entity = server.subject_view(id)?;
                Some(EventPayload::EntityUnleash(EntityUnleash {
                    entity_type: entity.kind.clone(),
                    entity,
                    reason,
                }))
            },
        );
    }

    pub(crate) fn drop_leash(&mut self, id: EntityId, reason: EntityUnleashReason) {
        self.notify_unleash(id, reason);
        self.release_leash(id);
    }

    fn release_leash(&mut self, id: EntityId) {
        let leashed = self
            .entities
            .store
            .get(id)
            .is_some_and(|entity| entity.extra.contains_key(LEASH));
        self.remove_leash(id);
        if leashed && self.config.rules.mob_drops {
            let height = self
                .entities
                .store
                .get(id)
                .map_or(0.0, |entity| f64::from(entity.base.dimensions.height));
            self.drop_at_location(
                id,
                height,
                ItemStack::new(ItemKind::Lead.id(), std::num::NonZeroU8::MIN),
                Vec3::ZERO,
            );
        }
    }

    pub(crate) fn tick_leash(&mut self, id: EntityId) {
        let Some(entity) = self.entities.store.get(id) else {
            return;
        };
        let Some(uuid) = holder_uuid(&entity.extra) else {
            return;
        };
        let alive = entity
            .living()
            .is_some_and(|state| !living::is_dead_or_dying(&entity.base, state));
        let Some(holder) = self.resolve_holder(uuid) else {
            if entity.base.tick_count > leash::LOOSE_TICKS {
                self.drop_leash(id, EntityUnleashReason::HolderGone);
            }
            return;
        };
        if !alive || !holder.alive {
            self.drop_leash(id, EntityUnleashReason::HolderGone);
            return;
        }
        self.announce_leash(id, holder.id);
        let Some(entity) = self.entities.store.get(id) else {
            return;
        };
        let distance = leash::center_distance(entity.base.bounding_box().center(), holder.center);
        let entity_width = entity.base.dimensions.width;
        let (elastic, snap) = leash::distances(entity.base.kind);
        if leash::snaps_at(snap, distance) {
            self.play_sound(
                None,
                holder.position,
                LEAD_BREAK_SOUND,
                SoundSource::Neutral,
                1.0,
                1.0,
            );
            self.drop_leash(id, EntityUnleashReason::Distance);
            return;
        }
        let kind = entity.base.kind;
        if leash::must_pull_at(elastic, distance, entity_width, holder.width) {
            self.pull_on_leash(id, &holder);
            if kind.extends("Camel") {
                self.camel_on_leash_pull(id);
            }
            if kind == rspigot_entities::EntityKind::HappyGhast {
                self.happy_ghast_on_leash_pull(id);
            }
        }
        if kind == rspigot_entities::EntityKind::HappyGhast
            && holder.kind == rspigot_entities::EntityKind::HappyGhast
        {
            self.happy_ghast_notify_leash_holder(holder.id);
        }
        self.spin_on_leash(id);
    }

    fn pull_on_leash(&mut self, id: EntityId, holder: &Holder) {
        let Some(entity) = self.entities.store.get(id) else {
            return;
        };
        let own = Body {
            position: entity.base.position,
            yaw: entity.base.yaw,
            width: entity.base.dimensions.width,
            height: entity.base.dimensions.height,
        };
        let movement = entity.base.velocity;
        let elastic = leash::distances(entity.base.kind).0;
        let Some(pull) = leash::elastic_pull_at(elastic, &own, &holder.body(), movement) else {
            return;
        };
        let push = leash::impulse(&pull, holder.movement, movement);
        if let Some(entity) = self.entities.store.get_mut(id) {
            entity.base.velocity = entity.base.velocity + push;
            entity.base.needs_sync = true;
            if entity.base.velocity.y > FALL_MOVEMENT_FLOOR
                && entity.base.fall_distance > FALL_DISTANCE_CAP
            {
                entity.base.fall_distance = FALL_DISTANCE_CAP;
            }
        }
        *self.entities.leash_spin.entry(id).or_insert(0.0) += leash::TORQUE_FACTOR * pull.torque;
    }

    fn spin_on_leash(&mut self, id: EntityId) {
        let spin = self.entities.leash_spin.get(&id).copied().unwrap_or(0.0);
        let Some(entity) = self.entities.store.get(id) else {
            return;
        };
        let below = physics::movement_affecting_pos(&entity.base, &self.world.chunks);
        let friction = physics::friction(self.world.chunks.block_or_air(below));
        let friction = leash::angular_friction(
            entity.base.on_ground,
            friction,
            entity.base.fluid.in_liquid(),
        );
        if let Some(entity) = self.entities.store.get_mut(id) {
            let yaw = (f64::from(entity.base.yaw) - spin) as f32;
            entity.base.set_yaw(yaw);
        }
        self.entities
            .leash_spin
            .insert(id, spin * f64::from(friction));
    }
}

fn is_enemy(kind: rspigot_entities::EntityKind) -> bool {
    [
        "Monster",
        "Slime",
        "Ghast",
        "Phantom",
        "Hoglin",
        "Shulker",
        "EnderDragon",
    ]
    .iter()
    .any(|class| kind.extends(class))
}

impl<L: ClientLink> Server<L> {
    fn leash_target_alive(&self, target: EntityId) -> bool {
        self.entities.store.get(target).is_some_and(|entity| {
            !entity.base.is_removed()
                && entity.living().is_some_and(|state| {
                    state.is_mob() && !living::is_dead_or_dying(&entity.base, state)
                })
        })
    }

    fn can_attach_leash(&self, target: EntityId, player: PlayerId) -> bool {
        let (Some(entity), Some(owner)) =
            (self.entities.store.get(target), self.players.get(&player))
        else {
            return false;
        };
        if is_enemy(entity.base.kind) {
            return false;
        }
        let height = owner.height();
        let center = Vec3::new(
            owner.position.x,
            owner.position.y + height / 2.0,
            owner.position.z,
        );
        let distance = leash::center_distance(entity.base.bounding_box().center(), center);
        !leash::snaps_at(leash::distances(entity.base.kind).1, distance)
    }

    fn resend_link(&mut self, player: PlayerId, target: EntityId) {
        let holder = self
            .entities
            .store
            .get(target)
            .and_then(|entity| self.leash_holder_of(entity));
        self.push(
            Audience::One(player),
            ClientEvent::EntityLink {
                entity: target,
                holder,
            },
        );
    }

    pub(crate) fn leash_interact(
        &mut self,
        player: PlayerId,
        target: EntityId,
        hand: Hand,
        held: Option<&ItemStack>,
    ) -> Option<Interaction> {
        if !self.leash_target_alive(target) {
            return None;
        }
        let entity = self.entities.store.get(target)?;
        let holder = holder_uuid(&entity.extra);
        if holder == Some(player.0) {
            return Some(self.ask_unleash(player, target, hand, held.cloned()));
        }
        let stack = held?;
        if stack.item != ItemKind::Lead.id() {
            return None;
        }
        let held_by_player = holder.is_some_and(|uuid| self.players.contains_key(&PlayerId(uuid)));
        if held_by_player || !self.can_attach_leash(target, player) {
            return None;
        }
        Some(self.ask_leash(player, target, hand, stack.clone()))
    }

    fn ask_unleash(
        &mut self,
        player: PlayerId,
        target: EntityId,
        hand: Hand,
        stack: Option<ItemStack>,
    ) -> Interaction {
        let mut then = Continuation::Unleash(Box::new(LeashStep {
            player,
            target,
            hand,
            stack,
            parked: false,
        }));
        let parked = self.parks(Lane::Player(player), &then);
        if let Continuation::Unleash(step) = &mut then {
            step.parked = parked;
        }
        self.fire(player, then, move |server| {
            let entity = server.entity_view(target)?;
            Some(EventPayload::PlayerUnleashEntity(PlayerUnleashEntity {
                entity_type: entity.kind.clone(),
                entity,
                hand: slot_of(hand),
                player: server.snapshot_of(player)?,
                reason: EntityUnleashReason::PlayerUnleash,
            }))
        });
        if parked {
            Interaction::Consume
        } else {
            Interaction::Predicted
        }
    }

    fn ask_leash(
        &mut self,
        player: PlayerId,
        target: EntityId,
        hand: Hand,
        stack: ItemStack,
    ) -> Interaction {
        let holder = self.players.get(&player).map(|owner| owner.entity_id);
        let mut then = Continuation::Leash(Box::new(LeashStep {
            player,
            target,
            hand,
            stack: Some(stack),
            parked: false,
        }));
        let parked = self.parks(Lane::Player(player), &then);
        if let Continuation::Leash(step) = &mut then {
            step.parked = parked;
        }
        self.fire(player, then, move |server| {
            Some(EventPayload::PlayerLeashEntity(PlayerLeashEntity {
                entity: server.entity_view(target)?,
                hand: slot_of(hand),
                leash_holder: server.subject_view(holder?)?,
                player: server.snapshot_of(player)?,
            }))
        });
        if parked {
            Interaction::Consume
        } else {
            Interaction::ServerOnly
        }
    }

    pub(crate) fn finish_unleash(&mut self, step: LeashStep, cancelled: bool) {
        let LeashStep {
            player,
            target,
            hand,
            parked,
            ..
        } = step;
        if cancelled {
            self.resend_link(player, target);
            return;
        }
        let still = self
            .entities
            .store
            .get(target)
            .is_some_and(|entity| holder_uuid(&entity.extra) == Some(player.0));
        if !still || !self.leash_target_alive(target) {
            return;
        }
        if self.infinite_materials(player) {
            self.remove_leash(target);
        } else {
            self.release_leash(target);
        }
        self.entity_sound(target, LEAD_UNTIED_SOUND, 1.0, 1.0);
        if parked {
            self.player_swing(player, hand, false);
            self.refresh_hands(player);
        }
    }

    pub(crate) fn finish_leash(&mut self, step: LeashStep, cancelled: bool) {
        let LeashStep {
            player,
            target,
            hand,
            stack,
            parked,
        } = step;
        if cancelled {
            self.send_inventory(player);
            self.resend_link(player, target);
            return;
        }
        let Some(holder) = self.players.get(&player).map(|owner| owner.entity_id) else {
            return;
        };
        if !self.leash_target_alive(target) || self.held(player, hand) != stack {
            return;
        }
        if self
            .entities
            .store
            .get(target)
            .is_some_and(|entity| entity.extra.contains_key(LEASH))
        {
            self.release_leash(target);
        }
        if let Some(entity) = self.entities.store.get_mut(target) {
            let mut link = Compound::new();
            link.insert("UUID", uuid_tag(player.0));
            entity.extra.insert(LEASH, link);
        }
        self.announce_leash(target, holder);
        self.entity_sound(target, LEAD_TIED_SOUND, 1.0, 1.0);
        self.consume_held(player, hand, 1);
        if parked {
            self.player_swing(player, hand, true);
            self.refresh_hands(player);
        }
    }
}

impl<L: ClientLink> Server<L> {
    pub(crate) fn tie_leash(&mut self, id: EntityId, holder: EntityId) -> Result<(), String> {
        if id == holder {
            return Err("an entity cannot hold its own leash".to_owned());
        }
        if !self.leash_target_alive(id) {
            return Err("only a living mob can be leashed".to_owned());
        }
        let uuid = self
            .entities
            .store
            .get(holder)
            .map(|entity| entity.base.uuid)
            .or_else(|| {
                self.players
                    .values()
                    .find(|player| player.entity_id == holder)
                    .map(|player| player.id.0)
            })
            .ok_or_else(|| "the leash holder is gone".to_owned())?;
        self.remove_leash(id);
        if let Some(entity) = self.entities.store.get_mut(id) {
            let mut link = Compound::new();
            link.insert("UUID", uuid_tag(uuid));
            entity.extra.insert(LEASH, link);
        }
        self.announce_leash(id, holder);
        Ok(())
    }
}

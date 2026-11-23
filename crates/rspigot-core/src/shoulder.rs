use rspigot_domain::entity::species::parrot;
use rspigot_domain::entity::{EntityState, RemovalReason, tamable};
use rspigot_domain::player::{GameMode, PlayerId};
use rspigot_domain::{EntityId, Vec3};
use rspigot_entities::EntityKind as Kind;
use rspigot_nbt::Compound;

use rspigot_domain::random::RandomSource;
use rspigot_entities::SoundSource;

use crate::events::{MetadataEntry, MetadataValue};
use crate::mobs::ServerRandom;
use crate::plugins::events::{CreatureSpawnReason, EntityRemoveCause};
use crate::plugins::steps::Birth;
use crate::ports::ClientLink;
use crate::server::Server;

pub(crate) const SHOULDER_LEFT_INDEX: u8 = 19;
pub(crate) const SHOULDER_RIGHT_INDEX: u8 = 20;
pub(crate) const RIDE_COOLDOWN: i32 = 100;
pub(crate) const SIT_DELAY: i64 = 20;
const FALL_LIMIT: f64 = 0.5;
const SHOULDER_SOUND_ODDS: i32 = 200;
const PLAYER_WIDTH: f64 = 0.6;

#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct Shoulders {
    pub(crate) left: Option<Compound>,
    pub(crate) right: Option<Compound>,
    pub(crate) since: i64,
}

fn is_vocal_parrot(shoulder: &Compound) -> bool {
    shoulder.get_str("id") == Some(Kind::Parrot.name())
        && !shoulder.get_bool("Silent").unwrap_or(false)
}

fn variant_of(shoulder: &Option<Compound>) -> Option<u32> {
    let tag = shoulder.as_ref()?;
    if tag.get_str("id") != Some(Kind::Parrot.name()) {
        return None;
    }
    let variant = tag
        .get_i32("Variant")
        .unwrap_or(0)
        .clamp(0, parrot::VARIANTS.len() as i32 - 1);
    Some(variant as u32)
}

pub(crate) fn shoulder_entries(shoulders: &Shoulders) -> Vec<MetadataEntry> {
    [
        (SHOULDER_LEFT_INDEX, &shoulders.left),
        (SHOULDER_RIGHT_INDEX, &shoulders.right),
    ]
    .into_iter()
    .filter_map(|(index, shoulder)| {
        variant_of(shoulder).map(|variant| MetadataEntry {
            index,
            value: MetadataValue::OptionalUnsignedInt(Some(variant)),
        })
    })
    .collect()
}

impl<L: ClientLink> Server<L> {
    pub(crate) fn tick_shoulders(&mut self) {
        let ids: Vec<PlayerId> = self.players.keys().copied().collect();
        for id in ids {
            self.tick_player_shoulders(id);
        }
    }

    fn tick_player_shoulders(&mut self, id: PlayerId) {
        let in_water = self.feet_in_water(id);
        let Some(player) = self.players.get(&id) else {
            return;
        };
        let carrying = player.shoulders.left.is_some() || player.shoulders.right.is_some();
        let drops = player.vitals.fall_distance > FALL_LIMIT
            || in_water
            || player.flying
            || player.game_mode == GameMode::Spectator
            || player.sleeping.is_some();
        let singing = [
            player.shoulders.left.as_ref(),
            player.shoulders.right.as_ref(),
        ]
        .map(|shoulder| shoulder.is_some_and(is_vocal_parrot));
        for _ in singing.into_iter().filter(|vocal| *vocal) {
            self.shoulder_parrot_sound(id);
        }
        if carrying && drops {
            self.remove_entities_on_shoulder(id);
        }
        self.parrots_land_on_owner(id, in_water);
    }

    fn shoulder_parrot_sound(&mut self, id: PlayerId) {
        if ServerRandom(&mut self.rng).next_int(SHOULDER_SOUND_ODDS) != 0 {
            return;
        }
        let Some(player) = self.players.get(&id) else {
            return;
        };
        let position = player.position;
        let speaker = player.entity_id;
        let bounds = rspigot_domain::entity::Aabb::of_size(
            position,
            PLAYER_WIDTH as f32,
            player.height() as f32,
        );
        if self.imitate_nearby_mobs(speaker, bounds, position, SoundSource::Players) {
            return;
        }
        let sound = self.parrot_ambient_sound_for();
        let mut random = ServerRandom(&mut self.rng);
        let pitch = (random.next_float() - random.next_float()) * 0.2 + 1.0;
        self.play_sound(None, position, sound, SoundSource::Players, 1.0, pitch);
    }

    fn parrots_land_on_owner(&mut self, id: PlayerId, in_water: bool) {
        let Some(player) = self.players.get(&id) else {
            return;
        };
        let may_sit = player.game_mode != GameMode::Spectator
            && !player.flying
            && !in_water
            && player.on_ground
            && !self.entities.rides.is_passenger(player.entity_id)
            && (player.shoulders.left.is_none() || player.shoulders.right.is_none());
        if !may_sit {
            return;
        }
        let owner = player.profile.id.0;
        let body = rspigot_domain::entity::Aabb::of_size(
            player.position,
            PLAYER_WIDTH as f32,
            player.height() as f32,
        );
        let landing: Option<EntityId> = self
            .entities
            .store
            .iter()
            .filter(|entity| entity.base.kind == Kind::Parrot && !entity.base.is_removed())
            .filter(|entity| {
                tamable::owner(&entity.base) == Some(owner)
                    && !tamable::is_in_sitting_pose(&entity.base)
                    && entity.base.tick_count > RIDE_COOLDOWN
                    && self.leash_holder_of(entity).is_none()
                    && entity.base.bounding_box().intersects(&body)
                    && matches!(entity.state, EntityState::Living(_))
            })
            .map(|entity| entity.base.id)
            .next();
        if let Some(parrot) = landing {
            self.parrot_lands_on_shoulder(id, parrot);
        }
    }

    fn parrot_lands_on_shoulder(&mut self, player: PlayerId, parrot: EntityId) {
        let Some(nbt) = self.saved_entity(parrot) else {
            return;
        };
        if !self.set_entity_on_shoulder(player, nbt) {
            return;
        }
        self.remove_entity(
            parrot,
            RemovalReason::Discarded,
            Some(EntityRemoveCause::Pickup),
        );
        self.broadcast_metadata(player);
    }

    pub(crate) fn set_entity_on_shoulder(&mut self, id: PlayerId, nbt: Compound) -> bool {
        let now = self.world.level.game_time;
        let in_water = self.feet_in_water(id);
        let Some(player) = self.players.get_mut(&id) else {
            return false;
        };
        if in_water || !player.on_ground {
            return false;
        }
        if player.shoulders.left.is_none() {
            player.shoulders.left = Some(nbt);
        } else if player.shoulders.right.is_none() {
            player.shoulders.right = Some(nbt);
        } else {
            return false;
        }
        player.shoulders.since = now;
        true
    }

    pub(crate) fn remove_entities_on_shoulder(&mut self, id: PlayerId) {
        let now = self.world.level.game_time;
        let Some(player) = self.players.get_mut(&id) else {
            return;
        };
        if player.shoulders.since + SIT_DELAY >= now {
            return;
        }
        let at = player.position;
        let left = player.shoulders.left.take();
        let right = player.shoulders.right.take();
        for shoulder in [left, right].into_iter().flatten() {
            self.respawn_shoulder_entity(at, shoulder);
        }
        self.broadcast_metadata(id);
    }

    fn respawn_shoulder_entity(&mut self, at: Vec3, nbt: Compound) {
        let Some(kind) = nbt.get_str("id").and_then(Kind::from_name) else {
            return;
        };
        let Ok((mut entity, _)) = self.summon_entity(
            kind,
            at,
            Some(nbt),
            rspigot_domain::spawn::SpawnReason::Load,
        ) else {
            return;
        };
        entity.base.tick_count = 0;
        self.spawn_entity(entity, Birth::of(CreatureSpawnReason::ShoulderEntity));
    }
}

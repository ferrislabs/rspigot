use std::collections::BTreeMap;

use rspigot_domain::item::ItemStack;
use rspigot_domain::npc::{NpcKey, NpcName, SkinTextures};
use rspigot_domain::player::{GameMode, GameProfile, PlayerId, ProfileProperty};
use rspigot_domain::{EntityId, Rotation, Vec3};
use rustc_hash::FxHashMap;
use uuid::Uuid;

use crate::events::{
    ClientEvent, EquipmentSlot, MetadataEntry, MetadataValue, PlayerListEntry, Pose,
};
use crate::ports::ClientLink;
use crate::server::{Audience, Server};

pub(crate) const SKIN_LAYERS_INDEX: u8 = 16;
pub(crate) const POSE_INDEX: u8 = 6;
pub(crate) const EYE_HEIGHT: f64 = 1.62;
pub(crate) const MAX_NPCS_PER_PLUGIN: usize = 256;

#[derive(Debug, Clone)]
pub(crate) struct Npc {
    pub id: EntityId,
    pub uuid: Uuid,
    pub owner: String,
    pub key: NpcKey,
    pub name: NpcName,
    pub skin: Option<SkinTextures>,
    pub skin_layers: u8,
    pub position: Vec3,
    pub rotation: Rotation,
    pub pose: Pose,
    pub equipment: Vec<(EquipmentSlot, ItemStack)>,
    pub gaze_range: f64,
}

impl Npc {
    pub(crate) fn list_entry(&self) -> PlayerListEntry {
        PlayerListEntry {
            profile: GameProfile {
                id: PlayerId(self.uuid),
                name: self.name.as_str().to_owned(),
                properties: self
                    .skin
                    .iter()
                    .map(|textures| ProfileProperty {
                        name: "textures".to_owned(),
                        value: textures.value.clone(),
                        signature: Some(textures.signature.clone()),
                    })
                    .collect(),
            },
            game_mode: GameMode::Survival,
            latency_ms: 0,
            listed: false,
            display_name: None,
        }
    }

    pub(crate) fn metadata(&self) -> Vec<MetadataEntry> {
        vec![
            MetadataEntry {
                index: POSE_INDEX,
                value: MetadataValue::Pose(self.pose),
            },
            MetadataEntry {
                index: SKIN_LAYERS_INDEX,
                value: MetadataValue::Byte(i8::from_ne_bytes([self.skin_layers])),
            },
        ]
    }

    pub(crate) fn spawn_events(&self) -> Vec<ClientEvent> {
        let mut events = vec![
            ClientEvent::PlayerInfo(vec![self.list_entry()]),
            ClientEvent::SpawnEntity {
                entity: self.id,
                uuid: self.uuid,
                kind: crate::events::EntityKind::Player,
                position: self.position,
                rotation: self.rotation,
                head_yaw: self.rotation.yaw,
                data: 0,
                velocity: Vec3::ZERO,
            },
            ClientEvent::EntityHeadRotation {
                entity: self.id,
                yaw: self.rotation.yaw,
            },
            ClientEvent::EntityMetadata {
                entity: self.id,
                entries: self.metadata(),
            },
        ];
        if !self.equipment.is_empty() {
            events.push(ClientEvent::EntityEquipment {
                entity: self.id,
                slots: self
                    .equipment
                    .iter()
                    .map(|(slot, stack)| (*slot, Some(stack.clone())))
                    .collect(),
            });
        }
        events
    }

    pub(crate) fn despawn_events(&self) -> Vec<ClientEvent> {
        vec![
            ClientEvent::RemoveEntities(vec![self.id]),
            ClientEvent::PlayerInfoRemove(vec![PlayerId(self.uuid)]),
        ]
    }

    pub(crate) fn eye(&self) -> Vec3 {
        Vec3::new(
            self.position.x,
            self.position.y + EYE_HEIGHT,
            self.position.z,
        )
    }
}

pub(crate) fn view_of(npc: &Npc) -> crate::plugins::EntityView {
    crate::plugins::EntityView {
        id: npc.id,
        uuid: npc.uuid,
        kind: rspigot_entities::EntityKind::Player.name().to_owned(),
        position: npc.position,
        player: None,
    }
}

pub(crate) fn bounds_of(npc: &Npc) -> rspigot_domain::entity::Aabb {
    let height = match npc.pose {
        Pose::Crouching => 1.5,
        Pose::Sleeping => 0.2,
        Pose::Swimming | Pose::FallFlying | Pose::SpinAttack => 0.6,
        _ => 1.8,
    };
    rspigot_domain::entity::Aabb::of_size(npc.position, 0.6, height)
}

pub(crate) fn rotation_toward(from: Vec3, to: Vec3, current: Rotation) -> Rotation {
    let (dx, dy, dz) = (to.x - from.x, to.y - from.y, to.z - from.z);
    let flat = dx.hypot(dz);
    if flat < 1.0e-6 && dy.abs() < 1.0e-6 {
        return current;
    }
    let yaw = (dz.atan2(dx).to_degrees() - 90.0) as f32;
    let pitch = (-dy.atan2(flat).to_degrees()) as f32;
    Rotation::new(wrap_degrees(yaw), pitch.clamp(-90.0, 90.0))
}

fn wrap_degrees(angle: f32) -> f32 {
    let wrapped = angle.rem_euclid(360.0);
    if wrapped >= 180.0 {
        wrapped - 360.0
    } else {
        wrapped
    }
}

#[derive(Debug, Default)]
pub(crate) struct Npcs {
    by_id: BTreeMap<EntityId, Npc>,
    by_key: FxHashMap<(String, NpcKey), EntityId>,
}

impl Npcs {
    pub(crate) fn insert(&mut self, npc: Npc) {
        self.by_key
            .insert((npc.owner.clone(), npc.key.clone()), npc.id);
        self.by_id.insert(npc.id, npc);
    }

    pub(crate) fn remove(&mut self, id: EntityId) -> Option<Npc> {
        let npc = self.by_id.remove(&id)?;
        self.by_key.remove(&(npc.owner.clone(), npc.key.clone()));
        Some(npc)
    }

    pub(crate) fn get(&self, id: EntityId) -> Option<&Npc> {
        self.by_id.get(&id)
    }

    pub(crate) fn get_mut(&mut self, id: EntityId) -> Option<&mut Npc> {
        self.by_id.get_mut(&id)
    }

    pub(crate) fn find(&self, owner: &str, key: &NpcKey) -> Option<&Npc> {
        let id = self.by_key.get(&(owner.to_owned(), key.clone()))?;
        self.by_id.get(id)
    }

    pub(crate) fn count_of(&self, owner: &str) -> usize {
        self.by_id.values().filter(|npc| npc.owner == owner).count()
    }

    pub(crate) fn name_taken(&self, name: &str) -> bool {
        self.by_id
            .values()
            .any(|npc| npc.name.as_str().eq_ignore_ascii_case(name))
    }

    pub(crate) fn iter(&self) -> impl Iterator<Item = &Npc> {
        self.by_id.values()
    }

    pub(crate) fn ids(&self) -> Vec<EntityId> {
        self.by_id.keys().copied().collect()
    }
}

impl<L: ClientLink> Server<L> {
    fn npc_sees(&self, viewer: PlayerId, npc: &Npc) -> bool {
        let range = self.config.player_tracking_range * self.config.player_tracking_range;
        self.players.get(&viewer).is_some_and(|player| {
            player.dim == self.world.dimension.key
                && player.position.distance_squared(npc.position) <= range
                && player.streamer.has_sent(npc.position.chunk())
        })
    }

    pub(crate) fn update_npc_viewers(&mut self) {
        for id in self.entities.npcs.ids() {
            self.update_npc_viewers_of(id);
        }
    }

    pub(crate) fn update_npc_viewers_of(&mut self, id: EntityId) {
        let Some(npc) = self.entities.npcs.get(id).cloned() else {
            return;
        };
        let viewers: Vec<PlayerId> = self.players.keys().copied().collect();
        for viewer in viewers {
            let visible = self.npc_sees(viewer, &npc);
            let Some(player) = self.players.get_mut(&viewer) else {
                continue;
            };
            let tracking = player.tracked.contains(&id);
            if visible && !tracking {
                player.tracked.insert(id);
                for event in npc.spawn_events() {
                    player.link.send(event);
                }
            } else if !visible && tracking {
                player.tracked.remove(&id);
                for event in npc.despawn_events() {
                    player.link.send(event);
                }
            }
        }
    }

    pub(crate) fn tick_npc_gazes(&mut self) {
        let watchers: Vec<(EntityId, Vec3, f64, Rotation)> = self
            .entities
            .npcs
            .iter()
            .filter(|npc| npc.gaze_range > 0.0)
            .map(|npc| (npc.id, npc.eye(), npc.gaze_range, npc.rotation))
            .collect();
        for (id, eye, range, rotation) in watchers {
            let nearest = self
                .players
                .values()
                .filter(|player| player.dim == self.world.dimension.key && !player.vitals.removed)
                .map(|player| {
                    let target = Vec3::new(
                        player.position.x,
                        player.position.y + EYE_HEIGHT,
                        player.position.z,
                    );
                    (eye.distance_squared(target), target)
                })
                .filter(|(distance, _)| *distance <= range * range)
                .min_by(|a, b| a.0.total_cmp(&b.0));
            if let Some((_, target)) = nearest {
                let facing = rotation_toward(eye, target, rotation);
                if facing != rotation {
                    self.turn_npc(id, facing);
                }
            }
        }
    }

    pub(crate) fn turn_npc(&mut self, id: EntityId, rotation: Rotation) {
        let Some(npc) = self.entities.npcs.get_mut(id) else {
            return;
        };
        npc.rotation = rotation;
        let position = npc.position;
        self.push(
            Audience::Trackers(id),
            ClientEvent::EntityMoved {
                entity: id,
                from: position,
                to: position,
                rotation: Some(rotation),
                on_ground: true,
            },
        );
        self.push(
            Audience::Trackers(id),
            ClientEvent::EntityHeadRotation {
                entity: id,
                yaw: rotation.yaw,
            },
        );
    }

    pub(crate) fn move_npc(&mut self, id: EntityId, to: Vec3) {
        let Some(npc) = self.entities.npcs.get_mut(id) else {
            return;
        };
        let from = npc.position;
        npc.position = to;
        let rotation = npc.rotation;
        self.push(
            Audience::Trackers(id),
            ClientEvent::EntityMoved {
                entity: id,
                from,
                to,
                rotation: Some(rotation),
                on_ground: true,
            },
        );
        self.update_npc_viewers_of(id);
    }

    pub(crate) fn drop_npc(&mut self, id: EntityId) {
        let Some(npc) = self.entities.npcs.remove(id) else {
            return;
        };
        for player in self.players.values_mut() {
            if player.tracked.remove(&id) {
                for event in npc.despawn_events() {
                    player.link.send(event);
                }
            }
        }
    }

    pub(crate) fn npc_metadata_changed(&mut self, id: EntityId) {
        let Some(npc) = self.entities.npcs.get(id) else {
            return;
        };
        let entries = npc.metadata();
        self.push(
            Audience::Trackers(id),
            ClientEvent::EntityMetadata {
                entity: id,
                entries,
            },
        );
    }
}

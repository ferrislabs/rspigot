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
pub(crate) const WALK_SPEED: f64 = 4.317;
pub(crate) const MAX_WALK_SPEED: f64 = 20.0;
const NPC_WIDTH: f32 = 0.6;
const NPC_HEIGHT: f32 = 1.8;
pub(crate) const LABEL_OFFSET: f64 = 2.35;
const BILLBOARD_INDEX: u8 = 15;
const TEXT_INDEX: u8 = 23;
const BILLBOARD_CENTER: i8 = 3;

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
    pub label: Option<String>,
    pub label_id: EntityId,
    pub hidden: rustc_hash::FxHashSet<PlayerId>,
    pub walk: Option<Walk>,
}

#[derive(Debug, Clone)]
pub(crate) struct Walk {
    pub waypoints: Vec<Vec3>,
    pub next: usize,
    pub step: f64,
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

    pub(crate) fn label_uuid(&self) -> Uuid {
        Uuid::from_u128(self.uuid.as_u128().wrapping_add(1))
    }

    pub(crate) fn label_position(&self) -> Vec3 {
        Vec3::new(
            self.position.x,
            self.position.y + LABEL_OFFSET,
            self.position.z,
        )
    }

    pub(crate) fn label_spawn_events(&self) -> Vec<ClientEvent> {
        let Some(label) = &self.label else {
            return Vec::new();
        };
        vec![
            ClientEvent::SpawnEntity {
                entity: self.label_id,
                uuid: self.label_uuid(),
                kind: crate::entities::wire_kind(rspigot_entities::EntityKind::TextDisplay),
                position: self.label_position(),
                rotation: Rotation::new(0.0, 0.0),
                head_yaw: 0.0,
                data: 0,
                velocity: Vec3::ZERO,
            },
            self.label_metadata_event(label),
        ]
    }

    pub(crate) fn label_metadata_event(&self, label: &str) -> ClientEvent {
        ClientEvent::EntityMetadata {
            entity: self.label_id,
            entries: vec![
                MetadataEntry {
                    index: BILLBOARD_INDEX,
                    value: MetadataValue::Byte(BILLBOARD_CENTER),
                },
                MetadataEntry {
                    index: TEXT_INDEX,
                    value: MetadataValue::Component(
                        rspigot_domain::text::Component::from_section_codes(label),
                    ),
                },
            ],
        }
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
        events.extend(self.label_spawn_events());
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
        let mut gone = vec![self.id];
        if self.label.is_some() {
            gone.push(self.label_id);
        }
        vec![
            ClientEvent::RemoveEntities(gone),
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
        meta: crate::plugins::MetaSnapshot::default(),
        id: npc.id,
        uuid: npc.uuid,
        kind: rspigot_entities::EntityKind::Player.name().to_owned(),
        position: npc.position,
        player: None,
        marks: Default::default(),
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
        !npc.hidden.contains(&viewer)
            && self.players.get(&viewer).is_some_and(|player| {
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

    pub(crate) fn set_npc_hidden(&mut self, id: EntityId, viewer: PlayerId, hidden: bool) {
        let Some(npc) = self.entities.npcs.get_mut(id) else {
            return;
        };
        if hidden {
            npc.hidden.insert(viewer);
        } else {
            npc.hidden.remove(&viewer);
        }
        self.update_npc_viewers_of(id);
    }

    pub(crate) fn tick_npc_gazes(&mut self) {
        let watchers: Vec<(EntityId, Vec3, f64, Rotation)> = self
            .entities
            .npcs
            .iter()
            .filter(|npc| npc.gaze_range > 0.0 && npc.walk.is_none())
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

    pub(crate) fn start_npc_walk(
        &mut self,
        id: EntityId,
        target: Vec3,
        speed: f64,
    ) -> Result<(), String> {
        use rspigot_ai::{MobProfile, MobState, PathFinder};

        let speed = if speed == 0.0 { WALK_SPEED } else { speed };
        if !(speed.is_finite() && speed > 0.0 && speed <= MAX_WALK_SPEED) {
            return Err(format!(
                "a walking speed is above 0 and at most {MAX_WALK_SPEED} blocks per second, got {speed}"
            ));
        }
        let Some(npc) = self.entities.npcs.get(id) else {
            return Ok(());
        };
        let finder = PathFinder::new(MobProfile::new(NPC_WIDTH, NPC_HEIGHT));
        let mob = MobState {
            position: npc.position,
            on_ground: true,
            in_floatable_fluid: false,
        };
        let path = finder
            .find_path(&self.chunk_view(), &mob, &[target.block()], 1)
            .filter(rspigot_ai::Path::can_reach)
            .ok_or_else(|| "there is no path to that position".to_owned())?;
        let mut waypoints: Vec<Vec3> = (0..path.node_count())
            .map(|index| path.entity_pos_at_node(NPC_WIDTH, index))
            .collect();
        if let Some(last) = waypoints.last().copied()
            && (target.x - last.x).hypot(target.z - last.z) <= 1.5
        {
            waypoints.push(Vec3::new(target.x, last.y, target.z));
        }
        if let Some(npc) = self.entities.npcs.get_mut(id) {
            npc.walk = Some(Walk {
                waypoints,
                next: 0,
                step: speed / 20.0,
            });
        }
        Ok(())
    }

    pub(crate) fn tick_npc_walks(&mut self) {
        let walkers: Vec<EntityId> = self
            .entities
            .npcs
            .iter()
            .filter(|npc| npc.walk.is_some())
            .map(|npc| npc.id)
            .collect();
        for id in walkers {
            let Some(npc) = self.entities.npcs.get(id) else {
                continue;
            };
            let Some(walk) = npc.walk.clone() else {
                continue;
            };
            let (mut position, mut next, mut left) = (npc.position, walk.next, walk.step);
            let mut heading = None;
            while left > 0.0 && next < walk.waypoints.len() {
                let goal = walk.waypoints[next];
                let (dx, dy, dz) = (
                    goal.x - position.x,
                    goal.y - position.y,
                    goal.z - position.z,
                );
                let distance = (dx * dx + dy * dy + dz * dz).sqrt();
                if distance > 1.0e-9 && dx.hypot(dz) > 1.0e-9 {
                    heading = Some((dx, dz));
                }
                if distance <= left {
                    position = goal;
                    left -= distance;
                    next += 1;
                } else {
                    let ratio = left / distance;
                    position = Vec3::new(
                        position.x + dx * ratio,
                        position.y + dy * ratio,
                        position.z + dz * ratio,
                    );
                    left = 0.0;
                }
            }
            let rotation = heading.map(|(dx, dz)| {
                Rotation::new(wrap_degrees((dz.atan2(dx).to_degrees() - 90.0) as f32), 0.0)
            });
            self.step_npc(id, position, rotation);
            if let Some(npc) = self.entities.npcs.get_mut(id) {
                npc.walk = (next < walk.waypoints.len()).then_some(Walk { next, ..walk });
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
        if let Some(npc) = self.entities.npcs.get_mut(id) {
            npc.walk = None;
        }
        self.step_npc(id, to, None);
    }

    pub(crate) fn step_npc(&mut self, id: EntityId, to: Vec3, facing: Option<Rotation>) {
        let Some(npc) = self.entities.npcs.get_mut(id) else {
            return;
        };
        let from = npc.position;
        npc.position = to;
        if let Some(facing) = facing {
            npc.rotation = facing;
        }
        let rotation = npc.rotation;
        let label = npc
            .label
            .is_some()
            .then(|| (npc.label_id, npc.label_position()));
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
        if let Some((label_id, at)) = label {
            self.push(
                Audience::Trackers(id),
                ClientEvent::EntityMoved {
                    entity: label_id,
                    from: Vec3::new(from.x, from.y + LABEL_OFFSET, from.z),
                    to: at,
                    rotation: None,
                    on_ground: false,
                },
            );
        }
        self.update_npc_viewers_of(id);
    }

    pub(crate) fn set_npc_label(&mut self, id: EntityId, label: Option<String>) {
        let Some(npc) = self.entities.npcs.get_mut(id) else {
            return;
        };
        let had = npc.label.is_some();
        npc.label = label;
        let npc = npc.clone();
        match (&npc.label, had) {
            (Some(label), true) => {
                let event = npc.label_metadata_event(label);
                self.push(Audience::Trackers(id), event);
            }
            (Some(_), false) => {
                for event in npc.label_spawn_events() {
                    self.push(Audience::Trackers(id), event);
                }
            }
            (None, true) => {
                self.push(
                    Audience::Trackers(id),
                    ClientEvent::RemoveEntities(vec![npc.label_id]),
                );
            }
            (None, false) => {}
        }
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

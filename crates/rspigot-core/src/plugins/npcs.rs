use rspigot_domain::npc::{DEFAULT_SKIN_LAYERS, NpcKey, NpcName, SkinTextures};
use rspigot_domain::player::PlayerId;
use rspigot_domain::{EntityId, Rotation, Vec3};
use uuid::Uuid;

use crate::events::{Animation, ClientEvent, EquipmentSlot, Pose};
use crate::npc::{MAX_NPCS_PER_PLUGIN, Npc, rotation_toward};
use crate::plugins::items::stack_of;
use crate::plugins::resolve::{self, finite_not_negative};
use crate::plugins::{ActionError, ActionErrorKind, ItemSpec};
use crate::ports::ClientLink;
use crate::server::{Audience, Server};

#[derive(Debug, Clone, PartialEq)]
pub struct NpcSpec {
    pub key: String,
    pub name: String,
    pub position: Vec3,
    pub rotation: Option<Rotation>,
    pub skin: Option<(String, String)>,
    pub skin_layers: Option<u8>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct NpcView {
    pub id: EntityId,
    pub uuid: Uuid,
    pub key: String,
    pub name: String,
    pub world: String,
    pub position: Vec3,
    pub rotation: Rotation,
    pub skin: Option<(String, String)>,
    pub skin_layers: u8,
    pub pose: String,
    pub gaze_range: f64,
    pub label: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum NpcOrder {
    Teleport(Vec3),
    Remove,
    SetSkin {
        value: String,
        signature: String,
    },
    SetRotation(Rotation),
    SetPose(String),
    SetEquipment {
        slot: String,
        item: Option<ItemSpec>,
    },
    SwingMainHand,
    SwingOffHand,
    LookAt(Vec3),
    LookAtNearest(f64),
    SetSkinLayers(u8),
    SetLabel(String),
    HideFrom(PlayerId),
    ShowTo(PlayerId),
}

impl NpcOrder {
    pub const fn name(&self) -> &'static str {
        match self {
            Self::Teleport(_) => "npc.teleport",
            Self::Remove => "npc.remove",
            Self::SetSkin { .. } => "npc.set_skin",
            Self::SetRotation(_) => "npc.set_rotation",
            Self::SetPose(_) => "npc.set_pose",
            Self::SetEquipment { .. } => "npc.set_equipment",
            Self::SwingMainHand => "npc.swing_main_hand",
            Self::SwingOffHand => "npc.swing_off_hand",
            Self::LookAt(_) => "npc.look_at",
            Self::LookAtNearest(_) => "npc.look_at_nearest",
            Self::SetSkinLayers(_) => "npc.set_skin_layers",
            Self::SetLabel(_) => "npc.set_label",
            Self::HideFrom(_) => "npc.hide_from",
            Self::ShowTo(_) => "npc.show_to",
        }
    }
}

const POSES: [(&str, Pose); 6] = [
    ("standing", Pose::Standing),
    ("crouching", Pose::Crouching),
    ("swimming", Pose::Swimming),
    ("fall_flying", Pose::FallFlying),
    ("spin_attack", Pose::SpinAttack),
    ("sleeping", Pose::Sleeping),
];

const SLOTS: [(&str, EquipmentSlot); 6] = [
    ("main_hand", EquipmentSlot::MainHand),
    ("off_hand", EquipmentSlot::OffHand),
    ("feet", EquipmentSlot::Feet),
    ("legs", EquipmentSlot::Legs),
    ("chest", EquipmentSlot::Chest),
    ("head", EquipmentSlot::Head),
];

fn pose_of(name: &str) -> Result<Pose, String> {
    POSES
        .iter()
        .find(|(key, _)| *key == name)
        .map(|(_, pose)| *pose)
        .ok_or_else(|| format!("{name} is not a pose an npc can take"))
}

fn pose_name(pose: Pose) -> &'static str {
    POSES
        .iter()
        .find(|(_, candidate)| *candidate == pose)
        .map_or("standing", |(key, _)| key)
}

fn slot_of(name: &str) -> Result<EquipmentSlot, String> {
    SLOTS
        .iter()
        .find(|(key, _)| *key == name)
        .map(|(_, slot)| *slot)
        .ok_or_else(|| format!("{name} is not an equipment slot of an npc"))
}

impl<L: ClientLink> Server<L> {
    pub(super) fn create_npc_for_plugin(
        &mut self,
        connection: u64,
        spec: NpcSpec,
    ) -> Result<NpcView, ActionError> {
        let owner = self.plugin_name(connection);
        let key = NpcKey::try_from(spec.key).map_err(|error| error.to_string())?;
        if let Some(existing) = self.entities.npcs.find(&owner, &key) {
            return Ok(self.view_of_npc(existing));
        }
        let name = NpcName::try_from(spec.name).map_err(|error| error.to_string())?;
        if self.entities.npcs.count_of(&owner) >= MAX_NPCS_PER_PLUGIN {
            return Err(ActionError::new(
                ActionErrorKind::LimitReached,
                format!("a plugin holds at most {MAX_NPCS_PER_PLUGIN} npcs"),
            ));
        }
        let taken = self.entities.npcs.name_taken(name.as_str())
            || self
                .players
                .values()
                .any(|player| player.profile.name.eq_ignore_ascii_case(name.as_str()));
        if taken {
            return Err(ActionError::from(format!(
                "the name {name} is already used"
            )));
        }
        let position = resolve::position_of(spec.position)?;
        let rotation = spec.rotation.unwrap_or_default();
        let id = self.allocate_entity_id();
        let label_id = self.allocate_entity_id();
        let uuid = self.random_uuid();
        self.entities.npcs.insert(Npc {
            id,
            uuid,
            owner,
            key,
            name,
            skin: spec
                .skin
                .map(|(value, signature)| SkinTextures { value, signature }),
            skin_layers: spec.skin_layers.unwrap_or(DEFAULT_SKIN_LAYERS),
            position,
            rotation,
            pose: Pose::Standing,
            equipment: Vec::new(),
            gaze_range: 0.0,
            label: None,
            label_id,
            hidden: Default::default(),
        });
        self.update_npc_viewers_of(id);
        self.npc_view(id)
            .ok_or_else(|| ActionError::from("the npc was not kept".to_owned()))
    }

    pub(super) fn apply_npc_order(&mut self, connection: u64, id: EntityId, order: NpcOrder) {
        let name = order.name();
        let owner = self.plugin_name(connection);
        let Some(npc) = self.entities.npcs.get(id) else {
            return;
        };
        if npc.owner != owner {
            self.warn_plugin(
                connection,
                name,
                format!("the npc belongs to the plugin {}", npc.owner),
            );
            return;
        }
        if let Err(reason) = self.npc_order(id, order) {
            self.warn_plugin(connection, name, reason);
        }
    }

    fn npc_order(&mut self, id: EntityId, order: NpcOrder) -> Result<(), String> {
        match order {
            NpcOrder::Teleport(position) => {
                let position = resolve::position_of(position)?;
                self.move_npc(id, position);
            }
            NpcOrder::Remove => self.drop_npc(id),
            NpcOrder::SetSkin { value, signature } => {
                if let Some(npc) = self.entities.npcs.get_mut(id) {
                    npc.skin = Some(SkinTextures { value, signature });
                }
                self.respawn_npc(id);
            }
            NpcOrder::SetRotation(rotation) => {
                if !(rotation.yaw.is_finite() && rotation.pitch.is_finite()) {
                    return Err("the rotation is not finite".to_owned());
                }
                self.turn_npc(
                    id,
                    Rotation::new(rotation.yaw, rotation.pitch.clamp(-90.0, 90.0)),
                );
            }
            NpcOrder::SetPose(pose) => {
                let pose = pose_of(&pose)?;
                if let Some(npc) = self.entities.npcs.get_mut(id) {
                    npc.pose = pose;
                }
                self.npc_metadata_changed(id);
            }
            NpcOrder::SetSkinLayers(layers) => {
                if let Some(npc) = self.entities.npcs.get_mut(id) {
                    npc.skin_layers = layers;
                }
                self.npc_metadata_changed(id);
            }
            NpcOrder::SetEquipment { slot, item } => {
                let slot = slot_of(&slot)?;
                let stack = item.as_ref().map(stack_of).transpose()?.flatten();
                self.set_npc_equipment(id, slot, stack);
            }
            NpcOrder::SetLabel(text) => {
                self.set_npc_label(id, (!text.is_empty()).then_some(text));
            }
            NpcOrder::HideFrom(viewer) => self.set_npc_hidden(id, viewer, true),
            NpcOrder::ShowTo(viewer) => self.set_npc_hidden(id, viewer, false),
            NpcOrder::SwingMainHand => self.animate_npc(id, Animation::SwingMainArm),
            NpcOrder::SwingOffHand => self.animate_npc(id, Animation::SwingOffHand),
            NpcOrder::LookAt(target) => {
                let target = resolve::position_of(target)?;
                if let Some(npc) = self.entities.npcs.get(id) {
                    let facing = rotation_toward(npc.eye(), target, npc.rotation);
                    self.turn_npc(id, facing);
                }
            }
            NpcOrder::LookAtNearest(range) => {
                finite_not_negative("range", range)?;
                if let Some(npc) = self.entities.npcs.get_mut(id) {
                    npc.gaze_range = range;
                }
            }
        }
        Ok(())
    }

    fn respawn_npc(&mut self, id: EntityId) {
        let Some(npc) = self.entities.npcs.get(id).cloned() else {
            return;
        };
        for player in self.players.values() {
            if player.tracked.contains(&id) {
                for event in npc.despawn_events() {
                    player.link.send(event);
                }
                for event in npc.spawn_events() {
                    player.link.send(event);
                }
            }
        }
    }

    fn set_npc_equipment(
        &mut self,
        id: EntityId,
        slot: EquipmentSlot,
        stack: Option<rspigot_domain::item::ItemStack>,
    ) {
        let Some(npc) = self.entities.npcs.get_mut(id) else {
            return;
        };
        npc.equipment.retain(|(held, _)| *held != slot);
        if let Some(stack) = &stack {
            npc.equipment.push((slot, stack.clone()));
        }
        self.push(
            Audience::Trackers(id),
            ClientEvent::EntityEquipment {
                entity: id,
                slots: vec![(slot, stack)],
            },
        );
    }

    fn animate_npc(&mut self, id: EntityId, animation: Animation) {
        self.push(
            Audience::Trackers(id),
            ClientEvent::EntityAnimation {
                entity: id,
                animation,
            },
        );
    }

    fn view_of_npc(&self, npc: &Npc) -> NpcView {
        NpcView {
            id: npc.id,
            uuid: npc.uuid,
            key: npc.key.as_str().to_owned(),
            name: npc.name.as_str().to_owned(),
            world: self.world.level.name.clone(),
            position: npc.position,
            rotation: npc.rotation,
            skin: npc
                .skin
                .as_ref()
                .map(|skin| (skin.value.clone(), skin.signature.clone())),
            skin_layers: npc.skin_layers,
            pose: pose_name(npc.pose).to_owned(),
            gaze_range: npc.gaze_range,
            label: npc.label.clone(),
        }
    }

    pub(super) fn npc_view(&self, id: EntityId) -> Option<NpcView> {
        self.entities.npcs.get(id).map(|npc| self.view_of_npc(npc))
    }

    pub(super) fn npc_by_key(&self, connection: u64, key: &str) -> Option<NpcView> {
        let key = NpcKey::try_from(key.to_owned()).ok()?;
        let owner = self.plugin_name(connection);
        self.entities
            .npcs
            .find(&owner, &key)
            .map(|npc| self.view_of_npc(npc))
    }

    pub(super) fn npc_views(&self, connection: u64) -> Vec<NpcView> {
        let owner = self.plugin_name(connection);
        self.entities
            .npcs
            .iter()
            .filter(|npc| npc.owner == owner)
            .map(|npc| self.view_of_npc(npc))
            .collect()
    }
}

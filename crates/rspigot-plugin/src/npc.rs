use rspigot_plugin_protocol::v1::npc_order::Order;
use rspigot_plugin_protocol::v1::{self, answer, from_plugin, question};
use uuid::Uuid;

use crate::link::Link;
use crate::wire::parse_uuid;
use crate::{Error, ItemStack, Player, Position, Rotation};

pub const DEFAULT_SKIN_LAYERS: u8 = 0x7f;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NpcSkin {
    pub value: String,
    pub signature: String,
}

impl NpcSkin {
    pub fn new(value: impl Into<String>, signature: impl Into<String>) -> Self {
        Self {
            value: value.into(),
            signature: signature.into(),
        }
    }

    fn to_wire(&self) -> v1::NpcSkin {
        v1::NpcSkin {
            value: self.value.clone(),
            signature: self.signature.clone(),
        }
    }

    fn from_wire(skin: v1::NpcSkin) -> Self {
        Self {
            value: skin.value,
            signature: skin.signature,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NpcPose {
    Standing,
    Crouching,
    Swimming,
    FallFlying,
    SpinAttack,
    Sleeping,
}

impl NpcPose {
    pub const ALL: [Self; 6] = [
        Self::Standing,
        Self::Crouching,
        Self::Swimming,
        Self::FallFlying,
        Self::SpinAttack,
        Self::Sleeping,
    ];

    pub const fn name(self) -> &'static str {
        match self {
            Self::Standing => "standing",
            Self::Crouching => "crouching",
            Self::Swimming => "swimming",
            Self::FallFlying => "fall_flying",
            Self::SpinAttack => "spin_attack",
            Self::Sleeping => "sleeping",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|pose| pose.name() == name)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NpcSlot {
    MainHand,
    OffHand,
    Feet,
    Legs,
    Chest,
    Head,
}

impl NpcSlot {
    pub const fn name(self) -> &'static str {
        match self {
            Self::MainHand => "main_hand",
            Self::OffHand => "off_hand",
            Self::Feet => "feet",
            Self::Legs => "legs",
            Self::Chest => "chest",
            Self::Head => "head",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct NpcSpec {
    pub key: String,
    pub name: String,
    pub position: Position,
    pub rotation: Option<Rotation>,
    pub skin: Option<NpcSkin>,
    pub skin_layers: Option<u8>,
}

impl NpcSpec {
    pub fn new(key: impl Into<String>, name: impl Into<String>, position: Position) -> Self {
        Self {
            key: key.into(),
            name: name.into(),
            position,
            rotation: None,
            skin: None,
            skin_layers: None,
        }
    }

    #[must_use]
    pub fn rotation(mut self, rotation: Rotation) -> Self {
        self.rotation = Some(rotation);
        self
    }

    #[must_use]
    pub fn skin(mut self, skin: NpcSkin) -> Self {
        self.skin = Some(skin);
        self
    }

    #[must_use]
    pub fn skin_layers(mut self, layers: u8) -> Self {
        self.skin_layers = Some(layers);
        self
    }

    fn into_wire(self) -> v1::NpcSpec {
        v1::NpcSpec {
            key: self.key,
            name: self.name,
            position: Some(self.position.to_wire()),
            rotation: self.rotation.map(Rotation::to_wire),
            skin: self.skin.as_ref().map(NpcSkin::to_wire),
            skin_layers: self.skin_layers.map(u32::from),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Npcs {
    link: Link,
}

impl Npcs {
    pub(crate) const fn new(link: Link) -> Self {
        Self { link }
    }

    pub async fn create(&self, spec: NpcSpec) -> Result<Npc, Error> {
        let state = self
            .link
            .ask(
                question::Question::CreateNpc(spec.into_wire()),
                "a created npc",
                |body| match body {
                    answer::Body::Npc(state) => Some(state),
                    _ => None,
                },
            )
            .await?;
        Npc::from_wire(state, &self.link)
    }

    pub async fn get(&self, key: impl Into<String>) -> Result<Option<Npc>, Error> {
        let found = self
            .link
            .ask(
                question::Question::NpcByKey(key.into()),
                "an npc",
                |body| match body {
                    answer::Body::Npc(state) => Some(state),
                    _ => None,
                },
            )
            .await;
        match found {
            Ok(state) => Npc::from_wire(state, &self.link).map(Some),
            Err(Error::NotFound) => Ok(None),
            Err(error) => Err(error),
        }
    }

    pub async fn get_npcs(&self) -> Result<Vec<Npc>, Error> {
        let list = self
            .link
            .ask(
                question::Question::Npcs(v1::Empty {}),
                "the npcs",
                |body| match body {
                    answer::Body::Npcs(list) => Some(list),
                    _ => None,
                },
            )
            .await?;
        list.npcs
            .into_iter()
            .map(|state| Npc::from_wire(state, &self.link))
            .collect()
    }
}

#[derive(Debug, Clone)]
pub struct Npc {
    pub entity_id: i32,
    pub uuid: Uuid,
    pub key: String,
    pub name: String,
    pub position: Position,
    pub rotation: Rotation,
    link: Link,
}

impl PartialEq for Npc {
    fn eq(&self, other: &Self) -> bool {
        self.entity_id == other.entity_id
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct NpcState {
    pub npc: Npc,
    pub world: String,
    pub skin: Option<NpcSkin>,
    pub skin_layers: u8,
    pub pose: NpcPose,
    pub gaze_range: f64,
    pub label: Option<String>,
}

impl Npc {
    pub(crate) fn from_wire(state: v1::NpcState, link: &Link) -> Result<Self, Error> {
        Ok(Self {
            entity_id: state.entity_id,
            uuid: parse_uuid(&state.uuid, "an npc")?,
            key: state.key,
            name: state.name,
            position: Position::from_wire(state.position, "position")?,
            rotation: Rotation::from_wire(state.rotation)?,
            link: link.clone(),
        })
    }

    pub async fn get_state(&self) -> Result<NpcState, Error> {
        let state = self
            .link
            .ask(
                question::Question::Npc(self.entity_id),
                "an npc state",
                |body| match body {
                    answer::Body::Npc(state) => Some(state),
                    _ => None,
                },
            )
            .await?;
        let pose = NpcPose::from_name(&state.pose)
            .ok_or_else(|| Error::Malformed(format!("the npc pose {}", state.pose)))?;
        let layers = u8::try_from(state.skin_layers)
            .map_err(|_| Error::Malformed(format!("the skin layers {}", state.skin_layers)))?;
        let (world, skin, gaze_range, label) = (
            state.world.clone(),
            state.skin.clone().map(NpcSkin::from_wire),
            state.gaze_range,
            state.label.clone(),
        );
        Ok(NpcState {
            npc: Self::from_wire(state, &self.link)?,
            world,
            skin,
            skin_layers: layers,
            pose,
            gaze_range,
            label,
        })
    }

    pub fn teleport(&self, position: Position) {
        self.order(Order::Teleport(position.to_wire()));
    }

    pub fn remove(&self) {
        self.order(Order::Remove(v1::Empty {}));
    }

    pub fn set_skin(&self, skin: &NpcSkin) {
        self.order(Order::SetSkin(skin.to_wire()));
    }

    pub fn set_skin_layers(&self, layers: u8) {
        self.order(Order::SetSkinLayers(u32::from(layers)));
    }

    pub fn set_rotation(&self, rotation: Rotation) {
        self.order(Order::SetRotation(rotation.to_wire()));
    }

    pub fn set_pose(&self, pose: NpcPose) {
        self.order(Order::SetPose(pose.name().to_owned()));
    }

    pub fn set_equipment(&self, slot: NpcSlot, item: Option<ItemStack>) {
        self.order(Order::SetEquipment(v1::NpcEquipment {
            slot: slot.name().to_owned(),
            item: item.map(ItemStack::into_wire),
        }));
    }

    pub fn set_label(&self, text: impl Into<String>) {
        self.order(Order::SetLabel(text.into()));
    }

    pub fn clear_label(&self) {
        self.order(Order::SetLabel(String::new()));
    }

    pub fn hide_from(&self, player: &Player) {
        self.order(Order::HideFrom(player.uuid.to_string()));
    }

    pub fn show_to(&self, player: &Player) {
        self.order(Order::ShowTo(player.uuid.to_string()));
    }

    pub fn swing_main_hand(&self) {
        self.order(Order::SwingMainHand(v1::Empty {}));
    }

    pub fn swing_off_hand(&self) {
        self.order(Order::SwingOffHand(v1::Empty {}));
    }

    pub fn look_at(&self, position: Position) {
        self.order(Order::LookAt(position.to_wire()));
    }

    pub fn look_at_nearest(&self, range: f64) {
        self.order(Order::LookAtNearest(v1::NpcGaze { range }));
    }

    pub fn stop_looking(&self) {
        self.look_at_nearest(0.0);
    }

    fn order(&self, order: Order) {
        self.link.order(from_plugin::Body::Npc(v1::NpcOrder {
            npc: self.entity_id,
            order: Some(order),
        }));
    }
}

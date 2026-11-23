use rspigot_domain::player::PlayerId;
use rspigot_domain::{BlockPos, EntityId};
use rspigot_nbt::{Compound, Tag};
use uuid::Uuid;

use crate::client_extras::ResourcePackOffer;
use crate::events::{ClientEvent, EquipmentSlot};
use crate::plugins::block::block_state_of;
use crate::plugins::resolve::{text, within};
use crate::plugins::{BlockState, ItemSpec, stack_of};
use crate::ports::ClientLink;
use crate::server::{Audience, Server};
use crate::sign;

const MAX_BLOCK_CHANGES: usize = 65_536;
const SIGN_LINES: usize = 4;
const DYES: [&str; 16] = [
    "white",
    "orange",
    "magenta",
    "light_blue",
    "yellow",
    "lime",
    "pink",
    "gray",
    "light_gray",
    "cyan",
    "purple",
    "blue",
    "brown",
    "green",
    "red",
    "black",
];
const SLOTS: [(&str, EquipmentSlot); 8] = [
    ("mainhand", EquipmentSlot::MainHand),
    ("offhand", EquipmentSlot::OffHand),
    ("feet", EquipmentSlot::Feet),
    ("legs", EquipmentSlot::Legs),
    ("chest", EquipmentSlot::Chest),
    ("head", EquipmentSlot::Head),
    ("body", EquipmentSlot::Body),
    ("saddle", EquipmentSlot::Saddle),
];

#[derive(Debug, Clone, PartialEq)]
pub enum ClientChange {
    Blocks(Vec<(BlockPos, BlockState)>),
    Sign {
        pos: BlockPos,
        lines: Vec<String>,
        color: Option<String>,
        glowing: bool,
    },
    Equipment {
        entity: EntityId,
        slots: Vec<(String, Option<ItemSpec>)>,
    },
    Hurt {
        entity: EntityId,
        yaw: f32,
    },
    ResourcePack {
        id: Uuid,
        url: String,
        hash: String,
        required: bool,
        prompt: Option<String>,
    },
    RemoveResourcePack(Option<Uuid>),
}

impl ClientChange {
    pub const fn name(&self) -> &'static str {
        match self {
            Self::Blocks(_) => "player.client.send_block_changes",
            Self::Sign { .. } => "player.client.send_sign_change",
            Self::Equipment { .. } => "player.client.send_equipment_change",
            Self::Hurt { .. } => "player.client.send_hurt_animation",
            Self::ResourcePack { .. } => "player.resource_pack.send",
            Self::RemoveResourcePack(_) => "player.resource_pack.remove",
        }
    }
}

fn slot_of(name: &str) -> Result<EquipmentSlot, String> {
    SLOTS
        .iter()
        .find(|(key, _)| *key == name)
        .map(|(_, slot)| *slot)
        .ok_or_else(|| format!("{name} is not an equipment slot"))
}

impl<L: ClientLink> Server<L> {
    pub(super) fn client_change(
        &mut self,
        id: PlayerId,
        change: ClientChange,
    ) -> Result<(), String> {
        match change {
            ClientChange::Blocks(changes) => self.send_block_changes(id, &changes),
            ClientChange::Sign {
                pos,
                lines,
                color,
                glowing,
            } => self.send_sign_change(id, pos, &lines, color.as_deref(), glowing),
            ClientChange::Equipment { entity, slots } => {
                self.send_equipment_change(id, entity, &slots)
            }
            ClientChange::Hurt { entity, yaw } => {
                if !yaw.is_finite() {
                    return Err(format!("the yaw {yaw} is not finite"));
                }
                self.push(
                    Audience::One(id),
                    ClientEvent::HurtAnimation { entity, yaw },
                );
                Ok(())
            }
            ClientChange::ResourcePack {
                id: pack,
                url,
                hash,
                required,
                prompt,
            } => {
                let offer = ResourcePackOffer {
                    id: pack,
                    url,
                    hash,
                    required,
                    prompt: prompt.as_deref().map(text),
                };
                self.send_resource_pack(id, offer)
                    .map_err(|error| error.to_string())
            }
            ClientChange::RemoveResourcePack(pack) => self
                .clear_resource_packs(id, pack)
                .map_err(|error| error.to_string()),
        }
    }

    fn send_block_changes(
        &mut self,
        id: PlayerId,
        changes: &[(BlockPos, BlockState)],
    ) -> Result<(), String> {
        if changes.len() > MAX_BLOCK_CHANGES {
            return Err(format!(
                "at most {MAX_BLOCK_CHANGES} block changes at once, got {}",
                changes.len()
            ));
        }
        let height = self.world.dimension.height;
        let mut resolved = Vec::with_capacity(changes.len());
        for (pos, data) in changes {
            within("y", pos.y, height.min_y..=height.max_y())?;
            let state = block_state_of(data)
                .ok_or_else(|| format!("{} does not take those properties", data.kind))?;
            resolved.push((*pos, state));
        }
        for (pos, state) in resolved {
            self.push(Audience::One(id), ClientEvent::BlockUpdate { pos, state });
        }
        Ok(())
    }

    fn send_sign_change(
        &mut self,
        id: PlayerId,
        pos: BlockPos,
        lines: &[String],
        color: Option<&str>,
        glowing: bool,
    ) -> Result<(), String> {
        if lines.len() > SIGN_LINES {
            return Err(format!(
                "a sign has {SIGN_LINES} lines, got {}",
                lines.len()
            ));
        }
        let color = color.unwrap_or("black");
        if !DYES.contains(&color) {
            return Err(format!("{color} is not a dye colour"));
        }
        let height = self.world.dimension.height;
        within("y", pos.y, height.min_y..=height.max_y())?;
        let kind = self
            .world
            .chunks
            .block(pos)
            .map_or("sign", sign::entity_kind);
        let messages = (0..SIGN_LINES)
            .map(|index| sign::line_tag(None, lines.get(index).map_or("", String::as_str)))
            .collect();
        let mut front = Compound::new();
        front.insert("messages", Tag::List(messages));
        front.insert("color", Tag::String(color.to_owned()));
        front.insert("has_glowing_text", Tag::Byte(i8::from(glowing)));
        let mut data = Compound::new();
        data.insert("front_text", Tag::Compound(front));
        data.insert("is_waxed", Tag::Byte(0));
        self.push(
            Audience::One(id),
            ClientEvent::BlockEntityUpdate { pos, kind, data },
        );
        Ok(())
    }

    fn send_equipment_change(
        &mut self,
        id: PlayerId,
        entity: EntityId,
        edits: &[(String, Option<ItemSpec>)],
    ) -> Result<(), String> {
        let mut slots = Vec::with_capacity(edits.len());
        for (name, item) in edits {
            let stack = match item {
                Some(spec) => stack_of(spec)?,
                None => None,
            };
            slots.push((slot_of(name)?, stack));
        }
        if slots.is_empty() {
            return Err("an equipment change needs at least one slot".to_owned());
        }
        self.push(
            Audience::One(id),
            ClientEvent::EntityEquipment { entity, slots },
        );
        Ok(())
    }
}

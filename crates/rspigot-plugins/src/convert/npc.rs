use rspigot_core::plugins::{NpcOrder, NpcSpec, NpcView, Order};
use rspigot_domain::EntityId;
use rspigot_plugin_protocol::v1::{self, npc_order::Order as Wire};

use super::Malformed;
use super::shared::{self, required, wire_rotation, wire_vec};

pub(super) fn order(order: v1::NpcOrder) -> Result<Order, Malformed> {
    let npc = EntityId(order.npc);
    let order = match required(order.order)? {
        Wire::Teleport(position) => NpcOrder::Teleport(shared::vec3(position)),
        Wire::Remove(_) => NpcOrder::Remove,
        Wire::SetSkin(skin) => NpcOrder::SetSkin {
            value: skin.value,
            signature: skin.signature,
        },
        Wire::SetRotation(rotation) => NpcOrder::SetRotation(shared::rotation(rotation)),
        Wire::SetPose(pose) => NpcOrder::SetPose(pose),
        Wire::SetEquipment(equipment) => NpcOrder::SetEquipment {
            slot: equipment.slot,
            item: equipment.item.map(shared::item),
        },
        Wire::SwingMainHand(_) => NpcOrder::SwingMainHand,
        Wire::SwingOffHand(_) => NpcOrder::SwingOffHand,
        Wire::LookAt(target) => NpcOrder::LookAt(shared::vec3(target)),
        Wire::LookAtNearest(gaze) => NpcOrder::LookAtNearest(gaze.range),
        Wire::SetSkinLayers(layers) => NpcOrder::SetSkinLayers(layers_of(layers)?),
        Wire::SetLabel(text) => NpcOrder::SetLabel(text),
        Wire::HideFrom(player) => NpcOrder::HideFrom(super::player::id(player)?),
        Wire::ShowTo(player) => NpcOrder::ShowTo(super::player::id(player)?),
    };
    Ok(Order::Npc { npc, order })
}

fn layers_of(layers: u32) -> Result<u8, Malformed> {
    u8::try_from(layers).map_err(|_| Malformed::OutOfRange {
        field: "skin_layers",
        value: i64::from(layers),
    })
}

pub(super) fn spec(spec: v1::NpcSpec) -> Result<NpcSpec, Malformed> {
    Ok(NpcSpec {
        key: spec.key,
        name: spec.name,
        position: shared::vec3(required(spec.position)?),
        rotation: spec.rotation.map(shared::rotation),
        skin: spec.skin.map(|skin| (skin.value, skin.signature)),
        skin_layers: spec.skin_layers.map(layers_of).transpose()?,
    })
}

pub(super) fn wire_state(view: NpcView) -> v1::NpcState {
    v1::NpcState {
        entity_id: view.id.0,
        uuid: view.uuid.hyphenated().to_string(),
        key: view.key,
        name: view.name,
        world: view.world,
        position: Some(wire_vec(view.position)),
        rotation: Some(wire_rotation(view.rotation)),
        skin: view
            .skin
            .map(|(value, signature)| v1::NpcSkin { value, signature }),
        skin_layers: u32::from(view.skin_layers),
        pose: view.pose,
        gaze_range: view.gaze_range,
        label: view.label,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wire_spec() -> v1::NpcSpec {
        v1::NpcSpec {
            key: "agent-7".to_owned(),
            name: "Scout".to_owned(),
            position: Some(v1::Vec3 {
                x: 1.0,
                y: 64.0,
                z: 2.0,
            }),
            rotation: None,
            skin: None,
            skin_layers: Some(0x7f),
        }
    }

    #[test]
    fn a_spec_without_a_position_is_malformed() {
        let mut spec = wire_spec();
        spec.position = None;
        assert!(matches!(super::spec(spec), Err(Malformed::Empty)));
    }

    #[test]
    fn skin_layers_above_a_byte_are_out_of_range() {
        let mut spec = wire_spec();
        spec.skin_layers = Some(256);
        assert!(matches!(
            super::spec(spec),
            Err(Malformed::OutOfRange {
                field: "skin_layers",
                value: 256
            })
        ));
    }

    #[test]
    fn an_order_without_a_body_is_malformed() {
        let order = v1::NpcOrder {
            npc: 3,
            order: None,
        };
        assert!(matches!(super::order(order), Err(Malformed::Empty)));
    }

    #[test]
    fn a_remove_order_keeps_the_entity_id() {
        let order = v1::NpcOrder {
            npc: 3,
            order: Some(Wire::Remove(v1::Empty {})),
        };
        assert_eq!(
            super::order(order).unwrap(),
            Order::Npc {
                npc: EntityId(3),
                order: NpcOrder::Remove
            }
        );
    }
}

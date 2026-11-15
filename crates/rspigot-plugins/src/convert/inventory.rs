use rspigot_core::plugins::{InventoryOrder, InventoryState, Order};
use rspigot_plugin_protocol::v1::{self, inventory_order::Order as Wire};

use super::Malformed;
use super::player;
use super::shared::{self, required};

pub(super) fn order(order: v1::InventoryOrder) -> Result<Order, Malformed> {
    let player = player::id(order.player)?;
    let order = match required(order.order)? {
        Wire::Add(item) => InventoryOrder::Add(shared::item(item)),
        Wire::Set(v1::SetSlot { slot, item }) => InventoryOrder::Set {
            slot,
            item: item.map(shared::item),
        },
        Wire::Remove(v1::RemoveItems { key, count }) => InventoryOrder::Remove { key, count },
        Wire::Clear(_) => InventoryOrder::Clear,
        Wire::SetHeldSlot(slot) => InventoryOrder::SetHeldSlot(slot),
    };
    Ok(Order::Inventory { player, order })
}

pub(super) fn wire_state(state: InventoryState) -> v1::InventoryState {
    v1::InventoryState {
        slots: (0..)
            .zip(state.slots)
            .map(|(slot, item)| v1::Slot {
                slot,
                item: item.map(shared::wire_item),
            })
            .collect(),
        held_slot: i32::from(state.held_slot),
    }
}

#[cfg(test)]
mod tests {
    use rspigot_core::plugins::{
        Answer, FromPlugin, InventoryOrder, InventoryState, ItemSpec, Order, Question, ToPlugin,
    };
    use rspigot_domain::player::PlayerId;
    use rspigot_plugin_protocol::v1::{self, answer, inventory_order, question};
    use uuid::Uuid;

    use super::*;
    use crate::convert::{from_plugin, to_plugin};

    const STEVE: &str = "00000000-0000-0000-0000-000000000001";

    fn convert(order: inventory_order::Order) -> Result<InventoryOrder, Malformed> {
        let converted = from_plugin(
            v1::FromPlugin {
                body: Some(from_plugin::Body::Inventory(v1::InventoryOrder {
                    player: STEVE.into(),
                    order: Some(order),
                })),
                origin: None,
            },
            &mut Vec::new(),
        )?;
        let FromPlugin::Order(Order::Inventory { player, order }) = converted else {
            panic!("expected an inventory order");
        };
        assert_eq!(player, PlayerId(Uuid::from_u128(1)));
        Ok(order)
    }

    fn wire_sword() -> v1::Item {
        v1::Item {
            key: "minecraft:diamond_sword".into(),
            count: 1,
            name: Some("§6Excalibur".into()),
            lore: vec!["§7Old".into(), "Sharp".into()],
            enchantments: vec![v1::Enchant {
                key: "minecraft:sharpness".into(),
                level: 5,
            }],
            components: vec![10, 0],
            custom_model_data: None,
        }
    }

    fn sword() -> ItemSpec {
        ItemSpec {
            name: Some("§6Excalibur".into()),
            lore: vec!["§7Old".into(), "Sharp".into()],
            enchantments: vec![("minecraft:sharpness".into(), 5)],
            components: vec![10, 0],
            ..ItemSpec::of("minecraft:diamond_sword", 1)
        }
    }

    #[test]
    fn custom_model_data_converts_both_ways() {
        let wire = v1::Item {
            custom_model_data: Some(v1::CustomModelData {
                floats: vec![1.5],
                flags: vec![true],
                strings: vec!["skin".into()],
                colors: vec![255],
            }),
            ..wire_sword()
        };
        let spec = ItemSpec {
            custom_model_data: Some(Box::new(rspigot_domain::item::CustomModelData {
                floats: vec![1.5],
                flags: vec![true],
                strings: vec!["skin".into()],
                colors: vec![255],
            })),
            ..sword()
        };
        assert_eq!(
            convert(inventory_order::Order::Add(wire.clone())).unwrap(),
            InventoryOrder::Add(spec.clone())
        );
        assert_eq!(super::super::shared::wire_item(spec), wire);
    }

    #[test]
    fn items_convert_both_ways() {
        assert_eq!(
            convert(inventory_order::Order::Add(wire_sword())).unwrap(),
            InventoryOrder::Add(sword())
        );
        assert_eq!(
            convert(inventory_order::Order::Set(v1::SetSlot {
                slot: 39,
                item: None
            }))
            .unwrap(),
            InventoryOrder::Set {
                slot: 39,
                item: None
            }
        );
        assert_eq!(
            convert(inventory_order::Order::Set(v1::SetSlot {
                slot: 2,
                item: Some(wire_sword())
            }))
            .unwrap(),
            InventoryOrder::Set {
                slot: 2,
                item: Some(sword())
            }
        );
        assert_eq!(
            convert(inventory_order::Order::Remove(v1::RemoveItems {
                key: "minecraft:stone".into(),
                count: 15
            }))
            .unwrap(),
            InventoryOrder::Remove {
                key: "minecraft:stone".into(),
                count: 15
            }
        );
        assert_eq!(
            convert(inventory_order::Order::Clear(v1::Empty {})).unwrap(),
            InventoryOrder::Clear
        );
        assert_eq!(
            convert(inventory_order::Order::SetHeldSlot(3)).unwrap(),
            InventoryOrder::SetHeldSlot(3)
        );
        let empty = from_plugin(
            v1::FromPlugin {
                body: Some(from_plugin::Body::Inventory(v1::InventoryOrder {
                    player: STEVE.into(),
                    order: None,
                })),
                origin: None,
            },
            &mut Vec::new(),
        );
        assert!(matches!(empty, Err(Malformed::Empty)));

        let mut slots = vec![None; 41];
        slots[0] = Some(sword());
        slots[40] = Some(ItemSpec::of("minecraft:shield", 1));
        let wire = wire_state(InventoryState {
            slots,
            held_slot: 4,
        });
        assert_eq!(wire.held_slot, 4);
        assert_eq!(wire.slots.len(), 41);
        assert_eq!(
            wire.slots[0],
            v1::Slot {
                slot: 0,
                item: Some(wire_sword())
            }
        );
        assert_eq!(
            wire.slots[39],
            v1::Slot {
                slot: 39,
                item: None
            }
        );
        assert_eq!(wire.slots[40].slot, 40);
        assert_eq!(
            wire.slots[40].item.as_ref().map(|item| item.key.as_str()),
            Some("minecraft:shield")
        );
    }

    #[test]
    fn the_inventory_question_and_answer_keep_their_shape() {
        let asked = from_plugin(
            v1::FromPlugin {
                body: Some(from_plugin::Body::Question(v1::Question {
                    request: 9,
                    question: Some(question::Question::Inventory(STEVE.into())),
                })),
                origin: None,
            },
            &mut Vec::new(),
        );
        assert_eq!(
            asked.unwrap(),
            FromPlugin::Question {
                request: 9,
                question: Question::Inventory(PlayerId(Uuid::from_u128(1)))
            }
        );
        let answered = to_plugin(ToPlugin::Answer {
            request: 9,
            answer: Answer::Inventory(InventoryState {
                slots: vec![None; 41],
                held_slot: 0,
            }),
        });
        let Some(to_plugin::Body::Answer(v1::Answer {
            request: 9,
            body: Some(answer::Body::Inventory(state)),
        })) = answered.body
        else {
            panic!("expected an inventory answer");
        };
        assert_eq!(state.slots.len(), 41);
        assert!(state.slots.iter().all(|slot| slot.item.is_none()));
    }
}

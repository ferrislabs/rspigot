use rspigot_core::plugins::{BlockOrder, BlockState, Order};
use rspigot_plugin_protocol::v1::{self, block_order::Order as Wire};

use super::Malformed;
use super::shared::{self, required};

pub(super) fn order(order: v1::BlockOrder) -> Result<Order, Malformed> {
    let pos = shared::block_pos(required(order.pos)?);
    let order = match required(order.order)? {
        Wire::Set(key) => BlockOrder::Set(key),
        Wire::SetWith(v1::SetBlockWith { block, properties }) => BlockOrder::SetWith {
            kind: block,
            properties: properties
                .into_iter()
                .map(|property| (property.name, property.value))
                .collect(),
        },
        Wire::BreakNaturally(_) => BlockOrder::BreakNaturally,
        Wire::Meta(order) => BlockOrder::Meta(super::meta::order(order)?),
        Wire::SetData(edit) => BlockOrder::SetData(
            edit.properties
                .into_iter()
                .map(|property| (property.name, property.value))
                .collect(),
        ),
        Wire::Tile(edit) => BlockOrder::Tile(super::tiles::edit(edit)?),
        Wire::Pdc(change) => BlockOrder::Pdc(super::objects::pdc_change(change)?),
    };
    Ok(Order::Block { pos, order })
}

pub(super) fn state(state: v1::BlockState) -> BlockState {
    BlockState {
        kind: state.block,
        properties: state
            .properties
            .into_iter()
            .map(|property| (property.name, property.value))
            .collect(),
    }
}

pub(super) fn wire_state(state: BlockState) -> v1::BlockState {
    v1::BlockState {
        block: state.kind,
        properties: state
            .properties
            .into_iter()
            .map(|(name, value)| v1::Property { name, value })
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use rspigot_core::plugins::{
        Answer, BlockOrder, BlockState, FromPlugin, Order, Question, ToPlugin,
    };
    use rspigot_domain::BlockPos;
    use rspigot_plugin_protocol::v1::{self, answer, block_order};

    use super::*;
    use crate::convert::{Malformed, from_plugin, question, to_plugin};

    const AT: v1::BlockPos = v1::BlockPos { x: 2, y: 64, z: -3 };

    fn convert(
        pos: Option<v1::BlockPos>,
        order: Option<block_order::Order>,
    ) -> Result<FromPlugin, Malformed> {
        from_plugin(
            v1::FromPlugin {
                body: Some(from_plugin::Body::Block(v1::BlockOrder { pos, order })),
                origin: None,
                request: None,
                in_world: None,
            },
            &mut Vec::new(),
        )
    }

    fn at(order: BlockOrder) -> FromPlugin {
        FromPlugin::Order(Order::Block {
            pos: BlockPos::new(2, 64, -3),
            order,
        })
    }

    #[test]
    fn block_orders_and_state_convert() {
        assert_eq!(
            convert(
                Some(AT),
                Some(block_order::Order::SetWith(v1::SetBlockWith {
                    block: "minecraft:oak_stairs".into(),
                    properties: vec![v1::Property {
                        name: "facing".into(),
                        value: "east".into(),
                    }],
                }))
            )
            .unwrap(),
            at(BlockOrder::SetWith {
                kind: "minecraft:oak_stairs".into(),
                properties: vec![("facing".into(), "east".into())],
            })
        );
        assert_eq!(
            convert(
                Some(AT),
                Some(block_order::Order::BreakNaturally(v1::Empty {}))
            )
            .unwrap(),
            at(BlockOrder::BreakNaturally)
        );
        assert_eq!(
            convert(
                Some(AT),
                Some(block_order::Order::Set("minecraft:stone".into()))
            )
            .unwrap(),
            at(BlockOrder::Set("minecraft:stone".into()))
        );
        assert!(matches!(
            convert(None, Some(block_order::Order::BreakNaturally(v1::Empty {}))),
            Err(Malformed::Empty)
        ));
        assert!(matches!(convert(Some(AT), None), Err(Malformed::Empty)));

        let asked = from_plugin(
            v1::FromPlugin {
                body: Some(from_plugin::Body::Question(v1::Question {
                    request: 5,
                    question: Some(question::Question::Block(AT)),
                })),
                origin: None,
                request: None,
                in_world: None,
            },
            &mut Vec::new(),
        );
        assert_eq!(
            asked.unwrap(),
            FromPlugin::Question {
                request: 5,
                question: Question::Block(BlockPos::new(2, 64, -3))
            }
        );

        let state = BlockState {
            kind: "minecraft:oak_stairs".into(),
            properties: vec![
                ("facing".into(), "east".into()),
                ("half".into(), "bottom".into()),
            ],
        };
        let wire = v1::BlockState {
            block: "minecraft:oak_stairs".into(),
            properties: vec![
                v1::Property {
                    name: "facing".into(),
                    value: "east".into(),
                },
                v1::Property {
                    name: "half".into(),
                    value: "bottom".into(),
                },
            ],
        };
        assert_eq!(wire_state(state.clone()), wire);
        let answered = to_plugin(ToPlugin::Answer {
            request: 5,
            answer: Answer::Block(state),
        });
        assert_eq!(
            answered.body,
            Some(to_plugin::Body::Answer(v1::Answer {
                request: 5,
                body: Some(answer::Body::Block(wire)),
            }))
        );
    }
}

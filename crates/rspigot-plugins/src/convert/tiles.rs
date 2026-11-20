use rspigot_core::plugins::{
    BlockInfo, ContentsView, FurnaceEdit, SpawnerEdit, TileData, TileEdit, TileView,
};
use rspigot_plugin_protocol::v1::{self, tile_edit::Edit as Wire, tile_state};

use super::Malformed;
use super::shared::{self, required};

pub(super) fn edit(edit: v1::TileEdit) -> Result<TileEdit, Malformed> {
    Ok(match required(edit.edit)? {
        Wire::SignLines(sign) => TileEdit::SignLines {
            front: sign.front,
            lines: sign.lines,
        },
        Wire::SignStyle(style) => TileEdit::SignStyle {
            front: style.front,
            color: style.color,
            glowing: style.glowing,
        },
        Wire::SignWaxed(waxed) => TileEdit::SignWaxed(waxed),
        Wire::InventorySet(set) => TileEdit::InventorySet {
            slot: set.slot,
            item: set.item.map(shared::item),
        },
        Wire::InventoryAdd(item) => TileEdit::InventoryAdd(shared::item(item)),
        Wire::InventoryClear(_) => TileEdit::InventoryClear,
        Wire::Furnace(furnace) => TileEdit::Furnace(FurnaceEdit {
            burn_time: furnace.burn_time,
            cook_time: furnace.cook_time,
            cook_time_total: furnace.cook_time_total,
        }),
        Wire::Spawner(spawner) => TileEdit::Spawner(SpawnerEdit {
            entity: spawner.entity,
            delay: spawner.delay,
            min_delay: spawner.min_delay,
            max_delay: spawner.max_delay,
            spawn_count: spawner.spawn_count,
            max_nearby: spawner.max_nearby,
            required_player_range: spawner.required_player_range,
            spawn_range: spawner.spawn_range,
        }),
        Wire::SkullOwner(owner) => TileEdit::SkullOwner {
            name: owner.name,
            uuid: owner.uuid.map(shared::uuid).transpose()?,
        },
        Wire::BannerPatterns(banner) => TileEdit::BannerPatterns(patterns(banner)),
        Wire::CommandBlock(command) => TileEdit::CommandBlock {
            command: command.command,
            name: command.name,
        },
        Wire::Beacon(effects) => TileEdit::Beacon {
            primary: effects.primary,
            secondary: effects.secondary,
        },
        Wire::JukeboxDisc(item) => TileEdit::JukeboxDisc(shared::item(item)),
        Wire::JukeboxClear(_) => TileEdit::JukeboxClear,
        Wire::EndGateway(exit) => TileEdit::EndGateway {
            exit: shared::block_pos(required(exit.exit)?),
            exact: exit.exact,
        },
        Wire::LecternBook(item) => TileEdit::LecternBook(shared::item(item)),
        Wire::LecternClear(_) => TileEdit::LecternClear,
        Wire::DecoratedPot(pot) => TileEdit::DecoratedPot(pot.sherds),
    })
}

fn patterns(banner: v1::BannerPatterns) -> Vec<(String, String)> {
    banner
        .patterns
        .into_iter()
        .map(|pattern| (pattern.pattern, pattern.color))
        .collect()
}

fn wire_patterns(patterns: Vec<(String, String)>) -> v1::BannerPatterns {
    v1::BannerPatterns {
        patterns: patterns
            .into_iter()
            .map(|(pattern, color)| v1::BannerPattern { pattern, color })
            .collect(),
    }
}

fn contents(contents: ContentsView) -> v1::InventoryContents {
    v1::InventoryContents {
        size: i32::try_from(contents.size).unwrap_or(i32::MAX),
        slots: contents
            .slots
            .into_iter()
            .map(|(slot, item)| shared::wire_slot((i32::try_from(slot).unwrap_or(i32::MAX), item)))
            .collect(),
    }
}

pub(super) fn wire_tile(view: TileView) -> v1::TileState {
    let state = match view.data {
        TileData::Sign(sign) => tile_state::State::Sign(v1::SignState {
            front: sign.front,
            back: sign.back,
            waxed: sign.waxed,
            front_color: sign.front_color,
            back_color: sign.back_color,
            front_glowing: sign.front_glowing,
            back_glowing: sign.back_glowing,
        }),
        TileData::Inventory(inventory) => tile_state::State::Inventory(contents(inventory)),
        TileData::Furnace(furnace) => tile_state::State::Furnace(v1::FurnaceState {
            burn_time: furnace.burn_time,
            cook_time: furnace.cook_time,
            cook_time_total: furnace.cook_time_total,
            lit: furnace.lit,
            contents: Some(contents(furnace.contents)),
        }),
        TileData::Spawner(spawner) => tile_state::State::Spawner(v1::SpawnerState {
            entity: spawner.entity,
            delay: spawner.delay,
            min_delay: spawner.min_delay,
            max_delay: spawner.max_delay,
            spawn_count: spawner.spawn_count,
            max_nearby: spawner.max_nearby,
            required_player_range: spawner.required_player_range,
            spawn_range: spawner.spawn_range,
        }),
        TileData::Skull { name, uuid } => tile_state::State::Skull(v1::SkullOwner {
            name,
            uuid: uuid.map(|uuid| uuid.hyphenated().to_string()),
        }),
        TileData::Banner(banner) => tile_state::State::Banner(wire_patterns(banner)),
        TileData::CommandBlock { command, name } => {
            tile_state::State::CommandBlock(v1::CommandState { command, name })
        }
        TileData::Beacon {
            levels,
            primary,
            secondary,
        } => tile_state::State::Beacon(v1::BeaconState {
            levels,
            primary,
            secondary,
        }),
        TileData::Jukebox(item) => tile_state::State::Jukebox(v1::DiscState {
            item: item.map(shared::wire_item),
        }),
        TileData::Lectern(item) => tile_state::State::Lectern(v1::DiscState {
            item: item.map(shared::wire_item),
        }),
        TileData::EndGateway { exit, exact } => tile_state::State::EndGateway(v1::GatewayExit {
            exit: exit.map(shared::wire_block_pos),
            exact,
        }),
        TileData::DecoratedPot(sherds) => tile_state::State::DecoratedPot(v1::PotSherds { sherds }),
    };
    v1::TileState {
        kind: view.kind,
        state: Some(state),
    }
}

pub(super) fn wire_info(info: BlockInfo) -> v1::BlockInfo {
    v1::BlockInfo {
        light: info.light,
        sky_light: info.sky_light,
        block_light: info.block_light,
        power: info.power,
        biome: info.biome,
    }
}

#[cfg(test)]
mod tests {
    use rspigot_core::plugins::{Answer, ItemSpec, SignView, TileData, TileEdit, TileView};
    use rspigot_core::plugins::{BlockInfo, BlockOrder, FromPlugin, Order, Question, ToPlugin};
    use rspigot_domain::BlockPos;
    use rspigot_plugin_protocol::v1::{self, answer, block_order, tile_edit, tile_state};

    use crate::convert::{Malformed, from_plugin, question, to_plugin};

    const AT: v1::BlockPos = v1::BlockPos { x: 2, y: 64, z: -3 };

    fn convert(order: block_order::Order) -> Result<FromPlugin, Malformed> {
        from_plugin(
            v1::FromPlugin {
                body: Some(from_plugin::Body::Block(v1::BlockOrder {
                    pos: Some(AT),
                    order: Some(order),
                })),
                origin: None,
                request: None,
                in_world: None,
            },
            &mut Vec::new(),
        )
    }

    fn tile(edit: tile_edit::Edit) -> Result<FromPlugin, Malformed> {
        convert(block_order::Order::Tile(v1::TileEdit { edit: Some(edit) }))
    }

    fn expected(order: BlockOrder) -> FromPlugin {
        FromPlugin::Order(Order::Block {
            pos: BlockPos::new(2, 64, -3),
            order,
        })
    }

    #[test]
    fn tile_orders_convert_and_an_empty_one_is_malformed() {
        assert_eq!(
            tile(tile_edit::Edit::SignLines(v1::SignLines {
                front: false,
                lines: vec!["a".into()],
            }))
            .unwrap(),
            expected(BlockOrder::Tile(TileEdit::SignLines {
                front: false,
                lines: vec!["a".into()],
            }))
        );
        assert_eq!(
            tile(tile_edit::Edit::InventoryAdd(v1::Item {
                key: "minecraft:stone".into(),
                count: 2,
                ..Default::default()
            }))
            .unwrap(),
            expected(BlockOrder::Tile(TileEdit::InventoryAdd(ItemSpec::of(
                "minecraft:stone",
                2
            ))))
        );
        assert_eq!(
            tile(tile_edit::Edit::EndGateway(v1::GatewayExit {
                exit: Some(AT),
                exact: true,
            }))
            .unwrap(),
            expected(BlockOrder::Tile(TileEdit::EndGateway {
                exit: BlockPos::new(2, 64, -3),
                exact: true,
            }))
        );
        assert!(matches!(
            tile(tile_edit::Edit::EndGateway(v1::GatewayExit {
                exit: None,
                exact: false,
            })),
            Err(Malformed::Empty)
        ));
        assert!(matches!(
            tile(tile_edit::Edit::SkullOwner(v1::SkullOwner {
                name: None,
                uuid: Some("nope".into()),
            })),
            Err(Malformed::Uuid(_))
        ));
        assert!(matches!(
            convert(block_order::Order::Tile(v1::TileEdit { edit: None })),
            Err(Malformed::Empty)
        ));
        assert_eq!(
            convert(block_order::Order::SetData(v1::BlockDataEdit {
                properties: vec![v1::Property {
                    name: "facing".into(),
                    value: "north".into(),
                }],
            }))
            .unwrap(),
            expected(BlockOrder::SetData(vec![("facing".into(), "north".into())]))
        );
    }

    #[test]
    fn the_new_questions_and_answers_convert() {
        let ask = |asked| {
            from_plugin(
                v1::FromPlugin {
                    body: Some(from_plugin::Body::Question(v1::Question {
                        request: 5,
                        question: Some(asked),
                    })),
                    origin: None,
                    request: None,
                    in_world: None,
                },
                &mut Vec::new(),
            )
        };
        assert_eq!(
            ask(question::Question::Tile(AT)).unwrap(),
            FromPlugin::Question {
                request: 5,
                question: Question::Tile(BlockPos::new(2, 64, -3)),
            }
        );
        assert_eq!(
            ask(question::Question::BlockInfo(AT)).unwrap(),
            FromPlugin::Question {
                request: 5,
                question: Question::BlockInfo(BlockPos::new(2, 64, -3)),
            }
        );
        let body = |answer: Answer| match to_plugin(ToPlugin::Answer { request: 5, answer }).body {
            Some(rspigot_plugin_protocol::v1::to_plugin::Body::Answer(v1::Answer {
                body, ..
            })) => body,
            other => panic!("expected an answer, got {other:?}"),
        };
        let sign = SignView {
            front: vec!["a".into(); 4],
            back: vec![String::new(); 4],
            waxed: true,
            front_color: "red".into(),
            back_color: "black".into(),
            front_glowing: true,
            back_glowing: false,
        };
        let Some(answer::Body::Tile(wire)) = body(Answer::Tile(TileView {
            kind: "minecraft:sign".into(),
            data: TileData::Sign(sign),
        })) else {
            panic!("expected a tile");
        };
        assert_eq!(wire.kind, "minecraft:sign");
        assert!(matches!(
            wire.state,
            Some(tile_state::State::Sign(v1::SignState { waxed: true, .. }))
        ));
        assert_eq!(
            body(Answer::BlockInfo(BlockInfo {
                light: 15,
                sky_light: 15,
                block_light: 0,
                power: 4,
                biome: "minecraft:plains".into(),
            })),
            Some(answer::Body::BlockInfo(v1::BlockInfo {
                light: 15,
                sky_light: 15,
                block_light: 0,
                power: 4,
                biome: "minecraft:plains".into(),
            }))
        );
    }
}

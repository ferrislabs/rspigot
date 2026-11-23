use prost::Message;
use rspigot_plugin_protocol::v1::{
    self, ClientBlock, ClientBlocks, CreateInventory, FromPlugin, InventoryKind, LookupAnswer,
    LookupQuestion, PackOffer, ScheduleTask, TaskDue, ToPlugin, from_plugin, lookup_answer,
    lookup_question, player_extra, server_order, to_plugin,
};

fn through<M: Message + Default>(message: &M) -> M {
    M::decode(message.encode_to_vec().as_slice()).unwrap()
}

#[test]
fn a_scheduled_task_and_its_due_notice_survive_the_wire() {
    let order = FromPlugin {
        body: Some(from_plugin::Body::Server(v1::ServerOrder {
            order: Some(server_order::Order::ScheduleTask(ScheduleTask {
                id: 9,
                delay: 20,
                period: 5,
            })),
        })),
        origin: None,
        request: Some(3),
        in_world: None,
    };
    assert_eq!(through(&order), order);
    let due = ToPlugin {
        body: Some(to_plugin::Body::TaskDue(TaskDue { id: 9 })),
    };
    assert_eq!(through(&due), due);
}

#[test]
fn an_inventory_kind_with_trades_and_client_changes_survive_the_wire() {
    let create = CreateInventory {
        id: 1,
        title: "Shop".to_owned(),
        kind: InventoryKind::Merchant as i32,
        slots: 0,
        trades: vec![v1::Trade::default()],
        ..Default::default()
    };
    assert_eq!(through(&create), create);
    let blocks = v1::PlayerExtra {
        order: Some(player_extra::Order::SendBlockChanges(ClientBlocks {
            changes: vec![ClientBlock {
                pos: Some(v1::BlockPos { x: 1, y: 2, z: 3 }),
                state: Some(v1::BlockState {
                    block: "minecraft:stone".to_owned(),
                    properties: Vec::new(),
                }),
            }],
        })),
    };
    assert_eq!(through(&blocks), blocks);
    let pack = v1::PlayerExtra {
        order: Some(player_extra::Order::SendResourcePack(PackOffer {
            id: "00000000-0000-0000-0000-000000000001".to_owned(),
            url: "https://example.org/p.zip".to_owned(),
            hash: "00".to_owned(),
            required: true,
            prompt: Some("hi".to_owned()),
        })),
    };
    assert_eq!(through(&pack), pack);
}

#[test]
fn lookups_survive_the_wire() {
    let question = LookupQuestion {
        query: Some(lookup_question::Query::GenerateLoot(v1::LootQuery {
            table: "minecraft:chests/x".to_owned(),
            position: Some(v1::Vec3 {
                x: 1.0,
                y: 2.0,
                z: 3.0,
            }),
            looter: None,
            luck: 0.5,
            seed: Some(-4),
        })),
    };
    assert_eq!(through(&question), question);
    let answer = LookupAnswer {
        answer: Some(lookup_answer::Answer::Located(v1::StructureLocation {
            structure: Some("minecraft:village_plains".to_owned()),
            position: Some(v1::Vec3 {
                x: 1.0,
                y: 2.0,
                z: 3.0,
            }),
        })),
    };
    assert_eq!(through(&answer), answer);
}

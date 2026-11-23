use rspigot_core::plugins::{
    Answer, BlockOrder, BorderOrder, CustomInventoryOrder, EntityOrder, FluidMode, FromPlugin,
    IngredientSpec, LookupAnswer, LookupQuestion, LootTarget, MerchantProgress, ObjectQuestion,
    Order, PdcChange, PdcHolder, PdcValue, PlayerAction, PlayerOrder, ProfileInfo, ProfileLookup,
    Question, RecipeShape, ServerOrder, ToPlugin, WorldObjectOrder, WorldOrder,
};
use rspigot_domain::player::PlayerId;
use rspigot_domain::{BlockPos, ChunkPos, EntityId};
use rspigot_plugin_protocol::v1::{
    self, answer, block_order, custom_inventory_order, entity_order, extra_question, from_plugin,
    lookup_answer, lookup_question, object_world_order, pdc_query, player_extra, player_order,
    question, recipe_def, recipe_ingredient, server_order, to_plugin, world_order,
};
use uuid::Uuid;

use super::{from_plugin as decode, to_plugin as encode};

const STEVE: &str = "00000000-0000-0000-0000-000000000001";

fn convert(body: from_plugin::Body) -> FromPlugin {
    decode(
        v1::FromPlugin {
            body: Some(body),
            origin: None,
            request: None,
            in_world: None,
        },
        &mut Vec::new(),
    )
    .expect("the message converts")
}

fn extra(order: player_extra::Order) -> from_plugin::Body {
    from_plugin::Body::Player(v1::PlayerOrder {
        player: STEVE.to_owned(),
        order: Some(player_order::Order::Extra(v1::PlayerExtra {
            order: Some(order),
        })),
    })
}

fn action(order: player_extra::Order) -> PlayerAction {
    match convert(extra(order)) {
        FromPlugin::Order(Order::Player {
            order: PlayerOrder::Action(action),
            ..
        }) => action,
        other => panic!("a player action expected, got {other:?}"),
    }
}

fn lookup(query: lookup_question::Query) -> LookupQuestion {
    match convert(from_plugin::Body::Question(v1::Question {
        request: 1,
        question: Some(question::Question::Lookup(v1::LookupQuestion {
            query: Some(query),
        })),
    })) {
        FromPlugin::Question {
            question: Question::Lookup(lookup),
            ..
        } => lookup,
        other => panic!("a lookup expected, got {other:?}"),
    }
}

#[test]
fn viewer_cookie_link_and_criterion_orders_convert() {
    assert_eq!(
        action(player_extra::Order::AwardCriterion(v1::CriterionOrder {
            advancement: "a:b".to_owned(),
            criterion: "c".to_owned(),
        })),
        PlayerAction::AwardCriterion {
            advancement: "a:b".to_owned(),
            criterion: "c".to_owned()
        }
    );
    assert_eq!(
        action(player_extra::Order::HideEntity(9)),
        PlayerAction::HideEntity(EntityId(9))
    );
    assert_eq!(
        action(player_extra::Order::SetPlayerBorder(v1::WorldBorderOrder {
            change: Some(v1::world_border_order::Change::SetCenter(
                v1::BorderCenter { x: 1.0, z: 2.0 }
            )),
        })),
        PlayerAction::SetBorder(BorderOrder::SetCenter { x: 1.0, z: 2.0 })
    );
    assert_eq!(
        action(player_extra::Order::ResetPlayerBorder(v1::Empty {})),
        PlayerAction::ResetBorder
    );
    assert_eq!(
        action(player_extra::Order::StoreCookie(v1::CookieStore {
            key: "plugin:k".to_owned(),
            payload: vec![1],
        })),
        PlayerAction::StoreCookie {
            key: "plugin:k".to_owned(),
            payload: vec![1]
        }
    );
    assert_eq!(
        action(player_extra::Order::SendLinks(v1::LinkList {
            links: vec![v1::LinkEntry {
                label: "l".to_owned(),
                url: "u".to_owned()
            }]
        })),
        PlayerAction::SendLinks(vec![("l".to_owned(), "u".to_owned())])
    );
}

#[test]
fn a_cookie_question_and_its_answer_convert() {
    let asked = convert(from_plugin::Body::Question(v1::Question {
        request: 4,
        question: Some(question::Question::Extra(v1::ExtraQuestion {
            query: Some(extra_question::Query::RetrieveCookie(v1::CookieQuery {
                player: STEVE.to_owned(),
                key: "plugin:k".to_owned(),
            })),
        })),
    }));
    assert!(matches!(
        asked,
        FromPlugin::Question {
            question: Question::Extra(rspigot_core::plugins::ExtraQuestion::RetrieveCookie { .. }),
            ..
        }
    ));
    let Some(to_plugin::Body::Answer(v1::Answer {
        body:
            Some(answer::Body::Extra(v1::ExtraAnswer {
                answer: Some(heard),
            })),
        ..
    })) = encode(ToPlugin::Answer {
        request: 4,
        answer: Answer::Extra(rspigot_core::plugins::ExtraAnswer::Cookie(Some(vec![3]))),
    })
    .body
    else {
        panic!("an extra answer");
    };
    assert_eq!(
        heard,
        v1::extra_answer::Answer::Cookie(v1::CookieValue {
            payload: Some(vec![3])
        })
    );
}

#[test]
fn block_and_entity_loot_biome_and_chunk_pdc_orders_convert() {
    let block = |order| {
        convert(from_plugin::Body::Block(v1::BlockOrder {
            pos: Some(v1::BlockPos { x: 1, y: 2, z: 3 }),
            order: Some(order),
        }))
    };
    assert_eq!(
        block(block_order::Order::SetBiome("minecraft:desert".to_owned())),
        FromPlugin::Order(Order::Block {
            pos: BlockPos::new(1, 2, 3),
            order: BlockOrder::SetBiome("minecraft:desert".to_owned())
        })
    );
    assert_eq!(
        block(block_order::Order::SetLootTable(v1::LootTableSet {
            table: Some("t".to_owned()),
            seed: 4
        })),
        FromPlugin::Order(Order::Block {
            pos: BlockPos::new(1, 2, 3),
            order: BlockOrder::SetLootTable {
                table: Some("t".to_owned()),
                seed: 4
            }
        })
    );
    assert_eq!(
        convert(from_plugin::Body::Entity(v1::EntityOrder {
            entity: 8,
            order: Some(entity_order::Order::SetLootTable(v1::LootTableSet {
                table: None,
                seed: 0
            })),
        })),
        FromPlugin::Order(Order::Entity {
            entity: EntityId(8),
            order: EntityOrder::SetLootTable {
                table: None,
                seed: 0
            }
        })
    );
    let set = v1::PdcChange {
        change: Some(v1::pdc_change::Change::Set(v1::PdcSet {
            key: "plugin:k".to_owned(),
            value: Some(v1::PdcValue {
                value: Some(v1::pdc_value::Value::Int(5)),
            }),
        })),
    };
    assert_eq!(
        convert(from_plugin::Body::World(v1::WorldOrder {
            order: Some(world_order::Order::Object(v1::ObjectWorldOrder {
                order: Some(object_world_order::Order::ChunkPdc(v1::ChunkPdcChange {
                    x: -1,
                    z: 2,
                    change: Some(set.clone()),
                })),
            })),
        })),
        FromPlugin::Order(Order::World(WorldOrder::Object(
            WorldObjectOrder::ChunkPdc {
                chunk: ChunkPos::new(-1, 2),
                change: PdcChange::Set {
                    key: "plugin:k".to_owned(),
                    value: PdcValue::Int(5)
                }
            }
        )))
    );
    let holder = |holder| match convert(from_plugin::Body::Question(v1::Question {
        request: 2,
        question: Some(question::Question::Object(v1::ObjectQuestion {
            query: Some(v1::object_question::Query::Pdc(v1::PdcQuery {
                holder: Some(holder),
            })),
        })),
    })) {
        FromPlugin::Question {
            question: Question::Object(ObjectQuestion::Pdc(holder)),
            ..
        } => holder,
        other => panic!("a pdc question, got {other:?}"),
    };
    assert_eq!(
        holder(pdc_query::Holder::Chunk(v1::ChunkCoords { x: 3, z: 4 })),
        PdcHolder::Chunk(ChunkPos::new(3, 4))
    );
    assert_eq!(
        holder(pdc_query::Holder::World(v1::Empty {})),
        PdcHolder::World
    );
}

#[test]
fn ray_tree_profile_and_lootable_questions_convert() {
    let LookupQuestion::RayTrace(ray) = lookup(lookup_question::Query::RayTrace(v1::RayTrace {
        origin: Some(v1::Vec3 {
            x: 1.0,
            y: 2.0,
            z: 3.0,
        }),
        direction: Some(v1::Vec3 {
            x: 0.0,
            y: -1.0,
            z: 0.0,
        }),
        max_distance: 9.0,
        blocks: true,
        entities: false,
        fluid: v1::FluidMode::Always as i32,
        ignore_passable: true,
        ray_size: 0.5,
        ignore_entity: Some(7),
    })) else {
        panic!("a ray");
    };
    assert_eq!(
        (ray.fluid, ray.ignore_entity, ray.entities),
        (FluidMode::Always, Some(EntityId(7)), false)
    );
    assert_eq!(
        lookup(lookup_question::Query::GenerateTree(v1::GenerateTree {
            pos: Some(v1::BlockPos { x: 1, y: 2, z: 3 }),
            feature: "oak".to_owned(),
        })),
        LookupQuestion::GenerateTree {
            pos: BlockPos::new(1, 2, 3),
            feature: "oak".to_owned()
        }
    );
    assert_eq!(
        lookup(lookup_question::Query::Profile(v1::ProfileQuery {
            by: Some(v1::profile_query::By::Name("Steve".to_owned())),
        })),
        LookupQuestion::Profile(ProfileLookup::Name("Steve".to_owned()))
    );
    assert_eq!(
        lookup(lookup_question::Query::Lootable(v1::LootTarget {
            target: Some(v1::loot_target::Target::Entity(3)),
        })),
        LookupQuestion::Lootable(LootTarget::Entity(EntityId(3)))
    );
    assert_eq!(
        lookup(lookup_question::Query::LootTables(v1::Empty {})),
        LookupQuestion::LootTables
    );
}

#[test]
fn a_bad_fluid_mode_or_profile_uuid_is_malformed() {
    let bad = |query| {
        decode(
            v1::FromPlugin {
                body: Some(from_plugin::Body::Question(v1::Question {
                    request: 1,
                    question: Some(question::Question::Lookup(v1::LookupQuestion {
                        query: Some(query),
                    })),
                })),
                origin: None,
                request: None,
                in_world: None,
            },
            &mut Vec::new(),
        )
        .is_err()
    };
    assert!(bad(lookup_question::Query::RayTrace(v1::RayTrace {
        origin: Some(v1::Vec3::default()),
        direction: Some(v1::Vec3::default()),
        fluid: 99,
        ..Default::default()
    })));
    assert!(bad(lookup_question::Query::Profile(v1::ProfileQuery {
        by: Some(v1::profile_query::By::Uuid("nope".to_owned())),
    })));
}

#[test]
fn lookup_answers_for_rays_profiles_and_lootables_go_on_the_wire() {
    let wire = |answer| {
        let Some(to_plugin::Body::Answer(v1::Answer {
            body:
                Some(answer::Body::Lookup(v1::LookupAnswer {
                    answer: Some(heard),
                })),
            ..
        })) = encode(ToPlugin::Answer {
            request: 1,
            answer: Answer::Lookup(answer),
        })
        .body
        else {
            panic!("a lookup answer");
        };
        heard
    };
    let id = PlayerId(Uuid::from_u128(1));
    assert_eq!(
        wire(LookupAnswer::Profile(ProfileInfo {
            name: "Steve".to_owned(),
            uuid: id.0,
            textures: Some(("v".to_owned(), None)),
            online: true,
        })),
        lookup_answer::Answer::Profile(v1::ProfileInfo {
            name: "Steve".to_owned(),
            uuid: id.0.to_string(),
            textures: Some(v1::ProfileTextures {
                value: "v".to_owned(),
                signature: None
            }),
            online: true,
        })
    );
    assert_eq!(
        wire(LookupAnswer::Lootable {
            table: Some("t".to_owned()),
            seed: 3
        }),
        lookup_answer::Answer::Lootable(v1::LootTableSet {
            table: Some("t".to_owned()),
            seed: 3
        })
    );
    let lookup_answer::Answer::RayHit(hit) =
        wire(LookupAnswer::RayHit(rspigot_core::plugins::RayHit {
            hit: Some(rspigot_core::plugins::TargetHit::Block {
                pos: BlockPos::new(1, 2, 3),
                face: rspigot_core::events::Direction::Up,
                kind: "minecraft:stone".to_owned(),
            }),
            position: rspigot_domain::Vec3::new(1.5, 3.0, 3.5),
            distance: 2.0,
        }))
    else {
        panic!("a ray hit");
    };
    assert_eq!(hit.distance, 2.0);
    assert!(matches!(
        hit.hit.and_then(|target| target.hit),
        Some(v1::target::Hit::Block(_))
    ));
}

#[test]
fn merchant_progress_and_the_new_recipe_bodies_convert() {
    let progress = v1::MerchantProgress {
        level: 4,
        xp: 9,
        show_progress: true,
        can_restock: false,
    };
    assert_eq!(
        convert(from_plugin::Body::CustomInventory(
            v1::CustomInventoryOrder {
                inventory: 2,
                order: Some(custom_inventory_order::Order::SetProgress(progress)),
            }
        )),
        FromPlugin::Order(Order::CustomInventory {
            inventory: 2,
            order: CustomInventoryOrder::SetProgress(MerchantProgress {
                level: 4,
                xp: 9,
                show_progress: true,
                can_restock: false
            })
        })
    );
    let ingredient = |choice| v1::RecipeIngredient {
        choice: Some(choice),
    };
    let items = |key: &str| {
        ingredient(recipe_ingredient::Choice::Items(v1::ItemKeys {
            keys: vec![key.to_owned()],
        }))
    };
    let def = |body| match convert(from_plugin::Body::Server(v1::ServerOrder {
        order: Some(server_order::Order::AddRecipe(v1::RecipeDef {
            id: "plugin:r".to_owned(),
            result: Some(v1::Item {
                key: "minecraft:stick".to_owned(),
                count: 1,
                ..Default::default()
            }),
            body: Some(body),
            ..Default::default()
        })),
    })) {
        FromPlugin::Order(Order::Server(ServerOrder::AddRecipe(spec))) => spec.shape,
        other => panic!("a recipe, got {other:?}"),
    };
    assert!(matches!(
        def(recipe_def::Body::Transmute(v1::TransmuteRecipe {
            input: Some(items("minecraft:stick")),
            material: Some(ingredient(recipe_ingredient::Choice::Exact(v1::Item {
                key: "minecraft:emerald".to_owned(),
                count: 1,
                ..Default::default()
            }))),
        })),
        RecipeShape::Transmute {
            material: IngredientSpec::Exact(_),
            ..
        }
    ));
    assert!(matches!(
        def(recipe_def::Body::SmithingTrim(v1::SmithingTrimRecipe {
            template: Some(items("minecraft:stick")),
            base: Some(items("minecraft:stick")),
            addition: Some(items("minecraft:stick")),
            pattern: "minecraft:coast".to_owned(),
        })),
        RecipeShape::SmithingTrim { pattern, .. } if pattern == "minecraft:coast"
    ));
}

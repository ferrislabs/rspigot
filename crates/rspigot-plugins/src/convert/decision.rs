use rspigot_core::plugins::{FromPlugin, Verdict};
use rspigot_plugin_protocol::v1;

use super::events::change;

pub(super) fn decision(decision: v1::Decision, warnings: &mut Vec<String>) -> FromPlugin {
    let event = decision.event;
    FromPlugin::Decision {
        event,
        verdicts: decision
            .verdicts
            .into_iter()
            .map(|verdict| convert(event, verdict, warnings))
            .collect(),
    }
}

fn convert(event: u64, verdict: v1::Verdict, warnings: &mut Vec<String>) -> Verdict {
    let handler = verdict.handler;
    let patch = verdict
        .patch
        .and_then(|patch| patch.change)
        .and_then(|patched| match change(patched) {
            Ok(patch) => Some(patch),
            Err(error) => {
                warnings.push(format!(
                    "patch of handler {handler} for event {event} was dropped: {error}"
                ));
                None
            }
        });
    Verdict {
        handler,
        cancel: verdict.cancel,
        patch,
    }
}

#[cfg(test)]
mod tests {
    use rspigot_core::plugins::events::{
        EntityTargetPatch, PlayerCommandSendPatch, PlayerMovePatch, PotionSplashPatch,
    };
    use rspigot_core::plugins::{EntityView, EventPatch, ListDiff, Location};
    use rspigot_domain::{EntityId, Rotation, Vec3};
    use rspigot_plugin_protocol::v1::{from_plugin, patch};
    use uuid::Uuid;

    use super::*;
    use crate::convert;

    fn moving(to: Option<v1::Location>) -> Option<v1::Patch> {
        Some(v1::Patch {
            change: Some(patch::Change::PlayerMove(v1::PlayerMovePatch { to })),
        })
    }

    fn targeting(target: Option<v1::NullableEntitySnapshot>) -> Option<v1::Patch> {
        Some(v1::Patch {
            change: Some(patch::Change::EntityTarget(v1::EntityTargetPatch {
                target,
            })),
        })
    }

    fn decide(verdicts: Vec<v1::Verdict>, warnings: &mut Vec<String>) -> FromPlugin {
        convert::from_plugin(
            v1::FromPlugin {
                body: Some(from_plugin::Body::Decision(v1::Decision {
                    event: 7,
                    verdicts,
                })),
                origin: None,
                request: None,
                in_world: None,
            },
            warnings,
        )
        .expect("a decision always converts")
    }

    fn zombie() -> v1::EntitySnapshot {
        v1::EntitySnapshot {
            entity_id: 4,
            uuid: "00000000-0000-0000-0000-000000000004".to_owned(),
            kind: "minecraft:zombie".to_owned(),
            position: Some(v1::Vec3 {
                x: 0.0,
                y: 64.0,
                z: 0.0,
            }),
            player: None,
        }
    }

    #[test]
    fn a_decision_converts_every_verdict_with_its_patch() {
        let verdicts = vec![
            v1::Verdict {
                handler: 2,
                cancel: false,
                patch: moving(Some(v1::Location {
                    world: "minecraft:overworld".to_owned(),
                    position: Some(v1::Vec3 {
                        x: 4.5,
                        y: 70.0,
                        z: -1.5,
                    }),
                    rotation: Some(v1::Rotation {
                        yaw: 0.0,
                        pitch: 0.0,
                    }),
                })),
            },
            v1::Verdict {
                handler: 4,
                cancel: true,
                patch: None,
            },
            v1::Verdict {
                handler: 5,
                cancel: false,
                patch: moving(None),
            },
            v1::Verdict {
                handler: 6,
                cancel: false,
                patch: Some(v1::Patch { change: None }),
            },
        ];
        let mut warnings = Vec::new();
        assert_eq!(
            decide(verdicts, &mut warnings),
            FromPlugin::Decision {
                event: 7,
                verdicts: vec![
                    Verdict {
                        handler: 2,
                        cancel: false,
                        patch: Some(EventPatch::PlayerMove(PlayerMovePatch {
                            to: Some(Location {
                                world: "minecraft:overworld".to_owned(),
                                position: Vec3::new(4.5, 70.0, -1.5),
                                rotation: Rotation::default(),
                            }),
                        })),
                    },
                    Verdict {
                        handler: 4,
                        cancel: true,
                        patch: None,
                    },
                    Verdict {
                        handler: 5,
                        cancel: false,
                        patch: Some(EventPatch::PlayerMove(PlayerMovePatch { to: None })),
                    },
                    Verdict {
                        handler: 6,
                        cancel: false,
                        patch: None,
                    },
                ],
            }
        );
        assert!(warnings.is_empty());
    }

    #[test]
    fn an_indexed_patch_converts_each_entity_and_its_value() {
        let splash = |entries| {
            vec![v1::Verdict {
                handler: 1,
                cancel: false,
                patch: Some(v1::Patch {
                    change: Some(patch::Change::PotionSplash(v1::PotionSplashPatch {
                        intensities: entries,
                    })),
                }),
            }]
        };
        let mut warnings = Vec::new();
        let FromPlugin::Decision { verdicts, .. } = decide(
            splash(vec![v1::EntitySnapshotDoubleEntry {
                key: Some(zombie()),
                value: 0.25,
            }]),
            &mut warnings,
        ) else {
            panic!("expected a decision");
        };
        assert!(warnings.is_empty());
        assert_eq!(
            verdicts[0].patch,
            Some(EventPatch::PotionSplash(PotionSplashPatch {
                intensities: vec![(
                    EntityView {
                        id: EntityId(4),
                        uuid: Uuid::from_u128(4),
                        kind: "minecraft:zombie".to_owned(),
                        position: Vec3::new(0.0, 64.0, 0.0),
                        player: None,
                    },
                    0.25
                )],
            }))
        );
        decide(
            splash(vec![v1::EntitySnapshotDoubleEntry {
                key: None,
                value: 0.25,
            }]),
            &mut warnings,
        );
        assert_eq!(
            warnings,
            ["patch of handler 1 for event 7 was dropped: the message carries nothing"]
        );
    }

    #[test]
    fn a_list_patch_converts_its_removals_and_each_addition() {
        let commands = |commands| {
            vec![v1::Verdict {
                handler: 1,
                cancel: false,
                patch: Some(v1::Patch {
                    change: Some(patch::Change::PlayerCommandSend(
                        v1::PlayerCommandSendPatch { commands },
                    )),
                }),
            }]
        };
        let mut warnings = Vec::new();
        let FromPlugin::Decision { verdicts, .. } = decide(
            commands(Some(v1::StringListDiff {
                removed: vec![3, 0],
                replaced: vec![v1::StringReplacement {
                    index: 1,
                    value: "tp".to_owned(),
                }],
                added: vec!["warp".to_owned()],
            })),
            &mut warnings,
        ) else {
            panic!("expected a decision");
        };
        assert_eq!(
            verdicts[0].patch,
            Some(EventPatch::PlayerCommandSend(PlayerCommandSendPatch {
                commands: ListDiff {
                    removed: vec![3, 0],
                    replaced: vec![(1, "tp".to_owned())],
                    added: vec!["warp".to_owned()],
                },
            }))
        );
        let FromPlugin::Decision { verdicts, .. } = decide(commands(None), &mut warnings) else {
            panic!("expected a decision");
        };
        assert_eq!(
            verdicts[0].patch,
            Some(EventPatch::PlayerCommandSend(
                PlayerCommandSendPatch::default()
            ))
        );
        let mut stranger = zombie();
        stranger.uuid = "nobody".to_owned();
        let dropped = v1::DroppedItem {
            entity: Some(stranger),
            item: Some(v1::Item {
                key: "minecraft:dirt".to_owned(),
                count: 1,
                ..v1::Item::default()
            }),
        };
        let FromPlugin::Decision { verdicts, .. } = decide(
            vec![v1::Verdict {
                handler: 2,
                cancel: false,
                patch: Some(v1::Patch {
                    change: Some(patch::Change::BlockDropItem(v1::BlockDropItemPatch {
                        items: Some(v1::DroppedItemListDiff {
                            removed: Vec::new(),
                            replaced: vec![v1::DroppedItemReplacement {
                                index: 0,
                                value: Some(dropped),
                            }],
                            added: Vec::new(),
                        }),
                    })),
                }),
            }],
            &mut warnings,
        ) else {
            panic!("expected a decision");
        };
        assert_eq!(verdicts[0].patch, None);
        assert_eq!(
            warnings,
            ["patch of handler 2 for event 7 was dropped: `nobody` is not an entity UUID"]
        );
    }

    #[test]
    fn a_nullable_patch_tells_a_cleared_value_from_an_untouched_one() {
        let target = |patch| {
            let FromPlugin::Decision { verdicts, .. } = decide(
                vec![v1::Verdict {
                    handler: 1,
                    cancel: false,
                    patch,
                }],
                &mut Vec::new(),
            ) else {
                panic!("expected a decision");
            };
            verdicts[0].patch.clone()
        };
        assert_eq!(
            target(targeting(None)),
            Some(EventPatch::EntityTarget(EntityTargetPatch { target: None }))
        );
        assert_eq!(
            target(targeting(Some(v1::NullableEntitySnapshot { value: None }))),
            Some(EventPatch::EntityTarget(EntityTargetPatch {
                target: Some(None)
            }))
        );
        assert_eq!(
            target(targeting(Some(v1::NullableEntitySnapshot {
                value: Some(zombie())
            }))),
            Some(EventPatch::EntityTarget(EntityTargetPatch {
                target: Some(Some(EntityView {
                    id: EntityId(4),
                    uuid: Uuid::from_u128(4),
                    kind: "minecraft:zombie".to_owned(),
                    position: Vec3::new(0.0, 64.0, 0.0),
                    player: None,
                }))
            }))
        );
    }

    #[test]
    fn a_malformed_patch_drops_only_that_patch() {
        let mut stranger = zombie();
        stranger.uuid = "nobody".to_owned();
        let mut warnings = Vec::new();
        let decided = decide(
            vec![
                v1::Verdict {
                    handler: 1,
                    cancel: true,
                    patch: targeting(Some(v1::NullableEntitySnapshot {
                        value: Some(stranger),
                    })),
                },
                v1::Verdict {
                    handler: 2,
                    cancel: false,
                    patch: targeting(None),
                },
            ],
            &mut warnings,
        );
        assert_eq!(
            decided,
            FromPlugin::Decision {
                event: 7,
                verdicts: vec![
                    Verdict {
                        handler: 1,
                        cancel: true,
                        patch: None,
                    },
                    Verdict {
                        handler: 2,
                        cancel: false,
                        patch: Some(EventPatch::EntityTarget(EntityTargetPatch { target: None })),
                    },
                ],
            }
        );
        assert_eq!(
            warnings,
            ["patch of handler 1 for event 7 was dropped: `nobody` is not an entity UUID"]
        );
    }
}

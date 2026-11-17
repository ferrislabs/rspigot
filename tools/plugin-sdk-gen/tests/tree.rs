use plugin_sdk_gen::descriptor::{
    DescriptorProto, FieldDescriptorProto, FieldOptions, FileDescriptorProto, FileDescriptorSet,
    OneofDescriptorProto,
};
use plugin_sdk_gen::tree::Tree;

fn verb(name: &str, number: i32, type_name: &str, path: &str) -> FieldDescriptorProto {
    FieldDescriptorProto {
        name: Some(name.to_owned()),
        number: Some(number),
        r#type: Some(11),
        type_name: Some(type_name.to_owned()),
        oneof_index: Some(0),
        options: Some(FieldOptions {
            path: Some(path.to_owned()),
        }),
    }
}

fn player_order() -> DescriptorProto {
    DescriptorProto {
        name: Some("PlayerOrder".to_owned()),
        field: vec![
            FieldDescriptorProto {
                name: Some("player".to_owned()),
                number: Some(1),
                r#type: Some(9),
                ..Default::default()
            },
            verb(
                "close_inventory",
                48,
                ".rspigot.plugin.v1.Empty",
                "player.inventory.close",
            ),
            verb(
                "play_sound",
                43,
                ".rspigot.plugin.v1.Sound",
                "player.sound.play",
            ),
            FieldDescriptorProto {
                name: Some("kick".to_owned()),
                number: Some(18),
                r#type: Some(9),
                oneof_index: Some(0),
                ..Default::default()
            },
        ],
        oneof_decl: vec![OneofDescriptorProto {
            name: Some("order".to_owned()),
        }],
    }
}

fn from_plugin() -> DescriptorProto {
    DescriptorProto {
        name: Some("FromPlugin".to_owned()),
        field: vec![FieldDescriptorProto {
            name: Some("player".to_owned()),
            number: Some(3),
            r#type: Some(11),
            type_name: Some(".rspigot.plugin.v1.PlayerOrder".to_owned()),
            oneof_index: Some(0),
            ..Default::default()
        }],
        oneof_decl: vec![OneofDescriptorProto {
            name: Some("body".to_owned()),
        }],
    }
}

#[test]
fn paths_build_a_tree_of_handles() {
    let set = FileDescriptorSet {
        file: vec![FileDescriptorProto {
            name: Some("rspigot/plugin/v1/plugin.proto".to_owned()),
            package: Some("rspigot.plugin.v1".to_owned()),
            message_type: vec![player_order(), from_plugin()],
        }],
    };

    let tree = Tree::build(&set).unwrap();

    let handles: Vec<&str> = tree.handles().iter().map(|h| h.path.as_str()).collect();
    assert_eq!(handles, ["player.inventory", "player.sound"]);
    let inventory = &tree.handles()[0];
    assert_eq!(inventory.name, "PlayerInventoryHandle");
    assert_eq!(inventory.verbs.len(), 1);
    assert_eq!(inventory.verbs[0].name, "close");
    assert_eq!(inventory.verbs[0].variant, "CloseInventory");
    let sound = &tree.handles()[1];
    assert_eq!(sound.verbs[0].name, "play");
    assert_eq!(sound.verbs[0].variant, "PlaySound");
    assert_eq!(sound.verbs[0].order, "PlayerOrder");
    assert_eq!(sound.verbs[0].body, "Player");
}

fn field(
    name: &str,
    number: i32,
    kind: i32,
    type_name: Option<&str>,
    path: &str,
) -> FieldDescriptorProto {
    FieldDescriptorProto {
        name: Some(name.to_owned()),
        number: Some(number),
        r#type: Some(kind),
        type_name: type_name.map(str::to_owned),
        oneof_index: Some(0),
        options: Some(FieldOptions {
            path: Some(path.to_owned()),
        }),
    }
}

fn envelope(order: &str, field_name: &str) -> DescriptorProto {
    DescriptorProto {
        name: Some("FromPlugin".to_owned()),
        field: vec![FieldDescriptorProto {
            name: Some(field_name.to_owned()),
            number: Some(4),
            r#type: Some(11),
            type_name: Some(format!(".rspigot.plugin.v1.{order}")),
            oneof_index: Some(0),
            ..Default::default()
        }],
        oneof_decl: vec![OneofDescriptorProto {
            name: Some("body".to_owned()),
        }],
    }
}

fn set_of(messages: Vec<DescriptorProto>) -> FileDescriptorSet {
    FileDescriptorSet {
        file: vec![FileDescriptorProto {
            name: Some("rspigot/plugin/v1/plugin.proto".to_owned()),
            package: Some("rspigot.plugin.v1".to_owned()),
            message_type: messages,
        }],
    }
}

fn targetless(paths: &[(&str, &str)]) -> FileDescriptorSet {
    let fields = paths
        .iter()
        .zip(10..)
        .map(|((name, path), number)| {
            field(name, number, 11, Some(".rspigot.plugin.v1.Sound"), path)
        })
        .collect();
    set_of(vec![
        DescriptorProto {
            name: Some("WorldOrder".to_owned()),
            field: fields,
            oneof_decl: vec![OneofDescriptorProto {
                name: Some("order".to_owned()),
            }],
        },
        envelope("WorldOrder", "world"),
    ])
}

#[test]
fn an_order_without_a_target_field_builds_a_handle_that_holds_no_identifier() {
    let tree = Tree::build(&targetless(&[("play_sound", "world.sound.play")])).unwrap();

    let sound = &tree.handles()[0];
    assert_eq!(sound.name, "WorldSoundHandle");
    assert_eq!(sound.verbs[0].target, None);
    let files = plugin_sdk_gen::emit::files(&tree);
    let source = &files["world_sound.rs"];
    assert!(source.contains("pub(crate) fn new(link: Link) -> Self"));
    assert!(!source.contains("target"));
}

#[test]
fn a_targeted_order_keeps_the_identifier_of_its_handle() {
    let set = set_of(vec![player_order(), from_plugin()]);

    let files = plugin_sdk_gen::emit::files(&Tree::build(&set).unwrap());

    let source = &files["player_sound.rs"];
    assert!(source.contains("pub(crate) fn new(target: Arc<str>, link: Link) -> Self"));
    assert!(source.contains("player: self.target.to_string(),"));
    assert!(source.contains("value: impl Into<v1::Sound>"));
}

#[test]
fn a_verb_is_refused_twice_in_the_same_domain() {
    let set = targetless(&[
        ("play_sound", "world.sound.play"),
        ("stop_sound", "world.sound.play"),
    ]);

    let error = Tree::build(&set).unwrap_err().to_string();

    assert!(error.contains("already taken"), "{error}");
}

#[test]
fn an_enum_payload_is_sent_as_its_number() {
    let mut order = player_order();
    order.field.push(field(
        "set_game_mode",
        17,
        14,
        Some(".rspigot.plugin.v1.GameMode"),
        "player.game_mode.set",
    ));

    let files =
        plugin_sdk_gen::emit::files(&Tree::build(&set_of(vec![order, from_plugin()])).unwrap());

    let source = &files["player_game_mode.rs"];
    assert!(source.contains("value: impl Into<v1::GameMode>"));
    assert!(source.contains("Into::<v1::GameMode>::into(value) as i32"));
}

#[test]
fn a_domain_shadowing_a_snapshot_field_carries_its_value() {
    let mut order = player_order();
    order.field.push(verb(
        "reset_mode",
        60,
        ".rspigot.plugin.v1.Empty",
        "player.game_mode.reset",
    ));
    let set = set_of(vec![order, from_plugin()]);

    let files = plugin_sdk_gen::emit::files(&Tree::build(&set).unwrap());

    let source = &files["player_game_mode.rs"];
    assert!(source.contains("pub(crate) value: GameMode,"));
    assert!(
        source.contains("pub(crate) fn new(target: Arc<str>, link: Link, value: GameMode) -> Self")
    );
    assert!(source.contains("pub const fn value(&self) -> GameMode"));
    assert!(!files["player_sound.rs"].contains("value()"));
}

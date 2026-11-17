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

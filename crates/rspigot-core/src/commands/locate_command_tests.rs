use super::permission::{LEVEL_GAMEMASTERS, may_use};
use super::tree::{ArgumentParser, CommandNodeKind, build_tree};

fn node(tree: &super::CommandTree, path: &[&str]) -> usize {
    path.iter().fold(tree.root, |node, step| {
        tree.nodes[node]
            .children
            .iter()
            .copied()
            .find(|child| match &tree.nodes[*child].kind {
                CommandNodeKind::Literal(name) | CommandNodeKind::Argument { name, .. } => {
                    name == step
                }
                CommandNodeKind::Root => false,
            })
            .unwrap_or_else(|| panic!("{step}"))
    })
}

fn parser(tree: &super::CommandTree, path: &[&str]) -> ArgumentParser {
    match &tree.nodes[node(tree, path)].kind {
        CommandNodeKind::Argument { parser, .. } => parser.clone(),
        _ => panic!("not an argument"),
    }
}

#[test]
fn locate_and_place_use_the_vanilla_argument_types() {
    let tree = build_tree(2);
    assert_eq!(
        parser(&tree, &["locate", "structure", "structure"]),
        ArgumentParser::ResourceOrTagKey {
            registry: "minecraft:worldgen/structure".to_owned()
        }
    );
    assert_eq!(
        parser(&tree, &["locate", "biome", "biome"]),
        ArgumentParser::ResourceOrTag {
            registry: "minecraft:worldgen/biome".to_owned()
        }
    );
    assert_eq!(
        parser(&tree, &["locate", "poi", "poi"]),
        ArgumentParser::ResourceOrTag {
            registry: "minecraft:point_of_interest_type".to_owned()
        }
    );
    assert_eq!(
        parser(&tree, &["place", "feature", "feature"]),
        ArgumentParser::Named("minecraft:feature")
    );
    assert_eq!(
        parser(&tree, &["place", "jigsaw", "pool"]),
        ArgumentParser::ResourceKey {
            registry: "minecraft:worldgen/template_pool".to_owned()
        }
    );
    assert_eq!(
        parser(&tree, &["place", "jigsaw", "pool", "target", "max_depth"]),
        ArgumentParser::Integer {
            min: Some(1),
            max: Some(20)
        }
    );
    assert_eq!(
        parser(&tree, &["place", "template", "template", "pos", "rotation"]),
        ArgumentParser::Named("minecraft:template_rotation")
    );
    assert_eq!(
        parser(
            &tree,
            &["place", "template", "template", "pos", "rotation", "mirror"]
        ),
        ArgumentParser::Named("minecraft:template_mirror")
    );
    assert!(
        tree.nodes[node(
            &tree,
            &[
                "place",
                "template",
                "template",
                "pos",
                "rotation",
                "mirror",
                "integrity",
                "seed",
                "strict"
            ]
        )]
        .executable
    );
}

#[test]
fn both_commands_need_permission_level_two() {
    for name in ["locate", "place"] {
        assert!(!may_use(name, 1));
        assert!(may_use(name, LEVEL_GAMEMASTERS));
        assert!(
            !build_tree(1)
                .nodes
                .iter()
                .any(|n| matches!(&n.kind, CommandNodeKind::Literal(l) if l == name))
        );
        assert!(
            build_tree(2)
                .nodes
                .iter()
                .any(|n| matches!(&n.kind, CommandNodeKind::Literal(l) if l == name))
        );
    }
}

#[test]
fn advancement_and_recipe_use_resource_keys_the_client_can_resolve() {
    let tree = build_tree(2);
    let key = |registry: &str| ArgumentParser::ResourceKey {
        registry: registry.to_owned(),
    };
    for mode in ["from", "through", "until"] {
        assert_eq!(
            parser(
                &tree,
                &["advancement", "grant", "targets", mode, "advancement"]
            ),
            key("minecraft:advancement")
        );
    }
    assert_eq!(
        parser(
            &tree,
            &["advancement", "revoke", "targets", "only", "advancement"]
        ),
        key("minecraft:advancement")
    );
    assert_eq!(
        parser(&tree, &["recipe", "give", "targets", "recipe"]),
        key("minecraft:recipe")
    );
}

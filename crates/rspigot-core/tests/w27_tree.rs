use rspigot_core::commands::{ArgumentParser, CommandNodeKind, CommandTree, build_tree};
use serde_json::{Value, json};

const REPORT: &str = "../../../re/datagen/generated/reports/commands.json";

const SPIGOT_ROOTS: [&str; 5] = ["plugins", "tps", "restart", "spigot", "timings"];

const NOT_MINE: [&str; 6] = [
    "gamerule",
    "effect",
    "damage",
    "loot",
    "advancement",
    "spectate",
];

fn parser_json(parser: &ArgumentParser) -> (String, Value) {
    let named = |name: &str| (name.to_owned(), Value::Null);
    let registry = |name: &str, registry: &str| (name.to_owned(), json!({ "registry": registry }));
    let bounds = |name: &str, min: Option<Value>, max: Option<Value>| {
        let mut object = serde_json::Map::new();
        if let Some(min) = min {
            object.insert("min".to_owned(), min);
        }
        if let Some(max) = max {
            object.insert("max".to_owned(), max);
        }
        (
            name.to_owned(),
            if object.is_empty() {
                Value::Null
            } else {
                Value::Object(object)
            },
        )
    };
    match parser {
        ArgumentParser::Bool => named("brigadier:bool"),
        ArgumentParser::Integer { min, max } => bounds(
            "brigadier:integer",
            min.map(Value::from),
            max.map(Value::from),
        ),
        ArgumentParser::Float { min, max } => bounds(
            "brigadier:float",
            min.map(|v| json!(f64::from(v))),
            max.map(|v| json!(f64::from(v))),
        ),
        ArgumentParser::Double { min, max } => bounds(
            "brigadier:double",
            min.map(Value::from),
            max.map(Value::from),
        ),
        ArgumentParser::Word => ("brigadier:string".to_owned(), json!({"type": "word"})),
        ArgumentParser::QuotedString => ("brigadier:string".to_owned(), json!({"type": "phrase"})),
        ArgumentParser::GreedyString => ("brigadier:string".to_owned(), json!({"type": "greedy"})),
        ArgumentParser::Entity {
            single,
            players_only,
        } => (
            "minecraft:entity".to_owned(),
            json!({
                "amount": if *single { "single" } else { "multiple" },
                "type": if *players_only { "players" } else { "entities" },
            }),
        ),
        ArgumentParser::GameProfile => named("minecraft:game_profile"),
        ArgumentParser::BlockPos => named("minecraft:block_pos"),
        ArgumentParser::Vec3 => named("minecraft:vec3"),
        ArgumentParser::BlockState => named("minecraft:block_state"),
        ArgumentParser::ItemStack => named("minecraft:item_stack"),
        ArgumentParser::Message => named("minecraft:message"),
        ArgumentParser::Time { min } => ("minecraft:time".to_owned(), json!({ "min": min })),
        ArgumentParser::GameMode => named("minecraft:gamemode"),
        ArgumentParser::Dimension => named("minecraft:dimension"),
        ArgumentParser::ResourceLocation => named("minecraft:resource_location"),
        ArgumentParser::Resource { registry: r } => registry("minecraft:resource", r),
        ArgumentParser::NbtCompoundTag => named("minecraft:nbt_compound_tag"),
        ArgumentParser::ScoreHolder { multiple } => (
            "minecraft:score_holder".to_owned(),
            json!({"amount": if *multiple { "multiple" } else { "single" }}),
        ),
        ArgumentParser::ResourceOrTag { registry: r } => registry("minecraft:resource_or_tag", r),
        ArgumentParser::ResourceKey { registry: r } => registry("minecraft:resource_key", r),
        ArgumentParser::ResourceOrTagKey { registry: r } => {
            registry("minecraft:resource_or_tag_key", r)
        }
        ArgumentParser::ResourceSelector { registry: r } => {
            registry("minecraft:resource_selector", r)
        }
        ArgumentParser::Named(name) => named(name),
    }
}

fn same_props(left: &Value, right: &Value) -> bool {
    match (left, right) {
        (Value::Number(a), Value::Number(b)) => {
            let (a, b) = (a.as_f64().unwrap_or(0.0), b.as_f64().unwrap_or(0.0));
            (a - b).abs() <= 1e-6 * a.abs().max(b.abs()).max(1.0)
        }
        (Value::Object(a), Value::Object(b)) => {
            a.len() == b.len()
                && a.iter()
                    .all(|(key, value)| b.get(key).is_some_and(|other| same_props(value, other)))
        }
        _ => left == right,
    }
}

fn walk(tree: &CommandTree, node: usize, expected: &Value, path: &str, out: &mut Vec<String>) {
    let children = expected
        .get("children")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    let executable = expected
        .get("executable")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    if tree.nodes[node].executable != executable && tree.nodes[node].redirect.is_none() {
        out.push(format!(
            "{path}: executable is {} but vanilla has {executable}",
            tree.nodes[node].executable
        ));
    }
    if expected.get("redirect").is_some() {
        if tree.nodes[node].redirect.is_none() && tree.nodes[node].children.is_empty() {
            out.push(format!("{path}: missing redirect"));
        }
        return;
    }
    let effective = tree.nodes[node].redirect.unwrap_or(node);
    for (name, child) in &children {
        let found = tree.nodes[effective]
            .children
            .iter()
            .copied()
            .find(|index| match &tree.nodes[*index].kind {
                CommandNodeKind::Literal(literal) => child["type"] == "literal" && literal == name,
                CommandNodeKind::Argument { name: arg, .. } => {
                    child["type"] == "argument" && arg == name
                }
                CommandNodeKind::Root => false,
            });
        let child_path = format!("{path} {name}");
        let Some(found) = found else {
            out.push(format!("{child_path}: missing"));
            continue;
        };
        if let CommandNodeKind::Argument { parser, .. } = &tree.nodes[found].kind {
            let (kind, props) = parser_json(parser);
            let want_kind = child["parser"].as_str().unwrap_or_default();
            let want_props = child.get("properties").cloned().unwrap_or(Value::Null);
            if kind != want_kind || !same_props(&props, &want_props) {
                out.push(format!(
                    "{child_path}: parser {kind} {props} but vanilla has {want_kind} {want_props}"
                ));
            }
        }
        walk(tree, found, child, &child_path, out);
    }
    if children.is_empty() && tree.nodes[node].redirect.is_some() {
        return;
    }
    for index in &tree.nodes[effective].children {
        let (label, is_literal) = match &tree.nodes[*index].kind {
            CommandNodeKind::Literal(name) => (name.clone(), true),
            CommandNodeKind::Argument { name, .. } => (name.clone(), false),
            CommandNodeKind::Root => continue,
        };
        let known = children
            .get(&label)
            .is_some_and(|child| (child["type"] == "literal") == is_literal);
        if !known {
            out.push(format!("{path} {label}: not in vanilla"));
        }
    }
}

#[test]
fn w27_tree_matches_the_vanilla_command_report() {
    let Ok(text) = std::fs::read_to_string(REPORT) else {
        return;
    };
    let report: Value = serde_json::from_str(&text).expect("commands.json");
    let tree = build_tree(4);
    let mut problems = Vec::new();
    let mut seen = Vec::new();
    for (name, expected) in report["children"].as_object().expect("children") {
        if NOT_MINE.contains(&name.as_str()) {
            continue;
        }
        let found = tree.nodes[tree.root].children.iter().copied().find(
            |index| matches!(&tree.nodes[*index].kind, CommandNodeKind::Literal(l) if l == name),
        );
        let Some(found) = found else {
            problems.push(format!("{name}: missing root command"));
            continue;
        };
        seen.push(found);
        walk(&tree, found, expected, name, &mut problems);
    }
    for index in &tree.nodes[tree.root].children {
        if let CommandNodeKind::Literal(name) = &tree.nodes[*index].kind
            && !SPIGOT_ROOTS.contains(&name.as_str())
            && report["children"].get(name).is_none()
            && !seen.contains(index)
        {
            problems.push(format!("{name}: root command not in vanilla"));
        }
    }
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}

use super::{ArgumentParser, TreeBuilder, entities, player_targets, single_entity};

fn float(min: Option<f32>, max: Option<f32>) -> ArgumentParser {
    ArgumentParser::Float { min, max }
}

const SOURCES: [&str; 11] = [
    "master", "music", "record", "weather", "block", "hostile", "neutral", "player", "ambient",
    "voice", "ui",
];

fn sound_commands(b: &mut TreeBuilder, root: usize) {
    let playsound = b.literal(root, "playsound", false);
    let sound = b.suggesting(
        playsound,
        "sound",
        ArgumentParser::ResourceLocation,
        "minecraft:available_sounds",
        false,
    );
    for source in SOURCES {
        let node = b.literal(sound, source, false);
        let targets = b.argument(node, "targets", player_targets(), true);
        let pos = b.argument(targets, "pos", ArgumentParser::Vec3, true);
        let volume = b.argument(pos, "volume", float(Some(0.0), None), true);
        let pitch = b.argument(volume, "pitch", float(Some(0.0), Some(2.0)), true);
        b.argument(pitch, "minVolume", float(Some(0.0), Some(1.0)), true);
    }
    let stopsound = b.literal(root, "stopsound", false);
    let targets = b.argument(stopsound, "targets", player_targets(), true);
    let any = b.literal(targets, "*", true);
    b.suggesting(
        any,
        "sound",
        ArgumentParser::ResourceLocation,
        "minecraft:available_sounds",
        true,
    );
    for source in SOURCES {
        let node = b.literal(targets, source, true);
        b.suggesting(
            node,
            "sound",
            ArgumentParser::ResourceLocation,
            "minecraft:available_sounds",
            true,
        );
    }
    let particle = b.literal(root, "particle", false);
    let name = b.argument(
        particle,
        "name",
        ArgumentParser::Named("minecraft:particle"),
        true,
    );
    let pos = b.argument(name, "pos", ArgumentParser::Vec3, true);
    let delta = b.argument(pos, "delta", ArgumentParser::Vec3, false);
    let speed = b.argument(delta, "speed", float(Some(0.0), None), false);
    let count = b.argument(
        speed,
        "count",
        ArgumentParser::Integer {
            min: Some(0),
            max: None,
        },
        true,
    );
    for mode in ["force", "normal"] {
        let node = b.literal(count, mode, true);
        b.argument(node, "viewers", player_targets(), true);
    }
}

fn double() -> ArgumentParser {
    ArgumentParser::Double {
        min: None,
        max: None,
    }
}

fn resource(registry: &str) -> ArgumentParser {
    ArgumentParser::Resource {
        registry: registry.to_owned(),
    }
}

fn entity_commands(b: &mut TreeBuilder, root: usize) {
    let attribute = b.literal(root, "attribute", false);
    let target = b.argument(attribute, "target", single_entity(), false);
    let name = b.argument(target, "attribute", resource("minecraft:attribute"), false);
    let get = b.literal(name, "get", true);
    b.argument(get, "scale", double(), true);
    let base = b.literal(name, "base", false);
    let base_get = b.literal(base, "get", true);
    b.argument(base_get, "scale", double(), true);
    let base_set = b.literal(base, "set", false);
    b.argument(base_set, "value", double(), true);
    b.literal(base, "reset", true);
    let modifier = b.literal(name, "modifier", false);
    let add = b.literal(modifier, "add", false);
    let id = b.argument(add, "id", ArgumentParser::ResourceLocation, false);
    let value = b.argument(id, "value", double(), false);
    for operation in ["add_value", "add_multiplied_base", "add_multiplied_total"] {
        b.literal(value, operation, true);
    }
    let remove = b.literal(modifier, "remove", false);
    b.argument(remove, "id", ArgumentParser::ResourceLocation, true);
    let modifier_value = b.literal(modifier, "value", false);
    let value_get = b.literal(modifier_value, "get", false);
    let value_id = b.argument(value_get, "id", ArgumentParser::ResourceLocation, true);
    b.argument(value_id, "scale", double(), true);

    let damage = b.literal(root, "damage", false);
    let target = b.argument(damage, "target", single_entity(), false);
    let amount = b.argument(target, "amount", float(Some(0.0), None), true);
    let kind = b.argument(
        amount,
        "damageType",
        resource("minecraft:damage_type"),
        true,
    );
    let at = b.literal(kind, "at", false);
    b.argument(at, "location", ArgumentParser::Vec3, true);
    let by = b.literal(kind, "by", false);
    let by_entity = b.argument(by, "entity", single_entity(), true);
    let from = b.literal(by_entity, "from", false);
    b.argument(from, "cause", single_entity(), true);

    let enchant = b.literal(root, "enchant", false);
    let targets = b.argument(enchant, "targets", entities(), false);
    let enchantment = b.argument(
        targets,
        "enchantment",
        resource("minecraft:enchantment"),
        true,
    );
    b.argument(
        enchantment,
        "level",
        ArgumentParser::Integer {
            min: Some(0),
            max: None,
        },
        true,
    );
}

fn rotation() -> ArgumentParser {
    ArgumentParser::Named("minecraft:rotation")
}

fn anchor() -> ArgumentParser {
    ArgumentParser::Named("minecraft:entity_anchor")
}

fn movement_commands(b: &mut TreeBuilder, root: usize) {
    let ride = b.literal(root, "ride", false);
    let target = b.argument(ride, "target", single_entity(), false);
    let mount = b.literal(target, "mount", false);
    b.argument(mount, "vehicle", single_entity(), true);
    b.literal(target, "dismount", true);

    let rotate = b.literal(root, "rotate", false);
    let target = b.argument(rotate, "target", single_entity(), false);
    b.argument(target, "rotation", rotation(), true);
    let facing = b.literal(target, "facing", false);
    let facing_entity = b.literal(facing, "entity", false);
    let entity = b.argument(facing_entity, "facingEntity", single_entity(), true);
    b.argument(entity, "facingAnchor", anchor(), true);
    b.argument(facing, "facingLocation", ArgumentParser::Vec3, true);

    let spectate = b.literal(root, "spectate", true);
    let target = b.argument(spectate, "target", single_entity(), true);
    b.argument(target, "player", player_targets(), true);

    let swing = b.literal(root, "swing", false);
    let targets = b.argument(swing, "targets", entities(), false);
    for hand in ["mainhand", "offhand"] {
        let node = b.literal(targets, hand, true);
        let animation = b.argument(
            node,
            "animation",
            ArgumentParser::Named("minecraft:swing_animation"),
            true,
        );
        b.argument(animation, "duration", ArgumentParser::Time { min: 1 }, true);
    }
}

fn column() -> ArgumentParser {
    ArgumentParser::Named("minecraft:column_pos")
}

fn vec2() -> ArgumentParser {
    ArgumentParser::Named("minecraft:vec2")
}

fn int(min: Option<i32>) -> ArgumentParser {
    ArgumentParser::Integer { min, max: None }
}

fn world_commands(b: &mut TreeBuilder, root: usize) {
    let forceload = b.literal(root, "forceload", false);
    for action in ["add", "remove"] {
        let node = b.literal(forceload, action, false);
        let from = b.argument(node, "from", column(), true);
        b.argument(from, "to", column(), true);
    }
    let remove = forceload_remove(b, forceload);
    b.literal(remove, "all", true);
    let query = b.literal(forceload, "query", true);
    b.argument(query, "pos", column(), true);

    let border = b.literal(root, "worldborder", false);
    let bounded = || ArgumentParser::Double {
        min: Some(-59_999_968.0),
        max: Some(59_999_968.0),
    };
    for action in ["add", "set"] {
        let node = b.literal(border, action, false);
        let distance = b.argument(node, "distance", bounded(), true);
        b.argument(distance, "time", ArgumentParser::Time { min: 0 }, true);
    }
    let center = b.literal(border, "center", false);
    b.argument(center, "pos", vec2(), true);
    let damage = b.literal(border, "damage", false);
    let amount = b.literal(damage, "amount", false);
    b.argument(amount, "damagePerBlock", float(Some(0.0), None), true);
    let buffer = b.literal(damage, "buffer", false);
    b.argument(buffer, "distance", float(Some(0.0), None), true);
    b.literal(border, "get", true);
    let warning = b.literal(border, "warning", false);
    let distance = b.literal(warning, "distance", false);
    b.argument(distance, "distance", int(Some(0)), true);
    let time = b.literal(warning, "time", false);
    b.argument(time, "time", ArgumentParser::Time { min: 0 }, true);

    let spread = b.literal(root, "spreadplayers", false);
    let center = b.argument(spread, "center", vec2(), false);
    let spread_distance = b.argument(center, "spreadDistance", float(Some(0.0), None), false);
    let range = b.argument(spread_distance, "maxRange", float(Some(1.0), None), false);
    let teams = b.argument(range, "respectTeams", ArgumentParser::Bool, false);
    b.argument(teams, "targets", entities(), true);
    let under = b.literal(range, "under", false);
    let height = b.argument(under, "maxHeight", int(None), false);
    let under_teams = b.argument(height, "respectTeams", ArgumentParser::Bool, false);
    b.argument(under_teams, "targets", entities(), true);

    let fillbiome = b.literal(root, "fillbiome", false);
    let from = b.argument(fillbiome, "from", ArgumentParser::BlockPos, false);
    let to = b.argument(from, "to", ArgumentParser::BlockPos, false);
    let biome = b.argument(to, "biome", resource("minecraft:worldgen/biome"), true);
    let replace = b.literal(biome, "replace", false);
    b.argument(
        replace,
        "filter",
        ArgumentParser::ResourceOrTag {
            registry: "minecraft:worldgen/biome".to_owned(),
        },
        true,
    );
}

fn forceload_remove(b: &mut TreeBuilder, forceload: usize) -> usize {
    b.nodes[forceload]
        .children
        .iter()
        .copied()
        .find(|child| b.nodes[*child].kind == super::CommandNodeKind::Literal("remove".to_owned()))
        .unwrap_or(forceload)
}

pub(super) fn extra_commands(b: &mut TreeBuilder, root: usize) {
    sound_commands(b, root);
    entity_commands(b, root);
    movement_commands(b, root);
    world_commands(b, root);
}

pub(super) fn admin_extras(b: &mut TreeBuilder, root: usize) {
    let tick = b.literal(root, "tick", false);
    b.literal(tick, "query", true);
    let rate = b.literal(tick, "rate", false);
    b.argument(rate, "rate", float(Some(1.0), Some(10_000.0)), true);
    b.literal(tick, "freeze", true);
    b.literal(tick, "unfreeze", true);
    let step = b.literal(tick, "step", true);
    b.argument(step, "time", ArgumentParser::Time { min: 1 }, true);
    b.literal(step, "stop", true);
    let sprint = b.literal(tick, "sprint", false);
    b.argument(sprint, "time", ArgumentParser::Time { min: 1 }, true);
    b.literal(sprint, "stop", true);

    let transfer = b.literal(root, "transfer", false);
    let host = b.argument(transfer, "hostname", ArgumentParser::QuotedString, true);
    let port = b.argument(
        host,
        "port",
        ArgumentParser::Integer {
            min: Some(1),
            max: Some(65_535),
        },
        true,
    );
    b.argument(port, "players", player_targets(), true);

    let idle = b.literal(root, "setidletimeout", false);
    b.argument(idle, "minutes", int(Some(0)), true);

    let debug = b.literal(root, "debug", false);
    b.literal(debug, "start", true);
    b.literal(debug, "stop", true);
}

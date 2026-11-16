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

pub(super) fn extra_commands(b: &mut TreeBuilder, root: usize) {
    sound_commands(b, root);
    entity_commands(b, root);
    movement_commands(b, root);
}

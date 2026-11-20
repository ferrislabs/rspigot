use super::{ArgumentParser, TreeBuilder, entities, player_targets, single_entity};

const ASK_SERVER: &str = "minecraft:ask_server";

fn resource(registry: &str) -> ArgumentParser {
    ArgumentParser::Resource {
        registry: registry.to_owned(),
    }
}

fn advancement_commands(b: &mut TreeBuilder, root: usize) {
    let advancement = b.literal(root, "advancement", false);
    for action in ["grant", "revoke"] {
        let node = b.literal(advancement, action, false);
        let targets = b.argument(node, "targets", player_targets(), false);
        b.literal(targets, "everything", true);
        for mode in ["from", "through", "until"] {
            let mode = b.literal(targets, mode, false);
            b.suggesting(
                mode,
                "advancement",
                resource("minecraft:advancement"),
                ASK_SERVER,
                true,
            );
        }
        let only = b.literal(targets, "only", false);
        let name = b.suggesting(
            only,
            "advancement",
            resource("minecraft:advancement"),
            ASK_SERVER,
            true,
        );
        b.suggesting(
            name,
            "criterion",
            ArgumentParser::GreedyString,
            ASK_SERVER,
            true,
        );
    }
}

fn recipe_commands(b: &mut TreeBuilder, root: usize) {
    let recipe = b.literal(root, "recipe", false);
    for action in ["give", "take"] {
        let node = b.literal(recipe, action, false);
        let targets = b.argument(node, "targets", player_targets(), false);
        b.literal(targets, "*", true);
        b.suggesting(
            targets,
            "recipe",
            resource("minecraft:recipe"),
            ASK_SERVER,
            true,
        );
    }
}

fn slot_source() -> ArgumentParser {
    ArgumentParser::Named("minecraft:slot_source")
}

fn modifier() -> ArgumentParser {
    ArgumentParser::Named("minecraft:loot_modifier")
}

fn item_provider(b: &mut TreeBuilder, slots: usize) {
    let from = b.literal(slots, "from", false);
    let block = b.literal(from, "block", false);
    let source = b.argument(block, "source", ArgumentParser::BlockPos, false);
    let source_slots = b.argument(source, "sourceSlots", slot_source(), true);
    b.argument(source_slots, "modifier", modifier(), true);
    let entity = b.literal(from, "entity", false);
    let source = b.argument(entity, "source", entities(), false);
    let source_slots = b.argument(source, "sourceSlots", slot_source(), true);
    b.argument(source_slots, "modifier", modifier(), true);
    let with = b.literal(slots, "with", false);
    let item = b.argument(with, "item", ArgumentParser::ItemStack, true);
    b.argument(
        item,
        "count",
        ArgumentParser::Integer {
            min: Some(1),
            max: Some(99),
        },
        true,
    );
}

fn item_commands(b: &mut TreeBuilder, root: usize) {
    let item = b.literal(root, "item", false);
    for action in ["fill", "override", "replace"] {
        let node = b.literal(item, action, false);
        let block = b.literal(node, "block", false);
        let target = b.argument(block, "target", ArgumentParser::BlockPos, false);
        let slots = b.suggesting(target, "slots", slot_source(), ASK_SERVER, false);
        item_provider(b, slots);
        let entity = b.literal(node, "entity", false);
        let target = b.argument(entity, "target", entities(), false);
        let slots = b.suggesting(target, "slots", slot_source(), ASK_SERVER, false);
        item_provider(b, slots);
    }
    let modify = b.literal(item, "modify", false);
    let block = b.literal(modify, "block", false);
    let target = b.argument(block, "target", ArgumentParser::BlockPos, false);
    let slots = b.suggesting(target, "slots", slot_source(), ASK_SERVER, false);
    b.argument(slots, "modifier", modifier(), true);
    let entity = b.literal(modify, "entity", false);
    let target = b.argument(entity, "target", entities(), false);
    let slots = b.suggesting(target, "slots", slot_source(), ASK_SERVER, false);
    b.argument(slots, "modifier", modifier(), true);
}

fn tool(b: &mut TreeBuilder, parent: usize) {
    b.literal(parent, "mainhand", true);
    b.literal(parent, "offhand", true);
    b.argument(parent, "tool", ArgumentParser::ItemStack, true);
}

fn loot_sources(b: &mut TreeBuilder, parent: usize) {
    let fish = b.literal(parent, "fish", false);
    let table = b.suggesting(
        fish,
        "loot_table",
        ArgumentParser::Named("minecraft:loot_table"),
        ASK_SERVER,
        false,
    );
    let pos = b.argument(table, "pos", ArgumentParser::BlockPos, true);
    tool(b, pos);
    let kill = b.literal(parent, "kill", false);
    b.argument(kill, "target", single_entity(), true);
    let loot = b.literal(parent, "loot", false);
    b.suggesting(
        loot,
        "loot_table",
        ArgumentParser::Named("minecraft:loot_table"),
        ASK_SERVER,
        true,
    );
    let mine = b.literal(parent, "mine", false);
    let pos = b.argument(mine, "pos", ArgumentParser::BlockPos, true);
    tool(b, pos);
}

fn loot_commands(b: &mut TreeBuilder, root: usize) {
    let loot = b.literal(root, "loot", false);
    let give = b.literal(loot, "give", false);
    let players = b.argument(give, "players", player_targets(), false);
    loot_sources(b, players);
    let insert = b.literal(loot, "insert", false);
    let pos = b.argument(insert, "targetPos", ArgumentParser::BlockPos, false);
    loot_sources(b, pos);
    let spawn = b.literal(loot, "spawn", false);
    let pos = b.argument(spawn, "targetPos", ArgumentParser::Vec3, false);
    loot_sources(b, pos);
    let replace = b.literal(loot, "replace", false);
    let block = b.literal(replace, "block", false);
    let pos = b.argument(block, "targetPos", ArgumentParser::BlockPos, false);
    let slot = b.argument(
        pos,
        "slot",
        ArgumentParser::Named("minecraft:item_slot"),
        false,
    );
    let entity = b.literal(replace, "entity", false);
    let entities = b.argument(entity, "entities", entities(), false);
    let entity_slot = b.argument(
        entities,
        "slot",
        ArgumentParser::Named("minecraft:item_slot"),
        false,
    );
    for slot in [slot, entity_slot] {
        let count = b.argument(
            slot,
            "count",
            ArgumentParser::Integer {
                min: Some(0),
                max: None,
            },
            false,
        );
        loot_sources(b, count);
        loot_sources(b, slot);
    }
}

fn waypoint_commands(b: &mut TreeBuilder, root: usize) {
    let waypoint = b.literal(root, "waypoint", false);
    b.literal(waypoint, "list", true);
    let modify = b.literal(waypoint, "modify", false);
    let target = b.argument(modify, "waypoint", single_entity(), false);
    let color = b.literal(target, "color", false);
    b.argument(
        color,
        "color",
        ArgumentParser::Named("minecraft:team_color"),
        true,
    );
    let hex = b.literal(color, "hex", false);
    b.argument(
        hex,
        "color",
        ArgumentParser::Named("minecraft:hex_color"),
        true,
    );
    b.literal(color, "reset", true);
    let style = b.literal(target, "style", false);
    b.literal(style, "reset", true);
    let set = b.literal(style, "set", false);
    b.argument(set, "style", ArgumentParser::ResourceLocation, true);
}

fn stopwatch_commands(b: &mut TreeBuilder, root: usize) {
    let stopwatch = b.literal(root, "stopwatch", false);
    for action in ["create", "remove", "restart"] {
        let node = b.literal(stopwatch, action, false);
        b.suggesting(
            node,
            "id",
            ArgumentParser::ResourceLocation,
            ASK_SERVER,
            true,
        );
    }
    let query = b.literal(stopwatch, "query", false);
    let id = b.suggesting(
        query,
        "id",
        ArgumentParser::ResourceLocation,
        ASK_SERVER,
        true,
    );
    b.argument(
        id,
        "scale",
        ArgumentParser::Double {
            min: None,
            max: None,
        },
        true,
    );
}

fn locate_commands(b: &mut TreeBuilder, root: usize) {
    let locate = b.literal(root, "locate", false);
    let structure = b.literal(locate, "structure", false);
    b.suggesting(
        structure,
        "structure",
        ArgumentParser::ResourceOrTagKey {
            registry: "minecraft:worldgen/structure".to_owned(),
        },
        ASK_SERVER,
        true,
    );
    let biome = b.literal(locate, "biome", false);
    b.argument(
        biome,
        "biome",
        ArgumentParser::ResourceOrTag {
            registry: "minecraft:worldgen/biome".to_owned(),
        },
        true,
    );
    let poi = b.literal(locate, "poi", false);
    b.argument(
        poi,
        "poi",
        ArgumentParser::ResourceOrTag {
            registry: "minecraft:point_of_interest_type".to_owned(),
        },
        true,
    );
}

fn place_commands(b: &mut TreeBuilder, root: usize) {
    let place = b.literal(root, "place", false);
    let feature = b.literal(place, "feature", false);
    let name = b.argument(
        feature,
        "feature",
        ArgumentParser::Named("minecraft:feature"),
        true,
    );
    b.argument(name, "pos", ArgumentParser::BlockPos, true);
    let jigsaw = b.literal(place, "jigsaw", false);
    let pool = b.suggesting(
        jigsaw,
        "pool",
        ArgumentParser::ResourceKey {
            registry: "minecraft:worldgen/template_pool".to_owned(),
        },
        ASK_SERVER,
        false,
    );
    let target = b.argument(pool, "target", ArgumentParser::ResourceLocation, false);
    let depth = b.argument(
        target,
        "max_depth",
        ArgumentParser::Integer {
            min: Some(1),
            max: Some(20),
        },
        true,
    );
    b.argument(depth, "position", ArgumentParser::BlockPos, true);
    let structure = b.literal(place, "structure", false);
    let name = b.suggesting(
        structure,
        "structure",
        ArgumentParser::ResourceKey {
            registry: "minecraft:worldgen/structure".to_owned(),
        },
        ASK_SERVER,
        true,
    );
    b.argument(name, "pos", ArgumentParser::BlockPos, true);
    let template = b.literal(place, "template", false);
    let name = b.suggesting(
        template,
        "template",
        ArgumentParser::ResourceLocation,
        ASK_SERVER,
        true,
    );
    let pos = b.argument(name, "pos", ArgumentParser::BlockPos, true);
    let rotation = b.argument(
        pos,
        "rotation",
        ArgumentParser::Named("minecraft:template_rotation"),
        true,
    );
    let mirror = b.argument(
        rotation,
        "mirror",
        ArgumentParser::Named("minecraft:template_mirror"),
        true,
    );
    let integrity = b.argument(
        mirror,
        "integrity",
        ArgumentParser::Float {
            min: Some(0.0),
            max: Some(1.0),
        },
        true,
    );
    let seed = b.argument(
        integrity,
        "seed",
        ArgumentParser::Integer {
            min: None,
            max: None,
        },
        true,
    );
    b.literal(seed, "strict", true);
}

pub(super) fn game_commands(b: &mut TreeBuilder, root: usize) {
    advancement_commands(b, root);
    recipe_commands(b, root);
    item_commands(b, root);
    loot_commands(b, root);
    waypoint_commands(b, root);
    stopwatch_commands(b, root);
    locate_commands(b, root);
    place_commands(b, root);
}

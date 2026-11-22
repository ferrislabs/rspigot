use super::{ArgumentParser, TreeBuilder, single_entity};

const ASK_SERVER: &str = "minecraft:ask_server";

fn function_argument(b: &mut TreeBuilder, parent: usize, executable: bool) -> usize {
    b.suggesting(
        parent,
        "name",
        ArgumentParser::Named("minecraft:function"),
        ASK_SERVER,
        executable,
    )
}

fn pack_argument(b: &mut TreeBuilder, parent: usize, name: &str, executable: bool) -> usize {
    b.suggesting(
        parent,
        name,
        ArgumentParser::QuotedString,
        ASK_SERVER,
        executable,
    )
}

fn path(b: &mut TreeBuilder, parent: usize) {
    b.argument(
        parent,
        "path",
        ArgumentParser::Named("minecraft:nbt_path"),
        true,
    );
}

pub(super) fn function_commands(b: &mut TreeBuilder, root: usize) {
    let function = b.literal(root, "function", false);
    let name = function_argument(b, function, true);
    b.argument(name, "arguments", ArgumentParser::NbtCompoundTag, true);
    let with = b.literal(name, "with", false);
    let entity = b.literal(with, "entity", false);
    let source = b.argument(entity, "source", single_entity(), true);
    path(b, source);
    let block = b.literal(with, "block", false);
    let source = b.argument(block, "source", ArgumentParser::BlockPos, true);
    path(b, source);
    let storage = b.literal(with, "storage", false);
    let source = b.argument(storage, "source", ArgumentParser::ResourceLocation, true);
    path(b, source);

    let schedule = b.literal(root, "schedule", false);
    let scheduled = b.literal(schedule, "function", false);
    let name = b.suggesting(
        scheduled,
        "function",
        ArgumentParser::Named("minecraft:function"),
        ASK_SERVER,
        false,
    );
    let time = b.argument(name, "time", ArgumentParser::Time { min: 0 }, true);
    for mode in ["append", "replace"] {
        b.literal(time, mode, true);
    }
    let clear = b.literal(schedule, "clear", false);
    b.suggesting(
        clear,
        "function",
        ArgumentParser::ResourceLocation,
        ASK_SERVER,
        true,
    );

    let returning = b.literal(root, "return", false);
    b.argument(
        returning,
        "value",
        ArgumentParser::Integer {
            min: None,
            max: None,
        },
        true,
    );
    b.literal(returning, "fail", true);
    let run = b.literal(returning, "run", false);
    b.nodes[run].redirect = Some(root);

    let datapack = b.literal(root, "datapack", false);
    let enable = b.literal(datapack, "enable", false);
    let name = pack_argument(b, enable, "name", true);
    b.literal(name, "first", true);
    b.literal(name, "last", true);
    for place in ["before", "after"] {
        let node = b.literal(name, place, false);
        pack_argument(b, node, "existing", true);
    }
    let disable = b.literal(datapack, "disable", false);
    pack_argument(b, disable, "name", true);
    let list = b.literal(datapack, "list", true);
    for kind in ["available", "enabled"] {
        b.literal(list, kind, true);
    }
    let create = b.literal(datapack, "create", false);
    let id = b.argument(create, "id", ArgumentParser::QuotedString, false);
    b.argument(
        id,
        "description",
        ArgumentParser::Named("minecraft:component"),
        true,
    );
}

use super::permission::{LEVEL_ADMINS, LEVEL_GAMEMASTERS, LEVEL_OWNERS};

#[derive(Debug, Clone, PartialEq)]
pub enum ArgumentParser {
    Bool,
    Integer { min: Option<i32>, max: Option<i32> },
    Float { min: Option<f32>, max: Option<f32> },
    Double { min: Option<f64>, max: Option<f64> },
    Word,
    QuotedString,
    GreedyString,
    Entity { single: bool, players_only: bool },
    GameProfile,
    BlockPos,
    Vec3,
    BlockState,
    ItemStack,
    Message,
    Time { min: i32 },
    GameMode,
    Dimension,
    ResourceLocation,
    Resource { registry: String },
    NbtCompoundTag,
    ScoreHolder { multiple: bool },
    ResourceOrTag { registry: String },
    Named(&'static str),
}

#[derive(Debug, Clone, PartialEq)]
pub enum CommandNodeKind {
    Root,
    Literal(String),
    Argument {
        name: String,
        parser: ArgumentParser,
        suggests: Option<String>,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct CommandNode {
    pub kind: CommandNodeKind,
    pub executable: bool,
    pub children: Vec<usize>,
    pub redirect: Option<usize>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CommandTree {
    pub nodes: Vec<CommandNode>,
    pub root: usize,
}

struct TreeBuilder {
    nodes: Vec<CommandNode>,
}

impl TreeBuilder {
    fn add(&mut self, parent: usize, kind: CommandNodeKind, executable: bool) -> usize {
        self.nodes.push(CommandNode {
            kind,
            executable,
            children: Vec::new(),
            redirect: None,
        });
        let index = self.nodes.len() - 1;
        self.nodes[parent].children.push(index);
        index
    }

    fn literal(&mut self, parent: usize, name: &str, executable: bool) -> usize {
        self.add(
            parent,
            CommandNodeKind::Literal(name.to_owned()),
            executable,
        )
    }

    fn argument(
        &mut self,
        parent: usize,
        name: &str,
        parser: ArgumentParser,
        executable: bool,
    ) -> usize {
        self.add(
            parent,
            CommandNodeKind::Argument {
                name: name.to_owned(),
                parser,
                suggests: None,
            },
            executable,
        )
    }

    fn suggesting(
        &mut self,
        parent: usize,
        name: &str,
        parser: ArgumentParser,
        suggests: &str,
        executable: bool,
    ) -> usize {
        self.add(
            parent,
            CommandNodeKind::Argument {
                name: name.to_owned(),
                parser,
                suggests: Some(suggests.to_owned()),
            },
            executable,
        )
    }

    fn alias(&mut self, parent: usize, name: &str, target: usize) {
        let index = self.literal(parent, name, false);
        self.nodes[index].redirect = Some(target);
    }
}

fn entities() -> ArgumentParser {
    ArgumentParser::Entity {
        single: false,
        players_only: false,
    }
}

fn single_entity() -> ArgumentParser {
    ArgumentParser::Entity {
        single: true,
        players_only: false,
    }
}

fn mob_effect() -> ArgumentParser {
    ArgumentParser::Resource {
        registry: "minecraft:mob_effect".to_owned(),
    }
}

fn player_arg() -> ArgumentParser {
    ArgumentParser::Entity {
        single: true,
        players_only: true,
    }
}

pub fn build_tree(level: u8) -> CommandTree {
    let mut b = TreeBuilder {
        nodes: vec![CommandNode {
            kind: CommandNodeKind::Root,
            executable: false,
            children: Vec::new(),
            redirect: None,
        }],
    };
    let root = 0;
    for name in ["help", "list", "version", "plugins", "tps"] {
        b.literal(root, name, true);
    }
    let me = b.literal(root, "me", false);
    b.argument(me, "action", ArgumentParser::Message, true);
    let msg = b.literal(root, "msg", false);
    let target = b.argument(msg, "targets", player_arg(), false);
    b.argument(target, "message", ArgumentParser::Message, true);
    b.alias(root, "tell", msg);
    b.alias(root, "w", msg);
    if level >= LEVEL_GAMEMASTERS {
        gamemaster_commands(&mut b, root);
    }
    if level >= LEVEL_ADMINS {
        admin_commands(&mut b, root);
    }
    if level >= LEVEL_OWNERS {
        for name in ["save-all", "save-on", "save-off", "stop"] {
            b.literal(root, name, true);
        }
    }
    CommandTree {
        nodes: b.nodes,
        root,
    }
}

fn redirecting(b: &mut TreeBuilder, node: usize, target: usize) {
    b.nodes[node].redirect = Some(target);
}

fn position_argument(b: &mut TreeBuilder, parent: usize, name: &str) -> usize {
    b.argument(parent, name, ArgumentParser::BlockPos, false)
}

fn data_target_nodes(
    b: &mut TreeBuilder,
    parent: usize,
    mut leaf: impl FnMut(&mut TreeBuilder, usize),
) {
    let entity = b.literal(parent, "entity", false);
    let target = b.argument(entity, "target", single_entity(), false);
    let path = b.argument(
        target,
        "path",
        ArgumentParser::Named("minecraft:nbt_path"),
        false,
    );
    leaf(b, path);
    let block = b.literal(parent, "block", false);
    let pos = position_argument(b, block, "targetPos");
    let path = b.argument(
        pos,
        "path",
        ArgumentParser::Named("minecraft:nbt_path"),
        false,
    );
    leaf(b, path);
    let storage = b.literal(parent, "storage", false);
    let target = b.argument(storage, "target", ArgumentParser::ResourceLocation, false);
    let path = b.argument(
        target,
        "path",
        ArgumentParser::Named("minecraft:nbt_path"),
        false,
    );
    leaf(b, path);
}

fn condition_tree(b: &mut TreeBuilder, parent: usize, execute: usize) {
    let finish = |b: &mut TreeBuilder, node: usize| {
        b.nodes[node].executable = true;
        redirecting(b, node, execute);
    };
    let block = b.literal(parent, "block", false);
    let pos = position_argument(b, block, "pos");
    let predicate = b.argument(
        pos,
        "block",
        ArgumentParser::Named("minecraft:block_predicate"),
        false,
    );
    finish(b, predicate);
    let biome = b.literal(parent, "biome", false);
    let pos = position_argument(b, biome, "pos");
    let resource = b.argument(
        pos,
        "biome",
        ArgumentParser::ResourceOrTag {
            registry: "minecraft:worldgen/biome".to_owned(),
        },
        false,
    );
    finish(b, resource);
    let loaded = b.literal(parent, "loaded", false);
    let pos = position_argument(b, loaded, "pos");
    finish(b, pos);
    let dimension = b.literal(parent, "dimension", false);
    let key = b.argument(dimension, "dimension", ArgumentParser::Dimension, false);
    finish(b, key);
    let score = b.literal(parent, "score", false);
    let holder = b.argument(
        score,
        "target",
        ArgumentParser::ScoreHolder { multiple: false },
        false,
    );
    let objective = b.argument(
        holder,
        "targetObjective",
        ArgumentParser::Named("minecraft:objective"),
        false,
    );
    for operator in ["=", "<", "<=", ">", ">="] {
        let op = b.literal(objective, operator, false);
        let source = b.argument(
            op,
            "source",
            ArgumentParser::ScoreHolder { multiple: false },
            false,
        );
        let source_objective = b.argument(
            source,
            "sourceObjective",
            ArgumentParser::Named("minecraft:objective"),
            false,
        );
        finish(b, source_objective);
    }
    let matches = b.literal(objective, "matches", false);
    let range = b.argument(
        matches,
        "range",
        ArgumentParser::Named("minecraft:int_range"),
        false,
    );
    finish(b, range);
    let blocks = b.literal(parent, "blocks", false);
    let start = position_argument(b, blocks, "start");
    let end = position_argument(b, start, "end");
    let destination = position_argument(b, end, "destination");
    for mode in ["all", "masked"] {
        let node = b.literal(destination, mode, false);
        finish(b, node);
    }
    let entity = b.literal(parent, "entity", false);
    let entities_node = b.argument(entity, "entities", entities(), false);
    finish(b, entities_node);
    let predicate = b.literal(parent, "predicate", false);
    let id = b.argument(
        predicate,
        "predicate",
        ArgumentParser::Named("minecraft:loot_predicate"),
        false,
    );
    finish(b, id);
    let function = b.literal(parent, "function", false);
    let name = b.argument(
        function,
        "name",
        ArgumentParser::Named("minecraft:function"),
        false,
    );
    finish(b, name);
    let data = b.literal(parent, "data", false);
    let entity = b.literal(data, "entity", false);
    let source = b.argument(entity, "source", single_entity(), false);
    let path = b.argument(
        source,
        "path",
        ArgumentParser::Named("minecraft:nbt_path"),
        false,
    );
    finish(b, path);
    let block = b.literal(data, "block", false);
    let pos = position_argument(b, block, "sourcePos");
    let path = b.argument(
        pos,
        "path",
        ArgumentParser::Named("minecraft:nbt_path"),
        false,
    );
    finish(b, path);
    let storage = b.literal(data, "storage", false);
    let source = b.argument(storage, "source", ArgumentParser::ResourceLocation, false);
    let path = b.argument(
        source,
        "path",
        ArgumentParser::Named("minecraft:nbt_path"),
        false,
    );
    finish(b, path);
}

fn store_tree(b: &mut TreeBuilder, parent: usize, execute: usize) {
    let score = b.literal(parent, "score", false);
    let holders = b.argument(
        score,
        "targets",
        ArgumentParser::ScoreHolder { multiple: true },
        false,
    );
    let objective = b.argument(
        holders,
        "objective",
        ArgumentParser::Named("minecraft:objective"),
        false,
    );
    redirecting(b, objective, execute);
    let bossbar = b.literal(parent, "bossbar", false);
    let id = b.argument(bossbar, "id", ArgumentParser::ResourceLocation, false);
    for field in ["value", "max"] {
        let node = b.literal(id, field, false);
        redirecting(b, node, execute);
    }
    data_target_nodes(b, parent, |b, path| {
        for kind in ["int", "float", "short", "long", "double", "byte"] {
            let node = b.literal(path, kind, false);
            let scale = b.argument(
                node,
                "scale",
                ArgumentParser::Double {
                    min: None,
                    max: None,
                },
                false,
            );
            redirecting(b, scale, execute);
        }
    });
}

fn execute_tree(b: &mut TreeBuilder, root: usize) {
    let execute = b.literal(root, "execute", false);
    let run = b.literal(execute, "run", false);
    redirecting(b, run, root);
    for name in ["if", "unless"] {
        let node = b.literal(execute, name, false);
        condition_tree(b, node, execute);
    }
    for name in ["as", "at"] {
        let node = b.literal(execute, name, false);
        let targets = b.argument(node, "targets", entities(), false);
        redirecting(b, targets, execute);
    }
    let store = b.literal(execute, "store", false);
    for mode in ["result", "success"] {
        let node = b.literal(store, mode, false);
        store_tree(b, node, execute);
    }
    let positioned = b.literal(execute, "positioned", false);
    let pos = b.argument(positioned, "pos", ArgumentParser::Vec3, false);
    redirecting(b, pos, execute);
    let positioned_as = b.literal(positioned, "as", false);
    let targets = b.argument(positioned_as, "targets", entities(), false);
    redirecting(b, targets, execute);
    let over = b.literal(positioned, "over", false);
    let map = b.argument(
        over,
        "heightmap",
        ArgumentParser::Named("minecraft:heightmap"),
        false,
    );
    redirecting(b, map, execute);
    let rotated = b.literal(execute, "rotated", false);
    let rot = b.argument(
        rotated,
        "rot",
        ArgumentParser::Named("minecraft:rotation"),
        false,
    );
    redirecting(b, rot, execute);
    let rotated_as = b.literal(rotated, "as", false);
    let targets = b.argument(rotated_as, "targets", entities(), false);
    redirecting(b, targets, execute);
    let facing = b.literal(execute, "facing", false);
    let facing_entity = b.literal(facing, "entity", false);
    let targets = b.argument(facing_entity, "targets", entities(), false);
    let anchor = b.argument(
        targets,
        "anchor",
        ArgumentParser::Named("minecraft:entity_anchor"),
        false,
    );
    redirecting(b, anchor, execute);
    let pos = b.argument(facing, "pos", ArgumentParser::Vec3, false);
    redirecting(b, pos, execute);
    let align = b.literal(execute, "align", false);
    let axes = b.argument(
        align,
        "axes",
        ArgumentParser::Named("minecraft:swizzle"),
        false,
    );
    redirecting(b, axes, execute);
    let anchored = b.literal(execute, "anchored", false);
    let anchor = b.argument(
        anchored,
        "anchor",
        ArgumentParser::Named("minecraft:entity_anchor"),
        false,
    );
    redirecting(b, anchor, execute);
    let within = b.literal(execute, "in", false);
    let dimension = b.argument(within, "dimension", ArgumentParser::Dimension, false);
    redirecting(b, dimension, execute);
    let summon = b.literal(execute, "summon", false);
    let entity = b.suggesting(
        summon,
        "entity",
        ArgumentParser::Resource {
            registry: "minecraft:entity_type".to_owned(),
        },
        "minecraft:summonable_entities",
        false,
    );
    redirecting(b, entity, execute);
    let on = b.literal(execute, "on", false);
    for relation in [
        "owner",
        "leasher",
        "target",
        "attacker",
        "vehicle",
        "controller",
        "origin",
        "passengers",
    ] {
        let node = b.literal(on, relation, false);
        redirecting(b, node, execute);
    }
}

fn data_endpoints(b: &mut TreeBuilder, parent: usize, source: bool) -> [usize; 3] {
    let key = if source { "source" } else { "target" };
    let entity = b.literal(parent, "entity", false);
    let entity_node = b.argument(entity, key, single_entity(), false);
    let block = b.literal(parent, "block", false);
    let block_node = b.argument(
        block,
        if source { "sourcePos" } else { "targetPos" },
        ArgumentParser::BlockPos,
        false,
    );
    let storage = b.literal(parent, "storage", false);
    let storage_node = b.argument(storage, key, ArgumentParser::ResourceLocation, false);
    [entity_node, block_node, storage_node]
}

fn data_commands(b: &mut TreeBuilder, root: usize) {
    let nbt_path = || ArgumentParser::Named("minecraft:nbt_path");
    let data = b.literal(root, "data", false);
    let merge = b.literal(data, "merge", false);
    for node in data_endpoints(b, merge, false) {
        b.argument(node, "nbt", ArgumentParser::NbtCompoundTag, true);
    }
    let get = b.literal(data, "get", false);
    for node in data_endpoints(b, get, true) {
        b.nodes[node].executable = true;
        let path = b.argument(node, "path", nbt_path(), true);
        b.argument(
            path,
            "scale",
            ArgumentParser::Double {
                min: None,
                max: None,
            },
            true,
        );
    }
    let remove = b.literal(data, "remove", false);
    for node in data_endpoints(b, remove, false) {
        b.argument(node, "path", nbt_path(), true);
    }
    let modify = b.literal(data, "modify", false);
    for node in data_endpoints(b, modify, false) {
        let path = b.argument(node, "targetPath", nbt_path(), false);
        let mut operations = Vec::new();
        for name in ["set", "merge", "append", "prepend"] {
            operations.push(b.literal(path, name, false));
        }
        let insert = b.literal(path, "insert", false);
        operations.push(b.argument(
            insert,
            "index",
            ArgumentParser::Integer {
                min: None,
                max: None,
            },
            false,
        ));
        for operation in operations {
            let from = b.literal(operation, "from", false);
            for source in data_endpoints(b, from, true) {
                b.nodes[source].executable = true;
                b.argument(source, "sourcePath", nbt_path(), true);
            }
            let string = b.literal(operation, "string", false);
            for source in data_endpoints(b, string, true) {
                b.nodes[source].executable = true;
                let path = b.argument(source, "sourcePath", nbt_path(), true);
                let start = b.argument(
                    path,
                    "start",
                    ArgumentParser::Integer {
                        min: None,
                        max: None,
                    },
                    true,
                );
                b.argument(
                    start,
                    "end",
                    ArgumentParser::Integer {
                        min: None,
                        max: None,
                    },
                    true,
                );
            }
            let value = b.literal(operation, "value", false);
            b.argument(
                value,
                "value",
                ArgumentParser::Named("minecraft:nbt_tag"),
                true,
            );
        }
    }
}

fn rule_commands(b: &mut TreeBuilder, root: usize) {
    let gamerule = b.literal(root, "gamerule", false);
    for name in crate::game_rule_packet::RULE_NAMES {
        let rule = b.literal(gamerule, name, true);
        let numeric = matches!(
            name,
            "players_sleeping_percentage"
                | "random_tick_speed"
                | "fire_spread_radius_around_player"
                | "max_snow_accumulation_height"
        );
        let parser = if numeric {
            ArgumentParser::Integer {
                min: None,
                max: None,
            }
        } else {
            ArgumentParser::Bool
        };
        b.argument(rule, "value", parser, true);
    }
    let default_mode = b.literal(root, "defaultgamemode", false);
    b.argument(default_mode, "gamemode", ArgumentParser::GameMode, true);
    let experience = b.literal(root, "experience", false);
    for action in ["add", "set"] {
        let node = b.literal(experience, action, false);
        let targets = b.argument(node, "targets", player_targets(), false);
        let amount = b.argument(
            targets,
            "amount",
            ArgumentParser::Integer {
                min: if action == "set" { Some(0) } else { None },
                max: None,
            },
            true,
        );
        for unit in ["points", "levels"] {
            b.literal(amount, unit, true);
        }
    }
    let query = b.literal(experience, "query", false);
    let target = b.argument(query, "targets", player_arg(), false);
    for unit in ["points", "levels"] {
        b.literal(target, unit, true);
    }
    b.alias(root, "xp", experience);
    let tag = b.literal(root, "tag", false);
    let targets = b.argument(tag, "targets", entities(), false);
    for action in ["add", "remove"] {
        let node = b.literal(targets, action, false);
        b.argument(node, "name", ArgumentParser::Word, true);
    }
    b.literal(targets, "list", true);
    let random = b.literal(root, "random", false);
    for mode in ["value", "roll"] {
        let node = b.literal(random, mode, false);
        let range = b.argument(
            node,
            "range",
            ArgumentParser::Named("minecraft:int_range"),
            true,
        );
        b.argument(range, "sequence", ArgumentParser::ResourceLocation, true);
    }
    let spawnpoint = b.literal(root, "spawnpoint", true);
    let targets = b.argument(spawnpoint, "targets", player_targets(), true);
    let pos = b.argument(targets, "pos", ArgumentParser::BlockPos, true);
    b.argument(
        pos,
        "angle",
        ArgumentParser::Float {
            min: None,
            max: None,
        },
        true,
    );
}

fn player_targets() -> ArgumentParser {
    ArgumentParser::Entity {
        single: false,
        players_only: true,
    }
}

fn block_commands(b: &mut TreeBuilder, root: usize) {
    let setblock = b.literal(root, "setblock", false);
    let pos = position_argument(b, setblock, "pos");
    let block = b.argument(pos, "block", ArgumentParser::BlockState, true);
    for mode in ["destroy", "keep", "replace"] {
        b.literal(block, mode, true);
    }
    let fill = b.literal(root, "fill", false);
    let from = position_argument(b, fill, "from");
    let to = position_argument(b, from, "to");
    let block = b.argument(to, "block", ArgumentParser::BlockState, true);
    let replace = b.literal(block, "replace", true);
    b.argument(
        replace,
        "filter",
        ArgumentParser::Named("minecraft:block_predicate"),
        true,
    );
    for mode in ["keep", "outline", "hollow", "destroy"] {
        b.literal(block, mode, true);
    }
    let clone = b.literal(root, "clone", false);
    let begin = position_argument(b, clone, "begin");
    let end = position_argument(b, begin, "end");
    let destination = position_argument(b, end, "destination");
    b.nodes[destination].executable = true;
    let replace = b.literal(destination, "replace", true);
    let masked = b.literal(destination, "masked", true);
    let filtered = b.literal(destination, "filtered", false);
    let filter = b.argument(
        filtered,
        "filter",
        ArgumentParser::Named("minecraft:block_predicate"),
        true,
    );
    for parent in [replace, masked, filter] {
        for flag in ["force", "move", "normal"] {
            b.literal(parent, flag, true);
        }
    }
}

fn gamemaster_commands(b: &mut TreeBuilder, root: usize) {
    block_commands(b, root);
    data_commands(b, root);
    rule_commands(b, root);
    b.literal(root, "seed", true);
    let say = b.literal(root, "say", false);
    b.argument(say, "message", ArgumentParser::Message, true);
    let gamemode = b.literal(root, "gamemode", false);
    for mode in ["survival", "creative", "adventure", "spectator"] {
        let m = b.literal(gamemode, mode, true);
        b.argument(m, "target", player_arg(), true);
    }
    let teleport = b.literal(root, "teleport", false);
    let targets = b.argument(teleport, "targets", entities(), false);
    b.argument(targets, "location", ArgumentParser::Vec3, true);
    b.argument(targets, "destination", single_entity(), true);
    b.argument(teleport, "location", ArgumentParser::Vec3, true);
    b.argument(teleport, "destination", single_entity(), true);
    b.alias(root, "tp", teleport);
    execute_tree(b, root);
    let summon = b.literal(root, "summon", false);
    let entity = b.suggesting(
        summon,
        "entity",
        ArgumentParser::Resource {
            registry: "minecraft:entity_type".to_owned(),
        },
        "minecraft:summonable_entities",
        true,
    );
    let pos = b.argument(entity, "pos", ArgumentParser::Vec3, true);
    b.argument(pos, "nbt", ArgumentParser::NbtCompoundTag, true);
    let time = b.literal(root, "time", false);
    let set = b.literal(time, "set", false);
    for preset in ["day", "noon", "night", "midnight"] {
        b.literal(set, preset, true);
    }
    b.argument(set, "time", ArgumentParser::Time { min: 0 }, true);
    let add = b.literal(time, "add", false);
    b.argument(add, "time", ArgumentParser::Time { min: 0 }, true);
    let query = b.literal(time, "query", false);
    for q in ["daytime", "gametime", "day"] {
        b.literal(query, q, true);
    }
    let give = b.literal(root, "give", false);
    let who = b.argument(give, "targets", player_arg(), false);
    let item = b.argument(who, "item", ArgumentParser::ItemStack, true);
    b.argument(
        item,
        "count",
        ArgumentParser::Integer {
            min: Some(1),
            max: Some(6400),
        },
        true,
    );
    let clear = b.literal(root, "clear", true);
    b.argument(clear, "targets", player_arg(), true);
    let kill = b.literal(root, "kill", true);
    b.argument(kill, "targets", entities(), true);
    let difficulty = b.literal(root, "difficulty", true);
    for d in ["peaceful", "easy", "normal", "hard"] {
        b.literal(difficulty, d, true);
    }
    let weather = b.literal(root, "weather", false);
    for w in ["clear", "rain", "thunder"] {
        b.literal(weather, w, true);
    }
    let spawn = b.literal(root, "setworldspawn", true);
    b.argument(spawn, "pos", ArgumentParser::BlockPos, true);
    let effect = b.literal(root, "effect", false);
    let clear = b.literal(effect, "clear", true);
    let clear_targets = b.argument(clear, "targets", entities(), true);
    b.argument(clear_targets, "effect", mob_effect(), true);
    let give = b.literal(effect, "give", false);
    let give_targets = b.argument(give, "targets", entities(), false);
    let give_effect = b.argument(give_targets, "effect", mob_effect(), true);
    let seconds = b.argument(
        give_effect,
        "seconds",
        ArgumentParser::Integer {
            min: Some(1),
            max: Some(1_000_000),
        },
        true,
    );
    let infinite = b.literal(give_effect, "infinite", true);
    for duration in [seconds, infinite] {
        let amplifier = b.argument(
            duration,
            "amplifier",
            ArgumentParser::Integer {
                min: Some(0),
                max: Some(255),
            },
            true,
        );
        b.argument(amplifier, "hideParticles", ArgumentParser::Bool, true);
    }
}

fn admin_commands(b: &mut TreeBuilder, root: usize) {
    let kick = b.literal(root, "kick", false);
    let kicked = b.argument(kick, "targets", player_arg(), true);
    b.argument(kicked, "reason", ArgumentParser::Message, true);
    for name in ["op", "deop"] {
        let node = b.literal(root, name, false);
        b.argument(node, "targets", ArgumentParser::GameProfile, true);
    }
    let whitelist = b.literal(root, "whitelist", false);
    for action in ["on", "off", "list", "reload"] {
        b.literal(whitelist, action, true);
    }
    for action in ["add", "remove"] {
        let node = b.literal(whitelist, action, false);
        b.argument(node, "targets", ArgumentParser::GameProfile, true);
    }
    let ban = b.literal(root, "ban", false);
    let banned = b.argument(ban, "targets", ArgumentParser::GameProfile, true);
    b.argument(banned, "reason", ArgumentParser::Message, true);
    let ban_ip = b.literal(root, "ban-ip", false);
    let address = b.argument(ban_ip, "target", ArgumentParser::Word, true);
    b.argument(address, "reason", ArgumentParser::Message, true);
    let pardon = b.literal(root, "pardon", false);
    b.argument(pardon, "targets", ArgumentParser::GameProfile, true);
    let pardon_ip = b.literal(root, "pardon-ip", false);
    b.argument(pardon_ip, "target", ArgumentParser::Word, true);
    let banlist = b.literal(root, "banlist", true);
    for list in ["ips", "players"] {
        b.literal(banlist, list, true);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn owner_tree_contains_gamemode_and_redirects_resolve() {
        let tree = build_tree(4);
        let names: Vec<&str> = tree.nodes[tree.root]
            .children
            .iter()
            .filter_map(|i| match &tree.nodes[*i].kind {
                CommandNodeKind::Literal(name) => Some(name.as_str()),
                _ => None,
            })
            .collect();
        assert!(names.contains(&"gamemode"));
        assert!(names.contains(&"tp"));
        assert!(names.contains(&"summon"));
        assert!(names.contains(&"execute"));
        for name in ["ban", "ban-ip", "pardon", "pardon-ip", "banlist"] {
            assert!(names.contains(&name), "{name}");
        }
        let summon_entity = tree.nodes.iter().find(|n| {
            matches!(&n.kind, CommandNodeKind::Argument { name, suggests, .. }
                if name == "entity" && suggests.as_deref() == Some("minecraft:summonable_entities"))
        });
        assert!(summon_entity.is_some());
        for node in &tree.nodes {
            if let Some(target) = node.redirect {
                assert!(target < tree.nodes.len());
            }
            assert!(node.children.iter().all(|c| *c < tree.nodes.len()));
        }
    }

    #[test]
    fn execute_offers_every_vanilla_modifier_and_redirects_back() {
        let tree = build_tree(2);
        let literal = |parent: usize, name: &str| {
            tree.nodes[parent]
                .children
                .iter()
                .copied()
                .find(|c| tree.nodes[*c].kind == CommandNodeKind::Literal(name.to_owned()))
        };
        let execute = literal(tree.root, "execute").expect("execute");
        for name in [
            "as",
            "at",
            "positioned",
            "rotated",
            "facing",
            "align",
            "anchored",
            "in",
            "on",
            "summon",
            "if",
            "unless",
            "store",
            "run",
        ] {
            assert!(literal(execute, name).is_some(), "{name}");
        }
        let run = literal(execute, "run").expect("run");
        assert_eq!(tree.nodes[run].redirect, Some(tree.root));
        let at = literal(execute, "at").expect("at");
        let targets = tree.nodes[at].children[0];
        assert_eq!(tree.nodes[targets].redirect, Some(execute));
        let unless = literal(execute, "unless").expect("unless");
        for name in [
            "block",
            "blocks",
            "biome",
            "loaded",
            "dimension",
            "score",
            "entity",
            "predicate",
            "function",
            "data",
        ] {
            assert!(literal(unless, name).is_some(), "{name}");
        }
        let store = literal(execute, "store").expect("store");
        let result = literal(store, "result").expect("result");
        for name in ["score", "bossbar", "entity", "block", "storage"] {
            assert!(literal(result, name).is_some(), "{name}");
        }
    }

    #[test]
    fn each_level_sees_only_what_it_may_use() {
        let roots = |level: u8| -> Vec<String> {
            let tree = build_tree(level);
            tree.nodes[tree.root]
                .children
                .iter()
                .filter_map(|i| match &tree.nodes[*i].kind {
                    CommandNodeKind::Literal(name) => Some(name.clone()),
                    _ => None,
                })
                .collect()
        };
        let has = |level: u8, name: &str| roots(level).iter().any(|n| n == name);
        assert!(has(0, "msg") && !has(0, "gamemode") && !has(0, "seed"));
        assert!(has(1, "msg") && !has(1, "gamemode"));
        assert!(has(2, "gamemode") && has(2, "tp") && !has(2, "ban") && !has(2, "stop"));
        assert!(has(3, "ban") && has(3, "op") && !has(3, "stop") && !has(3, "save-all"));
        assert!(has(4, "stop") && has(4, "save-all") && has(4, "ban"));
        for level in 0..=4 {
            for name in roots(level) {
                assert!(
                    crate::commands::permission::may_use(&name, level),
                    "{name} at {level}"
                );
            }
        }
    }
}

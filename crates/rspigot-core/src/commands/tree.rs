mod extra;
mod functions;
mod game;
mod surface;

use crate::plugins::{ArgumentKind, CommandArgument, PluginRoot};

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
    ResourceKey { registry: String },
    ResourceOrTagKey { registry: String },
    ResourceSelector { registry: String },
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

const ASK_SERVER: &str = "minecraft:ask_server";

fn parser_of(kind: ArgumentKind) -> ArgumentParser {
    match kind {
        ArgumentKind::Player => ArgumentParser::Entity {
            single: true,
            players_only: true,
        },
        ArgumentKind::Entities => ArgumentParser::Entity {
            single: false,
            players_only: false,
        },
        ArgumentKind::Item => ArgumentParser::ItemStack,
        ArgumentKind::Integer => ArgumentParser::Integer {
            min: None,
            max: None,
        },
        ArgumentKind::Double => ArgumentParser::Double {
            min: None,
            max: None,
        },
        ArgumentKind::Float => ArgumentParser::Float {
            min: None,
            max: None,
        },
        ArgumentKind::Location => ArgumentParser::Vec3,
        ArgumentKind::GreedyText => ArgumentParser::GreedyString,
        ArgumentKind::Text | ArgumentKind::Choice | ArgumentKind::Literal => ArgumentParser::Word,
        ArgumentKind::Boolean => ArgumentParser::Bool,
    }
}

fn node_of(argument: &CommandArgument) -> CommandNodeKind {
    if argument.kind == ArgumentKind::Literal {
        return CommandNodeKind::Literal(argument.name.to_ascii_lowercase());
    }
    CommandNodeKind::Argument {
        name: argument.name.clone(),
        parser: parser_of(argument.kind),
        suggests: (argument.suggested || argument.kind == ArgumentKind::Choice)
            .then(|| ASK_SERVER.to_owned()),
    }
}

impl CommandTree {
    pub(crate) fn with_plugin_commands(&self, roots: &[PluginRoot<'_>]) -> Self {
        let mut tree = self.clone();
        for root in roots {
            let executable = root
                .specs
                .iter()
                .any(|spec| spec.arguments.iter().all(|argument| argument.optional));
            let head = tree.attach(
                tree.root,
                CommandNodeKind::Literal(root.name.clone()),
                executable,
            );
            for spec in &root.specs {
                let mut parent = head;
                for (index, argument) in spec.arguments.iter().enumerate() {
                    let executable = spec.arguments[index + 1..]
                        .iter()
                        .all(|later| later.optional);
                    parent = tree.merge(parent, node_of(argument), executable);
                }
            }
            for alias in &root.aliases {
                let index = tree.attach(
                    tree.root,
                    CommandNodeKind::Literal(alias.clone()),
                    executable,
                );
                tree.nodes[index].redirect = Some(head);
            }
        }
        tree
    }

    fn merge(&mut self, parent: usize, kind: CommandNodeKind, executable: bool) -> usize {
        let same = self.nodes[parent]
            .children
            .iter()
            .copied()
            .find(|child| self.nodes[*child].kind == kind);
        match same {
            Some(child) => {
                self.nodes[child].executable |= executable;
                child
            }
            None => self.attach(parent, kind, executable),
        }
    }

    fn attach(&mut self, parent: usize, kind: CommandNodeKind, executable: bool) -> usize {
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

fn time_subtree(b: &mut TreeBuilder, parent: usize, of_clock: bool) {
    let set = b.literal(parent, "set", false);
    b.argument(set, "time", ArgumentParser::Time { min: 0 }, true);
    b.argument(set, "timemarker", ArgumentParser::ResourceLocation, true);
    let add = b.literal(parent, "add", false);
    b.argument(add, "time", ArgumentParser::Time { min: i32::MIN }, true);
    b.literal(parent, "pause", true);
    b.literal(parent, "resume", true);
    let rate = b.literal(parent, "rate", false);
    b.argument(
        rate,
        "rate",
        ArgumentParser::Float {
            min: Some(1.0e-5),
            max: Some(1000.0),
        },
        true,
    );
    let query = b.literal(parent, "query", false);
    b.literal(query, "time", true);
    if !of_clock {
        b.literal(query, "gametime", true);
    }
    let timeline = b.argument(
        query,
        "timeline",
        ArgumentParser::Resource {
            registry: "minecraft:timeline".to_owned(),
        },
        true,
    );
    b.literal(timeline, "repetition", true);
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
    for name in ["version", "plugins"] {
        b.literal(root, name, true);
    }
    if level >= super::permission::LEVEL_MODERATORS {
        b.literal(root, "tps", true);
    }
    let help = b.literal(root, "help", true);
    b.argument(help, "command", ArgumentParser::GreedyString, true);
    let list = b.literal(root, "list", true);
    b.literal(list, "uuids", true);
    random_commands(&mut b, root, level >= LEVEL_GAMEMASTERS);
    let me = b.literal(root, "me", false);
    b.argument(me, "action", ArgumentParser::Message, true);
    let msg = b.literal(root, "msg", false);
    let target = b.argument(msg, "targets", player_targets(), false);
    b.argument(target, "message", ArgumentParser::Message, true);
    b.alias(root, "tell", msg);
    b.alias(root, "w", msg);
    open_score_commands(&mut b, root);
    if level >= LEVEL_GAMEMASTERS {
        gamemaster_commands(&mut b, root);
    }
    if level >= LEVEL_ADMINS {
        admin_commands(&mut b, root);
    }
    if level >= LEVEL_OWNERS {
        let save_all = b.literal(root, "save-all", true);
        b.literal(save_all, "flush", true);
        for name in ["save-on", "save-off", "stop", "restart"] {
            b.literal(root, name, true);
        }
        surface::owner_surface(&mut b, root);
        let spigot = b.literal(root, "spigot", false);
        b.literal(spigot, "reload", true);
        let timings = b.literal(root, "timings", false);
        for action in ["reset", "report", "on", "off", "paste", "verbon", "verboff"] {
            b.literal(timings, action, true);
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
    let pos = position_argument(b, block, "target");
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
    let pos = position_argument(b, block, "source");
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
    let items = b.literal(parent, "items", false);
    let slots = b.literal(parent, "slots", false);
    for (root, with_predicate) in [(items, true), (slots, false)] {
        let block = b.literal(root, "block", false);
        let pos = position_argument(b, block, "source");
        let slot = b.argument(
            pos,
            "slots",
            ArgumentParser::Named("minecraft:slot_source"),
            !with_predicate,
        );
        let entity = b.literal(root, "entity", false);
        let who = b.argument(entity, "source", entities(), false);
        let entity_slot = b.argument(
            who,
            "slots",
            ArgumentParser::Named("minecraft:slot_source"),
            !with_predicate,
        );
        for node in [slot, entity_slot] {
            if with_predicate {
                let predicate = b.argument(
                    node,
                    "item_predicate",
                    ArgumentParser::Named("minecraft:item_predicate"),
                    false,
                );
                finish(b, predicate);
            } else {
                finish(b, node);
            }
        }
    }
    let stopwatch = b.literal(parent, "stopwatch", false);
    let id = b.argument(stopwatch, "id", ArgumentParser::ResourceLocation, false);
    let range = b.argument(
        id,
        "range",
        ArgumentParser::Named("minecraft:float_range"),
        false,
    );
    finish(b, range);
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
    let block_node = b.argument(block, key, ArgumentParser::BlockPos, false);
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
    for node in data_endpoints(b, get, false) {
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
            let compute = b.literal(operation, "compute", false);
            let block = b.literal(compute, "block", false);
            let block_pos = position_argument(b, block, "computePos");
            let default = b.literal(compute, "default", false);
            let entity = b.literal(compute, "entity", false);
            let entity_target = b.argument(entity, "computeTarget", single_entity(), false);
            for parent in [block_pos, default, entity_target] {
                for (kind, parser) in [
                    ("float", "minecraft:context_float_provider"),
                    ("integer", "minecraft:context_int_provider"),
                ] {
                    let node = b.literal(parent, kind, false);
                    b.argument(node, "provider", ArgumentParser::Named(parser), true);
                }
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

fn text_commands(b: &mut TreeBuilder, root: usize) {
    let component = || ArgumentParser::Named("minecraft:component");
    let tellraw = b.literal(root, "tellraw", false);
    let targets = b.argument(tellraw, "targets", player_targets(), false);
    b.argument(targets, "message", component(), true);
    let title = b.literal(root, "title", false);
    let targets = b.argument(title, "targets", player_targets(), false);
    for name in ["clear", "reset"] {
        b.literal(targets, name, true);
    }
    for name in ["title", "subtitle", "actionbar"] {
        let node = b.literal(targets, name, false);
        b.argument(node, "title", component(), true);
    }
    let times = b.literal(targets, "times", false);
    let fade_in = b.argument(times, "fadeIn", ArgumentParser::Time { min: 0 }, false);
    let stay = b.argument(fade_in, "stay", ArgumentParser::Time { min: 0 }, false);
    b.argument(stay, "fadeOut", ArgumentParser::Time { min: 0 }, true);
}

fn rule_commands(b: &mut TreeBuilder, root: usize) {
    let gamerule = b.literal(root, "gamerule", false);
    for name in crate::game_rule_packet::RULE_NAMES {
        let rule = b.literal(gamerule, name, true);
        let parser = match crate::game_rule_packet::integer_bounds(name) {
            Some((min, max)) => ArgumentParser::Integer {
                min: (min != i32::MIN).then_some(min),
                max: (max != i32::MAX).then_some(max),
            },
            None => ArgumentParser::Bool,
        };
        b.argument(rule, "value", parser, true);
    }
    let default_mode = b.literal(root, "defaultgamemode", false);
    b.argument(default_mode, "gamemode", ArgumentParser::GameMode, true);
    let experience = b.literal(root, "experience", false);
    for action in ["add", "set"] {
        let node = b.literal(experience, action, false);
        let targets = b.argument(node, "target", player_targets(), false);
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
    let target = b.argument(query, "target", player_arg(), false);
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
    let spawnpoint = b.literal(root, "spawnpoint", true);
    let targets = b.argument(spawnpoint, "targets", player_targets(), true);
    let pos = b.argument(targets, "pos", ArgumentParser::BlockPos, true);
    b.argument(
        pos,
        "rotation",
        ArgumentParser::Named("minecraft:rotation"),
        true,
    );
}

fn random_commands(b: &mut TreeBuilder, root: usize, gamemaster: bool) {
    let random = b.literal(root, "random", false);
    for mode in ["value", "roll"] {
        let node = b.literal(random, mode, false);
        let range = b.argument(
            node,
            "range",
            ArgumentParser::Named("minecraft:int_range"),
            true,
        );
        if gamemaster {
            b.argument(range, "sequence", ArgumentParser::ResourceLocation, true);
        }
    }
    if gamemaster {
        let reset = b.literal(random, "reset", false);
        let all = b.literal(reset, "*", true);
        let sequence = b.argument(reset, "sequence", ArgumentParser::ResourceLocation, true);
        for parent in [all, sequence] {
            let seed = b.argument(
                parent,
                "seed",
                ArgumentParser::Integer {
                    min: None,
                    max: None,
                },
                true,
            );
            let world = b.argument(seed, "includeWorldSeed", ArgumentParser::Bool, true);
            b.argument(world, "includeSequenceId", ArgumentParser::Bool, true);
        }
    }
}

fn objective() -> ArgumentParser {
    ArgumentParser::Named("minecraft:objective")
}

fn score_holders() -> ArgumentParser {
    ArgumentParser::ScoreHolder { multiple: true }
}

fn component_argument() -> ArgumentParser {
    ArgumentParser::Named("minecraft:component")
}

fn open_score_commands(b: &mut TreeBuilder, root: usize) {
    let trigger = b.literal(root, "trigger", false);
    let name = b.argument(trigger, "objective", objective(), true);
    for mode in ["add", "set"] {
        let node = b.literal(name, mode, false);
        b.argument(
            node,
            "value",
            ArgumentParser::Integer {
                min: None,
                max: None,
            },
            true,
        );
    }
    let teammsg = b.literal(root, "teammsg", false);
    b.argument(teammsg, "message", ArgumentParser::Message, true);
    b.alias(root, "tm", teammsg);
}

fn number_format_nodes(b: &mut TreeBuilder, parent: usize) {
    b.literal(parent, "blank", true);
    let fixed = b.literal(parent, "fixed", false);
    b.argument(fixed, "contents", component_argument(), true);
    let styled = b.literal(parent, "styled", false);
    b.argument(
        styled,
        "style",
        ArgumentParser::Named("minecraft:style"),
        true,
    );
}

fn scoreboard_commands(b: &mut TreeBuilder, root: usize) {
    let scoreboard = b.literal(root, "scoreboard", false);
    let objectives = b.literal(scoreboard, "objectives", false);
    b.literal(objectives, "list", true);
    let add = b.literal(objectives, "add", false);
    let name = b.argument(add, "objective", ArgumentParser::Word, false);
    let criteria = b.argument(
        name,
        "criteria",
        ArgumentParser::Named("minecraft:objective_criteria"),
        true,
    );
    b.argument(criteria, "displayName", component_argument(), true);
    let remove = b.literal(objectives, "remove", false);
    b.argument(remove, "objective", objective(), true);
    let setdisplay = b.literal(objectives, "setdisplay", false);
    let slot = b.argument(
        setdisplay,
        "slot",
        ArgumentParser::Named("minecraft:scoreboard_slot"),
        true,
    );
    b.argument(slot, "objective", objective(), true);
    let modify = b.literal(objectives, "modify", false);
    let target = b.argument(modify, "objective", objective(), false);
    let displayname = b.literal(target, "displayname", false);
    b.argument(displayname, "displayName", component_argument(), true);
    let rendertype = b.literal(target, "rendertype", false);
    for kind in ["hearts", "integer"] {
        b.literal(rendertype, kind, true);
    }
    let auto_update = b.literal(target, "displayautoupdate", false);
    b.argument(auto_update, "value", ArgumentParser::Bool, true);
    let numberformat = b.literal(target, "numberformat", true);
    number_format_nodes(b, numberformat);
    let players = b.literal(scoreboard, "players", false);
    let list = b.literal(players, "list", true);
    b.argument(
        list,
        "target",
        ArgumentParser::ScoreHolder { multiple: false },
        true,
    );
    let get = b.literal(players, "get", false);
    let holder = b.argument(
        get,
        "target",
        ArgumentParser::ScoreHolder { multiple: false },
        false,
    );
    b.argument(holder, "objective", objective(), true);
    for action in ["set", "add", "remove"] {
        let node = b.literal(players, action, false);
        let targets = b.argument(node, "targets", score_holders(), false);
        let name = b.argument(targets, "objective", objective(), false);
        b.argument(
            name,
            "score",
            ArgumentParser::Integer {
                min: if action == "set" { None } else { Some(0) },
                max: None,
            },
            true,
        );
    }
    let reset = b.literal(players, "reset", false);
    let targets = b.argument(reset, "targets", score_holders(), true);
    b.argument(targets, "objective", objective(), true);
    let enable = b.literal(players, "enable", false);
    let targets = b.argument(enable, "targets", score_holders(), false);
    b.argument(targets, "objective", objective(), true);
    let operation = b.literal(players, "operation", false);
    let targets = b.argument(operation, "targets", score_holders(), false);
    let target_objective = b.argument(targets, "targetObjective", objective(), false);
    let operator = b.argument(
        target_objective,
        "operation",
        ArgumentParser::Named("minecraft:operation"),
        false,
    );
    let source = b.argument(operator, "source", score_holders(), false);
    b.argument(source, "sourceObjective", objective(), true);
    let display = b.literal(players, "display", false);
    let name = b.literal(display, "name", false);
    let targets = b.argument(name, "targets", score_holders(), false);
    let name_objective = b.argument(targets, "objective", objective(), true);
    b.argument(name_objective, "name", component_argument(), true);
    let format = b.literal(display, "numberformat", false);
    let targets = b.argument(format, "targets", score_holders(), false);
    let format_objective = b.argument(targets, "objective", objective(), true);
    number_format_nodes(b, format_objective);
}

fn bossbar_commands(b: &mut TreeBuilder, root: usize) {
    let bossbar = b.literal(root, "bossbar", false);
    let id_of = |b: &mut TreeBuilder, parent: usize, executable: bool| {
        b.argument(parent, "id", ArgumentParser::ResourceLocation, executable)
    };
    let add = b.literal(bossbar, "add", false);
    let id = id_of(b, add, false);
    b.argument(id, "name", component_argument(), true);
    let remove = b.literal(bossbar, "remove", false);
    id_of(b, remove, true);
    b.literal(bossbar, "list", true);
    let get = b.literal(bossbar, "get", false);
    let id = id_of(b, get, false);
    for field in ["value", "max", "visible", "players"] {
        b.literal(id, field, true);
    }
    let set = b.literal(bossbar, "set", false);
    let id = id_of(b, set, false);
    let color = b.literal(id, "color", false);
    for name in ["pink", "blue", "red", "green", "yellow", "purple", "white"] {
        b.literal(color, name, true);
    }
    let style = b.literal(id, "style", false);
    for name in [
        "progress",
        "notched_6",
        "notched_10",
        "notched_12",
        "notched_20",
    ] {
        b.literal(style, name, true);
    }
    let name = b.literal(id, "name", false);
    b.argument(name, "name", component_argument(), true);
    for (field, min) in [("value", 0), ("max", 1)] {
        let node = b.literal(id, field, false);
        b.argument(
            node,
            field,
            ArgumentParser::Integer {
                min: Some(min),
                max: None,
            },
            true,
        );
    }
    let visible = b.literal(id, "visible", false);
    b.argument(visible, "visible", ArgumentParser::Bool, true);
    let players = b.literal(id, "players", true);
    b.argument(players, "targets", player_targets(), true);
}

fn team_commands(b: &mut TreeBuilder, root: usize) {
    let team_name = || ArgumentParser::Named("minecraft:team");
    let team = b.literal(root, "team", false);
    let list = b.literal(team, "list", true);
    b.argument(list, "team", team_name(), true);
    let add = b.literal(team, "add", false);
    let name = b.argument(add, "team", ArgumentParser::Word, true);
    b.argument(name, "displayName", component_argument(), true);
    let remove = b.literal(team, "remove", false);
    b.argument(remove, "team", team_name(), true);
    let empty = b.literal(team, "empty", false);
    b.argument(empty, "team", team_name(), true);
    let join = b.literal(team, "join", false);
    let target = b.argument(join, "team", team_name(), true);
    b.argument(target, "members", score_holders(), true);
    let leave = b.literal(team, "leave", false);
    b.argument(leave, "members", score_holders(), true);
    let modify = b.literal(team, "modify", false);
    let target = b.argument(modify, "team", team_name(), false);
    let display = b.literal(target, "displayName", false);
    b.argument(display, "displayName", component_argument(), true);
    let color = b.literal(target, "color", false);
    b.argument(
        color,
        "value",
        ArgumentParser::Named("minecraft:team_color"),
        true,
    );
    b.literal(color, "reset", true);
    for flag in ["friendlyFire", "seeFriendlyInvisibles"] {
        let node = b.literal(target, flag, false);
        b.argument(node, "allowed", ArgumentParser::Bool, true);
    }
    for (option, values) in [
        (
            "nametagVisibility",
            ["always", "never", "hideForOtherTeams", "hideForOwnTeam"],
        ),
        (
            "deathMessageVisibility",
            ["always", "never", "hideForOtherTeams", "hideForOwnTeam"],
        ),
        (
            "collisionRule",
            ["always", "never", "pushOtherTeams", "pushOwnTeam"],
        ),
    ] {
        let node = b.literal(target, option, false);
        for value in values {
            b.literal(node, value, true);
        }
    }
    for option in ["prefix", "suffix"] {
        let node = b.literal(target, option, false);
        b.argument(node, option, component_argument(), true);
    }
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
    for mode in ["destroy", "keep", "replace", "strict"] {
        b.literal(block, mode, true);
    }
    let fill = b.literal(root, "fill", false);
    let from = position_argument(b, fill, "from");
    let to = position_argument(b, from, "to");
    let block = b.argument(to, "block", ArgumentParser::BlockState, true);
    let replace = b.literal(block, "replace", true);
    let filter = b.argument(
        replace,
        "filter",
        ArgumentParser::Named("minecraft:block_predicate"),
        true,
    );
    for mode in ["destroy", "hollow", "outline", "strict"] {
        b.literal(filter, mode, true);
    }
    for mode in ["destroy", "hollow", "keep", "outline", "strict"] {
        b.literal(block, mode, true);
    }
    let clone = b.literal(root, "clone", false);
    let begin = position_argument(b, clone, "begin");
    let end = position_argument(b, begin, "end");
    clone_destination(b, end);
    let to_node = b.literal(end, "to", false);
    let target_dimension = b.argument(to_node, "targetDimension", ArgumentParser::Dimension, false);
    clone_destination(b, target_dimension);
    let from_node = b.literal(clone, "from", false);
    let source_dimension = b.argument(
        from_node,
        "sourceDimension",
        ArgumentParser::Dimension,
        false,
    );
    let begin = position_argument(b, source_dimension, "begin");
    let end = position_argument(b, begin, "end");
    clone_destination(b, end);
    let to_node = b.literal(end, "to", false);
    let target_dimension = b.argument(to_node, "targetDimension", ArgumentParser::Dimension, false);
    clone_destination(b, target_dimension);
}

fn clone_destination(b: &mut TreeBuilder, parent: usize) {
    let destination = position_argument(b, parent, "destination");
    b.nodes[destination].executable = true;
    clone_modes(b, destination);
    let strict = b.literal(destination, "strict", true);
    clone_modes(b, strict);
}

fn clone_modes(b: &mut TreeBuilder, parent: usize) {
    let replace = b.literal(parent, "replace", true);
    let masked = b.literal(parent, "masked", true);
    let filtered = b.literal(parent, "filtered", false);
    let filter = b.argument(
        filtered,
        "filter",
        ArgumentParser::Named("minecraft:block_predicate"),
        true,
    );
    for node in [replace, masked, filter] {
        for flag in ["force", "move", "normal"] {
            b.literal(node, flag, true);
        }
    }
}

fn gamemaster_commands(b: &mut TreeBuilder, root: usize) {
    block_commands(b, root);
    data_commands(b, root);
    rule_commands(b, root);
    text_commands(b, root);
    scoreboard_commands(b, root);
    team_commands(b, root);
    bossbar_commands(b, root);
    b.literal(root, "seed", true);
    b.literal(root, "reload", true);
    let say = b.literal(root, "say", false);
    b.argument(say, "message", ArgumentParser::Message, true);
    let gamemode = b.literal(root, "gamemode", false);
    let mode = b.argument(gamemode, "gamemode", ArgumentParser::GameMode, true);
    b.argument(mode, "target", player_targets(), true);
    let teleport = b.literal(root, "teleport", false);
    let targets = b.argument(teleport, "targets", entities(), false);
    let location = b.argument(targets, "location", ArgumentParser::Vec3, true);
    b.argument(targets, "destination", single_entity(), true);
    b.argument(
        location,
        "rotation",
        ArgumentParser::Named("minecraft:rotation"),
        true,
    );
    let facing = b.literal(location, "facing", false);
    let entity = b.literal(facing, "entity", false);
    let face = b.argument(entity, "facingEntity", single_entity(), true);
    b.argument(
        face,
        "facingAnchor",
        ArgumentParser::Named("minecraft:entity_anchor"),
        true,
    );
    b.argument(facing, "facingLocation", ArgumentParser::Vec3, true);
    b.argument(teleport, "location", ArgumentParser::Vec3, true);
    b.argument(teleport, "destination", single_entity(), true);
    b.alias(root, "tp", teleport);
    execute_tree(b, root);
    extra::extra_commands(b, root);
    functions::function_commands(b, root);
    game::game_commands(b, root);
    surface::gamemaster_surface(b, root);
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
    time_subtree(b, time, false);
    let of = b.literal(time, "of", false);
    let clock = b.argument(
        of,
        "clock",
        ArgumentParser::Resource {
            registry: "minecraft:world_clock".to_owned(),
        },
        false,
    );
    time_subtree(b, clock, true);
    let give = b.literal(root, "give", false);
    let who = b.argument(give, "targets", player_targets(), false);
    let item = b.argument(who, "item", ArgumentParser::ItemStack, true);
    b.argument(
        item,
        "count",
        ArgumentParser::Integer {
            min: Some(1),
            max: None,
        },
        true,
    );
    let clear = b.literal(root, "clear", true);
    let clear_targets = b.argument(clear, "targets", player_targets(), true);
    let clear_item = b.argument(
        clear_targets,
        "item",
        ArgumentParser::Named("minecraft:item_predicate"),
        true,
    );
    b.argument(
        clear_item,
        "maxCount",
        ArgumentParser::Integer {
            min: Some(0),
            max: None,
        },
        true,
    );
    let kill = b.literal(root, "kill", true);
    b.argument(kill, "targets", entities(), true);
    let difficulty = b.literal(root, "difficulty", true);
    for d in ["peaceful", "easy", "normal", "hard"] {
        b.literal(difficulty, d, true);
    }
    let weather = b.literal(root, "weather", false);
    for w in ["clear", "rain", "thunder"] {
        let node = b.literal(weather, w, true);
        b.argument(node, "duration", ArgumentParser::Time { min: 1 }, true);
    }
    let spawn = b.literal(root, "setworldspawn", true);
    let spawn_pos = b.argument(spawn, "pos", ArgumentParser::BlockPos, true);
    b.argument(
        spawn_pos,
        "rotation",
        ArgumentParser::Named("minecraft:rotation"),
        true,
    );
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
    extra::admin_extras(b, root);
    let kick = b.literal(root, "kick", false);
    let kicked = b.argument(kick, "targets", player_targets(), true);
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

    fn root_names(level: u8) -> Vec<String> {
        let tree = build_tree(level);
        tree.nodes[tree.root]
            .children
            .iter()
            .filter_map(|i| match &tree.nodes[*i].kind {
                CommandNodeKind::Literal(name) => Some(name.clone()),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn scoreboard_commands_follow_their_permission_levels() {
        assert!(!root_names(0).iter().any(|n| n == "bossbar"));
        assert!(root_names(2).iter().any(|n| n == "bossbar"));
        let player = root_names(0);
        for name in ["trigger", "teammsg", "tm"] {
            assert!(player.iter().any(|n| n == name), "{name}");
        }
        assert!(!player.iter().any(|n| n == "scoreboard" || n == "team"));
        let operator = root_names(2);
        assert!(operator.iter().any(|n| n == "scoreboard"));
        assert!(operator.iter().any(|n| n == "team"));
    }

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
    fn the_function_commands_follow_the_vanilla_tree() {
        let tree = build_tree(2);
        let literal = |parent: usize, name: &str| {
            tree.nodes[parent]
                .children
                .iter()
                .copied()
                .find(|c| tree.nodes[*c].kind == CommandNodeKind::Literal(name.to_owned()))
        };
        let argument = |parent: usize, wanted: &str| {
            tree.nodes[parent].children.iter().copied().find(|c| {
                matches!(&tree.nodes[*c].kind, CommandNodeKind::Argument { name, .. } if name == wanted)
            })
        };
        let function = literal(tree.root, "function").expect("function");
        let name = argument(function, "name").expect("name");
        assert!(tree.nodes[name].executable);
        assert!(matches!(
            &tree.nodes[name].kind,
            CommandNodeKind::Argument {
                parser: ArgumentParser::Named("minecraft:function"),
                suggests: Some(_),
                ..
            }
        ));
        let arguments = argument(name, "arguments").expect("arguments");
        assert!(matches!(
            &tree.nodes[arguments].kind,
            CommandNodeKind::Argument {
                parser: ArgumentParser::NbtCompoundTag,
                ..
            }
        ));
        let with = literal(name, "with").expect("with");
        for source in ["entity", "block", "storage"] {
            assert!(literal(with, source).is_some(), "{source}");
        }
        let schedule = literal(tree.root, "schedule").expect("schedule");
        let scheduled = literal(schedule, "function").expect("function");
        let name = argument(scheduled, "function").expect("function");
        let time = argument(name, "time").expect("time");
        assert!(matches!(
            &tree.nodes[time].kind,
            CommandNodeKind::Argument {
                parser: ArgumentParser::Time { min: 0 },
                ..
            }
        ));
        assert!(literal(time, "append").is_some() && literal(time, "replace").is_some());
        assert!(literal(schedule, "clear").is_some());
        let returning = literal(tree.root, "return").expect("return");
        assert!(literal(returning, "fail").is_some());
        let run = literal(returning, "run").expect("run");
        assert_eq!(tree.nodes[run].redirect, Some(tree.root));
        let datapack = literal(tree.root, "datapack").expect("datapack");
        for name in ["enable", "disable", "list"] {
            assert!(literal(datapack, name).is_some(), "{name}");
        }
        assert!(
            !root_names(1)
                .iter()
                .any(|n| n == "function" || n == "datapack")
        );
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

    #[test]
    fn the_k4_commands_follow_their_vanilla_levels() {
        let gamemaster = [
            "attribute",
            "damage",
            "enchant",
            "particle",
            "playsound",
            "stopsound",
            "ride",
            "rotate",
            "spectate",
            "swing",
            "forceload",
            "worldborder",
            "spreadplayers",
            "fillbiome",
        ];
        let admin = ["tick", "transfer", "setidletimeout", "debug"];
        let moderator = root_names(1);
        let operator = root_names(2);
        let admins = root_names(3);
        for name in gamemaster.iter().chain(&admin) {
            assert!(!moderator.iter().any(|n| n == name), "{name}");
        }
        for name in gamemaster {
            assert!(operator.iter().any(|n| n == name), "{name}");
            assert!(admins.iter().any(|n| n == name), "{name}");
        }
        for name in admin {
            assert!(!operator.iter().any(|n| n == name), "{name}");
            assert!(admins.iter().any(|n| n == name), "{name}");
        }
    }

    #[test]
    fn the_k4_trees_have_no_dangling_children_or_redirects() {
        let tree = build_tree(4);
        for node in &tree.nodes {
            assert!(node.children.iter().all(|c| *c < tree.nodes.len()));
            if let Some(target) = node.redirect {
                assert!(target < tree.nodes.len());
            }
        }
        let literal = |parent: usize, name: &str| {
            tree.nodes[parent]
                .children
                .iter()
                .copied()
                .find(|c| tree.nodes[*c].kind == CommandNodeKind::Literal(name.to_owned()))
        };
        let forceload = literal(tree.root, "forceload").expect("forceload");
        let remove = literal(forceload, "remove").expect("remove");
        assert!(literal(remove, "all").is_some());
        let border = literal(tree.root, "worldborder").expect("worldborder");
        let warning = literal(border, "warning").expect("warning");
        assert!(literal(warning, "distance").is_some() && literal(warning, "time").is_some());
        let tick = literal(tree.root, "tick").expect("tick");
        let step = literal(tick, "step").expect("step");
        assert!(tree.nodes[step].executable && literal(step, "stop").is_some());
    }
}

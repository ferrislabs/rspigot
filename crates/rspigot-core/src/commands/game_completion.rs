use rspigot_data::advancements;

use super::slots;
use super::tree::{CommandNodeKind, CommandTree};
use crate::ports::ClientLink;
use crate::server::Server;

const COMMANDS: [&str; 8] = [
    "locate",
    "place",
    "advancement",
    "recipe",
    "item",
    "loot",
    "waypoint",
    "stopwatch",
];

fn literal(tree: &CommandTree, parent: usize, word: &str) -> Option<usize> {
    tree.nodes[parent].children.iter().copied().find(
        |child| matches!(&tree.nodes[*child].kind, CommandNodeKind::Literal(name) if name == word),
    )
}

fn argument(tree: &CommandTree, parent: usize) -> Option<usize> {
    tree.nodes[parent]
        .children
        .iter()
        .copied()
        .find(|child| matches!(tree.nodes[*child].kind, CommandNodeKind::Argument { .. }))
}

impl<L: ClientLink> Server<L> {
    pub(crate) fn game_command_names(&self, level: u8, head: &str) -> Option<Vec<String>> {
        let words: Vec<&str> = head.split_whitespace().collect();
        let first = words.first()?.trim_start_matches("minecraft:");
        if !COMMANDS.contains(&first) {
            return None;
        }
        let tree = self.command_tree(level);
        let mut node = literal(&tree, tree.root, first)?;
        for word in &words[1..] {
            match literal(&tree, node, word).or_else(|| argument(&tree, node)) {
                Some(next) => node = next,
                None => return Some(Vec::new()),
            }
        }
        let mut names = Vec::new();
        for child in &tree.nodes[node].children {
            match &tree.nodes[*child].kind {
                CommandNodeKind::Literal(name) => names.push(name.clone()),
                CommandNodeKind::Argument { name, .. } => {
                    names.extend(self.argument_names(name, words.last().copied()));
                }
                CommandNodeKind::Root => {}
            }
        }
        Some(names)
    }

    fn argument_names(&self, argument: &str, previous: Option<&str>) -> Vec<String> {
        match argument {
            "targets" | "players" | "entities" | "target" | "source" | "waypoint" => self
                .players
                .values()
                .map(|player| player.profile.name.clone())
                .collect(),
            "advancement" => advancements::all()
                .iter()
                .map(|advancement| advancement.id.to_owned())
                .collect(),
            "criterion" => previous
                .map(|id| {
                    if id.contains(':') {
                        id.to_owned()
                    } else {
                        format!("minecraft:{id}")
                    }
                })
                .and_then(|id| advancements::find(&id))
                .map(|(_, found)| {
                    found
                        .criteria
                        .iter()
                        .map(|criterion| criterion.name.to_owned())
                        .collect()
                })
                .unwrap_or_default(),
            "recipe" => crate::recipe_book::all_recipe_ids()
                .map(str::to_owned)
                .collect(),
            "slots" | "sourceSlots" | "slot" => slots::names(),
            "loot_table" => rspigot_data::loot::tables()
                .iter()
                .map(|(name, _)| (*name).to_owned())
                .collect(),
            "modifier" => self
                .functions
                .data
                .item_modifier_ids()
                .map(str::to_owned)
                .collect(),
            "structure" => crate::locate::structure_ids()
                .map(str::to_owned)
                .chain(crate::locate::structure_tag_names().map(|tag| format!("#{tag}")))
                .collect(),
            "id" => self.stopwatches.keys().cloned().collect(),
            _ => Vec::new(),
        }
    }
}

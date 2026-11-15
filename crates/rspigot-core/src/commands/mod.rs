mod dispatch;
pub mod permission;
pub mod selector;
mod tree;

pub use dispatch::{CommandContext, CommandOutcome, CommandSender, Target, Weather, execute};
pub use tree::{ArgumentParser, CommandNode, CommandNodeKind, CommandTree, build_tree};

#[cfg(test)]
mod execute_tests;

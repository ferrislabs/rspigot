mod args;
mod blocks;
mod data;
mod dispatch;
mod execute;
pub mod permission;
pub mod selector;
mod server_commands;
pub mod source;
mod tree;

pub use dispatch::{CommandContext, CommandOutcome, CommandSender, Target, Weather, execute};
pub use tree::{ArgumentParser, CommandNode, CommandNodeKind, CommandTree, build_tree};

#[cfg(test)]
mod execute_tests;

#[cfg(test)]
mod selector_tests;

#[cfg(test)]
mod execute_chain_tests;

#[cfg(test)]
mod blocks_tests;

#[cfg(test)]
mod data_tests;

#[cfg(test)]
mod server_command_tests;

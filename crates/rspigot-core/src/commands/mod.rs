mod admin_commands;
mod args;
mod blocks;
mod boss_commands;
mod data;
mod dispatch;
mod entity_commands;
mod execute;
mod json_text;
mod movement_commands;
pub mod permission;
mod scoreboard_commands;
pub mod selector;
mod server_commands;
pub(crate) mod signable;
mod sound_commands;
pub mod source;
mod team_commands;
mod text_commands;
mod tick_commands;
mod tree;
mod world_commands;

pub use dispatch::{
    CommandContext, CommandOutcome, CommandSender, Target, TitleAction, Weather, execute,
};
pub use entity_commands::DamageSpec;
pub use tree::{ArgumentParser, CommandNode, CommandNodeKind, CommandTree, build_tree};
pub use world_commands::FillBiomeProblem;

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

#[cfg(test)]
mod scoreboard_command_tests;

#[cfg(test)]
mod boss_command_tests;

#[cfg(test)]
mod sound_command_tests;

#[cfg(test)]
mod entity_command_tests;

#[cfg(test)]
mod movement_command_tests;

#[cfg(test)]
mod tick_command_tests;

#[cfg(test)]
mod world_command_tests;

#[cfg(test)]
mod admin_command_tests;

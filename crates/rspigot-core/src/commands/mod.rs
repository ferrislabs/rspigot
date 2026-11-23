mod admin_commands;
mod advancement_commands;
mod announce;
mod args;
mod blocks;
mod boss_commands;
mod bukkit_commands;
mod chat_commands;
mod compute;
mod data;
mod dispatch;
mod entity_commands;
mod execute;
mod function_commands;
mod game_commands;
mod game_completion;
mod item_commands;
pub(crate) mod json_text;
mod locate_commands;
mod loot_commands;
pub(crate) mod loot_source;
mod movement_commands;
mod named;
pub mod permission;
mod place_commands;
mod player_commands;
mod recipe_commands;
mod scoreboard_commands;
pub mod selector;
mod server_commands;
pub(crate) mod signable;
mod slot_access;
pub(crate) mod slots;
mod sound_commands;
pub use surface::{ProfileQuery, Surface};
pub mod source;
mod stopwatch_commands;
mod surface;
mod surface_commands;
mod team_commands;
mod test_commands;
mod text_commands;
pub(crate) mod text_resolve;
mod tick_commands;
mod time_commands;
mod tree;
mod waypoint_commands;
mod world_commands;

pub(crate) use dispatch::refuse;
pub use dispatch::{
    CommandContext, CommandOutcome, CommandSender, FaceRequest, Target, TitleAction, Weather,
    execute,
};
pub use entity_commands::DamageSpec;
pub(crate) use function_commands::{run_scheduled, run_server_tag};
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

#[cfg(test)]
mod function_command_tests;

#[cfg(test)]
mod game_command_tests;

#[cfg(test)]
mod locate_command_tests;

use super::dispatch::{CommandContext, CommandOutcome, CommandSender};

pub(super) fn run<C: CommandContext>(
    ctx: &mut C,
    sender: CommandSender,
    label: &str,
    rest: &str,
) -> CommandOutcome {
    match label {
        "advancement" => super::advancement_commands::run(ctx, sender, rest),
        "recipe" => super::recipe_commands::run(ctx, sender, rest),
        "item" => super::item_commands::run(ctx, sender, rest),
        "locate" => super::locate_commands::run(ctx, sender, rest),
        "place" => super::place_commands::run(ctx, sender, rest),
        "loot" => super::loot_commands::run(ctx, sender, rest),
        "waypoint" => super::waypoint_commands::run(ctx, sender, rest),
        "stopwatch" => super::stopwatch_commands::run(ctx, sender, rest),
        _ => CommandOutcome::Unknown,
    }
}

use rspigot_domain::text::Component;

use super::dispatch::{
    CommandContext, CommandOutcome, CommandSender, fail, next_argument, select_one, usage,
};

const COLORS: [&str; 16] = [
    "black",
    "dark_blue",
    "dark_green",
    "dark_aqua",
    "dark_red",
    "dark_purple",
    "gold",
    "gray",
    "dark_gray",
    "blue",
    "green",
    "aqua",
    "red",
    "light_purple",
    "yellow",
    "white",
];

fn valid_hex(token: &str) -> bool {
    token.len() == 6 && token.chars().all(|c| c.is_ascii_hexdigit())
}

pub(super) fn run<C: CommandContext>(
    ctx: &mut C,
    sender: CommandSender,
    rest: &str,
) -> CommandOutcome {
    let help = "/waypoint (list|modify <waypoint> (color|style) ...)";
    let Some((action, rest)) = next_argument(rest) else {
        return usage(ctx, sender, help);
    };
    match action {
        "list" => {
            let dimension = ctx
                .current_dimension()
                .map_or_else(|| "minecraft:overworld".to_owned(), |id| id.to_string());
            ctx.reply(
                sender,
                Component::translatable(
                    "commands.waypoint.list.empty",
                    vec![Component::text(dimension)],
                ),
            );
            ctx.record_result(0);
            CommandOutcome::Success
        }
        "modify" => {
            let Some((selector, rest)) = next_argument(rest) else {
                return usage(ctx, sender, help);
            };
            let Some((property, rest)) = next_argument(rest) else {
                return usage(ctx, sender, help);
            };
            let valid = match (property, next_argument(rest)) {
                ("color", Some(("reset", _))) => true,
                ("color", Some(("hex", after))) => {
                    next_argument(after).is_some_and(|(hex, _)| valid_hex(hex))
                }
                ("color", Some((name, _))) => COLORS.contains(&name),
                ("style", Some(("reset", _))) => true,
                ("style", Some(("set", after))) => next_argument(after).is_some(),
                _ => false,
            };
            if !valid {
                return usage(ctx, sender, help);
            }
            if let Err(outcome) = select_one(ctx, sender, selector) {
                return outcome;
            }
            fail(ctx, sender, "argument.waypoint.invalid", Vec::new())
        }
        _ => usage(ctx, sender, help),
    }
}

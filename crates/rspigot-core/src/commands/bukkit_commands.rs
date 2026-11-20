use rspigot_domain::text::{Component, NamedColor};

use super::dispatch::{CommandContext, CommandOutcome, CommandSender, fail, next_argument, usage};

const TIMINGS_USAGE: &str = "/timings <reset|report|on|off|paste|verbon|verboff>";
const SPIGOT_USAGE: &str = "/spigot <reload>";

fn reply<C: CommandContext>(
    ctx: &mut C,
    sender: CommandSender,
    message: impl Into<String>,
) -> CommandOutcome {
    ctx.reply(sender, Component::text(message.into()));
    CommandOutcome::Success
}

fn coloured<C: CommandContext>(
    ctx: &mut C,
    sender: CommandSender,
    color: NamedColor,
    message: &str,
) {
    ctx.reply(sender, Component::text(message).color(color));
}

fn restart<C: CommandContext>(ctx: &mut C, sender: CommandSender) -> CommandOutcome {
    if !ctx.restart() {
        return fail(ctx, sender, "commands.reload.failure", Vec::new());
    }
    CommandOutcome::Success
}

fn spigot<C: CommandContext>(ctx: &mut C, sender: CommandSender, rest: &str) -> CommandOutcome {
    match next_argument(rest) {
        Some(("reload", _)) => {
            coloured(
                ctx,
                sender,
                NamedColor::Red,
                "Please note that this command is not supported and may cause issues when using some plugins.",
            );
            coloured(
                ctx,
                sender,
                NamedColor::Red,
                "If you encounter any issues please use the /stop command to restart your server.",
            );
            if !ctx.request_reload(false) {
                return fail(ctx, sender, "commands.reload.failure", Vec::new());
            }
            coloured(ctx, sender, NamedColor::Green, "Reload complete.");
            CommandOutcome::Success
        }
        _ => usage(ctx, sender, SPIGOT_USAGE),
    }
}

fn timings<C: CommandContext>(ctx: &mut C, sender: CommandSender, rest: &str) -> CommandOutcome {
    let Some((action, _)) = next_argument(rest) else {
        return usage(ctx, sender, TIMINGS_USAGE);
    };
    match action.to_ascii_lowercase().as_str() {
        "on" => {
            ctx.timings_enable(true);
            reply(ctx, sender, "Enabled Timings & Reset")
        }
        "off" => {
            ctx.timings_enable(false);
            reply(ctx, sender, "Disabled Timings")
        }
        "verbon" => {
            ctx.timings_verbose(true);
            reply(ctx, sender, "Enabled Verbose Timings")
        }
        "verboff" => {
            ctx.timings_verbose(false);
            reply(ctx, sender, "Disabled Verbose Timings")
        }
        "reset" => {
            ctx.timings_reset();
            reply(
                ctx,
                sender,
                "Timings reset. Please wait 5-10 minutes before using /timings report.",
            )
        }
        "report" | "paste" => {
            if !ctx.timings_enabled() {
                return reply(ctx, sender, "Please enable timings by typing /timings on");
            }
            match ctx.timings_report() {
                Some(file) => reply(
                    ctx,
                    sender,
                    format!("Timings report written to timings/{file} (nothing is uploaded)"),
                ),
                None => fail(ctx, sender, "commands.reload.failure", Vec::new()),
            }
        }
        _ => usage(ctx, sender, TIMINGS_USAGE),
    }
}

pub(super) fn run<C: CommandContext>(
    ctx: &mut C,
    sender: CommandSender,
    label: &str,
    rest: &str,
) -> CommandOutcome {
    match label {
        "restart" => restart(ctx, sender),
        "spigot" => spigot(ctx, sender, rest),
        "timings" => timings(ctx, sender, rest),
        _ => CommandOutcome::Unknown,
    }
}

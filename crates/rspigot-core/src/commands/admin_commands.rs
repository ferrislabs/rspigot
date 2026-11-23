use rspigot_domain::player::PlayerId;
use rspigot_domain::text::Component;

use super::dispatch::{CommandContext, CommandOutcome, CommandSender, fail, next_argument, usage};
use super::server_commands::players;

fn text(value: impl ToString) -> Component {
    Component::text(value.to_string())
}

fn transfer<C: CommandContext>(ctx: &mut C, sender: CommandSender, rest: &str) -> CommandOutcome {
    let help = "/transfer <hostname> [<port>] [<players>]";
    let Some((host, rest)) = next_argument(rest) else {
        return usage(ctx, sender, help);
    };
    let host = host.trim_matches('"');
    let mut port = 25_565;
    let mut rest = rest;
    if let Some((raw, after)) = next_argument(rest) {
        let Ok(value) = raw.parse::<i32>() else {
            return fail(ctx, sender, "parsing.int.invalid", vec![text(raw)]);
        };
        if value < 1 {
            return fail(
                ctx,
                sender,
                "argument.integer.low",
                vec![text(1), text(value)],
            );
        }
        if value > 65_535 {
            return fail(
                ctx,
                sender,
                "argument.integer.big",
                vec![text(65_535), text(value)],
            );
        }
        port = value;
        rest = after;
    }
    let targets: Vec<PlayerId> = match next_argument(rest) {
        Some((selector, _)) => match players(ctx, sender, selector) {
            Ok(found) => found,
            Err(outcome) => return outcome,
        },
        None => match sender {
            CommandSender::Player(id) => vec![id],
            CommandSender::Console => {
                return fail(
                    ctx,
                    sender,
                    "commands.transfer.error.no_players",
                    Vec::new(),
                );
            }
        },
    };
    for player in &targets {
        ctx.transfer_player(*player, host, port);
    }
    ctx.record_result(i32::try_from(targets.len()).unwrap_or(i32::MAX));
    let message = match targets.as_slice() {
        [only] => Component::translatable(
            "commands.transfer.success.single",
            vec![
                ctx.target_name(super::Target::Player(*only)),
                text(host),
                text(port),
            ],
        ),
        many => Component::translatable(
            "commands.transfer.success.multiple",
            vec![text(many.len()), text(host), text(port)],
        ),
    };
    ctx.reply(sender, message);
    CommandOutcome::Success
}

fn setidletimeout<C: CommandContext>(
    ctx: &mut C,
    sender: CommandSender,
    rest: &str,
) -> CommandOutcome {
    let Some((raw, _)) = next_argument(rest) else {
        return usage(ctx, sender, "/setidletimeout <minutes>");
    };
    let Ok(minutes) = raw.parse::<i32>() else {
        return fail(ctx, sender, "parsing.int.invalid", vec![text(raw)]);
    };
    if minutes < 0 {
        return fail(
            ctx,
            sender,
            "argument.integer.low",
            vec![text(0), text(minutes)],
        );
    }
    ctx.set_idle_timeout(u32::try_from(minutes).unwrap_or(0));
    ctx.record_result(minutes);
    let message = if minutes < 1 {
        Component::translatable("commands.setidletimeout.success.disabled", Vec::new())
    } else {
        Component::translatable("commands.setidletimeout.success", vec![text(minutes)])
    };
    ctx.reply(sender, message);
    CommandOutcome::Success
}

fn debug<C: CommandContext>(ctx: &mut C, sender: CommandSender, rest: &str) -> CommandOutcome {
    let help = "/debug start|stop|function <name>";
    let Some((action, after)) = next_argument(rest) else {
        return usage(ctx, sender, help);
    };
    match action {
        "function" => match next_argument(after) {
            Some((name, _)) => super::function_commands::debug_function(ctx, sender, name),
            None => usage(ctx, sender, help),
        },
        "start" => {
            if !ctx.start_profiler() {
                return fail(ctx, sender, "commands.debug.alreadyRunning", Vec::new());
            }
            ctx.record_result(0);
            ctx.reply(
                sender,
                Component::translatable("commands.debug.started", Vec::new()),
            );
            CommandOutcome::Success
        }
        "stop" => {
            let Some((seconds, ticks)) = ctx.stop_profiler() else {
                return fail(ctx, sender, "commands.debug.notRunning", Vec::new());
            };
            let rate = if seconds > 0.0 {
                ticks as f64 / seconds
            } else {
                0.0
            };
            ctx.record_result(rate.round() as i32);
            ctx.reply(
                sender,
                Component::translatable(
                    "commands.debug.stopped",
                    vec![
                        text(format!("{seconds:.2}")),
                        text(ticks),
                        text(format!("{rate:.2}")),
                    ],
                ),
            );
            CommandOutcome::Success
        }
        _ => usage(ctx, sender, help),
    }
}

pub(super) fn run<C: CommandContext>(
    ctx: &mut C,
    sender: CommandSender,
    label: &str,
    rest: &str,
) -> CommandOutcome {
    match label {
        "transfer" => transfer(ctx, sender, rest),
        "setidletimeout" => setidletimeout(ctx, sender, rest),
        _ => debug(ctx, sender, rest),
    }
}

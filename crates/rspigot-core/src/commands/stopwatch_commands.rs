use std::time::{SystemTime, UNIX_EPOCH};

use rspigot_domain::text::Component;

use super::dispatch::{CommandContext, CommandOutcome, CommandSender, fail, next_argument, usage};

fn text(value: impl ToString) -> Component {
    Component::text(value.to_string())
}

fn now_millis() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_millis() as i64)
}

fn normalised(id: &str) -> String {
    if id.contains(':') {
        id.to_owned()
    } else {
        format!("minecraft:{id}")
    }
}

pub(super) fn run<C: CommandContext>(
    ctx: &mut C,
    sender: CommandSender,
    rest: &str,
) -> CommandOutcome {
    let help = "/stopwatch (create|query|restart|remove) <id>";
    let Some((action, rest)) = next_argument(rest) else {
        return usage(ctx, sender, help);
    };
    let Some((raw, rest)) = next_argument(rest) else {
        return usage(ctx, sender, help);
    };
    let id = normalised(raw);
    let started = ctx.stopwatch_started(&id);
    match (action, started) {
        ("create", Some(_)) => fail(
            ctx,
            sender,
            "commands.stopwatch.already_exists",
            vec![text(&id)],
        ),
        ("create", None) => {
            ctx.set_stopwatch(&id, Some(now_millis()));
            ctx.reply(
                sender,
                Component::translatable("commands.stopwatch.create.success", vec![text(&id)]),
            );
            ctx.record_result(1);
            CommandOutcome::Success
        }
        ("query" | "restart" | "remove", None) => fail(
            ctx,
            sender,
            "commands.stopwatch.does_not_exist",
            vec![text(&id)],
        ),
        ("query", Some(started)) => {
            let scale = match next_argument(rest) {
                None => 1.0,
                Some((raw, _)) => match raw.parse::<f64>() {
                    Ok(value) => value,
                    Err(_) => return fail(ctx, sender, "parsing.double.invalid", vec![text(raw)]),
                },
            };
            let seconds = (now_millis() - started) as f64 / 1000.0;
            ctx.reply(
                sender,
                Component::translatable("commands.stopwatch.query", vec![text(&id), text(seconds)]),
            );
            ctx.record_result((seconds * scale) as i32);
            CommandOutcome::Success
        }
        ("restart", Some(_)) => {
            ctx.set_stopwatch(&id, Some(now_millis()));
            ctx.reply(
                sender,
                Component::translatable("commands.stopwatch.restart.success", vec![text(&id)]),
            );
            ctx.record_result(1);
            CommandOutcome::Success
        }
        ("remove", Some(_)) => {
            ctx.set_stopwatch(&id, None);
            ctx.reply(
                sender,
                Component::translatable("commands.stopwatch.remove.success", vec![text(&id)]),
            );
            ctx.record_result(1);
            CommandOutcome::Success
        }
        _ => usage(ctx, sender, help),
    }
}

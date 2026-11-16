use rspigot_domain::text::Component;
use rspigot_domain::tick_rate::TickRate;

use super::dispatch::{
    CommandContext, CommandOutcome, CommandSender, fail, next_argument, parse_time, usage,
};

fn text(value: impl ToString) -> Component {
    Component::text(value.to_string())
}

fn millis(nanos: u64) -> String {
    format!("{:.1}", nanos as f64 / 1.0e6)
}

fn reply<C: CommandContext>(ctx: &mut C, sender: CommandSender, key: &str, args: Vec<Component>) {
    ctx.reply(sender, Component::translatable(key, args));
}

fn query<C: CommandContext>(ctx: &mut C, sender: CommandSender) -> CommandOutcome {
    let state = ctx.tick_state();
    let average = millis(state.average_nanos());
    let rate = format!("{:.1}", state.rate());
    if state.is_sprinting() {
        reply(ctx, sender, "commands.tick.status.sprinting", Vec::new());
        reply(
            ctx,
            sender,
            "commands.tick.query.rate.sprinting",
            vec![text(&rate), text(&average)],
        );
    } else {
        let key = if state.is_frozen() {
            "commands.tick.status.frozen"
        } else if state.is_lagging() {
            "commands.tick.status.lagging"
        } else {
            "commands.tick.status.running"
        };
        reply(ctx, sender, key, Vec::new());
        reply(
            ctx,
            sender,
            "commands.tick.query.rate.running",
            vec![
                text(&rate),
                text(&average),
                text(millis(state.nanos_per_tick())),
            ],
        );
    }
    let sorted = state.sorted_samples();
    let at = |share: f64| {
        let index =
            ((sorted.len() as f64 * share).round() as usize).min(sorted.len().saturating_sub(1));
        sorted.get(index).copied().unwrap_or(0)
    };
    reply(
        ctx,
        sender,
        "commands.tick.query.percentiles",
        vec![
            text(millis(sorted.get(sorted.len() / 2).copied().unwrap_or(0))),
            text(millis(at(0.95))),
            text(millis(at(0.99))),
            text(sorted.len()),
        ],
    );
    ctx.record_result(state.rate().round() as i32);
    CommandOutcome::Success
}

fn rate<C: CommandContext>(ctx: &mut C, sender: CommandSender, rest: &str) -> CommandOutcome {
    let Some((raw, _)) = next_argument(rest) else {
        return usage(ctx, sender, "/tick rate <rate>");
    };
    let Ok(value) = raw.parse::<f32>() else {
        return fail(ctx, sender, "parsing.float.invalid", vec![text(raw)]);
    };
    if value < TickRate::MIN_RATE {
        return fail(
            ctx,
            sender,
            "argument.float.low",
            vec![text(TickRate::MIN_RATE), text(value)],
        );
    }
    if value > TickRate::MAX_RATE {
        return fail(
            ctx,
            sender,
            "argument.float.big",
            vec![text(TickRate::MAX_RATE), text(value)],
        );
    }
    ctx.change_tick_rate(|state| state.set_rate(value));
    ctx.record_result(value.round() as i32);
    reply(
        ctx,
        sender,
        "commands.tick.rate.success",
        vec![text(format!("{value:.1}"))],
    );
    CommandOutcome::Success
}

fn ticks<C: CommandContext>(
    ctx: &mut C,
    sender: CommandSender,
    raw: Option<&str>,
) -> Result<i32, CommandOutcome> {
    let Some(raw) = raw else {
        return Ok(1);
    };
    match parse_time(raw) {
        Some(value) if value >= 1 => Ok(i32::try_from(value).unwrap_or(i32::MAX)),
        Some(value) => Err(fail(
            ctx,
            sender,
            "argument.time.invalid_tick_count",
            vec![text(value)],
        )),
        None => Err(fail(ctx, sender, "argument.time.invalid_unit", Vec::new())),
    }
}

fn freeze<C: CommandContext>(ctx: &mut C, sender: CommandSender, frozen: bool) -> CommandOutcome {
    ctx.change_tick_rate(|state| {
        if frozen {
            state.stop_sprint();
            state.stop_step();
        }
        state.set_frozen(frozen);
    });
    ctx.record_result(i32::from(frozen));
    let key = if frozen {
        "commands.tick.status.frozen"
    } else {
        "commands.tick.status.running"
    };
    reply(ctx, sender, key, Vec::new());
    CommandOutcome::Success
}

fn step<C: CommandContext>(ctx: &mut C, sender: CommandSender, rest: &str) -> CommandOutcome {
    let first = next_argument(rest).map(|(raw, _)| raw);
    if first == Some("stop") {
        if !ctx.change_tick_rate(|state| state.stop_step()) {
            return fail(ctx, sender, "commands.tick.step.stop.fail", Vec::new());
        }
        ctx.record_result(1);
        reply(ctx, sender, "commands.tick.step.stop.success", Vec::new());
        return CommandOutcome::Success;
    }
    let advance = match ticks(ctx, sender, first) {
        Ok(value) => value,
        Err(outcome) => return outcome,
    };
    if !ctx.change_tick_rate(|state| state.step_if_frozen(advance)) {
        return fail(ctx, sender, "commands.tick.step.fail", Vec::new());
    }
    ctx.record_result(1);
    reply(
        ctx,
        sender,
        "commands.tick.step.success",
        vec![text(advance)],
    );
    CommandOutcome::Success
}

fn sprint<C: CommandContext>(ctx: &mut C, sender: CommandSender, rest: &str) -> CommandOutcome {
    let Some((first, _)) = next_argument(rest) else {
        return usage(ctx, sender, "/tick sprint <time>|stop");
    };
    if first == "stop" {
        if ctx.change_tick_rate(|state| state.stop_sprint()).is_none() {
            return fail(ctx, sender, "commands.tick.sprint.stop.fail", Vec::new());
        }
        ctx.record_result(1);
        reply(ctx, sender, "commands.tick.sprint.stop.success", Vec::new());
        return CommandOutcome::Success;
    }
    let time = match ticks(ctx, sender, Some(first)) {
        Ok(value) => value,
        Err(outcome) => return outcome,
    };
    if ctx.change_tick_rate(|state| state.request_sprint(i64::from(time))) {
        reply(ctx, sender, "commands.tick.sprint.stop.success", Vec::new());
    }
    reply(ctx, sender, "commands.tick.status.sprinting", Vec::new());
    ctx.record_result(1);
    CommandOutcome::Success
}

pub(super) fn run<C: CommandContext>(
    ctx: &mut C,
    sender: CommandSender,
    rest: &str,
) -> CommandOutcome {
    let Some((action, rest)) = next_argument(rest) else {
        return usage(ctx, sender, "/tick query|rate|freeze|unfreeze|step|sprint");
    };
    match action {
        "query" => query(ctx, sender),
        "rate" => rate(ctx, sender, rest),
        "freeze" => freeze(ctx, sender, true),
        "unfreeze" => freeze(ctx, sender, false),
        "step" => step(ctx, sender, rest),
        "sprint" => sprint(ctx, sender, rest),
        _ => usage(ctx, sender, "/tick query|rate|freeze|unfreeze|step|sprint"),
    }
}

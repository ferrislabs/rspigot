use rspigot_domain::Identifier;
use rspigot_domain::text::Component;
use rspigot_domain::world_clock::{self, ClockRuntime, MAX_RATE, MIN_RATE};

use super::announce::reply_and_broadcast;
use super::dispatch::{CommandContext, CommandOutcome, CommandSender, fail, usage};

const USAGE: &str = "/time <set|add|query|pause|resume|rate|of> ...";

fn text(value: impl ToString) -> Component {
    Component::text(value.to_string())
}

fn parse_ticks(input: &str, minimum: i64) -> Result<i64, Component> {
    let (digits, unit) = match input.chars().last() {
        Some('d') => (&input[..input.len() - 1], 24_000.0),
        Some('s') => (&input[..input.len() - 1], 20.0),
        Some('t') => (&input[..input.len() - 1], 1.0),
        _ => (input, 1.0),
    };
    let value: f32 = digits
        .parse()
        .map_err(|_| Component::translatable("argument.time.invalid_value", vec![text(input)]))?;
    let ticks = (f64::from(value) * unit).round() as i64;
    if ticks < minimum {
        return Err(Component::translatable(
            "argument.time.tick_count_too_low",
            vec![text(minimum), text(ticks)],
        ));
    }
    if ticks > i64::from(i32::MAX) {
        return Err(Component::translatable(
            "argument.integer.big",
            vec![text(i32::MAX), text(ticks)],
        ));
    }
    Ok(ticks)
}

fn default_clock<C: CommandContext>(ctx: &C) -> &'static str {
    match ctx.current_dimension() {
        Some(dimension) if dimension.to_string() == world_clock::THE_END => world_clock::THE_END,
        _ => world_clock::OVERWORLD,
    }
}

fn dimension_of(clock: &str) -> Option<Identifier> {
    Identifier::parse(match clock {
        world_clock::THE_END => "minecraft:the_end",
        _ => "minecraft:overworld",
    })
    .ok()
}

pub(super) fn run<C: CommandContext>(
    ctx: &mut C,
    sender: CommandSender,
    args: &[&str],
) -> CommandOutcome {
    match args.first().copied() {
        Some("of") => {
            let Some(raw) = args.get(1) else {
                return usage(ctx, sender, USAGE);
            };
            if !world_clock::clock_known(raw) {
                return fail(
                    ctx,
                    sender,
                    "argument.resource.not_found",
                    vec![text(raw), text("minecraft:world_clock")],
                );
            }
            let clock = world_clock::normalize(raw);
            let clock = world_clock::CLOCKS
                .into_iter()
                .find(|known| *known == clock)
                .unwrap_or(world_clock::OVERWORLD);
            if args.get(2) == Some(&"query") && args.get(3) == Some(&"gametime") {
                return usage(ctx, sender, USAGE);
            }
            on_clock(ctx, sender, clock, &args[2..])
        }
        _ => {
            let clock = default_clock(ctx);
            act(ctx, sender, clock, args)
        }
    }
}

fn on_clock<C: CommandContext>(
    ctx: &mut C,
    sender: CommandSender,
    clock: &'static str,
    args: &[&str],
) -> CommandOutcome {
    if clock == default_clock(ctx) {
        return act(ctx, sender, clock, args);
    }
    let Some(dimension) = dimension_of(clock) else {
        return usage(ctx, sender, USAGE);
    };
    let mut outcome = None;
    let ran = ctx.run_in_dimension(&dimension, true, |inner| {
        outcome = Some(act(inner, sender, clock, args));
    });
    match (ran, outcome) {
        (true, Some(outcome)) => outcome,
        _ => fail(
            ctx,
            sender,
            "commands.time.no_default_clock",
            vec![text(dimension)],
        ),
    }
}

fn act<C: CommandContext>(
    ctx: &mut C,
    sender: CommandSender,
    clock: &'static str,
    args: &[&str],
) -> CommandOutcome {
    match (args.first().copied(), args.get(1).copied()) {
        (Some("set"), Some(value)) => set(ctx, sender, clock, value),
        (Some("add"), Some(value)) => {
            let ticks = match parse_ticks(value, i64::from(i32::MIN)) {
                Ok(ticks) => ticks,
                Err(error) => return reject(ctx, sender, error),
            };
            let total = ctx.day_time() + ticks;
            ctx.skip_day_time(total);
            ctx.record_result(world_clock::wrap(total));
            reply_and_broadcast(
                ctx,
                sender,
                Component::translatable(
                    "commands.time.set.absolute",
                    vec![text(clock), text(total)],
                ),
            );
            CommandOutcome::Success
        }
        (Some("pause"), None) => pause(ctx, sender, clock, true),
        (Some("resume"), None) => pause(ctx, sender, clock, false),
        (Some("rate"), Some(value)) => rate(ctx, sender, clock, value),
        (Some("query"), Some(what)) => query(ctx, sender, clock, what, args.get(2).copied()),
        _ => usage(ctx, sender, USAGE),
    }
}

fn reject<C: CommandContext>(
    ctx: &mut C,
    sender: CommandSender,
    error: Component,
) -> CommandOutcome {
    ctx.reply(sender, error.color(rspigot_domain::text::NamedColor::Red));
    CommandOutcome::Failure
}

fn set<C: CommandContext>(
    ctx: &mut C,
    sender: CommandSender,
    clock: &'static str,
    value: &str,
) -> CommandOutcome {
    if value.contains(':') || value.chars().any(char::is_alphabetic) && !is_number(value) {
        return set_marker(ctx, sender, clock, value);
    }
    let ticks = match parse_ticks(value, 0) {
        Ok(ticks) => ticks,
        Err(error) => return reject(ctx, sender, error),
    };
    if ctx.day_time() == ticks {
        return fail(
            ctx,
            sender,
            "commands.time.set.already_at_time",
            vec![text(clock), text(ticks)],
        );
    }
    ctx.set_day_time(ticks);
    ctx.record_result(ticks as i32);
    reply_and_broadcast(
        ctx,
        sender,
        Component::translatable("commands.time.set.absolute", vec![text(clock), text(ticks)]),
    );
    CommandOutcome::Success
}

fn is_number(value: &str) -> bool {
    let body = value.strip_suffix(['d', 's', 't']).unwrap_or(value);
    body.parse::<f32>().is_ok()
}

fn set_marker<C: CommandContext>(
    ctx: &mut C,
    sender: CommandSender,
    clock: &'static str,
    marker_id: &str,
) -> CommandOutcome {
    let marker_name = world_clock::normalize(marker_id);
    let Some((timeline, marker)) = world_clock::marker_of(clock, marker_id) else {
        return fail(
            ctx,
            sender,
            "commands.time.no_time_marker_found",
            vec![text(clock), text(marker_name)],
        );
    };
    let total = ctx.day_time();
    let wait = timeline.ticks_until(total, marker.ticks);
    if wait == 0 {
        return fail(
            ctx,
            sender,
            "commands.time.set.already_at_time_marker",
            vec![text(clock), text(marker_name)],
        );
    }
    ctx.set_day_time(total + wait);
    ctx.record_result(world_clock::wrap(total + wait));
    reply_and_broadcast(
        ctx,
        sender,
        Component::translatable(
            "commands.time.set.time_marker",
            vec![text(clock), text(marker_name)],
        ),
    );
    CommandOutcome::Success
}

fn pause<C: CommandContext>(
    ctx: &mut C,
    sender: CommandSender,
    clock: &'static str,
    paused: bool,
) -> CommandOutcome {
    let state = ctx.clock_runtime();
    if state.paused == paused {
        let key = if paused {
            "commands.time.pause.already_paused"
        } else {
            "commands.time.pause.already_running"
        };
        return fail(ctx, sender, key, vec![text(clock)]);
    }
    ctx.set_clock_runtime(ClockRuntime { paused, ..state });
    ctx.record_result(1);
    let key = if paused {
        "commands.time.pause"
    } else {
        "commands.time.resume"
    };
    reply_and_broadcast(ctx, sender, Component::translatable(key, vec![text(clock)]));
    CommandOutcome::Success
}

fn rate<C: CommandContext>(
    ctx: &mut C,
    sender: CommandSender,
    clock: &'static str,
    value: &str,
) -> CommandOutcome {
    let Ok(rate) = value.parse::<f32>() else {
        return fail(ctx, sender, "parsing.float.invalid", vec![text(value)]);
    };
    if rate < MIN_RATE {
        return fail(
            ctx,
            sender,
            "argument.float.low",
            vec![text(MIN_RATE), text(rate)],
        );
    }
    if rate > MAX_RATE {
        return fail(
            ctx,
            sender,
            "argument.float.big",
            vec![text(MAX_RATE), text(rate)],
        );
    }
    let state = ctx.clock_runtime();
    if state.rate == rate {
        return fail(
            ctx,
            sender,
            "commands.time.rate.already_same",
            vec![text(clock), text(rate)],
        );
    }
    ctx.set_clock_runtime(ClockRuntime { rate, ..state });
    ctx.record_result(1);
    reply_and_broadcast(
        ctx,
        sender,
        Component::translatable("commands.time.rate", vec![text(clock), text(rate)]),
    );
    CommandOutcome::Success
}

fn query<C: CommandContext>(
    ctx: &mut C,
    sender: CommandSender,
    clock: &'static str,
    what: &str,
    repetition: Option<&str>,
) -> CommandOutcome {
    match what {
        "time" => {
            let total = ctx.day_time();
            ctx.record_result(world_clock::wrap(total));
            ctx.reply(
                sender,
                Component::translatable(
                    "commands.time.query.absolute",
                    vec![text(clock), text(total)],
                ),
            );
        }
        "gametime" => {
            let total = ctx.game_time();
            ctx.record_result(world_clock::wrap(total));
            ctx.reply(
                sender,
                Component::translatable("commands.time.query.gametime", vec![text(total)]),
            );
        }
        id => {
            let Some(timeline) = world_clock::timeline(id) else {
                return fail(
                    ctx,
                    sender,
                    "argument.resource.not_found",
                    vec![text(id), text("minecraft:timeline")],
                );
            };
            if timeline.clock != clock {
                return fail(
                    ctx,
                    sender,
                    "commands.time.wrong_timeline_for_clock",
                    vec![text(clock), text(timeline.id)],
                );
            }
            let total = ctx.day_time();
            let (key, value) = match repetition {
                Some("repetition") => (
                    "commands.time.query.timeline.repetitions",
                    timeline.period_count(total),
                ),
                None => (
                    "commands.time.query.timeline",
                    timeline.current_ticks(total),
                ),
                Some(_) => return usage(ctx, sender, USAGE),
            };
            ctx.record_result(world_clock::wrap(value));
            ctx.reply(
                sender,
                Component::translatable(key, vec![text(timeline.id), text(value)]),
            );
        }
    }
    CommandOutcome::Success
}

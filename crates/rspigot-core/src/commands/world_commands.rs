use rspigot_domain::text::Component;

use rspigot_domain::world_border::{MAX_CENTER, MAX_SIZE};

use super::dispatch::{
    CommandContext, CommandOutcome, CommandSender, fail, next_argument, parse_time, usage,
};

const MAX_FORCED_AREA: i64 = 256;
const COORDINATE_LIMIT: i32 = 30_000_000;

fn text(value: impl ToString) -> Component {
    Component::text(value.to_string())
}

fn column_coordinate(raw: &str, origin: f64) -> Result<i32, Component> {
    let (relative, number) = match raw.strip_prefix('~') {
        Some(rest) => (true, rest),
        None => (false, raw),
    };
    let value = if number.is_empty() && relative {
        0
    } else {
        number
            .parse::<i32>()
            .map_err(|_| Component::translatable("parsing.int.invalid", vec![text(number)]))?
    };
    Ok(if relative {
        origin.floor() as i32 + value
    } else {
        value
    })
}

fn column<'a, C: CommandContext>(
    ctx: &C,
    sender: CommandSender,
    input: &'a str,
) -> Result<((i32, i32), &'a str), Component> {
    let incomplete = || Component::translatable("argument.pos2d.incomplete", Vec::new());
    let (x, rest) = next_argument(input).ok_or_else(incomplete)?;
    let (z, rest) = next_argument(rest).ok_or_else(incomplete)?;
    let origin = ctx.sender_origin(sender);
    Ok((
        (
            column_coordinate(x, origin.x)?,
            column_coordinate(z, origin.z)?,
        ),
        rest,
    ))
}

fn chunk_label(x: i32, z: i32) -> Component {
    text(format!("[{x}, {z}]"))
}

fn dimension_label<C: CommandContext>(ctx: &C) -> Component {
    text(
        ctx.current_dimension()
            .map_or_else(|| "minecraft:overworld".to_owned(), |key| key.to_string()),
    )
}

fn forceload<C: CommandContext>(ctx: &mut C, sender: CommandSender, rest: &str) -> CommandOutcome {
    let help = "/forceload add|remove|query ...";
    let Some((action, rest)) = next_argument(rest) else {
        return usage(ctx, sender, help);
    };
    match action {
        "add" | "remove" => {
            if action == "remove" && next_argument(rest).map(|(raw, _)| raw) == Some("all") {
                return forceload_remove_all(ctx, sender);
            }
            forceload_change(ctx, sender, rest, action == "add")
        }
        "query" => forceload_query(ctx, sender, rest),
        _ => usage(ctx, sender, help),
    }
}

fn forceload_remove_all<C: CommandContext>(ctx: &mut C, sender: CommandSender) -> CommandOutcome {
    for (x, z) in ctx.forced_chunks() {
        ctx.set_chunk_forced(x, z, false);
    }
    ctx.record_result(1);
    let key = "commands.forceload.removed.all";
    ctx.reply(
        sender,
        Component::translatable(key, vec![dimension_label(ctx)]),
    );
    CommandOutcome::Success
}

fn forceload_change<C: CommandContext>(
    ctx: &mut C,
    sender: CommandSender,
    rest: &str,
    add: bool,
) -> CommandOutcome {
    let (from, rest) = match column(ctx, sender, rest) {
        Ok(found) => found,
        Err(error) => return super::server_commands::reply_err(ctx, sender, error),
    };
    let to = match next_argument(rest) {
        None => from,
        Some(_) => match column(ctx, sender, rest) {
            Ok((found, _)) => found,
            Err(error) => return super::server_commands::reply_err(ctx, sender, error),
        },
    };
    let low = (from.0.min(to.0), from.1.min(to.1));
    let high = (from.0.max(to.0), from.1.max(to.1));
    if low.0 <= -COORDINATE_LIMIT
        || low.1 <= -COORDINATE_LIMIT
        || high.0 >= COORDINATE_LIMIT
        || high.1 >= COORDINATE_LIMIT
    {
        return fail(ctx, sender, "argument.pos.outofworld", Vec::new());
    }
    let (min_x, min_z) = (low.0 >> 4, low.1 >> 4);
    let (max_x, max_z) = (high.0 >> 4, high.1 >> 4);
    let area = i64::from(max_x - min_x + 1) * i64::from(max_z - min_z + 1);
    if area > MAX_FORCED_AREA {
        return fail(
            ctx,
            sender,
            "commands.forceload.toobig",
            vec![text(MAX_FORCED_AREA), text(area)],
        );
    }
    let mut changed = 0;
    for x in min_x..=max_x {
        for z in min_z..=max_z {
            if ctx.set_chunk_forced(x, z, add) {
                changed += 1;
            }
        }
    }
    let verb = if add { "added" } else { "removed" };
    if changed == 0 {
        return fail(
            ctx,
            sender,
            &format!("commands.forceload.{verb}.failure"),
            Vec::new(),
        );
    }
    ctx.record_result(changed);
    let message = if changed == 1 {
        Component::translatable(
            format!("commands.forceload.{verb}.single"),
            vec![chunk_label(min_x, min_z), dimension_label(ctx)],
        )
    } else {
        Component::translatable(
            format!("commands.forceload.{verb}.multiple"),
            vec![
                text(changed),
                dimension_label(ctx),
                chunk_label(min_x, min_z),
                chunk_label(max_x, max_z),
            ],
        )
    };
    ctx.reply(sender, message);
    CommandOutcome::Success
}

fn forceload_query<C: CommandContext>(
    ctx: &mut C,
    sender: CommandSender,
    rest: &str,
) -> CommandOutcome {
    if next_argument(rest).is_none() {
        let chunks = ctx.forced_chunks();
        if chunks.is_empty() {
            return fail(
                ctx,
                sender,
                "commands.forceload.added.none",
                vec![dimension_label(ctx)],
            );
        }
        let list = chunks
            .iter()
            .map(|(x, z)| format!("[{x}, {z}]"))
            .collect::<Vec<_>>()
            .join(", ");
        ctx.record_result(i32::try_from(chunks.len()).unwrap_or(i32::MAX));
        let message = if chunks.len() == 1 {
            Component::translatable(
                "commands.forceload.list.single",
                vec![dimension_label(ctx), text(list)],
            )
        } else {
            Component::translatable(
                "commands.forceload.list.multiple",
                vec![text(chunks.len()), dimension_label(ctx), text(list)],
            )
        };
        ctx.reply(sender, message);
        return CommandOutcome::Success;
    }
    let ((x, z), _) = match column(ctx, sender, rest) {
        Ok(found) => found,
        Err(error) => return super::server_commands::reply_err(ctx, sender, error),
    };
    if x <= -COORDINATE_LIMIT
        || z <= -COORDINATE_LIMIT
        || x >= COORDINATE_LIMIT
        || z >= COORDINATE_LIMIT
    {
        return fail(ctx, sender, "argument.pos.outofworld", Vec::new());
    }
    let (chunk_x, chunk_z) = (x >> 4, z >> 4);
    let key = if ctx.is_chunk_forced(chunk_x, chunk_z) {
        "commands.forceload.query.success"
    } else {
        "commands.forceload.query.failure"
    };
    let arguments = vec![chunk_label(chunk_x, chunk_z), dimension_label(ctx)];
    if key.ends_with("failure") {
        return fail(ctx, sender, key, arguments);
    }
    ctx.record_result(1);
    ctx.reply(sender, Component::translatable(key, arguments));
    CommandOutcome::Success
}

fn number(raw: &str, min: f64, max: f64) -> Result<f64, Component> {
    let value: f64 = raw
        .parse()
        .map_err(|_| Component::translatable("parsing.double.invalid", vec![text(raw)]))?;
    if value < min {
        return Err(Component::translatable(
            "argument.double.low",
            vec![text(min), text(value)],
        ));
    }
    if value > max {
        return Err(Component::translatable(
            "argument.double.big",
            vec![text(max), text(value)],
        ));
    }
    Ok(value)
}

fn ticks_of(raw: &str) -> Result<i64, Component> {
    match parse_time(raw) {
        Some(value) => Ok(value),
        None => Err(Component::translatable(
            "argument.time.invalid_unit",
            Vec::new(),
        )),
    }
}

fn seconds(ticks: i64) -> String {
    format!("{:.2}", ticks as f64 / 20.0)
}

fn worldborder<C: CommandContext>(
    ctx: &mut C,
    sender: CommandSender,
    rest: &str,
) -> CommandOutcome {
    let help = "/worldborder add|set|center|damage|get|warning ...";
    let Some((action, rest)) = next_argument(rest) else {
        return usage(ctx, sender, help);
    };
    match action {
        "get" => {
            let size = ctx.world_border().size(ctx.world_game_time());
            ctx.record_result((size + 0.5).floor() as i32);
            ctx.reply(
                sender,
                Component::translatable(
                    "commands.worldborder.get",
                    vec![text(format!("{size:.0}"))],
                ),
            );
            CommandOutcome::Success
        }
        "set" | "add" => border_size(ctx, sender, rest, action == "add"),
        "center" => border_center(ctx, sender, rest),
        "damage" => border_damage(ctx, sender, rest),
        "warning" => border_warning(ctx, sender, rest),
        _ => usage(ctx, sender, help),
    }
}

fn border_size<C: CommandContext>(
    ctx: &mut C,
    sender: CommandSender,
    rest: &str,
    add: bool,
) -> CommandOutcome {
    let Some((raw_distance, rest)) = next_argument(rest) else {
        return usage(ctx, sender, "/worldborder set|add <distance> [<time>]");
    };
    let distance = match number(raw_distance, -MAX_SIZE, MAX_SIZE) {
        Ok(value) => value,
        Err(error) => return super::server_commands::reply_err(ctx, sender, error),
    };
    let time = match next_argument(rest) {
        None => 0,
        Some((raw, _)) => match ticks_of(raw) {
            Ok(value) => value,
            Err(error) => return super::server_commands::reply_err(ctx, sender, error),
        },
    };
    let now = ctx.world_game_time();
    let border = ctx.world_border();
    let current = border.size(now);
    let (target, ticks) = if add {
        (current + distance, border.lerp_time(now) + time)
    } else {
        (distance, time)
    };
    if target == current {
        return fail(
            ctx,
            sender,
            "commands.worldborder.set.failed.nochange",
            Vec::new(),
        );
    }
    if target < 1.0 {
        return fail(
            ctx,
            sender,
            "commands.worldborder.set.failed.small",
            Vec::new(),
        );
    }
    if target > MAX_SIZE {
        return fail(
            ctx,
            sender,
            "commands.worldborder.set.failed.big",
            vec![text(MAX_SIZE)],
        );
    }
    let formatted = format!("{target:.1}");
    ctx.change_world_border(|border, now| {
        if ticks > 0 {
            border.lerp_size(current, target, now, ticks);
        } else {
            border.set_size(target);
        }
    });
    ctx.record_result((target - current).round() as i32);
    let message = if ticks <= 0 {
        Component::translatable("commands.worldborder.set.immediate", vec![text(formatted)])
    } else {
        let key = if target <= current {
            "commands.worldborder.set.shrink"
        } else {
            "commands.worldborder.set.grow"
        };
        Component::translatable(key, vec![text(formatted), text(seconds(ticks))])
    };
    ctx.reply(sender, message);
    CommandOutcome::Success
}

fn border_center<C: CommandContext>(
    ctx: &mut C,
    sender: CommandSender,
    rest: &str,
) -> CommandOutcome {
    let incomplete = || Component::translatable("argument.pos2d.incomplete", Vec::new());
    let origin = ctx.sender_origin(sender);
    let coordinate = |raw: &str, base: f64| -> Result<f64, Component> {
        let (relative, body) = match raw.strip_prefix('~') {
            Some(body) => (true, body),
            None => (false, raw),
        };
        let value = if relative && body.is_empty() {
            0.0
        } else {
            body.parse::<f64>()
                .map_err(|_| Component::translatable("parsing.double.invalid", vec![text(body)]))?
        };
        Ok(if relative { base + value } else { value })
    };
    let parsed = next_argument(rest)
        .ok_or_else(incomplete)
        .and_then(|(x, after)| {
            let (z, _) = next_argument(after).ok_or_else(incomplete)?;
            Ok((coordinate(x, origin.x)?, coordinate(z, origin.z)?))
        });
    let (x, z) = match parsed {
        Ok(found) => found,
        Err(error) => return super::server_commands::reply_err(ctx, sender, error),
    };
    if ctx.world_border().center() == (x, z) {
        return fail(
            ctx,
            sender,
            "commands.worldborder.center.failed",
            Vec::new(),
        );
    }
    if x.abs() > MAX_CENTER || z.abs() > MAX_CENTER {
        return fail(
            ctx,
            sender,
            "commands.worldborder.set.failed.far",
            vec![text(MAX_CENTER)],
        );
    }
    ctx.change_world_border(|border, _| border.set_center(x, z));
    ctx.record_result(0);
    ctx.reply(
        sender,
        Component::translatable(
            "commands.worldborder.center.success",
            vec![text(format!("{x:.2}")), text(format!("{z:.2}"))],
        ),
    );
    CommandOutcome::Success
}

fn border_damage<C: CommandContext>(
    ctx: &mut C,
    sender: CommandSender,
    rest: &str,
) -> CommandOutcome {
    let help = "/worldborder damage amount|buffer <value>";
    let Some((kind, rest)) = next_argument(rest) else {
        return usage(ctx, sender, help);
    };
    let Some((raw, _)) = next_argument(rest) else {
        return usage(ctx, sender, help);
    };
    let value = match number(raw, 0.0, f64::from(f32::MAX)) {
        Ok(value) => f64::from(value as f32),
        Err(error) => return super::server_commands::reply_err(ctx, sender, error),
    };
    let border = ctx.world_border();
    let (current, key, failed) = match kind {
        "amount" => (
            border.damage_per_block(),
            "commands.worldborder.damage.amount.success",
            "commands.worldborder.damage.amount.failed",
        ),
        "buffer" => (
            border.safe_zone(),
            "commands.worldborder.damage.buffer.success",
            "commands.worldborder.damage.buffer.failed",
        ),
        _ => return usage(ctx, sender, help),
    };
    if current == value {
        return fail(ctx, sender, failed, Vec::new());
    }
    ctx.change_world_border(|border, _| {
        if kind == "amount" {
            border.set_damage_per_block(value);
        } else {
            border.set_safe_zone(value);
        }
    });
    ctx.record_result(value.round() as i32);
    ctx.reply(
        sender,
        Component::translatable(key, vec![text(format!("{value:.2}"))]),
    );
    CommandOutcome::Success
}

fn border_warning<C: CommandContext>(
    ctx: &mut C,
    sender: CommandSender,
    rest: &str,
) -> CommandOutcome {
    let help = "/worldborder warning distance|time <value>";
    let Some((kind, rest)) = next_argument(rest) else {
        return usage(ctx, sender, help);
    };
    let Some((raw, _)) = next_argument(rest) else {
        return usage(ctx, sender, help);
    };
    let border = ctx.world_border();
    match kind {
        "distance" => {
            let Ok(value) = raw.parse::<i32>() else {
                return fail(ctx, sender, "parsing.int.invalid", vec![text(raw)]);
            };
            if value < 0 {
                return fail(
                    ctx,
                    sender,
                    "argument.integer.low",
                    vec![text(0), text(value)],
                );
            }
            if border.warning_blocks() == value {
                return fail(
                    ctx,
                    sender,
                    "commands.worldborder.warning.distance.failed",
                    Vec::new(),
                );
            }
            ctx.change_world_border(|border, _| border.set_warning_blocks(value));
            ctx.record_result(value);
            ctx.reply(
                sender,
                Component::translatable(
                    "commands.worldborder.warning.distance.success",
                    vec![text(value)],
                ),
            );
            CommandOutcome::Success
        }
        "time" => {
            let ticks = match ticks_of(raw) {
                Ok(value) => i32::try_from(value).unwrap_or(i32::MAX),
                Err(error) => return super::server_commands::reply_err(ctx, sender, error),
            };
            if border.warning_time() == ticks {
                return fail(
                    ctx,
                    sender,
                    "commands.worldborder.warning.time.failed",
                    Vec::new(),
                );
            }
            ctx.change_world_border(|border, _| border.set_warning_time(ticks));
            ctx.record_result(ticks);
            ctx.reply(
                sender,
                Component::translatable(
                    "commands.worldborder.warning.time.success",
                    vec![text(seconds(i64::from(ticks)))],
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
        "worldborder" => worldborder(ctx, sender, rest),
        _ => forceload(ctx, sender, rest),
    }
}

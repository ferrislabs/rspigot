use rspigot_domain::text::Component;

use super::dispatch::{CommandContext, CommandOutcome, CommandSender, fail, next_argument, usage};

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

pub(super) fn run<C: CommandContext>(
    ctx: &mut C,
    sender: CommandSender,
    label: &str,
    rest: &str,
) -> CommandOutcome {
    match label {
        _ => forceload(ctx, sender, rest),
    }
}

use rspigot_domain::text::Component;

use rspigot_blocks::BlockKind;
use rspigot_domain::spread::{self, Bounds, Point};
use rspigot_domain::world_border::{MAX_CENTER, MAX_SIZE};
use rspigot_domain::{BlockPos, Vec3};

use super::blocks::pos_at;
use super::dispatch::{
    CommandContext, CommandOutcome, CommandSender, Target, fail, next_argument, parse_time, select,
    usage,
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

fn java_float(value: f32) -> String {
    if value.fract() == 0.0 {
        format!("{value:.1}")
    } else {
        value.to_string()
    }
}

fn spawn_height<C: CommandContext>(ctx: &C, x: f64, z: f64, max_height: i32) -> i32 {
    let (min_y, _) = ctx.world_height();
    let column = (x.floor() as i32, z.floor() as i32);
    let air = |y: i32| {
        ctx.block_state_at(BlockPos::new(column.0, y, column.1))
            .is_none_or(|state| state.is_air())
    };
    let mut y = max_height + 1;
    let mut above = air(y);
    y -= 1;
    let mut here = air(y);
    while y > min_y {
        y -= 1;
        let below = air(y);
        if !below && here && above {
            return y + 1;
        }
        above = here;
        here = below;
    }
    max_height + 1
}

fn safe_spot<C: CommandContext>(ctx: &C, point: Point, max_height: i32) -> bool {
    let y = spawn_height(ctx, point.x, point.z, max_height) - 1;
    if y >= max_height {
        return false;
    }
    let pos = BlockPos::new(point.x.floor() as i32, y, point.z.floor() as i32);
    ctx.block_state_at(pos).is_none_or(|state| {
        !state.flags().has_fluid()
            && !matches!(state.kind(), Some(BlockKind::Fire | BlockKind::SoulFire))
    })
}

fn splitmix(state: &mut u64) -> f64 {
    *state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
    let mut z = *state;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^= z >> 31;
    (z >> 11) as f64 / (1u64 << 53) as f64
}

fn spreadplayers<C: CommandContext>(
    ctx: &mut C,
    sender: CommandSender,
    rest: &str,
) -> CommandOutcome {
    let help = "/spreadplayers <center> <spreadDistance> <maxRange> [under <maxHeight>] <respectTeams> <targets>";
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
    let Some((raw_x, rest)) = next_argument(rest) else {
        return usage(ctx, sender, help);
    };
    let Some((raw_z, rest)) = next_argument(rest) else {
        return usage(ctx, sender, help);
    };
    let (center_x, center_z) = match (coordinate(raw_x, origin.x), coordinate(raw_z, origin.z)) {
        (Ok(x), Ok(z)) => (x as f32, z as f32),
        (Err(error), _) | (_, Err(error)) => {
            return super::server_commands::reply_err(ctx, sender, error);
        }
    };
    let Some((raw_spread, rest)) = next_argument(rest) else {
        return usage(ctx, sender, help);
    };
    let Some((raw_range, rest)) = next_argument(rest) else {
        return usage(ctx, sender, help);
    };
    let spread_distance = match number(raw_spread, 0.0, f64::from(f32::MAX)) {
        Ok(value) => value,
        Err(error) => return super::server_commands::reply_err(ctx, sender, error),
    };
    let max_range = match number(raw_range, 1.0, f64::from(f32::MAX)) {
        Ok(value) => value,
        Err(error) => return super::server_commands::reply_err(ctx, sender, error),
    };
    let (min_y, top) = ctx.world_height();
    let mut max_height = top;
    let mut rest = rest;
    if next_argument(rest).map(|(word, _)| word) == Some("under") {
        let Some((_, after)) = next_argument(rest) else {
            return usage(ctx, sender, help);
        };
        let Some((raw_height, after)) = next_argument(after) else {
            return usage(ctx, sender, help);
        };
        let Ok(height) = raw_height.parse::<i32>() else {
            return fail(ctx, sender, "parsing.int.invalid", vec![text(raw_height)]);
        };
        max_height = height;
        rest = after;
    }
    let Some((raw_teams, rest)) = next_argument(rest) else {
        return usage(ctx, sender, help);
    };
    let respect_teams = match raw_teams {
        "true" => true,
        "false" => false,
        _ => return fail(ctx, sender, "parsing.bool.invalid", vec![text(raw_teams)]),
    };
    let Some((selector, _)) = next_argument(rest) else {
        return usage(ctx, sender, help);
    };
    let targets = match select(ctx, sender, Some(selector)) {
        Ok(found) => found,
        Err(outcome) => return outcome,
    };
    if max_height < min_y {
        return fail(
            ctx,
            sender,
            "commands.spreadplayers.failed.invalid.height",
            vec![text(max_height), text(min_y)],
        );
    }
    let groups: Vec<Option<String>> = targets
        .iter()
        .map(|target| match target {
            Target::Player(_) if respect_teams => {
                ctx.score_holder_name(*target).and_then(|holder| {
                    ctx.scoreboard()
                        .team_of(&holder)
                        .map(|team| team.name.clone())
                })
            }
            _ => None,
        })
        .collect();
    let mut teams: Vec<Option<String>> = Vec::new();
    for group in &groups {
        if !teams.contains(group) {
            teams.push(group.clone());
        }
    }
    let count = if respect_teams {
        teams.len()
    } else {
        targets.len()
    };
    let mut seed = ctx.random_seed();
    let bounds = Bounds::around(f64::from(center_x), f64::from(center_z), max_range);
    let placed = spread::spread(
        count,
        spread_distance,
        bounds,
        || splitmix(&mut seed),
        |point| safe_spot(ctx, point, max_height),
    );
    let points = match placed {
        Ok(points) => points,
        Err(failure) => {
            let key = if respect_teams {
                "commands.spreadplayers.failed.teams"
            } else {
                "commands.spreadplayers.failed.entities"
            };
            return fail(
                ctx,
                sender,
                key,
                vec![
                    text(count),
                    text(java_float(center_x)),
                    text(java_float(center_z)),
                    text(format!("{:.2}", failure.smallest_distance)),
                ],
            );
        }
    };
    let mut total = 0.0;
    for (index, target) in targets.iter().enumerate() {
        let slot = if respect_teams {
            teams
                .iter()
                .position(|team| *team == groups[index])
                .unwrap_or(0)
        } else {
            index
        };
        let point = points[slot];
        let y = spawn_height(ctx, point.x, point.z, max_height);
        let spot = Vec3::new(point.x.floor() + 0.5, f64::from(y), point.z.floor() + 0.5);
        ctx.teleport_target(*target, spot);
        total += spread::nearest_distance(&points, slot);
    }
    let average = if targets.len() < 2 {
        0.0
    } else {
        total / targets.len() as f64
    };
    let key = if respect_teams {
        "commands.spreadplayers.success.teams"
    } else {
        "commands.spreadplayers.success.entities"
    };
    ctx.record_result(i32::try_from(points.len()).unwrap_or(i32::MAX));
    ctx.reply(
        sender,
        Component::translatable(
            key,
            vec![
                text(points.len()),
                text(java_float(center_x)),
                text(java_float(center_z)),
                text(format!("{average:.2}")),
            ],
        ),
    );
    CommandOutcome::Success
}

pub enum FillBiomeProblem {
    NotLoaded,
    NoChange,
}

fn biome_id<C: CommandContext>(
    ctx: &mut C,
    sender: CommandSender,
    raw: &str,
) -> Result<u16, CommandOutcome> {
    let name = if raw.contains(':') {
        raw.to_owned()
    } else {
        format!("minecraft:{raw}")
    };
    rspigot_data::registries::biome_id(&name).ok_or_else(|| {
        fail(
            ctx,
            sender,
            "argument.resource.not_found",
            vec![text(&name), text("minecraft:worldgen/biome")],
        )
    })
}

fn fillbiome<C: CommandContext>(ctx: &mut C, sender: CommandSender, rest: &str) -> CommandOutcome {
    let help = "/fillbiome <from> <to> <biome> [replace <filter>]";
    let (from, rest) = match pos_at(ctx, sender, rest) {
        Ok(found) => found,
        Err(error) => return super::server_commands::reply_err(ctx, sender, error),
    };
    let (to, rest) = match pos_at(ctx, sender, rest) {
        Ok(found) => found,
        Err(error) => return super::server_commands::reply_err(ctx, sender, error),
    };
    let Some((raw_biome, rest)) = next_argument(rest) else {
        return usage(ctx, sender, help);
    };
    let biome = match biome_id(ctx, sender, raw_biome) {
        Ok(found) => found,
        Err(outcome) => return outcome,
    };
    let replaceable: Option<Vec<u16>> = match next_argument(rest) {
        None => None,
        Some(("replace", after)) => {
            let Some((raw, _)) = next_argument(after) else {
                return usage(ctx, sender, help);
            };
            match raw.strip_prefix('#') {
                Some(tag) => {
                    let name = if tag.contains(':') {
                        tag.to_owned()
                    } else {
                        format!("minecraft:{tag}")
                    };
                    let Some(members) = rspigot_data::tags::tag("minecraft:worldgen/biome", &name)
                    else {
                        return fail(
                            ctx,
                            sender,
                            "argument.resource_tag.not_found",
                            vec![text(&name), text("minecraft:worldgen/biome")],
                        );
                    };
                    Some(
                        members
                            .iter()
                            .filter_map(|id| u16::try_from(*id).ok())
                            .collect(),
                    )
                }
                None => match biome_id(ctx, sender, raw) {
                    Ok(found) => Some(vec![found]),
                    Err(outcome) => return outcome,
                },
            }
        }
        Some(_) => return usage(ctx, sender, help),
    };
    let quantize = |pos: BlockPos| BlockPos::new(pos.x & !3, pos.y & !3, pos.z & !3);
    let (from, to) = (quantize(from), quantize(to));
    let min = BlockPos::new(from.x.min(to.x), from.y.min(to.y), from.z.min(to.z));
    let max = BlockPos::new(from.x.max(to.x), from.y.max(to.y), from.z.max(to.z));
    let volume =
        i64::from(max.x - min.x + 1) * i64::from(max.y - min.y + 1) * i64::from(max.z - min.z + 1);
    let limit = ctx
        .game_rule_text("max_block_modifications")
        .and_then(|value| value.parse::<i64>().ok())
        .unwrap_or(32_768);
    if volume > limit {
        return fail(
            ctx,
            sender,
            "commands.fillbiome.toobig",
            vec![text(limit), text(volume)],
        );
    }
    match ctx.fill_biome(min, max, biome, replaceable.as_deref()) {
        Ok(changed) => {
            ctx.record_result(changed);
            ctx.reply(
                sender,
                Component::translatable(
                    "commands.fillbiome.success.count",
                    vec![
                        text(changed),
                        text(min.x),
                        text(min.y),
                        text(min.z),
                        text(max.x),
                        text(max.y),
                        text(max.z),
                    ],
                ),
            );
            CommandOutcome::Success
        }
        Err(FillBiomeProblem::NotLoaded) => fail(ctx, sender, "argument.pos.unloaded", Vec::new()),
        Err(FillBiomeProblem::NoChange) => {
            fail(ctx, sender, "commands.fillbiome.no_changes", Vec::new())
        }
    }
}

pub(super) fn run<C: CommandContext>(
    ctx: &mut C,
    sender: CommandSender,
    label: &str,
    rest: &str,
) -> CommandOutcome {
    match label {
        "fillbiome" => fillbiome(ctx, sender, rest),
        "worldborder" => worldborder(ctx, sender, rest),
        "spreadplayers" => spreadplayers(ctx, sender, rest),
        _ => forceload(ctx, sender, rest),
    }
}

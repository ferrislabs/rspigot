use rspigot_blocks::StateId;
use rspigot_domain::text::{Component, NamedColor};
use rspigot_domain::{BlockPos, Identifier};
use rspigot_nbt::Compound;

use super::announce::reply_and_broadcast;
use super::args::{BlockPredicate, parse_block_state};
use super::dispatch::{
    CommandContext, CommandOutcome, CommandSender, fail, next_argument, parse_vec3,
};

const MAX_BLOCKS: i64 = 32_768;

fn block_limit<C: CommandContext>(ctx: &C) -> i64 {
    ctx.game_rule_text("max_block_modifications")
        .and_then(|value| value.parse::<i64>().ok())
        .unwrap_or(MAX_BLOCKS)
}

fn reply_error<C: CommandContext>(
    ctx: &mut C,
    sender: CommandSender,
    error: Component,
) -> CommandOutcome {
    ctx.reply(sender, error.color(NamedColor::Red));
    CommandOutcome::Failure
}

pub(super) fn pos_at<'a, C: CommandContext>(
    ctx: &C,
    sender: CommandSender,
    input: &'a str,
) -> Result<(BlockPos, &'a str), Component> {
    let incomplete = || Component::translatable("argument.pos3d.incomplete", Vec::new());
    let (x, rest) = next_argument(input).ok_or_else(incomplete)?;
    let (y, rest) = next_argument(rest).ok_or_else(incomplete)?;
    let (z, rest) = next_argument(rest).ok_or_else(incomplete)?;
    let position = parse_vec3(ctx, sender, [x, y, z])?;
    Ok((
        BlockPos::new(
            position.x.floor() as i32,
            position.y.floor() as i32,
            position.z.floor() as i32,
        ),
        rest,
    ))
}

fn checked<C: CommandContext>(ctx: &C, pos: BlockPos) -> Result<BlockPos, Component> {
    if !ctx.in_world_height(pos.y) || pos.x.abs() > 30_000_000 || pos.z.abs() > 30_000_000 {
        return Err(Component::translatable(
            "argument.pos.outofworld",
            Vec::new(),
        ));
    }
    if !ctx.is_position_loaded(pos) {
        return Err(Component::translatable("argument.pos.unloaded", Vec::new()));
    }
    Ok(pos)
}

fn number(value: i32) -> Component {
    Component::text(value.to_string())
}

fn bounds(a: BlockPos, b: BlockPos) -> (BlockPos, BlockPos) {
    (
        BlockPos::new(a.x.min(b.x), a.y.min(b.y), a.z.min(b.z)),
        BlockPos::new(a.x.max(b.x), a.y.max(b.y), a.z.max(b.z)),
    )
}

fn volume(min: BlockPos, max: BlockPos) -> i64 {
    i64::from(max.x - min.x + 1) * i64::from(max.y - min.y + 1) * i64::from(max.z - min.z + 1)
}

fn place<C: CommandContext>(
    ctx: &mut C,
    pos: BlockPos,
    state: StateId,
    nbt: Option<&Compound>,
    strict: bool,
) -> bool {
    if strict {
        ctx.place_block_strict(pos, state, nbt)
    } else {
        ctx.place_block(pos, state, nbt)
    }
}

pub(super) fn setblock<C: CommandContext>(
    ctx: &mut C,
    sender: CommandSender,
    rest: &str,
) -> CommandOutcome {
    let (pos, rest) = match pos_at(ctx, sender, rest) {
        Ok(found) => found,
        Err(error) => return reply_error(ctx, sender, error),
    };
    let Some((block, rest)) = next_argument(rest) else {
        return super::dispatch::usage(
            ctx,
            sender,
            "/setblock <pos> <block> [destroy|keep|replace]",
        );
    };
    let (state, nbt) = match parse_block_state(block) {
        Ok(found) => found,
        Err(error) => return reply_error(ctx, sender, error),
    };
    let mode = next_argument(rest)
        .map(|(mode, _)| mode)
        .unwrap_or("replace");
    if !matches!(mode, "replace" | "destroy" | "keep" | "strict") {
        return super::dispatch::usage(
            ctx,
            sender,
            "/setblock <pos> <block> [destroy|keep|replace|strict]",
        );
    }
    let strict = mode == "strict";
    let pos = match checked(ctx, pos) {
        Ok(pos) => pos,
        Err(error) => return reply_error(ctx, sender, error),
    };
    if mode == "keep" && ctx.block_state_at(pos).is_some_and(|old| !old.is_air()) {
        return fail(ctx, sender, "commands.setblock.failed", Vec::new());
    }
    if mode == "destroy" {
        ctx.destroy_block(pos);
    }
    let unchanged = ctx.block_state_at(pos) == Some(state) && nbt.is_none();
    if unchanged || !place(ctx, pos, state, nbt.as_ref(), strict) {
        return fail(ctx, sender, "commands.setblock.failed", Vec::new());
    }
    ctx.record_result(1);
    reply_and_broadcast(
        ctx,
        sender,
        Component::translatable(
            "commands.setblock.success",
            vec![number(pos.x), number(pos.y), number(pos.z)],
        ),
    );
    CommandOutcome::Success
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Shape {
    Replace,
    Keep,
    Outline,
    Hollow,
    Destroy,
}

fn shape_of(word: &str) -> Option<Shape> {
    Some(match word {
        "keep" => Shape::Keep,
        "outline" => Shape::Outline,
        "hollow" => Shape::Hollow,
        "destroy" => Shape::Destroy,
        _ => return None,
    })
}

fn on_shell(min: BlockPos, max: BlockPos, p: BlockPos) -> bool {
    p.x == min.x || p.x == max.x || p.y == min.y || p.y == max.y || p.z == min.z || p.z == max.z
}

pub(super) fn fill<C: CommandContext>(
    ctx: &mut C,
    sender: CommandSender,
    rest: &str,
) -> CommandOutcome {
    let usage = "/fill <from> <to> <block> [destroy|hollow|keep|outline|replace|strict]";
    let (from, rest) = match pos_at(ctx, sender, rest) {
        Ok(found) => found,
        Err(error) => return reply_error(ctx, sender, error),
    };
    let (to, rest) = match pos_at(ctx, sender, rest) {
        Ok(found) => found,
        Err(error) => return reply_error(ctx, sender, error),
    };
    let Some((block, rest)) = next_argument(rest) else {
        return super::dispatch::usage(ctx, sender, usage);
    };
    let (state, nbt) = match parse_block_state(block) {
        Ok(found) => found,
        Err(error) => return reply_error(ctx, sender, error),
    };
    let mut shape = Shape::Replace;
    let mut strict = false;
    let mut filter = None;
    match next_argument(rest) {
        None => {}
        Some(("replace", after)) => {
            if let Some((text, tail)) = next_argument(after) {
                match BlockPredicate::parse(text) {
                    Ok(parsed) => filter = Some(parsed),
                    Err(error) => return reply_error(ctx, sender, error),
                }
                match next_argument(tail) {
                    None => {}
                    Some(("strict", _)) => strict = true,
                    Some((word, _)) => match shape_of(word) {
                        Some(found) if found != Shape::Keep => shape = found,
                        _ => return super::dispatch::usage(ctx, sender, usage),
                    },
                }
            }
        }
        Some(("strict", _)) => strict = true,
        Some((word, _)) => match shape_of(word) {
            Some(found) => shape = found,
            None => return super::dispatch::usage(ctx, sender, usage),
        },
    }
    let (min, max) = bounds(from, to);
    let total = volume(min, max);
    let limit = block_limit(ctx);
    if total > limit {
        return fail(
            ctx,
            sender,
            "commands.fill.toobig",
            vec![
                Component::text(limit.to_string()),
                Component::text(total.to_string()),
            ],
        );
    }
    for corner in [min, max] {
        if let Err(error) = checked(ctx, corner) {
            return reply_error(ctx, sender, error);
        }
    }
    let mut changed = 0;
    for y in min.y..=max.y {
        for z in min.z..=max.z {
            for x in min.x..=max.x {
                let pos = BlockPos::new(x, y, z);
                let Some(old) = ctx.block_state_at(pos) else {
                    continue;
                };
                if let Some(filter) = &filter {
                    let entity = filter
                        .needs_block_entity()
                        .then(|| ctx.block_entity_data(pos))
                        .flatten();
                    if !filter.matches(old, entity.as_ref()) {
                        continue;
                    }
                }
                let shell = on_shell(min, max, pos);
                let target = match shape {
                    Shape::Replace => state,
                    Shape::Keep => {
                        if !old.is_air() {
                            continue;
                        }
                        state
                    }
                    Shape::Outline => {
                        if !shell {
                            continue;
                        }
                        state
                    }
                    Shape::Hollow => {
                        if shell {
                            state
                        } else {
                            StateId::AIR
                        }
                    }
                    Shape::Destroy => {
                        ctx.destroy_block(pos);
                        state
                    }
                };
                let wants_nbt = shell || shape != Shape::Hollow;
                let data = if wants_nbt && target == state {
                    nbt.as_ref()
                } else {
                    None
                };
                if place(ctx, pos, target, data, strict) {
                    changed += 1;
                }
            }
        }
    }
    if changed == 0 {
        return fail(ctx, sender, "commands.fill.failed", Vec::new());
    }
    ctx.record_result(changed);
    reply_and_broadcast(
        ctx,
        sender,
        Component::translatable("commands.fill.success", vec![number(changed)]),
    );
    CommandOutcome::Success
}

struct Copied {
    offset: (i32, i32, i32),
    state: StateId,
    data: Option<Compound>,
}

fn in_dimension<C: CommandContext, R>(
    ctx: &mut C,
    dimension: Option<&Identifier>,
    run: impl FnOnce(&mut C) -> R,
) -> Option<R> {
    let Some(dimension) = dimension else {
        return Some(run(ctx));
    };
    let mut result = None;
    ctx.run_in_dimension(dimension, true, |inner| result = Some(run(inner)));
    result
}

fn dimension_argument(token: &str) -> Result<Identifier, Component> {
    Identifier::parse(token).map_err(|_| {
        Component::translatable("argument.dimension.invalid", vec![Component::text(token)])
    })
}

pub(super) fn clone<C: CommandContext>(
    ctx: &mut C,
    sender: CommandSender,
    rest: &str,
) -> CommandOutcome {
    let usage = "/clone [from <dimension>] <begin> <end> [to <dimension>] <destination> [strict] [replace|masked|filtered <filter>] [force|move|normal]";
    let mut rest = rest;
    let mut source_dimension = None;
    if let Some(("from", after)) = next_argument(rest) {
        let Some((token, after)) = next_argument(after) else {
            return super::dispatch::usage(ctx, sender, usage);
        };
        match dimension_argument(token) {
            Ok(found) => source_dimension = Some(found),
            Err(error) => return reply_error(ctx, sender, error),
        }
        rest = after;
    }
    let (begin, after) = match pos_at(ctx, sender, rest) {
        Ok(found) => found,
        Err(error) => return reply_error(ctx, sender, error),
    };
    let (end, after) = match pos_at(ctx, sender, after) {
        Ok(found) => found,
        Err(error) => return reply_error(ctx, sender, error),
    };
    let mut target_dimension = None;
    let mut rest = after;
    if let Some(("to", after)) = next_argument(rest) {
        let Some((token, after)) = next_argument(after) else {
            return super::dispatch::usage(ctx, sender, usage);
        };
        match dimension_argument(token) {
            Ok(found) => target_dimension = Some(found),
            Err(error) => return reply_error(ctx, sender, error),
        }
        rest = after;
    }
    let (destination, mut rest) = match pos_at(ctx, sender, rest) {
        Ok(found) => found,
        Err(error) => return reply_error(ctx, sender, error),
    };
    let mut masked = false;
    let mut filter: Option<BlockPredicate> = None;
    let mut force = false;
    let mut moving = false;
    let mut strict = false;
    while let Some((word, after)) = next_argument(rest) {
        rest = after;
        match word {
            "replace" | "normal" => {}
            "strict" => strict = true,
            "masked" => masked = true,
            "filtered" => {
                let Some((text, after)) = next_argument(rest) else {
                    return super::dispatch::usage(ctx, sender, usage);
                };
                match BlockPredicate::parse(text) {
                    Ok(parsed) => filter = Some(parsed),
                    Err(error) => return reply_error(ctx, sender, error),
                }
                rest = after;
            }
            "force" => force = true,
            "move" => moving = true,
            _ => return super::dispatch::usage(ctx, sender, usage),
        }
    }
    let (min, max) = bounds(begin, end);
    let total = volume(min, max);
    let limit = block_limit(ctx);
    if total > limit {
        return fail(
            ctx,
            sender,
            "commands.clone.toobig",
            vec![
                Component::text(limit.to_string()),
                Component::text(total.to_string()),
            ],
        );
    }
    let delta = (
        destination.x - min.x,
        destination.y - min.y,
        destination.z - min.z,
    );
    let far = BlockPos::new(max.x + delta.0, max.y + delta.1, max.z + delta.2);
    let same_dimension = match (&source_dimension, &target_dimension) {
        (None, None) => true,
        (Some(a), Some(b)) => a == b,
        (Some(a), None) => ctx.current_dimension().as_ref() == Some(a),
        (None, Some(b)) => ctx.current_dimension().as_ref() == Some(b),
    };
    let overlaps = same_dimension
        && destination.x <= max.x
        && far.x >= min.x
        && destination.y <= max.y
        && far.y >= min.y
        && destination.z <= max.z
        && far.z >= min.z;
    if overlaps && !force && !moving {
        return fail(ctx, sender, "commands.clone.overlap", Vec::new());
    }
    let source_ok = in_dimension(ctx, source_dimension.as_ref(), |inner| {
        [min, max]
            .into_iter()
            .try_for_each(|corner| checked(inner, corner).map(|_| ()))
    });
    match source_ok {
        Some(Ok(())) => {}
        Some(Err(error)) => return reply_error(ctx, sender, error),
        None => return fail(ctx, sender, "argument.dimension.invalid", Vec::new()),
    }
    let target_ok = in_dimension(ctx, target_dimension.as_ref(), |inner| {
        [destination, far]
            .into_iter()
            .try_for_each(|corner| checked(inner, corner).map(|_| ()))
    });
    match target_ok {
        Some(Ok(())) => {}
        Some(Err(error)) => return reply_error(ctx, sender, error),
        None => return fail(ctx, sender, "argument.dimension.invalid", Vec::new()),
    }
    let copied = in_dimension(ctx, source_dimension.as_ref(), |inner| {
        let mut copied = Vec::new();
        for y in min.y..=max.y {
            for z in min.z..=max.z {
                for x in min.x..=max.x {
                    let pos = BlockPos::new(x, y, z);
                    let Some(state) = inner.block_state_at(pos) else {
                        continue;
                    };
                    if masked && state.is_air() {
                        continue;
                    }
                    let data = inner.block_entity_data(pos);
                    if let Some(filter) = &filter
                        && !filter.matches(state, data.as_ref())
                    {
                        continue;
                    }
                    copied.push(Copied {
                        offset: (x - min.x, y - min.y, z - min.z),
                        state,
                        data,
                    });
                }
            }
        }
        if moving {
            for block in &copied {
                let pos = BlockPos::new(
                    min.x + block.offset.0,
                    min.y + block.offset.1,
                    min.z + block.offset.2,
                );
                place(inner, pos, StateId::AIR, None, strict);
            }
        }
        copied
    })
    .unwrap_or_default();
    let count = in_dimension(ctx, target_dimension.as_ref(), |inner| {
        let mut count = 0;
        for block in &copied {
            let pos = BlockPos::new(
                destination.x + block.offset.0,
                destination.y + block.offset.1,
                destination.z + block.offset.2,
            );
            if place(inner, pos, block.state, block.data.as_ref(), strict) {
                count += 1;
            }
        }
        count
    })
    .unwrap_or(0);
    if count == 0 {
        return fail(ctx, sender, "commands.clone.failed", Vec::new());
    }
    ctx.record_result(count);
    reply_and_broadcast(
        ctx,
        sender,
        Component::translatable("commands.clone.success", vec![number(count)]),
    );
    CommandOutcome::Success
}

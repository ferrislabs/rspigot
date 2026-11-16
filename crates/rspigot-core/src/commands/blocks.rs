use rspigot_blocks::StateId;
use rspigot_domain::BlockPos;
use rspigot_domain::text::{Component, NamedColor};
use rspigot_nbt::Compound;

use super::args::{BlockPredicate, parse_block_state};
use super::dispatch::{
    CommandContext, CommandOutcome, CommandSender, fail, next_argument, parse_vec3,
};

const MAX_BLOCKS: i64 = 32_768;

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
    if !matches!(mode, "replace" | "destroy" | "keep") {
        return super::dispatch::usage(
            ctx,
            sender,
            "/setblock <pos> <block> [destroy|keep|replace]",
        );
    }
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
    if unchanged || !ctx.place_block(pos, state, nbt.as_ref()) {
        return fail(ctx, sender, "commands.setblock.failed", Vec::new());
    }
    ctx.record_result(1);
    ctx.reply(
        sender,
        Component::translatable(
            "commands.setblock.success",
            vec![number(pos.x), number(pos.y), number(pos.z)],
        ),
    );
    CommandOutcome::Success
}

#[derive(PartialEq)]
enum FillMode {
    Replace(Option<BlockPredicate>),
    Keep,
    Outline,
    Hollow,
    Destroy,
}

fn on_shell(min: BlockPos, max: BlockPos, p: BlockPos) -> bool {
    p.x == min.x || p.x == max.x || p.y == min.y || p.y == max.y || p.z == min.z || p.z == max.z
}

pub(super) fn fill<C: CommandContext>(
    ctx: &mut C,
    sender: CommandSender,
    rest: &str,
) -> CommandOutcome {
    let usage = "/fill <from> <to> <block> [destroy|hollow|keep|outline|replace]";
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
    let mode = match next_argument(rest) {
        None => FillMode::Replace(None),
        Some(("replace", after)) => match next_argument(after) {
            None => FillMode::Replace(None),
            Some((filter, _)) => match BlockPredicate::parse(filter) {
                Ok(filter) => FillMode::Replace(Some(filter)),
                Err(error) => return reply_error(ctx, sender, error),
            },
        },
        Some(("keep", _)) => FillMode::Keep,
        Some(("outline", _)) => FillMode::Outline,
        Some(("hollow", _)) => FillMode::Hollow,
        Some(("destroy", _)) => FillMode::Destroy,
        Some(_) => return super::dispatch::usage(ctx, sender, usage),
    };
    let (min, max) = bounds(from, to);
    let total = volume(min, max);
    if total > MAX_BLOCKS {
        return fail(
            ctx,
            sender,
            "commands.fill.toobig",
            vec![
                Component::text(MAX_BLOCKS.to_string()),
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
                let shell = on_shell(min, max, pos);
                let target = match &mode {
                    FillMode::Replace(None) => state,
                    FillMode::Replace(Some(filter)) => {
                        let entity = filter
                            .needs_block_entity()
                            .then(|| ctx.block_entity_data(pos))
                            .flatten();
                        if !filter.matches(old, entity.as_ref()) {
                            continue;
                        }
                        state
                    }
                    FillMode::Keep => {
                        if !old.is_air() {
                            continue;
                        }
                        state
                    }
                    FillMode::Outline => {
                        if !shell {
                            continue;
                        }
                        state
                    }
                    FillMode::Hollow => {
                        if shell {
                            state
                        } else {
                            StateId::AIR
                        }
                    }
                    FillMode::Destroy => {
                        ctx.destroy_block(pos);
                        state
                    }
                };
                let wants_nbt = shell || mode != FillMode::Hollow;
                let data = if wants_nbt && target == state {
                    nbt.as_ref()
                } else {
                    None
                };
                if ctx.place_block(pos, target, data) {
                    changed += 1;
                }
            }
        }
    }
    if changed == 0 {
        return fail(ctx, sender, "commands.fill.failed", Vec::new());
    }
    ctx.record_result(changed);
    ctx.reply(
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

pub(super) fn clone<C: CommandContext>(
    ctx: &mut C,
    sender: CommandSender,
    rest: &str,
) -> CommandOutcome {
    let usage =
        "/clone <begin> <end> <destination> [replace|masked|filtered <filter>] [force|move|normal]";
    let (begin, rest) = match pos_at(ctx, sender, rest) {
        Ok(found) => found,
        Err(error) => return reply_error(ctx, sender, error),
    };
    let (end, rest) = match pos_at(ctx, sender, rest) {
        Ok(found) => found,
        Err(error) => return reply_error(ctx, sender, error),
    };
    let (destination, mut rest) = match pos_at(ctx, sender, rest) {
        Ok(found) => found,
        Err(error) => return reply_error(ctx, sender, error),
    };
    let mut masked = false;
    let mut filter: Option<BlockPredicate> = None;
    let mut force = false;
    let mut moving = false;
    while let Some((word, after)) = next_argument(rest) {
        rest = after;
        match word {
            "replace" | "normal" => {}
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
    if total > MAX_BLOCKS {
        return fail(
            ctx,
            sender,
            "commands.clone.toobig",
            vec![
                Component::text(MAX_BLOCKS.to_string()),
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
    let destination_max = far;
    let overlaps = destination.x <= max.x
        && destination_max.x >= min.x
        && destination.y <= max.y
        && destination_max.y >= min.y
        && destination.z <= max.z
        && destination_max.z >= min.z;
    if overlaps && !force && !moving {
        return fail(ctx, sender, "commands.clone.overlap", Vec::new());
    }
    for corner in [min, max, destination, far] {
        if let Err(error) = checked(ctx, corner) {
            return reply_error(ctx, sender, error);
        }
    }
    let mut copied = Vec::new();
    for y in min.y..=max.y {
        for z in min.z..=max.z {
            for x in min.x..=max.x {
                let pos = BlockPos::new(x, y, z);
                let Some(state) = ctx.block_state_at(pos) else {
                    continue;
                };
                if masked && state.is_air() {
                    continue;
                }
                let data = ctx.block_entity_data(pos);
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
            ctx.place_block(pos, StateId::AIR, None);
        }
    }
    let mut count = 0;
    for block in &copied {
        let pos = BlockPos::new(
            destination.x + block.offset.0,
            destination.y + block.offset.1,
            destination.z + block.offset.2,
        );
        if ctx.place_block(pos, block.state, block.data.as_ref()) {
            count += 1;
        }
    }
    if count == 0 {
        return fail(ctx, sender, "commands.clone.failed", Vec::new());
    }
    ctx.record_result(count);
    ctx.reply(
        sender,
        Component::translatable("commands.clone.success", vec![number(count)]),
    );
    CommandOutcome::Success
}

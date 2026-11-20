use rspigot_domain::BlockPos;
use rspigot_domain::text::{Component, NamedColor};

use super::blocks::pos_at;
use super::dispatch::{CommandContext, CommandOutcome, CommandSender, fail, next_argument, usage};
use crate::chunks::WorldQuery;
use crate::locate::{PlaceKind, QueryKind, normalised};
use crate::ports::{PlaceRequest, TemplateMirror, TemplateRotation};
use crate::structure_ids::STRUCTURES;

const HELP: &str = "/place (feature|jigsaw|structure|template) ...";

fn reply_error<C: CommandContext>(
    ctx: &mut C,
    sender: CommandSender,
    error: Component,
) -> CommandOutcome {
    ctx.reply(sender, error.color(NamedColor::Red));
    CommandOutcome::Failure
}

fn position<'a, C: CommandContext>(
    ctx: &C,
    sender: CommandSender,
    rest: &'a str,
) -> Result<(BlockPos, &'a str), Component> {
    if next_argument(rest).is_none() {
        let origin = ctx.sender_origin(sender);
        return Ok((
            BlockPos::new(
                origin.x.floor() as i32,
                origin.y.floor() as i32,
                origin.z.floor() as i32,
            ),
            rest,
        ));
    }
    pos_at(ctx, sender, rest)
}

fn loaded<C: CommandContext>(ctx: &C, pos: BlockPos) -> Result<(), Component> {
    if !ctx.in_world_height(pos.y) || pos.x.abs() > 30_000_000 || pos.z.abs() > 30_000_000 {
        return Err(Component::translatable(
            "argument.pos.outofworld",
            Vec::new(),
        ));
    }
    if !ctx.is_position_loaded(pos) {
        return Err(Component::translatable("argument.pos.unloaded", Vec::new()));
    }
    Ok(())
}

fn rotation(name: &str) -> Option<TemplateRotation> {
    match name {
        "none" => Some(TemplateRotation::None),
        "clockwise_90" => Some(TemplateRotation::Clockwise90),
        "180" => Some(TemplateRotation::Clockwise180),
        "counterclockwise_90" => Some(TemplateRotation::Counterclockwise90),
        _ => None,
    }
}

fn mirror(name: &str) -> Option<TemplateMirror> {
    match name {
        "none" => Some(TemplateMirror::None),
        "left_right" => Some(TemplateMirror::LeftRight),
        "front_back" => Some(TemplateMirror::FrontBack),
        _ => None,
    }
}

fn submit<C: CommandContext>(
    ctx: &mut C,
    sender: CommandSender,
    request: PlaceRequest,
    kind: PlaceKind,
    pos: BlockPos,
    label: String,
) -> CommandOutcome {
    if let Err(error) = loaded(ctx, pos) {
        return reply_error(ctx, sender, error);
    }
    if ctx.start_world_query(
        sender,
        WorldQuery::Place(request),
        QueryKind::Place(kind, pos),
        label,
    ) {
        CommandOutcome::Success
    } else {
        fail(ctx, sender, "commands.place.structure.failed", Vec::new())
    }
}

fn structure<C: CommandContext>(ctx: &mut C, sender: CommandSender, rest: &str) -> CommandOutcome {
    let Some((raw, rest)) = next_argument(rest) else {
        return usage(ctx, sender, "/place structure <structure> [<pos>]");
    };
    let id = normalised(raw);
    if !STRUCTURES.contains(&id.as_str()) {
        return fail(
            ctx,
            sender,
            "commands.place.structure.invalid",
            vec![Component::text(id)],
        );
    }
    let (pos, _) = match position(ctx, sender, rest) {
        Ok(found) => found,
        Err(error) => return reply_error(ctx, sender, error),
    };
    let request = PlaceRequest::Structure {
        id: id.clone(),
        pos,
    };
    submit(ctx, sender, request, PlaceKind::Structure, pos, id)
}

fn jigsaw<C: CommandContext>(ctx: &mut C, sender: CommandSender, rest: &str) -> CommandOutcome {
    let help = "/place jigsaw <pool> <target> <max_depth> [<pos>]";
    let Some((pool, rest)) = next_argument(rest) else {
        return usage(ctx, sender, help);
    };
    let Some((target, rest)) = next_argument(rest) else {
        return usage(ctx, sender, help);
    };
    let Some((depth, rest)) = next_argument(rest) else {
        return usage(ctx, sender, help);
    };
    let Ok(max_depth) = depth.parse::<i32>() else {
        return fail(
            ctx,
            sender,
            "parsing.int.invalid",
            vec![Component::text(depth)],
        );
    };
    if !(1..=20).contains(&max_depth) {
        let key = if max_depth < 1 {
            "argument.integer.low"
        } else {
            "argument.integer.big"
        };
        let bound = if max_depth < 1 { "1" } else { "20" };
        return fail(
            ctx,
            sender,
            key,
            vec![Component::text(bound), Component::text(depth)],
        );
    }
    let (pos, _) = match position(ctx, sender, rest) {
        Ok(found) => found,
        Err(error) => return reply_error(ctx, sender, error),
    };
    let pool = normalised(pool);
    let request = PlaceRequest::Jigsaw {
        pool: pool.clone(),
        target: normalised(target),
        max_depth,
        pos,
    };
    submit(ctx, sender, request, PlaceKind::Jigsaw, pos, pool)
}

fn template<C: CommandContext>(ctx: &mut C, sender: CommandSender, rest: &str) -> CommandOutcome {
    let help = "/place template <template> [<pos>] [<rotation>] [<mirror>] [<integrity>] [<seed>] [strict]";
    let Some((raw, rest)) = next_argument(rest) else {
        return usage(ctx, sender, help);
    };
    let id = normalised(raw);
    let (pos, rest) = match position(ctx, sender, rest) {
        Ok(found) => found,
        Err(error) => return reply_error(ctx, sender, error),
    };
    let mut rotation_value = TemplateRotation::None;
    let mut mirror_value = TemplateMirror::None;
    let mut integrity = 1.0f32;
    let mut seed = 0i32;
    let mut rest = rest;
    if let Some((token, next)) = next_argument(rest) {
        let Some(found) = rotation(token) else {
            return usage(ctx, sender, help);
        };
        rotation_value = found;
        rest = next;
    }
    if let Some((token, next)) = next_argument(rest) {
        let Some(found) = mirror(token) else {
            return usage(ctx, sender, help);
        };
        mirror_value = found;
        rest = next;
    }
    if let Some((token, next)) = next_argument(rest) {
        let Ok(value) = token.parse::<f32>() else {
            return fail(
                ctx,
                sender,
                "parsing.float.invalid",
                vec![Component::text(token)],
            );
        };
        if !(0.0..=1.0).contains(&value) {
            return usage(ctx, sender, help);
        }
        integrity = value;
        rest = next;
    }
    if let Some((token, next)) = next_argument(rest) {
        let Ok(value) = token.parse::<i32>() else {
            return fail(
                ctx,
                sender,
                "parsing.int.invalid",
                vec![Component::text(token)],
            );
        };
        seed = value;
        rest = next;
    }
    if let Some((token, _)) = next_argument(rest)
        && token != "strict"
    {
        return usage(ctx, sender, help);
    }
    let request = PlaceRequest::Template {
        id: id.clone(),
        pos,
        rotation: rotation_value,
        mirror: mirror_value,
        integrity,
        seed,
    };
    submit(ctx, sender, request, PlaceKind::Template, pos, id)
}

fn feature<C: CommandContext>(ctx: &mut C, sender: CommandSender, rest: &str) -> CommandOutcome {
    let Some((raw, rest)) = next_argument(rest) else {
        return usage(ctx, sender, "/place feature <feature> [<pos>]");
    };
    let (pos, _) = match position(ctx, sender, rest) {
        Ok(found) => found,
        Err(error) => return reply_error(ctx, sender, error),
    };
    let request = PlaceRequest::Feature {
        id: normalised(raw),
        pos,
    };
    submit(
        ctx,
        sender,
        request,
        PlaceKind::Feature,
        pos,
        normalised(raw),
    )
}

pub(super) fn run<C: CommandContext>(
    ctx: &mut C,
    sender: CommandSender,
    rest: &str,
) -> CommandOutcome {
    let Some((kind, rest)) = next_argument(rest) else {
        return usage(ctx, sender, HELP);
    };
    match kind {
        "structure" => structure(ctx, sender, rest),
        "jigsaw" => jigsaw(ctx, sender, rest),
        "template" => template(ctx, sender, rest),
        "feature" => feature(ctx, sender, rest),
        _ => usage(ctx, sender, HELP),
    }
}

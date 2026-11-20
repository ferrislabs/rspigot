use rspigot_domain::text::{Component, NamedColor};

use super::dispatch::{CommandContext, CommandOutcome, CommandSender, fail, next_argument, usage};
use crate::chunks::WorldQuery;
use crate::locate::{
    QueryKind, flat_distance, locate_poi, located, normalised, resolve_biomes, resolve_poi_types,
    resolve_structures,
};

const HELP: &str = "/locate (structure|biome|poi) <name>";

fn reply_error<C: CommandContext>(
    ctx: &mut C,
    sender: CommandSender,
    error: Component,
) -> CommandOutcome {
    ctx.reply(sender, error.color(NamedColor::Red));
    CommandOutcome::Failure
}

fn label(argument: &str) -> String {
    match argument.strip_prefix('#') {
        Some(tag) => format!("#{}", normalised(tag)),
        None => normalised(argument),
    }
}

fn start<C: CommandContext>(
    ctx: &mut C,
    sender: CommandSender,
    query: WorldQuery,
    kind: QueryKind,
    label: String,
) -> CommandOutcome {
    if ctx.start_world_query(sender, query, kind, label) {
        CommandOutcome::Success
    } else {
        fail(ctx, sender, "commands.locate.structure.invalid", Vec::new())
    }
}

pub(super) fn run<C: CommandContext>(
    ctx: &mut C,
    sender: CommandSender,
    rest: &str,
) -> CommandOutcome {
    let Some((kind, rest)) = next_argument(rest) else {
        return usage(ctx, sender, HELP);
    };
    let Some((argument, _)) = next_argument(rest) else {
        return usage(ctx, sender, HELP);
    };
    let shown = label(argument);
    match kind {
        "structure" => match resolve_structures(argument) {
            Ok(ids) => start(
                ctx,
                sender,
                WorldQuery::Structures(ids),
                QueryKind::Structure,
                shown,
            ),
            Err(error) => reply_error(ctx, sender, error),
        },
        "biome" => match resolve_biomes(argument) {
            Ok(ids) => start(
                ctx,
                sender,
                WorldQuery::Biomes(ids),
                QueryKind::Biome,
                shown,
            ),
            Err(error) => reply_error(ctx, sender, error),
        },
        "poi" => match resolve_poi_types(argument) {
            Ok(types) => match locate_poi(ctx, sender, &types) {
                Some((found, pos)) => {
                    let origin = ctx.sender_origin(sender);
                    let name = if argument.starts_with('#') {
                        found
                    } else {
                        shown
                    };
                    ctx.reply(
                        sender,
                        located(
                            "commands.locate.poi.success",
                            &name,
                            pos,
                            false,
                            flat_distance(origin, pos.x, pos.z),
                        ),
                    );
                    CommandOutcome::Success
                }
                None => fail(
                    ctx,
                    sender,
                    "commands.locate.poi.not_found",
                    vec![Component::text(shown)],
                ),
            },
            Err(error) => reply_error(ctx, sender, error),
        },
        _ => usage(ctx, sender, HELP),
    }
}

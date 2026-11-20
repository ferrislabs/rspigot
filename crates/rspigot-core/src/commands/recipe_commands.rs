use rspigot_domain::text::{Component, NamedColor};

use super::dispatch::{CommandContext, CommandOutcome, CommandSender, fail, next_argument, usage};
use crate::recipe_book::{all_recipe_ids, recipe_exists};

fn text(value: impl ToString) -> Component {
    Component::text(value.to_string())
}

pub(super) fn run<C: CommandContext>(
    ctx: &mut C,
    sender: CommandSender,
    rest: &str,
) -> CommandOutcome {
    let help = "/recipe (give|take) <targets> (*|<recipe>)";
    let Some((action, rest)) = next_argument(rest) else {
        return usage(ctx, sender, help);
    };
    let give = match action {
        "give" => true,
        "take" => false,
        _ => return usage(ctx, sender, help),
    };
    let Some((selector, rest)) = next_argument(rest) else {
        return usage(ctx, sender, help);
    };
    let Some((recipe, _)) = next_argument(rest) else {
        return usage(ctx, sender, help);
    };
    let players = match ctx.select_players(sender, selector) {
        Ok(found) if !found.is_empty() => found,
        Ok(_) => return fail(ctx, sender, "argument.entity.notfound.player", Vec::new()),
        Err(message) => {
            ctx.reply(sender, message.color(NamedColor::Red));
            return CommandOutcome::Failure;
        }
    };
    let ids: Vec<&str> = if recipe == "*" {
        all_recipe_ids().collect()
    } else {
        let id = if recipe.contains(':') {
            recipe.to_owned()
        } else {
            format!("minecraft:{recipe}")
        };
        if !recipe_exists(&id) {
            return fail(ctx, sender, "recipe.notFound", vec![text(&id)]);
        }
        vec![
            all_recipe_ids()
                .find(|known| *known == id)
                .unwrap_or_default(),
        ]
    };
    let mut count = 0usize;
    for player in &players {
        count += if give {
            ctx.give_recipes(*player, &ids)
        } else {
            ctx.take_recipes_of(*player, &ids)
        };
    }
    ctx.record_result(i32::try_from(count).unwrap_or(i32::MAX));
    let verb = if give { "give" } else { "take" };
    if count == 0 {
        return fail(
            ctx,
            sender,
            &format!("commands.recipe.{verb}.failed"),
            Vec::new(),
        );
    }
    let (suffix, who) = match players.as_slice() {
        [single] => ("single", ctx.player_name(*single).unwrap_or_default()),
        many => ("multiple", many.len().to_string()),
    };
    ctx.reply(
        sender,
        Component::translatable(
            format!("commands.recipe.{verb}.success.{suffix}"),
            vec![text(count), text(who)],
        ),
    );
    CommandOutcome::Success
}

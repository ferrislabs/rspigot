use rspigot_domain::player::PlayerId;
use rspigot_domain::text::{Component, NamedColor};

use super::dispatch::{
    CommandContext, CommandOutcome, CommandSender, Target, TitleAction, fail, next_argument, usage,
};
use super::json_text;
use super::text_resolve;

fn reply_err<C: CommandContext>(
    ctx: &mut C,
    sender: CommandSender,
    e: Component,
) -> CommandOutcome {
    ctx.reply(sender, e.color(NamedColor::Red));
    CommandOutcome::Failure
}

pub(super) fn targets<C: CommandContext>(
    ctx: &mut C,
    sender: CommandSender,
    selector: &str,
) -> Result<Vec<PlayerId>, CommandOutcome> {
    let found = ctx
        .select_players(sender, selector)
        .map_err(|e| reply_err(ctx, sender, e))?;
    if found.is_empty() {
        return Err(fail(
            ctx,
            sender,
            "argument.entity.notfound.player",
            Vec::new(),
        ));
    }
    Ok(found)
}

fn tellraw<C: CommandContext>(ctx: &mut C, sender: CommandSender, rest: &str) -> CommandOutcome {
    let Some((selector, message)) = next_argument(rest) else {
        return usage(ctx, sender, "/tellraw <targets> <message>");
    };
    let found = match targets(ctx, sender, selector) {
        Ok(found) => found,
        Err(outcome) => return outcome,
    };
    let component = match json_text::parse(message) {
        Ok(component) => component,
        Err(error) => return reply_err(ctx, sender, error),
    };
    for player in &found {
        let shown =
            match text_resolve::resolve(ctx, sender, &component, Some(Target::Player(*player))) {
                Ok(shown) => shown,
                Err(error) => return reply_err(ctx, sender, error),
            };
        ctx.tell_player(*player, shown, false);
    }
    ctx.record_result(found.len() as i32);
    CommandOutcome::Success
}

fn title<C: CommandContext>(ctx: &mut C, sender: CommandSender, rest: &str) -> CommandOutcome {
    let help = "/title <targets> <clear|reset|title|subtitle|actionbar|times> ...";
    let Some((selector, rest)) = next_argument(rest) else {
        return usage(ctx, sender, help);
    };
    let found = match targets(ctx, sender, selector) {
        Ok(found) => found,
        Err(outcome) => return outcome,
    };
    let Some((action, rest)) = next_argument(rest) else {
        return usage(ctx, sender, help);
    };
    let single = found.len() == 1;
    let suffix = if single { "single" } else { "multiple" };
    let who = if single {
        ctx.target_name(Target::Player(found[0]))
    } else {
        Component::text(found.len().to_string())
    };
    let (key, title_action) = match action {
        "clear" => (
            format!("commands.title.cleared.{suffix}"),
            Some(TitleAction::Clear),
        ),
        "reset" => (
            format!("commands.title.reset.{suffix}"),
            Some(TitleAction::Reset),
        ),
        "title" | "subtitle" | "actionbar" => {
            let component = match json_text::parse(rest) {
                Ok(component) => component,
                Err(error) => return reply_err(ctx, sender, error),
            };
            let key = format!("commands.title.show.{action}.{suffix}");
            for player in &found {
                let shown = match text_resolve::resolve(
                    ctx,
                    sender,
                    &component,
                    Some(Target::Player(*player)),
                ) {
                    Ok(shown) => shown,
                    Err(error) => return reply_err(ctx, sender, error),
                };
                match action {
                    "actionbar" => ctx.tell_player(*player, shown, true),
                    "title" => ctx.title_player(*player, TitleAction::Title(shown)),
                    _ => ctx.title_player(*player, TitleAction::Subtitle(shown)),
                }
            }
            (key, None)
        }
        "times" => {
            let numbers: Vec<i32> = rest
                .split_whitespace()
                .filter_map(|n| n.parse().ok())
                .collect();
            let [fade_in, stay, fade_out] = numbers.as_slice() else {
                return usage(
                    ctx,
                    sender,
                    "/title <targets> times <fadeIn> <stay> <fadeOut>",
                );
            };
            if [fade_in, stay, fade_out].iter().any(|n| **n < 0) {
                return fail(ctx, sender, "argument.integer.low", Vec::new());
            }
            (
                format!("commands.title.times.{suffix}"),
                Some(TitleAction::Times {
                    fade_in: *fade_in,
                    stay: *stay,
                    fade_out: *fade_out,
                }),
            )
        }
        _ => return usage(ctx, sender, help),
    };
    if let Some(action) = title_action {
        for player in &found {
            ctx.title_player(*player, action.clone());
        }
    }
    ctx.record_result(found.len() as i32);
    ctx.reply(sender, Component::translatable(key, vec![who]));
    CommandOutcome::Success
}

pub(super) fn run<C: CommandContext>(
    ctx: &mut C,
    sender: CommandSender,
    label: &str,
    rest: &str,
) -> CommandOutcome {
    if label == "tellraw" {
        tellraw(ctx, sender, rest)
    } else {
        title(ctx, sender, rest)
    }
}

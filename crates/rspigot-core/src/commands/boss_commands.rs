use rspigot_domain::Identifier;
use rspigot_domain::boss_event::{BossColor, BossMessages, BossOverlay};
use rspigot_domain::custom_boss::CustomBossBar;
use rspigot_domain::player::PlayerId;

use super::dispatch::{CommandContext, CommandOutcome, CommandSender, fail, next_argument, usage};
use super::json_text;
use super::scoreboard_commands::{joined, parse_int, reply_err, success, text};

const HELP: &str = "/bossbar <add|get|list|remove|set> ...";

fn id_of<C: CommandContext>(
    ctx: &mut C,
    sender: CommandSender,
    raw: &str,
) -> Result<Identifier, CommandOutcome> {
    Identifier::parse(raw).map_err(|_| fail(ctx, sender, "argument.id.invalid", Vec::new()))
}

fn existing<C: CommandContext>(
    ctx: &mut C,
    sender: CommandSender,
    raw: &str,
) -> Result<(Identifier, CustomBossBar), CommandOutcome> {
    let id = id_of(ctx, sender, raw)?;
    match ctx.boss_bar(&id) {
        Some(bar) => Ok((id, bar)),
        None => Err(fail(
            ctx,
            sender,
            "commands.bossbar.unknown",
            vec![text(&id)],
        )),
    }
}

fn online_players<C: CommandContext>(ctx: &C, bar: &CustomBossBar) -> Vec<String> {
    let mut names: Vec<String> = bar
        .event()
        .players()
        .filter_map(|player| ctx.player_name(player))
        .collect();
    names.sort_by_key(|name| name.to_ascii_lowercase());
    names
}

pub(super) fn run<C: CommandContext>(
    ctx: &mut C,
    sender: CommandSender,
    rest: &str,
) -> CommandOutcome {
    let Some((action, rest)) = next_argument(rest) else {
        return usage(ctx, sender, HELP);
    };
    match action {
        "add" => add(ctx, sender, rest),
        "remove" => remove(ctx, sender, rest),
        "list" => list(ctx, sender),
        "get" => get(ctx, sender, rest),
        "set" => set(ctx, sender, rest),
        _ => usage(ctx, sender, HELP),
    }
}

fn add<C: CommandContext>(ctx: &mut C, sender: CommandSender, rest: &str) -> CommandOutcome {
    let Some((raw, rest)) = next_argument(rest) else {
        return usage(ctx, sender, "/bossbar add <id> <name>");
    };
    let id = match id_of(ctx, sender, raw) {
        Ok(id) => id,
        Err(outcome) => return outcome,
    };
    if rest.trim().is_empty() {
        return usage(ctx, sender, "/bossbar add <id> <name>");
    }
    let name = match json_text::parse(rest.trim())
        .and_then(|name| super::text_resolve::resolve_for_source(ctx, sender, &name))
    {
        Ok(name) => name,
        Err(error) => return reply_err(ctx, sender, error),
    };
    if ctx.boss_bar(&id).is_some() || !ctx.create_boss_bar(id.clone(), name) {
        return fail(
            ctx,
            sender,
            "commands.bossbar.create.failed",
            vec![text(&id)],
        );
    }
    let Some(bar) = ctx.boss_bar(&id) else {
        return CommandOutcome::Failure;
    };
    ctx.record_result(ctx.boss_bars().len() as i32);
    success(
        ctx,
        sender,
        "commands.bossbar.create.success",
        vec![bar.display_name()],
    );
    CommandOutcome::Success
}

fn remove<C: CommandContext>(ctx: &mut C, sender: CommandSender, rest: &str) -> CommandOutcome {
    let Some((raw, _)) = next_argument(rest) else {
        return usage(ctx, sender, "/bossbar remove <id>");
    };
    let (id, bar) = match existing(ctx, sender, raw) {
        Ok(found) => found,
        Err(outcome) => return outcome,
    };
    ctx.remove_boss_bar(&id);
    ctx.record_result(ctx.boss_bars().len() as i32);
    success(
        ctx,
        sender,
        "commands.bossbar.remove.success",
        vec![bar.display_name()],
    );
    CommandOutcome::Success
}

fn list<C: CommandContext>(ctx: &mut C, sender: CommandSender) -> CommandOutcome {
    let bars = ctx.boss_bars();
    ctx.record_result(bars.len() as i32);
    if bars.is_empty() {
        success(ctx, sender, "commands.bossbar.list.bars.none", Vec::new());
        return CommandOutcome::Success;
    }
    let shown = joined(bars.iter().map(CustomBossBar::display_name).collect());
    success(
        ctx,
        sender,
        "commands.bossbar.list.bars.some",
        vec![text(bars.len()), shown],
    );
    CommandOutcome::Success
}

fn get<C: CommandContext>(ctx: &mut C, sender: CommandSender, rest: &str) -> CommandOutcome {
    let help = "/bossbar get <id> <value|max|visible|players>";
    let Some((raw, rest)) = next_argument(rest) else {
        return usage(ctx, sender, help);
    };
    let (_, bar) = match existing(ctx, sender, raw) {
        Ok(found) => found,
        Err(outcome) => return outcome,
    };
    let Some((field, _)) = next_argument(rest) else {
        return usage(ctx, sender, help);
    };
    let name = bar.display_name();
    match field {
        "value" => {
            ctx.record_result(bar.value());
            success(
                ctx,
                sender,
                "commands.bossbar.get.value",
                vec![name, text(bar.value())],
            );
        }
        "max" => {
            ctx.record_result(bar.max());
            success(
                ctx,
                sender,
                "commands.bossbar.get.max",
                vec![name, text(bar.max())],
            );
        }
        "visible" => {
            let visible = bar.event().is_visible();
            ctx.record_result(i32::from(visible));
            let key = if visible {
                "commands.bossbar.get.visible.visible"
            } else {
                "commands.bossbar.get.visible.hidden"
            };
            success(ctx, sender, key, vec![name]);
        }
        "players" => {
            let names = online_players(ctx, &bar);
            ctx.record_result(names.len() as i32);
            if names.is_empty() {
                success(ctx, sender, "commands.bossbar.get.players.none", vec![name]);
            } else {
                success(
                    ctx,
                    sender,
                    "commands.bossbar.get.players.some",
                    vec![
                        name,
                        text(names.len()),
                        joined(names.iter().map(text).collect()),
                    ],
                );
            }
        }
        _ => return usage(ctx, sender, help),
    }
    CommandOutcome::Success
}

fn change<C: CommandContext>(
    ctx: &mut C,
    id: &Identifier,
    change: impl FnOnce(&mut CustomBossBar) -> BossMessages,
) {
    ctx.change_boss_bar(id, |bar| ((), change(bar)));
}

fn unchanged<C: CommandContext>(ctx: &mut C, sender: CommandSender, key: &str) -> CommandOutcome {
    fail(ctx, sender, key, Vec::new())
}

fn set<C: CommandContext>(ctx: &mut C, sender: CommandSender, rest: &str) -> CommandOutcome {
    let help = "/bossbar set <id> <color|max|name|players|style|value|visible> ...";
    let Some((raw, rest)) = next_argument(rest) else {
        return usage(ctx, sender, help);
    };
    let (id, bar) = match existing(ctx, sender, raw) {
        Ok(found) => found,
        Err(outcome) => return outcome,
    };
    let Some((field, rest)) = next_argument(rest) else {
        return usage(ctx, sender, help);
    };
    let value = rest.trim();
    let shown = bar.display_name();
    match field {
        "color" => {
            let Some(color) = BossColor::from_name(value) else {
                return usage(ctx, sender, "/bossbar set <id> color <color>");
            };
            if color == bar.event().color() {
                return unchanged(ctx, sender, "commands.bossbar.set.color.unchanged");
            }
            change(ctx, &id, |bar| bar.event_mut().set_color(color));
            ctx.record_result(0);
            let name = ctx.boss_bar(&id).map_or(shown, |bar| bar.display_name());
            success(
                ctx,
                sender,
                "commands.bossbar.set.color.success",
                vec![name],
            );
        }
        "style" => {
            let Some(overlay) = BossOverlay::from_name(value) else {
                return usage(ctx, sender, "/bossbar set <id> style <style>");
            };
            if overlay == bar.event().overlay() {
                return unchanged(ctx, sender, "commands.bossbar.set.style.unchanged");
            }
            change(ctx, &id, |bar| bar.event_mut().set_overlay(overlay));
            ctx.record_result(0);
            success(
                ctx,
                sender,
                "commands.bossbar.set.style.success",
                vec![shown],
            );
        }
        "value" => {
            let amount = match parse_int(ctx, sender, value, Some(0)) {
                Ok(amount) => amount,
                Err(outcome) => return outcome,
            };
            if amount == bar.value() {
                return unchanged(ctx, sender, "commands.bossbar.set.value.unchanged");
            }
            change(ctx, &id, |bar| bar.set_value(amount));
            ctx.record_result(amount);
            success(
                ctx,
                sender,
                "commands.bossbar.set.value.success",
                vec![shown, text(amount)],
            );
        }
        "max" => {
            let amount = match parse_int(ctx, sender, value, Some(1)) {
                Ok(amount) => amount,
                Err(outcome) => return outcome,
            };
            if amount == bar.max() {
                return unchanged(ctx, sender, "commands.bossbar.set.max.unchanged");
            }
            change(ctx, &id, |bar| bar.set_max(amount));
            ctx.record_result(amount);
            success(
                ctx,
                sender,
                "commands.bossbar.set.max.success",
                vec![shown, text(amount)],
            );
        }
        "visible" => {
            let visible = match value {
                "true" => true,
                "false" => false,
                _ => return usage(ctx, sender, "/bossbar set <id> visible <true|false>"),
            };
            if visible == bar.event().is_visible() {
                let key = if visible {
                    "commands.bossbar.set.visibility.unchanged.visible"
                } else {
                    "commands.bossbar.set.visibility.unchanged.hidden"
                };
                return unchanged(ctx, sender, key);
            }
            change(ctx, &id, |bar| bar.event_mut().set_visible(visible));
            ctx.record_result(0);
            let key = if visible {
                "commands.bossbar.set.visible.success.visible"
            } else {
                "commands.bossbar.set.visible.success.hidden"
            };
            success(ctx, sender, key, vec![shown]);
        }
        "name" => {
            if value.is_empty() {
                return usage(ctx, sender, "/bossbar set <id> name <name>");
            }
            let name = match json_text::parse(value)
                .and_then(|name| super::text_resolve::resolve_for_source(ctx, sender, &name))
            {
                Ok(name) => name,
                Err(error) => return reply_err(ctx, sender, error),
            };
            if &name == bar.event().name() {
                return unchanged(ctx, sender, "commands.bossbar.set.name.unchanged");
            }
            change(ctx, &id, |bar| bar.event_mut().set_name(name));
            ctx.record_result(0);
            let name = ctx.boss_bar(&id).map_or(shown, |bar| bar.display_name());
            success(ctx, sender, "commands.bossbar.set.name.success", vec![name]);
        }
        "players" => return set_players(ctx, sender, &id, &bar, value),
        _ => return usage(ctx, sender, help),
    }
    CommandOutcome::Success
}

fn set_players<C: CommandContext>(
    ctx: &mut C,
    sender: CommandSender,
    id: &Identifier,
    bar: &CustomBossBar,
    selector: &str,
) -> CommandOutcome {
    let wanted: Vec<PlayerId> = if selector.is_empty() {
        Vec::new()
    } else {
        match ctx.select_players(sender, selector) {
            Ok(players) => players,
            Err(error) => return reply_err(ctx, sender, error),
        }
    };
    let current: Vec<PlayerId> = bar.event().players().collect();
    let removed: Vec<PlayerId> = current
        .iter()
        .copied()
        .filter(|player| !wanted.contains(player))
        .collect();
    let added: Vec<PlayerId> = wanted
        .iter()
        .copied()
        .filter(|player| !current.contains(player))
        .collect();
    if removed.is_empty() && added.is_empty() {
        return unchanged(ctx, sender, "commands.bossbar.set.players.unchanged");
    }
    change(ctx, id, |bar| {
        let mut messages = BossMessages::new();
        for player in removed {
            messages.extend(bar.event_mut().remove_player(player));
        }
        for player in added {
            messages.extend(bar.event_mut().add_player(player));
        }
        messages
    });
    let Some(bar) = ctx.boss_bar(id) else {
        return CommandOutcome::Failure;
    };
    let names = online_players(ctx, &bar);
    ctx.record_result(names.len() as i32);
    if names.is_empty() {
        success(
            ctx,
            sender,
            "commands.bossbar.set.players.success.none",
            vec![bar.display_name()],
        );
    } else {
        success(
            ctx,
            sender,
            "commands.bossbar.set.players.success.some",
            vec![
                bar.display_name(),
                text(names.len()),
                joined(names.iter().map(text).collect()),
            ],
        );
    }
    CommandOutcome::Success
}

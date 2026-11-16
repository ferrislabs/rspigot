use rspigot_domain::player::{GameMode, PlayerId};
use rspigot_domain::text::{Component, NamedColor};

use super::dispatch::{
    CommandContext, CommandOutcome, CommandSender, Target, fail, next_argument, parse_vec3, usage,
};
use super::selector::parse_int_range;

fn text(value: impl ToString) -> Component {
    Component::text(value.to_string())
}

fn reply_err<C: CommandContext>(
    ctx: &mut C,
    sender: CommandSender,
    e: Component,
) -> CommandOutcome {
    ctx.reply(sender, e.color(NamedColor::Red));
    CommandOutcome::Failure
}

fn players<C: CommandContext>(
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

fn entities<C: CommandContext>(
    ctx: &mut C,
    sender: CommandSender,
    selector: &str,
) -> Result<Vec<Target>, CommandOutcome> {
    let found = ctx
        .select_entities(sender, selector)
        .map_err(|e| reply_err(ctx, sender, e))?;
    if found.is_empty() {
        return Err(fail(
            ctx,
            sender,
            "argument.entity.notfound.entity",
            Vec::new(),
        ));
    }
    Ok(found)
}

fn default_game_mode<C: CommandContext>(
    ctx: &mut C,
    sender: CommandSender,
    rest: &str,
) -> CommandOutcome {
    let Some((name, _)) = next_argument(rest) else {
        return usage(ctx, sender, "/defaultgamemode <mode>");
    };
    let Some(mode) = GameMode::from_name(name).filter(|_| name.parse::<u8>().is_err()) else {
        return fail(ctx, sender, "argument.gamemode.invalid", vec![text(name)]);
    };
    ctx.set_default_game_mode(mode);
    ctx.record_result(i32::from(mode.id()));
    ctx.reply(
        sender,
        Component::translatable(
            "commands.defaultgamemode.success",
            vec![Component::translatable(
                format!("gameMode.{}", mode.name()),
                Vec::new(),
            )],
        ),
    );
    CommandOutcome::Success
}

fn gamerule<C: CommandContext>(ctx: &mut C, sender: CommandSender, rest: &str) -> CommandOutcome {
    let Some((rule, rest)) = next_argument(rest) else {
        return usage(ctx, sender, "/gamerule <rule> [value]");
    };
    let rule = rule.strip_prefix("minecraft:").unwrap_or(rule);
    let Some(current) = ctx.game_rule_text(rule) else {
        return fail(ctx, sender, "argument.rule.invalid", vec![text(rule)]);
    };
    let Some((value, _)) = next_argument(rest) else {
        ctx.record_result(
            current
                .parse::<i32>()
                .unwrap_or(i32::from(current == "true")),
        );
        ctx.reply(
            sender,
            Component::translatable("commands.gamerule.query", vec![text(rule), text(current)]),
        );
        return CommandOutcome::Success;
    };
    match ctx.set_game_rule_text(rule, value) {
        Ok(()) => {
            ctx.record_result(value.parse::<i32>().unwrap_or(i32::from(value == "true")));
            ctx.reply(
                sender,
                Component::translatable("commands.gamerule.set", vec![text(rule), text(value)]),
            );
            CommandOutcome::Success
        }
        Err(key) => fail(ctx, sender, key, vec![text(value)]),
    }
}

fn experience<C: CommandContext>(ctx: &mut C, sender: CommandSender, rest: &str) -> CommandOutcome {
    let help = "/experience <add|set|query> <targets> ...";
    let Some((action, rest)) = next_argument(rest) else {
        return usage(ctx, sender, help);
    };
    let Some((selector, rest)) = next_argument(rest) else {
        return usage(ctx, sender, help);
    };
    let targets = match players(ctx, sender, selector) {
        Ok(found) => found,
        Err(outcome) => return outcome,
    };
    if action == "query" {
        let Some((unit, _)) = next_argument(rest) else {
            return usage(ctx, sender, "/experience query <target> <points|levels>");
        };
        let [target] = targets.as_slice() else {
            return fail(ctx, sender, "argument.player.toomany", Vec::new());
        };
        let Some((levels, points)) = ctx.experience_state(*target) else {
            return fail(ctx, sender, "argument.entity.notfound.player", Vec::new());
        };
        let name = ctx.target_name(Target::Player(*target));
        let (key, value) = match unit {
            "levels" => ("commands.experience.query.levels", levels),
            "points" => ("commands.experience.query.points", points),
            _ => return usage(ctx, sender, "/experience query <target> <points|levels>"),
        };
        ctx.record_result(value);
        ctx.reply(
            sender,
            Component::translatable(key, vec![name, text(value)]),
        );
        return CommandOutcome::Success;
    }
    if !matches!(action, "add" | "set") {
        return usage(ctx, sender, help);
    }
    let Some((amount, rest)) = next_argument(rest) else {
        return usage(ctx, sender, help);
    };
    let Ok(amount) = amount.parse::<i32>() else {
        return fail(ctx, sender, "parsing.int.invalid", vec![text(amount)]);
    };
    let levels = match next_argument(rest).map(|(unit, _)| unit) {
        None | Some("points") => false,
        Some("levels") => true,
        Some(_) => return usage(ctx, sender, help),
    };
    if action == "set" && amount < 0 {
        return fail(
            ctx,
            sender,
            "argument.integer.low",
            vec![text(0), text(amount)],
        );
    }
    let mut changed = 0;
    for player in &targets {
        if action == "add" {
            ctx.add_experience(*player, amount, levels);
            changed += 1;
        } else if ctx.set_experience(*player, amount, levels) {
            changed += 1;
        }
    }
    let unit = if levels { "levels" } else { "points" };
    if changed == 0 {
        return fail(
            ctx,
            sender,
            &format!("commands.experience.set.{unit}.invalid"),
            Vec::new(),
        );
    }
    let key = format!(
        "commands.experience.{action}.{unit}.success.{}",
        if targets.len() == 1 {
            "single"
        } else {
            "multiple"
        }
    );
    let second = if targets.len() == 1 {
        ctx.target_name(Target::Player(targets[0]))
    } else {
        text(targets.len())
    };
    ctx.record_result(targets.len() as i32);
    ctx.reply(
        sender,
        Component::translatable(key, vec![text(amount), second]),
    );
    CommandOutcome::Success
}

fn tag<C: CommandContext>(ctx: &mut C, sender: CommandSender, rest: &str) -> CommandOutcome {
    let help = "/tag <targets> <add|remove|list> [name]";
    let Some((selector, rest)) = next_argument(rest) else {
        return usage(ctx, sender, help);
    };
    let targets = match entities(ctx, sender, selector) {
        Ok(found) => found,
        Err(outcome) => return outcome,
    };
    let Some((action, rest)) = next_argument(rest) else {
        return usage(ctx, sender, help);
    };
    let single = targets.len() == 1;
    let suffix = if single { "single" } else { "multiple" };
    match action {
        "add" | "remove" => {
            let Some((name, _)) = next_argument(rest) else {
                return usage(ctx, sender, help);
            };
            let mut changed = 0;
            for target in &targets {
                let done = if action == "add" {
                    ctx.add_tag(*target, name)
                } else {
                    ctx.remove_tag(*target, name)
                };
                changed += i32::from(done);
            }
            if changed == 0 {
                return fail(
                    ctx,
                    sender,
                    &format!("commands.tag.{action}.failed"),
                    Vec::new(),
                );
            }
            let second = if single {
                ctx.target_name(targets[0])
            } else {
                text(targets.len())
            };
            ctx.record_result(changed);
            ctx.reply(
                sender,
                Component::translatable(
                    format!("commands.tag.{action}.success.{suffix}"),
                    vec![text(name), second],
                ),
            );
            CommandOutcome::Success
        }
        "list" => {
            let mut all: Vec<String> = targets.iter().flat_map(|t| ctx.tags_of(*t)).collect();
            all.sort();
            all.dedup();
            let (key, args) = match (single, all.is_empty()) {
                (true, true) => (
                    "commands.tag.list.single.empty",
                    vec![ctx.target_name(targets[0])],
                ),
                (true, false) => (
                    "commands.tag.list.single.success",
                    vec![
                        ctx.target_name(targets[0]),
                        text(all.len()),
                        text(all.join(", ")),
                    ],
                ),
                (false, true) => (
                    "commands.tag.list.multiple.empty",
                    vec![text(targets.len())],
                ),
                (false, false) => (
                    "commands.tag.list.multiple.success",
                    vec![text(targets.len()), text(all.len()), text(all.join(", "))],
                ),
            };
            ctx.record_result(all.len() as i32);
            ctx.reply(sender, Component::translatable(key, args));
            CommandOutcome::Success
        }
        _ => usage(ctx, sender, help),
    }
}

fn saving<C: CommandContext>(ctx: &mut C, sender: CommandSender, enabled: bool) -> CommandOutcome {
    if !ctx.set_saving(enabled) {
        let key = if enabled {
            "commands.save.alreadyOn"
        } else {
            "commands.save.alreadyOff"
        };
        return fail(ctx, sender, key, Vec::new());
    }
    let key = if enabled {
        "commands.save.enabled"
    } else {
        "commands.save.disabled"
    };
    ctx.record_result(1);
    ctx.reply(sender, Component::translatable(key, Vec::new()));
    CommandOutcome::Success
}

fn random<C: CommandContext>(ctx: &mut C, sender: CommandSender, rest: &str) -> CommandOutcome {
    let help = "/random <value|roll> <range> [sequence]";
    let Some((mode, rest)) = next_argument(rest) else {
        return usage(ctx, sender, help);
    };
    if !matches!(mode, "value" | "roll") {
        return usage(ctx, sender, help);
    }
    let Some((range, _)) = next_argument(rest) else {
        return usage(ctx, sender, help);
    };
    let (min, max) = match parse_int_range(range) {
        Ok((min, max)) => (min.unwrap_or(i32::MIN), max.unwrap_or(i32::MAX)),
        Err(error) => return reply_err(ctx, sender, error),
    };
    let size = i64::from(max) - i64::from(min) + 1;
    if size < 2 {
        return fail(
            ctx,
            sender,
            "commands.random.error.range_too_small",
            Vec::new(),
        );
    }
    if size > i64::from(i32::MAX) {
        return fail(
            ctx,
            sender,
            "commands.random.error.range_too_large",
            Vec::new(),
        );
    }
    let value = ctx.random_between(min, max);
    ctx.record_result(value);
    if mode == "value" {
        ctx.reply(
            sender,
            Component::translatable("commands.random.sample.success", vec![text(value)]),
        );
    } else {
        let name = ctx.sender_name(sender);
        ctx.broadcast(Component::translatable(
            "commands.random.roll",
            vec![text(name), text(value), text(min), text(max)],
        ));
    }
    CommandOutcome::Success
}

fn spawnpoint<C: CommandContext>(ctx: &mut C, sender: CommandSender, rest: &str) -> CommandOutcome {
    let mut cursor = rest;
    let mut targets: Option<Vec<PlayerId>> = None;
    if let Some((first, after)) = next_argument(cursor)
        && !first.starts_with('~')
        && !first.starts_with('^')
        && first.parse::<f64>().is_err()
    {
        targets = Some(match players(ctx, sender, first) {
            Ok(found) => found,
            Err(outcome) => return outcome,
        });
        cursor = after;
    }
    let targets = match targets {
        Some(found) => found,
        None => match sender {
            CommandSender::Player(id) => vec![id],
            CommandSender::Console => {
                return fail(ctx, sender, "permissions.requires.player", Vec::new());
            }
        },
    };
    let position = match next_argument(cursor) {
        Some(_) => {
            let mut raw = [""; 3];
            let mut after = cursor;
            for slot in &mut raw {
                let Some((token, next)) = next_argument(after) else {
                    return fail(ctx, sender, "argument.pos3d.incomplete", Vec::new());
                };
                *slot = token;
                after = next;
            }
            cursor = after;
            match parse_vec3(ctx, sender, raw) {
                Ok(position) => position,
                Err(error) => return reply_err(ctx, sender, error),
            }
        }
        None => ctx.sender_origin(sender),
    };
    let angle = match next_argument(cursor) {
        Some((token, _)) => match token.parse::<f32>() {
            Ok(angle) => angle,
            Err(_) => return fail(ctx, sender, "parsing.float.invalid", vec![text(token)]),
        },
        None => 0.0,
    };
    let pos = rspigot_domain::BlockPos::new(
        position.x.floor() as i32,
        position.y.floor() as i32,
        position.z.floor() as i32,
    );
    let dimension = ctx
        .current_dimension()
        .unwrap_or_else(|| rspigot_domain::Identifier::minecraft("overworld"));
    for player in &targets {
        ctx.set_spawn_point(*player, dimension.clone(), pos, angle);
    }
    let suffix = if targets.len() == 1 {
        "single"
    } else {
        "multiple"
    };
    let last = if targets.len() == 1 {
        ctx.target_name(Target::Player(targets[0]))
    } else {
        text(targets.len())
    };
    ctx.record_result(targets.len() as i32);
    ctx.reply(
        sender,
        Component::translatable(
            format!("commands.spawnpoint.success.{suffix}"),
            vec![
                text(pos.x),
                text(pos.y),
                text(pos.z),
                text(angle),
                text(dimension),
                last,
            ],
        ),
    );
    CommandOutcome::Success
}

pub(super) fn run<C: CommandContext>(
    ctx: &mut C,
    sender: CommandSender,
    label: &str,
    rest: &str,
) -> CommandOutcome {
    match label {
        "defaultgamemode" => default_game_mode(ctx, sender, rest),
        "gamerule" => gamerule(ctx, sender, rest),
        "experience" | "xp" => experience(ctx, sender, rest),
        "tag" => tag(ctx, sender, rest),
        "save-on" => saving(ctx, sender, true),
        "save-off" => saving(ctx, sender, false),
        "random" => random(ctx, sender, rest),
        _ => spawnpoint(ctx, sender, rest),
    }
}

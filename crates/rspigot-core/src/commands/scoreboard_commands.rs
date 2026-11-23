use rspigot_domain::scoreboard::{
    Criteria, DisplaySlot, NumberFormat, Objective, RenderType, ScoreboardMessage,
};
use rspigot_domain::text::{Component, NamedColor};

use super::dispatch::{CommandContext, CommandOutcome, CommandSender, fail, next_argument, usage};
use super::json_text;

pub(super) fn text(value: impl ToString) -> Component {
    Component::text(value.to_string())
}

pub(super) fn bracketed(inner: Component) -> Component {
    Component::text("[")
        .append(inner)
        .append(Component::text("]"))
}

pub(super) fn joined(items: Vec<Component>) -> Component {
    let mut out = Component::text("");
    for (index, item) in items.into_iter().enumerate() {
        if index > 0 {
            out = out.append(Component::text(", "));
        }
        out = out.append(item);
    }
    out
}

pub(super) fn reply_err<C: CommandContext>(
    ctx: &mut C,
    sender: CommandSender,
    error: Component,
) -> CommandOutcome {
    ctx.reply(sender, error.color(NamedColor::Red));
    CommandOutcome::Failure
}

pub(super) fn success<C: CommandContext>(
    ctx: &mut C,
    sender: CommandSender,
    key: &str,
    args: Vec<Component>,
) {
    ctx.reply(sender, Component::translatable(key, args));
}

pub(super) fn parse_int<C: CommandContext>(
    ctx: &mut C,
    sender: CommandSender,
    raw: &str,
    min: Option<i32>,
) -> Result<i32, CommandOutcome> {
    let Ok(value) = raw.parse::<i32>() else {
        return Err(fail(ctx, sender, "parsing.int.invalid", vec![text(raw)]));
    };
    if let Some(min) = min.filter(|min| value < *min) {
        return Err(fail(
            ctx,
            sender,
            "argument.integer.low",
            vec![text(min), text(value)],
        ));
    }
    Ok(value)
}

pub(super) fn parse_bool<C: CommandContext>(
    ctx: &mut C,
    sender: CommandSender,
    raw: &str,
) -> Result<bool, CommandOutcome> {
    match raw {
        "true" => Ok(true),
        "false" => Ok(false),
        _ => Err(fail(ctx, sender, "parsing.bool.invalid", vec![text(raw)])),
    }
}

pub(super) fn objective_display(objective: &Objective) -> Component {
    bracketed(objective.display_name.clone())
}

pub(super) fn find_objective<C: CommandContext>(
    ctx: &mut C,
    sender: CommandSender,
    name: &str,
) -> Result<Objective, CommandOutcome> {
    match ctx.scoreboard().objective(name) {
        Some(objective) => Ok(objective.clone()),
        None => Err(fail(
            ctx,
            sender,
            "arguments.objective.notFound",
            vec![text(name)],
        )),
    }
}

pub(super) fn writable_objective<C: CommandContext>(
    ctx: &mut C,
    sender: CommandSender,
    name: &str,
) -> Result<Objective, CommandOutcome> {
    let objective = find_objective(ctx, sender, name)?;
    if objective.criteria.read_only() {
        return Err(fail(
            ctx,
            sender,
            "arguments.objective.readonly",
            vec![text(name)],
        ));
    }
    Ok(objective)
}

pub(super) fn resolve_holders<C: CommandContext>(
    ctx: &mut C,
    sender: CommandSender,
    token: &str,
) -> Result<Vec<String>, Component> {
    if token == "*" {
        let all: Vec<String> = ctx
            .scoreboard()
            .holders()
            .into_iter()
            .map(str::to_owned)
            .collect();
        if all.is_empty() {
            return Err(Component::translatable(
                "argument.scoreHolder.empty",
                Vec::new(),
            ));
        }
        return Ok(all);
    }
    if token.starts_with('@') {
        let found = ctx.select_entities(sender, token)?;
        let mut names: Vec<String> = Vec::new();
        for target in found {
            if let Some(name) = ctx.score_holder_name(target)
                && !names.contains(&name)
            {
                names.push(name);
            }
        }
        if names.is_empty() {
            return Err(Component::translatable(
                "argument.entity.notfound.entity",
                Vec::new(),
            ));
        }
        return Ok(names);
    }
    Ok(vec![token.to_owned()])
}

pub(super) fn resolve_single_holder<C: CommandContext>(
    ctx: &mut C,
    sender: CommandSender,
    token: &str,
) -> Result<String, Component> {
    let mut found = resolve_holders(ctx, sender, token)?;
    if found.len() > 1 {
        return Err(Component::translatable(
            "argument.entity.toomany",
            Vec::new(),
        ));
    }
    Ok(found.remove(0))
}

pub(super) fn holders<C: CommandContext>(
    ctx: &mut C,
    sender: CommandSender,
    token: &str,
) -> Result<Vec<String>, CommandOutcome> {
    resolve_holders(ctx, sender, token).map_err(|error| reply_err(ctx, sender, error))
}

pub(super) fn single_holder<C: CommandContext>(
    ctx: &mut C,
    sender: CommandSender,
    token: &str,
) -> Result<String, CommandOutcome> {
    resolve_single_holder(ctx, sender, token).map_err(|error| reply_err(ctx, sender, error))
}

fn component_of<C: CommandContext>(
    ctx: &mut C,
    sender: CommandSender,
    raw: &str,
) -> Result<Component, CommandOutcome> {
    json_text::parse(raw)
        .and_then(|component| super::text_resolve::resolve_for_source(ctx, sender, &component))
        .map_err(|error| reply_err(ctx, sender, error))
}

fn number_format_from<C: CommandContext>(
    ctx: &mut C,
    sender: CommandSender,
    rest: &str,
) -> Result<Option<NumberFormat>, CommandOutcome> {
    let Some((kind, rest)) = next_argument(rest) else {
        return Ok(None);
    };
    match kind {
        "blank" => Ok(Some(NumberFormat::Blank)),
        "fixed" => Ok(Some(NumberFormat::Fixed(component_of(
            ctx,
            sender,
            rest.trim(),
        )?))),
        "styled" => json_text::parse_style(rest.trim())
            .map(|style| Some(NumberFormat::Styled(style)))
            .map_err(|error| reply_err(ctx, sender, error)),
        _ => Err(CommandOutcome::Unknown),
    }
}

pub(super) fn run<C: CommandContext>(
    ctx: &mut C,
    sender: CommandSender,
    label: &str,
    rest: &str,
) -> CommandOutcome {
    match label {
        "scoreboard" => scoreboard(ctx, sender, rest),
        _ => trigger(ctx, sender, rest),
    }
}

fn scoreboard<C: CommandContext>(ctx: &mut C, sender: CommandSender, rest: &str) -> CommandOutcome {
    let Some((group, rest)) = next_argument(rest) else {
        return usage(ctx, sender, "/scoreboard <objectives|players> ...");
    };
    match group {
        "objectives" => objectives(ctx, sender, rest),
        "players" => players(ctx, sender, rest),
        _ => usage(ctx, sender, "/scoreboard <objectives|players> ..."),
    }
}

fn objectives<C: CommandContext>(ctx: &mut C, sender: CommandSender, rest: &str) -> CommandOutcome {
    let help = "/scoreboard objectives <list|add|remove|setdisplay|modify> ...";
    let Some((action, rest)) = next_argument(rest) else {
        return usage(ctx, sender, help);
    };
    match action {
        "list" => objectives_list(ctx, sender),
        "add" => objectives_add(ctx, sender, rest),
        "remove" => objectives_remove(ctx, sender, rest),
        "setdisplay" => objectives_setdisplay(ctx, sender, rest),
        "modify" => objectives_modify(ctx, sender, rest),
        _ => usage(ctx, sender, help),
    }
}

fn objectives_list<C: CommandContext>(ctx: &mut C, sender: CommandSender) -> CommandOutcome {
    let all: Vec<Objective> = ctx.scoreboard().objectives().cloned().collect();
    let count = all.len() as i32;
    ctx.record_result(count);
    if all.is_empty() {
        success(
            ctx,
            sender,
            "commands.scoreboard.objectives.list.empty",
            Vec::new(),
        );
        return CommandOutcome::Success;
    }
    let names = joined(all.iter().map(objective_display).collect());
    success(
        ctx,
        sender,
        "commands.scoreboard.objectives.list.success",
        vec![text(count), names],
    );
    CommandOutcome::Success
}

fn objectives_add<C: CommandContext>(
    ctx: &mut C,
    sender: CommandSender,
    rest: &str,
) -> CommandOutcome {
    let help = "/scoreboard objectives add <objective> <criteria> [<displayName>]";
    let Some((name, rest)) = next_argument(rest) else {
        return usage(ctx, sender, help);
    };
    let Some((criteria, rest)) = next_argument(rest) else {
        return usage(ctx, sender, help);
    };
    let Some(criteria) = crate::scoreboard::parse_criteria(criteria) else {
        return fail(
            ctx,
            sender,
            "argument.criteria.invalid",
            vec![text(criteria)],
        );
    };
    let display = if rest.trim().is_empty() {
        text(name)
    } else {
        match component_of(ctx, sender, rest.trim()) {
            Ok(display) => display,
            Err(outcome) => return outcome,
        }
    };
    let objective = Objective::new(name, criteria, display);
    let shown = objective_display(&objective);
    let added = ctx.change_scoreboard(|board| {
        let result = board.add_objective(objective);
        (result, Vec::new())
    });
    if added.is_err() {
        return fail(
            ctx,
            sender,
            "commands.scoreboard.objectives.add.duplicate",
            Vec::new(),
        );
    }
    let count = ctx.scoreboard().objectives().count() as i32;
    ctx.record_result(count);
    success(
        ctx,
        sender,
        "commands.scoreboard.objectives.add.success",
        vec![shown],
    );
    CommandOutcome::Success
}

fn objectives_remove<C: CommandContext>(
    ctx: &mut C,
    sender: CommandSender,
    rest: &str,
) -> CommandOutcome {
    let Some((name, _)) = next_argument(rest) else {
        return usage(ctx, sender, "/scoreboard objectives remove <objective>");
    };
    let objective = match find_objective(ctx, sender, name) {
        Ok(objective) => objective,
        Err(outcome) => return outcome,
    };
    ctx.change_scoreboard(|board| ((), board.remove_objective(name)));
    let count = ctx.scoreboard().objectives().count() as i32;
    ctx.record_result(count);
    success(
        ctx,
        sender,
        "commands.scoreboard.objectives.remove.success",
        vec![objective_display(&objective)],
    );
    CommandOutcome::Success
}

fn objectives_setdisplay<C: CommandContext>(
    ctx: &mut C,
    sender: CommandSender,
    rest: &str,
) -> CommandOutcome {
    let Some((slot, rest)) = next_argument(rest) else {
        return usage(
            ctx,
            sender,
            "/scoreboard objectives setdisplay <slot> [<objective>]",
        );
    };
    let Some(slot) = DisplaySlot::from_name(slot) else {
        return fail(
            ctx,
            sender,
            "argument.scoreboardDisplaySlot.invalid",
            vec![text(slot)],
        );
    };
    let current = ctx.scoreboard().display(slot).map(str::to_owned);
    let Some((name, _)) = next_argument(rest) else {
        if current.is_none() {
            return fail(
                ctx,
                sender,
                "commands.scoreboard.objectives.display.alreadyEmpty",
                Vec::new(),
            );
        }
        ctx.change_scoreboard(|board| ((), board.set_display(slot, None).unwrap_or_default()));
        success(
            ctx,
            sender,
            "commands.scoreboard.objectives.display.cleared",
            vec![text(slot.name())],
        );
        return CommandOutcome::Success;
    };
    let objective = match find_objective(ctx, sender, name) {
        Ok(objective) => objective,
        Err(outcome) => return outcome,
    };
    if current.as_deref() == Some(name) {
        return fail(
            ctx,
            sender,
            "commands.scoreboard.objectives.display.alreadySet",
            Vec::new(),
        );
    }
    ctx.change_scoreboard(|board| ((), board.set_display(slot, Some(name)).unwrap_or_default()));
    success(
        ctx,
        sender,
        "commands.scoreboard.objectives.display.set",
        vec![text(slot.name()), objective.display_name.clone()],
    );
    CommandOutcome::Success
}

fn objectives_modify<C: CommandContext>(
    ctx: &mut C,
    sender: CommandSender,
    rest: &str,
) -> CommandOutcome {
    let help = "/scoreboard objectives modify <objective> <displayname|rendertype|displayautoupdate|numberformat> ...";
    let Some((name, rest)) = next_argument(rest) else {
        return usage(ctx, sender, help);
    };
    let objective = match find_objective(ctx, sender, name) {
        Ok(objective) => objective,
        Err(outcome) => return outcome,
    };
    let Some((property, rest)) = next_argument(rest) else {
        return usage(ctx, sender, help);
    };
    match property {
        "displayname" => {
            let display = match component_of(ctx, sender, rest.trim()) {
                Ok(display) => display,
                Err(outcome) => return outcome,
            };
            let shown = display.clone();
            ctx.change_scoreboard(|board| {
                (
                    (),
                    board.modify_objective(name, |o| o.display_name = display),
                )
            });
            success(
                ctx,
                sender,
                "commands.scoreboard.objectives.modify.displayname",
                vec![text(name), bracketed(shown)],
            );
            CommandOutcome::Success
        }
        "rendertype" => {
            let Some(render) =
                next_argument(rest).and_then(|(token, _)| RenderType::from_name(token))
            else {
                return usage(
                    ctx,
                    sender,
                    "/scoreboard objectives modify <objective> rendertype <hearts|integer>",
                );
            };
            ctx.change_scoreboard(|board| {
                ((), board.modify_objective(name, |o| o.render_type = render))
            });
            success(
                ctx,
                sender,
                "commands.scoreboard.objectives.modify.rendertype",
                vec![objective_display(&objective)],
            );
            CommandOutcome::Success
        }
        "displayautoupdate" => {
            let Some((raw, _)) = next_argument(rest) else {
                return usage(
                    ctx,
                    sender,
                    "/scoreboard objectives modify <objective> displayautoupdate <value>",
                );
            };
            let enabled = match parse_bool(ctx, sender, raw) {
                Ok(enabled) => enabled,
                Err(outcome) => return outcome,
            };
            if objective.display_auto_update != enabled {
                ctx.change_scoreboard(|board| {
                    (
                        (),
                        board.modify_objective(name, |o| o.display_auto_update = enabled),
                    )
                });
                let key = if enabled {
                    "commands.scoreboard.objectives.modify.displayAutoUpdate.enable"
                } else {
                    "commands.scoreboard.objectives.modify.displayAutoUpdate.disable"
                };
                success(ctx, sender, key, vec![objective_display(&objective)]);
            }
            let result = i32::from(enabled);
            ctx.record_result(result);
            CommandOutcome::Success
        }
        "numberformat" => {
            let format = match number_format_from(ctx, sender, rest) {
                Ok(format) => format,
                Err(outcome) => return outcome,
            };
            let key = if format.is_some() {
                "commands.scoreboard.objectives.modify.objectiveFormat.set"
            } else {
                "commands.scoreboard.objectives.modify.objectiveFormat.clear"
            };
            ctx.change_scoreboard(|board| {
                (
                    (),
                    board.modify_objective(name, |o| o.number_format = format),
                )
            });
            success(ctx, sender, key, vec![objective_display(&objective)]);
            CommandOutcome::Success
        }
        _ => usage(ctx, sender, help),
    }
}

fn players<C: CommandContext>(ctx: &mut C, sender: CommandSender, rest: &str) -> CommandOutcome {
    let help = "/scoreboard players <list|set|get|add|remove|reset|enable|operation|display> ...";
    let Some((action, rest)) = next_argument(rest) else {
        return usage(ctx, sender, help);
    };
    match action {
        "list" => players_list(ctx, sender, rest),
        "get" => players_get(ctx, sender, rest),
        "set" | "add" | "remove" => players_change(ctx, sender, action, rest),
        "reset" => players_reset(ctx, sender, rest),
        "enable" => players_enable(ctx, sender, rest),
        "operation" => players_operation(ctx, sender, rest),
        "display" => players_display(ctx, sender, rest),
        _ => usage(ctx, sender, help),
    }
}

fn players_list<C: CommandContext>(
    ctx: &mut C,
    sender: CommandSender,
    rest: &str,
) -> CommandOutcome {
    let Some((token, _)) = next_argument(rest) else {
        let names: Vec<String> = ctx
            .scoreboard()
            .holders()
            .into_iter()
            .map(str::to_owned)
            .collect();
        ctx.record_result(names.len() as i32);
        if names.is_empty() {
            success(
                ctx,
                sender,
                "commands.scoreboard.players.list.empty",
                Vec::new(),
            );
            return CommandOutcome::Success;
        }
        let count = names.len();
        success(
            ctx,
            sender,
            "commands.scoreboard.players.list.success",
            vec![text(count), joined(names.iter().map(text).collect())],
        );
        return CommandOutcome::Success;
    };
    let holder = match single_holder(ctx, sender, token) {
        Ok(holder) => holder,
        Err(outcome) => return outcome,
    };
    let entries: Vec<(String, i32)> = ctx
        .scoreboard()
        .holder_scores(&holder)
        .into_iter()
        .map(|(objective, score)| (objective.to_owned(), score.value))
        .collect();
    ctx.record_result(entries.len() as i32);
    if entries.is_empty() {
        success(
            ctx,
            sender,
            "commands.scoreboard.players.list.entity.empty",
            vec![text(&holder)],
        );
        return CommandOutcome::Success;
    }
    success(
        ctx,
        sender,
        "commands.scoreboard.players.list.entity.success",
        vec![text(&holder), text(entries.len())],
    );
    for (objective, value) in entries {
        let shown = ctx
            .scoreboard()
            .objective(&objective)
            .map_or_else(|| text(&objective), objective_display);
        success(
            ctx,
            sender,
            "commands.scoreboard.players.list.entity.entry",
            vec![shown, text(value)],
        );
    }
    CommandOutcome::Success
}

fn players_get<C: CommandContext>(
    ctx: &mut C,
    sender: CommandSender,
    rest: &str,
) -> CommandOutcome {
    let help = "/scoreboard players get <target> <objective>";
    let Some((token, rest)) = next_argument(rest) else {
        return usage(ctx, sender, help);
    };
    let Some((name, _)) = next_argument(rest) else {
        return usage(ctx, sender, help);
    };
    let holder = match single_holder(ctx, sender, token) {
        Ok(holder) => holder,
        Err(outcome) => return outcome,
    };
    let objective = match find_objective(ctx, sender, name) {
        Ok(objective) => objective,
        Err(outcome) => return outcome,
    };
    let Some(value) = ctx
        .scoreboard()
        .score(&holder, name)
        .map(|score| score.value)
    else {
        return fail(
            ctx,
            sender,
            "commands.scoreboard.players.get.null",
            vec![objective_display(&objective), text(&holder)],
        );
    };
    ctx.record_result(value);
    success(
        ctx,
        sender,
        "commands.scoreboard.players.get.success",
        vec![text(&holder), text(value), objective_display(&objective)],
    );
    CommandOutcome::Success
}

fn players_change<C: CommandContext>(
    ctx: &mut C,
    sender: CommandSender,
    action: &str,
    rest: &str,
) -> CommandOutcome {
    let help = format!("/scoreboard players {action} <targets> <objective> <score>");
    let Some((token, rest)) = next_argument(rest) else {
        return usage(ctx, sender, &help);
    };
    let Some((name, rest)) = next_argument(rest) else {
        return usage(ctx, sender, &help);
    };
    let Some((raw, _)) = next_argument(rest) else {
        return usage(ctx, sender, &help);
    };
    let targets = match holders(ctx, sender, token) {
        Ok(targets) => targets,
        Err(outcome) => return outcome,
    };
    let objective = match writable_objective(ctx, sender, name) {
        Ok(objective) => objective,
        Err(outcome) => return outcome,
    };
    let min = (action != "set").then_some(0);
    let amount = match parse_int(ctx, sender, raw, min) {
        Ok(amount) => amount,
        Err(outcome) => return outcome,
    };
    let displays: Vec<Option<Component>> = targets
        .iter()
        .map(|holder| ctx.holder_display_name(holder))
        .collect();
    let finals: Vec<i32> = ctx.change_scoreboard(|board| {
        let mut messages = Vec::new();
        let mut finals = Vec::new();
        for (holder, display) in targets.iter().zip(&displays) {
            let current = board.score(holder, name).map_or(0, |score| score.value);
            let next = match action {
                "set" => amount,
                "add" => current.wrapping_add(amount),
                _ => current.wrapping_sub(amount),
            };
            if let Ok(sent) = board.set_score(holder, name, next, display.clone()) {
                messages.extend(sent);
            }
            finals.push(next);
        }
        (finals, messages)
    });
    let total = finals
        .iter()
        .fold(0i32, |sum, value| sum.wrapping_add(*value));
    let result = if action == "set" {
        amount.wrapping_mul(targets.len() as i32)
    } else {
        total
    };
    ctx.record_result(result);
    let shown = objective_display(&objective);
    let single = targets.len() == 1;
    let (key, args) = match (action, single) {
        ("set", true) => (
            "commands.scoreboard.players.set.success.single",
            vec![shown, text(&targets[0]), text(amount)],
        ),
        ("set", false) => (
            "commands.scoreboard.players.set.success.multiple",
            vec![shown, text(targets.len()), text(amount)],
        ),
        ("add", true) => (
            "commands.scoreboard.players.add.success.single",
            vec![text(amount), shown, text(&targets[0]), text(finals[0])],
        ),
        ("add", false) => (
            "commands.scoreboard.players.add.success.multiple",
            vec![text(amount), shown, text(targets.len())],
        ),
        (_, true) => (
            "commands.scoreboard.players.remove.success.single",
            vec![text(amount), shown, text(&targets[0]), text(finals[0])],
        ),
        (_, false) => (
            "commands.scoreboard.players.remove.success.multiple",
            vec![text(amount), shown, text(targets.len())],
        ),
    };
    success(ctx, sender, key, args);
    CommandOutcome::Success
}

fn players_reset<C: CommandContext>(
    ctx: &mut C,
    sender: CommandSender,
    rest: &str,
) -> CommandOutcome {
    let Some((token, rest)) = next_argument(rest) else {
        return usage(
            ctx,
            sender,
            "/scoreboard players reset <targets> [<objective>]",
        );
    };
    let targets = match holders(ctx, sender, token) {
        Ok(targets) => targets,
        Err(outcome) => return outcome,
    };
    let objective = match next_argument(rest) {
        Some((name, _)) => match find_objective(ctx, sender, name) {
            Ok(objective) => Some(objective),
            Err(outcome) => return outcome,
        },
        None => None,
    };
    ctx.change_scoreboard(|board| {
        let mut messages = Vec::new();
        for holder in &targets {
            match &objective {
                Some(objective) => messages.extend(board.reset_score(holder, &objective.name)),
                None => messages.extend(board.reset_holder(holder)),
            }
        }
        ((), messages)
    });
    ctx.record_result(targets.len() as i32);
    let single = targets.len() == 1;
    let (key, args) = match (&objective, single) {
        (Some(objective), true) => (
            "commands.scoreboard.players.reset.specific.single",
            vec![objective_display(objective), text(&targets[0])],
        ),
        (Some(objective), false) => (
            "commands.scoreboard.players.reset.specific.multiple",
            vec![objective_display(objective), text(targets.len())],
        ),
        (None, true) => (
            "commands.scoreboard.players.reset.all.single",
            vec![text(&targets[0])],
        ),
        (None, false) => (
            "commands.scoreboard.players.reset.all.multiple",
            vec![text(targets.len())],
        ),
    };
    success(ctx, sender, key, args);
    CommandOutcome::Success
}

fn players_enable<C: CommandContext>(
    ctx: &mut C,
    sender: CommandSender,
    rest: &str,
) -> CommandOutcome {
    let help = "/scoreboard players enable <targets> <objective>";
    let Some((token, rest)) = next_argument(rest) else {
        return usage(ctx, sender, help);
    };
    let Some((name, _)) = next_argument(rest) else {
        return usage(ctx, sender, help);
    };
    let targets = match holders(ctx, sender, token) {
        Ok(targets) => targets,
        Err(outcome) => return outcome,
    };
    let objective = match find_objective(ctx, sender, name) {
        Ok(objective) => objective,
        Err(outcome) => return outcome,
    };
    if objective.criteria != Criteria::Trigger {
        return fail(
            ctx,
            sender,
            "commands.scoreboard.players.enable.invalid",
            Vec::new(),
        );
    }
    let changed: usize = ctx.change_scoreboard(|board| {
        let mut messages = Vec::new();
        let mut changed = 0;
        for holder in &targets {
            let was_locked = board.score(holder, name).is_none_or(|score| score.locked);
            if was_locked {
                if let Ok(sent) = board.set_locked(holder, name, false) {
                    messages.extend(sent);
                }
                changed += 1;
            }
        }
        (changed, messages)
    });
    if changed == 0 {
        return fail(
            ctx,
            sender,
            "commands.scoreboard.players.enable.failed",
            Vec::new(),
        );
    }
    ctx.record_result(changed as i32);
    let shown = objective_display(&objective);
    if targets.len() == 1 {
        success(
            ctx,
            sender,
            "commands.scoreboard.players.enable.success.single",
            vec![shown, text(&targets[0])],
        );
    } else {
        success(
            ctx,
            sender,
            "commands.scoreboard.players.enable.success.multiple",
            vec![shown, text(targets.len())],
        );
    }
    CommandOutcome::Success
}

fn apply_operation(operation: &str, target: i32, source: i32) -> Option<Result<(i32, i32), ()>> {
    Some(match operation {
        "=" => Ok((source, source)),
        "+=" => Ok((target.wrapping_add(source), source)),
        "-=" => Ok((target.wrapping_sub(source), source)),
        "*=" => Ok((target.wrapping_mul(source), source)),
        "/=" => {
            if source == 0 {
                Err(())
            } else {
                Ok((floor_div(target, source), source))
            }
        }
        "%=" => {
            if source == 0 {
                Err(())
            } else {
                Ok((floor_mod(target, source), source))
            }
        }
        "<" => Ok((target.min(source), source)),
        ">" => Ok((target.max(source), source)),
        "><" => Ok((source, target)),
        _ => return None,
    })
}

fn floor_div(value: i32, divisor: i32) -> i32 {
    let quotient = value.wrapping_div(divisor);
    if value.wrapping_rem(divisor) != 0 && ((value < 0) != (divisor < 0)) {
        quotient.wrapping_sub(1)
    } else {
        quotient
    }
}

fn floor_mod(value: i32, divisor: i32) -> i32 {
    value.wrapping_sub(floor_div(value, divisor).wrapping_mul(divisor))
}

fn players_operation<C: CommandContext>(
    ctx: &mut C,
    sender: CommandSender,
    rest: &str,
) -> CommandOutcome {
    let help = "/scoreboard players operation <targets> <targetObjective> <operation> <source> <sourceObjective>";
    let Some((token, rest)) = next_argument(rest) else {
        return usage(ctx, sender, help);
    };
    let Some((target_name, rest)) = next_argument(rest) else {
        return usage(ctx, sender, help);
    };
    let Some((operation, rest)) = next_argument(rest) else {
        return usage(ctx, sender, help);
    };
    let Some((source_token, rest)) = next_argument(rest) else {
        return usage(ctx, sender, help);
    };
    let Some((source_name, _)) = next_argument(rest) else {
        return usage(ctx, sender, help);
    };
    let targets = match holders(ctx, sender, token) {
        Ok(targets) => targets,
        Err(outcome) => return outcome,
    };
    let objective = match writable_objective(ctx, sender, target_name) {
        Ok(objective) => objective,
        Err(outcome) => return outcome,
    };
    if apply_operation(operation, 0, 0).is_none() {
        return fail(ctx, sender, "arguments.operation.invalid", Vec::new());
    }
    let sources = match holders(ctx, sender, source_token) {
        Ok(sources) => sources,
        Err(outcome) => return outcome,
    };
    if let Err(outcome) = find_objective(ctx, sender, source_name) {
        return outcome;
    }
    let displays: Vec<Option<Component>> = targets
        .iter()
        .map(|holder| ctx.holder_display_name(holder))
        .collect();
    let outcome: Result<Vec<i32>, ()> = ctx.change_scoreboard(|board| {
        let mut messages: Vec<ScoreboardMessage> = Vec::new();
        let mut finals = Vec::new();
        for (target, display) in targets.iter().zip(&displays) {
            for source in &sources {
                let a = board.score(target, target_name).map_or(0, |s| s.value);
                let b = board.score(source, source_name).map_or(0, |s| s.value);
                let Some(Ok((new_a, new_b))) = apply_operation(operation, a, b) else {
                    return (Err(()), messages);
                };
                if let Ok(sent) = board.set_score(target, target_name, new_a, display.clone()) {
                    messages.extend(sent);
                }
                if let Ok(sent) = board.set_score(source, source_name, new_b, None) {
                    messages.extend(sent);
                }
            }
            finals.push(board.score(target, target_name).map_or(0, |s| s.value));
        }
        (Ok(finals), messages)
    });
    let Ok(finals) = outcome else {
        return fail(ctx, sender, "arguments.operation.div0", Vec::new());
    };
    let total = finals
        .iter()
        .fold(0i32, |sum, value| sum.wrapping_add(*value));
    ctx.record_result(total);
    let shown = objective_display(&objective);
    if targets.len() == 1 {
        success(
            ctx,
            sender,
            "commands.scoreboard.players.operation.success.single",
            vec![shown, text(&targets[0]), text(finals[0])],
        );
    } else {
        success(
            ctx,
            sender,
            "commands.scoreboard.players.operation.success.multiple",
            vec![shown, text(targets.len())],
        );
    }
    CommandOutcome::Success
}

fn players_display<C: CommandContext>(
    ctx: &mut C,
    sender: CommandSender,
    rest: &str,
) -> CommandOutcome {
    let help = "/scoreboard players display <name|numberformat> <targets> <objective> ...";
    let Some((property, rest)) = next_argument(rest) else {
        return usage(ctx, sender, help);
    };
    if property != "name" && property != "numberformat" {
        return usage(ctx, sender, help);
    }
    let Some((token, rest)) = next_argument(rest) else {
        return usage(ctx, sender, help);
    };
    let Some((name, rest)) = next_argument(rest) else {
        return usage(ctx, sender, help);
    };
    let targets = match holders(ctx, sender, token) {
        Ok(targets) => targets,
        Err(outcome) => return outcome,
    };
    let objective = match find_objective(ctx, sender, name) {
        Ok(objective) => objective,
        Err(outcome) => return outcome,
    };
    let shown = objective_display(&objective);
    let single = targets.len() == 1;
    if property == "name" {
        let display = if rest.trim().is_empty() {
            None
        } else {
            match component_of(ctx, sender, rest.trim()) {
                Ok(display) => Some(display),
                Err(outcome) => return outcome,
            }
        };
        let set = display.clone();
        ctx.change_scoreboard(|board| {
            let mut messages = Vec::new();
            for holder in &targets {
                if let Ok(sent) = board.set_score_display(holder, name, set.clone()) {
                    messages.extend(sent);
                }
            }
            ((), messages)
        });
        ctx.record_result(targets.len() as i32);
        let (key, args) = match (display, single) {
            (Some(display), true) => (
                "commands.scoreboard.players.display.name.set.success.single",
                vec![display, text(&targets[0]), shown],
            ),
            (Some(display), false) => (
                "commands.scoreboard.players.display.name.set.success.multiple",
                vec![display, text(targets.len()), shown],
            ),
            (None, true) => (
                "commands.scoreboard.players.display.name.clear.success.single",
                vec![text(&targets[0]), shown],
            ),
            (None, false) => (
                "commands.scoreboard.players.display.name.clear.success.multiple",
                vec![text(targets.len()), shown],
            ),
        };
        success(ctx, sender, key, args);
        return CommandOutcome::Success;
    }
    let format = match number_format_from(ctx, sender, rest) {
        Ok(format) => format,
        Err(outcome) => return outcome,
    };
    let is_set = format.is_some();
    ctx.change_scoreboard(|board| {
        let mut messages = Vec::new();
        for holder in &targets {
            if let Ok(sent) = board.set_score_number_format(holder, name, format.clone()) {
                messages.extend(sent);
            }
        }
        ((), messages)
    });
    ctx.record_result(targets.len() as i32);
    let (key, args) = match (is_set, single) {
        (true, true) => (
            "commands.scoreboard.players.display.numberFormat.set.success.single",
            vec![text(&targets[0]), shown],
        ),
        (true, false) => (
            "commands.scoreboard.players.display.numberFormat.set.success.multiple",
            vec![text(targets.len()), shown],
        ),
        (false, true) => (
            "commands.scoreboard.players.display.numberFormat.clear.success.single",
            vec![text(&targets[0]), shown],
        ),
        (false, false) => (
            "commands.scoreboard.players.display.numberFormat.clear.success.multiple",
            vec![text(targets.len()), shown],
        ),
    };
    success(ctx, sender, key, args);
    CommandOutcome::Success
}

fn trigger<C: CommandContext>(ctx: &mut C, sender: CommandSender, rest: &str) -> CommandOutcome {
    let help = "/trigger <objective> [add|set <value>]";
    let CommandSender::Player(player) = sender else {
        return fail(ctx, sender, "permissions.requires.player", Vec::new());
    };
    let Some((name, rest)) = next_argument(rest) else {
        return usage(ctx, sender, help);
    };
    let objective = match find_objective(ctx, sender, name) {
        Ok(objective) => objective,
        Err(outcome) => return outcome,
    };
    let (mode, amount) = match next_argument(rest) {
        None => ("simple", 1),
        Some((mode @ ("add" | "set"), rest)) => {
            let Some((raw, _)) = next_argument(rest) else {
                return usage(ctx, sender, help);
            };
            match parse_int(ctx, sender, raw, None) {
                Ok(amount) => (mode, amount),
                Err(outcome) => return outcome,
            }
        }
        Some(_) => return usage(ctx, sender, help),
    };
    if objective.criteria != Criteria::Trigger {
        return fail(ctx, sender, "commands.trigger.failed.invalid", Vec::new());
    }
    let holder = ctx.player_name(player).unwrap_or_default();
    let primed = ctx
        .scoreboard()
        .score(&holder, name)
        .is_some_and(|score| !score.locked);
    if !primed {
        return fail(ctx, sender, "commands.trigger.failed.unprimed", Vec::new());
    }
    let display = ctx.holder_display_name(&holder);
    let value = ctx.change_scoreboard(|board| {
        let mut messages = board.set_locked(&holder, name, true).unwrap_or_default();
        let current = board.score(&holder, name).map_or(0, |score| score.value);
        let next = if mode == "set" {
            amount
        } else {
            current.wrapping_add(amount)
        };
        if let Ok(sent) = board.set_score(&holder, name, next, display) {
            messages.extend(sent);
        }
        (next, messages)
    });
    ctx.record_result(value);
    let shown = objective_display(&objective);
    let (key, args) = match mode {
        "add" => ("commands.trigger.add.success", vec![shown, text(amount)]),
        "set" => ("commands.trigger.set.success", vec![shown, text(amount)]),
        _ => ("commands.trigger.simple.success", vec![shown]),
    };
    success(ctx, sender, key, args);
    CommandOutcome::Success
}

use std::sync::Arc;

use rspigot_domain::Identifier;
use rspigot_domain::text::{Component, NamedColor};
use rspigot_nbt::{Compound, Tag};

use super::data::{self, DataSource};
use super::dispatch::{
    CommandContext, CommandOutcome, CommandSender, execute, fail, next_argument, parse_time, usage,
};
use super::source::Source;
use crate::functions::{
    Callback, Function, InstantiationError, MAX_NESTING, PackInfo, PackProblem, PackSource, Packs,
    Placement, Returned, ScheduledEvent, VANILLA_PACK,
};

const FUNCTION_USAGE: &str = "/function <name> [arguments]";
const SCHEDULE_USAGE: &str = "/schedule <function|clear> ...";
const DATAPACK_USAGE: &str = "/datapack <enable|disable|list> ...";
const GAMEMASTER_CAP: u8 = 2;
const LIMIT_RULE: &str = "max_command_sequence_length";

fn text(value: impl ToString) -> Component {
    Component::text(value.to_string())
}

fn red<C: CommandContext>(
    ctx: &mut C,
    sender: CommandSender,
    message: Component,
) -> CommandOutcome {
    ctx.reply(sender, message.color(NamedColor::Red));
    CommandOutcome::Failure
}

pub(super) fn run<C: CommandContext>(
    ctx: &mut C,
    sender: CommandSender,
    label: &str,
    rest: &str,
) -> CommandOutcome {
    match label {
        "function" => function(ctx, sender, rest),
        "schedule" => schedule(ctx, sender, rest),
        "return" => returning(ctx, sender, rest),
        "datapack" => datapack(ctx, sender, rest),
        _ => CommandOutcome::Unknown,
    }
}

enum Target {
    Function(Arc<Function>),
    Tag(Identifier, Vec<Arc<Function>>),
}

fn parse_target<C: CommandContext>(ctx: &C, token: &str) -> Result<Target, Component> {
    let (name, is_tag) = match token.strip_prefix('#') {
        Some(name) => (name, true),
        None => (token, false),
    };
    let id = Identifier::parse(name)
        .map_err(|_| Component::translatable("argument.id.invalid", Vec::new()))?;
    let library = &ctx.function_state().library;
    if is_tag {
        library
            .tag(&id)
            .map(|members| Target::Tag(id.clone(), members.to_vec()))
            .ok_or_else(|| {
                Component::translatable("arguments.function.tag.unknown", vec![text(&id)])
            })
    } else {
        library
            .function(&id)
            .map(|found| Target::Function(Arc::clone(found)))
            .ok_or_else(|| Component::translatable("arguments.function.unknown", vec![text(&id)]))
    }
}

fn function<C: CommandContext>(ctx: &mut C, sender: CommandSender, rest: &str) -> CommandOutcome {
    let Some((token, rest)) = next_argument(rest) else {
        return usage(ctx, sender, FUNCTION_USAGE);
    };
    let target = match parse_target(ctx, token) {
        Ok(target) => target,
        Err(message) => return red(ctx, sender, message),
    };
    let arguments = match parse_arguments(ctx, sender, rest) {
        Ok(arguments) => arguments,
        Err(outcome) => return outcome,
    };
    let functions = match target {
        Target::Function(found) => vec![found],
        Target::Tag(id, members) => {
            if members.is_empty() {
                return fail(
                    ctx,
                    sender,
                    "commands.function.scheduled.no_functions",
                    vec![text(&id)],
                );
            }
            members
        }
    };
    run_functions(ctx, sender, &functions, arguments.as_ref())
}

fn parse_arguments<C: CommandContext>(
    ctx: &mut C,
    sender: CommandSender,
    rest: &str,
) -> Result<Option<Compound>, CommandOutcome> {
    let rest = rest.trim();
    if rest.is_empty() {
        return Ok(None);
    }
    if rest.starts_with('{') {
        return match rspigot_nbt::parse_compound(rest) {
            Ok((compound, used)) if rest[used..].trim().is_empty() => Ok(Some(compound)),
            Ok(_) => Err(usage(ctx, sender, FUNCTION_USAGE)),
            Err(error) => Err(fail(ctx, sender, error.key, Vec::new())),
        };
    }
    let Some(("with", rest)) = next_argument(rest) else {
        return Err(usage(ctx, sender, FUNCTION_USAGE));
    };
    let DataSource { data, path } = data::read_source(ctx, sender, rest)?;
    let mut found = match &path {
        Some(path) => path
            .get(&Tag::Compound(data))
            .into_iter()
            .cloned()
            .collect(),
        None => vec![Tag::Compound(data)],
    };
    match found.len() {
        0 => Err(fail(
            ctx,
            sender,
            "commands.data.get.unknown",
            vec![text(
                path.map_or_else(String::new, |p| p.source().to_owned()),
            )],
        )),
        1 => match found.remove(0) {
            Tag::Compound(compound) => Ok(Some(compound)),
            other => Err(fail(
                ctx,
                sender,
                "commands.function.error.argument_not_compound",
                vec![text(format!("{:?}", other.tag_type()))],
            )),
        },
        _ => Err(fail(
            ctx,
            sender,
            "commands.function.error.argument_not_compound",
            vec![text("List")],
        )),
    }
}

fn instantiation_failure(id: &Identifier, error: &InstantiationError) -> Component {
    let reason = match error {
        InstantiationError::MissingArguments => {
            Component::translatable("commands.function.error.missing_arguments", vec![text(id)])
        }
        InstantiationError::MissingArgument(name) => Component::translatable(
            "commands.function.error.missing_argument",
            vec![text(id), text(name)],
        ),
    };
    Component::translatable(
        "commands.function.instantiationFailure",
        vec![text(id), reason],
    )
}

fn chain_limit<C: CommandContext>(ctx: &C) -> u64 {
    ctx.game_rule_text(LIMIT_RULE)
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(crate::functions::DEFAULT_COMMAND_LIMIT)
}

pub(crate) fn invoke<C: CommandContext>(
    ctx: &mut C,
    sender: CommandSender,
    function: &Function,
    arguments: Option<&Compound>,
    cap: u8,
) -> Result<Option<Returned>, Component> {
    let commands = function
        .commands(arguments)
        .map_err(|error| instantiation_failure(function.id(), &error))?;
    let limit = chain_limit(ctx);
    let runtime = &mut ctx.function_state_mut().runtime;
    if runtime.depth >= MAX_NESTING {
        tracing::error!(
            "Function {} is nested more than {MAX_NESTING} levels deep, stopping",
            function.id()
        );
        runtime.aborted = true;
        return Ok(Some(Returned::Failure));
    }
    if runtime.depth == 0 {
        runtime.executed = 0;
        runtime.aborted = false;
        runtime.returned = None;
        runtime.limit = limit;
    }
    runtime.depth += 1;
    let saved = (runtime.silent, runtime.permission_cap);
    runtime.silent = true;
    runtime.permission_cap = Some(saved.1.map_or(cap, |previous| previous.min(cap)));
    let mut returned = None;
    for command in &commands {
        let runtime = &mut ctx.function_state_mut().runtime;
        if runtime.aborted {
            break;
        }
        if runtime.executed >= runtime.limit {
            runtime.aborted = true;
            tracing::info!(
                "Command execution stopped due to limit (executed {} commands)",
                runtime.executed
            );
            break;
        }
        runtime.executed += 1;
        let outcome = execute(ctx, sender, command);
        if outcome == CommandOutcome::Unknown || outcome == CommandOutcome::Denied {
            tracing::debug!("Function {}: cannot run '{command}'", function.id());
        }
        if let Some(value) = ctx.function_state_mut().runtime.returned.take() {
            returned = Some(value);
            break;
        }
    }
    let runtime = &mut ctx.function_state_mut().runtime;
    runtime.depth -= 1;
    runtime.silent = saved.0;
    runtime.permission_cap = saved.1;
    Ok(returned)
}

fn run_functions<C: CommandContext>(
    ctx: &mut C,
    sender: CommandSender,
    functions: &[Arc<Function>],
    arguments: Option<&Compound>,
) -> CommandOutcome {
    let quiet = ctx.function_state().runtime.silent;
    if !quiet {
        let message = match functions {
            [only] => {
                Component::translatable("commands.function.scheduled.single", vec![text(only.id())])
            }
            many => {
                let mut list = Component::text("");
                for (index, function) in many.iter().enumerate() {
                    if index > 0 {
                        list = list.append(text(", "));
                    }
                    list = list.append(text(function.id()));
                }
                Component::translatable("commands.function.scheduled.multiple", vec![list])
            }
        };
        ctx.reply(sender, message);
    }
    let mut sum: Option<i32> = None;
    let mut failures = 0;
    for function in functions {
        let returned = match invoke(ctx, sender, function, arguments, GAMEMASTER_CAP) {
            Ok(returned) => returned,
            Err(message) => return red(ctx, sender, message),
        };
        let value = match returned {
            Some(Returned::Value(value)) => value,
            Some(Returned::Failure) => {
                failures += 1;
                0
            }
            None => continue,
        };
        if !quiet {
            ctx.reply(
                sender,
                Component::translatable(
                    "commands.function.result",
                    vec![text(function.id()), text(value)],
                ),
            );
        }
        if returned != Some(Returned::Failure) {
            sum = Some(sum.unwrap_or(0).wrapping_add(value));
        }
    }
    match sum {
        Some(total) => {
            ctx.record_result(total);
            CommandOutcome::Success
        }
        None if failures > 0 => {
            ctx.record_result(0);
            CommandOutcome::Failure
        }
        None => CommandOutcome::Success,
    }
}

fn run_as_server<C: CommandContext>(ctx: &mut C, functions: &[Arc<Function>]) {
    let cap = ctx.server_function_permission();
    let source: Source = ctx.server_function_source();
    for function in functions {
        let mut problem = None;
        ctx.run_with_source(&source, |ctx| {
            if let Err(message) = invoke(ctx, CommandSender::Console, function, None, cap) {
                problem = Some(message.plain_text());
            }
        });
        if let Some(problem) = problem {
            tracing::warn!("Could not run function {}: {problem}", function.id());
        }
    }
}

pub(super) fn probe<C: CommandContext>(
    ctx: &mut C,
    sender: CommandSender,
    id: &Identifier,
) -> Result<i32, Component> {
    let Some(function) = ctx.function_state().library.function(id).map(Arc::clone) else {
        return Err(Component::translatable(
            "arguments.function.unknown",
            vec![text(id)],
        ));
    };
    let returned = invoke(ctx, sender, &function, None, GAMEMASTER_CAP)?;
    Ok(match returned {
        Some(Returned::Value(value)) => i32::from(value != 0),
        _ => 0,
    })
}

pub(crate) fn run_server_tag<C: CommandContext>(ctx: &mut C, tag: &Identifier) {
    let Some(members) = ctx
        .function_state()
        .library
        .tag(tag)
        .map(<[Arc<Function>]>::to_vec)
    else {
        return;
    };
    run_as_server(ctx, &members);
}

pub(crate) fn run_scheduled<C: CommandContext>(ctx: &mut C, event: &ScheduledEvent) {
    let library = &ctx.function_state().library;
    let members = match &event.callback {
        Callback::Function(id) => library.function(id).map(|found| vec![Arc::clone(found)]),
        Callback::Tag(id) => library.tag(id).map(<[Arc<Function>]>::to_vec),
    };
    if let Some(members) = members {
        run_as_server(ctx, &members);
    }
}

fn returning<C: CommandContext>(ctx: &mut C, sender: CommandSender, rest: &str) -> CommandOutcome {
    let Some((first, rest)) = next_argument(rest) else {
        return usage(ctx, sender, "/return <value|fail|run>");
    };
    match first {
        "fail" => {
            give_back(ctx, Returned::Failure);
            ctx.record_result(0);
            CommandOutcome::Failure
        }
        "run" => {
            let command = rest.trim();
            if command.is_empty() {
                return usage(ctx, sender, "/return run <command>");
            }
            ctx.take_result();
            let outcome = execute(ctx, sender, command);
            if outcome == CommandOutcome::Success {
                let value = ctx.take_result().unwrap_or(1);
                give_back(ctx, Returned::Value(value));
                ctx.record_result(value);
                CommandOutcome::Success
            } else {
                give_back(ctx, Returned::Failure);
                ctx.record_result(0);
                CommandOutcome::Failure
            }
        }
        number => match number.parse::<i32>() {
            Ok(value) => {
                give_back(ctx, Returned::Value(value));
                ctx.record_result(value);
                CommandOutcome::Success
            }
            Err(_) => fail(ctx, sender, "parsing.int.invalid", vec![text(number)]),
        },
    }
}

fn give_back<C: CommandContext>(ctx: &mut C, value: Returned) {
    let runtime = &mut ctx.function_state_mut().runtime;
    if runtime.depth > 0 && runtime.returned.is_none() {
        runtime.returned = Some(value);
    }
}

fn schedule<C: CommandContext>(ctx: &mut C, sender: CommandSender, rest: &str) -> CommandOutcome {
    let Some((action, rest)) = next_argument(rest) else {
        return usage(ctx, sender, SCHEDULE_USAGE);
    };
    match action {
        "function" => schedule_function(ctx, sender, rest),
        "clear" => schedule_clear(ctx, sender, rest),
        _ => usage(ctx, sender, SCHEDULE_USAGE),
    }
}

fn schedule_function<C: CommandContext>(
    ctx: &mut C,
    sender: CommandSender,
    rest: &str,
) -> CommandOutcome {
    const USAGE: &str = "/schedule function <function> <time> [append|replace]";
    let Some((token, rest)) = next_argument(rest) else {
        return usage(ctx, sender, USAGE);
    };
    let Some((time, rest)) = next_argument(rest) else {
        return usage(ctx, sender, USAGE);
    };
    let append = match next_argument(rest) {
        None => false,
        Some(("append", _)) => true,
        Some(("replace", _)) => false,
        Some(_) => return usage(ctx, sender, USAGE),
    };
    let target = match parse_target(ctx, token) {
        Ok(target) => target,
        Err(message) => return red(ctx, sender, message),
    };
    let Some(delay) = parse_time(time) else {
        return fail(ctx, sender, "argument.time.invalid_unit", Vec::new());
    };
    if delay == 0 {
        return fail(ctx, sender, "commands.schedule.same_tick", Vec::new());
    }
    let (name, callback, key, shown) = match target {
        Target::Function(found) => {
            if found.is_macro() {
                return fail(ctx, sender, "commands.schedule.macro", Vec::new());
            }
            let id = found.id().clone();
            (
                id.to_string(),
                Callback::Function(id.clone()),
                "commands.schedule.created.function",
                id,
            )
        }
        Target::Tag(id, _) => (
            format!("#{id}"),
            Callback::Tag(id.clone()),
            "commands.schedule.created.tag",
            id,
        ),
    };
    let trigger_time = ctx.game_time().saturating_add(delay);
    ctx.function_state_mut().scheduled.add(
        ScheduledEvent {
            name,
            trigger_time,
            callback,
        },
        !append,
    );
    ctx.reply(
        sender,
        Component::translatable(key, vec![text(&shown), text(delay), text(trigger_time)]),
    );
    ctx.record_result(trigger_time.rem_euclid(i64::from(i32::MAX)) as i32);
    CommandOutcome::Success
}

fn schedule_clear<C: CommandContext>(
    ctx: &mut C,
    sender: CommandSender,
    rest: &str,
) -> CommandOutcome {
    let Some((token, _)) = next_argument(rest) else {
        return usage(ctx, sender, "/schedule clear <function>");
    };
    let name = match token.strip_prefix('#') {
        Some(tag) => Identifier::parse(tag).map(|id| format!("#{id}")),
        None => Identifier::parse(token).map(|id| id.to_string()),
    };
    let Ok(name) = name else {
        return fail(ctx, sender, "argument.id.invalid", Vec::new());
    };
    let removed = ctx.function_state_mut().scheduled.remove(&name);
    if removed == 0 {
        return fail(
            ctx,
            sender,
            "commands.schedule.cleared.failure",
            vec![text(&name)],
        );
    }
    ctx.reply(
        sender,
        Component::translatable(
            "commands.schedule.cleared.success",
            vec![text(removed), text(&name)],
        ),
    );
    ctx.record_result(i32::try_from(removed).unwrap_or(i32::MAX));
    CommandOutcome::Success
}

fn unquote(token: &str) -> String {
    let inner = token
        .strip_prefix('"')
        .and_then(|rest| rest.strip_suffix('"'))
        .or_else(|| {
            token
                .strip_prefix('\'')
                .and_then(|rest| rest.strip_suffix('\''))
        })
        .unwrap_or(token);
    let mut out = String::with_capacity(inner.len());
    let mut escaped = false;
    for c in inner.chars() {
        if escaped || c != '\\' {
            out.push(c);
            escaped = false;
        } else {
            escaped = true;
        }
    }
    out
}

fn pack_link(pack: &PackInfo) -> Component {
    let title = if pack.id == VANILLA_PACK {
        Component::translatable("dataPack.vanilla.name", Vec::new())
    } else {
        match pack.source {
            PackSource::World => Component::translatable(
                "pack.nameAndSource",
                vec![
                    text(&pack.id),
                    Component::translatable("pack.source.world", Vec::new()),
                ],
            ),
            PackSource::BuiltIn => text(&pack.id),
        }
    };
    text("[").append(title).append(text("]"))
}

fn pack_list(packs: &[&PackInfo]) -> Component {
    let mut list = Component::text("");
    for (index, pack) in packs.iter().enumerate() {
        if index > 0 {
            list = list.append(text(", "));
        }
        list = list.append(pack_link(pack));
    }
    list
}

fn datapack<C: CommandContext>(ctx: &mut C, sender: CommandSender, rest: &str) -> CommandOutcome {
    let Some((action, rest)) = next_argument(rest) else {
        return usage(ctx, sender, DATAPACK_USAGE);
    };
    match action {
        "list" => match next_argument(rest) {
            None => {
                list_enabled(ctx, sender);
                list_available(ctx, sender)
            }
            Some(("enabled", _)) => list_enabled(ctx, sender),
            Some(("available", _)) => list_available(ctx, sender),
            Some(_) => usage(ctx, sender, "/datapack list [available|enabled]"),
        },
        "enable" => {
            let Some((name, rest)) = next_argument(rest) else {
                return usage(
                    ctx,
                    sender,
                    "/datapack enable <name> [first|last|before|after]",
                );
            };
            let placement = match next_argument(rest) {
                None | Some(("last", _)) => Placement::Last,
                Some(("first", _)) => Placement::First,
                Some((mode @ ("before" | "after"), rest)) => {
                    let Some((other, _)) = next_argument(rest) else {
                        return usage(
                            ctx,
                            sender,
                            "/datapack enable <name> before|after <existing>",
                        );
                    };
                    let other = unquote(other);
                    if mode == "before" {
                        Placement::Before(other)
                    } else {
                        Placement::After(other)
                    }
                }
                Some(_) => {
                    return usage(
                        ctx,
                        sender,
                        "/datapack enable <name> [first|last|before|after]",
                    );
                }
            };
            change(ctx, sender, &unquote(name), true, |packs, id| {
                packs.enable(id, &placement)
            })
        }
        "disable" => {
            let Some((name, _)) = next_argument(rest) else {
                return usage(ctx, sender, "/datapack disable <name>");
            };
            change(ctx, sender, &unquote(name), false, |packs, id| {
                packs.disable(id)
            })
        }
        _ => usage(ctx, sender, DATAPACK_USAGE),
    }
}

fn list_enabled<C: CommandContext>(ctx: &mut C, sender: CommandSender) -> CommandOutcome {
    let packs = &ctx.function_state().packs;
    let shown: Vec<&PackInfo> = packs
        .enabled()
        .iter()
        .filter_map(|id| packs.info(id))
        .collect();
    let message = if shown.is_empty() {
        Component::translatable("commands.datapack.list.enabled.none", Vec::new())
    } else {
        Component::translatable(
            "commands.datapack.list.enabled.success",
            vec![text(shown.len()), pack_list(&shown)],
        )
    };
    let count = shown.len();
    ctx.reply(sender, message);
    ctx.record_result(i32::try_from(count).unwrap_or(i32::MAX));
    CommandOutcome::Success
}

fn list_available<C: CommandContext>(ctx: &mut C, sender: CommandSender) -> CommandOutcome {
    let shown: Vec<&PackInfo> = ctx.function_state().packs.disabled().collect();
    let message = if shown.is_empty() {
        Component::translatable("commands.datapack.list.available.none", Vec::new())
    } else {
        Component::translatable(
            "commands.datapack.list.available.success",
            vec![text(shown.len()), pack_list(&shown)],
        )
    };
    let count = shown.len();
    ctx.reply(sender, message);
    ctx.record_result(i32::try_from(count).unwrap_or(i32::MAX));
    CommandOutcome::Success
}

fn change<C: CommandContext>(
    ctx: &mut C,
    sender: CommandSender,
    id: &str,
    enabling: bool,
    apply: impl FnOnce(&mut Packs, &str) -> Result<(), PackProblem>,
) -> CommandOutcome {
    let mut packs = ctx.function_state().packs.clone();
    if let Err(problem) = apply(&mut packs, id) {
        return match problem {
            PackProblem::Unknown(name) => {
                fail(ctx, sender, "commands.datapack.unknown", vec![text(name)])
            }
            PackProblem::AlreadyEnabled(name) => fail(
                ctx,
                sender,
                "commands.datapack.enable.failed",
                vec![text(name)],
            ),
            PackProblem::NotEnabled(name) => fail(
                ctx,
                sender,
                "commands.datapack.disable.failed",
                vec![text(name)],
            ),
        };
    }
    let link = packs.info(id).map_or_else(|| text(id), pack_link);
    ctx.apply_packs(packs);
    let key = if enabling {
        "commands.datapack.modify.enable"
    } else {
        "commands.datapack.modify.disable"
    };
    ctx.reply(sender, Component::translatable(key, vec![link]));
    ctx.record_result(1);
    CommandOutcome::Success
}

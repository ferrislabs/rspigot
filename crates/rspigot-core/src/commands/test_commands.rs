use rspigot_domain::text::Component;

use super::dispatch::{CommandContext, CommandOutcome, CommandSender, fail, next_argument, usage};
use crate::pack_data::registry_overlay;

const REGISTRY: &str = "minecraft:test_instance";
const BUILT_IN: [&str; 1] = ["minecraft:always_pass"];

fn text(value: impl ToString) -> Component {
    Component::text(value.to_string())
}

fn instances() -> Vec<String> {
    let mut found: Vec<String> = BUILT_IN.iter().map(|id| (*id).to_owned()).collect();
    if let Some(overlay) = registry_overlay::published() {
        for entry in overlay.entries(REGISTRY) {
            if !found.contains(&entry.id) {
                found.push(entry.id.clone());
            }
        }
    }
    found.sort();
    found
}

fn glob(pattern: &str, value: &str) -> bool {
    match pattern.split_once('*') {
        None => pattern == value,
        Some((head, tail)) => value.strip_prefix(head).is_some_and(|rest| {
            (0..=rest.len()).any(|cut| rest.is_char_boundary(cut) && glob(tail, &rest[cut..]))
        }),
    }
}

fn select(spec: &str) -> Vec<String> {
    let pattern = if spec.contains(':') || spec.starts_with('#') || spec == "*" {
        spec.to_owned()
    } else {
        format!("minecraft:{spec}")
    };
    instances()
        .into_iter()
        .filter(|id| glob(&pattern, id))
        .collect()
}

fn unresolved<C: CommandContext>(ctx: &mut C, sender: CommandSender, spec: &str) -> CommandOutcome {
    fail(
        ctx,
        sender,
        "argument.resource_selector.not_found",
        vec![text(spec), text(REGISTRY)],
    )
}

fn run_tests<C: CommandContext>(
    ctx: &mut C,
    sender: CommandSender,
    ids: &[String],
    times: i32,
) -> CommandOutcome {
    let runnable: Vec<&String> = ids
        .iter()
        .filter(|id| BUILT_IN.contains(&id.as_str()))
        .collect();
    if let Some(blocked) = ids.iter().find(|id| !BUILT_IN.contains(&id.as_str())) {
        return fail(
            ctx,
            sender,
            "commands.test.error.structure_not_found",
            vec![text(blocked)],
        );
    }
    let total = i32::try_from(runnable.len())
        .unwrap_or(i32::MAX)
        .saturating_mul(times.max(1));
    ctx.reply(
        sender,
        Component::translatable("commands.test.run.running", vec![text(total)]),
    );
    ctx.reply(
        sender,
        Component::translatable("commands.test.summary", vec![text(total)]),
    );
    ctx.reply(
        sender,
        Component::translatable("commands.test.summary.all_required_passed", Vec::new()),
    );
    ctx.record_result(total);
    CommandOutcome::Success
}

fn number(token: Option<&str>, fallback: i32) -> Result<i32, String> {
    token.map_or(Ok(fallback), |value| {
        value.parse::<i32>().map_err(|_| value.to_owned())
    })
}

pub(super) fn run<C: CommandContext>(
    ctx: &mut C,
    sender: CommandSender,
    rest: &str,
) -> CommandOutcome {
    let help = "/test (run|runmultiple|runthese|runthat|runclosest|runfailed|verify|locate|create|pos|clearall|clearthese|clearthat|resetthese|resetthat|resetclosest|stop) ...";
    let Some((action, rest)) = next_argument(rest) else {
        return usage(ctx, sender, help);
    };
    match action {
        "run" | "runmultiple" | "verify" | "locate" => {
            let Some((spec, tail)) = next_argument(rest) else {
                return usage(ctx, sender, help);
            };
            let ids = select(spec);
            if ids.is_empty() {
                return unresolved(ctx, sender, spec);
            }
            if action == "locate" {
                ctx.reply(
                    sender,
                    Component::translatable("commands.test.locate.started", Vec::new()),
                );
                ctx.reply(
                    sender,
                    Component::translatable("commands.test.locate.done", vec![text(0)]),
                );
                ctx.record_result(0);
                return CommandOutcome::Success;
            }
            let count = match number(next_argument(tail).map(|(n, _)| n), 1) {
                Ok(count) => count,
                Err(bad) => return fail(ctx, sender, "parsing.int.invalid", vec![text(bad)]),
            };
            run_tests(ctx, sender, &ids, count)
        }
        "create" => {
            let Some((id, _)) = next_argument(rest) else {
                return usage(ctx, sender, help);
            };
            fail(
                ctx,
                sender,
                "commands.test.error.structure_not_found",
                vec![text(id)],
            )
        }
        "pos" => {
            let at = ctx.sender_origin(sender);
            fail(
                ctx,
                sender,
                "commands.test.error.no_test_containing_pos",
                vec![
                    text(at.x.floor() as i32),
                    text(at.y.floor() as i32),
                    text(at.z.floor() as i32),
                ],
            )
        }
        "runthat" | "runthese" | "runclosest" => fail(
            ctx,
            sender,
            "commands.test.error.no_test_instances",
            Vec::new(),
        ),
        "runfailed" => fail(ctx, sender, "commands.test.no_tests", Vec::new()),
        "clearall" | "clearthat" | "clearthese" => fail(
            ctx,
            sender,
            "commands.test.clear.error.no_tests",
            Vec::new(),
        ),
        "resetthat" | "resetthese" | "resetclosest" => fail(
            ctx,
            sender,
            "commands.test.reset.error.no_tests",
            Vec::new(),
        ),
        "stop" => {
            ctx.reply(
                sender,
                Component::translatable("commands.test.clear.success", vec![text(0)]),
            );
            CommandOutcome::Success
        }
        _ => usage(ctx, sender, help),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selectors_support_exact_ids_and_wildcards() {
        assert_eq!(select("minecraft:always_pass"), ["minecraft:always_pass"]);
        assert_eq!(select("always_pass"), ["minecraft:always_pass"]);
        assert_eq!(select("minecraft:*"), ["minecraft:always_pass"]);
        assert_eq!(select("*"), ["minecraft:always_pass"]);
        assert!(select("t:nothing").is_empty());
    }
}

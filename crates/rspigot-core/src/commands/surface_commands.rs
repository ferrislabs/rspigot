use rspigot_domain::text::Component;

use super::dispatch::{
    CommandContext, CommandOutcome, CommandSender, Target, fail, next_argument, select_one, usage,
};
use super::surface::ProfileQuery;
use super::text_commands::targets;
use crate::dialogs::{self, Resolved};

fn text(value: impl ToString) -> Component {
    Component::text(value.to_string())
}

fn resolve_dialog<C: CommandContext>(
    ctx: &mut C,
    sender: CommandSender,
    spec: &str,
) -> Result<Resolved, CommandOutcome> {
    let spec = spec.trim();
    let parsed = if spec.starts_with('{') {
        dialogs::inline(spec)
    } else {
        match dialogs::lookup(spec) {
            Some(found) => found,
            None => {
                let id = if spec.contains(':') {
                    spec.to_owned()
                } else {
                    format!("minecraft:{spec}")
                };
                return Err(fail(
                    ctx,
                    sender,
                    "argument.resource_or_id.no_such_element",
                    vec![text(&id), text(dialogs::REGISTRY)],
                ));
            }
        }
    };
    parsed.map_err(|reason| {
        fail(
            ctx,
            sender,
            "argument.resource_or_id.failed_to_parse",
            vec![text(reason)],
        )
    })
}

fn dialog<C: CommandContext>(ctx: &mut C, sender: CommandSender, rest: &str) -> CommandOutcome {
    let help = "/dialog (show <targets> <dialog>|clear <targets>)";
    let Some((action, rest)) = next_argument(rest) else {
        return usage(ctx, sender, help);
    };
    let Some((selector, rest)) = next_argument(rest) else {
        return usage(ctx, sender, help);
    };
    let show = match action {
        "show" => true,
        "clear" => false,
        _ => return usage(ctx, sender, help),
    };
    let found = match targets(ctx, sender, selector) {
        Ok(found) => found,
        Err(outcome) => return outcome,
    };
    let prepared = if show {
        if rest.trim().is_empty() {
            return usage(ctx, sender, help);
        }
        match resolve_dialog(ctx, sender, rest) {
            Ok(dialog) => Some(dialog),
            Err(outcome) => return outcome,
        }
    } else {
        None
    };
    for player in &found {
        match &prepared {
            Some(dialog) => ctx.show_dialog(*player, dialog),
            None => ctx.clear_dialog(*player),
        }
    }
    let verb = if show { "show" } else { "clear" };
    let (suffix, who) = if let [only] = found.as_slice() {
        ("single", ctx.target_name(Target::Player(*only)))
    } else {
        ("multiple", text(found.len()))
    };
    ctx.reply(
        sender,
        Component::translatable(format!("commands.dialog.{verb}.{suffix}"), vec![who]),
    );
    ctx.record_result(found.len() as i32);
    CommandOutcome::Success
}

fn quoted(value: &str) -> String {
    format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\""))
}

fn serialize_profile(profile: &rspigot_domain::player::GameProfile) -> String {
    let (a, b, c, d) = {
        let bits = profile.id.0.as_u128();
        (
            (bits >> 96) as i32,
            (bits >> 64) as i32,
            (bits >> 32) as i32,
            bits as i32,
        )
    };
    let properties: Vec<String> = profile
        .properties
        .iter()
        .map(|property| {
            let signature = property
                .signature
                .as_ref()
                .map_or(String::new(), |sig| format!(",signature:{}", quoted(sig)));
            format!(
                "{{name:{},value:{}{signature}}}",
                quoted(&property.name),
                quoted(&property.value)
            )
        })
        .collect();
    format!(
        "{{id:[I;{a},{b},{c},{d}],name:{},properties:[{}]}}",
        quoted(&profile.name),
        properties.join(",")
    )
}

fn profile_component(profile: &rspigot_domain::player::GameProfile) -> Component {
    let serialized = serialize_profile(profile);
    let mut shown = text(format!("[{}]", profile.name));
    shown.style.click = Some(Box::new(rspigot_domain::text::ClickEvent::CopyToClipboard(
        serialized,
    )));
    shown.style.hover = Some(Box::new(Component::translatable(
        "commands.fetchprofile.copy_component",
        Vec::new(),
    )));
    shown
}

fn fetch_profile<C: CommandContext>(
    ctx: &mut C,
    sender: CommandSender,
    rest: &str,
) -> CommandOutcome {
    let help = "/fetchprofile (name <name>|id <uuid>|entity <entity>)";
    let Some((mode, rest)) = next_argument(rest) else {
        return usage(ctx, sender, help);
    };
    let rest = rest.trim();
    if rest.is_empty() {
        return usage(ctx, sender, help);
    }
    let (kind, label, profile) = match mode {
        "name" => (
            "name",
            text(rest),
            ctx.resolve_profile(ProfileQuery::Name(rest)),
        ),
        "id" => {
            let Ok(id) = uuid::Uuid::parse_str(rest) else {
                return fail(ctx, sender, "argument.uuid.invalid", Vec::new());
            };
            ("id", text(rest), ctx.resolve_profile(ProfileQuery::Id(id)))
        }
        "entity" => {
            let target = match select_one(ctx, sender, rest) {
                Ok(target) => target,
                Err(outcome) => return outcome,
            };
            let shown = ctx.target_name(target);
            let found = match target {
                Target::Player(id) => ctx.resolve_profile(ProfileQuery::Id(id.0)),
                Target::Entity(_) => None,
            };
            match found {
                Some(profile) => ("entity", shown, Some(profile)),
                None => {
                    return fail(ctx, sender, "commands.fetchprofile.no_profile", vec![shown]);
                }
            }
        }
        _ => return usage(ctx, sender, help),
    };
    let Some(profile) = profile else {
        return fail(
            ctx,
            sender,
            &format!("commands.fetchprofile.{kind}.failure"),
            vec![label],
        );
    };
    ctx.reply(
        sender,
        Component::translatable(
            format!("commands.fetchprofile.{kind}.success"),
            vec![label, profile_component(&profile)],
        ),
    );
    ctx.record_result(1);
    CommandOutcome::Success
}

fn publish<C: CommandContext>(ctx: &mut C, sender: CommandSender, rest: &str) -> CommandOutcome {
    let help = "/publish [<allowCommands> [<port>]]";
    let mut parts = rest.split_whitespace();
    if let Some(flag) = parts.next()
        && !matches!(flag, "true" | "false")
    {
        return fail(ctx, sender, "parsing.bool.invalid", vec![text(flag)]);
    }
    if let Some(port) = parts.next() {
        match port.parse::<i32>() {
            Ok(value) if value < 0 => {
                return fail(
                    ctx,
                    sender,
                    "argument.integer.low",
                    vec![text(0), text(value)],
                );
            }
            Ok(value) if value > 65535 => {
                return fail(
                    ctx,
                    sender,
                    "argument.integer.big",
                    vec![text(65535), text(value)],
                );
            }
            Ok(_) => {}
            Err(_) => return fail(ctx, sender, "parsing.int.invalid", vec![text(port)]),
        }
    }
    if parts.next().is_some() {
        return usage(ctx, sender, help);
    }
    match ctx.server_port() {
        Some(port) => fail(
            ctx,
            sender,
            "commands.publish.alreadyPublished.lan",
            vec![text(port)],
        ),
        None => fail(ctx, sender, "commands.publish.failed", Vec::new()),
    }
}

fn unpublish<C: CommandContext>(ctx: &mut C, sender: CommandSender, rest: &str) -> CommandOutcome {
    if !rest.trim().is_empty() {
        return usage(ctx, sender, "/unpublish");
    }
    fail(ctx, sender, "commands.unpublish.notPublished", Vec::new())
}

fn jfr<C: CommandContext>(ctx: &mut C, sender: CommandSender, rest: &str) -> CommandOutcome {
    match rest.trim() {
        "start" => fail(ctx, sender, "commands.jfr.start.failed", Vec::new()),
        "stop" => fail(
            ctx,
            sender,
            "commands.jfr.dump.failed",
            vec![text("JFR is not available in rspigot")],
        ),
        _ => usage(ctx, sender, "/jfr (start|stop)"),
    }
}

fn perf<C: CommandContext>(ctx: &mut C, sender: CommandSender, rest: &str) -> CommandOutcome {
    match rest.trim() {
        "start" => {
            let requester = match sender {
                CommandSender::Player(id) => Some(id),
                CommandSender::Console => None,
            };
            if !ctx.perf_start(requester) {
                return fail(ctx, sender, "commands.perf.alreadyRunning", Vec::new());
            }
            ctx.record_result(0);
            ctx.reply(
                sender,
                Component::translatable("commands.perf.started", Vec::new()),
            );
            CommandOutcome::Success
        }
        "stop" => {
            let Some(report) = ctx.perf_stop() else {
                return fail(ctx, sender, "commands.perf.notRunning", Vec::new());
            };
            let rate = if report.seconds > 0.0 {
                report.ticks as f64 / report.seconds
            } else {
                0.0
            };
            ctx.record_result(rate.round() as i32);
            ctx.reply(sender, crate::server_surface::stopped_message(&report));
            ctx.reply(sender, crate::server_surface::saved_message(&report));
            CommandOutcome::Success
        }
        _ => usage(ctx, sender, "/perf (start|stop)"),
    }
}

fn identifier_of(spec: &str) -> Option<rspigot_domain::Identifier> {
    let full = if spec.contains(':') {
        spec.to_owned()
    } else {
        format!("minecraft:{spec}")
    };
    rspigot_domain::Identifier::parse(&full).ok()
}

fn post_effect<C: CommandContext>(
    ctx: &mut C,
    sender: CommandSender,
    rest: &str,
) -> CommandOutcome {
    let help = "/posteffect (add|remove <targets> <posteffect>|clear <targets>|list <target>)";
    let Some((action, rest)) = next_argument(rest) else {
        return usage(ctx, sender, help);
    };
    let Some((selector, rest)) = next_argument(rest) else {
        return usage(ctx, sender, help);
    };
    let found = match targets(ctx, sender, selector) {
        Ok(found) => found,
        Err(outcome) => return outcome,
    };
    let single = found.len() == 1;
    let suffix = if single { "single" } else { "multiple" };
    let who = if single {
        ctx.target_name(Target::Player(found[0]))
    } else {
        text(found.len())
    };
    if action == "list" {
        if !single {
            return fail(ctx, sender, "argument.entity.toomany", Vec::new());
        }
        let effects = ctx.post_effects_of(found[0]);
        if effects.is_empty() {
            ctx.reply(
                sender,
                Component::translatable("commands.posteffect.list.empty", vec![who]),
            );
        } else {
            let names: Vec<String> = effects.iter().map(ToString::to_string).collect();
            ctx.reply(
                sender,
                Component::translatable(
                    "commands.posteffect.list.success",
                    vec![who, text(effects.len()), text(names.join(", "))],
                ),
            );
        }
        ctx.record_result(effects.len() as i32);
        return CommandOutcome::Success;
    }
    if action == "clear" {
        let mut changed = 0;
        for player in &found {
            if !ctx.post_effects_of(*player).is_empty() {
                ctx.set_post_effects(*player, Vec::new());
                changed += 1;
            }
        }
        if changed == 0 {
            return fail(ctx, sender, "commands.posteffect.clear.failed", Vec::new());
        }
        let who = if single { who } else { text(changed) };
        ctx.reply(
            sender,
            Component::translatable(
                format!("commands.posteffect.clear.success.{suffix}"),
                vec![who],
            ),
        );
        ctx.record_result(changed);
        return CommandOutcome::Success;
    }
    let adding = match action {
        "add" => true,
        "remove" => false,
        _ => return usage(ctx, sender, help),
    };
    let Some((spec, _)) = next_argument(rest) else {
        return usage(ctx, sender, help);
    };
    let Some(effect) = identifier_of(spec) else {
        return fail(ctx, sender, "argument.id.invalid", Vec::new());
    };
    let mut changed = 0;
    for player in &found {
        let mut effects = ctx.post_effects_of(*player);
        let present = effects.contains(&effect);
        if adding == present {
            continue;
        }
        if adding {
            effects.push(effect.clone());
        } else {
            effects.retain(|known| *known != effect);
        }
        ctx.set_post_effects(*player, effects);
        changed += 1;
    }
    let verb = if adding { "add" } else { "remove" };
    if changed == 0 {
        return fail(
            ctx,
            sender,
            &format!("commands.posteffect.{verb}.failed"),
            Vec::new(),
        );
    }
    let who = if single { who } else { text(changed) };
    ctx.reply(
        sender,
        Component::translatable(
            format!("commands.posteffect.{verb}.success.{suffix}"),
            vec![text(&effect), who],
        ),
    );
    ctx.record_result(changed);
    CommandOutcome::Success
}

pub(super) fn run<C: CommandContext>(
    ctx: &mut C,
    sender: CommandSender,
    label: &str,
    rest: &str,
) -> CommandOutcome {
    match label {
        "dialog" => dialog(ctx, sender, rest),
        "publish" => publish(ctx, sender, rest),
        "unpublish" => unpublish(ctx, sender, rest),
        "jfr" => jfr(ctx, sender, rest),
        "perf" => perf(ctx, sender, rest),
        "posteffect" => post_effect(ctx, sender, rest),
        "fetchprofile" => fetch_profile(ctx, sender, rest),
        _ => CommandOutcome::Unknown,
    }
}

use rspigot_domain::Identifier;
use rspigot_domain::text::{ClickEvent, Color, Component};

use super::dispatch::{
    CommandContext, CommandOutcome, CommandSender, Target, fail, next_argument, select_one, usage,
};
use crate::waypoints::{DEFAULT_STYLE, team_color_rgb};

fn text(value: impl ToString) -> Component {
    Component::text(value.to_string())
}

fn hex_value(token: &str) -> Option<u32> {
    let digits = token.strip_prefix('#').unwrap_or(token);
    if digits.len() == 6 {
        u32::from_str_radix(digits, 16).ok()
    } else {
        None
    }
}

fn list<C: CommandContext>(ctx: &mut C, sender: CommandSender) -> CommandOutcome {
    let dimension = ctx.current_dimension().unwrap_or_else(|| {
        Identifier::parse("minecraft:overworld").unwrap_or_else(|_| unreachable!())
    });
    let found = ctx.waypoint_transmitters(&dimension);
    if found.is_empty() {
        ctx.reply(
            sender,
            Component::translatable(
                "commands.waypoint.list.empty",
                vec![text(dimension.to_string())],
            ),
        );
        ctx.record_result(0);
        return CommandOutcome::Success;
    }
    let mut joined = Component::text("");
    for (index, (player, pos)) in found.iter().enumerate() {
        if index > 0 {
            joined = joined.append(text(", "));
        }
        let name = ctx.player_name(*player).unwrap_or_default();
        let color = ctx.waypoint_icon(*player).color;
        let mut entry = text(name);
        entry.style.click = Some(Box::new(ClickEvent::RunCommand(format!(
            "/execute in {dimension} run tp @s {} {} {}",
            pos.x, pos.y, pos.z
        ))));
        entry.style.color = color.map(Color::Rgb);
        joined = joined.append(entry);
    }
    ctx.reply(
        sender,
        Component::translatable(
            "commands.waypoint.list.success",
            vec![text(found.len()), joined],
        ),
    );
    ctx.record_result(found.len() as i32);
    CommandOutcome::Success
}

fn modify<C: CommandContext>(
    ctx: &mut C,
    sender: CommandSender,
    rest: &str,
    help: &str,
) -> CommandOutcome {
    let Some((selector, rest)) = next_argument(rest) else {
        return usage(ctx, sender, help);
    };
    let Some((property, rest)) = next_argument(rest) else {
        return usage(ctx, sender, help);
    };
    let argument = next_argument(rest);
    let edit = match (property, argument) {
        ("color", Some(("reset", _))) => Edit::Color(None, None),
        ("color", Some(("hex", after))) => match next_argument(after).map(|(hex, _)| hex) {
            Some(hex) => match hex_value(hex) {
                Some(value) => {
                    Edit::Color(Some(value), Some(hex.trim_start_matches('#').to_owned()))
                }
                None => return usage(ctx, sender, help),
            },
            None => return usage(ctx, sender, help),
        },
        ("color", Some((name, _))) => match team_color_rgb(name) {
            Some(value) => Edit::Color(Some(value), Some(name.to_owned())),
            None => return usage(ctx, sender, help),
        },
        ("style", Some(("reset", _))) => Edit::Style(DEFAULT_STYLE.to_owned()),
        ("style", Some(("set", after))) => match next_argument(after) {
            Some((style, _)) => Edit::Style(style.to_owned()),
            None => return usage(ctx, sender, help),
        },
        _ => return usage(ctx, sender, help),
    };
    let target = match select_one(ctx, sender, selector) {
        Ok(target) => target,
        Err(outcome) => return outcome,
    };
    let Target::Player(player) = target else {
        return fail(ctx, sender, "argument.waypoint.invalid", Vec::new());
    };
    let mut icon = ctx.waypoint_icon(player);
    let message = match edit {
        Edit::Color(None, _) => {
            icon.color = None;
            Component::translatable("commands.waypoint.modify.color.reset", Vec::new())
        }
        Edit::Color(Some(value), Some(label)) => {
            icon.color = Some(value);
            let mut shown = text(label);
            shown.style.color = Some(Color::Rgb(value));
            Component::translatable("commands.waypoint.modify.color", vec![shown])
        }
        Edit::Color(Some(_), None) => return usage(ctx, sender, help),
        Edit::Style(style) => {
            let Ok(id) = Identifier::parse(&if style.contains(':') {
                style.clone()
            } else {
                format!("minecraft:{style}")
            }) else {
                return fail(ctx, sender, "argument.id.invalid", Vec::new());
            };
            icon.style = id;
            Component::translatable("commands.waypoint.modify.style", Vec::new())
        }
    };
    ctx.set_waypoint_icon(player, icon);
    ctx.reply(sender, message);
    ctx.record_result(1);
    CommandOutcome::Success
}

enum Edit {
    Color(Option<u32>, Option<String>),
    Style(String),
}

pub(super) fn run<C: CommandContext>(
    ctx: &mut C,
    sender: CommandSender,
    rest: &str,
) -> CommandOutcome {
    let help = "/waypoint (list|modify <waypoint> (color|style) ...)";
    let Some((action, rest)) = next_argument(rest) else {
        return usage(ctx, sender, help);
    };
    match action {
        "list" => list(ctx, sender),
        "modify" => modify(ctx, sender, rest, help),
        _ => usage(ctx, sender, help),
    }
}

use rspigot_domain::scoreboard::{CollisionRule, Team, Visibility};
use rspigot_domain::text::{Component, NamedColor};

use super::dispatch::{
    CommandContext, CommandOutcome, CommandSender, Target, fail, next_argument, usage,
};
use super::json_text;
use super::scoreboard_commands::{holders, joined, parse_bool, reply_err, success, text};
use crate::events::ChatKind;

fn find_team<C: CommandContext>(
    ctx: &mut C,
    sender: CommandSender,
    name: &str,
) -> Result<Team, CommandOutcome> {
    match ctx.scoreboard().team(name) {
        Some(team) => Ok(team.clone()),
        None => Err(fail(ctx, sender, "team.notFound", vec![text(name)])),
    }
}

fn component_of<C: CommandContext>(
    ctx: &mut C,
    sender: CommandSender,
    raw: &str,
) -> Result<Component, CommandOutcome> {
    json_text::parse(raw).map_err(|error| reply_err(ctx, sender, error))
}

pub(super) fn run<C: CommandContext>(
    ctx: &mut C,
    sender: CommandSender,
    label: &str,
    rest: &str,
) -> CommandOutcome {
    match label {
        "team" => team(ctx, sender, rest),
        _ => teammsg(ctx, sender, rest),
    }
}

fn team<C: CommandContext>(ctx: &mut C, sender: CommandSender, rest: &str) -> CommandOutcome {
    let help = "/team <list|add|remove|empty|join|leave|modify> ...";
    let Some((action, rest)) = next_argument(rest) else {
        return usage(ctx, sender, help);
    };
    match action {
        "list" => team_list(ctx, sender, rest),
        "add" => team_add(ctx, sender, rest),
        "remove" => team_remove(ctx, sender, rest),
        "empty" => team_empty(ctx, sender, rest),
        "join" => team_join(ctx, sender, rest),
        "leave" => team_leave(ctx, sender, rest),
        "modify" => team_modify(ctx, sender, rest),
        _ => usage(ctx, sender, help),
    }
}

fn team_list<C: CommandContext>(ctx: &mut C, sender: CommandSender, rest: &str) -> CommandOutcome {
    let Some((name, _)) = next_argument(rest) else {
        let teams: Vec<Team> = ctx.scoreboard().teams().cloned().collect();
        ctx.record_result(teams.len() as i32);
        if teams.is_empty() {
            success(ctx, sender, "commands.team.list.teams.empty", Vec::new());
            return CommandOutcome::Success;
        }
        let shown = joined(teams.iter().map(Team::formatted_display_name).collect());
        success(
            ctx,
            sender,
            "commands.team.list.teams.success",
            vec![text(teams.len()), shown],
        );
        return CommandOutcome::Success;
    };
    let team = match find_team(ctx, sender, name) {
        Ok(team) => team,
        Err(outcome) => return outcome,
    };
    let members: Vec<String> = ctx
        .scoreboard()
        .team_members(name)
        .into_iter()
        .map(str::to_owned)
        .collect();
    ctx.record_result(members.len() as i32);
    if members.is_empty() {
        success(
            ctx,
            sender,
            "commands.team.list.members.empty",
            vec![team.formatted_display_name()],
        );
        return CommandOutcome::Success;
    }
    success(
        ctx,
        sender,
        "commands.team.list.members.success",
        vec![
            team.formatted_display_name(),
            text(members.len()),
            joined(members.iter().map(text).collect()),
        ],
    );
    CommandOutcome::Success
}

fn team_add<C: CommandContext>(ctx: &mut C, sender: CommandSender, rest: &str) -> CommandOutcome {
    let Some((name, rest)) = next_argument(rest) else {
        return usage(ctx, sender, "/team add <team> [<displayName>]");
    };
    let display = if rest.trim().is_empty() {
        text(name)
    } else {
        match component_of(ctx, sender, rest.trim()) {
            Ok(display) => display,
            Err(outcome) => return outcome,
        }
    };
    let team = Team::new(name, display);
    let shown = team.formatted_display_name();
    let added = ctx.change_scoreboard(|board| match board.add_team(team) {
        Ok(messages) => (true, messages),
        Err(_) => (false, Vec::new()),
    });
    if !added {
        return fail(ctx, sender, "commands.team.add.duplicate", Vec::new());
    }
    let count = ctx.scoreboard().teams().count() as i32;
    ctx.record_result(count);
    success(ctx, sender, "commands.team.add.success", vec![shown]);
    CommandOutcome::Success
}

fn team_remove<C: CommandContext>(
    ctx: &mut C,
    sender: CommandSender,
    rest: &str,
) -> CommandOutcome {
    let Some((name, _)) = next_argument(rest) else {
        return usage(ctx, sender, "/team remove <team>");
    };
    let team = match find_team(ctx, sender, name) {
        Ok(team) => team,
        Err(outcome) => return outcome,
    };
    ctx.change_scoreboard(|board| ((), board.remove_team(name)));
    let count = ctx.scoreboard().teams().count() as i32;
    ctx.record_result(count);
    success(
        ctx,
        sender,
        "commands.team.remove.success",
        vec![team.formatted_display_name()],
    );
    CommandOutcome::Success
}

fn team_empty<C: CommandContext>(ctx: &mut C, sender: CommandSender, rest: &str) -> CommandOutcome {
    let Some((name, _)) = next_argument(rest) else {
        return usage(ctx, sender, "/team empty <team>");
    };
    let team = match find_team(ctx, sender, name) {
        Ok(team) => team,
        Err(outcome) => return outcome,
    };
    if ctx.scoreboard().team_members(name).is_empty() {
        return fail(ctx, sender, "commands.team.empty.unchanged", Vec::new());
    }
    let count = ctx.change_scoreboard(|board| board.empty_team(name));
    ctx.record_result(count as i32);
    success(
        ctx,
        sender,
        "commands.team.empty.success",
        vec![text(count), team.formatted_display_name()],
    );
    CommandOutcome::Success
}

fn team_join<C: CommandContext>(ctx: &mut C, sender: CommandSender, rest: &str) -> CommandOutcome {
    let Some((name, rest)) = next_argument(rest) else {
        return usage(ctx, sender, "/team join <team> [<members>]");
    };
    let team = match find_team(ctx, sender, name) {
        Ok(team) => team,
        Err(outcome) => return outcome,
    };
    let members = match next_argument(rest) {
        Some((token, _)) => match holders(ctx, sender, token) {
            Ok(members) => members,
            Err(outcome) => return outcome,
        },
        None => match sender {
            CommandSender::Player(player) => vec![ctx.player_name(player).unwrap_or_default()],
            CommandSender::Console => {
                return fail(ctx, sender, "permissions.requires.entity", Vec::new());
            }
        },
    };
    ctx.change_scoreboard(|board| {
        let mut messages = Vec::new();
        for member in &members {
            if let Ok(sent) = board.join_team(member, name) {
                messages.extend(sent);
            }
        }
        ((), messages)
    });
    ctx.record_result(members.len() as i32);
    if members.len() == 1 {
        success(
            ctx,
            sender,
            "commands.team.join.success.single",
            vec![text(&members[0]), team.formatted_display_name()],
        );
    } else {
        success(
            ctx,
            sender,
            "commands.team.join.success.multiple",
            vec![text(members.len()), team.formatted_display_name()],
        );
    }
    CommandOutcome::Success
}

fn team_leave<C: CommandContext>(ctx: &mut C, sender: CommandSender, rest: &str) -> CommandOutcome {
    let Some((token, _)) = next_argument(rest) else {
        return usage(ctx, sender, "/team leave <members>");
    };
    let members = match holders(ctx, sender, token) {
        Ok(members) => members,
        Err(outcome) => return outcome,
    };
    ctx.change_scoreboard(|board| {
        let mut messages = Vec::new();
        for member in &members {
            messages.extend(board.leave_team(member));
        }
        ((), messages)
    });
    ctx.record_result(members.len() as i32);
    if members.len() == 1 {
        success(
            ctx,
            sender,
            "commands.team.leave.success.single",
            vec![text(&members[0])],
        );
    } else {
        success(
            ctx,
            sender,
            "commands.team.leave.success.multiple",
            vec![text(members.len())],
        );
    }
    CommandOutcome::Success
}

fn flag_option<C: CommandContext>(
    ctx: &mut C,
    sender: CommandSender,
    team: &Team,
    rest: &str,
    friendly_fire: bool,
) -> CommandOutcome {
    let Some((raw, _)) = next_argument(rest) else {
        return usage(ctx, sender, "/team modify <team> <option> <true|false>");
    };
    let enabled = match parse_bool(ctx, sender, raw) {
        Ok(enabled) => enabled,
        Err(outcome) => return outcome,
    };
    let current = if friendly_fire {
        team.friendly_fire
    } else {
        team.see_friendly_invisibles
    };
    let base = if friendly_fire {
        "commands.team.option.friendlyfire"
    } else {
        "commands.team.option.seeFriendlyInvisibles"
    };
    if current == enabled {
        let key = if enabled {
            format!("{base}.alreadyEnabled")
        } else {
            format!("{base}.alreadyDisabled")
        };
        return fail(ctx, sender, &key, Vec::new());
    }
    ctx.change_scoreboard(|board| {
        let messages = board.modify_team(&team.name, |team| {
            if friendly_fire {
                team.friendly_fire = enabled;
            } else {
                team.see_friendly_invisibles = enabled;
            }
        });
        ((), messages)
    });
    let key = if enabled {
        format!("{base}.enabled")
    } else {
        format!("{base}.disabled")
    };
    success(ctx, sender, &key, vec![team.formatted_display_name()]);
    CommandOutcome::Success
}

fn team_modify<C: CommandContext>(
    ctx: &mut C,
    sender: CommandSender,
    rest: &str,
) -> CommandOutcome {
    let help = "/team modify <team> <option> <value>";
    let Some((name, rest)) = next_argument(rest) else {
        return usage(ctx, sender, help);
    };
    let team = match find_team(ctx, sender, name) {
        Ok(team) => team,
        Err(outcome) => return outcome,
    };
    let Some((option, rest)) = next_argument(rest) else {
        return usage(ctx, sender, help);
    };
    match option {
        "displayName" => {
            let display = match component_of(ctx, sender, rest.trim()) {
                Ok(display) => display,
                Err(outcome) => return outcome,
            };
            if display == team.display_name {
                return fail(
                    ctx,
                    sender,
                    "commands.team.option.name.unchanged",
                    Vec::new(),
                );
            }
            ctx.change_scoreboard(|board| {
                (
                    (),
                    board.modify_team(name, |team| team.display_name = display),
                )
            });
            let shown = ctx
                .scoreboard()
                .team(name)
                .map_or_else(|| text(name), Team::formatted_display_name);
            success(
                ctx,
                sender,
                "commands.team.option.name.success",
                vec![shown],
            );
            CommandOutcome::Success
        }
        "color" => team_color(ctx, sender, &team, rest),
        "friendlyFire" => flag_option(ctx, sender, &team, rest, true),
        "seeFriendlyInvisibles" => flag_option(ctx, sender, &team, rest, false),
        "nametagVisibility" | "deathMessageVisibility" => {
            visibility_option(ctx, sender, &team, option, rest)
        }
        "collisionRule" => collision_option(ctx, sender, &team, rest),
        "prefix" | "suffix" => {
            let value = match component_of(ctx, sender, rest.trim()) {
                Ok(value) => value,
                Err(outcome) => return outcome,
            };
            let shown = value.clone();
            let prefix = option == "prefix";
            ctx.change_scoreboard(|board| {
                let messages = board.modify_team(name, |team| {
                    if prefix {
                        team.prefix = value;
                    } else {
                        team.suffix = value;
                    }
                });
                ((), messages)
            });
            let key = if prefix {
                "commands.team.option.prefix.success"
            } else {
                "commands.team.option.suffix.success"
            };
            success(ctx, sender, key, vec![shown]);
            CommandOutcome::Success
        }
        _ => usage(ctx, sender, help),
    }
}

fn team_color<C: CommandContext>(
    ctx: &mut C,
    sender: CommandSender,
    team: &Team,
    rest: &str,
) -> CommandOutcome {
    let Some((raw, _)) = next_argument(rest) else {
        return usage(ctx, sender, "/team modify <team> color <value>");
    };
    let color = if raw == "reset" {
        None
    } else {
        match NamedColor::from_name(raw) {
            Some(color) => Some(color),
            None => return fail(ctx, sender, "argument.color.invalid", vec![text(raw)]),
        }
    };
    if team.color == color {
        return fail(
            ctx,
            sender,
            "commands.team.option.color.unchanged",
            Vec::new(),
        );
    }
    ctx.change_scoreboard(|board| ((), board.modify_team(&team.name, |t| t.color = color)));
    let shown = ctx
        .scoreboard()
        .team(&team.name)
        .map_or_else(|| text(&team.name), Team::formatted_display_name);
    match color {
        Some(color) => success(
            ctx,
            sender,
            "commands.team.option.color.success",
            vec![shown, text(color.name())],
        ),
        None => success(
            ctx,
            sender,
            "commands.team.option.color.clear.success",
            vec![shown],
        ),
    }
    CommandOutcome::Success
}

fn visibility_option<C: CommandContext>(
    ctx: &mut C,
    sender: CommandSender,
    team: &Team,
    option: &str,
    rest: &str,
) -> CommandOutcome {
    let Some((raw, _)) = next_argument(rest) else {
        return usage(ctx, sender, "/team modify <team> <option> <value>");
    };
    let Some(value) = Visibility::from_name(raw) else {
        return fail(ctx, sender, "argument.enum.invalid", vec![text(raw)]);
    };
    let nametag = option == "nametagVisibility";
    let current = if nametag {
        team.nametag_visibility
    } else {
        team.death_message_visibility
    };
    let base = if nametag {
        "commands.team.option.nametagVisibility"
    } else {
        "commands.team.option.deathMessageVisibility"
    };
    if current == value {
        return fail(ctx, sender, &format!("{base}.unchanged"), Vec::new());
    }
    ctx.change_scoreboard(|board| {
        let messages = board.modify_team(&team.name, |team| {
            if nametag {
                team.nametag_visibility = value;
            } else {
                team.death_message_visibility = value;
            }
        });
        ((), messages)
    });
    success(
        ctx,
        sender,
        &format!("{base}.success"),
        vec![
            team.formatted_display_name(),
            Component::translatable(value.translation_key(), Vec::new()),
        ],
    );
    CommandOutcome::Success
}

fn collision_option<C: CommandContext>(
    ctx: &mut C,
    sender: CommandSender,
    team: &Team,
    rest: &str,
) -> CommandOutcome {
    let Some((raw, _)) = next_argument(rest) else {
        return usage(ctx, sender, "/team modify <team> collisionRule <value>");
    };
    let Some(value) = CollisionRule::from_name(raw) else {
        return fail(ctx, sender, "argument.enum.invalid", vec![text(raw)]);
    };
    if team.collision_rule == value {
        return fail(
            ctx,
            sender,
            "commands.team.option.collisionRule.unchanged",
            Vec::new(),
        );
    }
    ctx.change_scoreboard(|board| {
        (
            (),
            board.modify_team(&team.name, |team| team.collision_rule = value),
        )
    });
    success(
        ctx,
        sender,
        "commands.team.option.collisionRule.success",
        vec![
            team.formatted_display_name(),
            Component::translatable(value.translation_key(), Vec::new()),
        ],
    );
    CommandOutcome::Success
}

fn teammsg<C: CommandContext>(ctx: &mut C, sender: CommandSender, rest: &str) -> CommandOutcome {
    let CommandSender::Player(player) = sender else {
        return fail(ctx, sender, "permissions.requires.entity", Vec::new());
    };
    let message = rest.trim();
    if message.is_empty() {
        return usage(ctx, sender, "/teammsg <message>");
    }
    let name = ctx.player_name(player).unwrap_or_default();
    let Some(team) = ctx.scoreboard().team_of(&name).cloned() else {
        return fail(ctx, sender, "commands.teammsg.failed.noteam", Vec::new());
    };
    let recipients: Vec<_> = ctx
        .scoreboard()
        .team_members(&team.name)
        .into_iter()
        .filter_map(|member| ctx.find_player(member))
        .collect();
    let team_name = team.formatted_display_name();
    let from = ctx.target_name(Target::Player(player));
    let body = super::chat_commands::with_selectors(ctx, sender, message);
    let mut targets = recipients.clone();
    if !targets.contains(&player) {
        targets.push(player);
    }
    for recipient in &targets {
        let kind = if *recipient == player {
            ChatKind::TeamOutgoing
        } else {
            ChatKind::TeamIncoming
        };
        ctx.send_chat(
            *recipient,
            kind,
            from.clone(),
            Some(team_name.clone()),
            body.clone(),
        );
    }
    ctx.record_result(recipients.len() as i32);
    CommandOutcome::Success
}

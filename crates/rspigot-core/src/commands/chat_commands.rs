use rspigot_domain::text::{Component, NamedColor};

use super::dispatch::{
    CommandContext, CommandOutcome, CommandSender, Target, next_argument, usage,
};
use super::text_commands::targets;
use crate::events::ChatKind;

fn speaker<C: CommandContext>(ctx: &C, sender: CommandSender) -> Component {
    match sender {
        CommandSender::Player(id) => ctx.target_name(Target::Player(id)),
        CommandSender::Console => Component::text(ctx.sender_name(sender)),
    }
}

fn selector_end(text: &str) -> usize {
    let mut depth = 0;
    for (index, c) in text.char_indices() {
        match c {
            '[' => depth += 1,
            ']' if depth > 0 => {
                depth -= 1;
                if depth == 0 {
                    return index + 1;
                }
            }
            c if c.is_whitespace() && depth == 0 => return index,
            _ => {}
        }
    }
    text.len()
}

pub(super) fn with_selectors<C: CommandContext>(
    ctx: &C,
    sender: CommandSender,
    message: &str,
) -> Component {
    if ctx.permission_level(sender) < super::permission::LEVEL_GAMEMASTERS {
        return Component::text(message);
    }
    let mut out = Component::text("");
    let mut plain = String::new();
    let mut rest = message;
    let mut replaced = false;
    while let Some(at) = rest.find('@') {
        let (before, from) = rest.split_at(at);
        let boundary = before.chars().last().is_none_or(char::is_whitespace);
        let end = selector_end(from);
        let candidate = &from[..end];
        let names = (boundary && super::selector::parse(candidate).is_ok())
            .then(|| ctx.select_entities(sender, candidate).ok())
            .flatten();
        match names {
            Some(found) => {
                plain.push_str(before);
                if !plain.is_empty() {
                    out = out.append(Component::text(std::mem::take(&mut plain)));
                }
                for (index, target) in found.iter().enumerate() {
                    if index > 0 {
                        out = out.append(Component::text(", "));
                    }
                    out = out.append(ctx.target_name(*target));
                }
                replaced = true;
            }
            None => plain.push_str(&rest[..at + end]),
        }
        rest = &from[end..];
    }
    plain.push_str(rest);
    if !replaced {
        return Component::text(message);
    }
    if !plain.is_empty() {
        out = out.append(Component::text(plain));
    }
    out
}

fn announce<C: CommandContext>(ctx: &mut C, sender: CommandSender, kind: ChatKind, message: &str) {
    let name = speaker(ctx, sender);
    let body = with_selectors(ctx, sender, message);
    for player in ctx.player_ids() {
        ctx.send_chat(player, kind, name.clone(), None, body.clone());
    }
    let key = match kind {
        ChatKind::EmoteCommand => "chat.type.emote",
        _ => "chat.type.announcement",
    };
    tracing::info!(
        "{}",
        crate::console::console_text(&Component::translatable(key, vec![name, body]))
    );
}

fn say<C: CommandContext>(ctx: &mut C, sender: CommandSender, rest: &str) -> CommandOutcome {
    let message = rest.trim();
    if message.is_empty() {
        return usage(ctx, sender, "/say <message>");
    }
    announce(ctx, sender, ChatKind::SayCommand, message);
    CommandOutcome::Success
}

fn me<C: CommandContext>(ctx: &mut C, sender: CommandSender, rest: &str) -> CommandOutcome {
    let action = rest.trim();
    if action.is_empty() {
        return usage(ctx, sender, "/me <action>");
    }
    announce(ctx, sender, ChatKind::EmoteCommand, action);
    CommandOutcome::Success
}

fn msg<C: CommandContext>(ctx: &mut C, sender: CommandSender, rest: &str) -> CommandOutcome {
    let Some((selector, message)) = next_argument(rest) else {
        return usage(ctx, sender, "/msg <targets> <message>");
    };
    let message = message.trim();
    if message.is_empty() {
        return usage(ctx, sender, "/msg <targets> <message>");
    }
    let found = match targets(ctx, sender, selector) {
        Ok(found) => found,
        Err(outcome) => return outcome,
    };
    let from = speaker(ctx, sender);
    let body = with_selectors(ctx, sender, message);
    for target in &found {
        let to = ctx.target_name(Target::Player(*target));
        match sender {
            CommandSender::Player(id) => ctx.send_chat(
                id,
                ChatKind::MsgOutgoing,
                from.clone(),
                Some(to.clone()),
                body.clone(),
            ),
            CommandSender::Console => ctx.reply(
                sender,
                Component::translatable(
                    "commands.message.display.outgoing",
                    vec![to.clone(), body.clone()],
                )
                .color(NamedColor::Gray)
                .italic(true),
            ),
        }
        ctx.send_chat(
            *target,
            ChatKind::MsgIncoming,
            from.clone(),
            Some(to),
            body.clone(),
        );
    }
    ctx.record_result(found.len() as i32);
    CommandOutcome::Success
}

pub(super) fn run<C: CommandContext>(
    ctx: &mut C,
    sender: CommandSender,
    label: &str,
    rest: &str,
) -> CommandOutcome {
    match label {
        "say" => say(ctx, sender, rest),
        "me" => me(ctx, sender, rest),
        _ => msg(ctx, sender, rest),
    }
}

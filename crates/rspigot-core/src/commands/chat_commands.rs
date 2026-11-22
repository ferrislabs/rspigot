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

fn announce<C: CommandContext>(ctx: &mut C, sender: CommandSender, kind: ChatKind, message: &str) {
    let name = speaker(ctx, sender);
    let body = Component::text(message);
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
    let body = Component::text(message);
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

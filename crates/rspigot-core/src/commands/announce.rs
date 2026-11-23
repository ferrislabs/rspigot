use rspigot_domain::text::{Component, NamedColor};

use super::dispatch::{CommandContext, CommandSender};
use super::permission::LEVEL_GAMEMASTERS;

pub(super) fn reply_and_broadcast<C: CommandContext>(
    ctx: &mut C,
    sender: CommandSender,
    message: Component,
) {
    ctx.reply(sender, message.clone());
    broadcast_to_ops(ctx, sender, message);
}

pub(super) fn broadcast_to_ops<C: CommandContext>(
    ctx: &mut C,
    sender: CommandSender,
    message: Component,
) {
    let name = match sender {
        CommandSender::Player(id) => ctx.target_name(super::dispatch::Target::Player(id)),
        CommandSender::Console => Component::text(ctx.sender_name(sender)),
    };
    let relayed = Component::translatable("chat.type.admin", vec![name, message])
        .color(NamedColor::Gray)
        .italic(true);
    for player in ctx.player_ids() {
        let listener = CommandSender::Player(player);
        if listener != sender && ctx.permission_level(listener) >= LEVEL_GAMEMASTERS {
            ctx.reply(listener, relayed.clone());
        }
    }
}

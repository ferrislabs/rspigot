use std::num::NonZeroU8;

use rspigot_domain::item::ItemStack;
use rspigot_domain::player::{GameMode, PlayerId};
use rspigot_domain::text::Component;
use rspigot_items::ItemKind;
use rspigot_nbt::Tag;

use super::announce::{broadcast_to_ops, reply_and_broadcast};
use super::dispatch::{CommandContext, CommandOutcome, CommandSender, fail, next_argument, usage};
use super::item_commands::parse_item;
use super::text_commands::targets;

const MAX_STACKS: i64 = 100;
const MAX_STACK_COMPONENT: &str = "minecraft:max_stack_size";

fn text(value: impl ToString) -> Component {
    Component::text(value.to_string())
}

fn rejected<C: CommandContext>(
    ctx: &mut C,
    sender: CommandSender,
    error: Component,
) -> CommandOutcome {
    ctx.reply(sender, error.color(rspigot_domain::text::NamedColor::Red));
    CommandOutcome::Failure
}

fn self_or_selected<C: CommandContext>(
    ctx: &mut C,
    sender: CommandSender,
    token: Option<&str>,
) -> Result<Vec<PlayerId>, CommandOutcome> {
    match (token, sender) {
        (Some(token), _) => targets(ctx, sender, token),
        (None, CommandSender::Player(id)) => Ok(vec![id]),
        (None, CommandSender::Console) => Err(fail(
            ctx,
            sender,
            "argument.entity.notfound.player",
            Vec::new(),
        )),
    }
}

fn name_of<C: CommandContext>(ctx: &C, player: PlayerId) -> Component {
    ctx.target_name(super::dispatch::Target::Player(player))
}

pub(super) fn gamemode<C: CommandContext>(
    ctx: &mut C,
    sender: CommandSender,
    rest: &str,
) -> CommandOutcome {
    let Some((mode, rest)) = next_argument(rest) else {
        return usage(
            ctx,
            sender,
            "/gamemode <survival|creative|adventure|spectator> [targets]",
        );
    };
    let Some(mode) = GameMode::from_name(mode) else {
        return usage(
            ctx,
            sender,
            "/gamemode <survival|creative|adventure|spectator> [targets]",
        );
    };
    let token = next_argument(rest).map(|(token, _)| token);
    let found = match self_or_selected(ctx, sender, token) {
        Ok(found) => found,
        Err(outcome) => return outcome,
    };
    let mode_name = Component::translatable(format!("gameMode.{}", mode.name()), Vec::new());
    let mut changed = 0;
    for target in &found {
        let (replies, relayed) = if CommandSender::Player(*target) == sender {
            let message =
                Component::translatable("commands.gamemode.success.self", vec![mode_name.clone()]);
            (vec![(sender, message.clone())], message)
        } else {
            let message = Component::translatable(
                "commands.gamemode.success.other",
                vec![name_of(ctx, *target), mode_name.clone()],
            );
            (
                vec![
                    (
                        CommandSender::Player(*target),
                        Component::translatable("gameMode.changed", vec![mode_name.clone()]),
                    ),
                    (sender, message.clone()),
                ],
                message,
            )
        };
        ctx.set_game_mode(*target, mode, replies);
        broadcast_to_ops(ctx, sender, relayed);
        changed += 1;
    }
    ctx.record_result(changed);
    CommandOutcome::Success
}

fn stack_limit(stack: &ItemStack) -> u8 {
    match stack.components.get(MAX_STACK_COMPONENT) {
        Some(Tag::Int(value)) => u8::try_from((*value).clamp(1, 99)).unwrap_or(64),
        _ => stack.item.max_stack_size(),
    }
}

pub(super) fn give<C: CommandContext>(
    ctx: &mut C,
    sender: CommandSender,
    rest: &str,
) -> CommandOutcome {
    let help = "/give <targets> <item> [count]";
    let Some((selector, rest)) = next_argument(rest) else {
        return usage(ctx, sender, help);
    };
    let Some((item_token, rest)) = next_argument(rest) else {
        return usage(ctx, sender, help);
    };
    let found = match targets(ctx, sender, selector) {
        Ok(found) => found,
        Err(outcome) => return outcome,
    };
    let stack = match parse_item(item_token) {
        Ok(stack) => stack,
        Err(error) => return rejected(ctx, sender, error),
    };
    let count = match next_argument(rest) {
        Some((token, _)) => match token.parse::<i64>() {
            Ok(count) if count >= 1 && count <= i64::from(i32::MAX) => count,
            Ok(count) => {
                let key = if count < 1 {
                    "argument.integer.low"
                } else {
                    "argument.integer.big"
                };
                let bound = if count < 1 { 1 } else { i64::from(i32::MAX) };
                return fail(ctx, sender, key, vec![text(bound), text(count)]);
            }
            Err(_) => return fail(ctx, sender, "parsing.int.invalid", vec![text(token)]),
        },
        None => 1,
    };
    let per_stack = stack_limit(&stack);
    let limit = i64::from(per_stack) * MAX_STACKS;
    let label = text(format!(
        "[{}]",
        stack.item.kind().map_or("minecraft:air", ItemKind::name)
    ));
    if count > limit {
        return fail(
            ctx,
            sender,
            "commands.give.failed.toomanyitems",
            vec![text(limit), label],
        );
    }
    for target in &found {
        if stack.components.is_empty() {
            ctx.give(*target, stack.item, count as u32);
            continue;
        }
        let mut left = count;
        while left > 0 {
            let take = left.min(i64::from(per_stack));
            let mut piece = stack.clone();
            piece.count = NonZeroU8::new(take as u8).unwrap_or(NonZeroU8::MIN);
            ctx.give_stack(*target, piece);
            left -= take;
        }
    }
    let message = match found.as_slice() {
        [one] => Component::translatable(
            "commands.give.success.single",
            vec![text(count), label, name_of(ctx, *one)],
        ),
        many => Component::translatable(
            "commands.give.success.multiple",
            vec![text(count), label, text(many.len())],
        ),
    };
    reply_and_broadcast(ctx, sender, message);
    ctx.record_result(found.len() as i32);
    CommandOutcome::Success
}

pub(super) enum ItemMatch {
    Any,
    Item(String),
    Tag(String),
}

pub(super) struct ItemPredicate {
    base: ItemMatch,
    tests: Vec<(String, Option<Tag>, bool)>,
}

impl ItemPredicate {
    pub(super) fn parse(token: &str) -> Result<Self, Component> {
        let (head, body) = match token.find('[') {
            Some(at) => {
                let body = token[at + 1..].strip_suffix(']').ok_or_else(|| {
                    Component::translatable("argument.item.component.expected", Vec::new())
                })?;
                (&token[..at], Some(body))
            }
            None => (token, None),
        };
        let base = if head == "*" {
            ItemMatch::Any
        } else if let Some(tag) = head.strip_prefix('#') {
            ItemMatch::Tag(namespaced(tag))
        } else {
            let id = namespaced(head);
            ItemKind::from_name(&id).ok_or_else(|| {
                Component::translatable("argument.item.id.invalid", vec![text(&id)])
            })?;
            ItemMatch::Item(id)
        };
        let mut tests = Vec::new();
        for entry in body.map(split_entries).unwrap_or_default() {
            let entry = entry.trim();
            if entry.is_empty() {
                continue;
            }
            if let Some(key) = entry.strip_prefix('!') {
                tests.push((namespaced(key.trim()), None, false));
            } else if let Some((key, value)) = entry.split_once('=') {
                let tag = rspigot_nbt::parse_tag(value.trim())
                    .map_err(|error| Component::translatable(error.key, Vec::new()))?;
                tests.push((namespaced(key.trim()), Some(tag), true));
            } else {
                tests.push((namespaced(entry), None, true));
            }
        }
        Ok(Self { base, tests })
    }

    pub(super) fn matches(&self, stack: &ItemStack) -> bool {
        let id = stack.item.kind().map_or("minecraft:air", ItemKind::name);
        let base = match &self.base {
            ItemMatch::Any => true,
            ItemMatch::Item(wanted) => wanted == id,
            ItemMatch::Tag(tag) => rspigot_data::tags::tag("minecraft:item", tag)
                .is_some_and(|ids| ids.contains(&u32::from(stack.item.0))),
        };
        base && self.tests.iter().all(|(key, value, present)| {
            match (stack.components.get(key), value) {
                (Some(actual), Some(wanted)) => *present && actual == wanted,
                (Some(_), None) => *present,
                (None, _) => !*present,
            }
        })
    }
}

fn namespaced(name: &str) -> String {
    if name.contains(':') {
        name.to_owned()
    } else {
        format!("minecraft:{name}")
    }
}

fn split_entries(body: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let (mut depth, mut start, mut quote) = (0i32, 0usize, None);
    for (index, c) in body.char_indices() {
        if let Some(open) = quote {
            if c == open {
                quote = None;
            }
            continue;
        }
        match c {
            '"' | '\'' => quote = Some(c),
            '[' | '{' => depth += 1,
            ']' | '}' => depth -= 1,
            ',' if depth == 0 => {
                parts.push(&body[start..index]);
                start = index + 1;
            }
            _ => {}
        }
    }
    parts.push(&body[start..]);
    parts
}

pub(super) fn clear<C: CommandContext>(
    ctx: &mut C,
    sender: CommandSender,
    rest: &str,
) -> CommandOutcome {
    let help = "/clear [targets] [item] [maxCount]";
    let (selector, rest) = match next_argument(rest) {
        Some((token, rest)) => (Some(token), rest),
        None => (None, ""),
    };
    let found = match self_or_selected(ctx, sender, selector) {
        Ok(found) => found,
        Err(outcome) => return outcome,
    };
    let predicate = match next_argument(rest) {
        Some((token, _)) => match ItemPredicate::parse(token) {
            Ok(predicate) => Some(predicate),
            Err(error) => return rejected(ctx, sender, error),
        },
        None => None,
    };
    let limit = match next_argument(rest).and_then(|(_, after)| next_argument(after)) {
        Some((token, _)) => match token.parse::<i32>() {
            Ok(value) if value >= 0 => Some(value as u32),
            Ok(value) => {
                return fail(
                    ctx,
                    sender,
                    "argument.integer.low",
                    vec![text(0), text(value)],
                );
            }
            Err(_) => return usage(ctx, sender, help),
        },
        None => None,
    };
    let mode = match limit {
        None => ClearMode::All,
        Some(0) => ClearMode::Query,
        Some(max) => ClearMode::Remove(max),
    };
    let mut total = 0u32;
    for target in &found {
        let left = match mode {
            ClearMode::Remove(max) => ClearMode::Remove(max.saturating_sub(total)),
            other => other,
        };
        total += clear_one(ctx, *target, predicate.as_ref(), left);
    }
    let who = match found.as_slice() {
        [one] => name_of(ctx, *one),
        many => text(many.len()),
    };
    if total == 0 {
        let key = match found.as_slice() {
            [_] => "commands.clear.failed.single",
            _ => "commands.clear.failed.multiple",
        };
        return fail(ctx, sender, key, vec![who]);
    }
    let key = match (mode == ClearMode::Query, found.len()) {
        (true, 1) => "commands.clear.test.single",
        (true, _) => "commands.clear.test.multiple",
        (false, 1) => "commands.clear.success.single",
        (false, _) => "commands.clear.success.multiple",
    };
    reply_and_broadcast(
        ctx,
        sender,
        Component::translatable(key, vec![text(total), who]),
    );
    ctx.record_result(total as i32);
    CommandOutcome::Success
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ClearMode {
    All,
    Query,
    Remove(u32),
}

fn clear_one<C: CommandContext>(
    ctx: &mut C,
    target: PlayerId,
    predicate: Option<&ItemPredicate>,
    mode: ClearMode,
) -> u32 {
    if predicate.is_none() && mode == ClearMode::All {
        return ctx.clear_inventory(target);
    }
    let mut counted = 0u32;
    for (slot, stack) in ctx.inventory_stacks(target) {
        if !predicate.is_none_or(|p| p.matches(&stack)) {
            continue;
        }
        let present = u32::from(stack.count.get());
        let take = match mode {
            ClearMode::Query => {
                counted += present;
                continue;
            }
            ClearMode::All => present,
            ClearMode::Remove(max) => present.min(max.saturating_sub(counted)),
        };
        if take == 0 {
            break;
        }
        ctx.take_from_slot(target, slot, take);
        counted += take;
    }
    counted
}

pub(super) fn kick<C: CommandContext>(
    ctx: &mut C,
    sender: CommandSender,
    rest: &str,
) -> CommandOutcome {
    let Some((selector, rest)) = next_argument(rest) else {
        return usage(ctx, sender, "/kick <targets> [reason]");
    };
    let found = match targets(ctx, sender, selector) {
        Ok(found) => found,
        Err(outcome) => return outcome,
    };
    let reason = match rest.trim() {
        "" => "Kicked by an operator".to_owned(),
        given => given.to_owned(),
    };
    for target in &found {
        let name = name_of(ctx, *target);
        ctx.kick(*target, Component::text(reason.clone()));
        reply_and_broadcast(
            ctx,
            sender,
            Component::translatable("commands.kick.success", vec![name, text(&reason)]),
        );
    }
    ctx.record_result(found.len() as i32);
    CommandOutcome::Success
}

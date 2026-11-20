use std::num::NonZeroU8;

use rspigot_data::loot::Function;
use rspigot_domain::BlockPos;
use rspigot_domain::item::ItemStack;
use rspigot_domain::text::{Component, NamedColor};
use rspigot_items::ItemKind;
use rspigot_nbt::parse_tag;

use super::blocks::pos_at;
use super::dispatch::{
    CommandContext, CommandOutcome, CommandSender, Target, fail, next_argument, select, usage,
};
use super::slots::{SlotHolder, is_single, slot_ids};

const MAX_COUNT: i32 = 99;

fn text(value: impl ToString) -> Component {
    Component::text(value.to_string())
}

fn reply_error<C: CommandContext>(
    ctx: &mut C,
    sender: CommandSender,
    error: Component,
) -> CommandOutcome {
    ctx.reply(sender, error.color(NamedColor::Red));
    CommandOutcome::Failure
}

fn namespaced(id: &str) -> String {
    if id.contains(':') {
        id.to_owned()
    } else {
        format!("minecraft:{id}")
    }
}

fn split_top_level(body: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let (mut depth, mut start) = (0i32, 0usize);
    let mut quote: Option<char> = None;
    let mut escaped = false;
    for (index, c) in body.char_indices() {
        if let Some(q) = quote {
            if escaped {
                escaped = false;
            } else if c == '\\' {
                escaped = true;
            } else if c == q {
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

pub(super) fn parse_item(token: &str) -> Result<ItemStack, Component> {
    let (name, body) = match token.find('[') {
        Some(at) => {
            let body = token[at + 1..].strip_suffix(']').ok_or_else(|| {
                Component::translatable("argument.item.component.expected", Vec::new())
            })?;
            (&token[..at], Some(body))
        }
        None => (token, None),
    };
    let id = namespaced(name);
    let kind = ItemKind::from_name(&id)
        .ok_or_else(|| Component::translatable("argument.item.id.invalid", vec![text(&id)]))?;
    let mut stack = ItemStack::new(kind.id(), NonZeroU8::MIN);
    for entry in body.map(split_top_level).unwrap_or_default() {
        let entry = entry.trim();
        if entry.is_empty() || entry.starts_with('!') {
            continue;
        }
        let Some((key, value)) = entry.split_once('=') else {
            return Err(Component::translatable(
                "argument.item.component.expected",
                Vec::new(),
            ));
        };
        let value = parse_tag(value.trim())
            .map_err(|error| Component::translatable(error.key, Vec::new()))?;
        stack.components.insert(namespaced(key.trim()), value);
    }
    Ok(stack)
}

fn item_label(stack: &ItemStack) -> Component {
    let name = stack.item.kind().map_or("minecraft:air", ItemKind::name);
    text(format!("[{name}]"))
}

pub(super) fn parse_modifier<C: CommandContext>(
    ctx: &C,
    token: &str,
) -> Result<&'static [Function], Component> {
    let unsupported = |reason: String| {
        Component::translatable(
            "argument.resource_or_id.failed_to_parse",
            vec![text(reason)],
        )
    };
    if !token.starts_with(['{', '[']) {
        return ctx.item_modifier(&namespaced(token)).ok_or_else(|| {
            Component::translatable(
                "argument.resource.not_found",
                vec![text(namespaced(token)), text("minecraft:item_modifier")],
            )
        });
    }
    let value: serde_json::Value =
        serde_json::from_str(token).map_err(|error| unsupported(error.to_string()))?;
    crate::pack_data::parse_functions(&value).map_err(|error| unsupported(error.to_string()))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Op {
    Replace,
    Fill,
    Override,
    Modify,
}

enum Holders {
    Entities(Vec<Target>),
    Block(BlockPos),
}

impl Holders {
    fn holders(&self) -> Vec<SlotHolder> {
        match self {
            Self::Entities(targets) => targets.iter().copied().map(SlotHolder::Entity).collect(),
            Self::Block(pos) => vec![SlotHolder::Block(*pos)],
        }
    }
}

fn position(pos: BlockPos) -> [Component; 3] {
    [text(pos.x), text(pos.y), text(pos.z)]
}

fn holders_at<'a, C: CommandContext>(
    ctx: &mut C,
    sender: CommandSender,
    kind: &str,
    rest: &'a str,
) -> Result<(Holders, &'a str), CommandOutcome> {
    match kind {
        "block" => {
            let (pos, rest) =
                pos_at(ctx, sender, rest).map_err(|error| reply_error(ctx, sender, error))?;
            Ok((Holders::Block(pos), rest))
        }
        _ => {
            let Some((selector, rest)) = next_argument(rest) else {
                return Err(usage(ctx, sender, "/item ... entity <targets> <slots>"));
            };
            Ok((
                Holders::Entities(select(ctx, sender, Some(selector))?),
                rest,
            ))
        }
    }
}

fn slot_list<C: CommandContext>(
    ctx: &mut C,
    sender: CommandSender,
    name: &str,
) -> Result<Vec<i32>, CommandOutcome> {
    slot_ids(name).ok_or_else(|| fail(ctx, sender, "slot.unknown", vec![text(name)]))
}

fn existing<C: CommandContext>(ctx: &C, holder: SlotHolder, ids: &[i32]) -> Vec<i32> {
    ids.iter()
        .copied()
        .filter(|id| ctx.read_slot(holder, *id).is_some())
        .collect()
}

fn check_holders<C: CommandContext>(
    ctx: &mut C,
    sender: CommandSender,
    holders: &Holders,
    ids: &[i32],
    name: &str,
    source: bool,
) -> Result<(), CommandOutcome> {
    let side = if source { "source" } else { "target" };
    if let Holders::Block(pos) = holders
        && ctx.container_slots(*pos) == 0
    {
        let [x, y, z] = position(*pos);
        return Err(fail(
            ctx,
            sender,
            &format!("commands.item.{side}.not_a_container"),
            vec![x, y, z],
        ));
    }
    let any = holders
        .holders()
        .into_iter()
        .any(|holder| !existing(ctx, holder, ids).is_empty());
    if any || holders_many(holders) {
        return Ok(());
    }
    Err(if is_single(name) {
        fail(
            ctx,
            sender,
            &format!("commands.item.{side}.no_such_slot"),
            vec![text(name)],
        )
    } else {
        fail(
            ctx,
            sender,
            &format!("commands.item.{side}.no_such_slot.unnamed"),
            Vec::new(),
        )
    })
}

fn holders_many(holders: &Holders) -> bool {
    matches!(holders, Holders::Entities(targets) if targets.len() > 1)
}

fn provider<C: CommandContext>(
    ctx: &mut C,
    sender: CommandSender,
    rest: &str,
) -> Result<(Vec<Option<ItemStack>>, Option<Component>), CommandOutcome> {
    let Some((word, rest)) = next_argument(rest) else {
        return Err(usage(ctx, sender, "/item replace ... (with|from) ..."));
    };
    if word == "with" {
        let Some((token, rest)) = next_argument(rest) else {
            return Err(usage(ctx, sender, "/item replace ... with <item> [count]"));
        };
        let mut stack = parse_item(token).map_err(|error| reply_error(ctx, sender, error))?;
        if let Some((raw, _)) = next_argument(rest) {
            let Ok(count) = raw.parse::<i32>() else {
                return Err(fail(ctx, sender, "parsing.int.invalid", vec![text(raw)]));
            };
            if !(1..=MAX_COUNT).contains(&count) {
                let key = if count < 1 {
                    "argument.integer.low"
                } else {
                    "argument.integer.high"
                };
                let bound = if count < 1 { 1 } else { MAX_COUNT };
                return Err(fail(ctx, sender, key, vec![text(bound), text(count)]));
            }
            stack.count = NonZeroU8::new(count as u8).unwrap_or(NonZeroU8::MIN);
        }
        let label = item_label(&stack);
        return Ok((vec![Some(stack)], Some(label)));
    }
    if word != "from" {
        return Err(usage(ctx, sender, "/item replace ... (with|from) ..."));
    }
    let Some((kind, rest)) = next_argument(rest) else {
        return Err(usage(
            ctx,
            sender,
            "/item replace ... from (block|entity) ...",
        ));
    };
    let (holders, rest) = holders_at(ctx, sender, kind, rest)?;
    let Some((slots, rest)) = next_argument(rest) else {
        return Err(usage(
            ctx,
            sender,
            "/item replace ... from ... <sourceSlots>",
        ));
    };
    let ids = slot_list(ctx, sender, slots)?;
    check_holders(ctx, sender, &holders, &ids, slots, true)?;
    let functions = match next_argument(rest) {
        Some((token, _)) => {
            Some(parse_modifier(ctx, token).map_err(|e| reply_error(ctx, sender, e))?)
        }
        None => None,
    };
    let mut items = Vec::new();
    for holder in holders.holders() {
        for id in existing(ctx, holder, &ids) {
            let stack = ctx.read_slot(holder, id).flatten();
            let stack = match (stack, functions) {
                (Some(stack), Some(functions)) => {
                    ctx.apply_item_modifier(sender, functions, &stack)
                }
                (stack, _) => stack,
            };
            items.push(stack);
        }
    }
    Ok((items, None))
}

fn distribute<C: CommandContext>(
    ctx: &mut C,
    holder: SlotHolder,
    ids: &[i32],
    items: &[Option<ItemStack>],
    op: Op,
) -> usize {
    let mut changed = 0;
    for (index, id) in existing(ctx, holder, ids).into_iter().enumerate() {
        let stack = match op {
            Op::Fill if !items.is_empty() => items[index % items.len()].clone(),
            Op::Fill | Op::Modify => continue,
            Op::Replace => match items.get(index) {
                Some(stack) => stack.clone(),
                None => continue,
            },
            Op::Override => items.get(index).cloned().flatten(),
        };
        if ctx.write_slot(holder, id, stack) {
            changed += 1;
        }
    }
    changed
}

fn modify_slots<C: CommandContext>(
    ctx: &mut C,
    sender: CommandSender,
    holder: SlotHolder,
    ids: &[i32],
    functions: &[Function],
) -> usize {
    let mut changed = 0;
    for id in existing(ctx, holder, ids) {
        let Some(Some(stack)) = ctx.read_slot(holder, id) else {
            continue;
        };
        let result = ctx.apply_item_modifier(sender, functions, &stack);
        if ctx.write_slot(holder, id, result) {
            changed += 1;
        }
    }
    changed
}

fn report<C: CommandContext>(
    ctx: &mut C,
    sender: CommandSender,
    holders: &Holders,
    op: Op,
    slots: usize,
    targets: usize,
    known: Option<Component>,
) -> CommandOutcome {
    ctx.record_result(i32::try_from(slots).unwrap_or(i32::MAX));
    if slots == 0 {
        return match known {
            Some(item) => fail(
                ctx,
                sender,
                "commands.item.target.failed.known_item",
                vec![item],
            ),
            None => fail(ctx, sender, "commands.item.target.failed", Vec::new()),
        };
    }
    let message = match (holders, op) {
        (Holders::Block(pos), Op::Modify) => {
            let [x, y, z] = position(*pos);
            Component::translatable(
                "commands.item.block.modify.success",
                vec![text(slots), x, y, z],
            )
        }
        (Holders::Block(pos), _) => {
            let [x, y, z] = position(*pos);
            let mut args = vec![text(slots), x, y, z];
            let key = match known {
                Some(item) => {
                    args.push(item);
                    "commands.item.block.replace.success.known_item"
                }
                None => "commands.item.block.replace.success",
            };
            Component::translatable(key, args)
        }
        (Holders::Entities(list), op) => {
            let family = if op == Op::Modify {
                "modify"
            } else {
                "replace"
            };
            let single = list.len() == 1;
            let (suffix, first, second) = if single {
                ("single", text(slots), Some(ctx.target_name(list[0])))
            } else {
                ("multiple", text(targets), None)
            };
            let mut args = vec![first];
            args.extend(second);
            let mut key = format!("commands.item.entity.{family}.success.{suffix}");
            if op != Op::Modify
                && let Some(item) = known
            {
                args.push(item);
                key.push_str(".known_item");
            }
            Component::translatable(key, args)
        }
    };
    ctx.reply(sender, message);
    CommandOutcome::Success
}

pub(super) fn run<C: CommandContext>(
    ctx: &mut C,
    sender: CommandSender,
    rest: &str,
) -> CommandOutcome {
    let help = "/item (replace|fill|override|modify) (block|entity) <target> <slots> ...";
    let Some((word, rest)) = next_argument(rest) else {
        return usage(ctx, sender, help);
    };
    let op = match word {
        "replace" => Op::Replace,
        "fill" => Op::Fill,
        "override" => Op::Override,
        "modify" => Op::Modify,
        _ => return usage(ctx, sender, help),
    };
    let Some((kind, rest)) = next_argument(rest) else {
        return usage(ctx, sender, help);
    };
    if !matches!(kind, "block" | "entity") {
        return usage(ctx, sender, help);
    }
    let (holders, rest) = match holders_at(ctx, sender, kind, rest) {
        Ok(found) => found,
        Err(outcome) => return outcome,
    };
    let Some((slots, rest)) = next_argument(rest) else {
        return usage(ctx, sender, help);
    };
    let ids = match slot_list(ctx, sender, slots) {
        Ok(ids) => ids,
        Err(outcome) => return outcome,
    };
    if let Err(outcome) = check_holders(ctx, sender, &holders, &ids, slots, false) {
        return outcome;
    }
    let (items, known, functions) = if op == Op::Modify {
        let Some((token, _)) = next_argument(rest) else {
            return usage(ctx, sender, help);
        };
        match parse_modifier(ctx, token) {
            Ok(functions) => (Vec::new(), None, functions),
            Err(error) => return reply_error(ctx, sender, error),
        }
    } else {
        match provider(ctx, sender, rest) {
            Ok((items, known)) => (items, known, &[][..]),
            Err(outcome) => return outcome,
        }
    };
    let (mut slot_total, mut target_total) = (0, 0);
    for holder in holders.holders() {
        let changed = if op == Op::Modify {
            modify_slots(ctx, sender, holder, &ids, functions)
        } else {
            distribute(ctx, holder, &ids, &items, op)
        };
        slot_total += changed;
        target_total += usize::from(changed > 0);
    }
    report(ctx, sender, &holders, op, slot_total, target_total, known)
}

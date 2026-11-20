use rspigot_domain::item::ItemStack;
use rspigot_domain::player::PlayerId;
use rspigot_domain::text::{Component, NamedColor};
use rspigot_domain::{BlockPos, Vec3};

use super::blocks::pos_at;
use super::dispatch::{
    CommandContext, CommandOutcome, CommandSender, Target, fail, next_argument, parse_vec3, select,
    select_one, usage,
};
use super::item_commands::parse_item;
use super::loot_source::{LootSource, LootTool};
use super::slots::{SlotHolder, is_single, slot_ids};

const SOURCES: [&str; 4] = ["fish", "kill", "loot", "mine"];

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

enum Sink {
    Give(Vec<PlayerId>),
    Insert(BlockPos),
    Spawn(Vec3),
    ReplaceBlock {
        pos: BlockPos,
        slot: i32,
        count: Option<usize>,
    },
    ReplaceEntity {
        targets: Vec<Target>,
        slot: i32,
        count: Option<usize>,
    },
}

fn slot_and_count<'a, C: CommandContext>(
    ctx: &mut C,
    sender: CommandSender,
    rest: &'a str,
) -> Result<(i32, Option<usize>, &'a str), CommandOutcome> {
    let Some((name, rest)) = next_argument(rest) else {
        return Err(usage(ctx, sender, "/loot replace ... <slot> [<count>] ..."));
    };
    let first = match slot_ids(name)
        .filter(|_| is_single(name))
        .and_then(|ids| ids.first().copied())
    {
        Some(first) => first,
        None => return Err(fail(ctx, sender, "slot.unknown", vec![text(name)])),
    };
    match next_argument(rest) {
        Some((word, after)) if !SOURCES.contains(&word) => match word.parse::<i32>() {
            Ok(count) if count >= 0 => Ok((first, Some(count as usize), after)),
            Ok(count) => Err(fail(
                ctx,
                sender,
                "argument.integer.low",
                vec![text(0), text(count)],
            )),
            Err(_) => Err(fail(ctx, sender, "parsing.int.invalid", vec![text(word)])),
        },
        _ => Ok((first, None, rest)),
    }
}

fn sink_of<'a, C: CommandContext>(
    ctx: &mut C,
    sender: CommandSender,
    word: &str,
    rest: &'a str,
) -> Result<(Sink, &'a str), CommandOutcome> {
    match word {
        "give" => {
            let Some((selector, rest)) = next_argument(rest) else {
                return Err(usage(ctx, sender, "/loot give <players> ..."));
            };
            match ctx.select_players(sender, selector) {
                Ok(players) if !players.is_empty() => Ok((Sink::Give(players), rest)),
                Ok(_) => Err(fail(
                    ctx,
                    sender,
                    "argument.entity.notfound.player",
                    Vec::new(),
                )),
                Err(error) => Err(reply_error(ctx, sender, error)),
            }
        }
        "insert" => {
            let (pos, rest) =
                pos_at(ctx, sender, rest).map_err(|error| reply_error(ctx, sender, error))?;
            Ok((Sink::Insert(pos), rest))
        }
        "spawn" => {
            let mut tokens = [""; 3];
            let mut rest = rest;
            for token in &mut tokens {
                let Some((found, after)) = next_argument(rest) else {
                    return Err(fail(ctx, sender, "argument.pos3d.incomplete", Vec::new()));
                };
                *token = found;
                rest = after;
            }
            let position =
                parse_vec3(ctx, sender, tokens).map_err(|error| reply_error(ctx, sender, error))?;
            Ok((Sink::Spawn(position), rest))
        }
        "replace" => {
            let Some((kind, rest)) = next_argument(rest) else {
                return Err(usage(ctx, sender, "/loot replace (block|entity) ..."));
            };
            if kind == "block" {
                let (pos, rest) =
                    pos_at(ctx, sender, rest).map_err(|error| reply_error(ctx, sender, error))?;
                let (slot, count, rest) = slot_and_count(ctx, sender, rest)?;
                Ok((Sink::ReplaceBlock { pos, slot, count }, rest))
            } else if kind == "entity" {
                let Some((selector, rest)) = next_argument(rest) else {
                    return Err(usage(ctx, sender, "/loot replace entity <entities> ..."));
                };
                let targets = select(ctx, sender, Some(selector))?;
                let (slot, count, rest) = slot_and_count(ctx, sender, rest)?;
                Ok((
                    Sink::ReplaceEntity {
                        targets,
                        slot,
                        count,
                    },
                    rest,
                ))
            } else {
                Err(usage(ctx, sender, "/loot replace (block|entity) ..."))
            }
        }
        _ => Err(usage(ctx, sender, "/loot (give|insert|replace|spawn) ...")),
    }
}

fn tool_of<C: CommandContext>(
    ctx: &mut C,
    sender: CommandSender,
    rest: &str,
) -> Result<LootTool, CommandOutcome> {
    match next_argument(rest) {
        None | Some(("mainhand", _)) => Ok(LootTool::Mainhand),
        Some(("offhand", _)) => Ok(LootTool::Offhand),
        Some((token, _)) => parse_item(token)
            .map(LootTool::Stack)
            .map_err(|error| reply_error(ctx, sender, error)),
    }
}

fn source_of<C: CommandContext>(
    ctx: &mut C,
    sender: CommandSender,
    word: &str,
    rest: &str,
) -> Result<(LootSource, Option<String>), CommandOutcome> {
    let help = "/loot ... (fish|kill|loot|mine) ...";
    match word {
        "loot" => {
            let Some((table, _)) = next_argument(rest) else {
                return Err(usage(ctx, sender, help));
            };
            Ok((LootSource::Table(table.to_owned()), Some(table.to_owned())))
        }
        "fish" => {
            let Some((table, rest)) = next_argument(rest) else {
                return Err(usage(ctx, sender, help));
            };
            let (_, rest) =
                pos_at(ctx, sender, rest).map_err(|error| reply_error(ctx, sender, error))?;
            let tool = tool_of(ctx, sender, rest)?;
            Ok((
                LootSource::Fish {
                    table: table.to_owned(),
                    tool,
                },
                Some(table.to_owned()),
            ))
        }
        "mine" => {
            let (pos, rest) =
                pos_at(ctx, sender, rest).map_err(|error| reply_error(ctx, sender, error))?;
            let tool = tool_of(ctx, sender, rest)?;
            Ok((LootSource::Mine { pos, tool }, None))
        }
        "kill" => {
            let Some((selector, _)) = next_argument(rest) else {
                return Err(usage(ctx, sender, help));
            };
            let target = select_one(ctx, sender, selector)?;
            Ok((LootSource::Kill(target), None))
        }
        _ => Err(usage(ctx, sender, help)),
    }
}

fn item_total(stacks: &[ItemStack]) -> u32 {
    stacks
        .iter()
        .map(|stack| u32::from(stack.count.get()))
        .sum()
}

fn dropped<C: CommandContext>(
    ctx: &mut C,
    sender: CommandSender,
    stacks: &[ItemStack],
    table: Option<&str>,
) -> CommandOutcome {
    let mut args = match stacks {
        [only] => vec![
            text(only.count.get()),
            text(format!(
                "[{}]",
                only.item
                    .kind()
                    .map_or("minecraft:air", rspigot_items::ItemKind::name)
            )),
        ],
        _ => vec![text(item_total(stacks))],
    };
    let mut key = if stacks.len() == 1 {
        "commands.drop.success.single".to_owned()
    } else {
        "commands.drop.success.multiple".to_owned()
    };
    if let Some(table) = table {
        key.push_str("_with_table");
        args.push(text(table));
    }
    ctx.reply(sender, Component::translatable(key, args));
    ctx.record_result(i32::try_from(stacks.len()).unwrap_or(i32::MAX));
    CommandOutcome::Success
}

fn replaced<C: CommandContext>(
    ctx: &mut C,
    holders: &[SlotHolder],
    slot: i32,
    count: Option<usize>,
    stacks: &[ItemStack],
) -> Vec<usize> {
    let size = count.unwrap_or(stacks.len());
    holders
        .iter()
        .map(|holder| {
            (0..size)
                .filter(|offset| {
                    let stack = stacks.get(*offset).cloned();
                    ctx.write_slot(*holder, slot + *offset as i32, stack)
                })
                .count()
        })
        .collect()
}

pub(super) fn run<C: CommandContext>(
    ctx: &mut C,
    sender: CommandSender,
    rest: &str,
) -> CommandOutcome {
    let Some((word, rest)) = next_argument(rest) else {
        return usage(ctx, sender, "/loot (give|insert|replace|spawn) ...");
    };
    let (sink, rest) = match sink_of(ctx, sender, word, rest) {
        Ok(found) => found,
        Err(outcome) => return outcome,
    };
    let Some((kind, rest)) = next_argument(rest) else {
        return usage(ctx, sender, "/loot ... (fish|kill|loot|mine) ...");
    };
    let (source, table) = match source_of(ctx, sender, kind, rest) {
        Ok(found) => found,
        Err(outcome) => return outcome,
    };
    let stacks = match ctx.roll_loot(sender, &source) {
        Ok(stacks) => stacks,
        Err(error) => return reply_error(ctx, sender, error),
    };
    let table = table.as_deref();
    match sink {
        Sink::Give(players) => {
            for player in players {
                for stack in &stacks {
                    ctx.give_stack(player, stack.clone());
                }
            }
            dropped(ctx, sender, &stacks, table)
        }
        Sink::Spawn(at) => {
            for stack in &stacks {
                ctx.drop_stack(at, stack.clone());
            }
            dropped(ctx, sender, &stacks, table)
        }
        Sink::Insert(pos) => {
            if ctx.container_slots(pos) == 0 {
                return fail(
                    ctx,
                    sender,
                    "commands.item.target.not_a_container",
                    vec![text(pos.x), text(pos.y), text(pos.z)],
                );
            }
            let centre = Vec3::new(
                f64::from(pos.x) + 0.5,
                f64::from(pos.y) + 0.5,
                f64::from(pos.z) + 0.5,
            );
            for stack in &stacks {
                if let Some(Some(rest)) = ctx.insert_stack(pos, stack.clone()) {
                    ctx.drop_stack(centre, rest);
                }
            }
            dropped(ctx, sender, &stacks, table)
        }
        Sink::ReplaceBlock { pos, slot, count } => {
            if ctx.container_slots(pos) == 0 {
                return fail(
                    ctx,
                    sender,
                    "commands.item.target.not_a_container",
                    vec![text(pos.x), text(pos.y), text(pos.z)],
                );
            }
            let changed = replaced(ctx, &[SlotHolder::Block(pos)], slot, count, &stacks)[0];
            ctx.record_result(i32::try_from(stacks.len()).unwrap_or(i32::MAX));
            if changed == 0 {
                return fail(ctx, sender, "commands.item.target.failed", Vec::new());
            }
            ctx.reply(
                sender,
                Component::translatable(
                    "commands.item.block.replace.success",
                    vec![text(changed), text(pos.x), text(pos.y), text(pos.z)],
                ),
            );
            CommandOutcome::Success
        }
        Sink::ReplaceEntity {
            targets,
            slot,
            count,
        } => {
            let holders: Vec<SlotHolder> =
                targets.iter().copied().map(SlotHolder::Entity).collect();
            let changed = replaced(ctx, &holders, slot, count, &stacks);
            ctx.record_result(i32::try_from(stacks.len()).unwrap_or(i32::MAX));
            let total: usize = changed.iter().sum();
            if total == 0 {
                return fail(ctx, sender, "commands.item.target.failed", Vec::new());
            }
            let message = match targets.as_slice() {
                [single] => Component::translatable(
                    "commands.item.entity.replace.success.single",
                    vec![text(total), ctx.target_name(*single)],
                ),
                many => Component::translatable(
                    "commands.item.entity.replace.success.multiple",
                    vec![text(many.len())],
                ),
            };
            ctx.reply(sender, message);
            CommandOutcome::Success
        }
    }
}

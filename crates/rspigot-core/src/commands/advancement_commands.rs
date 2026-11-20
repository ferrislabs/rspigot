use rspigot_data::advancements::{self, Advancement};
use rspigot_domain::player::PlayerId;
use rspigot_domain::text::Component;

use super::dispatch::{CommandContext, CommandOutcome, CommandSender, fail, next_argument, usage};

fn text(value: impl ToString) -> Component {
    Component::text(value.to_string())
}

fn namespaced(id: &str) -> String {
    if id.contains(':') {
        id.to_owned()
    } else {
        format!("minecraft:{id}")
    }
}

fn label(advancement: &Advancement) -> Component {
    match &advancement.display {
        Some(display) => Component::text("[")
            .append(Component::translatable(display.title, Vec::new()))
            .append(Component::text("]")),
        None => text(advancement.id),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mode {
    Only,
    From,
    Until,
    Through,
    Everything,
}

fn ancestors(index: usize) -> Vec<usize> {
    let mut chain = Vec::new();
    let mut parent = advancements::all()[index]
        .parent
        .and_then(advancements::find);
    while let Some((at, found)) = parent {
        chain.push(at);
        parent = found.parent.and_then(advancements::find);
    }
    chain.reverse();
    chain
}

fn descendants(index: usize) -> Vec<usize> {
    (0..advancements::all().len())
        .filter(|candidate| *candidate != index && ancestors(*candidate).contains(&index))
        .collect()
}

fn selection(mode: Mode, index: usize) -> Vec<usize> {
    match mode {
        Mode::Only => vec![index],
        Mode::From => std::iter::once(index).chain(descendants(index)).collect(),
        Mode::Until => ancestors(index).into_iter().chain([index]).collect(),
        Mode::Through => ancestors(index)
            .into_iter()
            .chain([index])
            .chain(descendants(index))
            .collect(),
        Mode::Everything => (0..advancements::all().len()).collect(),
    }
}

fn parse_mode(word: &str) -> Option<Mode> {
    Some(match word {
        "only" => Mode::Only,
        "from" => Mode::From,
        "until" => Mode::Until,
        "through" => Mode::Through,
        "everything" => Mode::Everything,
        _ => return None,
    })
}

fn perform<C: CommandContext>(
    ctx: &mut C,
    grant: bool,
    player: PlayerId,
    advancement: &Advancement,
    only: Option<&str>,
) -> bool {
    let names: Vec<&'static str> = match only {
        Some(criterion) => advancement
            .criteria
            .iter()
            .filter(|known| known.name == criterion)
            .map(|known| known.name)
            .collect(),
        None => advancement
            .criteria
            .iter()
            .map(|known| known.name)
            .collect(),
    };
    let mut changed = false;
    for name in names {
        changed |= if grant {
            ctx.grant_criterion(player, advancement.id, name)
        } else {
            ctx.revoke_criterion(player, advancement.id, name)
        };
    }
    changed
}

pub(super) fn run<C: CommandContext>(
    ctx: &mut C,
    sender: CommandSender,
    rest: &str,
) -> CommandOutcome {
    let help = "/advancement (grant|revoke) <targets> (only|from|until|through|everything) [<advancement> [<criterion>]]";
    let Some((action, rest)) = next_argument(rest) else {
        return usage(ctx, sender, help);
    };
    let grant = match action {
        "grant" => true,
        "revoke" => false,
        _ => return usage(ctx, sender, help),
    };
    let Some((selector, rest)) = next_argument(rest) else {
        return usage(ctx, sender, help);
    };
    let Some((mode_word, rest)) = next_argument(rest) else {
        return usage(ctx, sender, help);
    };
    let Some(mode) = parse_mode(mode_word) else {
        return usage(ctx, sender, help);
    };
    let players = match ctx.select_players(sender, selector) {
        Ok(found) if !found.is_empty() => found,
        Ok(_) => {
            return fail(ctx, sender, "argument.entity.notfound.player", Vec::new());
        }
        Err(message) => {
            ctx.reply(sender, message.color(rspigot_domain::text::NamedColor::Red));
            return CommandOutcome::Failure;
        }
    };
    let (root, criterion) = if mode == Mode::Everything {
        (None, None)
    } else {
        let Some((id, rest)) = next_argument(rest) else {
            return usage(ctx, sender, help);
        };
        let id = namespaced(id);
        let Some((index, found)) = advancements::find(&id) else {
            return fail(
                ctx,
                sender,
                "argument.resource.not_found",
                vec![text(&id), text("minecraft:advancement")],
            );
        };
        let criterion = match (mode, next_argument(rest)) {
            (Mode::Only, Some((name, _))) => {
                let wanted = [name.to_owned(), namespaced(name)];
                let known = found
                    .criteria
                    .iter()
                    .find(|known| wanted.iter().any(|w| w == known.name));
                match known {
                    Some(known) => Some(known.name),
                    None => {
                        return fail(
                            ctx,
                            sender,
                            "commands.advancement.criterionNotFound",
                            vec![text(&id), text(name)],
                        );
                    }
                }
            }
            _ => None,
        };
        (Some((index, found)), criterion)
    };
    let list = match root {
        Some((index, _)) => selection(mode, index),
        None => selection(Mode::Everything, 0),
    };
    let verb = if grant { "grant" } else { "revoke" };
    let who = if let [only] = players.as_slice() {
        ctx.player_name(*only).map_or_else(|| text(""), text)
    } else {
        text(players.len())
    };
    let target_key = if players.len() == 1 { "one" } else { "many" };
    let mut count = 0usize;
    for player in &players {
        for index in &list {
            if perform(ctx, grant, *player, &advancements::all()[*index], criterion) {
                count += 1;
            }
        }
    }
    ctx.record_result(i32::try_from(count).unwrap_or(i32::MAX));
    if let (Some(name), Some((_, found))) = (criterion, root) {
        let key = format!("commands.advancement.{verb}.criterion.to.{target_key}");
        let args = vec![text(name), label(found), who];
        if count == 0 {
            return fail(ctx, sender, &format!("{key}.failure"), args);
        }
        ctx.reply(
            sender,
            Component::translatable(format!("{key}.success"), args),
        );
        return CommandOutcome::Success;
    }
    let (kind, first) = match (list.as_slice(), root) {
        ([only], _) => ("one", label(&advancements::all()[*only])),
        _ => ("many", text(if count == 0 { list.len() } else { count })),
    };
    let key = format!("commands.advancement.{verb}.{kind}.to.{target_key}");
    let args = vec![first, who];
    if count == 0 {
        return fail(ctx, sender, &format!("{key}.failure"), args);
    }
    ctx.reply(
        sender,
        Component::translatable(format!("{key}.success"), args),
    );
    CommandOutcome::Success
}

use rspigot_domain::Identifier;
use rspigot_domain::text::{Component, NamedColor};
use rspigot_nbt::{Compound, NbtPath, Tag};

use super::dispatch::{CommandContext, CommandOutcome, CommandSender, fail, next_argument, usage};
use super::source::DataTarget;

const USAGE: &str = "/data <get|merge|modify|remove> ...";

struct Spec {
    target: DataTarget,
    kind: &'static str,
}

fn error<C: CommandContext>(
    ctx: &mut C,
    sender: CommandSender,
    key: &str,
    args: Vec<Component>,
) -> CommandOutcome {
    fail(ctx, sender, key, args)
}

fn reply_err<C: CommandContext>(
    ctx: &mut C,
    sender: CommandSender,
    e: Component,
) -> CommandOutcome {
    ctx.reply(sender, e.color(NamedColor::Red));
    CommandOutcome::Failure
}

fn target<'a, C: CommandContext>(
    ctx: &mut C,
    sender: CommandSender,
    input: &'a str,
) -> Result<(Spec, &'a str), CommandOutcome> {
    let Some((kind, rest)) = next_argument(input) else {
        return Err(usage(ctx, sender, USAGE));
    };
    match kind {
        "entity" => {
            let Some((token, rest)) = next_argument(rest) else {
                return Err(usage(ctx, sender, USAGE));
            };
            let found = ctx
                .select_entities(sender, token)
                .map_err(|e| reply_err(ctx, sender, e))?;
            match found.as_slice() {
                [single] => Ok((
                    Spec {
                        target: DataTarget::Entity(*single),
                        kind: "entity",
                    },
                    rest,
                )),
                [] => Err(error(
                    ctx,
                    sender,
                    "argument.entity.notfound.entity",
                    Vec::new(),
                )),
                _ => Err(error(ctx, sender, "argument.entity.toomany", Vec::new())),
            }
        }
        "block" => {
            let mut cursor = rest;
            let mut raw = [""; 3];
            for slot in &mut raw {
                let Some((token, after)) = next_argument(cursor) else {
                    return Err(error(ctx, sender, "argument.pos3d.incomplete", Vec::new()));
                };
                *slot = token;
                cursor = after;
            }
            let position = super::dispatch::parse_vec3(ctx, sender, raw)
                .map_err(|e| reply_err(ctx, sender, e))?;
            let pos = rspigot_domain::BlockPos::new(
                position.x.floor() as i32,
                position.y.floor() as i32,
                position.z.floor() as i32,
            );
            Ok((
                Spec {
                    target: DataTarget::Block(pos),
                    kind: "block",
                },
                cursor,
            ))
        }
        "storage" => {
            let Some((token, rest)) = next_argument(rest) else {
                return Err(usage(ctx, sender, USAGE));
            };
            let Ok(id) = Identifier::parse(token) else {
                return Err(error(ctx, sender, "argument.id.invalid", Vec::new()));
            };
            Ok((
                Spec {
                    target: DataTarget::Storage(id),
                    kind: "storage",
                },
                rest,
            ))
        }
        _ => Err(usage(ctx, sender, USAGE)),
    }
}

pub(super) struct DataSource {
    pub data: Compound,
    pub path: Option<NbtPath>,
}

pub(super) fn read_source<C: CommandContext>(
    ctx: &mut C,
    sender: CommandSender,
    input: &str,
) -> Result<DataSource, CommandOutcome> {
    let (spec, rest) = target(ctx, sender, input)?;
    let data = compound_of(read(ctx, sender, &spec)?);
    let path = match next_argument(rest) {
        Some((token, _)) => Some(path_of(ctx, sender, token)?),
        None => None,
    };
    Ok(DataSource { data, path })
}

fn label<C: CommandContext>(ctx: &C, spec: &Spec) -> Component {
    match &spec.target {
        DataTarget::Entity(target) => ctx.target_name(*target),
        DataTarget::Block(pos) => Component::text(format!("{}, {}, {}", pos.x, pos.y, pos.z)),
        DataTarget::Storage(id) => Component::text(id.to_string()),
    }
}

fn path_of<C: CommandContext>(
    ctx: &mut C,
    sender: CommandSender,
    token: &str,
) -> Result<NbtPath, CommandOutcome> {
    NbtPath::parse(token).map_err(|e| error(ctx, sender, e.key, Vec::new()))
}

fn read<C: CommandContext>(
    ctx: &mut C,
    sender: CommandSender,
    spec: &Spec,
) -> Result<Tag, CommandOutcome> {
    ctx.read_data(&spec.target)
        .map(Tag::Compound)
        .map_err(|e| reply_err(ctx, sender, e))
}

fn compound_of(tag: Tag) -> Compound {
    match tag {
        Tag::Compound(compound) => compound,
        _ => Compound::new(),
    }
}

fn save<C: CommandContext>(
    ctx: &mut C,
    sender: CommandSender,
    spec: &Spec,
    tag: Tag,
) -> Result<(), CommandOutcome> {
    ctx.write_data(&spec.target, compound_of(tag))
        .map_err(|e| reply_err(ctx, sender, e))
}

fn modified<C: CommandContext>(ctx: &mut C, sender: CommandSender, spec: &Spec) -> CommandOutcome {
    let key = format!("commands.data.{}.modified", spec.kind);
    let name = label(ctx, spec);
    ctx.reply(sender, Component::translatable(key, vec![name]));
    ctx.record_result(1);
    CommandOutcome::Success
}

fn numeric(tag: &Tag) -> Option<f64> {
    match tag {
        Tag::Byte(_)
        | Tag::Short(_)
        | Tag::Int(_)
        | Tag::Long(_)
        | Tag::Float(_)
        | Tag::Double(_) => tag.as_f64(),
        _ => None,
    }
}

fn get<C: CommandContext>(ctx: &mut C, sender: CommandSender, rest: &str) -> CommandOutcome {
    let (spec, rest) = match target(ctx, sender, rest) {
        Ok(found) => found,
        Err(outcome) => return outcome,
    };
    let data = match read(ctx, sender, &spec) {
        Ok(data) => data,
        Err(outcome) => return outcome,
    };
    let name = label(ctx, &spec);
    let Some((path_text, rest)) = next_argument(rest) else {
        let key = format!("commands.data.{}.query", spec.kind);
        ctx.reply(
            sender,
            Component::translatable(key, vec![name, Component::text(data.to_string())]),
        );
        let size = i32::try_from(compound_of(data).len()).unwrap_or(i32::MAX);
        ctx.record_result(size);
        return CommandOutcome::Success;
    };
    let path = match path_of(ctx, sender, path_text) {
        Ok(path) => path,
        Err(outcome) => return outcome,
    };
    let found = path.get(&data);
    let [value] = found.as_slice() else {
        return error(
            ctx,
            sender,
            "commands.data.get.unknown",
            vec![Component::text(path.source())],
        );
    };
    let value = (*value).clone();
    let scale = match next_argument(rest) {
        Some((text, _)) => match text.parse::<f64>() {
            Ok(scale) => Some(scale),
            Err(_) => {
                return error(
                    ctx,
                    sender,
                    "parsing.double.invalid",
                    vec![Component::text(text)],
                );
            }
        },
        None => None,
    };
    let result = match (&value, scale) {
        (value, Some(scale)) => match numeric(value) {
            Some(number) => (number * scale) as i32,
            None => {
                return error(
                    ctx,
                    sender,
                    "commands.data.get.invalid",
                    vec![Component::text(path.source())],
                );
            }
        },
        (Tag::List(list), None) => list.len() as i32,
        (Tag::Compound(compound), None) => compound.len() as i32,
        (Tag::String(text), None) => text.chars().count() as i32,
        (value, None) => numeric(value).map_or(1, |number| number as i32),
    };
    let key = if scale.is_some() {
        format!("commands.data.{}.get", spec.kind)
    } else {
        format!("commands.data.{}.query", spec.kind)
    };
    let args = if let Some(scale) = scale {
        vec![
            Component::text(path.source()),
            name,
            Component::text(format!("{scale:.2}")),
            Component::text(result.to_string()),
        ]
    } else {
        vec![name, Component::text(value.to_string())]
    };
    ctx.reply(sender, Component::translatable(key, args));
    ctx.record_result(result);
    CommandOutcome::Success
}

fn merge<C: CommandContext>(ctx: &mut C, sender: CommandSender, rest: &str) -> CommandOutcome {
    let (spec, rest) = match target(ctx, sender, rest) {
        Ok(found) => found,
        Err(outcome) => return outcome,
    };
    let Ok((patch, used)) = rspigot_nbt::parse_compound(rest.trim()) else {
        return usage(ctx, sender, "/data merge <target> <nbt>");
    };
    if used != rest.trim().len() {
        return usage(ctx, sender, "/data merge <target> <nbt>");
    }
    let mut data = match read(ctx, sender, &spec) {
        Ok(data) => compound_of(data),
        Err(outcome) => return outcome,
    };
    let before = data.clone();
    for (key, value) in patch.iter() {
        let merged = match (data.get(key), value) {
            (Some(Tag::Compound(existing)), Tag::Compound(extra)) => {
                Tag::Compound(merge_compounds(existing, extra))
            }
            _ => value.clone(),
        };
        data.insert(key, merged);
    }
    if data == before {
        return error(ctx, sender, "commands.data.merge.failed", Vec::new());
    }
    if let Err(outcome) = save(ctx, sender, &spec, Tag::Compound(data)) {
        return outcome;
    }
    modified(ctx, sender, &spec)
}

fn merge_compounds(base: &Compound, extra: &Compound) -> Compound {
    let mut out = base.clone();
    for (key, value) in extra.iter() {
        let merged = match (out.get(key), value) {
            (Some(Tag::Compound(existing)), Tag::Compound(more)) => {
                Tag::Compound(merge_compounds(existing, more))
            }
            _ => value.clone(),
        };
        out.insert(key, merged);
    }
    out
}

fn remove<C: CommandContext>(ctx: &mut C, sender: CommandSender, rest: &str) -> CommandOutcome {
    let (spec, rest) = match target(ctx, sender, rest) {
        Ok(found) => found,
        Err(outcome) => return outcome,
    };
    let Some((path_text, _)) = next_argument(rest) else {
        return usage(ctx, sender, "/data remove <target> <path>");
    };
    let path = match path_of(ctx, sender, path_text) {
        Ok(path) => path,
        Err(outcome) => return outcome,
    };
    let mut data = match read(ctx, sender, &spec) {
        Ok(data) => data,
        Err(outcome) => return outcome,
    };
    if path.remove(&mut data) == 0 {
        return error(ctx, sender, "commands.data.merge.failed", Vec::new());
    }
    if let Err(outcome) = save(ctx, sender, &spec, data) {
        return outcome;
    }
    modified(ctx, sender, &spec)
}

enum Operation {
    Set,
    Merge,
    Append,
    Prepend,
    Insert(i32),
}

fn sources<C: CommandContext>(
    ctx: &mut C,
    sender: CommandSender,
    rest: &str,
) -> Result<Vec<Tag>, CommandOutcome> {
    let Some((kind, rest)) = next_argument(rest) else {
        return Err(usage(
            ctx,
            sender,
            "/data modify <target> <path> <operation> <source>",
        ));
    };
    match kind {
        "value" => {
            let text = rest.trim();
            let tag =
                rspigot_nbt::parse_tag(text).map_err(|e| error(ctx, sender, e.key, Vec::new()))?;
            Ok(vec![tag])
        }
        "from" | "string" => {
            let (spec, rest) = target(ctx, sender, rest)?;
            let data = read(ctx, sender, &spec)?;
            let mut values = match next_argument(rest) {
                Some((path_text, _)) => {
                    let path = path_of(ctx, sender, path_text)?;
                    let found: Vec<Tag> = path.get(&data).into_iter().cloned().collect();
                    if found.is_empty() {
                        return Err(error(
                            ctx,
                            sender,
                            "commands.data.get.unknown",
                            vec![Component::text(path.source())],
                        ));
                    }
                    found
                }
                None => vec![data],
            };
            if kind == "string" {
                let mut tokens = rest;
                let _ = next_argument(tokens).map(|(_, after)| tokens = after);
                let numbers: Vec<usize> = tokens
                    .split_whitespace()
                    .filter_map(|t| t.parse().ok())
                    .collect();
                values = values
                    .into_iter()
                    .map(|tag| match tag {
                        Tag::String(text) => {
                            let chars: Vec<char> = text.chars().collect();
                            let start = numbers.first().copied().unwrap_or(0);
                            let end = numbers.get(1).copied().unwrap_or(chars.len());
                            if start > end || end > chars.len() {
                                return Err(error(
                                    ctx,
                                    sender,
                                    "commands.data.modify.invalid_substring",
                                    vec![
                                        Component::text(start.to_string()),
                                        Component::text(end.to_string()),
                                    ],
                                ));
                            }
                            Ok(Tag::String(chars[start..end].iter().collect()))
                        }
                        _ => Err(error(
                            ctx,
                            sender,
                            "commands.data.modify.expected_string",
                            Vec::new(),
                        )),
                    })
                    .collect::<Result<_, _>>()?;
            }
            Ok(values)
        }
        _ => Err(usage(
            ctx,
            sender,
            "/data modify <target> <path> <operation> <source>",
        )),
    }
}

fn modify<C: CommandContext>(ctx: &mut C, sender: CommandSender, rest: &str) -> CommandOutcome {
    let (spec, rest) = match target(ctx, sender, rest) {
        Ok(found) => found,
        Err(outcome) => return outcome,
    };
    let Some((path_text, rest)) = next_argument(rest) else {
        return usage(
            ctx,
            sender,
            "/data modify <target> <path> <operation> <source>",
        );
    };
    let path = match path_of(ctx, sender, path_text) {
        Ok(path) => path,
        Err(outcome) => return outcome,
    };
    let Some((operation, rest)) = next_argument(rest) else {
        return usage(
            ctx,
            sender,
            "/data modify <target> <path> <operation> <source>",
        );
    };
    let (operation, rest) = match operation {
        "set" => (Operation::Set, rest),
        "merge" => (Operation::Merge, rest),
        "append" => (Operation::Append, rest),
        "prepend" => (Operation::Prepend, rest),
        "insert" => {
            let Some((index, after)) = next_argument(rest) else {
                return usage(
                    ctx,
                    sender,
                    "/data modify <target> <path> insert <index> <source>",
                );
            };
            let Ok(index) = index.parse::<i32>() else {
                return error(
                    ctx,
                    sender,
                    "parsing.int.invalid",
                    vec![Component::text(index)],
                );
            };
            (Operation::Insert(index), after)
        }
        _ => {
            return usage(
                ctx,
                sender,
                "/data modify <target> <path> <operation> <source>",
            );
        }
    };
    let values = match sources(ctx, sender, rest) {
        Ok(values) => values,
        Err(outcome) => return outcome,
    };
    let mut data = match read(ctx, sender, &spec) {
        Ok(data) => data,
        Err(outcome) => return outcome,
    };
    let before = data.clone();
    let failure = std::cell::RefCell::new(None::<&'static str>);
    match operation {
        Operation::Set => {
            let Some(last) = values.last() else {
                return error(
                    ctx,
                    sender,
                    "commands.data.modify.expected_value",
                    Vec::new(),
                );
            };
            path.set(&mut data, last);
        }
        Operation::Merge => {
            let sources: Vec<&Compound> = values.iter().filter_map(Tag::as_compound).collect();
            if sources.len() != values.len() {
                return error(
                    ctx,
                    sender,
                    "commands.data.modify.expected_object",
                    Vec::new(),
                );
            }
            let applied = path.update(&mut data, |tag| match tag {
                Tag::Compound(existing) => {
                    for source in &sources {
                        *existing = merge_compounds(existing, source);
                    }
                }
                _ => *failure.borrow_mut() = Some("commands.data.modify.expected_object"),
            });
            if applied == 0 {
                return error(
                    ctx,
                    sender,
                    "commands.data.modify.expected_object",
                    Vec::new(),
                );
            }
        }
        Operation::Append | Operation::Prepend | Operation::Insert(_) => {
            if path.count(&data) == 0 {
                path.set(&mut data, &Tag::List(Vec::new()));
            }
            path.update(&mut data, |tag| match tag {
                Tag::List(list) => {
                    let at = match operation {
                        Operation::Append => list.len(),
                        Operation::Prepend => 0,
                        Operation::Insert(index) => {
                            let len = list.len() as i32;
                            let resolved = if index < 0 { len + index + 1 } else { index };
                            if resolved < 0 || resolved > len {
                                *failure.borrow_mut() = Some("commands.data.modify.invalid_index");
                                return;
                            }
                            resolved as usize
                        }
                        _ => return,
                    };
                    for (offset, value) in values.iter().enumerate() {
                        list.insert(at + offset, value.clone());
                    }
                }
                _ => *failure.borrow_mut() = Some("commands.data.modify.expected_list"),
            });
        }
    }
    if let Some(key) = failure.into_inner() {
        return error(ctx, sender, key, Vec::new());
    }
    if data == before {
        return error(ctx, sender, "commands.data.modify.failed", Vec::new());
    }
    if let Err(outcome) = save(ctx, sender, &spec, data) {
        return outcome;
    }
    modified(ctx, sender, &spec)
}

pub(super) fn run<C: CommandContext>(
    ctx: &mut C,
    sender: CommandSender,
    rest: &str,
) -> CommandOutcome {
    let Some((action, rest)) = next_argument(rest) else {
        return usage(ctx, sender, USAGE);
    };
    match action {
        "get" => get(ctx, sender, rest),
        "merge" => merge(ctx, sender, rest),
        "modify" => modify(ctx, sender, rest),
        "remove" => remove(ctx, sender, rest),
        _ => usage(ctx, sender, USAGE),
    }
}

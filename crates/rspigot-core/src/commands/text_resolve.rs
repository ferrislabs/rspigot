use rspigot_domain::Identifier;
use rspigot_domain::text::{Component, Content, NamedColor, NbtContents, NbtSource, Style};
use rspigot_domain::text_nbt::component_from_tag;
use rspigot_nbt::{NbtPath, Tag};

use super::dispatch::{CommandContext, CommandSender, Target};
use super::selector::split_argument;
use super::source::DataTarget;

const DEPTH_LIMIT: usize = 100;

fn error(key: &str) -> Component {
    Component::translatable(key, Vec::new())
}

fn default_separator() -> Component {
    Component::text(", ").color(NamedColor::Gray)
}

pub fn resolve<C: CommandContext>(
    ctx: &C,
    sender: CommandSender,
    component: &Component,
    entity: Option<Target>,
) -> Result<Component, Component> {
    resolve_at(ctx, sender, component, entity, 0)
}

fn resolve_at<C: CommandContext>(
    ctx: &C,
    sender: CommandSender,
    component: &Component,
    entity: Option<Target>,
    depth: usize,
) -> Result<Component, Component> {
    if depth > DEPTH_LIMIT {
        return Ok(Component::text("..."));
    }
    let mut resolved = resolve_contents(ctx, sender, &component.content, entity, depth + 1)?;
    for child in &component.extra {
        resolved
            .extra
            .push(resolve_at(ctx, sender, child, entity, depth + 1)?);
    }
    resolved.style = resolve_style(ctx, sender, &component.style, entity, depth)?;
    Ok(resolved)
}

fn resolve_style<C: CommandContext>(
    ctx: &C,
    sender: CommandSender,
    style: &Style,
    entity: Option<Target>,
    depth: usize,
) -> Result<Style, Component> {
    let mut style = style.clone();
    if let Some(hover) = &style.hover {
        style.hover = Some(Box::new(resolve_at(ctx, sender, hover, entity, depth + 1)?));
    }
    Ok(style)
}

fn resolve_contents<C: CommandContext>(
    ctx: &C,
    sender: CommandSender,
    content: &Content,
    entity: Option<Target>,
    depth: usize,
) -> Result<Component, Component> {
    match content {
        Content::Translatable {
            key,
            fallback,
            args,
        } => {
            let args = args
                .iter()
                .map(|arg| resolve_at(ctx, sender, arg, entity, depth))
                .collect::<Result<Vec<_>, _>>()?;
            let mut out = Component::translatable(key.clone(), args);
            if let Content::Translatable { fallback: slot, .. } = &mut out.content {
                slot.clone_from(fallback);
            }
            Ok(out)
        }
        Content::Score { name, objective } => score(ctx, sender, name, objective, entity),
        Content::Selector { pattern, separator } => {
            let targets = ctx.select_entities(sender, pattern)?;
            let names = targets
                .iter()
                .map(|target| ctx.target_name(*target))
                .collect::<Vec<_>>();
            let separator = separator_of(ctx, sender, separator.as_deref(), entity, depth)?;
            Ok(join(names, &separator))
        }
        Content::Nbt(nbt) => {
            let NbtContents {
                path,
                interpret,
                plain,
                separator,
                source,
            } = &**nbt;
            let separator = separator_of(ctx, sender, separator.as_deref(), entity, depth)?;
            let tags = nbt_tags(ctx, sender, path, source)?;
            let parts = tags
                .iter()
                .map(|tag| {
                    if *interpret {
                        let parsed = component_from_tag(tag)
                            .ok_or_else(|| error("argument.component.invalid"))?;
                        resolve_at(ctx, sender, &parsed, entity, depth)
                    } else {
                        Ok(if *plain {
                            Component::text(tag.to_string())
                        } else {
                            pretty(tag)
                        })
                    }
                })
                .collect::<Result<Vec<_>, Component>>()?;
            Ok(join(parts, &separator))
        }
        other => Ok(Component {
            content: other.clone(),
            style: Style::default(),
            extra: Vec::new(),
        }),
    }
}

fn separator_of<C: CommandContext>(
    ctx: &C,
    sender: CommandSender,
    separator: Option<&Component>,
    entity: Option<Target>,
    depth: usize,
) -> Result<Component, Component> {
    match separator {
        Some(separator) => resolve_at(ctx, sender, separator, entity, depth),
        None => Ok(default_separator()),
    }
}

fn join(parts: Vec<Component>, separator: &Component) -> Component {
    let mut parts = parts.into_iter();
    let Some(mut head) = parts.next() else {
        return Component::text("");
    };
    for part in parts {
        head = head.append(separator.clone()).append(part);
    }
    if head.extra.is_empty() {
        head
    } else {
        Component::text("").append(head)
    }
}

fn score<C: CommandContext>(
    ctx: &C,
    sender: CommandSender,
    name: &str,
    objective: &str,
    entity: Option<Target>,
) -> Result<Component, Component> {
    let holder = if name.starts_with('@') {
        let found = ctx.select_entities(sender, name)?;
        match found.as_slice() {
            [one] => ctx.score_holder_name(*one),
            [] => return Err(error("argument.entity.notfound.entity")),
            _ => return Err(error("argument.entity.toomany")),
        }
    } else if name == "*" {
        entity.and_then(|target| ctx.score_holder_name(target))
    } else {
        Some(name.to_owned())
    };
    let value =
        holder.and_then(|holder| ctx.scoreboard().score(&holder, objective).map(|s| s.value));
    Ok(Component::text(
        value.map(|value| value.to_string()).unwrap_or_default(),
    ))
}

fn nbt_tags<C: CommandContext>(
    ctx: &C,
    sender: CommandSender,
    path: &str,
    source: &NbtSource,
) -> Result<Vec<Tag>, Component> {
    let path = NbtPath::parse(path).map_err(|_| error("arguments.nbtpath.node.invalid"))?;
    let targets = match source {
        NbtSource::Storage(id) => vec![DataTarget::Storage(
            Identifier::parse(id).map_err(|_| error("argument.id.invalid"))?,
        )],
        NbtSource::Block(pos) => {
            let mut parts = [""; 3];
            let mut cursor = pos.as_str();
            for slot in &mut parts {
                let (token, rest) = split_argument(cursor);
                *slot = token;
                cursor = rest;
            }
            let at = super::dispatch::parse_vec3(ctx, sender, parts)?;
            vec![DataTarget::Block(rspigot_domain::BlockPos::new(
                at.x.floor() as i32,
                at.y.floor() as i32,
                at.z.floor() as i32,
            ))]
        }
        NbtSource::Entity(selector) => ctx
            .select_entities(sender, selector)?
            .into_iter()
            .map(DataTarget::Entity)
            .collect(),
    };
    let mut tags = Vec::new();
    for target in &targets {
        if let Ok(data) = ctx.read_data(target) {
            let root = Tag::Compound(data);
            tags.extend(path.get(&root).into_iter().cloned());
        }
    }
    Ok(tags)
}

fn pretty(tag: &Tag) -> Component {
    match tag {
        Tag::String(text) => Component::text(format!("{:?}", text)).color(NamedColor::Green),
        Tag::Byte(v) => suffixed(v.to_string(), "b"),
        Tag::Short(v) => suffixed(v.to_string(), "s"),
        Tag::Int(v) => Component::text(v.to_string()).color(NamedColor::Gold),
        Tag::Long(v) => suffixed(v.to_string(), "L"),
        Tag::Float(v) => suffixed(v.to_string(), "f"),
        Tag::Double(v) => suffixed(v.to_string(), "d"),
        Tag::List(items) => {
            let mut out = Component::text("[");
            for (index, item) in items.iter().enumerate() {
                if index > 0 {
                    out = out.append(Component::text(", "));
                }
                out = out.append(pretty(item));
            }
            out.append(Component::text("]"))
        }
        Tag::Compound(compound) => {
            let mut out = Component::text("{");
            for (index, (key, value)) in compound.iter().enumerate() {
                if index > 0 {
                    out = out.append(Component::text(", "));
                }
                out = out
                    .append(Component::text(key.to_owned()).color(NamedColor::Aqua))
                    .append(Component::text(": "))
                    .append(pretty(value));
            }
            out.append(Component::text("}"))
        }
        other => Component::text(other.to_string()),
    }
}

fn suffixed(number: String, suffix: &str) -> Component {
    Component::text(number)
        .color(NamedColor::Gold)
        .append(Component::text(suffix.to_owned()).color(NamedColor::Red))
}

pub fn resolve_for_source<C: CommandContext>(
    ctx: &C,
    sender: CommandSender,
    component: &Component,
) -> Result<Component, Component> {
    let executor = ctx.command_source(sender).executor;
    resolve(ctx, sender, component, executor)
}

use rspigot_data::loot::{CopyOperation, ListMode, MergeStrategy, NameTarget};
use rspigot_domain::BlockStateId;
use rspigot_nbt::{Compound, NbtPath, Tag, parse_compound, parse_tag};

const BLOCK_STATE: &str = "minecraft:block_state";
const LORE: &str = "minecraft:lore";
const CUSTOM_DATA: &str = "minecraft:custom_data";
const TOOLTIP_DISPLAY: &str = "minecraft:tooltip_display";
const HIDDEN_COMPONENTS: &str = "hidden_components";
const HIDE_TOOLTIP: &str = "hide_tooltip";
const MAX_LORE_LINES: usize = 256;

pub(crate) fn copy_state(
    components: &mut Compound,
    state: Option<BlockStateId>,
    block: &str,
    properties: &[&str],
) {
    let Some(state) = state.filter(|state| state.kind().is_some_and(|kind| kind.name() == block))
    else {
        return;
    };
    let mut held = match components.remove(BLOCK_STATE) {
        Some(Tag::Compound(held)) => held,
        _ => Compound::new(),
    };
    for name in properties {
        if let Some(value) = state.property(name) {
            held.insert(*name, value);
        }
    }
    components.insert(BLOCK_STATE, held);
}

pub(crate) fn set_name(components: &mut Compound, name: &str, target: NameTarget) {
    if let Ok(tag) = parse_tag(name) {
        components.insert(target.component(), tag);
    }
}

fn lines_of(lines: &[&str]) -> Vec<Tag> {
    lines
        .iter()
        .filter_map(|line| parse_tag(line).ok())
        .collect()
}

fn offset_in(offset: i32, length: usize) -> Option<usize> {
    usize::try_from(offset.max(0))
        .ok()
        .filter(|at| *at <= length)
}

pub(crate) fn set_lore(components: &mut Compound, lines: &[&str], mode: ListMode) {
    let values = lines_of(lines);
    let mut held: Vec<Tag> = components
        .get_list(LORE)
        .map(<[Tag]>::to_vec)
        .unwrap_or_default();
    match mode {
        ListMode::ReplaceAll => held = values,
        ListMode::Append => held.extend(values),
        ListMode::Insert { offset } => {
            let Some(at) = offset_in(offset, held.len()) else {
                return;
            };
            held.splice(at..at, values);
        }
        ListMode::ReplaceSection { offset, size } => {
            let Some(at) = offset_in(offset, held.len()) else {
                return;
            };
            let span = size.map_or(values.len(), |size| usize::try_from(size).unwrap_or(0));
            let end = (at + span).min(held.len());
            held.splice(at..end, values);
        }
    }
    held.truncate(MAX_LORE_LINES);
    components.insert(LORE, Tag::List(held));
}

fn merge_into(target: &mut Compound, source: &Compound) {
    for (key, value) in source.iter() {
        match (target.get_mut(key), value) {
            (Some(Tag::Compound(existing)), Tag::Compound(added)) => merge_into(existing, added),
            _ => {
                target.insert(key, value.clone());
            }
        }
    }
}

fn custom_data(components: &mut Compound) -> Compound {
    match components.remove(CUSTOM_DATA) {
        Some(Tag::Compound(data)) => data,
        _ => Compound::new(),
    }
}

pub(crate) fn set_custom_data(components: &mut Compound, tag: &str) {
    let Ok((added, _)) = parse_compound(tag) else {
        return;
    };
    let mut data = custom_data(components);
    merge_into(&mut data, &added);
    components.insert(CUSTOM_DATA, data);
}

pub(crate) fn copy_custom_data(
    components: &mut Compound,
    source: &Compound,
    operations: &[CopyOperation],
) {
    let source = Tag::Compound(source.clone());
    let mut data = Tag::Compound(custom_data(components));
    for operation in operations {
        let (Ok(from), Ok(to)) = (
            NbtPath::parse(operation.source),
            NbtPath::parse(operation.target),
        ) else {
            continue;
        };
        let Some(value) = from.get(&source).first().map(|value| (*value).clone()) else {
            continue;
        };
        match operation.strategy {
            MergeStrategy::Replace => {
                to.set(&mut data, &value);
            }
            MergeStrategy::Append => {
                if to.count(&data) == 0 {
                    to.set(&mut data, &Tag::List(Vec::new()));
                }
                to.update(&mut data, |target| {
                    if let Tag::List(list) = target {
                        list.push(value.clone());
                    }
                });
            }
            MergeStrategy::Merge => {
                if let Tag::Compound(added) = &value {
                    if to.count(&data) == 0 {
                        to.set(&mut data, &Tag::Compound(Compound::new()));
                    }
                    to.update(&mut data, |target| {
                        if let Tag::Compound(existing) = target {
                            merge_into(existing, added);
                        }
                    });
                }
            }
        }
    }
    if let Tag::Compound(data) = data {
        components.insert(CUSTOM_DATA, data);
    }
}

pub(crate) fn toggle_tooltips(components: &mut Compound, toggles: &[(&str, bool)]) {
    let mut display = match components.remove(TOOLTIP_DISPLAY) {
        Some(Tag::Compound(display)) => display,
        _ => Compound::new(),
    };
    let mut hidden: Vec<Tag> = display
        .get_list(HIDDEN_COMPONENTS)
        .map(<[Tag]>::to_vec)
        .unwrap_or_default();
    for (name, shown) in toggles {
        let position = hidden.iter().position(|held| held.as_str() == Some(name));
        match (shown, position) {
            (true, Some(at)) => {
                hidden.remove(at);
            }
            (false, None) => hidden.push(Tag::String((*name).to_owned())),
            _ => {}
        }
    }
    let hides_all = display.get_bool(HIDE_TOOLTIP).unwrap_or(false);
    if hidden.is_empty() && !hides_all {
        return;
    }
    display.insert(HIDDEN_COMPONENTS, Tag::List(hidden));
    components.insert(TOOLTIP_DISPLAY, display);
}

pub(crate) fn set_components(components: &mut Compound, values: &[(&str, &str)]) {
    for (name, value) in values {
        if let Ok(tag) = parse_tag(value) {
            components.insert(*name, tag);
        }
    }
}

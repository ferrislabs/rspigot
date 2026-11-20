use rspigot_nbt::{Compound, Tag};

use crate::dye;
use crate::item::{ItemStack, Slot};

pub const BANNER: usize = 0;
pub const DYE: usize = 1;
pub const PATTERN: usize = 2;
pub const RESULT: usize = 3;
pub const INPUTS: usize = 3;

const MAX_LAYERS: usize = 6;
const REGISTRY: &str = "minecraft:banner_pattern";
const NO_ITEM_REQUIRED: &str = "minecraft:no_item_required";
const PROVIDES: &str = "minecraft:provides_banner_patterns";
const LAYERS: &str = "minecraft:banner_patterns";
const NAMESPACE: &str = "minecraft:";
const PATTERN_ITEM_SUFFIX: &str = "_banner_pattern";

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Loom {
    pub selected: i32,
    pub patterns: &'static [u32],
}

fn provided_tag(stack: &ItemStack) -> Option<String> {
    if let Some(tag) = stack.components.get_str(PROVIDES) {
        return Some(tag.trim_start_matches('#').to_owned());
    }
    let name = stack.item.kind()?.name();
    let base = name
        .strip_prefix(NAMESPACE)?
        .strip_suffix(PATTERN_ITEM_SUFFIX)?;
    Some(format!("{NAMESPACE}pattern_item/{base}"))
}

pub fn selectable(pattern: Option<&ItemStack>) -> &'static [u32] {
    let tag = match pattern {
        None => Some(NO_ITEM_REQUIRED.to_owned()),
        Some(stack) => provided_tag(stack),
    };
    tag.and_then(|tag| rspigot_data::tags::tag(REGISTRY, &tag))
        .unwrap_or(&[])
}

fn layer_count(banner: &ItemStack) -> usize {
    banner.components.get_list(LAYERS).map_or(0, <[Tag]>::len)
}

fn pattern_name(id: u32) -> Option<&'static str> {
    let index = usize::try_from(id).ok()?;
    rspigot_data::registries::synced_entries(REGISTRY)?
        .get(index)
        .copied()
}

fn dye_name(stack: &ItemStack) -> Option<&'static str> {
    dye::of_dye_item(stack).map(|index| dye::DYE_COLORS[index].name)
}

fn result_for(container: &[Slot], pattern: u32) -> Slot {
    let banner = container.get(BANNER)?.as_ref()?;
    let color = dye_name(container.get(DYE)?.as_ref()?)?;
    let mut layers = banner
        .components
        .get_list(LAYERS)
        .map(<[Tag]>::to_vec)
        .unwrap_or_default();
    layers.push(Tag::Compound(
        Compound::new()
            .with("pattern", pattern_name(pattern)?)
            .with("color", color),
    ));
    let mut result = banner.clone();
    result.count = std::num::NonZeroU8::MIN;
    result.components.insert(LAYERS, Tag::List(layers));
    Some(result)
}

fn set_result(container: &mut [Slot], pattern: Option<u32>) {
    let result = pattern.and_then(|pattern| result_for(container, pattern));
    if let Some(slot) = container.get_mut(RESULT) {
        *slot = result;
    }
}

fn chosen(loom: &Loom) -> Option<usize> {
    usize::try_from(loom.selected)
        .ok()
        .filter(|index| *index < loom.patterns.len())
}

pub fn refresh(container: &mut [Slot], loom: &mut Loom) {
    let banner = container.get(BANNER).and_then(Option::as_ref);
    let has_dye = container.get(DYE).is_some_and(Option::is_some);
    let Some(banner) = banner.filter(|_| has_dye) else {
        loom.patterns = &[];
        loom.selected = -1;
        set_result(container, None);
        return;
    };
    let full = layer_count(banner) >= MAX_LAYERS;
    let previous = chosen(loom).map(|index| loom.patterns[index]);
    let patterns = selectable(container.get(PATTERN).and_then(Option::as_ref));
    loom.patterns = patterns;
    let index = if patterns.len() == 1 {
        Some(0)
    } else {
        previous.and_then(|id| patterns.iter().position(|candidate| *candidate == id))
    };
    loom.selected = index.and_then(|i| i32::try_from(i).ok()).unwrap_or(-1);
    match index {
        Some(_) if full => {
            loom.selected = -1;
            set_result(container, None);
        }
        Some(index) => set_result(container, Some(patterns[index])),
        None => set_result(container, None),
    }
}

pub fn press(container: &mut [Slot], loom: &mut Loom, button: i32) -> bool {
    let Some(index) = usize::try_from(button)
        .ok()
        .filter(|index| *index < loom.patterns.len())
    else {
        return false;
    };
    let full = container
        .get(BANNER)
        .and_then(Option::as_ref)
        .is_some_and(|banner| layer_count(banner) >= MAX_LAYERS);
    if full {
        return false;
    }
    loom.selected = button;
    set_result(container, Some(loom.patterns[index]));
    true
}

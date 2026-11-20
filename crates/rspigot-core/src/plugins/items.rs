use std::num::NonZeroU8;
use std::ops::RangeInclusive;

use rspigot_data::registries::synced_index;
use rspigot_domain::item::{
    CUSTOM_MODEL_DATA, CUSTOM_NAME, CustomModelData, ENCHANTMENTS, ItemStack, LORE, MAX_LORE_LINES,
};
use rspigot_domain::text::Component;
use rspigot_items::ItemKind;
use rspigot_nbt::{Compound, Tag};

use crate::plugins::item_components;
use crate::plugins::resolve::text;

const COUNT: RangeInclusive<u8> = 1..=99;
const ENCHANTMENT: &str = "minecraft:enchantment";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ItemSpec {
    pub key: String,
    pub count: i32,
    pub name: Option<String>,
    pub lore: Vec<String>,
    pub enchantments: Vec<(String, i32)>,
    pub custom_model_data: Option<Box<CustomModelData>>,
    pub components: Vec<u8>,
}

impl ItemSpec {
    pub fn of(key: impl Into<String>, count: i32) -> Self {
        Self {
            key: key.into(),
            count,
            name: None,
            lore: Vec::new(),
            enchantments: Vec::new(),
            custom_model_data: None,
            components: Vec::new(),
        }
    }
}

pub(crate) fn stack_of(spec: &ItemSpec) -> Result<Option<ItemStack>, String> {
    let kind =
        ItemKind::from_name(&spec.key).ok_or_else(|| format!("unknown item {}", spec.key))?;
    if kind == ItemKind::Air {
        return Ok(None);
    }
    let count = u8::try_from(spec.count)
        .ok()
        .filter(|count| COUNT.contains(count))
        .and_then(NonZeroU8::new)
        .ok_or_else(|| format!("count {} is outside 1..=99", spec.count))?;
    if spec.lore.len() > MAX_LORE_LINES {
        return Err(format!(
            "lore has {} lines, more than {MAX_LORE_LINES}",
            spec.lore.len()
        ));
    }
    let enchantments = spec
        .enchantments
        .iter()
        .map(|(key, level)| enchantment_of(key, *level))
        .collect::<Result<Vec<_>, _>>()?;
    let mut stack = ItemStack::new(kind.id(), count);
    let mut components = components_of(&spec.components)?;
    item_components::validate(&components)?;
    item_components::normalize(&mut components);
    stack.components = components;
    stack.set_custom_name(spec.name.as_deref().map(text).as_ref());
    let lore: Vec<Component> = spec.lore.iter().map(|line| text(line)).collect();
    stack.set_lore(&lore);
    stack.set_enchantments(&enchantments);
    stack.set_custom_model_data(spec.custom_model_data.as_deref());
    Ok(Some(stack))
}

pub(crate) fn spec_of(stack: &ItemStack) -> ItemSpec {
    let mut others = stack.components.clone();
    for typed in [CUSTOM_NAME, LORE, ENCHANTMENTS, CUSTOM_MODEL_DATA] {
        others.remove(typed);
    }
    item_components::legacy(&mut others);
    let mut components = Vec::new();
    if !others.is_empty() {
        rspigot_nbt::write_network(&mut components, &Tag::Compound(others));
    }
    ItemSpec {
        key: stack
            .item
            .kind()
            .map_or("minecraft:air", ItemKind::name)
            .to_owned(),
        count: i32::from(stack.count.get()),
        name: stack.custom_name().map(|name| name.to_legacy()),
        lore: stack.lore().iter().map(Component::to_legacy).collect(),
        enchantments: stack
            .enchantments()
            .into_iter()
            .map(|(key, level)| (key, i32::from(level)))
            .collect(),
        custom_model_data: stack.custom_model_data().map(Box::new),
        components,
    }
}

fn enchantment_of(key: &str, level: i32) -> Result<(String, u8), String> {
    if synced_index(ENCHANTMENT, key).is_none() {
        return Err(format!("unknown enchantment {key}"));
    }
    let level = u8::try_from(level)
        .ok()
        .filter(|level| *level > 0)
        .ok_or_else(|| format!("enchantment level {level} is outside 1..=255"))?;
    Ok((key.to_owned(), level))
}

fn components_of(bytes: &[u8]) -> Result<Compound, String> {
    if bytes.is_empty() {
        return Ok(Compound::new());
    }
    let mut rest = bytes;
    match rspigot_nbt::read_network_compound(&mut rest) {
        Ok(Some(components)) if rest.is_empty() => Ok(components),
        _ => Err("the components are not one network NBT compound".to_owned()),
    }
}

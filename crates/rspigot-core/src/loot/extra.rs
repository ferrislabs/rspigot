use std::num::NonZeroU8;

use rand::Rng;
use rspigot_data::loot::{
    AttributeModifier, ContainerComponent, CustomModelEdit, EntityTarget, Explosion, ExtraFunction,
    ListEdit, PageKind, SlotContainer, SlotSource,
};
use rspigot_domain::ItemId;
use rspigot_domain::dye::DYE_COLORS;
use rspigot_domain::entity::save::{item_stack_from_nbt, item_stack_to_nbt};
use rspigot_domain::item::ItemStack;
use rspigot_entities::EntityKind;
use rspigot_items::ItemKind;
use rspigot_nbt::{Compound, Tag};

use super::{Produce, Roller, Working, item_matches};
use crate::loot_components::{edit_list, page_tags};

const PROFILE: &str = "minecraft:profile";
const WRITTEN_BOOK: &str = "minecraft:written_book_content";
const WRITABLE_BOOK: &str = "minecraft:writable_book_content";
const CUSTOM_MODEL_DATA: &str = "minecraft:custom_model_data";
const FIREWORKS: &str = "minecraft:fireworks";
const FIREWORK_EXPLOSION: &str = "minecraft:firework_explosion";
const BANNER_PATTERNS: &str = "minecraft:banner_patterns";
const ATTRIBUTE_MODIFIERS: &str = "minecraft:attribute_modifiers";
const DYED_COLOR: &str = "minecraft:dyed_color";
const POTION_CONTENTS: &str = "minecraft:potion_contents";
const DYEABLE: &str = "minecraft:cauldron_can_remove_dye";
const POTION_REGISTRY: &str = "minecraft:potion";
const DEFAULT_SHAPE: &str = "small_ball";
const MAX_PAGES: usize = 100;
const BLOCK_ENTITY_COMPONENTS: &[&str] = &[
    "minecraft:container",
    "minecraft:container_loot",
    "minecraft:custom_name",
    "minecraft:lock",
    "minecraft:bees",
    "minecraft:banner_patterns",
    "minecraft:pot_decorations",
    "minecraft:profile",
    "minecraft:note_block_sound",
];

fn working_of(stack: &ItemStack) -> Working {
    Working {
        item: stack.item.0,
        count: i32::from(stack.count.get()),
        components: stack.components.clone(),
    }
}

fn stack_of(working: &Working) -> Option<ItemStack> {
    if working.is_empty() {
        return None;
    }
    let count = u8::try_from(working.count.clamp(1, 99)).ok()?;
    let mut stack = ItemStack::new(ItemId(working.item), NonZeroU8::new(count)?);
    stack.components = working.components.clone();
    Some(stack)
}

pub(super) fn container_items(stack: &ItemStack, component: ContainerComponent) -> Vec<ItemStack> {
    let Some(Tag::List(held)) = stack.components.get(component.component()) else {
        return Vec::new();
    };
    held.iter()
        .filter_map(|entry| match (entry, component) {
            (Tag::Compound(entry), ContainerComponent::Container) => entry.get_compound("item"),
            (Tag::Compound(entry), _) => Some(entry),
            _ => None,
        })
        .filter_map(item_stack_from_nbt)
        .collect()
}

fn write_items(
    components: &mut Compound,
    component: ContainerComponent,
    items: Vec<(Option<i32>, ItemStack)>,
) {
    let tags = items
        .into_iter()
        .enumerate()
        .map(|(index, (slot, stack))| {
            let item = item_stack_to_nbt(&stack);
            match component {
                ContainerComponent::Container => {
                    let slot = slot.unwrap_or_else(|| i32::try_from(index).unwrap_or(0));
                    Tag::Compound(Compound::new().with("slot", slot).with("item", item))
                }
                _ => Tag::Compound(item),
            }
        })
        .collect();
    components.insert(component.component(), Tag::List(tags));
}

fn explosion_tag(explosion: &Explosion, mut base: Compound) -> Compound {
    if let Some(shape) = explosion.shape {
        base.insert("shape", shape);
    } else if !base.contains_key("shape") {
        base.insert("shape", DEFAULT_SHAPE);
    }
    if let Some(colors) = explosion.colors {
        base.insert("colors", colors.to_vec());
    }
    if let Some(colors) = explosion.fade_colors {
        base.insert("fade_colors", colors.to_vec());
    }
    if let Some(trail) = explosion.trail {
        base.insert("has_trail", trail);
    }
    if let Some(twinkle) = explosion.twinkle {
        base.insert("has_twinkle", twinkle);
    }
    base
}

fn held_list(components: &Compound, component: &str, key: &str) -> Vec<Tag> {
    match components.get(component) {
        Some(Tag::Compound(held)) => held.get_list(key).map(<[Tag]>::to_vec).unwrap_or_default(),
        _ => Vec::new(),
    }
}

fn held_compound(components: &mut Compound, component: &str) -> Compound {
    match components.remove(component) {
        Some(Tag::Compound(held)) => held,
        _ => Compound::new(),
    }
}

fn edited<T: Copy>(
    held: &mut Compound,
    key: &str,
    edit: Option<ListEdit<T>>,
    tag: impl Fn(T) -> Tag,
) {
    let Some(edit) = edit else {
        return;
    };
    let current = match held.get(key) {
        Some(Tag::List(list)) => list.clone(),
        _ => Vec::new(),
    };
    let values = edit.values.iter().map(|value| tag(*value)).collect();
    if let Some(updated) = edit_list(current, values, edit.mode) {
        held.insert(key, Tag::List(updated));
    }
}

fn mixed_dyes(existing: Option<i32>, dyes: &[i32]) -> i32 {
    let mut sums = [0_i32; 3];
    let mut peak = 0_i32;
    let mut count = 0_i32;
    for color in existing.into_iter().chain(dyes.iter().copied()) {
        let channels = [(color >> 16) & 255, (color >> 8) & 255, color & 255];
        peak += channels.iter().copied().max().unwrap_or(0);
        for (sum, channel) in sums.iter_mut().zip(channels) {
            *sum += channel;
        }
        count += 1;
    }
    if count == 0 {
        return 0;
    }
    let averages = sums.map(|sum| sum / count);
    let mean_peak = peak as f32 / count as f32;
    let top = averages.iter().copied().max().unwrap_or(0).max(1) as f32;
    let [red, green, blue] = averages.map(|channel| (channel as f32 * mean_peak / top) as i32);
    ((red & 255) << 16) | ((green & 255) << 8) | (blue & 255)
}

impl<R: Rng + ?Sized> Roller<'_, '_, R> {
    pub(super) fn slot_stacks(&self, source: &SlotSource) -> Vec<ItemStack> {
        match source {
            SlotSource::Empty => Vec::new(),
            SlotSource::Range { container, slots } => {
                let ids = crate::commands::slots::slot_ids(slots).unwrap_or_default();
                let held: &[(i32, ItemStack)] = match container {
                    SlotContainer::Entity(target) => self
                        .context
                        .target(*target)
                        .map_or(&[][..], |entity| entity.inventory.as_slice()),
                    SlotContainer::BlockEntity => self.context.container,
                };
                ids.iter()
                    .filter_map(|id| held.iter().find(|(slot, _)| slot == id))
                    .map(|(_, stack)| stack.clone())
                    .collect()
            }
            SlotSource::Contents { source, component } => self
                .slot_stacks(source)
                .iter()
                .flat_map(|stack| container_items(stack, *component))
                .collect(),
            SlotSource::Filtered { source, filter } => self
                .slot_stacks(source)
                .into_iter()
                .filter(|stack| item_matches(filter, Some(stack)))
                .collect(),
            SlotSource::Limit { source, limit } => self
                .slot_stacks(source)
                .into_iter()
                .take(usize::try_from(*limit).unwrap_or(0))
                .collect(),
            SlotSource::Group(terms) => terms
                .iter()
                .flat_map(|term| self.slot_stacks(term))
                .collect(),
        }
    }

    fn generated_items(&mut self, entries: &'static [rspigot_data::loot::Entry]) -> Vec<ItemStack> {
        let kept = std::mem::take(&mut self.drops);
        let mut candidates = Vec::new();
        for entry in entries {
            self.expand(entry, &[], &mut candidates);
        }
        for candidate in candidates {
            match candidate.produce {
                Produce::Item(item) => self.emit(Working::new(item), &candidate.modifiers),
                Produce::Items(ids) => {
                    for id in ids {
                        if let Ok(item) = u16::try_from(*id) {
                            self.emit(Working::new(item), &candidate.modifiers);
                        }
                    }
                }
                Produce::Table(table) => self.table(table, &candidate.modifiers),
                Produce::Dynamic(name) => {
                    for item in self.dynamic_items(name) {
                        self.emit(Working::new(item), &candidate.modifiers);
                    }
                }
                Produce::Slots(source) => {
                    for stack in self.slot_stacks(source) {
                        self.emit(working_of(&stack), &candidate.modifiers);
                    }
                }
                Produce::Nothing => {}
            }
        }
        std::mem::replace(&mut self.drops, kept)
    }

    fn attribute_entry(&mut self, modifier: &AttributeModifier) -> Tag {
        let amount = self.float(modifier.amount);
        let slot = match modifier.slots {
            [only] => *only,
            slots => slots[self.rng.random_range(0..slots.len())],
        };
        let mut entry = Compound::new()
            .with("type", modifier.attribute)
            .with("id", modifier.id)
            .with("amount", f64::from(amount))
            .with("operation", modifier.operation);
        if slot != "any" {
            entry.insert("slot", slot);
        }
        Tag::Compound(entry)
    }

    fn target_is_player(&self, target: EntityTarget) -> Option<&str> {
        self.context
            .target(target)
            .filter(|entity| entity.kind == Some(EntityKind::Player))
            .and_then(|entity| entity.name.as_deref())
    }

    pub(super) fn apply_extra(&mut self, function: &ExtraFunction, stack: Working) -> Working {
        let mut stack = stack;
        match function {
            ExtraFunction::Discard => {
                stack.item = ItemId::AIR.0;
                stack.count = 0;
            }
            ExtraFunction::CopyName(target) => {
                if let Some(name) = self.context.target(*target).and_then(|e| e.name.as_deref()) {
                    stack.components.insert("minecraft:custom_name", name);
                }
            }
            ExtraFunction::CopyComponents {
                from,
                include,
                exclude,
            } => {
                let wanted: Vec<&str> = include
                    .unwrap_or(BLOCK_ENTITY_COMPONENTS)
                    .iter()
                    .copied()
                    .filter(|name| !exclude.contains(name))
                    .collect();
                match from {
                    None => {
                        if let Some(data) = self.context.block_entity {
                            for (name, value) in
                                crate::block_components::collect(data, &wanted).iter()
                            {
                                stack.components.insert(name, value.clone());
                            }
                        }
                    }
                    Some(target) => {
                        if let Some(entity) = self.context.target(*target) {
                            for (name, snbt) in &entity.components {
                                let kept = include.is_none_or(|list| list.contains(name))
                                    && !exclude.contains(name);
                                if kept && let Ok(tag) = rspigot_nbt::parse_tag(snbt) {
                                    stack.components.insert(*name, tag);
                                }
                            }
                        }
                    }
                }
            }
            ExtraFunction::FillPlayerHead(target) => {
                if stack.item == ItemKind::PlayerHead.id().0
                    && let Some(name) = self.target_is_player(*target)
                {
                    stack
                        .components
                        .insert(PROFILE, Compound::new().with("name", name));
                }
            }
            ExtraFunction::ModifyContents {
                component,
                modifier,
            } => {
                let held: Vec<(Option<i32>, ItemStack)> =
                    match stack.components.get(component.component()) {
                        Some(Tag::List(list)) => list
                            .iter()
                            .filter_map(|entry| match (entry, component) {
                                (Tag::Compound(entry), ContainerComponent::Container) => {
                                    let item = item_stack_from_nbt(entry.get_compound("item")?)?;
                                    Some((entry.get_i32("slot"), item))
                                }
                                (Tag::Compound(entry), _) => {
                                    item_stack_from_nbt(entry).map(|item| (None, item))
                                }
                                _ => None,
                            })
                            .collect(),
                        _ => return stack,
                    };
                let mut changed = Vec::with_capacity(held.len());
                for (slot, item) in held {
                    let mut working = working_of(&item);
                    for function in *modifier {
                        working = self.apply(function, working);
                    }
                    if let Some(item) = stack_of(&working) {
                        changed.push((slot, item));
                    }
                }
                write_items(&mut stack.components, *component, changed);
            }
            ExtraFunction::SetContents { component, entries } => {
                if !stack.is_empty() {
                    let items = self
                        .generated_items(entries)
                        .into_iter()
                        .map(|item| (None, item))
                        .collect();
                    write_items(&mut stack.components, *component, items);
                }
            }
            ExtraFunction::SetAttributes { modifiers, replace } => {
                let mut entries = if *replace {
                    Vec::new()
                } else {
                    match stack.components.get(ATTRIBUTE_MODIFIERS) {
                        Some(Tag::List(held)) => held.clone(),
                        _ => Vec::new(),
                    }
                };
                for modifier in *modifiers {
                    entries.push(self.attribute_entry(modifier));
                }
                stack
                    .components
                    .insert(ATTRIBUTE_MODIFIERS, Tag::List(entries));
            }
            ExtraFunction::SetBannerPattern { patterns, append } => {
                let mut layers = match stack.components.get(BANNER_PATTERNS) {
                    Some(Tag::List(held)) if *append => held.clone(),
                    _ => Vec::new(),
                };
                for layer in *patterns {
                    layers.push(Tag::Compound(
                        Compound::new()
                            .with("pattern", layer.pattern)
                            .with("color", layer.color),
                    ));
                }
                stack.components.insert(BANNER_PATTERNS, Tag::List(layers));
            }
            ExtraFunction::SetBookCover {
                title,
                author,
                generation,
            } => {
                let mut book = held_compound(&mut stack.components, WRITTEN_BOOK);
                if let Some(title) = title {
                    book.insert("title", *title);
                } else if !book.contains_key("title") {
                    book.insert("title", "");
                }
                if let Some(author) = author {
                    book.insert("author", *author);
                } else if !book.contains_key("author") {
                    book.insert("author", "");
                }
                if let Some(generation) = generation {
                    book.insert("generation", *generation);
                }
                stack.components.insert(WRITTEN_BOOK, book);
            }
            ExtraFunction::SetCustomModelData(CustomModelEdit {
                floats,
                flags,
                strings,
                colors,
            }) => {
                let mut data = held_compound(&mut stack.components, CUSTOM_MODEL_DATA);
                edited(&mut data, "floats", *floats, Tag::Float);
                edited(&mut data, "flags", *flags, |flag| Tag::Byte(i8::from(flag)));
                edited(&mut data, "strings", *strings, |text| {
                    Tag::String(text.to_owned())
                });
                edited(&mut data, "colors", *colors, Tag::Int);
                stack.components.insert(CUSTOM_MODEL_DATA, data);
            }
            ExtraFunction::SetFireworkExplosion(explosion) => {
                let base = held_compound(&mut stack.components, FIREWORK_EXPLOSION);
                stack
                    .components
                    .insert(FIREWORK_EXPLOSION, explosion_tag(explosion, base));
            }
            ExtraFunction::SetFireworks {
                explosions,
                flight_duration,
            } => {
                let mut fireworks = held_compound(&mut stack.components, FIREWORKS);
                if let Some(duration) = flight_duration {
                    fireworks.insert("flight_duration", *duration as i8);
                } else if !fireworks.contains_key("flight_duration") {
                    fireworks.insert("flight_duration", 0_i8);
                }
                edited(&mut fireworks, "explosions", *explosions, |explosion| {
                    Tag::Compound(explosion_tag(&explosion, Compound::new()))
                });
                if !fireworks.contains_key("explosions") {
                    fireworks.insert("explosions", Tag::List(Vec::new()));
                }
                stack.components.insert(FIREWORKS, fireworks);
            }
            ExtraFunction::SetPages { kind, pages, mode } => {
                let (component, values) = match kind {
                    PageKind::Writable => (
                        WRITABLE_BOOK,
                        pages
                            .iter()
                            .map(|page| Tag::String((*page).to_owned()))
                            .collect(),
                    ),
                    PageKind::Written => (WRITTEN_BOOK, page_tags(pages)),
                };
                let held = held_list(&stack.components, component, "pages");
                if let Some(mut updated) = edit_list(held, values, *mode) {
                    updated.truncate(MAX_PAGES);
                    let mut content = held_compound(&mut stack.components, component);
                    if *kind == PageKind::Written {
                        for key in ["title", "author"] {
                            if !content.contains_key(key) {
                                content.insert(key, "");
                            }
                        }
                    }
                    content.insert("pages", Tag::List(updated));
                    stack.components.insert(component, content);
                }
            }
            ExtraFunction::SetRandomDyes(number) => {
                if !stack.is_empty() && crate::interact::item_tag(ItemId(stack.item), DYEABLE) {
                    let wanted = self.int(*number).max(0);
                    let dyes: Vec<i32> = (0..wanted)
                        .map(|_| DYE_COLORS[self.rng.random_range(0..DYE_COLORS.len())].texture)
                        .collect();
                    let existing = match stack.components.get(DYED_COLOR) {
                        Some(Tag::Int(color)) => Some(*color),
                        _ => None,
                    };
                    stack
                        .components
                        .insert(DYED_COLOR, mixed_dyes(existing, &dyes));
                }
            }
            ExtraFunction::SetRandomPotion(options) => {
                let names = rspigot_data::registries::static_entries(POTION_REGISTRY);
                let pool: Vec<u32> = match options {
                    Some(options) => options.ids(POTION_REGISTRY).to_vec(),
                    None => (0..names.map_or(0, <[&str]>::len))
                        .filter_map(|index| u32::try_from(index).ok())
                        .collect(),
                };
                if !pool.is_empty()
                    && let Some(names) = names
                {
                    let pick = pool[self.rng.random_range(0..pool.len())];
                    if let Some(name) = names.get(pick as usize) {
                        let mut contents = held_compound(&mut stack.components, POTION_CONTENTS);
                        contents.insert("potion", *name);
                        stack.components.insert(POTION_CONTENTS, contents);
                    }
                }
            }
            ExtraFunction::Filtered {
                filter,
                on_pass,
                on_fail,
            } => {
                let single = stack_of(&stack);
                let branch = if item_matches(filter, single.as_ref()) {
                    *on_pass
                } else {
                    *on_fail
                };
                for function in branch {
                    stack = self.apply(function, stack);
                }
            }
        }
        stack
    }
}

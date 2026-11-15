use rspigot_core::events::{BookDisplay, BookEntry, BookStack, CriterionProgress};
use rspigot_core::recipe_book::{BookKind, RecipeBookSettings};
use rspigot_data::advancements::Advancement;
use rspigot_domain::text::Component;
use rspigot_items::ItemSet;
use rspigot_protocol::play::clientbound as cb;
use rspigot_protocol::play::inventory::{RecipeBookTypeSettings, SlotDisplay, recipe_book_flags};
use rspigot_protocol::play::slot::{DataComponentPatch, ItemStackTemplate};
use rspigot_protocol::play::types::HolderSet;

fn ingredient(set: ItemSet) -> SlotDisplay {
    if let Some(tag) = set.tag() {
        return SlotDisplay::Tag(tag.to_owned());
    }
    match set.items() {
        [only] => SlotDisplay::Item(i32::from(only.0)),
        many => SlotDisplay::Composite(
            many.iter()
                .map(|item| SlotDisplay::Item(i32::from(item.0)))
                .collect(),
        ),
    }
}

fn stack(result: BookStack) -> SlotDisplay {
    SlotDisplay::ItemStack(ItemStackTemplate {
        item: i32::from(result.item.0),
        count: i32::from(result.count),
        components: DataComponentPatch::EMPTY,
    })
}

pub(crate) fn recipe_display(display: &BookDisplay) -> cb::RecipeDisplay {
    match display {
        BookDisplay::Shapeless {
            ingredients,
            result,
            station,
        } => cb::RecipeDisplay::Shapeless {
            ingredients: ingredients.iter().copied().map(ingredient).collect(),
            result: stack(*result),
            station: SlotDisplay::Item(i32::from(station.0)),
        },
        BookDisplay::Shaped {
            width,
            height,
            cells,
            result,
            station,
        } => cb::RecipeDisplay::Shaped {
            width: i32::from(*width),
            height: i32::from(*height),
            ingredients: cells
                .iter()
                .map(|cell| cell.map_or(SlotDisplay::Empty, ingredient))
                .collect(),
            result: stack(*result),
            station: SlotDisplay::Item(i32::from(station.0)),
        },
        BookDisplay::Furnace {
            ingredient: input,
            result,
            station,
            duration,
            experience,
        } => cb::RecipeDisplay::Furnace {
            ingredient: ingredient(*input),
            fuel: SlotDisplay::AnyFuel,
            result: stack(*result),
            station: SlotDisplay::Item(i32::from(station.0)),
            duration: i32::from(*duration),
            experience: *experience,
        },
    }
}

fn entry(entry: &BookEntry) -> cb::RecipeBookEntry {
    let mut flags = 0;
    if entry.notify {
        flags |= recipe_book_flags::NOTIFICATION;
    }
    if entry.highlight {
        flags |= recipe_book_flags::HIGHLIGHT;
    }
    cb::RecipeBookEntry {
        entry: cb::RecipeDisplayEntry {
            display_id: entry.display_id,
            display: recipe_display(&entry.display),
            group: entry.group,
            category: entry.category,
            requirements: (!entry.requirements.is_empty()).then(|| {
                entry
                    .requirements
                    .iter()
                    .map(|set| {
                        HolderSet::Ids(set.items().iter().map(|item| i32::from(item.0)).collect())
                    })
                    .collect()
            }),
        },
        flags,
    }
}

pub(crate) fn book_add(entries: &[BookEntry], replace: bool) -> cb::RecipeBookAdd {
    cb::RecipeBookAdd {
        entries: entries.iter().map(entry).collect(),
        replace,
    }
}

pub(crate) fn book_settings(settings: &RecipeBookSettings) -> cb::RecipeBookSettings {
    let of = |kind| {
        let (open, filtering) = settings.get(kind);
        RecipeBookTypeSettings { open, filtering }
    };
    cb::RecipeBookSettings {
        crafting: of(BookKind::Crafting),
        furnace: of(BookKind::Furnace),
        blast_furnace: of(BookKind::BlastFurnace),
        smoker: of(BookKind::Smoker),
    }
}

fn text(key: &str) -> rspigot_protocol::types::Text {
    rspigot_protocol::types::Text(crate::text::to_nbt(&Component::translatable(
        key.to_owned(),
        Vec::new(),
    )))
}

fn positioned(found: &Advancement) -> cb::PositionedAdvancement {
    cb::PositionedAdvancement {
        id: found.id.to_owned(),
        advancement: cb::AdvancementData {
            parent: found.parent.map(str::to_owned),
            display: found.display.map(|display| cb::AdvancementDisplay {
                title: text(display.title),
                description: text(display.description),
                icon: ItemStackTemplate {
                    item: i32::from(display.icon),
                    count: 1,
                    components: DataComponentPatch::EMPTY,
                },
                frame: i32::from(display.frame),
                show_toast: display.show_toast,
                hidden: display.hidden,
                background: display.background.map(str::to_owned),
            }),
            requirements: found
                .requirements
                .iter()
                .map(|group| group.iter().map(|name| (*name).to_owned()).collect())
                .collect(),
            telemetry: found.telemetry,
        },
        x: found.display.map_or(0.0, |display| display.x),
        y: found.display.map_or(0.0, |display| display.y),
    }
}

pub(crate) fn advancements_update(
    reset: bool,
    added: &[&'static Advancement],
    removed: &[&'static str],
    progress: &[(&'static str, CriterionProgress)],
    show: bool,
) -> cb::UpdateAdvancements {
    cb::UpdateAdvancements {
        reset,
        added: added.iter().map(|found| positioned(found)).collect(),
        removed: removed.iter().map(|id| (*id).to_owned()).collect(),
        progress: progress
            .iter()
            .map(|(id, criteria)| cb::ProgressEntry {
                id: (*id).to_owned(),
                criteria: criteria
                    .iter()
                    .map(|(name, obtained)| cb::CriterionEntry {
                        name: (*name).to_owned(),
                        obtained: *obtained,
                    })
                    .collect(),
            })
            .collect(),
        show,
    }
}

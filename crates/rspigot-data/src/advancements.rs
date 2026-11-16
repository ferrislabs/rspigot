use crate::generated::advancements::ADVANCEMENTS;
use crate::tags;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ItemMatch {
    Item(u16),
    Tag(&'static str),
}

impl ItemMatch {
    pub fn matches(&self, item: u16) -> bool {
        match self {
            Self::Item(wanted) => *wanted == item,
            Self::Tag(name) => tags::tag("minecraft:item", name)
                .is_some_and(|items| items.binary_search(&u32::from(item)).is_ok()),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Trigger {
    Impossible,
    Tick,
    SleptInBed,
    EntityKilledPlayer,
    RecipeUnlocked(&'static str),
    InventoryChanged {
        items: &'static [ItemMatch],
        occupied: i32,
    },
    ConsumeItem(Option<ItemMatch>),
    KilledEntity(Option<&'static str>),
    Event {
        name: &'static str,
        conditions: Option<&'static str>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Display {
    pub icon: u16,
    pub title: &'static str,
    pub description: &'static str,
    pub frame: u8,
    pub background: Option<&'static str>,
    pub show_toast: bool,
    pub announce_to_chat: bool,
    pub hidden: bool,
    pub x: f32,
    pub y: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Criterion {
    pub name: &'static str,
    pub trigger: Trigger,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rewards {
    pub recipes: &'static [&'static str],
    pub experience: i32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Advancement {
    pub id: &'static str,
    pub parent: Option<&'static str>,
    pub display: Option<Display>,
    pub criteria: &'static [Criterion],
    pub requirements: &'static [&'static [&'static str]],
    pub rewards: Rewards,
    pub telemetry: bool,
}

pub fn all() -> &'static [Advancement] {
    &ADVANCEMENTS
}

pub fn find(id: &str) -> Option<(usize, &'static Advancement)> {
    ADVANCEMENTS
        .binary_search_by(|advancement| advancement.id.cmp(id))
        .ok()
        .map(|index| (index, &ADVANCEMENTS[index]))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn advancements_are_sorted_and_found_by_id() {
        assert!(all().windows(2).all(|pair| pair[0].id < pair[1].id));
        let (index, root) = find("minecraft:story/root").expect("a root");
        assert_eq!(all()[index].id, root.id);
        assert!(root.parent.is_none());
        assert!(
            root.display
                .is_some_and(|display| display.background.is_some())
        );
    }

    #[test]
    fn every_parent_exists_and_every_requirement_names_a_criterion() {
        for advancement in all() {
            if let Some(parent) = advancement.parent {
                assert!(find(parent).is_some(), "{parent}");
            }
            for group in advancement.requirements {
                for name in *group {
                    assert!(
                        advancement
                            .criteria
                            .iter()
                            .any(|criterion| criterion.name == *name),
                        "{} {name}",
                        advancement.id
                    );
                }
            }
        }
    }

    #[test]
    fn the_first_children_of_a_tab_sit_one_column_right_of_their_root() {
        let (_, root) = find("minecraft:story/root").expect("a root");
        let root = root.display.expect("a display");
        let (_, stone) = find("minecraft:story/mine_stone").expect("an advancement");
        let stone = stone.display.expect("a display");
        assert_eq!((root.x, stone.x), (0.0, 1.0));
    }

    #[test]
    fn a_trigger_without_a_typed_form_keeps_its_name_and_raw_conditions() {
        let (_, adventuring) =
            find("minecraft:adventure/adventuring_time").expect("an advancement");
        let badlands = adventuring
            .criteria
            .iter()
            .find(|criterion| criterion.name == "minecraft:badlands")
            .expect("a criterion");
        let Trigger::Event {
            name,
            conditions: Some(conditions),
        } = badlands.trigger
        else {
            panic!("a location trigger");
        };
        assert_eq!(name, "minecraft:location");
        assert!(conditions.contains("minecraft:badlands"));
        let (_, brew) = find("minecraft:nether/brew_potion").expect("an advancement");
        assert!(brew.criteria.iter().any(|criterion| matches!(
            criterion.trigger,
            Trigger::Event {
                name: "minecraft:brewed_potion",
                conditions: None
            }
        )));
    }

    #[test]
    fn an_item_match_reads_a_direct_item_and_a_tag() {
        let tag = ItemMatch::Tag("minecraft:stone_tool_materials");
        let members = tags::tag("minecraft:item", "minecraft:stone_tool_materials").expect("a tag");
        let member = u16::try_from(members[0]).expect("an item id");
        assert!(tag.matches(member));
        assert!(!tag.matches(u16::MAX));
        assert!(ItemMatch::Item(5).matches(5));
    }
}

use std::collections::{BTreeMap, BTreeSet};
use std::sync::OnceLock;
use std::time::{SystemTime, UNIX_EPOCH};

use chrono::{DateTime, Utc};
use rspigot_data::advancements::{self, Advancement, ItemMatch, Trigger};
use rspigot_domain::item::ItemStack;
use rspigot_domain::player::PlayerId;
use rspigot_domain::text::{Component, NamedColor};
use serde_json::{Map, Value};

use crate::events::ClientEvent;
use crate::plugins::events::{PlayerAdvancementCriterionGrant, PlayerAdvancementDone};
use crate::plugins::steps::Continuation;
use crate::plugins::{EventKind, EventPayload};
use crate::ports::ClientLink;
use crate::server::{Audience, Server};

const TIME_FORMAT: &str = "%Y-%m-%d %H:%M:%S %z";
const ROOT_HOPS: usize = 3;

struct Tree {
    children: Vec<Vec<usize>>,
    roots: Vec<usize>,
}

fn tree() -> &'static Tree {
    static TREE: OnceLock<Tree> = OnceLock::new();
    TREE.get_or_init(|| {
        let all = advancements::all();
        let mut children = vec![Vec::new(); all.len()];
        let mut roots = Vec::new();
        for (index, advancement) in all.iter().enumerate() {
            match advancement
                .parent
                .and_then(advancements::find)
                .map(|(parent, _)| parent)
            {
                Some(parent) => children[parent].push(index),
                None => roots.push(index),
            }
        }
        Tree { children, roots }
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Rule {
    Show,
    Hide,
    Keep,
}

fn rule_of(advancement: &Advancement, done: bool) -> Rule {
    match &advancement.display {
        None => Rule::Hide,
        Some(_) if done => Rule::Show,
        Some(display) if display.hidden => Rule::Hide,
        Some(_) => Rule::Keep,
    }
}

fn shown_by_ancestry(rules: &[Rule]) -> bool {
    for hop in 0..ROOT_HOPS {
        match rules.len().checked_sub(hop + 1).map(|at| rules[at]) {
            None | Some(Rule::Show) => return true,
            Some(Rule::Hide) => return false,
            Some(Rule::Keep) => {}
        }
    }
    false
}

fn visit(
    index: usize,
    rules: &mut Vec<Rule>,
    done: &impl Fn(usize) -> bool,
    shown: &mut BTreeSet<usize>,
) -> bool {
    let node_done = done(index);
    rules.push(rule_of(&advancements::all()[index], node_done));
    let mut visible = node_done;
    for child in &tree().children[index] {
        visible |= visit(*child, rules, done, shown);
    }
    let displayed = advancements::all()[index].display.is_some();
    if displayed && (visible || shown_by_ancestry(rules)) {
        shown.insert(index);
    }
    rules.pop();
    visible
}

fn visible_set(done: &impl Fn(usize) -> bool) -> BTreeSet<usize> {
    let mut shown = BTreeSet::new();
    let mut rules = Vec::new();
    for root in &tree().roots {
        visit(*root, &mut rules, done, &mut shown);
    }
    shown
}

fn now_millis() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_millis() as i64)
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PlayerAdvancements {
    obtained: BTreeMap<usize, BTreeMap<&'static str, i64>>,
    visible: BTreeSet<usize>,
    dirty: BTreeSet<usize>,
    selected_tab: Option<usize>,
    announced: bool,
    inventory_seen: Option<i32>,
}

impl PlayerAdvancements {
    pub fn has(&self, index: usize, criterion: &str) -> bool {
        self.obtained
            .get(&index)
            .is_some_and(|criteria| criteria.contains_key(criterion))
    }

    pub fn is_done(&self, index: usize) -> bool {
        let advancement = &advancements::all()[index];
        let Some(criteria) = self.obtained.get(&index) else {
            return false;
        };
        advancement.requirements.iter().all(|group| {
            group
                .iter()
                .any(|criterion| criteria.contains_key(criterion))
        }) && !advancement.requirements.is_empty()
    }

    pub fn done_ids(&self) -> Vec<&'static str> {
        self.obtained
            .keys()
            .filter(|index| self.is_done(**index))
            .map(|index| advancements::all()[*index].id)
            .collect()
    }

    fn grant(&mut self, index: usize, criterion: &'static str, when: i64) -> bool {
        let criteria = self.obtained.entry(index).or_default();
        if criteria.contains_key(criterion) {
            return false;
        }
        criteria.insert(criterion, when);
        self.dirty.insert(index);
        true
    }

    fn criteria_of(&self, index: usize) -> Vec<(&'static str, Option<i64>)> {
        let obtained = self.obtained.get(&index);
        advancements::all()[index]
            .criteria
            .iter()
            .map(|criterion| {
                (
                    criterion.name,
                    obtained.and_then(|map| map.get(criterion.name).copied()),
                )
            })
            .collect()
    }

    pub fn to_json(&self) -> String {
        let mut root = Map::new();
        for (index, criteria) in &self.obtained {
            let advancement = &advancements::all()[*index];
            let mut stamps = Map::new();
            for (name, when) in criteria {
                let stamp = DateTime::<Utc>::from_timestamp_millis(*when)
                    .map(|time| time.format(TIME_FORMAT).to_string())
                    .unwrap_or_default();
                stamps.insert((*name).to_owned(), Value::String(stamp));
            }
            let mut entry = Map::new();
            entry.insert("criteria".to_owned(), Value::Object(stamps));
            entry.insert("done".to_owned(), Value::Bool(self.is_done(*index)));
            root.insert(advancement.id.to_owned(), Value::Object(entry));
        }
        Value::Object(root).to_string()
    }

    pub fn from_json(text: &str) -> Self {
        let mut loaded = Self::default();
        let Ok(Value::Object(root)) = serde_json::from_str::<Value>(text) else {
            return loaded;
        };
        for (id, entry) in &root {
            let Some((index, advancement)) = advancements::find(id) else {
                continue;
            };
            let Some(Value::Object(criteria)) = entry.get("criteria") else {
                continue;
            };
            for (name, stamp) in criteria {
                let Some(known) = advancement
                    .criteria
                    .iter()
                    .find(|criterion| criterion.name == name)
                else {
                    continue;
                };
                let when = stamp
                    .as_str()
                    .and_then(|text| DateTime::parse_from_str(text, TIME_FORMAT).ok())
                    .map_or(0, |time| time.timestamp_millis());
                loaded
                    .obtained
                    .entry(index)
                    .or_default()
                    .insert(known.name, when);
            }
        }
        loaded
    }
}

#[derive(Debug)]
pub(crate) struct CriterionStep {
    pub(crate) player: PlayerId,
    pub(crate) index: usize,
    pub(crate) criterion: &'static str,
}

fn item_ids(stacks: &[Option<ItemStack>]) -> (Vec<u16>, i32) {
    let mut items: Vec<u16> = stacks.iter().flatten().map(|stack| stack.item.0).collect();
    let occupied = items.len() as i32;
    items.sort_unstable();
    items.dedup();
    (items, occupied)
}

fn matches_any(wanted: &ItemMatch, items: &[u16]) -> bool {
    items.iter().any(|item| wanted.matches(*item))
}

impl<L: ClientLink> Server<L> {
    pub fn award_advancement(&mut self, id: PlayerId, advancement: &str) -> bool {
        let Some((index, found)) = advancements::find(advancement) else {
            return false;
        };
        if !self.players.contains_key(&id) {
            return false;
        }
        for criterion in found.criteria {
            self.award_criterion(id, index, criterion.name);
        }
        true
    }

    pub fn done_advancements(&self, id: PlayerId) -> Vec<&'static str> {
        self.players
            .get(&id)
            .map(|player| player.advancements.done_ids())
            .unwrap_or_default()
    }

    pub(crate) fn award_criterion(&mut self, id: PlayerId, index: usize, criterion: &'static str) {
        let found = &advancements::all()[index];
        let Some(player) = self.players.get(&id) else {
            return;
        };
        if player.advancements.has(index, criterion)
            || !found.criteria.iter().any(|known| known.name == criterion)
        {
            return;
        }
        let step = CriterionStep {
            player: id,
            index,
            criterion,
        };
        self.fire(id, Continuation::Criterion(step), move |server| {
            Some(EventPayload::PlayerAdvancementCriterionGrant(
                PlayerAdvancementCriterionGrant {
                    advancement: found.id.to_owned(),
                    criterion: criterion.to_owned(),
                    player: server.snapshot_of(id)?,
                },
            ))
        });
    }

    pub(crate) fn finish_criterion(&mut self, step: CriterionStep, cancelled: bool) {
        if cancelled {
            return;
        }
        let CriterionStep {
            player: id,
            index,
            criterion,
        } = step;
        let Some(player) = self.players.get_mut(&id) else {
            return;
        };
        let was_done = player.advancements.is_done(index);
        if !player.advancements.grant(index, criterion, now_millis()) {
            return;
        }
        let now_done = player.advancements.is_done(index);
        if !was_done && now_done {
            self.complete_advancement(id, index);
        }
        self.flush_advancements(id, false, true);
    }

    fn complete_advancement(&mut self, id: PlayerId, index: usize) {
        let found = &advancements::all()[index];
        self.fire(
            id,
            Continuation::Notice(EventKind::PlayerAdvancementDone),
            move |server| {
                Some(EventPayload::PlayerAdvancementDone(PlayerAdvancementDone {
                    advancement: found.id.to_owned(),
                    player: server.snapshot_of(id)?,
                }))
            },
        );
        self.award_recipes(id, found.rewards.recipes);
        if found.rewards.experience > 0 {
            self.give_experience(id, found.rewards.experience);
        }
        if let Some(display) = &found.display
            && display.announce_to_chat
        {
            self.announce_advancement(id, display);
        }
    }

    fn announce_advancement(
        &mut self,
        id: PlayerId,
        display: &rspigot_data::advancements::Display,
    ) {
        let Some(name) = self.players.get(&id).map(|player| player.name().to_owned()) else {
            return;
        };
        let (key, color) = match display.frame {
            1 => ("chat.type.advancement.challenge", NamedColor::DarkPurple),
            2 => ("chat.type.advancement.goal", NamedColor::Green),
            _ => ("chat.type.advancement.task", NamedColor::Green),
        };
        let title = Component::text("[")
            .append(Component::translatable(display.title, Vec::new()))
            .append(Component::text("]"))
            .color(color);
        let message = Component::translatable(key, vec![Component::text(name), title]);
        tracing::info!("{}", crate::console::console_text(&message));
        self.push(Audience::All, ClientEvent::system(message));
    }

    pub(crate) fn flush_advancements(&mut self, id: PlayerId, reset: bool, show: bool) {
        let Some(player) = self.players.get_mut(&id) else {
            return;
        };
        let state = &mut player.advancements;
        let shown = visible_set(&|index| state.is_done(index));
        let all = advancements::all();
        let added: Vec<usize> = if reset {
            shown.iter().copied().collect()
        } else {
            shown.difference(&state.visible).copied().collect()
        };
        let removed: Vec<&'static str> = if reset {
            Vec::new()
        } else {
            state
                .visible
                .difference(&shown)
                .map(|index| all[*index].id)
                .collect()
        };
        let mut progress: BTreeSet<usize> = state
            .dirty
            .iter()
            .copied()
            .filter(|index| shown.contains(index))
            .collect();
        progress.extend(
            added
                .iter()
                .copied()
                .filter(|index| state.obtained.contains_key(index)),
        );
        state.visible = shown;
        state.dirty.clear();
        if !reset && added.is_empty() && removed.is_empty() && progress.is_empty() {
            return;
        }
        let event = ClientEvent::Advancements {
            reset,
            added: added.iter().map(|index| &all[*index]).collect(),
            removed,
            progress: progress
                .iter()
                .map(|index| (all[*index].id, state.criteria_of(*index)))
                .collect(),
            show,
        };
        player.link.send(event);
    }

    pub(crate) fn send_initial_advancements(&mut self, id: PlayerId) {
        self.flush_advancements(id, true, false);
    }

    pub(crate) fn select_advancement_tab(&mut self, id: PlayerId, tab: Option<&str>) {
        let selected = tab
            .and_then(advancements::find)
            .filter(|(_, found)| found.parent.is_none() && found.display.is_some())
            .map(|(index, _)| index);
        let Some(player) = self.players.get_mut(&id) else {
            return;
        };
        if player.advancements.selected_tab == selected {
            return;
        }
        player.advancements.selected_tab = selected;
        player.link.send(ClientEvent::SelectAdvancementsTab(
            selected.map(|index| advancements::all()[index].id),
        ));
    }

    fn trigger(&mut self, id: PlayerId, matches: impl Fn(&Trigger) -> bool) {
        let Some(player) = self.players.get(&id) else {
            return;
        };
        let mut granted: Vec<(usize, &'static str)> = Vec::new();
        for (index, found) in advancements::all().iter().enumerate() {
            if player.advancements.is_done(index) {
                continue;
            }
            for criterion in found.criteria {
                if matches(&criterion.trigger) && !player.advancements.has(index, criterion.name) {
                    granted.push((index, criterion.name));
                }
            }
        }
        for (index, criterion) in granted {
            self.award_criterion(id, index, criterion);
        }
    }

    pub(crate) fn trigger_inventory_changed(&mut self, id: PlayerId) {
        let Some(player) = self.players.get(&id) else {
            return;
        };
        let (items, occupied) = item_ids(player.inventory.slots());
        self.trigger(id, |trigger| match trigger {
            Trigger::InventoryChanged {
                items: wanted,
                occupied: least,
            } => occupied >= *least && wanted.iter().all(|one| matches_any(one, &items)),
            _ => false,
        });
    }

    pub(crate) fn trigger_consume_item(&mut self, id: PlayerId, stack: &ItemStack) {
        let item = stack.item.0;
        self.trigger(id, |trigger| match trigger {
            Trigger::ConsumeItem(wanted) => wanted.as_ref().is_none_or(|one| one.matches(item)),
            _ => false,
        });
    }

    pub(crate) fn trigger_killed_entity(&mut self, id: PlayerId, entity: &str) {
        self.trigger(id, |trigger| match trigger {
            Trigger::KilledEntity(wanted) => wanted.is_none_or(|name| name == entity),
            _ => false,
        });
    }

    pub(crate) fn trigger_entity_killed_player(&mut self, id: PlayerId) {
        self.trigger(id, |trigger| matches!(trigger, Trigger::EntityKilledPlayer));
    }

    pub(crate) fn trigger_slept_in_bed(&mut self, id: PlayerId) {
        self.trigger(id, |trigger| matches!(trigger, Trigger::SleptInBed));
    }

    pub(crate) fn trigger_recipe_unlocked(&mut self, id: PlayerId, recipe: &str) {
        self.trigger(
            id,
            |trigger| matches!(trigger, Trigger::RecipeUnlocked(wanted) if *wanted == recipe),
        );
    }

    pub(crate) fn tick_advancements(&mut self) {
        let ids: Vec<PlayerId> = self.players.keys().copied().collect();
        for id in ids {
            let Some(player) = self.players.get_mut(&id) else {
                continue;
            };
            let first = !std::mem::replace(&mut player.advancements.announced, true);
            let state = player.inventory_state;
            let changed = player.advancements.inventory_seen != Some(state);
            player.advancements.inventory_seen = Some(state);
            let periodic = player.vitals.tick_count % 20 == 0;
            if changed {
                self.trigger_inventory_changed(id);
            }
            if first || periodic {
                self.trigger(id, |trigger| matches!(trigger, Trigger::Tick));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn story() -> (usize, usize, usize) {
        let root = advancements::find("minecraft:story/root").expect("root").0;
        let child = tree().children[root][0];
        let grandchild = tree().children[child][0];
        (root, child, grandchild)
    }

    #[test]
    fn the_roots_and_their_first_children_are_shown_from_the_start() {
        let shown = visible_set(&|_| false);
        let (root, child, grandchild) = story();
        assert!(shown.contains(&root) && shown.contains(&child));
        assert!(!shown.contains(&grandchild));
    }

    #[test]
    fn advancements_without_a_display_are_never_shown() {
        let shown = visible_set(&|_| true);
        let recipe = advancements::find("minecraft:recipes/misc/charcoal")
            .expect("recipe")
            .0;
        assert!(!shown.contains(&recipe));
    }

    #[test]
    fn a_done_advancement_shows_its_children_and_their_children() {
        let (_, child, grandchild) = story();
        let shown = visible_set(&|index| index == child);
        assert!(shown.contains(&grandchild));
    }

    #[test]
    fn progress_survives_the_json_file_with_its_done_flag() {
        let (index, found) =
            advancements::find("minecraft:story/mine_stone").expect("an advancement");
        let mut state = PlayerAdvancements::default();
        assert!(state.grant(index, found.criteria[0].name, 1_700_000_000_000));
        assert!(!state.grant(index, found.criteria[0].name, 5));
        let text = state.to_json();
        assert!(text.contains("\"done\":true"));
        let restored = PlayerAdvancements::from_json(&text);
        assert!(restored.is_done(index));
        assert_eq!(restored.done_ids(), vec!["minecraft:story/mine_stone"]);
    }

    #[test]
    fn unknown_advancements_and_criteria_are_dropped_on_load() {
        let text = r#"{"minecraft:nope":{"criteria":{"x":"2020-01-01 00:00:00 +0000"},"done":true},
            "minecraft:story/mine_stone":{"criteria":{"nope":"2020-01-01 00:00:00 +0000"},"done":false}}"#;
        assert_eq!(
            PlayerAdvancements::from_json(text),
            PlayerAdvancements::default()
        );
    }
}

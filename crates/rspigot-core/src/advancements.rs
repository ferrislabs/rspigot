use std::cell::Cell;
use std::collections::{BTreeMap, BTreeSet};
use std::time::{SystemTime, UNIX_EPOCH};

use chrono::{DateTime, Utc};
use rspigot_data::advancements::{self, Advancement, ItemMatch, Trigger};
use rspigot_data::registries;
use rspigot_domain::entity::Entity;
use rspigot_domain::item::ItemStack;
use rspigot_domain::player::PlayerId;
use rspigot_domain::text::{Component, NamedColor};
use rspigot_domain::{BlockPos, EntityId, Vec3};
use rspigot_entities::EntityKind as Kind;
use serde_json::{Map, Value};

use crate::advancement_conditions::{
    self as conditions, DamageFacts, Distance, EntityFacts, Facts, Place,
};
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

fn build_tree(all: &[Advancement]) -> Tree {
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
}

fn tree() -> &'static Tree {
    thread_local! {
        static CACHE: Cell<Option<(usize, &'static Tree)>> = const { Cell::new(None) };
    }
    let all = advancements::all();
    let key = all.as_ptr() as usize;
    if let Some((cached, tree)) = CACHE.with(Cell::get)
        && cached == key
    {
        return tree;
    }
    let tree: &'static Tree = Box::leak(Box::new(build_tree(all)));
    CACHE.with(|cache| cache.set(Some((key, tree))));
    tree
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
    nether_entry: Option<BlockPos>,
    levitation_start: Option<u64>,
    lava_start: Option<(u64, u64)>,
    inside: BTreeSet<u16>,
}

impl PlayerAdvancements {
    pub(crate) fn reindex(&mut self, old: &[Advancement], new: &[Advancement]) {
        let obtained = std::mem::take(&mut self.obtained);
        for (index, criteria) in obtained {
            let Some(found) = old
                .get(index)
                .and_then(|advancement| new.iter().position(|one| one.id == advancement.id))
            else {
                continue;
            };
            for (name, when) in criteria {
                if let Some(criterion) = new[found]
                    .criteria
                    .iter()
                    .find(|criterion| criterion.name == name)
                {
                    self.obtained
                        .entry(found)
                        .or_default()
                        .insert(criterion.name, when);
                }
            }
        }
        self.selected_tab = self
            .selected_tab
            .and_then(|index| old.get(index))
            .and_then(|advancement| new.iter().position(|one| one.id == advancement.id));
        self.visible.clear();
        self.dirty.clear();
    }

    pub(crate) fn set_nether_entry(&mut self, at: Option<BlockPos>) {
        self.nether_entry = at;
    }

    pub(crate) fn take_nether_entry(&mut self) -> Option<BlockPos> {
        self.nether_entry.take()
    }

    pub(crate) fn levitation_height(&mut self, levitating: bool, y: f64) -> Option<f64> {
        if !levitating {
            self.levitation_start = None;
            return None;
        }
        let start = f64::from_bits(*self.levitation_start.get_or_insert(y.to_bits()));
        Some(y - start)
    }

    pub(crate) fn lava_distance(&mut self, in_lava: bool, x: f64, z: f64) -> Option<f64> {
        if !in_lava {
            self.lava_start = None;
            return None;
        }
        let (sx, sz) = self
            .lava_start
            .get_or_insert((x.to_bits(), z.to_bits()))
            .to_owned();
        Some((x - f64::from_bits(sx)).hypot(z - f64::from_bits(sz)))
    }

    pub(crate) fn entered_blocks(&mut self, now: BTreeSet<u16>) -> Vec<u16> {
        let entered = now.difference(&self.inside).copied().collect();
        self.inside = now;
        entered
    }

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

    fn revoke(&mut self, index: usize, criterion: &str) -> bool {
        let Some(criteria) = self.obtained.get_mut(&index) else {
            return false;
        };
        let removed = criteria.remove(criterion).is_some();
        if criteria.is_empty() {
            self.obtained.remove(&index);
        }
        if removed {
            self.dirty.insert(index);
        }
        removed
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

#[derive(Debug, Default)]
pub(crate) struct AdvancementEvent {
    pub(crate) name: &'static str,
    pub(crate) item: Option<u16>,
    pub(crate) inventory: Vec<u16>,
    pub(crate) entities: Vec<(&'static str, EntityFacts<'static>)>,
    pub(crate) at: Option<BlockPos>,
    pub(crate) recipe: Option<String>,
    pub(crate) loot_table: Option<String>,
    pub(crate) to: Option<String>,
    pub(crate) amounts: Vec<(&'static str, f64)>,
    pub(crate) damage: Option<DamageFacts<'static>>,
    pub(crate) victims: Vec<EntityFacts<'static>>,
    pub(crate) weapon: Option<u16>,
    pub(crate) ingredients: Vec<u16>,
    pub(crate) looking_at: Option<&'static str>,
}

impl AdvancementEvent {
    pub(crate) fn named(name: &'static str) -> Self {
        Self {
            name,
            ..Self::default()
        }
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

    pub fn obtained_criteria(&self, id: PlayerId, advancement: &str) -> Vec<&'static str> {
        let Some((index, found)) = advancements::find(advancement) else {
            return Vec::new();
        };
        let Some(player) = self.players.get(&id) else {
            return Vec::new();
        };
        found
            .criteria
            .iter()
            .filter(|criterion| player.advancements.has(index, criterion.name))
            .map(|criterion| criterion.name)
            .collect()
    }

    pub fn advancement_is_done(&self, id: PlayerId, advancement: &str) -> bool {
        let Some((index, _)) = advancements::find(advancement) else {
            return false;
        };
        self.players
            .get(&id)
            .is_some_and(|player| player.advancements.is_done(index))
    }

    pub fn grant_advancement_criterion(
        &mut self,
        id: PlayerId,
        advancement: &str,
        criterion: &str,
    ) -> bool {
        let Some((index, found)) = advancements::find(advancement) else {
            return false;
        };
        let Some(known) = found.criteria.iter().find(|known| known.name == criterion) else {
            return false;
        };
        let Some(player) = self.players.get(&id) else {
            return false;
        };
        if player.advancements.has(index, known.name) {
            return false;
        }
        self.award_criterion(id, index, known.name);
        true
    }

    pub fn revoke_advancement_criterion(
        &mut self,
        id: PlayerId,
        advancement: &str,
        criterion: &str,
    ) -> bool {
        let Some((index, _)) = advancements::find(advancement) else {
            return false;
        };
        let Some(player) = self.players.get_mut(&id) else {
            return false;
        };
        if !player.advancements.revoke(index, criterion) {
            return false;
        }
        self.flush_advancements(id, false, false);
        true
    }

    pub fn done_advancements(&self, id: PlayerId) -> Vec<&'static str> {
        self.players
            .get(&id)
            .map(|player| player.advancements.done_ids())
            .unwrap_or_default()
    }

    pub(crate) fn advancement_disabled(&self, id: &str) -> bool {
        self.config.disabled_advancements.iter().any(|pattern| {
            pattern == "*"
                || pattern
                    .strip_suffix('*')
                    .map_or(pattern == id, |prefix| id.starts_with(prefix))
        })
    }

    pub(crate) fn award_criterion(&mut self, id: PlayerId, index: usize, criterion: &'static str) {
        let found = &advancements::all()[index];
        if self.advancement_disabled(found.id) {
            return;
        }
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
        self.grant_reward_loot(id, found.rewards.loot);
        if let Some(function) = found.rewards.function {
            self.run_reward_function(id, function);
        }
        if let Some(display) = &found.display
            && display.announce_to_chat
            && self.config.rules.show_advancement_messages
        {
            self.announce_advancement(id, display);
        }
    }

    fn grant_reward_loot(&mut self, id: PlayerId, tables: &[&str]) {
        if tables.is_empty() {
            return;
        }
        let mut me = crate::loot::LootEntity::of(Kind::Player);
        me.name = self.players.get(&id).map(|player| player.name().to_owned());
        let mut drops = Vec::new();
        for name in tables {
            let Some(table) = rspigot_data::loot::table(name) else {
                continue;
            };
            let context = crate::loot::LootContext {
                this: Some(&me),
                player: Some(&me),
                ..crate::loot::LootContext::default()
            };
            drops.extend(crate::loot::table_drops(table, &context, &mut self.rng));
        }
        for stack in drops {
            self.give_or_drop(id, stack);
        }
    }

    fn run_reward_function(&mut self, id: PlayerId, function: &str) {
        let Some(name) = self.players.get(&id).map(|player| player.name().to_owned()) else {
            return;
        };
        let line = format!("execute as {name} at {name} run function {function}");
        crate::commands::execute(self, crate::commands::CommandSender::Console, &line);
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

    fn due(&self, id: PlayerId, matches: impl Fn(&Trigger) -> bool) -> Vec<(usize, &'static str)> {
        let Some(player) = self.players.get(&id) else {
            return Vec::new();
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
        granted
    }

    fn trigger(&mut self, id: PlayerId, matches: impl Fn(&Trigger) -> bool) {
        for (index, criterion) in self.due(id, matches) {
            self.award_criterion(id, index, criterion);
        }
    }

    pub(crate) fn trigger_item(&mut self, id: PlayerId, name: &'static str, stack: &ItemStack) {
        let mut event = AdvancementEvent::named(name);
        event.item = Some(stack.item.0);
        self.trigger_event(id, event);
    }

    pub(crate) fn entity_slot(
        slot: &'static str,
        entity: &Entity,
    ) -> (&'static str, EntityFacts<'static>) {
        let mut facts = EntityFacts::of(entity.base.kind.name(), entity.is_baby());
        facts.component = crate::advancement_facts::variant_component(&entity.base);
        facts.body = crate::advancement_facts::body_stack(entity);
        (slot, facts)
    }

    pub(crate) fn slot_near(
        &self,
        slot: &'static str,
        id: EntityId,
        from: Vec3,
    ) -> Option<(&'static str, EntityFacts<'static>)> {
        let (slot, mut facts) = self.slot_of(slot, id)?;
        facts.distance = self
            .entity_position(id)
            .or_else(|| {
                self.player_of_entity(id)
                    .and_then(|p| self.players.get(&p))
                    .map(|p| p.position)
            })
            .map(|at| Distance {
                horizontal: (at.x - from.x).hypot(at.z - from.z),
                absolute: (at - from).length(),
            });
        Some((slot, facts))
    }

    pub(crate) fn facts_of(&self, id: EntityId, from: Vec3) -> Option<EntityFacts<'static>> {
        self.slot_near("entity", id, from).map(|(_, facts)| facts)
    }

    pub(crate) fn slot_of(
        &self,
        slot: &'static str,
        id: EntityId,
    ) -> Option<(&'static str, EntityFacts<'static>)> {
        self.entities
            .store
            .get(id)
            .map(|entity| Self::entity_slot(slot, entity))
            .or_else(|| {
                self.player_of_entity(id)
                    .map(|_| (slot, EntityFacts::of(Kind::Player.name(), false)))
            })
    }

    pub(crate) fn trigger_event(&mut self, id: PlayerId, event: AdvancementEvent) {
        let granted = {
            let Some(player) = self.players.get(&id) else {
                return;
            };
            let dimension = self.world.dimension.key.to_string();
            let here = player.position.block();
            let anchor = event.at.unwrap_or(here);
            let structures_here = self.structures_at(here);
            let structures_there = self.structures_at(anchor);
            let place = |pos: BlockPos, y: f64| Place {
                dimension: &dimension,
                biome: registries::biome_name(self.biome_at(pos).0),
                y,
                structures: if pos == here {
                    &structures_here
                } else {
                    &structures_there
                },
                smokey: event.at.is_some() && self.is_smokey_pos(pos),
            };
            let chunks = &self.world.chunks;
            let mut facts =
                Facts::with_blocks(|dx, dy, dz| chunks.block(anchor.offset(dx, dy, dz)));
            facts.item = event.item;
            facts.inventory = event.inventory;
            facts.playable = event
                .item
                .is_some_and(crate::advancement_facts::is_playable);
            facts.entities = event.entities;
            facts.player = Some(place(here, player.position.y));
            facts.location = Some(place(anchor, f64::from(anchor.y)));
            facts.recipe = event.recipe.as_deref();
            facts.loot_table = event.loot_table.as_deref();
            facts.to = event.to.as_deref();
            facts.amounts = event.amounts;
            facts.damage = event.damage;
            facts.victims = event.victims;
            facts.weapon = event.weapon;
            facts.ingredients = event.ingredients;
            facts.effects = player
                .effects
                .iter()
                .filter_map(|e| e.effect.name())
                .collect();
            let mut own = EntityFacts::of(Kind::Player.name(), false);
            own.looking_at = event.looking_at;
            own.mainhand = player.inventory.held().map(|stack| stack.item.0);
            if let Some(vehicle) = self.entities.rides.vehicle_of(player.entity_id) {
                own.vehicle = self.kind_of(vehicle).map(Kind::name);
                own.vehicle_passenger = self
                    .entities
                    .rides
                    .passengers_of(vehicle)
                    .iter()
                    .find(|rider| **rider != player.entity_id)
                    .and_then(|rider| self.kind_of(*rider))
                    .map(Kind::name);
            }
            facts.player_entity = Some(own);
            let name = event.name;
            self.due(id, |trigger| {
                matches!(trigger, Trigger::Event { name: known, conditions }
                    if *known == name && conditions::matches(*conditions, &facts))
            })
        };
        for (index, criterion) in granted {
            self.award_criterion(id, index, criterion);
        }
    }

    pub(crate) fn trigger_inventory_changed(&mut self, id: PlayerId) {
        let Some(player) = self.players.get(&id) else {
            return;
        };
        let (_, occupied) = item_ids(player.inventory.slots());
        let slots = player.inventory.slots();
        let full = slots
            .iter()
            .flatten()
            .filter(|stack| i32::from(stack.count.get()) >= i32::from(stack.item.max_stack_size()))
            .count();
        let mut event = AdvancementEvent::named("minecraft:inventory_changed");
        event.inventory = slots.iter().flatten().map(|stack| stack.item.0).collect();
        event.amounts.push(("slots.occupied", f64::from(occupied)));
        event.amounts.push(("slots.full", full as f64));
        event
            .amounts
            .push(("slots.empty", (slots.len() as i32 - occupied) as f64));
        self.trigger_event(id, event);
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
        self.trigger_item(id, "minecraft:consume_item", stack);
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
                self.auto_discover_recipes(id);
                self.trigger_inventory_changed(id);
            }
            if first || periodic {
                self.trigger(id, |trigger| matches!(trigger, Trigger::Tick));
            }
            if periodic {
                self.trigger_event(id, AdvancementEvent::named("minecraft:location"));
            }
            self.tick_levitation(id);
            self.tick_lava_ride(id);
            self.tick_inside_blocks(id);
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

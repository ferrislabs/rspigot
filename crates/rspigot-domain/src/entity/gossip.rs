use std::collections::BTreeMap;

use rspigot_nbt::{Compound, Tag};
use uuid::Uuid;

use crate::entity::save::{read_uuid, uuid_tag};
use crate::random::RandomSource;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum GossipType {
    MajorNegative,
    MinorNegative,
    MinorPositive,
    MajorPositive,
    Trading,
}

impl GossipType {
    pub const ALL: [Self; 5] = [
        Self::MajorNegative,
        Self::MinorNegative,
        Self::MinorPositive,
        Self::MajorPositive,
        Self::Trading,
    ];

    pub const fn id(self) -> &'static str {
        match self {
            Self::MajorNegative => "major_negative",
            Self::MinorNegative => "minor_negative",
            Self::MinorPositive => "minor_positive",
            Self::MajorPositive => "major_positive",
            Self::Trading => "trading",
        }
    }

    pub const fn weight(self) -> i32 {
        match self {
            Self::MajorNegative => -5,
            Self::MinorNegative => -1,
            Self::MinorPositive => 1,
            Self::MajorPositive => 5,
            Self::Trading => 1,
        }
    }

    pub const fn max(self) -> i32 {
        match self {
            Self::MajorNegative => 100,
            Self::MinorNegative => 200,
            Self::MinorPositive => 25,
            Self::MajorPositive => 20,
            Self::Trading => 25,
        }
    }

    pub const fn decay_per_day(self) -> i32 {
        match self {
            Self::MajorNegative => 10,
            Self::MinorNegative => 20,
            Self::MinorPositive => 1,
            Self::MajorPositive => 0,
            Self::Trading => 2,
        }
    }

    pub const fn decay_per_transfer(self) -> i32 {
        match self {
            Self::MajorNegative => 10,
            Self::MinorNegative => 20,
            Self::MinorPositive => 5,
            Self::MajorPositive => 20,
            Self::Trading => 20,
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|kind| kind.id() == id)
    }

    const fn slot(self) -> usize {
        self as usize
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReputationEvent {
    ZombieVillagerCured,
    Trade,
    VillagerHurt,
    VillagerKilled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GossipEntry {
    pub target: Uuid,
    pub kind: GossipType,
    pub value: i32,
}

impl GossipEntry {
    pub const fn weighted_value(&self) -> i32 {
        self.value * self.kind.weight()
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct EntityGossips {
    entries: [i32; 5],
}

impl EntityGossips {
    fn get(&self, kind: GossipType) -> i32 {
        self.entries[kind.slot()]
    }

    fn is_empty(&self) -> bool {
        self.entries.iter().all(|value| *value == 0)
    }

    fn weighted(&self, accept: impl Fn(GossipType) -> bool) -> i32 {
        GossipType::ALL
            .into_iter()
            .filter(|kind| accept(*kind))
            .map(|kind| self.get(kind) * kind.weight())
            .sum()
    }

    fn unweighted(&self, accept: impl Fn(GossipType) -> bool) -> i32 {
        GossipType::ALL
            .into_iter()
            .filter(|kind| accept(*kind))
            .map(|kind| self.get(kind))
            .sum()
    }

    fn decay(&mut self) {
        for kind in GossipType::ALL {
            let value = self.get(kind);
            if value == 0 {
                continue;
            }
            let next = value - kind.decay_per_day();
            self.entries[kind.slot()] = if next < 2 { 0 } else { next };
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GossipContainer {
    gossips: BTreeMap<Uuid, EntityGossips>,
}

impl GossipContainer {
    pub fn is_empty(&self) -> bool {
        self.gossips.is_empty()
    }

    pub fn clear(&mut self) {
        self.gossips.clear();
    }

    pub fn decay(&mut self) {
        self.gossips.retain(|_, gossips| {
            gossips.decay();
            !gossips.is_empty()
        });
    }

    pub fn unpack(&self) -> Vec<GossipEntry> {
        let mut entries = Vec::new();
        for (target, gossips) in &self.gossips {
            for kind in GossipType::ALL {
                let value = gossips.get(kind);
                if value != 0 {
                    entries.push(GossipEntry {
                        target: *target,
                        kind,
                        value,
                    });
                }
            }
        }
        entries
    }

    pub fn reputation(&self, entity: Uuid) -> i32 {
        self.reputation_of(entity, |_| true)
    }

    pub fn reputation_of(&self, entity: Uuid, accept: impl Fn(GossipType) -> bool) -> i32 {
        self.gossips
            .get(&entity)
            .map_or(0, |gossips| gossips.weighted(accept))
    }

    pub fn unweighted_reputation_of(
        &self,
        entity: Uuid,
        accept: impl Fn(GossipType) -> bool,
    ) -> i32 {
        self.gossips
            .get(&entity)
            .map_or(0, |gossips| gossips.unweighted(accept))
    }

    pub fn count_for_type(&self, kind: GossipType, accept: impl Fn(f64) -> bool) -> usize {
        self.gossips
            .values()
            .filter(|gossips| accept(f64::from(gossips.get(kind) * kind.weight())))
            .count()
    }

    pub fn add(&mut self, target: Uuid, kind: GossipType, amount: i32) {
        let gossips = self.gossips.entry(target).or_default();
        let old = gossips.get(kind);
        let mut merged = old + amount;
        if merged > kind.max() {
            merged = kind.max().max(old);
        }
        let value = merged.min(kind.max()).max(0);
        gossips.entries[kind.slot()] = if value < 2 { 0 } else { value };
        if gossips.is_empty() {
            self.gossips.remove(&target);
        }
    }

    pub fn set(&mut self, target: Uuid, kind: GossipType, amount: i32) {
        let current = self.unweighted_reputation_of(target, |candidate| candidate == kind);
        let delta = amount - current;
        if delta == 0 {
            return;
        }
        self.add(target, kind, delta);
    }

    pub fn remove(&mut self, target: Uuid, kind: GossipType, amount: i32) {
        self.add(target, kind, -amount);
    }

    pub fn remove_type(&mut self, target: Uuid, kind: GossipType) {
        if self.gossips.contains_key(&target) {
            self.set(target, kind, 0);
            if self
                .gossips
                .get(&target)
                .is_some_and(EntityGossips::is_empty)
            {
                self.gossips.remove(&target);
            }
        }
    }

    pub fn put_all(&mut self, other: &Self) {
        for (target, gossips) in &other.gossips {
            let own = self.gossips.entry(*target).or_default();
            for kind in GossipType::ALL {
                let value = gossips.get(kind);
                if value != 0 {
                    own.entries[kind.slot()] = value;
                }
            }
        }
    }

    pub fn select_for_transfer(
        &self,
        random: &mut impl RandomSource,
        max_count: i32,
    ) -> Vec<GossipEntry> {
        let entries = self.unpack();
        if entries.is_empty() {
            return Vec::new();
        }
        let mut cumulative = Vec::with_capacity(entries.len());
        let mut total = 0;
        for entry in &entries {
            total += entry.weighted_value().abs();
            cumulative.push(total - 1);
        }
        let mut chosen: Vec<usize> = Vec::new();
        for _ in 0..max_count {
            let roll = random.next_int(total);
            let index = match cumulative.binary_search(&roll) {
                Ok(found) => found,
                Err(insertion) => insertion,
            };
            if let Some(index) = Some(index).filter(|index| *index < entries.len())
                && !chosen.contains(&index)
            {
                chosen.push(index);
            }
        }
        chosen.into_iter().map(|index| entries[index]).collect()
    }

    pub fn transfer_from(
        &mut self,
        source: &Self,
        random: &mut impl RandomSource,
        max_count: i32,
    ) -> usize {
        let selected = source.select_for_transfer(random, max_count);
        for entry in &selected {
            let value = entry.value - entry.kind.decay_per_transfer();
            if value > 1 {
                let own = self.unweighted_reputation_of(entry.target, |kind| kind == entry.kind);
                self.set(entry.target, entry.kind, own.max(value));
            }
        }
        selected.len()
    }

    pub fn to_nbt(&self) -> Tag {
        Tag::List(
            self.unpack()
                .into_iter()
                .map(|entry| {
                    Tag::Compound(
                        Compound::new()
                            .with("Target", uuid_tag(entry.target))
                            .with("Type", entry.kind.id())
                            .with("Value", entry.value),
                    )
                })
                .collect(),
        )
    }

    pub fn from_nbt(tag: &Tag) -> Self {
        let mut container = Self::default();
        let Tag::List(entries) = tag else {
            return container;
        };
        for entry in entries {
            let Tag::Compound(entry) = entry else {
                continue;
            };
            let (Some(target), Some(kind), Some(value)) = (
                entry.get("Target").and_then(read_uuid),
                entry.get_str("Type").and_then(GossipType::from_id),
                entry.get_i32("Value"),
            ) else {
                continue;
            };
            container.gossips.entry(target).or_default().entries[kind.slot()] = value;
        }
        container
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::random::LegacyRandom;

    fn id(value: u128) -> Uuid {
        Uuid::from_u128(value)
    }

    #[test]
    fn the_constants_match_the_gossip_types() {
        assert_eq!(GossipType::MajorNegative.weight(), -5);
        assert_eq!(GossipType::MinorNegative.max(), 200);
        assert_eq!(GossipType::MinorPositive.decay_per_day(), 1);
        assert_eq!(GossipType::MajorPositive.decay_per_day(), 0);
        assert_eq!(GossipType::Trading.decay_per_transfer(), 20);
        assert_eq!(GossipType::from_id("trading"), Some(GossipType::Trading));
    }

    #[test]
    fn adding_clamps_to_the_maximum_and_drops_values_below_two() {
        let mut gossips = GossipContainer::default();
        gossips.add(id(1), GossipType::MajorPositive, 15);
        gossips.add(id(1), GossipType::MajorPositive, 15);
        assert_eq!(gossips.reputation(id(1)), 20 * 5);
        gossips.add(id(2), GossipType::Trading, 1);
        assert_eq!(gossips.reputation(id(2)), 0);
        assert_eq!(gossips.unpack().len(), 1);
    }

    #[test]
    fn reputation_is_the_weighted_sum_over_the_selected_types() {
        let mut gossips = GossipContainer::default();
        gossips.add(id(1), GossipType::MajorPositive, 10);
        gossips.add(id(1), GossipType::MinorNegative, 30);
        assert_eq!(gossips.reputation(id(1)), 50 - 30);
        assert_eq!(
            gossips.reputation_of(id(1), |kind| kind == GossipType::MajorPositive),
            50
        );
        assert_eq!(gossips.unweighted_reputation_of(id(1), |_| true), 40);
    }

    #[test]
    fn decay_subtracts_the_daily_amount_and_prunes_what_falls_below_two() {
        let mut gossips = GossipContainer::default();
        gossips.add(id(1), GossipType::MinorNegative, 25);
        gossips.add(id(1), GossipType::MinorPositive, 3);
        gossips.add(id(2), GossipType::Trading, 3);
        gossips.decay();
        assert_eq!(
            gossips.unweighted_reputation_of(id(1), |kind| kind == GossipType::MinorNegative),
            5
        );
        assert_eq!(
            gossips.unweighted_reputation_of(id(1), |kind| kind == GossipType::MinorPositive),
            2
        );
        assert_eq!(
            gossips
                .unpack()
                .iter()
                .filter(|e| e.target == id(2))
                .count(),
            0
        );
        gossips.decay();
        assert_eq!(
            gossips.unweighted_reputation_of(id(1), |kind| kind == GossipType::MinorPositive),
            0
        );
    }

    #[test]
    fn a_transfer_copies_the_decayed_values_and_keeps_the_larger_one() {
        let mut source = GossipContainer::default();
        source.add(id(1), GossipType::MajorNegative, 50);
        let mut target = GossipContainer::default();
        target.add(id(1), GossipType::MajorNegative, 45);
        let mut random = LegacyRandom::new(3);
        let count = target.transfer_from(&source, &mut random, 10);
        assert_eq!(count, 1);
        assert_eq!(
            target.unweighted_reputation_of(id(1), |kind| kind == GossipType::MajorNegative),
            45
        );
        let mut fresh = GossipContainer::default();
        fresh.transfer_from(&source, &mut random, 10);
        assert_eq!(
            fresh.unweighted_reputation_of(id(1), |kind| kind == GossipType::MajorNegative),
            40
        );
    }

    #[test]
    fn values_that_do_not_survive_the_transfer_loss_are_not_copied() {
        let mut source = GossipContainer::default();
        source.add(id(1), GossipType::MajorPositive, 20);
        let mut target = GossipContainer::default();
        let mut random = LegacyRandom::new(8);
        target.transfer_from(&source, &mut random, 10);
        assert!(target.is_empty());
    }

    #[test]
    fn gossips_round_trip_through_the_vanilla_list_layout() {
        let mut gossips = GossipContainer::default();
        gossips.add(id(7), GossipType::MajorPositive, 20);
        gossips.add(id(7), GossipType::Trading, 5);
        let tag = gossips.to_nbt();
        let Tag::List(entries) = &tag else {
            panic!("list expected");
        };
        let Tag::Compound(first) = &entries[0] else {
            panic!("compound expected");
        };
        assert_eq!(first.get_str("Type"), Some("major_positive"));
        assert_eq!(first.get_i32("Value"), Some(20));
        assert_eq!(GossipContainer::from_nbt(&tag), gossips);
    }

    #[test]
    fn removing_a_type_clears_the_target_when_nothing_is_left() {
        let mut gossips = GossipContainer::default();
        gossips.add(id(1), GossipType::MajorNegative, 10);
        gossips.remove_type(id(1), GossipType::MajorNegative);
        assert!(gossips.is_empty());
    }

    #[test]
    fn counting_filters_on_the_weighted_value() {
        let mut gossips = GossipContainer::default();
        gossips.add(id(1), GossipType::MajorNegative, 10);
        gossips.add(id(2), GossipType::MajorNegative, 4);
        assert_eq!(
            gossips.count_for_type(GossipType::MajorNegative, |v| v <= -20.0),
            2
        );
        assert_eq!(
            gossips.count_for_type(GossipType::MajorNegative, |v| v <= -30.0),
            1
        );
    }
}

use std::collections::HashSet;
use std::sync::OnceLock;

use super::super::place::Run;
use super::super::prov::{BlockSet, Pos};
use super::super::treedeco::java_hash_order;

const SIDES: [(i32, i32, i32); 6] = [
    (0, -1, 0),
    (0, 1, 0),
    (0, 0, -1),
    (0, 0, 1),
    (-1, 0, 0),
    (1, 0, 0),
];

struct JavaSet {
    table: Vec<Vec<Pos>>,
    len: usize,
}

impl JavaSet {
    fn new() -> Self {
        Self {
            table: vec![Vec::new(); 16],
            len: 0,
        }
    }

    fn bucket(&self, (x, y, z): Pos) -> usize {
        let hash = y
            .wrapping_add(z.wrapping_mul(31))
            .wrapping_mul(31)
            .wrapping_add(x) as u32;
        ((hash ^ (hash >> 16)) as usize) & (self.table.len() - 1)
    }

    fn add(&mut self, pos: Pos) {
        let bucket = self.bucket(pos);
        if self.table[bucket].contains(&pos) {
            return;
        }
        self.table[bucket].push(pos);
        self.len += 1;
        if self.len > self.table.len() * 3 / 4 {
            let grown = vec![Vec::new(); self.table.len() * 2];
            let old = std::mem::replace(&mut self.table, grown);
            for pos in old.into_iter().flatten() {
                let bucket = self.bucket(pos);
                self.table[bucket].push(pos);
            }
        }
    }

    fn pop_first(&mut self) -> Option<Pos> {
        let chain = self.table.iter_mut().find(|chain| !chain.is_empty())?;
        self.len -= 1;
        Some(chain.remove(0))
    }
}

pub(super) struct Bounds {
    min: Pos,
    max: Pos,
}

impl Bounds {
    pub(super) fn of(groups: &[&[Pos]]) -> Option<Self> {
        let mut all = groups.iter().flat_map(|group| group.iter().copied());
        let first = all.next()?;
        let mut bounds = Self {
            min: first,
            max: first,
        };
        for (x, y, z) in all {
            bounds.min = (
                bounds.min.0.min(x),
                bounds.min.1.min(y),
                bounds.min.2.min(z),
            );
            bounds.max = (
                bounds.max.0.max(x),
                bounds.max.1.max(y),
                bounds.max.2.max(z),
            );
        }
        Some(bounds)
    }

    fn contains(&self, (x, y, z): Pos) -> bool {
        (self.min.0..=self.max.0).contains(&x)
            && (self.min.1..=self.max.1).contains(&y)
            && (self.min.2..=self.max.2).contains(&z)
    }
}

fn distance_at(state: rspigot_domain::BlockStateId) -> Option<usize> {
    static HOLDS: OnceLock<BlockSet> = OnceLock::new();
    if HOLDS
        .get_or_init(|| BlockSet::tag("minecraft:prevents_nearby_leaf_decay"))
        .contains(state)
    {
        return Some(0);
    }
    state.property("distance")?.parse().ok()
}

pub(super) fn update(run: &mut Run, bounds: &Bounds, logs: &[Pos], filled: &[Pos]) {
    let mut full: HashSet<Pos> = filled
        .iter()
        .copied()
        .filter(|pos| bounds.contains(*pos))
        .collect();
    let mut layers: Vec<JavaSet> = (0..7).map(|_| JavaSet::new()).collect();
    for pos in java_hash_order(logs) {
        layers[0].add(pos);
    }
    let mut layer = 0usize;
    while layer < 7 {
        let Some(pos) = layers[layer].pop_first() else {
            layer += 1;
            continue;
        };
        if !bounds.contains(pos) {
            continue;
        }
        if layer != 0 {
            let state = run.region.get(pos.0, pos.1, pos.2);
            if let Some(updated) = state.with_property("distance", &layer.to_string()) {
                run.region.set(pos.0, pos.1, pos.2, updated);
            }
        }
        full.insert(pos);
        for (dx, dy, dz) in SIDES {
            let next = (pos.0 + dx, pos.1 + dy, pos.2 + dz);
            if !bounds.contains(next) || full.contains(&next) {
                continue;
            }
            let state = run.region.get(next.0, next.1, next.2);
            let Some(current) = distance_at(state) else {
                continue;
            };
            let candidate = current.min(layer + 1);
            if candidate < 7 {
                layers[candidate].add(next);
                layer = layer.min(candidate);
            }
        }
    }
}

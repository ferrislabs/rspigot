use crate::generated::static_registries::STATIC;
use crate::generated::synced::{BIOME, DIMENSION_TYPE};

pub use crate::generated::synced::SYNCED;

pub struct SyncedRegistry {
    pub id: &'static str,
    pub entries: &'static [&'static str],
}

pub(crate) struct StaticRegistry {
    pub(crate) id: &'static str,
    pub(crate) entries: &'static [&'static str],
    pub(crate) by_name: &'static [u16],
}

impl StaticRegistry {
    pub(crate) fn index_of(&self, name: &str) -> Option<u16> {
        let position = self
            .by_name
            .binary_search_by(|&index| self.entries[usize::from(index)].cmp(name))
            .ok()?;
        Some(self.by_name[position])
    }

    pub(crate) fn name_of(&self, id: u16) -> Option<&'static str> {
        self.entries.get(usize::from(id)).copied()
    }
}

pub(crate) fn static_registry(registry: &str) -> Option<&'static StaticRegistry> {
    STATIC
        .binary_search_by(|candidate| candidate.id.cmp(registry))
        .ok()
        .map(|position| &STATIC[position])
}

pub fn static_registry_ids() -> impl Iterator<Item = &'static str> {
    STATIC.iter().map(|registry| registry.id)
}

pub fn synced_entries(registry: &str) -> Option<&'static [&'static str]> {
    SYNCED
        .iter()
        .find(|candidate| candidate.id == registry)
        .map(|candidate| candidate.entries)
}

pub fn synced_index(registry: &str, entry: &str) -> Option<u32> {
    index_in(synced_entries(registry)?, entry)
}

pub fn static_index(registry: &str, entry: &str) -> Option<u32> {
    static_registry(registry)?.index_of(entry).map(u32::from)
}

pub fn static_entries(registry: &str) -> Option<&'static [&'static str]> {
    static_registry(registry).map(|candidate| candidate.entries)
}

pub fn biome_id(name: &str) -> Option<u16> {
    index_in(SYNCED[BIOME].entries, name).and_then(|index| u16::try_from(index).ok())
}

pub fn biome_name(id: u16) -> Option<&'static str> {
    SYNCED[BIOME].entries.get(usize::from(id)).copied()
}

pub fn dimension_type_id(name: &str) -> Option<u32> {
    index_in(SYNCED[DIMENSION_TYPE].entries, name)
}

fn index_in(entries: &[&str], entry: &str) -> Option<u32> {
    entries
        .binary_search_by(|candidate| (*candidate).cmp(entry))
        .ok()
        .and_then(|index| u32::try_from(index).ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_static_registry_is_listed_with_its_entries() {
        let ids: Vec<&str> = static_registry_ids().collect();
        assert_eq!(ids.len(), 95);
        assert!(ids.contains(&"minecraft:activity"));
        assert!(ids.iter().all(|id| static_entries(id).is_some()));
    }
}

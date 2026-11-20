use std::cell::RefCell;
use std::collections::BTreeMap;
use std::sync::{Arc, RwLock};

pub use crate::generated::tags::TAGS;

pub struct RegistryTags {
    pub registry: &'static str,
    pub tags: &'static [(&'static str, &'static [u32])],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HolderSet {
    Tag(&'static str),
    Direct(&'static [u32]),
}

impl HolderSet {
    pub fn ids(&self, registry: &str) -> &'static [u32] {
        match self {
            Self::Tag(name) => tag(registry, name).unwrap_or(&[]),
            Self::Direct(ids) => ids,
        }
    }

    pub fn contains(&self, registry: &str, id: u32) -> bool {
        self.ids(registry).binary_search(&id).is_ok()
    }
}

#[derive(Debug, Default)]
pub struct TagOverlay {
    registries: BTreeMap<String, BTreeMap<String, &'static [u32]>>,
}

impl TagOverlay {
    pub fn insert(&mut self, registry: &str, name: &str, ids: &'static [u32]) {
        self.registries
            .entry(registry.to_owned())
            .or_default()
            .insert(name.to_owned(), ids);
    }

    pub fn get(&self, registry: &str, name: &str) -> Option<&'static [u32]> {
        self.registries.get(registry)?.get(name).copied()
    }

    pub fn is_empty(&self) -> bool {
        self.registries.values().all(BTreeMap::is_empty)
    }

    pub fn registries(&self) -> impl Iterator<Item = (&str, &BTreeMap<String, &'static [u32]>)> {
        self.registries
            .iter()
            .map(|(registry, tags)| (registry.as_str(), tags))
    }
}

thread_local! {
    static ACTIVE: RefCell<Option<Arc<TagOverlay>>> = const { RefCell::new(None) };
}

static PUBLISHED: RwLock<Option<Arc<TagOverlay>>> = RwLock::new(None);

pub fn activate(overlay: &Arc<TagOverlay>) {
    ACTIVE.with(|active| {
        let mut active = active.borrow_mut();
        let same = match active.as_ref() {
            Some(current) => Arc::ptr_eq(current, overlay),
            None => overlay.is_empty(),
        };
        if !same {
            *active = (!overlay.is_empty()).then(|| Arc::clone(overlay));
        }
    });
}

pub fn publish(overlay: Option<Arc<TagOverlay>>) {
    let mut published = PUBLISHED
        .write()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    *published = overlay;
}

pub fn published() -> Option<Arc<TagOverlay>> {
    PUBLISHED
        .read()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .clone()
}

pub fn tag(registry: &str, name: &str) -> Option<&'static [u32]> {
    let overridden = ACTIVE.with(|active| {
        active
            .borrow()
            .as_ref()
            .and_then(|overlay| overlay.get(registry, name))
    });
    overridden.or_else(|| static_tag(registry, name))
}

pub fn static_tag(registry: &str, name: &str) -> Option<&'static [u32]> {
    let registry = TAGS
        .binary_search_by(|candidate| candidate.registry.cmp(registry))
        .ok()?;
    let tags = TAGS[registry].tags;
    tags.binary_search_by(|(candidate, _)| (*candidate).cmp(name))
        .ok()
        .map(|index| tags[index].1)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registries;

    #[test]
    fn tag_lookup() {
        let stone = registries::static_index("minecraft:block", "minecraft:stone").unwrap();
        let dirt = registries::static_index("minecraft:block", "minecraft:dirt").unwrap();
        let pickaxe = tag("minecraft:block", "minecraft:mineable/pickaxe").unwrap();
        assert!(pickaxe.contains(&stone));
        assert!(tag("minecraft:block", "minecraft:nope").is_none());
        assert!(tag("minecraft:nope", "minecraft:mineable/pickaxe").is_none());
        let set = HolderSet::Tag("minecraft:mineable/pickaxe");
        assert!(set.contains("minecraft:block", stone));
        assert!(!set.contains("minecraft:block", dirt));
        assert!(!HolderSet::Tag("minecraft:nope").contains("minecraft:block", stone));
        assert_eq!(set.ids("minecraft:block"), pickaxe);
        let direct = HolderSet::Direct(&[3, 9]);
        assert!(direct.contains("minecraft:block", 9));
        assert!(!direct.contains("minecraft:block", 4));
        assert_eq!(direct.ids("minecraft:block"), &[3, 9]);
        assert!(
            HolderSet::Tag("minecraft:nope")
                .ids("minecraft:block")
                .is_empty()
        );
    }

    #[test]
    fn an_active_overlay_wins_over_the_static_tag() {
        let stone = registries::static_index("minecraft:block", "minecraft:stone").unwrap();
        let ids: &'static [u32] = Vec::leak(vec![stone]);
        let mut overlay = TagOverlay::default();
        overlay.insert("minecraft:block", "minecraft:mineable/pickaxe", ids);
        overlay.insert("minecraft:block", "t:new", ids);
        let overlay = Arc::new(overlay);
        activate(&overlay);
        assert_eq!(
            tag("minecraft:block", "minecraft:mineable/pickaxe"),
            Some(ids)
        );
        assert_eq!(tag("minecraft:block", "t:new"), Some(ids));
        assert!(HolderSet::Tag("t:new").contains("minecraft:block", stone));
        assert!(tag("minecraft:block", "minecraft:climbable").is_some());
        activate(&Arc::new(TagOverlay::default()));
        assert!(tag("minecraft:block", "t:new").is_none());
        assert!(
            tag("minecraft:block", "minecraft:mineable/pickaxe")
                .unwrap()
                .len()
                > 1
        );
    }

    #[test]
    fn published_tags_are_shared_across_threads() {
        let mut overlay = TagOverlay::default();
        overlay.insert("minecraft:item", "t:shared_probe", &[1]);
        publish(Some(Arc::new(overlay)));
        let seen = std::thread::spawn(|| {
            published().and_then(|o| o.get("minecraft:item", "t:shared_probe"))
        })
        .join()
        .unwrap();
        assert_eq!(seen, Some(&[1][..]));
        publish(None);
        assert!(published().is_none());
    }
}

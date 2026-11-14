use crate::generated::compostable::{COMPOSTABLES, LAYER_PROVIDERS};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Layers {
    pub name: &'static str,
    pub one: u32,
    pub zero: u32,
}

impl Layers {
    pub fn total(&self) -> u32 {
        self.one + self.zero
    }
}

pub fn layers(item: u16) -> Option<Layers> {
    let index = COMPOSTABLES
        .binary_search_by_key(&item, |(candidate, _)| *candidate)
        .ok()?;
    let (name, one, zero) = LAYER_PROVIDERS[usize::from(COMPOSTABLES[index].1)];
    Some(Layers { name, one, zero })
}

pub fn count() -> usize {
    COMPOSTABLES.len()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registries;

    fn item(name: &str) -> u16 {
        registries::static_index("minecraft:item", name).unwrap() as u16
    }

    #[test]
    fn vanilla_item_classes() {
        let wheat_seeds = layers(item("minecraft:wheat_seeds")).unwrap();
        assert_eq!(wheat_seeds.name, "minecraft:compostable/low");
        assert_eq!((wheat_seeds.one, wheat_seeds.zero), (30, 70));
        let apple = layers(item("minecraft:apple")).unwrap();
        assert_eq!((apple.one, apple.zero), (65, 35));
        let bread = layers(item("minecraft:bread")).unwrap();
        assert_eq!((bread.one, bread.zero), (85, 15));
        let cake = layers(item("minecraft:cake")).unwrap();
        assert_eq!((cake.one, cake.zero), (1, 0));
        assert_eq!(cake.total(), 1);
    }

    #[test]
    fn non_compostable_items_have_no_layers() {
        assert!(layers(item("minecraft:stone")).is_none());
        assert!(layers(item("minecraft:bone_meal")).is_none());
    }

    #[test]
    fn table_is_sorted_and_complete() {
        assert_eq!(count(), 122);
        assert!(COMPOSTABLES.windows(2).all(|pair| pair[0].0 < pair[1].0));
    }
}

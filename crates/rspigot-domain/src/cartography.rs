use std::cell::RefCell;
use std::num::NonZeroU8;

use rspigot_items::ItemKind;
use rspigot_nbt::Compound;

use crate::item::{ItemStack, Slot};
use crate::map_data::{MAX_SCALE, MapData};

pub const MAP_ID: &str = "minecraft:map_id";
pub const MAP_POST_PROCESSING: &str = "minecraft:map_post_processing";
pub const SCALE: &str = "scale";
pub const LOCK: &str = "lock";

pub const MAP_SLOT: usize = 0;
pub const MATERIAL_SLOT: usize = 1;
pub const RESULT_SLOT: usize = 2;
pub const INPUTS: usize = 2;

const CLONE_COUNT: u8 = 2;
const MAP_DECORATIONS: &str = "minecraft:map_decorations";
pub const PENDING_EXPLORATION: &str = "rspigot:exploration_map";
const TARGET_KEY: &str = "+";
const TARGET_ROTATION: f32 = 180.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MapFacts {
    pub locked: bool,
    pub scale: u8,
}

impl MapFacts {
    pub fn of(data: &MapData) -> Self {
        Self {
            locked: data.locked,
            scale: data.scale,
        }
    }

    pub fn extendable(self) -> bool {
        !self.locked && self.scale < MAX_SCALE
    }
}

thread_local! {
    static FACTS: RefCell<Vec<(i32, MapFacts)>> = const { RefCell::new(Vec::new()) };
}

pub fn with_facts<R>(facts: Vec<(i32, MapFacts)>, run: impl FnOnce() -> R) -> R {
    let previous = FACTS.with(|held| std::mem::replace(&mut *held.borrow_mut(), facts));
    let outcome = run();
    FACTS.with(|held| *held.borrow_mut() = previous);
    outcome
}

pub fn map_id(stack: &ItemStack) -> Option<i32> {
    stack.components.get_i32(MAP_ID)
}

pub fn facts_of(stack: &ItemStack) -> Option<MapFacts> {
    let id = map_id(stack)?;
    FACTS.with(|held| {
        held.borrow()
            .iter()
            .find(|(known, _)| *known == id)
            .map(|(_, facts)| *facts)
    })
}

pub fn is_map_item(stack: &ItemStack) -> bool {
    stack
        .item
        .kind()
        .is_some_and(|kind| kind.class() == "MapItem")
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Zoom,
    Lock,
    Clone,
}

pub fn action(material: &ItemStack, facts: MapFacts) -> Option<Action> {
    match material.item.kind()? {
        ItemKind::Paper if facts.extendable() => Some(Action::Zoom),
        ItemKind::GlassPane if !facts.locked => Some(Action::Lock),
        ItemKind::Map => Some(Action::Clone),
        _ => None,
    }
}

fn single_with(map: &ItemStack, post_processing: &str) -> Slot {
    let mut result = map.clone();
    result.count = NonZeroU8::MIN;
    result
        .components
        .insert(MAP_POST_PROCESSING, post_processing);
    Some(result)
}

pub fn extended(map: &ItemStack) -> Slot {
    single_with(map, SCALE)
}

pub fn result(map: &Slot, material: &Slot) -> Slot {
    let (map, material) = (map.as_ref()?, material.as_ref()?);
    if !is_map_item(map) {
        return None;
    }
    let facts = facts_of(map)?;
    match action(material, facts)? {
        Action::Zoom => single_with(map, SCALE),
        Action::Lock => single_with(map, LOCK),
        Action::Clone => {
            let mut copies = map.clone();
            copies.count = NonZeroU8::new(CLONE_COUNT)?;
            Some(copies)
        }
    }
}

pub fn pending(stack: &ItemStack) -> Option<&str> {
    if !is_map_item(stack) {
        return None;
    }
    stack.components.get_str(MAP_POST_PROCESSING)
}

pub fn scaled(data: &MapData) -> MapData {
    MapData::fresh(
        f64::from(data.center_x),
        f64::from(data.center_z),
        (data.scale + 1).min(MAX_SCALE),
        data.tracking_position,
        data.unlimited_tracking,
        &data.dimension,
    )
}

pub fn locked_copy(data: &MapData) -> Option<MapData> {
    let mut copy = MapData::from_nbt(&data.to_nbt())?;
    copy.locked = true;
    copy.mark_dirty();
    Some(copy)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExplorationSpec {
    pub destination: String,
    pub decoration: String,
    pub zoom: i32,
    pub search_radius: i32,
    pub skip_existing_chunks: bool,
}

impl ExplorationSpec {
    pub fn pending(&self) -> Compound {
        Compound::new()
            .with("destination", self.destination.as_str())
            .with("decoration", self.decoration.as_str())
            .with("zoom", self.zoom)
            .with("search_radius", self.search_radius)
            .with("skip_existing_chunks", self.skip_existing_chunks)
    }

    fn from_pending(entry: &Compound) -> Option<Self> {
        Some(Self {
            destination: entry.get_str("destination")?.to_owned(),
            decoration: entry.get_str("decoration")?.to_owned(),
            zoom: entry.get_i32("zoom")?,
            search_radius: entry.get_i32("search_radius")?,
            skip_existing_chunks: entry.get_bool("skip_existing_chunks")?,
        })
    }
}

pub trait MapSource {
    fn explore(&mut self, spec: &ExplorationSpec, stack: ItemStack) -> ItemStack;
}

pub struct NoMaps;

impl MapSource for NoMaps {
    fn explore(&mut self, _: &ExplorationSpec, stack: ItemStack) -> ItemStack {
        stack
    }
}

pub fn mark_pending_exploration_in(components: &mut Compound, spec: &ExplorationSpec) {
    components.insert(PENDING_EXPLORATION, spec.pending());
}

pub fn mark_pending_exploration(stack: &mut ItemStack, spec: &ExplorationSpec) {
    mark_pending_exploration_in(&mut stack.components, spec);
}

pub fn has_pending_exploration(stack: &ItemStack) -> bool {
    stack.components.contains_key(PENDING_EXPLORATION)
}

pub fn take_pending_exploration(stack: &mut ItemStack) -> Option<ExplorationSpec> {
    let entry = stack.components.remove(PENDING_EXPLORATION)?;
    ExplorationSpec::from_pending(entry.as_compound()?)
}

pub fn add_target_decoration(stack: &mut ItemStack, kind: &str, x: i32, z: i32) {
    let mut decorations = stack
        .components
        .get_compound(MAP_DECORATIONS)
        .cloned()
        .unwrap_or_default();
    decorations.insert(
        TARGET_KEY,
        Compound::new()
            .with("type", kind)
            .with("x", f64::from(x))
            .with("z", f64::from(z))
            .with("rotation", TARGET_ROTATION),
    );
    stack.components.insert(MAP_DECORATIONS, decorations);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn map(id: i32, count: u8) -> ItemStack {
        let mut stack = ItemStack::new(ItemKind::FilledMap.id(), NonZeroU8::new(count).unwrap());
        stack.components.insert(MAP_ID, id);
        stack
    }

    fn material(kind: ItemKind) -> Slot {
        Some(ItemStack::new(kind.id(), NonZeroU8::MIN))
    }

    fn facts(locked: bool, scale: u8) -> Vec<(i32, MapFacts)> {
        vec![(5, MapFacts { locked, scale })]
    }

    #[test]
    fn paper_zooms_an_unlocked_map_below_the_maximum_scale() {
        let zoomed = with_facts(facts(false, 3), || {
            result(&Some(map(5, 1)), &material(ItemKind::Paper))
        })
        .expect("a result");
        assert_eq!(zoomed.count.get(), 1);
        assert_eq!(zoomed.components.get_str(MAP_POST_PROCESSING), Some(SCALE));
        assert_eq!(zoomed.components.get_i32(MAP_ID), Some(5));
        for (locked, scale) in [(false, 4), (true, 0)] {
            assert_eq!(
                with_facts(facts(locked, scale), || result(
                    &Some(map(5, 1)),
                    &material(ItemKind::Paper)
                )),
                None
            );
        }
    }

    #[test]
    fn a_glass_pane_locks_a_map_that_is_not_locked() {
        let locked = with_facts(facts(false, 4), || {
            result(&Some(map(5, 1)), &material(ItemKind::GlassPane))
        })
        .expect("a result");
        assert_eq!(locked.components.get_str(MAP_POST_PROCESSING), Some(LOCK));
        assert_eq!(
            with_facts(facts(true, 0), || result(
                &Some(map(5, 1)),
                &material(ItemKind::GlassPane)
            )),
            None
        );
    }

    #[test]
    fn an_empty_map_clones_two_copies_even_of_a_locked_map() {
        let copies = with_facts(facts(true, 4), || {
            result(&Some(map(5, 1)), &material(ItemKind::Map))
        })
        .expect("a result");
        assert_eq!(copies.count.get(), 2);
        assert_eq!(copies.components.get_i32(MAP_ID), Some(5));
        assert_eq!(copies.components.get_str(MAP_POST_PROCESSING), None);
    }

    #[test]
    fn a_target_decoration_is_stored_under_the_plus_key_facing_down() {
        let mut stack = map(9, 1);
        add_target_decoration(&mut stack, "minecraft:monument", 120, -48);
        let entries = stack
            .components
            .get_compound(MAP_DECORATIONS)
            .expect("entries");
        let target = entries.get_compound("+").expect("the target");
        assert_eq!(target.get_str("type"), Some("minecraft:monument"));
        assert_eq!(target.get_f64("x"), Some(120.0));
        assert_eq!(target.get_f64("z"), Some(-48.0));
        assert_eq!(target.get_f64("rotation"), Some(180.0));
    }

    #[test]
    fn a_pending_exploration_round_trips_through_the_stack() {
        let spec = ExplorationSpec {
            destination: "minecraft:on_treasure_maps".to_owned(),
            decoration: "minecraft:red_x".to_owned(),
            zoom: 1,
            search_radius: 50,
            skip_existing_chunks: false,
        };
        let mut stack = ItemStack::new(ItemKind::BuriedTreasureMap.id(), NonZeroU8::MIN);
        assert!(!has_pending_exploration(&stack));
        mark_pending_exploration(&mut stack, &spec);
        assert!(has_pending_exploration(&stack));
        assert_eq!(take_pending_exploration(&mut stack), Some(spec));
        assert!(!has_pending_exploration(&stack));
    }

    #[test]
    fn nothing_is_offered_without_facts_or_material() {
        assert_eq!(result(&Some(map(5, 1)), &material(ItemKind::Paper)), None);
        with_facts(facts(false, 0), || {
            assert_eq!(result(&Some(map(5, 1)), &None), None);
            assert_eq!(result(&Some(map(5, 1)), &material(ItemKind::Stone)), None);
            assert_eq!(result(&Some(map(6, 1)), &material(ItemKind::Paper)), None);
        });
    }

    #[test]
    fn a_zoomed_map_is_blank_on_the_next_scale_and_a_locked_copy_keeps_its_pixels() {
        let mut data = MapData::fresh(10.0, 20.0, 1, true, false, "minecraft:overworld");
        data.set_color(3, 4, 44);
        let bigger = scaled(&data);
        assert_eq!(bigger.scale, 2);
        assert!(bigger.colors().iter().all(|c| *c == 0));
        assert!(bigger.tracking_position && !bigger.unlimited_tracking);
        assert_eq!(
            scaled(&MapData::fresh(0.0, 0.0, 4, true, false, "d")).scale,
            4
        );
        let copy = locked_copy(&data).expect("a copy");
        assert!(copy.locked && !data.locked);
        assert_eq!(copy.colors()[3 + 4 * 128], 44);
        assert_eq!(
            (copy.center_x, copy.center_z),
            (data.center_x, data.center_z)
        );
    }
}

mod atlas;
mod common;

use std::sync::{Arc, Mutex};

use atlas::*;
use common::*;
use rspigot_core::events::{ClientEvent, Direction, Hand, PlayerInput};
use rspigot_domain::item::ItemStack;
use rspigot_domain::{BlockPos, EntityId, Vec3};
use rspigot_entities::EntityKind as Kind;
use rspigot_items::ItemKind;

type Asked = Arc<Mutex<Vec<Vec<String>>>>;

const CHEST: BlockPos = BlockPos::new(5, 64, 5);
const DECORATIONS: &str = "minecraft:map_decorations";

fn chest_world(table: &'static str, structure: Option<(&str, Vec3)>) -> (Scene, Arc<Atlas>, Asked) {
    let asked = Arc::new(Mutex::new(Vec::new()));
    let meadow = Meadow {
        structure: structure.map(|(id, at)| (id.to_owned(), at)),
        asked: Arc::clone(&asked),
        loot_chests: vec![(CHEST, table)],
    };
    let (mut s, atlas) = scene_with_atlas(meadow);
    stream_chunk_of(&mut s, CHEST);
    s.players[0].link.take();
    (s, atlas, asked)
}

fn open_chest(s: &mut Scene) -> Window {
    let alice = s.players[0].clone();
    send_input(
        &mut s.server,
        &alice,
        PlayerInput::UseItemOn {
            hand: Hand::Main,
            pos: CHEST,
            face: Direction::Up,
            cursor: Vec3::new(0.5, 1.0, 0.5),
            inside_block: false,
            sequence: 1,
        },
    );
    Window::opened(alice.link.take())
}

fn chest_items(window: &Window) -> Vec<ItemStack> {
    (0..27).filter_map(|slot| window.slot(slot)).collect()
}

fn target_of(stack: &ItemStack) -> (String, f64, f64, f64) {
    let entries = stack
        .components
        .get_compound(DECORATIONS)
        .expect("decorations");
    let target = entries.get_compound("+").expect("the target marker");
    (
        target.get_str("type").expect("type").to_owned(),
        target.get_f64("x").expect("x"),
        target.get_f64("z").expect("z"),
        target.get_f64("rotation").expect("rotation"),
    )
}

#[test]
fn a_treasure_map_in_a_chest_points_at_the_nearest_structure_of_its_destination() {
    let (mut s, atlas, asked) = chest_world(
        "minecraft:chests/shipwreck_map",
        Some(("minecraft:buried_treasure", Vec3::new(800.0, 64.0, -300.0))),
    );
    let window = open_chest(&mut s);

    let maps: Vec<ItemStack> = chest_items(&window)
        .into_iter()
        .filter(|stack| stack.item.kind() == Some(ItemKind::BuriedTreasureMap))
        .collect();
    assert_eq!(maps.len(), 1);
    let map = &maps[0];
    assert_eq!(
        target_of(map),
        ("minecraft:red_x".to_owned(), 800.0, -300.0, 180.0)
    );
    assert_eq!(
        asked.lock().unwrap().as_slice(),
        [vec!["minecraft:buried_treasure".to_owned()]]
    );
    let id = map.components.get_i32(MAP_ID).expect("a map id");
    s.server.save_maps();
    let data = atlas.map(id);
    assert_eq!(data.scale, 1);
    assert_eq!((data.center_x, data.center_z), (832, -192));
    assert!(data.tracking_position && data.unlimited_tracking && !data.locked);
    assert_eq!(data.dimension, OVERWORLD);
}

#[test]
fn a_treasure_map_without_a_structure_in_reach_is_left_out_of_the_chest() {
    for structure in [
        None,
        Some(("minecraft:buried_treasure", Vec3::new(900.0, 64.0, 0.0))),
    ] {
        let (mut s, atlas, asked) = chest_world("minecraft:chests/shipwreck_map", structure);
        let window = open_chest(&mut s);
        assert_eq!(asked.lock().unwrap().len(), 1);
        assert!(
            chest_items(&window)
                .iter()
                .all(|stack| !stack.components.contains_key(MAP_ID))
        );
        assert!(
            chest_items(&window)
                .iter()
                .all(|stack| stack.item.kind() != Some(ItemKind::BuriedTreasureMap))
        );
        assert!(atlas.file("maps/last_id").is_none());
    }
}

const NO_AI: &str = "{NoAI:1b,VillagerDataFinalized:1b,VillagerData:{type:\"minecraft:plains\",profession:\"minecraft:cartographer\",level:5}}";

fn summon_cartographer(s: &mut Scene) -> EntityId {
    console(s, &format!("summon minecraft:villager 9.5 64 8.5 {NO_AI}"));
    s.server
        .entities_of(Kind::Villager)
        .into_iter()
        .map(|entity| entity.id)
        .next()
        .expect("a villager")
}

fn offers_after_interacting(
    s: &mut Scene,
    target: EntityId,
) -> Vec<rspigot_domain::entity::trading::MerchantOffer> {
    let alice = s.players[0].clone();
    send_input(
        &mut s.server,
        &alice,
        PlayerInput::Interact {
            target,
            hand: Hand::Main,
            sneaking: false,
            location: Vec3::new(0.0, 0.5, 0.0),
        },
    );
    alice
        .link
        .take()
        .into_iter()
        .rev()
        .find_map(|event| match event {
            ClientEvent::MerchantOffers { offers, .. } => Some(offers),
            _ => None,
        })
        .expect("the merchant screen")
}

#[test]
fn a_master_cartographer_sells_a_mansion_map_located_when_the_offers_are_generated() {
    let asked = Arc::new(Mutex::new(Vec::new()));
    let meadow = Meadow {
        structure: Some((
            "minecraft:mansion".to_owned(),
            Vec3::new(-640.0, 70.0, 1000.0),
        )),
        asked: Arc::clone(&asked),
        loot_chests: Vec::new(),
    };
    let (mut s, atlas) = scene_with_atlas(meadow);
    stream_chunk_of(&mut s, BlockPos::new(3, 64, 3));
    let villager = summon_cartographer(&mut s);

    let offers = offers_after_interacting(&mut s, villager);

    let map = offers
        .iter()
        .find(|offer| offer.result.item == ItemKind::WoodlandMansionMap.id())
        .expect("the mansion map offer");
    assert_eq!(map.cost_a.item, ItemKind::Emerald.id());
    assert_eq!(map.cost_a.count, 14);
    assert_eq!(
        map.cost_b.as_ref().map(|cost| cost.item),
        Some(ItemKind::Compass.id())
    );
    assert_eq!(
        target_of(&map.result),
        ("minecraft:mansion".to_owned(), -640.0, 1000.0, 180.0)
    );
    assert_eq!(
        asked.lock().unwrap().as_slice(),
        [vec!["minecraft:mansion".to_owned()]]
    );
    let id = map.result.components.get_i32(MAP_ID).expect("a map id");
    s.server.save_maps();
    let data = atlas.map(id);
    assert_eq!(data.scale, 2);
    assert!(data.tracking_position && data.unlimited_tracking);
}

#[test]
fn a_cartographer_whose_structure_cannot_be_found_drops_the_map_offer() {
    let (mut s, _atlas) = scene_with_atlas(Meadow::default());
    stream_chunk_of(&mut s, BlockPos::new(3, 64, 3));
    let villager = summon_cartographer(&mut s);

    let offers = offers_after_interacting(&mut s, villager);

    assert!(
        offers
            .iter()
            .all(|offer| offer.result.item != ItemKind::WoodlandMansionMap.id())
    );
    assert_eq!(offers.len(), 1);
}

#[test]
fn a_carried_exploration_map_is_sent_to_its_holder_like_a_filled_map() {
    let (mut s, atlas) = scene_with_atlas(Meadow::default());
    let mut stored =
        rspigot_domain::map_data::MapData::fresh(800.0, -300.0, 1, true, true, OVERWORLD);
    stored.set_color(2, 2, 8);
    atlas.put_map(4, &stored);
    let mut stack = filled_stack(4);
    stack.item = ItemKind::BuriedTreasureMap.id();
    put_in_slot(&mut s, 9, stack);
    s.players[0].link.take();

    s.tick(20);

    let sent: Vec<_> = s.players[0]
        .link
        .take()
        .into_iter()
        .filter_map(|event| match event {
            ClientEvent::MapData(update) => Some(update),
            _ => None,
        })
        .collect();
    assert!(sent.iter().any(|update| update.map_id == 4));
}

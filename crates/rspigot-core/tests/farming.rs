mod common;

use common::*;
use rspigot_core::events::{ClientEvent, Direction, Hand, Inbound, PlayerInput};
use rspigot_core::plugins::{Answer, BlockOrder, EventKind, EventPayload, Order, Question};
use rspigot_domain::{BlockPos, Vec3};
use rspigot_entities::EntityKind as Kind;

const SPOT: BlockPos = BlockPos::new(2, 64, 2);

fn console(s: &mut Scene, line: &str) {
    s.server.handle(Inbound::Console(line.to_owned()));
    s.server.deliver();
}

fn ready() -> Scene {
    let mut s = scene(&["Alice"]);
    stream_chunk_of(&mut s, BlockPos::new(1, 64, 1));
    s
}

fn set(s: &mut Scene, pos: BlockPos, kind: &str, properties: &[(&str, &str)]) {
    s.act(Order::Block {
        pos,
        order: BlockOrder::SetWith {
            kind: kind.to_owned(),
            properties: properties
                .iter()
                .map(|(name, value)| ((*name).to_owned(), (*value).to_owned()))
                .collect(),
        },
    });
}

fn property(s: &mut Scene, pos: BlockPos, name: &str) -> Option<String> {
    match s.ask(Question::Block(pos)) {
        Answer::Block(state) => state
            .properties
            .into_iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value),
        _ => None,
    }
}

fn use_on(s: &mut Scene, pos: BlockPos) {
    let alice = s.players[0].clone();
    send_input(
        &mut s.server,
        &alice,
        PlayerInput::UseItemOn {
            hand: Hand::Main,
            pos,
            face: Direction::Up,
            cursor: Vec3::new(0.5, 1.0, 0.5),
            inside_block: false,
            sequence: 3,
        },
    );
    s.tick(1);
}

fn events(s: &Scene) -> Vec<ClientEvent> {
    s.players[0].link.take()
}

fn sounds(events: &[ClientEvent]) -> Vec<&'static str> {
    events
        .iter()
        .filter_map(|event| match event {
            ClientEvent::Sound { sound, .. } => Some(*sound),
            _ => None,
        })
        .collect()
}

fn level_events(events: &[ClientEvent]) -> Vec<(i32, i32)> {
    events
        .iter()
        .filter_map(|event| match event {
            ClientEvent::LevelEvent { event, data, .. } => Some((*event, *data)),
            _ => None,
        })
        .collect()
}

fn crop(s: &mut Scene, age: &str) {
    set(s, SPOT.offset(0, -1, 0), "minecraft:farmland", &[]);
    set(s, SPOT, "minecraft:wheat", &[("age", age)]);
}

fn age(s: &mut Scene) -> i32 {
    property(s, SPOT, "age").unwrap().parse().unwrap()
}

#[test]
fn bone_meal_grows_young_wheat_by_two_to_five_stages_with_particles() {
    let mut s = ready();
    crop(&mut s, "0");
    console(&mut s, "give Alice minecraft:bone_meal 4");
    events(&s);
    use_on(&mut s, SPOT);
    let grown = age(&mut s);
    assert!((2..=5).contains(&grown), "{grown}");
    assert!(level_events(&events(&s)).contains(&(1505, 15)));
}

#[test]
fn bone_meal_is_not_used_on_mature_wheat() {
    let mut s = ready();
    crop(&mut s, "7");
    console(&mut s, "give Alice minecraft:bone_meal 4");
    events(&s);
    use_on(&mut s, SPOT);
    assert_eq!(age(&mut s), 7);
    assert!(
        !level_events(&events(&s))
            .iter()
            .any(|(event, _)| *event == 1505)
    );
}

#[test]
fn a_plugin_can_cancel_the_crop_growth_of_a_bone_meal() {
    let mut s = scene_with(&["Alice"], vec![deciding(1, EventKind::BlockGrow)]);
    stream_chunk_of(&mut s, BlockPos::new(1, 64, 1));
    crop(&mut s, "0");
    console(&mut s, "give Alice minecraft:bone_meal 4");
    use_on(&mut s, SPOT);
    let asked = last_asked(&s.plugin, EventKind::BlockGrow);
    let EventPayload::BlockGrow(grow) = &asked.payload else {
        panic!("a block grow");
    };
    assert_eq!(grow.block, SPOT);
    assert_eq!(age(&mut s), 0);
    rule_on(&mut s.server, 1, asked.id, true, None);
    assert_eq!(age(&mut s), 0);
}

#[test]
fn seeds_added_to_an_empty_composter_always_fill_one_layer() {
    let mut s = ready();
    set(&mut s, SPOT, "minecraft:composter", &[("level", "0")]);
    console(&mut s, "give Alice minecraft:wheat_seeds 8");
    events(&s);
    use_on(&mut s, SPOT);
    assert_eq!(property(&mut s, SPOT, "level").as_deref(), Some("1"));
    assert!(level_events(&events(&s)).contains(&(1500, 1)));
}

#[test]
fn a_full_composter_turns_ready_after_twenty_ticks_and_gives_bone_meal() {
    let mut s = ready();
    set(&mut s, SPOT, "minecraft:composter", &[("level", "7")]);
    events(&s);
    s.tick(10);
    assert_eq!(property(&mut s, SPOT, "level").as_deref(), Some("7"));
    s.tick(15);
    assert_eq!(property(&mut s, SPOT, "level").as_deref(), Some("8"));
    assert!(sounds(&events(&s)).contains(&"minecraft:block.composter.ready"));
    use_on(&mut s, SPOT);
    assert_eq!(property(&mut s, SPOT, "level").as_deref(), Some("0"));
    assert!(sounds(&events(&s)).contains(&"minecraft:block.composter.empty"));
    let items = s.server.entities_of(Kind::Item);
    assert_eq!(items.len(), 1);
    let at = items[0].position;
    assert!(at.y > 64.9 && at.y < 66.0, "{at:?}");
    assert!(
        (at.x - 2.5).abs() <= 1.0 && (at.z - 2.5).abs() <= 1.0,
        "{at:?}"
    );
}

#[test]
fn a_composter_filled_by_the_seventh_layer_schedules_its_ready_tick() {
    let mut s = ready();
    set(&mut s, SPOT, "minecraft:composter", &[("level", "6")]);
    console(&mut s, "give Alice minecraft:wheat 8");
    let mut ready_at = None;
    for _ in 0..60 {
        use_on(&mut s, SPOT);
        if property(&mut s, SPOT, "level").as_deref() == Some("7") {
            ready_at = Some(());
            break;
        }
    }
    assert!(ready_at.is_some());
    s.tick(25);
    assert_eq!(property(&mut s, SPOT, "level").as_deref(), Some("8"));
}

const FARMER: &str = "{VillagerData:{type:\"minecraft:plains\",profession:\"minecraft:farmer\",level:1},Inventory:[{id:\"minecraft:wheat_seeds\",count:30}],Brain:{memories:{\"minecraft:job_site\":{value:{dimension:\"minecraft:overworld\",pos:[I;3,64,3]}}}}}";

fn workday() -> Scene {
    let server = configured_level(
        |_| {},
        |level| {
            level.day_time = 4_000;
            level.daylight_cycle = false;
        },
    )
    .with_rng_seed(11);
    let mut s = scene_on(server, &["Alice"], Vec::new());
    stream_chunk_of(&mut s, BlockPos::new(1, 64, 1));
    s
}

fn summon_farmer(s: &mut Scene) -> rspigot_domain::EntityId {
    console(s, &format!("summon minecraft:villager 2.5 64 3.5 {FARMER}"));
    s.server
        .entities_of(Kind::Villager)
        .into_iter()
        .map(|entity| entity.id)
        .next_back()
        .expect("a villager")
}

fn seeds_of(s: &Scene, id: rspigot_domain::EntityId) -> i32 {
    let saved = s.server.saved_entity(id).expect("a saved villager");
    let Some(list) = saved.get_list("Inventory") else {
        return 0;
    };
    list.iter()
        .filter_map(rspigot_nbt::Tag::as_compound)
        .filter(|item| item.get_str("id") == Some("minecraft:wheat_seeds"))
        .filter_map(|item| item.get_i32("count"))
        .sum()
}

#[test]
fn a_farmer_at_its_composter_composts_surplus_seeds_into_the_composter() {
    let mut s = workday();
    set(
        &mut s,
        BlockPos::new(3, 64, 3),
        "minecraft:composter",
        &[("level", "0")],
    );
    let farmer = summon_farmer(&mut s);
    assert_eq!(seeds_of(&s, farmer), 30);
    let mut level = 0;
    for _ in 0..40 {
        s.tick(50);
        level = property(&mut s, BlockPos::new(3, 64, 3), "level")
            .unwrap()
            .parse()
            .unwrap();
        if level > 0 {
            break;
        }
    }
    assert!(level > 0, "the composter stayed empty");
    assert!(seeds_of(&s, farmer) < 30);
    assert!(
        seeds_of(&s, farmer) >= 10,
        "ten seeds are kept for planting"
    );
}

const BONE_MEAL_FARMER: &str = "{VillagerData:{type:\"minecraft:plains\",profession:\"minecraft:farmer\",level:1},Inventory:[{id:\"minecraft:bone_meal\",count:5}],Brain:{memories:{\"minecraft:job_site\":{value:{dimension:\"minecraft:overworld\",pos:[I;3,64,3]}}}}}";

fn bone_meals_of(s: &Scene, id: rspigot_domain::EntityId) -> i32 {
    let saved = s.server.saved_entity(id).expect("a saved villager");
    let Some(list) = saved.get_list("Inventory") else {
        return 0;
    };
    list.iter()
        .filter_map(rspigot_nbt::Tag::as_compound)
        .filter(|item| item.get_str("id") == Some("minecraft:bone_meal"))
        .filter_map(|item| item.get_i32("count"))
        .sum()
}

#[test]
fn a_farmer_with_bone_meal_grows_the_young_crops_around_it() {
    let mut s = workday();
    set(
        &mut s,
        BlockPos::new(3, 64, 3),
        "minecraft:composter",
        &[("level", "0")],
    );
    let mut crops = Vec::new();
    for x in 0..=6 {
        for z in 0..=6 {
            if (x, z) != (3, 3) {
                set(&mut s, BlockPos::new(x, 63, z), "minecraft:farmland", &[]);
                set(
                    &mut s,
                    BlockPos::new(x, 64, z),
                    "minecraft:wheat",
                    &[("age", "0")],
                );
                crops.push(BlockPos::new(x, 64, z));
            }
        }
    }
    console(
        &mut s,
        &format!("summon minecraft:villager 2.5 64 2.5 {BONE_MEAL_FARMER}"),
    );
    let farmer = s
        .server
        .entities_of(Kind::Villager)
        .into_iter()
        .map(|entity| entity.id)
        .next_back()
        .expect("a villager");
    let mut grown = false;
    for _ in 0..80 {
        s.tick(50);
        for crop in &crops {
            if property(&mut s, *crop, "age").is_none_or(|age| age != "0") {
                grown = true;
            }
        }
        if grown {
            break;
        }
    }
    assert!(grown, "the wheat never grew");
    assert!(bone_meals_of(&s, farmer) < 5);
}

#[test]
fn a_plugin_is_asked_before_a_farmer_adds_seeds_to_the_composter_and_can_refuse() {
    let server = configured_level(
        |_| {},
        |level| {
            level.day_time = 4_000;
            level.daylight_cycle = false;
        },
    )
    .with_rng_seed(11);
    let mut s = scene_on(
        server,
        &["Alice"],
        vec![deciding(1, EventKind::EntityChangeBlock)],
    );
    stream_chunk_of(&mut s, BlockPos::new(1, 64, 1));
    let composter = BlockPos::new(3, 64, 3);
    set(&mut s, composter, "minecraft:composter", &[("level", "0")]);
    let farmer = summon_farmer(&mut s);
    for _ in 0..200 {
        s.tick(10);
        if !asked_for(&s.plugin, EventKind::EntityChangeBlock).is_empty() {
            break;
        }
    }
    let asked = last_asked(&s.plugin, EventKind::EntityChangeBlock);
    let EventPayload::EntityChangeBlock(change) = &asked.payload else {
        panic!("an entity change block");
    };
    assert_eq!(change.block, composter);
    assert_eq!(change.to, "minecraft:composter");
    assert_eq!(property(&mut s, composter, "level").as_deref(), Some("0"));
    rule_on(&mut s.server, 1, asked.id, true, None);
    assert_eq!(property(&mut s, composter, "level").as_deref(), Some("0"));
    assert_eq!(seeds_of(&s, farmer), 30);
    for _ in 0..20 {
        s.tick(1);
        if last_asked(&s.plugin, EventKind::EntityChangeBlock).id != asked.id {
            break;
        }
    }
    let again = last_asked(&s.plugin, EventKind::EntityChangeBlock);
    assert_ne!(again.id, asked.id);
    rule_on(&mut s.server, 1, again.id, false, None);
    assert_eq!(property(&mut s, composter, "level").as_deref(), Some("1"));
    assert_eq!(seeds_of(&s, farmer), 29);
}

#[test]
fn a_farmer_empties_a_ready_composter_into_a_bone_meal_item_before_composting() {
    let mut s = workday();
    let composter = BlockPos::new(3, 64, 3);
    set(&mut s, composter, "minecraft:composter", &[("level", "8")]);
    summon_farmer(&mut s);
    let mut emptied = false;
    for _ in 0..40 {
        s.tick(50);
        if property(&mut s, composter, "level").as_deref() != Some("8") {
            emptied = true;
            break;
        }
    }
    assert!(emptied, "the composter was never emptied");
    let items = s.server.entities_of(Kind::Item);
    assert_eq!(items.len(), 1);
}

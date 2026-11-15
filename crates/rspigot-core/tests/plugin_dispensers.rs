mod common;
#[path = "common/screen.rs"]
mod screen;
#[path = "common/seeded.rs"]
mod seeded;

use common::*;
use rspigot_blocks::BlockKind;
use rspigot_core::plugins::events::{BlockDispensePatch, InventoryMoveItemPatch};
use rspigot_core::plugins::{
    Answer, BlockOrder, EventKind, EventPatch, EventPayload, InventoryOwner, ItemSpec, Order,
    Question,
};
use rspigot_domain::{BlockPos, Vec3};
use seeded::{Seed, contents, world};

const DISPENSER: BlockPos = BlockPos::new(3, 64, 3);
const FRONT: BlockPos = BlockPos::new(4, 64, 3);
const POWER: BlockPos = BlockPos::new(3, 64, 2);

fn dispenser(kind: BlockKind, items: &[(&'static str, u8)]) -> Seed {
    let mut seed = Seed::block(DISPENSER, kind).with("facing", "east");
    for (key, count) in items {
        seed = seed.holding(key, *count);
    }
    seed
}

fn put(s: &mut Scene, pos: BlockPos, kind: &str) {
    s.act(Order::Block {
        pos,
        order: BlockOrder::Set(kind.to_owned()),
    });
}

fn kind_at(s: &mut Scene, pos: BlockPos) -> String {
    match s.ask(Question::Block(pos)) {
        Answer::Block(state) => state.kind,
        other => panic!("expected a block, got {other:?}"),
    }
}

fn count_of(s: &mut Scene, kind: &str) -> usize {
    match s.ask(Question::Entities {
        near: None,
        kind: Some(kind.to_owned()),
    }) {
        Answer::Entities(entities) => entities.len(),
        other => panic!("expected entities, got {other:?}"),
    }
}

fn power(s: &mut Scene) {
    put(s, POWER, "minecraft:redstone_block");
}

#[test]
fn a_powered_dispenser_drops_one_item_four_ticks_later() {
    let mut s = world(
        vec![dispenser(BlockKind::Dispenser, &[("minecraft:stone", 3)])],
        Vec::new(),
    );
    let alice = s.players[0].clone();
    power(&mut s);
    s.tick(3);
    assert_eq!(s.server.ground_items().len(), 0);
    s.tick(2);
    let ground = s.server.ground_items();
    assert_eq!(ground.len(), 1);
    assert_eq!(ground[0].1.count.get(), 1);
    assert_eq!(
        contents(&mut s, &alice, DISPENSER, 1),
        [Some(("minecraft:stone", 2))]
    );
}

#[test]
fn an_empty_dispenser_drops_nothing() {
    let mut s = world(vec![dispenser(BlockKind::Dispenser, &[])], Vec::new());
    power(&mut s);
    s.tick(6);
    assert_eq!(s.server.ground_items().len(), 0);
}

#[test]
fn a_dispense_event_carries_the_item_and_the_launch_direction() {
    let mut s = world(
        vec![dispenser(BlockKind::Dispenser, &[("minecraft:stone", 3)])],
        vec![observe(1, EventKind::BlockDispense)],
    );
    power(&mut s);
    s.tick(5);
    let heard = settled_for(&s.plugin, EventKind::BlockDispense);
    let EventPayload::BlockDispense(event) = &heard.first().expect("a dispense").payload else {
        panic!("a dispense event");
    };
    assert_eq!(event.block, DISPENSER);
    assert_eq!(
        (event.item.key.as_str(), event.item.count),
        ("minecraft:stone", 1)
    );
    assert!(event.velocity.x > 0.0);
}

#[test]
fn a_cancelled_dispense_keeps_the_item() {
    let mut s = world(
        vec![dispenser(BlockKind::Dispenser, &[("minecraft:stone", 3)])],
        vec![deciding(1, EventKind::BlockDispense)],
    );
    let alice = s.players[0].clone();
    power(&mut s);
    s.tick(5);
    let asked = last_asked(&s.plugin, EventKind::BlockDispense);
    rule_on(&mut s.server, 1, asked.id, true, None);
    assert_eq!(s.server.ground_items().len(), 0);
    assert_eq!(
        contents(&mut s, &alice, DISPENSER, 1),
        [Some(("minecraft:stone", 3))]
    );
}

#[test]
fn a_patched_item_is_what_the_dispenser_drops() {
    let mut s = world(
        vec![dispenser(BlockKind::Dispenser, &[("minecraft:stone", 3)])],
        vec![deciding(1, EventKind::BlockDispense)],
    );
    let alice = s.players[0].clone();
    power(&mut s);
    s.tick(5);
    let asked = last_asked(&s.plugin, EventKind::BlockDispense);
    rule_on(
        &mut s.server,
        1,
        asked.id,
        false,
        Some(EventPatch::BlockDispense(BlockDispensePatch {
            item: Some(ItemSpec::of("minecraft:diamond", 1)),
            velocity: None,
        })),
    );
    let ground = s.server.ground_items();
    assert_eq!(ground.len(), 1);
    assert_eq!(
        ground[0].1.item.kind().map(|kind| kind.name()),
        Some("minecraft:diamond")
    );
    assert_eq!(
        contents(&mut s, &alice, DISPENSER, 1),
        [Some(("minecraft:stone", 2))]
    );
}

#[test]
fn a_dropper_pushes_the_item_into_the_container_in_front_of_it() {
    let seeds = vec![
        dispenser(BlockKind::Dropper, &[("minecraft:stone", 3)]),
        Seed::block(FRONT, BlockKind::Chest),
    ];
    let mut s = world(seeds, vec![observe(1, EventKind::InventoryMoveItem)]);
    let alice = s.players[0].clone();
    power(&mut s);
    s.tick(5);
    assert_eq!(
        contents(&mut s, &alice, FRONT, 1),
        [Some(("minecraft:stone", 1))]
    );
    assert_eq!(
        contents(&mut s, &alice, DISPENSER, 1),
        [Some(("minecraft:stone", 2))]
    );
    let heard = settled_for(&s.plugin, EventKind::InventoryMoveItem);
    let EventPayload::InventoryMoveItem(event) = &heard.first().expect("a move").payload else {
        panic!("a move event");
    };
    assert_eq!(event.initiator.owner, InventoryOwner::Block(DISPENSER));
    assert_eq!(event.initiator.kind, "DROPPER");
    assert_eq!(event.destination.owner, InventoryOwner::Block(FRONT));
}

#[test]
fn a_dropper_move_can_be_cancelled_or_patched() {
    let seeds = vec![
        dispenser(BlockKind::Dropper, &[("minecraft:stone", 3)]),
        Seed::block(FRONT, BlockKind::Chest),
    ];
    let mut s = world(seeds, vec![deciding(1, EventKind::InventoryMoveItem)]);
    let alice = s.players[0].clone();
    power(&mut s);
    s.tick(5);
    let asked = last_asked(&s.plugin, EventKind::InventoryMoveItem);
    rule_on(
        &mut s.server,
        1,
        asked.id,
        false,
        Some(EventPatch::InventoryMoveItem(InventoryMoveItemPatch {
            item: Some(ItemSpec::of("minecraft:diamond", 1)),
        })),
    );
    assert_eq!(
        contents(&mut s, &alice, FRONT, 1),
        [Some(("minecraft:diamond", 1))]
    );
}

#[test]
fn a_dropper_with_nothing_in_front_drops_the_item() {
    let mut s = world(
        vec![dispenser(BlockKind::Dropper, &[("minecraft:stone", 3)])],
        Vec::new(),
    );
    power(&mut s);
    s.tick(5);
    assert_eq!(s.server.ground_items().len(), 1);
}

#[test]
fn a_dispenser_fires_arrows_and_throws_eggs() {
    let mut s = world(
        vec![dispenser(
            BlockKind::Dispenser,
            &[("minecraft:arrow", 2), ("minecraft:snowball", 2)],
        )],
        Vec::new(),
    );
    power(&mut s);
    s.tick(5);
    let shots = count_of(&mut s, "minecraft:arrow") + count_of(&mut s, "minecraft:snowball");
    assert_eq!(shots, 1);
}

#[test]
fn a_dispenser_primes_a_tnt_item_in_front_of_it() {
    let mut s = world(
        vec![dispenser(BlockKind::Dispenser, &[("minecraft:tnt", 1)])],
        Vec::new(),
    );
    power(&mut s);
    s.tick(5);
    assert_eq!(count_of(&mut s, "minecraft:tnt"), 1);
}

#[test]
fn a_dispenser_empties_a_water_bucket_and_keeps_the_empty_one() {
    let mut s = world(
        vec![dispenser(
            BlockKind::Dispenser,
            &[("minecraft:water_bucket", 1)],
        )],
        Vec::new(),
    );
    let alice = s.players[0].clone();
    power(&mut s);
    s.tick(5);
    assert_eq!(kind_at(&mut s, FRONT), "minecraft:water");
    assert_eq!(
        contents(&mut s, &alice, DISPENSER, 1),
        [Some(("minecraft:bucket", 1))]
    );
}

#[test]
fn a_dispenser_scoops_up_a_water_source_with_an_empty_bucket() {
    let mut s = world(
        vec![dispenser(BlockKind::Dispenser, &[("minecraft:bucket", 1)])],
        Vec::new(),
    );
    let alice = s.players[0].clone();
    put(&mut s, FRONT, "minecraft:water");
    power(&mut s);
    s.tick(5);
    assert_eq!(kind_at(&mut s, FRONT), "minecraft:air");
    assert_eq!(
        contents(&mut s, &alice, DISPENSER, 1),
        [Some(("minecraft:water_bucket", 1))]
    );
}

fn spawn(s: &mut Scene, kind: &str) {
    let answer = s.ask(Question::Spawn {
        kind: kind.to_owned(),
        position: Vec3::new(4.5, 64.0, 3.5),
    });
    assert!(matches!(answer, Answer::Spawned(_)));
}

#[test]
fn a_dispenser_puts_armor_on_a_mob_in_front_of_it() {
    let mut s = world(
        vec![dispenser(
            BlockKind::Dispenser,
            &[("minecraft:iron_helmet", 1)],
        )],
        vec![observe(1, EventKind::BlockDispenseArmor)],
    );
    let alice = s.players[0].clone();
    spawn(&mut s, "minecraft:cow");
    power(&mut s);
    s.tick(5);
    let heard = settled_for(&s.plugin, EventKind::BlockDispenseArmor);
    let EventPayload::BlockDispenseArmor(event) = &heard.first().expect("an armor event").payload
    else {
        panic!("an armor event");
    };
    assert_eq!(event.target_entity.kind, "minecraft:cow");
    assert_eq!(event.item.key, "minecraft:iron_helmet");
    assert_eq!(contents(&mut s, &alice, DISPENSER, 1), [None]);
    assert_eq!(s.server.ground_items().len(), 0);
}

#[test]
fn a_cancelled_armor_dispense_leaves_the_helmet_in_the_dispenser() {
    let mut s = world(
        vec![dispenser(
            BlockKind::Dispenser,
            &[("minecraft:iron_helmet", 1)],
        )],
        vec![deciding(1, EventKind::BlockDispenseArmor)],
    );
    let alice = s.players[0].clone();
    spawn(&mut s, "minecraft:cow");
    power(&mut s);
    s.tick(5);
    let asked = last_asked(&s.plugin, EventKind::BlockDispenseArmor);
    rule_on(&mut s.server, 1, asked.id, true, None);
    assert_eq!(
        contents(&mut s, &alice, DISPENSER, 1),
        [Some(("minecraft:iron_helmet", 1))]
    );
}

#[test]
fn a_dispenser_shears_a_sheep_in_front_of_it() {
    let mut s = world(
        vec![dispenser(BlockKind::Dispenser, &[("minecraft:shears", 1)])],
        vec![observe(1, EventKind::BlockShearEntity)],
    );
    spawn(&mut s, "minecraft:sheep");
    power(&mut s);
    s.tick(5);
    let heard = settled_for(&s.plugin, EventKind::BlockShearEntity);
    let EventPayload::BlockShearEntity(event) = &heard.first().expect("a shear event").payload
    else {
        panic!("a shear event");
    };
    assert_eq!(event.block, DISPENSER);
    assert_eq!(event.entity.kind, "minecraft:sheep");
    assert_eq!(event.tool.key, "minecraft:shears");
    assert!(!s.server.ground_items().is_empty());
}

#[test]
fn a_cancelled_shear_keeps_the_wool() {
    let mut s = world(
        vec![dispenser(BlockKind::Dispenser, &[("minecraft:shears", 1)])],
        vec![deciding(1, EventKind::BlockShearEntity)],
    );
    spawn(&mut s, "minecraft:sheep");
    power(&mut s);
    s.tick(5);
    let asked = last_asked(&s.plugin, EventKind::BlockShearEntity);
    rule_on(&mut s.server, 1, asked.id, true, None);
    assert!(s.server.ground_items().is_empty());
}

#[test]
fn a_dispenser_lights_a_tnt_block_with_flint_and_steel() {
    let mut s = world(
        vec![dispenser(
            BlockKind::Dispenser,
            &[("minecraft:flint_and_steel", 1)],
        )],
        vec![observe(1, EventKind::TNTPrime)],
    );
    put(&mut s, FRONT, "minecraft:tnt");
    power(&mut s);
    s.tick(5);
    assert_eq!(kind_at(&mut s, FRONT), "minecraft:air");
    assert_eq!(count_of(&mut s, "minecraft:tnt"), 1);
    let heard = settled_for(&s.plugin, EventKind::TNTPrime);
    assert!(heard.iter().any(|fired| matches!(
        &fired.payload,
        EventPayload::TNTPrime(event)
            if event.cause == rspigot_core::plugins::events::TNTPrimeCause::Dispenser
    )));
}

fn kinded(
    s: &Scene,
    kind: rspigot_entities::EntityKind,
) -> Vec<rspigot_core::entities::EntitySnapshot> {
    s.server.entities_of(kind)
}

#[test]
fn a_dispenser_places_a_minecart_on_the_rail_in_front_of_it() {
    let mut s = world(
        vec![
            dispenser(BlockKind::Dispenser, &[("minecraft:minecart", 2)]),
            Seed::block(FRONT, BlockKind::Rail).with("shape", "north_south"),
        ],
        Vec::new(),
    );
    power(&mut s);
    s.tick(5);
    let carts = kinded(&s, rspigot_entities::EntityKind::Minecart);
    assert_eq!(carts.len(), 1);
    assert!((carts[0].position.x - 4.625).abs() < 0.3);
    assert!((carts[0].position.y - 64.1).abs() < 0.3);
    assert_eq!(s.server.ground_items().len(), 0);
}

#[test]
fn a_dispenser_drops_a_minecart_when_there_is_no_rail() {
    let mut s = world(
        vec![dispenser(
            BlockKind::Dispenser,
            &[("minecraft:minecart", 1)],
        )],
        Vec::new(),
    );
    power(&mut s);
    s.tick(5);
    assert!(kinded(&s, rspigot_entities::EntityKind::Minecart).is_empty());
    assert_eq!(s.server.ground_items().len(), 1);
}

#[test]
fn a_dispenser_launches_a_boat_on_the_water_in_front_of_it() {
    let mut s = world(
        vec![
            dispenser(BlockKind::Dispenser, &[("minecraft:oak_boat", 1)]),
            Seed::block(FRONT, BlockKind::Water),
        ],
        Vec::new(),
    );
    power(&mut s);
    s.tick(5);
    assert_eq!(kinded(&s, rspigot_entities::EntityKind::OakBoat).len(), 1);
}

#[test]
fn a_dispenser_stands_an_armor_stand_in_front_of_it() {
    let mut s = world(
        vec![dispenser(
            BlockKind::Dispenser,
            &[("minecraft:armor_stand", 1)],
        )],
        Vec::new(),
    );
    power(&mut s);
    s.tick(5);
    let stands = kinded(&s, rspigot_entities::EntityKind::ArmorStand);
    assert_eq!(stands.len(), 1);
    assert!((stands[0].position.x - 4.5).abs() < 1e-6);
}

#[test]
fn a_dispenser_hatches_the_mob_of_a_spawn_egg() {
    let mut s = world(
        vec![dispenser(
            BlockKind::Dispenser,
            &[("minecraft:pig_spawn_egg", 1)],
        )],
        Vec::new(),
    );
    power(&mut s);
    s.tick(5);
    assert_eq!(kinded(&s, rspigot_entities::EntityKind::Pig).len(), 1);
}

#[test]
fn a_dispenser_turns_dirt_into_mud_with_a_water_bottle() {
    let mut s = world(
        vec![
            Seed::block(DISPENSER, BlockKind::Dispenser)
                .with("facing", "east")
                .carrying(
                    "Items",
                    rspigot_nbt::Tag::List(vec![rspigot_nbt::Tag::Compound(
                        rspigot_nbt::Compound::new()
                            .with("Slot", 0_i8)
                            .with("id", "minecraft:potion")
                            .with("count", 1_i32)
                            .with(
                                "components",
                                rspigot_nbt::Compound::new().with(
                                    "minecraft:potion_contents",
                                    rspigot_nbt::Compound::new().with("potion", "minecraft:water"),
                                ),
                            ),
                    )]),
                ),
            Seed::block(FRONT, BlockKind::Dirt),
        ],
        Vec::new(),
    );
    power(&mut s);
    s.tick(5);
    assert_eq!(kind_at(&mut s, FRONT), "minecraft:mud");
    let alice = s.players[0].clone();
    assert_eq!(
        contents(&mut s, &alice, DISPENSER, 1),
        [Some(("minecraft:glass_bottle", 1))]
    );
}

#[test]
fn a_dispenser_fills_a_glass_bottle_from_a_water_block() {
    let mut s = world(
        vec![
            dispenser(BlockKind::Dispenser, &[("minecraft:glass_bottle", 1)]),
            Seed::block(FRONT, BlockKind::Water),
        ],
        Vec::new(),
    );
    power(&mut s);
    s.tick(5);
    let alice = s.players[0].clone();
    let held = contents(&mut s, &alice, DISPENSER, 1);
    assert_eq!(held, [Some(("minecraft:potion", 1))]);
}

#[test]
fn a_dispenser_places_powder_snow_and_keeps_the_empty_bucket() {
    let mut s = world(
        vec![dispenser(
            BlockKind::Dispenser,
            &[("minecraft:powder_snow_bucket", 1)],
        )],
        Vec::new(),
    );
    power(&mut s);
    s.tick(5);
    assert_eq!(kind_at(&mut s, FRONT), "minecraft:powder_snow");
    let alice = s.players[0].clone();
    assert_eq!(
        contents(&mut s, &alice, DISPENSER, 1),
        [Some(("minecraft:bucket", 1))]
    );
}

#[test]
fn a_dispenser_takes_honey_from_a_full_hive_and_resets_it() {
    let mut s = world(
        vec![
            dispenser(BlockKind::Dispenser, &[("minecraft:glass_bottle", 1)]),
            Seed::block(FRONT, BlockKind::BeeNest).with("honey_level", "5"),
        ],
        Vec::new(),
    );
    power(&mut s);
    s.tick(5);
    let alice = s.players[0].clone();
    assert_eq!(
        contents(&mut s, &alice, DISPENSER, 1),
        [Some(("minecraft:honey_bottle", 1))]
    );
    match s.ask(Question::Block(FRONT)) {
        Answer::Block(state) => assert!(
            state
                .properties
                .iter()
                .any(|(key, value)| key == "honey_level" && value == "0")
        ),
        other => panic!("expected a block, got {other:?}"),
    }
}

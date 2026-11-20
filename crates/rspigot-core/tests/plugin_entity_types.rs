mod common;

use common::*;
use rspigot_core::plugins::{Answer, EntityOrder, EntityTrait, Order, Question};
use rspigot_domain::{EntityId, Vec3};

const AT: Vec3 = Vec3::new(4.5, 64.0, 4.5);

fn arena() -> Scene {
    let mut s = scene(&["Alice"]);
    stream_chunk_of(&mut s, AT.block());
    s
}

fn spawn(s: &mut Scene, kind: &str) -> EntityId {
    let at = AT;
    let Answer::Spawned(view) = s.ask(Question::Spawn {
        kind: kind.into(),
        position: at,
    }) else {
        panic!("expected the spawned {kind}");
    };
    view.id
}

fn change(s: &mut Scene, entity: EntityId, change: EntityTrait) {
    s.act(Order::Entity {
        entity,
        order: EntityOrder::Change(change),
    });
}

fn saved(s: &Scene, entity: EntityId) -> rspigot_nbt::Compound {
    s.server.saved_entity(entity).expect("the entity is saved")
}

#[test]
fn as_zombie_is_none_for_a_creeper() {
    let mut s = arena();
    let creeper = spawn(&mut s, "minecraft:creeper");
    let before = saved(&s, creeper);

    change(&mut s, creeper, EntityTrait::ZombieBaby(true));

    assert_eq!(saved(&s, creeper), before);
    assert!(
        s.warnings()
            .iter()
            .any(|warning| warning.contains("entity.change") && warning.contains("zombie"))
    );
}

#[test]
fn a_zombie_becomes_a_baby() {
    let mut s = arena();
    let zombie = spawn(&mut s, "minecraft:zombie");
    change(&mut s, zombie, EntityTrait::ZombieBaby(false));
    assert_ne!(saved(&s, zombie).get_bool("IsBaby"), Some(true));

    change(&mut s, zombie, EntityTrait::ZombieBaby(true));

    assert_eq!(saved(&s, zombie).get_bool("IsBaby"), Some(true));
}

#[test]
fn a_creeper_is_powered() {
    let mut s = arena();
    let creeper = spawn(&mut s, "minecraft:creeper");

    change(&mut s, creeper, EntityTrait::CreeperPowered(true));

    assert_eq!(saved(&s, creeper).get_bool("powered"), Some(true));
}

#[test]
fn a_sheep_changes_colour_and_is_sheared() {
    let mut s = arena();
    let sheep = spawn(&mut s, "minecraft:sheep");

    change(&mut s, sheep, EntityTrait::SheepColor(14));
    change(&mut s, sheep, EntityTrait::SheepSheared(true));

    let saved = saved(&s, sheep);
    assert_eq!(saved.get_i64("Color"), Some(14));
    assert_eq!(saved.get_bool("Sheared"), Some(true));
}

#[test]
fn a_slime_changes_size_and_a_colour_out_of_range_is_refused() {
    let mut s = arena();
    let slime = spawn(&mut s, "minecraft:slime");
    let sheep = spawn(&mut s, "minecraft:sheep");

    change(&mut s, slime, EntityTrait::SlimeSize(3));
    change(&mut s, sheep, EntityTrait::SheepColor(16));

    assert_eq!(saved(&s, slime).get_i64("Size"), Some(2));
    assert!(
        s.warnings()
            .iter()
            .any(|warning| warning.contains("colour"))
    );
}

#[test]
fn a_villager_takes_a_profession_and_a_wolf_a_collar() {
    let mut s = arena();
    let villager = spawn(&mut s, "minecraft:villager");
    let wolf = spawn(&mut s, "minecraft:wolf");

    change(&mut s, villager, EntityTrait::VillagerProfession(5));
    change(&mut s, wolf, EntityTrait::WolfCollar(3));

    let villager = saved(&s, villager);
    let data = villager
        .get_compound("VillagerData")
        .expect("villager data");
    assert_eq!(data.get_str("profession"), Some("minecraft:farmer"));
    assert_eq!(saved(&s, wolf).get_i64("CollarColor"), Some(3));
}

#[test]
fn a_cat_and_a_rabbit_take_their_look() {
    let mut s = arena();
    let cat = spawn(&mut s, "minecraft:cat");
    let rabbit = spawn(&mut s, "minecraft:rabbit");

    change(&mut s, cat, EntityTrait::CatCollar(11));
    change(&mut s, rabbit, EntityTrait::RabbitVariant(4));

    assert_eq!(saved(&s, cat).get_i64("CollarColor"), Some(11));
    assert_eq!(saved(&s, rabbit).get_i64("RabbitType"), Some(4));
}

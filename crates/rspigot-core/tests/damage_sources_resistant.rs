mod common;

use common::*;
use rspigot_core::events::Inbound;
use rspigot_entities::EntityKind as Kind;

fn console(s: &mut Scene, line: &str) {
    s.server.handle(Inbound::Console(line.to_owned()));
    s.server.deliver();
}

fn settled() -> Scene {
    let mut s = scene(&["Alice"]);
    s.tick(40);
    s
}

fn items(s: &Scene) -> usize {
    s.server.entities_of(Kind::Item).len()
}

fn drop_into(s: &mut Scene, block: &str, item: &str) {
    console(s, &format!("setblock 5 64 5 {block}"));
    console(
        s,
        &format!("summon minecraft:item 5.5 64.2 5.5 {{Item:{{id:\"minecraft:{item}\",count:1}}}}"),
    );
    assert_eq!(items(s), 1);
}

#[test]
fn a_dropped_stick_burns_up_in_lava() {
    let mut s = settled();
    drop_into(&mut s, "minecraft:lava", "stick");
    s.tick(40);
    assert_eq!(items(&s), 0);
}

#[test]
fn a_dropped_netherite_ingot_survives_lava() {
    let mut s = settled();
    drop_into(&mut s, "minecraft:lava", "netherite_ingot");
    s.tick(100);
    assert_eq!(items(&s), 1);
}

#[test]
fn a_dropped_stick_burns_up_in_fire() {
    let mut s = settled();
    drop_into(&mut s, "minecraft:fire", "stick");
    s.tick(60);
    assert_eq!(items(&s), 0);
}

#[test]
fn ancient_debris_survives_fire() {
    let mut s = settled();
    drop_into(&mut s, "minecraft:fire", "ancient_debris");
    s.tick(100);
    assert_eq!(items(&s), 1);
}

#[test]
fn a_stack_can_override_its_resistance_with_the_component() {
    let mut s = settled();
    console(&mut s, "setblock 5 64 5 minecraft:lava");
    console(
        &mut s,
        "summon minecraft:item 5.5 64.2 5.5 {Item:{id:\"minecraft:stick\",count:1,components:{\"minecraft:damage_resistant\":{types:\"#minecraft:is_fire\"}}}}",
    );
    assert_eq!(items(&s), 1);
    s.tick(100);
    assert_eq!(items(&s), 1);
}

fn blast(item: &str) -> usize {
    let wanted = match item {
        "stick" => rspigot_items::ItemKind::Stick.id(),
        "nether_star" => rspigot_items::ItemKind::NetherStar.id(),
        _ => rspigot_items::ItemKind::NetheriteIngot.id(),
    };
    let mut s = settled();
    console(
        &mut s,
        &format!("summon minecraft:item 5.5 64.2 5.5 {{Item:{{id:\"minecraft:{item}\",count:1}}}}"),
    );
    console(&mut s, "summon minecraft:tnt 5.5 64.0 5.5 {fuse:5s}");
    s.tick(20);
    s.server
        .ground_items()
        .iter()
        .filter(|(_, stack)| stack.item == wanted)
        .count()
}

#[test]
fn an_explosion_destroys_a_stick_and_netherite_but_not_a_nether_star() {
    assert_eq!(blast("stick"), 0);
    assert_eq!(blast("nether_star"), 1);
    assert_eq!(blast("netherite_ingot"), 0);
}

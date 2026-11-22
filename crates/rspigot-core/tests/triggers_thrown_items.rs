mod common;

use common::*;
use rspigot_core::events::Inbound;
use rspigot_entities::EntityKind as Kind;

const ADVANCEMENT: &str = "minecraft:husbandry/allay_deliver_item_to_player";

fn console(s: &mut Scene, line: &str) {
    s.server.handle(Inbound::Console(line.to_owned()));
    s.server.deliver();
}

fn uuid_nbt(uuid: uuid::Uuid) -> String {
    let bits = uuid.as_u128();
    format!(
        "[I;{},{},{},{}]",
        (bits >> 96) as u32 as i32,
        (bits >> 64) as u32 as i32,
        (bits >> 32) as u32 as i32,
        bits as u32 as i32
    )
}

fn done(s: &Scene) -> bool {
    s.server
        .done_advancements(s.players[0].id)
        .contains(&ADVANCEMENT)
}

#[test]
fn an_item_thrown_by_an_allay_completes_the_delivery_when_the_player_picks_it_up() {
    let mut s = scene(&["Alice"]);
    s.tick(40);
    console(&mut s, "summon minecraft:allay 5.5 66 5.5 {NoAI:1b}");
    let allay = s.server.entities_of(Kind::Allay)[0].uuid;
    console(
        &mut s,
        &format!(
            "summon minecraft:item 0.5 64 0.5 {{Item:{{id:\"minecraft:stick\",count:1}},Thrower:{},PickupDelay:0s}}",
            uuid_nbt(allay)
        ),
    );
    assert!(!done(&s));
    s.tick(5);
    assert!(done(&s));
}

#[test]
fn an_item_nobody_threw_does_not_complete_the_delivery() {
    let mut s = scene(&["Alice"]);
    s.tick(40);
    console(
        &mut s,
        "summon minecraft:item 0.5 64 0.5 {Item:{id:\"minecraft:stick\",count:1},PickupDelay:0s}",
    );
    s.tick(5);
    assert!(!done(&s));
}

#[test]
fn a_piglin_picking_up_gold_the_player_threw_completes_distract_piglin() {
    let mut s = scene(&["Alice"]);
    s.tick(40);
    console(&mut s, "summon minecraft:piglin 3.5 64 0.5");
    let thrower = uuid_nbt(s.players[0].id.0);
    console(
        &mut s,
        &format!(
            "summon minecraft:item 3.5 64 0.5 {{Item:{{id:\"minecraft:gold_ingot\",count:1}},Thrower:{thrower},PickupDelay:0s}}"
        ),
    );
    for _ in 0..200 {
        s.tick(1);
        if s.server
            .done_advancements(s.players[0].id)
            .contains(&"minecraft:nether/distract_piglin")
        {
            break;
        }
    }
    assert_eq!(s.server.entities_of(Kind::Item).len(), 0, "item untouched");
    assert!(
        s.server
            .done_advancements(s.players[0].id)
            .contains(&"minecraft:nether/distract_piglin")
    );
}

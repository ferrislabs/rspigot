mod common;

use common::*;
use rspigot_core::events::{Hand, Inbound, PlayerInput};
use rspigot_domain::Vec3;
use rspigot_entities::EntityKind as Kind;

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

fn interact(s: &mut Scene, item: &str, kind: Kind, nbt: &str) {
    s.tick(40);
    let owner = uuid_nbt(s.players[0].id.0);
    console(
        s,
        &format!(
            "summon minecraft:{} 2.5 64 0.5 {{NoAI:1b,Owner:{owner}{nbt}}}",
            kind.name().trim_start_matches("minecraft:")
        ),
    );
    let target = s.server.entities_of(kind)[0].id;
    console(s, &format!("give Alice {item} 1"));
    let alice = s.players[0].clone();
    send_input(&mut s.server, &alice, PlayerInput::HeldSlot(0));
    s.tick(2);
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
    s.tick(2);
}

fn done(s: &Scene, advancement: &str) -> bool {
    s.server
        .done_advancements(s.players[0].id)
        .contains(&advancement)
}

const DAMAGED: &str = ",Sitting:1b,equipment:{body:{id:\"minecraft:wolf_armor\",count:1,components:{\"minecraft:damage\":5}}}";

#[test]
fn repairing_wolf_armor_completely_grants_repair_wolf_armor() {
    let mut s = scene(&["Alice"]);
    interact(&mut s, "minecraft:armadillo_scute", Kind::Wolf, DAMAGED);
    assert!(done(&s, "minecraft:husbandry/repair_wolf_armor"));
}

#[test]
fn a_partial_repair_does_not() {
    let mut s = scene(&["Alice"]);
    let heavy = DAMAGED.replace(":5}", ":40}");
    interact(&mut s, "minecraft:armadillo_scute", Kind::Wolf, &heavy);
    assert!(!done(&s, "minecraft:husbandry/repair_wolf_armor"));
}

#[test]
fn leashing_a_frog_ticks_its_variant_criterion() {
    let mut s = scene(&["Alice"]);
    interact(
        &mut s,
        "minecraft:lead",
        Kind::Frog,
        ",variant:\"minecraft:warm\"",
    );
    let obtained = s.server.obtained_criteria(
        s.players[0].id,
        "minecraft:husbandry/leash_all_frog_variants",
    );
    assert_eq!(obtained, vec!["minecraft:warm"]);
}

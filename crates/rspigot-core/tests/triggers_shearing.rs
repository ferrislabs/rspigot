mod common;

use common::*;
use rspigot_core::events::{Hand, Inbound, PlayerInput};
use rspigot_domain::Vec3;
use rspigot_entities::EntityKind as Kind;

const ADVANCEMENT: &str = "minecraft:husbandry/remove_wolf_armor";

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

fn shear(armored: bool) -> Scene {
    let mut s = scene(&["Alice"]);
    s.tick(40);
    let armor = if armored {
        ",equipment:{body:{id:\"minecraft:wolf_armor\",count:1}}"
    } else {
        ""
    };
    let owner = uuid_nbt(s.players[0].id.0);
    console(
        &mut s,
        &format!("summon minecraft:wolf 2.5 64 0.5 {{NoAI:1b,Owner:{owner}{armor}}}"),
    );
    let wolf = s.server.entities_of(Kind::Wolf)[0].id;
    console(&mut s, "give Alice minecraft:shears 1");
    let alice = s.players[0].clone();
    send_input(&mut s.server, &alice, PlayerInput::HeldSlot(0));
    s.tick(2);
    send_input(
        &mut s.server,
        &alice,
        PlayerInput::Interact {
            target: wolf,
            hand: Hand::Main,
            sneaking: false,
            location: Vec3::new(0.0, 0.5, 0.0),
        },
    );
    s.tick(2);
    s
}

#[test]
fn shearing_the_armor_off_a_wolf_completes_remove_wolf_armor() {
    assert!(done(&shear(true)));
}

#[test]
fn shearing_a_bare_wolf_does_not() {
    assert!(!done(&shear(false)));
}

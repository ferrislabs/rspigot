mod common;

use common::*;
use rspigot_core::events::{ClientEvent, Inbound};
use rspigot_domain::effect::EffectDuration;

fn console(s: &mut Scene, line: &str) {
    s.server.handle(Inbound::Console(line.to_owned()));
    s.server.deliver();
}

#[test]
fn a_hidden_weaker_effect_comes_back_when_the_stronger_one_ends() {
    let mut s = scene(&["Alice"]);
    console(&mut s, "effect give Alice minecraft:speed 30 0");
    console(&mut s, "effect give Alice minecraft:speed 2 2");
    s.players[0].link.take();
    s.tick(60);
    let events = s.players[0].link.take();
    let speed = rspigot_domain::effect::EffectId::from_name("minecraft:speed").unwrap();
    assert!(
        !events.iter().any(
            |event| matches!(event, ClientEvent::EffectRemoved { effect, .. } if *effect == speed)
        ),
        "{events:?}"
    );
    let back = events.iter().find_map(|event| match event {
        ClientEvent::EffectApplied { effect, .. } if effect.effect == speed => Some(*effect),
        _ => None,
    });
    let back = back.expect("the weaker speed is announced again");
    assert_eq!(back.amplifier, 0);
    let EffectDuration::Ticks(left) = back.duration else {
        panic!("a finite duration");
    };
    assert!(left > 400, "{left}");
}

#[test]
fn a_hidden_effect_restored_on_a_mob_is_sent_to_its_trackers() {
    let mut s = scene(&["Alice"]);
    stream_chunk_of(&mut s, rspigot_domain::BlockPos::new(8, 64, 11));
    console(&mut s, "summon minecraft:cow 8.5 64 11.5");
    s.tick(5);
    console(&mut s, "effect give @e[type=cow] minecraft:speed 30 0");
    console(&mut s, "effect give @e[type=cow] minecraft:speed 2 2");
    s.players[0].link.take();
    s.tick(60);
    let events = s.players[0].link.take();
    let speed = rspigot_domain::effect::EffectId::from_name("minecraft:speed").unwrap();
    let cow = s.server.entities_of(rspigot_entities::EntityKind::Cow)[0].id;
    let back = events.iter().find_map(|event| match event {
        ClientEvent::EffectApplied { entity, effect, .. }
            if *entity == cow && effect.effect == speed =>
        {
            Some(*effect)
        }
        _ => None,
    });
    assert_eq!(back.map(|effect| effect.amplifier), Some(0), "{events:?}");
}

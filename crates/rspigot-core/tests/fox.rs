mod common;

use common::*;
use rspigot_core::events::{ClientEvent, Inbound, MetadataValue, PlayerInput};
use rspigot_core::plugins::{Answer, BlockOrder, Order, Question};
use rspigot_domain::entity::save::uuid_tag;
use rspigot_domain::{BlockPos, EntityId};
use rspigot_entities::EntityKind as Kind;
use rspigot_items::ItemKind;
use rspigot_nbt::Tag;
use uuid::Uuid;

fn console(s: &mut Scene, line: &str) {
    s.server.handle(Inbound::Console(line.to_owned()));
    s.server.deliver();
}

fn ready() -> Scene {
    let mut s = scene(&["Alice"]);
    stream_chunk_of(&mut s, BlockPos::new(1, 64, 1));
    s
}

fn summon(s: &mut Scene, nbt: &str, x: f64, z: f64) -> EntityId {
    let before = s.server.entities_of(Kind::Fox).len();
    console(s, &format!("summon minecraft:fox {x} 64 {z} {nbt}"));
    let foxes = s.server.entities_of(Kind::Fox);
    assert_eq!(foxes.len(), before + 1);
    foxes.last().unwrap().id
}

fn saved(s: &Scene, id: EntityId) -> rspigot_nbt::Compound {
    s.server.saved_entity(id).expect("the fox exists")
}

fn trusting(s: &Scene, extra: &str) -> String {
    let value = s.players[0].id.0.as_u128();
    let parts: Vec<String> = (0..4)
        .map(|i| ((value >> (96 - 32 * i)) as u32 as i32).to_string())
        .collect();
    format!("{{Trusted:[[I;{}]],{extra}}}", parts.join(","))
}

fn stay_close(s: &mut Scene, fox: EntityId) {
    let at = s
        .server
        .entities_of(Kind::Fox)
        .into_iter()
        .find(|entity| entity.id == fox)
        .map(|entity| entity.position);
    if let Some(at) = at {
        console(s, &format!("tp Alice {} {} {}", at.x, at.y, at.z));
    }
}

fn ground(s: &Scene, item: ItemKind) -> u32 {
    s.server
        .ground_items()
        .iter()
        .filter(|(_, stack)| stack.item == item.id())
        .map(|(_, stack)| u32::from(stack.count.get()))
        .sum()
}

fn mouth(s: &Scene, id: EntityId) -> Option<String> {
    saved(s, id)
        .get_compound("equipment")
        .and_then(|equipment| equipment.get_compound("mainhand"))
        .and_then(|stack| stack.get_str("id"))
        .map(str::to_owned)
}

fn priorities(s: &Scene, id: EntityId) -> Vec<i32> {
    s.server
        .mob_ai(id)
        .expect("the fox has an AI")
        .targets()
        .goals()
        .iter()
        .map(rspigot_ai::WrappedGoal::priority)
        .collect()
}

#[test]
fn a_fox_keeps_its_variant_states_and_trusted_players_through_nbt() {
    let mut s = ready();
    let id = summon(
        &mut s,
        "{NoAI:1b,Type:\"snow\",Sleeping:1b,Sitting:1b,Crouching:1b,Trusted:[[I;1,2,3,4],[I;5,6,7,8]]}",
        1.5,
        1.5,
    );
    let nbt = saved(&s, id);
    assert_eq!(nbt.get_str("Type"), Some("snow"));
    assert_eq!(nbt.get_bool("Sleeping"), Some(true));
    assert_eq!(nbt.get_bool("Sitting"), Some(true));
    assert_eq!(nbt.get_bool("Crouching"), Some(true));
    let trusted = nbt.get_list("Trusted").expect("trusted list").to_vec();
    assert_eq!(
        trusted,
        vec![
            uuid_tag(Uuid::from_u128(0x0000_0001_0000_0002_0000_0003_0000_0004)),
            uuid_tag(Uuid::from_u128(0x0000_0005_0000_0006_0000_0007_0000_0008)),
        ]
    );
    assert!(matches!(trusted[0], Tag::IntArray(_)));
}

#[test]
fn a_fox_syncs_its_variant_flags_and_trusted_slots_at_the_vanilla_indexes() {
    let mut s = ready();
    s.players[0].link.take();
    let id = summon(
        &mut s,
        "{NoAI:1b,Type:\"snow\",Sleeping:1b,Trusted:[[I;1,2,3,4]]}",
        1.5,
        1.5,
    );
    s.tick(2);
    let entries: Vec<_> = s.players[0]
        .link
        .take()
        .into_iter()
        .filter_map(|event| match event {
            ClientEvent::EntityMetadata { entity, entries } if entity == id => Some(entries),
            _ => None,
        })
        .flatten()
        .collect();
    let value = |index: u8| {
        entries
            .iter()
            .find(|entry| entry.index == index)
            .map(|entry| entry.value.clone())
    };
    assert_eq!(value(18), Some(MetadataValue::VarInt(1)));
    assert_eq!(value(19), Some(MetadataValue::Byte(32)));
    assert_eq!(
        value(20),
        Some(MetadataValue::OptionalEntity(Some(Uuid::from_u128(
            0x0000_0001_0000_0002_0000_0003_0000_0004
        ))))
    );
}

#[test]
fn hunting_goals_follow_the_variant_of_a_loaded_fox() {
    let mut s = ready();
    let red = summon(&mut s, "{NoAI:1b,Type:\"red\"}", 1.5, 1.5);
    let snow = summon(&mut s, "{NoAI:1b,Type:\"snow\"}", 3.5, 1.5);
    assert_eq!(priorities(&s, red), vec![3, 4, 4, 6]);
    assert_eq!(priorities(&s, snow), vec![3, 4, 6, 6]);
}

#[test]
fn sweet_berries_put_a_fox_in_love() {
    let mut s = ready();
    console(&mut s, "give Alice minecraft:sweet_berries 2");
    let fox = summon(&mut s, "{NoAI:1b}", 1.5, 1.5);
    let alice = s.players[0].clone();
    send_input(
        &mut s.server,
        &alice,
        PlayerInput::Interact {
            target: fox,
            hand: rspigot_core::events::Hand::Main,
            sneaking: false,
            location: rspigot_domain::Vec3::new(0.0, 0.5, 0.0),
        },
    );
    assert!(saved(&s, fox).get_i32("InLove").unwrap_or(0) > 0);
}

#[test]
fn a_fox_picks_up_one_item_of_a_stack_and_only_with_an_empty_mouth() {
    let mut s = ready();
    let fox = summon(&mut s, "{NoAI:1b}", 9.5, 9.5);
    console(
        &mut s,
        "summon minecraft:item 9.5 64 9.5 {Item:{id:\"minecraft:emerald\",count:3},PickupDelay:0s}",
    );
    s.tick(3);
    assert_eq!(mouth(&s, fox).as_deref(), Some("minecraft:emerald"));
    assert_eq!(ground(&s, ItemKind::Emerald), 2);
    console(
        &mut s,
        "summon minecraft:item 9.5 64 9.5 {Item:{id:\"minecraft:feather\",count:1},PickupDelay:0s}",
    );
    s.tick(5);
    assert_eq!(ground(&s, ItemKind::Feather), 1);
    assert_eq!(mouth(&s, fox).as_deref(), Some("minecraft:emerald"));
    let drop = saved(&s, fox)
        .get_compound("drop_chances")
        .and_then(|chances| chances.get_f64("mainhand"));
    assert_eq!(drop, Some(2.0));
}

#[test]
fn an_item_still_on_its_pickup_delay_is_left_alone() {
    let mut s = ready();
    let fox = summon(&mut s, "{NoAI:1b}", 1.5, 1.5);
    console(
        &mut s,
        "summon minecraft:item 1.5 64 1.5 {Item:{id:\"minecraft:emerald\",count:1},PickupDelay:100s}",
    );
    s.tick(3);
    assert_eq!(mouth(&s, fox), None);
    assert_eq!(ground(&s, ItemKind::Emerald), 1);
}

#[test]
fn a_dead_fox_drops_what_it_holds_in_its_mouth() {
    let mut s = ready();
    summon(
        &mut s,
        "{NoAI:1b,equipment:{mainhand:{id:\"minecraft:emerald\",count:1}}}",
        1.5,
        1.5,
    );
    console(&mut s, "kill @e[type=fox]");
    s.tick(2);
    assert_eq!(ground(&s, ItemKind::Emerald), 1);
}

#[test]
fn a_fox_eats_the_food_in_its_mouth_after_thirty_seconds() {
    let mut s = ready();
    console(&mut s, "time set 18000");
    let nbt = trusting(
        &s,
        "Sitting:1b,home_radius:4,home_pos:[I;1,64,1],equipment:{mainhand:{id:\"minecraft:bread\",count:1}}",
    );
    let fox = summon(&mut s, &nbt, 1.5, 1.5);
    assert_eq!(mouth(&s, fox).as_deref(), Some("minecraft:bread"));
    s.tick(100);
    assert_eq!(mouth(&s, fox).as_deref(), Some("minecraft:bread"));
    let mut eaten = false;
    for round in 0..3000 {
        s.tick(1);
        if round % 20 == 0 {
            stay_close(&mut s, fox);
        }
        if mouth(&s, fox).is_none() {
            eaten = true;
            break;
        }
    }
    assert!(eaten);
}

#[test]
fn a_fox_counts_one_tick_per_tick_before_it_eats() {
    let mut s = ready();
    console(&mut s, "time set 18000");
    let nbt = trusting(
        &s,
        "Sitting:1b,home_radius:4,home_pos:[I;1,64,1],equipment:{mainhand:{id:\"minecraft:bread\",count:1}}",
    );
    let fox = summon(&mut s, &nbt, 1.5, 1.5);
    let mut ticks = 0;
    while mouth(&s, fox).is_some() && ticks < 1000 {
        s.tick(1);
        ticks += 1;
        if ticks % 20 == 0 {
            stay_close(&mut s, fox);
        }
    }
    assert!((600..1000).contains(&ticks), "ate after {ticks} ticks");
}

#[test]
fn bred_foxes_trust_their_breeder_and_hunt_only_after_a_reload() {
    let mut s = ready();
    console(&mut s, "give Alice minecraft:sweet_berries 4");
    let first = summon(&mut s, "{Type:\"red\"}", 1.5, 1.5);
    let second = summon(&mut s, "{Type:\"snow\"}", 2.5, 1.5);
    let alice = s.players[0].clone();
    for fox in [first, second] {
        send_input(
            &mut s.server,
            &alice,
            PlayerInput::Interact {
                target: fox,
                hand: rspigot_core::events::Hand::Main,
                sneaking: false,
                location: rspigot_domain::Vec3::new(0.0, 0.5, 0.0),
            },
        );
    }
    let mut born = None;
    for _ in 0..600 {
        s.tick(1);
        let foxes = s.server.entities_of(Kind::Fox);
        if foxes.len() == 3 {
            born = foxes
                .iter()
                .map(|fox| fox.id)
                .find(|id| *id != first && *id != second);
            break;
        }
    }
    let baby = born.expect("the foxes bred");
    let nbt = saved(&s, baby);
    assert_eq!(
        nbt.get_list("Trusted").map(<[Tag]>::to_vec),
        Some(vec![uuid_tag(alice.id.0)])
    );
    assert!(matches!(nbt.get_str("Type"), Some("red" | "snow")));
    assert!(nbt.get_i32("Age").unwrap_or(0) < 0);
    assert_eq!(priorities(&s, baby), vec![3]);
}

#[test]
fn a_fox_picks_ripe_berries_and_keeps_one_in_its_mouth() {
    let mut s = ready();
    s.act(Order::Block {
        pos: BlockPos::new(1, 64, 1),
        order: BlockOrder::SetWith {
            kind: "minecraft:sweet_berry_bush".to_owned(),
            properties: vec![("age".to_owned(), "3".to_owned())],
        },
    });
    let nbt = trusting(&s, "Sitting:1b");
    let fox = summon(&mut s, &nbt, 1.5, 1.5);
    let mut picked = false;
    for round in 0..4000 {
        s.tick(1);
        if round % 20 == 0 {
            stay_close(&mut s, fox);
        }
        let Answer::Block(state) = s.ask(Question::Block(BlockPos::new(1, 64, 1))) else {
            panic!("a block answer");
        };
        if state
            .properties
            .iter()
            .any(|(name, value)| name == "age" && value == "1")
        {
            picked = true;
            break;
        }
    }
    assert!(picked, "the bush was never picked");
    assert_eq!(mouth(&s, fox).as_deref(), Some("minecraft:sweet_berries"));
    assert!(ground(&s, ItemKind::SweetBerries) >= 1);
}

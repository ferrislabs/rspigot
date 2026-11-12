mod common;

use common::*;
use rspigot_core::config::PluginSettings;
use rspigot_core::events::{ClientEvent, EntityStatus, EquipmentSlot, Inbound, PlayerInput};
use rspigot_core::plugins::events::EntityDeathPatch;
use rspigot_core::plugins::{
    Answer, EntityOrder, EventKind, EventPatch, EventPayload, HandlerSpec, ItemSpec, Order,
    Priority, Question,
};
use rspigot_domain::EntityId;
use rspigot_entities::EntityKind as Kind;

fn waiting_scene(handlers: Vec<HandlerSpec>) -> Scene {
    let mut server = server(PluginSettings {
        decision_ticks: 1_000,
        ..PluginSettings::default()
    });
    let plugin = connect_with(&mut server, 1, "tester", handlers);
    let alice = join(&mut server, "Alice");
    server.deliver();
    alice.link.take();
    Scene {
        server,
        plugin,
        players: vec![alice],
    }
}

fn console(s: &mut Scene, line: &str) {
    s.server.handle(Inbound::Console(line.to_owned()));
    s.server.deliver();
}

fn only(s: &Scene, kind: Kind) -> EntityId {
    let found = s.server.entities_of(kind);
    assert_eq!(found.len(), 1, "{kind:?}");
    found[0].id
}

fn hit_then_kill(s: &mut Scene, kind: &str, nbt: &str) -> EntityId {
    let alice = s.players[0].clone();
    stream_chunk_of(s, rspigot_domain::BlockPos::new(1, 64, 0));
    console(s, &format!("summon {kind} 1.5 64 0.5 {nbt}"));
    let id = only(s, Kind::from_name(kind).expect("a kind"));
    s.tick(1);
    send_input(&mut s.server, &alice, PlayerInput::Attack(id));
    s.act(Order::Entity {
        entity: id,
        order: EntityOrder::Kill,
    });
    id
}

fn ground(s: &Scene) -> Vec<String> {
    s.server
        .ground_items()
        .into_iter()
        .map(|(_, stack)| format!("{}x{}", stack.item.0, stack.count))
        .collect()
}

fn dead(s: &mut Scene, id: EntityId) -> bool {
    s.server.mob_damage(id).is_some_and(|state| state.dead)
}

#[test]
fn a_mob_death_reaches_entity_death_handlers_then_its_drops_spawn_items_and_orbs() {
    let mut s = scene_with(
        &["Alice"],
        vec![
            observe(1, EventKind::EntityDeath),
            observe(2, EventKind::EntitySpawn),
            observe(3, EventKind::EntityDropItem),
        ],
    );
    let cow = hit_then_kill(&mut s, "minecraft:cow", "{NoAI:1b}");
    let kinds: Vec<EventKind> = settled_kinds(&s);
    let death = kinds
        .iter()
        .position(|kind| *kind == EventKind::EntityDeath)
        .expect("the cow death");
    assert!(
        kinds[death + 1..]
            .iter()
            .all(|kind| matches!(kind, EventKind::ItemSpawn | EventKind::EntitySpawn))
    );
    assert!(kinds[death + 1..].contains(&EventKind::ItemSpawn));
    assert_eq!(kinds.last(), Some(&EventKind::EntitySpawn));
    assert!(!kinds.contains(&EventKind::EntityDropItem));
    let EventPayload::EntityDeath(event) =
        &settled_for(&s.plugin, EventKind::EntityDeath)[0].payload
    else {
        panic!("an entity death");
    };
    assert_eq!(event.entity.id, cow);
    assert_eq!(event.entity_type, "minecraft:cow");
    assert_eq!(event.damage_source.kind, "minecraft:generic_kill");
    assert!(event.dropped_exp > 0);
    assert!(
        event
            .drops
            .iter()
            .any(|drop| drop.key == "minecraft:leather" || drop.key == "minecraft:beef")
    );
}

fn settled_kinds(s: &Scene) -> Vec<EventKind> {
    fired(&s.plugin)
        .into_iter()
        .map(|event| event.payload.kind())
        .collect()
}

#[test]
fn a_mob_death_waits_with_its_loot_and_drops_what_settled() {
    let mut s = waiting_scene(vec![deciding(1, EventKind::EntityDeath)]);
    let alice = s.players[0].clone();
    let zombie = hit_then_kill(
        &mut s,
        "minecraft:zombie",
        "{NoAI:1b,equipment:{head:{id:\"minecraft:iron_helmet\",count:1}},drop_chances:{head:2.0f}}",
    );
    assert!(!dead(&mut s, zombie));
    assert!(ground(&s).is_empty());
    let equipment = s
        .server
        .mob_damage(zombie)
        .expect("the dead zombie")
        .equipment;
    assert_eq!(equipment.len(), 1, "the helmet waits on the zombie");
    let asked = last_asked(&s.plugin, EventKind::EntityDeath);
    let EventPayload::EntityDeath(event) = &asked.payload else {
        panic!("an entity death");
    };
    assert!(
        event
            .drops
            .iter()
            .any(|drop| drop.key == "minecraft:iron_helmet")
    );
    alice.link.take();
    rule_on(
        &mut s.server,
        1,
        asked.id,
        false,
        Some(EventPatch::EntityDeath(EntityDeathPatch {
            dropped_exp: Some(0),
            drops: replacing(&event.drops, vec![ItemSpec::of("minecraft:diamond", 2)]),
        })),
    );
    assert!(dead(&mut s, zombie));
    let diamond = rspigot_items::ItemKind::Diamond.id().0;
    assert_eq!(ground(&s), [format!("{diamond}x2")]);
    assert!(s.server.experience_orbs().is_empty());
    assert!(
        s.server
            .mob_damage(zombie)
            .expect("the dead zombie")
            .equipment
            .is_empty()
    );
    s.server.deliver();
    assert!(alice.link.take().iter().any(|event| matches!(
        event,
        ClientEvent::EntityEquipment { entity, slots } if *entity == zombie
            && slots.contains(&(EquipmentSlot::Head, None))
    )));
}

fn died(s: &mut Scene, id: EntityId) -> bool {
    s.server.deliver();
    s.players[0].link.take().iter().any(|event| {
        matches!(
            event,
            ClientEvent::EntityStatus {
                entity,
                status: EntityStatus::Death,
            } if *entity == id
        )
    })
}

#[test]
fn a_mob_whose_death_waits_lies_dying_until_it_settles() {
    let mut s = waiting_scene(vec![deciding(1, EventKind::EntityDeath)]);
    let cow = hit_then_kill(&mut s, "minecraft:cow", "{NoAI:1b}");
    assert!(!dead(&mut s, cow));
    assert!(!died(&mut s, cow));
    s.tick(25);
    assert!(matches!(s.ask(Question::Entity(cow)), Answer::Entity(_)));
    assert!(!died(&mut s, cow));
    assert!(ground(&s).is_empty());
    let asked = last_asked(&s.plugin, EventKind::EntityDeath);
    rule_on(&mut s.server, 1, asked.id, false, None);
    assert!(dead(&mut s, cow));
    assert!(died(&mut s, cow));
    assert!(
        !ground(&s).is_empty(),
        "the loot drops once the death settled"
    );
    s.tick(25);
    assert!(matches!(s.ask(Question::Entity(cow)), Answer::NotFound));
}

#[test]
fn a_damaged_equipment_drop_with_the_default_chance_empties_its_slot() {
    let mut s = scene_with(&["Alice"], Vec::new());
    let alice = s.players[0].clone();
    stream_chunk_of(&mut s, rspigot_domain::BlockPos::new(1, 64, 0));
    let helmet = rspigot_items::ItemKind::IronHelmet.id();
    for _ in 0..400 {
        let before: Vec<EntityId> = s
            .server
            .entities_of(Kind::Zombie)
            .iter()
            .map(|zombie| zombie.id)
            .collect();
        console(
            &mut s,
            "summon minecraft:zombie 1.5 64 0.5 {NoAI:1b,equipment:{head:{id:\"minecraft:iron_helmet\",count:1}}}",
        );
        let zombie = s
            .server
            .entities_of(Kind::Zombie)
            .iter()
            .map(|zombie| zombie.id)
            .find(|id| !before.contains(id))
            .expect("a new zombie");
        s.tick(1);
        send_input(&mut s.server, &alice, PlayerInput::Attack(zombie));
        s.act(Order::Entity {
            entity: zombie,
            order: EntityOrder::Kill,
        });
        let dropped = s
            .server
            .ground_items()
            .iter()
            .any(|(_, stack)| stack.item == helmet);
        if dropped {
            let equipment = s
                .server
                .mob_damage(zombie)
                .expect("the dead zombie")
                .equipment;
            assert!(equipment.is_empty(), "{equipment:?}");
            return;
        }
    }
    panic!("no helmet dropped in 400 kills");
}

#[test]
fn two_plugins_that_each_remove_a_drop_both_keep_it_off_the_ground() {
    let deciding_death = |priority| HandlerSpec {
        priority,
        ..deciding(1, EventKind::EntityDeath)
    };
    for later_answers_first in [false, true] {
        let mut s = waiting_scene(vec![deciding_death(Priority::Low)]);
        connect_with(
            &mut s.server,
            2,
            "second",
            vec![deciding_death(Priority::High)],
        );
        let zombie = hit_then_kill(
            &mut s,
            "minecraft:zombie",
            "{NoAI:1b,equipment:{head:{id:\"minecraft:iron_helmet\",count:1},chest:{id:\"minecraft:iron_chestplate\",count:1}},drop_chances:{head:2.0f,chest:2.0f}}",
        );
        let asked = last_asked(&s.plugin, EventKind::EntityDeath);
        let EventPayload::EntityDeath(event) = &asked.payload else {
            panic!("an entity death");
        };
        let [first, second] = ["minecraft:iron_helmet", "minecraft:iron_chestplate"].map(|gone| {
            assert!(event.drops.iter().any(|drop| drop.key == gone));
            Some(EventPatch::EntityDeath(EntityDeathPatch {
                drops: removing(&event.drops, |drop| drop.key == gone),
                ..EntityDeathPatch::default()
            }))
        });
        let mut answers = vec![(1, first), (2, second)];
        if later_answers_first {
            answers.reverse();
        }
        for (connection, patch) in answers {
            rule_on(&mut s.server, connection, asked.id, false, patch);
        }
        assert!(dead(&mut s, zombie));
        let thrown: Vec<String> = s
            .server
            .ground_items()
            .iter()
            .filter_map(|(_, stack)| Some(stack.item.kind()?.name().to_owned()))
            .collect();
        assert!(
            !thrown
                .iter()
                .any(|name| name == "minecraft:iron_helmet" || name == "minecraft:iron_chestplate"),
            "{later_answers_first} {thrown:?}"
        );
    }
}

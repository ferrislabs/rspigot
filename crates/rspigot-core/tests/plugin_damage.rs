mod common;

use common::*;
use rspigot_core::config::PluginSettings;
use rspigot_core::events::{ClientEvent, Inbound, PlayerInput};
use rspigot_core::plugins::events::{
    EntityDamageByBlockPatch, EntityDamageByEntityPatch, EntityDamageCause, EntityDamagePatch,
};
use rspigot_core::plugins::{
    Answer, BlockOrder, EntityOrder, EntityView, EventKind, EventPatch, EventPayload, HandlerSpec,
    Order, Question, ToPlugin, WorldOrder,
};
use rspigot_domain::world::Difficulty;
use rspigot_domain::{BlockPos, EntityId, Vec3};

fn waiting_scene(names: &[&str], handlers: Vec<HandlerSpec>) -> Scene {
    let mut server = server(PluginSettings {
        decision_ticks: 1_000,
        ..PluginSettings::default()
    });
    let plugin = connect_with(&mut server, 1, "tester", handlers);
    let players: Vec<Player> = names.iter().map(|name| join(&mut server, name)).collect();
    server.deliver();
    for player in &players {
        player.link.take();
    }
    Scene {
        server,
        plugin,
        players,
    }
}

fn spawn(s: &mut Scene, kind: &str, position: Vec3) -> EntityView {
    let Answer::Spawned(view) = s.ask(Question::Spawn {
        kind: kind.into(),
        position,
    }) else {
        panic!("expected the spawned {kind}");
    };
    view
}

fn health(s: &mut Scene, id: EntityId) -> Option<f32> {
    match s.ask(Question::Entity(id)) {
        Answer::Entity(details) => details.health,
        _ => None,
    }
}

fn velocity(s: &mut Scene, id: EntityId) -> Vec3 {
    match s.ask(Question::Entity(id)) {
        Answer::Entity(details) => details.velocity,
        _ => panic!("expected the entity {id:?}"),
    }
}

fn attack(s: &mut Scene, player: &Player, target: EntityId) {
    send_input(&mut s.server, player, PlayerInput::Attack(target));
}

fn cow_beside(s: &mut Scene) -> EntityView {
    spawn(s, "minecraft:cow", Vec3::new(1.5, 64.0, 0.5))
}

fn sounds(player: &Player) -> Vec<&'static str> {
    player
        .link
        .take()
        .into_iter()
        .filter_map(|event| match event {
            ClientEvent::Sound { sound, .. } => Some(sound),
            _ => None,
        })
        .collect()
}

fn alice_entity(s: &mut Scene) -> i32 {
    let Answer::Player(state) = s.ask(Question::Player(s.players[0].id)) else {
        panic!("expected Alice");
    };
    state.player.entity_id
}

fn damage_patch(kind: EventKind, damage: f64) -> EventPatch {
    match kind {
        EventKind::EntityDamageByEntity => {
            EventPatch::EntityDamageByEntity(EntityDamageByEntityPatch {
                damage: Some(damage),
            })
        }
        EventKind::EntityDamageByBlock => {
            EventPatch::EntityDamageByBlock(EntityDamageByBlockPatch {
                damage: Some(damage),
            })
        }
        _ => EventPatch::EntityDamage(EntityDamagePatch {
            damage: Some(damage),
        }),
    }
}

#[test]
fn a_player_hit_reaches_the_damage_and_the_damage_by_entity_handlers() {
    let mut s = scene_with(
        &["Alice"],
        vec![
            observe(1, EventKind::EntityDamage),
            observe(2, EventKind::EntityDamageByEntity),
            observe(3, EventKind::EntityDamageByBlock),
        ],
    );
    let alice = s.players[0].clone();
    let cow = cow_beside(&mut s);
    attack(&mut s, &alice, cow.id);
    let heard: Vec<(EventKind, Vec<u32>)> = s
        .plugin
        .received()
        .into_iter()
        .filter_map(|message| match message {
            ToPlugin::Event {
                handlers, payload, ..
            } => Some((payload.kind(), handlers)),
            _ => None,
        })
        .collect();
    assert_eq!(heard, [(EventKind::EntityDamageByEntity, vec![1, 2])]);
    let entity = alice_entity(&mut s);
    let EventPayload::EntityDamageByEntity(hit) =
        &settled_for(&s.plugin, EventKind::EntityDamageByEntity)[0].payload
    else {
        panic!("a damage by entity");
    };
    assert_eq!(hit.cause, EntityDamageCause::EntityAttack);
    assert_eq!(hit.damager.id, EntityId(entity));
    assert_eq!(hit.damager.kind, "minecraft:player");
    assert_eq!(hit.entity, cow);
    assert_eq!(hit.damage_source.kind, "minecraft:player_attack");
    assert!(hit.damage > 0.0);
    assert_eq!(hit.final_damage, hit.damage);
    assert!(health(&mut s, cow.id).is_some_and(|health| health < 10.0));
}

#[test]
fn a_player_hit_carries_the_attacking_player_and_a_mob_carries_none() {
    let mut s = scene_with(
        &["Alice"],
        vec![observe(1, EventKind::EntityDamageByEntity)],
    );
    let alice = s.players[0].clone();
    let cow = cow_beside(&mut s);
    attack(&mut s, &alice, cow.id);
    let entity = alice_entity(&mut s);
    let EventPayload::EntityDamageByEntity(hit) =
        &settled_for(&s.plugin, EventKind::EntityDamageByEntity)[0].payload
    else {
        panic!("a damage by entity");
    };
    let player = hit.damager.player.as_ref().expect("the attacking player");
    assert_eq!((player.name.as_str(), player.entity_id), ("Alice", entity));
    assert_eq!(
        hit.damage_source
            .causing
            .as_ref()
            .and_then(|causing| causing.player.as_ref()),
        Some(player)
    );
    assert!(hit.entity.player.is_none());
}

#[test]
fn a_cancelled_hit_gives_no_knockback_sound_or_counter_change() {
    let mut s = waiting_scene(&["Alice"], vec![deciding(1, EventKind::EntityDamage)]);
    let alice = s.players[0].clone();
    let cow = cow_beside(&mut s);
    s.tick(1);
    alice.link.take();
    attack(&mut s, &alice, cow.id);
    assert_eq!(health(&mut s, cow.id), Some(10.0));
    let asked = last_asked(&s.plugin, EventKind::EntityDamageByEntity);
    let before = velocity(&mut s, cow.id);
    rule_on(&mut s.server, 1, asked.id, true, None);
    s.server.deliver();
    assert_eq!(health(&mut s, cow.id), Some(10.0));
    assert_eq!(velocity(&mut s, cow.id), before);
    let events = alice.link.take();
    assert!(
        !events
            .iter()
            .any(|event| matches!(event, ClientEvent::DamageEvent { .. }))
    );
    let heard: Vec<&str> = events
        .iter()
        .filter_map(|event| match event {
            ClientEvent::Sound { sound, .. } => Some(*sound),
            _ => None,
        })
        .collect();
    assert_eq!(heard, ["minecraft:entity.player.attack.nodamage"]);
    assert!(
        events
            .iter()
            .any(|event| matches!(event, ClientEvent::InventoryContent { .. }))
    );
}

#[test]
fn a_patched_damage_drives_the_hit_and_its_follow_up() {
    let mut s = waiting_scene(&["Alice"], vec![deciding(1, EventKind::EntityDamage)]);
    let alice = s.players[0].clone();
    let cow = cow_beside(&mut s);
    s.tick(1);
    alice.link.take();
    attack(&mut s, &alice, cow.id);
    assert!(sounds(&alice).is_empty());
    let asked = last_asked(&s.plugin, EventKind::EntityDamageByEntity);
    rule_on(
        &mut s.server,
        1,
        asked.id,
        false,
        Some(damage_patch(EventKind::EntityDamage, 4.0)),
    );
    s.server.deliver();
    assert_eq!(health(&mut s, cow.id), Some(6.0));
    assert_ne!(velocity(&mut s, cow.id), Vec3::ZERO);
    assert!(sounds(&alice).contains(&"minecraft:entity.player.attack.weak"));
}

#[test]
fn the_event_comes_before_the_invulnerability_window_which_a_higher_patch_passes() {
    let mut s = waiting_scene(&["Alice"], vec![deciding(1, EventKind::EntityDamage)]);
    let cow = cow_beside(&mut s);
    let hurt = |s: &mut Scene, amount| {
        s.act(Order::Entity {
            entity: cow.id,
            order: EntityOrder::Damage(amount),
        });
    };
    hurt(&mut s, 2.0);
    let first = last_asked(&s.plugin, EventKind::EntityDamage);
    rule_on(&mut s.server, 1, first.id, false, None);
    assert_eq!(health(&mut s, cow.id), Some(8.0));
    hurt(&mut s, 1.0);
    let second = last_asked(&s.plugin, EventKind::EntityDamage);
    assert_ne!(first.id, second.id);
    let EventPayload::EntityDamage(blocked) = &second.payload else {
        panic!("a plain damage");
    };
    assert_eq!(blocked.cause, EntityDamageCause::Custom);
    assert_eq!(blocked.damage, 1.0);
    rule_on(
        &mut s.server,
        1,
        second.id,
        false,
        Some(damage_patch(EventKind::EntityDamage, 5.0)),
    );
    assert_eq!(health(&mut s, cow.id), Some(5.0));
}

#[test]
fn a_zero_damage_is_still_asked() {
    let mut s = waiting_scene(&["Alice"], vec![deciding(1, EventKind::EntityDamage)]);
    let cow = cow_beside(&mut s);
    s.act(Order::Entity {
        entity: cow.id,
        order: EntityOrder::Damage(0.0),
    });
    let asked = last_asked(&s.plugin, EventKind::EntityDamage);
    rule_on(
        &mut s.server,
        1,
        asked.id,
        false,
        Some(damage_patch(EventKind::EntityDamage, 3.0)),
    );
    assert_eq!(health(&mut s, cow.id), Some(7.0));
}

#[test]
fn the_kill_command_and_the_void_name_their_cause() {
    let mut s = scene_with(
        &["Alice"],
        vec![
            observe(1, EventKind::EntityDamage),
            observe(2, EventKind::EntityDamageByBlock),
        ],
    );
    let cow = cow_beside(&mut s);
    s.server
        .handle(Inbound::Console("kill @e[type=cow]".to_owned()));
    s.server.deliver();
    let EventPayload::EntityDamage(killed) =
        &settled_for(&s.plugin, EventKind::EntityDamage)[0].payload
    else {
        panic!("a plain damage");
    };
    assert_eq!(killed.cause, EntityDamageCause::Kill);
    assert_eq!(killed.entity.id, cow.id);
    let fallen = cow_beside(&mut s);
    s.act(Order::Entity {
        entity: fallen.id,
        order: EntityOrder::Teleport(Vec3::new(1.5, -130.0, 0.5)),
    });
    s.tick(3);
    let EventPayload::EntityDamageByBlock(void) =
        &settled_for(&s.plugin, EventKind::EntityDamageByBlock)[0].payload
    else {
        panic!("a damage by block");
    };
    assert_eq!(void.cause, EntityDamageCause::Void);
    assert_eq!(void.damager, None);
    assert_eq!(void.entity.id, fallen.id);
}

#[test]
fn lava_is_asked_once_while_its_damage_waits_and_names_its_block() {
    let mut s = waiting_scene(
        &["Alice"],
        vec![deciding(1, EventKind::EntityDamageByBlock)],
    );
    let at = Vec3::new(4.5, 64.0, 4.5);
    stream_chunk_of(&mut s, at.block());
    let lava = BlockPos::new(4, 64, 4);
    s.act(Order::Block {
        pos: lava,
        order: BlockOrder::Set("minecraft:lava".into()),
    });
    let zombie = spawn(&mut s, "minecraft:zombie", at);
    s.tick(10);
    let asked = asked_for(&s.plugin, EventKind::EntityDamageByBlock);
    assert_eq!(asked.len(), 1);
    let EventPayload::EntityDamageByBlock(burn) = &asked[0].payload else {
        panic!("a damage by block");
    };
    assert_eq!(burn.cause, EntityDamageCause::Lava);
    assert_eq!(burn.damager, Some(lava));
    assert_eq!(burn.entity.id, zombie.id);
    rule_on(&mut s.server, 1, asked[0].id, true, None);
    assert_eq!(health(&mut s, zombie.id), Some(20.0));
    s.tick(1);
    assert_eq!(
        asked_for(&s.plugin, EventKind::EntityDamageByBlock).len(),
        2
    );
}

#[test]
fn without_a_decider_a_hit_lands_at_once() {
    let mut s = scene_with(&["Alice"], vec![observe(1, EventKind::EntityDamage)]);
    let alice = s.players[0].clone();
    let cow = cow_beside(&mut s);
    attack(&mut s, &alice, cow.id);
    assert!(asked_for(&s.plugin, EventKind::EntityDamageByEntity).is_empty());
    assert!(health(&mut s, cow.id).is_some_and(|health| health < 10.0));
}

#[test]
fn a_difficulty_scaled_damage_never_asks_for_a_player_in_peaceful() {
    let mut s = waiting_scene(&["Alice"], vec![deciding(1, EventKind::EntityDamage)]);
    let alice = EntityId(alice_entity(&mut s));
    s.act(Order::World(WorldOrder::SetDifficulty(
        Difficulty::Peaceful,
    )));
    s.act(Order::World(WorldOrder::Explode {
        position: Vec3::new(1.5, 64.0, 0.5),
        power: 2.0,
        breaks_blocks: false,
    }));
    let asked: Vec<Fired> = fired(&s.plugin)
        .into_iter()
        .filter(|event| match &event.payload {
            EventPayload::EntityDamage(damage) => damage.entity.id == alice,
            EventPayload::EntityDamageByBlock(damage) => damage.entity.id == alice,
            EventPayload::EntityDamageByEntity(damage) => damage.entity.id == alice,
            _ => false,
        })
        .collect();
    assert!(asked.is_empty(), "{asked:?}");
    let Answer::Player(state) = s.ask(Question::Player(s.players[0].id)) else {
        panic!("expected Alice");
    };
    assert_eq!(state.health, 20.0);
}

#[test]
fn a_mob_hit_whose_attacker_died_while_it_waited_lands_nothing() {
    let mut s = waiting_scene(
        &["Alice"],
        vec![deciding(1, EventKind::EntityDamageByEntity)],
    );
    let alice = s.players[0].id;
    let zombie = spawn(&mut s, "minecraft:zombie", Vec3::new(0.5, 64.0, 1.5));
    s.server
        .do_hurt_target(zombie.id, rspigot_core::commands::Target::Player(alice));
    let asked = last_asked(&s.plugin, EventKind::EntityDamageByEntity);
    s.act(Order::Entity {
        entity: zombie.id,
        order: EntityOrder::Kill,
    });
    rule_on(&mut s.server, 1, asked.id, false, None);
    let Answer::Player(state) = s.ask(Question::Player(alice)) else {
        panic!("expected Alice");
    };
    assert_eq!(state.health, 20.0);
}

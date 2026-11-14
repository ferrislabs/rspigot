mod common;

use std::sync::Arc;
use std::time::Duration;

use common::*;
use rspigot_core::config::{GameConfig, LevelData, PluginSettings};
use rspigot_core::events::{
    ClientEvent, EntityKind as WireKind, EquipmentSlot, Inbound, MetadataValue,
};
use rspigot_core::plugins::{EventKind, EventPayload, HandlerSpec};
use rspigot_core::ports::ChunkGenerator;
use rspigot_core::server::Server;
use rspigot_core::status::StatusBoard;
use rspigot_domain::chunk::ChunkColumn;
use rspigot_domain::world::Dimension;
use rspigot_domain::{BiomeId, BlockPos, BlockStateId, ChunkPos, EntityId, Vec3};
use rspigot_entities::EntityKind as Kind;

struct Plains;

impl ChunkGenerator for Plains {
    fn generate(&self, dimension: &Dimension, pos: ChunkPos) -> ChunkColumn {
        let biome = rspigot_data::registries::biome_id("minecraft:plains").unwrap();
        let mut chunk = ChunkColumn::empty(pos, dimension.height, BiomeId(biome));
        for x in 0..16 {
            for z in 0..16 {
                for y in 60..=63 {
                    chunk.set_block(
                        BlockPos::new(pos.min_block_x() + x, y, pos.min_block_z() + z),
                        BlockStateId(1),
                    );
                }
            }
        }
        chunk
    }

    fn spawn_point(&self, _: &Dimension) -> Vec3 {
        Vec3::new(0.5, 64.0, 0.5)
    }
}

fn plains_server(adjust: impl FnOnce(&mut GameConfig)) -> Server<Link> {
    let mut config = GameConfig {
        view_distance: 3,
        chunk_workers: 2,
        spawn_protection: 0,
        plugins: PluginSettings {
            decision_ticks: 1_000,
            ..PluginSettings::default()
        },
        ..GameConfig::default()
    };
    adjust(&mut config);
    let data = LevelData {
        spawn_mobs: false,
        day_time: 18_000,
        daylight_cycle: false,
        ..LevelData::default()
    };
    let mut server = Server::new(
        config,
        data,
        Arc::new(NoStore),
        Arc::new(Plains),
        StatusBoard::new(20),
    )
    .with_rng_seed(7);
    server.wait_for_spawn(Duration::from_secs(10));
    server
}

fn ready(handlers: Vec<HandlerSpec>) -> Scene {
    ready_with(handlers, |_| {})
}

fn ready_with(handlers: Vec<HandlerSpec>, adjust: impl FnOnce(&mut GameConfig)) -> Scene {
    let mut s = scene_on(plains_server(adjust), &["Alice"], handlers);
    stream_chunk_of(&mut s, BlockPos::new(1, 64, 1));
    s
}

fn console(s: &mut Scene, line: &str) {
    s.server.handle(Inbound::Console(line.to_owned()));
    s.server.deliver();
}

fn summon(s: &mut Scene, kind: Kind, nbt: &str, at: Vec3) -> EntityId {
    let before = s.server.entities_of(kind).len();
    let name = kind.name();
    console(
        s,
        &format!("summon {name} {} {} {} {nbt}", at.x, at.y, at.z),
    );
    let found = s.server.entities_of(kind);
    assert_eq!(found.len(), before + 1, "{name} was not summoned");
    found.last().unwrap().id
}

fn snapshot(s: &Scene, kind: Kind, id: EntityId) -> rspigot_core::entities::EntitySnapshot {
    s.server
        .entities_of(kind)
        .into_iter()
        .find(|entity| entity.id == id)
        .expect("the entity exists")
}

const CHARGING: u8 = 17;
const NEAR: Vec3 = Vec3::new(0.5, 64.0, 6.5);

fn flag(events: &[ClientEvent], entity: EntityId, index: u8) -> Option<bool> {
    events.iter().rev().find_map(|event| match event {
        ClientEvent::EntityMetadata {
            entity: found,
            entries,
        } if *found == entity => {
            entries
                .iter()
                .find_map(|entry| match (&entry.value, entry.index == index) {
                    (MetadataValue::Boolean(value), true) => Some(*value),
                    _ => None,
                })
        }
        _ => None,
    })
}

fn main_hand(
    events: &[ClientEvent],
    entity: EntityId,
) -> Vec<Option<rspigot_domain::item::ItemStack>> {
    events
        .iter()
        .filter_map(|event| match event {
            ClientEvent::EntityEquipment {
                entity: found,
                slots,
            } if *found == entity => slots
                .iter()
                .find(|(slot, _)| *slot == EquipmentSlot::MainHand)
                .map(|(_, stack)| stack.clone()),
            _ => None,
        })
        .collect()
}

fn is_charged(stack: &Option<rspigot_domain::item::ItemStack>) -> bool {
    stack.as_ref().is_some_and(|stack| {
        stack
            .components
            .get_list("minecraft:charged_projectiles")
            .is_some_and(|list| !list.is_empty())
    })
}

fn sounds(events: &[ClientEvent]) -> Vec<&'static str> {
    events
        .iter()
        .filter_map(|event| match event {
            ClientEvent::Sound { sound, .. } => Some(*sound),
            _ => None,
        })
        .collect()
}

fn alice_health(events: &[ClientEvent]) -> Option<f32> {
    events.iter().rev().find_map(|event| match event {
        ClientEvent::Health { health, .. } => Some(*health),
        _ => None,
    })
}

fn arrows(events: &[ClientEvent]) -> Vec<(EntityId, Vec3)> {
    events
        .iter()
        .filter_map(|event| match event {
            ClientEvent::SpawnEntity {
                entity,
                kind: WireKind::Other(id),
                position,
                ..
            } if *id == Kind::Arrow.id().0 => Some((*entity, *position)),
            _ => None,
        })
        .collect()
}

fn equipment_of(s: &Scene, id: EntityId, slot: &str) -> Option<rspigot_nbt::Compound> {
    s.server
        .saved_entity(id)?
        .get_compound("equipment")?
        .get_compound(slot)
        .cloned()
}

struct Shot {
    events: Vec<ClientEvent>,
    heard: Vec<(usize, &'static str)>,
    charging: Option<usize>,
    loaded: Option<usize>,
    fired: Option<usize>,
}

fn heard_at(shot: &Shot, sound: &str) -> usize {
    shot.heard
        .iter()
        .find(|(_, heard)| *heard == sound)
        .unwrap_or_else(|| panic!("{sound} was never heard"))
        .0
}

fn watch(s: &mut Scene, pillager: EntityId, ticks: usize) -> Shot {
    let mut shot = Shot {
        events: Vec::new(),
        heard: Vec::new(),
        charging: None,
        loaded: None,
        fired: None,
    };
    for tick in 0..ticks {
        s.tick(1);
        let batch = s.players[0].link.take();
        if shot.charging.is_none() && flag(&batch, pillager, CHARGING) == Some(true) {
            shot.charging = Some(tick);
        }
        if shot.loaded.is_none() && main_hand(&batch, pillager).iter().any(is_charged) {
            shot.loaded = Some(tick);
        }
        if shot.fired.is_none() && !arrows(&batch).is_empty() {
            shot.fired = Some(tick);
        }
        shot.heard
            .extend(sounds(&batch).into_iter().map(|sound| (tick, sound)));
        shot.events.extend(batch);
        if shot.fired.is_some() {
            break;
        }
    }
    shot
}

#[test]
fn a_summoned_pillager_holds_a_crossbow_in_its_main_hand() {
    let mut s = ready(Vec::new());
    let pillager = summon(&mut s, Kind::Pillager, "", NEAR);
    let crossbow = equipment_of(&s, pillager, "mainhand").expect("a crossbow");
    assert_eq!(crossbow.get_str("id"), Some("minecraft:crossbow"));
    assert!(equipment_of(&s, pillager, "offhand").is_none());
}

#[test]
fn a_pillager_charges_for_twenty_five_ticks_then_waits_and_fires_a_crossbow_arrow() {
    let mut s = ready(Vec::new());
    let pillager = summon(&mut s, Kind::Pillager, "", NEAR);
    s.players[0].link.take();
    let shot = watch(&mut s, pillager, 600);
    let charging = shot.charging.expect("the pillager charged");
    let loaded = shot.loaded.expect("the crossbow was loaded");
    let fired = shot.fired.expect("an arrow was fired");
    let start = heard_at(&shot, "minecraft:item.crossbow.loading_start");
    let mid = heard_at(&shot, "minecraft:item.crossbow.loading_middle");
    let end = heard_at(&shot, "minecraft:item.crossbow.loading_end");
    assert_eq!((mid - start, end - start, end), (8, 19, loaded));
    assert!(charging < loaded);
    assert!((21..=41).contains(&(fired - loaded)), "{}", fired - loaded);
    assert_eq!(flag(&shot.events, pillager, CHARGING), Some(false));
    let heard = sounds(&shot.events);
    for sound in [
        "minecraft:item.crossbow.loading_start",
        "minecraft:item.crossbow.loading_middle",
        "minecraft:item.crossbow.loading_end",
        "minecraft:item.crossbow.shoot",
    ] {
        assert_eq!(
            heard.iter().filter(|heard| **heard == sound).count(),
            1,
            "{sound}"
        );
    }
    let hands = main_hand(&shot.events, pillager);
    let last_charged = hands
        .iter()
        .rposition(is_charged)
        .expect("a charged crossbow");
    assert!(
        hands[last_charged + 1..]
            .iter()
            .all(|hand| !is_charged(hand))
    );
    let worn = hands.last().and_then(Option::as_ref).expect("a crossbow");
    assert_eq!(worn.components.get_i32("minecraft:damage"), Some(1));
}

fn crossbow_nbt(enchantments: &str) -> String {
    format!(
        "{{equipment:{{mainhand:{{id:\"minecraft:crossbow\",count:1,components:{{\"minecraft:enchantments\":{enchantments}}}}}}}}}"
    )
}

fn arrows_by_tick(s: &mut Scene, pillager: EntityId, ticks: usize) -> Vec<(usize, EntityId, Vec3)> {
    let mut found = Vec::new();
    for tick in 0..ticks {
        s.tick(1);
        let batch = s.players[0].link.take();
        let _ = pillager;
        for (id, position) in arrows(&batch) {
            found.push((tick, id, position));
        }
        if !found.is_empty() {
            break;
        }
    }
    found
}

#[test]
fn a_crossbow_arrow_leaves_the_pillager_eye_fast_owned_and_unpickable() {
    let mut s = ready(Vec::new());
    let pillager = summon(&mut s, Kind::Pillager, "", NEAR);
    s.players[0].link.take();
    let fired = arrows_by_tick(&mut s, pillager, 600);
    assert_eq!(fired.len(), 1);
    let (_, arrow, position) = fired[0];
    assert!(
        (position.y - (64.0 + 1.62 - 0.1)).abs() < 0.2,
        "{position:?}"
    );
    let speed = snapshot(&s, Kind::Arrow, arrow).velocity.length();
    assert!((1.2..2.0).contains(&speed), "{speed}");
    let saved = s.server.saved_entity(arrow).expect("a saved arrow");
    assert_eq!(
        saved.get("Owner"),
        s.server.saved_entity(pillager).unwrap().get("UUID")
    );
    assert_eq!(
        saved.get_str("SoundEvent"),
        Some("minecraft:item.crossbow.hit")
    );
    assert_eq!(saved.get_i64("pickup").unwrap_or(0), 0);
    assert_eq!(saved.get_i64("PierceLevel").unwrap_or(0), 0);
}

#[test]
fn a_crossbow_arrow_hurts_a_player_by_its_speed_times_two_and_rings_the_crossbow_hit() {
    let mut s = ready(Vec::new());
    summon(&mut s, Kind::Pillager, "", NEAR);
    s.players[0].link.take();
    let mut hurt = None;
    let mut heard = Vec::new();
    for _ in 0..600 {
        s.tick(1);
        let batch = s.players[0].link.take();
        heard.extend(sounds(&batch));
        if let Some(health) = alice_health(&batch).filter(|health| *health < 20.0) {
            hurt = Some(health);
            break;
        }
    }
    let hurt = hurt.expect("the player was hurt");
    assert!((2.5..=4.0).contains(&(20.0 - hurt)), "{hurt}");
    assert!(heard.contains(&"minecraft:item.crossbow.hit"), "{heard:?}");
}

#[test]
fn a_piercing_crossbow_fires_a_piercing_arrow() {
    let mut s = ready(Vec::new());
    let pillager = summon(
        &mut s,
        Kind::Pillager,
        &crossbow_nbt("{\"minecraft:piercing\":4}"),
        NEAR,
    );
    s.players[0].link.take();
    let fired = arrows_by_tick(&mut s, pillager, 600);
    assert_eq!(fired.len(), 1);
    let saved = s.server.saved_entity(fired[0].1).unwrap();
    assert_eq!(saved.get_i64("PierceLevel"), Some(4));
}

#[test]
fn a_multishot_crossbow_loads_and_fires_three_arrows_fanned_around_the_aim() {
    let mut s = ready(Vec::new());
    let pillager = summon(
        &mut s,
        Kind::Pillager,
        &crossbow_nbt("{\"minecraft:multishot\":1}"),
        NEAR,
    );
    s.players[0].link.take();
    let mut events = Vec::new();
    let mut fired = Vec::new();
    for _ in 0..600 {
        s.tick(1);
        let batch = s.players[0].link.take();
        fired = arrows(&batch);
        events.extend(batch);
        if !fired.is_empty() {
            break;
        }
    }
    assert_eq!(fired.len(), 3);
    let loaded = main_hand(&events, pillager)
        .into_iter()
        .flatten()
        .find(|stack| {
            stack
                .components
                .get_list("minecraft:charged_projectiles")
                .is_some()
        })
        .expect("a loaded crossbow");
    let list = loaded
        .components
        .get_list("minecraft:charged_projectiles")
        .unwrap();
    assert_eq!(list.len(), 3);
    let tangible = list
        .iter()
        .filter(|tag| {
            tag.as_compound()
                .and_then(|stack| stack.get_compound("components"))
                .is_none_or(|components| {
                    !components.contains_key("minecraft:intangible_projectile")
                })
        })
        .count();
    assert_eq!(tangible, 1);
    let pitches: Vec<f32> = events
        .iter()
        .filter_map(|event| match event {
            ClientEvent::Sound { sound, pitch, .. }
                if *sound == "minecraft:item.crossbow.shoot" =>
            {
                Some(*pitch)
            }
            _ => None,
        })
        .collect();
    assert_eq!(pitches.len(), 3);
    assert_eq!(pitches[0], 1.0);
    assert!(
        (1.0 / 2.3 + 0.63..=1.0 / 1.8 + 0.63).contains(&pitches[1]),
        "{pitches:?}"
    );
    assert!(
        (1.0 / 2.3 + 0.43..=1.0 / 1.8 + 0.43).contains(&pitches[2]),
        "{pitches:?}"
    );
    let velocities: Vec<Vec3> = fired
        .iter()
        .map(|(id, _)| snapshot(&s, Kind::Arrow, *id).velocity.normalize())
        .collect();
    let mut sideways: Vec<f64> = velocities.iter().map(|v| v.x).collect();
    sideways.sort_by(f64::total_cmp);
    assert!(sideways[2] - sideways[0] > 0.1, "{sideways:?}");
}

#[test]
fn a_quick_charge_crossbow_loads_sooner_with_its_own_charging_sound() {
    let mut s = ready(Vec::new());
    let pillager = summon(
        &mut s,
        Kind::Pillager,
        &crossbow_nbt("{\"minecraft:quick_charge\":3}"),
        NEAR,
    );
    s.players[0].link.take();
    let shot = watch(&mut s, pillager, 600);
    let start = heard_at(&shot, "minecraft:item.crossbow.quick_charge_3");
    let end = heard_at(&shot, "minecraft:item.crossbow.loading_end");
    assert_eq!(end - start, 7);
    assert_eq!(Some(end), shot.loaded);
    assert!(shot.fired.is_some());
}

#[test]
fn arrows_held_in_the_off_hand_are_spent_one_per_load() {
    let mut s = ready(Vec::new());
    let pillager = summon(
        &mut s,
        Kind::Pillager,
        "{equipment:{mainhand:{id:\"minecraft:crossbow\",count:1},offhand:{id:\"minecraft:arrow\",count:3}}}",
        NEAR,
    );
    s.players[0].link.take();
    let shot = watch(&mut s, pillager, 600);
    assert!(shot.fired.is_some());
    let offhand = equipment_of(&s, pillager, "offhand").expect("arrows are left");
    assert_eq!(offhand.get_i32("count"), Some(2));
}

#[test]
fn the_last_held_arrow_empties_the_off_hand() {
    let mut s = ready(Vec::new());
    let pillager = summon(
        &mut s,
        Kind::Pillager,
        "{equipment:{mainhand:{id:\"minecraft:crossbow\",count:1},offhand:{id:\"minecraft:arrow\",count:1}}}",
        NEAR,
    );
    s.players[0].link.take();
    let shot = watch(&mut s, pillager, 600);
    assert!(shot.fired.is_some());
    assert!(equipment_of(&s, pillager, "offhand").is_none());
}

#[test]
fn a_pillager_with_the_crossbow_in_its_off_hand_charges_and_fires_it() {
    let mut s = ready(Vec::new());
    let pillager = summon(
        &mut s,
        Kind::Pillager,
        "{equipment:{offhand:{id:\"minecraft:crossbow\",count:1}}}",
        NEAR,
    );
    s.players[0].link.take();
    let shot = watch(&mut s, pillager, 600);
    assert!(shot.charging.is_some());
    assert!(shot.fired.is_some());
    let worn = equipment_of(&s, pillager, "offhand").expect("a crossbow");
    assert_eq!(
        worn.get_compound("components")
            .and_then(|c| c.get_i32("minecraft:damage")),
        Some(1)
    );
}

#[test]
fn a_pillager_without_a_crossbow_never_charges() {
    let mut s = ready(Vec::new());
    let pillager = summon(&mut s, Kind::Pillager, "{}", NEAR);
    s.players[0].link.take();
    let shot = watch(&mut s, pillager, 200);
    assert!(shot.charging.is_none() && shot.fired.is_none());
}

#[test]
fn a_shoot_bow_event_describes_the_crossbow_shot_and_a_cancel_spares_the_arrow_and_the_wear() {
    let mut s = ready(vec![deciding(1, EventKind::EntityShootBow)]);
    let pillager = summon(&mut s, Kind::Pillager, "", NEAR);
    let mut heard = Vec::new();
    for _ in 0..600 {
        s.tick(1);
        heard.extend(sounds(&s.players[0].link.take()));
        if !asked_for(&s.plugin, EventKind::EntityShootBow).is_empty() {
            break;
        }
    }
    let asked = last_asked(&s.plugin, EventKind::EntityShootBow);
    let EventPayload::EntityShootBow(shot) = &asked.payload else {
        panic!("a shoot bow event");
    };
    assert_eq!(shot.entity_type, "minecraft:pillager");
    assert_eq!(shot.force, 1.6);
    assert_eq!(
        shot.hand,
        rspigot_core::plugins::events::EquipmentSlot::Hand
    );
    assert_eq!(
        shot.bow.as_ref().map(|bow| bow.key.as_str()),
        Some("minecraft:crossbow")
    );
    assert_eq!(
        shot.consumable.as_ref().map(|ammo| ammo.key.as_str()),
        Some("minecraft:arrow")
    );
    assert_eq!(shot.projectile.kind, "minecraft:arrow");
    assert!(heard.contains(&"minecraft:item.crossbow.shoot"));
    rule_on(&mut s.server, 1, asked.id, true, None);
    s.tick(2);
    assert!(s.server.entities_of(Kind::Arrow).is_empty());
    let kept = equipment_of(&s, pillager, "mainhand").expect("a crossbow");
    assert!(
        kept.get_compound("components")
            .is_none_or(|c| c.get_i32("minecraft:damage").is_none())
    );
}

#[test]
fn an_allowed_shoot_bow_event_launches_the_arrow_and_wears_the_crossbow() {
    let mut s = ready(vec![deciding(1, EventKind::EntityShootBow)]);
    let pillager = summon(&mut s, Kind::Pillager, "", NEAR);
    for _ in 0..600 {
        s.tick(1);
        if !asked_for(&s.plugin, EventKind::EntityShootBow).is_empty() {
            break;
        }
    }
    let asked = last_asked(&s.plugin, EventKind::EntityShootBow);
    rule_on(&mut s.server, 1, asked.id, false, None);
    s.tick(1);
    assert_eq!(s.server.entities_of(Kind::Arrow).len(), 1);
    let worn = equipment_of(&s, pillager, "mainhand").expect("a crossbow");
    assert_eq!(
        worn.get_compound("components")
            .and_then(|c| c.get_i32("minecraft:damage")),
        Some(1)
    );
}

#[test]
fn the_launch_of_a_crossbow_arrow_is_a_projectile_launch() {
    let mut s = ready(vec![deciding(1, EventKind::ProjectileLaunch)]);
    summon(&mut s, Kind::Pillager, "", NEAR);
    for _ in 0..600 {
        s.tick(1);
        if !asked_for(&s.plugin, EventKind::ProjectileLaunch).is_empty() {
            break;
        }
    }
    let asked = last_asked(&s.plugin, EventKind::ProjectileLaunch);
    let EventPayload::ProjectileLaunch(launch) = &asked.payload else {
        panic!("a projectile launch");
    };
    assert_eq!(launch.entity_type, "minecraft:arrow");
    rule_on(&mut s.server, 1, asked.id, true, None);
    s.tick(1);
    assert!(s.server.entities_of(Kind::Arrow).is_empty());
}

#[test]
fn a_killed_pillager_drops_a_guaranteed_crossbow_and_no_ominous_bottle() {
    let mut s = ready(Vec::new());
    summon(
        &mut s,
        Kind::Pillager,
        "{NoAI:1b,drop_chances:{mainhand:2.0f},equipment:{mainhand:{id:\"minecraft:crossbow\",count:1}}}",
        Vec3::new(30.5, 64.0, 30.5),
    );
    console(&mut s, "kill @e[type=pillager]");
    s.tick(2);
    let items: Vec<String> = s
        .server
        .entities_of(Kind::Item)
        .into_iter()
        .filter_map(|item| {
            s.server
                .saved_entity(item.id)?
                .get_compound("Item")?
                .get_str("id")
                .map(str::to_owned)
        })
        .collect();
    assert_eq!(items, vec!["minecraft:crossbow".to_owned()]);
}

fn patrol_nbt(leader: bool, target: &str) -> String {
    format!(
        "{{Patrolling:1b,PatrolLeader:{}b,patrol_target:[I;{target}],CanJoinRaid:1b,equipment:{{mainhand:{{id:\"minecraft:crossbow\",count:1}}}}}}",
        i32::from(leader)
    )
}

#[test]
fn a_patrol_walks_toward_its_target_and_does_not_despawn() {
    let mut s = ready(Vec::new());
    console(&mut s, "gamemode creative Alice");
    let leader = summon(
        &mut s,
        Kind::Pillager,
        &patrol_nbt(true, "140,64,30"),
        Vec3::new(6.5, 64.0, 6.5),
    );
    let follower = summon(
        &mut s,
        Kind::Pillager,
        &patrol_nbt(false, "140,64,30"),
        Vec3::new(8.5, 64.0, 6.5),
    );
    let goal = Vec3::new(140.5, 64.0, 30.5);
    let gap = |s: &Scene, id| {
        let position = snapshot(s, Kind::Pillager, id).position;
        Vec3::new(position.x - goal.x, 0.0, position.z - goal.z).length()
    };
    let before = (gap(&s, leader), gap(&s, follower));
    s.tick(260);
    let after = (gap(&s, leader), gap(&s, follower));
    assert!(after.0 < before.0 - 3.0, "{before:?} {after:?}");
    assert!(after.1 < before.1 - 3.0, "{before:?} {after:?}");
    assert_eq!(s.server.entities_of(Kind::Pillager).len(), 2);
}

#[test]
fn a_patrolling_pillager_holds_its_ground_and_shouts_until_the_player_comes_close() {
    let mut s = ready(Vec::new());
    let alice = s.server.player_entity_id(s.players[0].id);
    console(&mut s, "tp Alice 0.5 64 22.5");
    let first = summon(
        &mut s,
        Kind::Pillager,
        &patrol_nbt(true, "0,64,40"),
        Vec3::new(0.5, 64.0, 8.5),
    );
    let second = summon(
        &mut s,
        Kind::Pillager,
        &patrol_nbt(false, "0,64,40"),
        Vec3::new(2.5, 64.0, 8.5),
    );
    s.players[0].link.take();
    let mut charged = false;
    for _ in 0..80 {
        s.tick(1);
        let batch = s.players[0].link.take();
        charged |= flag(&batch, first, CHARGING) == Some(true);
        charged |= flag(&batch, second, CHARGING) == Some(true);
    }
    assert!(
        !charged,
        "a holding patrol does not shoot at fifteen blocks"
    );
    assert_eq!(s.server.mob_target(first), alice);
    assert_eq!(s.server.mob_target(second), alice);
    console(&mut s, "tp Alice 0.5 64 14.5");
    s.players[0].link.take();
    for _ in 0..200 {
        s.tick(1);
        let batch = s.players[0].link.take();
        charged |= flag(&batch, first, CHARGING) == Some(true);
        charged |= flag(&batch, second, CHARGING) == Some(true);
        if charged {
            break;
        }
    }
    assert!(
        charged,
        "the patrol fires once the player is within ten blocks"
    );
}

#[test]
fn a_crossbow_with_one_use_left_breaks_on_its_last_shot() {
    let mut s = ready(Vec::new());
    let pillager = summon(
        &mut s,
        Kind::Pillager,
        "{equipment:{mainhand:{id:\"minecraft:crossbow\",count:1,components:{\"minecraft:damage\":464}}}}",
        NEAR,
    );
    s.players[0].link.take();
    let shot = watch(&mut s, pillager, 600);
    assert!(shot.fired.is_some());
    s.tick(1);
    assert!(equipment_of(&s, pillager, "mainhand").is_none());
    let hands = main_hand(&shot.events, pillager);
    assert!(hands.last().is_some_and(Option::is_none));
    assert!(shot.events.iter().any(|event| matches!(
        event,
        ClientEvent::EntityStatus { entity, status: rspigot_core::events::EntityStatus::EquipmentBreak(EquipmentSlot::MainHand) }
            if *entity == pillager
    )));
}

mod common;

use common::*;
use rspigot_core::events::Inbound;
use rspigot_domain::{BlockPos, EntityId};
use rspigot_entities::EntityKind as Kind;
use rspigot_nbt::Compound;

fn ready_at(day_time: i64) -> Scene {
    let server = configured_level(|_| {}, |level| level.day_time = day_time).with_rng_seed(3);
    let mut s = scene_on(server, &["Alice"], Vec::new());
    stream_chunk_of(&mut s, BlockPos::new(1, 64, 1));
    s.tick(5);
    s
}

fn ready_spawning() -> Scene {
    let server = configured_level(
        |config| config.view_distance = 6,
        |level| {
            level.day_time = 6_000;
            level.spawn_mobs = true;
            level.daylight_cycle = false;
        },
    )
    .with_rng_seed(4);
    let mut s = scene_on(server, &["Alice"], Vec::new());
    stream_chunk_of(&mut s, BlockPos::new(1, 64, 1));
    console(&mut s, "forceload add -64 -64 64 64");
    s.tick(60);
    s
}

fn ready() -> Scene {
    ready_at(18_000)
}

fn console(s: &mut Scene, line: &str) {
    s.server.handle(Inbound::Console(line.to_owned()));
    s.server.deliver();
}

fn summon(s: &mut Scene, kind: Kind, at: &str, nbt: &str) -> EntityId {
    let known: Vec<EntityId> = s
        .server
        .entities_of(kind)
        .into_iter()
        .map(|entity| entity.id)
        .collect();
    console(s, &format!("summon {} {at} {nbt}", kind.name()));
    s.server
        .entities_of(kind)
        .into_iter()
        .map(|entity| entity.id)
        .find(|id| !known.contains(id))
        .expect("a new entity")
}

fn saved(s: &Scene, id: EntityId) -> Compound {
    s.server.saved_entity(id).expect("a saved entity")
}

fn effects_of(s: &Scene, id: EntityId) -> Vec<(String, i32)> {
    saved(s, id)
        .get_list("active_effects")
        .map(|list| {
            list.iter()
                .filter_map(|tag| {
                    let compound = tag.as_compound()?;
                    Some((
                        compound.get_str("id")?.to_owned(),
                        compound.get_i32("duration").unwrap_or(0),
                    ))
                })
                .collect()
        })
        .unwrap_or_default()
}

fn has_effect(s: &Scene, id: EntityId, name: &str) -> bool {
    effects_of(s, id).iter().any(|(effect, _)| effect == name)
}

const TIPPED_SLOWNESS: &str = "{item:{id:\"minecraft:tipped_arrow\",count:1,components:{\"minecraft:potion_contents\":{potion:\"minecraft:slowness\"}}},pickup:0b,damage:2.0d}";

#[test]
fn a_tipped_arrow_applies_its_potion_to_the_mob_it_hits() {
    let mut s = ready();
    let zombie = summon(&mut s, Kind::Zombie, "6.5 64 12.5", "{NoAI:1b,Silent:1b}");
    summon(
        &mut s,
        Kind::Arrow,
        "6.5 65 8.5",
        &format!("{{Motion:[0.0d,0.0d,2.0d],{}", &TIPPED_SLOWNESS[1..]),
    );
    s.tick(6);
    assert!(
        has_effect(&s, zombie, "minecraft:slowness"),
        "{:?}",
        effects_of(&s, zombie)
    );
}

#[test]
fn a_tipped_arrow_carries_the_potion_colour_in_its_metadata() {
    let mut s = ready();
    let arrow = summon(
        &mut s,
        Kind::Arrow,
        "6.5 70 8.5",
        &format!("{{NoGravity:1b,{}", &TIPPED_SLOWNESS[1..]),
    );
    s.tick(2);
    assert_eq!(
        s.server.entity_data_int(arrow, 11),
        Some(
            rspigot_domain::potion::effects_color(
                &rspigot_domain::potion::effects_of("minecraft:slowness")
                    .unwrap()
                    .iter()
                    .filter_map(|e| e.instance())
                    .collect::<Vec<_>>()
            )
            .unwrap()
        )
    );
}

fn health(s: &Scene, kind: Kind, id: EntityId) -> f32 {
    s.server
        .entities_of(kind)
        .into_iter()
        .find(|entity| entity.id == id)
        .and_then(|entity| entity.health)
        .expect("a living entity")
}

#[test]
fn a_piercing_arrow_goes_through_level_plus_one_mobs_and_stays_in_flight() {
    let mut s = ready();
    let near = summon(&mut s, Kind::Zombie, "6.5 64 11.5", "{NoAI:1b,Silent:1b}");
    let middle = summon(&mut s, Kind::Zombie, "6.5 64 13.5", "{NoAI:1b,Silent:1b}");
    let far = summon(&mut s, Kind::Zombie, "6.5 64 15.5", "{NoAI:1b,Silent:1b}");
    summon(
        &mut s,
        Kind::Arrow,
        "6.5 65 8.5",
        "{Motion:[0.0d,0.0d,2.0d],PierceLevel:1b,NoGravity:1b,damage:2.0d}",
    );
    s.tick(12);
    assert!(health(&s, Kind::Zombie, near) < 20.0);
    assert!(health(&s, Kind::Zombie, middle) < 20.0);
    assert_eq!(health(&s, Kind::Zombie, far), 20.0);
    assert!(s.server.entities_of(Kind::Arrow).is_empty());
}

#[test]
fn a_plain_arrow_stops_at_the_first_mob() {
    let mut s = ready();
    let near = summon(&mut s, Kind::Zombie, "6.5 64 11.5", "{NoAI:1b,Silent:1b}");
    let far = summon(&mut s, Kind::Zombie, "6.5 64 13.5", "{NoAI:1b,Silent:1b}");
    summon(
        &mut s,
        Kind::Arrow,
        "6.5 65 8.5",
        "{Motion:[0.0d,0.0d,2.0d],NoGravity:1b,damage:2.0d}",
    );
    s.tick(12);
    assert!(health(&s, Kind::Zombie, near) < 20.0);
    assert_eq!(health(&s, Kind::Zombie, far), 20.0);
}

#[test]
fn a_spectral_arrow_makes_the_mob_it_hits_glow_for_200_ticks() {
    let mut s = ready();
    let zombie = summon(&mut s, Kind::Zombie, "6.5 64 12.5", "{NoAI:1b,Silent:1b}");
    summon(
        &mut s,
        Kind::SpectralArrow,
        "6.5 65 8.5",
        "{Motion:[0.0d,0.0d,2.0d],NoGravity:1b,damage:2.0d}",
    );
    s.tick(10);
    let glowing: Vec<_> = effects_of(&s, zombie)
        .into_iter()
        .filter(|(name, _)| name == "minecraft:glowing")
        .collect();
    assert_eq!(glowing.len(), 1);
    assert!(glowing[0].1 > 150 && glowing[0].1 <= 200, "{glowing:?}");
}

fn fired_arrow_of(s: &mut Scene, kind: Kind) -> Option<Compound> {
    summon(
        s,
        kind,
        "7.5 64 0.5",
        "{Silent:1b,equipment:{mainhand:{id:\"minecraft:bow\",count:1}}}",
    );
    for _ in 0..300 {
        s.tick(1);
        if let Some(arrow) = s.server.entities_of(Kind::Arrow).first() {
            return Some(saved(s, arrow.id));
        }
    }
    None
}

fn arrow_effects(arrow: &Compound) -> Vec<(String, i32)> {
    arrow
        .get_compound("item")
        .and_then(|item| item.get_compound("components"))
        .and_then(|components| components.get_compound("minecraft:potion_contents"))
        .and_then(|contents| contents.get_list("custom_effects"))
        .map(|list| {
            list.iter()
                .filter_map(|tag| {
                    let effect = tag.as_compound()?;
                    Some((
                        effect.get_str("id")?.to_owned(),
                        effect.get_i32("duration")?,
                    ))
                })
                .collect()
        })
        .unwrap_or_default()
}

#[test]
fn a_stray_shoots_arrows_with_slowness_600() {
    let mut s = ready();
    let arrow = fired_arrow_of(&mut s, Kind::Stray).expect("an arrow");
    assert_eq!(
        arrow_effects(&arrow),
        vec![("minecraft:slowness".to_owned(), 600)]
    );
}

#[test]
fn a_bogged_shoots_arrows_with_poison_100() {
    let mut s = ready();
    let arrow = fired_arrow_of(&mut s, Kind::Bogged).expect("an arrow");
    assert_eq!(
        arrow_effects(&arrow),
        vec![("minecraft:poison".to_owned(), 100)]
    );
}

#[test]
fn a_parched_shoots_arrows_with_weakness_600() {
    let mut s = ready();
    let arrow = fired_arrow_of(&mut s, Kind::Parched).expect("an arrow");
    assert_eq!(
        arrow_effects(&arrow),
        vec![("minecraft:weakness".to_owned(), 600)]
    );
}

#[test]
fn a_wither_skeleton_arrow_is_on_fire() {
    let mut s = ready();
    let arrow = fired_arrow_of(&mut s, Kind::WitherSkeleton).expect("an arrow");
    assert!(arrow.get_i32("Fire").unwrap_or(0) > 0 || arrow.get_i64("Fire").unwrap_or(0) > 0);
}

fn effects_applied_to_alice(s: &Scene) -> Vec<(String, u32)> {
    s.players[0]
        .link
        .take()
        .into_iter()
        .filter_map(|event| match event {
            rspigot_core::events::ClientEvent::EffectApplied { effect, .. } => Some((
                effect.effect.name().unwrap_or_default().to_owned(),
                match effect.duration {
                    rspigot_domain::effect::EffectDuration::Ticks(ticks) => ticks,
                    rspigot_domain::effect::EffectDuration::Infinite => u32::MAX,
                },
            )),
            _ => None,
        })
        .collect()
}

fn melee_on_alice(s: &mut Scene, kind: Kind, nbt: &str) -> Vec<(String, u32)> {
    summon(s, kind, "1.5 64 0.5", nbt);
    let mut seen = Vec::new();
    for _ in 0..200 {
        s.tick(1);
        seen.extend(effects_applied_to_alice(s));
        if !seen.is_empty() {
            break;
        }
    }
    seen
}

#[test]
fn a_wither_skeleton_hit_gives_the_target_wither_for_200_ticks() {
    let mut s = ready();
    let seen = melee_on_alice(
        &mut s,
        Kind::WitherSkeleton,
        "{equipment:{mainhand:{id:\"minecraft:stone_sword\",count:1}}}",
    );
    assert!(
        seen.iter()
            .any(|(name, ticks)| name == "minecraft:wither" && *ticks == 200),
        "{seen:?}"
    );
}

#[test]
fn a_cave_spider_bite_poisons_for_seven_seconds_on_normal() {
    let mut s = ready();
    console(&mut s, "difficulty normal");
    let seen = melee_on_alice(&mut s, Kind::CaveSpider, "{}");
    assert!(
        seen.iter()
            .any(|(name, ticks)| name == "minecraft:poison" && *ticks == 140),
        "{seen:?}"
    );
}

#[test]
fn a_cave_spider_bite_poisons_for_fifteen_seconds_on_hard() {
    let mut s = ready();
    console(&mut s, "difficulty hard");
    let seen = melee_on_alice(&mut s, Kind::CaveSpider, "{}");
    assert!(
        seen.iter()
            .any(|(name, ticks)| name == "minecraft:poison" && *ticks == 300),
        "{seen:?}"
    );
}

#[test]
fn a_cave_spider_bite_does_not_poison_on_easy() {
    let mut s = ready();
    console(&mut s, "difficulty easy");
    summon(&mut s, Kind::CaveSpider, "1.5 64 0.5", "{}");
    let mut seen = Vec::new();
    for _ in 0..120 {
        s.tick(1);
        seen.extend(effects_applied_to_alice(&s));
    }
    assert!(seen.is_empty(), "{seen:?}");
}

fn killed_by_wither(s: &mut Scene, ground: &str) -> BlockPos {
    console(s, &format!("setblock 6 63 6 {ground}"));
    summon(s, Kind::Wither, "3.5 66 3.5", "{NoAI:1b,Silent:1b,Invul:0}");
    summon(s, Kind::Zombie, "6.5 64 6.5", "{NoAI:1b,Silent:1b}");
    console(
        s,
        "damage @e[type=zombie,limit=1] 100 minecraft:mob_attack by @e[type=wither,limit=1]",
    );
    s.tick(2);
    BlockPos::new(6, 64, 6)
}

#[test]
fn a_mob_killed_by_the_wither_leaves_a_wither_rose_block_on_dirt() {
    let mut s = ready();
    let at = killed_by_wither(&mut s, "minecraft:dirt");
    assert_eq!(
        s.server.block_state_at(at),
        Some(rspigot_blocks::BlockKind::WitherRose.default_state())
    );
}

#[test]
fn a_mob_killed_by_the_wither_drops_the_rose_item_when_it_cannot_stand() {
    let mut s = ready();
    let at = killed_by_wither(&mut s, "minecraft:stone");
    assert_ne!(
        s.server.block_state_at(at),
        Some(rspigot_blocks::BlockKind::WitherRose.default_state())
    );
    s.tick(2);
    assert!(s.server.entities_of(Kind::Item).iter().any(|item| {
        s.server.saved_entity(item.id).is_some_and(|nbt| {
            nbt.get_compound("Item")
                .and_then(|stack| stack.get_str("id"))
                == Some("minecraft:wither_rose")
        })
    }));
}

#[test]
fn a_mob_not_killed_by_the_wither_leaves_no_rose() {
    let mut s = ready();
    console(&mut s, "setblock 6 63 6 minecraft:dirt");
    summon(&mut s, Kind::Zombie, "6.5 64 6.5", "{NoAI:1b,Silent:1b}");
    console(&mut s, "kill @e[type=zombie,limit=1]");
    s.tick(2);
    assert_ne!(
        s.server.block_state_at(BlockPos::new(6, 64, 6)),
        Some(rspigot_blocks::BlockKind::WitherRose.default_state())
    );
}

const BANNER: &str = "{id:\"minecraft:white_banner\",count:1,components:{\"minecraft:banner_patterns\":[{pattern:\"minecraft:rhombus\",color:\"cyan\"},{pattern:\"minecraft:stripe_bottom\",color:\"light_gray\"},{pattern:\"minecraft:stripe_center\",color:\"gray\"},{pattern:\"minecraft:border\",color:\"light_gray\"},{pattern:\"minecraft:stripe_middle\",color:\"black\"},{pattern:\"minecraft:half_horizontal\",color:\"light_gray\"},{pattern:\"minecraft:circle\",color:\"light_gray\"},{pattern:\"minecraft:border\",color:\"black\"}]}}";

fn item_ids(s: &Scene) -> Vec<String> {
    s.server
        .entities_of(Kind::Item)
        .iter()
        .filter_map(|item| {
            s.server
                .saved_entity(item.id)?
                .get_compound("Item")?
                .get_str("id")
                .map(str::to_owned)
        })
        .collect()
}

fn pillager_dies(s: &mut Scene, nbt: &str) -> Vec<String> {
    summon(s, Kind::Pillager, "6.5 64 6.5", nbt);
    console(s, "kill @e[type=pillager,limit=1]");
    s.tick(30);
    item_ids(s)
}

#[test]
fn a_captain_pillager_drops_the_ominous_bottle() {
    let mut s = ready();
    let items = pillager_dies(
        &mut s,
        &format!("{{NoAI:1b,PatrolLeader:1b,equipment:{{head:{BANNER}}}}}"),
    );
    assert!(
        items.contains(&"minecraft:ominous_bottle".to_owned()),
        "{items:?}"
    );
}

#[test]
fn a_pillager_without_the_banner_or_the_leader_flag_drops_no_ominous_bottle() {
    let mut s = ready();
    let items = pillager_dies(&mut s, "{NoAI:1b,PatrolLeader:1b}");
    assert!(
        !items.contains(&"minecraft:ominous_bottle".to_owned()),
        "{items:?}"
    );
    let mut s = ready();
    let items = pillager_dies(&mut s, &format!("{{NoAI:1b,equipment:{{head:{BANNER}}}}}"));
    assert!(
        !items.contains(&"minecraft:ominous_bottle".to_owned()),
        "{items:?}"
    );
}

#[test]
fn a_villager_struck_by_lightning_becomes_a_persistent_witch() {
    let mut s = ready();
    console(&mut s, "difficulty normal");
    summon(&mut s, Kind::Villager, "6.5 64 6.5", "{NoAI:1b,Silent:1b}");
    console(&mut s, "summon minecraft:lightning_bolt 6.5 64 6.5");
    s.tick(3);
    assert!(s.server.entities_of(Kind::Villager).is_empty());
    let witches = s.server.entities_of(Kind::Witch);
    assert_eq!(witches.len(), 1);
    let nbt = saved(&s, witches[0].id);
    assert_eq!(nbt.get_bool("PersistenceRequired"), Some(true));
    assert!(
        nbt.get_compound("equipment")
            .is_none_or(|e| e.get_compound("mainhand").is_none())
    );
}

#[test]
fn a_villager_struck_by_lightning_in_peaceful_stays_a_villager() {
    let mut s = ready();
    console(&mut s, "difficulty peaceful");
    summon(&mut s, Kind::Villager, "6.5 64 6.5", "{NoAI:1b,Silent:1b}");
    console(&mut s, "summon minecraft:lightning_bolt 6.5 64 6.5");
    s.tick(3);
    assert_eq!(s.server.entities_of(Kind::Villager).len(), 1);
    assert!(s.server.entities_of(Kind::Witch).is_empty());
}

#[test]
fn a_breeze_deflects_an_arrow_instead_of_being_hurt() {
    let mut s = ready();
    let breeze = summon(&mut s, Kind::Breeze, "6.5 64 12.5", "{NoAI:1b,Silent:1b}");
    let arrow = summon(
        &mut s,
        Kind::Arrow,
        "6.5 65 8.5",
        "{Motion:[0.0d,0.0d,2.0d],NoGravity:1b,damage:2.0d}",
    );
    let full = health(&s, Kind::Breeze, breeze);
    s.tick(8);
    assert_eq!(health(&s, Kind::Breeze, breeze), full);
    let arrows = s.server.entities_of(Kind::Arrow);
    assert_eq!(arrows.len(), 1);
    assert_eq!(arrows[0].id, arrow);
    assert!(arrows[0].velocity.z < 0.0, "{:?}", arrows[0].velocity);
}

#[test]
fn a_breeze_does_not_deflect_a_wind_charge() {
    let mut s = ready();
    summon(&mut s, Kind::Breeze, "6.5 64 12.5", "{NoAI:1b,Silent:1b}");
    summon(
        &mut s,
        Kind::WindCharge,
        "6.5 65 8.5",
        "{Motion:[0.0d,0.0d,0.5d]}",
    );
    s.tick(20);
    assert!(s.server.entities_of(Kind::WindCharge).is_empty());
}

fn play_disc(s: &mut Scene, at: BlockPos) {
    use rspigot_core::events::{Direction, Hand, PlayerInput};
    console(
        s,
        &format!("setblock {} {} {} minecraft:jukebox", at.x, at.y, at.z),
    );
    console(s, "give Alice minecraft:music_disc_13 1");
    let alice = s.players[0].clone();
    s.tick(2);
    send_input(
        &mut s.server,
        &alice,
        PlayerInput::UseItemOn {
            hand: Hand::Main,
            pos: at,
            face: Direction::Up,
            cursor: rspigot_domain::Vec3::new(0.5, 1.0, 0.5),
            inside_block: false,
            sequence: 1,
        },
    );
}

const ALLAY_DANCING: u8 = 16;

#[test]
fn an_allay_near_a_playing_jukebox_dances() {
    let mut s = ready();
    console(&mut s, "gamemode survival Alice");
    let allay = summon(&mut s, Kind::Allay, "5.5 65 3.5", "{NoAI:1b}");
    assert_eq!(s.server.entity_data_bool(allay, ALLAY_DANCING), Some(false));
    play_disc(&mut s, BlockPos::new(3, 64, 3));
    s.tick(40);
    assert_eq!(s.server.entity_data_bool(allay, ALLAY_DANCING), Some(true));
}

#[test]
fn an_allay_far_from_the_jukebox_does_not_dance() {
    let mut s = ready();
    console(&mut s, "gamemode survival Alice");
    let allay = summon(&mut s, Kind::Allay, "5.5 65 40.5", "{NoAI:1b}");
    play_disc(&mut s, BlockPos::new(3, 64, 3));
    s.tick(40);
    assert_eq!(s.server.entity_data_bool(allay, ALLAY_DANCING), Some(false));
}

fn rails(s: &mut Scene, from: i32, to: i32, z: i32) {
    for x in from..=to {
        console(
            s,
            &format!("setblock {x} 64 {z} minecraft:rail[shape=east_west]"),
        );
    }
}

fn fuse_of(s: &Scene, cart: EntityId) -> Option<i32> {
    s.server.saved_entity(cart)?.get_i32("fuse")
}

#[test]
fn a_tnt_minecart_on_a_powered_activator_rail_starts_a_fuse() {
    let mut s = ready();
    console(&mut s, "setblock 3 64 3 minecraft:activator_rail");
    console(&mut s, "setblock 3 64 4 minecraft:redstone_block");
    let cart = summon(&mut s, Kind::TntMinecart, "3.5 64.1 3.5", "{}");
    s.tick(3);
    let fuse = fuse_of(&s, cart).expect("fuse");
    assert!((1..80).contains(&fuse), "{fuse}");
}

#[test]
fn a_tnt_minecart_on_an_unpowered_activator_rail_stays_idle() {
    let mut s = ready();
    console(
        &mut s,
        "setblock 3 64 3 minecraft:activator_rail[powered=false]",
    );
    let cart = summon(&mut s, Kind::TntMinecart, "3.5 64.1 3.5", "{}");
    s.tick(3);
    assert_eq!(fuse_of(&s, cart), Some(-1));
}

#[test]
fn a_primed_tnt_minecart_explodes_when_its_fuse_runs_out() {
    let mut s = ready();
    console(&mut s, "setblock 8 63 3 minecraft:dirt");
    let cart = summon(&mut s, Kind::TntMinecart, "8.5 64.1 3.5", "{fuse:4}");
    console(&mut s, "setblock 8 64 3 minecraft:air");
    s.tick(8);
    assert!(
        s.server
            .entities_of(Kind::TntMinecart)
            .iter()
            .all(|entity| entity.id != cart)
    );
}

#[test]
fn a_flaming_arrow_sets_off_a_tnt_minecart() {
    let mut s = ready();
    let cart = summon(&mut s, Kind::TntMinecart, "6.5 64.1 12.5", "{}");
    summon(
        &mut s,
        Kind::Arrow,
        "6.5 64.5 8.5",
        "{Motion:[0.0d,0.0d,2.0d],NoGravity:1b,Fire:200s}",
    );
    s.tick(8);
    assert!(
        s.server
            .entities_of(Kind::TntMinecart)
            .iter()
            .all(|entity| entity.id != cart),
        "the cart blew up"
    );
}

#[test]
fn a_plain_arrow_does_not_set_off_a_tnt_minecart() {
    let mut s = ready();
    let cart = summon(&mut s, Kind::TntMinecart, "6.5 64.1 12.5", "{}");
    summon(
        &mut s,
        Kind::Arrow,
        "6.5 64.5 8.5",
        "{Motion:[0.0d,0.0d,2.0d],NoGravity:1b}",
    );
    s.tick(8);
    assert!(
        s.server
            .entities_of(Kind::TntMinecart)
            .iter()
            .any(|entity| entity.id == cart)
    );
}

#[test]
fn a_tnt_minecart_that_falls_three_blocks_or_more_explodes() {
    let mut s = ready();
    let cart = summon(&mut s, Kind::TntMinecart, "6.5 78 12.5", "{}");
    s.tick(70);
    assert!(
        s.server
            .entities_of(Kind::TntMinecart)
            .iter()
            .all(|entity| entity.id != cart)
    );
}

#[test]
fn a_furnace_minecart_with_fuel_pushes_itself_along_the_rail() {
    let mut s = ready();
    rails(&mut s, 2, 14, 3);
    let cart = summon(
        &mut s,
        Kind::FurnaceMinecart,
        "4.5 64.1 3.5",
        "{Fuel:600s,PushX:1.0d,PushZ:0.0d}",
    );
    let start = s.server.entities_of(Kind::FurnaceMinecart)[0].position.x;
    s.tick(20);
    let now = s.server.entities_of(Kind::FurnaceMinecart)[0].position.x;
    assert!(now > start + 1.0, "{start} {now}");
    assert!(saved(&s, cart).get_i32("Fuel").unwrap_or(0) < 600);
}

#[test]
fn a_furnace_minecart_without_fuel_stays_put() {
    let mut s = ready();
    rails(&mut s, 2, 14, 3);
    summon(&mut s, Kind::FurnaceMinecart, "4.5 64.1 3.5", "{}");
    s.tick(20);
    let now = s.server.entities_of(Kind::FurnaceMinecart)[0].position.x;
    assert!((now - 4.5).abs() < 0.1, "{now}");
}

#[test]
fn coal_fuels_a_furnace_minecart_for_3600_ticks() {
    use rspigot_core::events::{Hand, PlayerInput};
    let mut s = ready();
    rails(&mut s, 2, 14, 3);
    let cart = summon(&mut s, Kind::FurnaceMinecart, "4.5 64.1 3.5", "{}");
    console(&mut s, "give Alice minecraft:coal 2");
    let alice = s.players[0].clone();
    send_input(&mut s.server, &alice, PlayerInput::HeldSlot(0));
    s.tick(2);
    send_input(
        &mut s.server,
        &alice,
        PlayerInput::Interact {
            target: cart,
            hand: Hand::Main,
            sneaking: false,
            location: rspigot_domain::Vec3::new(0.0, 0.5, 0.0),
        },
    );
    s.tick(1);
    let fuel = saved(&s, cart).get_i32("Fuel").unwrap_or(0);
    assert!((3590..=3600).contains(&fuel), "{fuel}");
}

#[test]
fn a_spawner_minecart_spawns_its_mob_near_a_player() {
    let mut s = ready();
    summon(
        &mut s,
        Kind::SpawnerMinecart,
        "4.5 64.1 4.5",
        "{Delay:1s,SpawnData:{entity:{id:\"minecraft:zombie\"},custom_spawn_rules:{block_light_limit:[0,15],sky_light_limit:[0,15]}},RequiredPlayerRange:16s,SpawnCount:2s,SpawnRange:4s,MinSpawnDelay:10s,MaxSpawnDelay:20s,MaxNearbyEntities:6s}",
    );
    s.tick(60);
    assert!(!s.server.entities_of(Kind::Zombie).is_empty());
}

#[test]
fn a_spawner_minecart_with_no_player_in_range_stays_idle() {
    let mut s = ready();
    summon(
        &mut s,
        Kind::SpawnerMinecart,
        "100.5 64.1 100.5",
        "{Delay:1s,SpawnData:{entity:{id:\"minecraft:zombie\"},custom_spawn_rules:{block_light_limit:[0,15],sky_light_limit:[0,15]}},RequiredPlayerRange:16s}",
    );
    s.tick(60);
    assert!(s.server.entities_of(Kind::Zombie).is_empty());
}

#[test]
fn a_detector_rail_outputs_the_success_count_of_a_command_block_minecart() {
    let mut s = ready();
    console(
        &mut s,
        "setblock 3 64 3 minecraft:detector_rail[shape=east_west]",
    );
    console(&mut s, "setblock 4 64 3 minecraft:comparator[facing=west]");
    console(&mut s, "setblock 5 64 3 minecraft:redstone_wire");
    summon(
        &mut s,
        Kind::CommandBlockMinecart,
        "3.5 64.1 3.5",
        "{SuccessCount:7}",
    );
    s.tick(10);
    let power = match s.ask(rspigot_core::plugins::Question::Block(BlockPos::new(
        5, 64, 3,
    ))) {
        rspigot_core::plugins::Answer::Block(state) => state
            .properties
            .iter()
            .find(|(key, _)| key == "power")
            .map(|(_, value)| value.clone()),
        other => panic!("{other:?}"),
    };
    assert_eq!(power.as_deref(), Some("7"));
}

fn block_prop(s: &mut Scene, pos: BlockPos, name: &str) -> Option<String> {
    match s.ask(rspigot_core::plugins::Question::Block(pos)) {
        rspigot_core::plugins::Answer::Block(state) => state
            .properties
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.clone()),
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_wind_charge_bursting_next_to_a_lever_pulls_it() {
    let mut s = ready();
    console(
        &mut s,
        "setblock 6 64 12 minecraft:lever[face=floor,powered=false]",
    );
    summon(
        &mut s,
        Kind::WindCharge,
        "6.5 66 12.5",
        "{Motion:[0.0d,-0.5d,0.0d]}",
    );
    s.tick(10);
    assert_eq!(
        block_prop(&mut s, BlockPos::new(6, 64, 12), "powered").as_deref(),
        Some("true")
    );
}

#[test]
fn a_wind_charge_bursting_next_to_a_wooden_door_opens_it() {
    let mut s = ready();
    console(
        &mut s,
        "setblock 6 64 12 minecraft:oak_door[half=lower,open=false]",
    );
    console(
        &mut s,
        "setblock 6 65 12 minecraft:oak_door[half=upper,open=false]",
    );
    summon(
        &mut s,
        Kind::WindCharge,
        "7.5 66.5 12.5",
        "{Motion:[0.0d,-0.5d,0.0d]}",
    );
    s.tick(10);
    assert_eq!(
        block_prop(&mut s, BlockPos::new(6, 64, 12), "open").as_deref(),
        Some("true")
    );
}

#[test]
fn a_wind_charge_bursting_next_to_an_iron_door_leaves_it_shut() {
    let mut s = ready();
    console(
        &mut s,
        "setblock 6 64 12 minecraft:iron_door[half=lower,open=false]",
    );
    console(
        &mut s,
        "setblock 6 65 12 minecraft:iron_door[half=upper,open=false]",
    );
    summon(
        &mut s,
        Kind::WindCharge,
        "7.5 66.5 12.5",
        "{Motion:[0.0d,-0.5d,0.0d]}",
    );
    s.tick(10);
    assert_eq!(
        block_prop(&mut s, BlockPos::new(6, 64, 12), "open").as_deref(),
        Some("false")
    );
}

fn shear(s: &mut Scene, target: EntityId) {
    use rspigot_core::events::{Hand, PlayerInput};
    let alice = s.players[0].clone();
    send_input(
        &mut s.server,
        &alice,
        PlayerInput::Interact {
            target,
            hand: Hand::Main,
            sneaking: false,
            location: rspigot_domain::Vec3::new(0.0, 0.5, 0.0),
        },
    );
    s.tick(2);
}

fn mushrooms(s: &Scene) -> i32 {
    s.server
        .entities_of(Kind::Item)
        .iter()
        .filter_map(|item| {
            s.server
                .saved_entity(item.id)?
                .get_compound("Item")
                .cloned()
        })
        .filter(|stack| {
            stack
                .get_str("id")
                .is_some_and(|id| id.ends_with("_mushroom"))
        })
        .map(|stack| stack.get_i32("count").unwrap_or(1))
        .sum()
}

#[test]
fn shears_on_a_bogged_drop_two_mushrooms_and_mark_it_sheared() {
    let mut s = ready_at(6_000);
    console(&mut s, "give Alice minecraft:shears 1");
    let alice = s.players[0].clone();
    send_input(
        &mut s.server,
        &alice,
        rspigot_core::events::PlayerInput::HeldSlot(0),
    );
    let bogged = summon(&mut s, Kind::Bogged, "1.5 64 0.5", "{NoAI:1b,Silent:1b}");
    s.tick(2);
    shear(&mut s, bogged);
    assert_eq!(saved(&s, bogged).get_bool("sheared"), Some(true));
    assert_eq!(mushrooms(&s), 2);
    shear(&mut s, bogged);
    assert_eq!(mushrooms(&s), 2);
}

#[test]
fn a_dispenser_with_shears_shears_a_bogged_in_front_of_it() {
    let mut s = ready_at(6_000);
    console(
        &mut s,
        "setblock 6 64 6 minecraft:dispenser[facing=east]{Items:[{Slot:0b,id:\"minecraft:shears\",count:1}]}",
    );
    let bogged = summon(&mut s, Kind::Bogged, "7.5 64 6.5", "{NoAI:1b,Silent:1b}");
    console(&mut s, "setblock 5 64 6 minecraft:redstone_block");
    s.tick(8);
    assert_eq!(saved(&s, bogged).get_bool("sheared"), Some(true));
    assert_eq!(mushrooms(&s), 2);
}

fn stand(s: &mut Scene) {
    let alice = s.players[0].clone();
    send_input(
        &mut s.server,
        &alice,
        rspigot_core::events::PlayerInput::Move {
            position: Some(rspigot_domain::Vec3::new(0.5, 64.0, 0.5)),
            rotation: Some(rspigot_domain::Rotation::new(0.0, 0.0)),
            on_ground: true,
        },
    );
    s.tick(1);
}

fn owned_parrot(s: &mut Scene, at: &str) -> EntityId {
    stand(s);
    let uuid = s.players[0].id.0.as_u128();
    let parts = [
        (uuid >> 96) as u32 as i32,
        (uuid >> 64) as u32 as i32,
        (uuid >> 32) as u32 as i32,
        uuid as u32 as i32,
    ];
    summon(
        s,
        Kind::Parrot,
        at,
        &format!(
            "{{NoAI:1b,Silent:1b,Tame:1b,Owner:[I;{},{},{},{}],Variant:2}}",
            parts[0], parts[1], parts[2], parts[3]
        ),
    )
}

#[test]
fn a_tame_parrot_landing_on_its_owner_sits_on_the_shoulder() {
    let mut s = ready();
    owned_parrot(&mut s, "0.5 64 0.5");
    s.tick(130);
    assert!(s.server.entities_of(Kind::Parrot).is_empty());
    assert_eq!(
        s.server.player_shoulder_entities(s.players[0].id),
        (true, false)
    );
}

#[test]
fn a_second_parrot_takes_the_right_shoulder() {
    let mut s = ready();
    owned_parrot(&mut s, "0.5 64 0.5");
    owned_parrot(&mut s, "0.5 64 0.5");
    s.tick(130);
    assert_eq!(
        s.server.player_shoulder_entities(s.players[0].id),
        (true, true)
    );
}

#[test]
fn a_parrot_that_is_not_tame_stays_on_the_ground() {
    let mut s = ready();
    stand(&mut s);
    summon(&mut s, Kind::Parrot, "0.5 64 0.5", "{Silent:1b}");
    s.tick(130);
    assert_eq!(s.server.entities_of(Kind::Parrot).len(), 1);
    assert_eq!(
        s.server.player_shoulder_entities(s.players[0].id),
        (false, false)
    );
}

#[test]
fn damage_to_the_owner_sends_the_shoulder_parrot_flying_again() {
    let mut s = ready();
    owned_parrot(&mut s, "0.5 64 0.5");
    s.tick(130);
    assert_eq!(
        s.server.player_shoulder_entities(s.players[0].id),
        (true, false)
    );
    console(&mut s, "damage Alice 1 minecraft:generic");
    s.tick(2);
    assert_eq!(
        s.server.player_shoulder_entities(s.players[0].id),
        (false, false)
    );
    assert_eq!(s.server.entities_of(Kind::Parrot).len(), 1);
}

#[test]
fn water_drops_the_shoulder_parrot() {
    let mut s = ready();
    owned_parrot(&mut s, "0.5 64 0.5");
    s.tick(130);
    console(&mut s, "setblock 0 64 0 minecraft:water");
    console(&mut s, "setblock 0 65 0 minecraft:water");
    s.tick(25);
    assert_eq!(
        s.server.player_shoulder_entities(s.players[0].id),
        (false, false)
    );
    assert_eq!(s.server.entities_of(Kind::Parrot).len(), 1);
}

#[test]
fn the_shoulder_parrot_is_saved_with_the_player() {
    let (mut s, saves) = saving_scene(&["Alice"], Vec::new());
    stream_chunk_of(&mut s, BlockPos::new(1, 64, 1));
    s.tick(5);
    owned_parrot(&mut s, "0.5 64 0.5");
    s.tick(130);
    let id = s.players[0].id;
    let leave = leave_of(&s.players[0]);
    s.server.handle(leave);
    s.server.deliver();
    let saved = rspigot_core::ports::PlayerStore::load_player(&*saves, id)
        .unwrap()
        .expect("the player was saved");
    assert!(
        saved
            .get_compound("ShoulderEntityLeft")
            .is_some_and(|parrot| parrot.get_str("id") == Some("minecraft:parrot"))
    );
}

#[test]
fn the_cat_spawner_adds_a_cat_near_a_village_with_more_than_four_occupied_beds() {
    let mut s = ready_spawning();
    s.server.seed_village(BlockPos::new(0, 64, 0), 5);
    s.tick(1_300);
    assert_eq!(s.server.entities_of(Kind::Cat).len(), 1);
}

#[test]
fn the_cat_spawner_does_nothing_without_a_village() {
    let mut s = ready_spawning();
    s.tick(1_300);
    assert!(s.server.entities_of(Kind::Cat).is_empty());
}

#[test]
fn the_cat_spawner_stops_at_five_cats_in_range() {
    let mut s = ready_spawning();
    s.server.seed_village(BlockPos::new(0, 64, 0), 5);
    for index in 0..5 {
        summon(
            &mut s,
            Kind::Cat,
            &format!("{}.5 64 2.5", index),
            "{NoAI:1b,Silent:1b}",
        );
    }
    s.tick(1_300);
    assert_eq!(s.server.entities_of(Kind::Cat).len(), 5);
}

fn plant_heart(s: &mut Scene) {
    console(s, "setblock 5 64 6 minecraft:pale_oak_log[axis=x]");
    console(s, "setblock 7 64 6 minecraft:pale_oak_log[axis=x]");
    console(s, "setblock 6 64 6 minecraft:creaking_heart[axis=x]");
}

fn heart_state(s: &mut Scene) -> Option<String> {
    block_prop(s, BlockPos::new(6, 64, 6), "creaking_heart_state")
}

#[test]
fn a_creaking_heart_wakes_at_night_and_spawns_a_creaking_linked_to_it() {
    let mut s = ready_at(18_000);
    plant_heart(&mut s);
    s.tick(120);
    assert_eq!(heart_state(&mut s).as_deref(), Some("awake"));
    let creakings = s.server.entities_of(Kind::Creaking);
    assert_eq!(creakings.len(), 1);
    let link = s
        .server
        .block_entity_nbt(BlockPos::new(6, 64, 6))
        .and_then(|nbt| nbt.get("creaking").cloned());
    let uuid = creakings[0].uuid.as_u128();
    let expected = [
        (uuid >> 96) as u32 as i32,
        (uuid >> 64) as u32 as i32,
        (uuid >> 32) as u32 as i32,
        uuid as u32 as i32,
    ];
    assert_eq!(link, Some(rspigot_nbt::Tag::IntArray(expected.to_vec())));
}

#[test]
fn a_creaking_heart_sleeps_through_the_day() {
    let mut s = ready_at(6_000);
    plant_heart(&mut s);
    s.tick(120);
    assert_eq!(heart_state(&mut s).as_deref(), Some("dormant"));
    assert!(s.server.entities_of(Kind::Creaking).is_empty());
}

#[test]
fn a_creaking_heart_without_both_logs_is_uprooted_and_spawns_nothing() {
    let mut s = ready_at(18_000);
    console(&mut s, "setblock 5 64 6 minecraft:pale_oak_log[axis=x]");
    console(&mut s, "setblock 6 64 6 minecraft:creaking_heart[axis=x]");
    s.tick(120);
    assert_eq!(heart_state(&mut s).as_deref(), Some("uprooted"));
    assert!(s.server.entities_of(Kind::Creaking).is_empty());
}

#[test]
fn breaking_the_heart_tears_its_creaking_down() {
    let mut s = ready_at(18_000);
    plant_heart(&mut s);
    s.tick(120);
    assert_eq!(s.server.entities_of(Kind::Creaking).len(), 1);
    console(&mut s, "setblock 6 64 6 minecraft:air");
    s.tick(5);
    assert!(s.server.entities_of(Kind::Creaking).is_empty());
}

#[test]
fn a_creaking_farther_than_34_blocks_from_its_heart_is_torn_down() {
    let mut s = ready_at(18_000);
    plant_heart(&mut s);
    s.tick(120);
    let original = s.server.entities_of(Kind::Creaking)[0].uuid;
    console(&mut s, "tp @e[type=creaking] 44.5 64 6.5");
    s.tick(60);
    assert!(
        s.server
            .entities_of(Kind::Creaking)
            .iter()
            .all(|creaking| creaking.uuid != original)
    );
}

#[test]
fn the_comparator_reads_15_minus_the_scaled_distance_to_the_creaking() {
    let mut s = ready_at(18_000);
    plant_heart(&mut s);
    console(&mut s, "setblock 6 64 7 minecraft:comparator[facing=north]");
    console(&mut s, "setblock 6 64 8 minecraft:redstone_wire");
    s.tick(120);
    let creaking = s.server.entities_of(Kind::Creaking);
    let at = creaking[0].position;
    let distance = ((at.x - 6.5).powi(2) + (at.y - 64.5).powi(2) + (at.z - 6.5).powi(2)).sqrt();
    let expected = 15 - ((distance.clamp(0.0, 32.0) / 32.0) * 15.0).floor() as i32;
    s.tick(25);
    let power = block_prop(&mut s, BlockPos::new(6, 64, 8), "power");
    assert_eq!(
        power.as_deref(),
        Some(expected.to_string().as_str()),
        "{distance}"
    );
}

#[test]
fn a_creaking_closer_than_34_blocks_stays_linked() {
    let mut s = ready_at(18_000);
    plant_heart(&mut s);
    s.tick(120);
    console(&mut s, "tp @e[type=creaking] 30.5 64 6.5");
    s.tick(60);
    assert_eq!(s.server.entities_of(Kind::Creaking).len(), 1);
}

#[test]
fn hurting_a_creaking_spreads_resin_over_the_logs_of_an_awake_heart() {
    let mut s = ready_at(18_000);
    plant_heart(&mut s);
    s.tick(120);
    console(
        &mut s,
        "damage @e[type=creaking,limit=1] 1 minecraft:player_attack by Alice",
    );
    s.tick(2);
    let mut resin = 0;
    for x in 3..=9 {
        for y in 62..=66 {
            for z in 4..=8 {
                let state = s.server.block_state_at(BlockPos::new(x, y, z));
                if state.and_then(|state| state.kind())
                    == Some(rspigot_blocks::BlockKind::ResinClump)
                {
                    resin += 1;
                }
            }
        }
    }
    assert!(resin >= 1, "{resin}");
}

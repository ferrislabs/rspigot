mod common;

use common::*;
use rspigot_core::events::{Hand, Inbound, PlayerInput};
use rspigot_domain::{BlockPos, EntityId, Vec3};
use rspigot_entities::EntityKind as Kind;
use rspigot_nbt::{Compound, Tag};

fn world() -> Scene {
    let mut s = scene(&["Alice"]);
    stream_chunk_of(&mut s, BlockPos::new(1, 64, 1));
    s
}

fn console(s: &mut Scene, line: &str) {
    s.server.handle(Inbound::Console(line.to_owned()));
    s.server.deliver();
}

fn summon_at(s: &mut Scene, kind: Kind, at: &str, nbt: &str) -> EntityId {
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
    s.server.saved_entity(id).expect("a saved mob")
}

fn interact(s: &mut Scene, target: EntityId) {
    let alice = s.players[0].clone();
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

fn position(s: &Scene, kind: Kind, id: EntityId) -> Vec3 {
    s.server
        .entities_of(kind)
        .into_iter()
        .find(|entity| entity.id == id)
        .expect("the entity")
        .position
}

#[test]
fn a_llama_summons_with_its_vanilla_state_and_persists_it() {
    let mut s = world();
    let llama = summon_at(
        &mut s,
        Kind::Llama,
        "3.5 64 0.5",
        "{Strength:4,Variant:2,Tame:1b,ChestedHorse:1b,Temper:7}",
    );
    let nbt = saved(&s, llama);
    assert_eq!(nbt.get_i32("Strength"), Some(4));
    assert_eq!(nbt.get_i32("Variant"), Some(2));
    assert_eq!(nbt.get_i32("Temper"), Some(7));
    assert_eq!(nbt.get_bool("ChestedHorse"), Some(true));
    assert_eq!(nbt.get_bool("Tame"), Some(true));
    assert!(nbt.get_i32("DespawnDelay").is_none());
}

#[test]
fn a_trader_llama_persists_its_despawn_delay_and_a_plain_one_rolls_its_stats() {
    let mut s = world();
    let trader = summon_at(
        &mut s,
        Kind::TraderLlama,
        "3.5 64 0.5",
        "{DespawnDelay:1200,Strength:2}",
    );
    let nbt = saved(&s, trader);
    assert!(
        nbt.get_i32("DespawnDelay")
            .is_some_and(|delay| delay <= 1200)
    );
    let plain = summon_at(&mut s, Kind::Llama, "5.5 64 0.5", "{}");
    let nbt = saved(&s, plain);
    assert!((1..=5).contains(&nbt.get_i32("Strength").unwrap()));
    assert!((0..=3).contains(&nbt.get_i32("Variant").unwrap()));
}

#[test]
fn a_llama_spits_at_a_wild_wolf_and_the_spit_flies_then_vanishes() {
    let mut s = world();
    summon_at(&mut s, Kind::Llama, "3.5 64 0.5", "{Strength:3}");
    let wolf = summon_at(&mut s, Kind::Wolf, "6.5 64 0.5", "{NoAI:1b}");
    let mut spat = false;
    let mut vanished = false;
    for _ in 0..400 {
        s.tick(1);
        let flying = !s.server.entities_of(Kind::LlamaSpit).is_empty();
        spat |= flying;
        if spat && !flying {
            vanished = true;
            break;
        }
    }
    assert!(spat, "the llama spits at the wolf");
    assert!(vanished, "the spit is discarded after its flight");
    s.tick(120);
    let health = s
        .server
        .entities_of(Kind::Wolf)
        .into_iter()
        .find(|entity| entity.id == wolf)
        .and_then(|entity| entity.health);
    assert!(health.is_some_and(|health| health < 8.0), "{health:?}");
}

#[test]
fn a_hay_block_heals_a_hurt_llama_raises_its_temper_and_a_carpet_dresses_a_tame_one() {
    let mut s = world();
    let llama = summon_at(&mut s, Kind::Llama, "2.5 64 0.5", "{Health:10f,Strength:2}");
    console(&mut s, "give Alice minecraft:hay_block 2");
    interact(&mut s, llama);
    let nbt = saved(&s, llama);
    assert_eq!(nbt.get_i32("Temper"), Some(6));
    assert!(nbt.get_f64("Health").is_some_and(|health| health > 19.0));
    let tame = summon_at(&mut s, Kind::Llama, "2.5 64 2.5", "{Tame:1b}");
    console(&mut s, "clear Alice");
    console(&mut s, "give Alice minecraft:white_carpet 1");
    interact(&mut s, tame);
    let nbt = format!("{:?}", saved(&s, tame));
    assert!(nbt.contains("white_carpet"), "{nbt}");
}

#[test]
fn a_saddle_does_not_fit_a_llama_and_wheat_gives_three_temper() {
    let mut s = world();
    let llama = summon_at(&mut s, Kind::Llama, "2.5 64 0.5", "{Tame:1b}");
    console(&mut s, "give Alice minecraft:saddle 1");
    interact(&mut s, llama);
    assert!(!format!("{:?}", saved(&s, llama)).contains("saddle"));
    let wild = summon_at(&mut s, Kind::Llama, "2.5 64 2.5", "{}");
    console(&mut s, "clear Alice");
    console(&mut s, "give Alice minecraft:wheat 1");
    interact(&mut s, wild);
    assert_eq!(saved(&s, wild).get_i32("Temper"), Some(3));
}

#[test]
fn the_wandering_trader_arrives_with_two_leashed_trader_llamas() {
    let mut s = world();
    let mut spawned = false;
    for _ in 0..400 {
        if s.server.spawn_wandering_trader()
            && !s.server.entities_of(Kind::WanderingTrader).is_empty()
        {
            spawned = true;
            break;
        }
    }
    assert!(spawned);
    s.tick(1);
    let trader = s.server.entities_of(Kind::WanderingTrader)[0];
    let llamas = s.server.entities_of(Kind::TraderLlama);
    assert_eq!(llamas.len(), 2);
    for llama in llamas {
        let nbt = saved(&s, llama.id);
        let Some(Tag::Compound(leash)) = nbt.get("leash") else {
            panic!("a leash: {nbt:?}");
        };
        assert!(leash.get("UUID").is_some());
        let at = llama.position;
        assert!((at.x - trader.position.x).abs() <= 5.0);
    }
}

#[test]
fn a_leashed_llama_is_pulled_toward_its_holder_and_the_lead_snaps_past_twelve_blocks() {
    let mut s = world();
    summon_at(
        &mut s,
        Kind::WanderingTrader,
        "3.5 64 0.5",
        "{NoAI:1b,DespawnDelay:48000,UUID:[I;1,2,3,4]}",
    );
    let llama = summon_at(
        &mut s,
        Kind::TraderLlama,
        "3.5 64 8.5",
        "{PersistenceRequired:1b,leash:{UUID:[I;1,2,3,4]}}",
    );
    let before = position(&s, Kind::TraderLlama, llama);
    s.tick(1);
    assert!(saved(&s, llama).get("leash").is_some());
    s.tick(10);
    let after = position(&s, Kind::TraderLlama, llama);
    assert!(after.z < before.z, "{before:?} -> {after:?}");
    console(
        &mut s,
        "tp @e[type=minecraft:wandering_trader] 3.5 64 -20.5",
    );
    s.tick(3);
    assert!(saved(&s, llama).get("leash").is_none());
}

#[test]
fn a_free_llama_joins_the_caravan_of_a_leashed_one_and_follows_it() {
    let mut s = world();
    summon_at(
        &mut s,
        Kind::WanderingTrader,
        "3.5 64 0.5",
        "{NoAI:1b,DespawnDelay:48000,UUID:[I;1,2,3,4]}",
    );
    summon_at(
        &mut s,
        Kind::TraderLlama,
        "3.5 64 3.5",
        "{NoAI:1b,leash:{UUID:[I;1,2,3,4]}}",
    );
    let follower = summon_at(&mut s, Kind::Llama, "3.5 64 9.5", "{}");
    let before = position(&s, Kind::Llama, follower);
    s.tick(60);
    let after = position(&s, Kind::Llama, follower);
    assert!(
        (after.z - 3.5).abs() < (before.z - 3.5).abs() - 1.0,
        "{before:?} -> {after:?}"
    );
}

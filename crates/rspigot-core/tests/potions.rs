mod common;

use std::sync::Arc;
use std::time::Duration;

use common::*;
use rspigot_core::config::{GameConfig, LevelData, PluginSettings};
use rspigot_core::events::{ClientEvent, EntityKind as WireKind, Inbound};
use rspigot_core::plugins::events::PotionSplashPatch;
use rspigot_core::plugins::{
    Answer, BlockOrder, EventKind, EventPatch, EventPayload, HandlerSpec, Order, Question,
};
use rspigot_core::ports::ChunkGenerator;
use rspigot_core::server::Server;
use rspigot_core::status::StatusBoard;
use rspigot_domain::chunk::ChunkColumn;
use rspigot_domain::damage::DamageType;
use rspigot_domain::effect::{EffectDuration, EffectInstance};
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

fn health(s: &Scene, kind: Kind, id: EntityId) -> f32 {
    s.server
        .entities_of(kind)
        .into_iter()
        .find(|entity| entity.id == id)
        .and_then(|entity| entity.health)
        .expect("a living entity")
}

fn exists(s: &Scene, kind: Kind, id: EntityId) -> bool {
    s.server
        .entities_of(kind)
        .into_iter()
        .any(|entity| entity.id == id)
}

fn snapshot(s: &Scene, kind: Kind, id: EntityId) -> rspigot_core::entities::EntitySnapshot {
    s.server
        .entities_of(kind)
        .into_iter()
        .find(|entity| entity.id == id)
        .expect("the entity exists")
}

fn uuid_ints(uuid: uuid::Uuid) -> [i32; 4] {
    let (high, low) = uuid.as_u64_pair();
    [
        (high >> 32) as i32,
        high as i32,
        (low >> 32) as i32,
        low as i32,
    ]
}

fn player_owner(s: &Scene) -> String {
    let [a, b, c, d] = uuid_ints(s.players[0].id.0);
    format!("[I;{a},{b},{c},{d}]")
}

fn spawned(events: &[ClientEvent], kind: Kind) -> Vec<(EntityId, Vec3, i32)> {
    events
        .iter()
        .filter_map(|event| match event {
            ClientEvent::SpawnEntity {
                entity,
                kind: WireKind::Other(id),
                position,
                data,
                ..
            } if *id == kind.id().0 => Some((*entity, *position, *data)),
            _ => None,
        })
        .collect()
}

fn damage_of(events: &[ClientEvent], id: EntityId, kind: DamageType) -> usize {
    events
        .iter()
        .filter(|event| {
            matches!(event, ClientEvent::DamageEvent { entity, kind: found, .. }
                if *entity == id && *found == kind)
        })
        .count()
}

fn level_events(events: &[ClientEvent], wanted: i32) -> usize {
    events
        .iter()
        .filter(|event| matches!(event, ClientEvent::LevelEvent { event, .. } if *event == wanted))
        .count()
}

const FAR: Vec3 = Vec3::new(20.5, 120.0, 20.5);

fn potion_nbt(potion: &str, motion: &str) -> String {
    format!(
        "{{Item:{{id:\"minecraft:splash_potion\",count:1,components:{{\"minecraft:potion_contents\":{{potion:\"minecraft:{potion}\"}}}}}},Motion:{motion}}}"
    )
}

fn throw_potion(s: &mut Scene, potion: &str, at: Vec3, motion: &str) -> EntityId {
    summon(s, Kind::SplashPotion, &potion_nbt(potion, motion), at)
}

const DROP: &str = "[0.0d,-1.0d,0.0d]";

fn drop_potion(s: &mut Scene, potion: &str, at: Vec3) -> EntityId {
    throw_potion(s, potion, Vec3::new(at.x, 66.0, at.z), DROP)
}

fn effects_applied(events: &[ClientEvent]) -> Vec<EffectInstance> {
    events
        .iter()
        .filter_map(|event| match event {
            ClientEvent::EffectApplied { effect, .. } => Some(*effect),
            _ => None,
        })
        .collect()
}

fn level_event_data(events: &[ClientEvent], wanted: i32) -> Vec<i32> {
    events
        .iter()
        .filter_map(|event| match event {
            ClientEvent::LevelEvent { event, data, .. } if *event == wanted => Some(*data),
            _ => None,
        })
        .collect()
}

fn effect_names(s: &Scene, id: EntityId) -> Vec<(String, i32)> {
    let saved = s.server.saved_entity(id).expect("a saved entity");
    saved
        .get_list("active_effects")
        .map(|effects| {
            effects
                .iter()
                .filter_map(|effect| {
                    let effect = effect.as_compound()?;
                    Some((
                        effect.get_str("id")?.to_owned(),
                        effect.get_i32("duration")?,
                    ))
                })
                .collect()
        })
        .unwrap_or_default()
}

fn alice_position(s: &mut Scene) -> Vec3 {
    let Answer::Player(state) = s.ask(Question::Player(s.players[0].id)) else {
        panic!("expected Alice");
    };
    state.player.position
}

#[test]
fn a_splash_potion_accelerates_downward_by_five_hundredths_with_the_air_drag() {
    let mut s = ready(Vec::new());
    let id = throw_potion(&mut s, "water", FAR, "[0.4d,0.2d,0.0d]");
    let drag = f64::from(0.99f32);
    let mut expected = Vec3::new(0.4, 0.2, 0.0);
    for _ in 0..6 {
        s.tick(1);
        expected = (expected + Vec3::new(0.0, -0.05, 0.0)).scale(drag);
        let now = snapshot(&s, Kind::SplashPotion, id).velocity;
        assert!((now.x - expected.x).abs() < 1.0e-9, "{now:?} {expected:?}");
        assert!((now.y - expected.y).abs() < 1.0e-9, "{now:?} {expected:?}");
    }
}

#[test]
fn a_splash_potion_in_water_keeps_four_fifths_of_its_speed() {
    let mut s = ready(Vec::new());
    s.act(Order::Block {
        pos: BlockPos::new(30, 100, 30),
        order: BlockOrder::Set("minecraft:water".to_owned()),
    });
    let id = throw_potion(
        &mut s,
        "water",
        Vec3::new(30.5, 100.4, 30.5),
        "[0.0d,0.0d,0.5d]",
    );
    s.tick(2);
    let speed = snapshot(&s, Kind::SplashPotion, id).velocity;
    assert!((speed.z - 0.5 * f64::from(0.8f32) * f64::from(0.99f32)).abs() < 0.05);
}

#[test]
fn a_splash_potion_hitting_the_ground_splashes_in_its_colour_and_is_discarded() {
    let mut s = ready(Vec::new());
    let id = drop_potion(&mut s, "poison", Vec3::new(10.5, 66.0, 10.5));
    s.players[0].link.take();
    s.tick(3);
    assert!(!exists(&s, Kind::SplashPotion, id));
    let events = s.players[0].link.take();
    let poison = EffectInstance::new(
        rspigot_domain::effect::EffectId::from_name("minecraft:poison").unwrap(),
        EffectDuration::Ticks(900),
        0,
        true,
    );
    let expected = rspigot_domain::potion::effects_color(&[poison]).unwrap();
    assert_eq!(level_event_data(&events, 2002), vec![expected]);
    assert_eq!(level_events(&events, 1053), 1);
    assert_eq!(level_events(&events, 2007), 0);
}

#[test]
fn an_instant_potion_splashes_with_the_instant_event_and_sound() {
    let mut s = ready(Vec::new());
    drop_potion(&mut s, "healing", Vec3::new(10.5, 66.0, 10.5));
    s.players[0].link.take();
    s.tick(3);
    let events = s.players[0].link.take();
    assert_eq!(level_events(&events, 2007), 1);
    assert_eq!(level_events(&events, 1054), 1);
    assert_eq!(level_events(&events, 2002), 0);
    assert_eq!(level_events(&events, 1053), 0);
}

#[test]
fn a_water_bottle_splashes_with_the_default_water_colour() {
    let mut s = ready(Vec::new());
    drop_potion(&mut s, "water", Vec3::new(10.5, 66.0, 10.5));
    s.players[0].link.take();
    s.tick(3);
    let events = s.players[0].link.take();
    assert_eq!(
        level_event_data(&events, 2002),
        vec![rspigot_domain::potion::DEFAULT_COLOR]
    );
}

#[test]
fn a_silent_splash_potion_makes_no_sound() {
    let mut s = ready(Vec::new());
    let nbt = format!("{{Silent:1b,{}", &potion_nbt("poison", DROP)[1..]);
    summon(
        &mut s,
        Kind::SplashPotion,
        &nbt,
        Vec3::new(10.5, 66.0, 10.5),
    );
    s.players[0].link.take();
    s.tick(3);
    let events = s.players[0].link.take();
    assert_eq!(level_events(&events, 2002), 1);
    assert_eq!(level_events(&events, 1053), 0);
}

#[test]
fn a_direct_hit_gives_the_player_the_whole_poison_duration() {
    let mut s = ready(Vec::new());
    let at = alice_position(&mut s);
    s.players[0].link.take();
    throw_potion(&mut s, "poison", Vec3::new(at.x, 66.0, at.z), DROP);
    s.tick(2);
    let applied = effects_applied(&s.players[0].link.take());
    assert_eq!(applied.len(), 1);
    assert_eq!(applied[0].duration, EffectDuration::Ticks(900));
    assert_eq!(applied[0].amplifier, 0);
    assert!(applied[0].visible);
}

#[test]
fn the_effect_duration_shrinks_with_the_distance_to_the_splash() {
    let mut s = ready(Vec::new());
    let at = alice_position(&mut s);
    s.players[0].link.take();
    drop_potion(&mut s, "poison", Vec3::new(at.x + 2.4, 64.0, at.z));
    s.tick(3);
    let applied = effects_applied(&s.players[0].link.take());
    assert_eq!(applied.len(), 1);
    let gap = (at.x + 2.4 - 0.125) - (at.x + 0.3);
    let intensity = 1.0 - gap / 4.0;
    let expected = (intensity * 900.0 + 0.5) as u32;
    assert_eq!(applied[0].duration, EffectDuration::Ticks(expected));
}

#[test]
fn a_player_four_blocks_away_is_out_of_reach() {
    let mut s = ready(Vec::new());
    let at = alice_position(&mut s);
    s.players[0].link.take();
    drop_potion(&mut s, "poison", Vec3::new(at.x + 5.0, 64.0, at.z));
    s.tick(3);
    assert!(effects_applied(&s.players[0].link.take()).is_empty());
}

#[test]
fn an_effect_that_would_end_within_a_second_is_not_applied() {
    let mut s = ready(Vec::new());
    let at = alice_position(&mut s);
    let nbt = "{Item:{id:\"minecraft:splash_potion\",count:1,components:{\"minecraft:potion_contents\":{custom_effects:[{id:\"minecraft:poison\",duration:40}]}}},Motion:[0.0d,-1.0d,0.0d]}";
    summon(
        &mut s,
        Kind::SplashPotion,
        nbt,
        Vec3::new(at.x + 2.4, 66.0, at.z),
    );
    s.players[0].link.take();
    s.tick(3);
    assert!(effects_applied(&s.players[0].link.take()).is_empty());
    let nbt = nbt.replace("duration:40", "duration:80");
    summon(
        &mut s,
        Kind::SplashPotion,
        &nbt,
        Vec3::new(at.x + 2.4, 66.0, at.z),
    );
    s.tick(3);
    assert_eq!(effects_applied(&s.players[0].link.take()).len(), 1);
}

#[test]
fn a_splash_potion_poisons_a_mob_it_hits() {
    let mut s = ready(Vec::new());
    let cow = summon(&mut s, Kind::Cow, "{NoAI:1b}", Vec3::new(30.5, 64.0, 30.5));
    throw_potion(&mut s, "slowness", Vec3::new(30.5, 66.0, 30.5), DROP);
    s.tick(2);
    let effects = effect_names(&s, cow);
    assert_eq!(effects.len(), 1);
    assert_eq!(effects[0].0, "minecraft:slowness");
    assert!(effects[0].1 >= 1790 && effects[0].1 <= 1800, "{effects:?}");
}

#[test]
fn a_harming_splash_hurts_a_living_mob_for_six_as_indirect_magic() {
    let mut s = ready(Vec::new());
    let pig = summon(&mut s, Kind::Pig, "{NoAI:1b}", Vec3::new(30.5, 64.0, 30.5));
    s.players[0].link.take();
    throw_potion(&mut s, "harming", Vec3::new(30.5, 66.0, 30.5), DROP);
    s.tick(2);
    assert_eq!(health(&s, Kind::Pig, pig), 4.0);
    let events = s.players[0].link.take();
    assert_eq!(damage_of(&events, pig, DamageType::IndirectMagic), 1);
}

#[test]
fn a_strong_harming_splash_hurts_for_twelve() {
    let mut s = ready(Vec::new());
    let cow = summon(&mut s, Kind::Cow, "{NoAI:1b}", Vec3::new(30.5, 64.0, 30.5));
    throw_potion(&mut s, "strong_harming", Vec3::new(30.5, 66.0, 30.5), DROP);
    s.tick(2);
    assert_eq!(health(&s, Kind::Cow, cow), 0.0);
}

#[test]
fn a_healing_splash_harms_the_undead_for_six_as_indirect_magic() {
    let mut s = ready(Vec::new());
    let zombie = summon(
        &mut s,
        Kind::Zombie,
        "{NoAI:1b}",
        Vec3::new(30.5, 64.0, 30.5),
    );
    s.players[0].link.take();
    throw_potion(&mut s, "healing", Vec3::new(30.5, 66.0, 30.5), DROP);
    s.tick(2);
    assert_eq!(health(&s, Kind::Zombie, zombie), 14.0);
    assert_eq!(
        damage_of(&s.players[0].link.take(), zombie, DamageType::IndirectMagic),
        1
    );
}

#[test]
fn a_harming_splash_heals_a_hurt_undead_mob_by_four() {
    let mut s = ready(Vec::new());
    let zombie = summon(
        &mut s,
        Kind::Zombie,
        "{NoAI:1b,Health:10f}",
        Vec3::new(30.5, 64.0, 30.5),
    );
    throw_potion(&mut s, "harming", Vec3::new(30.5, 66.0, 30.5), DROP);
    s.tick(2);
    assert_eq!(health(&s, Kind::Zombie, zombie), 14.0);
}

#[test]
fn a_healing_splash_heals_a_hurt_living_mob() {
    let mut s = ready(Vec::new());
    let cow = summon(
        &mut s,
        Kind::Cow,
        "{NoAI:1b,Health:3f}",
        Vec3::new(30.5, 64.0, 30.5),
    );
    throw_potion(&mut s, "healing", Vec3::new(30.5, 66.0, 30.5), DROP);
    s.tick(2);
    assert_eq!(health(&s, Kind::Cow, cow), 7.0);
}

#[test]
fn a_witch_resists_a_splash_of_harming_down_to_fifteen_percent() {
    let mut s = ready(Vec::new());
    let witch = summon(
        &mut s,
        Kind::Witch,
        "{NoAI:1b}",
        Vec3::new(30.5, 64.0, 30.5),
    );
    throw_potion(&mut s, "harming", Vec3::new(30.5, 66.0, 30.5), DROP);
    s.tick(2);
    let now = health(&s, Kind::Witch, witch);
    assert!((now - (26.0 - 6.0 * 0.15)).abs() < 1.0e-4, "{now}");
}

#[test]
fn a_water_bottle_hurts_a_nearby_blaze_for_one_and_spares_a_far_one() {
    let mut s = ready(Vec::new());
    let near = summon(
        &mut s,
        Kind::Blaze,
        "{NoAI:1b}",
        Vec3::new(33.0, 64.0, 30.5),
    );
    let far = summon(
        &mut s,
        Kind::Blaze,
        "{NoAI:1b}",
        Vec3::new(36.5, 64.0, 30.5),
    );
    s.players[0].link.take();
    drop_potion(&mut s, "water", Vec3::new(30.5, 64.0, 30.5));
    s.tick(3);
    assert_eq!(health(&s, Kind::Blaze, near), 19.0);
    assert_eq!(health(&s, Kind::Blaze, far), 20.0);
    let events = s.players[0].link.take();
    assert_eq!(damage_of(&events, near, DamageType::IndirectMagic), 1);
}

#[test]
fn a_potion_other_than_water_does_not_hurt_water_sensitive_mobs() {
    let mut s = ready(Vec::new());
    let blaze = summon(
        &mut s,
        Kind::Blaze,
        "{NoAI:1b}",
        Vec3::new(31.5, 64.0, 30.5),
    );
    drop_potion(&mut s, "awkward", Vec3::new(30.5, 64.0, 30.5));
    s.tick(3);
    assert_eq!(health(&s, Kind::Blaze, blaze), 20.0);
}

#[test]
fn a_water_bottle_puts_out_a_burning_mob_within_four_blocks_only() {
    let mut s = ready(Vec::new());
    let near = summon(
        &mut s,
        Kind::Zombie,
        "{NoAI:1b,Fire:600s}",
        Vec3::new(32.5, 64.0, 30.5),
    );
    let far = summon(
        &mut s,
        Kind::Zombie,
        "{NoAI:1b,Fire:600s}",
        Vec3::new(36.5, 64.0, 30.5),
    );
    drop_potion(&mut s, "water", Vec3::new(30.5, 64.0, 30.5));
    s.tick(3);
    let fire = |s: &Scene, id| {
        s.server
            .saved_entity(id)
            .and_then(|saved| saved.get_i64("Fire"))
            .unwrap_or(0)
    };
    assert!(fire(&s, near) <= 0, "{}", fire(&s, near));
    assert!(fire(&s, far) > 0);
}

fn state_of(s: &mut Scene, pos: BlockPos) -> (String, Vec<(String, String)>) {
    match s.ask(Question::Block(pos)) {
        Answer::Block(state) => (state.kind, state.properties),
        other => panic!("expected a block, got {other:?}"),
    }
}

fn set_block(s: &mut Scene, pos: BlockPos, state: &str) {
    s.act(Order::Block {
        pos,
        order: BlockOrder::Set(state.to_owned()),
    });
}

#[test]
fn a_water_bottle_douses_fire_on_the_block_it_lands_beside_and_around_it() {
    let mut s = ready(Vec::new());
    for pos in [
        BlockPos::new(30, 64, 30),
        BlockPos::new(31, 64, 30),
        BlockPos::new(30, 64, 32),
    ] {
        set_block(&mut s, pos, "minecraft:fire");
    }
    drop_potion(&mut s, "water", Vec3::new(30.5, 64.0, 30.5));
    s.tick(3);
    assert_eq!(
        state_of(&mut s, BlockPos::new(30, 64, 30)).0,
        "minecraft:air"
    );
    assert_eq!(
        state_of(&mut s, BlockPos::new(31, 64, 30)).0,
        "minecraft:air"
    );
    assert_eq!(
        state_of(&mut s, BlockPos::new(30, 64, 32)).0,
        "minecraft:fire"
    );
}

#[test]
fn only_a_water_bottle_douses_fire() {
    let mut s = ready(Vec::new());
    set_block(&mut s, BlockPos::new(30, 64, 30), "minecraft:fire");
    drop_potion(&mut s, "awkward", Vec3::new(30.5, 64.0, 30.5));
    s.tick(3);
    assert_eq!(
        state_of(&mut s, BlockPos::new(30, 64, 30)).0,
        "minecraft:fire"
    );
}

#[test]
fn a_water_bottle_extinguishes_a_lit_candle_and_a_lit_campfire() {
    let mut s = ready(Vec::new());
    for (pos, kind) in [
        (BlockPos::new(30, 64, 30), "minecraft:candle"),
        (BlockPos::new(31, 65, 30), "minecraft:campfire"),
    ] {
        s.act(Order::Block {
            pos,
            order: BlockOrder::SetWith {
                kind: kind.to_owned(),
                properties: vec![("lit".to_owned(), "true".to_owned())],
            },
        });
    }
    let lit = |props: &[(String, String)]| {
        props
            .iter()
            .find(|(name, _)| name == "lit")
            .map(|(_, value)| value.clone())
    };
    assert_eq!(
        lit(&state_of(&mut s, BlockPos::new(30, 64, 30)).1).as_deref(),
        Some("true")
    );
    s.players[0].link.take();
    drop_potion(&mut s, "water", Vec3::new(30.5, 64.0, 30.5));
    s.tick(3);
    assert_eq!(
        lit(&state_of(&mut s, BlockPos::new(30, 64, 30)).1).as_deref(),
        Some("false")
    );
    assert_eq!(
        lit(&state_of(&mut s, BlockPos::new(31, 65, 30)).1).as_deref(),
        Some("false")
    );
    let events = s.players[0].link.take();
    assert_eq!(level_events(&events, 1009), 1);
}

#[test]
fn a_potion_in_the_air_does_not_splash_until_it_hits_something() {
    let mut s = ready(Vec::new());
    let id = throw_potion(&mut s, "poison", FAR, "[0.0d,0.0d,0.0d]");
    s.players[0].link.take();
    s.tick(10);
    assert!(exists(&s, Kind::SplashPotion, id));
    assert_eq!(level_events(&s.players[0].link.take(), 2002), 0);
}

fn splash_scene(handlers: Vec<HandlerSpec>) -> (Scene, EntityId) {
    let mut s = ready(handlers);
    let cow = summon(&mut s, Kind::Cow, "{NoAI:1b}", Vec3::new(30.5, 64.0, 30.5));
    (s, cow)
}

#[test]
fn a_splash_asks_the_plugins_with_the_affected_entities_and_the_hit_block() {
    let (mut s, cow) = splash_scene(vec![deciding(1, EventKind::PotionSplash)]);
    drop_potion(&mut s, "slowness", Vec3::new(31.5, 64.0, 30.5));
    s.tick(3);
    let asked = last_asked(&s.plugin, EventKind::PotionSplash);
    let EventPayload::PotionSplash(splash) = &asked.payload else {
        panic!("a potion splash");
    };
    assert_eq!(splash.entity_type, "minecraft:splash_potion");
    assert_eq!(splash.hit_block, Some(BlockPos::new(31, 63, 30)));
    assert!(splash.hit_entity.is_none());
    assert_eq!(
        splash
            .affected_entities
            .iter()
            .map(|view| view.id)
            .collect::<Vec<_>>(),
        vec![cow]
    );
    assert_eq!(splash.intensities.len(), 1);
    let (view, intensity) = &splash.intensities[0];
    assert_eq!(view.id, cow);
    assert!(*intensity > 0.0 && *intensity < 1.0, "{intensity}");
    assert!(effect_names(&s, cow).is_empty());
}

#[test]
fn a_cancelled_splash_applies_no_effect_but_still_splashes_and_vanishes() {
    let (mut s, cow) = splash_scene(vec![deciding(1, EventKind::PotionSplash)]);
    let potion = drop_potion(&mut s, "harming", Vec3::new(30.5, 64.0, 30.5));
    s.tick(3);
    let asked = last_asked(&s.plugin, EventKind::PotionSplash);
    s.players[0].link.take();
    rule_on(&mut s.server, 1, asked.id, true, None);
    assert_eq!(health(&s, Kind::Cow, cow), 10.0);
    assert!(!exists(&s, Kind::SplashPotion, potion));
    let events = s.players[0].link.take();
    assert_eq!(level_events(&events, 2007), 1);
}

#[test]
fn an_allowed_splash_applies_the_effects_after_the_answer() {
    let (mut s, cow) = splash_scene(vec![deciding(1, EventKind::PotionSplash)]);
    let potion = drop_potion(&mut s, "harming", Vec3::new(30.5, 64.0, 30.5));
    s.tick(3);
    let asked = last_asked(&s.plugin, EventKind::PotionSplash);
    assert!(exists(&s, Kind::SplashPotion, potion));
    rule_on(&mut s.server, 1, asked.id, false, None);
    assert_eq!(health(&s, Kind::Cow, cow), 4.0);
    assert!(!exists(&s, Kind::SplashPotion, potion));
}

#[test]
fn a_plugin_can_lower_an_intensity_or_remove_an_entity_from_the_splash() {
    let (mut s, cow) = splash_scene(vec![deciding(1, EventKind::PotionSplash)]);
    drop_potion(&mut s, "harming", Vec3::new(30.5, 64.0, 30.5));
    s.tick(3);
    let asked = last_asked(&s.plugin, EventKind::PotionSplash);
    let EventPayload::PotionSplash(splash) = &asked.payload else {
        panic!("a potion splash");
    };
    let view = splash.intensities[0].0.clone();
    rule_on(
        &mut s.server,
        1,
        asked.id,
        false,
        Some(EventPatch::PotionSplash(PotionSplashPatch {
            intensities: vec![(view.clone(), 0.5)],
        })),
    );
    assert_eq!(health(&s, Kind::Cow, cow), 7.0);
    drop_potion(&mut s, "harming", Vec3::new(30.5, 64.0, 30.5));
    s.tick(3);
    let asked = last_asked(&s.plugin, EventKind::PotionSplash);
    rule_on(
        &mut s.server,
        1,
        asked.id,
        false,
        Some(EventPatch::PotionSplash(PotionSplashPatch {
            intensities: vec![(view, 0.0)],
        })),
    );
    assert_eq!(health(&s, Kind::Cow, cow), 7.0);
}

#[test]
fn a_plugin_cannot_raise_an_intensity_above_one() {
    let (mut s, cow) = splash_scene(vec![deciding(1, EventKind::PotionSplash)]);
    drop_potion(&mut s, "harming", Vec3::new(31.5, 64.0, 30.5));
    s.tick(3);
    let asked = last_asked(&s.plugin, EventKind::PotionSplash);
    let EventPayload::PotionSplash(splash) = &asked.payload else {
        panic!("a potion splash");
    };
    let view = splash.intensities[0].0.clone();
    rule_on(
        &mut s.server,
        1,
        asked.id,
        false,
        Some(EventPatch::PotionSplash(PotionSplashPatch {
            intensities: vec![(view, 5.0)],
        })),
    );
    assert_eq!(health(&s, Kind::Cow, cow), 4.0);
}

#[test]
fn an_entity_hit_is_a_cancellable_projectile_hit_and_a_cancel_lets_the_potion_fly_on() {
    let mut s = ready(vec![deciding(1, EventKind::ProjectileHit)]);
    let cow = summon(&mut s, Kind::Cow, "{NoAI:1b}", Vec3::new(30.5, 64.0, 30.5));
    let potion = throw_potion(
        &mut s,
        "harming",
        Vec3::new(30.5, 65.2, 26.5),
        "[0.0d,0.0d,0.8d]",
    );
    for _ in 0..10 {
        s.tick(1);
        if !asked_for(&s.plugin, EventKind::ProjectileHit).is_empty() {
            break;
        }
    }
    let asked = last_asked(&s.plugin, EventKind::ProjectileHit);
    let EventPayload::ProjectileHit(hit) = &asked.payload else {
        panic!("a projectile hit");
    };
    assert_eq!(hit.entity.id, potion);
    assert_eq!(hit.entity_type, "minecraft:splash_potion");
    assert_eq!(hit.hit_entity.as_ref().map(|view| view.id), Some(cow));
    s.tick(3);
    assert!(exists(&s, Kind::SplashPotion, potion));
    assert_eq!(health(&s, Kind::Cow, cow), 10.0);
    rule_on(&mut s.server, 1, asked.id, true, None);
    assert!(exists(&s, Kind::SplashPotion, potion));
    assert_eq!(health(&s, Kind::Cow, cow), 10.0);
}

#[test]
fn an_allowed_entity_hit_splashes_on_the_target_and_discards_the_potion() {
    let mut s = ready(vec![deciding(1, EventKind::ProjectileHit)]);
    let cow = summon(&mut s, Kind::Cow, "{NoAI:1b}", Vec3::new(30.5, 64.0, 30.5));
    let potion = throw_potion(
        &mut s,
        "harming",
        Vec3::new(30.5, 65.2, 26.5),
        "[0.0d,0.0d,0.8d]",
    );
    for _ in 0..10 {
        s.tick(1);
        if !asked_for(&s.plugin, EventKind::ProjectileHit).is_empty() {
            break;
        }
    }
    let asked = last_asked(&s.plugin, EventKind::ProjectileHit);
    rule_on(&mut s.server, 1, asked.id, false, None);
    assert!(!exists(&s, Kind::SplashPotion, potion));
    assert_eq!(health(&s, Kind::Cow, cow), 4.0);
}

#[test]
fn a_block_hit_is_announced_with_the_block_and_the_face() {
    let mut s = ready(vec![observe(2, EventKind::ProjectileHit)]);
    let potion = drop_potion(&mut s, "water", Vec3::new(3.5, 66.0, 3.5));
    s.tick(3);
    assert!(!exists(&s, Kind::SplashPotion, potion));
    let settled = settled_for(&s.plugin, EventKind::ProjectileHit);
    let EventPayload::ProjectileHit(hit) = &settled.last().expect("a hit").payload else {
        panic!("a projectile hit");
    };
    assert_eq!(hit.hit_block, Some(BlockPos::new(3, 63, 3)));
    assert_eq!(hit.entity_type, "minecraft:splash_potion");
}

#[test]
fn a_splash_potion_keeps_its_item_when_saved() {
    let mut s = ready(Vec::new());
    let potion = throw_potion(&mut s, "weakness", FAR, "[0.0d,0.0d,0.0d]");
    let saved = s.server.saved_entity(potion).expect("saved");
    assert_eq!(
        saved
            .get_compound("Item")
            .and_then(|item| item.get_str("id")),
        Some("minecraft:splash_potion")
    );
    assert!(
        saved
            .get_compound("Item")
            .and_then(|item| item.get_compound("components"))
            .and_then(|components| components.get_compound("minecraft:potion_contents"))
            .and_then(|contents| contents.get_str("potion"))
            == Some("minecraft:weakness")
    );
}

fn potion_name_of(s: &Scene, id: EntityId) -> String {
    s.server
        .saved_entity(id)
        .and_then(|saved| {
            saved
                .get_compound("Item")?
                .get_compound("components")?
                .get_compound("minecraft:potion_contents")?
                .get_str("potion")
                .map(str::to_owned)
        })
        .expect("a potion item")
}

fn first_potion_thrown(s: &mut Scene, ticks: u32) -> Option<(EntityId, Vec3)> {
    for _ in 0..ticks {
        s.tick(1);
        let batch = s.players[0].link.take();
        if let Some((id, _, _)) = spawned(&batch, Kind::SplashPotion).first() {
            let velocity = snapshot(s, Kind::SplashPotion, *id).velocity;
            return Some((*id, velocity));
        }
    }
    None
}

#[test]
fn a_witch_throws_slowness_at_a_player_ten_blocks_away_for_its_first_potion() {
    let mut s = ready(Vec::new());
    let witch = summon(&mut s, Kind::Witch, "{}", Vec3::new(0.5, 64.0, 11.5));
    s.players[0].link.take();
    let (potion, velocity) = first_potion_thrown(&mut s, 400).expect("a potion was thrown");
    assert_eq!(potion_name_of(&s, potion), "minecraft:slowness");
    let saved = s.server.saved_entity(potion).expect("saved");
    assert_eq!(
        saved.get("Owner"),
        s.server.saved_entity(witch).unwrap().get("UUID")
    );
    let speed = velocity.length();
    assert!(speed > 0.45 && speed < 1.05, "{speed}");
    assert!(velocity.z < 0.0, "{velocity:?}");
}

#[test]
fn a_witch_close_to_its_target_throws_slower_potions() {
    let mut s = ready(Vec::new());
    summon(&mut s, Kind::Witch, "{}", Vec3::new(0.5, 64.0, 2.0));
    s.players[0].link.take();
    let (potion, velocity) = first_potion_thrown(&mut s, 400).expect("a potion was thrown");
    assert_ne!(potion_name_of(&s, potion), "minecraft:slowness");
    let speed = velocity.length();
    assert!(speed < 0.8, "{speed}");
}

#[test]
fn a_launch_by_a_witch_is_a_cancellable_projectile_launch_of_a_splash_potion() {
    let mut s = ready(vec![deciding(1, EventKind::ProjectileLaunch)]);
    summon(&mut s, Kind::Witch, "{}", Vec3::new(0.5, 64.0, 8.5));
    for _ in 0..400 {
        s.tick(1);
        if !asked_for(&s.plugin, EventKind::ProjectileLaunch).is_empty() {
            break;
        }
    }
    let asked = last_asked(&s.plugin, EventKind::ProjectileLaunch);
    let EventPayload::ProjectileLaunch(launch) = &asked.payload else {
        panic!("a projectile launch");
    };
    assert_eq!(launch.entity_type, "minecraft:splash_potion");
    assert!(s.server.entities_of(Kind::SplashPotion).is_empty());
    rule_on(&mut s.server, 1, asked.id, true, None);
    assert!(s.server.entities_of(Kind::SplashPotion).is_empty());
}

#[test]
fn a_hurt_witch_drinks_a_healing_potion_slowly_then_heals() {
    let mut s = ready(Vec::new());
    let witch = summon(
        &mut s,
        Kind::Witch,
        "{Health:10f,NoAI:0b}",
        Vec3::new(30.5, 64.0, 30.5),
    );
    s.players[0].link.take();
    let mut drinking_from = None;
    let mut speeds = Vec::new();
    for tick in 0..1200 {
        s.tick(1);
        let batch = s.players[0].link.take();
        for event in &batch {
            match event {
                ClientEvent::EntityEquipment { entity, slots } if *entity == witch => {
                    for (slot, stack) in slots {
                        if *slot == rspigot_core::events::EquipmentSlot::MainHand
                            && stack.is_some()
                            && drinking_from.is_none()
                        {
                            drinking_from = Some(tick);
                        }
                    }
                }
                ClientEvent::Attributes { entity, attributes } if *entity == witch => {
                    for attribute in attributes {
                        if attribute.attribute == "minecraft:movement_speed" {
                            speeds.push(
                                attribute
                                    .modifiers
                                    .iter()
                                    .map(|modifier| (modifier.id.to_string(), modifier.amount))
                                    .collect::<Vec<_>>(),
                            );
                        }
                    }
                }
                _ => {}
            }
        }
        if drinking_from.is_some() && health(&s, Kind::Witch, witch) > 10.0 {
            break;
        }
    }
    assert!(drinking_from.is_some(), "the witch took a potion");
    assert!(
        health(&s, Kind::Witch, witch) >= 14.0,
        "{}",
        health(&s, Kind::Witch, witch)
    );
    assert!(
        speeds
            .iter()
            .any(|mods| mods.contains(&("minecraft:drinking".to_owned(), -0.25))),
        "{speeds:?}"
    );
    assert!(
        speeds
            .last()
            .is_some_and(|mods| !mods.iter().any(|(id, _)| id == "minecraft:drinking")),
        "{speeds:?}"
    );
}

#[test]
fn a_witch_never_hurts_itself_with_its_own_potion() {
    let mut s = ready(Vec::new());
    let witch = summon(
        &mut s,
        Kind::Witch,
        "{NoAI:1b}",
        Vec3::new(30.5, 64.0, 30.5),
    );
    let [a, b, c, d] = uuid_ints(
        s.server
            .entities_of(Kind::Witch)
            .first()
            .map(|entity| entity.uuid)
            .unwrap(),
    );
    let nbt = format!(
        "{{Owner:[I;{a},{b},{c},{d}],{}",
        &potion_nbt("harming", DROP)[1..]
    );
    summon(
        &mut s,
        Kind::SplashPotion,
        &nbt,
        Vec3::new(30.5, 66.0, 30.5),
    );
    s.tick(3);
    assert_eq!(health(&s, Kind::Witch, witch), 26.0);
}

#[test]
fn poison_ticks_a_mob_for_one_every_twenty_five_ticks_and_never_kills_it() {
    let mut s = ready(Vec::new());
    let cow = summon(
        &mut s,
        Kind::Cow,
        "{NoAI:1b,Health:5f}",
        Vec3::new(30.5, 64.0, 30.5),
    );
    throw_potion(&mut s, "poison", Vec3::new(30.5, 66.0, 30.5), DROP);
    s.tick(2);
    let start = health(&s, Kind::Cow, cow);
    s.tick(60);
    assert_eq!(start - health(&s, Kind::Cow, cow), 2.0);
    s.tick(400);
    assert_eq!(health(&s, Kind::Cow, cow), 1.0);
}

#[test]
fn regeneration_heals_a_hurt_mob_one_point_every_fifty_ticks() {
    let mut s = ready(Vec::new());
    let cow = summon(
        &mut s,
        Kind::Cow,
        "{NoAI:1b,Health:2f}",
        Vec3::new(30.5, 64.0, 30.5),
    );
    throw_potion(&mut s, "regeneration", Vec3::new(30.5, 66.0, 30.5), DROP);
    s.tick(2);
    let start = health(&s, Kind::Cow, cow);
    s.tick(110);
    assert_eq!(health(&s, Kind::Cow, cow) - start, 2.0);
}

fn two_players(pvp: bool) -> Scene {
    let server = plains_server(|config| config.pvp = pvp);
    let mut s = scene_on(server, &["Alice", "Bob"], Vec::new());
    stream_chunk_of(&mut s, BlockPos::new(1, 64, 1));
    s
}

#[test]
fn with_pvp_off_a_players_potion_spares_other_players_from_harmful_effects_only() {
    let mut s = two_players(false);
    let alice = player_owner(&s);
    let nbt = |potion: &str| format!("{{Owner:{alice},{}", &potion_nbt(potion, DROP)[1..]);
    s.players[0].link.take();
    s.players[1].link.take();
    summon(
        &mut s,
        Kind::SplashPotion,
        &nbt("poison"),
        Vec3::new(0.5, 66.0, 0.5),
    );
    s.tick(2);
    assert_eq!(effects_applied(&s.players[0].link.take()).len(), 1);
    assert!(effects_applied(&s.players[1].link.take()).is_empty());
    summon(
        &mut s,
        Kind::SplashPotion,
        &nbt("swiftness"),
        Vec3::new(0.5, 66.0, 0.5),
    );
    s.tick(2);
    assert_eq!(effects_applied(&s.players[1].link.take()).len(), 1);
}

#[test]
fn with_pvp_on_a_players_potion_poisons_other_players() {
    let mut s = two_players(true);
    let alice = player_owner(&s);
    s.players[1].link.take();
    summon(
        &mut s,
        Kind::SplashPotion,
        &format!("{{Owner:{alice},{}", &potion_nbt("poison", DROP)[1..]),
        Vec3::new(0.5, 66.0, 0.5),
    );
    s.tick(2);
    assert_eq!(effects_applied(&s.players[1].link.take()).len(), 1);
}

mod common;

use std::sync::Arc;
use std::time::Duration;

use common::*;
use rspigot_core::config::{GameConfig, LevelData, PluginSettings};
use rspigot_core::events::{ClientEvent, EntityStatus, Inbound};
use rspigot_core::plugins::events::SpellcasterSpell;
use rspigot_core::plugins::{BlockOrder, EntityOrder, EventKind, EventPayload, HandlerSpec, Order};
use rspigot_core::ports::ChunkGenerator;
use rspigot_core::server::Server;
use rspigot_core::status::StatusBoard;
use rspigot_domain::chunk::ChunkColumn;
use rspigot_domain::damage::DamageType;
use rspigot_domain::effect::{EffectDuration, EffectId};
use rspigot_domain::world::{Difficulty, Dimension};
use rspigot_domain::{BiomeId, BlockPos, BlockStateId, ChunkPos, EntityId, Vec3};
use rspigot_entities::EntityKind as Kind;

struct Plains;

impl ChunkGenerator for Plains {
    fn generate(&self, dimension: &Dimension, pos: ChunkPos) -> ChunkColumn {
        let biome = rspigot_data::registries::biome_id("minecraft:plains").unwrap();
        let mut chunk = ChunkColumn::empty(pos, dimension.height, BiomeId(biome));
        chunk.inhabited_time = 72_000_000;
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
        difficulty: Difficulty::Normal,
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
    let known: Vec<EntityId> = s
        .server
        .entities_of(kind)
        .into_iter()
        .map(|entity| entity.id)
        .collect();
    let name = kind.name();
    console(
        s,
        &format!("summon {name} {} {} {} {nbt}", at.x, at.y, at.z),
    );
    s.server
        .entities_of(kind)
        .into_iter()
        .map(|entity| entity.id)
        .find(|id| !known.contains(id))
        .unwrap_or_else(|| panic!("{name} was not summoned"))
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

fn saved(s: &Scene, id: EntityId) -> rspigot_nbt::Compound {
    s.server.saved_entity(id).expect("a saved entity")
}

fn uuid_text(uuid: uuid::Uuid) -> String {
    let (high, low) = uuid.as_u64_pair();
    format!(
        "[I;{},{},{},{}]",
        (high >> 32) as i32,
        high as i32,
        (low >> 32) as i32,
        low as i32
    )
}

fn alice(s: &Scene) -> EntityId {
    s.server.player_entity_id(s.players[0].id).unwrap()
}

fn effect_ids(s: &Scene, id: EntityId) -> Vec<(String, i32)> {
    saved(s, id)
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

fn damage_of(events: &[ClientEvent], id: EntityId, kind: DamageType) -> usize {
    events
        .iter()
        .filter(|event| {
            matches!(event, ClientEvent::DamageEvent { entity, kind: found, .. }
                if *entity == id && *found == kind)
        })
        .count()
}

fn spell_events(s: &Scene) -> Vec<SpellcasterSpell> {
    asked_for(&s.plugin, EventKind::EntitySpellCast)
        .into_iter()
        .filter_map(|event| match event.payload {
            EventPayload::EntitySpellCast(cast) => Some(cast.spell),
            _ => None,
        })
        .collect()
}

const HOME: Vec3 = Vec3::new(6.5, 64.0, 0.5);
const AWAY: Vec3 = Vec3::new(30.5, 64.0, 30.5);

fn tick_until(s: &mut Scene, limit: u32, done: impl Fn(&Scene) -> bool) -> bool {
    for _ in 0..limit {
        if done(s) {
            return true;
        }
        s.tick(1);
    }
    done(s)
}

#[test]
fn an_evoker_summons_three_vexes_owned_by_it_bound_near_it_with_a_limited_life() {
    let mut s = ready(Vec::new());
    let evoker = summon(&mut s, Kind::Evoker, "", HOME);
    assert!(tick_until(&mut s, 900, |s| !s
        .server
        .entities_of(Kind::Vex)
        .is_empty()));
    s.tick(1);
    let vexes = s.server.entities_of(Kind::Vex);
    assert_eq!(vexes.len() % 3, 0, "{}", vexes.len());
    let owner = snapshot(&s, Kind::Evoker, evoker);
    for vex in &vexes {
        let nbt = saved(&s, vex.id);
        let uuid = owner.uuid.as_u128();
        let expected = rspigot_nbt::Tag::IntArray(vec![
            (uuid >> 96) as i32,
            (uuid >> 64) as i32,
            (uuid >> 32) as i32,
            uuid as i32,
        ]);
        assert_eq!(nbt.get("owner"), Some(&expected));
        let bound = nbt.get_int_array("bound_pos").expect("a bound position");
        assert!(
            (bound[0] - owner.position.x.floor() as i32).abs() <= 4,
            "{bound:?}"
        );
        assert!(
            (bound[2] - owner.position.z.floor() as i32).abs() <= 4,
            "{bound:?}"
        );
        let life = nbt.get_i32("life_ticks").expect("a limited life");
        assert!((0..=2400).contains(&life), "{life}");
    }
}

#[test]
fn summoned_vexes_are_announced_as_spell_spawns() {
    use rspigot_core::plugins::events::CreatureSpawnReason;
    let mut s = ready(vec![
        observe(2, EventKind::CreatureSpawn),
        observe(3, EventKind::EntitySpawn),
    ]);
    summon(&mut s, Kind::Evoker, "", HOME);
    assert!(tick_until(&mut s, 900, |s| !s
        .server
        .entities_of(Kind::Vex)
        .is_empty()));
    s.tick(2);
    let vexes: Vec<_> = settled_for(&s.plugin, EventKind::CreatureSpawn)
        .into_iter()
        .filter_map(|event| match event.payload {
            EventPayload::CreatureSpawn(spawn) if spawn.entity_type == Kind::Vex.name() => {
                Some(spawn.spawn_reason)
            }
            _ => None,
        })
        .collect();
    assert_eq!(vexes.len() % 3, 0);
    assert!(!vexes.is_empty());
    assert!(
        vexes
            .iter()
            .all(|reason| *reason == CreatureSpawnReason::Spell)
    );
}

#[test]
fn a_summon_spell_asks_the_spell_cast_event_and_a_cancel_stops_every_spell() {
    let mut s = ready(vec![deciding(1, EventKind::EntitySpellCast)]);
    summon(&mut s, Kind::Evoker, "", HOME);
    let mut seen = 0;
    for _ in 0..900 {
        s.tick(1);
        let asked = asked_for(&s.plugin, EventKind::EntitySpellCast);
        if asked.len() > seen {
            let last = asked.last().unwrap();
            rule_on(&mut s.server, 1, last.id, true, None);
            seen = asked.len();
        }
    }
    assert!(seen >= 2, "{seen}");
    assert!(spell_events(&s).contains(&SpellcasterSpell::SummonVex));
    assert!(spell_events(&s).contains(&SpellcasterSpell::Fangs));
    assert!(s.server.entities_of(Kind::Vex).is_empty());
    assert!(s.server.entities_of(Kind::EvokerFangs).is_empty());
}

#[test]
fn an_allowed_fangs_spell_raises_a_line_of_sixteen_or_two_rings_of_fangs_owned_by_the_evoker() {
    let mut s = ready(vec![deciding(1, EventKind::EntitySpellCast)]);
    let evoker = summon(&mut s, Kind::Evoker, "", HOME);
    let mut ruled = 0;
    let mut raised = false;
    for _ in 0..900 {
        s.tick(1);
        let asked = asked_for(&s.plugin, EventKind::EntitySpellCast);
        for event in &asked[ruled..] {
            rule_on(&mut s.server, 1, event.id, false, None);
        }
        ruled = asked.len();
        if !s.server.entities_of(Kind::EvokerFangs).is_empty() {
            raised = true;
            break;
        }
    }
    assert!(raised);
    let fangs = s.server.entities_of(Kind::EvokerFangs);
    assert!(fangs.len() == 13 || fangs.len() == 16, "{}", fangs.len());
    let owner = snapshot(&s, Kind::Evoker, evoker).uuid.as_u128();
    let expected = rspigot_nbt::Tag::IntArray(vec![
        (owner >> 96) as i32,
        (owner >> 64) as i32,
        (owner >> 32) as i32,
        owner as i32,
    ]);
    for fang in fangs {
        assert_eq!(saved(&s, fang.id).get("Owner"), Some(&expected));
    }
}

#[test]
fn fangs_strike_eight_ticks_after_their_warmup_for_six_and_vanish_twenty_three_ticks_in() {
    let mut s = ready(Vec::new());
    let zombie = summon(&mut s, Kind::Zombie, "{NoAI:1b}", AWAY);
    let fangs = summon(
        &mut s,
        Kind::EvokerFangs,
        "{Warmup:2}",
        Vec3::new(AWAY.x, AWAY.y, AWAY.z),
    );
    assert_eq!(saved(&s, fangs).get_i32("Warmup"), Some(2));
    let full = snapshot(&s, Kind::Zombie, zombie).health.unwrap();
    s.tick(9);
    assert_eq!(snapshot(&s, Kind::Zombie, zombie).health.unwrap(), full);
    s.tick(1);
    assert_eq!(
        snapshot(&s, Kind::Zombie, zombie).health.unwrap(),
        full - 6.0
    );
    s.tick(14);
    assert!(exists(&s, Kind::EvokerFangs, fangs));
    s.tick(1);
    assert!(!exists(&s, Kind::EvokerFangs, fangs));
}

#[test]
fn fangs_tell_clients_to_snap_shut_once_and_hurt_a_player_with_magic_damage() {
    let mut s = ready(Vec::new());
    let alice = alice(&s);
    s.players[0].link.take();
    summon(
        &mut s,
        Kind::EvokerFangs,
        "{Warmup:0}",
        Vec3::new(0.5, 64.0, 0.5),
    );
    let mut events = Vec::new();
    for _ in 0..30 {
        s.tick(1);
        events.extend(s.players[0].link.take());
    }
    let snaps = events
        .iter()
        .filter(|event| {
            matches!(
                event,
                ClientEvent::EntityStatus {
                    status: EntityStatus::Event(4),
                    ..
                }
            )
        })
        .count();
    assert_eq!(snaps, 1);
    assert_eq!(damage_of(&events, alice, DamageType::Magic), 1);
    assert!(
        s.server
            .player_health(s.players[0].id)
            .is_some_and(|health| health < 20.0)
    );
}

#[test]
fn fangs_never_hurt_their_evoker_its_vexes_or_other_illagers_but_hurt_everything_else() {
    let mut s = ready(Vec::new());
    let evoker = summon(&mut s, Kind::Evoker, "{NoAI:1b}", AWAY);
    let evoker_uuid = snapshot(&s, Kind::Evoker, evoker).uuid;
    let vex = summon(
        &mut s,
        Kind::Vex,
        &format!("{{NoAI:1b,owner:{}}}", uuid_text(evoker_uuid)),
        AWAY,
    );
    let vindicator = summon(&mut s, Kind::Vindicator, "{NoAI:1b}", AWAY);
    let stray_vex = summon(&mut s, Kind::Vex, "{NoAI:1b}", AWAY);
    let cow = summon(&mut s, Kind::Cow, "{NoAI:1b}", AWAY);
    summon(
        &mut s,
        Kind::EvokerFangs,
        &format!("{{Warmup:0,Owner:{}}}", uuid_text(evoker_uuid)),
        AWAY,
    );
    let before = |s: &Scene, kind, id| snapshot(s, kind, id).health.unwrap();
    let evoker_health = before(&s, Kind::Evoker, evoker);
    let vex_health = before(&s, Kind::Vex, vex);
    let vindicator_health = before(&s, Kind::Vindicator, vindicator);
    let stray_health = before(&s, Kind::Vex, stray_vex);
    let cow_health = before(&s, Kind::Cow, cow);
    s.tick(12);
    assert_eq!(before(&s, Kind::Evoker, evoker), evoker_health);
    assert_eq!(before(&s, Kind::Vex, vex), vex_health);
    assert_eq!(before(&s, Kind::Vindicator, vindicator), vindicator_health);
    assert_eq!(before(&s, Kind::Cow, cow), cow_health - 6.0);
    assert!(before(&s, Kind::Vex, stray_vex) < stray_health);
}

#[test]
fn fangs_keep_their_warmup_and_owner_across_a_save() {
    let mut s = ready(Vec::new());
    let owner = uuid::Uuid::from_u128(0xabcdef);
    let fangs = summon(
        &mut s,
        Kind::EvokerFangs,
        &format!("{{Warmup:30,Owner:{}}}", uuid_text(owner)),
        AWAY,
    );
    s.tick(5);
    let nbt = saved(&s, fangs);
    assert_eq!(nbt.get_i32("Warmup"), Some(25));
    let uuid = owner.as_u128();
    assert_eq!(
        nbt.get("Owner"),
        Some(&rspigot_nbt::Tag::IntArray(vec![
            (uuid >> 96) as i32,
            (uuid >> 64) as i32,
            (uuid >> 32) as i32,
            uuid as i32,
        ]))
    );
}

#[test]
fn an_evoker_turns_a_blue_sheep_red_but_leaves_other_colors_alone() {
    let mut s = ready(Vec::new());
    let blue = summon(&mut s, Kind::Sheep, "{NoAI:1b,Color:11b}", AWAY);
    let white = summon(
        &mut s,
        Kind::Sheep,
        "{NoAI:1b,Color:0b}",
        Vec3::new(AWAY.x + 1.5, AWAY.y, AWAY.z),
    );
    summon(
        &mut s,
        Kind::Evoker,
        "",
        Vec3::new(AWAY.x - 4.0, AWAY.y, AWAY.z),
    );
    assert!(tick_until(&mut s, 400, |s| saved(s, blue).get_i64("Color") == Some(14)));
    assert_eq!(saved(&s, white).get_i64("Color"), Some(0));
}

#[test]
fn an_evoker_leaves_the_sheep_alone_when_mob_griefing_is_off() {
    let mut s = ready_with(Vec::new(), |config| config.rules.mob_griefing = false);
    let blue = summon(&mut s, Kind::Sheep, "{NoAI:1b,Color:11b}", AWAY);
    summon(
        &mut s,
        Kind::Evoker,
        "",
        Vec3::new(AWAY.x - 4.0, AWAY.y, AWAY.z),
    );
    s.tick(300);
    assert_eq!(saved(&s, blue).get_i64("Color"), Some(11));
}

#[test]
fn an_illusioner_wields_a_bow_and_shoots_arrows_at_a_player() {
    let mut s = ready(Vec::new());
    let illusioner = summon(&mut s, Kind::Illusioner, "", Vec3::new(9.5, 64.0, 0.5));
    let nbt = saved(&s, illusioner);
    let main = nbt
        .get_compound("equipment")
        .and_then(|equipment| equipment.get_compound("mainhand"));
    assert_eq!(
        main.and_then(|stack| stack.get_str("id")),
        Some("minecraft:bow")
    );
    assert!(tick_until(&mut s, 400, |s| !s
        .server
        .entities_of(Kind::Arrow)
        .is_empty()));
}

fn blindness_applied(events: &[ClientEvent]) -> Vec<EffectDuration> {
    let wanted = EffectId::from_name("minecraft:blindness").unwrap();
    events
        .iter()
        .filter_map(|event| match event {
            ClientEvent::EffectApplied { effect, .. } if effect.effect == wanted => {
                Some(effect.duration)
            }
            _ => None,
        })
        .collect()
}

#[test]
fn a_hard_illusioner_blinds_the_player_for_twenty_seconds_and_turns_invisible_by_mirror() {
    let mut s = ready_with(Vec::new(), |config| config.difficulty = Difficulty::Hard);
    let illusioner = summon(&mut s, Kind::Illusioner, "", Vec3::new(9.5, 64.0, 0.5));
    let mut blinded = Vec::new();
    for _ in 0..600 {
        s.tick(1);
        blinded.extend(blindness_applied(&s.players[0].link.take()));
        if !blinded.is_empty() {
            break;
        }
    }
    assert_eq!(blinded, vec![EffectDuration::Ticks(400)]);
    assert!(tick_until(&mut s, 600, |s| {
        effect_ids(s, illusioner)
            .iter()
            .any(|(id, _)| id == "minecraft:invisibility")
    }));
    let invisible = effect_ids(&s, illusioner)
        .into_iter()
        .find(|(id, _)| id == "minecraft:invisibility")
        .unwrap();
    assert!(invisible.1 > 1100 && invisible.1 <= 1200, "{invisible:?}");
}

#[test]
fn an_illusioner_on_easy_difficulty_never_blinds() {
    let mut s = ready_with(Vec::new(), |config| config.difficulty = Difficulty::Easy);
    summon(&mut s, Kind::Illusioner, "", Vec3::new(9.5, 64.0, 0.5));
    let mut blinded = Vec::new();
    for _ in 0..600 {
        s.tick(1);
        blinded.extend(blindness_applied(&s.players[0].link.take()));
    }
    assert!(blinded.is_empty());
}

#[test]
fn the_mirror_image_is_a_spell_event_with_the_disappear_spell() {
    let mut s = ready(vec![deciding(1, EventKind::EntitySpellCast)]);
    summon(&mut s, Kind::Illusioner, "", Vec3::new(9.5, 64.0, 0.5));
    for _ in 0..200 {
        s.tick(1);
        if let Some(last) = asked_for(&s.plugin, EventKind::EntitySpellCast).last() {
            rule_on(&mut s.server, 1, last.id, false, None);
        }
        if spell_events(&s).contains(&SpellcasterSpell::Disappear) {
            break;
        }
    }
    assert!(spell_events(&s).contains(&SpellcasterSpell::Disappear));
}

#[test]
fn a_casting_evoker_saves_its_spell_ticks_and_counts_them_down() {
    let mut s = ready(Vec::new());
    let evoker = summon(&mut s, Kind::Evoker, "", HOME);
    assert!(tick_until(&mut s, 600, |s| saved(s, evoker)
        .get_i32("SpellTicks")
        .is_some_and(|ticks| ticks > 0)));
    let ticks = saved(&s, evoker).get_i32("SpellTicks").unwrap();
    s.tick(1);
    let later = saved(&s, evoker).get_i32("SpellTicks").unwrap();
    assert!(later < ticks || later == 0, "{ticks} {later}");
}

#[test]
fn a_dead_evoker_drops_a_totem_of_undying() {
    let mut s = ready(Vec::new());
    summon(&mut s, Kind::Evoker, "{NoAI:1b}", AWAY);
    console(&mut s, "kill @e[type=evoker]");
    s.tick(3);
    let items: Vec<String> = s
        .server
        .entities_of(Kind::Item)
        .into_iter()
        .filter_map(|item| {
            saved(&s, item.id)
                .get_compound("Item")
                .and_then(|stack| stack.get_str("id").map(str::to_owned))
        })
        .collect();
    assert!(
        items.contains(&"minecraft:totem_of_undying".to_owned()),
        "{items:?}"
    );
}

fn village_scene(difficulty: Difficulty) -> Scene {
    let server = configured_level(
        |config| config.difficulty = difficulty,
        |level| {
            level.day_time = 4_000;
            level.daylight_cycle = false;
        },
    )
    .with_rng_seed(11);
    let mut s = scene_on(server, &["Alice"], Vec::new());
    stream_chunk_of(&mut s, BlockPos::new(1, 64, 1));
    s.act(Order::Block {
        pos: BlockPos::new(8, 64, 8),
        order: BlockOrder::SetWith {
            kind: "minecraft:lectern".to_owned(),
            properties: Vec::new(),
        },
    });
    console(
        &mut s,
        "summon minecraft:villager 7.5 64 8.5 {VillagerData:{type:\"minecraft:plains\",profession:\"minecraft:librarian\",level:2},Xp:20}",
    );
    s.tick(200);
    s
}

fn raiders(s: &Scene) -> Vec<EntityId> {
    [
        Kind::Pillager,
        Kind::Vindicator,
        Kind::Witch,
        Kind::Ravager,
        Kind::Evoker,
    ]
    .into_iter()
    .flat_map(|kind| s.server.entities_of(kind))
    .map(|entity| entity.id)
    .collect()
}

fn tick_until_wave(s: &mut Scene, wave: i32) {
    for _ in 0..1_500 {
        if s.server
            .raids()
            .iter()
            .next()
            .is_some_and(|raid| raid.groups_spawned() >= wave)
        {
            return;
        }
        s.tick(1);
    }
    panic!("wave {wave} never spawned");
}

#[test]
fn the_fifth_wave_of_a_raid_brings_an_evoker_that_joins_the_raid() {
    let mut s = village_scene(Difficulty::Normal);
    console(&mut s, "effect give Alice minecraft:bad_omen 120 0");
    s.tick(650);
    for wave in 1..=4 {
        tick_until_wave(&mut s, wave);
        s.tick(5);
        assert!(s.server.entities_of(Kind::Evoker).is_empty(), "wave {wave}");
        for id in raiders(&s) {
            s.act(Order::Entity {
                entity: id,
                order: EntityOrder::Kill,
            });
        }
        s.tick(2);
    }
    tick_until_wave(&mut s, 5);
    s.tick(5);
    let evokers = s.server.entities_of(Kind::Evoker);
    assert_eq!(evokers.len(), 1);
    let nbt = saved(&s, evokers[0].id);
    assert_eq!(nbt.get_i32("Wave"), Some(5));
    assert_eq!(nbt.get_bool("CanJoinRaid"), Some(true));
    assert!(nbt.get_i32("RaidId").is_some());
    let raid = s.server.raids().iter().next().unwrap();
    assert_eq!(raid.total_raiders_alive() as usize, raiders(&s).len());
}

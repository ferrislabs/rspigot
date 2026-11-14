mod common;

use std::sync::Arc;
use std::time::Duration;

use common::*;
use rspigot_core::config::{GameConfig, LevelData, PluginSettings};
use rspigot_core::events::{Hand, Inbound, PlayerInput};
use rspigot_core::plugins::events::{ExpBottlePatch, PlayerEggThrowPatch};
use rspigot_core::plugins::{
    Answer, EventKind, EventPatch, EventPayload, HandlerSpec, ItemSpec, Question,
};
use rspigot_core::ports::ChunkGenerator;
use rspigot_core::server::Server;
use rspigot_core::status::StatusBoard;
use rspigot_domain::chunk::ChunkColumn;
use rspigot_domain::world::Dimension;
use rspigot_domain::{BiomeId, BlockPos, BlockStateId, ChunkPos, EntityId, Rotation, Vec3};
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

fn exists(s: &Scene, kind: Kind, id: EntityId) -> bool {
    s.server
        .entities_of(kind)
        .into_iter()
        .any(|entity| entity.id == id)
}

fn owner_of(s: &Scene) -> String {
    let (high, low) = s.players[0].id.0.as_u64_pair();
    format!(
        "[I;{},{},{},{}]",
        (high >> 32) as i32,
        high as i32,
        (low >> 32) as i32,
        low as i32
    )
}

fn falling(s: &mut Scene, kind: Kind, extra: &str) -> EntityId {
    let nbt = format!("{{Owner:{},{extra}Motion:[0.0d,-1.0d,0.0d]}}", owner_of(s));
    summon(s, kind, &nbt, Vec3::new(31.5, 66.0, 30.5))
}

#[test]
fn an_experience_bottle_asks_then_drops_orbs_and_a_patch_sets_the_amount() {
    let mut s = ready(vec![deciding(1, EventKind::ExpBottle)]);
    let bottle = falling(&mut s, Kind::ExperienceBottle, "");
    s.tick(4);
    let asked = last_asked(&s.plugin, EventKind::ExpBottle);
    let EventPayload::ExpBottle(event) = &asked.payload else {
        panic!("expected a bottle event");
    };
    assert_eq!(event.entity.id, bottle);
    assert!((3..=11).contains(&event.experience));
    assert!(event.show_effect);
    assert!(event.hit_block.is_some());
    assert!(s.server.entities_of(Kind::ExperienceOrb).is_empty());
    rule_on(
        &mut s.server,
        1,
        asked.id,
        false,
        Some(EventPatch::ExpBottle(ExpBottlePatch {
            experience: Some(0),
            show_effect: None,
        })),
    );
    s.tick(2);
    assert!(s.server.entities_of(Kind::ExperienceOrb).is_empty());
    assert!(!exists(&s, Kind::ExperienceBottle, bottle));
}

#[test]
fn an_experience_bottle_gives_orbs_when_allowed() {
    let mut s = ready(vec![observe(1, EventKind::ExpBottle)]);
    falling(&mut s, Kind::ExperienceBottle, "");
    s.tick(4);
    assert!(!s.server.entities_of(Kind::ExperienceOrb).is_empty());
    assert_eq!(settled_for(&s.plugin, EventKind::ExpBottle).len(), 1);
}

#[test]
fn a_thrown_egg_hatches_the_chickens_a_plugin_asks_for() {
    let mut s = ready(vec![deciding(1, EventKind::PlayerEggThrow)]);
    let egg = falling(&mut s, Kind::Egg, "");
    s.tick(4);
    let asked = last_asked(&s.plugin, EventKind::PlayerEggThrow);
    let EventPayload::PlayerEggThrow(event) = &asked.payload else {
        panic!("expected an egg event");
    };
    assert_eq!(event.egg.id, egg);
    assert_eq!(event.hatching_type, "minecraft:chicken");
    assert_eq!(event.player.name, "Alice");
    assert!(s.server.entities_of(Kind::Chicken).is_empty());
    rule_on(
        &mut s.server,
        1,
        asked.id,
        false,
        Some(EventPatch::PlayerEggThrow(PlayerEggThrowPatch {
            hatching: Some(true),
            hatching_type: None,
            num_hatches: Some(3),
        })),
    );
    s.tick(2);
    assert_eq!(s.server.entities_of(Kind::Chicken).len(), 3);
    assert!(!exists(&s, Kind::Egg, egg));
}

#[test]
fn a_thrown_egg_hatches_nothing_when_the_plugin_refuses() {
    let mut s = ready(vec![deciding(1, EventKind::PlayerEggThrow)]);
    falling(&mut s, Kind::Egg, "");
    s.tick(4);
    let asked = last_asked(&s.plugin, EventKind::PlayerEggThrow);
    rule_on(
        &mut s.server,
        1,
        asked.id,
        false,
        Some(EventPatch::PlayerEggThrow(PlayerEggThrowPatch {
            hatching: Some(false),
            hatching_type: None,
            num_hatches: None,
        })),
    );
    s.tick(2);
    assert!(s.server.entities_of(Kind::Chicken).is_empty());
}

const POISON: &str = "Item:{id:\"minecraft:lingering_potion\",count:1,components:{\"minecraft:potion_contents\":{potion:\"minecraft:poison\"}}},";

#[test]
fn a_lingering_potion_leaves_a_cloud_unless_the_plugin_cancels() {
    let mut s = ready(vec![deciding(1, EventKind::LingeringPotionSplash)]);
    let potion = falling(&mut s, Kind::LingeringPotion, POISON);
    s.tick(4);
    let asked = last_asked(&s.plugin, EventKind::LingeringPotionSplash);
    let EventPayload::LingeringPotionSplash(event) = &asked.payload else {
        panic!("expected a lingering event");
    };
    assert_eq!(event.entity.id, potion);
    assert_eq!(event.area_effect_cloud.kind, "minecraft:area_effect_cloud");
    assert!(s.server.entities_of(Kind::AreaEffectCloud).is_empty());
    rule_on(&mut s.server, 1, asked.id, false, None);
    assert_eq!(s.server.entities_of(Kind::AreaEffectCloud).len(), 1);
    assert!(!exists(&s, Kind::LingeringPotion, potion));

    falling(&mut s, Kind::LingeringPotion, POISON);
    s.tick(4);
    let asked = last_asked(&s.plugin, EventKind::LingeringPotionSplash);
    rule_on(&mut s.server, 1, asked.id, true, None);
    assert_eq!(s.server.entities_of(Kind::AreaEffectCloud).len(), 1);
    assert!(s.server.entities_of(Kind::LingeringPotion).is_empty());
}

#[test]
fn a_player_throws_an_egg_and_spends_one() {
    let mut s = ready(Vec::new());
    console(&mut s, "give Alice egg 2");
    let alice = s.players[0].clone();
    send_input(
        &mut s.server,
        &alice,
        PlayerInput::UseItem {
            hand: Hand::Main,
            sequence: 1,
            rotation: Rotation::new(0.0, -90.0),
        },
    );
    s.tick(1);
    assert_eq!(s.server.entities_of(Kind::Egg).len(), 1);
    match s.ask(Question::Inventory(alice.id)) {
        Answer::Inventory(inventory) => {
            assert_eq!(inventory.slots[0], Some(ItemSpec::of("minecraft:egg", 1)));
        }
        other => panic!("expected the inventory, got {other:?}"),
    }
}

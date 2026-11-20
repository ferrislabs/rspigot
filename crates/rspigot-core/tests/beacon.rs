mod common;

use std::sync::Arc;
use std::time::Duration;

use common::*;
use rspigot_blocks::BlockKind;
use rspigot_core::config::{GameConfig, LevelData};
use rspigot_core::events::{ClientEvent, Inbound};
use rspigot_core::ports::ChunkGenerator;
use rspigot_core::server::Server;
use rspigot_core::status::StatusBoard;
use rspigot_domain::chunk::{BlockEntity, ChunkColumn};
use rspigot_domain::effect::EffectDuration;
use rspigot_domain::world::Dimension;
use rspigot_domain::{BiomeId, BlockPos, BlockStateId, ChunkPos, Identifier, Vec3};
use rspigot_nbt::Compound;

const NEAR: BlockPos = BlockPos::new(17, 64, 8);
const FAR: BlockPos = BlockPos::new(48, 64, 8);

#[derive(Clone)]
struct Spec {
    at: BlockPos,
    layers: i32,
    primary: &'static str,
    secondary: Option<&'static str>,
    extra: Vec<(BlockPos, BlockStateId)>,
}

impl Spec {
    fn new(at: BlockPos, layers: i32, primary: &'static str) -> Self {
        Self {
            at,
            layers,
            primary,
            secondary: None,
            extra: Vec::new(),
        }
    }

    fn and(mut self, secondary: &'static str) -> Self {
        self.secondary = Some(secondary);
        self
    }

    fn with(mut self, at: BlockPos, kind: BlockKind) -> Self {
        self.extra.push((at, kind.default_state()));
        self
    }
}

impl ChunkGenerator for Spec {
    fn generate(&self, dimension: &Dimension, pos: ChunkPos) -> ChunkColumn {
        let mut chunk = ChunkColumn::empty(pos, dimension.height, BiomeId(0));
        for x in 0..16 {
            for z in 0..16 {
                for y in 60..=63 {
                    chunk.set_block(
                        BlockPos::new(pos.min_block_x() + x, y, pos.min_block_z() + z),
                        BlockKind::Stone.default_state(),
                    );
                }
            }
        }
        let iron = BlockKind::IronBlock.default_state();
        for step in 1..=self.layers {
            for dx in -step..=step {
                for dz in -step..=step {
                    let at = BlockPos::new(self.at.x + dx, self.at.y - step, self.at.z + dz);
                    if at.chunk() == pos {
                        chunk.set_block(at, iron);
                    }
                }
            }
        }
        if self.at.chunk() == pos {
            chunk.set_block(self.at, BlockKind::Beacon.default_state());
            let mut data = Compound::new().with("primary_effect", self.primary);
            if let Some(secondary) = self.secondary {
                data.insert("secondary_effect", secondary);
            }
            chunk.set_block_entity(BlockEntity {
                pos: self.at,
                kind: Identifier::minecraft("beacon"),
                data,
            });
        }
        for (at, state) in &self.extra {
            if at.chunk() == pos {
                chunk.set_block(*at, *state);
            }
        }
        chunk
    }

    fn spawn_point(&self, _: &Dimension) -> Vec3 {
        Vec3::new(8.5, 64.0, 8.5)
    }
}

fn scene_of(spec: Spec) -> Scene {
    let config = GameConfig {
        view_distance: 4,
        chunk_workers: 2,
        spawn_protection: 0,
        ..GameConfig::default()
    };
    let data = LevelData {
        spawn_mobs: false,
        day_time: 6_000,
        daylight_cycle: false,
        ..LevelData::default()
    };
    let mut server = Server::new(
        config,
        data,
        Arc::new(NoStore),
        Arc::new(spec),
        StatusBoard::new(20),
    )
    .with_rng_seed(7);
    server.wait_for_spawn(Duration::from_secs(10));
    scene_on(server, &["Alice"], Vec::new())
}

fn run(s: &mut Scene, ticks: u32) -> Vec<ClientEvent> {
    s.tick(ticks);
    s.players[0].link.take()
}

fn effects(events: &[ClientEvent], name: &str) -> Vec<(u8, EffectDuration, bool)> {
    events
        .iter()
        .filter_map(|event| match event {
            ClientEvent::EffectApplied { effect, .. } if effect.effect.name() == Some(name) => {
                Some((effect.amplifier, effect.duration, effect.ambient))
            }
            _ => None,
        })
        .collect()
}

fn sounds(events: &[ClientEvent], name: &str) -> usize {
    events
        .iter()
        .filter(|event| matches!(event, ClientEvent::Sound { sound, .. } if *sound == name))
        .count()
}

#[test]
fn a_level_one_pyramid_gives_the_primary_power_after_the_beat() {
    let mut s = scene_of(Spec::new(NEAR, 1, "minecraft:speed"));
    let events = run(&mut s, 90);
    assert_eq!(
        effects(&events, "minecraft:speed"),
        vec![(0, EffectDuration::Ticks(220), true)]
    );
}

#[test]
fn level_four_with_the_same_powers_doubles_the_amplifier() {
    let mut s = scene_of(Spec::new(NEAR, 4, "minecraft:haste").and("minecraft:haste"));
    let events = run(&mut s, 90);
    assert_eq!(
        effects(&events, "minecraft:haste"),
        vec![(1, EffectDuration::Ticks(340), true)]
    );
}

#[test]
fn level_four_with_different_powers_adds_regeneration() {
    let mut s = scene_of(Spec::new(NEAR, 4, "minecraft:speed").and("minecraft:regeneration"));
    let events = run(&mut s, 90);
    assert_eq!(
        effects(&events, "minecraft:speed"),
        vec![(0, EffectDuration::Ticks(340), true)]
    );
    assert_eq!(
        effects(&events, "minecraft:regeneration"),
        vec![(0, EffectDuration::Ticks(340), true)]
    );
}

#[test]
fn a_player_out_of_range_gets_nothing() {
    let mut s = scene_of(Spec::new(FAR, 1, "minecraft:speed"));
    let events = run(&mut s, 90);
    assert!(effects(&events, "minecraft:speed").is_empty());
}

#[test]
fn an_opaque_block_above_stops_the_effects_and_the_sounds() {
    let spec =
        Spec::new(NEAR, 1, "minecraft:speed").with(BlockPos::new(17, 70, 8), BlockKind::Stone);
    let mut s = scene_of(spec);
    let events = run(&mut s, 90);
    assert!(effects(&events, "minecraft:speed").is_empty());
    assert_eq!(sounds(&events, "minecraft:block.beacon.activate"), 0);
    assert_eq!(sounds(&events, "minecraft:block.beacon.ambient"), 0);
}

#[test]
fn stained_glass_lets_the_beam_and_the_powers_through() {
    let spec = Spec::new(NEAR, 1, "minecraft:speed")
        .with(BlockPos::new(17, 66, 8), BlockKind::RedStainedGlass);
    let mut s = scene_of(spec);
    let events = run(&mut s, 90);
    assert_eq!(effects(&events, "minecraft:speed").len(), 1);
}

#[test]
fn the_levels_follow_the_pyramid_from_one_to_four() {
    for layers in 1..=4 {
        let mut s = scene_of(Spec::new(NEAR, layers, "minecraft:speed"));
        let events = run(&mut s, 90);
        let ticks = u32::try_from((9 + layers * 2) * 20).unwrap();
        assert_eq!(
            effects(&events, "minecraft:speed"),
            vec![(0, EffectDuration::Ticks(ticks), true)],
            "layers {layers}"
        );
    }
}

#[test]
fn a_missing_pyramid_block_lowers_the_levels() {
    let mut s = scene_of(Spec::new(NEAR, 4, "minecraft:speed"));
    console(&mut s, "setblock 19 60 10 air");
    console(&mut s, "setblock 18 61 9 air");
    let events = run(&mut s, 90);
    assert_eq!(
        effects(&events, "minecraft:speed"),
        vec![(0, EffectDuration::Ticks(260), true)]
    );
}

fn console(s: &mut Scene, line: &str) {
    s.server.handle(Inbound::Console(line.to_owned()));
    s.server.deliver();
}

#[test]
fn the_activate_and_deactivate_sounds_fire_once() {
    let mut s = scene_of(Spec::new(NEAR, 1, "minecraft:speed"));
    let events = run(&mut s, 90);
    assert_eq!(sounds(&events, "minecraft:block.beacon.activate"), 1);
    assert_eq!(sounds(&events, "minecraft:block.beacon.ambient"), 1);
    console(&mut s, "setblock 17 70 8 stone");
    let events = run(&mut s, 160);
    assert_eq!(sounds(&events, "minecraft:block.beacon.deactivate"), 1);
    assert_eq!(sounds(&events, "minecraft:block.beacon.activate"), 0);
    assert_eq!(sounds(&events, "minecraft:block.beacon.ambient"), 0);
}

#[test]
fn constructing_a_level_four_beacon_completes_its_advancements() {
    let mut s = scene_of(Spec::new(NEAR, 4, "minecraft:speed"));
    let alice = s.players[0].id;
    assert!(s.server.done_advancements(alice).is_empty());
    s.tick(90);
    let done = s.server.done_advancements(alice);
    assert!(done.contains(&"minecraft:nether/create_beacon"));
    assert!(done.contains(&"minecraft:nether/create_full_beacon"));
}

#[test]
fn a_level_one_beacon_only_completes_the_first_advancement() {
    let mut s = scene_of(Spec::new(NEAR, 1, "minecraft:speed"));
    let alice = s.players[0].id;
    s.tick(90);
    let done = s.server.done_advancements(alice);
    assert!(done.contains(&"minecraft:nether/create_beacon"));
    assert!(!done.contains(&"minecraft:nether/create_full_beacon"));
}

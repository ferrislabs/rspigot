#[path = "blocks_common/mod.rs"]
mod blocks_common;
mod common;

use blocks_common::*;
use common::*;
use rspigot_blocks::BlockKind;
use rspigot_core::events::ClientEvent;
use rspigot_domain::effect::EffectDuration;
use rspigot_domain::{BlockPos, BlockStateId};
use rspigot_entities::EntityKind as Kind;
use rspigot_nbt::Compound;

const CONDUIT: BlockPos = BlockPos::new(14, 66, 8);
const PLAYER_WATER: BlockPos = BlockPos::new(8, 64, 8);
const POOL: BlockPos = BlockPos::new(17, 70, 8);

fn water() -> BlockStateId {
    BlockKind::Water.default_state()
}

fn conduit_cells(frame: usize) -> Cells {
    let mut cells = Cells::default();
    for dx in -1..=1 {
        for dy in -1..=1 {
            for dz in -1..=1 {
                cells.blocks.push((CONDUIT.offset(dx, dy, dz), water()));
            }
        }
    }
    let mut placed = 0;
    for dx in -2i32..=2 {
        for dy in -2i32..=2 {
            for dz in -2i32..=2 {
                let (ax, ay, az) = (dx.abs(), dy.abs(), dz.abs());
                let outside = ax > 1 || ay > 1 || az > 1;
                let ring = dx == 0 && (ay == 2 || az == 2)
                    || dy == 0 && (ax == 2 || az == 2)
                    || dz == 0 && (ax == 2 || ay == 2);
                if outside && ring && placed < frame {
                    cells.blocks.push((
                        CONDUIT.offset(dx, dy, dz),
                        BlockKind::Prismarine.default_state(),
                    ));
                    placed += 1;
                }
            }
        }
    }
    cells.blocks.push((
        CONDUIT,
        BlockKind::Conduit
            .state_with(&[("waterlogged", "true")])
            .unwrap(),
    ));
    cells.blocks.push((PLAYER_WATER, water()));
    cells.blocks.push((POOL, water()));
    cells.entity(CONDUIT, "conduit", Compound::new())
}

fn powers(events: &[ClientEvent]) -> Vec<(u8, EffectDuration, bool)> {
    events
        .iter()
        .filter_map(|event| match event {
            ClientEvent::EffectApplied { effect, .. }
                if effect.effect.name() == Some("minecraft:conduit_power") =>
            {
                Some((effect.amplifier, effect.duration, effect.ambient))
            }
            _ => None,
        })
        .collect()
}

#[test]
fn a_framed_conduit_in_water_gives_conduit_power_to_a_swimmer_in_range() {
    let mut s = scene_of(conduit_cells(42), 3, 0);
    s.tick(45);
    assert_eq!(
        powers(&s.players[0].link.take()),
        vec![(0, EffectDuration::Ticks(260), true)]
    );
}

#[test]
fn a_conduit_with_too_few_frame_blocks_stays_idle() {
    let mut s = scene_of(conduit_cells(15), 3, 0);
    s.tick(45);
    assert!(powers(&s.players[0].link.take()).is_empty());
}

#[test]
fn the_effect_needs_water_or_rain_on_the_player() {
    let mut s = scene_of(conduit_cells(42), 3, 0);
    s.tick(2);
    console(&mut s, "setblock 8 64 8 air");
    s.players[0].link.take();
    s.tick(90);
    assert!(powers(&s.players[0].link.take()).is_empty());
}

#[test]
fn a_conduit_placed_in_the_world_gets_its_block_entity() {
    let mut s = scene_of(Cells::default(), 3, 0);
    console(&mut s, "setblock 3 64 3 conduit");
    assert!(s.server.block_entity_nbt(BlockPos::new(3, 64, 3)).is_some());
}

#[test]
fn a_full_conduit_hurts_a_hostile_mob_in_water_within_eight_blocks() {
    let mut s = scene_of(conduit_cells(42), 3, 0);
    s.tick(2);
    console(&mut s, "summon zombie 17.5 70 8.5 {NoAI:1b}");
    let before = s.server.entities_of(Kind::Zombie)[0].health.unwrap();
    s.tick(45);
    let after = s.server.entities_of(Kind::Zombie)[0].health.unwrap();
    assert_eq!(before - after, 4.0);
}

#[test]
fn a_conduit_with_less_than_forty_two_blocks_does_not_attack() {
    let mut s = scene_of(conduit_cells(41), 3, 0);
    s.tick(2);
    console(&mut s, "summon zombie 17.5 70 8.5 {NoAI:1b}");
    let before = s.server.entities_of(Kind::Zombie)[0].health.unwrap();
    s.tick(45);
    let after = s.server.entities_of(Kind::Zombie)[0].health.unwrap();
    assert_eq!(before, after);
}

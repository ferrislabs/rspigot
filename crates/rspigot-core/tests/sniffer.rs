mod common;

use std::num::NonZeroU8;
use std::sync::Arc;
use std::time::Duration;

use common::*;
use rspigot_blocks::BlockKind;
use rspigot_core::config::{GameConfig, LevelData};
use rspigot_core::events::{
    ClientEvent, Direction, EntityStatus, Hand, Inbound, MetadataValue, PlayerInput,
};
use rspigot_core::ports::ChunkGenerator;
use rspigot_core::server::Server;
use rspigot_core::status::StatusBoard;
use rspigot_domain::chunk::ChunkColumn;
use rspigot_domain::item::ItemStack;
use rspigot_domain::world::Dimension;
use rspigot_domain::{BiomeId, BlockPos, BlockStateId, ChunkPos, EntityId, Vec3};
use rspigot_entities::EntityKind as Kind;
use rspigot_items::ItemKind;

const MOSS: BlockPos = BlockPos::new(2, 63, 2);
const DIRT_EGG_BASE: BlockPos = BlockPos::new(2, 63, 4);
const STATE_INDEX: u8 = 18;
const FARMLAND: BlockPos = BlockPos::new(4, 63, 2);
const FARMLAND_TWO: BlockPos = BlockPos::new(6, 63, 2);

struct Meadow;

impl ChunkGenerator for Meadow {
    fn generate(&self, dimension: &Dimension, pos: ChunkPos) -> ChunkColumn {
        let biome = rspigot_data::registries::biome_id("minecraft:plains").unwrap();
        let mut chunk = ChunkColumn::empty(pos, dimension.height, BiomeId(biome));
        for x in 0..16 {
            for z in 0..16 {
                let at = |y: i32| BlockPos::new(pos.min_block_x() + x, y, pos.min_block_z() + z);
                for y in 60..=62 {
                    chunk.set_block(at(y), BlockStateId(1));
                }
                chunk.set_block(at(63), BlockKind::GrassBlock.default_state());
            }
        }
        if MOSS.chunk() == pos {
            chunk.set_block(MOSS, BlockKind::MossBlock.default_state());
            chunk.set_block(FARMLAND, BlockKind::Farmland.default_state());
            chunk.set_block(FARMLAND_TWO, BlockKind::Farmland.default_state());
        }
        chunk
    }

    fn spawn_point(&self, _: &Dimension) -> Vec3 {
        Vec3::new(0.5, 64.0, 0.5)
    }
}

fn meadow_server(seed: u64) -> Server<Link> {
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
        Arc::new(Meadow),
        StatusBoard::new(20),
    )
    .with_rng_seed(seed);
    server.wait_for_spawn(Duration::from_secs(10));
    server
}

fn ready(seed: u64) -> Scene {
    let mut s = scene_on(meadow_server(seed), &["Alice"], Vec::new());
    stream_chunk_of(&mut s, BlockPos::new(1, 64, 1));
    s.tick(40);
    s
}

fn console(s: &mut Scene, line: &str) {
    s.server.handle(Inbound::Console(line.to_owned()));
    s.server.deliver();
}

fn summon(s: &mut Scene, at: &str, nbt: &str) -> EntityId {
    let known: Vec<EntityId> = s
        .server
        .entities_of(Kind::Sniffer)
        .into_iter()
        .map(|entity| entity.id)
        .collect();
    console(s, &format!("summon minecraft:sniffer {at} {nbt}"));
    s.server
        .entities_of(Kind::Sniffer)
        .into_iter()
        .map(|entity| entity.id)
        .find(|id| !known.contains(id))
        .expect("a new sniffer")
}

fn creative(s: &mut Scene) {
    console(s, "gamemode creative Alice");
}

fn put_in_hand(s: &mut Scene, item: ItemKind) {
    let alice = s.players[0].clone();
    send_input(
        &mut s.server,
        &alice,
        PlayerInput::CreativeSlot {
            slot: 36,
            stack: Some(ItemStack::new(item.id(), NonZeroU8::MIN)),
        },
    );
}

fn place_on(s: &mut Scene, below: BlockPos) {
    let alice = s.players[0].clone();
    send_input(
        &mut s.server,
        &alice,
        PlayerInput::UseItemOn {
            hand: Hand::Main,
            pos: below,
            face: Direction::Up,
            cursor: Vec3::new(0.5, 1.0, 0.5),
            inside_block: false,
            sequence: 1,
        },
    );
}

fn items_on_the_ground(s: &Scene) -> Vec<String> {
    s.server
        .entities_of(Kind::Item)
        .into_iter()
        .filter_map(|item| s.server.saved_entity(item.id))
        .filter_map(|nbt| {
            nbt.get_compound("Item")
                .and_then(|stack| stack.get_str("id").map(str::to_owned))
        })
        .collect()
}

fn saved(s: &Scene, id: EntityId) -> rspigot_nbt::Compound {
    s.server.saved_entity(id).expect("a saved sniffer")
}

fn explored(s: &Scene, id: EntityId) -> Vec<Vec<i32>> {
    saved(s, id)
        .get_compound("Brain")
        .and_then(|brain| brain.get_compound("memories"))
        .and_then(|memories| memories.get_compound("minecraft:sniffer_explored_positions"))
        .and_then(|entry| entry.get_list("value"))
        .map(|list| {
            list.iter()
                .filter_map(|tag| tag.as_compound())
                .filter_map(|entry| entry.get_int_array("pos").map(<[i32]>::to_vec))
                .collect()
        })
        .unwrap_or_default()
}

#[derive(Default)]
struct Seen {
    states: Vec<i32>,
    dig_events: usize,
    sounds: Vec<&'static str>,
}

impl Seen {
    fn absorb(&mut self, events: Vec<ClientEvent>, sniffer: EntityId) {
        for event in events {
            match event {
                ClientEvent::EntityMetadata { entity, entries } if entity == sniffer => {
                    for entry in entries {
                        if let (STATE_INDEX, MetadataValue::State(_, state)) =
                            (entry.index, entry.value)
                        {
                            self.states.push(state);
                        }
                    }
                }
                ClientEvent::EntityStatus {
                    entity,
                    status: EntityStatus::Event(63),
                } if entity == sniffer => self.dig_events += 1,
                ClientEvent::Sound { sound, .. } => self.sounds.push(sound),
                _ => {}
            }
        }
    }
}

fn watch_until_seed(s: &mut Scene, sniffer: EntityId, limit: u32) -> (Seen, bool) {
    let mut seen = Seen::default();
    for _ in 0..limit {
        s.tick(1);
        seen.absorb(s.players[0].link.take(), sniffer);
        let found = items_on_the_ground(s)
            .iter()
            .any(|id| id == "minecraft:torchflower_seeds" || id == "minecraft:pitcher_pod");
        if found {
            for _ in 0..250 {
                s.tick(1);
                seen.absorb(s.players[0].link.take(), sniffer);
            }
            return (seen, true);
        }
    }
    (seen, false)
}

#[test]
fn a_sniffer_on_grass_sniffs_digs_and_unearths_a_seed() {
    let mut s = ready(3);
    let sniffer = summon(&mut s, "5.5 64 5.5", "{}");
    let (seen, found) = watch_until_seed(&mut s, sniffer, 20_000);
    assert!(found, "no seed after twenty thousand ticks");
    for state in [3, 4, 5] {
        assert!(seen.states.contains(&state), "{state} in {:?}", seen.states);
    }
    assert!(seen.dig_events >= 1);
    assert!(seen.sounds.contains(&"minecraft:entity.sniffer.sniffing"));
    assert!(seen.sounds.contains(&"minecraft:entity.sniffer.drop_seed"));
    assert!(
        seen.sounds
            .contains(&"minecraft:entity.sniffer.digging_stop")
            || seen.states.contains(&6)
    );
}

#[test]
fn a_sniffer_that_dug_remembers_the_spot_in_its_save() {
    let mut s = ready(5);
    let sniffer = summon(&mut s, "5.5 64 5.5", "{}");
    let (_, found) = watch_until_seed(&mut s, sniffer, 20_000);
    assert!(found);
    s.tick(400);
    let spots = explored(&s, sniffer);
    assert!(!spots.is_empty(), "no explored position saved");
    assert!(spots.iter().all(|pos| pos.len() == 3 && pos[1] == 63));
}

#[test]
fn explored_positions_survive_a_save_and_load_of_the_sniffer() {
    let mut s = ready(7);
    let sniffer = summon(
        &mut s,
        "5.5 64 5.5",
        "{Brain:{memories:{\"minecraft:sniffer_explored_positions\":{value:[{dimension:\"minecraft:overworld\",pos:[I;5,63,5]}]}}}}",
    );
    s.tick(2);
    assert_eq!(explored(&s, sniffer), vec![vec![5, 63, 5]]);
}

#[test]
fn a_sniffer_never_digs_up_the_same_block_twice() {
    let mut s = ready(11);
    let sniffer = summon(&mut s, "5.5 64 5.5", "{}");
    let (_, found) = watch_until_seed(&mut s, sniffer, 20_000);
    assert!(found);
    s.tick(600);
    let spots = explored(&s, sniffer);
    let mut unique = spots.clone();
    unique.sort();
    unique.dedup();
    assert_eq!(unique.len(), spots.len());
}

#[test]
fn a_digging_sniffer_rises_and_goes_back_to_idling() {
    let mut s = ready(3);
    let sniffer = summon(&mut s, "5.5 64 5.5", "{}");
    let mut seen = Seen::default();
    let mut digging = false;
    for _ in 0..20_000 {
        s.tick(1);
        seen.absorb(s.players[0].link.take(), sniffer);
        if seen.states.last() == Some(&5) {
            digging = true;
            break;
        }
    }
    assert!(digging, "the sniffer never dug");
    for _ in 0..400 {
        s.tick(1);
        seen.absorb(s.players[0].link.take(), sniffer);
        if seen.states.last() == Some(&0) {
            break;
        }
    }
    let dig = seen.states.iter().rposition(|state| *state == 5).unwrap();
    assert_eq!(seen.states[dig + 1..].first(), Some(&6));
    assert_eq!(seen.states.last(), Some(&0));
}

fn lay(s: &mut Scene) -> Vec<EntityId> {
    let first = summon(s, "10.5 64 10.5", "{InLove:600}");
    let second = summon(s, "11.5 64 10.5", "{InLove:600}");
    vec![first, second]
}

#[test]
fn two_sniffers_in_love_lay_a_sniffer_egg_instead_of_a_calf() {
    let mut s = ready(13);
    let parents = lay(&mut s);
    let before = items_on_the_ground(&s)
        .iter()
        .filter(|id| id.as_str() == "minecraft:sniffer_egg")
        .count();
    let mut orbs = 0;
    for _ in 0..600 {
        s.tick(1);
        s.players[0].link.take();
        orbs = s.server.entities_of(Kind::ExperienceOrb).len();
        let eggs = items_on_the_ground(&s)
            .iter()
            .filter(|id| id.as_str() == "minecraft:sniffer_egg")
            .count();
        if eggs > before {
            break;
        }
    }
    let eggs = items_on_the_ground(&s)
        .iter()
        .filter(|id| id.as_str() == "minecraft:sniffer_egg")
        .count();
    assert_eq!(eggs, before + 1);
    assert_eq!(s.server.entities_of(Kind::Sniffer).len(), 2);
    assert!(orbs >= 1);
    for parent in parents {
        let nbt = saved(&s, parent);
        assert_eq!(nbt.get_i32("InLove"), Some(0));
        let age = nbt.get_i32("Age").unwrap_or(0);
        assert!((5_900..=6_000).contains(&age), "{age}");
    }
}

#[test]
fn the_egg_plop_is_heard_when_the_egg_is_laid() {
    let mut s = ready(13);
    lay(&mut s);
    let mut heard = false;
    for _ in 0..600 {
        s.tick(1);
        for event in s.players[0].link.take() {
            if let ClientEvent::Sound { sound, .. } = event
                && sound == "minecraft:block.sniffer_egg.plop"
            {
                heard = true;
            }
        }
        if heard {
            break;
        }
    }
    assert!(heard);
}

#[test]
fn a_calf_born_from_an_egg_takes_two_days_to_grow_and_never_digs() {
    let mut s = ready(17);
    let calf = summon(&mut s, "5.5 64 5.5", "{Age:-48000}");
    s.tick(2);
    let age = saved(&s, calf).get_i32("Age").unwrap();
    assert!((-48_000..-47_990).contains(&age), "{age}");
    let before = items_on_the_ground(&s).len();
    s.tick(6_000);
    assert_eq!(items_on_the_ground(&s).len(), before);
}

fn hatch_of(s: &Scene, base: BlockPos) -> Option<String> {
    let above = BlockPos::new(base.x, base.y + 1, base.z);
    let state = s.server.block_state_at(above)?;
    (state.kind() == Some(BlockKind::SnifferEgg))
        .then(|| state.property("hatch").map(str::to_owned))
        .flatten()
}

#[test]
fn a_sniffer_egg_cracks_twice_and_then_hatches_a_calf_much_faster_on_moss() {
    let mut s = ready(19);
    creative(&mut s);
    put_in_hand(&mut s, ItemKind::SnifferEgg);
    place_on(&mut s, MOSS);
    place_on(&mut s, DIRT_EGG_BASE);
    assert_eq!(hatch_of(&s, MOSS).as_deref(), Some("0"));
    assert_eq!(hatch_of(&s, DIRT_EGG_BASE).as_deref(), Some("0"));
    let mut cracks: Vec<(u32, u32)> = Vec::new();
    let mut moss_stage = 0;
    let mut dirt_stage = 0;
    let mut hatched_at = None;
    for tick in 1..=26_000u32 {
        s.tick(1);
        s.players[0].link.take();
        let moss = hatch_of(&s, MOSS).and_then(|h| h.parse().ok());
        let dirt = hatch_of(&s, DIRT_EGG_BASE).and_then(|h| h.parse().ok());
        if let Some(stage) = moss
            && stage != moss_stage
        {
            moss_stage = stage;
            cracks.push((tick, stage));
        }
        if let Some(stage) = dirt
            && stage != dirt_stage
        {
            dirt_stage = stage;
            cracks.push((tick, 100 + stage));
        }
        if moss.is_none() && hatched_at.is_none() {
            hatched_at = Some(tick);
        }
        if dirt.is_none() {
            break;
        }
    }
    let first_moss = cracks.iter().find(|(_, stage)| *stage == 1).unwrap().0;
    let first_dirt = cracks.iter().find(|(_, stage)| *stage == 101).unwrap().0;
    assert!((4_000..4_300).contains(&first_moss), "{first_moss}");
    assert!((8_000..8_300).contains(&first_dirt), "{first_dirt}");
    let moss_hatched = hatched_at.expect("the moss egg hatched");
    assert!((12_000..12_900).contains(&moss_hatched), "{moss_hatched}");
    let calves = s.server.entities_of(Kind::Sniffer);
    assert_eq!(calves.len(), 2);
    for calf in calves {
        let age = saved(&s, calf.id).get_i32("Age").unwrap();
        assert!(age < -20_000, "{age}");
    }
}

#[test]
fn the_cracking_and_hatching_sounds_are_heard() {
    let mut s = ready(19);
    creative(&mut s);
    put_in_hand(&mut s, ItemKind::SnifferEgg);
    place_on(&mut s, MOSS);
    let mut heard: Vec<&'static str> = Vec::new();
    for _ in 0..13_000 {
        s.tick(1);
        for event in s.players[0].link.take() {
            if let ClientEvent::Sound { sound, .. } = event
                && sound.starts_with("minecraft:block.sniffer_egg")
            {
                heard.push(sound);
            }
        }
        if hatch_of(&s, MOSS).is_none() {
            break;
        }
    }
    assert_eq!(
        heard,
        vec![
            "minecraft:block.sniffer_egg.crack",
            "minecraft:block.sniffer_egg.crack",
            "minecraft:block.sniffer_egg.hatch",
        ]
    );
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
    s.tick(1);
}

fn love_of(s: &Scene, id: EntityId) -> i32 {
    saved(s, id).get_i32("InLove").unwrap_or(0)
}

#[test]
fn torchflower_seeds_and_pitcher_pods_are_sniffer_food_and_nothing_else_is() {
    let mut s = ready(23);
    creative(&mut s);
    let sniffer = summon(&mut s, "1.5 64 1.5", "{NoAI:1b}");
    put_in_hand(&mut s, ItemKind::Wheat);
    interact(&mut s, sniffer);
    assert_eq!(love_of(&s, sniffer), 0);
    put_in_hand(&mut s, ItemKind::TorchflowerSeeds);
    interact(&mut s, sniffer);
    assert!((590..=600).contains(&love_of(&s, sniffer)));
    let other = summon(&mut s, "1.5 64 3.5", "{NoAI:1b}");
    put_in_hand(&mut s, ItemKind::PitcherPod);
    interact(&mut s, other);
    assert!((590..=600).contains(&love_of(&s, other)));
}

#[test]
fn a_calf_fed_its_food_grows_faster() {
    let mut s = ready(29);
    creative(&mut s);
    let calf = summon(&mut s, "1.5 64 1.5", "{NoAI:1b,Age:-48000}");
    put_in_hand(&mut s, ItemKind::TorchflowerSeeds);
    interact(&mut s, calf);
    s.tick(2);
    let age = saved(&s, calf).get_i32("Age").unwrap();
    assert!(age > -48_000 + 4_000, "{age}");
}

#[test]
fn the_seeds_a_sniffer_digs_up_can_be_planted_on_farmland() {
    let mut s = ready(31);
    creative(&mut s);
    put_in_hand(&mut s, ItemKind::TorchflowerSeeds);
    place_on(&mut s, FARMLAND);
    assert_eq!(above(&s, FARMLAND), Some(BlockKind::TorchflowerCrop));
    put_in_hand(&mut s, ItemKind::PitcherPod);
    place_on(&mut s, FARMLAND_TWO);
    assert_eq!(above(&s, FARMLAND_TWO), Some(BlockKind::PitcherCrop));
}

fn above(s: &Scene, pos: BlockPos) -> Option<BlockKind> {
    s.server
        .block_state_at(BlockPos::new(pos.x, pos.y + 1, pos.z))
        .and_then(BlockStateId::kind)
}

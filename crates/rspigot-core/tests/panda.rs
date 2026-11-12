mod common;

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use common::*;
use rspigot_blocks::BlockKind;
use rspigot_core::config::PluginSettings;
use rspigot_core::config::{GameConfig, LevelData};
use rspigot_core::events::{ClientEvent, EquipmentSlot, Hand, Inbound, MetadataValue, PlayerInput};
use rspigot_core::plugins::{EventKind, EventPayload, HandlerSpec};
use rspigot_core::ports::ChunkGenerator;
use rspigot_core::server::Server;
use rspigot_core::status::StatusBoard;
use rspigot_domain::chunk::ChunkColumn;
use rspigot_domain::world::Dimension;
use rspigot_domain::{BiomeId, ChunkPos};
use rspigot_domain::{BlockPos, EntityId, Vec3};
use rspigot_entities::EntityKind as Kind;
use rspigot_items::ItemKind;

const UNHAPPY: u8 = 18;
const SNEEZE_COUNTER: u8 = 19;
const EAT_COUNTER: u8 = 20;
const MAIN_GENE: u8 = 21;
const HIDDEN_GENE: u8 = 22;
const FLAGS: u8 = 23;
const SNEEZE: i8 = 2;
const SIT: i8 = 8;
const ON_BACK: i8 = 16;

#[derive(Default)]
struct Watch {
    data: HashMap<(EntityId, u8), MetadataValue>,
    main_hand: HashMap<EntityId, Option<ItemKind>>,
    sounds: Vec<&'static str>,
    removed: Vec<EntityId>,
}

impl Watch {
    fn feed(&mut self, events: Vec<ClientEvent>) {
        for event in events {
            match event {
                ClientEvent::EntityMetadata { entity, entries } => {
                    for entry in entries {
                        self.data.insert((entity, entry.index), entry.value);
                    }
                }
                ClientEvent::EntityEquipment { entity, slots } => {
                    for (slot, stack) in slots {
                        if slot == EquipmentSlot::MainHand {
                            self.main_hand
                                .insert(entity, stack.and_then(|stack| stack.item.kind()));
                        }
                    }
                }
                ClientEvent::Sound { sound, .. } => self.sounds.push(sound),
                ClientEvent::RemoveEntities(ids) => self.removed.extend(ids),
                _ => {}
            }
        }
    }

    fn byte(&self, entity: EntityId, index: u8) -> i8 {
        match self.data.get(&(entity, index)) {
            Some(MetadataValue::Byte(value)) => *value,
            _ => 0,
        }
    }

    fn int(&self, entity: EntityId, index: u8) -> i32 {
        match self.data.get(&(entity, index)) {
            Some(MetadataValue::VarInt(value)) => *value,
            _ => 0,
        }
    }

    fn flag(&self, entity: EntityId, mask: i8) -> bool {
        self.byte(entity, FLAGS) & mask != 0
    }

    fn holding(&self, entity: EntityId) -> Option<ItemKind> {
        self.main_hand.get(&entity).copied().flatten()
    }

    fn heard(&self, sound: &str) -> usize {
        self.sounds.iter().filter(|heard| **heard == sound).count()
    }
}

struct BambooGrove;

impl ChunkGenerator for BambooGrove {
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
        if pos == ChunkPos::new(0, 0) {
            chunk.set_block(BlockPos::new(5, 63, 5), BlockKind::Dirt.default_state());
            chunk.set_block(BlockPos::new(5, 64, 5), BlockKind::Bamboo.default_state());
        }
        chunk
    }

    fn spawn_point(&self, _: &Dimension) -> Vec3 {
        Vec3::new(0.5, 64.0, 0.5)
    }
}

fn grove() -> Server<Link> {
    let config = GameConfig {
        view_distance: 3,
        chunk_workers: 2,
        spawn_protection: 0,
        plugins: PluginSettings {
            decision_ticks: 1_000,
            ..PluginSettings::default()
        },
        ..GameConfig::default()
    };
    let mut server = Server::new(
        config,
        LevelData::default(),
        Arc::new(NoStore),
        Arc::new(BambooGrove),
        StatusBoard::new(20),
    );
    server.wait_for_spawn(Duration::from_secs(10));
    server
}

struct Zoo {
    s: Scene,
    watch: Watch,
}

impl Zoo {
    fn new(handlers: Vec<HandlerSpec>) -> Self {
        Self::on(
            server(PluginSettings {
                decision_ticks: 1_000,
                ..PluginSettings::default()
            }),
            handlers,
        )
    }

    fn on(mut server: Server<Link>, handlers: Vec<HandlerSpec>) -> Self {
        let plugin = connect_with(&mut server, 1, "tester", handlers);
        let alice = join(&mut server, "Alice");
        server.deliver();
        alice.link.take();
        let mut zoo = Self {
            s: Scene {
                server,
                plugin,
                players: vec![alice],
            },
            watch: Watch::default(),
        };
        stream_chunk_of(&mut zoo.s, BlockPos::new(1, 64, 1));
        zoo.s.players[0].link.take();
        zoo
    }

    fn console(&mut self, line: &str) {
        self.s.server.handle(Inbound::Console(line.to_owned()));
        self.s.server.deliver();
        self.collect();
    }

    fn collect(&mut self) {
        let events = self.s.players[0].link.take();
        self.watch.feed(events);
    }

    fn tick(&mut self, ticks: u32) {
        for _ in 0..ticks {
            self.s.tick(1);
            self.collect();
        }
    }

    fn pandas(&self) -> Vec<EntityId> {
        self.s
            .server
            .entities_of(Kind::Panda)
            .into_iter()
            .map(|panda| panda.id)
            .collect()
    }

    fn summon(&mut self, at: (f64, f64, f64), nbt: &str) -> EntityId {
        let before = self.pandas();
        self.console(&format!(
            "summon minecraft:panda {} {} {} {nbt}",
            at.0, at.1, at.2
        ));
        let panda = *self
            .pandas()
            .iter()
            .find(|id| !before.contains(id))
            .expect("a new panda");
        self.tick(10);
        panda
    }

    fn interact(&mut self, target: EntityId) {
        let alice = self.s.players[0].clone();
        send_input(
            &mut self.s.server,
            &alice,
            PlayerInput::Interact {
                target,
                hand: Hand::Main,
                sneaking: false,
                location: Vec3::new(0.0, 0.5, 0.0),
            },
        );
        self.collect();
    }

    fn attack(&mut self, target: EntityId) {
        let alice = self.s.players[0].clone();
        send_input(&mut self.s.server, &alice, PlayerInput::Attack(target));
        self.collect();
    }

    fn nbt(&self, id: EntityId) -> rspigot_nbt::Compound {
        self.s.server.saved_entity(id).expect("saved panda")
    }

    fn count(&mut self, key: &str) -> i32 {
        use rspigot_core::plugins::{Answer, Question};
        let Answer::Inventory(state) = self.s.ask(Question::Inventory(self.s.players[0].id)) else {
            panic!("expected Alice's inventory");
        };
        state
            .slots
            .iter()
            .flatten()
            .filter(|item| item.key == key)
            .map(|item| item.count)
            .sum()
    }
}

fn calm(extra: &str) -> String {
    format!("{{MainGene:\"normal\",HiddenGene:\"normal\"{extra}}}")
}

#[test]
fn genes_come_from_nbt_are_saved_and_reach_the_client() {
    let mut zoo = Zoo::new(Vec::new());
    let panda = zoo.summon(
        (1.5, 64.0, 1.5),
        "{NoAI:1b,MainGene:\"playful\",HiddenGene:\"brown\"}",
    );
    let saved = zoo.nbt(panda);
    assert_eq!(saved.get_str("MainGene"), Some("playful"));
    assert_eq!(saved.get_str("HiddenGene"), Some("brown"));
    assert_eq!(zoo.watch.byte(panda, MAIN_GENE), 3);
    assert_eq!(zoo.watch.byte(panda, HIDDEN_GENE), 4);
    assert_eq!(zoo.watch.byte(panda, FLAGS), 0);
}

#[test]
fn an_unknown_gene_name_loads_as_normal() {
    let mut zoo = Zoo::new(Vec::new());
    let panda = zoo.summon((1.5, 64.0, 1.5), "{NoAI:1b,MainGene:\"giant\"}");
    let saved = zoo.nbt(panda);
    assert_eq!(saved.get_str("MainGene"), Some("normal"));
    assert_eq!(saved.get_str("HiddenGene"), Some("normal"));
}

#[test]
fn a_plain_summon_rolls_genes_and_loot_pickup_is_on() {
    let mut zoo = Zoo::new(Vec::new());
    let panda = zoo.summon((1.5, 64.0, 1.5), "{NoAI:1b}");
    let saved = zoo.nbt(panda);
    assert!(saved.get_str("MainGene").is_some());
    assert!(saved.get_str("HiddenGene").is_some());
    assert_eq!(zoo.nbt(panda).get_bool("CanPickUpLoot"), Some(true));
    let saved_off = zoo.summon(
        (3.5, 64.0, 1.5),
        "{NoAI:1b,CanPickUpLoot:0b,\"Bukkit.updateLevel\":2}",
    );
    assert_eq!(zoo.nbt(saved_off).get_bool("CanPickUpLoot"), Some(false));
    let legacy = zoo.summon((5.5, 64.0, 1.5), "{NoAI:1b,CanPickUpLoot:0b}");
    assert_eq!(zoo.nbt(legacy).get_bool("CanPickUpLoot"), Some(true));
}

fn feed_twice(zoo: &mut Zoo, panda: EntityId) {
    zoo.interact(panda);
    zoo.interact(panda);
    zoo.tick(1);
}

#[test]
fn bamboo_puts_a_fresh_adult_in_love_before_feeding_it_by_hand() {
    let mut zoo = Zoo::new(Vec::new());
    zoo.console("give Alice minecraft:bamboo 4");
    let panda = zoo.summon((1.5, 64.0, 1.5), &calm(",NoAI:1b"));
    zoo.interact(panda);
    zoo.tick(1);
    assert_eq!(zoo.count("minecraft:bamboo"), 3);
    assert!(
        zoo.nbt(panda)
            .get_i32("InLove")
            .is_some_and(|ticks| ticks > 590)
    );
    assert!(!zoo.watch.flag(panda, SIT));
    assert_eq!(zoo.watch.holding(panda), None);
    zoo.interact(panda);
    zoo.tick(1);
    assert_eq!(zoo.count("minecraft:bamboo"), 2);
    assert!(zoo.watch.flag(panda, SIT));
    assert!(zoo.watch.int(panda, EAT_COUNTER) >= 1);
    assert_eq!(zoo.watch.holding(panda), Some(ItemKind::Bamboo));
    zoo.interact(panda);
    assert_eq!(zoo.count("minecraft:bamboo"), 2);
    zoo.tick(10);
    assert!(zoo.watch.int(panda, EAT_COUNTER) > 1);
    assert!(zoo.watch.heard("minecraft:entity.panda.eat") >= 1);
}

#[test]
fn a_hurt_panda_stands_up() {
    let mut zoo = Zoo::new(Vec::new());
    zoo.console("give Alice minecraft:bamboo 4");
    let panda = zoo.summon((1.5, 64.0, 1.5), &calm(",NoAI:1b"));
    feed_twice(&mut zoo, panda);
    assert!(zoo.watch.flag(panda, SIT));
    zoo.attack(panda);
    zoo.tick(1);
    assert!(!zoo.watch.flag(panda, SIT));
}

#[test]
fn an_invulnerable_panda_still_stands_up_when_hit() {
    let mut zoo = Zoo::new(Vec::new());
    zoo.console("give Alice minecraft:bamboo 4");
    let panda = zoo.summon((1.5, 64.0, 1.5), &calm(",NoAI:1b,Invulnerable:1b"));
    feed_twice(&mut zoo, panda);
    assert!(zoo.watch.flag(panda, SIT));
    zoo.attack(panda);
    zoo.tick(1);
    assert!(!zoo.watch.flag(panda, SIT));
}

#[test]
fn a_panda_that_eats_bamboo_loses_it_when_done_and_stands_up() {
    let mut zoo = Zoo::new(Vec::new());
    zoo.console("give Alice minecraft:bamboo 4");
    let panda = zoo.summon((1.5, 64.0, 1.5), &calm(",NoAI:1b"));
    feed_twice(&mut zoo, panda);
    zoo.tick(1500);
    assert_eq!(zoo.watch.holding(panda), None);
    assert!(!zoo.watch.flag(panda, SIT));
    assert_eq!(zoo.watch.int(panda, EAT_COUNTER), 0);
    assert!(zoo.watch.heard("minecraft:entity.panda.eat") >= 16);
}

#[test]
fn a_lazy_panda_lies_on_its_back_and_a_click_rolls_it_over_without_food() {
    let mut zoo = Zoo::new(Vec::new());
    zoo.console("give Alice minecraft:bamboo 3");
    let panda = zoo.summon((1.5, 64.0, 1.5), "{MainGene:\"lazy\",HiddenGene:\"lazy\"}");
    let mut lay = false;
    for _ in 0..6000 {
        zoo.tick(1);
        if zoo.watch.flag(panda, ON_BACK) {
            lay = true;
            break;
        }
    }
    assert!(lay, "the lazy panda never lay down");
    zoo.interact(panda);
    zoo.tick(1);
    assert!(!zoo.watch.flag(panda, ON_BACK));
    assert_eq!(zoo.count("minecraft:bamboo"), 3);
    assert_eq!(zoo.watch.holding(panda), None);
}

#[test]
fn a_panda_picks_up_bamboo_from_the_ground_only_when_empty_handed() {
    let mut zoo = Zoo::new(Vec::new());
    let panda = zoo.summon((9.5, 64.0, 9.5), &calm(",NoAI:1b"));
    zoo.console(
        "summon minecraft:item 9.5 64 9.5 {Item:{id:\"minecraft:bamboo\",count:1},PickupDelay:0s}",
    );
    zoo.tick(3);
    assert_eq!(zoo.watch.holding(panda), Some(ItemKind::Bamboo));
    assert_eq!(zoo.s.server.items_on_ground(), 0);
    let saved = zoo.nbt(panda);
    let chances = saved.get_compound("drop_chances").expect("drop chances");
    assert_eq!(chances.get_f64("mainhand"), Some(2.0));
    zoo.console(
        "summon minecraft:item 9.5 64 9.5 {Item:{id:\"minecraft:bamboo\",count:1},PickupDelay:0s}",
    );
    zoo.tick(3);
    assert_eq!(zoo.s.server.items_on_ground(), 1);
}

#[test]
fn a_panda_ignores_food_it_does_not_eat_and_items_it_cannot_reach_yet() {
    let mut zoo = Zoo::new(Vec::new());
    let panda = zoo.summon((9.5, 64.0, 9.5), &calm(",NoAI:1b"));
    zoo.console(
        "summon minecraft:item 9.5 64 9.5 {Item:{id:\"minecraft:stick\",count:1},PickupDelay:0s}",
    );
    zoo.console(
        "summon minecraft:item 9.5 64 9.5 {Item:{id:\"minecraft:bamboo\",count:1},PickupDelay:200s}",
    );
    zoo.tick(20);
    assert_eq!(zoo.watch.holding(panda), None);
    assert_eq!(zoo.s.server.items_on_ground(), 2);
}

#[test]
fn a_panda_that_cannot_pick_up_loot_leaves_bamboo_alone() {
    let mut zoo = Zoo::new(Vec::new());
    let panda = zoo.summon(
        (9.5, 64.0, 9.5),
        &calm(",NoAI:1b,CanPickUpLoot:0b,\"Bukkit.updateLevel\":2"),
    );
    zoo.console(
        "summon minecraft:item 9.5 64 9.5 {Item:{id:\"minecraft:bamboo\",count:1},PickupDelay:0s}",
    );
    zoo.tick(10);
    assert_eq!(zoo.watch.holding(panda), None);
    assert_eq!(zoo.s.server.items_on_ground(), 1);
}

#[test]
fn plugins_decide_whether_a_panda_picks_an_item_up() {
    let mut zoo = Zoo::new(vec![deciding(1, EventKind::EntityPickupItem)]);
    let panda = zoo.summon((9.5, 64.0, 9.5), &calm(",NoAI:1b"));
    zoo.console(
        "summon minecraft:item 9.5 64 9.5 {Item:{id:\"minecraft:bamboo\",count:1},PickupDelay:0s}",
    );
    zoo.tick(1);
    let asked = last_asked(&zoo.s.plugin, EventKind::EntityPickupItem);
    let EventPayload::EntityPickupItem(pickup) = &asked.payload else {
        panic!("a pickup");
    };
    assert_eq!(pickup.entity.id, panda);
    assert_eq!(pickup.entity_type, "minecraft:panda");
    assert_eq!(pickup.remaining, 0);
    assert_eq!(zoo.watch.holding(panda), None);
    rule_on(&mut zoo.s.server, 1, asked.id, true, None);
    assert_eq!(zoo.watch.holding(panda), None);
    assert_eq!(zoo.s.server.items_on_ground(), 1);
    zoo.tick(1);
    let asked = last_asked(&zoo.s.plugin, EventKind::EntityPickupItem);
    rule_on(&mut zoo.s.server, 1, asked.id, false, None);
    zoo.collect();
    assert_eq!(zoo.watch.holding(panda), Some(ItemKind::Bamboo));
    assert_eq!(zoo.s.server.items_on_ground(), 0);
}

#[test]
fn a_hungry_panda_walks_to_bamboo_on_the_ground_eats_it_and_stands_up() {
    let mut zoo = Zoo::new(Vec::new());
    let panda = zoo.summon((9.5, 64.0, 9.5), &calm(""));
    zoo.console(
        "summon minecraft:item 13.5 64 9.5 {Item:{id:\"minecraft:bamboo\",count:1},PickupDelay:0s}",
    );
    let mut held = false;
    let mut sat = false;
    for _ in 0..400 {
        zoo.tick(1);
        held |= zoo.watch.holding(panda) == Some(ItemKind::Bamboo);
        sat |= held && zoo.watch.flag(panda, SIT);
        if sat {
            break;
        }
    }
    assert!(held, "the panda never reached the bamboo");
    assert!(sat, "the panda never sat down to eat");
    assert_eq!(zoo.s.server.items_on_ground(), 0);
}

#[test]
fn pandas_in_love_without_bamboo_nearby_get_unhappy_and_do_not_breed() {
    let mut zoo = Zoo::new(Vec::new());
    zoo.console("give Alice minecraft:bamboo 8");
    let first = zoo.summon((1.5, 64.0, 1.5), &calm(""));
    let second = zoo.summon((3.5, 64.0, 1.5), &calm(""));
    zoo.interact(first);
    zoo.interact(second);
    let mut unhappy = false;
    for _ in 0..400 {
        zoo.tick(1);
        unhappy |= zoo.watch.int(first, UNHAPPY) > 0 || zoo.watch.int(second, UNHAPPY) > 0;
    }
    assert!(unhappy, "no panda got unhappy");
    assert!(zoo.watch.heard("minecraft:entity.panda.cant_breed") >= 1);
    assert_eq!(zoo.pandas().len(), 2);
}

#[test]
fn pandas_in_love_with_bamboo_nearby_breed_a_baby_with_genes() {
    let mut zoo = Zoo::on(grove(), Vec::new());
    zoo.console("give Alice minecraft:bamboo 8");
    let first = zoo.summon((1.5, 64.0, 1.5), &calm(""));
    let second = zoo.summon((3.5, 64.0, 1.5), &calm(""));
    zoo.interact(first);
    zoo.interact(second);
    for _ in 0..800 {
        zoo.tick(1);
        if zoo.pandas().len() == 3 {
            break;
        }
    }
    let all = zoo.pandas();
    assert_eq!(all.len(), 3);
    let child = *all
        .iter()
        .find(|id| **id != first && **id != second)
        .expect("a child");
    let saved = zoo.nbt(child);
    assert!(saved.get_i32("Age").is_some_and(|age| age < 0));
    for key in ["MainGene", "HiddenGene"] {
        assert!(saved.get_str(key).is_some(), "{key}");
    }
}

#[test]
fn a_baby_weak_panda_eventually_sneezes_and_the_sneeze_ends() {
    let mut zoo = Zoo::on(
        server(PluginSettings {
            decision_ticks: 1_000,
            ..PluginSettings::default()
        })
        .with_rng_seed(0),
        Vec::new(),
    );
    let baby = zoo.summon(
        (1.5, 64.0, 1.5),
        "{Age:-24000,MainGene:\"weak\",HiddenGene:\"weak\",NoAI:0b}",
    );
    let mut sneezed = false;
    for _ in 0..12_000 {
        zoo.tick(1);
        if zoo.watch.flag(baby, SNEEZE) {
            sneezed = true;
            break;
        }
    }
    assert!(sneezed, "the baby never sneezed");
    zoo.tick(1);
    assert!(zoo.watch.int(baby, SNEEZE_COUNTER) >= 1);
    assert!(zoo.watch.heard("minecraft:entity.panda.pre_sneeze") >= 1);
    zoo.tick(25);
    assert!(!zoo.watch.flag(baby, SNEEZE));
    assert_eq!(zoo.watch.int(baby, SNEEZE_COUNTER), 0);
    assert!(zoo.watch.heard("minecraft:entity.panda.sneeze") >= 1);
}

#[test]
fn an_aggressive_panda_hurt_by_a_player_attacks_back_and_calls_aggressive_friends_only() {
    let mut zoo = Zoo::new(Vec::new());
    let victim = zoo.summon(
        (1.5, 64.0, 2.5),
        "{MainGene:\"aggressive\",HiddenGene:\"aggressive\"}",
    );
    let friend = zoo.summon(
        (4.5, 64.0, 3.5),
        "{MainGene:\"aggressive\",HiddenGene:\"aggressive\"}",
    );
    let bystander = zoo.summon((4.5, 64.0, 6.5), &calm(""));
    let alice = zoo.s.server.player_entity_id(zoo.s.players[0].id);
    zoo.attack(victim);
    zoo.tick(6);
    assert_eq!(zoo.s.server.mob_target(victim), alice);
    assert_eq!(zoo.s.server.mob_target(friend), alice);
    assert_eq!(zoo.s.server.mob_target(bystander), None);
}

#[test]
fn a_panda_feeding_with_a_target_forgets_it() {
    let mut zoo = Zoo::new(Vec::new());
    zoo.console("give Alice minecraft:bamboo 3");
    let panda = zoo.summon(
        (1.5, 64.0, 2.5),
        "{MainGene:\"normal\",HiddenGene:\"normal\"}",
    );
    zoo.attack(panda);
    zoo.tick(6);
    let alice = zoo.s.server.player_entity_id(zoo.s.players[0].id);
    assert_eq!(zoo.s.server.mob_target(panda), alice);
    zoo.interact(panda);
    zoo.tick(4);
    assert_eq!(zoo.s.server.mob_target(panda), None);
}

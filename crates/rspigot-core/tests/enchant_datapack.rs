mod common;

use std::sync::{Arc, Mutex};
use std::time::Duration;

use std::num::NonZeroU8;

use common::{Link, Player, calm_server, join};
use rspigot_core::events::Inbound;
use rspigot_core::ports::{
    DatapackError, DatapackSource, PackContents, PackExtras, PackFile, PackMeta, PackRegistryFile,
};
use rspigot_core::server::Server;
use rspigot_domain::item::ItemStack;
use rspigot_domain::{Identifier, ItemId};

#[derive(Default)]
struct Packs(Mutex<(Vec<PackContents>, Vec<PackExtras>)>);

impl DatapackSource for Packs {
    fn read_packs(&self) -> Result<Vec<PackContents>, DatapackError> {
        Ok(self.0.lock().expect("packs").0.clone())
    }

    fn read_extras(&self) -> Result<Vec<PackExtras>, DatapackError> {
        Ok(self.0.lock().expect("packs").1.clone())
    }
}

fn file(id: &str, text: &str) -> PackFile {
    PackFile {
        id: Identifier::parse(id).expect("id"),
        text: text.to_owned(),
    }
}

fn pack(
    advancements: &[(&str, &str)],
    recipes: &[(&str, &str)],
    registries: &[(&str, &str, &str)],
) -> (Vec<PackContents>, Vec<PackExtras>) {
    (
        vec![PackContents {
            name: "main".to_owned(),
            meta: PackMeta::default(),
            ..PackContents::default()
        }],
        vec![PackExtras {
            name: "main".to_owned(),
            advancements: advancements.iter().map(|(i, t)| file(i, t)).collect(),
            recipes: recipes.iter().map(|(i, t)| file(i, t)).collect(),
            registries: registries
                .iter()
                .map(|(registry, i, t)| PackRegistryFile {
                    registry: (*registry).to_owned(),
                    file: file(i, t),
                })
                .collect(),
        }],
    )
}

struct Scene {
    server: Server<Link>,
    packs: Arc<Packs>,
    _alice: Player,
}

fn scene(initial: (Vec<PackContents>, Vec<PackExtras>)) -> Scene {
    let packs = Arc::new(Packs(Mutex::new(initial)));
    let mut server = calm_server().with_datapacks(Arc::clone(&packs));
    server.start(Duration::from_secs(10));
    let alice = join(&mut server, "Alice");
    server.deliver();
    alice.link.take();
    Scene {
        server,
        packs,
        _alice: alice,
    }
}

impl Scene {
    fn reload(&mut self, packs: (Vec<PackContents>, Vec<PackExtras>)) {
        *self.packs.0.lock().expect("packs") = packs;
        self.server.handle(Inbound::Console("reload".to_owned()));
        self.server.deliver();
    }
}

const BONUS: &str = r##"{
  "description": "bonus",
  "supported_items": "#minecraft:enchantable/mining",
  "weight": 1,
  "max_level": 3,
  "min_cost": {"base": 1, "per_level_above_first": 1},
  "max_cost": {"base": 5, "per_level_above_first": 1},
  "anvil_cost": 1,
  "slots": ["mainhand"],
  "effects": {
    "minecraft:block_experience": [
      {"effect": {"type": "minecraft:multiply", "factor": {"type": "minecraft:linear", "base": 2.0, "per_level_above_first": 1.0}}}
    ],
    "minecraft:item_damage": [
      {"effect": {"type": "minecraft:set", "value": 0.0},
       "requirements": {"condition": "minecraft:match_tool", "predicate": {"items": "#minecraft:enchantable/mining"}}}
    ]
  }
}"##;

const BLUNT_SILK: &str = r##"{
  "description": "silk",
  "supported_items": "#minecraft:enchantable/mining_loot",
  "weight": 1,
  "max_level": 1,
  "min_cost": {"base": 15, "per_level_above_first": 0},
  "max_cost": {"base": 65, "per_level_above_first": 0},
  "anvil_cost": 4,
  "slots": ["mainhand"]
}"##;

fn stack(item: &str, enchantments: &[(&str, u8)]) -> ItemStack {
    let id = rspigot_data::registries::static_index("minecraft:item", item).unwrap();
    let mut stack = ItemStack::new(ItemId(id as u16), NonZeroU8::MIN);
    let levels: Vec<(String, u8)> = enchantments
        .iter()
        .map(|(name, level)| ((*name).to_owned(), *level))
        .collect();
    stack.set_enchantments(&levels);
    stack
}

#[test]
fn a_datapack_adds_and_overrides_enchantment_definitions() {
    let mut s = scene(pack(&[], &[], &[]));
    s.server.tick();
    let silk = stack("minecraft:diamond_pickaxe", &[("minecraft:silk_touch", 1)]);
    let bonus = stack("minecraft:diamond_pickaxe", &[("t:bonus", 2)]);
    assert_eq!(s.server.enchanted_block_experience(Some(&silk), 6), 0);
    assert_eq!(s.server.enchanted_block_experience(Some(&bonus), 6), 6);

    s.reload(pack(
        &[],
        &[],
        &[
            ("enchantment", "t:bonus", BONUS),
            ("enchantment", "minecraft:silk_touch", BLUNT_SILK),
        ],
    ));
    s.server.tick();
    assert_eq!(s.server.enchanted_block_experience(Some(&bonus), 6), 18);
    assert_eq!(s.server.enchanted_item_damage(&bonus, 5), 0);
    assert_eq!(s.server.enchanted_block_experience(Some(&silk), 6), 6);
    let def = rspigot_data::enchantments::by_name("t:bonus").unwrap();
    assert_eq!(def.max_level, 3);
    assert_eq!(def.anvil_cost, 1);

    s.reload(pack(&[], &[], &[]));
    s.server.tick();
    assert_eq!(s.server.enchanted_block_experience(Some(&silk), 6), 0);
    assert_eq!(s.server.enchanted_block_experience(Some(&bonus), 6), 6);
    assert!(rspigot_data::enchantments::by_name("t:bonus").is_none());
}

#[test]
fn a_malformed_datapack_enchantment_is_skipped() {
    let mut s = scene(pack(
        &[],
        &[],
        &[("enchantment", "t:broken", r#"{"max_level": 1}"#)],
    ));
    s.server.tick();
    assert!(rspigot_data::enchantments::by_name("t:broken").is_none());
}

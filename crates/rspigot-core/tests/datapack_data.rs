mod common;

use std::sync::{Arc, Mutex};
use std::time::Duration;

use common::{Link, Player, calm_server, join, texts};
use rand::SeedableRng;
use rand::rngs::StdRng;
use rspigot_blocks::BlockKind;
use rspigot_core::events::Inbound;
use rspigot_core::loot::{self, LootContext};
use rspigot_core::ports::{
    DatapackError, DatapackSource, PackContents, PackFile, PackMeta, PackTag,
};
use rspigot_core::server::Server;
use rspigot_domain::Identifier;
use rspigot_items::ItemKind;

#[derive(Default)]
struct Packs(Mutex<Vec<PackContents>>);

impl DatapackSource for Packs {
    fn read_packs(&self) -> Result<Vec<PackContents>, DatapackError> {
        Ok(self.0.lock().expect("packs").clone())
    }
}

fn file(id: &str, text: &str) -> PackFile {
    PackFile {
        id: Identifier::parse(id).expect("id"),
        text: text.to_owned(),
    }
}

#[derive(Default)]
struct Spec<'a> {
    predicates: &'a [(&'a str, &'a str)],
    tables: &'a [(&'a str, &'a str)],
    tags: &'a [(&'a str, &'a str, &'a str)],
}

fn pack(name: &str, spec: &Spec<'_>) -> PackContents {
    PackContents {
        name: name.to_owned(),
        meta: PackMeta::default(),
        predicates: spec.predicates.iter().map(|(i, t)| file(i, t)).collect(),
        loot_tables: spec.tables.iter().map(|(i, t)| file(i, t)).collect(),
        tags: spec
            .tags
            .iter()
            .map(|(registry, id, text)| PackTag {
                registry: (*registry).to_owned(),
                file: file(id, text),
            })
            .collect(),
        ..PackContents::default()
    }
}

struct Scene {
    server: Server<Link>,
    packs: Arc<Packs>,
    alice: Player,
}

fn scene(initial: Vec<PackContents>) -> Scene {
    let packs = Arc::new(Packs(Mutex::new(initial)));
    let mut server = calm_server().with_datapacks(Arc::clone(&packs));
    server.start(Duration::from_secs(10));
    let alice = join(&mut server, "Alice");
    server.deliver();
    alice.link.take();
    Scene {
        server,
        packs,
        alice,
    }
}

impl Scene {
    fn console(&mut self, line: &str) {
        self.server.handle(Inbound::Console(line.to_owned()));
        self.server.deliver();
    }

    fn holds(&mut self, condition: &str) -> bool {
        self.alice.link.take();
        let line = format!("execute {condition} run tellraw Alice {{\"text\":\"yes\"}}");
        self.console(&line);
        texts(&self.alice).iter().any(|(text, _)| text == "yes")
    }

    fn replace_packs(&mut self, packs: Vec<PackContents>) {
        *self.packs.0.lock().expect("packs") = packs;
        self.console("reload");
    }
}

fn stone_drops(rng_seed: u64) -> Vec<&'static str> {
    let state = BlockKind::from_name("minecraft:stone")
        .expect("stone")
        .default_state();
    let context = LootContext::block(state, None, None);
    let mut rng = StdRng::seed_from_u64(rng_seed);
    loot::block_drops(&context, &mut rng)
        .iter()
        .map(|stack| stack.item.kind().map_or("?", ItemKind::name))
        .collect()
}

const ALWAYS: &str = r#"{"condition":"minecraft:random_chance","chance":1.0}"#;
const NEVER: &str = r#"{"condition":"minecraft:random_chance","chance":0.0}"#;
const ZOMBIE: &str = r#"{"condition":"minecraft:entity_properties","entity":"this","predicate":{"type":"minecraft:zombie"}}"#;

fn predicate_scene() -> Scene {
    scene(vec![pack(
        "main",
        &Spec {
            predicates: &[
                ("t:always", ALWAYS),
                ("t:never", NEVER),
                ("t:zombie", ZOMBIE),
                (
                    "t:ref",
                    r#"{"condition":"minecraft:reference","name":"t:always"}"#,
                ),
                (
                    "t:ref_never",
                    r#"{"condition":"minecraft:reference","name":"t:never"}"#,
                ),
                (
                    "t:inverted",
                    r#"{"condition":"minecraft:inverted","term":{"condition":"minecraft:random_chance","chance":0.0}}"#,
                ),
                (
                    "t:list",
                    r#"[{"condition":"minecraft:random_chance","chance":1.0},{"condition":"minecraft:random_chance","chance":0.0}]"#,
                ),
                (
                    "t:loop_a",
                    r#"{"condition":"minecraft:reference","name":"t:loop_b"}"#,
                ),
                (
                    "t:loop_b",
                    r#"{"condition":"minecraft:reference","name":"t:loop_a"}"#,
                ),
                ("t:broken", "{not json"),
                (
                    "t:unsupported",
                    r#"{"condition":"minecraft:enchantment_active_check","active":true}"#,
                ),
            ],
            ..Spec::default()
        },
    )])
}

#[test]
fn random_chance_one_holds_and_zero_fails() {
    let mut s = predicate_scene();
    assert!(s.holds("as Alice if predicate t:always"));
    assert!(!s.holds("as Alice if predicate t:never"));
    assert!(s.holds("as Alice unless predicate t:never"));
    assert!(!s.holds("as Alice unless predicate t:always"));
}

#[test]
fn references_inversions_and_arrays_compose() {
    let mut s = predicate_scene();
    assert!(s.holds("if predicate t:ref"));
    assert!(!s.holds("if predicate t:ref_never"));
    assert!(s.holds("if predicate t:inverted"));
    assert!(!s.holds("if predicate t:list"));
    assert!(!s.holds("if predicate t:loop_a"));
    assert!(!s.holds("if predicate minecraft:tool/can_silk_touch"));
}

#[test]
fn an_unknown_or_malformed_predicate_is_not_found() {
    let mut s = predicate_scene();
    assert!(!s.holds("if predicate t:nope"));
    assert!(!s.holds("if predicate t:broken"));
    assert!(!s.holds("if predicate t:unsupported"));
    assert!(!s.holds("unless predicate t:broken"));
}

#[test]
fn entity_properties_check_the_executing_entity() {
    let mut s = predicate_scene();
    s.console("summon minecraft:zombie 0 80 0");
    assert!(s.holds("as @e[type=minecraft:zombie,limit=1] if predicate t:zombie"));
    assert!(!s.holds("as Alice if predicate t:zombie"));
}

#[test]
fn the_selector_predicate_option_filters_and_negates() {
    let mut s = predicate_scene();
    s.console("summon minecraft:zombie 0 80 0");
    assert!(s.holds("if entity @e[type=minecraft:zombie,predicate=t:zombie]"));
    assert!(!s.holds("if entity @e[type=minecraft:zombie,predicate=!t:zombie]"));
    assert!(s.holds("if entity @a[predicate=!t:zombie]"));
    assert!(!s.holds("if entity @a[predicate=t:zombie]"));
    assert!(s.holds("if entity @a[predicate=t:always]"));
    assert!(!s.holds("if entity @a[predicate=t:never]"));
}

#[test]
fn reload_applies_a_changed_predicate() {
    let mut s = predicate_scene();
    assert!(!s.holds("if predicate t:flip"));
    s.replace_packs(vec![pack(
        "main",
        &Spec {
            predicates: &[("t:flip", ALWAYS)],
            ..Spec::default()
        },
    )]);
    assert!(s.holds("if predicate t:flip"));
    s.replace_packs(vec![pack(
        "main",
        &Spec {
            predicates: &[("t:flip", NEVER)],
            ..Spec::default()
        },
    )]);
    assert!(!s.holds("if predicate t:flip"));
    assert!(!s.holds("if predicate t:always"));
}

const DIAMOND_TABLE: &str = r#"{"type":"minecraft:block","pools":[{"rolls":1,"entries":[{"type":"minecraft:item","name":"minecraft:diamond"}]}]}"#;
const EMERALD_TABLE: &str = r#"{"type":"minecraft:block","pools":[{"rolls":1,"entries":[{"type":"minecraft:item","name":"minecraft:emerald"}]}]}"#;

#[test]
fn a_datapack_table_overrides_a_vanilla_block_drop() {
    let mut s = scene(vec![pack(
        "main",
        &Spec {
            tables: &[("minecraft:blocks/stone", DIAMOND_TABLE)],
            ..Spec::default()
        },
    )]);
    assert_eq!(stone_drops(1), ["minecraft:diamond"]);
    s.replace_packs(Vec::new());
    assert_eq!(stone_drops(1), ["minecraft:cobblestone"]);
}

#[test]
fn a_new_table_is_reachable_by_id_and_a_later_pack_wins() {
    let first = pack(
        "a",
        &Spec {
            tables: &[
                ("t:mine", DIAMOND_TABLE),
                ("minecraft:blocks/stone", DIAMOND_TABLE),
            ],
            ..Spec::default()
        },
    );
    let second = pack(
        "b",
        &Spec {
            tables: &[
                ("t:mine", EMERALD_TABLE),
                ("minecraft:blocks/stone", EMERALD_TABLE),
            ],
            ..Spec::default()
        },
    );
    let mut s = scene(vec![first.clone(), second]);
    let table = rspigot_data::loot::table("t:mine").expect("new table");
    let mut rng = StdRng::seed_from_u64(3);
    let drops = loot::table_drops(table, &LootContext::default(), &mut rng);
    assert_eq!(drops.len(), 1);
    assert_eq!(
        drops[0].item.kind().map(ItemKind::name),
        Some("minecraft:emerald")
    );
    assert_eq!(stone_drops(2), ["minecraft:emerald"]);
    s.replace_packs(vec![first]);
    assert_eq!(stone_drops(2), ["minecraft:diamond"]);
}

#[test]
fn a_loot_table_with_an_unsupported_function_skips_that_entry() {
    let _s = scene(vec![pack(
        "main",
        &Spec {
            tables: &[(
                "t:partial",
                r#"{"pools":[{"rolls":1,"entries":[{"type":"minecraft:item","name":"minecraft:diamond","functions":[{"function":"minecraft:set_name","name":"x"}]},{"type":"minecraft:item","name":"minecraft:emerald"}]}]}"#,
            )],
            ..Spec::default()
        },
    )]);
    let table = rspigot_data::loot::table("t:partial").expect("table");
    let mut rng = StdRng::seed_from_u64(3);
    let drops = loot::table_drops(table, &LootContext::default(), &mut rng);
    assert_eq!(drops.len(), 1);
    assert_eq!(
        drops[0].item.kind().map(ItemKind::name),
        Some("minecraft:emerald")
    );
}

const PICKAXE: &str = "minecraft:mineable/pickaxe";

fn stone_is(s: &mut Scene, tag: &str) -> bool {
    s.console("setblock 0 64 0 minecraft:stone");
    s.holds(&format!("if block 0 64 0 #{tag}"))
}

#[test]
fn a_datapack_tag_changes_a_block_predicate_lookup() {
    let mut s = scene(vec![pack("main", &Spec::default())]);
    assert!(stone_is(&mut s, PICKAXE));
    assert!(!stone_is(&mut s, "t:hard"));
    s.replace_packs(vec![pack(
        "main",
        &Spec {
            tags: &[
                ("block", "t:hard", r#"{"values":["minecraft:stone"]}"#),
                (
                    "block",
                    PICKAXE,
                    r#"{"replace":true,"values":["minecraft:dirt"]}"#,
                ),
            ],
            ..Spec::default()
        },
    )]);
    assert!(stone_is(&mut s, "t:hard"));
    assert!(!stone_is(&mut s, PICKAXE));
    s.replace_packs(Vec::new());
    assert!(stone_is(&mut s, PICKAXE));
    assert!(!stone_is(&mut s, "t:hard"));
}

#[test]
fn tags_append_nest_and_skip_missing_optional_entries() {
    let mut s = scene(vec![pack(
        "main",
        &Spec {
            tags: &[
                ("block", "t:inner", r#"{"values":["minecraft:stone"]}"#),
                (
                    "block",
                    "t:outer",
                    r##"{"values":["#t:inner",{"id":"t:missing","required":false},{"id":"#t:missing_tag","required":false}]}"##,
                ),
                (
                    "block",
                    "t:broken",
                    r##"{"values":["#t:inner","t:missing"]}"##,
                ),
                (
                    "block",
                    "minecraft:mineable/shovel",
                    r#"{"values":["minecraft:stone"]}"#,
                ),
            ],
            ..Spec::default()
        },
    )]);
    assert!(stone_is(&mut s, "t:outer"));
    assert!(stone_is(&mut s, "minecraft:mineable/shovel"));
    assert!(stone_is(&mut s, "minecraft:mineable/pickaxe"));
    assert!(!stone_is(&mut s, "t:broken"));
}

#[test]
fn tags_of_other_registries_are_merged_too() {
    let _s = scene(vec![pack(
        "main",
        &Spec {
            tags: &[("item", "t:gems", r#"{"values":["minecraft:diamond"]}"#)],
            ..Spec::default()
        },
    )]);
    let diamond = rspigot_data::registries::static_index("minecraft:item", "minecraft:diamond")
        .expect("diamond");
    let ids = rspigot_data::tags::tag("minecraft:item", "t:gems").expect("merged tag");
    assert_eq!(ids, [diamond]);
}

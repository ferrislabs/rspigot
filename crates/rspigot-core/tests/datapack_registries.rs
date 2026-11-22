mod common;

use std::sync::{Arc, Mutex};
use std::time::Duration;

use common::{Link, Player, calm_server, join};
use rspigot_core::events::{ClientEvent, Inbound};
use rspigot_core::ports::{
    DatapackError, DatapackSource, PackContents, PackExtras, PackFile, PackMeta, PackRegistryFile,
};
use rspigot_core::server::Server;
use rspigot_domain::Identifier;

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
    alice: Player,
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
        alice,
    }
}

impl Scene {
    fn reload(&mut self, packs: (Vec<PackContents>, Vec<PackExtras>)) {
        *self.packs.0.lock().expect("packs") = packs;
        self.server.handle(Inbound::Console("reload".to_owned()));
        self.server.deliver();
    }
}

const SHAPELESS: &str = r#"{"type":"minecraft:crafting_shapeless","ingredients":["minecraft:stick"],"result":{"id":"minecraft:diamond"}}"#;
const IMPOSSIBLE: &str = r#"{"criteria":{"c":{"trigger":"minecraft:impossible"}}}"#;

#[test]
fn datapack_recipes_load_at_start_and_reload_with_the_book_resent() {
    let mut s = scene(pack(&[], &[("t:sticks", SHAPELESS)], &[]));
    let id = s.alice.id;
    assert_eq!(s.server.award_recipes(id, &["t:sticks"]), 1);
    assert_eq!(s.server.award_recipes(id, &["t:more"]), 0);
    s.alice.link.take();
    s.reload(pack(&[], &[("t:more", SHAPELESS)], &[]));
    let events = s.alice.link.take();
    assert!(events.iter().any(|e| matches!(e, ClientEvent::RecipeSync)));
    assert!(events.iter().any(|e| matches!(e, ClientEvent::UpdateTags)));
    assert!(
        events
            .iter()
            .any(|e| matches!(e, ClientEvent::RecipeBookRemove(ids) if ids.len() == 1))
    );
    assert!(
        events
            .iter()
            .any(|e| matches!(e, ClientEvent::RecipeBookAdd { replace: true, .. }))
    );
    assert_eq!(s.server.award_recipes(id, &["t:more"]), 1);
    assert_eq!(s.server.award_recipes(id, &["t:sticks"]), 0);
}

#[test]
fn a_malformed_datapack_recipe_is_skipped_and_the_rest_load() {
    let mut s = scene(pack(&[], &[("t:bad", "{"), ("t:ok", SHAPELESS)], &[]));
    let id = s.alice.id;
    assert_eq!(s.server.award_recipes(id, &["t:bad", "t:ok"]), 1);
}

#[test]
fn datapack_advancements_load_and_reload_keeps_progress_by_id() {
    let mut s = scene(pack(&[], &[], &[]));
    let id = s.alice.id;
    assert!(!s.server.award_advancement(id, "a:first"));
    assert!(s.server.award_advancement(id, "minecraft:story/root"));
    assert!(s.server.advancement_is_done(id, "minecraft:story/root"));
    s.alice.link.take();
    s.reload(pack(&[("a:first", IMPOSSIBLE)], &[], &[]));
    let events = s.alice.link.take();
    assert!(
        events
            .iter()
            .any(|e| matches!(e, ClientEvent::Advancements { reset: true, .. }))
    );
    assert!(s.server.advancement_is_done(id, "minecraft:story/root"));
    assert!(s.server.award_advancement(id, "a:first"));
    assert!(s.server.advancement_is_done(id, "a:first"));
    s.reload(pack(&[], &[], &[]));
    assert!(!s.server.award_advancement(id, "a:first"));
    assert!(s.server.advancement_is_done(id, "minecraft:story/root"));
}

#[test]
fn a_datapack_advancement_overrides_the_built_in_one() {
    let mut s = scene(pack(&[("minecraft:story/root", IMPOSSIBLE)], &[], &[]));
    let id = s.alice.id;
    assert!(s.server.award_advancement(id, "minecraft:story/root"));
    assert_eq!(
        s.server.obtained_criteria(id, "minecraft:story/root"),
        ["c"]
    );
}

#[test]
fn registries_reach_the_overlay_and_the_enchantment_hand_off() {
    let mut s = scene(pack(
        &[],
        &[],
        &[
            ("trim_pattern", "t:p", r#"{"asset_id":"t:p"}"#),
            (
                "dialog",
                "t:d",
                r#"{"type":"minecraft:notice","title":"x"}"#,
            ),
            ("enchantment", "t:e", r#"{"max_level":3}"#),
            ("trial_spawner", "t:s", r#"{"spawn_range":4}"#),
            ("villager_trade", "t:v", r#"{}"#),
            ("trade_set", "t:ts", r#"{}"#),
            ("painting_variant", "t:pv", r#"{"width":1,"height":1}"#),
            ("jukebox_song", "t:j", r#"{"length_in_seconds":3}"#),
            ("cat_variant", "t:c", r#"{}"#),
        ],
    ));
    for (registry, id) in [
        ("trim_pattern", "t:p"),
        ("dialog", "t:d"),
        ("trial_spawner", "t:s"),
        ("villager_trade", "t:v"),
        ("trade_set", "t:ts"),
        ("painting_variant", "t:pv"),
        ("jukebox_song", "t:j"),
        ("cat_variant", "t:c"),
    ] {
        let found = s.server.datapack_registry(&format!("minecraft:{registry}"));
        assert!(found.iter().any(|(entry, _)| entry == id), "{registry}");
    }
    let enchantments = s.server.datapack_enchantments();
    assert_eq!(enchantments.len(), 1);
    assert_eq!(enchantments[0].id.as_str(), "t:e");
    assert!(enchantments[0].json.contains("max_level"));
    s.reload(pack(&[], &[], &[]));
    assert!(s.server.datapack_registry("minecraft:dialog").is_empty());
    assert!(s.server.datapack_enchantments().is_empty());
}

#[test]
fn a_world_datapack_folder_loads_through_the_real_loader() {
    let root = std::env::temp_dir().join(format!("rspigot-real-{}", std::process::id()));
    let write = |relative: &str, text: &str| {
        let path = root.join(relative);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    };
    write(
        "real/pack.mcmeta",
        r#"{"pack":{"description":"r","pack_format":94}}"#,
    );
    write("real/data/t/advancement/hello.json", IMPOSSIBLE);
    write("real/data/t/recipe/sticks.json", SHAPELESS);
    write(
        "real/data/t/dialog/d.json",
        r#"{"type":"minecraft:notice","title":"x"}"#,
    );
    let source = Arc::new(rspigot_anvil::DirectoryDatapacks::new(&root));
    let mut server = calm_server().with_datapacks(source);
    server.start(Duration::from_secs(10));
    let alice = join(&mut server, "Alice");
    server.deliver();
    let id = alice.id;
    assert!(server.award_advancement(id, "t:hello"));
    assert_eq!(server.award_recipes(id, &["t:sticks"]), 1);
    assert_eq!(server.datapack_registry("minecraft:dialog").len(), 1);
    write("real/data/t/recipe/later.json", SHAPELESS);
    server.handle(Inbound::Console("reload".to_owned()));
    server.deliver();
    assert_eq!(server.award_recipes(id, &["t:later"]), 1);
    std::fs::remove_dir_all(&root).unwrap();
}

#[test]
fn enabling_and_disabling_a_pack_reapplies_its_recipes_and_advancements() {
    let root = std::env::temp_dir().join(format!("rspigot-toggle-{}", std::process::id()));
    let write = |relative: &str, text: &str| {
        let path = root.join(relative);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    };
    write(
        "tg/pack.mcmeta",
        r#"{"pack":{"description":"r","pack_format":94}}"#,
    );
    write("tg/data/t/advancement/toggled.json", IMPOSSIBLE);
    write("tg/data/t/recipe/toggled.json", SHAPELESS);
    let source = Arc::new(rspigot_anvil::DirectoryDatapacks::new(&root));
    let mut server = calm_server().with_datapacks(source);
    server.start(Duration::from_secs(10));
    let alice = join(&mut server, "Alice");
    server.deliver();
    let id = alice.id;
    let run = |server: &mut Server<Link>, line: &str| {
        server.handle(Inbound::Console(line.to_owned()));
        server.deliver();
    };
    run(&mut server, "datapack disable \"file/tg\"");
    assert_eq!(server.award_recipes(id, &["t:toggled"]), 0);
    assert!(!server.award_advancement(id, "t:toggled"));
    run(&mut server, "datapack enable \"file/tg\"");
    assert_eq!(server.award_recipes(id, &["t:toggled"]), 1);
    assert!(server.award_advancement(id, "t:toggled"));
    std::fs::remove_dir_all(&root).unwrap();
}

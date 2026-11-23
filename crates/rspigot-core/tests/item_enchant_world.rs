mod common;

use common::*;
use rspigot_core::config::PluginSettings;
use rspigot_core::events::{ClientEvent, Inbound};
use rspigot_core::plugins::{
    Answer, InventoryOrder, ItemSpec, Order, PlayerOrder, Question, WorldOrder,
};
use rspigot_domain::Vec3;
use rspigot_entities::EntityKind as Kind;

fn scene_of() -> Scene {
    let mut s = scene_on(
        server(PluginSettings::default()).with_rng_seed(4),
        &["Alice"],
        Vec::new(),
    );
    s.tick(2);
    s
}

fn console(s: &mut Scene, line: &str) {
    s.server.handle(Inbound::Console(line.to_owned()));
    s.server.deliver();
}

fn enchanted(key: &str, enchantment: &str) -> ItemSpec {
    let mut spec = ItemSpec::of(key, 1);
    spec.enchantments = vec![(enchantment.to_owned(), 4)];
    spec
}

fn wear(s: &mut Scene, slot: i32, item: ItemSpec) {
    let alice = s.players[0].clone();
    s.act(Order::Inventory {
        player: alice.id,
        order: InventoryOrder::Set {
            slot,
            item: Some(item),
        },
    });
}

fn player_fire_after_lava(s: &mut Scene) -> i32 {
    let alice = s.players[0].clone();
    s.act(to(&alice, PlayerOrder::SetHealth(20.0)));
    console(s, "gamemode survival Alice");
    console(s, "setblock 0 64 0 minecraft:lava");
    s.tick(1);
    let Answer::Player(state) = s.ask(Question::Player(alice.id)) else {
        panic!("expected Alice");
    };
    state.fire_ticks
}

#[test]
fn fire_protection_shortens_the_burning_time_of_a_player() {
    let mut bare = scene_of();
    let base = player_fire_after_lava(&mut bare);
    let mut protected = scene_of();
    wear(
        &mut protected,
        38,
        enchanted("minecraft:iron_chestplate", "minecraft:fire_protection"),
    );
    protected.tick(1);
    let reduced = player_fire_after_lava(&mut protected);
    assert!(base >= 250, "{base}");
    assert!(reduced > 0 && reduced <= 130, "{reduced}");
}

#[test]
fn fire_protection_shortens_the_burning_time_of_a_mob() {
    let burn = |chest: Option<&str>| {
        let mut s = scene_of();
        let equipment = chest.map_or(String::new(), |enchantment| {
            format!(
                ",equipment:{{chest:{{id:\"minecraft:iron_chestplate\",count:1,components:{{\"minecraft:enchantments\":{{\"{enchantment}\":4}}}}}}}}"
            )
        });
        console(&mut s, "setblock 8 64 8 minecraft:lava");
        console(
            &mut s,
            &format!("summon minecraft:villager 8.5 64 8.5 {{NoAI:1b{equipment}}}"),
        );
        s.tick(1);
        let id = s.server.entities_of(Kind::Villager)[0].id;
        let Answer::Entity(view) = s.ask(Question::Entity(id)) else {
            panic!("expected the villager");
        };
        view.fire_ticks
    };
    let base = burn(None);
    let reduced = burn(Some("minecraft:fire_protection"));
    assert!(base >= 250, "{base}");
    assert!(reduced > 0 && reduced <= 130, "{reduced}");
}

fn blast_on_player(s: &mut Scene) -> f64 {
    let alice = s.players[0].clone();
    let _ = alice.link.take_raw();
    s.act(Order::World(WorldOrder::Explode {
        position: Vec3::new(2.5, 64.0, 0.5),
        power: 4.0,
        breaks_blocks: false,
    }));
    s.tick(1);
    alice
        .link
        .take_raw()
        .into_iter()
        .find_map(|event| match event {
            ClientEvent::Explode { knockback, .. } => knockback,
            _ => None,
        })
        .map_or(0.0, |push| push.x.abs())
}

#[test]
fn blast_protection_reduces_the_explosion_knockback_of_a_player() {
    let mut bare = scene_of();
    let base = blast_on_player(&mut bare);
    let mut protected = scene_of();
    wear(
        &mut protected,
        38,
        enchanted("minecraft:iron_chestplate", "minecraft:blast_protection"),
    );
    protected.tick(1);
    let reduced = blast_on_player(&mut protected);
    assert!(base > 0.1, "{base}");
    assert!(reduced < base * 0.6, "{reduced} {base}");
}

#[test]
fn blast_protection_reduces_the_explosion_knockback_of_a_mob() {
    let push = |enchantment: Option<&str>| {
        let mut s = scene_of();
        let equipment = enchantment.map_or(String::new(), |name| {
            format!(
                ",equipment:{{chest:{{id:\"minecraft:iron_chestplate\",count:1,components:{{\"minecraft:enchantments\":{{\"{name}\":4}}}}}}}}"
            )
        });
        console(
            &mut s,
            &format!("summon minecraft:villager 6.5 64 0.5 {{NoAI:1b{equipment}}}"),
        );
        let id = s.server.entities_of(Kind::Villager)[0].id;
        s.act(Order::World(WorldOrder::Explode {
            position: Vec3::new(4.5, 64.0, 0.5),
            power: 4.0,
            breaks_blocks: false,
        }));
        s.tick(1);
        s.server
            .entities_of(Kind::Villager)
            .iter()
            .find(|entity| entity.id == id)
            .map_or(0.0, |entity| entity.velocity.x.abs())
    };
    let base = push(None);
    let reduced = push(Some("minecraft:blast_protection"));
    assert!(base > 0.1, "{base}");
    assert!(reduced < base * 0.6, "{reduced} {base}");
}

mod destroyed {
    use super::*;
    use rspigot_nbt::{Compound, Tag};

    fn spill_after_blast(spec: ItemSpec) -> Vec<String> {
        let mut s = scene_of();
        let _ = s.ask(Question::DropItem {
            position: Vec3::new(8.5, 64.0, 0.5),
            item: spec,
        });
        assert_eq!(s.server.entities_of(Kind::Item).len(), 1);
        s.act(Order::World(WorldOrder::Explode {
            position: Vec3::new(8.5, 64.0, 1.5),
            power: 4.0,
            breaks_blocks: false,
        }));
        s.tick(2);
        s.server
            .entities_of(Kind::Item)
            .iter()
            .filter_map(|item| s.server.saved_entity(item.id))
            .filter_map(|data| {
                data.get_compound("Item")
                    .and_then(|stack| stack.get_str("id").map(str::to_owned))
            })
            .collect()
    }

    fn with(mut spec: ItemSpec, key: &str, value: Tag) -> ItemSpec {
        rspigot_nbt::write_network(
            &mut spec.components,
            &Tag::Compound(Compound::new().with(key, value)),
        );
        spec
    }

    #[test]
    fn a_destroyed_bundle_spills_its_contents() {
        let list = Tag::List(vec![
            Tag::Compound(
                Compound::new()
                    .with("id", "minecraft:stone")
                    .with("count", 5),
            ),
            Tag::Compound(
                Compound::new()
                    .with("id", "minecraft:dirt")
                    .with("count", 2),
            ),
        ]);
        let mut kinds = spill_after_blast(with(
            ItemSpec::of("minecraft:bundle", 1),
            "minecraft:bundle_contents",
            list,
        ));
        kinds.sort();
        assert_eq!(kinds, vec!["minecraft:dirt", "minecraft:stone"]);
    }

    #[test]
    fn a_destroyed_shulker_box_spills_its_items() {
        let list = Tag::List(vec![Tag::Compound(
            Compound::new().with("slot", 0).with(
                "item",
                Compound::new()
                    .with("id", "minecraft:diamond")
                    .with("count", 3),
            ),
        )]);
        let kinds = spill_after_blast(with(
            ItemSpec::of("minecraft:shulker_box", 1),
            "minecraft:container",
            list,
        ));
        assert_eq!(kinds, vec!["minecraft:diamond"]);
    }

    #[test]
    fn an_ordinary_item_just_disappears() {
        assert!(spill_after_blast(ItemSpec::of("minecraft:stick", 1)).is_empty());
    }
}

mod common;
#[path = "common/screen.rs"]
mod screen;

use std::sync::atomic::{AtomicU64, Ordering};

use common::*;
use rspigot_core::events::{BookDisplay, BookEntry, ClickInput, ClientEvent, Inbound, PlayerInput};
use rspigot_core::plugins::events::{
    CraftItem, FurnaceSmelt, FurnaceStartSmelt, PlayerRecipeBookClick, PlayerRecipeDiscover,
    PrepareItemCraft,
};
use rspigot_core::plugins::{
    ActionError, ActionErrorKind, Answer, EventKind, EventPayload, FromPlugin, IngredientSpec,
    MAX_PLUGIN_RECIPES, Order, PlayerOrder, Question, RecipeKind, RecipeShape, RecipeSpec,
    ServerOrder, ToPlugin,
};
use rspigot_domain::BlockPos;
use rspigot_domain::plugin_recipes::Category;
use rspigot_items::CookingKind;
use screen::*;

static NEXT: AtomicU64 = AtomicU64::new(50_000);

const TABLE: BlockPos = BlockPos::new(0, 64, 2);
const FURNACE: BlockPos = BlockPos::new(0, 64, -2);
const HOTBAR: i16 = 30;
const WAND: &str = "t:wand";
const BRIDGE: &str = "t:bridge";

fn item(key: &str) -> IngredientSpec {
    IngredientSpec::Items(vec![key.to_owned()])
}

fn planks() -> IngredientSpec {
    IngredientSpec::Tag("minecraft:planks".to_owned())
}

fn named(key: &str, count: i32, name: &str) -> rspigot_core::plugins::ItemSpec {
    let mut result = spec(key, count);
    result.name = Some(name.to_owned());
    result
}

fn recipe(id: &str, shape: RecipeShape, result: rspigot_core::plugins::ItemSpec) -> RecipeSpec {
    RecipeSpec {
        id: id.to_owned(),
        shape,
        result,
        group: String::new(),
        category: Category::Misc,
        override_vanilla: false,
        auto_discover: false,
    }
}

fn wand() -> RecipeSpec {
    recipe(
        WAND,
        RecipeShape::Shaped {
            pattern: vec!["S".to_owned(), "P".to_owned()],
            keys: vec![('S', item("minecraft:stick")), ('P', planks())],
            mirror: true,
        },
        named("minecraft:diamond_sword", 1, "Wand"),
    )
}

fn bridge() -> RecipeSpec {
    recipe(
        BRIDGE,
        RecipeShape::Shaped {
            pattern: vec!["ABA".to_owned(), " B ".to_owned()],
            keys: vec![('A', item("minecraft:stick")), ('B', planks())],
            mirror: true,
        },
        spec("minecraft:emerald", 3),
    )
}

fn cooking(id: &str, kind: CookingKind) -> RecipeSpec {
    recipe(
        id,
        RecipeShape::Cooking {
            kind,
            ingredient: item("minecraft:stick"),
            experience: 2.0,
            time: 20,
        },
        spec("minecraft:diamond", 2),
    )
}

fn perform(s: &mut Scene, connection: u64, mailbox: &Mailbox, order: Order) -> Option<Answer> {
    let request = NEXT.fetch_add(1, Ordering::Relaxed);
    common::order(
        &mut s.server,
        connection,
        FromPlugin::Action { request, order },
    );
    mailbox
        .received()
        .into_iter()
        .find_map(|message| match message {
            ToPlugin::Answer {
                request: answered,
                answer,
            } if answered == request => Some(answer),
            _ => None,
        })
}

fn add(s: &mut Scene, spec: RecipeSpec) -> Option<Answer> {
    let mailbox = s.plugin.clone();
    perform(
        s,
        1,
        &mailbox,
        Order::Server(ServerOrder::AddRecipe(Box::new(spec))),
    )
}

fn added(s: &mut Scene, spec: RecipeSpec) {
    assert_eq!(add(s, spec), Some(Answer::Done));
}

fn refused(answer: Option<Answer>) -> ActionError {
    match answer {
        Some(Answer::Failed(error)) => error,
        other => panic!("expected a refusal, got {other:?}"),
    }
}

fn recipes(s: &mut Scene) -> Vec<(String, RecipeKind, bool)> {
    match s.ask(Question::Recipes) {
        Answer::Recipes(views) => views
            .into_iter()
            .map(|view| (view.id, view.kind, view.plugin))
            .collect(),
        other => panic!("unexpected {other:?}"),
    }
}

fn has(s: &mut Scene, id: &str) -> bool {
    recipes(s).iter().any(|(known, _, _)| known == id)
}

fn discover(s: &mut Scene, player: &Player, id: &str) {
    s.act(to(player, PlayerOrder::DiscoverRecipe(id.to_owned())));
}

fn entries(player: &Player) -> Vec<BookEntry> {
    player
        .link
        .take()
        .into_iter()
        .filter_map(|event| match event {
            ClientEvent::RecipeBookAdd { entries, .. } => Some(entries),
            _ => None,
        })
        .flatten()
        .collect()
}

fn place(s: &mut Scene, player: &Player, window: i32, display_id: i32) {
    send_input(
        &mut s.server,
        player,
        PlayerInput::PlaceRecipe {
            window,
            display_id,
            max: false,
        },
    );
}

#[test]
fn invalid_recipes_are_refused_with_an_invalid_argument() {
    let mut s = scene(&["Alice"]);
    let shaped = |pattern: &[&str], keys: &[(char, IngredientSpec)]| {
        recipe(
            "t:bad",
            RecipeShape::Shaped {
                pattern: pattern.iter().map(|row| (*row).to_owned()).collect(),
                keys: keys.to_vec(),
                mirror: true,
            },
            spec("minecraft:stick", 1),
        )
    };
    let stick = || item("minecraft:stick");
    let mut bad = vec![
        shaped(&[], &[]),
        shaped(&["   "], &[]),
        shaped(&["AB"], &[('A', stick())]),
        shaped(&["A"], &[('A', stick()), ('B', stick())]),
        shaped(&["A", "A", "A", "A"], &[('A', stick())]),
        shaped(&["AAAA"], &[('A', stick())]),
        shaped(&["AA", "A"], &[('A', stick())]),
        shaped(&["A"], &[('A', item("minecraft:nope"))]),
        shaped(
            &["A"],
            &[('A', IngredientSpec::Tag("minecraft:nope".to_owned()))],
        ),
        shaped(&["A"], &[('A', IngredientSpec::Items(Vec::new()))]),
        shaped(&["A"], &[('A', item("minecraft:air"))]),
        recipe(
            "t:bad",
            RecipeShape::Shapeless(Vec::new()),
            spec("minecraft:stick", 1),
        ),
        recipe(
            "t:bad",
            RecipeShape::Shapeless(vec![stick(); 10]),
            spec("minecraft:stick", 1),
        ),
        recipe(
            "t:bad",
            RecipeShape::Shapeless(vec![stick()]),
            spec("minecraft:nope", 1),
        ),
        recipe(
            "t:bad",
            RecipeShape::Shapeless(vec![stick()]),
            spec("minecraft:air", 1),
        ),
        recipe(
            "t:bad",
            RecipeShape::Shapeless(vec![stick()]),
            spec("minecraft:stick", 0),
        ),
        recipe(
            "bad",
            RecipeShape::Shapeless(vec![stick()]),
            spec("minecraft:stick", 1),
        ),
        recipe(
            "T:Bad",
            RecipeShape::Shapeless(vec![stick()]),
            spec("minecraft:stick", 1),
        ),
        recipe(
            "minecraft:stick",
            RecipeShape::Shapeless(vec![stick()]),
            spec("minecraft:stick", 1),
        ),
    ];
    let mut slow = cooking("t:bad", CookingKind::Smelting);
    if let RecipeShape::Cooking { time, .. } = &mut slow.shape {
        *time = 0;
    }
    bad.push(slow);
    let mut negative = cooking("t:bad", CookingKind::Smelting);
    if let RecipeShape::Cooking { experience, .. } = &mut negative.shape {
        *experience = -1.0;
    }
    bad.push(negative);
    for spec in bad {
        let label = format!("{spec:?}");
        let error = refused(add(&mut s, spec));
        assert_eq!(error.kind, ActionErrorKind::InvalidArgument, "{label}");
    }
    assert!(!has(&mut s, "t:bad"));
    added(&mut s, wand());
    let error = refused(add(&mut s, wand()));
    assert_eq!(error.kind, ActionErrorKind::InvalidArgument);
}

#[test]
fn a_plugin_holds_at_most_the_recipe_limit() {
    let mut s = scene(&["Alice"]);
    for index in 0..MAX_PLUGIN_RECIPES {
        added(
            &mut s,
            recipe(
                &format!("t:many_{index}"),
                RecipeShape::Shapeless(vec![item("minecraft:stick")]),
                spec("minecraft:stick", 1),
            ),
        );
    }
    let error = refused(add(
        &mut s,
        recipe(
            "t:one_too_many",
            RecipeShape::Shapeless(vec![item("minecraft:stick")]),
            spec("minecraft:stick", 1),
        ),
    ));
    assert_eq!(error.kind, ActionErrorKind::LimitReached);
    let plugin = recipes(&mut s).iter().filter(|(_, _, own)| *own).count();
    assert_eq!(plugin, MAX_PLUGIN_RECIPES);
}

#[test]
fn discovering_sends_the_book_entry_and_undiscovering_takes_it_back() {
    let mut s = scene_with(
        &["Alice"],
        vec![observe(1, EventKind::PlayerRecipeDiscover)],
    );
    let alice = s.players[0].clone();
    added(&mut s, wand());
    assert!(entries(&alice).is_empty());
    discover(&mut s, &alice, WAND);
    let book = entries(&alice);
    assert_eq!(book.len(), 1);
    assert!(book[0].notify && book[0].highlight);
    assert!(matches!(
        &book[0].display,
        BookDisplay::Shaped {
            width: 1,
            height: 2,
            ..
        }
    ));
    assert_eq!(book[0].category, 3);
    assert_eq!(s.server.known_recipes(alice.id), vec![WAND]);
    let events = fired(&s.plugin);
    let EventPayload::PlayerRecipeDiscover(PlayerRecipeDiscover { recipe, .. }) =
        &events[0].payload
    else {
        panic!("a discovery");
    };
    assert_eq!(recipe, WAND);
    match s.ask(Question::DiscoveredRecipes(alice.id)) {
        Answer::RecipeIds(ids) => assert_eq!(ids, vec![WAND.to_owned()]),
        other => panic!("unexpected {other:?}"),
    }
    s.act(to(&alice, PlayerOrder::UndiscoverRecipe(WAND.to_owned())));
    let removed: Vec<i32> = alice
        .link
        .take()
        .into_iter()
        .filter_map(|event| match event {
            ClientEvent::RecipeBookRemove(ids) => Some(ids),
            _ => None,
        })
        .flatten()
        .collect();
    assert_eq!(removed, vec![book[0].display_id]);
    assert!(s.server.known_recipes(alice.id).is_empty());
    discover(&mut s, &alice, "t:nope");
    assert_eq!(s.warnings().len(), 1);
}

#[test]
fn the_player_grid_places_and_crafts_a_plugin_recipe_with_a_tag() {
    let mut s = scene_with(
        &["Alice"],
        vec![
            observe(1, EventKind::PrepareItemCraft),
            observe(2, EventKind::PlayerRecipeBookClick),
        ],
    );
    let alice = s.players[0].clone();
    added(&mut s, wand());
    discover(&mut s, &alice, WAND);
    let display = entries(&alice)[0].display_id;
    give(&mut s, &alice, 0, "minecraft:stick", 4);
    give(&mut s, &alice, 1, "minecraft:spruce_planks", 4);
    alice.link.take();
    place(&mut s, &alice, 0, display);
    let mut view = View::inventory();
    view.apply(&alice.link.take());
    assert_eq!(view.at(1), Some(("minecraft:stick", 1)));
    assert_eq!(view.at(3), Some(("minecraft:spruce_planks", 1)));
    assert_eq!(view.at(0), Some(("minecraft:diamond_sword", 1)));
    let prepared = fired(&s.plugin)
        .into_iter()
        .filter_map(|event| match event.payload {
            EventPayload::PrepareItemCraft(PrepareItemCraft { recipe, .. }) => recipe,
            _ => None,
        })
        .collect::<Vec<_>>();
    assert!(prepared.iter().any(|id| id == WAND), "{prepared:?}");
    let clicked = fired(&s.plugin)
        .into_iter()
        .find_map(|event| match event.payload {
            EventPayload::PlayerRecipeBookClick(PlayerRecipeBookClick {
                recipe,
                original_recipe,
                ..
            }) => Some((recipe, original_recipe)),
            _ => None,
        });
    assert_eq!(clicked, Some((WAND.to_owned(), WAND.to_owned())));
}

#[test]
fn a_crafting_table_places_and_crafts_a_three_by_three_plugin_recipe() {
    let mut s = scene_with(&["Alice"], vec![observe(1, EventKind::CraftItem)]);
    let alice = s.players[0].clone();
    added(&mut s, bridge());
    discover(&mut s, &alice, BRIDGE);
    let display = entries(&alice)[0].display_id;
    set_block(&mut s, TABLE, "minecraft:crafting_table");
    give(&mut s, &alice, 0, "minecraft:stick", 4);
    give(&mut s, &alice, 1, "minecraft:oak_planks", 4);
    alice.link.take();
    let mut table = open_block(&mut s, &alice, TABLE);
    place(&mut s, &alice, table.window, display);
    refresh(&alice, &mut table);
    assert_eq!(table.at(1), Some(("minecraft:stick", 1)));
    assert_eq!(table.at(2), Some(("minecraft:oak_planks", 1)));
    assert_eq!(table.at(3), Some(("minecraft:stick", 1)));
    assert_eq!(table.at(5), Some(("minecraft:oak_planks", 1)));
    assert_eq!(table.at(0), Some(("minecraft:emerald", 3)));
    let take = predict(
        &mut table,
        press(0, 0, ClickInput::Pickup),
        &[(0, None)],
        stack("minecraft:emerald", 3),
    );
    send_click(&mut s, &alice, &mut table, take);
    let crafted: Vec<String> = fired(&s.plugin)
        .into_iter()
        .filter_map(|event| match event.payload {
            EventPayload::CraftItem(CraftItem { recipe, .. }) => Some(recipe),
            _ => None,
        })
        .collect();
    assert_eq!(crafted, vec![BRIDGE.to_owned()]);
    assert_eq!(table.held(), Some(("minecraft:emerald", 3)));
}

#[test]
fn removing_a_recipe_stops_it_matching_and_only_its_owner_may_remove_it() {
    let mut s = scene(&["Alice"]);
    let alice = s.players[0].clone();
    let other = connect(&mut s.server, 2, "other");
    added(&mut s, wand());
    discover(&mut s, &alice, WAND);
    let display = entries(&alice)[0].display_id;
    common::order(
        &mut s.server,
        2,
        FromPlugin::Order(Order::Server(ServerOrder::RemoveRecipe(WAND.to_owned()))),
    );
    assert!(has(&mut s, WAND));
    assert!(
        other
            .received()
            .iter()
            .any(|message| matches!(message, ToPlugin::Warning(_)))
    );
    s.act(Order::Server(ServerOrder::RemoveRecipe(WAND.to_owned())));
    assert!(!has(&mut s, WAND));
    let removed =
        alice.link.take().into_iter().any(
            |event| matches!(event, ClientEvent::RecipeBookRemove(ids) if ids == vec![display]),
        );
    assert!(removed);
    assert!(s.server.known_recipes(alice.id).is_empty());
    give(&mut s, &alice, 0, "minecraft:stick", 4);
    give(&mut s, &alice, 1, "minecraft:oak_planks", 4);
    alice.link.take();
    place(&mut s, &alice, 0, display);
    let mut view = View::inventory();
    view.apply(&alice.link.take());
    assert_eq!(view.at(1), None);
    assert_eq!(view.at(0), None);
    discover(&mut s, &alice, WAND);
    assert_eq!(s.warnings().len(), 1);
}

#[test]
fn a_vanilla_recipe_wins_unless_the_plugin_one_overrides_it() {
    let mut s = scene(&["Alice"]);
    let log = [
        Some(stack("minecraft:oak_log", 1).expect("a stack")),
        None,
        None,
        None,
    ];
    let plain = || {
        recipe(
            "t:logs",
            RecipeShape::Shapeless(vec![item("minecraft:oak_log")]),
            spec("minecraft:diamond", 1),
        )
    };
    added(&mut s, plain());
    let made = |_: &Scene| rspigot_domain::crafting::result(2, 2, &log).map(|made| made.item);
    assert_eq!(
        made(&s),
        stack("minecraft:oak_planks", 1).map(|planks| planks.item)
    );
    s.act(Order::Server(ServerOrder::RemoveRecipe(
        "t:logs".to_owned(),
    )));
    let mut overriding = plain();
    overriding.override_vanilla = true;
    added(&mut s, overriding);
    assert_eq!(
        made(&s),
        stack("minecraft:diamond", 1).map(|diamond| diamond.item)
    );
    s.act(Order::Server(ServerOrder::RemoveRecipe(
        "t:logs".to_owned(),
    )));
    assert_eq!(
        made(&s),
        stack("minecraft:oak_planks", 1).map(|planks| planks.item)
    );
}

#[test]
fn a_disconnected_plugin_loses_its_recipes_and_the_books_forget_them() {
    let mut s = scene(&["Alice"]);
    let alice = s.players[0].clone();
    let other = connect(&mut s.server, 2, "other");
    assert_eq!(
        perform(
            &mut s,
            2,
            &other,
            Order::Server(ServerOrder::AddRecipe(Box::new(recipe(
                "t:gone",
                RecipeShape::Shapeless(vec![item("minecraft:stick")]),
                spec("minecraft:diamond", 1),
            )))),
        ),
        Some(Answer::Done)
    );
    assert!(has(&mut s, "t:gone"));
    discover(&mut s, &alice, "t:gone");
    alice.link.take();
    s.server
        .handle(Inbound::PluginDisconnected { connection: 2 });
    s.server.deliver();
    assert!(!has(&mut s, "t:gone"));
    assert!(s.server.known_recipes(alice.id).is_empty());
    assert!(
        alice
            .link
            .take()
            .iter()
            .any(|event| matches!(event, ClientEvent::RecipeBookRemove(_)))
    );
    assert!(
        rspigot_domain::crafting::result(2, 2, &[stack("minecraft:stick", 1), None, None, None])
            .is_none()
    );
}

#[test]
fn a_replacing_plugin_connection_clears_the_recipes_of_the_old_one() {
    let mut s = scene(&["Alice"]);
    added(&mut s, wand());
    let pair = [
        stack("minecraft:stick", 1),
        stack("minecraft:oak_planks", 1),
    ];
    assert!(rspigot_domain::crafting::result(1, 2, &pair).is_some());
    let _again = connect(&mut s.server, 3, "tester");
    assert!(rspigot_domain::crafting::result(1, 2, &pair).is_none());
}

#[test]
fn an_auto_discovered_recipe_reaches_the_book_when_an_ingredient_is_picked_up() {
    let mut s = scene(&["Alice"]);
    let alice = s.players[0].clone();
    let mut auto = wand();
    auto.auto_discover = true;
    added(&mut s, auto);
    s.tick(2);
    let custom = |player: &Player| {
        entries(player)
            .into_iter()
            .filter(|entry| entry.display_id >= 1_000_000)
            .count()
    };
    assert_eq!(custom(&alice), 0);
    give(&mut s, &alice, 0, "minecraft:stick", 1);
    s.tick(2);
    assert_eq!(custom(&alice), 1);
    assert!(s.server.known_recipes(alice.id).contains(&WAND));
}

fn furnace_scene(block: &str) -> (Scene, Player, View) {
    let settings = rspigot_core::config::PluginSettings {
        decision_ticks: 100,
        ..rspigot_core::config::PluginSettings::default()
    };
    let mut s = scene_on(
        server(settings),
        &["Alice"],
        vec![
            observe(1, EventKind::FurnaceStartSmelt),
            observe(2, EventKind::FurnaceSmelt),
        ],
    );
    let alice = s.players[0].clone();
    set_block(&mut s, FURNACE, block);
    give(&mut s, &alice, 0, "minecraft:stick", 2);
    give(&mut s, &alice, 1, "minecraft:coal", 2);
    alice.link.take();
    let mut view = open_block(&mut s, &alice, FURNACE);
    for (hotbar, slot, item) in [
        (HOTBAR, 0, ("minecraft:stick", 2)),
        (HOTBAR + 1, 1, ("minecraft:coal", 2)),
    ] {
        let take = predict(
            &mut view,
            press(hotbar, 0, ClickInput::Pickup),
            &[(hotbar, None)],
            stack(item.0, item.1),
        );
        send_click(&mut s, &alice, &mut view, take);
        let put = predict(
            &mut view,
            press(slot, 0, ClickInput::Pickup),
            &[(slot, stack(item.0, item.1))],
            None,
        );
        send_click(&mut s, &alice, &mut view, put);
    }
    (s, alice, view)
}

#[test]
fn furnaces_blast_furnaces_and_smokers_cook_a_plugin_recipe_with_its_time_and_result() {
    for (block, kind, total) in [
        ("minecraft:furnace", CookingKind::Smelting, 20),
        ("minecraft:blast_furnace", CookingKind::Blasting, 10),
        ("minecraft:smoker", CookingKind::Smoking, 10),
    ] {
        let (mut s, alice, mut view) = furnace_scene(block);
        added(&mut s, cooking("t:cook", kind));
        s.tick(u32::try_from(total).unwrap() + 6);
        view = {
            close_window(&mut s, &alice, &mut view);
            open_block(&mut s, &alice, FURNACE)
        };
        assert_eq!(view.at(2), Some(("minecraft:diamond", 2)), "{block}");
        assert_eq!(view.at(0), Some(("minecraft:stick", 1)), "{block}");
        let starts: Vec<(String, i32)> = fired(&s.plugin)
            .into_iter()
            .filter_map(|event| match event.payload {
                EventPayload::FurnaceStartSmelt(FurnaceStartSmelt {
                    recipe,
                    total_cook_time,
                    ..
                }) => Some((recipe, total_cook_time)),
                _ => None,
            })
            .collect();
        assert_eq!(
            starts.first(),
            Some(&("t:cook".to_owned(), total)),
            "{block}"
        );
        let smelted = fired(&s.plugin).into_iter().any(|event| {
            matches!(&event.payload, EventPayload::FurnaceSmelt(FurnaceSmelt { result, .. })
                if result.key == "minecraft:diamond" && result.count == 2)
        });
        assert!(smelted, "{block}");
    }
}

#[test]
fn a_plugin_cutting_recipe_is_offered_after_the_vanilla_cuts_and_cuts_with_its_count() {
    let mut s = scene(&["Alice"]);
    let alice = s.players[0].clone();
    added(
        &mut s,
        recipe(
            "t:cut",
            RecipeShape::Stonecutting(item("minecraft:stone")),
            named("minecraft:diamond", 3, "Gem"),
        ),
    );
    assert!(
        alice
            .link
            .take()
            .iter()
            .any(|event| matches!(event, ClientEvent::RecipeSync))
    );
    let offers = rspigot_domain::stonecutting::offers_for(rspigot_items::ItemKind::Stone.id());
    let button = i32::try_from(offers.len() - 1).unwrap();
    assert_eq!(offers.last().map(|offer| offer.id), Some("t:cut"));
    set_block(&mut s, TABLE, "minecraft:stonecutter");
    give(&mut s, &alice, 0, "minecraft:stone", 2);
    alice.link.take();
    let mut view = open_block(&mut s, &alice, TABLE);
    let take = predict(
        &mut view,
        press(29, 0, ClickInput::Pickup),
        &[(29, None)],
        stack("minecraft:stone", 2),
    );
    send_click(&mut s, &alice, &mut view, take);
    let put = predict(
        &mut view,
        press(0, 0, ClickInput::Pickup),
        &[(0, stack("minecraft:stone", 2))],
        None,
    );
    send_click(&mut s, &alice, &mut view, put);
    send_input(
        &mut s.server,
        &alice,
        PlayerInput::ContainerButtonClick {
            window: view.window,
            button,
        },
    );
    refresh(&alice, &mut view);
    assert_eq!(view.at(1), Some(("minecraft:diamond", 3)));
}

#[test]
fn a_plugin_smithing_recipe_is_accepted_and_the_recipe_kinds_are_listed() {
    let mut s = scene(&["Alice"]);
    added(
        &mut s,
        recipe(
            "t:smith",
            RecipeShape::Smithing {
                template: item("minecraft:netherite_upgrade_smithing_template"),
                base: item("minecraft:stick"),
                addition: item("minecraft:netherite_ingot"),
            },
            spec("minecraft:trident", 1),
        ),
    );
    added(&mut s, cooking("t:camp", CookingKind::Campfire));
    added(&mut s, wand());
    let listed = recipes(&mut s);
    let kind = |id: &str| listed.iter().find(|(known, _, _)| known == id).cloned();
    assert_eq!(
        kind("t:smith"),
        Some(("t:smith".to_owned(), RecipeKind::Smithing, true))
    );
    assert_eq!(
        kind("t:camp"),
        Some(("t:camp".to_owned(), RecipeKind::Campfire, true))
    );
    assert_eq!(
        kind(WAND),
        Some((WAND.to_owned(), RecipeKind::Shaped, true))
    );
    assert_eq!(
        kind("minecraft:stick"),
        Some(("minecraft:stick".to_owned(), RecipeKind::Shaped, false))
    );
    assert!(listed.len() <= rspigot_core::plugins::MAX_RECIPE_LIST);
}

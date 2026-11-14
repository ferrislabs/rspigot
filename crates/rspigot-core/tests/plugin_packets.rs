mod common;

use common::*;
use rspigot_core::events::PlayerInput;
use rspigot_core::plugins::events::{
    PlayerCustomClick, PlayerResourcePackStatus, ResourcePackStatus,
};
use rspigot_core::plugins::{EventKind, EventPayload};
use uuid::Uuid;

#[test]
fn a_resource_pack_answer_reaches_the_plugins_with_its_pack_and_status() {
    let mut s = scene_with(
        &["Alice"],
        vec![observe(1, EventKind::PlayerResourcePackStatus)],
    );
    let alice = s.players[0].clone();
    let pack = Uuid::from_u128(7);
    send_input(
        &mut s.server,
        &alice,
        PlayerInput::ResourcePack {
            id: pack,
            status: ResourcePackStatus::Declined,
        },
    );
    let events = fired(&s.plugin);
    assert_eq!(events.len(), 1);
    let EventPayload::PlayerResourcePackStatus(PlayerResourcePackStatus {
        pack_id,
        player,
        status,
    }) = &events[0].payload
    else {
        panic!("expected a pack status, got {:?}", events[0]);
    };
    assert_eq!(*pack_id, pack);
    assert_eq!(player.id, alice.id);
    assert_eq!(*status, ResourcePackStatus::Declined);
}

#[test]
fn a_custom_click_reaches_the_plugins_with_its_id_and_data() {
    let mut s = scene_with(&["Alice"], vec![observe(1, EventKind::PlayerCustomClick)]);
    let alice = s.players[0].clone();
    send_input(
        &mut s.server,
        &alice,
        PlayerInput::CustomClick {
            id: "shop:buy".to_owned(),
            data: Some("{\"n\":1}".to_owned()),
        },
    );
    let events = fired(&s.plugin);
    assert_eq!(events.len(), 1);
    let EventPayload::PlayerCustomClick(PlayerCustomClick {
        data,
        click_id,
        player,
    }) = &events[0].payload
    else {
        panic!("expected a custom click, got {:?}", events[0]);
    };
    assert_eq!(click_id, "shop:buy");
    assert_eq!(data.as_deref(), Some("{\"n\":1}"));
    assert_eq!(player.id, alice.id);
}

mod signs {
    use super::*;
    use rspigot_core::events::{ClientEvent, Direction, Hand, Inbound};
    use rspigot_core::plugins::events::{
        PlayerSignOpen, PlayerSignOpenCause, Side, SignChange, SignChangePatch,
    };
    use rspigot_core::plugins::{BlockOrder, EventPatch, ListDiff, Order};
    use rspigot_domain::{BlockPos, Vec3};

    const SIGN: BlockPos = BlockPos::new(2, 64, 2);

    fn console(s: &mut Scene, line: &str) {
        s.server.handle(Inbound::Console(line.to_owned()));
        s.server.deliver();
    }

    fn go(s: &mut Scene, line: &str) {
        console(s, line);
        let alice = s.players[0].clone();
        let id = alice.link.take().into_iter().find_map(|event| match event {
            ClientEvent::Teleport { id, .. } => Some(id),
            _ => None,
        });
        send_input(
            &mut s.server,
            &alice,
            PlayerInput::TeleportConfirm(id.expect("a teleport")),
        );
        s.tick(2);
    }

    fn standing(handlers: Vec<rspigot_core::plugins::HandlerSpec>) -> Scene {
        let mut s = scene_with(&["Alice"], handlers);
        stream_chunk_of(&mut s, SIGN);
        s.act(Order::Block {
            pos: SIGN,
            order: BlockOrder::SetWith {
                kind: "minecraft:oak_sign".to_owned(),
                properties: vec![("rotation".to_owned(), "0".to_owned())],
            },
        });
        go(&mut s, "tp Alice 2.5 64 5.5");
        s.players[0].link.take();
        s
    }

    fn click(s: &mut Scene) {
        let alice = s.players[0].clone();
        send_input(
            &mut s.server,
            &alice,
            PlayerInput::UseItemOn {
                hand: Hand::Main,
                pos: SIGN,
                face: Direction::South,
                cursor: Vec3::new(0.5, 0.5, 1.0),
                inside_block: false,
                sequence: 1,
            },
        );
    }

    fn edit(s: &mut Scene, lines: [&str; 4]) {
        let alice = s.players[0].clone();
        send_input(
            &mut s.server,
            &alice,
            PlayerInput::SignUpdate {
                pos: SIGN,
                front: true,
                lines: lines.map(str::to_owned).to_vec(),
            },
        );
    }

    fn editor_opened(s: &Scene) -> Option<bool> {
        s.players[0]
            .link
            .take()
            .into_iter()
            .find_map(|event| match event {
                ClientEvent::OpenSignEditor { pos, front } if pos == SIGN => Some(front),
                _ => None,
            })
    }

    fn front_lines(s: &Scene) -> Vec<String> {
        let data = s.server.block_entity_nbt(SIGN);
        data.and_then(|data| {
            data.get_compound("front_text").and_then(|text| {
                text.get_list("messages").map(|messages| {
                    messages
                        .iter()
                        .map(|line| match line {
                            rspigot_nbt::Tag::String(text) => text.clone(),
                            _ => String::new(),
                        })
                        .collect()
                })
            })
        })
        .unwrap_or_default()
    }

    const LINES: [&str; 4] = ["one", "two", "", "four"];

    #[test]
    fn right_clicking_a_sign_tells_the_plugins_and_opens_the_editor_on_the_facing_side() {
        let mut s = standing(vec![observe(1, EventKind::PlayerSignOpen)]);
        click(&mut s);
        let events = fired(&s.plugin);
        let EventPayload::PlayerSignOpen(PlayerSignOpen {
            cause, side, sign, ..
        }) = &events[0].payload
        else {
            panic!("expected a sign open, got {:?}", events[0]);
        };
        assert_eq!(*cause, PlayerSignOpenCause::Interact);
        assert_eq!(*side, Side::Front);
        assert_eq!(sign.pos, SIGN);
        assert_eq!(editor_opened(&s), Some(true));
    }

    #[test]
    fn the_back_of_a_sign_opens_the_back_editor() {
        let mut s = standing(Vec::new());
        go(&mut s, "tp Alice 2.5 64 -1.5");
        s.players[0].link.take();
        click(&mut s);
        assert_eq!(editor_opened(&s), Some(false));
    }

    #[test]
    fn a_cancelled_sign_open_opens_no_editor() {
        let mut s = standing(vec![deciding(1, EventKind::PlayerSignOpen)]);
        click(&mut s);
        let asked = last_asked(&s.plugin, EventKind::PlayerSignOpen);
        rule_on(&mut s.server, 1, asked.id, true, None);
        s.tick(2);
        assert_eq!(editor_opened(&s), None);
        edit(&mut s, LINES);
        assert!(front_lines(&s).is_empty());
    }

    #[test]
    fn an_edit_without_an_open_editor_changes_nothing() {
        let mut s = standing(vec![observe(1, EventKind::SignChange)]);
        edit(&mut s, LINES);
        assert!(fired(&s.plugin).is_empty());
        assert!(front_lines(&s).is_empty());
    }

    #[test]
    fn an_edit_reaches_the_plugins_and_writes_the_text_without_a_handler_decision() {
        let mut s = standing(vec![observe(1, EventKind::SignChange)]);
        click(&mut s);
        edit(&mut s, LINES);
        let events = fired(&s.plugin);
        let change = events
            .iter()
            .find_map(|event| match &event.payload {
                EventPayload::SignChange(SignChange {
                    block, lines, side, ..
                }) => Some((*block, lines.clone(), *side)),
                _ => None,
            })
            .expect("a sign change");
        assert_eq!(change.0, SIGN);
        assert_eq!(change.1, LINES.to_vec());
        assert_eq!(change.2, Side::Front);
        assert_eq!(front_lines(&s), LINES.to_vec());
    }

    #[test]
    fn a_cancelled_edit_keeps_the_old_text() {
        let mut s = standing(vec![deciding(1, EventKind::SignChange)]);
        click(&mut s);
        edit(&mut s, LINES);
        let asked = last_asked(&s.plugin, EventKind::SignChange);
        rule_on(&mut s.server, 1, asked.id, true, None);
        s.tick(2);
        assert!(front_lines(&s).is_empty());
    }

    #[test]
    fn a_patched_line_replaces_the_typed_one() {
        let mut s = standing(vec![deciding(1, EventKind::SignChange)]);
        click(&mut s);
        edit(&mut s, LINES);
        let asked = last_asked(&s.plugin, EventKind::SignChange);
        rule_on(
            &mut s.server,
            1,
            asked.id,
            false,
            Some(EventPatch::SignChange(SignChangePatch {
                lines: ListDiff {
                    removed: Vec::new(),
                    replaced: vec![(1, "patched".to_owned())],
                    added: Vec::new(),
                },
            })),
        );
        s.tick(2);
        assert_eq!(front_lines(&s), vec!["one", "patched", "", "four"]);
    }

    #[test]
    fn placing_a_sign_opens_its_editor_with_the_place_cause() {
        let mut s = standing(vec![observe(1, EventKind::PlayerSignOpen)]);
        console(&mut s, "give Alice minecraft:oak_sign");
        s.tick(2);
        let alice = s.players[0].clone();
        send_input(
            &mut s.server,
            &alice,
            PlayerInput::UseItemOn {
                hand: Hand::Main,
                pos: BlockPos::new(4, 63, 5),
                face: Direction::Up,
                cursor: Vec3::new(0.5, 1.0, 0.5),
                inside_block: false,
                sequence: 2,
            },
        );
        s.tick(2);
        let causes: Vec<PlayerSignOpenCause> = fired(&s.plugin)
            .into_iter()
            .filter_map(|event| match event.payload {
                EventPayload::PlayerSignOpen(open) => Some(open.cause),
                _ => None,
            })
            .collect();
        assert_eq!(causes, vec![PlayerSignOpenCause::Place]);
    }
}

mod books {
    use super::*;
    use rspigot_core::events::Inbound;
    use rspigot_core::plugins::events::{PlayerEditBook, PlayerEditBookPatch};
    use rspigot_core::plugins::{Answer, BookMeta, EventPatch, ItemSpec, Question};

    fn writable_with(handlers: Vec<rspigot_core::plugins::HandlerSpec>) -> Scene {
        let mut s = scene_with(&["Alice"], handlers);
        s.server
            .handle(Inbound::Console("give Alice writable_book 1".to_owned()));
        s.server.deliver();
        s
    }

    fn book(s: &mut Scene) -> ItemSpec {
        let Answer::Inventory(inventory) = s.ask(Question::Inventory(s.players[0].id)) else {
            panic!("expected the inventory");
        };
        inventory
            .slots
            .into_iter()
            .flatten()
            .find(|item| item.key.ends_with("_book"))
            .expect("a book")
    }

    fn content(item: &ItemSpec, key: &str) -> rspigot_nbt::Compound {
        let mut bytes = item.components.as_slice();
        let tag = rspigot_nbt::read_network_compound(&mut bytes)
            .unwrap()
            .expect("components");
        tag.get_compound(key).cloned().expect("the book content")
    }

    fn page_strings(content: &rspigot_nbt::Compound) -> Vec<String> {
        content
            .get_list("pages")
            .unwrap_or_default()
            .iter()
            .map(|page| match page {
                rspigot_nbt::Tag::String(text) => text.clone(),
                other => panic!("unexpected page {other:?}"),
            })
            .collect()
    }

    fn edit(s: &mut Scene, title: Option<&str>) {
        let alice = s.players[0].clone();
        send_input(
            &mut s.server,
            &alice,
            PlayerInput::EditBook {
                slot: 0,
                pages: vec!["first".to_owned(), "second".to_owned()],
                title: title.map(str::to_owned),
            },
        );
    }

    #[test]
    fn saving_a_book_tells_the_plugins_and_writes_its_pages() {
        let mut s = writable_with(vec![observe(1, EventKind::PlayerEditBook)]);
        edit(&mut s, None);
        let events = fired(&s.plugin);
        let EventPayload::PlayerEditBook(PlayerEditBook {
            new_book_meta,
            previous_book_meta,
            signing,
            player,
        }) = &events[0].payload
        else {
            panic!("expected an edit, got {:?}", events[0]);
        };
        assert!(!*signing);
        assert_eq!(player.id, s.players[0].id);
        assert_eq!(new_book_meta.pages, vec!["first", "second"]);
        assert!(previous_book_meta.pages.is_empty());
        let item = book(&mut s);
        assert_eq!(item.key, "minecraft:writable_book");
        assert_eq!(
            page_strings(&content(&item, "minecraft:writable_book_content")),
            vec!["first", "second"]
        );
    }

    #[test]
    fn signing_a_book_makes_a_written_book_with_the_title_and_the_author() {
        let mut s = writable_with(vec![observe(1, EventKind::PlayerEditBook)]);
        edit(&mut s, Some("Tale"));
        let events = fired(&s.plugin);
        let EventPayload::PlayerEditBook(PlayerEditBook {
            new_book_meta,
            signing,
            ..
        }) = &events[0].payload
        else {
            panic!("expected an edit");
        };
        assert!(*signing);
        assert_eq!(new_book_meta.title.as_deref(), Some("Tale"));
        assert_eq!(new_book_meta.author.as_deref(), Some("Alice"));
        let item = book(&mut s);
        assert_eq!(item.key, "minecraft:written_book");
        let written = content(&item, "minecraft:written_book_content");
        assert_eq!(written.get_str("author"), Some("Alice"));
        assert_eq!(page_strings(&written), vec!["first", "second"]);
    }

    #[test]
    fn a_cancelled_edit_keeps_the_book_and_resends_the_inventory() {
        let mut s = writable_with(vec![deciding(1, EventKind::PlayerEditBook)]);
        s.players[0].link.take();
        edit(&mut s, None);
        let asked = last_asked(&s.plugin, EventKind::PlayerEditBook);
        rule_on(&mut s.server, 1, asked.id, true, None);
        let item = book(&mut s);
        assert!(item.components.is_empty());
        assert!(s.players[0].link.take().iter().any(|event| matches!(
            event,
            rspigot_core::events::ClientEvent::InventoryContent { .. }
        )));
    }

    #[test]
    fn a_patch_can_replace_the_pages_and_turn_a_save_into_a_signature() {
        let mut s = writable_with(vec![deciding(1, EventKind::PlayerEditBook)]);
        edit(&mut s, None);
        let asked = last_asked(&s.plugin, EventKind::PlayerEditBook);
        rule_on(
            &mut s.server,
            1,
            asked.id,
            false,
            Some(EventPatch::PlayerEditBook(PlayerEditBookPatch {
                new_book_meta: Some(BookMeta {
                    title: Some("Patched".to_owned()),
                    author: Some("Bob".to_owned()),
                    pages: vec!["only".to_owned()],
                }),
                signing: Some(true),
            })),
        );
        let item = book(&mut s);
        assert_eq!(item.key, "minecraft:written_book");
        let written = content(&item, "minecraft:written_book_content");
        assert_eq!(written.get_str("title"), Some("Patched"));
        assert_eq!(written.get_str("author"), Some("Bob"));
        assert_eq!(page_strings(&written), vec!["only"]);
    }

    #[test]
    fn an_edit_of_something_that_is_not_a_writable_book_is_ignored() {
        let mut s = scene_with(&["Alice"], vec![observe(1, EventKind::PlayerEditBook)]);
        edit(&mut s, None);
        assert!(fired(&s.plugin).is_empty());
    }
}

mod recipe_book {
    use super::*;
    use rspigot_core::player_data::PlayerSave;
    use rspigot_core::plugins::events::{
        PlayerRecipeBookSettingsChange, PlayerRecipeBookSettingsChangeRecipeBookType as Kind,
    };
    use rspigot_core::ports::PlayerStore;

    fn change(s: &mut Scene, kind: Kind, open: bool, filtering: bool) {
        let alice = s.players[0].clone();
        send_input(
            &mut s.server,
            &alice,
            PlayerInput::RecipeBookSettings {
                kind,
                open,
                filtering,
            },
        );
    }

    fn saved(s: &mut Scene, saves: &Saves) -> PlayerSave {
        let alice = s.players[0].clone();
        s.server.handle(leave_of(&alice));
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        let data = loop {
            if let Some(data) = saves.load_player(alice.id).expect("a store") {
                break data;
            }
            assert!(std::time::Instant::now() < deadline, "the player was saved");
            std::thread::sleep(std::time::Duration::from_millis(5));
        };
        PlayerSave::from_nbt(&data).expect("the save decodes")
    }

    #[test]
    fn a_settings_change_reaches_the_plugins_and_is_stored_for_the_player() {
        let (mut s, saves) = saving_scene(
            &["Alice"],
            vec![observe(1, EventKind::PlayerRecipeBookSettingsChange)],
        );
        change(&mut s, Kind::Smoker, true, false);
        let events = fired(&s.plugin);
        let EventPayload::PlayerRecipeBookSettingsChange(PlayerRecipeBookSettingsChange {
            filtering,
            open,
            recipe_book_type,
            player,
        }) = &events[0].payload
        else {
            panic!("expected a settings change, got {:?}", events[0]);
        };
        assert_eq!(
            (*open, *filtering, *recipe_book_type),
            (true, false, Kind::Smoker)
        );
        assert_eq!(player.id, s.players[0].id);
        let save = saved(&mut s, &saves);
        assert_eq!(save.recipe_book.get(Kind::Smoker), (true, false));
        assert_eq!(save.recipe_book.get(Kind::Crafting), (false, false));
    }

    #[test]
    fn without_a_handler_the_setting_is_stored_at_once() {
        let (mut s, saves) = saving_scene(&["Alice"], Vec::new());
        change(&mut s, Kind::Furnace, false, true);
        let save = saved(&mut s, &saves);
        assert_eq!(save.recipe_book.get(Kind::Furnace), (false, true));
    }
}

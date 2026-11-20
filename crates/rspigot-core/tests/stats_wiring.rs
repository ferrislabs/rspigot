mod common;
#[path = "common/screen.rs"]
mod screen;

use common::*;
use rspigot_core::config::PluginSettings;
use rspigot_core::events::{
    ClickInput, ClientEvent, DigAction, Direction, Hand, Inbound, PlayerInput,
};
use rspigot_core::plugins::events::{PlayerStatisticIncrement, Statistic};
use rspigot_core::plugins::{EntityOrder, EventKind, EventPayload, Order};
use rspigot_core::ports::ProgressFile;
use rspigot_domain::world::Difficulty;
use rspigot_domain::{BlockPos, EntityId, Vec3};
use rspigot_entities::EntityKind as Kind;
use screen::*;

const CUSTOM: i32 = 8;
const CHEST_AT: BlockPos = BlockPos::new(2, 64, 0);

const DAMAGE_DEALT: i32 = 25;
const DAMAGE_DEALT_RESISTED: i32 = 27;
const DAMAGE_TAKEN: i32 = 28;
const DAMAGE_BLOCKED_BY_SHIELD: i32 = 29;
const DAMAGE_ABSORBED: i32 = 30;
const DAMAGE_RESISTED: i32 = 31;
const TALKED_TO_VILLAGER: i32 = 37;
const TRADED_WITH_VILLAGER: i32 = 38;
const INTERACT_WITH_BREWINGSTAND: i32 = 45;
const INTERACT_WITH_BEACON: i32 = 46;
const INSPECT_DROPPER: i32 = 47;
const INSPECT_HOPPER: i32 = 48;
const INSPECT_DISPENSER: i32 = 49;
const PLAY_NOTEBLOCK: i32 = 50;
const TUNE_NOTEBLOCK: i32 = 51;
const TRIGGER_TRAPPED_CHEST: i32 = 53;
const OPEN_ENDERCHEST: i32 = 54;
const ENCHANT_ITEM: i32 = 55;
const INTERACT_WITH_FURNACE: i32 = 57;
const INTERACT_WITH_CRAFTING_TABLE: i32 = 58;
const OPEN_CHEST: i32 = 59;
const OPEN_SHULKER_BOX: i32 = 62;
const OPEN_BARREL: i32 = 63;
const INTERACT_WITH_BLAST_FURNACE: i32 = 64;
const INTERACT_WITH_SMOKER: i32 = 65;
const INTERACT_WITH_CARTOGRAPHY_TABLE: i32 = 68;
const INTERACT_WITH_LOOM: i32 = 69;
const INTERACT_WITH_STONECUTTER: i32 = 70;
const BELL_RING: i32 = 71;
const RAID_TRIGGER: i32 = 72;
const RAID_WIN: i32 = 73;
const INTERACT_WITH_ANVIL: i32 = 74;
const INTERACT_WITH_GRINDSTONE: i32 = 75;
const INTERACT_WITH_SMITHING_TABLE: i32 = 77;

fn counted(s: &mut Scene, player: &Player) -> Vec<(i32, i32, i32)> {
    player.link.take();
    send_input(&mut s.server, player, PlayerInput::RequestStats);
    player
        .link
        .take()
        .into_iter()
        .find_map(|event| match event {
            ClientEvent::Stats(entries) => Some(entries),
            _ => None,
        })
        .expect("the stats were sent")
}

fn stat(s: &mut Scene, player: &Player, id: i32) -> i32 {
    counted(s, player)
        .iter()
        .find(|(kind, value, _)| (*kind, *value) == (CUSTOM, id))
        .map_or(0, |entry| entry.2)
}

fn console(s: &mut Scene, line: &str) {
    s.server.handle(Inbound::Console(line.to_owned()));
    s.server.deliver();
}

fn increments(s: &Scene) -> Vec<PlayerStatisticIncrement> {
    fired(&s.plugin)
        .into_iter()
        .filter_map(|event| match event.payload {
            EventPayload::PlayerStatisticIncrement(increment) => Some(increment),
            _ => None,
        })
        .collect()
}

fn rule_on_last(s: &mut Scene, cancel: bool) {
    let asked = last_asked(&s.plugin, EventKind::PlayerStatisticIncrement);
    rule_on(&mut s.server, 1, asked.id, cancel, None);
}

#[test]
fn opening_a_container_counts_its_interaction() {
    let mut s = scene(&["Alice"]);
    let alice = s.players[0].clone();
    let blocks = [
        ("minecraft:chest", OPEN_CHEST),
        ("minecraft:trapped_chest", TRIGGER_TRAPPED_CHEST),
        ("minecraft:ender_chest", OPEN_ENDERCHEST),
        ("minecraft:barrel", OPEN_BARREL),
        ("minecraft:shulker_box", OPEN_SHULKER_BOX),
        ("minecraft:furnace", INTERACT_WITH_FURNACE),
        ("minecraft:blast_furnace", INTERACT_WITH_BLAST_FURNACE),
        ("minecraft:smoker", INTERACT_WITH_SMOKER),
        ("minecraft:crafting_table", INTERACT_WITH_CRAFTING_TABLE),
        ("minecraft:hopper", INSPECT_HOPPER),
        ("minecraft:dropper", INSPECT_DROPPER),
        ("minecraft:dispenser", INSPECT_DISPENSER),
        ("minecraft:anvil", INTERACT_WITH_ANVIL),
        ("minecraft:grindstone", INTERACT_WITH_GRINDSTONE),
        ("minecraft:smithing_table", INTERACT_WITH_SMITHING_TABLE),
        ("minecraft:loom", INTERACT_WITH_LOOM),
        ("minecraft:stonecutter", INTERACT_WITH_STONECUTTER),
        (
            "minecraft:cartography_table",
            INTERACT_WITH_CARTOGRAPHY_TABLE,
        ),
        ("minecraft:brewing_stand", INTERACT_WITH_BREWINGSTAND),
        ("minecraft:beacon", INTERACT_WITH_BEACON),
    ];
    for (key, id) in blocks {
        set_block(&mut s, CHEST_AT, key);
        alice.link.take();
        let mut view = open_block(&mut s, &alice, CHEST_AT);
        assert_eq!(stat(&mut s, &alice, id), 1, "{key}");
        close_window(&mut s, &alice, &mut view);
    }
}

#[test]
fn a_second_opening_counts_again() {
    let mut s = scene(&["Alice"]);
    let alice = s.players[0].clone();
    set_block(&mut s, CHEST_AT, "minecraft:chest");
    for _ in 0..2 {
        let mut view = open_block(&mut s, &alice, CHEST_AT);
        close_window(&mut s, &alice, &mut view);
    }
    assert_eq!(stat(&mut s, &alice, OPEN_CHEST), 2);
}

#[test]
fn the_statistic_event_of_an_opening_fires_before_the_counter_changes() {
    let mut s = scene_with(
        &["Alice"],
        vec![observe(1, EventKind::PlayerStatisticIncrement)],
    );
    let alice = s.players[0].clone();
    set_block(&mut s, CHEST_AT, "minecraft:chest");
    open_block(&mut s, &alice, CHEST_AT);
    let events = increments(&s);
    let first = events.first().expect("an increment");
    assert_eq!(first.statistic, Statistic::ChestOpened);
    assert_eq!((first.previous_value, first.new_value), (0, 1));
    assert_eq!(stat(&mut s, &alice, OPEN_CHEST), 1);
}

#[test]
fn a_cancelled_opening_increment_keeps_the_counter_and_an_allowed_one_counts() {
    let mut s = scene_with(
        &["Alice"],
        vec![deciding(1, EventKind::PlayerStatisticIncrement)],
    );
    let alice = s.players[0].clone();
    set_block(&mut s, CHEST_AT, "minecraft:chest");
    let mut view = open_block(&mut s, &alice, CHEST_AT);
    assert_eq!(stat(&mut s, &alice, OPEN_CHEST), 0);
    rule_on_last(&mut s, true);
    assert_eq!(stat(&mut s, &alice, OPEN_CHEST), 0);
    close_window(&mut s, &alice, &mut view);
    open_block(&mut s, &alice, CHEST_AT);
    rule_on_last(&mut s, false);
    assert_eq!(stat(&mut s, &alice, OPEN_CHEST), 1);
}

#[test]
fn the_saved_stats_file_carries_the_interaction_keys() {
    let (mut s, saves) = saving_scene(&["Alice"], Vec::new());
    let alice = s.players[0].clone();
    set_block(&mut s, CHEST_AT, "minecraft:chest");
    open_block(&mut s, &alice, CHEST_AT);
    s.server.handle(leave_of(&alice));
    s.server.deliver();
    let text = saves
        .progress(alice.id, ProgressFile::Stats)
        .expect("a stats file");
    assert!(text.contains("\"minecraft:open_chest\":1"), "{text}");
    let back = join(&mut s.server, "Alice");
    s.server.deliver();
    assert_eq!(stat(&mut s, &back, OPEN_CHEST), 1);
}

const NOTE_AT: BlockPos = BlockPos::new(3, 64, 3);

#[test]
fn tuning_and_hitting_a_note_block_count_separately() {
    let mut s = scene(&["Alice"]);
    let alice = s.players[0].clone();
    set_block(&mut s, NOTE_AT, "minecraft:note_block");
    use_block(&mut s, &alice, NOTE_AT);
    assert_eq!(stat(&mut s, &alice, TUNE_NOTEBLOCK), 1);
    assert_eq!(stat(&mut s, &alice, PLAY_NOTEBLOCK), 0);
    send_input(
        &mut s.server,
        &alice,
        PlayerInput::Dig {
            action: DigAction::Start,
            pos: NOTE_AT,
            face: Direction::Up,
            sequence: 2,
        },
    );
    assert_eq!(stat(&mut s, &alice, PLAY_NOTEBLOCK), 1);
    assert_eq!(stat(&mut s, &alice, TUNE_NOTEBLOCK), 1);
}

#[test]
fn a_cancelled_note_increment_keeps_the_counter() {
    let mut s = scene_with(
        &["Alice"],
        vec![deciding(1, EventKind::PlayerStatisticIncrement)],
    );
    let alice = s.players[0].clone();
    set_block(&mut s, NOTE_AT, "minecraft:note_block");
    use_block(&mut s, &alice, NOTE_AT);
    let asked = last_asked(&s.plugin, EventKind::PlayerStatisticIncrement);
    let EventPayload::PlayerStatisticIncrement(increment) = &asked.payload else {
        panic!("an increment");
    };
    assert_eq!(increment.statistic, Statistic::NoteblockTuned);
    rule_on(&mut s.server, 1, asked.id, true, None);
    assert_eq!(stat(&mut s, &alice, TUNE_NOTEBLOCK), 0);
}

#[test]
fn a_player_ringing_a_bell_counts_it() {
    let mut s = scene(&["Alice"]);
    let alice = s.players[0].clone();
    set_block(&mut s, NOTE_AT, "minecraft:bell");
    send_input(
        &mut s.server,
        &alice,
        PlayerInput::UseItemOn {
            hand: Hand::Main,
            pos: NOTE_AT,
            face: Direction::North,
            cursor: Vec3::new(0.5, 0.5, 0.0),
            inside_block: false,
            sequence: 1,
        },
    );
    assert_eq!(stat(&mut s, &alice, BELL_RING), 1);
}

const TABLE: BlockPos = BlockPos::new(0, 64, 2);
const SWORD: &str = "minecraft:diamond_sword";
const LAPIS: &str = "minecraft:lapis_lazuli";
const HOTBAR_ZERO: i16 = 29;

fn shelves(s: &mut Scene) {
    for z in 0..=4 {
        set_block(s, BlockPos::new(-2, 64, z), "minecraft:bookshelf");
        set_block(s, BlockPos::new(2, 64, z), "minecraft:bookshelf");
    }
    for x in -1..=1 {
        set_block(s, BlockPos::new(x, 64, 0), "minecraft:bookshelf");
        set_block(s, BlockPos::new(x, 64, 4), "minecraft:bookshelf");
    }
    for z in 0..=4 {
        set_block(s, BlockPos::new(-2, 65, z), "minecraft:bookshelf");
    }
}

fn enchanting_scene(handlers: Vec<rspigot_core::plugins::HandlerSpec>) -> (Scene, Player, View) {
    let mut s = scene_with(&["Alice"], handlers);
    let alice = s.players[0].clone();
    set_block(&mut s, TABLE, "minecraft:enchanting_table");
    shelves(&mut s);
    s.server.give_experience(alice.id, 3000);
    alice.link.take();
    let table = open_block(&mut s, &alice, TABLE);
    (s, alice, table)
}

fn put_in(
    s: &mut Scene,
    alice: &Player,
    table: &mut View,
    hotbar: i32,
    key: &str,
    count: u8,
    to: i16,
) {
    give(s, alice, hotbar, key, i32::from(count));
    alice.link.take();
    refresh(alice, table);
    let from = HOTBAR_ZERO + hotbar as i16;
    let take = predict(
        table,
        press(from, 0, ClickInput::Pickup),
        &[(from, None)],
        stack(key, count),
    );
    send_click(s, alice, table, take);
    let place = predict(
        table,
        press(to, 0, ClickInput::Pickup),
        &[(to, stack(key, count))],
        None,
    );
    send_click(s, alice, table, place);
}

fn enchant(s: &mut Scene, alice: &Player, table: &mut View) {
    put_in(s, alice, table, 0, SWORD, 1, 0);
    put_in(s, alice, table, 1, LAPIS, 3, 1);
    s.server.handle(Inbound::Input {
        player: alice.id,
        input: PlayerInput::ContainerButtonClick {
            window: table.window,
            button: 2,
        },
    });
    s.server.deliver();
}

#[test]
fn enchanting_an_item_counts_enchant_item() {
    let (mut s, alice, mut table) = enchanting_scene(Vec::new());
    enchant(&mut s, &alice, &mut table);
    assert_eq!(stat(&mut s, &alice, ENCHANT_ITEM), 1);
}

#[test]
fn the_enchant_statistic_fires_before_the_change_and_a_cancel_keeps_the_counter() {
    let (mut s, alice, mut table) =
        enchanting_scene(vec![deciding(1, EventKind::PlayerStatisticIncrement)]);
    enchant(&mut s, &alice, &mut table);
    let asked = last_asked(&s.plugin, EventKind::PlayerStatisticIncrement);
    let EventPayload::PlayerStatisticIncrement(increment) = &asked.payload else {
        panic!("an increment");
    };
    assert_eq!(increment.statistic, Statistic::ItemEnchanted);
    assert_eq!((increment.previous_value, increment.new_value), (0, 1));
    assert_eq!(stat(&mut s, &alice, ENCHANT_ITEM), 0);
    rule_on(&mut s.server, 1, asked.id, true, None);
    assert_eq!(stat(&mut s, &alice, ENCHANT_ITEM), 0);
}

const FARMER: &str = "{NoAI:1b,Xp:0,VillagerDataFinalized:1b,VillagerData:{type:\"minecraft:plains\",profession:\"minecraft:farmer\",level:1}}";

fn summon_farmer(s: &mut Scene) -> EntityId {
    stream_chunk_of(s, BlockPos::new(1, 64, 1));
    console(
        s,
        &format!("summon {} 2.5 64 0.5 {FARMER}", Kind::Villager.name()),
    );
    s.server
        .entities_of(Kind::Villager)
        .into_iter()
        .map(|entity| entity.id)
        .next()
        .expect("a villager")
}

fn talk(s: &mut Scene, target: EntityId) {
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
}

#[test]
fn talking_to_a_villager_and_trading_count_both_statistics() {
    let mut s = scene(&["Alice"]);
    let alice = s.players[0].clone();
    let villager = summon_farmer(&mut s);
    alice.link.take();
    talk(&mut s, villager);
    let events = alice.link.take();
    let (window, offer) = events
        .iter()
        .find_map(|event| match event {
            ClientEvent::MerchantOffers { window, offers, .. } => {
                Some((*window, offers[0].clone()))
            }
            _ => None,
        })
        .expect("the offers");
    let mut view = View {
        window,
        state: 0,
        slots: Vec::new(),
        cursor: None,
        opened: true,
        data: Vec::new(),
    };
    view.apply(&events);
    assert_eq!(stat(&mut s, &alice, TALKED_TO_VILLAGER), 1);
    assert_eq!(stat(&mut s, &alice, TRADED_WITH_VILLAGER), 0);
    let cost = offer.cost_a.item.kind().expect("a cost").name();
    give(&mut s, &alice, 2, cost, offer.cost_a.count.min(64));
    view.apply(&alice.link.take());
    send_input(&mut s.server, &alice, PlayerInput::SelectTrade(0));
    view.apply(&alice.link.take());
    send_click(&mut s, &alice, &mut view, press(2, 0, ClickInput::Pickup));
    assert_eq!(stat(&mut s, &alice, TRADED_WITH_VILLAGER), 1);
}

#[test]
fn the_villager_statistic_fires_before_the_change_and_a_cancel_keeps_the_counter() {
    let mut s = scene_with(
        &["Alice"],
        vec![deciding(1, EventKind::PlayerStatisticIncrement)],
    );
    let alice = s.players[0].clone();
    let villager = summon_farmer(&mut s);
    talk(&mut s, villager);
    let asked = last_asked(&s.plugin, EventKind::PlayerStatisticIncrement);
    let EventPayload::PlayerStatisticIncrement(increment) = &asked.payload else {
        panic!("an increment");
    };
    assert_eq!(increment.statistic, Statistic::TalkedToVillager);
    assert_eq!((increment.previous_value, increment.new_value), (0, 1));
    assert_eq!(stat(&mut s, &alice, TALKED_TO_VILLAGER), 0);
    rule_on(&mut s.server, 1, asked.id, true, None);
    assert_eq!(stat(&mut s, &alice, TALKED_TO_VILLAGER), 0);
}

#[test]
fn absorbed_and_resisted_damage_count_in_tenths_for_the_victim() {
    let mut s = scene(&["Alice"]);
    let alice = s.players[0].clone();
    console(&mut s, "effect give Alice minecraft:absorption 60 0");
    console(&mut s, "effect give Alice minecraft:resistance 60 0");
    console(&mut s, "effect give Alice minecraft:instant_damage");
    s.tick(2);
    assert_eq!(stat(&mut s, &alice, DAMAGE_ABSORBED), 40);
    assert_eq!(stat(&mut s, &alice, DAMAGE_RESISTED), 12);
    assert_eq!(stat(&mut s, &alice, DAMAGE_TAKEN), 8);
}

#[test]
fn the_damage_statistic_fires_before_the_change_and_a_cancel_keeps_the_counter() {
    let mut s = scene_with(
        &["Alice"],
        vec![deciding(1, EventKind::PlayerStatisticIncrement)],
    );
    let alice = s.players[0].clone();
    console(&mut s, "effect give Alice minecraft:absorption 60 0");
    console(&mut s, "effect give Alice minecraft:instant_damage");
    s.tick(2);
    let asks = asked_for(&s.plugin, EventKind::PlayerStatisticIncrement);
    let absorbed = asks
        .iter()
        .find(|ask| {
            matches!(
                &ask.payload,
                EventPayload::PlayerStatisticIncrement(increment)
                    if increment.statistic == Statistic::DamageAbsorbed
            )
        })
        .expect("an absorbed increment");
    let EventPayload::PlayerStatisticIncrement(increment) = &absorbed.payload else {
        panic!("an increment");
    };
    assert_eq!((increment.previous_value, increment.new_value), (0, 40));
    rule_on(&mut s.server, 1, absorbed.id, true, None);
    assert_eq!(stat(&mut s, &alice, DAMAGE_ABSORBED), 0);
}

#[test]
fn hitting_a_mob_counts_the_damage_dealt_and_resisted() {
    let mut s = scene(&["Alice"]);
    let alice = s.players[0].clone();
    stream_chunk_of(&mut s, BlockPos::new(1, 64, 1));
    console(&mut s, "give Alice minecraft:diamond_sword");
    console(
        &mut s,
        "summon minecraft:zombie 0.5 64 1.5 {NoAI:1b,active_effects:[{id:\"minecraft:resistance\",amplifier:0b,duration:6000}]}",
    );
    let zombie = s
        .server
        .entities_of(Kind::Zombie)
        .into_iter()
        .map(|entity| entity.id)
        .next()
        .expect("a zombie");
    s.tick(25);
    send_input(&mut s.server, &alice, PlayerInput::Attack(zombie));
    assert!(stat(&mut s, &alice, DAMAGE_DEALT_RESISTED) > 0);
    assert!(stat(&mut s, &alice, DAMAGE_DEALT) > 0);
}

#[test]
fn a_raised_shield_counts_the_damage_it_blocked() {
    let mut s = scene(&["Alice"]);
    let alice = s.players[0].clone();
    stream_chunk_of(&mut s, BlockPos::new(1, 64, 1));
    console(&mut s, "time set 18000");
    console(&mut s, "give Alice minecraft:shield");
    s.tick(2);
    send_input(
        &mut s.server,
        &alice,
        PlayerInput::UseItem {
            hand: Hand::Main,
            sequence: 1,
            rotation: rspigot_domain::Rotation::new(0.0, 0.0),
        },
    );
    s.tick(8);
    console(&mut s, "summon minecraft:zombie 0.5 64 1.5");
    for _ in 0..300 {
        s.tick(1);
        if stat(&mut s, &alice, DAMAGE_BLOCKED_BY_SHIELD) > 0 {
            break;
        }
    }
    assert!(stat(&mut s, &alice, DAMAGE_BLOCKED_BY_SHIELD) > 0);
}

fn village(difficulty: Difficulty, handlers: Vec<rspigot_core::plugins::HandlerSpec>) -> Scene {
    let server = configured_level(
        |config| {
            config.difficulty = difficulty;
            config.plugins = PluginSettings {
                decision_ticks: 1_000,
                ..PluginSettings::default()
            };
        },
        |level| {
            level.day_time = 4_000;
            level.daylight_cycle = false;
        },
    )
    .with_rng_seed(11);
    let mut s = scene_on(server, &["Alice"], handlers);
    stream_chunk_of(&mut s, BlockPos::new(1, 64, 1));
    set_block(&mut s, BlockPos::new(8, 64, 8), "minecraft:lectern");
    console(
        &mut s,
        "summon minecraft:villager 7.5 64 8.5 {VillagerData:{type:\"minecraft:plains\",profession:\"minecraft:librarian\",level:2},Xp:20}",
    );
    s.tick(200);
    s
}

fn raiders(s: &Scene) -> Vec<EntityId> {
    [Kind::Pillager, Kind::Vindicator, Kind::Witch, Kind::Ravager]
        .into_iter()
        .flat_map(|kind| s.server.entities_of(kind))
        .map(|entity| entity.id)
        .collect()
}

fn kill_all(s: &mut Scene) {
    for id in raiders(s) {
        s.act(Order::Entity {
            entity: id,
            order: EntityOrder::Kill,
        });
    }
}

fn tick_until_wave(s: &mut Scene, wave: i32) {
    for _ in 0..800 {
        if s.server
            .raids()
            .iter()
            .next()
            .is_some_and(|raid| raid.groups_spawned() >= wave)
        {
            return;
        }
        s.tick(1);
    }
    panic!("wave {wave} never spawned");
}

#[test]
fn a_raid_trigger_counts_once_and_the_event_fires_before_the_change() {
    let mut s = village(
        Difficulty::Easy,
        vec![observe(1, EventKind::PlayerStatisticIncrement)],
    );
    let alice = s.players[0].clone();
    console(&mut s, "effect give Alice minecraft:bad_omen 120 0");
    s.tick(650);
    assert_eq!(stat(&mut s, &alice, RAID_TRIGGER), 1);
    let trigger = increments(&s)
        .into_iter()
        .find(|increment| increment.statistic == Statistic::RaidTrigger)
        .expect("a raid trigger increment");
    assert_eq!((trigger.previous_value, trigger.new_value), (0, 1));
}

#[test]
fn a_cancelled_raid_trigger_increment_keeps_the_counter() {
    let mut s = village(
        Difficulty::Easy,
        vec![deciding(1, EventKind::PlayerStatisticIncrement)],
    );
    let alice = s.players[0].clone();
    console(&mut s, "effect give Alice minecraft:bad_omen 120 0");
    s.tick(650);
    rule_on_last(&mut s, true);
    assert_eq!(stat(&mut s, &alice, RAID_TRIGGER), 0);
}

#[test]
fn winning_a_raid_counts_raid_win_for_the_hero() {
    let mut s = village(Difficulty::Easy, Vec::new());
    let alice = s.players[0].clone();
    console(&mut s, "effect give Alice minecraft:bad_omen 120 0");
    s.tick(650);
    for wave in 1..=3 {
        tick_until_wave(&mut s, wave);
        if wave < 3 {
            kill_all(&mut s);
            s.tick(2);
        }
    }
    let all = raiders(&s);
    let (last, rest) = all.split_last().expect("raiders");
    for id in rest {
        s.act(Order::Entity {
            entity: *id,
            order: EntityOrder::Kill,
        });
    }
    s.act(Order::Entity {
        entity: *last,
        order: EntityOrder::Teleport(Vec3::new(1.5, 64.0, 0.5)),
    });
    let health = [Kind::Pillager, Kind::Vindicator, Kind::Witch, Kind::Ravager]
        .into_iter()
        .flat_map(|kind| s.server.entities_of(kind))
        .find(|entity| entity.id == *last)
        .and_then(|entity| entity.health)
        .expect("a living raider");
    s.act(Order::Entity {
        entity: *last,
        order: EntityOrder::Damage(f64::from(health) - 0.01),
    });
    s.tick(25);
    send_input(&mut s.server, &alice, PlayerInput::Attack(*last));
    s.tick(60);
    assert_eq!(stat(&mut s, &alice, RAID_WIN), 1);
    assert_eq!(stat(&mut s, &alice, RAID_TRIGGER), 1);
}

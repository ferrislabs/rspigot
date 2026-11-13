mod common;
#[path = "common/screen.rs"]
mod screen;

use common::*;
use rspigot_core::events::{ClickInput, ClientEvent, ContainerClick, Hand, Inbound, PlayerInput};
use rspigot_domain::entity::trading::MerchantOffer;
use rspigot_domain::{BlockPos, EntityId, Vec3};
use rspigot_entities::EntityKind as Kind;
use rspigot_nbt::Compound;
use screen::*;

const NITWIT_FREE: &str = "{NoAI:1b,VillagerDataFinalized:1b,VillagerData:{type:\"minecraft:plains\",profession:\"minecraft:none\",level:1}}";
const MERCHANT_MENU: i32 = 19;

fn console(s: &mut Scene, line: &str) {
    s.server.handle(Inbound::Console(line.to_owned()));
    s.server.deliver();
}

fn summon(s: &mut Scene, kind: Kind, at: &str, nbt: &str) -> EntityId {
    let known: Vec<EntityId> = s
        .server
        .entities_of(kind)
        .into_iter()
        .map(|entity| entity.id)
        .collect();
    console(s, &format!("summon {} {at} {nbt}", kind.name()));
    s.server
        .entities_of(kind)
        .into_iter()
        .map(|entity| entity.id)
        .find(|id| !known.contains(id))
        .expect("a new entity")
}

fn ready() -> Scene {
    let mut s = scene(&["Alice"]);
    stream_chunk_of(&mut s, BlockPos::new(1, 64, 1));
    s
}

fn farmer(level: i32, xp: i32) -> String {
    format!(
        "{{NoAI:1b,Xp:{xp},VillagerDataFinalized:1b,VillagerData:{{type:\"minecraft:plains\",profession:\"minecraft:farmer\",level:{level}}}}}"
    )
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
}

struct Offered {
    window: i32,
    offers: Vec<MerchantOffer>,
    level: i32,
    xp: i32,
    progress: bool,
    restock: bool,
}

fn offered(events: &[ClientEvent]) -> Option<Offered> {
    events.iter().rev().find_map(|event| match event {
        ClientEvent::MerchantOffers {
            window,
            offers,
            level,
            xp,
            show_progress,
            can_restock,
        } => Some(Offered {
            window: *window,
            offers: offers.clone(),
            level: *level,
            xp: *xp,
            progress: *show_progress,
            restock: *can_restock,
        }),
        _ => None,
    })
}

fn saved(s: &Scene, id: EntityId) -> Compound {
    s.server.saved_entity(id).expect("a saved entity")
}

fn recipes(s: &Scene, id: EntityId) -> usize {
    saved(s, id)
        .get_compound("Offers")
        .and_then(|offers| offers.get_list("Recipes"))
        .map_or(0, <[rspigot_nbt::Tag]>::len)
}

fn open_trade(s: &mut Scene, target: EntityId) -> (View, Offered) {
    let alice = s.players[0].clone();
    alice.link.take();
    interact(s, target);
    let events = alice.link.take();
    let offers = offered(&events).expect("the offers are sent");
    let mut view = View {
        window: offers.window,
        state: 0,
        slots: Vec::new(),
        cursor: None,
        opened: true,
        data: Vec::new(),
    };
    view.apply(&events);
    (view, offers)
}

fn give_cost(s: &mut Scene, offer: &MerchantOffer, slot: i32) {
    let alice = s.players[0].clone();
    let name = offer.cost_a.item.kind().unwrap().name();
    give(s, &alice, slot, name, offer.cost_a.count.min(64));
}

#[test]
fn trading_with_a_farmer_opens_the_merchant_screen_and_sends_its_two_offers() {
    let mut s = ready();
    let villager = summon(&mut s, Kind::Villager, "2.5 64 0.5", &farmer(1, 0));
    let alice = s.players[0].clone();
    alice.link.take();
    interact(&mut s, villager);
    let events = alice.link.take();
    let screen = events.iter().find_map(|event| match event {
        ClientEvent::OpenScreen {
            menu,
            window,
            title,
        } => Some((*menu, *window, title.clone())),
        _ => None,
    });
    let (menu, window, _) = screen.expect("the screen opens");
    assert_eq!(menu, MERCHANT_MENU);
    let offers = offered(&events).expect("offers");
    assert_eq!(offers.window, window);
    assert_eq!(offers.offers.len(), 2);
    assert_eq!(offers.level, 1);
    assert_eq!(offers.xp, 0);
    assert!(offers.progress);
    assert!(offers.restock);
    assert_eq!(recipes(&s, villager), 2);
}

#[test]
fn a_villager_with_no_trades_shakes_its_head_instead_of_opening_a_screen() {
    let mut s = ready();
    let villager = summon(&mut s, Kind::Villager, "2.5 64 0.5", NITWIT_FREE);
    let alice = s.players[0].clone();
    alice.link.take();
    interact(&mut s, villager);
    let events = alice.link.take();
    assert!(
        !events
            .iter()
            .any(|event| matches!(event, ClientEvent::OpenScreen { .. }))
    );
    assert!(offered(&events).is_none());
    assert!(events.iter().any(|event| matches!(
        event,
        ClientEvent::Sound { sound, .. } if *sound == "minecraft:entity.villager.no"
    )));
}

#[test]
fn selecting_a_trade_fills_the_payment_and_taking_the_result_pays_and_rewards() {
    let mut s = ready();
    let villager = summon(&mut s, Kind::Villager, "2.5 64 0.5", &farmer(1, 0));
    let alice = s.players[0].clone();
    let (_, offers) = {
        alice.link.take();
        interact(&mut s, villager);
        let events = alice.link.take();
        (events.clone(), offered(&events).expect("offers"))
    };
    let offer = offers.offers[0].clone();
    s.server.handle(Inbound::Input {
        player: alice.id,
        input: PlayerInput::ContainerClose(offers.window),
    });
    s.server.deliver();
    give_cost(&mut s, &offer, 20);
    let (mut view, _) = open_trade(&mut s, villager);
    s.server.handle(Inbound::Input {
        player: alice.id,
        input: PlayerInput::SelectTrade(0),
    });
    s.server.deliver();
    view.apply(&alice.link.take());
    assert_eq!(
        view.at(0).map(|(name, _)| name),
        Some(offer.cost_a.item.kind().unwrap().name())
    );
    assert_eq!(
        view.at(2),
        Some((
            offer.result.item.kind().unwrap().name(),
            offer.result.count.get()
        ))
    );
    send_click(&mut s, &alice, &mut view, press(2, 0, ClickInput::Pickup));
    assert_eq!(
        view.held(),
        Some((
            offer.result.item.kind().unwrap().name(),
            offer.result.count.get()
        ))
    );
    assert!(view.at(0).is_none());
    let villager_nbt = saved(&s, villager);
    assert_eq!(villager_nbt.get_i32("Xp"), Some(offer.xp));
    let first = villager_nbt
        .get_compound("Offers")
        .and_then(|offers| offers.get_list("Recipes"))
        .and_then(|recipes| recipes.first())
        .and_then(|recipe| recipe.as_compound())
        .and_then(|recipe| recipe.get_i32("uses"));
    assert_eq!(first, Some(1));
    assert_eq!(s.server.entities_of(Kind::ExperienceOrb).len(), 1);
}

#[test]
fn enough_experience_raises_the_villager_level_and_adds_the_next_trades() {
    let mut s = ready();
    let villager = summon(&mut s, Kind::Villager, "2.5 64 0.5", &farmer(1, 9));
    let alice = s.players[0].clone();
    let (mut view, offers) = open_trade(&mut s, villager);
    let offer = offers.offers[0].clone();
    view.apply(&alice.link.take());
    give_cost(&mut s, &offer, 20);
    view.apply(&alice.link.take());
    s.server.handle(Inbound::Input {
        player: alice.id,
        input: PlayerInput::SelectTrade(0),
    });
    s.server.deliver();
    view.apply(&alice.link.take());
    s.server.handle(Inbound::Input {
        player: alice.id,
        input: PlayerInput::ContainerClick(ContainerClick {
            container: view.window,
            state_id: view.state,
            ..press(2, 0, ClickInput::Pickup)
        }),
    });
    s.server.deliver();
    let events = alice.link.take();
    let again = offered(&events).expect("the offers are sent again after the level up");
    assert_eq!(again.level, 2);
    assert_eq!(again.offers.len(), 4);
    assert_eq!(recipes(&s, villager), 4);
    let nbt = saved(&s, villager);
    assert_eq!(
        nbt.get_compound("VillagerData")
            .and_then(|data| data.get_i32("level")),
        Some(2)
    );
    assert!(nbt.get_i32("Xp").unwrap() >= 10);
}

#[test]
fn closing_the_screen_frees_the_villager_for_the_next_player() {
    let mut s = ready();
    let villager = summon(&mut s, Kind::Villager, "2.5 64 0.5", &farmer(1, 0));
    let alice = s.players[0].clone();
    let (mut view, _) = open_trade(&mut s, villager);
    interact(&mut s, villager);
    assert!(offered(&alice.link.take()).is_none());
    close_window(&mut s, &alice, &mut view);
    interact(&mut s, villager);
    assert!(offered(&alice.link.take()).is_some());
}

#[test]
fn walking_away_from_the_villager_closes_the_screen() {
    let mut s = ready();
    let villager = summon(&mut s, Kind::Villager, "2.5 64 0.5", &farmer(1, 0));
    let alice = s.players[0].clone();
    let (mut view, _) = open_trade(&mut s, villager);
    console(&mut s, "tp Alice 40.5 64 0.5");
    s.tick(2);
    view.apply(&alice.link.take());
    assert!(!view.opened);
}

#[test]
fn a_wandering_trader_offers_nine_trades_without_a_progress_bar() {
    let mut s = ready();
    let trader = summon(&mut s, Kind::WanderingTrader, "2.5 64 0.5", "{NoAI:1b}");
    let (_, offers) = open_trade(&mut s, trader);
    assert_eq!(offers.offers.len(), 9);
    assert_eq!(offers.level, 1);
    assert!(!offers.progress);
    assert!(!offers.restock);
    assert_eq!(recipes(&s, trader), 9);
}

fn effects_of(s: &Scene, id: EntityId) -> Vec<String> {
    saved(s, id)
        .get_list("active_effects")
        .map(|list| {
            list.iter()
                .filter_map(|effect| effect.as_compound())
                .filter_map(|effect| effect.get_str("id").map(str::to_owned))
                .collect()
        })
        .unwrap_or_default()
}

#[test]
fn a_wandering_trader_drinks_invisibility_at_night_and_milk_once_the_sun_is_up() {
    let mut s = ready();
    console(&mut s, "time set 18000");
    let trader = summon(&mut s, Kind::WanderingTrader, "5.5 64 5.5", "{}");
    s.tick(80);
    assert!(
        effects_of(&s, trader)
            .iter()
            .any(|effect| effect == "minecraft:invisibility"),
        "{:?}",
        saved(&s, trader)
    );
    console(&mut s, "time set 1000");
    s.tick(80);
    assert!(effects_of(&s, trader).is_empty());
}

#[test]
fn a_trader_that_is_trading_stops_walking_and_lets_go_when_the_player_steps_away() {
    let mut s = ready();
    let trader = summon(&mut s, Kind::WanderingTrader, "2.5 64 0.5", "{}");
    let alice = s.players[0].clone();
    let (mut view, _) = open_trade(&mut s, trader);
    let here = s.server.entities_of(Kind::WanderingTrader)[0].position;
    s.tick(20);
    let still = s.server.entities_of(Kind::WanderingTrader)[0].position;
    assert!((here.x - still.x).abs() < 0.5 && (here.z - still.z).abs() < 0.5);
    console(&mut s, "tp Alice 8.5 64 0.5");
    s.tick(3);
    view.apply(&alice.link.take());
    assert!(!view.opened);
}

#[test]
fn the_trader_event_places_a_trader_near_the_player_with_a_wander_target_and_a_home() {
    let mut s = ready();
    let mut spawned = false;
    for _ in 0..400 {
        if s.server.spawn_wandering_trader()
            && !s.server.entities_of(Kind::WanderingTrader).is_empty()
        {
            spawned = true;
            break;
        }
    }
    assert!(spawned, "a trader appears within a few hundred rolls");
    s.tick(1);
    let trader = s.server.entities_of(Kind::WanderingTrader)[0].id;
    let nbt = saved(&s, trader);
    let delay = nbt.get_i32("DespawnDelay").expect("a delay");
    assert!((47_900..=48_000).contains(&delay), "{delay}");
    let target = nbt.get("wander_target").expect("a wander target");
    let rspigot_nbt::Tag::IntArray(target) = target else {
        panic!("an int array");
    };
    assert!(target[0].abs() <= 1 && target[2].abs() <= 1, "{target:?}");
    let at = s.server.entities_of(Kind::WanderingTrader)[0].position;
    assert!(at.x.abs() <= 49.0 && at.z.abs() <= 49.0);
}

#[test]
fn killing_the_trader_closes_the_screen_for_the_player_trading_with_it() {
    let mut s = ready();
    let trader = summon(&mut s, Kind::WanderingTrader, "2.5 64 0.5", "{NoAI:1b}");
    let alice = s.players[0].clone();
    let (mut view, _) = open_trade(&mut s, trader);
    console(&mut s, "kill @e[type=minecraft:wandering_trader]");
    s.tick(2);
    view.apply(&alice.link.take());
    assert!(!view.opened);
}

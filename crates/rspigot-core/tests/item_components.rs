mod common;

use common::*;
use rspigot_core::config::PluginSettings;
use rspigot_core::events::{DigAction, Direction, Hand, Inbound, MovementAction, PlayerInput};
use rspigot_core::plugins::events::Pose;
use rspigot_core::plugins::{
    Answer, EffectSpec, EventKind, EventPayload, InventoryOrder, ItemSpec, Order, PlayerOrder,
    PlayerState, Question,
};
use rspigot_domain::{BlockPos, BlockStateId, EntityId, Rotation, Vec3};
use rspigot_nbt::{Compound, Tag};

const SEED: u64 = 17;
const STONE: BlockPos = BlockPos::new(2, 63, 2);

fn scene_of() -> Scene {
    scene_on(
        server(PluginSettings::default()).with_rng_seed(SEED),
        &["Alice"],
        vec![observe(1, EventKind::EntityPoseChange)],
    )
}

fn console(s: &mut Scene, line: &str) {
    s.server.handle(Inbound::Console(line.to_owned()));
    s.server.deliver();
}

fn with_components(mut spec: ItemSpec, components: Compound) -> ItemSpec {
    rspigot_nbt::write_network(&mut spec.components, &Tag::Compound(components));
    spec
}

fn put(s: &mut Scene, slot: i32, item: ItemSpec) {
    let alice = s.players[0].clone();
    s.act(Order::Inventory {
        player: alice.id,
        order: InventoryOrder::Set {
            slot,
            item: Some(item),
        },
    });
}

fn slot(s: &mut Scene, slot: usize) -> Option<ItemSpec> {
    let Answer::Inventory(inventory) = s.ask(Question::Inventory(s.players[0].id)) else {
        panic!("expected Alice's inventory");
    };
    inventory.slots[slot].clone()
}

fn state(s: &mut Scene) -> PlayerState {
    let Answer::Player(state) = s.ask(Question::Player(s.players[0].id)) else {
        panic!("expected Alice");
    };
    state
}

fn order(s: &mut Scene, order: PlayerOrder) {
    let alice = s.players[0].clone();
    s.act(to(&alice, order));
}

fn use_item(s: &mut Scene) {
    let alice = s.players[0].clone();
    send_input(
        &mut s.server,
        &alice,
        PlayerInput::UseItem {
            hand: Hand::Main,
            sequence: 1,
            rotation: Rotation::new(0.0, 0.0),
        },
    );
}

fn effect(s: &PlayerState, key: &str) -> Option<EffectSpec> {
    s.effects.iter().find(|e| e.key == key).cloned()
}

fn dig(s: &mut Scene, action: DigAction, pos: BlockPos) {
    let alice = s.players[0].clone();
    send_input(
        &mut s.server,
        &alice,
        PlayerInput::Dig {
            action,
            pos,
            face: Direction::Up,
            sequence: 3,
        },
    );
}

fn pickaxe_rule(speed: f32) -> Compound {
    Compound::new()
        .with("blocks", "#minecraft:mineable/pickaxe")
        .with("speed", speed)
        .with("correct_for_drops", true)
}

fn tool(speed: f32) -> Compound {
    Compound::new().with("rules", Tag::List(vec![Tag::Compound(pickaxe_rule(speed))]))
}

#[test]
fn a_stick_with_food_and_consumable_components_feeds_the_player() {
    let mut s = scene_of();
    order(&mut s, PlayerOrder::SetFoodLevel(5));
    put(
        &mut s,
        0,
        with_components(
            ItemSpec::of("minecraft:stick", 1),
            Compound::new()
                .with(
                    "minecraft:food",
                    Compound::new()
                        .with("nutrition", 6)
                        .with("saturation", 1.0f32),
                )
                .with("minecraft:consumable", Compound::new()),
        ),
    );
    use_item(&mut s);
    s.tick(40);
    assert_eq!(state(&mut s).food, 11);
    assert!(slot(&mut s, 0).is_none());
}

#[test]
fn removing_the_consumable_component_makes_an_apple_inedible() {
    let mut s = scene_of();
    order(&mut s, PlayerOrder::SetFoodLevel(5));
    put(
        &mut s,
        0,
        with_components(
            ItemSpec::of("minecraft:apple", 1),
            Compound::new().with("!minecraft:consumable", Compound::new()),
        ),
    );
    use_item(&mut s);
    s.tick(40);
    assert_eq!(state(&mut s).food, 5);
    assert_eq!(slot(&mut s, 0).unwrap().count, 1);
}

#[test]
fn a_consumable_component_sets_the_eating_time() {
    let mut s = scene_of();
    order(&mut s, PlayerOrder::SetFoodLevel(5));
    put(
        &mut s,
        0,
        with_components(
            ItemSpec::of("minecraft:apple", 1),
            Compound::new().with(
                "minecraft:consumable",
                Compound::new().with("consume_seconds", 5.0f32),
            ),
        ),
    );
    use_item(&mut s);
    s.tick(40);
    assert_eq!(state(&mut s).food, 5);
    s.tick(80);
    assert!(state(&mut s).food > 5);
}

#[test]
fn a_stack_death_protection_replaces_the_totem_effects() {
    let mut s = scene_of();
    put(
        &mut s,
        0,
        with_components(
            ItemSpec::of("minecraft:stick", 1),
            Compound::new().with(
                "minecraft:death_protection",
                Compound::new().with(
                    "death_effects",
                    Tag::List(vec![Tag::Compound(
                        Compound::new().with("type", "minecraft:clear_all_effects"),
                    )]),
                ),
            ),
        ),
    );
    order(&mut s, PlayerOrder::Damage(100.0));
    assert_eq!(s.server.player_health(s.players[0].id), Some(1.0));
    assert!(slot(&mut s, 0).is_none());
    assert!(state(&mut s).effects.is_empty());
}

#[test]
fn a_totem_without_its_death_protection_component_no_longer_saves() {
    let mut s = scene_of();
    put(
        &mut s,
        0,
        with_components(
            ItemSpec::of("minecraft:totem_of_undying", 1),
            Compound::new().with("!minecraft:death_protection", Compound::new()),
        ),
    );
    order(&mut s, PlayerOrder::Damage(100.0));
    assert_eq!(s.server.player_health(s.players[0].id), Some(0.0));
}

#[test]
fn a_vanilla_totem_still_grants_regeneration() {
    let mut s = scene_of();
    put(&mut s, 0, ItemSpec::of("minecraft:totem_of_undying", 1));
    order(&mut s, PlayerOrder::Damage(100.0));
    assert_eq!(s.server.player_health(s.players[0].id), Some(1.0));
    assert!(effect(&state(&mut s), "minecraft:regeneration").is_some());
}

fn poses(s: &Scene) -> Vec<(EntityId, Pose)> {
    settled_for(&s.plugin, EventKind::EntityPoseChange)
        .into_iter()
        .filter_map(|event| match event.payload {
            EventPayload::EntityPoseChange(change) => Some((change.entity.id, change.pose)),
            _ => None,
        })
        .collect()
}

fn glide_attempt(s: &mut Scene, chest: ItemSpec) -> bool {
    let alice = s.players[0].clone();
    put(s, 38, chest);
    stream_chunk_of(s, BlockPos::new(1, 64, 1));
    send_input(
        &mut s.server,
        &alice,
        PlayerInput::Move {
            position: Some(Vec3::new(0.5, 70.0, 0.5)),
            rotation: Some(Rotation::new(0.0, 0.0)),
            on_ground: false,
        },
    );
    s.tick(1);
    send_input(
        &mut s.server,
        &alice,
        PlayerInput::Movement(MovementAction::StartFallFlying),
    );
    s.tick(2);
    let Answer::Player(details) = s.ask(Question::Player(alice.id)) else {
        panic!("expected Alice");
    };
    let entity = EntityId(details.player.entity_id);
    poses(s).contains(&(entity, Pose::FallFlying))
}

#[test]
fn a_glider_component_lets_a_stick_glide() {
    let mut s = scene_of();
    let stick = with_components(
        ItemSpec::of("minecraft:stick", 1),
        Compound::new().with("minecraft:glider", Compound::new()),
    );
    assert!(glide_attempt(&mut s, stick));
}

#[test]
fn an_elytra_without_its_glider_component_cannot_glide() {
    let mut s = scene_of();
    let elytra = with_components(
        ItemSpec::of("minecraft:elytra", 1),
        Compound::new().with("!minecraft:glider", Compound::new()),
    );
    assert!(!glide_attempt(&mut s, elytra));
}

#[test]
fn a_plain_elytra_still_glides() {
    let mut s = scene_of();
    assert!(glide_attempt(&mut s, ItemSpec::of("minecraft:elytra", 1)));
}

#[test]
fn a_use_cooldown_component_starts_the_cooldown_of_a_thrown_item() {
    let mut s = scene_of();
    let alice = s.players[0].id;
    put(
        &mut s,
        0,
        with_components(
            ItemSpec::of("minecraft:snowball", 4),
            Compound::new().with(
                "minecraft:use_cooldown",
                Compound::new().with("seconds", 3.0f32),
            ),
        ),
    );
    use_item(&mut s);
    s.tick(1);
    assert!(
        s.server
            .is_on_cooldown(alice, rspigot_items::ItemKind::Snowball.id())
    );
    assert_eq!(slot(&mut s, 0).unwrap().count, 3);
}

#[test]
fn removing_use_cooldown_from_an_ender_pearl_skips_its_cooldown() {
    let mut s = scene_of();
    let alice = s.players[0].id;
    put(
        &mut s,
        0,
        with_components(
            ItemSpec::of("minecraft:ender_pearl", 4),
            Compound::new().with("!minecraft:use_cooldown", Compound::new()),
        ),
    );
    use_item(&mut s);
    s.tick(1);
    assert!(
        !s.server
            .is_on_cooldown(alice, rspigot_items::ItemKind::EnderPearl.id())
    );
}

#[test]
fn a_tool_component_on_a_stick_mines_stone_instantly_and_a_max_damage_component_breaks_it() {
    let mut s = scene_of();
    put(
        &mut s,
        0,
        with_components(
            ItemSpec::of("minecraft:stick", 1),
            Compound::new()
                .with("minecraft:tool", tool(1000.0))
                .with("minecraft:max_damage", 1),
        ),
    );
    stream_chunk_of(&mut s, STONE);
    assert_ne!(s.server.block_state_at(STONE), Some(BlockStateId::AIR));
    dig(&mut s, DigAction::Start, STONE);
    s.tick(2);
    assert_eq!(s.server.block_state_at(STONE), Some(BlockStateId::AIR));
    assert!(slot(&mut s, 0).is_none());
}

#[test]
fn a_plain_stick_is_too_slow_to_break_stone_in_one_hit() {
    let mut s = scene_of();
    put(&mut s, 0, ItemSpec::of("minecraft:stick", 1));
    stream_chunk_of(&mut s, STONE);
    dig(&mut s, DigAction::Start, STONE);
    s.tick(2);
    assert_ne!(s.server.block_state_at(STONE), Some(BlockStateId::AIR));
}

#[test]
fn a_give_command_with_components_changes_behaviour() {
    let mut s = scene_of();
    order(&mut s, PlayerOrder::SetFoodLevel(5));
    console(
        &mut s,
        "give Alice stick[food={nutrition:4,saturation:1.0f},consumable={}] 1",
    );
    use_item(&mut s);
    s.tick(40);
    assert_eq!(state(&mut s).food, 9);
}

const HELMET_SLOT: usize = 39;
const CHEST_SLOT: usize = 38;

#[test]
fn right_clicking_a_helmet_wears_it() {
    let mut s = scene_of();
    put(&mut s, 0, ItemSpec::of("minecraft:diamond_helmet", 1));
    use_item(&mut s);
    s.tick(1);
    assert_eq!(
        slot(&mut s, HELMET_SLOT).unwrap().key,
        "minecraft:diamond_helmet"
    );
    assert!(slot(&mut s, 0).is_none());
}

#[test]
fn right_clicking_a_helmet_swaps_it_with_the_worn_one() {
    let mut s = scene_of();
    put(&mut s, 39, ItemSpec::of("minecraft:iron_helmet", 1));
    put(&mut s, 0, ItemSpec::of("minecraft:diamond_helmet", 1));
    use_item(&mut s);
    s.tick(1);
    assert_eq!(
        slot(&mut s, HELMET_SLOT).unwrap().key,
        "minecraft:diamond_helmet"
    );
    assert_eq!(slot(&mut s, 0).unwrap().key, "minecraft:iron_helmet");
}

#[test]
fn a_stack_of_boots_wears_one_and_keeps_the_rest() {
    let mut s = scene_of();
    put(&mut s, 0, ItemSpec::of("minecraft:iron_boots", 3));
    use_item(&mut s);
    s.tick(1);
    assert_eq!(slot(&mut s, 36).unwrap().key, "minecraft:iron_boots");
    assert_eq!(slot(&mut s, 0).unwrap().count, 2);
}

#[test]
fn an_elytra_is_worn_on_the_chest() {
    let mut s = scene_of();
    put(&mut s, 0, ItemSpec::of("minecraft:elytra", 1));
    use_item(&mut s);
    s.tick(1);
    assert_eq!(slot(&mut s, CHEST_SLOT).unwrap().key, "minecraft:elytra");
}

#[test]
fn a_swappable_false_equippable_stays_in_hand() {
    let mut s = scene_of();
    put(&mut s, 0, ItemSpec::of("minecraft:carved_pumpkin", 1));
    use_item(&mut s);
    s.tick(1);
    assert!(slot(&mut s, HELMET_SLOT).is_none());
    assert_eq!(slot(&mut s, 0).unwrap().key, "minecraft:carved_pumpkin");
}

#[test]
fn an_equippable_component_makes_any_item_wearable() {
    let mut s = scene_of();
    put(
        &mut s,
        0,
        with_components(
            ItemSpec::of("minecraft:stick", 1),
            Compound::new().with("minecraft:equippable", Compound::new().with("slot", "head")),
        ),
    );
    use_item(&mut s);
    s.tick(1);
    assert_eq!(slot(&mut s, HELMET_SLOT).unwrap().key, "minecraft:stick");
}

#[test]
fn a_helmet_with_the_binding_curse_is_not_swapped_out_by_right_click() {
    let mut s = scene_of();
    let mut cursed = ItemSpec::of("minecraft:iron_helmet", 1);
    cursed.enchantments = vec![("minecraft:binding_curse".to_owned(), 1)];
    put(&mut s, 39, cursed);
    put(&mut s, 0, ItemSpec::of("minecraft:diamond_helmet", 1));
    use_item(&mut s);
    s.tick(1);
    assert_eq!(
        slot(&mut s, HELMET_SLOT).unwrap().key,
        "minecraft:iron_helmet"
    );
    assert_eq!(slot(&mut s, 0).unwrap().key, "minecraft:diamond_helmet");
}

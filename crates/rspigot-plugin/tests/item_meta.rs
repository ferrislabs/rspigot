use std::collections::BTreeMap;

use rspigot_plugin::{
    Attribute, AttributeModifier, BannerLayer, Color, ComponentValue, DyeColor, EffectType,
    EquipmentSlot, Equippable, FireworkExplosion, FireworkShape, Fireworks, ItemFlag, ItemStack,
    LodestoneTarget, LodestoneTracker, Material, MetaValue, Operation, PotionContents,
    PotionEffect, Profile, ProfileProperty, Rarity, Tool, ToolRule, Trim, UseCooldown, WrittenBook,
};
use uuid::Uuid;

fn sword() -> ItemStack {
    ItemStack::new(Material::DiamondSword, 1)
}

#[test]
fn scalar_builders_read_back_through_the_snapshot() {
    let item = sword()
        .unbreakable(true)
        .damage(7)
        .max_damage(900)
        .repair_cost(3)
        .max_stack_size(1)
        .item_name("Relic")
        .rarity(Rarity::Epic)
        .enchantment_glint_override(true)
        .item_model("test:relic")
        .map_id(12)
        .glider(true)
        .fire_resistant(true);
    let meta = item.meta();
    assert!(meta.unbreakable());
    assert_eq!(meta.damage(), Some(7));
    assert_eq!(meta.max_damage(), Some(900));
    assert_eq!(meta.repair_cost(), Some(3));
    assert_eq!(meta.max_stack_size(), Some(1));
    assert_eq!(meta.item_name().as_deref(), Some("Relic"));
    assert_eq!(meta.rarity(), Some(Rarity::Epic));
    assert_eq!(meta.enchantment_glint_override(), Some(true));
    assert_eq!(meta.item_model().as_deref(), Some("test:relic"));
    assert_eq!(meta.map_id(), Some(12));
    assert!(meta.glider());
    assert!(meta.fire_resistant());
    let plain = sword().meta();
    assert!(!plain.unbreakable());
    assert_eq!(plain.damage(), None);
    assert!(
        !sword()
            .unbreakable(true)
            .unbreakable(false)
            .meta()
            .unbreakable()
    );
}

#[test]
fn flags_hide_the_components_they_map_to_and_can_be_shown_again() {
    let item = sword()
        .hide_flags([ItemFlag::HideAttributes, ItemFlag::HideUnbreakable])
        .hide_tooltip(false);
    let meta = item.meta();
    assert_eq!(
        meta.flags(),
        [ItemFlag::HideAttributes, ItemFlag::HideUnbreakable]
    );
    assert!(
        meta.hidden_components()
            .contains(&"minecraft:unbreakable".to_owned())
    );
    assert!(!meta.hide_tooltip());
    let shown = item.show(ItemFlag::HideAttributes);
    assert_eq!(shown.meta().flags(), [ItemFlag::HideUnbreakable]);
    let all_hidden = shown.hide_tooltip(true);
    assert!(all_hidden.meta().hide_tooltip());
    let cleared = all_hidden
        .show(ItemFlag::HideUnbreakable)
        .hide_tooltip(false);
    assert!(cleared.component("minecraft:tooltip_display").is_none());
}

#[test]
fn attribute_modifiers_replace_by_attribute_and_id() {
    let first = AttributeModifier::new(
        Attribute::AttackDamage,
        "test:bonus",
        4.0,
        Operation::AddValue,
        EquipmentSlot::Hand,
    );
    let item = sword().attribute_modifier(first.clone());
    assert_eq!(item.meta().attribute_modifiers(), [first]);
    let stronger = AttributeModifier::new(
        Attribute::AttackDamage,
        "test:bonus",
        9.0,
        Operation::AddMultipliedBase,
        EquipmentSlot::Hand,
    );
    let other = AttributeModifier::new(
        Attribute::MaxHealth,
        "test:health",
        2.0,
        Operation::AddValue,
        EquipmentSlot::Chest,
    );
    let item = item
        .attribute_modifier(stronger.clone())
        .attribute_modifier(other.clone());
    assert_eq!(item.meta().attribute_modifiers(), [stronger, other]);
}

#[test]
fn specific_metas_round_trip_through_the_stack() {
    let potion = PotionContents {
        potion: Some("minecraft:strength".to_owned()),
        custom_color: Some(Color::rgb(10, 20, 30)),
        effects: vec![PotionEffect::new(EffectType::Speed, 200, 1)],
        custom_name: Some("brew".to_owned()),
    };
    let layers = vec![BannerLayer::new("minecraft:stripe_top", DyeColor::Red)];
    let profile = Profile {
        name: Some("Notch".to_owned()),
        id: Some(Uuid::from_u128(77)),
        properties: vec![ProfileProperty {
            name: "textures".to_owned(),
            value: "v".to_owned(),
            signature: None,
        }],
    };
    let book = WrittenBook {
        title: "Title".to_owned(),
        author: "Me".to_owned(),
        generation: 1,
        pages: vec!["a".to_owned(), "b".to_owned()],
    };
    let explosion = FireworkExplosion {
        shape: FireworkShape::Burst,
        colors: vec![1, 2],
        fade_colors: vec![3],
        trail: true,
        twinkle: false,
    };
    let rocket = Fireworks {
        flight_duration: 2,
        explosions: vec![explosion.clone()],
    };
    let trim = Trim::new("minecraft:quartz", "minecraft:coast");
    let tracker = LodestoneTracker {
        target: Some(LodestoneTarget {
            dimension: "minecraft:overworld".to_owned(),
            pos: [4, 5, 6],
        }),
        tracked: true,
    };
    let gear = Equippable::new(EquipmentSlot::Head);
    let cooldown = UseCooldown {
        seconds: 1.5,
        group: None,
    };
    let tool = Tool {
        rules: vec![ToolRule {
            blocks: vec!["minecraft:stone".to_owned()],
            speed: Some(8.0),
            correct_for_drops: Some(true),
        }],
        default_mining_speed: 1.0,
        damage_per_block: 1,
    };
    let inner = ItemStack::new(Material::Stick, 3)
        .name("Twig")
        .lore(["a"])
        .enchant(rspigot_plugin::Enchantment::Sharpness, 2)
        .damage(4);
    let item = ItemStack::new(Material::Stone, 1)
        .dyed_color(Color::rgb(1, 2, 3))
        .potion_contents(potion.clone())
        .base_color(DyeColor::Blue)
        .banner_patterns(layers.clone())
        .profile(profile.clone())
        .written_book_content(book.clone())
        .firework_explosion(explosion.clone())
        .fireworks(rocket.clone())
        .trim(trim.clone())
        .lodestone_tracker(tracker.clone())
        .instrument("minecraft:ponder_goat_horn")
        .jukebox_playable("minecraft:cat")
        .equippable(gear.clone())
        .use_cooldown(cooldown.clone())
        .tool(tool.clone())
        .damage_resistant(["minecraft:lava"])
        .bundle_contents(std::slice::from_ref(&inner))
        .container(std::slice::from_ref(&inner))
        .charged_projectiles(std::slice::from_ref(&inner));
    let meta = item.meta();
    assert_eq!(meta.dyed_color(), Some(Color::rgb(1, 2, 3)));
    assert_eq!(meta.potion(), Some(potion));
    assert_eq!(meta.base_color(), Some(DyeColor::Blue));
    assert_eq!(meta.banner_patterns(), layers);
    assert_eq!(meta.profile(), Some(profile));
    assert_eq!(meta.written_book(), Some(book));
    assert_eq!(meta.firework_explosion(), Some(explosion));
    assert_eq!(meta.fireworks(), Some(rocket));
    assert_eq!(meta.trim(), Some(trim));
    assert_eq!(meta.lodestone_tracker(), Some(tracker));
    assert_eq!(
        meta.instrument().as_deref(),
        Some("minecraft:ponder_goat_horn")
    );
    assert_eq!(meta.jukebox_playable().as_deref(), Some("minecraft:cat"));
    assert_eq!(meta.equippable(), Some(gear));
    assert_eq!(meta.use_cooldown(), Some(cooldown));
    assert_eq!(meta.tool(), Some(tool));
    assert_eq!(
        meta.damage_resistant(),
        Some(vec!["minecraft:lava".to_owned()])
    );
    assert_eq!(meta.bundle_contents(), std::slice::from_ref(&inner));
    assert_eq!(meta.container(), std::slice::from_ref(&inner));
    assert_eq!(meta.charged_projectiles(), [inner]);
    let books = ItemStack::new(Material::WritableBook, 1).writable_book_content(["x", "y"]);
    assert_eq!(
        books.meta().writable_book(),
        Some(vec!["x".to_owned(), "y".to_owned()])
    );
}

#[test]
fn the_generic_accessor_reads_writes_and_removes_any_component() {
    let mut item = sword();
    let value = ComponentValue::Compound(BTreeMap::from([
        ("n".to_owned(), ComponentValue::Int(3)),
        (
            "l".to_owned(),
            ComponentValue::List(vec![ComponentValue::String("s".to_owned())]),
        ),
        ("f".to_owned(), ComponentValue::Float(0.5)),
        ("b".to_owned(), ComponentValue::bool(true)),
        ("a".to_owned(), ComponentValue::LongArray(vec![1, 2])),
    ]));
    item.set_component("minecraft:bees", value.clone()).unwrap();
    assert_eq!(item.component("minecraft:bees"), Some(value));
    assert_eq!(item.component_keys(), ["minecraft:bees"]);
    assert!(item.remove_component("minecraft:bees"));
    assert!(!item.remove_component("minecraft:bees"));
    assert_eq!(item, sword());
    assert!(
        item.set_component("no namespace", ComponentValue::Int(1))
            .is_err()
    );
    assert!(
        item.set_component("minecraft:lore", ComponentValue::Int(1))
            .is_err()
    );
}

#[test]
fn item_data_sets_gets_removes_and_lists_keys() {
    let mut item = sword();
    assert!(item.meta_keys().is_empty());
    item.meta_set("relic:kind", "fire");
    item.meta_set("relic:charges", 3);
    item.meta_set("relic:nested", MetaValue::List(vec![MetaValue::Bool(true)]));
    assert_eq!(
        item.meta_get("relic:kind"),
        Some(MetaValue::String("fire".to_owned()))
    );
    assert_eq!(item.meta_get("relic:charges"), Some(MetaValue::Int(3)));
    assert_eq!(
        item.meta_keys(),
        ["relic:charges", "relic:kind", "relic:nested"]
    );
    assert!(item.meta_remove("relic:kind"));
    assert!(!item.meta_remove("relic:kind"));
    assert_eq!(item.meta_get("relic:kind"), None);
    item.meta_remove("relic:charges");
    item.meta_remove("relic:nested");
    assert_eq!(item, sword());
    assert!(item.component("minecraft:custom_data").is_none());
    let tagged = sword().with_meta("a:b", 1);
    assert_ne!(tagged, sword());
}

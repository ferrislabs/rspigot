use rspigot_nbt::{Compound, Tag};

use super::nbt::{compound_mut, ensure_namespaced, rename_key, walk_item_stacks};

const BOOLEAN_BLOCK_STATE_PROPERTIES: &[&str] = &[
    "attached",
    "bottom",
    "conditional",
    "disarmed",
    "drag",
    "enabled",
    "extended",
    "eye",
    "falling",
    "hanging",
    "has_bottle_0",
    "has_bottle_1",
    "has_bottle_2",
    "has_record",
    "has_book",
    "inverted",
    "in_wall",
    "lit",
    "locked",
    "occupied",
    "open",
    "persistent",
    "powered",
    "short",
    "signal_fire",
    "snowy",
    "triggered",
    "unstable",
    "waterlogged",
    "berries",
    "bloom",
    "shrieking",
    "can_summon",
    "up",
    "down",
    "north",
    "east",
    "south",
    "west",
    "slot_0_occupied",
    "slot_1_occupied",
    "slot_2_occupied",
    "slot_3_occupied",
    "slot_4_occupied",
    "slot_5_occupied",
    "cracked",
];

const BUCKETED_MOB_IDS: &[&str] = &[
    "minecraft:pufferfish_bucket",
    "minecraft:salmon_bucket",
    "minecraft:cod_bucket",
    "minecraft:tropical_fish_bucket",
    "minecraft:axolotl_bucket",
    "minecraft:tadpole_bucket",
];

const BUCKETED_MOB_TAGS: &[&str] = &[
    "NoAI",
    "Silent",
    "NoGravity",
    "Glowing",
    "Invulnerable",
    "Health",
    "Age",
    "Variant",
    "HuntingCooldown",
    "BucketVariantTag",
];

const POTION_HOLDER_IDS: &[&str] = &[
    "minecraft:potion",
    "minecraft:splash_potion",
    "minecraft:lingering_potion",
    "minecraft:tipped_arrow",
];

const CONTAINER_BLOCK_ENTITIES: &[&str] = &[
    "minecraft:shulker_box",
    "minecraft:chest",
    "minecraft:trapped_chest",
    "minecraft:furnace",
    "minecraft:ender_chest",
    "minecraft:dispenser",
    "minecraft:dropper",
    "minecraft:brewing_stand",
    "minecraft:hopper",
    "minecraft:barrel",
    "minecraft:smoker",
    "minecraft:blast_furnace",
    "minecraft:campfire",
    "minecraft:chiseled_bookshelf",
    "minecraft:crafter",
];

const DYE_COLORS: &[&str] = &[
    "white",
    "orange",
    "magenta",
    "light_blue",
    "yellow",
    "lime",
    "pink",
    "gray",
    "light_gray",
    "cyan",
    "purple",
    "blue",
    "brown",
    "green",
    "red",
    "black",
];

const MAP_DECORATION_TYPES: &[&str] = &[
    "player",
    "frame",
    "red_marker",
    "blue_marker",
    "target_x",
    "target_point",
    "player_off_map",
    "player_off_limits",
    "mansion",
    "monument",
    "banner_white",
    "banner_orange",
    "banner_magenta",
    "banner_light_blue",
    "banner_yellow",
    "banner_lime",
    "banner_pink",
    "banner_gray",
    "banner_light_gray",
    "banner_cyan",
    "banner_purple",
    "banner_blue",
    "banner_brown",
    "banner_green",
    "banner_red",
    "banner_black",
    "red_x",
    "village_desert",
    "village_plains",
    "village_savanna",
    "village_snowy",
    "village_taiga",
    "jungle_temple",
    "swamp_hut",
];

const FIREWORK_SHAPES: &[&str] = &["small_ball", "large_ball", "star", "creeper", "burst"];

const DEFAULT_LEATHER_COLOR: i32 = 10_511_680;

struct ItemData {
    id: String,
    count: i32,
    tag: Compound,
    components: Compound,
}

impl ItemData {
    fn is(&self, id: &str) -> bool {
        self.id == id
    }

    fn remove_tag(&mut self, key: &str) -> Option<Tag> {
        self.tag.remove(key)
    }

    fn set_component(&mut self, name: &str, value: impl Into<Tag>) {
        self.components.insert(name, value);
    }

    fn move_tag(&mut self, key: &str, component: &str) {
        if let Some(value) = self.tag.remove(key) {
            self.components.insert(component, value);
        }
    }

    fn move_tag_unless(&mut self, key: &str, component: &str, default: &Tag) {
        if let Some(value) = self.tag.remove(key)
            && !same_value(&value, default)
        {
            self.components.insert(component, value);
        }
    }

    fn move_tag_into(&mut self, key: &str, target: &mut Compound, field: &str) {
        if let Some(value) = self.tag.remove(key) {
            target.insert(field, value);
        }
    }

    fn fix_sub_tag(&mut self, key: &str, fix: impl FnOnce(&mut Self, Compound) -> Compound) {
        let Some(Tag::Compound(sub)) = self.tag.remove(key) else {
            return;
        };
        let fixed = fix(self, sub);
        if !fixed.is_empty() {
            self.tag.insert(key, fixed);
        }
    }
}

fn same_value(left: &Tag, right: &Tag) -> bool {
    match (left.as_i64(), right.as_i64()) {
        (Some(a), Some(b)) => a == b,
        _ => left == right,
    }
}

fn read(stack: &Compound) -> Option<ItemData> {
    let id = ensure_namespaced(stack.get_str("id")?);
    let count = i32::try_from(stack.get("Count")?.as_i64()?).ok()?;
    let tag = stack.get_compound("tag").cloned().unwrap_or_default();
    Some(ItemData {
        id,
        count,
        tag,
        components: Compound::new(),
    })
}

fn write(stack: &mut Compound, data: ItemData) {
    stack.remove("id");
    stack.remove("Count");
    stack.remove("tag");
    let mut components = data.components;
    if !data.tag.is_empty() {
        components.insert("minecraft:custom_data", data.tag);
    }
    let mut out = Compound::new()
        .with("id", data.id)
        .with("count", data.count);
    if !components.is_empty() {
        out.insert("components", components);
    }
    for (key, value) in std::mem::take(stack) {
        out.insert(key, value);
    }
    *stack = out;
}

fn flag(tag: Option<&Tag>) -> bool {
    tag.and_then(Tag::as_i64).is_some_and(|value| value != 0)
}

fn int_or(tag: Option<&Tag>, default: i64) -> i64 {
    tag.and_then(Tag::as_i64).unwrap_or(default)
}

fn string_or_empty(tag: Option<&Tag>) -> String {
    tag.and_then(Tag::as_str).unwrap_or_default().to_owned()
}

fn show_in_tooltip_off(mut compound: Compound, hidden: bool) -> Compound {
    if hidden {
        compound.insert("show_in_tooltip", false);
    }
    compound
}

fn fix_item_stack(data: &mut ItemData) {
    let hide_flags = int_or(data.remove_tag("HideFlags").as_ref(), 0);
    data.move_tag_unless("Damage", "minecraft:damage", &Tag::Int(0));
    data.move_tag_unless("RepairCost", "minecraft:repair_cost", &Tag::Int(0));
    data.move_tag("CustomModelData", "minecraft:custom_model_data");
    if let Some(Tag::Compound(state)) = data.remove_tag("BlockStateTag") {
        data.set_component("minecraft:block_state", fix_block_state_tag(state));
    }
    data.move_tag("EntityTag", "minecraft:entity_data");
    data.fix_sub_tag("BlockEntityTag", |data, tag| {
        let block_entity_id = ensure_namespaced(tag.get_str("id").unwrap_or(""));
        let fixed = fix_block_entity_tag(data, tag, &block_entity_id);
        let mut without_id = fixed.clone();
        without_id.remove("id");
        if without_id.is_empty() {
            Compound::new()
        } else {
            fixed
        }
    });
    data.move_tag("BlockEntityTag", "minecraft:block_entity_data");
    if let Some(unbreakable) = data.remove_tag("Unbreakable")
        && flag(Some(&unbreakable))
    {
        let marker = show_in_tooltip_off(Compound::new(), hide_flags & 4 != 0);
        data.set_component("minecraft:unbreakable", marker);
    }
    fix_enchantments(
        data,
        "Enchantments",
        "minecraft:enchantments",
        hide_flags & 1 != 0,
    );
    if data.is("minecraft:enchanted_book") {
        fix_enchantments(
            data,
            "StoredEnchantments",
            "minecraft:stored_enchantments",
            hide_flags & 32 != 0,
        );
    }
    data.fix_sub_tag("display", |data, display| {
        fix_display(data, display, hide_flags)
    });
    fix_adventure_mode_checks(data, hide_flags);
    fix_attribute_modifiers(data, hide_flags);
    if let Some(Tag::Compound(trim)) = data.remove_tag("Trim") {
        data.set_component(
            "minecraft:trim",
            show_in_tooltip_off(trim, hide_flags & 128 != 0),
        );
    }
    if hide_flags & 32 != 0 {
        data.set_component("minecraft:hide_additional_tooltip", Compound::new());
    }
    if data.is("minecraft:crossbow") {
        data.remove_tag("Charged");
        data.move_tag_unless(
            "ChargedProjectiles",
            "minecraft:charged_projectiles",
            &Tag::List(Vec::new()),
        );
    }
    if data.is("minecraft:bundle") {
        data.move_tag_unless("Items", "minecraft:bundle_contents", &Tag::List(Vec::new()));
    }
    if data.is("minecraft:filled_map") {
        data.move_tag("map", "minecraft:map_id");
        fix_map_decorations(data);
    }
    if POTION_HOLDER_IDS.contains(&data.id.as_str()) {
        fix_potion_contents(data);
    }
    if data.is("minecraft:writable_book") {
        fix_writable_book(data);
    }
    if data.is("minecraft:written_book") {
        fix_written_book(data);
    }
    if data.is("minecraft:suspicious_stew") {
        data.move_tag("effects", "minecraft:suspicious_stew_effects");
    }
    if data.is("minecraft:debug_stick") {
        data.move_tag("DebugProperty", "minecraft:debug_stick_state");
    }
    if BUCKETED_MOB_IDS.contains(&data.id.as_str()) {
        fix_bucketed_mob_data(data);
    }
    if data.is("minecraft:goat_horn") {
        data.move_tag("instrument", "minecraft:instrument");
    }
    if data.is("minecraft:knowledge_book") {
        data.move_tag("Recipes", "minecraft:recipes");
    }
    if data.is("minecraft:compass") {
        fix_lodestone_tracker(data);
    }
    if data.is("minecraft:firework_rocket") {
        data.fix_sub_tag("Fireworks", |data, rocket| {
            fix_firework_rocket(data, rocket)
        });
    }
    if data.is("minecraft:firework_star") {
        data.fix_sub_tag("Explosion", fix_firework_star);
    }
    if data.is("minecraft:player_head")
        && let Some(owner) = data.remove_tag("SkullOwner")
    {
        data.set_component("minecraft:profile", fix_profile(owner));
    }
}

fn fix_block_state_tag(state: Compound) -> Compound {
    state
        .into_iter()
        .map(|(key, value)| {
            let fixed = if BOOLEAN_BLOCK_STATE_PROPERTIES.contains(&key.as_str())
                && let Some(number) = value.as_i64()
            {
                Tag::String((number != 0).to_string())
            } else if let Some(number) = value.as_i64() {
                Tag::String(number.to_string())
            } else {
                value
            };
            (key, fixed)
        })
        .collect()
}

fn fix_block_entity_tag(data: &mut ItemData, mut tag: Compound, id: &str) -> Compound {
    if let Some(lock) = tag.remove("Lock") {
        data.set_component("minecraft:lock", lock);
    }
    if let Some(loot_table) = tag.remove("LootTable") {
        let mut loot = Compound::new().with("loot_table", loot_table);
        let seed = tag.get_i64("LootTableSeed").unwrap_or(0);
        if seed != 0 {
            loot.insert("seed", seed);
        }
        data.set_component("minecraft:container_loot", loot);
    }
    tag.remove("LootTableSeed");
    match id {
        "minecraft:skull" => {
            if let Some(sound) = tag.remove("note_block_sound") {
                data.set_component("minecraft:note_block_sound", sound);
            }
        }
        "minecraft:decorated_pot" => {
            if let Some(sherds) = tag.remove("sherds") {
                data.set_component("minecraft:pot_decorations", sherds);
            }
            if let Some(item) = tag.remove("item") {
                let slot = Compound::new().with("slot", 0).with("item", item);
                data.set_component("minecraft:container", Tag::List(vec![Tag::Compound(slot)]));
            }
        }
        "minecraft:banner" => {
            if let Some(patterns) = tag.remove("patterns") {
                data.set_component("minecraft:banner_patterns", patterns);
            }
            if let Some(base) = tag.remove("Base").and_then(|base| base.as_i64()) {
                let name = usize::try_from(base)
                    .ok()
                    .and_then(|index| DYE_COLORS.get(index))
                    .copied()
                    .unwrap_or("white");
                data.set_component("minecraft:base_color", name);
            }
        }
        "minecraft:beehive" => {
            if let Some(bees) = tag.remove("bees") {
                data.set_component("minecraft:bees", bees);
            }
        }
        id if CONTAINER_BLOCK_ENTITIES.contains(&id) => {
            if let Some(Tag::List(items)) = tag.remove("Items") {
                if items.is_empty() {
                    tag.insert("Items", Tag::List(items));
                } else {
                    let slots = items.into_iter().map(slot_entry).collect();
                    data.set_component("minecraft:container", Tag::List(slots));
                }
            }
        }
        _ => {}
    }
    tag
}

fn slot_entry(entry: Tag) -> Tag {
    let Tag::Compound(mut item) = entry else {
        return entry;
    };
    let slot = item
        .remove("Slot")
        .and_then(|slot| slot.as_i64())
        .unwrap_or(0)
        & 0xff;
    Tag::Compound(Compound::new().with("slot", slot as i32).with("item", item))
}

fn parse_enchantment(tag: &Tag) -> Option<(String, i32)> {
    let compound = tag.as_compound()?;
    let id = compound.get_str("id")?.to_owned();
    let level = compound.get("lvl")?.as_i64()?.clamp(0, 255) as i32;
    Some((id, level))
}

fn fix_enchantments(data: &mut ItemData, key: &str, component: &str, hide: bool) {
    let Some(removed) = data.remove_tag(key) else {
        return;
    };
    let enchantments: Vec<(String, i32)> = removed
        .as_list()
        .unwrap_or_default()
        .iter()
        .filter_map(parse_enchantment)
        .filter(|(_, level)| *level > 0)
        .collect();
    if !enchantments.is_empty() || hide {
        let mut levels = Compound::new();
        for (id, level) in &enchantments {
            levels.insert(id.as_str(), *level);
        }
        let value = show_in_tooltip_off(Compound::new().with("levels", levels), hide);
        data.set_component(component, value);
    }
    if enchantments.is_empty() {
        data.set_component("minecraft:enchantment_glint_override", true);
    }
}

fn fix_display(data: &mut ItemData, mut display: Compound, hide_flags: i64) -> Compound {
    if let Some(name) = display.remove("Name") {
        data.set_component("minecraft:custom_name", name);
    }
    if let Some(Tag::List(lore)) = display.remove("Lore") {
        data.set_component("minecraft:lore", Tag::List(lore));
    }
    let color = display.remove("color").and_then(|color| color.as_i64());
    let hide_color = hide_flags & 64 != 0;
    if color.is_some() || hide_color {
        let rgb = color.map_or(DEFAULT_LEATHER_COLOR, |c| c as i32);
        let value = show_in_tooltip_off(Compound::new().with("rgb", rgb), hide_color);
        data.set_component("minecraft:dyed_color", value);
    }
    if let Some(key) = display
        .remove("LocName")
        .and_then(|name| name.as_str().map(str::to_owned))
    {
        data.set_component("minecraft:item_name", translatable_json(&key));
    }
    if data.is("minecraft:filled_map")
        && let Some(color) = display.remove("MapColor")
    {
        data.set_component("minecraft:map_color", color);
    }
    display.remove("MapColor");
    display
}

fn translatable_json(key: &str) -> String {
    serde_json::json!({ "translate": key }).to_string()
}

fn fix_adventure_mode_checks(data: &mut ItemData, hide_flags: i64) {
    fix_block_state_predicates(
        data,
        "CanDestroy",
        "minecraft:can_break",
        hide_flags & 8 != 0,
    );
    fix_block_state_predicates(
        data,
        "CanPlaceOn",
        "minecraft:can_place_on",
        hide_flags & 16 != 0,
    );
}

fn fix_block_state_predicates(data: &mut ItemData, key: &str, component: &str, hide: bool) {
    let Some(Tag::List(entries)) = data.remove_tag(key) else {
        return;
    };
    let predicates = entries
        .into_iter()
        .map(|entry| match entry {
            Tag::String(text) => fix_block_state_predicate(&text),
            other => other,
        })
        .collect();
    let value = show_in_tooltip_off(
        Compound::new().with("predicates", Tag::List(predicates)),
        hide,
    );
    data.set_component(component, value);
}

fn fix_block_state_predicate(text: &str) -> Tag {
    let bracket = text.find('[');
    let brace = text.find('{');
    let mut end = text.len();
    if let Some(bracket) = bracket {
        end = end.min(bracket);
    }
    if let Some(brace) = brace {
        end = end.min(brace);
    }
    let mut predicate = Compound::new().with("blocks", text[..end].trim());
    if let Some(bracket) = bracket
        && let Some(close) = text[bracket..].find(']').map(|offset| bracket + offset)
    {
        let mut state = Compound::new();
        for property in text[bracket + 1..close].split(',') {
            if let Some((key, value)) = property.split_once('=') {
                state.insert(key.trim(), value.trim());
            }
        }
        if !state.is_empty() {
            predicate.insert("state", state);
        }
    }
    if let Some(brace) = brace
        && let Some(close) = text.rfind('}')
        && close >= brace
    {
        predicate.insert("nbt", &text[brace..=close]);
    }
    Tag::Compound(predicate)
}

fn operation_name(tag: Option<&Tag>) -> &'static str {
    match int_or(tag, 0) {
        1 => "add_multiplied_base",
        2 => "add_multiplied_total",
        _ => "add_value",
    }
}

fn fix_attribute_modifier(entry: Tag) -> Tag {
    let Tag::Compound(mut old) = entry else {
        return entry;
    };
    let mut modifier = Compound::new()
        .with("name", "")
        .with("amount", 0.0)
        .with("operation", "add_value");
    for (from, to) in [
        ("AttributeName", "type"),
        ("Slot", "slot"),
        ("UUID", "uuid"),
        ("Name", "name"),
        ("Amount", "amount"),
    ] {
        if let Some(value) = old.remove(from) {
            modifier.insert(to, value);
        }
    }
    if let Some(operation) = old.remove("Operation") {
        modifier.insert("operation", operation_name(Some(&operation)));
    }
    Tag::Compound(modifier)
}

fn fix_attribute_modifiers(data: &mut ItemData, hide_flags: i64) {
    let Some(removed) = data.remove_tag("AttributeModifiers") else {
        return;
    };
    let Tag::List(entries) = removed else {
        return;
    };
    let modifiers = entries.into_iter().map(fix_attribute_modifier).collect();
    let value = show_in_tooltip_off(
        Compound::new().with("modifiers", Tag::List(modifiers)),
        hide_flags & 2 != 0,
    );
    data.set_component("minecraft:attribute_modifiers", value);
}

fn fix_map_decorations(data: &mut ItemData) {
    let Some(Tag::List(entries)) = data.remove_tag("Decorations") else {
        return;
    };
    let mut decorations = Compound::new();
    for entry in entries {
        let Some(entry) = entry.as_compound() else {
            continue;
        };
        let kind = usize::try_from(int_or(entry.get("type"), 0))
            .ok()
            .and_then(|index| MAP_DECORATION_TYPES.get(index))
            .copied()
            .unwrap_or("player");
        let decoration = Compound::new()
            .with("type", kind)
            .with("x", entry.get_f64("x").unwrap_or(0.0))
            .with("z", entry.get_f64("z").unwrap_or(0.0))
            .with("rotation", entry.get_f64("rot").unwrap_or(0.0) as f32);
        decorations.insert(entry.get_str("id").unwrap_or(""), decoration);
    }
    if !decorations.is_empty() {
        data.set_component("minecraft:map_decorations", decorations);
    }
}

fn fix_potion_contents(data: &mut ItemData) {
    let mut contents = Compound::new();
    if let Some(Tag::String(potion)) = data.remove_tag("Potion")
        && potion != "minecraft:empty"
    {
        contents.insert("potion", potion);
    }
    data.move_tag_into("CustomPotionColor", &mut contents, "custom_color");
    data.move_tag_into("custom_potion_effects", &mut contents, "custom_effects");
    if !contents.is_empty() {
        data.set_component("minecraft:potion_contents", contents);
    }
}

fn filtered_text(raw: &str, filtered: Option<&Tag>) -> Tag {
    let mut text = Compound::new().with("raw", raw);
    if let Some(Tag::String(filtered)) = filtered {
        text.insert("filtered", filtered.as_str());
    }
    Tag::Compound(text)
}

fn fix_book_pages(data: &mut ItemData) -> Option<Tag> {
    let pages = data.remove_tag("pages");
    let filtered = data.remove_tag("filtered_pages");
    let pages = pages?;
    let pages = pages.as_list()?;
    if pages.is_empty() {
        return None;
    }
    let filtered = filtered.as_ref().and_then(Tag::as_compound);
    let list = pages
        .iter()
        .enumerate()
        .map(|(index, page)| {
            filtered_text(
                page.as_str().unwrap_or_default(),
                filtered.and_then(|map| map.get(&index.to_string())),
            )
        })
        .collect();
    Some(Tag::List(list))
}

fn fix_writable_book(data: &mut ItemData) {
    if let Some(pages) = fix_book_pages(data) {
        data.set_component(
            "minecraft:writable_book_content",
            Compound::new().with("pages", pages),
        );
    }
}

fn fix_written_book(data: &mut ItemData) {
    let pages = fix_book_pages(data);
    let title = string_or_empty(data.remove_tag("title").as_ref());
    let filtered_title = data.remove_tag("filtered_title");
    let mut content = Compound::new().with("title", filtered_text(&title, filtered_title.as_ref()));
    data.move_tag_into("author", &mut content, "author");
    data.move_tag_into("resolved", &mut content, "resolved");
    data.move_tag_into("generation", &mut content, "generation");
    if let Some(pages) = pages {
        content.insert("pages", pages);
    }
    data.set_component("minecraft:written_book_content", content);
}

fn fix_bucketed_mob_data(data: &mut ItemData) {
    let mut bucket = Compound::new();
    for key in BUCKETED_MOB_TAGS {
        data.move_tag_into(key, &mut bucket, key);
    }
    if !bucket.is_empty() {
        data.set_component("minecraft:bucket_entity_data", bucket);
    }
}

fn fix_lodestone_tracker(data: &mut ItemData) {
    let position = data.remove_tag("LodestonePos");
    let dimension = data.remove_tag("LodestoneDimension");
    let tracked = data.remove_tag("LodestoneTracked");
    if position.is_none() && dimension.is_none() {
        return;
    }
    let mut tracker = Compound::new();
    if let (Some(position), Some(dimension)) = (position, dimension) {
        tracker.insert(
            "target",
            Compound::new()
                .with("pos", position)
                .with("dimension", dimension),
        );
    }
    if tracked.as_ref().is_some_and(|tracked| !flag(Some(tracked))) {
        tracker.insert("tracked", false);
    }
    data.set_component("minecraft:lodestone_tracker", tracker);
}

fn fix_firework_explosion(mut explosion: Compound) -> Compound {
    let shape = usize::try_from(int_or(explosion.get("Type"), 0))
        .ok()
        .and_then(|index| FIREWORK_SHAPES.get(index))
        .copied()
        .unwrap_or("small_ball");
    explosion.insert("shape", shape);
    explosion.remove("Type");
    rename_key(&mut explosion, "Colors", "colors");
    rename_key(&mut explosion, "FadeColors", "fade_colors");
    rename_key(&mut explosion, "Trail", "has_trail");
    rename_key(&mut explosion, "Flicker", "has_twinkle");
    explosion
}

fn fix_firework_rocket(data: &mut ItemData, mut rocket: Compound) -> Compound {
    let explosions = match rocket.remove("Explosions") {
        Some(Tag::List(list)) => list,
        _ => Vec::new(),
    };
    let explosions = explosions
        .into_iter()
        .map(|explosion| match explosion {
            Tag::Compound(explosion) => Tag::Compound(fix_firework_explosion(explosion)),
            other => other,
        })
        .collect();
    let flight = int_or(rocket.remove("Flight").as_ref(), 0) as i8;
    data.set_component(
        "minecraft:fireworks",
        Compound::new()
            .with("explosions", Tag::List(explosions))
            .with("flight_duration", flight),
    );
    rocket
}

fn fix_firework_star(data: &mut ItemData, mut star: Compound) -> Compound {
    let explosion = fix_firework_explosion(star.clone());
    data.set_component("minecraft:firework_explosion", explosion);
    for key in ["Type", "Colors", "FadeColors", "Trail", "Flicker"] {
        star.remove(key);
    }
    star
}

fn is_valid_player_name(name: &str) -> bool {
    name.chars().count() <= 16 && name.chars().all(|c| c > ' ' && c < '\u{7f}')
}

fn fix_profile(owner: Tag) -> Compound {
    match owner {
        Tag::String(name) if is_valid_player_name(&name) => Compound::new().with("name", name),
        Tag::Compound(owner) => {
            let mut profile = Compound::new();
            let name = string_or_empty(owner.get("Name"));
            if is_valid_player_name(&name) {
                profile.insert("name", name);
            }
            if let Some(id) = owner.get("Id") {
                profile.insert("id", id.clone());
            }
            if let Some(properties) = owner.get_compound("Properties")
                && !properties.is_empty()
            {
                profile.insert("properties", fix_profile_properties(properties));
            }
            profile
        }
        _ => Compound::new(),
    }
}

fn fix_profile_properties(properties: &Compound) -> Tag {
    let mut out = Vec::new();
    for (name, values) in properties.iter() {
        for value in values.as_list().unwrap_or_default() {
            let Some(value) = value.as_compound() else {
                continue;
            };
            let mut property = Compound::new()
                .with("name", name)
                .with("value", value.get_str("Value").unwrap_or(""));
            if let Some(signature) = value.get_str("Signature") {
                property.insert("signature", signature);
            }
            out.push(Tag::Compound(property));
        }
    }
    Tag::List(out)
}

pub fn componentize_3818(root: &mut Compound) {
    walk_item_stacks(root, &mut |stack| {
        if !stack.contains_key("Count") {
            return;
        }
        let Some(mut data) = read(stack) else {
            return;
        };
        fix_item_stack(&mut data);
        write(stack, data);
    });
}

fn for_each_components<F: FnMut(&mut Compound)>(root: &mut Compound, visit: &mut F) {
    walk_item_stacks(root, &mut |stack| {
        if let Some(components) = compound_mut(stack, "components") {
            visit(components);
        }
    });
}

fn update_component(
    components: &mut Compound,
    name: &str,
    update: impl FnOnce(Tag) -> Option<Tag>,
) {
    if let Some(value) = components.remove(name)
        && let Some(updated) = update(value)
    {
        components.insert(name, updated);
    }
}

pub fn skull_profile_3820(root: &mut Compound) {
    super::nbt::walk_compounds(root, &mut |compound| {
        if compound.get_str("id") != Some("minecraft:skull") {
            return;
        }
        let owner = compound.remove("SkullOwner");
        let extra = compound.remove("ExtraType");
        if let Some(owner) = owner.or(extra) {
            compound.insert("profile", fix_profile(owner));
        }
    });
}

pub fn ominous_banner_name_3825(root: &mut Compound) {
    walk_item_stacks(root, &mut |stack| {
        let id = stack.get_str("id").map(ensure_namespaced);
        let Some(components) = compound_mut(stack, "components") else {
            return;
        };
        let name = components
            .get_str("minecraft:custom_name")
            .map(str::to_owned);
        let Some(name) = name else {
            return;
        };
        let translation = serde_json::from_str::<serde_json::Value>(&name)
            .ok()
            .and_then(|value| {
                value
                    .get("translate")
                    .and_then(|t| t.as_str().map(str::to_owned))
            });
        let Some(key) = translation.or_else(|| Some(name.clone())) else {
            return;
        };
        let overridden = match id.as_deref() {
            Some("minecraft:white_banner") => key == "block.minecraft.ominous_banner",
            Some("minecraft:filled_map") => MAP_NAME_KEYS.contains(&key.as_str()),
            _ => false,
        };
        if overridden {
            rename_key(components, "minecraft:custom_name", "minecraft:item_name");
        }
    });
}

const MAP_NAME_KEYS: &[&str] = &[
    "filled_map.buried_treasure",
    "filled_map.explorer_jungle",
    "filled_map.explorer_swamp",
    "filled_map.mansion",
    "filled_map.monument",
    "filled_map.trial_chambers",
    "filled_map.village_desert",
    "filled_map.village_plains",
    "filled_map.village_savanna",
    "filled_map.village_snowy",
    "filled_map.village_taiga",
];

pub fn food_to_consumable_4059(root: &mut Compound) {
    for_each_components(root, &mut |components| {
        let Some(Tag::Compound(mut food)) = components.remove("minecraft:food") else {
            return;
        };
        let seconds = food
            .remove("eat_seconds")
            .and_then(|seconds| seconds.as_f64())
            .unwrap_or(1.6) as f32;
        let effects: Vec<Tag> = food
            .remove("effects")
            .and_then(|effects| match effects {
                Tag::List(list) => Some(list),
                _ => None,
            })
            .unwrap_or_default()
            .into_iter()
            .map(|entry| {
                let entry = entry.as_compound();
                let effect = entry.and_then(|e| e.get("effect")).cloned();
                let probability =
                    entry.and_then(|e| e.get_f64("probability")).unwrap_or(1.0) as f32;
                let mut applied = Compound::new().with("type", "minecraft:apply_effects");
                applied.insert("effects", Tag::List(effect.into_iter().collect()));
                applied.insert("probability", probability);
                Tag::Compound(applied)
            })
            .collect();
        if let Some(remainder) = food.remove("using_converts_to") {
            components.insert("minecraft:use_remainder", remainder);
        }
        components.insert("minecraft:food", food);
        components.insert(
            "minecraft:consumable",
            Compound::new()
                .with("consume_seconds", seconds)
                .with("on_consume_effects", Tag::List(effects)),
        );
    });
}

pub fn fire_resistant_to_damage_resistant_4064(root: &mut Compound) {
    for_each_components(root, &mut |components| {
        if components.remove("minecraft:fire_resistant").is_some() {
            components.insert(
                "minecraft:damage_resistant",
                Compound::new().with("types", "#minecraft:is_fire"),
            );
        }
    });
}

fn escape_lock_name(name: &str) -> String {
    name.replace('\\', "\\\\").replace('"', "\\\"")
}

pub fn lock_to_predicate_4068(root: &mut Compound) {
    for_each_components(root, &mut |components| {
        update_component(components, "minecraft:lock", |lock| {
            let Tag::String(text) = &lock else {
                return Some(lock);
            };
            if text.is_empty() {
                return None;
            }
            let name = Tag::String(format!("\"{}\"", escape_lock_name(text)));
            Some(Tag::Compound(Compound::new().with(
                "components",
                Compound::new().with("minecraft:custom_name", name),
            )))
        });
    });
}

pub fn custom_model_data_expand_4175(root: &mut Compound) {
    for_each_components(root, &mut |components| {
        update_component(components, "minecraft:custom_model_data", |value| {
            let number = value.as_f64()?;
            Some(Tag::Compound(
                Compound::new().with("floats", Tag::List(vec![Tag::Float(number as f32)])),
            ))
        });
    });
}

pub fn equippable_asset_rename_4175(root: &mut Compound) {
    for_each_components(root, &mut |components| {
        if let Some(equippable) = compound_mut(components, "minecraft:equippable") {
            rename_key(equippable, "model", "asset_id");
        }
    });
}

pub fn invalid_lock_4176(root: &mut Compound) {
    for_each_components(root, &mut |components| {
        let broken = components
            .get_compound("minecraft:lock")
            .and_then(|lock| lock.get_compound("components"))
            .is_some_and(|inner| {
                inner.len() == 1 && inner.get_str("minecraft:custom_name") == Some("\"\"")
            });
        if broken {
            components.remove("minecraft:lock");
        }
    });
}

pub fn pot_decorations_component_4996(root: &mut Compound) {
    for_each_components(root, &mut |components| {
        update_component(
            components,
            "minecraft:pot_decorations",
            |value| match value {
                Tag::List(items) => Some(super::chunk::unpack_pot_decorations(&items)),
                other => Some(other),
            },
        );
    });
}

const ADDITIONAL_TOOLTIP_COMPONENTS: &[&str] = &[
    "minecraft:banner_patterns",
    "minecraft:bees",
    "minecraft:block_entity_data",
    "minecraft:block_state",
    "minecraft:bundle_contents",
    "minecraft:charged_projectiles",
    "minecraft:container",
    "minecraft:container_loot",
    "minecraft:firework_explosion",
    "minecraft:fireworks",
    "minecraft:instrument",
    "minecraft:map_id",
    "minecraft:painting/variant",
    "minecraft:pot_decorations",
    "minecraft:potion_contents",
    "minecraft:tropical_fish/pattern",
    "minecraft:written_book_content",
];

fn take_show_in_tooltip(component: &mut Compound) -> bool {
    component
        .remove("show_in_tooltip")
        .and_then(|flag| flag.as_i64())
        .is_none_or(|flag| flag != 0)
}

pub fn tooltip_display_4307(root: &mut Compound) {
    for_each_components(root, &mut |components| {
        let mut hidden: Vec<&str> = Vec::new();
        for name in ["minecraft:can_place_on", "minecraft:can_break"] {
            let Some(Tag::Compound(mut predicate)) = components.remove(name) else {
                continue;
            };
            let Some(predicates) = predicate.remove("predicates") else {
                components.insert(name, predicate);
                continue;
            };
            if !take_show_in_tooltip(&mut predicate) {
                hidden.push(name);
            }
            components.insert(name, predicates);
        }
        for name in ["minecraft:trim", "minecraft:unbreakable"] {
            if let Some(component) = compound_mut(components, name)
                && !take_show_in_tooltip(component)
            {
                hidden.push(name);
            }
        }
        for (name, field) in [
            ("minecraft:dyed_color", "rgb"),
            ("minecraft:attribute_modifiers", "modifiers"),
            ("minecraft:enchantments", "levels"),
            ("minecraft:stored_enchantments", "levels"),
            ("minecraft:jukebox_playable", "song"),
        ] {
            let Some(Tag::Compound(mut component)) = components.remove(name) else {
                continue;
            };
            if !take_show_in_tooltip(&mut component) {
                hidden.push(name);
            }
            match component.remove(field) {
                Some(inner) => components.insert(name, inner),
                None => components.insert(name, component),
            };
        }
        let hide_tooltip = components.remove("minecraft:hide_tooltip").is_some();
        if components
            .remove("minecraft:hide_additional_tooltip")
            .is_some()
        {
            for name in ADDITIONAL_TOOLTIP_COMPONENTS {
                if components.contains_key(name) && !hidden.contains(name) {
                    hidden.push(name);
                }
            }
        }
        if hide_tooltip || !hidden.is_empty() {
            components.insert(
                "minecraft:tooltip_display",
                Compound::new().with("hide_tooltip", hide_tooltip).with(
                    "hidden_components",
                    Tag::List(hidden.into_iter().map(Tag::from).collect()),
                ),
            );
        }
    });
}

pub fn swing_animation_split_5007(root: &mut Compound) {
    for_each_components(root, &mut |components| {
        if let Some(swing) = components.remove("minecraft:swing_animation") {
            components.insert("minecraft:attack_animation", swing.clone());
            components.insert("minecraft:interact_animation", swing);
        }
    });
}

pub fn map_color_removal_5008(root: &mut Compound) {
    for_each_components(root, &mut |components| {
        components.remove("minecraft:map_color");
    });
}

pub fn remove_block_transformer_5014(root: &mut Compound) {
    for_each_components(root, &mut |components| {
        components.remove("minecraft:block_transformer");
    });
}

pub fn unfilled_buried_treasure_map_5016(root: &mut Compound) {
    walk_item_stacks(root, &mut |stack| {
        if stack.get_str("id").map(ensure_namespaced).as_deref() != Some("minecraft:map") {
            return;
        }
        let Some(components) = compound_mut(stack, "components") else {
            return;
        };
        if let Some(Tag::Compound(name)) = components.get_mut("minecraft:item_name")
            && name.get_str("translate") == Some("filled_map.buried_treasure")
        {
            name.insert("translate", "item.minecraft.buried_treasure_map");
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn old_stack(id: &str) -> Compound {
        Compound::new().with("id", id).with("Count", 1i8)
    }

    fn component<'a>(stack: &'a Compound, name: &str) -> &'a Tag {
        stack
            .get_compound("components")
            .and_then(|components| components.get(name))
            .unwrap_or_else(|| panic!("missing component {name} in {stack:?}"))
    }

    fn componentized(mut stack: Compound) -> Compound {
        componentize_3818(&mut stack);
        stack
    }

    #[test]
    fn count_and_id_become_count_and_components() {
        let stack = componentized(old_stack("stone"));
        assert_eq!(stack.get_str("id"), Some("minecraft:stone"));
        assert_eq!(stack.get("count"), Some(&Tag::Int(1)));
        assert!(!stack.contains_key("Count") && !stack.contains_key("components"));
    }

    #[test]
    fn unknown_tag_data_becomes_custom_data() {
        let stack = componentized(
            old_stack("minecraft:stick").with("tag", Compound::new().with("Plugin", 5)),
        );
        assert_eq!(
            component(&stack, "minecraft:custom_data")
                .as_compound()
                .unwrap()
                .get_i32("Plugin"),
            Some(5)
        );
        assert!(!stack.contains_key("tag"));
    }

    #[test]
    fn damage_repair_cost_and_model_data_move_to_components() {
        let stack = componentized(
            old_stack("minecraft:iron_sword").with(
                "tag",
                Compound::new()
                    .with("Damage", 7)
                    .with("RepairCost", 3)
                    .with("CustomModelData", 12),
            ),
        );
        assert_eq!(component(&stack, "minecraft:damage"), &Tag::Int(7));
        assert_eq!(component(&stack, "minecraft:repair_cost"), &Tag::Int(3));
        assert_eq!(
            component(&stack, "minecraft:custom_model_data"),
            &Tag::Int(12)
        );
        let fresh = componentized(
            old_stack("minecraft:iron_sword").with("tag", Compound::new().with("Damage", 0)),
        );
        assert!(!fresh.contains_key("components"));
    }

    #[test]
    fn display_name_lore_and_color_become_components() {
        let stack = componentized(
            old_stack("minecraft:leather_boots").with(
                "tag",
                Compound::new().with(
                    "display",
                    Compound::new()
                        .with("Name", "{\"text\":\"Boots\"}")
                        .with("Lore", Tag::List(vec![Tag::String("\"a\"".into())]))
                        .with("color", 255),
                ),
            ),
        );
        assert_eq!(
            component(&stack, "minecraft:custom_name"),
            &Tag::String("{\"text\":\"Boots\"}".into())
        );
        assert_eq!(
            component(&stack, "minecraft:lore").as_list().unwrap().len(),
            1
        );
        assert_eq!(
            component(&stack, "minecraft:dyed_color")
                .as_compound()
                .unwrap()
                .get_i32("rgb"),
            Some(255)
        );
    }

    #[test]
    fn enchantments_become_a_levels_compound() {
        let stack = componentized(old_stack("minecraft:diamond_sword").with(
            "tag",
            Compound::new().with(
                "Enchantments",
                Tag::List(vec![
                        Tag::Compound(
                            Compound::new()
                                .with("id", "minecraft:sharpness")
                                .with("lvl", 5i16),
                        ),
                        Tag::Compound(
                            Compound::new()
                                .with("id", "minecraft:unbreaking")
                                .with("lvl", 0i16),
                        ),
                    ]),
            ),
        ));
        let levels = component(&stack, "minecraft:enchantments")
            .as_compound()
            .unwrap()
            .get_compound("levels")
            .unwrap();
        assert_eq!(levels.get_i32("minecraft:sharpness"), Some(5));
        assert!(!levels.contains_key("minecraft:unbreaking"));
    }

    #[test]
    fn hide_flags_turn_into_show_in_tooltip_false() {
        let stack = componentized(old_stack("minecraft:diamond_sword").with(
            "tag",
            Compound::new().with("HideFlags", 1).with(
                "Enchantments",
                Tag::List(vec![Tag::Compound(
                    Compound::new().with("id", "minecraft:sharpness").with("lvl", 1i16),
                )]),
            ),
        ));
        assert_eq!(
            component(&stack, "minecraft:enchantments")
                .as_compound()
                .unwrap()
                .get_bool("show_in_tooltip"),
            Some(false)
        );
    }

    #[test]
    fn attribute_modifiers_are_converted() {
        let stack = componentized(old_stack("minecraft:stick").with(
            "tag",
            Compound::new().with(
                "AttributeModifiers",
                Tag::List(vec![Tag::Compound(
                    Compound::new()
                        .with("AttributeName", "minecraft:generic.attack_damage")
                        .with("Name", "boost")
                        .with("Amount", 4.0)
                        .with("Operation", 2)
                        .with("Slot", "mainhand")
                        .with("UUID", Tag::IntArray(vec![1, 2, 3, 4])),
                )]),
            ),
        ));
        let modifier = component(&stack, "minecraft:attribute_modifiers")
            .as_compound()
            .unwrap()
            .get_list("modifiers")
            .unwrap()[0]
            .as_compound()
            .unwrap();
        assert_eq!(
            modifier.get_str("type"),
            Some("minecraft:generic.attack_damage")
        );
        assert_eq!(modifier.get_str("operation"), Some("add_multiplied_total"));
        assert_eq!(modifier.get_str("slot"), Some("mainhand"));
        assert_eq!(modifier.get_f64("amount"), Some(4.0));
    }

    #[test]
    fn potions_books_and_stew_use_their_components() {
        let potion = componentized(
            old_stack("minecraft:potion").with(
                "tag",
                Compound::new()
                    .with("Potion", "minecraft:healing")
                    .with("CustomPotionColor", 9),
            ),
        );
        let contents = component(&potion, "minecraft:potion_contents")
            .as_compound()
            .unwrap();
        assert_eq!(contents.get_str("potion"), Some("minecraft:healing"));
        assert_eq!(contents.get_i32("custom_color"), Some(9));

        let book = componentized(
            old_stack("minecraft:written_book").with(
                "tag",
                Compound::new()
                    .with("title", "Tome")
                    .with("author", "Me")
                    .with("generation", 1)
                    .with(
                        "pages",
                        Tag::List(vec![Tag::String("{\"text\":\"p\"}".into())]),
                    ),
            ),
        );
        let content = component(&book, "minecraft:written_book_content")
            .as_compound()
            .unwrap();
        assert_eq!(
            content.get_compound("title").unwrap().get_str("raw"),
            Some("Tome")
        );
        assert_eq!(content.get_str("author"), Some("Me"));
        assert_eq!(content.get_list("pages").unwrap().len(), 1);

        let stew = componentized(old_stack("minecraft:suspicious_stew").with(
            "tag",
            Compound::new().with("effects", Tag::List(Vec::new())),
        ));
        assert!(
            stew.get_compound("components")
                .unwrap()
                .contains_key("minecraft:suspicious_stew_effects")
        );
    }

    #[test]
    fn container_block_entity_tags_become_container_components_with_converted_items() {
        let inner = old_stack("minecraft:diamond")
            .with("Slot", 3i8)
            .with("tag", Compound::new().with("Damage", 2));
        let stack = componentized(
            old_stack("minecraft:shulker_box").with(
                "tag",
                Compound::new().with(
                    "BlockEntityTag",
                    Compound::new()
                        .with("id", "minecraft:shulker_box")
                        .with("Items", Tag::List(vec![Tag::Compound(inner)])),
                ),
            ),
        );
        let slot = component(&stack, "minecraft:container").as_list().unwrap()[0]
            .as_compound()
            .unwrap();
        assert_eq!(slot.get_i32("slot"), Some(3));
        let item = slot.get_compound("item").unwrap();
        assert_eq!(item.get_str("id"), Some("minecraft:diamond"));
        assert_eq!(item.get("count"), Some(&Tag::Int(1)));
        assert_eq!(
            item.get_compound("components")
                .unwrap()
                .get("minecraft:damage"),
            Some(&Tag::Int(2))
        );
        assert!(
            stack
                .get_compound("components")
                .unwrap()
                .get("minecraft:block_entity_data")
                .is_none()
        );
    }

    #[test]
    fn banners_decorated_pots_and_beehives_move_their_data() {
        let banner = componentized(
            old_stack("minecraft:white_banner").with(
                "tag",
                Compound::new().with(
                    "BlockEntityTag",
                    Compound::new()
                        .with("id", "minecraft:banner")
                        .with("Base", 14)
                        .with("patterns", Tag::List(Vec::new())),
                ),
            ),
        );
        assert_eq!(
            component(&banner, "minecraft:base_color"),
            &Tag::String("red".into())
        );
        assert!(
            banner
                .get_compound("components")
                .unwrap()
                .contains_key("minecraft:banner_patterns")
        );
    }

    #[test]
    fn player_heads_get_a_profile() {
        let stack = componentized(
            old_stack("minecraft:player_head").with(
                "tag",
                Compound::new().with(
                    "SkullOwner",
                    Compound::new()
                        .with("Name", "Notch")
                        .with("Id", Tag::IntArray(vec![1, 2, 3, 4])),
                ),
            ),
        );
        let profile = component(&stack, "minecraft:profile")
            .as_compound()
            .unwrap();
        assert_eq!(profile.get_str("name"), Some("Notch"));
        assert_eq!(profile.get_int_array("id"), Some(&[1, 2, 3, 4][..]));
    }

    #[test]
    fn firework_stars_and_rockets_are_converted() {
        let star = componentized(
            old_stack("minecraft:firework_star").with(
                "tag",
                Compound::new().with(
                    "Explosion",
                    Compound::new()
                        .with("Type", 2)
                        .with("Colors", Tag::IntArray(vec![5]))
                        .with("Trail", 1i8),
                ),
            ),
        );
        let explosion = component(&star, "minecraft:firework_explosion")
            .as_compound()
            .unwrap();
        assert_eq!(explosion.get_str("shape"), Some("star"));
        assert_eq!(explosion.get_int_array("colors"), Some(&[5][..]));
        assert_eq!(explosion.get_i32("has_trail"), Some(1));
        let rocket = componentized(old_stack("minecraft:firework_rocket").with(
            "tag",
            Compound::new().with("Fireworks", Compound::new().with("Flight", 2i8)),
        ));
        assert_eq!(
            component(&rocket, "minecraft:fireworks")
                .as_compound()
                .unwrap()
                .get("flight_duration"),
            Some(&Tag::Byte(2))
        );
    }

    #[test]
    fn adventure_predicates_are_parsed() {
        let stack = componentized(old_stack("minecraft:stick").with(
            "tag",
            Compound::new().with(
                "CanDestroy",
                Tag::List(vec![Tag::String(
                    "minecraft:oak_stairs[facing=north, half=top]{x:1}".into(),
                )]),
            ),
        ));
        let predicate = component(&stack, "minecraft:can_break")
            .as_compound()
            .unwrap()
            .get_list("predicates")
            .unwrap()[0]
            .as_compound()
            .unwrap();
        assert_eq!(predicate.get_str("blocks"), Some("minecraft:oak_stairs"));
        assert_eq!(
            predicate.get_compound("state").unwrap().get_str("half"),
            Some("top")
        );
        assert_eq!(predicate.get_str("nbt"), Some("{x:1}"));
    }

    #[test]
    fn crossbows_maps_and_buckets() {
        let map = componentized(
            old_stack("minecraft:filled_map").with("tag", Compound::new().with("map", 4)),
        );
        assert_eq!(component(&map, "minecraft:map_id"), &Tag::Int(4));
        let bucket = componentized(
            old_stack("minecraft:axolotl_bucket")
                .with("tag", Compound::new().with("Variant", 2).with("Age", 5)),
        );
        let data = component(&bucket, "minecraft:bucket_entity_data")
            .as_compound()
            .unwrap();
        assert_eq!(data.get_i32("Variant"), Some(2));
        let compass = componentized(
            old_stack("minecraft:compass").with(
                "tag",
                Compound::new()
                    .with("LodestonePos", Tag::IntArray(vec![1, 2, 3]))
                    .with("LodestoneDimension", "minecraft:overworld")
                    .with("LodestoneTracked", false),
            ),
        );
        let tracker = component(&compass, "minecraft:lodestone_tracker")
            .as_compound()
            .unwrap();
        assert_eq!(tracker.get_bool("tracked"), Some(false));
        assert_eq!(
            tracker.get_compound("target").unwrap().get_str("dimension"),
            Some("minecraft:overworld")
        );
    }

    #[test]
    fn already_componentized_stacks_are_left_alone() {
        let stack = Compound::new()
            .with("id", "minecraft:stone")
            .with("count", 3);
        assert_eq!(componentized(stack.clone()), stack);
    }

    #[test]
    fn food_becomes_food_plus_consumable() {
        let mut stack = Compound::new()
            .with("id", "minecraft:apple")
            .with("count", 1)
            .with(
                "components",
                Compound::new().with(
                    "minecraft:food",
                    Compound::new()
                        .with("nutrition", 4)
                        .with("eat_seconds", 2.0f32)
                        .with(
                            "effects",
                            Tag::List(vec![Tag::Compound(
                                Compound::new()
                                    .with("effect", Compound::new().with("id", "minecraft:hunger"))
                                    .with("probability", 0.5f32),
                            )]),
                        ),
                ),
            );
        food_to_consumable_4059(&mut stack);
        let components = stack.get_compound("components").unwrap();
        let consumable = components.get_compound("minecraft:consumable").unwrap();
        assert_eq!(consumable.get_f64("consume_seconds"), Some(2.0));
        let effect = consumable.get_list("on_consume_effects").unwrap()[0]
            .as_compound()
            .unwrap();
        assert_eq!(effect.get_str("type"), Some("minecraft:apply_effects"));
        assert_eq!(effect.get_f64("probability"), Some(0.5));
        let food = components.get_compound("minecraft:food").unwrap();
        assert!(!food.contains_key("effects") && !food.contains_key("eat_seconds"));
    }

    #[test]
    fn component_renames() {
        let mut stack = Compound::new()
            .with("id", "minecraft:netherite_sword")
            .with("count", 1)
            .with(
                "components",
                Compound::new()
                    .with("minecraft:fire_resistant", Compound::new())
                    .with("minecraft:custom_model_data", 7)
                    .with("minecraft:lock", "key")
                    .with("minecraft:map_color", 3)
                    .with("minecraft:block_transformer", Compound::new())
                    .with(
                        "minecraft:equippable",
                        Compound::new().with("model", "minecraft:x"),
                    )
                    .with(
                        "minecraft:swing_animation",
                        Compound::new().with("type", "stab"),
                    ),
            );
        fire_resistant_to_damage_resistant_4064(&mut stack);
        custom_model_data_expand_4175(&mut stack);
        lock_to_predicate_4068(&mut stack);
        equippable_asset_rename_4175(&mut stack);
        swing_animation_split_5007(&mut stack);
        map_color_removal_5008(&mut stack);
        remove_block_transformer_5014(&mut stack);
        let components = stack.get_compound("components").unwrap();
        assert_eq!(
            components
                .get_compound("minecraft:damage_resistant")
                .unwrap()
                .get_str("types"),
            Some("#minecraft:is_fire")
        );
        assert_eq!(
            components
                .get_compound("minecraft:custom_model_data")
                .unwrap()
                .get_list("floats"),
            Some(&[Tag::Float(7.0)][..])
        );
        assert_eq!(
            components
                .get_compound("minecraft:lock")
                .unwrap()
                .get_compound("components")
                .unwrap()
                .get_str("minecraft:custom_name"),
            Some("\"key\"")
        );
        assert!(
            components
                .get_compound("minecraft:equippable")
                .unwrap()
                .contains_key("asset_id")
        );
        assert!(components.contains_key("minecraft:attack_animation"));
        assert!(components.contains_key("minecraft:interact_animation"));
        assert!(!components.contains_key("minecraft:map_color"));
        assert!(!components.contains_key("minecraft:block_transformer"));
        assert!(!components.contains_key("minecraft:fire_resistant"));
    }

    #[test]
    fn tooltip_flags_become_a_tooltip_display_and_components_are_unwrapped() {
        let mut stack = Compound::new()
            .with("id", "minecraft:diamond_sword")
            .with("count", 1)
            .with(
                "components",
                Compound::new()
                    .with(
                        "minecraft:enchantments",
                        Compound::new()
                            .with("levels", Compound::new().with("minecraft:sharpness", 5))
                            .with("show_in_tooltip", false),
                    )
                    .with("minecraft:dyed_color", Compound::new().with("rgb", 255))
                    .with("minecraft:hide_additional_tooltip", Compound::new())
                    .with("minecraft:potion_contents", Compound::new())
                    .with(
                        "minecraft:can_break",
                        Compound::new()
                            .with(
                                "predicates",
                                Tag::List(vec![Tag::Compound(
                                    Compound::new().with("blocks", "stone"),
                                )]),
                            )
                            .with("show_in_tooltip", false),
                    ),
            );
        tooltip_display_4307(&mut stack);
        let components = stack.get_compound("components").unwrap();
        assert_eq!(
            components
                .get_compound("minecraft:enchantments")
                .unwrap()
                .get_i32("minecraft:sharpness"),
            Some(5)
        );
        assert_eq!(components.get("minecraft:dyed_color"), Some(&Tag::Int(255)));
        assert!(components.get_list("minecraft:can_break").is_some());
        assert!(!components.contains_key("minecraft:hide_additional_tooltip"));
        let display = components
            .get_compound("minecraft:tooltip_display")
            .unwrap();
        assert_eq!(display.get_bool("hide_tooltip"), Some(false));
        let hidden: Vec<_> = display
            .get_list("hidden_components")
            .unwrap()
            .iter()
            .filter_map(Tag::as_str)
            .collect();
        assert!(hidden.contains(&"minecraft:enchantments"));
        assert!(hidden.contains(&"minecraft:can_break"));
        assert!(hidden.contains(&"minecraft:potion_contents"));
        assert!(!hidden.contains(&"minecraft:dyed_color"));
    }

    #[test]
    fn broken_locks_are_dropped() {
        let mut stack = Compound::new()
            .with("id", "minecraft:chest")
            .with("count", 1)
            .with(
                "components",
                Compound::new().with(
                    "minecraft:lock",
                    Compound::new().with(
                        "components",
                        Compound::new().with("minecraft:custom_name", "\"\""),
                    ),
                ),
            );
        invalid_lock_4176(&mut stack);
        assert!(
            !stack
                .get_compound("components")
                .unwrap()
                .contains_key("minecraft:lock")
        );
    }

    #[test]
    fn ominous_banners_and_explorer_maps_use_item_name() {
        let mut banner = Compound::new()
            .with("id", "minecraft:white_banner")
            .with("count", 1)
            .with(
                "components",
                Compound::new().with(
                    "minecraft:custom_name",
                    "{\"translate\":\"block.minecraft.ominous_banner\"}",
                ),
            );
        ominous_banner_name_3825(&mut banner);
        assert!(
            banner
                .get_compound("components")
                .unwrap()
                .contains_key("minecraft:item_name")
        );
        let mut named = Compound::new()
            .with("id", "minecraft:white_banner")
            .with("count", 1)
            .with(
                "components",
                Compound::new().with("minecraft:custom_name", "{\"text\":\"mine\"}"),
            );
        ominous_banner_name_3825(&mut named);
        assert!(
            named
                .get_compound("components")
                .unwrap()
                .contains_key("minecraft:custom_name")
        );
    }

    #[test]
    fn skull_block_entities_get_a_profile() {
        let mut chunk = Compound::new().with(
            "block_entities",
            Tag::List(vec![Tag::Compound(
                Compound::new()
                    .with("id", "minecraft:skull")
                    .with("SkullOwner", "Steve"),
            )]),
        );
        skull_profile_3820(&mut chunk);
        let skull = chunk.get_list("block_entities").unwrap()[0]
            .as_compound()
            .unwrap();
        assert_eq!(
            skull.get_compound("profile").unwrap().get_str("name"),
            Some("Steve")
        );
        assert!(!skull.contains_key("SkullOwner"));
    }

    #[test]
    fn unfilled_buried_treasure_maps_get_the_new_translation_key() {
        let mut stack = Compound::new()
            .with("id", "minecraft:map")
            .with("count", 1)
            .with(
                "components",
                Compound::new().with(
                    "minecraft:item_name",
                    Compound::new().with("translate", "filled_map.buried_treasure"),
                ),
            );
        unfilled_buried_treasure_map_5016(&mut stack);
        assert_eq!(
            stack
                .get_compound("components")
                .unwrap()
                .get_compound("minecraft:item_name")
                .unwrap()
                .get_str("translate"),
            Some("item.minecraft.buried_treasure_map")
        );
    }
}

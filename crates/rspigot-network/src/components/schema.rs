use rspigot_nbt::{Compound, Tag};
use rspigot_protocol::play::wire::{read_var, write_var};

use super::{Custom, Def, Node, Reg, def, opt, read_string, req, take, write_string};

const SOUND_EVENT: Reg = Reg::Static("minecraft:sound_event");
const MOB_EFFECT: Reg = Reg::Static("minecraft:mob_effect");
const DAMAGE_TYPE: Reg = Reg::Synced("minecraft:damage_type");
const BLOCK: Reg = Reg::Static("minecraft:block");
const ITEM: Reg = Reg::Static("minecraft:item");
const ENCHANTMENT: Reg = Reg::Synced("minecraft:enchantment");
const COMPONENT_TYPE: Reg = Reg::Static("minecraft:data_component_type");

const SOUND: Node = Node::Holder(
    SOUND_EVENT,
    &Node::Rec(&[req("sound_id", Node::Ident), opt("range", Node::Float)]),
);

const RARITY: Node = Node::Enum(&["common", "uncommon", "rare", "epic"]);

const USE_ANIMATIONS: &[&str] = &[
    "none",
    "eat",
    "drink",
    "block",
    "bow",
    "trident",
    "crossbow",
    "spyglass",
    "toot_horn",
    "brush",
    "bundle",
    "spear",
];

const MOB_EFFECT_INSTANCE: Node = Node::Custom(Custom {
    enc: encode_effect,
    dec: decode_effect,
});

const CONSUME_EFFECT: Node = Node::Custom(Custom {
    enc: encode_consume_effect,
    dec: decode_consume_effect,
});

const CONSUME_APPLY: Node = Node::Rec(&[
    req("effects", Node::List(&MOB_EFFECT_INSTANCE, 1 << 20)),
    def("probability", Node::Float, Def::Float(1.0)),
]);
const CONSUME_REMOVE: Node = Node::Rec(&[req("effects", Node::Set(MOB_EFFECT))]);
const CONSUME_TELEPORT: Node = Node::Rec(&[
    def("diameter", Node::Float, Def::Float(16.0)),
    def("directional_particles", Node::Bool, Def::Bool(true)),
]);
const CONSUME_SOUND: Node = Node::Rec(&[req("sound", SOUND)]);
const CONSUME_CLEAR: Node = Node::Unit;

const CONSUME_VARIANTS: [(&str, Node); 5] = [
    ("minecraft:apply_effects", CONSUME_APPLY),
    ("minecraft:remove_effects", CONSUME_REMOVE),
    ("minecraft:clear_all_effects", CONSUME_CLEAR),
    ("minecraft:teleport_randomly", CONSUME_TELEPORT),
    ("minecraft:play_sound", CONSUME_SOUND),
];

fn effect_details(tag: &Tag, out: &mut Vec<u8>) -> Option<()> {
    let details = tag.as_compound()?;
    write_var(details.get_i32("amplifier").unwrap_or(0), out);
    write_var(details.get_i32("duration").unwrap_or(600), out);
    let particles = details.get_bool("show_particles").unwrap_or(true);
    out.push(u8::from(details.get_bool("ambient").unwrap_or(false)));
    out.push(u8::from(particles));
    out.push(u8::from(details.get_bool("show_icon").unwrap_or(particles)));
    match details.get("hidden_effect") {
        Some(hidden) => {
            out.push(1);
            effect_details(hidden, out)
        }
        None => {
            out.push(0);
            Some(())
        }
    }
}

fn read_flag(input: &mut &[u8]) -> Option<bool> {
    match take(input, 1)?[0] {
        0 => Some(false),
        1 => Some(true),
        _ => None,
    }
}

fn read_details(input: &mut &[u8], depth: u8) -> Option<Compound> {
    let depth = depth.checked_sub(1)?;
    let mut details = Compound::new();
    details.insert("amplifier", read_var(input).ok()?);
    details.insert("duration", read_var(input).ok()?);
    details.insert("ambient", read_flag(input)?);
    details.insert("show_particles", read_flag(input)?);
    details.insert("show_icon", read_flag(input)?);
    if read_flag(input)? {
        details.insert("hidden_effect", read_details(input, depth)?);
    }
    Some(details)
}

fn encode_effect(tag: &Tag, out: &mut Vec<u8>) -> Option<()> {
    let effect = tag.as_compound()?;
    MOB_EFFECT_ID.encode(effect.get("id")?, out)?;
    effect_details(tag, out)
}

const MOB_EFFECT_ID: Node = Node::Reg(MOB_EFFECT);

fn decode_effect(input: &mut &[u8]) -> Option<Tag> {
    let id = MOB_EFFECT_ID.decode(input)?;
    let mut effect = read_details(input, 8)?;
    effect.insert("id", id);
    Some(Tag::Compound(effect))
}

fn encode_consume_effect(tag: &Tag, out: &mut Vec<u8>) -> Option<()> {
    let name = tag.as_compound()?.get_str("type")?;
    let name = super::qualified(name);
    let index = CONSUME_VARIANTS
        .iter()
        .position(|(candidate, _)| *candidate == name)?;
    write_var(i32::try_from(index).ok()?, out);
    CONSUME_VARIANTS[index].1.encode(tag, out)
}

fn decode_consume_effect(input: &mut &[u8]) -> Option<Tag> {
    let index = usize::try_from(read_var(input).ok()?).ok()?;
    let (name, node) = CONSUME_VARIANTS.get(index)?;
    let mut effect = match node.decode(input)? {
        Tag::Compound(compound) => compound,
        _ => return None,
    };
    effect.insert("type", *name);
    Some(Tag::Compound(effect))
}

const ENCHANTMENTS: Node = Node::Custom(Custom {
    enc: encode_enchantments,
    dec: decode_enchantments,
});

fn encode_enchantments(tag: &Tag, out: &mut Vec<u8>) -> Option<()> {
    let enchantments = tag.as_compound()?;
    let levels = enchantments.get_compound("levels").unwrap_or(enchantments);
    write_var(i32::try_from(levels.len()).ok()?, out);
    for (name, level) in levels.iter() {
        write_var(ENCHANTMENT.index(name)?, out);
        write_var(i32::try_from(level.as_i64()?).ok()?, out);
    }
    Some(())
}

fn decode_enchantments(input: &mut &[u8]) -> Option<Tag> {
    let count = usize::try_from(read_var(input).ok()?).ok()?;
    let mut levels = Compound::new();
    for _ in 0..count {
        let name = ENCHANTMENT.name(read_var(input).ok()?)?;
        levels.insert(name, read_var(input).ok()?);
    }
    Some(Tag::Compound(levels))
}

const CUSTOM_MODEL_DATA: Node = Node::Rec(&[
    def("floats", Node::List(&Node::Float, 1 << 20), Def::Empty),
    def("flags", Node::List(&Node::Bool, 1 << 20), Def::Empty),
    def("strings", Node::List(&Node::Str, 1 << 20), Def::Empty),
    def("colors", Node::List(&Node::Int, 1 << 20), Def::Empty),
]);

const USE_EFFECTS: Node = Node::Rec(&[
    def("can_sprint", Node::Bool, Def::Bool(false)),
    def("interact_vibrations", Node::Bool, Def::Bool(true)),
    def("speed_multiplier", Node::Float, Def::Float(0.2)),
]);

const TOOLTIP_DISPLAY: Node = Node::Rec(&[
    def("hide_tooltip", Node::Bool, Def::Bool(false)),
    def(
        "hidden_components",
        Node::List(&Node::Reg(COMPONENT_TYPE), 1 << 20),
        Def::Empty,
    ),
]);

const FOOD: Node = Node::Rec(&[
    req("nutrition", Node::Var),
    req("saturation", Node::Float),
    def("can_always_eat", Node::Bool, Def::Bool(false)),
]);

const CONSUMABLE: Node = Node::Rec(&[
    def("consume_seconds", Node::Float, Def::Float(1.6)),
    def("animation", Node::Enum(USE_ANIMATIONS), Def::Str("eat")),
    def("sound", SOUND, Def::Str("minecraft:entity.generic.eat")),
    def("has_consume_particles", Node::Bool, Def::Bool(true)),
    def(
        "on_consume_effects",
        Node::List(&CONSUME_EFFECT, 1 << 20),
        Def::Empty,
    ),
]);

const USE_COOLDOWN: Node = Node::Rec(&[
    req("seconds", Node::Float),
    opt("cooldown_group", Node::Ident),
]);

const DAMAGE_RESISTANT: Node = Node::Rec(&[req("types", Node::Set(DAMAGE_TYPE))]);

const TOOL_RULE: Node = Node::Rec(&[
    req("blocks", Node::Set(BLOCK)),
    opt("speed", Node::Float),
    opt("correct_for_drops", Node::Bool),
]);

const TOOL: Node = Node::Rec(&[
    req("rules", Node::List(&TOOL_RULE, 1 << 20)),
    def("default_mining_speed", Node::Float, Def::Float(1.0)),
    def("damage_per_block", Node::Var, Def::Int(1)),
    def(
        "can_destroy_blocks_in_creative",
        Node::Bool,
        Def::Bool(true),
    ),
]);

const WEAPON: Node = Node::Rec(&[
    def("item_damage_per_attack", Node::Var, Def::Int(1)),
    def("disable_blocking_for_seconds", Node::Float, Def::Float(0.0)),
]);

const ENCHANTABLE: Node = Node::Rec(&[req("value", Node::Var)]);

const REPAIRABLE: Node = Node::Rec(&[req("items", Node::Set(ITEM))]);

const EQUIPMENT_SLOTS: &[&str] = &[
    "mainhand", "feet", "legs", "chest", "head", "offhand", "body", "saddle",
];

const SLOT_GROUPS: &[&str] = &[
    "any", "mainhand", "offhand", "hand", "feet", "legs", "chest", "head", "armor", "body",
    "saddle",
];

const SWING: Node = Node::Rec(&[
    def(
        "type",
        Node::Enum(&["none", "whack", "stab"]),
        Def::Str("whack"),
    ),
    def("duration", Node::Var, Def::Int(6)),
]);

const ATTRIBUTE_DISPLAY: Node = Node::Custom(Custom {
    enc: encode_attribute_display,
    dec: decode_attribute_display,
});

const DISPLAY_TYPES: &[&str] = &["default", "hidden", "override"];

fn encode_attribute_display(tag: &Tag, out: &mut Vec<u8>) -> Option<()> {
    let display = tag.as_compound()?;
    let name = display.get_str("type")?;
    let index = DISPLAY_TYPES
        .iter()
        .position(|candidate| *candidate == name)?;
    write_var(i32::try_from(index).ok()?, out);
    if name == "override" {
        Node::Text.encode(display.get("value")?, out)?;
    }
    Some(())
}

fn decode_attribute_display(input: &mut &[u8]) -> Option<Tag> {
    let index = usize::try_from(read_var(input).ok()?).ok()?;
    let name = *DISPLAY_TYPES.get(index)?;
    let mut display = Compound::new().with("type", name);
    if name == "override" {
        display.insert("value", Node::Text.decode(input)?);
    }
    Some(Tag::Compound(display))
}

fn default_display() -> Tag {
    Tag::Compound(Compound::new().with("type", "default"))
}

const ATTRIBUTE_ENTRY: Node = Node::Rec(&[
    req("type", Node::Reg(Reg::Static("minecraft:attribute"))),
    req("id", Node::Ident),
    req("amount", Node::Double),
    req(
        "operation",
        Node::Enum(&["add_value", "add_multiplied_base", "add_multiplied_total"]),
    ),
    def("slot", Node::Enum(SLOT_GROUPS), Def::Str("any")),
    def("display", ATTRIBUTE_DISPLAY, Def::With(default_display)),
]);

const EQUIPPABLE: Node = Node::Rec(&[
    req("slot", Node::Enum(EQUIPMENT_SLOTS)),
    def(
        "equip_sound",
        SOUND,
        Def::Str("minecraft:item.armor.equip_generic"),
    ),
    opt("asset_id", Node::Ident),
    opt("camera_overlay", Node::Ident),
    opt(
        "allowed_entities",
        Node::Set(Reg::Static("minecraft:entity_type")),
    ),
    def("dispensable", Node::Bool, Def::Bool(true)),
    def("swappable", Node::Bool, Def::Bool(true)),
    def("damage_on_hurt", Node::Bool, Def::Bool(true)),
    def("equip_on_interact", Node::Bool, Def::Bool(false)),
    def("can_be_sheared", Node::Bool, Def::Bool(false)),
    def(
        "shearing_sound",
        SOUND,
        Def::Str("minecraft:item.shears.snip"),
    ),
]);

const ATTACK_RANGE: Node = Node::Rec(&[
    def("min_reach", Node::Float, Def::Float(0.0)),
    def("max_reach", Node::Float, Def::Float(3.0)),
    def("min_creative_reach", Node::Float, Def::Float(0.0)),
    def("max_creative_reach", Node::Float, Def::Float(5.0)),
    def("hitbox_margin", Node::Float, Def::Float(0.3)),
    def("mob_factor", Node::Float, Def::Float(1.0)),
]);

const DEATH_PROTECTION: Node = Node::Rec(&[def(
    "death_effects",
    Node::List(&CONSUME_EFFECT, 1 << 20),
    Def::Empty,
)]);

fn default_reductions() -> Tag {
    Tag::List(vec![Tag::Compound(
        Compound::new()
            .with("horizontal_blocking_angle", 90.0f32)
            .with("base", 0.0f32)
            .with("factor", 1.0f32),
    )])
}

fn default_item_damage() -> Tag {
    Tag::Compound(
        Compound::new()
            .with("threshold", 1.0f32)
            .with("base", 0.0f32)
            .with("factor", 1.0f32),
    )
}

const DAMAGE_REDUCTION: Node = Node::Rec(&[
    def("horizontal_blocking_angle", Node::Float, Def::Float(90.0)),
    opt("type", Node::Set(DAMAGE_TYPE)),
    req("base", Node::Float),
    req("factor", Node::Float),
]);

const ITEM_DAMAGE_FUNCTION: Node = Node::Rec(&[
    req("threshold", Node::Float),
    req("base", Node::Float),
    req("factor", Node::Float),
]);

const BLOCKS_ATTACKS: Node = Node::Rec(&[
    def("block_delay_seconds", Node::Float, Def::Float(0.0)),
    def("disable_cooldown_scale", Node::Float, Def::Float(1.0)),
    def(
        "damage_reductions",
        Node::List(&DAMAGE_REDUCTION, 1 << 20),
        Def::With(default_reductions),
    ),
    def(
        "item_damage",
        ITEM_DAMAGE_FUNCTION,
        Def::With(default_item_damage),
    ),
    opt("bypassed_by", Node::Set(DAMAGE_TYPE)),
    opt("block_sound", SOUND),
    opt("disabled_sound", SOUND),
]);

const PIERCING_WEAPON: Node = Node::Rec(&[
    def("deals_knockback", Node::Bool, Def::Bool(true)),
    def("dismounts", Node::Bool, Def::Bool(false)),
    opt("sound", SOUND),
    opt("hit_sound", SOUND),
]);

const KINETIC_CONDITION: Node = Node::Rec(&[
    req("max_duration_ticks", Node::Var),
    def("min_speed", Node::Float, Def::Float(0.0)),
    def("min_relative_speed", Node::Float, Def::Float(0.0)),
]);

const KINETIC_WEAPON: Node = Node::Rec(&[
    def("contact_cooldown_ticks", Node::Var, Def::Int(10)),
    def("delay_ticks", Node::Var, Def::Int(0)),
    opt("dismount_conditions", KINETIC_CONDITION),
    opt("knockback_conditions", KINETIC_CONDITION),
    opt("damage_conditions", KINETIC_CONDITION),
    def("forward_movement", Node::Float, Def::Float(0.0)),
    def("damage_multiplier", Node::Float, Def::Float(1.0)),
    opt("sound", SOUND),
    opt("hit_sound", SOUND),
]);

const DYE_COLORS: Node = Node::Enum(&[
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
]);

const FIREWORK_SHAPE: Node = Node::Enum(&["small_ball", "large_ball", "star", "creeper", "burst"]);

const FIREWORK_EXPLOSION: Node = Node::Rec(&[
    req("shape", FIREWORK_SHAPE),
    def("colors", Node::List(&Node::Int, 1 << 20), Def::Empty),
    def("fade_colors", Node::List(&Node::Int, 1 << 20), Def::Empty),
    def("has_trail", Node::Bool, Def::Bool(false)),
    def("has_twinkle", Node::Bool, Def::Bool(false)),
]);

const FIREWORKS: Node = Node::Rec(&[
    def("flight_duration", Node::Var, Def::Int(0)),
    def(
        "explosions",
        Node::List(&FIREWORK_EXPLOSION, 256),
        Def::Empty,
    ),
]);

const POTION_FULL: Node = Node::Rec(&[
    opt("potion", Node::Reg(Reg::Static("minecraft:potion"))),
    opt("custom_color", Node::Int),
    def(
        "custom_effects",
        Node::List(&MOB_EFFECT_INSTANCE, 1 << 20),
        Def::Empty,
    ),
    opt("custom_name", Node::Str),
]);

const POTION_CONTENTS: Node = Node::Wrap("potion", &POTION_FULL);

const STEW_EFFECTS: Node = Node::List(
    &Node::Rec(&[
        req("id", Node::Reg(MOB_EFFECT)),
        def("duration", Node::Var, Def::Int(160)),
    ]),
    1 << 20,
);

const TRIM_MATERIAL: Node = Node::Holder(
    Reg::Synced("minecraft:trim_material"),
    &Node::Rec(&[
        req("palette_id", Node::Ident),
        req("description", Node::Text),
    ]),
);

const TRIM_PATTERN: Node = Node::Holder(
    Reg::Synced("minecraft:trim_pattern"),
    &Node::Rec(&[
        req("asset_id", Node::Ident),
        req("description", Node::Text),
        def("decal", Node::Bool, Def::Bool(false)),
    ]),
);

const TRIM: Node = Node::Rec(&[req("material", TRIM_MATERIAL), req("pattern", TRIM_PATTERN)]);

const BANNER_LAYERS: Node = Node::List(
    &Node::Rec(&[
        req(
            "pattern",
            Node::Holder(
                Reg::Synced("minecraft:banner_pattern"),
                &Node::Rec(&[
                    req("asset_id", Node::Ident),
                    req("translation_key", Node::Str),
                ]),
            ),
        ),
        req("color", DYE_COLORS),
    ]),
    1 << 20,
);

const POT_DECORATIONS: Node = Node::Rec(&[
    opt("back", Node::Stack),
    opt("left", Node::Stack),
    opt("right", Node::Stack),
    opt("front", Node::Stack),
]);

const STACK_LIST: Node = Node::List(&Node::Stack, 1 << 20);

const CONTAINER: Node = Node::Custom(Custom {
    enc: encode_container,
    dec: decode_container,
});

const CONTAINER_SLOTS: usize = 256;

fn encode_container(tag: &Tag, out: &mut Vec<u8>) -> Option<()> {
    let entries = tag.as_list()?;
    let mut slots: Vec<Option<&Tag>> = Vec::new();
    for entry in entries {
        let entry = entry.as_compound()?;
        let slot = usize::try_from(entry.get_i32("slot")?).ok()?;
        if slot >= CONTAINER_SLOTS {
            return None;
        }
        if slots.len() <= slot {
            slots.resize(slot + 1, None);
        }
        slots[slot] = Some(entry.get("item")?);
    }
    write_var(i32::try_from(slots.len()).ok()?, out);
    for slot in slots {
        match slot {
            Some(item) => {
                out.push(1);
                Node::Stack.encode(item, out)?;
            }
            None => out.push(0),
        }
    }
    Some(())
}

fn decode_container(input: &mut &[u8]) -> Option<Tag> {
    let len = usize::try_from(read_var(input).ok()?).ok()?;
    if len > CONTAINER_SLOTS {
        return None;
    }
    let mut entries = Vec::new();
    for slot in 0..len {
        if read_flag(input)? {
            entries.push(Tag::Compound(
                Compound::new()
                    .with("slot", i32::try_from(slot).ok()?)
                    .with("item", Node::Stack.decode(input)?),
            ));
        }
    }
    Some(Tag::List(entries))
}

const BOOK_PAGES: Node = Node::List(&Node::Filterable(&Node::Str), 100);

const WRITABLE_BOOK: Node = Node::Rec(&[def("pages", BOOK_PAGES, Def::Empty)]);

const WRITTEN_BOOK: Node = Node::Rec(&[
    req("title", Node::Filterable(&Node::Str)),
    req("author", Node::Str),
    def("generation", Node::Var, Def::Int(0)),
    def(
        "pages",
        Node::List(&Node::Filterable(&Node::Text), 100),
        Def::Empty,
    ),
    def("resolved", Node::Bool, Def::Bool(false)),
]);

const BLOCK_POS: Node = Node::Custom(Custom {
    enc: encode_block_pos,
    dec: decode_block_pos,
});

const POS_BITS: i64 = 0x3FF_FFFF;

fn encode_block_pos(tag: &Tag, out: &mut Vec<u8>) -> Option<()> {
    let coordinates: Vec<i64> = match tag {
        Tag::IntArray(values) => values.iter().map(|value| i64::from(*value)).collect(),
        Tag::List(values) => values.iter().map(Tag::as_i64).collect::<Option<_>>()?,
        _ => return None,
    };
    let [x, y, z] = coordinates[..] else {
        return None;
    };
    let packed = ((x & POS_BITS) << 38) | ((z & POS_BITS) << 12) | (y & 0xFFF);
    out.extend(packed.to_be_bytes());
    Some(())
}

fn decode_block_pos(input: &mut &[u8]) -> Option<Tag> {
    let packed = i64::from_be_bytes(take(input, 8)?.try_into().ok()?);
    let x = i32::try_from(packed >> 38).ok()?;
    let y = i32::try_from((packed << 52) >> 52).ok()?;
    let z = i32::try_from((packed << 26) >> 38).ok()?;
    Some(Tag::IntArray(vec![x, y, z]))
}

const LODESTONE_TRACKER: Node = Node::Rec(&[
    opt(
        "target",
        Node::Rec(&[req("dimension", Node::Ident), req("pos", BLOCK_POS)]),
    ),
    def("tracked", Node::Bool, Def::Bool(true)),
]);

const INSTRUMENT: Node = Node::Holder(
    Reg::Synced("minecraft:instrument"),
    &Node::Rec(&[
        req("sound_event", SOUND),
        req("use_duration", Node::Float),
        req("range", Node::Float),
        def("durability_damage", Node::Var, Def::Int(0)),
        req("description", Node::Text),
    ]),
);

const JUKEBOX_PLAYABLE: Node = Node::Wrap(
    "song",
    &Node::Rec(&[req(
        "song",
        Node::Holder(
            Reg::Synced("minecraft:jukebox_song"),
            &Node::Rec(&[
                req("sound_event", SOUND),
                req("description", Node::Text),
                req("length_in_seconds", Node::Float),
                req("comparator_output", Node::Var),
            ]),
        ),
    )]),
);

const STRING_MAP: Node = Node::Custom(Custom {
    enc: encode_string_map,
    dec: decode_string_map,
});

fn encode_string_map(tag: &Tag, out: &mut Vec<u8>) -> Option<()> {
    let map = tag.as_compound()?;
    write_var(i32::try_from(map.len()).ok()?, out);
    for (key, value) in map.iter() {
        write_string(key, out)?;
        write_string(value.as_str()?, out)?;
    }
    Some(())
}

fn decode_string_map(input: &mut &[u8]) -> Option<Tag> {
    let len = usize::try_from(read_var(input).ok()?).ok()?;
    let mut map = Compound::new();
    for _ in 0..len {
        let key = read_string(input)?;
        map.insert(key, read_string(input)?);
    }
    Some(Tag::Compound(map))
}

const ENTITY_DATA: Node = Node::Custom(Custom {
    enc: |tag, out| encode_typed(Reg::Static("minecraft:entity_type"), tag, out),
    dec: |input| decode_typed(Reg::Static("minecraft:entity_type"), input),
});

const BLOCK_ENTITY_DATA: Node = Node::Custom(Custom {
    enc: |tag, out| encode_typed(Reg::Static("minecraft:block_entity_type"), tag, out),
    dec: |input| decode_typed(Reg::Static("minecraft:block_entity_type"), input),
});

fn encode_typed(registry: Reg, tag: &Tag, out: &mut Vec<u8>) -> Option<()> {
    let data = tag.as_compound()?;
    write_var(registry.index(data.get_str("id")?)?, out);
    let rest: Compound = data
        .iter()
        .filter(|(key, _)| *key != "id")
        .map(|(key, value)| (key.to_owned(), value.clone()))
        .collect();
    rspigot_nbt::write_network(out, &Tag::Compound(rest));
    Some(())
}

fn decode_typed(registry: Reg, input: &mut &[u8]) -> Option<Tag> {
    let id = registry.name(read_var(input).ok()?)?;
    let mut data = rspigot_nbt::read_network_compound(input).ok()??;
    data.insert("id", id);
    Some(Tag::Compound(data))
}

const BEES: Node = Node::List(
    &Node::Rec(&[
        req("entity_data", ENTITY_DATA),
        req("ticks_in_hive", Node::Var),
        req("min_ticks_in_hive", Node::Var),
    ]),
    1 << 20,
);

const SIGN_TEXT: Node = Node::Rec(&[
    req("messages", Node::Fixed(&Node::Text, 4)),
    opt("filtered_messages", Node::Fixed(&Node::Text, 4)),
    def("color", DYE_COLORS, Def::Str("black")),
    def("has_glowing_text", Node::Bool, Def::Bool(false)),
]);

const CONSTANT_INT: Node = Node::Constant(&Node::Int);
const CONSTANT_FLOAT: Node = Node::Constant(&Node::Float);

const COMPOSTABLE: Node = Node::Rec(&[req("layers", CONSTANT_INT)]);

const COOKING_FUEL: Node = Node::Rec(&[
    req("burn_time", CONSTANT_INT),
    req("speed_multiplier", CONSTANT_FLOAT),
]);

const BREWING_FUEL: Node = Node::Rec(&[
    req("uses", CONSTANT_INT),
    req("speed_multiplier", CONSTANT_FLOAT),
]);

const SKIN_PATCH_MODELS: Node = Node::Enum(&["wide", "slim"]);

const PROFILE: Node = Node::Custom(Custom {
    enc: encode_profile,
    dec: decode_profile,
});

const PROPERTIES: Node = Node::List(
    &Node::Rec(&[
        req("name", Node::Str),
        req("value", Node::Str),
        opt("signature", Node::Str),
    ]),
    16,
);

const SKIN_FIELDS: [(&str, Node); 4] = [
    ("texture", Node::Ident),
    ("cape", Node::Ident),
    ("elytra", Node::Ident),
    ("model", SKIN_PATCH_MODELS),
];

fn uuid_bytes(tag: &Tag) -> Option<[u8; 16]> {
    let Tag::IntArray(parts) = tag else {
        return None;
    };
    let parts: [i32; 4] = parts.as_slice().try_into().ok()?;
    let mut bytes = [0u8; 16];
    for (chunk, part) in bytes.chunks_mut(4).zip(parts) {
        chunk.copy_from_slice(&part.to_be_bytes());
    }
    Some(bytes)
}

fn uuid_tag(input: &mut &[u8]) -> Option<Tag> {
    let bytes = take(input, 16)?;
    Some(Tag::IntArray(
        bytes
            .chunks(4)
            .map(|chunk| i32::from_be_bytes(chunk.try_into().unwrap_or_default()))
            .collect(),
    ))
}

fn encode_profile(tag: &Tag, out: &mut Vec<u8>) -> Option<()> {
    let empty = Compound::new();
    let named;
    let profile = match tag {
        Tag::String(name) => {
            named = Compound::new().with("name", name.as_str());
            &named
        }
        other => other.as_compound().unwrap_or(&empty),
    };
    let properties = profile
        .get("properties")
        .cloned()
        .unwrap_or(Tag::List(Vec::new()));
    match (profile.get("name"), profile.get("id")) {
        (Some(name), Some(id)) => {
            out.push(1);
            out.extend(uuid_bytes(id)?);
            write_string(name.as_str()?, out)?;
        }
        (name, id) => {
            out.push(0);
            match name {
                Some(name) => {
                    out.push(1);
                    write_string(name.as_str()?, out)?;
                }
                None => out.push(0),
            }
            match id {
                Some(id) => {
                    out.push(1);
                    out.extend(uuid_bytes(id)?);
                }
                None => out.push(0),
            }
        }
    }
    PROPERTIES.encode(&properties, out)?;
    for (key, node) in SKIN_FIELDS {
        match profile.get(key) {
            Some(value) => {
                out.push(1);
                node.encode(value, out)?;
            }
            None => out.push(0),
        }
    }
    Some(())
}

fn decode_profile(input: &mut &[u8]) -> Option<Tag> {
    let mut profile = Compound::new();
    if read_flag(input)? {
        profile.insert("id", uuid_tag(input)?);
        profile.insert("name", read_string(input)?);
    } else {
        if read_flag(input)? {
            profile.insert("name", read_string(input)?);
        }
        if read_flag(input)? {
            profile.insert("id", uuid_tag(input)?);
        }
    }
    let properties = PROPERTIES.decode(input)?;
    if !properties.as_list()?.is_empty() {
        profile.insert("properties", properties);
    }
    for (key, node) in SKIN_FIELDS {
        if read_flag(input)? {
            profile.insert(key, node.decode(input)?);
        }
    }
    Some(Tag::Compound(profile))
}

const TROPICAL_PATTERN: Node = Node::EnumIds(&[
    ("kob", 0),
    ("flopper", 1),
    ("sunstreak", 256),
    ("stripey", 257),
    ("snooper", 512),
    ("glitter", 513),
    ("dasher", 768),
    ("blockfish", 769),
    ("brinely", 1024),
    ("betty", 1025),
    ("spotty", 1280),
    ("clayfish", 1281),
]);

const RABBIT_VARIANT: Node = Node::EnumIds(&[
    ("brown", 0),
    ("white", 1),
    ("black", 2),
    ("white_splotched", 3),
    ("gold", 4),
    ("salt", 5),
    ("evil", 99),
]);

const MOB_VISIBILITY: Node = Node::Rec(&[
    req(
        "targeting_entity_types",
        Node::Set(Reg::Static("minecraft:entity_type")),
    ),
    req("visibility", Node::Float),
]);

const ENCHANTABLE_LIKE: Node = Node::Rec(&[req("nutrition", Node::Var)]);

const BLOCK_PREDICATE: Node = Node::Rec(&[
    opt("blocks", Node::Set(BLOCK)),
    opt("state", Node::Never),
    opt("nbt", Node::Nbt),
    opt("components", Node::Never),
    opt("predicates", Node::Never),
]);

const ADVENTURE_PREDICATE: Node = Node::Compact(&BLOCK_PREDICATE, 1 << 20);

const NEVER_HOLDER: Node = Node::Holder(DAMAGE_TYPE, &Node::Never);

pub(super) fn node(name: &str) -> Option<Node> {
    Some(match name {
        "minecraft:custom_data" | "minecraft:bucket_entity_data" => Node::Nbt,
        "minecraft:max_stack_size"
        | "minecraft:max_damage"
        | "minecraft:damage"
        | "minecraft:repair_cost"
        | "minecraft:additional_trade_cost"
        | "minecraft:map_id"
        | "minecraft:ominous_bottle_amplifier" => Node::Var,
        "minecraft:unbreakable"
        | "minecraft:creative_slot_lock"
        | "minecraft:glider"
        | "minecraft:waxed" => Node::Unit,
        "minecraft:use_effects" => USE_EFFECTS,
        "minecraft:custom_name" | "minecraft:item_name" => Node::Text,
        "minecraft:minimum_attack_charge" | "minecraft:potion_duration_scale" => Node::Float,
        "minecraft:damage_type" => NEVER_HOLDER,
        "minecraft:item_model" | "minecraft:tooltip_style" | "minecraft:note_block_sound" => {
            Node::Ident
        }
        "minecraft:lore" => Node::List(&Node::Text, 256),
        "minecraft:rarity" => RARITY,
        "minecraft:enchantments" | "minecraft:stored_enchantments" => ENCHANTMENTS,
        "minecraft:custom_model_data" => CUSTOM_MODEL_DATA,
        "minecraft:tooltip_display" => TOOLTIP_DISPLAY,
        "minecraft:enchantment_glint_override" => Node::Bool,
        "minecraft:food" => FOOD,
        "minecraft:consumable" => CONSUMABLE,
        "minecraft:use_remainder" => Node::Stack,
        "minecraft:use_cooldown" => USE_COOLDOWN,
        "minecraft:damage_resistant" => DAMAGE_RESISTANT,
        "minecraft:tool" => TOOL,
        "minecraft:weapon" => WEAPON,
        "minecraft:enchantable" => ENCHANTABLE,
        "minecraft:repairable" => REPAIRABLE,
        "minecraft:dyed_color" => Node::Int,
        "minecraft:can_break" | "minecraft:can_place_on" => ADVENTURE_PREDICATE,
        "minecraft:potion_contents" => POTION_CONTENTS,
        "minecraft:suspicious_stew_effects" => STEW_EFFECTS,
        "minecraft:firework_explosion" => FIREWORK_EXPLOSION,
        "minecraft:fireworks" => FIREWORKS,
        "minecraft:trim" => TRIM,
        "minecraft:provides_trim_material" => TRIM_MATERIAL,
        "minecraft:banner_patterns" => BANNER_LAYERS,
        "minecraft:base_color"
        | "minecraft:dye"
        | "minecraft:wolf/collar"
        | "minecraft:cat/collar"
        | "minecraft:sheep/color"
        | "minecraft:shulker/color"
        | "minecraft:cushion/color"
        | "minecraft:tropical_fish/base_color"
        | "minecraft:tropical_fish/pattern_color" => DYE_COLORS,
        "minecraft:pot_decorations" => POT_DECORATIONS,
        "minecraft:container" => CONTAINER,
        "minecraft:bundle_contents" | "minecraft:charged_projectiles" => STACK_LIST,
        "minecraft:sulfur_cube_content" => Node::Stack,
        "minecraft:written_book_content" => WRITTEN_BOOK,
        "minecraft:writable_book_content" => WRITABLE_BOOK,
        "minecraft:profile" => PROFILE,
        "minecraft:lodestone_tracker" => LODESTONE_TRACKER,
        "minecraft:instrument" => INSTRUMENT,
        "minecraft:jukebox_playable" => JUKEBOX_PLAYABLE,
        "minecraft:block_state" => STRING_MAP,
        "minecraft:entity_data" => ENTITY_DATA,
        "minecraft:block_entity_data" => BLOCK_ENTITY_DATA,
        "minecraft:break_sound" => SOUND,
        "minecraft:provides_banner_patterns" => Node::Set(Reg::Synced("minecraft:banner_pattern")),
        "minecraft:villager_food" => ENCHANTABLE_LIKE,
        "minecraft:mob_visibility" => MOB_VISIBILITY,
        "minecraft:map_post_processing" => Node::Enum(&["lock", "scale"]),
        "minecraft:sign_text_front" | "minecraft:sign_text_back" => SIGN_TEXT,
        "minecraft:provides_pottery_pattern" => {
            Node::Reg(Reg::Synced("minecraft:decorated_pot_pattern"))
        }
        "minecraft:compostable" => COMPOSTABLE,
        "minecraft:cooking_fuel" => COOKING_FUEL,
        "minecraft:brewing_fuel" => BREWING_FUEL,
        "minecraft:bees" => BEES,
        "minecraft:tropical_fish/pattern" => TROPICAL_PATTERN,
        "minecraft:salmon/size" => Node::Enum(&["small", "medium", "large"]),
        "minecraft:fox/variant" => Node::Enum(&["red", "snow"]),
        "minecraft:parrot/variant" => {
            Node::Enum(&["red_blue", "blue", "green", "yellow_blue", "gray"])
        }
        "minecraft:mooshroom/variant" => Node::Enum(&["red", "brown"]),
        "minecraft:rabbit/variant" => RABBIT_VARIANT,
        "minecraft:horse/variant" => Node::Enum(&[
            "white",
            "creamy",
            "chestnut",
            "brown",
            "black",
            "gray",
            "dark_brown",
        ]),
        "minecraft:llama/variant" => Node::Enum(&["creamy", "white", "brown", "gray"]),
        "minecraft:axolotl/variant" => Node::Enum(&["lucy", "wild", "gold", "cyan", "blue"]),
        "minecraft:villager/variant" => Node::Reg(Reg::Static("minecraft:villager_type")),
        "minecraft:painting/variant" => {
            Node::Holder(Reg::Synced("minecraft:painting_variant"), &Node::Never)
        }
        "minecraft:wolf/variant" => Node::Reg(Reg::Synced("minecraft:wolf_variant")),
        "minecraft:wolf/sound_variant" => Node::Reg(Reg::Synced("minecraft:wolf_sound_variant")),
        "minecraft:pig/variant" => Node::Reg(Reg::Synced("minecraft:pig_variant")),
        "minecraft:pig/sound_variant" => Node::Reg(Reg::Synced("minecraft:pig_sound_variant")),
        "minecraft:cow/variant" => Node::Reg(Reg::Synced("minecraft:cow_variant")),
        "minecraft:cow/sound_variant" => Node::Reg(Reg::Synced("minecraft:cow_sound_variant")),
        "minecraft:chicken/variant" => Node::Reg(Reg::Synced("minecraft:chicken_variant")),
        "minecraft:chicken/sound_variant" => {
            Node::Reg(Reg::Synced("minecraft:chicken_sound_variant"))
        }
        "minecraft:zombie_nautilus/variant" => {
            Node::Reg(Reg::Synced("minecraft:zombie_nautilus_variant"))
        }
        "minecraft:frog/variant" => Node::Reg(Reg::Synced("minecraft:frog_variant")),
        "minecraft:cat/variant" => Node::Reg(Reg::Synced("minecraft:cat_variant")),
        "minecraft:cat/sound_variant" => Node::Reg(Reg::Synced("minecraft:cat_sound_variant")),
        "minecraft:attribute_modifiers" => Node::List(&ATTRIBUTE_ENTRY, 1 << 20),
        "minecraft:equippable" => EQUIPPABLE,
        "minecraft:block_transformer" => Node::Reg(Reg::Synced("minecraft:block_transformer")),
        "minecraft:attack_range" => ATTACK_RANGE,
        "minecraft:death_protection" => DEATH_PROTECTION,
        "minecraft:blocks_attacks" => BLOCKS_ATTACKS,
        "minecraft:piercing_weapon" => PIERCING_WEAPON,
        "minecraft:kinetic_weapon" => KINETIC_WEAPON,
        "minecraft:attack_animation" | "minecraft:interact_animation" => SWING,
        _ => return None,
    })
}

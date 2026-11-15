use rspigot_nbt::{Compound, Tag};
use rspigot_protocol::play::wire::{read_var, write_var};

use super::{Custom, Def, Node, Reg, def, opt, req, take};

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
        _ => return None,
    })
}

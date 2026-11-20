import argparse
import json
import math
import os
import re
import struct
import sys

RE_ROOT = "/Users/baptiste.parmantier/Development/rspigot/re"
REPORTS = RE_ROOT + "/datagen/generated/reports"
DEFAULT_DUMP = REPORTS + "/items.json"
DEFAULT_REGISTRIES = REPORTS + "/registries.json"
DEFAULT_COMPONENTS = REPORTS + "/minecraft/components/item"
DEFAULT_DATAPACK = RE_ROOT + "/server/data/minecraft"
CRATE = os.path.normpath(
    os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "crates", "rspigot-items")
)
DEFAULT_FAMILIES = os.path.join(CRATE, "families.txt")
DEFAULT_OUT = os.path.join(CRATE, "src")
EXPECTED_VERSION = "26.3"
NAMESPACE = "minecraft:"
BLOCK_FAMILY = "BlockItem"
DEFAULT_FAMILY = "Plain"

RARITIES = ["common", "uncommon", "rare", "epic"]
EQUIPMENT_SLOTS = ["mainhand", "offhand", "feet", "legs", "chest", "head", "body", "saddle"]
OPERATIONS = ["add_value", "add_multiplied_base", "add_multiplied_total"]
SLOT_GROUPS = [
    "any", "mainhand", "offhand", "hand", "feet", "legs", "chest", "head", "armor", "body", "saddle",
]

RESERVED_STRUCTS = {
    "Item", "ItemId", "ItemKind", "ItemBehavior", "ItemSet", "ItemTag", "BlockSet", "BlockTag",
    "BlockId", "BlockKind", "Tool", "ToolRule", "Food", "Rarity", "EquipmentSlot", "Plain",
    "BlockItem", "Self", "Option", "Some", "None", "Result", "Ok", "Err", "Vec", "Box",
    "AttributeModifier", "ModifierOperation", "SlotGroup", "Craft", "CookingKind", "Fuel", "Product", "ProductComponent",
    "StewEffect", "FireworkShapes", "Consumable", "ConsumeAnimation", "ConsumeEffect",
    "ConsumeMobEffect", "PotionContentsDefault",
}

RUST_KEYWORDS = {
    "as", "break", "const", "continue", "crate", "else", "enum", "extern", "false", "fn", "for",
    "if", "impl", "in", "let", "loop", "match", "mod", "move", "mut", "pub", "ref", "return",
    "self", "static", "struct", "super", "trait", "true", "type", "unsafe", "use", "where",
    "while", "async", "await", "dyn", "abstract", "become", "box", "do", "final", "macro",
    "override", "priv", "typeof", "unsized", "virtual", "yield", "try", "gen",
}

FAMILY_TYPE = re.compile(r"^[A-Z][A-Za-z0-9]*(<(true|false|[0-9]+)(, *(true|false|[0-9]+))*>)?$")
MODULE_NAME = re.compile(r"^[a-z][a-z0-9_]*$")
STRUCT_NAME = re.compile(r"^[A-Z][A-Za-z0-9]*$")
CLASS_NAME = re.compile(r"^[A-Za-z_$][A-Za-z0-9_$]*$")
TAG_NAME = re.compile(r"^minecraft:[a-z0-9_/]+$")
MODIFIER_NAME = re.compile(r"^minecraft:[a-z0-9_./]+$")
SOUND_NAME = MODIFIER_NAME
WIDTH = 100


def fail(message):
    sys.exit("gen_items: " + message)


def load_json(path):
    with open(path, encoding="utf-8") as handle:
        return json.load(handle)


def struct_name(path):
    return "".join(part[:1].upper() + part[1:] for part in path.split("_"))


def f32(value):
    try:
        return struct.unpack("<f", struct.pack("<f", value))[0]
    except OverflowError:
        return math.copysign(math.inf, value)


def finite(value, what):
    if isinstance(value, bool) or not isinstance(value, (int, float)) or not math.isfinite(value):
        fail("%s is not a finite number: %r" % (what, value))
    return float(value)


def f32_literal(value, what):
    target = f32(finite(value, what))
    candidate = repr(target)
    for digits in range(1, 10):
        candidate = "%.*g" % (digits, target)
        if f32(float(candidate)) == target:
            break
    mantissa, _, exponent = candidate.lower().partition("e")
    if "." not in mantissa:
        mantissa += ".0"
    text = mantissa + ("e" + str(int(exponent)) if exponent else "")
    if not re.fullmatch(r"-?[0-9]+\.[0-9]+(e[0-9]+)?", text) or f32(float(text)) != target:
        fail("%s float %r has no literal form" % (what, value))
    return text


def f64_literal(value, what):
    text = repr(finite(value, what))
    mantissa, marker, exponent = text.partition("e")
    if "." not in mantissa:
        mantissa += ".0"
    text = mantissa + (marker + exponent if marker else "")
    if float(text) != float(value):
        fail("%s double %r has no literal form" % (what, value))
    return text


def modifier_name(value, what):
    if not isinstance(value, str) or not MODIFIER_NAME.match(value):
        fail("%s unsupported name %r" % (what, value))
    return '"' + value + '"'


def rust_string(value):
    if not re.fullmatch(r"[a-z0-9_:/]+", value):
        fail("unexpected string " + repr(value))
    return '"' + value + '"'


def boolean(value, what):
    if not isinstance(value, bool):
        fail("%s is not a boolean: %r" % (what, value))
    return "true" if value else "false"


def integer(value, low, high, what):
    if isinstance(value, bool) or not isinstance(value, int) or not low <= value <= high:
        fail("%s out of range [%d, %d]: %r" % (what, low, high, value))
    return value


def registry_names(registries, key):
    registry = registries.get(key)
    if not registry:
        fail("registries.json has no " + key)
    by_id = {}
    for name, entry in registry["entries"].items():
        by_id[entry["protocol_id"]] = name
    if sorted(by_id) != list(range(len(registry["entries"]))):
        fail(key + " protocol ids are not dense")
    return [by_id[index] for index in range(len(by_id))]


def tag_constant(prefix, name):
    if not TAG_NAME.match(name):
        fail("unsupported tag name " + repr(name))
    return prefix + "_" + re.sub(r"[^a-z0-9]", "_", name[len(NAMESPACE):]).upper()


def read_families(path, items):
    classes = {item["class"] for item in items}
    names = {item["name"] for item in items}
    by_class = {}
    by_item = {}
    if not os.path.exists(path):
        return by_class, by_item
    with open(path, encoding="utf-8") as handle:
        lines = handle.read().splitlines()
    for number, raw in enumerate(lines, 1):
        line = raw.strip()
        if not line or line.startswith("#"):
            continue
        parts = line.split()
        if len(parts) != 2:
            fail("%s:%d: expected '<key> <RustFamilyType>'" % (path, number))
        key, family = parts
        if not FAMILY_TYPE.match(family):
            fail("%s:%d: invalid family type %r" % (path, number, family))
        if key.startswith("item:"):
            name = key[len("item:"):]
            if ":" not in name:
                name = NAMESPACE + name
            if name not in names:
                fail("%s:%d: unknown item %r" % (path, number, name))
            if name in by_item:
                fail("%s:%d: duplicate item %r" % (path, number, name))
            by_item[name] = family
        else:
            if key not in classes:
                fail("%s:%d: unknown vanilla class %r" % (path, number, key))
            if key in by_class:
                fail("%s:%d: duplicate class %r" % (path, number, key))
            by_class[key] = family
    return by_class, by_item


def report_set(value, names, what):
    if isinstance(value, str) and value.startswith("#"):
        return {"tag": value[1:] if ":" in value else NAMESPACE + value[1:]}
    values = [value] if isinstance(value, str) else value
    if not isinstance(values, list):
        fail("%s unsupported holder set %r" % (what, value))
    ids = []
    for entry in values:
        entry = entry if ":" in entry else NAMESPACE + entry
        if entry not in names:
            fail("%s unknown entry %r" % (what, entry))
        ids.append(names[entry])
    return {"ids": ids}


def dump_set(value, key, what):
    if set(value) == {"tag"}:
        return {"tag": value["tag"]}
    if set(value) == {key}:
        return {"ids": value[key]}
    fail("%s unsupported dumped set %r" % (what, value))


def same_float(left, right):
    return f32(finite(left, "report")) == f32(finite(right, "dump"))


def check_report(item, report, block_ids, item_ids):
    name = item["name"]
    components = report["components"]

    def mismatch(component, expected, actual):
        fail("%s %s: report %r, dump %r" % (name, component, expected, actual))

    if components.get("minecraft:max_stack_size") != item["max_stack_size"]:
        mismatch("max_stack_size", components.get("minecraft:max_stack_size"), item["max_stack_size"])
    if components.get("minecraft:max_damage") != item["max_damage"]:
        mismatch("max_damage", components.get("minecraft:max_damage"), item["max_damage"])
    if components.get("minecraft:rarity") != item["rarity"]:
        mismatch("rarity", components.get("minecraft:rarity"), item["rarity"])
    tool = components.get("minecraft:tool")
    if (tool is None) != (item["tool"] is None):
        mismatch("tool", tool, item["tool"])
    if tool is not None:
        dumped = item["tool"]
        if len(tool.get("rules", [])) != len(dumped["rules"]):
            mismatch("tool rules", tool.get("rules"), dumped["rules"])
        for rule, mine in zip(tool.get("rules", []), dumped["rules"]):
            if report_set(rule["blocks"], block_ids, name) != dump_set(mine["blocks"], "blocks", name):
                mismatch("tool rule blocks", rule["blocks"], mine["blocks"])
            if ("speed" in rule) != (mine["speed"] is not None):
                mismatch("tool rule speed", rule.get("speed"), mine["speed"])
            if "speed" in rule and not same_float(rule["speed"], mine["speed"]):
                mismatch("tool rule speed", rule["speed"], mine["speed"])
            if rule.get("correct_for_drops") != mine["correct_for_drops"]:
                mismatch("tool rule drops", rule.get("correct_for_drops"), mine["correct_for_drops"])
        if "default_mining_speed" in tool and not same_float(tool["default_mining_speed"], dumped["default_mining_speed"]):
            mismatch("default_mining_speed", tool["default_mining_speed"], dumped["default_mining_speed"])
        for key in ("damage_per_block", "can_destroy_blocks_in_creative"):
            if key in tool and tool[key] != dumped[key]:
                mismatch(key, tool[key], dumped[key])
    food = components.get("minecraft:food")
    if (food is None) != (item["food"] is None):
        mismatch("food", food, item["food"])
    if food is not None:
        if food["nutrition"] != item["food"]["nutrition"]:
            mismatch("nutrition", food["nutrition"], item["food"]["nutrition"])
        if not same_float(food["saturation"], item["food"]["saturation"]):
            mismatch("saturation", food["saturation"], item["food"]["saturation"])
        if "can_always_eat" in food and food["can_always_eat"] != item["food"]["can_always_eat"]:
            mismatch("can_always_eat", food["can_always_eat"], item["food"]["can_always_eat"])
    equippable = components.get("minecraft:equippable")
    if (None if equippable is None else equippable["slot"]) != item["equippable"]:
        mismatch("equippable", equippable, item["equippable"])
    enchantable = components.get("minecraft:enchantable")
    if (None if enchantable is None else enchantable["value"]) != item["enchantable"]:
        mismatch("enchantable", enchantable, item["enchantable"])
    repairable = components.get("minecraft:repairable")
    if (repairable is None) != (item["repairable"] is None):
        mismatch("repairable", repairable, item["repairable"])
    if repairable is not None:
        if report_set(repairable["items"], item_ids, name) != dump_set(item["repairable"], "items", name):
            mismatch("repairable items", repairable["items"], item["repairable"])
    resistant = components.get("minecraft:damage_resistant")
    if (resistant is None) != (item["damage_resistant"] is None):
        mismatch("damage_resistant", resistant, item["damage_resistant"])
    if resistant is not None and resistant["types"] != "#" + item["damage_resistant"]["tag"]:
        mismatch("damage_resistant", resistant, item["damage_resistant"])
    reported = [
        {
            "attribute": entry["type"],
            "id": entry["id"],
            "amount": entry["amount"],
            "operation": entry["operation"],
            "slot": entry.get("slot", "any"),
        }
        for entry in components.get("minecraft:attribute_modifiers", [])
    ]
    if reported != item["attribute_modifiers"]:
        mismatch("attribute_modifiers", reported, item["attribute_modifiers"])
    remainder = components.get("minecraft:use_remainder")
    if (remainder is None) != (item["use_remainder"] is None):
        mismatch("use_remainder", remainder, item["use_remainder"])
    if remainder is not None:
        if set(remainder) != {"id"} or item_ids.get(remainder["id"]) != item["use_remainder"]["item"]:
            mismatch("use_remainder", remainder, item["use_remainder"])


def check_tool_semantics(item, block_tags, block_count):
    tool = item["tool"]

    def contains(rule, block):
        blocks = rule["blocks"]
        if "tag" in blocks:
            return block in block_tags[blocks["tag"]]
        return block in blocks["blocks"]

    correct = []
    speeds = []
    default = f32(tool["default_mining_speed"])
    for block in range(block_count):
        drops = next(
            (rule["correct_for_drops"] for rule in tool["rules"] if rule["correct_for_drops"] is not None and contains(rule, block)),
            False,
        )
        if drops:
            correct.append(block)
        speed = next(
            (f32(rule["speed"]) for rule in tool["rules"] if rule["speed"] is not None and contains(rule, block)),
            default,
        )
        if speed != default:
            speeds.append([block, speed])
    if correct != tool["correct_for_drops"]:
        fail("%s correct_for_drops disagrees with Tool.isCorrectForDrops" % item["name"])
    dumped = [[block, f32(speed)] for block, speed in tool["mining_speeds"]]
    if speeds != dumped:
        fail("%s mining speeds disagree with Tool.getMiningSpeed" % item["name"])


def validate(dump, registries, components_dir):
    if dump.get("version") != EXPECTED_VERSION:
        fail("dump version %r != %r" % (dump.get("version"), EXPECTED_VERSION))
    if dump["rarities"] != RARITIES:
        fail("vanilla rarities changed: %r" % dump["rarities"])
    if dump["equipment_slots"] != EQUIPMENT_SLOTS:
        fail("vanilla equipment slots changed: %r" % dump["equipment_slots"])
    item_names = registry_names(registries, "minecraft:item")
    block_names = registry_names(registries, "minecraft:block")
    integer(dump["absolute_max_stack_size"], 1, 255, "absolute max stack size")
    integer(dump["default_max_stack_size"], 1, dump["absolute_max_stack_size"], "default max stack size")
    if dump["block_count"] != len(block_names):
        fail("dump block count %r != registry %d" % (dump["block_count"], len(block_names)))
    items = dump["items"]
    if [item["name"] for item in items] != item_names:
        fail("item names or order differ from registries.json")
    report_files = {entry[: -len(".json")] for entry in os.listdir(components_dir) if entry.endswith(".json")}
    if report_files != {name[len(NAMESPACE):] for name in item_names}:
        fail("component reports differ from the item registry")
    block_ids = {name: index for index, name in enumerate(block_names)}
    item_ids = {name: index for index, name in enumerate(item_names)}
    block_tags = {}
    for tag in dump["block_tags"]:
        tag_constant("BLOCK", tag["name"])
        if tag["blocks"] != sorted(set(tag["blocks"])):
            fail("block tag %s is not sorted" % tag["name"])
        for block in tag["blocks"]:
            integer(block, 0, len(block_names) - 1, tag["name"])
        block_tags[tag["name"]] = set(tag["blocks"])
    item_tags = {}
    for tag in dump["item_tags"]:
        tag_constant("ITEM", tag["name"])
        if tag["items"] != sorted(set(tag["items"])):
            fail("item tag %s is not sorted" % tag["name"])
        for entry in tag["items"]:
            integer(entry, 0, len(item_names) - 1, tag["name"])
        item_tags[tag["name"]] = set(tag["items"])
    for tag in dump["all_item_tags"]:
        tag_constant("ITEM", tag["name"])
        if tag["items"] != sorted(set(tag["items"])):
            fail("item tag %s is not sorted" % tag["name"])
        for entry in tag["items"]:
            integer(entry, 0, len(item_names) - 1, tag["name"])
    all_item_tags = {tag["name"]: tag["items"] for tag in dump["all_item_tags"]}
    for tag in dump["item_tags"]:
        if all_item_tags.get(tag["name"]) != tag["items"]:
            fail("item tag %s differs from the full tag dump" % tag["name"])
    structs = set()
    placed = {}
    for index, item in enumerate(items):
        name = item["name"]
        if item["id"] != index:
            fail("%s registry id %r != %d" % (name, item["id"], index))
        if not name.startswith(NAMESPACE):
            fail("unsupported item namespace " + name)
        path = name[len(NAMESPACE):]
        if not MODULE_NAME.match(path) or path in RUST_KEYWORDS or path == "mod":
            fail("unsupported item name " + name)
        item["path"] = path
        item["struct"] = struct_name(path)
        if not STRUCT_NAME.match(item["struct"]) or item["struct"] in RESERVED_STRUCTS:
            fail("unsupported struct name %s for %s" % (item["struct"], name))
        if item["struct"] in structs:
            fail("duplicate struct name " + item["struct"])
        structs.add(item["struct"])
        if not CLASS_NAME.match(item["class"]):
            fail("%s has unsupported class %r" % (name, item["class"]))
        integer(item["max_stack_size"], 1, dump["absolute_max_stack_size"], name + " max stack size")
        if item["default_max_stack_size"] != item["max_stack_size"]:
            fail("%s default max stack size disagrees with its component" % name)
        if item["max_damage"] is not None:
            integer(item["max_damage"], 1, 2**31 - 1, name + " max damage")
        if item["rarity"] not in RARITIES:
            fail("%s has unknown rarity %r" % (name, item["rarity"]))
        boolean(item["block_item"], name + " block item")
        if item["block_item"] != (item["block"] is not None):
            fail("%s block item flag disagrees with its block" % name)
        if item["block"] is not None:
            integer(item["block"], 0, len(block_names) - 1, name + " block")
            if item["block"] in placed:
                fail("%s and %s place the same block" % (placed[item["block"]], name))
            placed[item["block"]] = name
        if item["wall_block"] is not None:
            integer(item["wall_block"], 0, len(block_names) - 1, name + " wall block")
        tool = item["tool"]
        if tool is not None:
            for rule in tool["rules"]:
                blocks = dump_set(rule["blocks"], "blocks", name)
                if "tag" in blocks and blocks["tag"] not in block_tags:
                    fail("%s references undumped block tag %s" % (name, blocks["tag"]))
                if "ids" in blocks:
                    if not blocks["ids"]:
                        fail("%s has an empty direct block set" % name)
                    for block in blocks["ids"]:
                        integer(block, 0, len(block_names) - 1, name + " tool block")
                if rule["speed"] is not None:
                    finite(rule["speed"], name + " tool speed")
                if rule["correct_for_drops"] is not None:
                    boolean(rule["correct_for_drops"], name + " tool drops")
            finite(tool["default_mining_speed"], name + " default mining speed")
            integer(tool["damage_per_block"], 0, 2**31 - 1, name + " damage per block")
            boolean(tool["can_destroy_blocks_in_creative"], name + " creative")
            check_tool_semantics(item, block_tags, len(block_names))
        food = item["food"]
        if food is not None:
            integer(food["nutrition"], 0, 2**31 - 1, name + " nutrition")
            finite(food["saturation"], name + " saturation")
            boolean(food["can_always_eat"], name + " can always eat")
        if item["equippable"] is not None and item["equippable"] not in EQUIPMENT_SLOTS:
            fail("%s has unknown equipment slot %r" % (name, item["equippable"]))
        if item["enchantable"] is not None:
            integer(item["enchantable"], 1, 2**31 - 1, name + " enchantable")
        if item["repairable"] is not None:
            repair = dump_set(item["repairable"], "items", name)
            if "tag" in repair and repair["tag"] not in item_tags:
                fail("%s references undumped item tag %s" % (name, repair["tag"]))
            for entry in repair.get("ids", []):
                integer(entry, 0, len(item_names) - 1, name + " repair item")
        if item["damage_resistant"] is not None:
            tag = item["damage_resistant"]["tag"]
            if not TAG_NAME.match(tag):
                fail("%s has unsupported damage tag %r" % (name, tag))
        for modifier in item["attribute_modifiers"]:
            modifier_name(modifier["attribute"], name + " modifier attribute")
            modifier_name(modifier["id"], name + " modifier id")
            finite(modifier["amount"], name + " modifier amount")
            if modifier["operation"] not in OPERATIONS:
                fail("%s has unknown modifier operation %r" % (name, modifier["operation"]))
            if modifier["slot"] not in SLOT_GROUPS:
                fail("%s has unknown modifier slot %r" % (name, modifier["slot"]))
        for key in ("use_remainder", "crafting_remainder"):
            remainder = item[key]
            if remainder is not None:
                integer(remainder["item"], 0, len(item_names) - 1, name + " " + key)
                if remainder["count"] != 1 or remainder["patch_empty"] is not True:
                    fail("%s %s is not a plain single item" % (name, key))
        report = load_json(os.path.join(components_dir, path + ".json"))
        check_report(item, report, block_ids, item_ids)
        item["weapon_damage"] = weapon_damage(report["components"].get("minecraft:weapon"), name)
    return block_names


FAST_COOKING = "minecraft:block/fast_cooking"
DEFAULT_SPEED = "minecraft:cooking/speed_default"


def provider_path(datapack, kind, name):
    prefix = NAMESPACE
    if not name.startswith(prefix):
        fail("unsupported provider name " + name)
    return os.path.join(datapack, "context_" + kind + "_provider", name[len(prefix):] + ".json")


def provider_constant(datapack, kind, name):
    value = load_json(provider_path(datapack, kind, name))
    if isinstance(value, bool) or not isinstance(value, (int, float)):
        fail("provider %s is not a constant" % name)
    return value


def check_speed_provider(datapack):
    speed = load_json(provider_path(datapack, "float", DEFAULT_SPEED))
    if speed.get("type") != "minecraft:conditional" or speed.get("condition") != FAST_COOKING:
        fail("unsupported cooking speed provider %r" % speed)
    normal = provider_constant(datapack, "float", speed["on_false"])
    fast = provider_constant(datapack, "float", speed["on_true"])
    if (normal, fast) != (1.0, 2.0):
        fail("cooking speed multipliers changed: %r" % ((normal, fast),))


def burn_times(datapack, name):
    spec = load_json(provider_path(datapack, "int", name))
    right = spec.get("right")
    valid = (
        spec.get("type") == "minecraft:div"
        and isinstance(spec.get("left"), int)
        and isinstance(right, dict)
        and right.get("type") == "minecraft:conditional"
        and right.get("condition") == FAST_COOKING
    )
    if not valid:
        fail("unsupported burn time provider %s: %r" % (name, spec))
    normal = spec["left"] // provider_constant(datapack, "int", right["on_false"])
    fast = spec["left"] // provider_constant(datapack, "int", right["on_true"])
    integer(normal, 1, 65535, name + " burn time")
    integer(fast, 1, 65535, name + " fast burn time")
    return normal, fast


def read_fuels(items, components_dir, datapack):
    check_speed_provider(datapack)
    cache = {}
    for item in items:
        report = load_json(os.path.join(components_dir, item["path"] + ".json"))
        fuel = report["components"].get("minecraft:cooking_fuel")
        item["cooking_fuel"] = None
        trim = report["components"].get("minecraft:provides_trim_material")
        if trim is not None and not (isinstance(trim, str) and ID_PATTERN.match(trim)):
            fail("%s has an unsupported trim material %r" % (item["name"], trim))
        item["trim_material"] = trim
        if fuel is None:
            continue
        if set(fuel) != {"burn_time", "speed_multiplier"} or fuel["speed_multiplier"] != DEFAULT_SPEED:
            fail("%s has an unsupported cooking fuel %r" % (item["name"], fuel))
        if fuel["burn_time"] not in cache:
            cache[fuel["burn_time"]] = burn_times(datapack, fuel["burn_time"])
        item["cooking_fuel"] = cache[fuel["burn_time"]]


CONSUMABLE = "minecraft:consumable"
CONSUME_ANIMATIONS = ["eat", "drink"]
DEFAULT_CONSUME_SECONDS = 1.6
DEFAULT_CONSUME_SOUND = "minecraft:entity.generic.eat"
DEFAULT_TELEPORT_DIAMETER = 16.0
MOB_EFFECT_KEYS = {"id", "amplifier", "duration", "show_icon"}


def consume_sound(value, name):
    if not isinstance(value, str) or not SOUND_NAME.match(value):
        fail("%s has an unsupported sound %r" % (name, value))
    return '"' + value + '"'


def mob_effect_node(effect, name):
    if not isinstance(effect, dict) or not set(effect) <= MOB_EFFECT_KEYS or "id" not in effect:
        fail("%s has an unsupported mob effect %r" % (name, effect))
    if not isinstance(effect["id"], str) or not ID_PATTERN.match(effect["id"]):
        fail("%s has an unsupported effect id %r" % (name, effect["id"]))
    return struct_node(
        "ConsumeMobEffect",
        [
            ("effect", Atom(rust_string(effect["id"]))),
            ("amplifier", number(integer(effect.get("amplifier", 0), 0, 255, name + " amplifier"))),
            ("duration", number(integer(effect.get("duration", 0), -1, 2**31 - 1, name + " duration"))),
            ("show_icon", flag(boolean(effect.get("show_icon", True), name + " show_icon") == "true")),
        ],
    )


def consume_effect_node(effect, name):
    kind = effect.get("type")
    allowed = {
        "minecraft:apply_effects": {"type", "effects", "probability"},
        "minecraft:remove_effects": {"type", "effects"},
        "minecraft:clear_all_effects": {"type"},
        "minecraft:teleport_randomly": {"type", "diameter"},
        "minecraft:play_sound": {"type", "sound"},
    }
    if kind not in allowed or not set(effect) <= allowed[kind]:
        fail("%s has an unsupported consume effect %r" % (name, effect))
    if kind == "minecraft:apply_effects":
        effects = effect["effects"]
        if not isinstance(effects, list):
            fail("%s apply_effects is not a list" % name)
        probability = f32_literal(effect.get("probability", 1.0), name + " probability")
        return struct_node(
            "ConsumeEffect::ApplyEffects",
            [
                ("effects", slice_node([mob_effect_node(entry, name) for entry in effects])),
                ("probability", Atom(probability)),
            ],
        )
    if kind == "minecraft:remove_effects":
        effects = effect["effects"]
        names = [effects] if isinstance(effects, str) else effects
        if not isinstance(names, list) or not all(
            isinstance(entry, str) and ID_PATTERN.match(entry) for entry in names
        ):
            fail("%s remove_effects has unsupported effects %r" % (name, effects))
        return call_node("ConsumeEffect::RemoveEffects", [slice_node([Atom(rust_string(entry)) for entry in names])])
    if kind == "minecraft:clear_all_effects":
        return Atom("ConsumeEffect::ClearAllEffects")
    if kind == "minecraft:teleport_randomly":
        diameter = f32_literal(effect.get("diameter", DEFAULT_TELEPORT_DIAMETER), name + " diameter")
        return struct_node("ConsumeEffect::TeleportRandomly", [("diameter", Atom(diameter))])
    return call_node("ConsumeEffect::PlaySound", [Atom(consume_sound(effect["sound"], name))])


def consumable_node(component, name):
    allowed = {"consume_seconds", "animation", "sound", "has_consume_particles", "on_consume_effects"}
    if not isinstance(component, dict) or not set(component) <= allowed:
        fail("%s has an unsupported consumable %r" % (name, component))
    animation = component.get("animation", "eat")
    if animation not in CONSUME_ANIMATIONS:
        fail("%s has an unsupported consume animation %r" % (name, animation))
    seconds = finite(component.get("consume_seconds", DEFAULT_CONSUME_SECONDS), name + " consume seconds")
    if seconds < 0:
        fail("%s has a negative consume time" % name)
    effects = [consume_effect_node(entry, name) for entry in component.get("on_consume_effects", [])]
    return call_node(
        "Some",
        [
            struct_node(
                "Consumable",
                [
                    ("consume_seconds", Atom(f32_literal(seconds, name + " consume seconds"))),
                    ("animation", Atom("ConsumeAnimation::" + struct_name(animation))),
                    ("sound", Atom(consume_sound(component.get("sound", DEFAULT_CONSUME_SOUND), name))),
                    (
                        "has_consume_particles",
                        flag(boolean(component.get("has_consume_particles", True), name) == "true"),
                    ),
                    ("on_consume_effects", slice_node(effects)),
                ],
            )
        ],
    )


def potion_contents_node(component, name):
    if not isinstance(component, dict) or not set(component) <= {"potion", "custom_effects"}:
        fail("%s has an unsupported potion contents %r" % (name, component))
    potion = component.get("potion")
    if potion is not None and (not isinstance(potion, str) or not ID_PATTERN.match(potion)):
        fail("%s has an unsupported potion %r" % (name, potion))
    custom = [mob_effect_node(entry, name) for entry in component.get("custom_effects", [])]
    return call_node(
        "Some",
        [
            struct_node(
                "PotionContentsDefault",
                [
                    ("potion", Atom("None" if potion is None else "Some(%s)" % rust_string(potion))),
                    ("custom_effects", slice_node(custom)),
                ],
            )
        ],
    )


def stew_effects_node(component, name):
    if not isinstance(component, list):
        fail("%s has unsupported stew effects %r" % (name, component))
    nodes = []
    for effect in component:
        expect_keys(effect, {"id", "duration"}, name)
        nodes.append(
            struct_node(
                "StewEffect",
                [
                    ("effect", Atom(rust_string(effect["id"]))),
                    ("duration", number(integer(effect["duration"], 0, 2**31 - 1, name + " duration"))),
                ],
            )
        )
    return call_node("Some", [Group("&[", nodes, "]")])


def read_consumables(items, components_dir):
    for item in items:
        report = load_json(os.path.join(components_dir, item["path"] + ".json"))
        components = report["components"]
        name = item["name"]
        item["consumable"] = None
        item["potion_contents"] = None
        item["stew_effects"] = None
        if CONSUMABLE in components:
            item["consumable"] = consumable_node(components[CONSUMABLE], name)
        if "minecraft:potion_contents" in components:
            item["potion_contents"] = potion_contents_node(components["minecraft:potion_contents"], name)
        if STEW_EFFECTS in components:
            item["stew_effects"] = stew_effects_node(components[STEW_EFFECTS], name)


def weapon_damage(weapon, name):
    if weapon is None:
        return None
    if not isinstance(weapon, dict) or set(weapon) - {"item_damage_per_attack", "disable_blocking_for_seconds"}:
        fail("%s has an unsupported weapon component %r" % (name, weapon))
    return integer(weapon.get("item_damage_per_attack", 1), 0, 2**31 - 1, name + " weapon damage")


def option(value, render):
    return "None" if value is None else "Some(%s)" % render(value)


def wrap(tokens, indent):
    lines = []
    current = ""
    for token in tokens:
        piece = token + ","
        if current and len(indent) + len(current) + 1 + len(piece) > WIDTH:
            lines.append(indent + current)
            current = piece
        else:
            current = piece if not current else current + " " + piece
    if current:
        lines.append(indent + current)
    return lines


def assignment(name, type_name, value):
    line = "    const %s: %s = %s;" % (name, type_name, value)
    if len(line) <= WIDTH:
        return [line]
    return ["    const %s: %s =" % (name, type_name), "        %s;" % value]


def block_set(value, used):
    if "tag" in value:
        used.add("tags")
        return "BlockSet::Tag(&tags::%s)" % tag_constant("BLOCK", value["tag"])
    used.add("BlockId")
    return "BlockSet::Direct(&[%s])" % ", ".join("BlockId(%d)" % block for block in sorted(set(value["blocks"])))


def item_set(value, used):
    if "tag" in value:
        used.add("tags")
        return "ItemSet::Tag(&tags::%s)" % tag_constant("ITEM", value["tag"])
    return "ItemSet::Direct(&[%s])" % ", ".join("ItemId(%d)" % entry for entry in sorted(set(value["items"])))


def tool_lines(tool, name, used):
    if tool is None:
        return ["    const TOOL: Option<Tool> = None;"]
    lines = ["    const TOOL: Option<Tool> = Some(Tool {"]
    if tool["rules"]:
        used.add("BlockSet")
        used.add("ToolRule")
        lines.append("        rules: &[")
        for rule in tool["rules"]:
            lines.append("            ToolRule {")
            lines.append("                blocks: %s," % block_set(rule["blocks"], used))
            speed = option(rule["speed"], lambda value: f32_literal(value, name + " speed"))
            lines.append("                speed: %s," % speed)
            drops = option(rule["correct_for_drops"], lambda value: boolean(value, name))
            lines.append("                correct_for_drops: %s," % drops)
            lines.append("            },")
        lines.append("        ],")
    else:
        lines.append("        rules: &[],")
    lines.append("        default_mining_speed: %s," % f32_literal(tool["default_mining_speed"], name))
    lines.append("        damage_per_block: %d," % tool["damage_per_block"])
    lines.append(
        "        can_destroy_blocks_in_creative: %s,"
        % boolean(tool["can_destroy_blocks_in_creative"], name)
    )
    lines.append("    });")
    return lines


def food_lines(food, name):
    if food is None:
        return ["    const FOOD: Option<Food> = None;"]
    return [
        "    const FOOD: Option<Food> = Some(Food {",
        "        nutrition: %d," % food["nutrition"],
        "        saturation: %s," % f32_literal(food["saturation"], name + " saturation"),
        "        can_always_eat: %s," % boolean(food["can_always_eat"], name),
        "    });",
    ]


def modifier_lines(modifiers, name, used):
    if not modifiers:
        return ["    const ATTRIBUTE_MODIFIERS: &'static [AttributeModifier] = &[];"]
    used.add("ModifierOperation")
    used.add("SlotGroup")
    lines = ["    const ATTRIBUTE_MODIFIERS: &'static [AttributeModifier] = &["]
    for modifier in modifiers:
        lines.append("        AttributeModifier {")
        lines.append("            attribute: %s," % modifier_name(modifier["attribute"], name))
        lines.append("            id: %s," % modifier_name(modifier["id"], name))
        lines.append("            amount: %s," % f64_literal(modifier["amount"], name + " modifier"))
        lines.append("            operation: ModifierOperation::%s," % struct_name(modifier["operation"]))
        lines.append("            slot: SlotGroup::%s," % struct_name(modifier["slot"]))
        lines.append("        },")
    lines.append("    ];")
    return lines


class Atom:
    def __init__(self, text):
        self.text = text


class Group:
    def __init__(self, open_, items, close, spaced=False):
        self.open = open_
        self.items = items
        self.close = close
        self.spaced = spaced


class Prefix:
    def __init__(self, text, node):
        self.text = text
        self.node = node


def flat(node):
    if isinstance(node, Atom):
        return node.text
    if isinstance(node, Prefix):
        return node.text + flat(node.node)
    inner = ", ".join(flat(item) for item in node.items)
    if node.spaced and inner:
        inner = " " + inner + " "
    return node.open + inner + node.close


def render(node, indent, prefix="", suffix=""):
    text = indent + prefix + flat(node) + suffix
    if len(text) <= WIDTH or isinstance(node, Atom) or (isinstance(node, Group) and not node.items):
        return [text]
    if isinstance(node, Prefix):
        return render(node.node, indent, prefix + node.text, suffix)
    if len(node.items) == 1 and not node.spaced:
        return render(node.items[0], indent, prefix + node.open, node.close + suffix)
    lines = [indent + prefix + node.open]
    for item in node.items:
        lines.extend(render(item, indent + "    ", "", ","))
    lines.append(indent + node.close + suffix)
    return lines


def struct_node(name, fields):
    return Group(name + " {", [Prefix(key + ": ", value) for key, value in fields], "}", True)


def call_node(name, args):
    return Group(name + "(", args, ")")


def slice_node(items):
    return Group("&[", items, "]")


def number(value):
    return Atom(str(value))


def flag(value):
    return Atom("true" if value else "false")


CRAFTING_TYPES = {
    "crafting_shaped",
    "crafting_shapeless",
    "crafting_transmute",
    "crafting_dye",
    "crafting_imbue",
    "crafting_decorated_pot",
    "crafting_special_bannerduplicate",
    "crafting_special_shielddecoration",
    "crafting_special_repairitem",
    "crafting_special_mapextending",
    "crafting_special_firework_star_fade",
    "crafting_special_firework_star",
    "crafting_special_firework_rocket",
    "crafting_special_bookcloning",
    "smelting",
    "blasting",
    "smoking",
    "campfire_cooking",
    "stonecutting",
    "smithing_transform",
    "smithing_trim",
}
ID_PATTERN = re.compile(r"^minecraft:[a-z0-9_]+$")
RECIPE_ID_LINE = re.compile(r'^\s+id: "minecraft:[a-z0-9_/]+",$')
COOKING_KINDS = {
    "smelting": "Smelting",
    "blasting": "Blasting",
    "smoking": "Smoking",
    "campfire_cooking": "Campfire",
}
STEW_EFFECTS = "minecraft:suspicious_stew_effects"
FIREWORK_SHAPES = ["burst", "creeper", "large_ball", "star"]


def recipe_paths(root):
    paths = []
    for dirpath, dirnames, filenames in os.walk(root):
        dirnames.sort()
        for filename in filenames:
            if filename.endswith(".json"):
                relative = os.path.relpath(os.path.join(dirpath, filename), root)
                paths.append(relative[: -len(".json")].replace(os.sep, "/"))
    return sorted(paths)


def expect_keys(value, allowed, where):
    if not isinstance(value, dict) or set(value) - set(allowed):
        fail("unsupported keys in %s: %s" % (where, json.dumps(value)))


class Crafts:
    def __init__(self, dump, item_names):
        self.tags = {tag["name"]: tag["items"] for tag in dump["all_item_tags"]}
        self.ids = {name: index for index, name in enumerate(item_names)}
        self.damageable = [item["id"] for item in dump["items"] if item["max_damage"] is not None]
        self.used = set()
        self.by_item = {}

    def item(self, name, where):
        name = name if ":" in name else NAMESPACE + name
        if name not in self.ids:
            fail("unknown item %s in %s" % (name, where))
        return self.ids[name]

    def ingredient(self, value, where, track=True):
        if isinstance(value, str) and value.startswith("#"):
            tag = value[1:] if ":" in value else NAMESPACE + value[1:]
            if tag not in self.tags:
                fail("unknown item tag %s in %s" % (tag, where))
            if track:
                self.used.add(tag)
            return Atom("ItemSet::Tag(&tags::%s)" % tag_constant("ITEM", tag)), set(self.tags[tag])
        values = [value] if isinstance(value, str) else value
        valid = isinstance(values, list) and values and all(
            isinstance(entry, str) and not entry.startswith("#") for entry in values
        )
        if not valid:
            fail("unsupported ingredient %s in %s" % (json.dumps(value), where))
        ids = sorted({self.item(entry, where) for entry in values})
        node = call_node("ItemSet::Direct", [slice_node([call_node("ItemId", [number(i)]) for i in ids])])
        return node, set(ids)

    def node(self, value, where):
        return self.ingredient(value, where)[0]

    def product(self, result, where):
        expect_keys(result, {"id", "count", "components"}, where)
        count = integer(result.get("count", 1), 1, 99, where + " count")
        components = []
        for key, value in result.get("components", {}).items():
            if key != STEW_EFFECTS:
                fail("unsupported result component %s in %s" % (key, where))
            effects = []
            for effect in value:
                expect_keys(effect, {"id", "duration"}, where)
                effects.append(
                    struct_node(
                        "StewEffect",
                        [
                            ("effect", Atom(rust_string(effect["id"]))),
                            ("duration", number(integer(effect["duration"], 0, 2**31 - 1, where + " duration"))),
                        ],
                    )
                )
            components.append(call_node("ProductComponent::SuspiciousStewEffects", [slice_node(effects)]))
        if not components:
            return call_node("Product::of", [number(count)])
        return struct_node("Product", [("count", number(count)), ("components", slice_node(components))])

    def result_item(self, recipe, where, fallback=None):
        result = recipe["result"]
        if "id" in result:
            return self.item(result["id"], where)
        if fallback is None or len(fallback) != 1:
            fail("%s has no result item to declare it on" % where)
        return next(iter(fallback))

    def same_item(self, recipe, key, owner, where):
        if self.item(recipe[key], where) != owner:
            fail("%s %s is not the result item" % (where, key))

    def shaped(self, recipe, where):
        expect_keys(recipe, {"type", "category", "group", "key", "pattern", "result", "show_notification"}, where)
        pattern = recipe["pattern"]
        keys = recipe["key"]
        if not pattern or any(not isinstance(row, str) or not row for row in pattern):
            fail("empty pattern row in " + where)
        if len({len(row) for row in pattern}) != 1:
            fail("ragged pattern in " + where)
        if any(len(key) != 1 or key == " " for key in keys):
            fail("unsupported key character in " + where)
        columns = [index for row in pattern for index, cell in enumerate(row) if cell != " "]
        rows = [index for index, row in enumerate(pattern) if row.strip(" ")]
        if not columns:
            fail("empty pattern in " + where)
        left, right, top, bottom = min(columns), max(columns), min(rows), max(rows)
        trimmed = [row[left : right + 1] for row in pattern[top : bottom + 1]]
        width, height = len(trimmed[0]), len(trimmed)
        if width > 3 or height > 3:
            fail("pattern larger than the grid in " + where)
        unused = set(keys) - {cell for row in trimmed for cell in row}
        if unused:
            fail("unused key %s in %s" % (sorted(unused), where))
        built = {}
        cells = []
        for row in trimmed:
            for cell in row:
                if cell == " ":
                    cells.append(Atom("None"))
                    continue
                if cell not in keys:
                    fail("pattern uses undefined key %r in %s" % (cell, where))
                if cell not in built:
                    built[cell] = self.node(keys[cell], where)
                cells.append(call_node("Some", [built[cell]]))
        texts = [flat(cell) for cell in cells]
        symmetrical = all(
            texts[y * width + x] == texts[y * width + width - 1 - x] for y in range(height) for x in range(width // 2)
        )
        owner = self.result_item(recipe, where)
        node = struct_node(
            "Craft::Shaped",
            [
                ("width", number(width)),
                ("height", number(height)),
                ("symmetrical", flag(symmetrical)),
                ("cells", slice_node(cells)),
                ("product", self.product(recipe["result"], where)),
            ],
        )
        return owner, node

    def shapeless(self, recipe, where):
        expect_keys(recipe, {"type", "category", "group", "ingredients", "result"}, where)
        ingredients = recipe["ingredients"]
        if not 1 <= len(ingredients) <= 9:
            fail("%d ingredients in %s" % (len(ingredients), where))
        node = struct_node(
            "Craft::Shapeless",
            [
                ("ingredients", slice_node([self.node(entry, where) for entry in ingredients])),
                ("product", self.product(recipe["result"], where)),
            ],
        )
        return self.result_item(recipe, where), node

    def transmute(self, recipe, where):
        expect_keys(
            recipe,
            {"type", "category", "group", "input", "material", "material_count", "add_material_count_to_result", "result"},
            where,
        )
        keeps_input = "id" not in recipe["result"]
        input_node, input_ids = self.ingredient(recipe["input"], where, track=not keeps_input)
        bounds = recipe.get("material_count", 1)
        if isinstance(bounds, int):
            low = high = bounds
        else:
            expect_keys(bounds, {"min", "max"}, where)
            low, high = bounds.get("min", 1), bounds.get("max", 8)
        integer(low, 1, 8, where + " min material")
        integer(high, low, 8, where + " max material")
        result = recipe["result"]
        expect_keys(result, {"id", "count"}, where)
        count = integer(result.get("count", 1), 1, 99, where + " count")
        material = self.node(recipe["material"], where)

        def craft(input_set):
            return struct_node(
                "Craft::Transmute",
                [
                    ("input", input_set),
                    ("material", material),
                    ("min_material", number(low)),
                    ("max_material", number(high)),
                    ("add_material_count", flag(recipe.get("add_material_count_to_result", False))),
                    ("count", number(count)),
                ],
            )

        if "id" in result:
            return self.item(result["id"], where), craft(input_node)
        return [
            (owner, craft(call_node("ItemSet::Direct", [slice_node([call_node("ItemId", [number(owner)])])])))
            for owner in sorted(input_ids)
        ]

    def single(self, recipe, where):
        kind = recipe["type"][len("minecraft:") :]
        if kind == "crafting_shaped":
            return self.shaped(recipe, where)
        if kind == "crafting_shapeless":
            return self.shapeless(recipe, where)
        if kind == "crafting_transmute":
            return self.transmute(recipe, where)
        if kind in COOKING_KINDS:
            result = recipe["result"]
            expect_keys(recipe, {"type", "category", "group", "cookingtime", "experience", "ingredient", "result"}, where)
            expect_keys(result, {"id"}, where)
            node = struct_node(
                "Craft::Cooking",
                [
                    ("id", Atom(rust_string(self.recipe_id))),
                    ("kind", Atom("CookingKind::" + COOKING_KINDS[kind])),
                    ("ingredient", self.node(recipe["ingredient"], where)),
                    ("experience", Atom(f32_literal(recipe["experience"], where + " experience"))),
                    ("cooking_time", number(integer(recipe["cookingtime"], 1, 65535, where + " cooking time"))),
                ],
            )
            return self.item(result["id"], where), node
        if kind == "stonecutting":
            result = recipe["result"]
            expect_keys(recipe, {"type", "ingredient", "result"}, where)
            expect_keys(result, {"id", "count"}, where)
            count = integer(result.get("count", 1), 1, 99, where + " count")
            node = struct_node(
                "Craft::Stonecutting",
                [
                    ("id", Atom(rust_string(self.recipe_id))),
                    ("ingredient", self.node(recipe["ingredient"], where)),
                    ("count", number(count)),
                ],
            )
            return self.item(result["id"], where), node
        if kind == "smithing_transform":
            result = recipe["result"]
            expect_keys(recipe, {"type", "template", "base", "addition", "result"}, where)
            expect_keys(result, {"id"}, where)
            fields = [(key, self.node(recipe[key], where)) for key in ("template", "base", "addition")]
            return self.item(result["id"], where), struct_node("Craft::SmithingTransform", fields)
        if kind == "smithing_trim":
            expect_keys(recipe, {"type", "template", "base", "addition", "pattern"}, where)
            template_node, template_ids = self.ingredient(recipe["template"], where)
            if len(template_ids) != 1:
                fail("%s template is not a single item" % where)
            pattern = recipe["pattern"]
            if not isinstance(pattern, str) or not ID_PATTERN.match(pattern):
                fail("%s unsupported trim pattern %r" % (where, pattern))
            node = struct_node(
                "Craft::SmithingTrim",
                [
                    ("template", template_node),
                    ("base", self.node(recipe["base"], where)),
                    ("addition", self.node(recipe["addition"], where)),
                    ("pattern", Atom(rust_string(pattern))),
                ],
            )
            return next(iter(template_ids)), node
        if kind == "crafting_special_repairitem":
            expect_keys(recipe, {"type"}, where)
            return [(owner, Atom("Craft::RepairItem")) for owner in self.damageable]
        result = recipe["result"]
        if kind == "crafting_dye":
            expect_keys(recipe, {"type", "group", "dye", "target", "result"}, where)
            expect_keys(result, {"id"}, where)
            owner = self.item(result["id"], where)
            self.same_item(recipe, "target", owner, where)
            return owner, struct_node("Craft::Dye", [("dye", self.node(recipe["dye"], where))])
        if kind == "crafting_imbue":
            expect_keys(recipe, {"type", "source", "material", "result"}, where)
            expect_keys(result, {"id", "count"}, where)
            node = struct_node(
                "Craft::Imbue",
                [
                    ("source", self.node(recipe["source"], where)),
                    ("material", self.node(recipe["material"], where)),
                    ("count", number(integer(result.get("count", 1), 1, 99, where + " count"))),
                ],
            )
            return self.item(result["id"], where), node
        if kind == "crafting_decorated_pot":
            expect_keys(recipe, {"type", "back", "front", "left", "right", "result"}, where)
            expect_keys(result, {"id"}, where)
            fields = [(side, self.node(recipe[side], where)) for side in ("back", "front", "left", "right")]
            return self.item(result["id"], where), struct_node("Craft::DecoratedPot", fields)
        if kind == "crafting_special_bannerduplicate":
            expect_keys(recipe, {"type", "banner", "result"}, where)
            expect_keys(result, {"id"}, where)
            owner = self.item(result["id"], where)
            self.same_item(recipe, "banner", owner, where)
            return owner, Atom("Craft::BannerDuplicate")
        if kind == "crafting_special_shielddecoration":
            expect_keys(recipe, {"type", "banner", "target", "result"}, where)
            expect_keys(result, {"id"}, where)
            owner = self.item(result["id"], where)
            self.same_item(recipe, "target", owner, where)
            return owner, struct_node("Craft::ShieldDecoration", [("banner", self.node(recipe["banner"], where))])
        if kind == "crafting_special_mapextending":
            expect_keys(recipe, {"type", "map", "material", "result"}, where)
            expect_keys(result, set(), where)
            map_node, map_ids = self.ingredient(recipe["map"], where)
            node = struct_node(
                "Craft::MapExtending", [("map", map_node), ("material", self.node(recipe["material"], where))]
            )
            return self.result_item(recipe, where, map_ids), node
        if kind == "crafting_special_firework_star_fade":
            expect_keys(recipe, {"type", "dye", "target", "result"}, where)
            expect_keys(result, {"id"}, where)
            owner = self.item(result["id"], where)
            self.same_item(recipe, "target", owner, where)
            return owner, struct_node("Craft::FireworkStarFade", [("dye", self.node(recipe["dye"], where))])
        if kind == "crafting_special_firework_star":
            expect_keys(recipe, {"type", "dye", "fuel", "shapes", "trail", "twinkle", "result"}, where)
            expect_keys(result, {"id"}, where)
            if sorted(recipe["shapes"]) != sorted(FIREWORK_SHAPES):
                fail("unexpected firework shapes in " + where)
            shapes = struct_node(
                "FireworkShapes", [(shape, self.node(recipe["shapes"][shape], where)) for shape in FIREWORK_SHAPES]
            )
            fields = [(key, self.node(recipe[key], where)) for key in ("dye", "fuel", "trail", "twinkle")]
            return self.item(result["id"], where), struct_node("Craft::FireworkStar", fields + [("shapes", shapes)])
        if kind == "crafting_special_firework_rocket":
            expect_keys(recipe, {"type", "fuel", "shell", "star", "result"}, where)
            expect_keys(result, {"id", "count"}, where)
            fields = [(key, self.node(recipe[key], where)) for key in ("shell", "star", "fuel")]
            count = integer(result.get("count", 1), 1, 99, where + " count")
            node = struct_node("Craft::FireworkRocket", fields + [("count", number(count))])
            return self.item(result["id"], where), node
        if kind == "crafting_special_bookcloning":
            expect_keys(recipe, {"type", "source", "material", "result"}, where)
            expect_keys(result, {"id"}, where)
            fields = [(key, self.node(recipe[key], where)) for key in ("source", "material")]
            return self.item(result["id"], where), struct_node("Craft::BookCloning", fields)
        fail("unhandled crafting recipe type %s in %s" % (kind, where))

    def identified(self, node):
        field = Prefix("id: ", Atom(rust_string(self.recipe_id)))
        if isinstance(node, Atom):
            return Group(node.text + " {", [field], "}", True)
        if any(isinstance(item, Prefix) and item.text == "id: " for item in node.items):
            return node
        return Group(node.open, [field] + node.items, node.close, node.spaced)

    def read(self, datapack):
        root = os.path.join(datapack, "recipe")
        total = 0
        expanded = 0
        for path in recipe_paths(root):
            recipe = load_json(os.path.join(root, path + ".json"))
            kind = recipe["type"]
            if not kind.startswith("minecraft:") or kind[len("minecraft:") :] not in CRAFTING_TYPES:
                continue
            total += 1
            self.recipe_id = NAMESPACE + path
            outcome = self.single(recipe, "recipe " + path)
            pairs = outcome if isinstance(outcome, list) else [outcome]
            expanded += len(pairs) - 1
            for owner, node in pairs:
                self.by_item.setdefault(owner, []).append(self.identified(node))
        declared = sum(len(nodes) for nodes in self.by_item.values())
        if declared != total + expanded:
            fail("declared %d crafts, expected %d" % (declared, total + expanded))
        return total


def item_file(item, behavior, crafts):
    name = item["name"]
    used = set()
    body = [
        "    const NAME: &'static str = %s;" % rust_string(name),
        "    const ID: ItemId = ItemId(%d);" % item["id"],
        '    const CLASS: &\'static str = "%s";' % item["class"],
        "    const MAX_STACK_SIZE: u8 = %d;" % item["max_stack_size"],
        "    const MAX_DAMAGE: Option<u32> = %s;" % option(item["max_damage"], str),
        "    const WEAPON_DAMAGE: Option<u32> = %s;" % option(item["weapon_damage"], str),
        "    const RARITY: Rarity = Rarity::%s;" % struct_name(item["rarity"]),
        "    const BLOCK: Option<BlockId> = %s;" % option(item["block"], lambda block: "BlockId(%d)" % block),
    ]
    body.extend(tool_lines(item["tool"], name, used))
    body.extend(food_lines(item["food"], name))
    body.append(
        "    const EQUIPPABLE: Option<EquipmentSlot> = %s;"
        % option(item["equippable"], lambda slot: "EquipmentSlot::%s" % struct_name(slot))
    )
    body.append("    const ENCHANTABLE: Option<u32> = %s;" % option(item["enchantable"], str))
    repairable = option(item["repairable"], lambda value: item_set(value, used))
    body.extend(assignment("REPAIRABLE", "Option<ItemSet>", repairable))
    resistant = option(item["damage_resistant"], lambda value: rust_string(value["tag"]))
    body.append("    const DAMAGE_RESISTANT: Option<&'static str> = %s;" % resistant)
    remainder = option(item["use_remainder"], lambda value: "ItemId(%d)" % value["item"])
    body.append("    const USE_REMAINDER: Option<ItemId> = %s;" % remainder)
    body.extend(modifier_lines(item["attribute_modifiers"], name, used))
    if item["crafting_remainder"] is not None:
        body.append("    const CRAFTING_REMAINDER: Option<ItemId> = Some(ItemId(%d));" % item["crafting_remainder"]["item"])
    consume_lines = []
    for key, type_name, constant in (
        ("consumable", "Option<Consumable>", "CONSUMABLE"),
        ("potion_contents", "Option<PotionContentsDefault>", "POTION_CONTENTS"),
        ("stew_effects", "Option<&'static [StewEffect]>", "SUSPICIOUS_STEW_EFFECTS"),
    ):
        if item[key] is not None:
            consume_lines.extend(render(item[key], "    ", "const %s: %s = " % (constant, type_name), ";"))
    body.extend(consume_lines)
    if item["trim_material"] is not None:
        body.append("    const TRIM_MATERIAL: Option<&'static str> = Some(%s);" % rust_string(item["trim_material"]))
    if item["cooking_fuel"] is not None:
        normal, fast = item["cooking_fuel"]
        body.extend(
            assignment(
                "COOKING_FUEL",
                "Option<Fuel>",
                "Some(Fuel { burn_time: %d, fast_burn_time: %d })" % (normal, fast),
            )
        )
    craft_lines = []
    if crafts:
        craft_lines = render(Group("&[", crafts, "]"), "    ", "const CRAFTS: &'static [Craft] = ", ";")
        body.extend(craft_lines)
        if any("tags::" in line for line in craft_lines):
            used.add("tags")
    body.append("    type Behavior = crate::family::%s;" % behavior)
    names = ["AttributeModifier", "EquipmentSlot", "Food", "Item", "ItemId", "ItemSet", "Rarity", "Tool"]
    names.extend(
        name for name in ("BlockSet", "ModifierOperation", "SlotGroup", "ToolRule") if name in used
    )
    if item["cooking_fuel"] is not None:
        names.append("Fuel")
    craft_text = "\n".join(craft_lines + consume_lines)
    names.extend(
        name
        for name in (
            "Consumable",
            "ConsumeAnimation",
            "ConsumeEffect",
            "ConsumeMobEffect",
            "CookingKind",
            "Craft",
            "FireworkShapes",
            "Fuel",
            "PotionContentsDefault",
            "Product",
            "ProductComponent",
            "StewEffect",
        )
        if re.search(r"\b" + name + r"\b", craft_text)
    )
    if "tags" in used:
        names.append("tags")
    names.sort(key=lambda entry: (entry[:1].islower(), entry))
    imports = ["use crate::{%s};" % ", ".join(names)]
    if len(imports[0]) > WIDTH:
        imports = ["use crate::{"] + wrap(names, "    ") + ["};"]
    lines = ["use rspigot_blocks::BlockId;", ""] + imports + [
        "",
        "pub struct %s;" % item["struct"],
        "",
        "impl Item for %s {" % item["struct"],
    ]
    lines.extend(body)
    lines.append("}")
    for line in lines:
        if len(line) > WIDTH and not RECIPE_ID_LINE.match(line):
            fail("%s line too long: %s" % (name, line))
    return "\n".join(lines) + "\n"


def tags_file(dump, extra):
    lines = [
        "use rspigot_blocks::BlockId;",
        "",
        "use crate::{BlockTag, ItemId, ItemTag};",
    ]
    for tag in dump["block_tags"]:
        lines.append("")
        lines.append("pub(crate) static %s: BlockTag = BlockTag {" % tag_constant("BLOCK", tag["name"]))
        lines.append("    name: %s," % rust_string(tag["name"]))
        lines.append("    blocks: &[")
        lines.extend(wrap(["BlockId(%d)" % block for block in tag["blocks"]], "        "))
        lines.append("    ],")
        lines.append("};")
    item_tags = {tag["name"]: tag for tag in dump["item_tags"]}
    for tag in dump["all_item_tags"]:
        if tag["name"] in extra:
            item_tags[tag["name"]] = tag
    for tag in (item_tags[name] for name in sorted(item_tags)):
        lines.append("")
        lines.append("pub(crate) static %s: ItemTag = ItemTag {" % tag_constant("ITEM", tag["name"]))
        lines.append("    name: %s," % rust_string(tag["name"]))
        lines.append("    items: &[")
        lines.extend(wrap(["ItemId(%d)" % entry for entry in tag["items"]], "        "))
        lines.append("    ],")
        lines.append("};")
    for line in lines:
        if len(line) > WIDTH:
            fail("tags line too long: " + line)
    return "\n".join(lines) + "\n"


def kind_file(items, block_count, default_stack):
    lines = ["pub const DEFAULT_MAX_STACK_SIZE: u8 = %d;" % default_stack, ""]
    lines.append("crate::lookup::item_kinds! {")
    lines.append("    count: %d," % len(items))
    lines.append("    kinds: [")
    lines.extend("        %s," % item["struct"] for item in items)
    lines.append("    ],")
    lines.append("}")
    lines.append("")
    by_path = sorted(items, key=lambda item: item["path"])
    lines.append("pub(crate) static NAMES: [(&str, ItemKind); %d] = [" % len(items))
    lines.extend('    ("%s", ItemKind::%s),' % (item["path"], item["struct"]) for item in by_path)
    lines.append("];")
    lines.append("")
    table = [None] * block_count
    for item in items:
        if item["block"] is not None:
            table[item["block"]] = item["struct"]
    lines.append("pub(crate) static BLOCKS: [Option<ItemKind>; %d] = [" % block_count)
    lines.extend("    None," if entry is None else "    Some(ItemKind::%s)," % entry for entry in table)
    lines.append("];")
    return "\n".join(lines) + "\n"


def mod_file(items):
    ordered = sorted(items, key=lambda item: item["path"])
    lines = ["mod %s;" % item["path"] for item in ordered]
    lines.append("")
    lines.extend("pub use %s::%s;" % (item["path"], item["struct"]) for item in ordered)
    return "\n".join(lines) + "\n"


def write(path, content, stats, check):
    stats["files"] += 1
    stats["bytes"] += len(content.encode("utf-8"))
    if os.path.exists(path):
        with open(path, encoding="utf-8") as handle:
            if handle.read() == content:
                return
    stats["written"] += 1
    if check:
        stats["stale"].append(path)
        return
    with open(path, "w", encoding="utf-8", newline="\n") as handle:
        handle.write(content)


def top(counter, limit):
    return sorted(counter.items(), key=lambda pair: (-pair[1], pair[0]))[:limit]


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--dump", default=DEFAULT_DUMP)
    parser.add_argument("--registries", default=DEFAULT_REGISTRIES)
    parser.add_argument("--components", default=DEFAULT_COMPONENTS)
    parser.add_argument("--datapack", default=DEFAULT_DATAPACK)
    parser.add_argument("--families", default=DEFAULT_FAMILIES)
    parser.add_argument("--out", default=DEFAULT_OUT)
    parser.add_argument("--check", action="store_true")
    parser.add_argument("--stats", action="store_true")
    args = parser.parse_args()

    dump = load_json(args.dump)
    registries = load_json(args.registries)
    block_names = validate(dump, registries, args.components)
    items = dump["items"]
    by_class, by_item = read_families(args.families, items)
    read_fuels(items, args.components, args.datapack)
    read_consumables(items, args.components)
    crafts = Crafts(dump, [item["name"] for item in items])
    recipes = crafts.read(args.datapack)

    items_dir = os.path.join(args.out, "items")
    if not args.check:
        os.makedirs(items_dir, exist_ok=True)
    stats = {"files": 0, "bytes": 0, "written": 0, "stale": []}
    expected_files = {"mod.rs"}
    families = {}
    for item in items:
        behavior = by_item.get(item["name"]) or by_class.get(item["class"]) or DEFAULT_FAMILY
        if (behavior == BLOCK_FAMILY) != (item["block"] is not None):
            fail("%s of class %s maps to family %s" % (item["name"], item["class"], behavior))
        families[behavior] = families.get(behavior, 0) + 1
        file_name = item["path"] + ".rs"
        expected_files.add(file_name)
        declared = crafts.by_item.get(item["id"], [])
        write(os.path.join(items_dir, file_name), item_file(item, behavior, declared), stats, args.check)
    write(os.path.join(items_dir, "mod.rs"), mod_file(items), stats, args.check)
    kind = kind_file(items, len(block_names), dump["default_max_stack_size"])
    write(os.path.join(args.out, "kind.rs"), kind, stats, args.check)
    write(os.path.join(args.out, "tags.rs"), tags_file(dump, crafts.used), stats, args.check)
    removed = 0
    if os.path.isdir(items_dir):
        for entry in sorted(os.listdir(items_dir)):
            if entry.endswith(".rs") and entry not in expected_files:
                removed += 1
                if args.check:
                    stats["stale"].append(os.path.join(items_dir, entry))
                else:
                    os.remove(os.path.join(items_dir, entry))
    print(
        "items %d classes %d block_items %d tools %d foods %d equippable %d modified %d block_tags %d item_tags %d families %d files %d bytes %d written %d removed %d"
        % (
            len(items),
            len({item["class"] for item in items}),
            sum(1 for item in items if item["block"] is not None),
            sum(1 for item in items if item["tool"] is not None),
            sum(1 for item in items if item["food"] is not None),
            sum(1 for item in items if item["equippable"] is not None),
            sum(1 for item in items if item["attribute_modifiers"]),
            len(dump["block_tags"]),
            len(dump["item_tags"]),
            len(by_class) + len(by_item),
            stats["files"],
            stats["bytes"],
            stats["written"],
            removed,
        )
    )
    print(
        "crafting recipes %d declared on %d items, %d tags"
        % (recipes, len(crafts.by_item), len(crafts.used))
    )
    if args.stats:
        per_class = {}
        for item in items:
            per_class[item["class"]] = per_class.get(item["class"], 0) + 1
        print("per class top 30 " + " ".join("%s=%d" % pair for pair in top(per_class, 30)))
        print("per family " + " ".join("%s=%d" % pair for pair in top(families, len(families))))
    if args.check and stats["stale"]:
        for path in stats["stale"]:
            print("stale " + path, file=sys.stderr)
        sys.exit(1)


if __name__ == "__main__":
    main()

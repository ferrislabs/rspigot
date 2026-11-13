import argparse
import json
import math
import os
import re
import struct
import subprocess
import sys

RE_ROOT = "/Users/baptiste.parmantier/Development/rspigot/re"
DEFAULT_REPORTS = RE_ROOT + "/datagen/generated/reports"
DEFAULT_DATAPACK = RE_ROOT + "/server/data/minecraft"
DEFAULT_CLASSES = RE_ROOT + "/server"
DEFAULT_JAVAP = "/opt/homebrew/opt/openjdk@25/bin/javap"
DEFAULT_OUT = os.path.normpath(
    os.path.join(
        os.path.dirname(os.path.abspath(__file__)),
        "..",
        "crates",
        "rspigot-data",
        "src",
        "generated",
    )
)

SYNCED = [
    ("BIOME", "worldgen/biome"),
    ("CHAT_TYPE", "chat_type"),
    ("TRIM_PATTERN", "trim_pattern"),
    ("TRIM_MATERIAL", "trim_material"),
    ("WOLF_VARIANT", "wolf_variant"),
    ("WOLF_SOUND_VARIANT", "wolf_sound_variant"),
    ("PIG_VARIANT", "pig_variant"),
    ("PIG_SOUND_VARIANT", "pig_sound_variant"),
    ("FROG_VARIANT", "frog_variant"),
    ("CAT_VARIANT", "cat_variant"),
    ("CAT_SOUND_VARIANT", "cat_sound_variant"),
    ("COW_SOUND_VARIANT", "cow_sound_variant"),
    ("COW_VARIANT", "cow_variant"),
    ("CHICKEN_SOUND_VARIANT", "chicken_sound_variant"),
    ("CHICKEN_VARIANT", "chicken_variant"),
    ("ZOMBIE_NAUTILUS_VARIANT", "zombie_nautilus_variant"),
    ("PAINTING_VARIANT", "painting_variant"),
    ("SULFUR_CUBE_ARCHETYPE", "sulfur_cube_archetype"),
    ("DIMENSION_TYPE", "dimension_type"),
    ("DAMAGE_TYPE", "damage_type"),
    ("BANNER_PATTERN", "banner_pattern"),
    ("ENCHANTMENT", "enchantment"),
    ("JUKEBOX_SONG", "jukebox_song"),
    ("INSTRUMENT", "instrument"),
    ("TEST_ENVIRONMENT", "test_environment"),
    ("TEST_INSTANCE", "test_instance"),
    ("DIALOG", "dialog"),
    ("WORLD_CLOCK", "world_clock"),
    ("TIMELINE", "timeline"),
    ("DECORATED_POT_PATTERN", "decorated_pot_pattern"),
    ("BLOCK_TRANSFORMER", "block_transformer"),
    ("BLOCK_STATE_PROVIDER", "worldgen/block_state_provider"),
]

AIR_BLOCKS = {
    "AIR": "minecraft:air",
    "VOID_AIR": "minecraft:void_air",
    "CAVE_AIR": "minecraft:cave_air",
}

EXPECTED_AIR_STATES = {"AIR": 0, "VOID_AIR": 18649, "CAVE_AIR": 18650}

LINE_WIDTH = 100
NAME_RE = re.compile(r"^[a-z0-9_.\-/]+$")
ID_RE = re.compile(r"^[a-z0-9_.\-]+:[a-z0-9_.\-/]+$")


def fail(message):
    raise SystemExit("error: " + message)


def load_json(path):
    with open(path, encoding="utf-8") as handle:
        return json.load(handle)


def normalize_id(value):
    if ":" not in value:
        value = "minecraft:" + value
    assert ID_RE.match(value), value
    return value


def enchantment_max_level(enchantment):
    path = os.path.join(DEFAULT_DATAPACK, "enchantment", enchantment.split(":", 1)[1] + ".json")
    with open(path) as handle:
        return json.load(handle)["max_level"]


def wrapped(tokens, indent):
    lines = []
    line = ""
    for token in tokens:
        piece = token + ","
        if line and len(indent) + len(line) + 1 + len(piece) > LINE_WIDTH:
            lines.append(indent + line)
            line = piece
        elif line:
            line = line + " " + piece
        else:
            line = piece
    if line:
        lines.append(indent + line)
    return lines


def quoted(values):
    for value in values:
        assert NAME_RE.match(value.replace(":", "/", 1)), value
    return ['"' + value + '"' for value in values]


def array_lines(tokens, indent, head, tail):
    if not tokens:
        return [indent + head + "&[]" + tail]
    lines = [indent + head + "&["]
    lines.extend(wrapped(tokens, indent + "    "))
    lines.append(indent + "]" + tail)
    return lines


def java_lines(javap, classes, class_name):
    result = subprocess.run(
        [javap, "-c", "-p", "-classpath", classes, class_name],
        check=True,
        capture_output=True,
        text=True,
    )
    return result.stdout.split("\n")


def static_init_start(lines):
    for index, line in enumerate(lines):
        if line.strip() == "static {};":
            return index
    fail("static initializer not found")


def java_registry_keys(lines):
    current = None
    keys = {}
    for line in lines[static_init_start(lines) :]:
        match = re.search(r"// String (.*)$", line)
        if match:
            current = match.group(1)
            continue
        match = re.search(r"putstatic\s+#\d+\s+// Field (\w+):", line)
        if match:
            keys[match.group(1)] = current
    return keys


def java_synchronized_order(lines):
    names = []
    for line in lines[static_init_start(lines) :]:
        match = re.search(
            r"getstatic\s+#\d+\s+// Field net/minecraft/core/registries/Registries\.(\w+):",
            line,
        )
        if match:
            names.append(match.group(1))
            continue
        match = re.search(r"putstatic\s+#\d+\s+// Field (\w+):", line)
        if match:
            if match.group(1) == "SYNCHRONIZED_REGISTRIES":
                return names
            names = []
    fail("synchronized registries not found")


def verify_synced_against_java(javap, classes):
    if not os.path.exists(javap):
        print("javap not found, synced registry verification skipped", file=sys.stderr)
        return False
    keys = java_registry_keys(
        java_lines(javap, classes, "net.minecraft.core.registries.Registries")
    )
    order = java_synchronized_order(
        java_lines(javap, classes, "net.minecraft.resources.RegistryDataLoader")
    )
    expected = [constant for constant, _ in SYNCED]
    assert order == expected, (order, expected)
    for constant, path in SYNCED:
        assert keys[constant] == path, (constant, keys[constant], path)
    return True


def json_names(root):
    names = []
    for dirpath, dirnames, filenames in os.walk(root):
        dirnames.sort()
        for filename in filenames:
            if filename.endswith(".json"):
                relative = os.path.relpath(os.path.join(dirpath, filename), root)
                names.append(relative[: -len(".json")].replace(os.sep, "/"))
    return sorted(names)


def build_static_registries(registries):
    result = []
    for registry_id in sorted(registries):
        spec = registries[registry_id]
        entries = spec["entries"]
        by_id = sorted(entries, key=lambda name: entries[name]["protocol_id"])
        assert [entries[name]["protocol_id"] for name in by_id] == list(range(len(by_id)))
        assert len(by_id) < 65536
        positions = {name: index for index, name in enumerate(by_id)}
        by_name = [positions[name] for name in sorted(by_id)]
        result.append((registry_id, by_id, by_name))
    return result


def emit_static_registries(static_registries):
    index_of = {registry_id: i for i, (registry_id, _, _) in enumerate(static_registries)}
    out = [
        "use crate::registries::StaticRegistry;",
        "",
        "pub(crate) const ENTITY_TYPE: usize = %d;" % index_of["minecraft:entity_type"],
        "",
        "pub(crate) static STATIC: [StaticRegistry; %d] = [" % len(static_registries),
    ]
    for registry_id, by_id, by_name in static_registries:
        out.append("    StaticRegistry {")
        out.append('        id: "%s",' % registry_id)
        out.extend(array_lines(quoted(by_id), "        ", "entries: ", ","))
        out.extend(array_lines([str(i) for i in by_name], "        ", "by_name: ", ","))
        out.append("    },")
    out.append("];")
    return "\n".join(out) + "\n"


def build_synced(datapack):
    result = []
    missing = []
    for constant, path in SYNCED:
        directory = os.path.join(datapack, path)
        if not os.path.isdir(directory):
            missing.append(path)
            names = []
        else:
            names = json_names(directory)
        for name in names:
            assert NAME_RE.match(name), name
        result.append((constant, "minecraft:" + path, ["minecraft:" + n for n in names]))
    return result, missing


def emit_synced(synced):
    out = ["use crate::registries::SyncedRegistry;", ""]
    for index, (constant, _, _) in enumerate(synced):
        if constant in ("BIOME", "DIMENSION_TYPE"):
            out.append("pub(crate) const %s: usize = %d;" % (constant, index))
    out.append("")
    out.append("pub static SYNCED: &[SyncedRegistry] = &[")
    for _, registry_id, entries in synced:
        out.append("    SyncedRegistry {")
        out.append('        id: "%s",' % registry_id)
        out.extend(array_lines(quoted(entries), "        ", "entries: ", ","))
        out.append("    },")
    out.append("];")
    return "\n".join(out) + "\n"


def datapack_registry_dirs(datapack):
    known = set()
    for entry in sorted(os.listdir(datapack)):
        if entry in ("tags", "datapacks") or not os.path.isdir(os.path.join(datapack, entry)):
            continue
        if entry == "worldgen":
            for child in sorted(os.listdir(os.path.join(datapack, entry))):
                if os.path.isdir(os.path.join(datapack, entry, child)):
                    known.add(entry + "/" + child)
        else:
            known.add(entry)
    return known


def read_tag_files(datapack, known):
    root = os.path.join(datapack, "tags")
    table = {}
    for relative in json_names(root):
        parts = relative.split("/")
        registry = None
        for length in range(len(parts) - 1, 0, -1):
            if "/".join(parts[:length]) in known:
                registry = "/".join(parts[:length])
                tag = "/".join(parts[length:])
                break
        if registry is None:
            fail("tag file without known registry: " + relative)
        values = load_json(os.path.join(root, relative + ".json"))["values"]
        table.setdefault(registry, {})["minecraft:" + tag] = values
    return table


def parse_tag_value(value):
    if isinstance(value, str):
        return value, True
    assert set(value) <= {"id", "required"}, value
    return value["id"], value.get("required", True)


def resolve_tags(registry, tag_files, ids):
    cache = {}

    def resolve(tag, stack):
        if tag in cache:
            return cache[tag]
        assert tag not in stack, (registry, tag)
        collected = set()
        for value in tag_files[tag]:
            text, required = parse_tag_value(value)
            if text.startswith("#"):
                reference = normalize_id(text[1:])
                if reference in tag_files:
                    collected |= resolve(reference, stack | {tag})
                elif required:
                    fail("unknown tag %s in %s %s" % (reference, registry, tag))
            else:
                entry = normalize_id(text)
                if entry in ids:
                    collected.add(ids[entry])
                elif required:
                    fail("unknown entry %s in %s %s" % (entry, registry, tag))
        cache[tag] = collected
        return collected

    return {tag: sorted(resolve(tag, frozenset())) for tag in sorted(tag_files)}


def build_tags(datapack, static_registries, synced):
    static_ids = {
        registry_id: {name: i for i, name in enumerate(by_id)}
        for registry_id, by_id, _ in static_registries
    }
    synced_ids = {
        registry_id: {name: i for i, name in enumerate(entries)}
        for _, registry_id, entries in synced
    }
    known = set(registry_id[len("minecraft:") :] for registry_id in static_ids)
    known |= set(registry_id[len("minecraft:") :] for registry_id in synced_ids)
    known |= datapack_registry_dirs(datapack)
    table = read_tag_files(datapack, known)
    result = {}
    skipped = []
    for registry in sorted(table):
        registry_id = "minecraft:" + registry
        if registry_id in static_ids:
            ids = static_ids[registry_id]
        elif registry_id in synced_ids:
            ids = synced_ids[registry_id]
        else:
            skipped.append(registry_id)
            continue
        result[registry_id] = resolve_tags(registry, table[registry], ids)
    return result, skipped


def emit_tags(tag_table):
    out = ["use crate::tags::RegistryTags;", "", "pub static TAGS: &[RegistryTags] = &["]
    for registry_id in sorted(tag_table):
        out.append("    RegistryTags {")
        out.append('        registry: "%s",' % registry_id)
        out.append("        tags: &[")
        for tag in sorted(tag_table[registry_id]):
            assert NAME_RE.match(tag.replace(":", "/", 1)), tag
            ids = [str(i) for i in tag_table[registry_id][tag]]
            line = '            ("%s", &[%s]),' % (tag, ", ".join(ids))
            if len(line) <= LINE_WIDTH:
                out.append(line)
            else:
                out.append('            ("%s", &[' % tag)
                out.extend(wrapped(ids, " " * 16))
                out.append("            ]),")
        out.append("        ],")
        out.append("    },")
    out.append("];")
    return "\n".join(out) + "\n"


COMPONENT_FUNCTIONS = {
    "minecraft:copy_components",
    "minecraft:copy_custom_data",
    "minecraft:copy_name",
    "minecraft:copy_state",
    "minecraft:set_components",
    "minecraft:set_custom_data",
    "minecraft:set_lore",
    "minecraft:set_name",
    "minecraft:toggle_tooltips",
}

ENTITY_TARGETS = {
    "this": "This",
    "attacker": "Attacker",
    "direct_attacker": "DirectAttacker",
    "attacking_player": "AttackingPlayer",
    "target_entity": "TargetEntity",
    "interacting_entity": "InteractingEntity",
}

COMPOSITE_ENTRIES = {
    "minecraft:alternatives": "Alternatives",
    "minecraft:sequence": "Sequence",
    "minecraft:group": "Group",
}

TABLE_TYPES = {
    "blocks": "minecraft:block",
    "entities": "minecraft:entity",
}

EXTRA_TABLES = [
    ("minecraft:charged_creeper/root", "minecraft:entity"),
    ("minecraft:brush/armadillo", "minecraft:entity_interact"),
    ("minecraft:gameplay/armadillo_shed", "minecraft:gift"),
    ("minecraft:gameplay/sniffer_digging", "minecraft:gift"),
    ("minecraft:gameplay/cat_morning_gift", "minecraft:gift"),
    ("minecraft:gameplay/chicken_lay", "minecraft:gift"),
    ("minecraft:gameplay/panda_sneeze", "minecraft:gift"),
    ("minecraft:gameplay/piglin_bartering", "minecraft:barter"),
    ("minecraft:gameplay/hero_of_the_village/armorer_gift", "minecraft:gift"),
    ("minecraft:gameplay/hero_of_the_village/baby_gift", "minecraft:gift"),
    ("minecraft:gameplay/hero_of_the_village/butcher_gift", "minecraft:gift"),
    ("minecraft:gameplay/hero_of_the_village/cartographer_gift", "minecraft:gift"),
    ("minecraft:gameplay/hero_of_the_village/cleric_gift", "minecraft:gift"),
    ("minecraft:gameplay/hero_of_the_village/farmer_gift", "minecraft:gift"),
    ("minecraft:gameplay/hero_of_the_village/fisherman_gift", "minecraft:gift"),
    ("minecraft:gameplay/hero_of_the_village/fletcher_gift", "minecraft:gift"),
    ("minecraft:gameplay/hero_of_the_village/leatherworker_gift", "minecraft:gift"),
    ("minecraft:gameplay/hero_of_the_village/librarian_gift", "minecraft:gift"),
    ("minecraft:gameplay/hero_of_the_village/mason_gift", "minecraft:gift"),
    ("minecraft:gameplay/hero_of_the_village/shepherd_gift", "minecraft:gift"),
    ("minecraft:gameplay/hero_of_the_village/toolsmith_gift", "minecraft:gift"),
    ("minecraft:gameplay/hero_of_the_village/unemployed_gift", "minecraft:gift"),
    ("minecraft:gameplay/hero_of_the_village/weaponsmith_gift", "minecraft:gift"),
    ("minecraft:shearing/sheep", "minecraft:shearing"),
    ("minecraft:shearing/snow_golem", "minecraft:shearing"),
    ("minecraft:till/rooted_dirt", "minecraft:block_interact"),
    ("minecraft:harvest/beehive", "minecraft:block_interact"),
]

REFERENCED_TABLE_TYPES = {"minecraft:block", "minecraft:entity", "minecraft:fishing", "minecraft:shearing"}

FARM_VARIANT_KEYS = {"asset_id", "baby_asset_id", "model", "spawn_conditions"}
WOLF_VARIANT_KEYS = {"assets", "baby_assets", "spawn_conditions"}
CAT_VARIANT_KEYS = {"asset_id", "baby_asset_id", "spawn_conditions"}
FROG_VARIANT_KEYS = {"asset_id", "spawn_conditions"}

MOB_VARIANTS = [
    ("COW_VARIANTS", "cow_variant", FARM_VARIANT_KEYS),
    ("PIG_VARIANTS", "pig_variant", FARM_VARIANT_KEYS),
    ("CHICKEN_VARIANTS", "chicken_variant", FARM_VARIANT_KEYS),
    ("WOLF_VARIANTS", "wolf_variant", WOLF_VARIANT_KEYS),
    ("CAT_VARIANTS", "cat_variant", CAT_VARIANT_KEYS),
    ("FROG_VARIANTS", "frog_variant", FROG_VARIANT_KEYS),
]

FARM_SOUND_KEYS = ["ambient", "death", "eat", "hurt", "step"]
WOLF_SOUND_KEYS = ["ambient", "death", "growl", "hurt", "pant", "step", "whine"]
CAT_SOUND_KEYS = [
    "ambient",
    "beg_for_food",
    "death",
    "eat",
    "hiss",
    "hurt",
    "purr",
    "purreow",
    "stray_ambient",
]

SOUND_VARIANTS = [
    ("COW_SOUND_VARIANTS", "cow_sound_variant", FARM_SOUND_KEYS),
    ("PIG_SOUND_VARIANTS", "pig_sound_variant", FARM_SOUND_KEYS),
    ("CHICKEN_SOUND_VARIANTS", "chicken_sound_variant", FARM_SOUND_KEYS),
    ("WOLF_SOUND_VARIANTS", "wolf_sound_variant", WOLF_SOUND_KEYS),
    ("CAT_SOUND_VARIANTS", "cat_sound_variant", CAT_SOUND_KEYS),
]

SOUND_SET_FIELDS = sorted(set(FARM_SOUND_KEYS) | set(WOLF_SOUND_KEYS) | set(CAT_SOUND_KEYS))

DYE_COLORS = [
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
]

ENTITY_FLAGS = {
    "is_on_fire": "on_fire",
    "is_baby": "baby",
}

COMPONENT_PREDICATES = {
    "minecraft:sheep/color": "dye",
    "minecraft:frog/variant": "minecraft:frog_variant",
    "minecraft:chicken/variant": "minecraft:chicken_variant",
}

LOOT_TYPES = [
    "BlockPredicate",
    "BonusFormula",
    "Condition",
    "DamageSourcePredicate",
    "EnchantmentPredicate",
    "Entry",
    "EntryKind",
    "EntityFlags",
    "EntityPredicate",
    "EntitySubPredicate",
    "EntityTarget",
    "FloatProvider",
    "Function",
    "FunctionKind",
    "IntProvider",
    "IntRange",
    "ItemPredicate",
    "LevelBasedValue",
    "Leaf",
    "LootTable",
    "Output",
    "Pool",
    "TagPredicate",
]


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
    if len(text) <= LINE_WIDTH or isinstance(node, Atom) or (
        isinstance(node, Group) and not node.items
    ):
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


def rs_struct(name, fields):
    return Group(name + " {", [Prefix(key + ": ", value) for key, value in fields], "}", True)


def rs_call(name, args):
    return Group(name + "(", args, ")")


def rs_slice(items):
    return Group("&[", items, "]")


def rs_array(items):
    return Group("[", items, "]")


def rs_tuple(items):
    return Group("(", items, ")")


def rs_ref(node):
    return Prefix("&", node)


def rs_option(node):
    return Atom("None") if node is None else rs_call("Some", [node])


def rs_str(value):
    assert isinstance(value, str) and re.match(r"^[a-z0-9_.:/\-]+$", value), value
    return Atom('"' + value + '"')


def rs_bool(value):
    assert isinstance(value, bool), value
    return Atom("true" if value else "false")


def rs_int(value):
    assert isinstance(value, int) and not isinstance(value, bool), value
    assert -(2**31) <= value < 2**31, value
    return Atom(str(value))


def to_f32(value):
    try:
        return struct.unpack("<f", struct.pack("<f", value))[0]
    except OverflowError:
        return math.inf


def rs_f32(value):
    assert isinstance(value, (int, float)) and not isinstance(value, bool), value
    target = to_f32(float(value))
    assert math.isfinite(target), value
    for digits in range(1, 10):
        text = "%.*g" % (digits, target)
        if to_f32(float(text)) == target:
            break
    if "e" in text:
        mantissa, exponent = text.split("e")
        text = mantissa + "e" + str(int(exponent))
    elif "." not in text:
        text = text + ".0"
    return Atom(text)


def rs_f64(value):
    assert isinstance(value, (int, float)) and not isinstance(value, bool), value
    assert math.isfinite(float(value)), value
    text = repr(float(value))
    assert re.fullmatch(r"-?[0-9]+\.[0-9]+", text), value
    return Atom(text)


def expect_keys(value, allowed, where):
    assert isinstance(value, dict), (where, value)
    extra = set(value) - set(allowed)
    if extra:
        fail("unsupported keys %s in %s: %s" % (sorted(extra), where, json.dumps(value)))


class LootBuilder:
    def __init__(self, ids, synced_names, block_properties, tag_table, where):
        self.ids = ids
        self.block_ids = ids["minecraft:block"]
        self.item_ids = ids["minecraft:item"]
        self.synced_names = synced_names
        self.enchantments = set(synced_names["minecraft:enchantment"])
        self.block_properties = block_properties
        self.tag_table = tag_table
        self.where = where
        self.references = set()
        self.tables = set()
        self.ignored = {}

    def note_ignored(self, construct):
        self.ignored.setdefault(construct, set()).add(self.where)

    def holder_set(self, value, registry):
        ids = self.ids[registry]
        if isinstance(value, str) and value.startswith("#"):
            tag = normalize_id(value[1:])
            if tag not in self.tag_table[registry]:
                fail("unknown %s tag %s in %s" % (registry, tag, self.where))
            return rs_call("HolderSet::Tag", [rs_str(tag)])
        values = [value] if isinstance(value, str) else value
        assert isinstance(values, list) and values, (self.where, value)
        resolved = []
        for entry in values:
            name = normalize_id(entry)
            if name not in ids:
                fail("unknown %s entry %s in %s" % (registry, name, self.where))
            resolved.append(ids[name])
        return rs_call("HolderSet::Direct", [rs_slice([Atom(str(i)) for i in sorted(set(resolved))])])

    def block_names(self, value):
        if isinstance(value, str) and value.startswith("#"):
            tag = normalize_id(value[1:])
            names = {name for name, i in self.block_ids.items() if i in set(self.tag_table["minecraft:block"][tag])}
            return sorted(names)
        values = [value] if isinstance(value, str) else value
        return [normalize_id(entry) for entry in values]

    def block_predicate(self, predicate):
        expect_keys(predicate, {"blocks", "state"}, self.where)
        state = predicate.get("state", {})
        assert isinstance(state, dict), (self.where, state)
        blocks = predicate.get("blocks")
        if blocks is not None:
            for name in self.block_names(blocks):
                properties = self.block_properties[name]
                for key, value in state.items():
                    if not isinstance(value, str):
                        fail("non exact state matcher in %s" % self.where)
                    if key not in properties or value not in properties[key]:
                        fail("state %s=%s not valid for %s in %s" % (key, value, name, self.where))
        pairs = [rs_tuple([rs_str(key), rs_str(state[key])]) for key in sorted(state)]
        return rs_struct(
            "BlockPredicate",
            [
                ("blocks", rs_option(None if blocks is None else self.holder_set(blocks, "minecraft:block"))),
                ("state", rs_slice(pairs)),
            ],
        )

    def int_range(self, value):
        if isinstance(value, int) and not isinstance(value, bool):
            return rs_struct("IntRange", [("min", rs_option(rs_int(value))), ("max", rs_option(rs_int(value)))])
        expect_keys(value, {"min", "max"}, self.where)
        return rs_struct(
            "IntRange",
            [
                ("min", rs_option(None if "min" not in value else rs_int(value["min"]))),
                ("max", rs_option(None if "max" not in value else rs_int(value["max"]))),
            ],
        )

    def enchantment_predicate(self, predicate):
        expect_keys(predicate, {"enchantments", "levels"}, self.where)
        names = None
        if "enchantments" in predicate:
            value = predicate["enchantments"]
            values = [value] if isinstance(value, str) else value
            names = []
            for entry in values:
                if entry.startswith("#"):
                    tag = normalize_id(entry[1:])
                    members = self.tag_table["minecraft:enchantment"].get(tag)
                    if members is None:
                        fail("unknown enchantment tag %s in %s" % (tag, self.where))
                    expanded = [self.synced_names["minecraft:enchantment"][i] for i in members]
                else:
                    expanded = [normalize_id(entry)]
                for name in expanded:
                    if name not in self.enchantments:
                        fail("unknown enchantment %s in %s" % (name, self.where))
                    names.append(rs_str(name))
        levels = None if "levels" not in predicate else self.int_range(predicate["levels"])
        return rs_struct(
            "EnchantmentPredicate",
            [
                ("enchantments", rs_option(None if names is None else rs_slice(names))),
                ("levels", rs_option(levels)),
            ],
        )

    def item_predicate(self, predicate):
        expect_keys(predicate, {"items", "predicates"}, self.where)
        sub = predicate.get("predicates", {})
        expect_keys(sub, {"minecraft:enchantments"}, self.where)
        enchantments = [self.enchantment_predicate(p) for p in sub.get("minecraft:enchantments", [])]
        items = predicate.get("items")
        return rs_struct(
            "ItemPredicate",
            [
                ("items", rs_option(None if items is None else self.holder_set(items, "minecraft:item"))),
                ("enchantments", rs_slice(enchantments)),
            ],
        )

    def optional_bool(self, value):
        return rs_option(None if value is None else rs_bool(value))

    def entity_flags(self, value):
        expect_keys(value, set(ENTITY_FLAGS), self.where)
        return rs_struct(
            "EntityFlags",
            [(field, self.optional_bool(value.get(key))) for key, field in sorted(ENTITY_FLAGS.items())],
        )

    def component_value(self, component, value):
        kind = COMPONENT_PREDICATES.get(component)
        if kind is None:
            fail("unsupported component predicate %s in %s" % (component, self.where))
        if not isinstance(value, str):
            fail("non exact component predicate %s in %s" % (component, self.where))
        if kind == "dye":
            if value not in self.ids["dye"]:
                fail("unknown dye colour %s in %s" % (value, self.where))
            return rs_str(value)
        name = normalize_id(value)
        if name not in self.synced_names[kind]:
            fail("unknown %s entry %s in %s" % (kind, name, self.where))
        return rs_str(name)

    def entity_sub_predicate(self, key, value):
        if key == "minecraft:type_specific/sheep":
            expect_keys(value, {"sheared"}, self.where)
            return rs_struct("EntitySubPredicate::Sheep", [("sheared", self.optional_bool(value.get("sheared")))])
        if key == "minecraft:type_specific/cube_mob":
            expect_keys(value, {"size"}, self.where)
            return rs_struct("EntitySubPredicate::CubeMob", [("size", self.int_range(value.get("size", {})))])
        if key == "minecraft:type_specific/raider":
            expect_keys(value, {"has_raid", "is_captain"}, self.where)
            return rs_struct(
                "EntitySubPredicate::Raider",
                [
                    ("has_raid", self.optional_bool(value.get("has_raid"))),
                    ("captain", self.optional_bool(value.get("is_captain"))),
                ],
            )
        fail("unsupported entity sub predicate %s in %s" % (key, self.where))

    def entity_predicate(self, value):
        assert isinstance(value, dict), (self.where, value)
        sub_keys = {"minecraft:type_specific/sheep", "minecraft:type_specific/cube_mob", "minecraft:type_specific/raider"}
        expect_keys(
            value,
            {"minecraft:entity_type", "minecraft:flags", "minecraft:equipment", "minecraft:vehicle", "minecraft:components"}
            | sub_keys,
            self.where,
        )
        kind = value.get("minecraft:entity_type")
        flags = self.entity_flags(value.get("minecraft:flags", {}))
        equipment = value.get("minecraft:equipment", {})
        expect_keys(equipment, {"mainhand"}, self.where)
        mainhand = equipment.get("mainhand")
        vehicle = value.get("minecraft:vehicle")
        components = value.get("minecraft:components", {})
        assert isinstance(components, dict), (self.where, components)
        pairs = [
            rs_tuple([rs_str(component), self.component_value(component, components[component])])
            for component in sorted(components)
        ]
        subs = [self.entity_sub_predicate(key, value[key]) for key in sorted(sub_keys) if key in value]
        return rs_struct(
            "EntityPredicate",
            [
                ("kind", rs_option(None if kind is None else self.holder_set(kind, "minecraft:entity_type"))),
                ("flags", flags),
                ("mainhand", rs_option(None if mainhand is None else self.item_predicate(mainhand))),
                ("vehicle", rs_option(None if vehicle is None else rs_ref(self.entity_predicate(vehicle)))),
                ("components", rs_slice(pairs)),
                ("sub", rs_slice(subs)),
            ],
        )

    def damage_source_predicate(self, value):
        expect_keys(value, {"tags", "source_entity", "direct_entity", "is_direct"}, self.where)
        tags = []
        for entry in value.get("tags", []):
            expect_keys(entry, {"id", "expected"}, self.where)
            tag = entry["id"]
            if not tag.startswith("#"):
                fail("damage type tag predicate without # in %s" % self.where)
            tag = normalize_id(tag[1:])
            if tag not in self.tag_table["minecraft:damage_type"]:
                fail("unknown damage type tag %s in %s" % (tag, self.where))
            tags.append(rs_struct("TagPredicate", [("tag", rs_str(tag)), ("expected", rs_bool(entry["expected"]))]))
        source = value.get("source_entity")
        direct = value.get("direct_entity")
        return rs_struct(
            "DamageSourcePredicate",
            [
                ("tags", rs_slice(tags)),
                ("source_entity", rs_option(None if source is None else rs_ref(self.entity_predicate(source)))),
                ("direct_entity", rs_option(None if direct is None else rs_ref(self.entity_predicate(direct)))),
                ("is_direct", self.optional_bool(value.get("is_direct"))),
            ],
        )

    def level_based_value(self, value):
        if isinstance(value, (int, float)) and not isinstance(value, bool):
            return rs_call("LevelBasedValue::Constant", [rs_f32(value)])
        kind = value.get("type") if isinstance(value, dict) else None
        if kind == "minecraft:linear":
            expect_keys(value, {"type", "base", "per_level_above_first"}, self.where)
            return rs_struct(
                "LevelBasedValue::Linear",
                [
                    ("base", rs_f32(value["base"])),
                    ("per_level_above_first", rs_f32(value["per_level_above_first"])),
                ],
            )
        fail("unsupported level based value %s in %s" % (json.dumps(value), self.where))

    def float_provider(self, value):
        if not isinstance(value, dict):
            return rs_call("FloatProvider::Constant", [rs_f32(value)])
        kind = value.get("type")
        if kind == "minecraft:constant":
            expect_keys(value, {"type", "value"}, self.where)
            return rs_call("FloatProvider::Constant", [rs_f32(value["value"])])
        if kind == "minecraft:uniform":
            expect_keys(value, {"type", "min", "max"}, self.where)
            return rs_struct(
                "FloatProvider::Uniform",
                [("min", self.float_constant(value["min"])), ("max", self.float_constant(value["max"]))],
            )
        fail("unsupported float provider %s in %s" % (json.dumps(value), self.where))

    def float_constant(self, value):
        if isinstance(value, dict):
            expect_keys(value, {"type", "value"}, self.where)
            if value.get("type") != "minecraft:constant":
                fail("unsupported float provider %s in %s" % (json.dumps(value), self.where))
            value = value["value"]
        return rs_f32(value)

    def int_constant(self, value):
        if isinstance(value, dict):
            expect_keys(value, {"type", "value"}, self.where)
            if value.get("type") != "minecraft:constant":
                fail("unsupported nested int provider %s in %s" % (json.dumps(value), self.where))
            value = value["value"]
        return rs_int(value)

    def int_provider(self, value):
        if not isinstance(value, dict):
            return rs_call("IntProvider::Constant", [rs_int(value)])
        kind = value.get("type")
        if kind == "minecraft:constant":
            expect_keys(value, {"type", "value"}, self.where)
            return rs_call("IntProvider::Constant", [rs_int(value["value"])])
        if kind == "minecraft:uniform":
            expect_keys(value, {"type", "min", "max"}, self.where)
            return rs_struct(
                "IntProvider::Uniform",
                [("min", self.int_constant(value["min"])), ("max", self.int_constant(value["max"]))],
            )
        if kind == "minecraft:binomial":
            expect_keys(value, {"type", "n", "p"}, self.where)
            return rs_struct(
                "IntProvider::Binomial",
                [("n", self.int_constant(value["n"])), ("p", self.float_constant(value["p"]))],
            )
        fail("unsupported int provider %s in %s" % (json.dumps(value), self.where))

    def condition(self, value):
        if isinstance(value, str):
            name = normalize_id(value)
            self.references.add(name)
            return rs_call("Condition::Reference", [rs_str(name)])
        kind = value.get("type") if isinstance(value, dict) else None
        if kind == "minecraft:survives_explosion":
            expect_keys(value, {"type"}, self.where)
            return Atom("Condition::SurvivesExplosion")
        if kind == "minecraft:match_block":
            rest = {k: v for k, v in value.items() if k != "type"}
            return rs_call("Condition::MatchBlock", [self.block_predicate(rest)])
        if kind == "minecraft:match_tool":
            expect_keys(value, {"type", "predicate"}, self.where)
            return rs_call("Condition::MatchTool", [self.item_predicate(value.get("predicate", {}))])
        if kind == "minecraft:location_check":
            expect_keys(value, {"type", "offsetX", "offsetY", "offsetZ", "predicate"}, self.where)
            predicate = value.get("predicate", {})
            expect_keys(predicate, {"block"}, self.where)
            block = predicate.get("block")
            self.note_ignored("location_check")
            offset = rs_array([rs_int(value.get(axis, 0)) for axis in ("offsetX", "offsetY", "offsetZ")])
            return rs_struct(
                "Condition::LocationCheck",
                [
                    ("offset", offset),
                    ("block", rs_option(None if block is None else self.block_predicate(block))),
                ],
            )
        if kind == "minecraft:entity_properties":
            expect_keys(value, {"type", "entity", "predicate"}, self.where)
            if value["entity"] not in ENTITY_TARGETS:
                fail("unknown entity target %s in %s" % (value["entity"], self.where))
            target = ENTITY_TARGETS[value["entity"]]
            predicate = value.get("predicate")
            return rs_struct(
                "Condition::EntityProperties",
                [
                    ("target", Atom("EntityTarget::" + target)),
                    (
                        "predicate",
                        rs_option(None if predicate is None else rs_ref(self.entity_predicate(predicate))),
                    ),
                ],
            )
        if kind == "minecraft:killed_by_player":
            expect_keys(value, {"type"}, self.where)
            return Atom("Condition::KilledByPlayer")
        if kind == "minecraft:damage_source_properties":
            expect_keys(value, {"type", "predicate"}, self.where)
            predicate = self.damage_source_predicate(value.get("predicate", {}))
            return rs_call("Condition::DamageSourceProperties", [rs_ref(predicate)])
        if kind == "minecraft:random_chance":
            expect_keys(value, {"type", "chance"}, self.where)
            return rs_call("Condition::RandomChance", [self.float_constant(value["chance"])])
        if kind == "minecraft:random_chance_with_enchanted_bonus":
            expect_keys(value, {"type", "enchantment", "unenchanted_chance", "enchanted_chance"}, self.where)
            enchantment = normalize_id(value["enchantment"])
            if enchantment not in self.enchantments:
                fail("unknown enchantment %s in %s" % (enchantment, self.where))
            return rs_struct(
                "Condition::RandomChanceWithEnchantedBonus",
                [
                    ("enchantment", rs_str(enchantment)),
                    ("unenchanted_chance", rs_f32(value["unenchanted_chance"])),
                    ("enchanted_chance", self.level_based_value(value["enchanted_chance"])),
                ],
            )
        if kind == "minecraft:table_bonus":
            expect_keys(value, {"type", "enchantment", "chances"}, self.where)
            enchantment = normalize_id(value["enchantment"])
            assert enchantment in self.enchantments, (self.where, enchantment)
            assert value["chances"], self.where
            return rs_struct(
                "Condition::TableBonus",
                [
                    ("enchantment", rs_str(enchantment)),
                    ("chances", rs_slice([rs_f32(c) for c in value["chances"]])),
                ],
            )
        if kind == "minecraft:inverted":
            expect_keys(value, {"type", "term"}, self.where)
            return rs_call("Condition::Inverted", [rs_ref(self.condition(value["term"]))])
        if kind in ("minecraft:any_of", "minecraft:all_of"):
            expect_keys(value, {"type", "terms"}, self.where)
            variant = "Condition::AnyOf" if kind == "minecraft:any_of" else "Condition::AllOf"
            return rs_call(variant, [rs_slice([self.condition(term) for term in value["terms"]])])
        fail("unsupported condition %s in %s" % (json.dumps(value), self.where))

    def optional_condition(self, owner):
        return rs_option(None if "condition" not in owner else self.condition(owner["condition"]))

    def bonus_formula(self, value):
        formula = value["formula"]
        parameters = value.get("parameters")
        if formula == "minecraft:ore_drops":
            assert parameters is None, self.where
            return Atom("BonusFormula::OreDrops")
        if formula == "minecraft:uniform_bonus_count":
            expect_keys(parameters, {"bonusMultiplier"}, self.where)
            return rs_struct(
                "BonusFormula::UniformBonusCount",
                [("bonus_multiplier", rs_int(parameters["bonusMultiplier"]))],
            )
        if formula == "minecraft:binomial_with_bonus_count":
            expect_keys(parameters, {"extra", "probability"}, self.where)
            return rs_struct(
                "BonusFormula::BinomialWithBonusCount",
                [
                    ("extra", rs_int(parameters["extra"])),
                    ("probability", rs_f32(parameters["probability"])),
                ],
            )
        fail("unsupported bonus formula %s in %s" % (formula, self.where))

    def function(self, value):
        assert isinstance(value, dict), (self.where, value)
        kind = value.get("type")
        if kind == "minecraft:set_count":
            expect_keys(value, {"type", "condition", "count", "add"}, self.where)
            body = rs_struct(
                "FunctionKind::SetCount",
                [("count", self.int_provider(value["count"])), ("add", rs_bool(value.get("add", False)))],
            )
        elif kind == "minecraft:apply_bonus":
            expect_keys(value, {"type", "condition", "enchantment", "formula", "parameters"}, self.where)
            enchantment = normalize_id(value["enchantment"])
            assert enchantment in self.enchantments, (self.where, enchantment)
            body = rs_struct(
                "FunctionKind::ApplyBonus",
                [("enchantment", rs_str(enchantment)), ("formula", self.bonus_formula(value))],
            )
        elif kind == "minecraft:limit_count":
            expect_keys(value, {"type", "condition", "limit"}, self.where)
            limit = value["limit"]
            expect_keys(limit, {"min", "max"}, self.where)
            body = rs_struct(
                "FunctionKind::LimitCount",
                [
                    ("min", rs_option(None if "min" not in limit else self.int_provider(limit["min"]))),
                    ("max", rs_option(None if "max" not in limit else self.int_provider(limit["max"]))),
                ],
            )
        elif kind == "minecraft:explosion_decay":
            expect_keys(value, {"type", "condition"}, self.where)
            body = Atom("FunctionKind::ExplosionDecay")
        elif kind == "minecraft:enchanted_count_increase":
            expect_keys(value, {"type", "condition", "enchantment", "count", "limit"}, self.where)
            enchantment = normalize_id(value["enchantment"])
            if enchantment not in self.enchantments:
                fail("unknown enchantment %s in %s" % (enchantment, self.where))
            body = rs_struct(
                "FunctionKind::EnchantedCountIncrease",
                [
                    ("enchantment", rs_str(enchantment)),
                    ("count", self.float_provider(value["count"])),
                    ("limit", rs_int(value.get("limit", 0))),
                ],
            )
        elif kind == "minecraft:furnace_smelt":
            expect_keys(value, {"type", "condition", "use_input_count"}, self.where)
            body = rs_struct(
                "FunctionKind::FurnaceSmelt",
                [("use_input_count", rs_bool(value.get("use_input_count", True)))],
            )
        elif kind == "minecraft:set_potion":
            expect_keys(value, {"type", "condition", "id"}, self.where)
            potion = normalize_id(value["id"])
            if potion not in self.ids["minecraft:potion"]:
                fail("unknown potion %s in %s" % (potion, self.where))
            body = rs_call("FunctionKind::SetPotion", [rs_str(potion)])
        elif kind == "minecraft:enchant_randomly":
            expect_keys(value, {"type", "condition", "options", "only_compatible"}, self.where)
            options = value.get("options")
            if not isinstance(options, str) or options.startswith("#"):
                fail("unsupported enchant_randomly options in %s" % self.where)
            enchantment = normalize_id(options)
            if enchantment not in self.enchantments:
                fail("unknown enchantment %s in %s" % (enchantment, self.where))
            max_level = enchantment_max_level(enchantment)
            body = rs_struct(
                "FunctionKind::EnchantRandomly",
                [("enchantment", rs_str(enchantment)), ("max_level", rs_int(max_level))],
            )
        elif kind == "minecraft:set_ominous_bottle_amplifier":
            expect_keys(value, {"type", "condition", "amplifier"}, self.where)
            body = rs_call("FunctionKind::SetOminousBottleAmplifier", [self.int_provider(value["amplifier"])])
        elif kind in COMPONENT_FUNCTIONS:
            self.note_ignored(kind)
            body = rs_call("FunctionKind::Components", [rs_str(kind)])
        else:
            fail("unsupported function %s in %s" % (json.dumps(value), self.where))
        return rs_struct("Function", [("condition", self.optional_condition(value)), ("kind", body)])

    def modifiers(self, owner):
        value = owner.get("modifier")
        if value is None:
            return rs_slice([])
        values = value if isinstance(value, list) else [value]
        return rs_slice([self.function(item) for item in values])

    def leaf(self, value, output):
        return rs_call(
            "EntryKind::Leaf",
            [
                rs_struct(
                    "Leaf",
                    [
                        ("weight", rs_int(value.get("weight", 1))),
                        ("quality", rs_int(value.get("quality", 0))),
                        ("output", output),
                    ],
                )
            ],
        )

    def entry(self, value):
        kind = value.get("type")
        leaf_keys = {"type", "condition", "modifier", "weight", "quality"}
        if kind == "minecraft:item":
            expect_keys(value, leaf_keys | {"name"}, self.where)
            name = normalize_id(value["name"])
            if name not in self.item_ids:
                fail("unknown item %s in %s" % (name, self.where))
            body = self.leaf(value, rs_call("Output::Item", [Atom(str(self.item_ids[name]))]))
        elif kind == "minecraft:tag":
            expect_keys(value, leaf_keys | {"items", "expand"}, self.where)
            body = self.leaf(
                value,
                rs_struct(
                    "Output::Items",
                    [
                        ("items", self.holder_set(value["items"], "minecraft:item")),
                        ("expand", rs_bool(value.get("expand", False))),
                    ],
                ),
            )
        elif kind == "minecraft:empty":
            expect_keys(value, leaf_keys, self.where)
            body = self.leaf(value, Atom("Output::Empty"))
        elif kind == "minecraft:dynamic":
            expect_keys(value, leaf_keys | {"name"}, self.where)
            self.note_ignored("dynamic")
            body = self.leaf(value, rs_call("Output::Dynamic", [rs_str(normalize_id(value["name"]))]))
        elif kind == "minecraft:loot_table":
            expect_keys(value, leaf_keys | {"value"}, self.where)
            if not isinstance(value["value"], str):
                fail("inline nested loot table in %s" % self.where)
            name = normalize_id(value["value"])
            self.tables.add(name)
            body = self.leaf(value, rs_call("Output::Table", [rs_str(name)]))
        elif kind in COMPOSITE_ENTRIES:
            expect_keys(value, {"type", "condition", "modifier", "children"}, self.where)
            children = rs_slice([self.entry(child) for child in value["children"]])
            body = rs_call("EntryKind::" + COMPOSITE_ENTRIES[kind], [children])
        else:
            fail("unsupported entry %s in %s" % (json.dumps(value), self.where))
        return rs_struct(
            "Entry",
            [
                ("condition", self.optional_condition(value)),
                ("modifier", self.modifiers(value)),
                ("kind", body),
            ],
        )

    def pool(self, value):
        expect_keys(value, {"rolls", "bonus_rolls", "condition", "modifier", "entries"}, self.where)
        return rs_struct(
            "Pool",
            [
                ("rolls", self.int_provider(value["rolls"])),
                ("bonus_rolls", self.float_constant(value.get("bonus_rolls", 0.0))),
                ("condition", self.optional_condition(value)),
                ("modifier", self.modifiers(value)),
                ("entries", rs_slice([self.entry(entry) for entry in value["entries"]])),
            ],
        )

    def table(self, value, allowed):
        expect_keys(value, {"type", "pools", "modifier", "random_sequence"}, self.where)
        if value["type"] not in allowed:
            fail("unexpected loot table type %s in %s" % (value["type"], self.where))
        return rs_struct(
            "LootTable",
            [
                ("pools", rs_slice([self.pool(pool) for pool in value.get("pools", [])])),
                ("modifier", self.modifiers(value)),
            ],
        )


def load_block_properties(reports):
    blocks = load_json(os.path.join(reports, "block_properties.json"))["blocks"]
    assert [block["id"] for block in blocks] == list(range(len(blocks)))
    return blocks


def build_loot(datapack, block_reports, static_registries, synced, tag_table):
    static_ids = {
        registry_id: {name: i for i, name in enumerate(by_id)}
        for registry_id, by_id, _ in static_registries
    }
    block_ids = static_ids["minecraft:block"]
    item_ids = static_ids["minecraft:item"]
    synced_names = {rid: entries for _, rid, entries in synced}
    assert [block["name"] for block in block_reports] == sorted(block_ids, key=block_ids.get)
    block_properties = {
        block["name"]: {p["name"]: p["values"] for p in block["properties"]} for block in block_reports
    }
    dyes = {name[len("minecraft:") : -len("_dye")] for name in item_ids if name.endswith("_dye")}
    assert len(dyes) == 16, sorted(dyes)
    ids = dict(static_ids)
    ids["dye"] = dyes
    references = set()
    ignored = {}

    def builder(where):
        return LootBuilder(ids, synced_names, block_properties, tag_table, where)

    tables = []
    pending = set()

    def add_table(name, path, allowed):
        current = builder(name)
        node = current.table(load_json(path), allowed)
        nonlocal references
        references |= current.references
        pending.update(current.tables)
        for construct, owners in current.ignored.items():
            ignored.setdefault(construct, set()).update(owners)
        tables.append((name, node))

    for directory in sorted(TABLE_TYPES):
        for path in json_names(os.path.join(datapack, "loot_table", directory)):
            assert NAME_RE.match(path), path
            name = "minecraft:%s/%s" % (directory, path)
            add_table(name, os.path.join(datapack, "loot_table", directory, path + ".json"), {TABLE_TYPES[directory]})
    for name, kind in EXTRA_TABLES:
        add_table(name, os.path.join(datapack, "loot_table", name.split(":", 1)[1] + ".json"), {kind})
    while True:
        emitted = {name for name, _ in tables}
        missing_tables = sorted(pending - emitted)
        if not missing_tables:
            break
        for name in missing_tables:
            path = os.path.join(datapack, "loot_table", name.split(":", 1)[1] + ".json")
            if not os.path.exists(path):
                fail("unresolved loot table reference " + name)
            add_table(name, path, REFERENCED_TABLE_TYPES)
    tables.sort(key=lambda pair: pair[0])

    predicates = []
    for path in json_names(os.path.join(datapack, "predicate")):
        name = "minecraft:" + path
        current = builder(name)
        node = current.condition(load_json(os.path.join(datapack, "predicate", path + ".json")))
        references |= current.references
        predicates.append((name, node, current.references))
    predicate_names = {name for name, _, _ in predicates}
    missing = references - predicate_names
    if missing:
        fail("unresolved predicate references: " + ", ".join(sorted(missing)))
    graph = {name: refs for name, _, refs in predicates}

    def check_cycle(name, stack):
        assert name not in stack, ("predicate cycle", name)
        for child in graph[name]:
            check_cycle(child, stack | {name})

    for name in graph:
        check_cycle(name, frozenset())

    table_index = {name: index for index, (name, _) in enumerate(tables)}
    assert len(tables) < 65535
    block_tables = []
    wall_mapped = []
    for block in block_reports:
        path = block["name"].split(":", 1)[1]
        own = table_index.get("minecraft:blocks/" + path)
        if own is None:
            floors = [
                other
                for other in block_reports
                if other["wall_variant"] == block["id"]
                and other["id"] != block["id"]
                and "minecraft:blocks/" + other["name"].split(":", 1)[1] in table_index
            ]
            assert len(floors) <= 1, (block["name"], floors)
            if floors:
                own = table_index["minecraft:blocks/" + floors[0]["name"].split(":", 1)[1]]
                wall_mapped.append(block["name"])
        block_tables.append(65535 if own is None else own)
    entity_names = sorted(static_ids["minecraft:entity_type"], key=static_ids["minecraft:entity_type"].get)
    entity_tables = [
        table_index.get("minecraft:entities/" + name.split(":", 1)[1], 65535) for name in entity_names
    ]
    return tables, block_tables, entity_tables, predicates, wall_mapped, ignored


def ingredient_items(value, item_ids, tag_table, where):
    if isinstance(value, list):
        result = set()
        for entry in value:
            result |= ingredient_items(entry, item_ids, tag_table, where)
        return result
    if not isinstance(value, str):
        fail("unsupported ingredient %s in %s" % (json.dumps(value), where))
    if value.startswith("#"):
        tag = normalize_id(value[1:])
        if tag not in tag_table["minecraft:item"]:
            fail("unknown item tag %s in %s" % (tag, where))
        return set(tag_table["minecraft:item"][tag])
    name = normalize_id(value)
    if name not in item_ids:
        fail("unknown item %s in %s" % (name, where))
    return {item_ids[name]}


def build_smelting(datapack, static_registries, tag_table):
    item_ids = {
        name: i for registry_id, by_id, _ in static_registries if registry_id == "minecraft:item" for i, name in enumerate(by_id)
    }
    root = os.path.join(datapack, "recipe")
    results = {}
    for path in json_names(root):
        recipe = load_json(os.path.join(root, path + ".json"))
        if recipe.get("type") != "minecraft:smelting":
            continue
        where = "recipe " + path
        expect_keys(recipe, {"type", "category", "group", "cookingtime", "experience", "ingredient", "result"}, where)
        result = recipe["result"]
        expect_keys(result, {"id", "count"}, where)
        if result.get("count", 1) != 1:
            fail("smelting result count %r in %s" % (result["count"], where))
        output = normalize_id(result["id"])
        if output not in item_ids:
            fail("unknown item %s in %s" % (output, where))
        for item in ingredient_items(recipe["ingredient"], item_ids, tag_table, where):
            previous = results.get(item)
            if previous is not None and previous != item_ids[output]:
                fail("item %d smelts into two results in %s" % (item, where))
            results[item] = item_ids[output]
    return sorted(results.items())


SPAWN_CATEGORIES = [
    "monster",
    "creature",
    "ambient",
    "axolotls",
    "underground_water_creature",
    "water_creature",
    "water_ambient",
    "misc",
]
NATURAL_MOB_SPAWNS = "minecraft:gameplay/natural_mob_spawns"
CREATURE_PROBABILITY = "minecraft:gameplay/creature_world_gen_spawn_probability"
DEFAULT_CREATURE_PROBABILITY = 0.1


def verify_spawn_defaults_against_java(javap, classes):
    if not os.path.exists(javap):
        return False
    lines = java_lines(javap, classes, "net.minecraft.world.attribute.EnvironmentAttributes")
    for index, line in enumerate(lines):
        if line.endswith("// String " + CREATURE_PROBABILITY.split(":", 1)[1]):
            for later in lines[index + 1 : index + 6]:
                match = re.search(r"// float ([0-9.eE+-]+)f$", later)
                if match:
                    assert to_f32(float(match.group(1))) == to_f32(DEFAULT_CREATURE_PROBABILITY), later
                    return True
            fail("creature spawn probability default not found after its name")
    fail(CREATURE_PROBABILITY + " not registered in EnvironmentAttributes")


def int_provider(value, where):
    if isinstance(value, int) and not isinstance(value, bool):
        return rs_call("SpawnCount::Constant", [rs_int(value)])
    expect_keys(value, ["type", "min_inclusive", "max_inclusive"], where)
    if value.get("type") != "minecraft:uniform":
        fail("unsupported int provider %s in %s" % (json.dumps(value), where))
    low, high = value["min_inclusive"], value["max_inclusive"]
    assert low <= high, (where, value)
    return rs_struct("SpawnCount::Uniform", [("min", rs_int(low)), ("max", rs_int(high))])


def build_biome_spawns(datapack, static_registries, synced):
    entity_ids = {}
    for registry_id, by_id, _ in static_registries:
        if registry_id == "minecraft:entity_type":
            entity_ids = {name: index for index, name in enumerate(by_id)}
    biomes = next(entries for constant, _, entries in synced if constant == "BIOME")
    table = []
    for biome in biomes:
        where = biome
        path = os.path.join(datapack, "worldgen", "biome", biome.split(":", 1)[1] + ".json")
        attributes = load_json(path).get("attributes", {})
        probability = attributes.get(CREATURE_PROBABILITY, DEFAULT_CREATURE_PROBABILITY)
        if isinstance(probability, dict):
            fail("modified creature spawn probability in " + where)
        categories = {category: [] for category in SPAWN_CATEGORIES}
        costs = []
        spawns = attributes.get(NATURAL_MOB_SPAWNS)
        if spawns is not None:
            expect_keys(spawns, ["argument", "modifier"], where)
            if spawns["modifier"] != "overlay":
                fail("unsupported natural mob spawns modifier %s in %s" % (spawns["modifier"], where))
            argument = spawns["argument"]
            expect_keys(argument, ["spawns_by_category", "spawn_costs"], where)
            for category, entries in argument.get("spawns_by_category", {}).items():
                if category not in categories:
                    fail("unknown mob category %s in %s" % (category, where))
                for entry in entries:
                    expect_keys(entry, ["type", "count", "weight"], where)
                    entity = normalize_id(entry["type"])
                    if entity not in entity_ids:
                        fail("unknown entity %s in %s" % (entity, where))
                    weight = entry["weight"]
                    assert isinstance(weight, int) and weight > 0, (where, entry)
                    categories[category].append(
                        rs_struct(
                            "Spawner",
                            [
                                ("entity", rs_int(entity_ids[entity])),
                                ("weight", rs_int(weight)),
                                ("count", int_provider(entry["count"], where)),
                            ],
                        )
                    )
            for entity, cost in sorted(argument.get("spawn_costs", {}).items()):
                expect_keys(cost, ["charge", "energy_budget"], where)
                entity = normalize_id(entity)
                if entity not in entity_ids:
                    fail("unknown entity %s in %s spawn costs" % (entity, where))
                costs.append(
                    rs_struct(
                        "SpawnCost",
                        [
                            ("entity", rs_int(entity_ids[entity])),
                            ("energy_budget", rs_f64(cost["energy_budget"])),
                            ("charge", rs_f64(cost["charge"])),
                        ],
                    )
                )
        fields = [("creature_probability", rs_f32(probability))]
        fields.extend((category, rs_slice(categories[category])) for category in SPAWN_CATEGORIES)
        fields.append(("costs", rs_slice(costs)))
        table.append(rs_struct("BiomeSpawns", fields))
    return table


def build_dimension_spawns(datapack):
    rows = []
    directory = os.path.join(datapack, "dimension_type")
    for name in json_names(directory):
        data = load_json(os.path.join(directory, name + ".json"))
        where = "minecraft:" + name
        limit = data["monster_spawn_block_light_limit"]
        assert isinstance(limit, int) and 0 <= limit <= 15, (where, limit)
        rows.append(
            rs_tuple(
                [
                    rs_str(where),
                    rs_struct(
                        "DimensionSpawns",
                        [
                            ("monster_spawn_light", int_provider(data["monster_spawn_light_level"], where)),
                            ("monster_spawn_block_light_limit", rs_int(limit)),
                            ("ambient_light", rs_f32(data["ambient_light"])),
                            ("has_ceiling", rs_bool(data["has_ceiling"])),
                        ],
                    ),
                ]
            )
        )
    return rows


def emit_spawns(biome_spawns, dimension_spawns):
    body = ["pub(crate) static BIOME_SPAWNS: [BiomeSpawns; %d] = [" % len(biome_spawns)]
    for node in biome_spawns:
        body.extend(render(node, "    ", "", ","))
    body.append("];")
    body.append("")
    body.append("pub(crate) static DIMENSION_SPAWNS: [(&str, DimensionSpawns); %d] = [" % len(dimension_spawns))
    for node in dimension_spawns:
        body.extend(render(node, "    ", "", ","))
    body.append("];")
    text = "\n".join(body) + "\n"
    names = ["BiomeSpawns", "DimensionSpawns", "SpawnCost", "SpawnCount", "Spawner"]
    return "use crate::spawns::{%s};\n\n" % ", ".join(used_imports(text, names)) + text


def text_str(value):
    return '"%s"' % value


def biome_holder_set(value, synced_names, tag_table, where):
    if isinstance(value, str) and value.startswith("#"):
        tag = normalize_id(value[1:])
        if tag not in tag_table["minecraft:worldgen/biome"]:
            fail("unknown biome tag %s in %s" % (tag, where))
        return "HolderSet::Tag(%s)" % text_str(tag)
    names = value if isinstance(value, list) else [value]
    biomes = synced_names["minecraft:worldgen/biome"]
    ids = []
    for name in names:
        name = normalize_id(name)
        if name not in biomes:
            fail("unknown biome %s in %s" % (name, where))
        ids.append(biomes.index(name))
    return "HolderSet::Direct(&[%s])" % ", ".join(str(i) for i in sorted(ids))


def moon_bound(range_value, key):
    if key not in range_value:
        return "None"
    return "Some(%s)" % rs_f32(range_value[key]).text


def spawn_condition(condition, synced_names, tag_table, datapack, where):
    if condition is None:
        return "None"
    kind = condition.get("type")
    if kind == "minecraft:biome":
        expect_keys(condition, {"type", "biomes"}, where)
        biomes = biome_holder_set(condition["biomes"], synced_names, tag_table, where)
        return "Some(VariantCondition::Biome(%s))" % biomes
    if kind == "minecraft:structure":
        expect_keys(condition, {"type", "structures"}, where)
        structures = condition["structures"]
        if not (isinstance(structures, str) and structures.startswith("#")):
            fail("unsupported structure set in %s: %s" % (where, json.dumps(structures)))
        tag = normalize_id(structures[1:])
        if not os.path.exists(os.path.join(datapack, "tags", "worldgen", "structure", tag.split(":", 1)[1] + ".json")):
            fail("unknown structure tag %s in %s" % (tag, where))
        return "Some(VariantCondition::Structure(%s))" % text_str(tag)
    if kind == "minecraft:moon_brightness":
        expect_keys(condition, {"type", "range"}, where)
        bounds = condition["range"]
        expect_keys(bounds, {"min", "max"}, where)
        return "Some(VariantCondition::MoonBrightness { min: %s, max: %s })" % (
            moon_bound(bounds, "min"),
            moon_bound(bounds, "max"),
        )
    fail("unsupported spawn condition %s in %s" % (kind, where))


def build_variants(datapack, synced, tag_table):
    synced_names = {registry_id: entries for _, registry_id, entries in synced}
    mobs = []
    for constant, path, keys in MOB_VARIANTS:
        entries = []
        for name in synced_names["minecraft:" + path]:
            where = "%s %s" % (path, name)
            value = load_json(os.path.join(datapack, path, name.split(":", 1)[1] + ".json"))
            expect_keys(value, keys, where)
            conditions = []
            for entry in value.get("spawn_conditions", []):
                expect_keys(entry, {"condition", "priority"}, where)
                condition = spawn_condition(entry.get("condition"), synced_names, tag_table, datapack, where)
                conditions.append((condition, entry["priority"]))
            entries.append((name, conditions))
        mobs.append((constant, entries))
    sounds = []
    for constant, path, keys in SOUND_VARIANTS:
        entries = []
        allowed = {key + "_sound" for key in keys}
        for name in synced_names["minecraft:" + path]:
            where = "%s %s" % (path, name)
            value = load_json(os.path.join(datapack, path, name.split(":", 1)[1] + ".json"))
            if "adult_sounds" in value:
                expect_keys(value, {"adult_sounds", "baby_sounds"}, where)
                adult, baby = value["adult_sounds"], value["baby_sounds"]
            else:
                adult, baby = value, value
            sets = []
            for sound_set in (adult, baby):
                expect_keys(sound_set, allowed, where)
                fields = []
                for key in SOUND_SET_FIELDS:
                    sound = sound_set.get(key + "_sound") if key in keys else None
                    fields.append("%s: %s" % (key, "None" if sound is None else "Some(%s)" % text_str(normalize_id(sound))))
                sets.append(fields)
            entries.append((name, sets))
        sounds.append((constant, entries))
    return mobs, sounds


def build_dye_mixes(datapack):
    root = os.path.join(datapack, "recipe")
    dyes = {"minecraft:%s_dye" % color: index for index, color in enumerate(DYE_COLORS)}
    mixes = {}
    for path in json_names(root):
        recipe = load_json(os.path.join(root, path + ".json"))
        if recipe.get("type") != "minecraft:crafting_shapeless":
            continue
        ingredients = recipe.get("ingredients", [])
        result = normalize_id(recipe.get("result", {}).get("id", ""))
        if len(ingredients) != 2 or result not in dyes:
            continue
        if not all(isinstance(i, str) and normalize_id(i) in dyes for i in ingredients):
            continue
        first, second = (dyes[normalize_id(i)] for i in ingredients)
        for pair in ((first, second), (second, first)):
            previous = mixes.get(pair)
            if previous is not None and previous != dyes[result]:
                fail("dyes %r mix into two colours in recipe %s" % (pair, path))
            mixes[pair] = dyes[result]
    return sorted((a, b, c) for (a, b), c in mixes.items())


def emit_variants(mobs, sounds, mixes):
    out = []
    for constant, entries in mobs:
        out.append("pub(crate) static %s: [MobVariant; %d] = [" % (constant, len(entries)))
        for name, conditions in entries:
            out.append("    MobVariant {")
            out.append("        name: %s," % text_str(name))
            out.append("        spawn_conditions: &[")
            for condition, priority in conditions:
                out.append("            SpawnCondition {")
                out.append("                condition: %s," % condition)
                out.append("                priority: %d," % priority)
                out.append("            },")
            out.append("        ],")
            out.append("    },")
        out.append("];")
        out.append("")
    for constant, entries in sounds:
        out.append("pub(crate) static %s: [SoundVariant; %d] = [" % (constant, len(entries)))
        for name, (adult, baby) in entries:
            out.append("    SoundVariant {")
            out.append("        name: %s," % text_str(name))
            for label, fields in (("adult", adult), ("baby", baby)):
                out.append("        %s: SoundSet {" % label)
                out.extend("            %s," % field for field in fields)
                out.append("        },")
            out.append("    },")
        out.append("];")
        out.append("")
    out.append("pub(crate) static DYE_MIXES: [(u8, u8, u8); %d] = [" % len(mixes))
    out.extend(wrapped(["(%d, %d, %d)" % mix for mix in mixes], "    "))
    out.append("];")
    text = "\n".join(out) + "\n"
    names = ["MobVariant", "SoundSet", "SoundVariant", "SpawnCondition", "VariantCondition"]
    head = "use crate::tags::HolderSet;\nuse crate::variants::{%s};\n\n" % ", ".join(used_imports(text, names))
    return head + text


def build_compostables(reports, datapack, static_registries):
    item_ids = {
        name: i for registry_id, by_id, _ in static_registries if registry_id == "minecraft:item" for i, name in enumerate(by_id)
    }
    provider_root = os.path.join(datapack, "context_int_provider", "compostable")
    providers = {}
    for name in json_names(provider_root):
        path = os.path.join(provider_root, name + ".json")
        value = load_json(path)
        key = "minecraft:compostable/" + name
        if value == 1:
            providers[key] = (1, 0)
            continue
        where = "context int provider " + key
        expect_keys(value, {"type", "cases", "default"}, where)
        if value["type"] != "minecraft:number_dispatcher" or len(value["cases"]) != 1:
            fail("unexpected shape of %s" % where)
        case = value["cases"][0]
        expect_keys(case, {"condition", "value"}, where)
        condition = case["condition"]
        if (
            case["value"] != 1
            or condition.get("type") != "minecraft:match_block"
            or condition.get("blocks") != "minecraft:composter"
            or condition.get("state") != {"level": "0"}
        ):
            fail("unexpected first case of %s" % where)
        default = value["default"]
        if default.get("type") != "minecraft:weighted_list":
            fail("unexpected default of %s" % where)
        if [entry["data"] for entry in default["distribution"]] != [1, 0]:
            fail("unexpected distribution order of %s" % where)
        weights = {entry["data"]: entry["weight"] for entry in default["distribution"]}
        if set(weights) != {0, 1}:
            fail("unexpected distribution of %s" % where)
        providers[key] = (weights[1], weights[0])
    names = sorted(providers)
    items_root = os.path.join(reports, "minecraft", "components", "item")
    entries = []
    for name in json_names(items_root):
        component = load_json(os.path.join(items_root, name + ".json"))["components"].get("minecraft:compostable")
        if component is None:
            continue
        expect_keys(component, {"layers"}, "compostable " + name)
        layers = component["layers"]
        if layers not in providers:
            fail("unknown compostable layers %s on %s" % (layers, name))
        item = "minecraft:" + name
        if item not in item_ids:
            fail("unknown compostable item %s" % item)
        entries.append((item_ids[item], names.index(layers)))
    entries.sort()
    return [(name, providers[name]) for name in names], entries


def emit_compostables(providers, entries):
    out = ["pub(crate) static LAYER_PROVIDERS: [(&str, u32, u32); %d] = [" % len(providers)]
    for name, (one, zero) in providers:
        out.append('    ("%s", %d, %d),' % (name, one, zero))
    out.append("];")
    out.append("")
    out.append("pub(crate) static COMPOSTABLES: [(u16, u8); %d] = [" % len(entries))
    out.extend(wrapped(["(%d, %d)" % entry for entry in entries], "    "))
    out.append("];")
    return "\n".join(out) + "\n"


def build_climate(datapack, synced):
    names = {registry_id: entries for _, registry_id, entries in synced}["minecraft:worldgen/biome"]
    climate = []
    for name in names:
        value = load_json(os.path.join(datapack, "worldgen", "biome", name.split(":", 1)[1] + ".json"))
        modifier = value.get("temperature_modifier", "none")
        if modifier not in ("none", "frozen"):
            fail("unknown temperature modifier %s in biome %s" % (modifier, name))
        climate.append((bool(value["has_precipitation"]), float(value["temperature"]), modifier == "frozen"))
    return climate


def emit_climate(climate):
    out = ["use crate::climate::Climate;", ""]
    out.append("pub(crate) static CLIMATE: [Climate; %d] = [" % len(climate))
    for precipitation, temperature, frozen in climate:
        out.append("    Climate {")
        out.append("        has_precipitation: %s," % ("true" if precipitation else "false"))
        out.append("        temperature: %s," % flat(rs_f32(temperature)))
        out.append("        frozen: %s," % ("true" if frozen else "false"))
        out.append("    },")
    out.append("];")
    return "\n".join(out) + "\n"


def used_imports(text, names):
    return [name for name in names if re.search(r"\b" + name + r"\b", text)]


def emit_loot(tables, block_tables, entity_tables, predicates, smelting):
    body = ["pub(crate) static TABLES: [(&str, LootTable); %d] = [" % len(tables)]
    for name, node in tables:
        body.extend(render(rs_tuple([rs_str(name), node]), "    ", "", ","))
    body.append("];")
    body.append("")
    body.append("pub(crate) static BLOCK_TABLES: [u16; %d] = [" % len(block_tables))
    body.extend(wrapped([str(index) for index in block_tables], "    "))
    body.append("];")
    body.append("")
    body.append("pub(crate) static ENTITY_TABLES: [u16; %d] = [" % len(entity_tables))
    body.extend(wrapped([str(index) for index in entity_tables], "    "))
    body.append("];")
    body.append("")
    body.append("pub(crate) static SMELTING: [(u16, u16); %d] = [" % len(smelting))
    body.extend(wrapped(["(%d, %d)" % pair for pair in smelting], "    "))
    body.append("];")
    body.append("")
    body.append("pub(crate) static PREDICATES: [(&str, Condition); %d] = [" % len(predicates))
    for name, node, _ in predicates:
        body.extend(render(rs_tuple([rs_str(name), node]), "    ", "", ","))
    body.append("];")
    text = "\n".join(body) + "\n"
    head = ["use crate::loot::{%s};" % ", ".join(used_imports(text, LOOT_TYPES))]
    if "HolderSet::" in text:
        head.append("use crate::tags::HolderSet;")
    return "\n".join(head) + "\n\n" + text


TRANSFORMER_TOOLS = ["axe", "hoe", "shovel"]
TRANSFORMER_GROUP_KEYS = [
    "block_state_provider",
    "item_damage_per_use",
    "sound",
    "particle",
    "transform_type",
    "update_from_neighbors",
    "disallowed_faces",
    "drop_strategy",
    "loot",
]
TRANSFORMER_FACES = ["down", "up", "north", "south", "west", "east"]


def parse_condition(node, where):
    expect_keys(node, ["type", "blocks", "tag", "offset", "predicates"], where)
    kind = node.get("type")
    if kind == "minecraft:matching_blocks":
        expect_keys(node, ["type", "blocks"], where)
        assert isinstance(node["blocks"], str) and not node["blocks"].startswith("#"), (where, node)
        return ("block", node["blocks"])
    if kind == "minecraft:matching_block_tag":
        expect_keys(node, ["type", "tag", "offset"], where)
        offset = node.get("offset", [0, 0, 0])
        assert isinstance(offset, list) and len(offset) == 3, (where, node)
        assert isinstance(node["tag"], str) and not node["tag"].startswith("#"), (where, node)
        return ("tag", node["tag"], tuple(offset))
    if kind == "minecraft:all_of":
        expect_keys(node, ["type", "predicates"], where)
        assert node["predicates"], (where, node)
        return ("all", [parse_condition(item, where) for item in node["predicates"]])
    fail("unsupported predicate %s in %s" % (kind, where))


def parse_then(node, where):
    if node.get("type") == "minecraft:copy_properties":
        expect_keys(node, ["type", "source"], where)
        source = node["source"]
        expect_keys(source, ["id", "properties"], where)
        return ("copy", source["id"], sorted((source.get("properties") or {}).items()))
    expect_keys(node, ["id", "properties"], where)
    return ("set", node["id"], sorted((node.get("properties") or {}).items()))


def build_transformers(datapack):
    tools = []
    for tool in TRANSFORMER_TOOLS:
        path = os.path.join(datapack, "block_transformer", tool + ".json")
        groups = load_json(path)
        assert isinstance(groups, list) and groups, path
        out = []
        for index, group in enumerate(groups):
            where = "block_transformer/%s.json group %d" % (tool, index)
            expect_keys(group, TRANSFORMER_GROUP_KEYS, where)
            provider = group["block_state_provider"]
            expect_keys(provider, ["type", "rules"], where)
            assert provider["type"] == "minecraft:rule_based", (where, provider["type"])
            rules = []
            for rule in provider["rules"]:
                expect_keys(rule, ["if_true", "then"], where)
                rules.append((parse_condition(rule["if_true"], where), parse_then(rule["then"], where)))
            assert rules, where
            faces = group.get("disallowed_faces", [])
            assert all(face in TRANSFORMER_FACES for face in faces), (where, faces)
            out.append(
                {
                    "sound": group["sound"],
                    "particle": group.get("particle"),
                    "transform_type": group.get("transform_type"),
                    "update_from_neighbors": group.get("update_from_neighbors", True),
                    "damage": group["item_damage_per_use"],
                    "disallowed_faces": faces,
                    "drop_strategy": group.get("drop_strategy"),
                    "loot": group.get("loot"),
                    "rules": rules,
                }
            )
        tools.append((tool, out))
    return tools


def emit_transformers(tools):
    def condition_node(condition):
        if condition[0] == "block":
            return rs_call("Condition::Block", [rs_str(condition[1])])
        if condition[0] == "tag":
            return rs_struct(
                "Condition::Tag",
                [
                    ("tag", rs_str(condition[1])),
                    ("offset", rs_tuple([rs_int(value) for value in condition[2]])),
                ],
            )
        return rs_call("Condition::AllOf", [rs_slice([condition_node(item) for item in condition[1]])])

    def then_node(then):
        pairs = rs_slice([rs_tuple([rs_str(key), rs_str(value)]) for key, value in then[2]])
        if then[0] == "copy":
            return rs_struct("Then::Copy", [("to", rs_str(then[1])), ("defaults", pairs)])
        return rs_struct("Then::Set", [("to", rs_str(then[1])), ("properties", pairs)])

    def optional(value):
        return rs_option(None if value is None else rs_str(value))

    def group_node(group):
        return rs_struct(
            "Group",
            [
                ("sound", rs_str(group["sound"])),
                ("particle", optional(group["particle"])),
                ("transform_type", optional(group["transform_type"])),
                ("update_from_neighbors", rs_bool(group["update_from_neighbors"])),
                ("damage", rs_int(group["damage"])),
                ("disallowed_faces", rs_slice([rs_str(face) for face in group["disallowed_faces"]])),
                ("drop_strategy", optional(group["drop_strategy"])),
                ("loot", optional(group["loot"])),
                (
                    "rules",
                    rs_slice(
                        [
                            rs_struct(
                                "Rule",
                                [("when", condition_node(rule[0])), ("then", then_node(rule[1]))],
                            )
                            for rule in group["rules"]
                        ]
                    ),
                ),
            ],
        )

    body = []
    for tool, groups in tools:
        body.append("pub(crate) static %s: [Group; %d] = [" % (tool.upper(), len(groups)))
        for group in groups:
            body.extend(render(group_node(group), "    ", "", ","))
        body.append("];")
        body.append("")
    return "use crate::transformers::{Condition, Group, Rule, Then};\n\n" + "\n".join(body)


TRADE_SET_KEYS = ["amount", "random_sequence", "trades", "allow_duplicates"]
TRADE_KEYS = [
    "wants",
    "additional_wants",
    "gives",
    "max_uses",
    "xp",
    "reputation_discount",
    "merchant_predicate",
    "given_item_modifier",
    "double_trade_price_enchantments",
]
TRADE_REQUIRES = {
    "minecraft:enchantments": "Enchantments",
    "minecraft:stored_enchantments": "StoredEnchantments",
    "minecraft:dyed_color": "DyedColor",
    "minecraft:map_id": "MapId",
}


def trade_tag_values(datapack, tag, stack=()):
    assert tag not in stack, tag
    path = os.path.join(datapack, "tags", "villager_trade", tag.split(":", 1)[1] + ".json")
    value = load_json(path)
    expect_keys(value, ["values", "replace"], path)
    out = []
    for entry in value["values"]:
        assert isinstance(entry, str), (path, entry)
        if entry.startswith("#"):
            for inner in trade_tag_values(datapack, normalize_id(entry[1:]), stack + (tag,)):
                if inner not in out:
                    out.append(inner)
        else:
            name = normalize_id(entry)
            if name not in out:
                out.append(name)
    return out


def trade_item(value, item_ids, where):
    name = normalize_id(value)
    if name not in item_ids:
        fail("unknown item %s in %s" % (name, where))
    return name


def trade_cost(value, item_ids, potions, where, default_count=1):
    expect_keys(value, ["id", "count", "components"], where)
    count = value.get("count", default_count)
    assert isinstance(count, int) and not isinstance(count, bool), (where, value)
    components = value.get("components")
    potion = None
    if components is not None:
        expect_keys(components, ["minecraft:potion_contents"], where)
        contents = components["minecraft:potion_contents"]
        expect_keys(contents, ["potion"], where)
        potion = normalize_id(contents["potion"])
        if potion not in potions:
            fail("unknown potion %s in %s" % (potion, where))
    return rs_struct(
        "TradeCost",
        [
            ("item", rs_str(trade_item(value["id"], item_ids, where))),
            ("count", rs_int(count)),
            ("potion", rs_option(None if potion is None else rs_str(potion))),
        ],
    )


def trade_tag_name(value, tags, where):
    assert isinstance(value, str) and value.startswith("#"), (where, value)
    name = normalize_id(value[1:])
    if name not in tags:
        fail("unknown enchantment tag %s in %s" % (name, where))
    return rs_str(name)


def trade_modifier(value, item_ids, potions, enchantment_tags, where):
    kind = value.get("type")
    if kind == "minecraft:enchant_randomly":
        expect_keys(value, ["type", "options", "only_compatible", "include_additional_cost_component"], where)
        return rs_struct(
            "Modifier::EnchantRandomly",
            [
                ("options", trade_tag_name(value["options"], enchantment_tags, where)),
                ("only_compatible", rs_bool(value.get("only_compatible", True))),
                ("additional_cost", rs_bool(value.get("include_additional_cost_component", False))),
            ],
        )
    if kind == "minecraft:enchant_with_levels":
        expect_keys(value, ["type", "options", "levels", "include_additional_cost_component"], where)
        levels = value["levels"]
        expect_keys(levels, ["type", "min", "max"], where)
        assert levels["type"] == "minecraft:uniform", where
        return rs_struct(
            "Modifier::EnchantWithLevels",
            [
                ("options", trade_tag_name(value["options"], enchantment_tags, where)),
                ("min", rs_int(levels["min"])),
                ("max", rs_int(levels["max"])),
                ("additional_cost", rs_bool(value.get("include_additional_cost_component", False))),
            ],
        )
    if kind == "minecraft:set_stew_effect":
        expect_keys(value, ["type", "effects"], where)
        entries = []
        for effect in value["effects"]:
            expect_keys(effect, ["type", "duration"], where)
            entries.append(rs_tuple([rs_str(normalize_id(effect["type"])), rs_int(effect["duration"])]))
        return rs_call("Modifier::SetStewEffects", [rs_slice(entries)])
    if kind == "minecraft:set_random_dyes":
        expect_keys(value, ["type", "number_of_dyes"], where)
        number = value["number_of_dyes"]
        expect_keys(number, ["type", "inputs"], where)
        base, binomial = number["inputs"]
        expect_keys(binomial, ["type", "n", "p"], where)
        assert number["type"] == "minecraft:add" and binomial["type"] == "minecraft:binomial", where
        return rs_struct(
            "Modifier::SetRandomDyes",
            [("base", rs_int(base)), ("trials", rs_int(binomial["n"])), ("chance", rs_f32(binomial["p"]))],
        )
    if kind == "minecraft:exploration_map":
        expect_keys(value, ["type", "decoration", "destination", "search_radius"], where)
        return rs_struct(
            "Modifier::ExplorationMap",
            [
                ("decoration", rs_str(normalize_id(value.get("decoration", "minecraft:mansion")))),
                ("destination", rs_str(normalize_id(value["destination"][1:]))),
                ("search_radius", rs_int(value["search_radius"])),
            ],
        )
    if kind == "minecraft:set_random_potion":
        expect_keys(value, ["type", "options"], where)
        assert value["options"] == "#minecraft:tradeable", where
        return Atom("Modifier::SetRandomPotion")
    if kind == "minecraft:set_potion":
        expect_keys(value, ["type", "id"], where)
        potion = normalize_id(value["id"])
        if potion not in potions:
            fail("unknown potion %s in %s" % (potion, where))
        return rs_call("Modifier::SetPotion", [rs_str(potion)])
    if kind == "minecraft:filtered":
        expect_keys(value, ["type", "item_filter", "on_fail"], where)
        assert value["on_fail"] == {"type": "minecraft:discard"}, where
        item_filter = value["item_filter"]
        expect_keys(item_filter, ["items", "predicates"], where)
        predicates = item_filter["predicates"]
        assert len(predicates) == 1, where
        (key,) = predicates
        if key not in TRADE_REQUIRES:
            fail("unsupported filter %s in %s" % (key, where))
        return rs_call("Modifier::Require", [Atom("Require::" + TRADE_REQUIRES[key])])
    fail("unsupported trade modifier %s in %s" % (kind, where))


def build_trading(datapack, reports, static_registries, synced, tag_table):
    item_ids = {
        name for registry_id, by_id, _ in static_registries if registry_id == "minecraft:item" for name in by_id
    }
    potions = {
        name for registry_id, by_id, _ in static_registries if registry_id == "minecraft:potion" for name in by_id
    }
    enchantment_names = {registry_id: entries for _, registry_id, entries in synced}["minecraft:enchantment"]
    enchantment_tags = {
        name for registry_id, by_name in tag_table.items() if registry_id == "minecraft:enchantment" for name in by_name
    }
    enchantments = []
    for name in enchantment_names:
        path = os.path.join(datapack, "enchantment", name.split(":", 1)[1] + ".json")
        value = load_json(path)
        where = "enchantment " + name
        for key in ("supported_items", "primary_items", "exclusive_set"):
            if key in value:
                assert value[key].startswith("#"), (where, key)
        enchantments.append(
            rs_struct(
                "EnchantmentDef",
                [
                    ("weight", rs_int(value["weight"])),
                    ("max_level", rs_int(value["max_level"])),
                    ("min_base", rs_int(value["min_cost"]["base"])),
                    ("min_step", rs_int(value["min_cost"]["per_level_above_first"])),
                    ("max_base", rs_int(value["max_cost"]["base"])),
                    ("max_step", rs_int(value["max_cost"]["per_level_above_first"])),
                    ("supported", rs_str(normalize_id(value["supported_items"][1:]))),
                    (
                        "primary",
                        rs_option(
                            rs_str(normalize_id(value["primary_items"][1:])) if "primary_items" in value else None
                        ),
                    ),
                    (
                        "exclusive",
                        rs_option(
                            rs_str(normalize_id(value["exclusive_set"][1:])) if "exclusive_set" in value else None
                        ),
                    ),
                ],
            )
        )
    item_index = {name: i for registry_id, by_id, _ in static_registries if registry_id == "minecraft:item" for i, name in enumerate(by_id)}
    components_root = os.path.join(reports, "minecraft", "components", "item")
    enchantability = []
    for name in json_names(components_root):
        component = load_json(os.path.join(components_root, name + ".json"))["components"].get("minecraft:enchantable")
        if component is None:
            continue
        expect_keys(component, ["value"], "enchantable " + name)
        enchantability.append((item_index["minecraft:" + name], component["value"]))
    enchantability.sort()

    trade_root = os.path.join(datapack, "villager_trade")
    trade_names = ["minecraft:" + name for name in json_names(trade_root)]
    trade_index = {name: i for i, name in enumerate(trade_names)}
    trades = []
    for name in trade_names:
        where = "villager_trade " + name
        value = load_json(os.path.join(trade_root, name.split(":", 1)[1] + ".json"))
        expect_keys(value, TRADE_KEYS, where)
        for key, expected in (("max_uses", int), ("xp", int), ("reputation_discount", (int, float))):
            assert key not in value or isinstance(value[key], expected), (where, key)
        variants = []
        if "merchant_predicate" in value:
            predicate = value["merchant_predicate"]
            expect_keys(predicate, ["type", "entity", "predicate"], where)
            assert predicate["type"] == "minecraft:entity_properties" and predicate["entity"] == "this", where
            inner = predicate["predicate"]
            expect_keys(inner, ["minecraft:predicates"], where)
            predicates = inner["minecraft:predicates"]
            expect_keys(predicates, ["minecraft:villager/variant"], where)
            listed = predicates["minecraft:villager/variant"]
            variants = [normalize_id(item) for item in (listed if isinstance(listed, list) else [listed])]
        modifiers = value.get("given_item_modifier", [])
        if isinstance(modifiers, dict):
            modifiers = [modifiers]
        gives = value["gives"]
        expect_keys(gives, ["id", "count"], where)
        double_price = value.get("double_trade_price_enchantments")
        trades.append(
            rs_struct(
                "Trade",
                [
                    ("name", rs_str(name)),
                    ("wants", trade_cost(value["wants"], item_ids, potions, where)),
                    (
                        "additional_wants",
                        rs_option(
                            trade_cost(value["additional_wants"], item_ids, potions, where)
                            if "additional_wants" in value
                            else None
                        ),
                    ),
                    ("gives", rs_str(trade_item(gives["id"], item_ids, where))),
                    ("gives_count", rs_int(gives.get("count", 1))),
                    ("max_uses", rs_int(value.get("max_uses", 4))),
                    ("xp", rs_int(value.get("xp", 1))),
                    ("reputation_discount", rs_f32(value.get("reputation_discount", 0.0))),
                    ("variants", rs_slice([rs_str(variant) for variant in variants])),
                    (
                        "modifiers",
                        rs_slice(
                            [trade_modifier(item, item_ids, potions, enchantment_tags, where) for item in modifiers]
                        ),
                    ),
                    (
                        "double_price",
                        rs_option(None if double_price is None else trade_tag_name(double_price, enchantment_tags, where)),
                    ),
                ],
            )
        )
    set_root = os.path.join(datapack, "trade_set")
    trade_sets = []
    for name in json_names(set_root):
        where = "trade_set " + name
        value = load_json(os.path.join(set_root, name + ".json"))
        expect_keys(value, TRADE_SET_KEYS, where)
        assert isinstance(value["amount"], int), where
        assert value["trades"].startswith("#"), where
        members = trade_tag_values(datapack, normalize_id(value["trades"][1:]))
        for member in members:
            if member not in trade_index:
                fail("unknown trade %s in %s" % (member, where))
        trade_sets.append(
            rs_struct(
                "TradeSet",
                [
                    ("name", rs_str("minecraft:" + name)),
                    ("trades", rs_slice([Atom(str(trade_index[member])) for member in members])),
                    ("amount", rs_int(value["amount"])),
                    ("allow_duplicates", rs_bool(value.get("allow_duplicates", False))),
                ],
            )
        )
    return enchantments, enchantability, trades, trade_sets


def emit_trading(enchantments, enchantability, trades, trade_sets):
    out = ["pub(crate) static ENCHANTMENTS: [EnchantmentDef; %d] = [" % len(enchantments)]
    for node in enchantments:
        out.extend(render(node, "    ", "", ","))
    out.append("];")
    out.append("")
    out.append("pub(crate) static ENCHANTABILITY: [(u16, u8); %d] = [" % len(enchantability))
    out.extend(wrapped(["(%d, %d)" % pair for pair in enchantability], "    "))
    out.append("];")
    out.append("")
    out.append("pub(crate) static TRADES: [Trade; %d] = [" % len(trades))
    for node in trades:
        out.extend(render(node, "    ", "", ","))
    out.append("];")
    out.append("")
    out.append("pub(crate) static TRADE_SETS: [TradeSet; %d] = [" % len(trade_sets))
    for node in trade_sets:
        out.extend(render(node, "    ", "", ","))
    out.append("];")
    text = "\n".join(out) + "\n"
    names = ["EnchantmentDef", "Modifier", "Require", "Trade", "TradeCost", "TradeSet"]
    head = "use crate::trading::{%s};\n\n" % ", ".join(used_imports(text, names))
    return head + text


def emit_mod():
    return "\n".join(
        [
            "pub mod block_transformers;",
            "pub mod climate;",
            "pub mod compostable;",
            "pub mod loot;",
            "pub mod spawns;",
            "pub mod static_registries;",
            "pub mod synced;",
            "pub mod tags;",
            "pub mod trading;",
            "pub mod variants;",
            "",
        ]
    )


def write_files(out_dir, files):
    os.makedirs(out_dir, exist_ok=True)
    for filename, text in files.items():
        assert "//" not in text, filename
        with open(os.path.join(out_dir, filename), "w", encoding="utf-8") as handle:
            handle.write(text)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--reports", default=DEFAULT_REPORTS)
    parser.add_argument("--datapack", default=DEFAULT_DATAPACK)
    parser.add_argument("--classes", default=DEFAULT_CLASSES)
    parser.add_argument("--javap", default=DEFAULT_JAVAP)
    parser.add_argument("--out", default=DEFAULT_OUT)
    parser.add_argument("--no-javap", action="store_true")
    args = parser.parse_args()

    verified = False
    if not args.no_javap:
        verified = verify_synced_against_java(args.javap, args.classes)

    registries = load_json(os.path.join(args.reports, "registries.json"))

    static_registries = build_static_registries(registries)
    synced, missing = build_synced(args.datapack)
    tag_table, skipped = build_tags(args.datapack, static_registries, synced)
    block_reports = load_block_properties(args.reports)
    tables, block_tables, entity_tables, predicates, wall_mapped, ignored = build_loot(
        args.datapack, block_reports, static_registries, synced, tag_table
    )
    smelting = build_smelting(args.datapack, static_registries, tag_table)
    biome_spawns = build_biome_spawns(args.datapack, static_registries, synced)
    dimension_spawns = build_dimension_spawns(args.datapack)
    if not args.no_javap:
        verify_spawn_defaults_against_java(args.javap, args.classes)
    mob_variants, sound_variants = build_variants(args.datapack, synced, tag_table)
    dye_mixes = build_dye_mixes(args.datapack)
    climate = build_climate(args.datapack, synced)
    transformers = build_transformers(args.datapack)
    compost_providers, compostables = build_compostables(args.reports, args.datapack, static_registries)
    trading = build_trading(args.datapack, args.reports, static_registries, synced, tag_table)

    files = {
        "mod.rs": emit_mod(),
        "static_registries.rs": emit_static_registries(static_registries),
        "synced.rs": emit_synced(synced),
        "tags.rs": emit_tags(tag_table),
        "loot.rs": emit_loot(tables, block_tables, entity_tables, predicates, smelting),
        "spawns.rs": emit_spawns(biome_spawns, dimension_spawns),
        "variants.rs": emit_variants(mob_variants, sound_variants, dye_mixes),
        "climate.rs": emit_climate(climate),
        "block_transformers.rs": emit_transformers(transformers),
        "compostable.rs": emit_compostables(compost_providers, compostables),
        "trading.rs": emit_trading(*trading),
    }
    write_files(args.out, files)

    total = 0
    for filename, text in files.items():
        size = len(text.encode("utf-8"))
        total += size
        print("%-24s %9d bytes" % (filename, size))
    print("%-24s %9d bytes" % ("total", total))
    print("items %d" % len(registries["minecraft:item"]["entries"]))
    print("static registries %d" % len(static_registries))
    print("javap verification %s" % ("passed" if verified else "skipped"))
    for _, registry_id, entries in synced:
        print("synced %-45s %5d" % (registry_id, len(entries)))
    print("synced without directory: %s" % (", ".join(missing) if missing else "none"))
    for registry_id in sorted(tag_table):
        print("tags %-45s %5d" % (registry_id, len(tag_table[registry_id])))
    print("tag registries skipped: %s" % (", ".join(skipped) if skipped else "none"))
    print("loot tables %d" % len(tables))
    print("blocks with loot table %d" % sum(1 for index in block_tables if index != 65535))
    print("entity types with loot table %d" % sum(1 for index in entity_tables if index != 65535))
    print("smelting inputs %d" % len(smelting))
    print("wall variants using floor table %d" % len(wall_mapped))
    print("named predicates %d" % len(predicates))
    print("biome spawn settings %d, dimension spawn settings %d" % (len(biome_spawns), len(dimension_spawns)))
    for constant, entries in mob_variants + sound_variants:
        print("variants %-24s %3d" % (constant, len(entries)))
    print("dye mixes %d" % len(dye_mixes))
    print("compostable items %d" % len(compostables))
    print("enchantments %d, trades %d, trade sets %d" % (len(trading[0]), len(trading[2]), len(trading[3])))
    for construct in sorted(ignored):
        owners = sorted(ignored[construct])
        print("ignored %s in %d tables: %s" % (construct, len(owners), ", ".join(owners)))


if __name__ == "__main__":
    main()

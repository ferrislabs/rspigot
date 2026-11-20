import argparse
import json
import re
import subprocess
import sys
import tomllib
from dataclasses import dataclass, field
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent
PROTO_DIR = ("crates", "rspigot-plugin-protocol", "proto", "rspigot", "plugin", "v1")
INPUTS = {
    "catalog": ("tools", "plugin-events", "catalog.json"),
    "overrides": ("tools", "plugin-events", "overrides.toml"),
    "types": (*PROTO_DIR, "types.proto"),
    "plugin": (*PROTO_DIR, "plugin.proto"),
    "library": ("crates", "rspigot-plugin", "src", "lib.rs"),
}
OUTPUTS = {
    "proto": (*PROTO_DIR, "events.proto"),
    "core": ("crates", "rspigot-core", "src", "plugins", "events.rs"),
    "host": ("crates", "rspigot-plugins", "src", "convert", "events.rs"),
    "sdk": ("crates", "rspigot-plugin", "src", "events.rs"),
    "macro": ("crates", "rspigot-plugin-macros", "src", "events.rs"),
    "doc": ("crates", "rspigot-plugin", "EVENTS.md"),
    "test": ("crates", "rspigot-plugin", "tests", "every_event.rs"),
}
RUST = {"core", "host", "sdk", "macro", "test"}
PAYLOAD_OFFSET = 9
DIFF_OFFSET = 1000
KEYWORDS = {
    "as", "break", "const", "continue", "crate", "else", "enum", "extern", "false", "fn", "for", "if",
    "impl", "in", "let", "loop", "match", "mod", "move", "mut", "pub", "ref", "return", "self", "Self",
    "static", "struct", "super", "trait", "true", "type", "unsafe", "use", "where", "while", "async",
    "await", "dyn", "abstract", "become", "box", "do", "final", "macro", "override", "priv", "typeof",
    "unsized", "virtual", "yield", "try", "gen",
}
RESERVED = {"id", "cancel", "is_cancelled", "is_rolled_back", "patch"}
CONTAINERS = {"java.util.List", "java.util.Set", "java.util.Collection", "java.util.Iterator"}
CONTRACT_ENUMS = {
    "org.bukkit.block.BlockFace": ("Face", ["DOWN", "UP", "NORTH", "SOUTH", "WEST", "EAST"]),
    "org.bukkit.event.inventory.ClickType": (
        "ClickKind",
        ["LEFT", "RIGHT", "SHIFT_LEFT", "SHIFT_RIGHT", "NUMBER_KEY", "SWAP_OFFHAND", "DROP", "CONTROL_DROP",
         "DOUBLE_CLICK", "MIDDLE"],
    ),
}
SDK_ALWAYS_READ = {"Face"}
VARIANT_RENAMES = {"Self": "Itself"}


class GenError(Exception):
    pass


def fail(message):
    raise GenError(message)


def snake(name):
    return re.sub(r"(?<=[a-z0-9])(?=[A-Z])|(?<=[A-Z])(?=[A-Z][a-z])", "_", name).lower()


def screaming(name):
    return snake(name).upper()


def pascal(constant):
    name = "".join(part[:1].upper() + part[1:].lower() for part in constant.split("_") if part)
    return VARIANT_RENAMES.get(name, name)


def pn(name):
    return "".join(part[:1].upper() + part[1:] for part in snake(name).split("_"))


def pv(constant):
    name = "".join(part[:1].upper() + part[1:].lower() for part in constant.split("_") if part)
    return name + "_" if name in KEYWORDS else name


def simple(java):
    return java.rsplit(".", 1)[1]


def stem(name):
    for prefix in ("get", "is", "has", "can", "should", "set", "add", "remove", "clear", "put"):
        if name.startswith(prefix) and len(name) > len(prefix) and name[len(prefix)].isupper():
            return name[len(prefix)].lower() + name[len(prefix) + 1 :]
    return name


def getter_method(name):
    if name.startswith("get") and len(name) > 3 and name[3].isupper():
        return snake(name[3:])
    return snake(name)


def signature(entry):
    params = entry.get("params", [])
    if not params or (entry["name"].startswith("set") and len(params) == 1 and "type" not in entry):
        return entry["name"]
    return f"{entry['name']}({', '.join(simple(param) if '.' in param else param for param in params)})"


def enum_name(java):
    if java in CONTRACT_ENUMS:
        return CONTRACT_ENUMS[java][0]
    name = simple(java)
    if "$" not in name:
        return name
    outer, inner = name.split("$", 1)
    outer = outer.removesuffix("Event") or outer
    for size in range(min(len(outer), len(inner)), 0, -1):
        if outer.endswith(inner[:size]):
            return outer + inner[size:]
    return outer + inner


@dataclass
class Leaf:
    id: str
    proto: str
    shape: str
    core: str
    sdk: str
    base: str
    sdk_copy: bool = False
    host_out: str = "{}"
    host_in: str = "{}"
    host_in_try: bool = False
    sdk_in: str = "{}"
    sdk_in_try: bool = False
    sdk_out: str = "{}"
    core_uses: tuple = ()
    sdk_uses: tuple = ()
    enum: object = None
    wire_copy: bool = True


@dataclass
class Ty:
    leaf: Leaf
    many: bool = False
    array: bool = False
    set: bool = False

    def same(self, other):
        return self.leaf.proto == other.leaf.proto and self.many == other.many


def scalar(id, proto, core, sdk=None, **extra):
    sdk = sdk or core
    return Leaf(id=id, proto=proto, shape="scalar", core=core, sdk=sdk, base=pascal(id), sdk_copy=True, **extra)


def text(id, sdk="String", sdk_in="{}", sdk_in_try=False, sdk_out="{}.clone()", sdk_copy=False, sdk_uses=()):
    return Leaf(
        id=id, proto="string", shape="scalar", core="String", sdk=sdk, base="String", sdk_copy=sdk_copy,
        sdk_in=sdk_in, sdk_in_try=sdk_in_try, sdk_out=sdk_out, sdk_uses=sdk_uses, wire_copy=False,
    )


def message(id, proto, core, sdk, host_out, host_in, sdk_in, sdk_out, host_in_try=False, sdk_in_try=True,
            sdk_copy=False, core_uses=(), sdk_uses=(), base=None, wire_copy=False):
    return Leaf(
        id=id, proto=proto, shape="message", core=core, sdk=sdk, base=base or proto, sdk_copy=sdk_copy,
        host_out=host_out, host_in=host_in, host_in_try=host_in_try, sdk_in=sdk_in, sdk_in_try=sdk_in_try,
        sdk_out=sdk_out, core_uses=core_uses, sdk_uses=sdk_uses, wire_copy=wire_copy,
    )


def contract(proto, core, sdk=None, sdk_copy=False, wire_copy=False):
    sdk = sdk or core
    return message(
        id=snake(proto), proto=proto, core=core, sdk=sdk,
        host_out=f"contract::wire_{snake(proto)}({{}})", host_in=f"contract::{snake(proto)}({{}})", host_in_try=True,
        sdk_in=f"{sdk}::from_wire({{}}, link)", sdk_out="{}.to_wire()", sdk_copy=sdk_copy,
        core_uses=(f"crate::plugins::{core}",), sdk_uses=(f"crate::{sdk}",), wire_copy=wire_copy,
    )


def key(id, sdk, reader):
    return text(id, sdk=sdk, sdk_in=f"{reader}({{}})", sdk_in_try=True, sdk_out="{}.key().to_owned()",
                sdk_copy=True, sdk_uses=(f"crate::{sdk}", f"crate::wire::{reader}"))


STRING = text("string")
LEAVES = {
    "boolean": scalar("bool", "bool", "bool"),
    "int": scalar("int", "int32", "i32"),
    "java.lang.Integer": scalar("int", "int32", "i32"),
    "long": scalar("long", "int64", "i64"),
    "float": scalar("float", "float", "f32"),
    "double": scalar("double", "double", "f64"),
    "byte": scalar(
        "byte", "int32", "i8", host_out="i32::from({})", host_in="shared::byte({})", host_in_try=True,
        sdk_in="byte({})", sdk_in_try=True, sdk_out="i32::from({})", sdk_uses=("crate::wire::byte",),
    ),
    "java.lang.String": STRING,
    "com.google.gson.JsonElement": STRING,
    "org.bukkit.NamespacedKey": STRING,
    "org.bukkit.advancement.Advancement": STRING,
    "org.bukkit.loot.LootTable": STRING,
    "org.bukkit.inventory.Recipe": STRING,
    "org.bukkit.inventory.CraftingRecipe": STRING,
    "org.bukkit.inventory.CookingRecipe": STRING,
    "org.bukkit.inventory.CampfireRecipe": STRING,
    "org.bukkit.GameEvent": STRING,
    "org.bukkit.generator.structure.Structure": STRING,
    "org.bukkit.entity.Villager$Profession": STRING,
    "org.bukkit.entity.Villager$ReputationType": STRING,
    "org.bukkit.entity.Villager$ReputationEvent": STRING,
    "java.util.UUID": Leaf(
        id="uuid", proto="string", shape="scalar", core="Uuid", sdk="Uuid", base="String", sdk_copy=True,
        host_out="{}.hyphenated().to_string()", host_in="shared::uuid({})", host_in_try=True,
        sdk_in="uuid({})", sdk_in_try=True, sdk_out="{}.hyphenated().to_string()",
        core_uses=("uuid::Uuid",), sdk_uses=("crate::Uuid", "crate::wire::uuid"), wire_copy=False,
    ),
    "java.net.InetAddress": Leaf(
        id="address", proto="string", shape="scalar", core="IpAddr", sdk="IpAddr", base="String", sdk_copy=True,
        host_out="{}.to_string()", host_in="shared::address({})", host_in_try=True,
        sdk_in="address({})", sdk_in_try=True, sdk_out="{}.to_string()",
        core_uses=("std::net::IpAddr",), sdk_uses=("std::net::IpAddr", "crate::wire::address"), wire_copy=False,
    ),
    "org.bukkit.World": text("world", sdk="World", sdk_in="world({}, link)", sdk_out="{}.name().unwrap_or_default().to_owned()",
                             sdk_uses=("crate::World", "crate::wire::world")),
    "org.bukkit.Material": key("material", "Material", "material"),
    "org.bukkit.entity.EntityType": key("entity_type", "EntityType", "entity_type"),
    "org.bukkit.enchantments.Enchantment": key("enchantment", "Enchantment", "enchantment"),
    "org.bukkit.potion.PotionEffectType": key("effect_type", "EffectType", "effect_type"),
    "org.bukkit.GameMode": Leaf(
        id="game_mode", proto="GameMode", shape="enum", core="GameMode", sdk="GameMode", base="GameMode",
        sdk_copy=True, host_out="player::wire_game_mode({})", host_in="player::game_mode({})", host_in_try=True,
        sdk_in="GameMode::from_wire({})", sdk_in_try=True, sdk_out="{}.to_wire() as i32",
        core_uses=("rspigot_domain::player::GameMode",), sdk_uses=("crate::GameMode",),
    ),
    "org.bukkit.Note": scalar(
        "note", "int32", "u8", host_out="i32::from({})", host_in="shared::note({})", host_in_try=True,
        sdk_in="note({})", sdk_in_try=True, sdk_out="i32::from({})", sdk_uses=("crate::wire::note",),
    ),
    "org.bukkit.util.CachedServerIcon": Leaf(
        id="icon", proto="bytes", shape="scalar", core="Vec<u8>", sdk="Vec<u8>", base="Bytes",
        sdk_out="{}.clone()", wire_copy=False,
    ),
    "org.bukkit.potion.PotionEffect": message(
        "effect", "Effect", "EffectSpec", "Effect", "shared::wire_effect({})", "shared::effect({})",
        "Effect::from_wire({})", "{}.to_wire()", sdk_copy=True,
        core_uses=("crate::plugins::EffectSpec",), sdk_uses=("crate::Effect",),
    ),
    "org.bukkit.inventory.ItemStack": message(
        "item", "Item", "ItemSpec", "ItemStack", "shared::wire_item({})", "shared::item({})",
        "ItemStack::from_wire({})", "{}.to_wire()",
        core_uses=("crate::plugins::ItemSpec",), sdk_uses=("crate::ItemStack",),
    ),
    "org.bukkit.block.data.BlockData": message(
        "block_data", "BlockState", "BlockState", "BlockState", "block::wire_state({})", "block::state({})",
        "BlockState::from_wire({})", "{}.to_wire()",
        core_uses=("crate::plugins::BlockState",), sdk_uses=("crate::BlockState",),
    ),
    "org.bukkit.block.Block": message(
        "block", "BlockPos", "BlockPos", "Block", "shared::wire_block_pos({})", "shared::block_pos({})",
        "block({}, link)", "{}.pos.to_wire()", sdk_in_try=False, wire_copy=True,
        core_uses=("rspigot_domain::BlockPos",), sdk_uses=("crate::Block", "crate::wire::block"),
    ),
    "org.bukkit.util.Vector": message(
        "vector", "Vec3", "Vec3", "Vector", "shared::wire_vec({})", "shared::vec3({})",
        "vector({})", "{}.to_wire()", sdk_in_try=False, sdk_copy=True, wire_copy=True,
        core_uses=("rspigot_domain::Vec3",), sdk_uses=("crate::Vector", "crate::wire::vector"),
    ),
    "org.bukkit.Location": contract("Location", "Location"),
    "org.bukkit.block.BlockState": contract("BlockSnapshot", "BlockSnapshot"),
    "org.bukkit.inventory.Inventory": contract("InventoryRef", "InventoryRef"),
    "org.bukkit.inventory.InventoryView": contract("InventoryView", "InventoryView"),
    "org.bukkit.Chunk": contract("ChunkRef", "ChunkRef"),
    "org.bukkit.damage.DamageSource": contract("DamageSource", "DamageSourceView", "DamageSource"),
    "org.bukkit.command.CommandSender": contract("CommandSender", "SenderView", "CommandSender"),
    "org.bukkit.inventory.MerchantRecipe": contract("Trade", "Trade"),
    "org.bukkit.plugin.Plugin": contract("PluginInfo", "PluginInfo"),
    "org.bukkit.util.BoundingBox": contract("BoundingBox", "BoundingBox", sdk_copy=True, wire_copy=True),
    "org.bukkit.Input": contract("Input", "Input", sdk_copy=True, wire_copy=True),
    "org.bukkit.block.CreatureSpawner": contract("SpawnerData", "SpawnerData"),
    "org.bukkit.block.TrialSpawner": contract("TrialSpawnerData", "TrialSpawnerData"),
    "org.bukkit.block.Lectern": contract("LecternData", "LecternData"),
    "org.bukkit.block.Sign": contract("SignData", "SignData"),
    "org.bukkit.enchantments.EnchantmentOffer": contract("EnchantOffer", "EnchantOffer", sdk_copy=True),
    "org.bukkit.entity.AnimalTamer": contract("OwnerRef", "OwnerRef"),
    "org.bukkit.inventory.InventoryHolder": contract("HolderRef", "HolderRef"),
    "org.bukkit.inventory.Merchant": contract("MerchantRef", "MerchantRef"),
    "org.bukkit.inventory.meta.BookMeta": contract("BookMeta", "BookMeta"),
    "org.bukkit.loot.LootContext": contract("LootContext", "LootContext"),
}
for _inventory in ("Crafting", "Smithing", "Brewer", "Anvil", "Grindstone", "Merchant"):
    LEAVES[f"org.bukkit.inventory.{_inventory}Inventory"] = LEAVES["org.bukkit.inventory.Inventory"]
for _view in ("Enchantment", "Anvil", "Merchant"):
    LEAVES[f"org.bukkit.inventory.view.{_view}View"] = LEAVES["org.bukkit.inventory.InventoryView"]
PLAYER = message(
    "player", "PlayerSnapshot", "PlayerSnapshot", "Player", "player::wire_player({})", "player::snapshot({})",
    "Player::from_snapshot({}, link)", "{}.to_snapshot()", host_in_try=True,
    core_uses=("crate::plugins::PlayerSnapshot",), sdk_uses=("crate::Player",),
)
ENTITY = message(
    "entity", "EntitySnapshot", "EntityView", "Entity", "entity::wire_view({})", "entity::view({})",
    "Entity::from_wire({}, link)", "{}.to_wire()", host_in_try=True,
    core_uses=("crate::plugins::EntityView",), sdk_uses=("crate::Entity",),
)
LEAVES["org.bukkit.entity.Player"] = PLAYER
LEAVES["org.bukkit.entity.HumanEntity"] = PLAYER
MAPS = {
    "java.util.Map<org.bukkit.enchantments.Enchantment, java.lang.Integer>": message(
        "enchant", "Enchant", "(String, i32)", "(Enchantment, u32)", "shared::wire_enchant({})",
        "shared::enchant({})", "enchant({})", "enchant_to_wire({})", sdk_copy=True,
        sdk_uses=("crate::Enchantment", "crate::wire::enchant", "crate::wire::enchant_to_wire"),
    ),
    "java.util.List<org.bukkit.entity.Item>": contract("DroppedItem", "DroppedItem"),
    "java.util.Map<java.lang.Integer, org.bukkit.inventory.ItemStack>": message(
        "slot", "Slot", "(i32, ItemSpec)", "(i32, ItemStack)", "shared::wire_slot({})", "shared::slot({})",
        "slot({})", "slot_to_wire({})", host_in_try=True, sdk_copy=False,
        core_uses=("crate::plugins::ItemSpec",), sdk_uses=("crate::ItemStack", "crate::wire::slot", "crate::wire::slot_to_wire"),
    ),
}


@dataclass
class EnumDef:
    java: str
    name: str
    constants: list
    numbers: dict
    reserved: list


@dataclass
class Field:
    name: str
    number: int
    ty: Ty
    method: str = None
    getter: str = None
    nullable: bool = False
    setter: str = None
    setter_method: str = None
    patch_nullable: bool = False
    direct: bool = False
    index: Leaf = None
    limit: int = None
    set: bool = False
    actor: bool = False


@dataclass
class Event:
    java: str
    bukkit: str
    name: str
    number: int
    cancellable: bool
    registration: str
    parents: list
    fields: list
    left_out: list
    state: str
    state_note: str
    cancel_sets: dict = field(default_factory=dict)
    main_result: str = None
    receivers: list = field(default_factory=list)

    @property
    def snake(self):
        return snake(self.name)

    @property
    def payload_number(self):
        return self.number + PAYLOAD_OFFSET

    @property
    def readable(self):
        return [f for f in self.fields if f.getter]

    @property
    def patched(self):
        return [f for f in self.fields if f.setter]

    @property
    def decidable(self):
        return self.cancellable or bool(self.patched)

    @property
    def writes_when_cancelled(self):
        return any(f.direct for f in self.patched)

    @property
    def cancels_by_result(self):
        return self.main_result is not None

    @property
    def kept_when_cancelled(self):
        return [f for f in self.patched if f.direct or f.name in self.cancel_sets]


class Model:
    def __init__(self, catalog, overrides, contract, library):
        self.catalog = catalog
        self.overrides = overrides
        self.contract = contract
        self.library = library
        self.types = catalog["types"]
        self.enums = {}
        self.unmapped = overrides.get("unmapped", {})
        self.events = []
        self.excluded = {}
        self.results = overrides.get("results", {})
        self.used_results = set()
        self.build()
        self.reserved = self.reserved_numbers()

    def reserved_numbers(self):
        reserved = {record["number"] for record in self.catalog["events"] if record["deprecated"] and record.get("number")}
        reserved |= {record["number"] for record in self.catalog.get("retired", [])}
        live = {record["number"] for record in self.catalog["events"] if not record["deprecated"]}
        clashing = reserved & live
        if clashing:
            fail(f"the retired event numbers {sorted(clashing)} are given to live events")
        return sorted(reserved)

    def enum_leaf(self, java):
        if java not in self.enums:
            record = self.types.get(java)
            constants = record and record.get("constants")
            if not constants:
                fail(f"the enum {java} has no listed constants")
            name = self.overrides.get("enums", {}).get(java, enum_name(java))
            if java in CONTRACT_ENUMS:
                seeded = CONTRACT_ENUMS[java][1]
                ordered = seeded + [constant for constant in constants if constant not in seeded]
            else:
                ordered = list(constants)
            numbers = record.get("numbers") or {}
            missing = [constant for constant in ordered if constant not in numbers]
            if missing:
                fail(f"the constants {missing} of {java} have no number, run tools/plugin-events/extract.py")
            if len(set(numbers.values())) != len(numbers):
                fail(f"two constants of {java} share a number")
            reserved = sorted(number for constant, number in numbers.items() if constant not in ordered)
            self.enums[java] = EnumDef(java, name, ordered, {constant: numbers[constant] for constant in ordered}, reserved)
        definition = self.enums[java]
        name = definition.name
        return Leaf(
            id=snake(name), proto=name, shape="enum", core=name, sdk=name, base=name, sdk_copy=True,
            host_out=f"{snake(name)}_to_wire({{}})", host_in=f"{snake(name)}_from_wire({{}})", host_in_try=True,
            sdk_in=f"{name}::from_wire({{}})", sdk_in_try=True, sdk_out="{}.to_wire()", enum=definition,
        )

    def leaf(self, java):
        if java in LEAVES:
            return LEAVES[java]
        record = self.types.get(java)
        if record is None:
            fail(f"the Java type {java} is not in the catalog")
        if record["kind"] == "entity":
            return ENTITY
        if record["kind"] == "enum":
            return self.enum_leaf(java)
        fail(f"no mapping for the Java type {java}")

    def unmapped_reason(self, java):
        for name, reason in self.unmapped.items():
            if re.search(rf"(^|[<, ]){re.escape(name)}([>, ]|$)", java):
                return reason
        return None

    def ty(self, java):
        java = java.strip()
        if java in MAPS:
            return Ty(MAPS[java], many=True)
        if java.endswith("[]"):
            return Ty(self.leaf(java[:-2]), many=True, array=True)
        generic = re.match(r"^([\w.$]+)<(.*)>$", java)
        if generic and generic[1] in CONTAINERS:
            inner = re.sub(r"^\?( (extends|super) )?", "", generic[2]).strip()
            return Ty(self.leaf(inner), many=True, set=generic[1] == "java.util.Set")
        if generic:
            return Ty(self.leaf(generic[1]))
        return Ty(self.leaf(java))

    def build(self):
        names = self.overrides.get("names", {})
        not_applicable = self.overrides.get("not_applicable", {})
        fired = self.overrides.get("fired", {})
        states = self.overrides.get("states", {})
        ready = set(states.get("ready", []))
        blocked = states.get("blocked", {})
        per_event = self.overrides.get("events", {})
        seen = set()
        known = {event["name"] for event in self.catalog["events"]}
        for table in (not_applicable, fired, blocked, per_event, names):
            for name in table:
                if name not in known:
                    fail(f"overrides name an unknown event {name}")
        for name in ready:
            if name not in known:
                fail(f"overrides name an unknown event {name}")
        for record in self.catalog["events"]:
            bukkit = record["name"]
            if bukkit in seen:
                fail(f"two events are called {bukkit}")
            seen.add(bukkit)
            places = [bukkit in not_applicable, bukkit in fired, bukkit in ready, bukkit in blocked, record["deprecated"]]
            if sum(places) != 1:
                fail(f"{bukkit} must have exactly one state (fired, ready, blocked, not applicable or deprecated), it has {sum(places)}")
            if record["deprecated"]:
                self.excluded[bukkit] = "obsolete: deprecated in Bukkit"
                continue
            if bukkit in not_applicable:
                self.excluded[bukkit] = f"not applicable: {not_applicable[bukkit]}"
                continue
            if bukkit in fired:
                state, note = "fired", fired[bukkit]
            elif bukkit in ready:
                state, note = "ready", ""
            else:
                state, note = "blocked", blocked[bukkit]
            self.events.append(self.event(record, names.get(bukkit, bukkit), state, note, per_event.get(bukkit, {})))
        unused = set(self.results) - self.used_results
        if unused:
            fail(f"the result overrides {sorted(unused)} match no event")
        self.check_names()
        self.families()

    def event(self, record, sdk_name, state, note, extra):
        allowed = {"getters", "rename", "collections", "omit", "unread", "revive", "applies_when_cancelled",
                   "pairs", "extra", "indexed", "limits", "sets", "actor"}
        for option in extra:
            if option not in allowed:
                fail(f"{record['name']}: unknown override `{option}`")
        omit = dict(extra.get("omit", {}))
        unread = dict(extra.get("unread", {}))
        revive = set(extra.get("revive", []))
        rename = extra.get("rename", {})
        extra_getters = set(extra.get("getters", []))
        collections = set(extra.get("collections", []))
        direct = set(extra.get("applies_when_cancelled", []))
        pairs = extra.get("pairs", {})
        limits = extra.get("limits", {})
        actor = extra.get("actor")
        numbers = record["fields"]
        used_overrides = set()
        left_out = []
        fields = {}

        def skipped(entry, kind):
            sig = signature(entry)
            if sig in omit:
                used_overrides.add(("omit", sig))
                left_out.append((sig, omit[sig]))
                return True
            if sig in unread:
                used_overrides.add(("unread", sig))
                left_out.append((sig, unread[sig]))
                return True
            if entry.get("deprecated") and sig not in revive:
                return True
            if sig in revive:
                used_overrides.add(("revive", sig))
            types = list(entry.get("params", [])) + ([entry["type"]] if kind != "setter" and "type" in entry else [])
            for java in types:
                reason = self.unmapped_reason(java)
                if reason:
                    left_out.append((sig, reason))
                    return True
            return False

        getters = list(record["getters"])
        for entry in record["other"]:
            if entry["name"] in extra_getters:
                used_overrides.add(("getters", entry["name"]))
                if entry["params"]:
                    fail(f"{record['name']}.{entry['name']} takes parameters and cannot be a field")
                getters.append(entry)
            elif not skipped(entry, "other"):
                fail(f"{record['name']}.{signature(entry)} has no get/set prefix: list it under `getters` or `omit`")
        indexed = {item["getter"]: item for item in extra.get("indexed", [])}
        indexed_setters = {}
        for entry in record["indexed"]:
            sig = signature(entry)
            if sig in indexed:
                item = indexed[sig]
                used_overrides.add(("indexed", sig))
                if len(entry["params"]) != 1:
                    fail(f"{record['name']}.{sig} must take one key")
                key = self.leaf(entry["params"][0])
                if key is not ENTITY:
                    fail(f"{record['name']}.{sig} is not keyed by an entity")
                self.check_ident(record["name"], item["name"])
                fields[item["name"]] = Field(
                    name=item["name"], number=numbers[entry["key"]], ty=Ty(self.leaf(entry["type"]), many=True),
                    method=getter_method(entry["name"]), getter=entry["name"], index=key,
                )
                indexed_setters[item["setter"]] = fields[item["name"]]
            elif not skipped(entry, "indexed"):
                fail(f"{record['name']}.{sig} is an indexed getter: list it under `omit` or `indexed`")
        for entry in getters:
            if skipped(entry, "getter"):
                continue
            raw = entry["key"]
            name = rename.get(raw, snake(raw))
            if raw in rename:
                used_overrides.add(("rename", raw))
            method = name if raw in rename else getter_method(entry["name"])
            entity_actor = False
            if raw == actor:
                actor_ty = self.ty(entry["type"])
                if actor_ty.many or actor_ty.leaf not in (PLAYER, ENTITY):
                    fail(f"{record['name']}: the actor {actor} is not a player getter")
                if actor_ty.leaf is PLAYER:
                    method = "player"
                else:
                    entity_actor = True
                used_overrides.add(("actor", actor))
            self.check_ident(record["name"], name)
            self.check_ident(record["name"], method)
            if name in fields:
                fail(f"{record['name']}: two getters give the field {name}")
            fields[name] = Field(
                name=name, number=numbers[raw], ty=self.ty(entry["type"]), method=method,
                getter=entry["name"], nullable=entry["nullable"] == "nullable", actor=entity_actor,
            )
        by_getter = {f.getter: f for f in fields.values()}
        for entry in record["setters"]:
            sig = signature(entry)
            if sig in indexed_setters:
                target = indexed_setters.pop(sig)
                key, value = (self.leaf(param) for param in entry["params"])
                if key is not target.index or value.proto != target.ty.leaf.proto:
                    fail(f"{record['name']}.{sig} does not write what its indexed getter reads")
                target.setter = entry["name"]
                target.setter_method = "set_" + snake(stem(entry["name"]))
                continue
            if skipped(entry, "setter"):
                continue
            if not entry["name"].startswith("set") or len(entry["params"]) != 1:
                fail(f"{record['name']}.{sig} is not a one-value setter: list it under `omit`")
            if sig in pairs:
                used_overrides.add(("pairs", sig))
                target = by_getter.get(pairs[sig])
                if target is None:
                    fail(f"{record['name']}: {sig} is paired with a missing getter {pairs[sig]}")
            else:
                raw = entry["key"]
                target = fields.get(rename.get(raw, snake(raw)))
            ty = self.ty(entry["params"][0])
            nullable = entry["nullable"][0] == "nullable"
            if target is None:
                raw = entry["key"]
                name = snake(raw)
                self.check_ident(record["name"], name)
                target = Field(name=name, number=numbers[raw], ty=ty)
                fields[name] = target
            elif not target.ty.same(ty):
                fail(f"{record['name']}.{sig} takes {ty.leaf.proto}, its getter gives {target.ty.leaf.proto}")
            if target.setter:
                fail(f"{record['name']}: two setters write {target.name}")
            if target.getter and not target.nullable:
                nullable = False
            target.setter = entry["name"]
            target.setter_method = "set_" + snake(stem(entry["name"]))
            target.patch_nullable = nullable and not ty.many
            if entry["name"] in direct:
                used_overrides.add(("applies_when_cancelled", entry["name"]))
                target.direct = True
        for setter, limit in limits.items():
            target = next((f for f in fields.values() if f.setter == setter), None)
            if target is None:
                continue
            if target.ty.many or target.ty.leaf.proto != "string" or not isinstance(limit, int) or limit < 1:
                fail(f"{record['name']}: the limit of {setter} needs a text setter and a positive length")
            target.limit = limit
            used_overrides.add(("limits", setter))
        for getter in collections:
            used_overrides.add(("collections", getter))
            target = by_getter.get(getter)
            if target is None or not target.ty.many:
                fail(f"{record['name']}: the collection {getter} is not a list field")
            if target.setter:
                fail(f"{record['name']}: the collection {getter} already has a setter")
            target.setter = getter
            target.setter_method = "set_" + target.name
        for item in extra.get("extra", []):
            name = item["name"]
            if name in fields or item["number"] in {f.number for f in fields.values()}:
                fail(f"{record['name']}: the extra field {name} collides with a field")
            target = Field(
                name=name, number=item["number"], ty=self.ty(item["java"]), method=name, getter=name,
                nullable=item.get("nullable", False),
            )
            if item.get("patch"):
                target.setter = name
                target.setter_method = "set_" + name
                target.patch_nullable = item.get("nullable", False)
            fields[name] = target
        readers = {f.getter: f for f in fields.values()}
        for getter in extra.get("sets", []):
            target = readers.get(getter)
            if target is None or not listed(target) or not target.setter:
                fail(f"{record['name']}: the set {getter} is not a list patch")
            if target.ty.set:
                fail(f"{record['name']}: {getter} is already a Java Set")
            target.set = True
        for f in fields.values():
            f.set = f.set or (listed(f) and f.ty.set)
        cancel_sets = {}
        main_result = None
        owner = next((java for java in [record["java"]] + record["parents"] if java in self.results), None)
        if owner is not None:
            self.used_results.add(owner)
            result = self.results[owner]
            for java_name in result["deny"]:
                target = by_getter.get(java_name)
                if target is None or not target.setter or target.ty.leaf.enum is None:
                    fail(f"{record['name']}: the result {java_name} is not a patchable enum field")
                if "DENY" not in target.ty.leaf.enum.constants:
                    fail(f"{record['name']}: DENY is not a constant of {target.ty.leaf.enum.name}")
                cancel_sets[target.name] = "DENY"
            main = by_getter.get(result["cancelled"])
            if main is None or main.name not in cancel_sets:
                fail(f"{record['name']}: the result {result['cancelled']} that tells a cancel is not one it denies")
            main_result = main.name
            if not record["cancellable"]:
                fail(f"{record['name']}: a result cancels an event that cannot be cancelled")
        if indexed_setters:
            fail(f"{record['name']}: the indexed setters {sorted(indexed_setters)} match nothing")
        for f in fields.values():
            if f.number >= DIFF_OFFSET:
                fail(f"{record['name']}.{f.name}: the field number {f.number} reaches the list diff numbers, which start at {DIFF_OFFSET}")
            if listed(f) and f.setter and not f.getter:
                fail(f"{record['name']}.{f.name}: a list patch is a diff against the list its getter reads, and that getter is left out")
            if listed(f) and f.setter and snapshot_field(f) in fields:
                fail(f"{record['name']}: the field {snapshot_field(f)} clashes with the snapshot of {f.name}")
        for kind, values in (("omit", omit), ("unread", unread), ("revive", revive), ("getters", extra_getters),
                             ("rename", rename), ("applies_when_cancelled", direct), ("pairs", pairs),
                             ("indexed", indexed), ("limits", limits),
                             ("actor", [actor] if actor else [])):
            for value in values:
                if (kind, value) not in used_overrides:
                    fail(f"{record['name']}: the override {kind} {value} matches nothing")
        ordered = sorted(fields.values(), key=lambda f: f.number)
        methods = [f.method for f in ordered if f.method] + [f.setter_method for f in ordered if f.setter_method] + ["player" for f in ordered if f.actor]
        if len(set(methods)) != len(methods):
            fail(f"{record['name']}: two methods share a name")
        for method in methods:
            if method in RESERVED:
                fail(f"{record['name']}: the method {method} clashes with a built-in method")
        return Event(
            java=record["java"], bukkit=record["name"], name=sdk_name, number=record["number"],
            cancellable=record["cancellable"], registration=record["registration"] or record["java"],
            parents=record["parents"], fields=ordered, left_out=sorted(left_out), state=state, state_note=note,
            cancel_sets=cancel_sets, main_result=main_result,
        )

    def check_ident(self, event, name):
        if name in KEYWORDS:
            fail(f"{event}: the field {name} is a Rust keyword, add a `rename`")

    def check_names(self):
        names = [event.name for event in self.events]
        if len(set(names)) != len(names):
            fail("two events share an SDK name")
        enum_names = [definition.name for definition in self.enums.values()]
        if len(set(enum_names)) != len(enum_names):
            fail("two generated enums share a name")
        clashing = set(enum_names) & (set(names) | {name + "Patch" for name in names})
        if clashing:
            fail(f"generated enums share a name with an event: {sorted(clashing)}, add an `enums` override")
        unused = set(self.overrides.get("enums", {})) - set(self.enums)
        if unused:
            fail(f"the enum overrides {sorted(unused)} match nothing")
        library = self.library.split("pub mod prelude")[0]
        exported = set()
        for group in re.findall(r"pub use [\w:]+::\{([^}]*)\}", library):
            exported.update(re.findall(r"\b[A-Z]\w*", group))
        exported.update(re.findall(r"pub use [\w:]+::([A-Z]\w*);", library))
        clash = exported & (set(names) | set(enum_names) | {"EventKind", "Call"})
        if clash:
            fail(f"generated SDK names clash with hand-written ones: {sorted(clash)}")
        taken = set(re.findall(r"^\s*(?:message|enum) (\w+)", self.contract, re.M))
        for name in names + enum_names:
            if name in taken:
                fail(f"the generated name {name} is already in the hand-written contract")
            if name + "Patch" in taken:
                fail(f"the generated name {name}Patch is already in the hand-written contract")

    def families(self):
        by_java = {event.java: event for event in self.events}
        for fired in self.events:
            chain = [fired.java] + fired.parents
            fired.receivers = [
                by_java[java] for java in chain
                if java in by_java and by_java[java].registration == fired.registration
            ]
        changed = True
        while changed:
            changed = False
            for fired in self.events:
                own = {f.name: f for f in fired.fields}
                for handler in fired.receivers:
                    for f in handler.fields:
                        other = own.get(f.name)
                        if other is None or f.ty.many:
                            continue
                        for attribute, present in (("nullable", f.getter and other.getter), ("patch_nullable", f.setter and other.setter)):
                            if present and getattr(f, attribute) != getattr(other, attribute):
                                setattr(f, attribute, True)
                                setattr(other, attribute, True)
                                changed = True
        for fired in self.events:
            for handler in fired.receivers:
                if handler is fired:
                    continue
                own = {f.name: f for f in fired.fields}
                for f in handler.fields:
                    other = own.get(f.name)
                    if other is None:
                        fail(f"{fired.name} lacks the field {f.name} of {handler.name}")
                    if f.getter and not other.getter:
                        fail(f"{fired.name}.{f.name} is not readable as in {handler.name}")
                    if f.setter and (not other.setter or other.patch_nullable != f.patch_nullable or not other.ty.same(f.ty) or other.set != f.set):
                        fail(f"{fired.name}.{f.name} cannot carry a patch of {handler.name}")
                    if f.getter and not projection(other, f):
                        fail(f"{fired.name}.{f.name} cannot be read as {handler.name}.{f.name}")

    def ordered_enums(self):
        return sorted(self.enums.values(), key=lambda definition: definition.name)


def listed(f):
    return f.ty.many and not f.index


def snapshot_field(f):
    return f"{f.name}_snapshot"


def list_diff(f):
    return f"{f.ty.leaf.base}ListDiff"


def replacement(f):
    return f"{f.ty.leaf.base}Replacement"


KIN = {"Item": "diff::same_item", "DroppedItem": "diff::same_entity"}


def patch_number(f):
    return f.number + DIFF_OFFSET if listed(f) else f.number


def wire_copy(event):
    return all(not f.ty.many and f.ty.leaf.wire_copy for f in event.readable)


def projection(source, target):
    if source.ty.many != target.ty.many:
        return None
    a, b = source.ty.leaf, target.ty.leaf
    if a.proto == b.proto:
        if source.ty.many:
            return "clone"
        if a.shape == "message":
            return "copy" if a.wire_copy else "clone"
        if source.nullable == target.nullable:
            return "copy" if a.wire_copy else "clone"
        if target.nullable:
            return "some_copy" if a.wire_copy else "some_clone"
        return None
    if a.proto == "PlayerSnapshot" and b.proto == "EntitySnapshot" and not source.ty.many:
        return "player_entity"
    return None


class Writer:
    def __init__(self):
        self.lines = []

    def __call__(self, line=""):
        self.lines.append(line)

    def text(self):
        return "\n".join(self.lines) + "\n"


def closure(template, var="value"):
    match = re.fullmatch(r"([\w:]+)\(\{\}\)", template)
    if match:
        return match[1]
    return f"|{var}| {template.format(var)}"


def apply(template, expression):
    return template.format(expression)


def identity(template):
    return template == "{}"


def entry_name(f):
    return f"{f.index.base}{f.ty.leaf.base}Entry"


def proto_payload(f):
    leaf = f.ty.leaf
    if f.index:
        return f"repeated {entry_name(f)}"
    if f.ty.many:
        return f"repeated {leaf.proto}"
    if f.nullable and leaf.shape != "message":
        return f"optional {leaf.proto}"
    return leaf.proto


def proto_patch(f):
    leaf = f.ty.leaf
    if f.index:
        return f"repeated {entry_name(f)}"
    if f.ty.many:
        return list_diff(f)
    if f.patch_nullable:
        return f"Nullable{leaf.base}"
    if leaf.shape == "message":
        return leaf.proto
    return f"optional {leaf.proto}"


def emit_proto(model):
    w = Writer()
    w('syntax = "proto3";')
    w("package rspigot.plugin.v1;")
    w()
    w('import "rspigot/plugin/v1/types.proto";')
    w()
    kinds = " ".join(f"EVENT_KIND_{screaming(event.name)} = {event.number};" for event in sorted(model.events, key=lambda e: e.number))
    if model.reserved:
        kinds += f" reserved {', '.join(str(number) for number in model.reserved)};"
    w(f"enum EventKind {{ EVENT_KIND_UNSPECIFIED = 0; {kinds} }}")
    for definition in model.ordered_enums():
        prefix = screaming(definition.name)
        values = " ".join(f"{prefix}_{constant} = {definition.numbers[constant]};" for constant in definition.constants)
        if definition.reserved:
            values += f" reserved {', '.join(str(number) for number in definition.reserved)};"
        w(f"enum {definition.name} {{ {prefix}_UNSPECIFIED = 0; {values} }}")
    w()
    wrappers = {}
    for event in model.events:
        for f in event.fields:
            if f.index:
                wrappers[entry_name(f)] = f"{f.index.proto} key = 1; {f.ty.leaf.proto} value = 2;"
        for f in event.patched:
            leaf = f.ty.leaf
            if f.index:
                continue
            if f.ty.many:
                wrappers[list_diff(f)] = f"repeated uint32 removed = 1; repeated {leaf.proto} added = 2; repeated {replacement(f)} replaced = 3;"
                wrappers[replacement(f)] = f"uint32 index = 1; {leaf.proto} value = 2;"
            elif f.patch_nullable:
                inner = leaf.proto if leaf.shape == "message" else f"optional {leaf.proto}"
                wrappers[f"Nullable{leaf.base}"] = f"{inner} value = 1;"
    for name in sorted(wrappers):
        w(f"message {name} {{ {wrappers[name]} }}")
    w()
    for event in sorted(model.events, key=lambda e: e.number):
        body = " ".join(f"{proto_payload(f)} {f.name} = {f.number};" for f in event.readable)
        w(f"message {event.name} {{{' ' + body + ' ' if body else ''}}}")
        if event.patched:
            body = " ".join(f"{proto_patch(f)} {f.name} = {patch_number(f)};" for f in event.patched)
            replaced = [str(f.number) for f in event.patched if listed(f)]
            if replaced:
                body += f" reserved {', '.join(replaced)};"
            w(f"message {event.name}Patch {{ {body} }}")
    w()
    payloads = ", ".join(str(number + PAYLOAD_OFFSET) for number in model.reserved)
    w("message Event {")
    w("  uint64 id = 1; repeated uint32 handlers = 2; Phase phase = 3; bool cancelled = 4; repeated BlockMeta block_meta = 5;")
    if payloads:
        w(f"  reserved {payloads};")
    w("  oneof payload {")
    for event in sorted(model.events, key=lambda e: e.number):
        w(f"    {event.name} {event.snake} = {event.payload_number};")
    w("  }")
    w("}")
    w("message Patch {")
    if payloads:
        w(f"  reserved {payloads};")
    w("  oneof change {")
    for event in sorted(model.events, key=lambda e: e.number):
        if event.patched:
            w(f"    {event.name}Patch {event.snake} = {event.payload_number};")
    w("  }")
    w("}")
    return w.text()


def core_type(f):
    leaf = f.ty.leaf
    if f.index:
        return f"Vec<({f.index.core}, {leaf.core})>"
    if f.ty.many:
        return f"Vec<{leaf.core}>"
    if f.nullable:
        return f"Option<{leaf.core}>"
    return leaf.core


def core_patch_type(f):
    leaf = f.ty.leaf
    if f.index:
        return f"Vec<({f.index.core}, {leaf.core})>"
    if f.ty.many:
        return f"{'SetDiff' if f.set else 'ListDiff'}<{leaf.core}>"
    if f.patch_nullable:
        return f"Option<Option<{leaf.core}>>"
    return f"Option<{leaf.core}>"


REFUSALS = ("icon_refusal", "location_refusal", "number_refusal", "text_refusal", "vector_refusal")


def value_check(f):
    proto = f.ty.leaf.proto
    name = f.name
    if f.limit:
        return f'self.{name}.as_deref().and_then(|value| text_refusal("{name}", value, {f.limit}))'
    if f.ty.leaf.id == "icon" and not f.ty.many and not f.index and not f.patch_nullable:
        return f'self.{name}.as_deref().and_then(|value| icon_refusal("{name}", value))'
    if proto == "Location":
        if listed(f):
            return f"self.{name}.added.iter().chain(self.{name}.replaced.iter().map(|(_, value)| value)).find_map(location_refusal)"
        if f.patch_nullable:
            return f"self.{name}.as_ref().and_then(Option::as_ref).and_then(location_refusal)"
        return f"self.{name}.as_ref().and_then(location_refusal)"
    checks = {
        "Vec3": ("vector_refusal", "{}"),
        "double": ("number_refusal", "{}"),
        "float": ("number_refusal", "f64::from({})"),
    }
    if proto not in checks:
        return None
    helper, value = checks[proto]
    if f.index:
        return f'self.{name}.iter().find_map(|(_, value)| {helper}("{name}", {value.format("*value")}))'
    if listed(f):
        return f'self.{name}.added.iter().chain(self.{name}.replaced.iter().map(|(_, value)| value)).find_map(|value| {helper}("{name}", {value.format("*value")}))'
    each = f'|value| {helper}("{name}", {value.format("value")})'
    if f.patch_nullable:
        return f"self.{name}.flatten().and_then({each})"
    return f"self.{name}.and_then({each})"


def refusal_check(f):
    values = value_check(f)
    if not listed(f):
        return values
    method = "slot_refusal" if f.ty.array else "refusal"
    index = f'self.{f.name}.{method}("{f.name}", event.{f.name}.len())'
    return index if values is None else f"{index}.or_else(|| {values})"


def reads_snapshot(e):
    return any(listed(f) for f in e.patched)


def emit_core(model):
    uses = {"std::fmt::Debug"} - {"std::fmt::Debug"}
    body = Writer()
    events = sorted(model.events, key=lambda e: e.name)
    for definition in model.ordered_enums():
        body("#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]")
        body(f"pub enum {definition.name} {{")
        for constant in definition.constants:
            body(f"    {pascal(constant)},")
        body("}")
        body()
    names = ", ".join(f"EventKind::{e.name}" for e in events)
    body("#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]")
    body("pub enum EventKind {")
    for e in events:
        body(f"    {e.name},")
    body("}")
    body()
    body("impl EventKind {")
    body(f"    pub const ALL: [Self; {len(events)}] = [{names.replace('EventKind::', 'Self::')}];")
    body()
    for method, test in (
        ("cancellable", lambda e: e.cancellable),
        ("decidable", lambda e: e.decidable),
        ("writes_when_cancelled", lambda e: e.writes_when_cancelled),
        ("cancels_by_result", lambda e: e.cancels_by_result),
    ):
        chosen = [e for e in events if test(e)]
        body(f"    pub const fn {method}(self) -> bool {{")
        if chosen:
            body(f"        matches!(self, {' | '.join('Self::' + e.name for e in chosen)})")
        else:
            body("        false")
        body("    }")
        body()
    body("    pub const fn name(self) -> &'static str {")
    body("        match self {")
    for e in events:
        body(f'            Self::{e.name} => "{e.name}",')
    body("        }")
    body("    }")
    body()
    body("    pub const fn key(self) -> &'static str {")
    body("        match self {")
    for e in events:
        body(f'            Self::{e.name} => "{e.snake}",')
    body("        }")
    body("    }")
    body()
    body("    pub fn from_key(key: &str) -> Option<Self> {")
    body("        Self::ALL.into_iter().find(|kind| kind.key() == key)")
    body("    }")
    body()
    body("    pub const fn receivers(self) -> &'static [Self] {")
    body("        match self {")
    for e in events:
        body(f"            Self::{e.name} => &[{', '.join('Self::' + r.name for r in sorted(e.receivers, key=lambda r: r.name))}],")
    body("        }")
    body("    }")
    body("}")
    body()
    for e in events:
        for f in e.fields:
            uses.update(f.ty.leaf.core_uses)
            if f.index:
                uses.update(f.index.core_uses)
        body("#[derive(Debug, Clone, PartialEq)]")
        if e.readable:
            body(f"pub struct {e.name} {{")
            for f in e.readable:
                body(f"    pub {f.name}: {core_type(f)},")
            body("}")
        else:
            body(f"pub struct {e.name};")
        body()
    for e in events:
        if not e.patched:
            continue
        body("#[derive(Debug, Clone, PartialEq, Default)]")
        body(f"pub struct {e.name}Patch {{")
        for f in e.patched:
            body(f"    pub {f.name}: {core_patch_type(f)},")
        body("}")
        body()
        body(f"impl {e.name}Patch {{")
        body("    fn then(self, later: Self) -> Self {")
        body("        Self {")
        for f in e.patched:
            if f.index:
                body(f"            {f.name}: merge_indexed(self.{f.name}, later.{f.name}),")
            elif listed(f):
                body(f"            {f.name}: self.{f.name}.then(later.{f.name}),")
            else:
                body(f"            {f.name}: later.{f.name}.or(self.{f.name}),")
        body("        }")
        body("    }")
        body()
        settled = [f for f in e.patched if f.getter]
        target = "event" if settled else "_event"
        body(f"    fn settle(self, {target}: &mut {e.name}) {{")
        for f in settled:
            if f.index:
                body(f"        event.{f.name} = merge_indexed(std::mem::take(&mut event.{f.name}), self.{f.name});")
                continue
            if listed(f):
                body(f"        event.{f.name} = self.{f.name}.apply(std::mem::take(&mut event.{f.name}));")
                continue
            value = "Some(value)" if f.nullable and not f.patch_nullable and not f.ty.many else "value"
            body(f"        if let Some(value) = self.{f.name} {{")
            body(f"            event.{f.name} = {value};")
            body("        }")
        body("    }")
        kept = e.kept_when_cancelled
        if kept:
            body()
            body("    fn when_cancelled(self) -> Option<Self> {")
            rest = ", ..Self::default()" if len(kept) < len(e.patched) else ""
            fields = ", ".join(f"{f.name}: self.{f.name}" for f in kept)
            body(f"        let kept = Self {{ {fields}{rest} }};")
            body("        (kept != Self::default()).then_some(kept)")
            body("    }")
        checks = [refusal_check(f) for f in e.patched if refusal_check(f)]
        if checks:
            body()
            body(f"    fn refusal(&self, event: &{e.name}) -> Option<String> {{" if reads_snapshot(e) else "    fn refusal(&self) -> Option<String> {")
            expression = checks[0] + "".join(f".or_else(|| {check})" for check in checks[1:])
            body(f"        {expression}")
            body("    }")
        body("}")
        body()
    body("#[allow(clippy::large_enum_variant)]")
    body("#[derive(Debug, Clone, PartialEq)]")
    body("pub enum EventPayload {")
    for e in events:
        body(f"    {e.name}({e.name}),")
    body("}")
    body()
    body("impl EventPayload {")
    body("    pub const fn kind(&self) -> EventKind {")
    body("        match self {")
    for e in events:
        body(f"            Self::{e.name}(_) => EventKind::{e.name},")
    body("        }")
    body("    }")
    body()
    by_result = [e for e in events if e.cancels_by_result]
    body("    pub const fn cancelled(&self) -> bool {")
    body("        match self {")
    for e in by_result:
        main = next(f for f in e.patched if f.name == e.main_result)
        body(f"            Self::{e.name}(event) => matches!(event.{main.name}, {main.ty.leaf.core}::{pascal(e.cancel_sets[main.name])}),")
    body("            _ => false,")
    body("        }")
    body("    }")
    body()
    body("    pub fn block_positions(&self, out: &mut Vec<BlockPos>) {")
    body("        match self {")
    for e in events:
        pushes = []
        for f in e.readable:
            if f.index or f.ty.leaf.id not in ("block", "block_snapshot"):
                continue
            tail = "" if f.ty.leaf.id == "block" else ".pos"
            deref = "*" if f.ty.leaf.id == "block" else ""
            if f.ty.many:
                mapper = ".copied()" if not tail else ".map(|item| item.pos)"
                pushes.append(f"                out.extend(event.{f.name}.iter(){mapper});")
            elif f.nullable:
                pushes.append(f"                if let Some(item) = &event.{f.name} {{ out.push({deref}item{tail}); }}")
            else:
                pushes.append(f"                out.push(event.{f.name}{tail});")
        if pushes:
            body(f"            Self::{e.name}(event) => {{")
            for line in pushes:
                body(line)
            body("            }")
    body("            _ => {}")
    body("        }")
    body("    }")
    body("}")
    body()
    patched = [e for e in events if e.patched]
    body("#[allow(clippy::large_enum_variant)]")
    body("#[derive(Debug, Clone, PartialEq)]")
    body("pub enum EventPatch {")
    for e in patched:
        body(f"    {e.name}({e.name}Patch),")
    body("}")
    body()
    body("impl EventPatch {")
    body("    pub const fn kind(&self) -> EventKind {")
    body("        match self {")
    for e in patched:
        body(f"            Self::{e.name}(_) => EventKind::{e.name},")
    body("        }")
    body("    }")
    body()
    body("    pub(crate) fn then(self, later: Self) -> Self {")
    body("        match (self, later) {")
    for e in patched:
        body(f"            (Self::{e.name}(earlier), Self::{e.name}(later)) => Self::{e.name}(earlier.then(later)),")
    if len(patched) > 1:
        body("            (_, later) => later,")
    body("        }")
    body("    }")
    body()
    body("    pub(crate) fn settle(self, payload: &mut EventPayload) {")
    body("        match (self, payload) {")
    for e in patched:
        body(f"            (Self::{e.name}(patch), EventPayload::{e.name}(event)) => patch.settle(event),")
    body("            _ => {}")
    body("        }")
    body("    }")
    body()
    body("    pub(crate) fn denial(kind: EventKind) -> Option<Self> {")
    body("        match kind {")
    for e in by_result:
        values = ", ".join(f"{name}: Some({next(f for f in e.patched if f.name == name).ty.leaf.core}::{pascal(constant)})" for name, constant in sorted(e.cancel_sets.items()))
        rest = ", ..Default::default()" if len(e.cancel_sets) < len(e.patched) else ""
        body(f"            EventKind::{e.name} => Some(Self::{e.name}({e.name}Patch {{ {values}{rest} }})),")
    body("            _ => None,")
    body("        }")
    body("    }")
    body()
    body("    pub(crate) fn when_cancelled(self) -> Option<Self> {")
    direct_events = [e for e in patched if e.kept_when_cancelled]
    body("        match self {")
    for e in direct_events:
        body(f"            Self::{e.name}(patch) => patch.when_cancelled().map(Self::{e.name}),")
    body("            _ => None,")
    body("        }")
    body("    }")
    body()
    located_events = [e for e in patched if any(refusal_check(f) for f in e.patched)]
    body("    pub(crate) fn refusal(&self, payload: &EventPayload) -> Option<String> {")
    body("        match (self, payload) {")
    for e in located_events:
        if reads_snapshot(e):
            body(f"            (Self::{e.name}(patch), EventPayload::{e.name}(event)) => patch.refusal(event),")
        else:
            body(f"            (Self::{e.name}(patch), _) => patch.refusal(),")
    body("            _ => None,")
    body("        }")
    body("    }")
    body()
    body("    pub(crate) fn lift(self, to: EventKind) -> Option<Self> {")
    body("        match (self, to) {")
    for e in patched:
        body(f"            (patch @ Self::{e.name}(_), EventKind::{e.name}) => Some(patch),")
    for fired in events:
        for handler in fired.receivers:
            if handler is fired or not handler.patched:
                continue
            fields = ", ".join(f"{f.name}: patch.{f.name}" for f in handler.patched)
            rest = ", ..Default::default()" if len(handler.patched) < len(fired.patched) else ""
            body(f"            (Self::{handler.name}(patch), EventKind::{fired.name}) => Some(Self::{fired.name}({fired.name}Patch {{ {fields}{rest} }})),")
    body("            _ => None,")
    body("        }")
    body("    }")
    body("}")
    head = Writer()
    for use in sorted(uses):
        head(f"use {use};")
    helpers = sorted({name for e in located_events for f in e.patched for name in REFUSALS if name in (refusal_check(f) or "")})
    if any(f.index for e in patched for f in e.patched):
        helpers.append("merge_indexed")
    if any(listed(f) and not f.set for e in patched for f in e.patched):
        helpers.append("ListDiff")
    if any(listed(f) and f.set for e in patched for f in e.patched):
        helpers.append("SetDiff")
    if helpers:
        head(f"use crate::plugins::patch::{{{', '.join(sorted(helpers))}}};")
    head()
    return head.text() + body.text()


class Usage:
    def __init__(self):
        self.out = set()
        self.into = set()
        self.uses = set()


def host_out(leaf, expression, usage):
    if leaf.enum:
        usage.out.add(leaf.enum.java)
    return apply(leaf.host_out, expression)


def host_out_closure(leaf, usage):
    if leaf.enum:
        usage.out.add(leaf.enum.java)
    return closure(leaf.host_out)


def host_in_closure(leaf, usage):
    if leaf.enum:
        usage.into.add(leaf.enum.java)
    return closure(leaf.host_in)


def host_payload_field(f, usage):
    leaf = f.ty.leaf
    value = f"event.{f.name}"
    if f.index:
        key = host_out(f.index, "key", usage)
        each = host_out(leaf, "value", usage)
        assigned = "value" if each == "value" else f"value: {each}"
        return f"{value}.into_iter().map(|(key, value)| v1::{pn(entry_name(f))} {{ key: Some({key}), {assigned} }}).collect()"
    if identity(leaf.host_out):
        if not f.ty.many and not f.nullable and leaf.shape == "message":
            return f"Some({value})"
        return value
    if leaf.shape == "enum":
        each = f"|value| {host_out(leaf, 'value', usage)} as i32"
        if f.ty.many:
            return f"{value}.into_iter().map({each}).collect()"
        if f.nullable:
            return f"{value}.map({each})"
        return f"{host_out(leaf, value, usage)} as i32"
    if f.ty.many:
        return f"{value}.into_iter().map({host_out_closure(leaf, usage)}).collect()"
    if f.nullable:
        return f"{value}.map({host_out_closure(leaf, usage)})"
    if leaf.shape == "message":
        return f"Some({host_out(leaf, value, usage)})"
    if leaf.shape == "enum":
        return f"{host_out(leaf, value, usage)} as i32"
    return host_out(leaf, value, usage)


def host_patch_field(f, usage):
    leaf = f.ty.leaf
    value = f"patch.{f.name}"
    if f.index:
        if f.index.enum or leaf.enum:
            fail(f"{f.name}: an indexed field cannot hold an enum")
        key = apply(f.index.host_in, "shared::required(entry.key)?") + ("?" if f.index.host_in_try else "")
        each = apply(leaf.host_in, "entry.value") + ("?" if leaf.host_in_try else "")
        return f"{value}.into_iter().map(|entry| Ok(({key}, {each}))).collect::<Result<Vec<_>, Malformed>>()?"
    convert = host_in_closure(leaf, usage)
    simple_in = identity(leaf.host_in)
    tail = ".transpose()?" if leaf.host_in_try else ""
    if f.ty.many:
        if simple_in:
            added = "diff.added"
        elif leaf.host_in_try:
            added = f"diff.added.into_iter().map({convert}).collect::<Result<Vec<_>, _>>()?"
        else:
            added = f"diff.added.into_iter().map({convert}).collect()"
        wire = "shared::required(entry.value)?" if leaf.shape == "message" else "entry.value"
        each = apply(leaf.host_in, wire) + ("?" if leaf.host_in_try else "")
        replaced = f"diff.replaced.into_iter().map(|entry| Ok((entry.index, {each}))).collect::<Result<Vec<_>, Malformed>>()?"
        return f"match {value} {{ Some(diff) => ListDiff {{ removed: diff.removed, replaced: {replaced}, added: {added} }}, None => ListDiff::default() }}"
    if f.patch_nullable:
        if simple_in:
            return f"{value}.map(|wrapped| wrapped.value)"
        inner = ".transpose()" if leaf.host_in_try else ""
        return f"{value}.map(|wrapped| wrapped.value.map({convert}){inner}){tail}"
    if simple_in:
        return value
    return f"{value}.map({convert}){tail}"


def emit_host(model):
    usage = Usage()
    events = sorted(model.events, key=lambda e: e.name)
    body = Writer()
    body("pub(super) fn event_kind(value: i32) -> Option<EventKind> {")
    body("    Some(match v1::EventKind::try_from(value).ok()? {")
    for e in events:
        body(f"        v1::EventKind::{pn(e.name)} => EventKind::{e.name},")
    body("        v1::EventKind::Unspecified => return None,")
    body("    })")
    body("}")
    body()
    body("pub(super) fn wire_payload(payload: EventPayload) -> event::Payload {")
    body("    match payload {")
    for e in events:
        if e.readable:
            body(f"        EventPayload::{e.name}(event) => event::Payload::{pn(e.name)}(v1::{pn(e.name)} {{")
            for f in e.readable:
                body(f"            {f.name}: {host_payload_field(f, usage)},")
            body("        }),")
        else:
            body(f"        EventPayload::{e.name}(_) => event::Payload::{pn(e.name)}(v1::{pn(e.name)} {{}}),")
    body("    }")
    body("}")
    body()
    patched = [e for e in events if e.patched]
    body("pub(super) fn change(change: patch::Change) -> Result<EventPatch, Malformed> {")
    body("    Ok(match change {")
    for e in patched:
        body(f"        patch::Change::{pn(e.name)}(patch) => EventPatch::{e.name}({e.name}Patch {{")
        for f in e.patched:
            body(f"            {f.name}: {host_patch_field(f, usage)},")
        body("        }),")
    body("    })")
    body("}")
    for definition in model.ordered_enums():
        name = definition.name
        if definition.java in usage.out:
            body()
            body(f"const fn {snake(name)}_to_wire(value: {name}) -> v1::{pn(name)} {{")
            body("    match value {")
            for constant in definition.constants:
                body(f"        {name}::{pascal(constant)} => v1::{pn(name)}::{pv(constant)},")
            body("    }")
            body("}")
        if definition.java in usage.into:
            body()
            body(f"fn {snake(name)}_from_wire(value: i32) -> Result<{name}, Malformed> {{")
            body(f"    Ok(match v1::{pn(name)}::try_from(value) {{")
            for constant in definition.constants:
                body(f"        Ok(v1::{pn(name)}::{pv(constant)}) => {name}::{pascal(constant)},")
            body(f'        Ok(v1::{pn(name)}::Unspecified) | Err(_) => return Err(Malformed::Enum {{ field: "{snake(name)}", value }}),')
            body("    })")
            body("}")
    enum_names = sorted({model.enums[java].name for java in usage.out | usage.into})
    core_names = sorted({"EventKind", "EventPatch", "EventPayload"} | {f"{e.name}Patch" for e in patched} | set(enum_names))
    head = Writer()
    head(f"use rspigot_core::plugins::events::{{{', '.join(core_names)}}};")
    if any(listed(f) for e in patched for f in e.patched):
        head("use rspigot_core::plugins::ListDiff;")
    head("use rspigot_plugin_protocol::v1::{self, event, patch};")
    head()
    sources = {"block", "contract", "entity", "player", "shared"}
    head("use super::Malformed;")
    used = sorted(source for source in sources if re.search(rf"\b{source}::", body.text()))
    if used:
        head(f"use super::{{{', '.join(used)}}};")
    head()
    return head.text() + body.text()


def sdk_type(f):
    leaf = f.ty.leaf
    if f.index:
        return f"Vec<({f.index.sdk}, {leaf.sdk})>"
    if f.ty.many:
        return f"Vec<{leaf.sdk}>"
    if f.nullable:
        return f"Option<{leaf.sdk}>"
    return leaf.sdk


def sdk_setter_type(f):
    leaf = f.ty.leaf
    if f.ty.many:
        return f"Vec<{leaf.sdk}>"
    if f.patch_nullable:
        return f"Option<{leaf.sdk}>"
    return leaf.sdk


def sdk_getter(f):
    leaf = f.ty.leaf
    if f.ty.many:
        return f"&[{leaf.sdk}]", f"&self.{f.name}"
    stringy = leaf.sdk == "String"
    if f.nullable:
        if leaf.sdk_copy:
            return f"Option<{leaf.sdk}>", f"self.{f.name}"
        if stringy:
            return "Option<&str>", f"self.{f.name}.as_deref()"
        return f"Option<&{leaf.sdk}>", f"self.{f.name}.as_ref()"
    if leaf.sdk_copy:
        return leaf.sdk, f"self.{f.name}"
    if stringy:
        return "&str", f"&self.{f.name}"
    return f"&{leaf.sdk}", f"&self.{f.name}"


def sdk_in_field(f, usage):
    leaf = f.ty.leaf
    value = f"message.{f.name}"
    if f.index:
        usage.uses.add("crate::wire::required")
        usage.uses.update(f.index.sdk_uses)
        key = apply(f.index.sdk_in, f'required(entry.key, "{f.name}")?') + ("?" if f.index.sdk_in_try else "")
        each = apply(leaf.sdk_in, "entry.value") + ("?" if leaf.sdk_in_try else "")
        return f"{value}.into_iter().map(|entry| Ok(({key}, {each}))).collect::<Result<Vec<_>, Error>>()?"
    if leaf.enum:
        usage.into.add(leaf.enum.java)
    usage.uses.update(use for use in leaf.sdk_uses if not use.endswith("_to_wire"))
    tail = "?" if leaf.sdk_in_try else ""
    convert = closure(leaf.sdk_in)
    if f.ty.many:
        if identity(leaf.sdk_in):
            return value
        if leaf.sdk_in_try:
            return f"{value}.into_iter().map({convert}).collect::<Result<Vec<_>, _>>()?"
        return f"{value}.into_iter().map({convert}).collect()"
    if leaf.shape == "message" and not f.nullable:
        usage.uses.add("crate::wire::required")
        return apply(leaf.sdk_in, f'required({value}, "{f.name}")?') + tail
    if f.nullable:
        if identity(leaf.sdk_in):
            return value
        return f"{value}.map({convert})" + (".transpose()?" if leaf.sdk_in_try else "")
    return apply(leaf.sdk_in, value) + tail


def sdk_out_patch(f, usage):
    leaf = f.ty.leaf
    if leaf.enum:
        usage.out.add(leaf.enum.java)
    usage.uses.update(leaf.sdk_uses)
    cloned = leaf.sdk_out == "{}.clone()"
    plain = identity(leaf.sdk_out)
    if f.patch_nullable:
        if plain and leaf.sdk_copy:
            inner = "value"
        elif plain or cloned:
            inner = "value.clone()"
        elif leaf.sdk_copy:
            inner = f"value.map({closure(leaf.sdk_out)})"
        else:
            inner = f"value.as_ref().map({closure(leaf.sdk_out)})"
        return f"Some(v1::{pn("Nullable" + leaf.base)} {{ value: {inner} }})"
    return f"Some({apply(leaf.sdk_out, 'value')})"


def sdk_wire_lists(f, usage):
    leaf = f.ty.leaf
    if leaf.enum:
        usage.out.add(leaf.enum.java)
    usage.uses.update(leaf.sdk_uses)
    usage.uses.add("crate::diff")
    if identity(leaf.sdk_out) or leaf.sdk_out == "{}.clone()":
        return "snapshot", "value.clone()"
    mapped = f".iter(){'.copied()' if leaf.sdk_copy else ''}.map({closure(leaf.sdk_out)})"
    return f"&snapshot{mapped}.collect::<Vec<_>>()", f"value{mapped}.collect()"


def emit_sdk(model):
    usage = Usage()
    events = sorted(model.events, key=lambda e: e.name)
    body = Writer()
    for definition in model.ordered_enums():
        body("#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]")
        body(f"pub enum {definition.name} {{")
        for constant in definition.constants:
            body(f"    {pascal(constant)},")
        body("}")
        body()
    body("#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]")
    body("pub enum EventKind {")
    for e in events:
        body(f"    {e.name},")
    body("}")
    body()
    body("impl EventKind {")
    body("    pub(crate) const fn to_wire(self) -> v1::EventKind {")
    body("        match self {")
    for e in events:
        body(f"            Self::{e.name} => v1::EventKind::{pn(e.name)},")
    body("        }")
    body("    }")
    body("}")
    body()
    for e in events:
        body("#[derive(Debug)]" if e.decidable else "#[derive(Debug, Clone)]")
        body(f"pub struct {e.name} {{")
        body("    id: u64,")
        if e.cancellable:
            if not e.cancels_by_result:
                body("    cancelled: bool,")
            body("    rolled_back: bool,")
        for f in e.readable:
            body(f"    {f.name}: {sdk_type(f)},")
        for f in e.patched:
            if listed(f):
                body(f"    {snapshot_field(f)}: Option<{sdk_type(f)}>,")
        if e.patched:
            body(f"    patch: v1::{pn(e.name + "Patch")},")
        body("}")
        body()
        body(f"impl {e.name} {{")
        values = [(f.name, sdk_in_field(f, usage)) for f in e.readable]
        link = "link" if any(re.search(r"\blink\b", value) for _, value in values) else "_link"
        message = "message" if values else "_message"
        body(f"    fn from_wire({message}: v1::{pn(e.name)}, meta: Meta, {link}: &Link) -> Result<Self, Error> {{")
        body("        Ok(Self {")
        body("            id: meta.id,")
        if e.cancellable:
            if not e.cancels_by_result:
                body("            cancelled: meta.cancelled,")
            body("            rolled_back: meta.rolled_back,")
        for name, value in values:
            body(f"            {name}: {value},")
        for f in e.patched:
            if listed(f):
                body(f"            {snapshot_field(f)}: None,")
        if e.patched:
            body(f"            patch: v1::{pn(e.name + "Patch")}::default(),")
        body("        })")
        body("    }")
        body()
        body("    pub const fn id(&self) -> u64 {")
        body("        self.id")
        body("    }")
        for f in e.readable:
            if f.index:
                body()
                body(f"    pub fn {f.method}(&self, key: &{f.index.sdk}) -> {f.ty.leaf.sdk} {{")
                body(f"        self.{f.name}.iter().find(|(known, _)| known.entity_id == key.entity_id).map(|(_, value)| *value).unwrap_or_default()")
                body("    }")
                continue
            returns, expression = sdk_getter(f)
            body()
            body(f"    pub fn {f.method}(&self) -> {returns} {{")
            body(f"        {expression}")
            body("    }")
            if f.actor:
                usage.uses.add("crate::Player")
                body()
                body("    pub fn player(&self) -> Option<&Player> {")
                if f.nullable:
                    body(f"        self.{f.name}.as_ref().and_then(Entity::player)")
                else:
                    body(f"        self.{f.name}.player()")
                body("    }")
        for f in e.patched:
            if f.index:
                body()
                body(f"    pub fn {f.setter_method}(&mut self, key: &{f.index.sdk}, value: {f.ty.leaf.sdk}) {{")
                body(f"        let entry = v1::{pn(entry_name(f))} {{ key: Some({apply(f.index.sdk_out, 'key')}), value }};")
                body(f"        match self.patch.{f.name}.iter_mut().find(|known| known.key.as_ref().is_some_and(|known| known.entity_id == key.entity_id)) {{")
                body("            Some(known) => *known = entry,")
                body(f"            None => self.patch.{f.name}.push(entry),")
                body("        }")
                body(f"        match self.{f.name}.iter_mut().find(|(known, _)| known.entity_id == key.entity_id) {{")
                body("            Some((_, known)) => *known = value,")
                body(f"            None => self.{f.name}.push((key.clone(), value)),")
                body("        }")
                body("    }")
                continue
            if listed(f):
                before, after = sdk_wire_lists(f, usage)
                body()
                body(f"    pub fn {f.setter_method}(&mut self, value: {sdk_setter_type(f)}) {{")
                body(f"        let snapshot = self.{snapshot_field(f)}.get_or_insert_with(|| std::mem::take(&mut self.{f.name}));")
                if f.ty.array:
                    diffed = f"diff::slots({before}, {after})"
                elif f.ty.leaf.proto in KIN:
                    diffed = f"diff::list_by({before}, {after}, {KIN[f.ty.leaf.proto]})"
                else:
                    diffed = f"diff::list({before}, {after})"
                value = "value: Some(value)" if f.ty.leaf.shape == "message" else "value"
                body(f"        self.patch.{f.name} = {diffed}.map(|diff| v1::{pn(list_diff(f))} {{ removed: diff.removed, replaced: diff.replaced.into_iter().map(|(index, value)| v1::{pn(replacement(f))} {{ index, {value} }}).collect(), added: diff.added }});")
                body(f"        self.{f.name} = value;")
                body("    }")
                continue
            body()
            body(f"    pub fn {f.setter_method}(&mut self, value: {sdk_setter_type(f)}) {{")
            body(f"        self.patch.{f.name} = {sdk_out_patch(f, usage)};")
            if f.getter:
                assigned = "Some(value)" if f.nullable and not f.patch_nullable and not f.ty.many else "value"
                body(f"        self.{f.name} = {assigned};")
            body("    }")
        if e.cancellable:
            body()
            if e.cancels_by_result:
                body("    pub fn cancel(&mut self) {")
                for name, constant in sorted(e.cancel_sets.items()):
                    target = next(f for f in e.patched if f.name == name)
                    body(f"        self.{target.setter_method}({target.ty.leaf.sdk}::{pascal(constant)});")
                body("    }")
                body()
                main = next(f for f in e.patched if f.name == e.main_result)
                body("    pub fn is_cancelled(&self) -> bool {")
                body(f"        self.{main.name} == {main.ty.leaf.sdk}::{pascal(e.cancel_sets[main.name])}")
                body("    }")
            else:
                body("    pub const fn cancel(&mut self) {")
                body("        self.cancelled = true;")
                body("    }")
                body()
                body("    pub const fn is_cancelled(&self) -> bool {")
                body("        self.cancelled")
                body("    }")
            body()
            body("    pub const fn is_rolled_back(&self) -> bool {")
            body("        self.rolled_back")
            body("    }")
        body("}")
        body()
        if e.decidable:
            body(f"impl From<{e.name}> for Verdict {{")
            body(f"    fn from(event: {e.name}) -> Self {{")
            cancel = "event.cancelled" if e.cancellable and not e.cancels_by_result else "false"
            if e.patched:
                body("        Self {")
                body(f"            cancel: {cancel},")
                body(f"            patch: (event.patch != v1::{pn(e.name + "Patch")}::default()).then(|| {{")
                body("                Box::new(v1::Patch {")
                body(f"                    change: Some(patch::Change::{pn(e.name)}(event.patch)),")
                body("                })")
                body("            }),")
                body("        }")
            else:
                body("        Self {")
                body(f"            cancel: {cancel},")
                body("            patch: None,")
                body("        }")
            body("    }")
            body("}")
            body()
    body("#[allow(clippy::large_enum_variant)]")
    body("#[derive(Debug)]")
    body("pub enum Call {")
    for e in events:
        body(f"    {e.name} {{ handler: u32, event: {e.name} }},")
    body("}")
    body()
    projections = []
    for fired in events:
        for handler in fired.receivers:
            if handler is not fired:
                projections.append((fired, handler))
    body("impl Received {")
    direct = [e for e in events if e.writes_when_cancelled]
    body("    pub(crate) const fn writes_when_cancelled(&self) -> bool {")
    if direct:
        body(f"        matches!(self.payload, {' | '.join(f'event::Payload::{pn(e.name)}(_)' for e in direct)})")
    else:
        body("        false")
    body("    }")
    body()
    body("    pub(crate) fn call(&self, handler: u32, kind: EventKind, link: &Link) -> Option<Result<Call, Error>> {")
    body("        let meta = self.meta;")
    body("        Some(match (&self.payload, kind) {")
    for e in events:
        copied = "*message" if wire_copy(e) else "message.clone()"
        body(f"            (event::Payload::{pn(e.name)}(message), EventKind::{e.name}) => {e.name}::from_wire({copied}, meta, link).map(|event| Call::{e.name} {{ handler, event }}),")
    for fired, handler in projections:
        body(f"            (event::Payload::{pn(fired.name)}(message), EventKind::{handler.name}) => {handler.name}::from_wire(project_{fired.snake}_{handler.snake}(message), meta, link).map(|event| Call::{handler.name} {{ handler, event }}),")
    body("            _ => return None,")
    body("        })")
    body("    }")
    body("}")
    for fired, handler in projections:
        own = {f.name: f for f in fired.fields}
        body()
        body(f"fn project_{fired.snake}_{handler.snake}(message: &v1::{pn(fired.name)}) -> v1::{pn(handler.name)} {{")
        if not handler.readable:
            body(f"    let _ = message;")
            body(f"    v1::{pn(handler.name)} {{}}")
            body("}")
            continue
        body(f"    v1::{pn(handler.name)} {{")
        for f in handler.readable:
            how = projection(own[f.name], f)
            value = f"message.{f.name}"
            expression = {
                "clone": f"{value}.clone()",
                "copy": value,
                "some_copy": f"Some({value})",
                "some_clone": f"Some({value}.clone())",
                "player_entity": f"{value}.as_ref().map(player_entity)",
            }[how]
            if how == "player_entity":
                usage.uses.add("crate::wire::player_entity")
            body(f"        {f.name}: {expression},")
        body("    }")
        body("}")
    for definition in model.ordered_enums():
        name = definition.name
        reads = definition.java in usage.into or name in SDK_ALWAYS_READ
        writes = definition.java in usage.out
        if not reads and not writes:
            continue
        body()
        body(f"impl {name} {{")
        if reads:
            body(f"    pub(crate) fn from_wire(value: i32) -> Result<Self, Error> {{")
            body(f"        match v1::{pn(name)}::try_from(value) {{")
            for constant in definition.constants:
                body(f"            Ok(v1::{pn(name)}::{pv(constant)}) => Ok(Self::{pascal(constant)}),")
            body(f'            Ok(v1::{pn(name)}::Unspecified) | Err(_) => Err(Error::Malformed(format!("an unknown {snake(name).replace("_", " ")} {{value}}"))),')
            body("        }")
            body("    }")
        if writes:
            if reads:
                body()
            body("    pub(crate) const fn to_wire(self) -> i32 {")
            body("        (match self {")
            for constant in definition.constants:
                body(f"            Self::{pascal(constant)} => v1::{pn(name)}::{pv(constant)},")
            body("        }) as i32")
            body("    }")
        body("}")
    head = Writer()
    head("use rspigot_plugin_protocol::v1::{self, event, patch};")
    head()
    uses = sorted(use for use in usage.uses if not re.fullmatch(r"crate::(\w+)", use) or use.split("::")[1] not in {d.name for d in model.enums.values()})
    for use in uses:
        head(f"use {use};")
    head("use crate::link::Link;")
    head("use crate::listener::Verdict;")
    head("use crate::runtime::{Meta, Received};")
    head("use crate::Error;")
    head()
    return head.text() + body.text()


def emit_macro(model):
    events = sorted(model.events, key=lambda e: e.name)
    w = Writer()
    w(f"pub(crate) const EVENTS: [(&str, bool); {len(events)}] = [")
    for e in events:
        w(f'    ("{e.name}", {"true" if e.decidable else "false"}),')
    w("];")
    return w.text()


def emit_test(model):
    events = sorted(model.events, key=lambda e: e.name)
    deciding = [e for e in events if e.decidable]
    w = Writer()
    w("use rspigot_plugin::prelude::*;")
    w("use rspigot_plugin::{Access, HandlerSpec};")
    w()
    w("#[derive(Plugin)]")
    w("struct Everything;")
    w()
    w("#[listeners]")
    w("impl Everything {")
    for e in events:
        w(f"    async fn observe_{e.snake}(&self, event: &rspigot_plugin::{e.name}) {{")
        w("        let _ = event.id();")
        w("    }")
        w()
    for e in deciding:
        w(f"    async fn decide_{e.snake}(&self, event: &mut rspigot_plugin::{e.name}) {{")
        w("        let _ = event.id();")
        w("    }")
        w()
    w("}")
    w()
    w("#[test]")
    w("fn every_event_takes_an_observer_and_every_decidable_one_a_decider() {")
    w("    let handlers: Vec<HandlerSpec> = <Everything as Listeners>::handlers();")
    w(f"    assert_eq!(handlers.len(), {len(events) + len(deciding)});")
    w("    let deciding = handlers.iter().filter(|spec| spec.access == Access::Decide).count();")
    w(f"    assert_eq!(deciding, {len(deciding)});")
    w("}")
    return w.text()


def setter_note(f):
    if f.direct:
        return " (applies even when cancelled)"
    if listed(f) and not f.set and not f.ty.array:
        return " (a reorder alone changes nothing)"
    return ""


def emit_doc(model):
    events = sorted(model.events, key=lambda e: e.bukkit)
    w = Writer()
    w("# Plugin events")
    w()
    w(f"Generated by `tools/gen_plugin_events.py` from `tools/plugin-events/catalog.json` (the Spigot API jar) and `tools/plugin-events/overrides.toml`. Do not edit.")
    w()
    fired = sum(e.state == "fired" for e in events)
    w(f"{len(events)} events, {sum(e.decidable for e in events)} decidable, {sum(e.cancellable for e in events)} cancellable, {sum(bool(e.patched) for e in events)} with setters. {fired} are fired by the server today; the others have their types and wait for their fire point.")
    w()
    w("An observing handler takes `&Event`. A deciding handler takes `&mut Event` and exists only for a decidable event (cancellable or with setters). `cancel()` exists only on a cancellable event. A handler also receives the events of the classes that share its Bukkit handler list and extend its event (column \"Also receives\"), as its own type.")
    w()
    by_result = sorted((e for e in events if e.cancels_by_result), key=lambda e: e.name)
    w(f"{', '.join(f'`{e.name}`' for e in by_result)} are cancelled through their result, as in Bukkit: `cancel()` sets {', '.join(sorted({f'`{name}`' for e in by_result for name in e.cancel_sets}))} to `Deny`, `is_cancelled()` reads the main result only, and a later handler that sets the result back lifts the cancel.")
    w()
    listed_setters = sorted({f.setter_method for e in events for f in e.patched if listed(f) and not f.ty.array})
    slot_setters = sorted({f"`{e.name}.{f.setter_method}`" for e in events for f in e.patched if listed(f) and f.ty.array})
    set_setters = sorted({f"`{e.name}.{f.setter_method}`" for e in events for f in e.patched if listed(f) and f.set})
    arrays = f" An array setter ({', '.join(slot_setters)}) keeps the length of the array: it only replaces elements in place." if slot_setters else ""
    w(f"A list setter ({', '.join(f'`{name}`' for name in listed_setters)}) sends what the handler changed in the list it received, in three parts: the elements it removed, the elements it changed in place (an item stack of the same item at the same place, a dropped item of the same entity) and the elements it added. The server folds the handlers in handler order: an element any handler removed is gone even if another handler changed it, the last handler to change an element wins it, and additions are appended in handler order. Every handler receives the original list, so a handler that wants an element there whatever the earlier handlers did pushes it again. A reorder alone carries nothing: the list keeps the order the server sent.{arrays}")
    w()
    w(f"These setters write sets, where an element added twice is there once: {', '.join(set_setters)}.")
    w()
    w("| Bukkit event | SDK type | Cancellable | Setters | Also receives | State |")
    w("|---|---|---|---|---|---|")
    for e in events:
        setters = ", ".join(f"`{f.setter_method}`" + setter_note(f) for f in e.patched) or "-"
        also = ", ".join(f"`{other.name}`" for other in sorted(model.events, key=lambda o: o.name) if e in other.receivers and other is not e) or "-"
        if e.state == "fired":
            state = "fired" + (f": {e.state_note}" if e.state_note else "")
        elif e.state == "ready":
            state = "not fired yet: its fire point comes with its domain"
        else:
            state = f"not fired yet: needs {e.state_note}"
        w(f"| `{e.bukkit}Event` | `{e.name}` | {'yes' if e.cancellable else 'no'} | {setters} | {also} | {state} |")
    w()
    w("## Left out")
    w()
    w("Accessors that have no field, with the reason.")
    w()
    w("| SDK type | Bukkit accessor | Reason |")
    w("|---|---|---|")
    for e in sorted(model.events, key=lambda e: e.name):
        for accessor, reason in e.left_out:
            w(f"| `{e.name}` | `{accessor}` | {reason} |")
    w()
    w("## Not generated")
    w()
    w("| Bukkit event | Reason |")
    w("|---|---|")
    for name, reason in sorted(model.excluded.items()):
        w(f"| `{name}Event` | {reason} |")
    return w.text()


def rustfmt(text, kind):
    result = subprocess.run(
        ["rustfmt", "--edition", "2024", "--emit", "stdout", "--quiet"],
        input=text, capture_output=True, text=True, cwd=REPO,
    )
    if result.returncode != 0:
        fail(f"rustfmt rejected the {kind} output: {result.stderr}")
    return result.stdout


def read(root, name):
    return root.joinpath(*INPUTS[name]).read_text()


def generate(root):
    catalog = json.loads(read(root, "catalog"))
    overrides = tomllib.loads(read(root, "overrides"))
    model = Model(catalog, overrides, read(root, "types") + read(root, "plugin"), read(root, "library"))
    texts = {
        "proto": emit_proto(model),
        "core": emit_core(model),
        "host": emit_host(model),
        "sdk": emit_sdk(model),
        "macro": emit_macro(model),
        "test": emit_test(model),
        "doc": emit_doc(model),
    }
    for kind in RUST:
        texts[kind] = rustfmt(texts[kind], kind)
    return model, texts


def check_dump(path, catalog):
    dump = json.loads(Path(path).read_text())
    known = {event["java"]: event for event in catalog["events"]}
    problems = []
    for event in dump["events"]:
        mine = known.get(event["event"])
        if mine is None:
            problems.append(f"{event['event']} is listenable in the API jar but missing from the catalog")
            continue
        if mine["deprecated"]:
            problems.append(f"{event['event']} is deprecated in the catalog but not in the dump")
        for key in ("getters", "setters", "indexed", "other"):
            theirs = sorted((entry["name"], tuple(entry.get("params", []))) for entry in event[key])
            ours = sorted((entry["name"], tuple(entry.get("params", []))) for entry in mine[key])
            if theirs != ours:
                problems.append(f"{event['event']}: its {key} differ from the catalog")
    return problems


def main():
    parser = argparse.ArgumentParser(description="generate every plugin event from the catalog")
    parser.add_argument("--check", action="store_true", help="fail when a generated file is stale")
    parser.add_argument("--check-dump", metavar="EVENT_TYPES_JSON", help="fail when the API dump has an event the catalog lacks")
    parser.add_argument("--root", type=Path, default=REPO, help="the repository to read and write (default: this one)")
    args = parser.parse_args()
    root = args.root.resolve()
    try:
        if args.check_dump:
            problems = check_dump(args.check_dump, json.loads(read(root, "catalog")))
            for problem in problems:
                print(problem, file=sys.stderr)
            return 1 if problems else 0
        model, texts = generate(root)
    except GenError as error:
        print(f"gen_plugin_events: {error}", file=sys.stderr)
        return 1
    targets = {kind: root.joinpath(*OUTPUTS[kind]) for kind in texts}
    if args.check:
        stale = [kind for kind, text in texts.items() if not targets[kind].exists() or targets[kind].read_text() != text]
        for kind in stale:
            print(f"{'/'.join(OUTPUTS[kind])} is stale, run `python3 -I tools/gen_plugin_events.py`", file=sys.stderr)
        return 1 if stale else 0
    for kind, text in texts.items():
        targets[kind].parent.mkdir(parents=True, exist_ok=True)
        targets[kind].write_text(text)
    decidable = sum(e.decidable for e in model.events)
    print(f"{len(model.events)} events, {decidable} decidable, {sum(e.cancellable for e in model.events)} cancellable, {sum(bool(e.patched) for e in model.events)} with setters")
    return 0


if __name__ == "__main__":
    sys.exit(main())

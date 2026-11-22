import argparse
import json
import math
import os
import re
import sys

RE_ROOT = "/Users/baptiste.parmantier/Development/rspigot/re"
DEFAULT_EFFECTS = RE_ROOT + "/datagen/generated/reports/mob_effects.json"
DEFAULT_REGISTRIES = RE_ROOT + "/datagen/generated/reports/registries.json"
CRATE = os.path.normpath(
    os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "crates", "rspigot-effects")
)
DEFAULT_FAMILIES = os.path.join(CRATE, "families.txt")
EXPECTED_VERSION = "26.3"
REGISTRY = "minecraft:mob_effect"

CATEGORIES = {
    "BENEFICIAL": "Beneficial",
    "HARMFUL": "Harmful",
    "NEUTRAL": "Neutral",
}

OPERATIONS = {
    "add_value": "AddValue",
    "add_multiplied_base": "AddMultipliedBase",
    "add_multiplied_total": "AddMultipliedTotal",
}

RESERVED_STRUCTS = {
    "AttributeOperation", "AttributeTemplate", "EffectBehavior", "EffectCategory", "EffectId",
    "EffectKind", "MobEffect", "Self", "Option", "Some", "None", "Result", "Ok", "Err", "Vec",
    "String", "Box", "Default", "Clone", "Copy", "Send", "Sync", "Sized", "Drop", "Fn", "FnMut",
    "FnOnce", "Iterator", "IntoIterator", "Extend", "ToString", "ToOwned", "PartialEq", "Eq",
    "PartialOrd", "Ord", "AsRef", "AsMut", "Into", "From", "TryFrom", "TryInto", "FromIterator",
    "DoubleEndedIterator", "ExactSizeIterator", "Unpin",
}

RUST_KEYWORDS = {
    "as", "break", "const", "continue", "crate", "else", "enum", "extern", "false", "fn", "for",
    "if", "impl", "in", "let", "loop", "match", "mod", "move", "mut", "pub", "ref", "return",
    "self", "static", "struct", "super", "trait", "true", "type", "unsafe", "use", "where",
    "while", "async", "await", "dyn", "abstract", "become", "box", "do", "final", "macro",
    "override", "priv", "typeof", "unsized", "virtual", "yield", "try", "gen",
}

FAMILY_TYPE = re.compile(r"^[A-Z][A-Za-z0-9]*(<(true|false|-?[0-9]+)(, *(true|false|-?[0-9]+))*>)?$")
CLASS_NAME = re.compile(r"^[A-Z][A-Za-z0-9]*$")
MODULE_NAME = re.compile(r"^[a-z][a-z0-9_]*$")
STRUCT_NAME = re.compile(r"^[A-Z][A-Za-z0-9]*$")
STRING = re.compile(r"^[a-z0-9_:.]+$")
FNV_OFFSET = 0xCBF29CE484222325
FNV_PRIME = 0x100000001B3
MASK = (1 << 64) - 1


def fail(message):
    sys.exit("gen_effects: " + message)


def load_json(path):
    with open(path, encoding="utf-8") as handle:
        return json.load(handle)


def is_int(value):
    return isinstance(value, int) and not isinstance(value, bool)


def registry_order(registries):
    entries = registries.get(REGISTRY, {}).get("entries")
    if not entries:
        fail("registries.json has no " + REGISTRY)
    ordered = sorted(entries.items(), key=lambda item: item[1]["protocol_id"])
    names = []
    for index, (name, entry) in enumerate(ordered):
        if entry["protocol_id"] != index:
            fail("registries.json %s protocol id %d != %d" % (name, entry["protocol_id"], index))
        names.append(name)
    return names


def read_families(path, classes, names):
    by_class = {}
    by_name = {}
    with open(path, encoding="utf-8") as handle:
        lines = handle.read().splitlines()
    for number, raw in enumerate(lines, 1):
        line = raw.strip()
        if not line or line.startswith("#"):
            continue
        parts = line.split()
        if len(parts) != 2:
            fail("%s:%d: expected '<VanillaClass> <RustFamilyType>'" % (path, number))
        key, family = parts
        if not FAMILY_TYPE.match(family):
            fail("%s:%d: invalid family type %r" % (path, number, family))
        if key in names:
            if key in by_name:
                fail("%s:%d: duplicate effect %r" % (path, number, key))
            by_name[key] = family
            continue
        if key not in classes:
            fail("%s:%d: unknown vanilla class %r" % (path, number, key))
        if key in by_class:
            fail("%s:%d: duplicate class %r" % (path, number, key))
        by_class[key] = family
    missing = sorted(classes - set(by_class))
    if missing:
        fail("%s: unmapped vanilla classes %s" % (path, ", ".join(missing)))
    return by_class, by_name


def struct_name(path):
    return "".join(part[:1].upper() + part[1:] for part in path.split("_"))


def rust_string(value):
    if not STRING.match(value):
        fail("unexpected string " + repr(value))
    return '"' + value + '"'


def f64_literal(value):
    if not isinstance(value, (int, float)) or isinstance(value, bool):
        fail("amount is not a number: " + repr(value))
    value = float(value)
    if not math.isfinite(value):
        fail("non finite amount " + repr(value))
    text = repr(value)
    if not re.search(r"[.eE]", text):
        text += ".0"
    return text


def validate_window(dump):
    durations = dump.get("tick_durations")
    amplifiers = dump.get("tick_amplifiers")
    if not (isinstance(durations, list) and len(durations) == 2 and all(is_int(v) for v in durations)):
        fail("tick_durations must be [first, last]")
    first, last = durations
    if not -(1 << 31) <= first <= last < (1 << 31):
        fail("tick_durations out of i32 range")
    if not (isinstance(amplifiers, list) and amplifiers):
        fail("tick_amplifiers must be a non empty list")
    for amplifier in amplifiers:
        if not is_int(amplifier) or not 0 <= amplifier <= 255:
            fail("tick amplifier %r is not a u8" % (amplifier,))
    return first, last, amplifiers


def validate(dump, registry, window):
    if dump.get("version") != EXPECTED_VERSION:
        fail("dump version %r != %r" % (dump.get("version"), EXPECTED_VERSION))
    effects = dump["effects"]
    names = [effect["name"] for effect in effects]
    if names != registry:
        fail("effect names differ from registries.json %s order" % REGISTRY)
    first, last, amplifiers = window
    structs = set()
    for index, effect in enumerate(effects):
        name = effect["name"]
        if effect["id"] != index:
            fail("%s registry id %d != %d" % (name, effect["id"], index))
        namespace, _, path = name.partition(":")
        if namespace != "minecraft" or not MODULE_NAME.match(path) or path in RUST_KEYWORDS:
            fail("unsupported effect name " + name)
        effect["path"] = path
        effect["struct"] = struct_name(path)
        if not STRUCT_NAME.match(effect["struct"]) or effect["struct"] in RESERVED_STRUCTS:
            fail("unsupported struct name %s for %s" % (effect["struct"], name))
        if effect["struct"] in structs:
            fail("duplicate struct name " + effect["struct"])
        structs.add(effect["struct"])
        if not CLASS_NAME.match(effect["class"]):
            fail("%s has unsupported class %r" % (name, effect["class"]))
        if effect["category"] not in CATEGORIES:
            fail("%s has unknown category %r" % (name, effect["category"]))
        if effect["beneficial"] != (effect["category"] == "BENEFICIAL"):
            fail("%s beneficial flag disagrees with category" % name)
        if not is_int(effect["color"]) or not 0 <= effect["color"] <= 0xFFFFFF:
            fail("%s color %r out of range" % (name, effect["color"]))
        if not isinstance(effect["instantaneous"], bool):
            fail("%s instantaneous is not a boolean" % name)
        rust_string(effect["description_id"])
        keys = set()
        for modifier in effect["modifiers"]:
            rust_string(modifier["attribute"])
            rust_string(modifier["id"])
            f64_literal(modifier["amount"])
            if modifier["operation"] not in OPERATIONS:
                fail("%s has unknown operation %r" % (name, modifier["operation"]))
            key = (modifier["attribute"], modifier["id"])
            if key in keys:
                fail("%s has duplicate modifier %s" % (name, key))
            keys.add(key)
        if effect["modifiers"] != sorted(effect["modifiers"], key=lambda m: (m["attribute"], m["id"])):
            fail("%s modifiers are not sorted" % name)
        ticks = effect["ticks"]
        if len(ticks) != len(amplifiers):
            fail("%s has %d tick samples for %d amplifiers" % (name, len(ticks), len(amplifiers)))
        for samples in ticks:
            if len(samples) != last - first + 1 or set(samples) - {"0", "1"}:
                fail("%s has malformed tick samples" % name)
    return effects


def fingerprint(ticks):
    value = FNV_OFFSET
    for samples in ticks:
        for sample in samples.encode("ascii"):
            value = ((value ^ sample) * FNV_PRIME) & MASK
    return value


def template(modifier):
    return "AttributeTemplate { attribute: %s, modifier: %s, amount: %s, operation: AttributeOperation::%s }" % (
        rust_string(modifier["attribute"]),
        rust_string(modifier["id"]),
        f64_literal(modifier["amount"]),
        OPERATIONS[modifier["operation"]],
    )


def effect_file(effect, behavior):
    modifiers = effect["modifiers"]
    imports = ["AttributeTemplate", "EffectCategory", "EffectId", "MobEffect"]
    if modifiers:
        imports.insert(0, "AttributeOperation")
    lines = [
        "use crate::{%s};" % ", ".join(imports),
        "",
        "pub struct %s;" % effect["struct"],
        "",
        "impl MobEffect for %s {" % effect["struct"],
        "    const NAME: &'static str = %s;" % rust_string(effect["name"]),
        "    const TRANSLATION_KEY: &'static str = %s;" % rust_string(effect["description_id"]),
        "    const ID: EffectId = EffectId(%d);" % effect["id"],
        "    const CATEGORY: EffectCategory = EffectCategory::%s;" % CATEGORIES[effect["category"]],
        "    const COLOR: u32 = 0x%06X;" % effect["color"],
        "    const INSTANTANEOUS: bool = %s;" % str(effect["instantaneous"]).lower(),
    ]
    if len(modifiers) == 1:
        lines.append("    const MODIFIERS: &'static [AttributeTemplate] = &[%s];" % template(modifiers[0]))
    elif modifiers:
        lines.append("    const MODIFIERS: &'static [AttributeTemplate] = &[")
        lines.extend("        %s," % template(modifier) for modifier in modifiers)
        lines.append("    ];")
    else:
        lines.append("    const MODIFIERS: &'static [AttributeTemplate] = &[];")
    lines.append("    type Behavior = crate::family::%s;" % behavior)
    lines.append("}")
    return "\n".join(lines) + "\n"


def mod_file(effects):
    ordered = sorted(effects, key=lambda effect: effect["path"])
    lines = ["mod %s;" % effect["path"] for effect in ordered]
    lines.append("")
    lines.extend("pub use %s::%s;" % (effect["path"], effect["struct"]) for effect in ordered)
    return "\n".join(lines) + "\n"


def kind_file(effects):
    lines = ["crate::registry::effect_kinds! {"]
    lines.append("    effects: %d," % len(effects))
    lines.append("    kinds: [")
    lines.extend("        %s," % effect["struct"] for effect in effects)
    lines.append("    ],")
    lines.append("}")
    lines.append("")
    by_path = sorted(effects, key=lambda effect: effect["path"])
    lines.append("pub(crate) static NAMES: [(&str, EffectKind); %d] = [" % len(effects))
    lines.extend('    ("%s", EffectKind::%s),' % (effect["path"], effect["struct"]) for effect in by_path)
    lines.append("];")
    return "\n".join(lines) + "\n"


def ticks_file(effects, window):
    first, last, amplifiers = window
    lines = [
        "use rspigot_effects::EffectKind;",
        "",
        "const DURATIONS: (i32, i32) = (%d, %d);" % (first, last),
        "const AMPLIFIERS: [u8; %d] = [%s];" % (len(amplifiers), ", ".join(str(a) for a in amplifiers)),
        "const FINGERPRINTS: [(EffectKind, u64); %d] = [" % len(effects),
    ]
    lines.extend(
        "    (EffectKind::%s, 0x%016x)," % (effect["struct"], fingerprint(effect["ticks"]))
        for effect in effects
    )
    lines.extend([
        "];",
        "",
        "fn fingerprint(kind: EffectKind) -> u64 {",
        "    let (first, last) = DURATIONS;",
        "    AMPLIFIERS",
        "        .iter()",
        "        .fold(0x%016x, |hash, &amplifier| {" % FNV_OFFSET,
        "            (first..=last).fold(hash, |hash, duration| {",
        "                let sample = if kind.should_tick(duration, amplifier) {",
        "                    b'1'",
        "                } else {",
        "                    b'0'",
        "                };",
        "                (hash ^ u64::from(sample)).wrapping_mul(0x%016x)" % FNV_PRIME,
        "            })",
        "        })",
        "}",
        "",
        "#[test]",
        "fn should_tick_matches_vanilla_samples() {",
        "    for (kind, expected) in FINGERPRINTS {",
        "        assert_eq!((kind, fingerprint(kind)), (kind, expected));",
        "    }",
        "}",
    ])
    return "\n".join(lines) + "\n"


def write(path, content, stats):
    stats["files"] += 1
    stats["bytes"] += len(content.encode("utf-8"))
    if os.path.exists(path):
        with open(path, encoding="utf-8") as handle:
            if handle.read() == content:
                return
    stats["written"] += 1
    with open(path, "w", encoding="utf-8", newline="\n") as handle:
        handle.write(content)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--effects", default=DEFAULT_EFFECTS)
    parser.add_argument("--registries", default=DEFAULT_REGISTRIES)
    parser.add_argument("--families", default=DEFAULT_FAMILIES)
    parser.add_argument("--crate", default=CRATE)
    args = parser.parse_args()

    dump = load_json(args.effects)
    registry = registry_order(load_json(args.registries))
    window = validate_window(dump)
    effects = validate(dump, registry, window)
    by_class, by_name = read_families(
        args.families, {effect["class"] for effect in effects}, {effect["name"] for effect in effects}
    )

    src = os.path.join(args.crate, "src")
    effects_dir = os.path.join(src, "effects")
    tests_dir = os.path.join(args.crate, "tests")
    os.makedirs(effects_dir, exist_ok=True)
    os.makedirs(tests_dir, exist_ok=True)
    stats = {"files": 0, "bytes": 0, "written": 0}
    expected_files = {"mod.rs"}
    families = {}
    for effect in effects:
        behavior = by_name.get(effect["name"]) or by_class[effect["class"]]
        families[behavior] = families.get(behavior, 0) + 1
        file_name = effect["path"] + ".rs"
        expected_files.add(file_name)
        write(os.path.join(effects_dir, file_name), effect_file(effect, behavior), stats)
    write(os.path.join(effects_dir, "mod.rs"), mod_file(effects), stats)
    write(os.path.join(src, "kind.rs"), kind_file(effects), stats)
    write(os.path.join(tests_dir, "vanilla_ticks.rs"), ticks_file(effects, window), stats)
    removed = 0
    for entry in sorted(os.listdir(effects_dir)):
        if entry.endswith(".rs") and entry not in expected_files:
            os.remove(os.path.join(effects_dir, entry))
            removed += 1
    print(
        "effects %d classes %d families %s files %d bytes %d written %d removed %d"
        % (
            len(effects),
            len(by_class),
            " ".join("%s=%d" % (name, families[name]) for name in sorted(families)),
            stats["files"],
            stats["bytes"],
            stats["written"],
            removed,
        )
    )


if __name__ == "__main__":
    main()

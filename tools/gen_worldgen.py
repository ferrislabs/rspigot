import argparse
import json
import os
import sys

RE_ROOT = "/Users/baptiste.parmantier/Development/rspigot/re"
DEFAULT_DATAPACK = RE_ROOT + "/server/data/minecraft"
DEFAULT_OUT = os.path.normpath(
    os.path.join(
        os.path.dirname(os.path.abspath(__file__)),
        "..",
        "crates",
        "rspigot-worldgen",
        "data",
        "worldgen.json",
    )
)

SURFACE_NOISES = [
    "minecraft:surface",
    "minecraft:surface_secondary",
    "minecraft:clay_bands_offset",
    "minecraft:badlands_pillar",
    "minecraft:badlands_pillar_roof",
    "minecraft:badlands_surface",
    "minecraft:iceberg_pillar",
    "minecraft:iceberg_pillar_roof",
    "minecraft:iceberg_surface",
    "minecraft:nether/temperature",
    "minecraft:nether/vegetation",
]


def load_json(path):
    with open(path, encoding="utf-8") as handle:
        return json.load(handle)


class Pack:
    def __init__(self, root):
        self.root = os.path.join(root, "worldgen")

    def path(self, kind, name):
        return os.path.join(self.root, kind, name.split(":", 1)[1] + ".json")

    def load(self, kind, name):
        return load_json(self.path(kind, name))

    def exists(self, kind, name):
        return os.path.exists(self.path(kind, name))

    def names(self, kind):
        base = os.path.join(self.root, kind)
        found = []
        for folder, _, files in os.walk(base):
            for file in files:
                if file.endswith(".json"):
                    relative = os.path.relpath(os.path.join(folder, file), base)
                    found.append("minecraft:" + relative[:-5].replace(os.sep, "/"))
        return sorted(found)


def resolve_material(pack, node, noises):
    if isinstance(node, str):
        if pack.exists("material_condition", node):
            return resolve_material(pack, pack.load("material_condition", node), noises)
        return resolve_material(pack, pack.load("material_rule", node), noises)
    if isinstance(node, list):
        return [resolve_material(pack, item, noises) for item in node]
    if not isinstance(node, dict):
        return node
    result = {}
    for key, value in node.items():
        if key in ("sequence",):
            result[key] = [resolve_material(pack, item, noises) for item in value]
        elif key in ("if_true", "then_run", "rule", "invert", "condition"):
            result[key] = resolve_material(pack, value, noises)
        else:
            result[key] = value
    if result.get("type") == "minecraft:noise_threshold":
        noises.add(result["noise"])
    return result


def noise_entry(pack, name):
    data = pack.load("noise", name)
    if data.get("normalize", "enabled") != "enabled":
        sys.exit("noise %s uses unsupported normalization" % name)
    return {
        "id": name,
        "base_octave": int(data["base_octave"]),
        "octave_count": int(data.get("octave_count", 1)),
        "base_amplitude": float(data.get("base_amplitude", 1.0)),
        "amplitude_modifiers": [float(v) for v in data.get("amplitude_modifiers", [])],
    }


def collect_feature_noises(node, noises):
    if isinstance(node, dict):
        for key, value in node.items():
            if key == "noise" and isinstance(value, str):
                noises.add(value)
            else:
                collect_feature_noises(value, noises)
    elif isinstance(node, list):
        for item in node:
            collect_feature_noises(item, noises)


def reachable_features(pack, biomes):
    placed_names = set()
    for biome in biomes.values():
        for step in biome["features"]:
            placed_names.update(step)
    placed = {}
    configured = {}

    def visit_placed(value):
        if isinstance(value, str):
            if value in placed:
                return
            if not pack.exists("placed_feature", value):
                configured_ref(value)
                return
            data = pack.load("placed_feature", value)
            placed[value] = data
            walk(data)
        else:
            walk(value)

    def configured_ref(name):
        if name in configured:
            return
        data = pack.load("feature", name)
        configured[name] = data
        walk(data)

    def walk(node):
        if isinstance(node, dict):
            for key, value in node.items():
                if key == "feature":
                    if isinstance(value, str):
                        if pack.exists("placed_feature", value) and "placement" not in node and "type" not in node:
                            visit_placed(value)
                        elif "placement" in node:
                            configured_ref(value)
                        elif pack.exists("feature", value) and not pack.exists("placed_feature", value):
                            configured_ref(value)
                        else:
                            visit_placed(value)
                    else:
                        walk(value)
                else:
                    walk(value)
        elif isinstance(node, list):
            for item in node:
                walk(item)

    for name in sorted(placed_names):
        visit_placed(name)
    return placed, configured


def processor_list_names(node, found):
    if isinstance(node, dict):
        for key, value in node.items():
            if key in ("fossil_processors", "overlay_processors") and isinstance(value, str):
                found.add(value)
            else:
                processor_list_names(value, found)
    elif isinstance(node, list):
        for item in node:
            processor_list_names(item, found)


def build(pack):
    noises = set(SURFACE_NOISES)
    surface_rule = {
        "surface": resolve_material(pack, "minecraft:overworld/surface", noises),
        "underground": resolve_material(pack, "minecraft:overworld/underground", noises),
        "nether": resolve_material(pack, "minecraft:nether", noises),
    }
    biomes = {}
    for name in pack.names("biome"):
        data = pack.load("biome", name)
        biomes[name] = {
            "temperature": data["temperature"],
            "temperature_modifier": data.get("temperature_modifier", "none"),
            "has_precipitation": data.get("has_precipitation", True),
            "features": data["features"],
        }
    placed = {name: pack.load("placed_feature", name) for name in pack.names("placed_feature")}
    configured = {name: pack.load("feature", name) for name in pack.names("feature")}
    processor_names = set()
    processor_list_names(configured, processor_names)
    collect_feature_noises(placed, noises)
    collect_feature_noises(configured, noises)
    noise_table = {name: noise_entry(pack, name) for name in sorted(noises)}
    return {
        "surface_rule": surface_rule,
        "biomes": biomes,
        "placed": placed,
        "configured": configured,
        "noises": noise_table,
        "processor_lists": {name: pack.load("processor_list", name) for name in sorted(processor_names)},
        "providers": {name: pack.load("block_state_provider", name) for name in pack.names("block_state_provider")},
    }


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--datapack", default=DEFAULT_DATAPACK)
    parser.add_argument("--out", default=DEFAULT_OUT)
    parser.add_argument("--check", action="store_true")
    args = parser.parse_args()
    text = json.dumps(build(Pack(args.datapack)), separators=(",", ":"), sort_keys=True) + "\n"
    if args.check:
        with open(args.out, encoding="utf-8") as handle:
            if handle.read() != text:
                sys.exit("%s is stale" % args.out)
        return
    os.makedirs(os.path.dirname(args.out), exist_ok=True)
    with open(args.out, "w", encoding="utf-8") as handle:
        handle.write(text)


if __name__ == "__main__":
    main()

import argparse
import importlib.util
import json
import os
import sys

RE_ROOT = "/Users/baptiste.parmantier/Development/rspigot/re"
DEFAULT_DATAPACK = RE_ROOT + "/server/data/minecraft"
HERE = os.path.dirname(os.path.abspath(__file__))
DATA = os.path.normpath(os.path.join(HERE, "..", "crates", "rspigot-worldgen", "data"))
TEMPLATE_GROUPS = (
    "minecraft:village/",
    "minecraft:pillager_outpost/",
    "minecraft:trail_ruins/",
    "minecraft:abandoned_camp/",
    "minecraft:ancient_city/",
    "minecraft:bastion/",
    "minecraft:trial_chambers/",
    "minecraft:ruined_portal/",
    "minecraft:shipwreck/",
    "minecraft:underwater_ruin/",
    "minecraft:igloo/",
    "minecraft:nether_fossils/",
    "minecraft:woodland_mansion/",
    "minecraft:end_city/",
)
EMBEDDED_GROUPS = (
    "minecraft:village/",
    "minecraft:pillager_outpost/",
    "minecraft:ruined_portal/",
    "minecraft:shipwreck/",
    "minecraft:underwater_ruin/",
    "minecraft:igloo/",
    "minecraft:nether_fossils/",
    "minecraft:woodland_mansion/",
    "minecraft:end_city/",
)
SHAPED_FOLDERS = (
    "ruined_portal",
    "shipwreck",
    "underwater_ruin",
    "igloo",
    "nether_fossils",
    "woodland_mansion",
    "end_city",
)
PACK_NAME = "vanilla-extras"
PACK_MCMETA = {"pack": {"description": "Vanilla jigsaw structures that rspigot does not embed", "pack_format": 121}}
DEFAULT_STRUCTURES = os.path.join(DATA, "structures.json")
DEFAULT_TEMPLATES = os.path.join(DATA, "jigsaw_templates.json")


def load_templates_module():
    spec = importlib.util.spec_from_file_location("gen_templates", os.path.join(HERE, "gen_templates.py"))
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def typed_reader(module):
    class Typed(module.Reader):
        def payload(self, kind):
            value = super().payload(kind)
            tag = {1: "b", 2: "s", 4: "l", 5: "f", 7: "ba", 11: "ia", 12: "la"}.get(kind)
            if tag is None:
                return value
            return {"$" + tag: value}

    return Typed


def read_typed(module, path):
    import gzip

    with gzip.open(path, "rb") as handle:
        reader = typed_reader(module)(handle.read())
    reader.take(">b")
    reader.string()
    return reader.payload(10)


def load_json(path):
    with open(path, encoding="utf-8") as handle:
        return json.load(handle)


def names(root, kind):
    base = os.path.join(root, kind)
    found = []
    for folder, _, files in os.walk(base):
        for file in files:
            if file.endswith(".json"):
                relative = os.path.relpath(os.path.join(folder, file), base)
                found.append(relative[:-5].replace(os.sep, "/"))
    return sorted(found)


class Tags:
    def __init__(self, root, kind):
        self.root = root
        self.kind = kind
        self.cache = {}

    def expand(self, name):
        key = name.split(":", 1)[1]
        if key in self.cache:
            return self.cache[key]
        path = os.path.join(self.root, "tags", self.kind, key + ".json")
        out = []
        for entry in load_json(path)["values"]:
            ident = entry["id"] if isinstance(entry, dict) else entry
            if ident.startswith("#"):
                out.extend(self.expand(ident[1:]))
            else:
                out.append(ident)
        seen = []
        for item in out:
            if item not in seen:
                seen.append(item)
        self.cache[key] = seen
        return seen

    def resolve(self, value):
        if isinstance(value, list):
            out = []
            for item in value:
                out.extend(self.resolve(item))
            return out
        if value.startswith("#"):
            return self.expand(value[1:])
        return [value]


def strip(name):
    return name.split(":", 1)[1] if ":" in name else name


def element_templates(element, out, pools, processors):
    kind = element["element_type"]
    if kind in ("minecraft:single_pool_element", "minecraft:legacy_single_pool_element"):
        out.add(element["location"])
        proc = element.get("processors")
        if isinstance(proc, str):
            processors.add(proc)
    elif kind == "minecraft:list_pool_element":
        for inner in element["elements"]:
            element_templates(inner, out, pools, processors)
    elif kind == "minecraft:feature_pool_element":
        pass


def collect_pools(root, start_pools):
    pools = {}
    templates = set()
    processors = set()
    pending = list(start_pools)
    while pending:
        name = pending.pop()
        if name in pools or name == "minecraft:empty":
            continue
        path = os.path.join(root, "worldgen", "template_pool", strip(name) + ".json")
        if not os.path.exists(path):
            continue
        pool = load_json(path)
        pools[name] = pool
        fallback = pool.get("fallback")
        if fallback:
            pending.append(fallback)
        for entry in pool["elements"]:
            element_templates(entry["element"], templates, pools, processors)
    return pools, templates, processors


def jigsaw_pool_refs(template):
    refs = set()
    for row in template["blocks"]:
        nbt = row[4] if len(row) > 4 else None
        if nbt and nbt.get("id") == "minecraft:jigsaw":
            refs.add(nbt["pool"])
    return refs


def nbt_json(value):
    if isinstance(value, dict):
        return {k: nbt_json(v) for k, v in value.items()}
    if isinstance(value, list):
        return [nbt_json(v) for v in value]
    return value


def convert_template(module, path):
    tag = read_typed(module, path)
    palettes = tag.get("palettes")
    if palettes is None:
        palettes = [tag["palette"]]
    palettes = [
        [
            [entry.get("Name", entry.get("id"))]
            + [[k, v] for k, v in sorted(entry.get("Properties", entry.get("properties", {})).items())]
            for entry in palette
        ]
        for palette in palettes
    ]
    blocks = []
    for block in tag["blocks"]:
        nbt = nbt_json(block["nbt"]) if "nbt" in block else None
        row = [block["pos"][0], block["pos"][1], block["pos"][2], block["state"]]
        if nbt:
            row.append(nbt)
        blocks.append(row)
    entities = []
    for entity in tag.get("entities", []):
        entities.append({"pos": entity["pos"], "block_pos": entity["blockPos"], "nbt": nbt_json(entity["nbt"])})
    return {"size": tag["size"], "palettes": palettes, "blocks": blocks, "entities": entities}


def shaped_templates(root):
    found = set()
    for name in SHAPED_FOLDERS:
        folder = os.path.join(root, "structure", name)
        found |= {
            "minecraft:" + name + "/" + file[:-4]
            for file in os.listdir(folder)
            if file.endswith(".nbt")
        }
    return found


def build(root):
    module = load_templates_module()
    biome_tags = Tags(root, "worldgen/biome")
    sets = {}
    for name in names(os.path.join(root, "worldgen"), "structure_set"):
        data = load_json(os.path.join(root, "worldgen", "structure_set", name + ".json"))
        placement = dict(data["placement"])
        if "preferred_biomes" in placement:
            placement["preferred_biomes"] = [strip(b) for b in biome_tags.resolve(placement["preferred_biomes"])]
        sets[name] = {"placement": placement, "structures": [{"structure": strip(s["structure"]), "weight": s["weight"]} for s in data["structures"]]}
    structures = {}
    raw_structures = {}
    starts = []
    for name in names(os.path.join(root, "worldgen"), "structure"):
        data = load_json(os.path.join(root, "worldgen", "structure", name + ".json"))
        raw_structures[name] = json.loads(json.dumps(data))
        data["biomes"] = [strip(b) for b in biome_tags.resolve(data["biomes"])]
        if data["type"] == "minecraft:jigsaw":
            starts.append(data["start_pool"])
            for alias in data.get("pool_aliases", []):
                walk_aliases(alias, starts)
        structures[name] = data
    pools, templates, processor_names = collect_pools(root, starts)
    expanded = set(templates) | shaped_templates(root)
    missing = set()
    while True:
        converted = {}
        extra = set()
        for ident in sorted(expanded):
            path = os.path.join(root, "structure", strip(ident) + ".nbt")
            if not ident.startswith(TEMPLATE_GROUPS) or not os.path.exists(path):
                continue
            converted[ident] = convert_template(module, path)
            for ref in jigsaw_pool_refs(converted[ident]):
                if ref not in pools and ref != "minecraft:empty" and ref not in missing:
                    extra.add(ref)
        if not extra:
            break
        more, more_templates, more_processors = collect_pools(root, sorted(extra))
        missing |= extra - set(more)
        pools.update(more)
        expanded |= more_templates
        processor_names |= more_processors
    processor_lists = {}
    for name in sorted(processor_names):
        processor_lists[name] = load_json(os.path.join(root, "worldgen", "processor_list", strip(name) + ".json"))
    return Collected(root, sets, structures, raw_structures, pools, processor_lists, converted)


def pool_processors(pool):
    found = set()

    def walk(element):
        if isinstance(element.get("processors"), str):
            found.add(element["processors"])
        for inner in element.get("elements", []):
            walk(inner)

    for entry in pool["elements"]:
        walk(entry["element"])
    return found


def embedded_pool(name):
    return name.startswith(EMBEDDED_GROUPS)


class Collected:
    def __init__(self, root, sets, structures, raw_structures, pools, processor_lists, templates):
        self.root = root
        self.sets = sets
        self.structures = structures
        self.raw_structures = raw_structures
        self.pools = pools
        self.processor_lists = processor_lists
        self.templates = templates

    def embedded_structure(self, name):
        data = self.structures[name]
        return data["type"] != "minecraft:jigsaw" or embedded_pool(data["start_pool"])

    def embedded(self):
        structures = {n: d for n, d in self.structures.items() if self.embedded_structure(n)}
        pools = {n: d for n, d in self.pools.items() if embedded_pool(n)}
        names = set()
        for pool in pools.values():
            names |= pool_processors(pool)
        lists = {n: d for n, d in self.processor_lists.items() if n in names}
        templates = {n: d for n, d in self.templates.items() if n.startswith(EMBEDDED_GROUPS)}
        structure_data = {"sets": self.sets, "structures": structures, "pools": pools, "processor_lists": lists}
        return structure_data, templates

    def pack_files(self):
        files = {"pack.mcmeta": dump(PACK_MCMETA).encode()}
        base = "data/minecraft/"
        for name, data in self.raw_structures.items():
            if not self.embedded_structure(name):
                files[base + "worldgen/structure/" + name + ".json"] = dump(data).encode()
        pools = {n: d for n, d in self.pools.items() if not embedded_pool(n)}
        names = set()
        for name, data in pools.items():
            files[base + "worldgen/template_pool/" + strip(name) + ".json"] = dump(data).encode()
            names |= pool_processors(data)
        for name in sorted(names):
            files[base + "worldgen/processor_list/" + strip(name) + ".json"] = dump(self.processor_lists[name]).encode()
        for name in self.templates:
            if not name.startswith(EMBEDDED_GROUPS):
                with open(os.path.join(self.root, "structure", strip(name) + ".nbt"), "rb") as handle:
                    files[base + "structure/" + strip(name) + ".nbt"] = handle.read()
        return files


def walk_aliases(node, out):
    if isinstance(node, dict):
        for key in ("alias", "target"):
            value = node.get(key)
            if isinstance(value, str) and ":" in value and key == "target":
                out.append(value)
        for key in ("targets",):
            for item in node.get(key, []):
                if isinstance(item, dict) and "data" in item:
                    out.append(item["data"])
        for value in node.values():
            walk_aliases(value, out)
    elif isinstance(node, list):
        for value in node:
            walk_aliases(value, out)


def dump(value):
    return json.dumps(value, separators=(",", ":"), sort_keys=True) + "\n"


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--datapack", default=DEFAULT_DATAPACK)
    parser.add_argument("--structures", default=DEFAULT_STRUCTURES)
    parser.add_argument("--templates", default=DEFAULT_TEMPLATES)
    parser.add_argument("--datapack-out")
    parser.add_argument("--check", action="store_true")
    args = parser.parse_args()
    collected = build(args.datapack)
    structure_data, templates = collected.embedded()
    outputs = {args.structures: dump(structure_data).encode(), args.templates: dump(templates).encode()}
    if args.datapack_out:
        for relative, data in collected.pack_files().items():
            outputs[os.path.join(args.datapack_out, relative)] = data
    for path, data in outputs.items():
        if args.check:
            if not os.path.exists(path) or open(path, "rb").read() != data:
                sys.exit("%s is stale" % path)
        else:
            os.makedirs(os.path.dirname(path), exist_ok=True)
            with open(path, "wb") as handle:
                handle.write(data)


if __name__ == "__main__":
    main()

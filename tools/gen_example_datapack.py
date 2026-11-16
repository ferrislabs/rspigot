import gzip
import json
import os
import struct

HERE = os.path.dirname(os.path.abspath(__file__))
OUT = os.path.normpath(os.path.join(HERE, "..", "example", "datapacks", "custom-hut"))


def name(text):
    raw = text.encode()
    return struct.pack(">H", len(raw)) + raw


def tag(kind, label, payload):
    return struct.pack(">b", kind) + name(label) + payload


def int_tag(label, value):
    return tag(3, label, struct.pack(">i", value))


def int_list(label, values):
    return tag(9, label, struct.pack(">bi", 3, len(values)) + b"".join(struct.pack(">i", v) for v in values))


def compound_list(label, items):
    return tag(9, label, struct.pack(">bi", 10, len(items)) + b"".join(items))


def compound(*fields):
    return b"".join(fields) + b"\x00"


def string_tag(label, text):
    return tag(8, label, name(text))


def template():
    blocks = []
    for y in range(4):
        for x in range(5):
            for z in range(5):
                edge = x in (0, 4) or z in (0, 4)
                if y == 0:
                    state = 0
                elif y == 3 or edge:
                    state = 1
                else:
                    continue
                blocks.append(compound(int_list("pos", [x, y, z]), int_tag("state", state)))
    palette = [compound(string_tag("Name", "minecraft:gold_block")), compound(string_tag("Name", "minecraft:oak_planks"))]
    root = (
        int_list("size", [5, 4, 5])
        + int_tag("DataVersion", 5023)
        + compound_list("palette", palette)
        + compound_list("blocks", blocks)
        + compound_list("entities", [])
    )
    return gzip.compress(b"\x0a" + name("") + root + b"\x00", mtime=0)


def dump(value):
    return json.dumps(value, indent=2, sort_keys=True) + "\n"


FILES = {
    "pack.mcmeta": dump({"pack": {"description": "A custom jigsaw hut", "pack_format": 121}}),
    "data/example/worldgen/structure/hut.json": dump(
        {
            "type": "minecraft:jigsaw",
            "biomes": "#minecraft:is_overworld",
            "step": "surface_structures",
            "terrain_adaptation": "none",
            "spawn_overrides": {},
            "start_pool": "example:hut",
            "size": 1,
            "max_distance_from_center": 80,
            "start_height": {"absolute": 0},
            "project_start_to_heightmap": "WORLD_SURFACE_WG",
            "use_expansion_hack": False,
        }
    ),
    "data/example/worldgen/structure_set/huts.json": dump(
        {
            "placement": {"type": "minecraft:random_spread", "spacing": 8, "separation": 4, "salt": 31337},
            "structures": [{"structure": "example:hut", "weight": 1}],
        }
    ),
    "data/example/worldgen/template_pool/hut.json": dump(
        {
            "fallback": "minecraft:empty",
            "elements": [
                {
                    "weight": 1,
                    "element": {
                        "element_type": "minecraft:single_pool_element",
                        "location": "example:hut",
                        "processors": "minecraft:empty",
                        "projection": "rigid",
                    },
                }
            ],
        }
    ),
}


def main():
    for relative, text in FILES.items():
        path = os.path.join(OUT, relative)
        os.makedirs(os.path.dirname(path), exist_ok=True)
        with open(path, "w", encoding="utf-8") as handle:
            handle.write(text)
    path = os.path.join(OUT, "data/example/structure/hut.nbt")
    os.makedirs(os.path.dirname(path), exist_ok=True)
    with open(path, "wb") as handle:
        handle.write(template())


if __name__ == "__main__":
    main()

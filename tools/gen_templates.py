import argparse
import gzip
import json
import os
import struct
import sys

RE_ROOT = "/Users/baptiste.parmantier/Development/rspigot/re"
DEFAULT_DATAPACK = RE_ROOT + "/server/data/minecraft"
DATA = os.path.normpath(
    os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "crates", "rspigot-worldgen", "data")
)
DEFAULT_SOURCE = os.path.join(DATA, "worldgen.json")
DEFAULT_OUT = os.path.join(DATA, "templates.json")


class Reader:
    def __init__(self, data):
        self.data = data
        self.at = 0

    def take(self, fmt):
        size = struct.calcsize(fmt)
        value = struct.unpack_from(fmt, self.data, self.at)[0]
        self.at += size
        return value

    def string(self):
        length = self.take(">H")
        text = self.data[self.at : self.at + length].decode("utf-8")
        self.at += length
        return text

    def payload(self, kind):
        if kind == 1:
            return self.take(">b")
        if kind == 2:
            return self.take(">h")
        if kind == 3:
            return self.take(">i")
        if kind == 4:
            return self.take(">q")
        if kind == 5:
            return self.take(">f")
        if kind == 6:
            return self.take(">d")
        if kind == 7:
            return [self.take(">b") for _ in range(self.take(">i"))]
        if kind == 8:
            return self.string()
        if kind == 9:
            item = self.take(">b")
            return [self.payload(item) for _ in range(self.take(">i"))]
        if kind == 10:
            result = {}
            while True:
                item = self.take(">b")
                if item == 0:
                    return result
                name = self.string()
                result[name] = self.payload(item)
        if kind == 11:
            return [self.take(">i") for _ in range(self.take(">i"))]
        if kind == 12:
            return [self.take(">q") for _ in range(self.take(">i"))]
        raise ValueError(kind)


def read_nbt(path):
    with gzip.open(path, "rb") as handle:
        reader = Reader(handle.read())
    reader.take(">b")
    reader.string()
    return reader.payload(10)


def template_ids(configured):
    found = set()

    def walk(node):
        if isinstance(node, dict):
            if node.get("type") == "minecraft:template":
                for entry in node["templates"]:
                    found.add(entry["data"]["id"])
            if node.get("type") == "minecraft:fossil":
                found.update(node["fossil_structures"])
                found.update(node["overlay_structures"])
            for value in node.values():
                walk(value)
        elif isinstance(node, list):
            for value in node:
                walk(value)

    walk(configured)
    return sorted(found)


def convert(path):
    tag = read_nbt(path)
    palettes = tag.get("palettes")
    if palettes is None:
        palettes = [tag["palette"]]
    if len(palettes) != 1:
        raise ValueError("%s: %d palettes" % (path, len(palettes)))
    palette = [
        [entry.get("Name", entry.get("id"))] + [[k, v] for k, v in sorted(entry.get("Properties", entry.get("properties", {})).items())]
        for entry in palettes[0]
    ]
    blocks = [[b["pos"][0], b["pos"][1], b["pos"][2], b["state"]] for b in tag["blocks"]]
    blocks.sort()
    return {"size": tag["size"], "palette": palette, "blocks": blocks}


def build(datapack, source):
    with open(source, encoding="utf-8") as handle:
        configured = json.load(handle)["configured"]
    out = {}
    for ident in template_ids(configured):
        path = os.path.join(datapack, "structure", ident.split(":", 1)[1] + ".nbt")
        out[ident] = convert(path)
    return out


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--datapack", default=DEFAULT_DATAPACK)
    parser.add_argument("--source", default=DEFAULT_SOURCE)
    parser.add_argument("--out", default=DEFAULT_OUT)
    parser.add_argument("--check", action="store_true")
    args = parser.parse_args()
    text = json.dumps(build(args.datapack, args.source), separators=(",", ":"), sort_keys=True) + "\n"
    if args.check:
        with open(args.out, encoding="utf-8") as handle:
            if handle.read() != text:
                sys.exit("%s is stale" % args.out)
        return
    with open(args.out, "w", encoding="utf-8") as handle:
        handle.write(text)


if __name__ == "__main__":
    main()

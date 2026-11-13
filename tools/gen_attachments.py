import argparse
import json
import os
import sys

RE_ROOT = "/Users/baptiste.parmantier/Development/rspigot/re"
DEFAULT_DUMP = RE_ROOT + "/datagen/generated/reports/entity_attachments.json"
CRATE = os.path.normpath(
    os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "crates", "rspigot-entities")
)
DEFAULT_OUT = os.path.join(CRATE, "src", "attachments.rs")
EXPECTED_VERSION = "26.3"
NAMESPACE = "minecraft:"


def struct_name(path):
    return "".join(part[:1].upper() + part[1:] for part in path.split("_"))


def literal(value):
    text = repr(float(value))
    return text


def points(values):
    return "&[" + ", ".join("[%s, %s, %s]" % tuple(literal(c) for c in point) for point in values) + "]"


def render(dump):
    if dump.get("version") != EXPECTED_VERSION:
        sys.exit("unexpected version %r" % dump.get("version"))
    passengers = []
    vehicles = []
    for name, entry in dump["types"].items():
        if not name.startswith(NAMESPACE):
            sys.exit("unexpected name " + name)
        kind = struct_name(name[len(NAMESPACE):])
        height = entry["height"]
        passenger = entry["passenger"]
        vehicle = entry["vehicle"]
        if len(passenger) != 1 or abs(passenger[0][0]) > 0 or abs(passenger[0][2]) > 0 or abs(passenger[0][1] - height) > 1e-6:
            passengers.append((kind, passenger))
        if vehicle != [[0.0, 0.0, 0.0]]:
            vehicles.append((kind, vehicle))
    lines = [
        "use crate::EntityKind;",
        "",
        "pub type Point = [f64; 3];",
        "",
        "pub fn passenger_points(kind: EntityKind) -> Option<&'static [Point]> {",
        "    Some(match kind {",
    ]
    for kind, values in passengers:
        lines.append("        EntityKind::%s => %s," % (kind, points(values)))
    lines += ["        _ => return None,", "    })", "}", "", "pub fn vehicle_points(kind: EntityKind) -> Option<&'static [Point]> {", "    Some(match kind {"]
    for kind, values in vehicles:
        lines.append("        EntityKind::%s => %s," % (kind, points(values)))
    lines += ["        _ => return None,", "    })", "}", ""]
    return "\n".join(lines)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--dump", default=DEFAULT_DUMP)
    parser.add_argument("--out", default=DEFAULT_OUT)
    parser.add_argument("--check", action="store_true")
    args = parser.parse_args()
    with open(args.dump, encoding="utf-8") as handle:
        text = render(json.load(handle))
    if args.check:
        with open(args.out, encoding="utf-8") as handle:
            if handle.read() != text:
                sys.exit("%s is stale" % args.out)
        return
    with open(args.out, "w", encoding="utf-8") as handle:
        handle.write(text)


main()

import argparse
import json
import os
import struct
import sys

RE_ROOT = "/Users/baptiste.parmantier/Development/rspigot/re"
DEFAULT_DATAPACK = RE_ROOT + "/server/data/minecraft"
DEFAULT_POINTS = RE_ROOT + "/datagen/generated/reports/biome_parameters/minecraft/overworld.json"
DEFAULT_OUT = os.path.normpath(
    os.path.join(
        os.path.dirname(os.path.abspath(__file__)),
        "..",
        "crates",
        "rspigot-data",
        "src",
        "generated",
        "multi_noise.rs",
    )
)

ROUTER_KEYS = ["temperature", "vegetation", "continents", "erosion", "depth", "ridges"]
PARAMETERS = ["temperature", "humidity", "continentalness", "erosion", "depth", "weirdness"]
NO_NODE = 65535


def f32(value):
    return struct.unpack("<f", struct.pack("<f", value))[0]


def quantize(value):
    scaled = f32(f32(value) * f32(10000.0))
    return int(scaled)


def float_literal(value):
    value = f32(value)
    for precision in range(1, 10):
        text = "%.*g" % (precision, value)
        if f32(float(text)) == value:
            break
    if "." not in text and "e" not in text and "n" not in text:
        text += ".0"
    return text + "_f32"


def double_literal(value):
    text = repr(float(value))
    if "." not in text and "e" not in text and "n" not in text:
        text += ".0"
    return text


def load_json(path):
    with open(path, encoding="utf-8") as handle:
        return json.load(handle)


class Graph:
    def __init__(self, datapack):
        self.datapack = datapack
        self.nodes = []
        self.by_name = {}
        self.noises = []
        self.noise_index = {}
        self.splines = []
        self.points = []

    def noise(self, name):
        if name in self.noise_index:
            return self.noise_index[name]
        path = os.path.join(self.datapack, "worldgen", "noise", name.split(":", 1)[1] + ".json")
        data = load_json(path)
        if data.get("normalize", "enabled") != "enabled":
            sys.exit("noise %s uses unsupported normalization %r" % (name, data["normalize"]))
        index = len(self.noises)
        self.noises.append(
            (
                name,
                int(data["base_octave"]),
                int(data.get("octave_count", 1)),
                float(data.get("base_amplitude", 1.0)),
                [float(v) for v in data.get("amplitude_modifiers", [])],
            )
        )
        self.noise_index[name] = index
        return index

    def push(self, node):
        self.nodes.append(node)
        return len(self.nodes) - 1

    def reference(self, name):
        if name in self.by_name:
            return self.by_name[name]
        path = os.path.join(self.datapack, "worldgen", "density_function", name.split(":", 1)[1] + ".json")
        index = self.build(load_json(path))
        self.by_name[name] = index
        return index

    def build(self, node):
        if isinstance(node, str):
            return self.reference(node)
        if isinstance(node, (int, float)):
            return self.push(("const", f32(float(node))))
        kind = node["type"].split(":", 1)[1]
        if kind == "cache":
            return self.build(node["input"])
        if kind == "constant":
            return self.push(("const", f32(float(node["argument"]))))
        if kind == "noise":
            shift_x = self.build(node["shift_x"]) if "shift_x" in node else NO_NODE
            shift_z = self.build(node["shift_z"]) if "shift_z" in node else NO_NODE
            if "shift_y" in node:
                sys.exit("noise with shift_y is not supported")
            return self.push(
                (
                    "noise",
                    self.noise(node["noise"]),
                    float(node["xz_scale"]),
                    float(node["y_scale"]),
                    shift_x,
                    shift_z,
                )
            )
        if kind in ("shift_a", "shift_b"):
            return self.push((kind, self.noise(node["noise"])))
        if kind == "gradient" or kind == "y_clamped_gradient":
            if node.get("axis", "y") != "y":
                sys.exit("gradient on axis %s is not supported" % node["axis"])
            if node.get("tiling", "clamped") != "clamped":
                sys.exit("gradient tiling %s is not supported" % node["tiling"])
            from_y = int(node["from_coordinate"])
            to_y = int(node["to_coordinate"])
            from_value = f32(float(node["from_value"]))
            to_value = f32(float(node["to_value"]))
            factor = f32(f32(to_value - from_value) / f32(float(to_y - from_y)))
            return self.push(("gradient", from_y, min(from_y, to_y), max(from_y, to_y), from_value, factor))
        if kind in ("add", "mul"):
            return self.push((kind, self.build(node["left"]), self.build(node["right"])))
        if kind == "abs":
            return self.push(("abs", self.build(node["input"])))
        if kind == "lerp":
            return self.push(("lerp", self.build(node["alpha"]), self.build(node["first"]), self.build(node["second"])))
        if kind == "blend_alpha":
            return self.push(("const", 1.0))
        if kind == "blend_offset":
            return self.push(("const", 0.0))
        if kind == "spline":
            return self.push(("spline", self.spline(node["spline"])))
        sys.exit("unsupported density function type %s" % node["type"])

    def spline(self, spec):
        coordinate = self.build(spec["coordinate"])
        raw = spec["points"]
        start = len(self.points)
        self.points.extend([None] * len(raw))
        index = len(self.splines)
        self.splines.append((coordinate, start, len(raw)))
        for offset, point in enumerate(raw):
            value = point["value"]
            if isinstance(value, dict):
                entry = ("nested", self.spline(value))
            else:
                entry = ("constant", f32(float(value)))
            self.points[start + offset] = (f32(float(point["location"])), f32(float(point["derivative"])), entry)
        return index


def render_node(node):
    kind = node[0]
    if kind == "const":
        return "DensityNode::Constant(%s)" % float_literal(node[1])
    if kind == "noise":
        return "DensityNode::Noise { noise: %d, xz_scale: %s, y_scale: %s, shift_x: %d, shift_z: %d }" % (
            node[1],
            double_literal(node[2]),
            double_literal(node[3]),
            node[4],
            node[5],
        )
    if kind == "shift_a":
        return "DensityNode::ShiftA { noise: %d }" % node[1]
    if kind == "shift_b":
        return "DensityNode::ShiftB { noise: %d }" % node[1]
    if kind == "gradient":
        return "DensityNode::Gradient { from: %d, min: %d, max: %d, from_value: %s, factor: %s }" % (
            node[1],
            node[2],
            node[3],
            float_literal(node[4]),
            float_literal(node[5]),
        )
    if kind == "add":
        return "DensityNode::Add { left: %d, right: %d }" % (node[1], node[2])
    if kind == "mul":
        return "DensityNode::Mul { left: %d, right: %d }" % (node[1], node[2])
    if kind == "abs":
        return "DensityNode::Abs { input: %d }" % node[1]
    if kind == "lerp":
        return "DensityNode::Lerp { alpha: %d, first: %d, second: %d }" % (node[1], node[2], node[3])
    if kind == "spline":
        return "DensityNode::Spline { spline: %d }" % node[1]
    raise AssertionError(kind)


def render_point(point):
    location, derivative, value = point
    if value[0] == "nested":
        rendered = "SplineValue::Nested(%d)" % value[1]
    else:
        rendered = "SplineValue::Constant(%s)" % float_literal(value[1])
    return "SplinePoint { location: %s, derivative: %s, value: %s }" % (
        float_literal(location),
        float_literal(derivative),
        rendered,
    )


def load_points(path, datapack):
    data = load_json(path)["biomes"]
    names = sorted({entry["biome"] for entry in data})
    for name in names:
        biome_path = os.path.join(datapack, "worldgen", "biome", name.split(":", 1)[1] + ".json")
        if not os.path.exists(biome_path):
            sys.exit("%s is not a datapack biome" % name)
    index = {name: i for i, name in enumerate(names)}
    points = []
    for entry in data:
        parameters = entry["parameters"]
        low = []
        high = []
        for key in PARAMETERS:
            value = parameters[key]
            if isinstance(value, list):
                low.append(quantize(value[0]))
                high.append(quantize(value[1]))
            else:
                low.append(quantize(value))
                high.append(quantize(value))
        points.append((low, high, quantize(parameters["offset"]), index[entry["biome"]]))
    return names, points


VARIANTS = [("NORMAL", "overworld"), ("LARGE", "large_biomes"), ("AMPLIFIED", "amplified")]


def render_graph(prefix, graph):
    lines = []
    lines.append("pub(crate) static %s_NOISES: [NoiseParameters; %d] = [" % (prefix, len(graph.noises)))
    for name, base_octave, count, amplitude, modifiers in graph.noises:
        lines.append("    NoiseParameters {")
        lines.append('        id: "%s",' % name)
        lines.append("        base_octave: %d," % base_octave)
        lines.append("        octave_count: %d," % count)
        lines.append("        base_amplitude: %s," % double_literal(amplitude))
        lines.append("        amplitude_modifiers: &[%s]," % ", ".join(double_literal(m) for m in modifiers))
        lines.append("    },")
    lines.append("];")
    lines.append("")
    lines.append("pub(crate) static %s_NODES: [DensityNode; %d] = [" % (prefix, len(graph.nodes)))
    for node in graph.nodes:
        lines.append("    %s," % render_node(node))
    lines.append("];")
    lines.append("")
    lines.append("pub(crate) static %s_SPLINES: [SplineSpec; %d] = [" % (prefix, len(graph.splines)))
    for coordinate, start, length in graph.splines:
        lines.append("    SplineSpec { coordinate: %d, start: %d, length: %d }," % (coordinate, start, length))
    lines.append("];")
    lines.append("")
    lines.append("pub(crate) static %s_SPLINE_POINTS: [SplinePoint; %d] = [" % (prefix, len(graph.points)))
    for point in graph.points:
        lines.append("    %s," % render_point(point))
    lines.append("];")
    lines.append("")
    lines.append(
        "pub(crate) static %s_ROUTER: [u16; 6] = [%s];" % (prefix, ", ".join(str(graph.router[k]) for k in ROUTER_KEYS))
    )
    lines.append("")
    return lines


def render(graphs, names, points):
    lines = [
        "use crate::multi_noise::{BiomePoint, DensityNode, NoiseParameters, SplinePoint, SplineSpec, SplineValue};",
        "",
        "pub(crate) static BIOMES: [&str; %d] = [" % len(names),
    ]
    for name in names:
        lines.append('    "%s",' % name)
    lines.append("];")
    lines.append("")
    for prefix, graph in graphs:
        lines.extend(render_graph(prefix, graph))
    lines.append("pub(crate) static POINTS: [BiomePoint; %d] = [" % len(points))
    for low, high, offset, biome in points:
        lines.append(
            "    BiomePoint { low: [%s], high: [%s], offset: %d, biome: %d },"
            % (", ".join(map(str, low)), ", ".join(map(str, high)), offset, biome)
        )
    lines.append("];")
    return "\n".join(lines) + "\n"


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--datapack", default=DEFAULT_DATAPACK)
    parser.add_argument("--points", default=DEFAULT_POINTS)
    parser.add_argument("--out", default=DEFAULT_OUT)
    parser.add_argument("--check", action="store_true")
    args = parser.parse_args()

    graphs = []
    for prefix, settings_name in VARIANTS:
        settings = load_json(os.path.join(args.datapack, "worldgen", "noise_settings", settings_name + ".json"))
        router = settings["noise_router"]
        graph = Graph(args.datapack)
        graph.router = {key: graph.build(router[key]) for key in ROUTER_KEYS}
        graphs.append((prefix, graph))
    names, points = load_points(args.points, args.datapack)
    text = render(graphs, names, points)
    assert "//" not in text
    if args.check:
        if not os.path.exists(args.out) or open(args.out, encoding="utf-8").read() != text:
            sys.exit("%s is stale" % args.out)
        return
    with open(args.out, "w", encoding="utf-8") as out:
        out.write(text)


main()

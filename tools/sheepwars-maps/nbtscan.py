import gzip
import os
import struct
import sys
import zlib


class Reader:
    def __init__(self, data):
        self.d = data
        self.p = 0

    def take(self, n):
        v = self.d[self.p:self.p + n]
        self.p += n
        return v

    def unpack(self, fmt):
        size = struct.calcsize(fmt)
        v = struct.unpack_from(fmt, self.d, self.p)[0]
        self.p += size
        return v

    def string(self):
        n = self.unpack(">H")
        return self.take(n).decode("utf-8", "replace")

    def payload(self, t):
        if t == 1:
            return self.unpack(">b")
        if t == 2:
            return self.unpack(">h")
        if t == 3:
            return self.unpack(">i")
        if t == 4:
            return self.unpack(">q")
        if t == 5:
            return self.unpack(">f")
        if t == 6:
            return self.unpack(">d")
        if t == 7:
            n = self.unpack(">i")
            return self.take(n)
        if t == 8:
            return self.string()
        if t == 9:
            it = self.unpack(">b")
            n = self.unpack(">i")
            return [self.payload(it) for _ in range(n)]
        if t == 10:
            out = {}
            while True:
                it = self.unpack(">b")
                if it == 0:
                    return out
                name = self.string()
                out[name] = self.payload(it)
        if t == 11:
            n = self.unpack(">i")
            v = list(struct.unpack_from(">%di" % n, self.d, self.p))
            self.p += 4 * n
            return v
        if t == 12:
            n = self.unpack(">i")
            v = list(struct.unpack_from(">%dq" % n, self.d, self.p))
            self.p += 8 * n
            return v
        raise ValueError("bad tag %d" % t)


def parse(data):
    r = Reader(data)
    t = r.unpack(">b")
    r.string()
    return r.payload(t)


def read_level(path):
    with open(path, "rb") as f:
        raw = f.read()
    return parse(gzip.decompress(raw))


def chunks(region_path):
    with open(region_path, "rb") as f:
        raw = f.read()
    for i in range(1024):
        e = struct.unpack_from(">I", raw, i * 4)[0]
        off, cnt = e >> 8, e & 255
        if off == 0 or cnt == 0:
            continue
        length = struct.unpack_from(">I", raw, off * 4096)[0]
        comp = raw[off * 4096 + 4]
        body = raw[off * 4096 + 5:off * 4096 + 4 + length]
        if comp == 2:
            body = zlib.decompress(body)
        elif comp == 1:
            body = gzip.decompress(body)
        elif comp == 3:
            pass
        else:
            raise ValueError("unsupported compression %d" % comp)
        yield parse(body)


AIR = {"minecraft:air", "minecraft:cave_air", "minecraft:void_air"}


def scan_chunk(c, acc):
    if "xPos" not in c:
        acc["skipped"] += 1
        sys.stderr.write("chunk without xPos, keys: %s\n" % sorted(c.keys()))
        return
    acc["chunks"] += 1
    dv = c.get("DataVersion")
    acc["versions"].add(dv)
    acc["status"].add(c.get("Status"))
    cx, cz = c["xPos"], c["zPos"]
    for s in c.get("sections", []):
        bs = s.get("block_states")
        if bs is None:
            continue
        acc["palette_sections"] += 1
        pal = [p.get("Name", p.get("id")) for p in bs["palette"]]
        non = [i for i, n in enumerate(pal) if n not in AIR]
        if not non:
            continue
        sy = s["Y"]
        data = bs.get("data")
        if len(pal) == 1 or data is None:
            idxs = [0] * 4096
        else:
            bits = max(4, (len(pal) - 1).bit_length())
            per = 64 // bits
            mask = (1 << bits) - 1
            idxs = []
            for v in data:
                v &= 0xFFFFFFFFFFFFFFFF
                for k in range(per):
                    idxs.append((v >> (k * bits)) & mask)
            idxs = idxs[:4096]
        nonset = set(non)
        for n, idx in enumerate(idxs):
            if idx in nonset:
                x = cx * 16 + (n & 15)
                z = cz * 16 + ((n >> 4) & 15)
                y = sy * 16 + (n >> 8)
                acc["count"] += 1
                if acc["min"] is None:
                    acc["min"] = [x, y, z]
                    acc["max"] = [x, y, z]
                else:
                    for j, v in enumerate((x, y, z)):
                        if v < acc["min"][j]:
                            acc["min"][j] = v
                        if v > acc["max"][j]:
                            acc["max"][j] = v


def scan_world(region_dir):
    acc = {"chunks": 0, "versions": set(), "status": set(), "palette_sections": 0,
           "count": 0, "min": None, "max": None, "regions": 0, "skipped": 0}
    for name in sorted(os.listdir(region_dir)):
        if name.endswith(".mca"):
            acc["regions"] += 1
            for c in chunks(os.path.join(region_dir, name)):
                scan_chunk(c, acc)
    return acc


def main():
    cmd = sys.argv[1]
    if cmd == "spawn":
        d = read_level(sys.argv[2])
        d = d.get("Data", d)
        print("%d %d %d" % (d["SpawnX"], d["SpawnY"], d["SpawnZ"]))
    elif cmd == "scan":
        a = scan_world(sys.argv[2])
        print("skipped=%d chunks=%d regions=%d versions=%s status=%s palette_sections=%d nonair=%d min=%s max=%s" % (
            a["skipped"], a["chunks"], a["regions"], sorted(a["versions"], key=str), sorted(a["status"], key=str),
            a["palette_sections"], a["count"], a["min"], a["max"]))
        if a["min"] is None or a["skipped"]:
            sys.exit(1)
    elif cmd == "json":
        a = scan_world(sys.argv[2])
        print("%s %s %d" % (" ".join(map(str, a["min"])), " ".join(map(str, a["max"])), a["regions"]))
    elif cmd == "dump":
        print(read_level(sys.argv[2]))


main()

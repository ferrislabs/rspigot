#!/bin/sh
set -eu
HERE="$(cd "$(dirname "$0")" && pwd)"
MC="$HOME/Library/Application Support/minecraft"
JDK="${JDK:-/opt/homebrew/opt/openjdk@25/bin}"
BUILD="${BUILD_DIR:-/tmp/rspigot-push-dump}"
OUT="${1:?usage: run.sh <output.rs>}"
case "$OUT" in
  /*) ;;
  *) OUT="$(pwd)/$OUT" ;;
esac

rm -rf "$BUILD"
mkdir -p "$BUILD/classes" "$BUILD/run"

python3 -I - "$MC" "$BUILD/classpath.txt" <<'EOF'
import json, os, sys
mc, out = sys.argv[1], sys.argv[2]
d = json.load(open(os.path.join(mc, "versions/26.3/26.3.json")))
cp = [os.path.join(mc, "versions/26.3/26.3.jar")]
for lib in d["libraries"]:
    if "rules" in lib:
        continue
    art = lib.get("downloads", {}).get("artifact")
    if not art:
        continue
    p = os.path.join(mc, "libraries", art["path"])
    if not os.path.exists(p):
        sys.exit("missing library " + p)
    cp.append(p)
open(out, "w").write(":".join(cp))
EOF

CP="$(cat "$BUILD/classpath.txt")"
"$JDK/javac" -nowarn -encoding UTF-8 -cp "$CP" -d "$BUILD/classes" "$HERE/PushDump.java"
cd "$BUILD/run"
"$JDK/java" -Xmx2g -cp "$BUILD/classes:$CP" PushDump "$OUT"

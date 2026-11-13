#!/bin/sh
set -eu
HERE="$(cd "$(dirname "$0")" && pwd)"
MC="$HOME/Library/Application Support/minecraft"
CLIENT_JAR="$MC/versions/26.3/26.3.jar"
JDK="${JDK:-/opt/homebrew/opt/openjdk@25/bin}"
BUILD="${BUILD_DIR:-/tmp/rspigot-attachment-dump}"
EXPECTED_SHA1=e877b6a07acd633fb3bb475002175cec036e7b87
OUT="${1:-/Users/baptiste.parmantier/Development/rspigot/re/datagen/generated/reports/entity_attachments.json}"
case "$OUT" in
  /*) ;;
  *) OUT="$(pwd)/$OUT" ;;
esac

actual=$(shasum -a 1 "$CLIENT_JAR" | cut -d' ' -f1)
if [ "$actual" != "$EXPECTED_SHA1" ]; then
  echo "client jar sha1 $actual does not match Mojang manifest $EXPECTED_SHA1" >&2
  exit 2
fi

rm -rf "$BUILD"
mkdir -p "$BUILD/classes" "$BUILD/run"

python3 -I - "$MC" "$BUILD/classpath.txt" <<'PY'
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
PY

CP="$(cat "$BUILD/classpath.txt")"
"$JDK/javac" -nowarn -encoding UTF-8 -cp "$CP" -d "$BUILD/classes" "$HERE/AttachmentDump.java"
cd "$BUILD/run"
"$JDK/java" -Xmx2g -cp "$BUILD/classes:$CP" AttachmentDump "$OUT"

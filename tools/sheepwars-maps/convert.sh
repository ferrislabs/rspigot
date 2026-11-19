#!/bin/sh
set -eu
HERE="$(cd "$(dirname "$0")" && pwd)"
ROOT="$(cd "$HERE/../.." && pwd)"
SRC="${1:?usage: convert.sh <Epicube_Map_v1 folder> [output folder]}"
OUT="${2:-$ROOT/example/server/maps/sheepwars}"
RE_DIR="${RE_DIR:-$ROOT/../re}"
JAVA="${JAVA:-/opt/homebrew/opt/openjdk@25/bin/java}"
PORT="${PORT:-25591}"
TIMEOUT="${TIMEOUT:-300}"
MAPS="arbres:Arbres buildings:Buildings dirigables:Dirigables galions:Galions ilot:Ilot japan-dream:Japan_Dream steampunk:SteamPunk"

[ -d "$SRC" ] || { echo "not a directory: $SRC" >&2; exit 1; }
[ -x "$JAVA" ] || { echo "java not found: $JAVA" >&2; exit 1; }
[ -d "$RE_DIR/server/org/bukkit/craftbukkit" ] || { echo "server classes not found in $RE_DIR/server" >&2; exit 1; }
mkdir -p "$OUT"
OUT="$(cd "$OUT" && pwd)"
WORK="$(mktemp -d "${TMPDIR:-/tmp}/sheepwars-maps.XXXXXX")"
trap 'rm -rf "$WORK"' EXIT INT TERM

upgrade() {
  dir="$1"
  mkdir -p "$dir/logs"
  echo "eula=true" > "$dir/eula.txt"
  printf 'level-name=world\nonline-mode=false\nserver-port=%s\nallow-nether=false\nspawn-protection=0\nview-distance=2\nsimulation-distance=2\nmotd=sheepwars-maps\n' "$PORT" > "$dir/server.properties"
  (cd "$dir" && exec "$JAVA" -Xmx1g -cp "$RE_DIR/server:$RE_DIR/libs/*" org.bukkit.craftbukkit.Main --forceUpgrade --nogui --nojline < /dev/null > "$dir/stdout.log" 2>&1) &
  pid=$!
  waited=0
  while ! grep -q 'Done (' "$dir/logs/latest.log" 2>/dev/null; do
    if ! kill -0 "$pid" 2>/dev/null; then
      echo "server exited before finishing, see $dir/stdout.log" >&2
      cp "$dir/stdout.log" "$OUT/last-failure.log" 2>/dev/null || true
      return 1
    fi
    waited=$((waited + 1))
    if [ "$waited" -gt "$TIMEOUT" ]; then
      kill "$pid" 2>/dev/null || true
      echo "timeout after ${TIMEOUT}s" >&2
      return 1
    fi
    sleep 1
  done
  kill "$pid" 2>/dev/null || true
  wait "$pid" 2>/dev/null || true
}

printf '[' > "$WORK/maps.json"
first=1
for entry in $MAPS; do
  slug="${entry%%:*}"
  name="$(printf '%s' "${entry#*:}" | tr '_' ' ')"
  from="$SRC/Epicube - $name"
  [ -f "$from/level.dat" ] && [ -d "$from/region" ] || { echo "missing map folder: $from" >&2; exit 1; }
  echo "== $slug"
  spawn="$(python3 -I "$HERE/nbtscan.py" spawn "$from/level.dat")"
  attempt=0
  while :; do
    attempt=$((attempt + 1))
    run="$WORK/$slug.$attempt"
    mkdir -p "$run/world"
    cp "$from/level.dat" "$run/world/level.dat"
    cp -R "$from/region" "$run/world/region"
    upgrade "$run"
    res="$run/world/dimensions/minecraft/overworld/region"
    if [ -d "$res" ] && python3 -I "$HERE/nbtscan.py" scan "$res"; then
      break
    fi
    [ "$attempt" -lt 3 ] || { echo "upgrade of $slug incomplete after $attempt attempts" >&2; exit 1; }
    echo "retrying $slug (unconverted chunks)" >&2
  done
  rm -rf "$OUT/$slug"
  mkdir -p "$OUT/$slug"
  cp "$run/world/level.dat" "$OUT/$slug/level.dat"
  cp -R "$res" "$OUT/$slug/region"
  [ "$first" = 1 ] || printf ',' >> "$WORK/maps.json"
  first=0
  set -- $spawn $(python3 -I "$HERE/nbtscan.py" json "$res")
  printf '\n  {"slug": "%s", "name": "%s", "spawn": [%s, %s, %s], "bounds": {"min": [%s, %s, %s], "max": [%s, %s, %s]}, "regions": %s}' \
    "$slug" "$name" "$1" "$2" "$3" "$4" "$5" "$6" "$7" "$8" "$9" "${10}" >> "$WORK/maps.json"
done
printf '\n]\n' >> "$WORK/maps.json"
cp "$WORK/maps.json" "$OUT/maps.json"
echo "done: $OUT"

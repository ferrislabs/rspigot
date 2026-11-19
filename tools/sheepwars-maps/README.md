# sheepwars-maps

Converts the EpiCube SheepWars maps (Minecraft 1.8 worlds) to the 26.3 world format (data version 5023) that rspigot reads.

## Provenance warning

These maps are EpiCube's, captured with WorldDownloader. Do NOT commit or redistribute them or their converted output. The output lives in `example/server/maps/`, which is git-ignored (`/example/server/*`).

## Usage

    sh tools/sheepwars-maps/convert.sh <Epicube_Map_v1 folder> [output folder]

The output folder defaults to `example/server/maps/sheepwars`. For each of the seven maps (arbres, buildings, dirigables, galions, ilot, japan-dream, steampunk) the script copies only `level.dat` and `region/` of `Epicube - <Name>` into a scratch directory, runs the Spigot 26.3 server with `--forceUpgrade` (Mojang data fixers), stops it once it reports `Done`, checks the result, and writes `<output>/<slug>/level.dat`, `<output>/<slug>/region/` and `<output>/maps.json` (slug, name, spawn from the source `level.dat`, non-air bounds, region count). The source folder is only read.

The upgrade is verified by `nbtscan.py`: every chunk must carry DataVersion 5023 and a block palette, non-air blocks must exist, and no legacy chunk may remain; otherwise the map is retried up to three times.

## Requirements

- POSIX sh and Python 3 (run with `-I`, standard library only)
- JDK 25 (`JAVA`, default `/opt/homebrew/opt/openjdk@25/bin/java`)
- the extracted server classes and libraries in `RE_DIR` (default `../re` next to the repository: `server/` and `libs/`)
- a free local TCP port (`PORT`, default 25591); the server runs offline and needs no network
- about 1 GB of RAM for the JVM and about 50 MB of disk per run; about 45 s for the seven maps

`TIMEOUT` (seconds, default 300) bounds each map.

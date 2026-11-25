# Datapacks


The server reads worldgen datapacks from `<world>/datapacks/`, in the vanilla layout: one folder per pack, each with a `pack.mcmeta`. Zipped packs are not read; extract them. Packs apply in alphabetical order, and a later pack replaces an earlier one that uses the same identifier, a vanilla one included. A file that fails to load is skipped with a warning in the log; the server keeps starting. Structure templates and the other worldgen files are read once at startup. Functions, predicates, loot tables and tags load from the same `<world>/datapacks/` folder, and `/reload` (or `/datapack enable|disable`) reads them again; zipped packs are still not read.

- `data/<ns>/function/**.mcfunction` and `data/<ns>/tags/function/**.json` (`#minecraft:load`, `#minecraft:tick`), run with `/function`, `/schedule` and `execute if function`.
- `data/<ns>/predicate/**.json`, a loot condition or an array of them (all must hold), used by `execute if|unless predicate <id>` and the selector option `predicate=<id>` / `predicate=!<id>`. An unsupported condition makes the file fail to load, with a warning.
- `data/<ns>/loot_table/**.json`, replacing the built-in table with the same identifier (a later pack replaces an earlier one) or adding a new one. Entries and functions the server cannot represent are skipped with a warning.
- `data/<ns>/tags/<registry>/**.json` for `block`, `item`, `entity_type`, `fluid`, `game_event`, `damage_type`, `enchantment`, `worldgen/biome` and `point_of_interest_type`, merged over the built-in tags (`replace`, `#nested` and optional entries work). Joining players receive the merged tags; a `/reload` does not resend them to players already online.

```mcfunction
# data/example/function/hello.mcfunction
execute if predicate example:night run say good evening
```

```json
{
  "condition": "minecraft:time_check",
  "value": { "min": 13000, "max": 23000 },
  "period": 24000
}
```

Worldgen overrides, read once at startup (a file the engine cannot evaluate is skipped with a warning naming it): `worldgen/noise`, `worldgen/density_function` (types the engine lacks, such as `end_islands`, `weird_scaled_sampler` and `shift`, are rejected by function name), `worldgen/noise_settings` for `minecraft:overworld`, `minecraft:large_biomes` and `minecraft:amplified` (router, aquifers, sea level, height; ore veins, default blocks and spawn target stay built in), `worldgen/biome` (temperature, precipitation, feature lists, carver list), `worldgen/carver` (cave and canyon), `worldgen/placed_feature`, `worldgen/feature`, and the structure files above. A `dimension/*.json` using the `minecraft:noise` generator with one of the three noise settings and the `minecraft:overworld` multi_noise preset can be named as a world generator. Biome climate parameters, surface rules, `dimension_type`, nether and end terrain cannot be overridden.

Villages, pillager outposts, woodland mansions, end cities and the non-jigsaw structures are built in. Abandoned camps, ancient cities, bastions, trail ruins and trial chambers come from [`datapacks/vanilla-extras`](vanilla-extras), which is embedded in the server binary: on startup it is installed into `<world>/datapacks/vanilla-extras` when that folder is absent, so nothing has to be copied. An existing `<world>/datapacks/vanilla-extras` is never overwritten; delete it to get the current pack back.

To add a custom structure, put these files in a pack (`<ns>` is your namespace), with the same JSON as vanilla:

- `data/<ns>/structure/<name>.nbt`: the gzip NBT template, saved from a structure block.
- `data/<ns>/worldgen/template_pool/<name>.json`: a pool whose `location` is `<ns>:<name>`.
- `data/<ns>/worldgen/structure/<name>.json`: a `minecraft:jigsaw` structure with a `start_pool`, `biomes` (a list or a `#tag`), `step` and `size`.
- `data/<ns>/worldgen/structure_set/<name>.json`: the `placement` (`random_spread` or `concentric_rings`) and the structure with its weight.
- Optional: `worldgen/processor_list` and biome tags in `tags/worldgen/biome`.

[`example/datapacks/custom-hut`](../example/datapacks/custom-hut) is a complete one-template example, built by `tools/gen_example_datapack.py`. Only jigsaw structures are generated from packs; other structure types in a pack are loaded but not placed. `python3 -I tools/gen_structures.py --datapack-out datapacks/vanilla-extras` rebuilds the extras pack from a decompiled vanilla datapack.

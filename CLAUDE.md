# rspigot

Spigot 26.3 compatible Minecraft server in Rust. Spec and workstream tracking: EPIC issue #11 (one sub-issue per crate). `docs/` is local only and never committed.

Layers: `rspigot-blocks`, `rspigot-items`, `rspigot-effects`, `rspigot-entities`, `rspigot-data`, `rspigot-nbt` < `rspigot-domain` < `rspigot-ai` < `rspigot-core` (ports + use cases) < adapters (`rspigot-network`, `rspigot-plugins`, `rspigot-anvil`, `rspigot-worldgen`, `rspigot-auth`) < `apps/rspigot` (composition root). `rspigot-protocol` is the wire format, used only by `rspigot-network`.

Game objects: one generated struct per block, item, mob effect and entity type (`crates/rspigot-{blocks,items,effects,entities}`), never edited by hand. Regenerate with `tools/<kind>-dump/run.sh` then `python3 -I tools/gen_<kinds>.py` (block, item, effect, entity). Items also declare the crafting-table recipes that produce them (`Item::CRAFTS`, read from the datapack by `gen_items.py`). Behaviour lives in hand-written families (`src/family/`), mapped from vanilla classes in each crate's `families.txt`. Loot tables, registries and the tool block transformers (axe, hoe, shovel): `python3 -I tools/gen_data.py`. Pathfinding block classes (`crates/rspigot-ai/src/classify.rs`, template in `tools/ai-classify/emit.py`): `sh tools/ai-classify/run.sh`.

Recipe book table (`crates/rspigot-data/src/generated/recipe_book.rs`): `python3 -I tools/gen_recipe_book.py`. Advancements with their tab layout (`.../generated/advancements.rs`): `python3 -I tools/gen_advancements.py`. Both take `--check`.

Plugin contract (`crates/rspigot-plugin-protocol/proto`): regenerate the Rust code with `cargo run --manifest-path tools/plugin-protocol-gen/Cargo.toml` (needs `protoc`); `-- --check` fails when it is stale.

Plugin SDK typed keys (`crates/rspigot-plugin/src/keys`): regenerate with `python3 -I tools/gen_plugin_keys.py`; `--check` fails when they are stale.

Plugin events (`tools/plugin-events`): `python3 -I tools/gen_plugin_events.py` then the protocol generator; `--check` fails when stale, `--check-dump <event-types.json>` when the jar has an event the catalog lacks. The catalog is derived from the API dump with `python3 -I tools/plugin-events/extract.py <api-dump>`, whose `--check` fails when it is stale. Generator tests: `python3 -I tools/plugin-events/test_gen.py`.

Rules: no `async-trait`, no comments in code, static dispatch first, `thiserror` per crate, `anyhow` only in `apps/rspigot`. Stacked PRs, one per crate, in dependency order.

Verify: `cargo fmt --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace`.
Run: from `example/server`, `cargo run --release -p rspigot` (listens on 25565, files in the working directory, ignored by git). Example plugins go in `example/plugins/<name>`.

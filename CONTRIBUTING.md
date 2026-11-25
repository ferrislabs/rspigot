# Contributing to rspigot

Thanks for your interest in rspigot! Bug reports, fixes, features and documentation are all welcome.

## Before you start

- **Bugs:** open an issue with the bug template. Say what you did, what you expected and what happened, with the server log and the client version.
- **Features:** open an issue first for anything bigger than a small fix, so we can agree on the design before you write code. Big pieces of work are tracked in EPIC issues, each with its own spec.
- **Security problems:** see [SECURITY.md](SECURITY.md), and never use a public issue.

## Development setup

The toolchain is pinned in [`rust-toolchain.toml`](rust-toolchain.toml). `rustup` installs it on first build.

```sh
cargo build --release -p rspigot
cd example/server && cargo run --release -p rspigot
```

`example/server` is ignored by git, so the server writes its files there. Join with a 26.3 client on `localhost:25565`, or run [`rspigot-probe`](apps/rspigot-probe/README.md) for a headless client.

## Before you open a pull request

Run the same checks as CI:

```sh
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

Some files are generated. Do not edit them by hand: regenerate them and commit the result.

| What | Command |
|---|---|
| Blocks, items, effects, entities | `tools/<kind>-dump/run.sh`, then `python3 -I tools/gen_<kinds>.py` |
| Loot tables, registries, tool transformers | `python3 -I tools/gen_data.py` |
| Recipe book, advancements | `python3 -I tools/gen_recipe_book.py`, `python3 -I tools/gen_advancements.py` |
| Plugin and proxy gRPC contracts | `cargo run --manifest-path tools/plugin-protocol-gen/Cargo.toml` (needs `protoc`) |
| Plugin SDK keys | `python3 -I tools/gen_plugin_keys.py` |
| Plugin events | `python3 -I tools/gen_plugin_events.py`, then the protocol generator |

Each generator takes `--check`, which fails when the committed output is stale.

## Code style

- **Layers.** The workspace is hexagonal. Domain crates do no I/O, `rspigot-core` owns the ports, and adapters (`rspigot-network`, `rspigot-plugins`, `rspigot-anvil` and the others) implement them. Dependencies point inwards.
- **Dispatch.** Prefer static dispatch: generics first, then enums, `dyn` only when needed. No `async-trait` in our own code.
- **Errors.** One `thiserror` enum per crate. `anyhow` only in the binaries.
- **Comments.** None in the code. Names, types and tests carry the meaning. Explain the why in the pull request.
- **Tests.** Every change comes with tests. Protocol changes get byte-level tests against the vanilla layout. Behaviour changes get a test that fails without the change.

## Pull requests

- Keep a pull request to one crate when you can. A change that spans several crates goes as stacked pull requests, in dependency order.
- Use [Conventional Commits](https://www.conventionalcommits.org/) for titles: `feat(proxy): …`, `fix(core): …`, `perf(network): …`, `docs: …`.
- Say in the description what changed, why, and how you tested it, by hand included.
- Pull requests are squash-merged.

## License

Unless you explicitly state otherwise, any contribution you intentionally submit for inclusion in rspigot, as defined in the Apache-2.0 license, is licensed under the [Apache License, Version 2.0](LICENSE), without any additional terms or conditions.

## Code of Conduct

This project follows the [Contributor Covenant](CODE_OF_CONDUCT.md). By taking part you agree to follow it.

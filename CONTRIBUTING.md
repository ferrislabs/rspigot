# Contributing to rspigot

Thanks for taking the time. Bug reports, fixes, new features and doc changes all help.

## Before you start

If something is broken, open an issue with the bug template. Tell us what you did, what you expected and what you got, and attach the server log and your client version. A comparison with vanilla or Spigot 26.3 helps a lot.

For anything bigger than a small fix, open an issue first so we can agree on the design before you write the code. Large pieces of work have an EPIC issue with their own spec, so look there before starting something big.

Security problems go through [SECURITY.md](SECURITY.md), never through a public issue.

## Setting up

The toolchain is pinned in [`rust-toolchain.toml`](rust-toolchain.toml), and `rustup` installs it the first time you build.

```sh
cargo build --release -p rspigot
cd example/server && cargo run --release -p rspigot
```

Git ignores `example/server`, so the server can write its files there. Join with a 26.3 client on `localhost:25565`, or use [`rspigot-probe`](apps/rspigot-probe/README.md) if you want a client without a screen.

## Before you open a pull request

Run these three and make sure they pass:

```sh
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

Some files are generated. Don't edit them by hand. Run the generator and commit what it writes.

| What | Command |
|---|---|
| Blocks, items, effects, entities | `tools/<kind>-dump/run.sh`, then `python3 -I tools/gen_<kinds>.py` |
| Loot tables, registries, tool transformers | `python3 -I tools/gen_data.py` |
| Recipe book, advancements | `python3 -I tools/gen_recipe_book.py`, `python3 -I tools/gen_advancements.py` |
| Plugin and proxy gRPC contracts | `cargo run --manifest-path tools/plugin-protocol-gen/Cargo.toml` (needs `protoc`) |
| Plugin SDK keys | `python3 -I tools/gen_plugin_keys.py` |
| Plugin events | `python3 -I tools/gen_plugin_events.py`, then the protocol generator |

Every generator accepts `--check`, which fails if the committed output is out of date.

## How the code is written

The workspace is hexagonal. Domain crates don't do I/O, `rspigot-core` owns the ports, and the adapters (`rspigot-network`, `rspigot-plugins`, `rspigot-anvil` and so on) implement them. Dependencies point inwards.

We reach for generics first, then enums, and `dyn` only when nothing else fits. We don't use `async-trait` in our own code. Each crate has one `thiserror` error enum, and `anyhow` stays in the binaries.

There are no comments in the code. If something needs explaining, a better name or a type usually does it, and the reasoning belongs in the pull request.

Changes come with tests. For protocol work that means byte-level tests against the vanilla layout. For a behaviour change, add a test that fails without your change.

## Pull requests

Try to keep a pull request to a single crate. When a change spans several crates, send stacked pull requests in dependency order.

Titles follow [Conventional Commits](https://www.conventionalcommits.org/), for example `feat(proxy): …`, `fix(core): …` or `docs: …`. In the description, say what changed, why, and how you tested it, including anything you checked by hand in a client. We squash-merge.

## License

Unless you say otherwise, anything you submit for inclusion in rspigot is licensed under the [Apache License, Version 2.0](LICENSE), with no extra terms.

## Code of Conduct

The project follows the [Contributor Covenant](CODE_OF_CONDUCT.md). Taking part means you agree to it.

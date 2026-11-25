<div align="center">

# rspigot

**A Minecraft Java Edition server and proxy, written in Rust.**

Spigot-compatible configuration and worlds, out-of-process plugins over gRPC, and a Velocity-style proxy that never restarts to ship plugin changes.

[![License: MIT OR Apache-2.0](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)](#license)
[![Minecraft 26.3](https://img.shields.io/badge/Minecraft-26.3%20(protocol%20777)-62B47A.svg)](#compatibility)
[![Rust 1.99](https://img.shields.io/badge/rust-1.99-orange.svg)](rust-toolchain.toml)

[Quick start](#quick-start) ·
[Plugins](#plugins) ·
[Proxy](#proxy) ·
[Datapacks](datapacks/README.md) ·
[Contributing](CONTRIBUTING.md)

</div>

---

## Why rspigot

- **Drop-in for Spigot setups.** It reads and writes `server.properties`, `spigot.yml`, `bukkit.yml`, `ops.json`, the ban and whitelist files, and Anvil worlds in Spigot's layout.
- **Plugins never take the server down.** A plugin is a separate process that talks to the server over gRPC. It can crash, restart or be redeployed while players keep playing.
- **Every Bukkit event, typed.** The Rust SDK covers every Bukkit event except the deprecated and JVM-only ones, with compile-time checked items, blocks, sounds and particles.
- **Vanilla behaviour first.** Commands, loot tables, recipes, advancements, structures and worldgen follow the 26.3 data. Plugin orders run through the same code as the matching vanilla command.
- **A proxy in the same family.** `rspigot-proxy` replaces Velocity in front of rspigot servers. It moves players between servers without a disconnect, and its plugins use the same gRPC model.
- **Small and fast.** In our load test at 50 clients, the proxy used about 10 times less memory and 6 times less CPU than Velocity, and delivered chunks 40 to 70 % faster. The numbers and their limits are in [#453](https://github.com/ferrislabs/rspigot/issues/453).

## Quick start

### From source

```sh
cargo build --release -p rspigot
./target/release/rspigot
```

On first start the server creates its configuration files in the working directory, then listens on port `25565`. For local testing, run it from `example/server`, which git ignores.

> [!NOTE]
> Like vanilla 26.3, `white-list` defaults to `true`. Run `whitelist add <name>` in the console, or set `white-list=false`, before you join. Operators from `ops.json` always get in, and `online-mode=false` skips Mojang authentication.

Options: `--port <n>`, `--world-dir <dir>`, `--online-mode <true|false>`. Type `help` in the console for commands. `stop` or Ctrl-C saves the world and exits.

### With Docker

```sh
docker run -it -p 25565:25565 -v rspigot:/data ghcr.io/ferrislabs/rspigot:latest
```

The configuration and the world live in `/data`, and `docker stop` saves the world. Pushing a tag `x.y.z` publishes `ghcr.io/ferrislabs/rspigot` for amd64 and arm64, tagged `x.y.z`, `x.y`, `x` and `latest`. Plugins in other containers need `host: 0.0.0.0` and a `token` in `plugins.yml`.

## Plugins

A plugin is a Rust program built on the [`rspigot-plugin`](crates/rspigot-plugin) crate. It connects to the server's plugin port (`25580` by default) and declares the events it listens to and the commands it adds.

```rust
use rspigot_plugin::prelude::*;

#[derive(Plugin, Default)]
struct Welcome;

#[listeners]
impl Welcome {
    async fn on_player_join(&self, event: &PlayerJoin) {
        let player = event.player();
        player.message.send(format!("{}Welcome {}!", ChatColor::Gold, player.name));
    }
}

#[tokio::main]
async fn main() -> Result<(), Error> {
    rspigot_plugin::run(Welcome::default()).await
}
```

- [`crates/rspigot-plugin/GUIDE.md`](crates/rspigot-plugin/GUIDE.md): the full guide (plugin port, events, handles, items, menus, permissions, commands, modules, config).
- [`crates/rspigot-plugin/EVENTS.md`](crates/rspigot-plugin/EVENTS.md): every event, its setters, and whether the server fires it today.
- [`example/plugins`](example/plugins): complete plugins, from a one-file welcome message to a three-level factions plugin.

## Proxy

`rspigot-proxy` sits in front of rspigot servers, like Velocity. Players join it once and move between servers without reconnecting.

```sh
cargo run --release -p rspigot-proxy -- <directory>
```

- **Modern forwarding.** Backends get the real UUID, name, skin and IP, signed with a shared secret.
- **Server switches without a disconnect.** The proxy clears what the old server showed (tab list, boss bars, scoreboard, titles) and keeps signed chat working.
- **Fallbacks.** A kicked player, or one whose server goes down, is moved to the next server of the `try` list.
- **Plugins over gRPC** with [`rspigot-proxy-plugin`](crates/rspigot-proxy-plugin): hooks on login, server choice, connection and kicks, proxy commands with tab completion, permissions, and orders to move, message or kick players. Redeploying a plugin never disconnects anyone.
- **Ready for production.** Prometheus metrics, `/health/live` and `/health/ready`, and a drain on SIGTERM.

Configuration and operations: [`apps/rspigot-proxy/README.md`](apps/rspigot-proxy/README.md). Example plugin: [`example/proxy-plugins/network`](example/proxy-plugins/network).

## Compatibility

| | |
|---|---|
| Minecraft | Java Edition 26.3, protocol 777, world version 5023 |
| Clients | Unmodified 26.3 clients. Other versions are refused with a clear message. |
| Worlds | Anvil worlds from Spigot 26.3, and worldgen datapacks ([guide](datapacks/README.md)) |
| Java plugins | Not supported. Plugins are Rust programs over gRPC. |

## Project layout

| Path | Role |
|---|---|
| `crates/rspigot-data` | Generated 26.3 tables: blocks, items, registries, tags |
| `crates/rspigot-nbt` | NBT model and codec |
| `crates/rspigot-domain` | World, chunk and player model |
| `crates/rspigot-core` | Ports, game loop, chunk loading and streaming, commands |
| `crates/rspigot-protocol` | Wire format and packets |
| `crates/rspigot-network` | Client connections (tokio) |
| `crates/rspigot-anvil` | Region file storage |
| `crates/rspigot-worldgen` | Chunk generators |
| `crates/rspigot-auth` | Mojang session verification |
| `crates/rspigot-plugins`, `rspigot-plugin`, `rspigot-plugin-protocol` | Plugin host, plugin SDK and the gRPC contract |
| `crates/rspigot-proxy-*` | Proxy core, network, plugin host and plugin SDK |
| `apps/rspigot` | The server binary |
| `apps/rspigot-proxy` | The proxy binary |
| `apps/rspigot-probe` | A test client that measures chunk delivery ([README](apps/rspigot-probe/README.md)) |

## Contributing

Contributions are welcome. [CONTRIBUTING.md](CONTRIBUTING.md) explains how to build, test and open a pull request, and the project follows its [Code of Conduct](CODE_OF_CONDUCT.md). Report security issues privately, as described in [SECURITY.md](SECURITY.md).

## License

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or <https://www.apache.org/licenses/LICENSE-2.0>)
- MIT license ([LICENSE-MIT](LICENSE-MIT) or <https://opensource.org/licenses/MIT>)

at your option.

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in the work by you, as defined in the Apache-2.0 license, shall be dual licensed as above, without any additional terms or conditions.

rspigot is not affiliated with Mojang Studios or Microsoft. Minecraft is a trademark of Mojang Studios.

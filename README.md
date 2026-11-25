<div align="center">

# rspigot

A Minecraft Java Edition server and proxy, written in Rust.

[![License: Apache-2.0](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)
[![Minecraft 26.3](https://img.shields.io/badge/Minecraft-26.3%20(protocol%20777)-62B47A.svg)](#compatibility)
[![Rust 1.99](https://img.shields.io/badge/rust-1.99-orange.svg)](rust-toolchain.toml)

[Quick start](#quick-start) ·
[Plugins](#plugins) ·
[Proxy](#proxy) ·
[Datapacks](datapacks/README.md) ·
[Contributing](CONTRIBUTING.md)

</div>

---

rspigot speaks the 26.3 protocol and reads a Spigot server's files as they are: `server.properties`, `spigot.yml`, `bukkit.yml`, `ops.json`, the ban and whitelist lists, and Anvil worlds in Spigot's layout. You can point it at an existing setup and start it.

Plugins are where it differs most from Spigot. A plugin is its own process and talks to the server over gRPC, so a plugin that crashes or gets redeployed doesn't take the players with it. The Rust SDK has a type for every Bukkit event, except the deprecated and JVM-only ones. Items, blocks, sounds and particles are enums, so a typo in `Material::DiamondSword` is a compile error instead of a bug report.

We try to behave like vanilla before anything else. Commands, loot tables, recipes, advancements, structures and worldgen come from the 26.3 data, and when a plugin teleports a player it goes through the same code as `/tp`.

The repository also has `rspigot-proxy`, which does Velocity's job in front of rspigot servers. Its plugins work the same way, so you can ship a proxy plugin change without restarting the proxy. In a local load test with 50 clients it used about a tenth of Velocity's memory and a sixth of its CPU. That test ran on one machine, and its limits are written up in [#453](https://github.com/ferrislabs/rspigot/issues/453).

## Quick start

### From source

```sh
cargo build --release -p rspigot
./target/release/rspigot
```

On first start the server writes its configuration files to the working directory and listens on port `25565`. For local testing, run it from `example/server`, which git ignores.

> [!NOTE]
> As in vanilla 26.3, `white-list` defaults to `true`. Run `whitelist add <name>` in the console, or set `white-list=false`, before you join. Operators from `ops.json` always get in, and `online-mode=false` skips Mojang authentication.

The binary takes `--port <n>`, `--world-dir <dir>` and `--online-mode <true|false>`. Type `help` in the console to list the commands. `stop` or Ctrl-C saves the world and exits.

### With Docker

```sh
docker run -it -p 25565:25565 -v rspigot:/data ghcr.io/ferrislabs/rspigot:latest
```

The configuration and the world live in `/data`, and `docker stop` saves the world. Each `x.y.z` tag publishes `ghcr.io/ferrislabs/rspigot` for amd64 and arm64, tagged `x.y.z`, `x.y`, `x` and `latest`. If your plugins run in other containers, set `host: 0.0.0.0` and a `token` in `plugins.yml`.

## Plugins

A plugin is a Rust program built on the [`rspigot-plugin`](crates/rspigot-plugin) crate. It connects to the server's plugin port, `25580` by default, and tells the server which events it listens to and which commands it adds.

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

The [plugin guide](crates/rspigot-plugin/GUIDE.md) covers the rest: the plugin port settings, events, handles, items, menus, permissions, commands, modules and config files. [EVENTS.md](crates/rspigot-plugin/EVENTS.md) lists every event with its setters and says whether the server fires it yet. For working code, [`example/plugins`](example/plugins) goes from a one-file welcome message up to a factions plugin split into three levels of modules.

## Proxy

`rspigot-proxy` sits in front of your rspigot servers. Players join it once and move between servers without reconnecting.

```sh
cargo run --release -p rspigot-proxy -- <directory>
```

It forwards each player's real UUID, name, skin and IP to the backends with Velocity's modern forwarding, signed with a shared secret. When a player changes server, the proxy clears what the old one left on screen (tab list, boss bars, scoreboard, titles) and keeps signed chat working. A kicked player, or one whose server goes down, lands on the next server in the `try` list.

Proxy plugins use [`rspigot-proxy-plugin`](crates/rspigot-proxy-plugin). They can step in at login, when the proxy picks a server, before a connection and after a kick. They can also add commands with tab completion, set permissions and move, message or kick players. For production there are Prometheus metrics, `/health/live` and `/health/ready`, and the proxy drains its players on SIGTERM.

Configuration and operations are in [`apps/rspigot-proxy/README.md`](apps/rspigot-proxy/README.md), and [`example/proxy-plugins/network`](example/proxy-plugins/network) is a small plugin with `/hub`, `/server` and `/find`.

## Compatibility

| | |
|---|---|
| Minecraft | Java Edition 26.3, protocol 777, world version 5023 |
| Clients | Unmodified 26.3 clients. Other versions get a message telling them which one to use. |
| Worlds | Anvil worlds from Spigot 26.3, plus worldgen datapacks ([guide](datapacks/README.md)) |
| Java plugins | Not supported. Plugins are Rust programs that talk gRPC. |

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
| `apps/rspigot` | The server |
| `apps/rspigot-proxy` | The proxy |
| `apps/rspigot-probe` | A headless client that measures chunk delivery ([README](apps/rspigot-probe/README.md)) |

## Contributing

Pull requests are welcome. [CONTRIBUTING.md](CONTRIBUTING.md) explains how to build, test and send one. Please follow the [Code of Conduct](CODE_OF_CONDUCT.md), and report security issues privately as described in [SECURITY.md](SECURITY.md).

## License

rspigot is licensed under the [Apache License, Version 2.0](LICENSE). Unless you say otherwise, anything you submit for inclusion is licensed the same way, with no extra terms.

rspigot is not affiliated with Mojang Studios or Microsoft. Minecraft is a trademark of Mojang Studios.

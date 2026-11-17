# rspigot

A Minecraft Java Edition 26.3 server (protocol 777) written in Rust. It reads and writes Spigot's configuration files and world layout.

## Run

```
cargo build --release -p rspigot
./target/release/rspigot
```

The server creates `server.properties`, `spigot.yml`, `bukkit.yml`, `ops.json`, `banned-players.json` and `banned-ips.json` in the working directory on first start, then listens on port 25565. For local testing, run it from `example/server`, which git ignores. Like vanilla 26.3, `white-list` defaults to `true`: run `whitelist add <name>` in the console (or set `white-list=false`) before joining. Operators from `ops.json` always get in. `online-mode=false` skips Mojang authentication.

Options: `--port <n>`, `--world-dir <dir>`, `--online-mode <true|false>`. Type `help` in the console for commands; `stop` or Ctrl-C saves the world and exits.

## Plugins

A plugin is a separate program, never loaded into the server. It connects to the server's plugin port when it starts, so it can crash, restart or be redeployed while the server keeps running. Plugins are written in Rust with the `rspigot-plugin` crate; [`example/plugins/welcome`](example/plugins/welcome) is a complete one. The `plugins` console command lists the connected plugins.

The server creates `plugins.yml` in the working directory on first start:

| Key | Default | Meaning |
|---|---|---|
| `host` | `127.0.0.1` | Address the plugin port listens on |
| `port` | `25580` | The plugin port, separate from the Minecraft port |
| `token` | `''` | Secret every plugin must send; `RSPIGOT_PLUGIN_TOKEN` overrides it |
| `decision-ticks` | `1` | Ticks an event waits for the deciding plugins (1 to 20) |
| `late-cancel-ticks` | `20` | Ticks after a move during which a late cancel still sends the player back (0 to 100) |
| `order` | `[]` | Plugin names; on equal priority the first listed runs first |
| `priorities` | `{}` | Overrides a handler's `#[priority]`, for a whole plugin (`welcome: high`) or one event, named in snake case (`welcome.player_move: lowest`) |

The server refuses to start if `host` is not a loopback address and no token is set. A plugin finds the server through `RSPIGOT_URL` (default `http://127.0.0.1:25580`) and sends `RSPIGOT_TOKEN` when the server has a token.

Every Bukkit event a plugin can use (all but the deprecated and JVM-only ones) has a type in `rspigot-plugin`, named after it without the `Event` suffix (`PlayerQuitEvent` is `PlayerLeave`). [`crates/rspigot-plugin/EVENTS.md`](crates/rspigot-plugin/EVENTS.md) lists them with their setters and says which ones the server fires today. A handler reads its event through methods (`event.player()`, `event.to()`). A handler also hears the events that share its Bukkit handler list, as its own type: an `EntityDamage` handler hears an `EntityDamageByEntity`, a `PlayerMove` handler does not hear a `PlayerTeleport`.

A handler that takes `&Event` observes: the server does not wait for it and tells it the settled outcome. A handler that takes `&mut Event` decides, on an event that can be cancelled or changed: it may call `event.cancel()` when the event is cancellable and the `set_` methods of the event (`set_to` on a move). Each deciding handler works on its own copy of the event. The server holds the action behind the event (a move, a chat message, a click, a drop) until a decider cancels, all of them answer or `decision-ticks` pass, then folds the answers in handler order (priority, then `order`, then name): any cancel wins, otherwise the last handler to set a field wins. Lists are the exception: a `set_` on a list (`set_recipients`, `set_drops`, `set_block_list`, `set_commands`...) sends what the handler changed in the list it received: the elements it removed, the ones it changed in place (an item stack of the same item at the same place, a dropped item of the same entity) and the ones it added. The server applies them in handler order: an element any handler removed is gone even if another handler changed it, the last handler to change an element wins it, and additions are appended, so two plugins that each remove a chat recipient both keep the message from them. Each handler receives the original list, so a later handler that wants an element there even if an earlier one removed it pushes it again: recipients, commands and explosion block lists are sets and keep it once. A reorder alone changes nothing. A plugin stops running its deciding handlers at its first cancel; the ones it skipped count as no opinion. A value the server cannot apply (a number that is not finite, a destination outside the world) is dropped with a warning to the plugin: it does not cancel the event, and the last valid value wins. A cancel of a move that misses that deadline, from a decider that had not answered yet, sends the player back if it arrives within `late-cancel-ticks`; after that, and for every other event, it is ignored and the plugin gets a warning. The events of one player settle one after the other, in the order they happened: a click waits for the move sent before it, and an event that is only observed still waits for the events of its player sent before it. An event no plugin listens to applies at once, as without plugins. An event born while another one applies, such as the `PlayerItemDamage` of a shear, settles before the rest of that player's events. Server events settle the same way per key: the events of one entity, one block position, one chunk or the world wait for each other, the others go on. An entity event born from the action of a player, such as the damage of a hit or the knockback that follows it, settles in that player's order. An event the game creates already cancelled, such as a `BlockBreak` with a sword in creative, is not asked: observers hear it cancelled. When a player leaves, their pending events settle at once with the answers received so far, before the player is saved; the pending events that act on an entity do the same when its chunk unloads, whatever lane holds them and in the order of that lane, before the entity is saved, and the server does the same for every lane when it stops. Where Bukkit ignores the outcome of an event (a punch in the air), the deciding handlers are still asked but nothing waits for them and their answers are dropped.

`PlayerInteract` and the inventory interaction events (`InventoryClick`, `InventoryDrag`, `CraftItem` and the others listed in `EVENTS.md`) are cancelled through their result, as in Bukkit: `cancel()` sets the result to `Deny`, `is_cancelled()` reads the main result, and a later handler that sets the result back lifts the cancel. Every deciding handler of these events runs, and the event is cancelled when its main result is still `Deny` once the answers are folded. Their direct writes (`set_current_item` and `set_cursor` on a click) apply even when the event ends up cancelled.

Inventory clicks, drags and creative slots of a player wait in that player's order, so a click sent after another one reads the window as the first one left it. A click is classified when its turn comes and applied once its event settles: if a write (`set_current_item`, `set_cursor`) targets a slot or a cursor another viewer changed meanwhile, the click is refused and the window resent, while a click without writes applies on the window as it is. A refused click is resent to the client, never applied. A spectator's click starts denied and a later handler can let it through; a click in a locked plugin inventory is born cancelled by the lock and not asked. `PrepareItemCraft` and `PrepareSmithing` settle before the next click of the player, so a shift-click on a patched result crafts the patched item. Each furnace asks its events in its own order and waits while one is asked.

Server list pings are answered by the network, outside the tick: when no plugin listens to `ServerListPing` the answer comes from `server.properties`, an observing plugin hears the ping without delaying it, and a deciding plugin makes every ping wait until the event settles, at most `decision-ticks`, so decide it only to change the server list. `ServerLoad` fires once, when the server starts running, for the plugins connected by then: a plugin that connects later never hears it. `PluginEnable` and `PluginDisable` tell the other plugins that a plugin connected or disconnected; at shutdown every plugin hears the `PluginDisable` of the others before the connections close. `ServerCommand` sees console lines only, run in the order they were typed: `dispatch_command` and player commands never fire it, `PlayerCommandPreprocess` covers players. `TabComplete` fires for every completion a player asks, vanilla commands included. `BroadcastMessage` fires for the plugin broadcast order, never for `/say` or `/me`. Every order a handler sends while it runs carries the id of its event, so a broadcast sent while handling a `BroadcastMessage`, deciding or observing, goes out without firing the event again, and any other broadcast waits behind the ones already asked.

Handlers take `&self`, so plugin state lives in fields behind a `Mutex` or atomics.
`rspigot-plugin` is not published: depend on it with a path or git dependency on `crates/rspigot-plugin`.

A plugin handles its events one at a time, in the order they arrive. Any `.await` in a handler, observers included, delays the plugin's later decisions: a cancel that misses the deadline no longer prevents the move, at best it rolls it back.

A handler reaches the server through handles. `event.player()` gives a `Player`. `player.server()` and `player.world()` give the `Server` and the `World`. `world.block(pos)` gives a `Block`; `world.get_block(pos)` reads one. An `Entity` is a mob, a dropped item or any other non-player entity: `world.entities.get_entities()` lists them, and `world.spawn` and `world.drop_item` wait and return the new one. A player is never an `Entity`: `player.target_entity` returns a `Target`, which is either a `Player` or an `Entity`. Going from one handle to another (`player.world()`, `world.block(pos)`) costs nothing, and a handle's fields (`player.name`, `player.position`) are a snapshot taken when it was made. The actions of the `World` and the `Server` are grouped the same way and keep the default world: `world.time().set(6000)`, `world.weather().set(Weather::Rain)`, `world.difficulty()`, `world.spawn_point()`, `world.pvp()`, `world.game_rule()`, `world.storage().save()`, `world.sound().play(cue)`, `world.particle().spawn(burst)`, `world.explosion().create(Explosion::new(pos, 4.0, true))`; `server.message().broadcast(text)`, `server.command().dispatch(line)`, `server.lifecycle().shutdown()`, `server.whitelist().add(name)`, `server.operators()`, `server.bans()` and `server.ip_bans()` (`add`, `remove`). Their lists are read on the same handles: `server.whitelist().get_names()`, `server.operators().get_names()`, `server.bans().get_entries()` and `server.ip_bans().get_entries()`. A player's actions are grouped by domain and named `domain().verb(...)`, with no shortcut: `player.message().send(text)`, `player.inventory().close()`, `player.health().set(5.0)`, `player.sound().play(SoundCue::new(Sound::EntityCowHurt))`. The domains are `inventory`, `health`, `food`, `experience`, `effects`, `abilities`, `movement`, `operator`, `connection`, `sound`, `particle`, `message`, `title`, `action_bar`, `tab_list`, `location`, `game_mode`, `command`, `visibility` and `scoreboard`. Each verb is generated from the `path` its order carries in `plugin.proto` (`cargo run --manifest-path tools/plugin-sdk-gen/Cargo.toml`, `-- --check` fails when stale). Other methods carry the Rust name of the Spigot method: `getTargetBlock` is `target_block`.

Most calls are orders (`message().send`, `health().set`, `experience().give`, `connection().kick`, `inventory().add`, `block.set`, `server.message().broadcast`): they return nothing and the plugin does not wait. The server applies them in the order the plugin sent them, before its next tick, with the code of the matching vanilla command or gameplay, so a plugin teleport behaves like `/tp`. An order sent while the plugin is not connected is dropped. An order the server cannot apply as given (a player that left, a value out of range) changes nothing, and the server logs a warning naming the plugin and the cause, which the plugin also logs: `food().set_level(21)` logs `rspigot: player.set_food_level was not applied: food level 21 is outside 0..=20`. Values Spigot refuses are refused this way, not clamped.

The actions whose outcome a plugin needs to know are awaited and return `Result<(), Error>`: `location().teleport(..)`, `teleport_facing(..)`, `teleport_to(..)`, `inventory().open(&inventory)`, `server.create_inventory(..)`, `server.create_storage(..)` and `world.spawn(..)`. The server answers with success or an `Error::Action(ActionError { kind, reason })` whose kind is `InvalidArgument`, `TargetGone` (the player left), `IncompatibleState` (a dead player, an inventory already open), `Cancelled` (a plugin cancelled the event), `NotFound` or `LimitReached`.

Questions are `async` and cost one round trip. Every read starts with `get_`, and the plural reads a list. `player.get_state()` returns a `PlayerState` (health, food, experience, effects, ping, address and more), `player.get_position()` the current position, `player.inventory().get_items()` the slots, and blocks, entities, custom inventories, the server and the world have their own `get_state()` (`world.get_block(pos)` for a block). Lists: `server.get_players()`, `server.get_players_named(name)`, `server.bans().get_entries()`, `world.entities.near(pos, 8.0).of(EntityType::Zombie).get_entities()`. `player.target_block(5.0)` and `player.target_entity(5.0)` return what the player looks at, up to 120 blocks away. A question about something that is gone returns `Error::NotFound`, one asked while the plugin is not connected `Error::Disconnected`, and one without an answer after 10 seconds `Error::Timeout`. Orders and questions travel on one stream, so a question sees the effect of every order sent before it. `world.spawn(EntityType::Chicken, pos)` and `world.drop_item(pos, item)` wait like a question; when the server refuses, they return `Error::Action` with its kind and reason, for example a monster on peaceful or a chunk that is not loaded.

Text a plugin sends (messages, titles, the action bar, the tab list, kick reasons, broadcasts, item names and lore, inventory titles) uses Spigot's `§` codes. `ChatColor` writes them: `format!("{}Welcome", ChatColor::Gold)` is `"§6Welcome"`. `&` is not a code and shows as typed. As in Spigot, a color code or `§r` also resets bold, italic and the other styles, so an item name that starts with one is not italic. Item names and lore read from the server come back in the same form.

Items, blocks, effects, entity types, sounds, particles and enchantments are enums, so a typo does not compile: `Material::DiamondSword`, `BlockType::Stone`, `EffectType::Speed`, `EntityType::Zombie`, `Sound::EntityCowHurt`, `Particle::HappyVillager`, `Enchantment::Sharpness`. `key()` and `from_key()` convert to and from the namespaced key (`minecraft:diamond_sword`). `Material` and `BlockType` are separate because not every item is a block. `Particle` only lists what the server can send, with the data it needs, such as `Particle::Block(BlockType)` or `Particle::Dust { color, scale }`. The lists in `crates/rspigot-plugin/src/keys` are generated from the 26.3 server data: regenerate them with `python3 -I tools/gen_plugin_keys.py`; `--check` fails when they are stale.

`ItemStack::new(Material::Diamond, 3)` takes `.name()`, `.lore()` and `.enchant(Enchantment::Sharpness, 5)`. They are stored as the vanilla item components, so they show in the tooltip, make the enchantment glint and are saved with the item; the server does not apply enchantment effects yet. An item read from the server keeps its other components (damage, potion contents, shulker box contents, book pages) when it is written back, so moving a damaged sword keeps its damage. Player inventory slots are `InventorySlot::Hotbar(0..=8)`, `Main(0..=26)`, `Armor(ArmorSlot::Helmet)` (or `Chestplate`, `Leggings`, `Boots`) and `Offhand`. `inventory.add` works like `/give`: what does not fit drops at the player's feet.

`server.create_inventory(rows, title).await` makes a menu: a chest of 1 to 6 rows where no click moves an item, in it or in the player's own inventory while it is open. `server.create_storage(rows, title).await` makes an open one: players move items freely, and `get_state()` returns its contents and viewers. Fill either with `set(slot, item)` and show it with `player.inventory().open(&inventory).await`. Only the plugin that made it hears its `InventoryClick` (the slot, such as `ClickedSlot::Menu(4)`, the kind of click and the item) and `InventoryClose` events. An inventory lasts as long as the connection of the plugin that made it: when the plugin disconnects, its inventories close on every client and are forgotten. A plugin holds at most 1024.

Like Spigot's `onEnable` and `onDisable`, the `#[listeners]` block may hold `async fn on_enable(&self, server: &Server)` and `async fn on_disable(&self)`. `on_enable` runs on every accepted connection, the first and each reconnect, before any event; `on_disable` runs when that connection ends. Create menus in `on_enable`: a `CustomInventory` made before a reconnect is dead, orders on it are dropped with a logged warning and its `resolve()` returns `Error::NotFound`.

`world.time().set(6000)` works like Spigot's `setTime`: it moves forward to that time of day and keeps the day count, where `/time set` sets the absolute time. `block.break_naturally()` drops the block's loot whatever tool it needs, so stone gives cobblestone, like Spigot's `breakNaturally()`; `GameRule::BlockDrops` still applies. `sound().play` and `particle().spawn` on a `Player` reach that player only; on the `World` they reach everyone nearby.

`player.connection().ban(BanTerms { reason, expires })`, `server.bans().add(Ban::new(name))` and `server.ip_bans().add(Ban::new(address))` use the same ban lists as the `/ban`, `/ban-ip`, `/pardon`, `/pardon-ip` and `/banlist` commands. The lists are kept in `banned-players.json` and `banned-ips.json`, in the vanilla format. Banning a name the server has never seen stores an entry without a UUID; it matches that name at login and gets the UUID then. A vanilla server skips such entries. A ban reason is stored as given, as vanilla stores it, and is not parsed as `§` text; `§` codes in it still render on the client.

## Datapacks

The server reads worldgen datapacks from `<world>/datapacks/`, in the vanilla layout: one folder per pack, each with a `pack.mcmeta`. Zipped packs are not read; extract them. Packs apply in alphabetical order, and a later pack replaces an earlier one that uses the same identifier, a vanilla one included. A file that fails to load is skipped with a warning in the log; the server keeps starting. Datapacks are read once at startup.

Villages, pillager outposts and the non-jigsaw structures are built in. Abandoned camps, ancient cities, bastions, trail ruins and trial chambers live in [`datapacks/vanilla-extras`](datapacks/vanilla-extras): copy that folder into `<world>/datapacks/` to get them.

To add a custom structure, put these files in a pack (`<ns>` is your namespace), with the same JSON as vanilla:

- `data/<ns>/structure/<name>.nbt`: the gzip NBT template, saved from a structure block.
- `data/<ns>/worldgen/template_pool/<name>.json`: a pool whose `location` is `<ns>:<name>`.
- `data/<ns>/worldgen/structure/<name>.json`: a `minecraft:jigsaw` structure with a `start_pool`, `biomes` (a list or a `#tag`), `step` and `size`.
- `data/<ns>/worldgen/structure_set/<name>.json`: the `placement` (`random_spread` or `concentric_rings`) and the structure with its weight.
- Optional: `worldgen/processor_list` and biome tags in `tags/worldgen/biome`.

[`example/datapacks/custom-hut`](example/datapacks/custom-hut) is a complete one-template example, built by `tools/gen_example_datapack.py`. Only jigsaw structures are generated from packs; other structure types in a pack are loaded but not placed. `python3 -I tools/gen_structures.py --datapack-out datapacks/vanilla-extras` rebuilds the extras pack from a decompiled vanilla datapack.

## Probe

`rspigot-probe` is an offline-mode client that logs in like a 26.3 client and measures chunk delivery:

```
./target/release/rspigot-probe --address 127.0.0.1:25565 --view 10 --walk 640 --speed 4
```

It prints one JSON line: time to first chunk, time until the whole view is loaded, chunks per second, duplicates, and how many chunks near the player were missing while walking.

## Layout

| Crate | Role |
|---|---|
| `rspigot-data` | Generated 26.3 tables (blocks, items, registries, tags) |
| `rspigot-nbt` | NBT model and codec |
| `rspigot-domain` | World, chunk and player model |
| `rspigot-core` | Ports, game loop, chunk loading and streaming, commands |
| `rspigot-protocol` | Wire format and packets |
| `rspigot-network` | TCP connections (tokio) |
| `rspigot-anvil` | Region file storage |
| `rspigot-worldgen` | Chunk generators |
| `rspigot-auth` | Mojang session verification |
| `apps/rspigot` | The server binary |
| `apps/rspigot-probe` | Test client |

## License

MIT, see LICENSE.

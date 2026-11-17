# welcome

An example plugin that uses each handle of the `rspigot-plugin` crate at least once.

When it connects, it creates a one-row menu titled "Menu" (in gold), with a compass named "Leave" in the middle slot. The menu is locked, so clicks never move its items.

When a player joins, it:

- says "Welcome <name>!" and shows a gold "Welcome" title with the player's name as subtitle (`Player`);
- gives them a compass named "Menu" (`Inventory`);
- logs how many players are online (`Server`);
- plays the level-up sound at their position (`World`) and logs the block under their feet (`Block`);
- spawns a chicken at their feet and makes it glow for 10 seconds (`Entity`);
- opens the menu (`CustomInventory`).

Clicking the compass in the menu closes it and says "See you around". The plugin also cancels every move, so players stay where they are.

It also creates a void world named `lobby` when it connects, and deletes it when it stops if no player is in it (a refusal is logged).

`/welcome <place>` needs the `welcome.use` right, and `<place>` completes from `lobby`, `hub` and `zombie`:

- `lobby` teleports the player to the lobby (`player.location().teleport_to_world`);
- `hub` transfers the player to `127.0.0.1:25565` (`player.connection().transfer`), only with the `welcome.hub` right (`player.permissions().has`); anyone else is told they may not;
- `zombie` spawns a baby zombie with 40 health at the player (`entity.attributes().set`, `entity.as_zombie()`).

Any failure is reported to the player. Every hit between entities is logged with `event.player()`, which is `None` when a mob hit.

Copy `permissions.yml` from this folder to the server folder to give `Steve` the hub right.

The API follows four rules (see the [SDK README](../../../crates/rspigot-plugin/README.md)): domains named `domain.verb(..)`, `get_` for reads, `event.player()` for the actor, and a `Result` for every action the server can refuse.

It reads its events through methods (`event.player()`, `event.slot()`); every event and its methods are listed in [`crates/rspigot-plugin/EVENTS.md`](../../../crates/rspigot-plugin/EVENTS.md).

Start the server in one terminal (it writes its files, `plugins.yml` included, in the directory it runs from):

```
cargo run --release -p rspigot
```

and the plugin in another:

```
cargo run -p welcome
```

With the defaults both sides use `127.0.0.1:25580`. The plugin logs `Connected to rspigot as welcome 0.1.0` once the server accepts it, and `Not connected to rspigot at <url> (<reason>), retrying in <n> s` while it cannot reach it. In the server console, `plugins` shows `Plugins (1): welcome`. Join with a client: the title shows, the menu opens and a glowing chicken appears, and every step snaps back. The plugin logs `<name> joined, <n> of <max> players online` and `<name> stands on minecraft:<block>`. Stop the plugin with Ctrl-C: an open menu closes and players move again.

To reach another server, set `RSPIGOT_URL` (for example `RSPIGOT_URL=http://10.0.0.5:25580`). If the server has a token, set `RSPIGOT_TOKEN` to it.

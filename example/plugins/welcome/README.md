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

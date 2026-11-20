# factions

An example plugin built as a tree of modules: `Factions` (the root, `#[derive(Plugin)]`) holds `Items`, `Zones`, `FactionCommands` and `Mobs`, and `Mobs` is itself a node that holds `PassiveMobs` and `AggressiveMobs`. Each leaf is a struct with its own `#[listeners]` block. The server still sees one plugin named `factions`.

`Items` gives the claim wand (`/faction wand`), an item tagged `factions:kind = claim_wand` in its persistent data; right-clicking a block with it claims the 17 by 17 square of land around the block. `Zones` cancels the `BlockBreak` of any block inside a claim. The two share one `Zones` value: it is `Clone`, and the clones hold the same `Arc`, which `Factions::new` hands to `Items` and `FactionCommands`.

`FactionCommands` declares `/faction`, `/faction claims` and `/faction unclaim` (permission `factions.admin`). `Items` declares `/faction wand`: a command root declared by two modules is one command tree on the server, and the plugin refuses to start when two modules declare the same full path.

`Mobs` is `#[module(own)]`: it also has a `#[listeners]` block (an `on_enable`), which runs before the ones of its children. `PassiveMobs` and `AggressiveMobs` both hear `CreatureSpawn` and tag the entity with the metadata `factions:kind` (`passive` or `aggressive`), the first declared one first.

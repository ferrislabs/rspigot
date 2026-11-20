# marker

An example plugin that ties behaviour to tagged blocks and mobs with the metadata of `rspigot-plugin`.

`/mark <kind>` tags what the player looks at, within 6 blocks: the mob if there is one, the block otherwise. The tag is the key `marker:kind` with `<kind>` as its text value (`meta.set`), saved with the chunk or the mob, so it survives a restart.

Two handlers read the tag from the event itself, without asking the server (`event.block().meta`, `event.entity().meta`):

- a block tagged `unbreakable` cannot be broken: the `BlockBreak` is cancelled;
- a mob tagged `immortal` takes no damage: the `EntityDamage` is cancelled.

`/mark clear` removes the tag from what the player looks at.

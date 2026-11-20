# relic

An example plugin that gives a custom item built with the item metadata of `rspigot-plugin`, and acts on it when the player uses it.

`/relic` puts a golden sword in the inventory. It is built with `ItemStack::new(...)` and its builders: unbreakable (`unbreakable`), the unbreakable and attribute lines hidden from the tooltip (`hide_flags`), four more attack damage in the main hand (`attribute_modifier`), an epic rarity (`rarity`), and two tags in the item data (`with_meta`): `relic:kind` set to `fire` and `relic:charges` set to 3. They are stored in the item itself, so they follow it into chests, drops and saves.

A `PlayerInteract` handler reads the tag from the item of the event, without asking the server (`event.item()`, `item.meta_get`): a right click with a `fire` relic uses one charge (`meta_set` on the stack, written back with `inventory.set`), gives Speed II for ten seconds and cancels the click. Once the charges are gone the relic only says it is spent.

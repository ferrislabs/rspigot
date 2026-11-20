# zoo

An example plugin that uses the entity and block verbs of `rspigot-plugin`.

`/zoo` spawns a zombie three blocks ahead of the player and renames it (`entity.name`), puts an iron helmet, chestplate and sword on it (`entity.equipment`), and turns its AI off so it stays calm (`entity.ai.set(false)`). Two blocks further it places a chest and fills it (`block.inventory.add`), and a sign next to the chest that it writes (`block.sign.set_lines`).

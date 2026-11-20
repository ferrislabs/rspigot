# market

An example plugin that uses the command, permission, config and recipe helpers of `rspigot-plugin`.

`/shop` (alias `/s`, `/store`) has the sub-commands `list`, `buy <item> <amount>` and `reload`, each with its own permission (`buy` needs `shop.buy`, `reload` needs `shop.admin`) and its own place in the client's command tree. When `open-to-all` is true, the plugin grants `shop.buy` to every player who joins (`player.permissions.set`) and resends their command tree (`player.command.refresh`); the grant lives in the server's memory and goes away when the plugin disconnects.

Prices come from `plugins/market/config.toml`, written from the defaults on first start (`Config::save_default`) and read with `Config::load`; `/shop reload` calls `reload()`. Run the plugin from the server's working directory so the file lands in `plugins/market/`.

`on_enable` registers one custom shaped recipe, `market:voucher` (emeralds and any planks in the `ABA` / ` B ` shape make three named papers), with `server.recipes.add(...).await`. It is `auto_discover`, so a player finds it in the recipe book when an emerald reaches their inventory. The server does not save the recipe: it is registered again on every connection and removed when the plugin disconnects.

It uses no custom merchant: the server cannot run non-villager merchants yet.

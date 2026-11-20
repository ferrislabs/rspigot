# market

An example plugin that uses the command, permission and config helpers of `rspigot-plugin`.

`/shop` (alias `/s`, `/store`) has the sub-commands `list`, `buy <item> <amount>` and `reload`, each with its own permission (`buy` needs `shop.buy`, `reload` needs `shop.admin`) and its own place in the client's command tree. When `open-to-all` is true, the plugin grants `shop.buy` to every player who joins (`player.permissions.set`) and resends their command tree (`player.command.refresh`); the grant lives in the server's memory and goes away when the plugin disconnects.

Prices come from `plugins/market/config.toml`, written from the defaults on first start (`Config::save_default`) and read with `Config::load`; `/shop reload` calls `reload()`. Run the plugin from the server's working directory so the file lands in `plugins/market/`.

It uses no custom recipe or merchant: the server cannot register recipes or run non-villager merchants yet.

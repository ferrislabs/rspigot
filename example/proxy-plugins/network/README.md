# proxy-network

An example plugin for `rspigot-proxy`, written with `rspigot-proxy-plugin`.

- `/hub` (or `/lobby`) sends the player to the lobby server.
- `/server` lists the servers, clickable. `/server <name>` moves the player there. It needs `proxy.command.server`.
- `/find <player>` tells which server a player is on.
- Players named in `STAFF` get `proxy.command.server` when they log in.
- A player kicked from any server but the lobby is sent back to the lobby with the kick reason.

## Run

```sh
STAFF=Steve LOBBY=lobby cargo run -p proxy-network
```

`RSPIGOT_PROXY_URL` (default `http://127.0.0.1:25579`) and `RSPIGOT_PROXY_TOKEN` point it at the proxy's `[plugins]` host.

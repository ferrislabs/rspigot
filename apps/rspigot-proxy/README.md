# rspigot-proxy

A proxy for Minecraft Java 26.3 (protocol 777) in front of rspigot servers, in the spirit of Velocity.
Spec: ferrislabs/rspigot#453.

## Run

```sh
cargo run --release -p rspigot-proxy -- <directory>
```

The first start writes a default `proxy.toml` in the directory. Put the forwarding secret in `forwarding.secret`, the same value as `settings.velocity.secret` in the backend's `spigot.yml` (with `settings.velocity.enabled: true`). Run the backends in offline mode.

## proxy.toml

| Key | Meaning |
|---|---|
| `bind` | Address the proxy listens on. |
| `online_mode` | Verify players with Mojang and encrypt the client connection. |
| `motd`, `max_players`, `favicon` | What the server list shows. `favicon` is a 64x64 PNG, skipped when missing. |
| `forwarding_secret_file` | File holding the modern forwarding secret. |
| `compression_threshold` | Packet size above which the client connection is compressed. `-1` turns it off. |
| `compression_level` | zlib level, 0 to 9. |
| `login_timeout_secs` | Time a client has to finish the handshake and login. |
| `connect_timeout_secs` | Time the proxy waits for a backend before trying the next `try` server. Default 5. |
| `try` | Servers tried in order when a player joins. |
| `[servers]` | Backend names and their `host:port`. |
| `[plugins]` | Optional. Starts the gRPC host for proxy plugins (`rspigot.proxy.v1`). |
| `plugins.bind` | Address of the plugin host. Keep it on loopback or a private network. |
| `plugins.token_file` | File holding the token plugins must send in their hello. Empty means no token. |
| `plugins.decide_timeout_ms` | How long the proxy waits for a plugin to decide on a hook before it goes on with the default. |

A joining player goes to the first `try` server that answers. A backend refusal (ban, whitelist, full) is shown to the player and the next server is not tried. If the backend goes down, the player gets a disconnect message; moving them to another server comes with milestone M3. The same account cannot join twice.

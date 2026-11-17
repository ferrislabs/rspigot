# rspigot-plugin

The SDK for plugins that run beside the server and talk to it over gRPC. `example/plugins/welcome` is a complete plugin that uses it.

## The API in four rules

- **Domains.** A handle groups what it does in domains named by a noun: `player.inventory()`, `player.title()`, `player.message()`, `player.connection()`, `player.permissions()`, `player.location()`, `world.sound()`, `entity.attributes()`.
- **`nom.verbe`.** A call reads `domain.verb(..)`: `player.message().send("hi")`, `player.inventory().open(&menu)`, `player.connection().transfer(host, port)`.
- **`get_` reads.** Everything that asks the server for data is `get_` and is `async`: `server.get_state()`, `world.get_block(pos)`, `entity.get_state()`. A field or a plain getter reads what the handle already holds.
- **Actions return a result.** An action the server can refuse is `async` and returns `Result<_, Error>` (`create`, `delete`, `spawn`, `transfer`, `teleport_to_world`, `open`). A refusal arrives as `Error::Action` with an `ActionErrorKind`. Fire-and-forget actions (`message().send`, `attributes().set`) return nothing.

## Events

A handler is `async fn on_<event>(&self, event: &Event)`, or `&mut Event` to decide. The actor is read with `event.player()`: it is a `&Player` when the event always has one, and an `Option<&Player>` when the actor may be an entity (`EntityDamageByEntity`, `BlockCanBuild`). Every event is listed in [`EVENTS.md`](EVENTS.md).

## Commands

```rust
#[command("welcome", permission = "welcome.use")]
async fn welcome(&self, ctx: Context, #[suggest(Self::places)] place: String) {
    ctx.reply(place);
}
```

Arguments are typed (`Player`, `Material`, `String`, numbers, `bool`, `Option<T>`). `#[suggest(path)]` names an `async fn(&self, &Player, &str) -> Vec<String>` that completes the argument while the player types.

## Permissions

The server reads groups and rights from `permissions.yml` (an example is in `example/plugins/welcome/permissions.yml`). `player.permissions().has("welcome.hub")` reads them from the snapshot, without waiting. `-node` removes a right, `kit.*` grants everything below `kit`, and operators of level 4 have every right.

## Worlds and entities

`server.world().create(WorldOptions { .. })` makes a world (`Generator::Void`, `Flat`, `Vanilla` or `Datapack`) and `world.delete(files)` removes it. `player.location().teleport_to_world(&world)` moves a player to its spawn. `entity.as_zombie()` (and the other typed views) is `Some` only for the matching type.

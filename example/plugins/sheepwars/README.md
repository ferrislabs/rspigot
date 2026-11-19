# sheepwars

A SheepWars mini-game plugin for rspigot: two teams (red and blue) fight on the EpiCube "Steampunk" map. Every 20 seconds each team receives a coloured wool item, a "sheep". A right click throws it at the enemy and it triggers an effect when it lands. The last team with a standing player wins.

## How a round goes

1. A player who joins is added to the round, sent to the lobby and shown the sidebar and boss bar. Teams are balanced automatically.
2. With 2 players the countdown starts (15 s). Under 2 players it is cancelled.
3. At the start everyone is teleported to their team spawn at their team spawn on the map and their inventory is cleared.
4. Every sheep interval each standing player gets one sheep. The kind rotates over the 12 EpiCube sheep (`SHEEP_STRATEGY`, a placeholder: the real distribution is unknown).
5. A player has 3 lives. Dying (or falling under `void_fall_y`) costs one life and respawns at the team spawn. At 0 lives the player is a spectator.
6. A win or a draw shows a title. After 10 s the round resets, everybody goes back to the lobby and is re-joined.
7. A player who joins during a round is a spectator in the lobby until the next round.

## Commands

| Command | Who | Effect |
| --- | --- | --- |
| `/sheepwars join` | everyone | joins the round (or spectates if it has started) |
| `/sheepwars leave` | everyone | leaves the round and becomes a spectator in the lobby |
| `/sheepwars where` | everyone | diagnostic: world, position, blocks under the player and under the lobby as the server sees them |
| `/sheepwars forcestart` | `sheepwars.admin` | skips the countdown, works with a single player |
| `/sheepwars stop` | `sheepwars.admin` | cancels the round and resets |

## Map

Only the EpiCube "Steampunk" map is played, there is no random choice.

| Spot | Block | Standing point |
| --- | --- | --- |
| Red spawn | (-95, 132, -8) | (-94.5, 132, -7.5) |
| Blue spawn | (-47, 132, 26) | (-46.5, 132, 26.5) |
| Lobby (the map's own `level.dat` spawn) | (-47, 156, 24) | (-46.5, 156, 24.5) |

Each dispenser is 2 blocks in front of its team spawn, toward the other spawn: red (-93, 132, -7), blue (-49, 132, 25). The void line is `MAP_VOID_FALL_Y` = 0.0 (the map's content reaches y = 0, so only a fall out of the map kills).

The converted map must be in `example/server/maps/sheepwars/steampunk` (`level.dat` and `region/*.mca`, Anvil), produced by a local conversion of the EpiCube 1.8 map. The folder is third-party and git-ignored: it is never committed.

The world is named `sheepwars_steampunk`. At startup, before the world is created, the plugin copies the `region/*.mca` files of the map into `<server_dir>/<level-name>_rspigot_sheepwars_steampunk/dimensions/rspigot/sheepwars_steampunk/region/` (`level-name` is read from `<server_dir>/server.properties`, default `world`), i.e. by default `example/server/world_rspigot_sheepwars_steampunk/dimensions/rspigot/sheepwars_steampunk/region/`. It copies only when that region folder does not exist or holds no `.mca` file: it never overwrites or deletes anything in an existing world. No block is generated or placed: the Void-generator world loads the stored chunks.

If the world already exists with wrong or empty files: stop the server, delete the folder `example/server/world_rspigot_sheepwars_steampunk` and its entry in `example/server/created-worlds.json`, restart.

Fallback: when the map folder is missing (and the world is not installed yet), the plugin logs a warning and plays on the generated platform (`ArenaSettings::fallback_platform()`, world `sheepwars`): 25 wide (x) by 70 long (z), 3 thick of quartz, top at y=64, centred on 0,0, red spawn at z=-26.5, blue at z=26.5, lobby at the centre, void line y=20. Its blocks are placed when chunks load (`Arena::place_chunk` from the `ChunkLoad` listener). The old `sheepwars` world is left untouched.

## arena.json

Read from the working directory at startup. Absent: `ArenaSettings::default()` (the Steampunk map). Invalid: the error is logged and the default is used. Fields: `world_name`, `lobby`, `void_fall_y`, `map` (optional: `name`, the map folder name, `source_dir`, `server_dir`, both paths relative to the working directory), `red` / `blue` each with only `spawn` and `dispenser` (a block position), and `platform` (`center`, `width` along x, `depth` along z, `thickness`, `block`; optional, only used when `map` is absent). Unknown fields are rejected, so old files with a `lobby_platform` or a `platform` per team are invalid. With a `map`, only the map name is validated (a plain folder name): the spawns are not checked against any platform. Without a `map`, the file is rejected when the platform is empty or when the lobby, a spawn or a dispenser is not above the platform footprint.

```json
{
  "world_name": "sheepwars_steampunk",
  "lobby": {"x": -46.5, "y": 156.0, "z": 24.5},
  "void_fall_y": 0.0,
  "map": {"name": "steampunk", "source_dir": "../../server/maps/sheepwars/steampunk", "server_dir": "../../server"},
  "red": {"spawn": {"x": -94.5, "y": 132.0, "z": -7.5}, "dispenser": {"x": -93, "y": 132, "z": -7}},
  "blue": {"spawn": {"x": -46.5, "y": 132.0, "z": 26.5}, "dispenser": {"x": -49, "y": 132, "z": 25}}
}
```

## Arena building

`Arena::create` installs the map files (see Map), then creates the world and computes the layout. For a map arena nothing is placed: `Layout::blocks()` and `chunks()` are empty and `Game::on_chunk_load` does nothing. If the world already exists on the server it is reused.

## Numbers you can tune

| What | Where |
| --- | --- |
| Min/max players, countdown, sheep interval, lives | `RoundRules::default()` in `src/domain/rules.rs` |
| Delay before the reset after a win | `RESET_DELAY_TICKS` in `src/game/session.rs` |
| Which sheep is given each time | `SHEEP_STRATEGY` in `src/game/session.rs` |
| Min players when forcing a start | `FORCED_MIN_PLAYERS` in `src/game/session.rs` |
| Concurrent flights, max flight time, landing grace, eye height, rider offset | `src/game/flight.rs` |
| Rider damage grace | `RIDER_GRACE_TICKS` in `src/game/session.rs` |
| Fuses, pulse period, incendiary swell, seeker run | named `const`s in `src/sheep/catalog.rs` |
| HUD and action bar refresh periods | `src/game/engine.rs` |
| Throw power (default, Healing is shorter) | `DEFAULT_THROW_POWER` in `src/sheep/launch.rs` (2.7: the smallest round value that crosses the 58.8 blocks between the Steampunk spawns), `HEALING_THROW_POWER` in `src/sheep/catalog.rs` |
| Debris (crater, fraction, caps, launch speeds, direction jitter, spawn lift) | `DEBRIS_*` in `src/sheep/debris.rs` |
| Sheep effects (radius, power, durations, seeker, meteors, warp push) | named `const`s in `src/sheep/catalog.rs` (all tunable guesses) |
| Spawns, lobby, void level, map | `arena.json` or the `MAP_*` consts in `src/arena/settings.rs` (`FALLBACK_*`/`DEFAULT_*` for the platform) |
| Tick length of the game loop | `TICK` in `src/main.rs` |

## The sheep

A thrown sheep is a small state machine (`Flight::step` in `src/game/flight.rs`, pure and unit tested, `Game` only executes its `Step`s). It flies until it is on the ground (after `LANDING_GRACE_TICKS`, or after `MAX_FLIGHT_TICKS`). Landing never detonates by itself: the sheep then waits for its fuse, in ticks, pulsing a smoke particle and a creeper hiss every `PULSE_INTERVAL_TICKS` (10). At the end of the fuse the plan of the kind runs and the sheep is removed.

| Kind | Name | Wool | After landing | Fuse (ticks) | Plan at the end | Constants |
| --- | --- | --- | --- | --- | --- | --- |
| `Explosive` | Mouton explosif | red | waits and pulses | 40 | plain TNT-like block-breaking explosion (power 4.0) | `EXPLOSIVE_POWER`, `EXPLOSIVE_BREAKS_BLOCKS` |
| `Lightning` | Invocateur de foudre | yellow | waits and pulses | 40 | lightning bolt at the sheep, thunder (untested on a real server) | `DEFAULT_FUSE_TICKS` |
| `Incendiary` | Mouton incendiaire | orange | swells: `Scale` attribute from 1.0 to 2.5, updated every 2 ticks | 60 | large block-breaking explosion plus flames | `INCENDIARY_POWER`, `INCENDIARY_BREAKS_BLOCKS`, `INCENDIARY_FUSE_TICKS`, `INCENDIARY_SCALE_START`, `INCENDIARY_SCALE_END`, `INCENDIARY_SCALE_INTERVAL_TICKS` |
| `Boarding` | Mouton d'abordage | white | no fuse: the rider is released the tick it lands | 0 | particles and a teleport sound only, no explosion | `BOARDING_RIDER_OFFSET`, `RIDER_GRACE_TICKS` |
| `Healing` | Mouton soigneur | pink | waits and pulses | 40 | regeneration on the thrower team nearby | `HEALING_*` |
| `Seeker` | Mouton chercheur | green | runs: every tick it finds the nearest enemy within `SEEKER_RANGE` and gets a horizontal velocity of `SEEKER_RUN_SPEED` toward it, with a hop of `SEEKER_HOP` while on the ground; with no enemy in range it stays still | max run 200 | explosion; detonates earlier within `SEEKER_CONTACT_RADIUS` (1.6) of an enemy. In flight it still homes at `SEEKER_SPEED` | `SEEKER_POWER`, `SEEKER_SPEED`, `SEEKER_RANGE`, `SEEKER_RUN_SPEED`, `SEEKER_HOP`, `SEEKER_CONTACT_RADIUS`, `SEEKER_RUN_TICKS` |
| `Splitting` | Mouton à fragmentation | gray | waits and pulses | 40 | small block-breaking explosion, then `SPLIT_COUNT` child seekers with a short fuse (max run 20) | `SPLIT_*` (`SPLIT_PARENT_BREAKS_BLOCKS`), `SPLIT_CHILD_FUSE_TICKS` |
| `Darkness` | Mouton ténébreux | black | waits and pulses | 40 | blindness on enemies nearby | `DARKNESS_*` |
| `Frozen` | Mouton glacé | light blue | waits and pulses | 40 | ice disc, slowness on enemies nearby | `FROZEN_*` |
| `Quake` | Mouton tremblement de terre | brown | waits and pulses | 40 | upward velocity on enemies nearby | `QUAKE_RADIUS`, `QUAKE_LIFT` |
| `Warp` | Mouton à distorsion | magenta | waits and pulses | 40 | pushes enemies away and up, small block-breaking explosion | `WARP_*` |
| `Intergalactic` | Mouton intergalactique | blue | waits and pulses | 40 | `INTERGALACTIC_METEORS` lethal explosions around the sheep, a lightning bolt, a distinct sound; a chat message announces the throw | `INTERGALACTIC_*` |

Every fuse is `fuse_ticks(kind)` in `src/sheep/catalog.rs` (default `DEFAULT_FUSE_TICKS` = 40). The plan itself is still instantaneous: the meteors of the Intergalactic sheep land all at once and the sky does not darken. The launch announcement is `announces_launch(kind)` in the catalogue, broadcast by `launch_announcement` in `src/game/session.rs`.

Source: blog post by Zephni, 7 Sept 2014 (https://zephnisurminecraft.blogspot.com/2014/09/presentation.html). It gives no number: every number is a guess. "Moutons Blindés" is a kit, not a sheep.

### Colours (unconfirmed)

The item wool and the flying sheep share one source, `wool_color(kind)` in `src/sheep/catalog.rs` (`wool_material` for the item, `dye_color` plus `Entity::as_sheep().set_color` for the entity, the children of a Splitting sheep get their own kind colour). The table comes from one unidentified old blog post found by a web search: treat every colour as unconfirmed. Warp has no source at all (magenta is a guess). Intergalactic is "dark blue" there, rendered as blue (no dark blue dye).

| Kind | Colour | Source |
| --- | --- | --- |
| Explosive | red | old blog post |
| Incendiary | orange | old blog post |
| Lightning | yellow | old blog post |
| Intergalactic | blue (dark blue) | old blog post |
| Seeker | green | old blog post |
| Darkness | black | old blog post |
| Splitting | gray | old blog post |
| Healing | pink | old blog post |
| Frozen | light blue | old blog post |
| Quake | brown | old blog post |
| Boarding | white | old blog post |
| Warp | magenta | guess |

The Explosive sheep ("Mouton explosif", "explosion semblable à de la TNT", minecraft-france.fr/epicube section 4.1) was lost earlier and is back.

### Debris

A breaking explosion (Explosive, Incendiary, Splitting parent, Warp, Intergalactic) throws part of its crater into the air as real falling block entities (`World::spawn_falling_block`). The pure planner (`plan_debris` in `src/sheep/debris.rs`, deterministic xorshift seeded from the impact cell) picks cells of the crater (disc of radius `power * DEBRIS_CRATER_PER_POWER`, `DEBRIS_DEPTH` layers under the floor top) with probability `DEBRIS_FRACTION`, at most `DEBRIS_MAX_PER_EXPLOSION` (12) per explosion and `DEBRIS_MAX_PER_PLAN` (32) per sheep, each with a launch velocity: a horizontal direction pointing away from the impact centre (within `DEBRIS_DIRECTION_JITTER` radians), a horizontal speed in `DEBRIS_HORIZONTAL_SPEED_MIN..MAX` and a vertical speed in `DEBRIS_VERTICAL_SPEED_MIN..MAX` blocks per tick, all below the server clamp of 3.9 per axis. The `Scatter` actions run before the explosion, concurrently: the block type is read, the origin is set to air, a block particle is emitted (`DEBRIS_PARTICLES`), and a falling block of that type is spawned at the block centre raised by `DEBRIS_SPAWN_LIFT` with the velocity. The server turns it back into a block when it lands; if the landing cell is occupied the block simply vanishes, and nothing is dropped. The blocks are real entities: they can land on players without hurting them. Failures to read or spawn are logged at debug and ignored.

## Root cause of the lost players

- Darkness blindness and Healing never reached anyone, the Seeker never ran: `Entities::get_entities` (`crates/rspigot-plugin/src/entity.rs:173`) is answered by `listed_entities` in `crates/rspigot-core/src/plugins/entity.rs:108-123` which walks `entities.store` only, and players are not in that store, so `world.entities.of(EntityType::Player)` was always empty (old `members`, `src/sheep/apply.rs`). Entity orders on a player also fail ("the entity is gone"), which broke the Boarding carry too. Fix in the plugin: `Game::roster` (`src/game/engine.rs`) reads each standing player with `Player::get_state` (position, dead), the pure `sheep::select` module picks targets by team and distance, effects go through `player.effects.add`, pushes through `player.movement.set_velocity`, the carry through `player.location.teleport`.

## Boarding: how the thrower rides

The thrower is carried, not mounted. Each tick while the Boarding sheep flies, the plugin teleports the thrower to the sheep position plus `BOARDING_RIDER_OFFSET` (1.0 block up) through `player.location.teleport`. Reason: the preferred mechanism, `/ride <name> mount <sheep>` through `player.command.perform`, is not usable. `PlayerOrder::PerformCommand` runs the command with the player's own rights (checked in `crates/rspigot-core/tests/plugin_player.rs`, `perform_command_runs_with_the_players_rights`) and `ride` needs permission level 2 (`crates/rspigot-core/src/commands/permission.rs`), so it fails for a non-operator player. The SDK has no passenger or mount order.

When the sheep lands the rider is released where it stands and the sheep is removed, without an explosion. The thrower is shielded (`Session::shield`) at every carry tick and at the release: fall, suffocation, fly-into-wall and contact damage are cancelled for `RIDER_GRACE_TICKS` (20) ticks after the last shield, then the shield expires. Other damage is not affected.

## Catching a sheep

Hit a thrown sheep (in the air, or landed with its fuse still running) until it dies, to stop it before it explodes or applies its effect. A hit by a player standing in the round is not cancelled: the vanilla damage and knockback apply, so the sheep (8 health, like any sheep) takes as many hits as the weapon needs, exactly as a vanilla sheep would; the hit count depends on the weapon. Each such hit records the last hitter and the tick on the flight (`Flight::record_hit`). Hits by spectators, eliminated players and players outside the round are cancelled, and so are hits on a detonating flight.

When the sheep dies (`EntityDeath`) the handler sets the drops to exactly one wool of its kind, so the server drops it where the sheep died and anyone can pick it up and throw it again; the dropped experience is 0 and no vanilla wool drops. The wool drops whoever or whatever killed it (void, fire, explosion included). It does not drop for a Splitting fragment, nor for a sheep that dies while already detonating (`Flight::loot`, pure, unit tested). In every case the flight is removed so no effect, fuse, debris or explosion ever runs, and a sound and a cloud particle play (`CAPTURE_SOUND`, `CAPTURE_PARTICLE`, `CAPTURE_PARTICLES`). A sheep that no longer exists when polled is dropped silently. The flight registry (`Registry::take`) has a single owner per flight, so the death handler and the per-tick poller can never both drop the wool or both detonate. A Boarding sheep that dies releases its rider, who gets the fall shield.

## Still unconfirmed

- The fuse of every sheep (40 ticks by default) is a guess, as are the incendiary swell (60 ticks, scale 1.0 to 2.5), the seeker run (speed 0.25, hop 0.3, contact 1.6, max 200 ticks) and the children fuse (20 ticks).
- Boarding carries the thrower by teleport each tick, it is not a real mount (see above).
- The thrower is not protected from explosions: explosions are not team filtered.
- Healing: the post says "tout joueur à proximité", it only heals the thrower team here.
- Splitting children use the Seeker kind (so they also home); the real fragment behaviour is unknown.
- Warp: "quelques blocs" are broken by the small explosion, not moved.
- Quake: "rebondir" is a single upward push, not repeated bounces.
- Distribution of sheep over time is unknown (rotation placeholder).

## Rules to confirm (from memory, not verified)

- Block break and place are cancelled for everyone, admins included, so the arena stays intact.
- Hunger is off, damage is cancelled outside the playing phase, friendly fire is off.
- 3 lives, 20 s sheep interval, 15 s countdown, 10 s reset delay, 2 players minimum.
- The sheep kind rotates in catalogue order, the same kind for both teams.
- A death keeps the inventory, so unused sheep are kept.
- Players leaving mid-round lose (counted as eliminated).
- Falling in the void outside a fight sends the player back to the lobby.

## Layout

- `src/domain`: rules and state of a round, no SDK.
- `src/arena`: arena settings, map installer (`install.rs`), layout (blocks per chunk, fallback platform only) and world creation.
- `src/sheep`: sheep catalogue, impact plans and their application.
- `src/hud`: sidebar, boss bar and messages.
- `src/game`: `Session` (pure reducer from player actions to `Intent`s, unit tested) and `Game` (executes intents through the SDK, runs flights and the HUD).
- `src/main.rs`: thin listeners and the command.

# sheepwars

A SheepWars mini-game plugin for rspigot: two teams (red and blue) fight on one single floating platform. Every 20 seconds each team receives a coloured wool item, a "sheep". A right click throws it at the enemy and it triggers an effect when it lands. The last team with a standing player wins.

## How a round goes

1. A player who joins is added to the round, sent to the lobby and shown the sidebar and boss bar. Teams are balanced automatically.
2. With 2 players the countdown starts (15 s). Under 2 players it is cancelled.
3. At the start everyone is teleported to their team spawn at either end of the platform and their inventory is cleared.
4. Every sheep interval each standing player gets one sheep. The kind rotates over the 11 EpiCube sheep (`SHEEP_STRATEGY`, a placeholder: the real distribution is unknown).
5. A player has 3 lives. Dying (or falling under `void_fall_y`) costs one life and respawns at the team spawn. At 0 lives the player is a spectator.
6. A win or a draw shows a title. After 10 s the round resets, everybody goes back to the lobby and is re-joined.
7. A player who joins during a round is a spectator in the lobby until the next round.

## Commands

| Command | Who | Effect |
| --- | --- | --- |
| `/sheepwars join` | everyone | joins the round (or spectates if it has started) |
| `/sheepwars leave` | everyone | leaves the round and becomes a spectator in the lobby |
| `/sheepwars forcestart` | `sheepwars.admin` | skips the countdown, works with a single player |
| `/sheepwars stop` | `sheepwars.admin` | cancels the round and resets |

## arena.json

Read from the working directory at startup. Absent: `ArenaSettings::default()`. Invalid: the error is logged and the default is used. Fields: `world_name`, `lobby`, `void_fall_y`, one shared `platform` (`center`, `width` along x, `depth` along z, `thickness`, `block`), and `red` / `blue` each with only `spawn` and `dispenser`. The lobby is a point on the same platform (no separate lobby platform). Files are rejected (default used) when the platform is empty or when the lobby, a spawn or a dispenser is not above the platform footprint. The schema changed: old files with a `platform` per team or a `lobby_platform` are now invalid. Default: 25 wide (x) by 70 long (z), 3 thick, top at y=64, centred on 0,0; red spawn at z=-26.5, blue at z=26.5, lobby at the centre.

## Arena building

`Arena::create` only creates the world and computes the layout. The platform blocks are placed when chunks load: the `ChunkLoad` listener calls `Arena::place_chunk` for the chunks of the arena world, which places `Layout::blocks_in_chunk`. A chunk that unloads and loads again is rebuilt. The first player to join may therefore see the platform appear a tick late.

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
| Throw power (default, Healing is shorter) | `DEFAULT_THROW_POWER` in `src/sheep/launch.rs`, `HEALING_THROW_POWER` in `src/sheep/catalog.rs` |
| Sheep effects (radius, power, durations, seeker, meteors, warp push) | named `const`s in `src/sheep/catalog.rs` (all tunable guesses) |
| Arena size, spawns, lobby, void level | `arena.json` or the `DEFAULT_*` consts in `src/arena/settings.rs` |
| Tick length of the game loop | `TICK` in `src/main.rs` |

## The sheep

A thrown sheep is a small state machine (`Flight::step` in `src/game/flight.rs`, pure and unit tested, `Game` only executes its `Step`s). It flies until it is on the ground (after `LANDING_GRACE_TICKS`, or after `MAX_FLIGHT_TICKS`). Landing never detonates by itself: the sheep then waits for its fuse, in ticks, pulsing a smoke particle and a creeper hiss every `PULSE_INTERVAL_TICKS` (10). At the end of the fuse the plan of the kind runs and the sheep is removed.

| Kind | Name | Wool | After landing | Fuse (ticks) | Plan at the end | Constants |
| --- | --- | --- | --- | --- | --- | --- |
| `Lightning` | Invocateur de foudre | yellow | waits and pulses | 40 | lightning bolt at the sheep, thunder (untested on a real server) | `DEFAULT_FUSE_TICKS` |
| `Incendiary` | Mouton incendiaire | orange | swells: `Scale` attribute from 1.0 to 2.5, updated every 2 ticks | 60 | large block-breaking explosion plus flames | `INCENDIARY_POWER`, `INCENDIARY_BREAKS_BLOCKS`, `INCENDIARY_FUSE_TICKS`, `INCENDIARY_SCALE_START`, `INCENDIARY_SCALE_END`, `INCENDIARY_SCALE_INTERVAL_TICKS` |
| `Boarding` | Mouton d'abordage | purple | no fuse: the rider is released the tick it lands | 0 | particles and a teleport sound only, no explosion | `BOARDING_RIDER_OFFSET`, `RIDER_GRACE_TICKS` |
| `Healing` | Mouton soigneur | pink | waits and pulses | 40 | regeneration on the thrower team nearby | `HEALING_*` |
| `Seeker` | Mouton chercheur | green | runs: every tick it finds the nearest enemy within `SEEKER_RANGE` and gets a horizontal velocity of `SEEKER_RUN_SPEED` toward it, with a hop of `SEEKER_HOP` while on the ground; with no enemy in range it stays still | max run 200 | explosion; detonates earlier within `SEEKER_CONTACT_RADIUS` (1.6) of an enemy. In flight it still homes at `SEEKER_SPEED` | `SEEKER_POWER`, `SEEKER_SPEED`, `SEEKER_RANGE`, `SEEKER_RUN_SPEED`, `SEEKER_HOP`, `SEEKER_CONTACT_RADIUS`, `SEEKER_RUN_TICKS` |
| `Splitting` | Mouton à fragmentation | gray | waits and pulses | 40 | small explosion, then `SPLIT_COUNT` child seekers with a short fuse (max run 20) | `SPLIT_*`, `SPLIT_CHILD_FUSE_TICKS` |
| `Darkness` | Mouton ténébreux | black | waits and pulses | 40 | blindness on enemies nearby | `DARKNESS_*` |
| `Frozen` | Mouton glacé | light blue | waits and pulses | 40 | ice disc, slowness on enemies nearby | `FROZEN_*` |
| `Quake` | Mouton tremblement de terre | brown | waits and pulses | 40 | upward velocity on enemies nearby | `QUAKE_RADIUS`, `QUAKE_LIFT` |
| `Warp` | Mouton à distorsion | magenta | waits and pulses | 40 | pushes enemies away and up, small block-breaking explosion | `WARP_*` |
| `Intergalactic` | Mouton intergalactique | blue | waits and pulses | 40 | `INTERGALACTIC_METEORS` lethal explosions around the sheep, a lightning bolt, a distinct sound; a chat message announces the throw | `INTERGALACTIC_*` |

Every fuse is `fuse_ticks(kind)` in `src/sheep/catalog.rs` (default `DEFAULT_FUSE_TICKS` = 40). The plan itself is still instantaneous: the meteors of the Intergalactic sheep land all at once and the sky does not darken. The launch announcement is `announces_launch(kind)` in the catalogue, broadcast by `launch_announcement` in `src/game/session.rs`.

Source: blog post by Zephni, 7 Sept 2014 (https://zephnisurminecraft.blogspot.com/2014/09/presentation.html). It gives no number: every number is a guess. "Moutons Blindés" is a kit, not a sheep.

## Boarding: how the thrower rides

The thrower is carried, not mounted. Each tick while the Boarding sheep flies, the plugin teleports the thrower to the sheep position plus `BOARDING_RIDER_OFFSET` (1.0 block up) through `Entity::teleport` (one extra player lookup per tick). Reason: the preferred mechanism, `/ride <name> mount <sheep>` through `player.command.perform`, is not usable. `PlayerOrder::PerformCommand` runs the command with the player's own rights (checked in `crates/rspigot-core/tests/plugin_player.rs`, `perform_command_runs_with_the_players_rights`) and `ride` needs permission level 2 (`crates/rspigot-core/src/commands/permission.rs`), so it fails for a non-operator player. The SDK has no passenger or mount order.

When the sheep lands the rider is released where it stands and the sheep is removed, without an explosion. The thrower is shielded (`Session::shield`) at every carry tick and at the release: fall, suffocation, fly-into-wall and contact damage are cancelled for `RIDER_GRACE_TICKS` (20) ticks after the last shield, then the shield expires. Other damage is not affected.

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
- `src/arena`: arena settings, layout (blocks per chunk) and world creation with per-chunk placement.
- `src/sheep`: sheep catalogue, impact plans and their application.
- `src/hud`: sidebar, boss bar and messages.
- `src/game`: `Session` (pure reducer from player actions to `Intent`s, unit tested) and `Game` (executes intents through the SDK, runs flights and the HUD).
- `src/main.rs`: thin listeners and the command.

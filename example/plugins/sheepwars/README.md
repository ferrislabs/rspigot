# sheepwars

A SheepWars mini-game plugin for rspigot: two teams (red and blue) fight on two floating platforms. Every 20 seconds each team receives a coloured wool item, a "sheep". A right click throws it at the enemy and it triggers an effect when it lands. The last team with a standing player wins.

## How a round goes

1. A player who joins is added to the round, sent to the lobby and shown the sidebar and boss bar. Teams are balanced automatically.
2. With 2 players the countdown starts (15 s). Under 2 players it is cancelled.
3. At the start everyone is teleported to their team platform and their inventory is cleared.
4. Every sheep interval each standing player gets one sheep. The kind rotates over the 11 EpiCube sheep (`SHEEP_STRATEGY`, a placeholder: the real distribution is unknown).
5. A player has 3 lives. Dying (or falling under `void_fall_y`) costs one life and respawns on the team platform. At 0 lives the player is a spectator.
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

Read from the working directory at startup. Absent: `ArenaSettings::default()`. Invalid: the error is logged and the default is used. Fields: `world_name`, `lobby`, `void_fall_y`, and `red` / `blue` each with `platform` (`center`, `width`, `depth`, `thickness`, `block`), `spawn` and `dispenser`. See `src/arena/settings.rs`.

## Numbers you can tune

| What | Where |
| --- | --- |
| Min/max players, countdown, sheep interval, lives | `RoundRules::default()` in `src/domain/rules.rs` |
| Delay before the reset after a win | `RESET_DELAY_TICKS` in `src/game/session.rs` |
| Which sheep is given each time | `SHEEP_STRATEGY` in `src/game/session.rs` |
| Min players when forcing a start | `FORCED_MIN_PLAYERS` in `src/game/session.rs` |
| Concurrent flights, max flight time, landing grace, eye height | `src/game/flight.rs` |
| HUD and action bar refresh periods | `src/game/engine.rs` |
| Throw power (default, Healing is shorter) | `DEFAULT_THROW_POWER` in `src/sheep/launch.rs`, `HEALING_THROW_POWER` in `src/sheep/catalog.rs` |
| Sheep effects (radius, power, durations, seeker, meteors, warp push) | named `const`s in `src/sheep/catalog.rs` (all tunable guesses) |
| Arena size, lobby, void level | `arena.json` or `src/arena/settings.rs` |
| Tick length of the game loop | `TICK` in `src/main.rs` |

## The sheep

Source: blog post by Zephni, 7 Sept 2014 (https://zephnisurminecraft.blogspot.com/2014/09/presentation.html). It describes effects but gives no number (radius, duration, cooldown, frequency): every number is a guess in `src/sheep/catalog.rs`. "Moutons Blindés" is a kit, not a sheep.

| Kind | Name | Wool | Plan | Constants |
| --- | --- | --- | --- | --- |
| `Lightning` | Invocateur de foudre | yellow | lightning bolt entity at the impact, thunder (untested on a real server) | none |
| `Incendiary` | Mouton incendiaire | orange | large block-breaking explosion plus flame particles, no fire blocks | `INCENDIARY_POWER`, `INCENDIARY_BREAKS_BLOCKS` |
| `Boarding` | Mouton d'abordage | purple | teleports the thrower to the impact | none |
| `Healing` | Mouton soigneur | pink | regeneration on the thrower team near the impact, short throw | `HEALING_RADIUS`, `HEALING_REGENERATION_*`, `HEALING_THROW_POWER` |
| `Seeker` | Mouton chercheur | green | homes on the nearest enemy in flight, explodes on landing | `SEEKER_POWER`, `SEEKER_SPEED`, `SEEKER_RANGE` |
| `Splitting` | Mouton à fragmentation | gray | small explosion, then `SPLIT_COUNT` child sheep (kind `SPLIT_CHILD`) that land and explode | `SPLIT_*` |
| `Darkness` | Mouton ténébreux | black | blindness on enemies nearby | `DARKNESS_*` |
| `Frozen` | Mouton glacé | light blue | ice disc, slowness on enemies nearby | `FROZEN_*` |
| `Quake` | Mouton tremblement de terre | brown | upward velocity on enemies nearby | `QUAKE_RADIUS`, `QUAKE_LIFT` |
| `Warp` | Mouton à distorsion | magenta | pushes enemies away and up, small block-breaking explosion | `WARP_*` |
| `Intergalactic` | Mouton intergalactique | blue | `INTERGALACTIC_METEORS` lethal explosions spread around the impact, a lightning bolt, a distinct sound; a chat message announces the throw | `INTERGALACTIC_*` |

The plan of a landing is instantaneous: the meteors of the Intergalactic sheep land all at once, not delayed, and the sky does not darken. The launch announcement is `announces_launch(kind)` in the catalogue, broadcast by `launch_announcement` in `src/game/session.rs`.

## Still unconfirmed

- The Seeker should explode on contact with an enemy: only homing plus explosion on landing is implemented.
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
- `src/arena`: arena settings, layout and world building.
- `src/sheep`: sheep catalogue, impact plans and their application.
- `src/hud`: sidebar, boss bar and messages.
- `src/game`: `Session` (pure reducer from player actions to `Intent`s, unit tested) and `Game` (executes intents through the SDK, runs flights and the HUD).
- `src/main.rs`: thin listeners and the command.

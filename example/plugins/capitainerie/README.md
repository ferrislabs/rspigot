# capitainerie

A bridge between a Rspigot server and [Capitainerie](https://github.com/ferrislabs/capitainerie), the agent platform. Each agent of the team shows up as an NPC that reports what it is doing; a player clicks it to give it a task and talks to the Captain from the chat.

## What a player sees

- **One NPC per agent** (`GET /api/v1/team/agents`, archived agents left out), standing in a line along `z` from the world spawn, looking at players within 8 blocks. Its label shows the agent name and its state: `Disponible`, `En pause`, or `En cours · <step>` with a five-segment bar, `done/total` and the task title (`GET /api/v1/runs/active`). The NPC crouches while a run is running.
- **Right click on an agent** opens a native dialog with a title and a description. "Lancer" creates a card assigned to the agent (`POST /api/v1/cards`) and starts its run (`POST /api/v1/cards/{id}/agent-runs`); the player is told the agent took the task, or why it could not (paused, no connector, no model provider, a run already active...). A blank title is refused.
- **Right click on the Captain** starts a conversation (`/api/v1/conversations`, streamed over SSE). Everything the player types goes to the Captain instead of the public chat, and the answer comes back line by line as `Capitaine: ...`. A card the Captain proposes is announced. `quitter`, `exit`, `quit`, or another right click on the Captain, ends it.
- When Capitainerie cannot be reached, the Captain's label says so and goes back to normal when it answers again. Agent NPCs keep their last label.

## Configuration

The plugin logs in as one Capitainerie user with a password grant, so every action is done and audited under that account.

| Variable | Meaning |
|---|---|
| `CAPITAINERIE_URL` | Base URL of the API, for example `http://localhost:4100` (required) |
| `CAPITAINERIE_TOKEN_URL` | The FerrisKey token endpoint of the organization's realm (required) |
| `CAPITAINERIE_CLIENT_ID` | A FerrisKey client for this bridge (required) |
| `CAPITAINERIE_USERNAME`, `CAPITAINERIE_PASSWORD` | The bridge user (required) |
| `CAPITAINERIE_CLIENT_SECRET` | If the client is confidential |
| `CAPITAINERIE_AUDIENCE` | Sent as `audience` when Capitainerie sets `AUTH_AUDIENCE` |
| `CAPITAINERIE_POLL_SECONDS` | Refresh interval, 1 to 60, default 2 |

On the Capitainerie side, the client must be listed in `AUTH_ACCEPTED_CLIENTS`, allow direct access grants, and its tokens must carry the API audience. The user has to be a human member of the organization: a service account is not enough, since only humans can create cards and conversations. `RSPIGOT_URL` and `RSPIGOT_TOKEN` work as for any plugin.

```
cargo run --release -p rspigot
CAPITAINERIE_URL=... CAPITAINERIE_TOKEN_URL=... cargo run -p capitainerie
```

## Limits

- One bridge user for every player: the audit trail does not tell players apart.
- It polls. The realtime WebSocket of Capitainerie only sends ids, so each event would trigger the same fetch anyway; the interval is the latency of a progress change.
- The conversation is always with the Captain: the API does not tie a conversation to an agent.
- Agents have no skin; the server picks the default one for each NPC.
- NPCs outlive a plugin restart and are adopted again by key (`agent:<id>`, `captain`); those of removed agents are deleted at the next refresh.

The end-to-end test (`tests/end_to_end.rs`) runs the real plugin binary against the real server core and a fake Capitainerie.

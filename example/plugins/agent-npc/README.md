# agent-npc

An example plugin that shows an agent as an NPC, the way an agent platform would.

When it connects, it creates a one-row menu titled "Task" (a "Start" book and a "Cancel" barrier) and a player-model NPC named `Scout` three blocks east of the world spawn, with an iron helmet, a gaze that follows players within 8 blocks, and the label "Idle".

Right clicking the NPC opens the menu (`on_player_interact_at_entity`, main hand only). "Start" closes it, crouches the NPC and walks the label through five steps, two seconds apart (`Working 20%` to `Working 100%`, then `Done`), swinging its arm at each step. "Cancel" stops the task, stands the NPC up and sets the label back to "Idle". A task started while another runs replaces it. The NPC is removed when the plugin stops.

The end-to-end test (`tests/end_to_end.rs`) runs the real plugin binary against the real server core: the NPC appears for a player, a click opens the menu, "Start" updates the label and "Cancel" restores it.

Start the server, then the plugin, as in the [welcome example](../welcome/README.md):

```
cargo run --release -p rspigot
cargo run -p agent-npc
```

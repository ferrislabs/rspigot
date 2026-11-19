use std::collections::{HashMap, HashSet};
use std::sync::Mutex;
use std::time::Duration;

use rspigot_plugin::{Npc, NpcPose, NpcSpec, Player, Position, Server, Uuid};

use crate::api::{Api, ApiError};
use crate::config::Config;
use crate::model::{ActiveRun, TeamAgent};
use crate::present::{CAPTAIN_NAME, agent_label, captain_label, click_slug, is_working, npc_name};

pub const AGENT_PREFIX: &str = "agent:";
pub const CAPTAIN_KEY: &str = "captain";
pub const RUN_PREFIX: &str = "capitainerie:run/";
pub const CANCEL_ID: &str = "capitainerie:cancel";
const SLOT_SPACING: f64 = 2.0;
const GAZE_RANGE: f64 = 8.0;
const QUIT_WORDS: [&str; 4] = ["quitter", "quit", "exit", "/quitter"];

#[derive(Debug, Clone, PartialEq)]
pub enum Clicked {
    Agent(TeamAgent),
    Captain,
}

struct Tracked {
    npc: Npc,
    agent: TeamAgent,
    label: String,
    working: bool,
    slot: usize,
}

struct Captain {
    npc: Npc,
    label: String,
}

#[derive(Default)]
struct Chat {
    conversation: Option<String>,
    busy: bool,
}

#[derive(Default)]
struct State {
    origin: Option<Position>,
    agents: HashMap<String, Tracked>,
    captain: Option<Captain>,
    chats: HashMap<Uuid, Chat>,
    online: Option<bool>,
}

pub struct Bridge {
    api: Api,
    poll: Duration,
    state: Mutex<State>,
}

pub enum Turn {
    Busy,
    Ready(Option<String>),
}

impl Bridge {
    pub fn new(config: Config) -> Self {
        Self {
            poll: config.poll,
            api: Api::new(config),
            state: Mutex::new(State::default()),
        }
    }

    fn state(&self) -> std::sync::MutexGuard<'_, State> {
        self.state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    pub async fn run(&self, server: Server) {
        let mut tick = tokio::time::interval(self.poll);
        tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            tick.tick().await;
            self.sync(&server).await;
        }
    }

    pub async fn sync(&self, server: &Server) {
        let (agents, runs) = tokio::join!(self.api.agents(), self.api.active_runs());
        match (agents, runs) {
            (Ok(agents), Ok(runs)) => {
                self.reconcile(server, agents, runs).await;
            }
            (agents, runs) => {
                let error = agents.err().or(runs.err());
                if let Some(error) = error {
                    tracing::warn!("Capitainerie is not reachable: {error}");
                }
                self.mark_offline();
            }
        }
    }

    fn mark_offline(&self) {
        let mut state = self.state();
        state.online = Some(false);
        let label = captain_label(false);
        if let Some(captain) = state.captain.as_mut()
            && captain.label != label
        {
            captain.npc.set_label(label.clone());
            captain.label = label;
        }
    }

    async fn origin(&self, server: &Server) -> Option<Position> {
        if let Some(origin) = self.state().origin {
            return Some(origin);
        }
        let spawn = match server.world.get_state().await {
            Ok(state) => state.spawn,
            Err(error) => {
                tracing::warn!("Could not read the world spawn: {error}");
                return None;
            }
        };
        let origin = Position {
            x: f64::from(spawn.x) + 3.5,
            y: f64::from(spawn.y),
            z: f64::from(spawn.z) + 0.5,
        };
        self.state().origin = Some(origin);
        Some(origin)
    }

    async fn reconcile(&self, server: &Server, agents: Vec<TeamAgent>, runs: Vec<ActiveRun>) {
        let Some(origin) = self.origin(server).await else {
            return;
        };
        let existing: HashMap<String, Npc> = server
            .world
            .npcs
            .get_npcs()
            .await
            .map(|npcs| npcs.into_iter().map(|npc| (npc.key.clone(), npc)).collect())
            .unwrap_or_default();
        let live: Vec<&TeamAgent> = agents.iter().filter(|agent| !agent.archived).collect();
        self.drop_gone(&existing, &live);
        self.ensure_captain(server, origin, &existing).await;
        let mut slots = self.used_slots(origin, &existing);
        let mut taken: HashSet<String> = existing
            .values()
            .map(|npc| npc.name.to_ascii_lowercase())
            .collect();
        for agent in live {
            let key = format!("{AGENT_PREFIX}{}", agent.id);
            let npc = match existing.get(&key) {
                Some(npc) => npc.clone(),
                None => {
                    let slot = (0..).find(|slot| !slots.contains(slot)).unwrap_or(0);
                    let name = npc_name(&agent.slug, &taken);
                    let at = slot_position(origin, slot);
                    match server
                        .world
                        .npcs
                        .create(NpcSpec::new(key.clone(), name.clone(), at))
                        .await
                    {
                        Ok(npc) => {
                            slots.insert(slot);
                            taken.insert(name.to_ascii_lowercase());
                            npc.look_at_nearest(GAZE_RANGE);
                            npc
                        }
                        Err(error) => {
                            tracing::warn!("No npc for {}: {error}", agent.slug);
                            continue;
                        }
                    }
                }
            };
            self.update_agent(&key, npc, agent, current_run(&runs, agent), origin);
        }
        self.state().online = Some(true);
        self.restore_captain();
    }

    fn drop_gone(&self, existing: &HashMap<String, Npc>, live: &[&TeamAgent]) {
        let wanted: HashSet<String> = live
            .iter()
            .map(|agent| format!("{AGENT_PREFIX}{}", agent.id))
            .collect();
        let mut state = self.state();
        for (key, npc) in existing {
            if key.starts_with(AGENT_PREFIX) && !wanted.contains(key) {
                npc.remove();
            }
        }
        state
            .agents
            .retain(|id, _| wanted.contains(&format!("{AGENT_PREFIX}{id}")));
    }

    async fn ensure_captain(
        &self,
        server: &Server,
        origin: Position,
        existing: &HashMap<String, Npc>,
    ) {
        if self.state().captain.is_some() {
            return;
        }
        let npc = match existing.get(CAPTAIN_KEY) {
            Some(npc) => npc.clone(),
            None => match server
                .world
                .npcs
                .create(NpcSpec::new(CAPTAIN_KEY, CAPTAIN_NAME, origin))
                .await
            {
                Ok(npc) => npc,
                Err(error) => {
                    tracing::warn!("No captain npc: {error}");
                    return;
                }
            },
        };
        npc.look_at_nearest(GAZE_RANGE);
        let label = captain_label(true);
        npc.set_label(label.clone());
        self.state().captain = Some(Captain { npc, label });
    }

    fn restore_captain(&self) {
        let mut state = self.state();
        let label = captain_label(true);
        if let Some(captain) = state.captain.as_mut()
            && captain.label != label
        {
            captain.npc.set_label(label.clone());
            captain.label = label;
        }
    }

    fn used_slots(&self, origin: Position, existing: &HashMap<String, Npc>) -> HashSet<usize> {
        existing
            .iter()
            .filter(|(key, _)| key.starts_with(AGENT_PREFIX))
            .filter_map(|(_, npc)| {
                let steps = (npc.position.z - origin.z) / SLOT_SPACING;
                (steps >= 1.0).then(|| steps.round() as usize - 1)
            })
            .collect()
    }

    fn update_agent(
        &self,
        key: &str,
        npc: Npc,
        agent: &TeamAgent,
        run: Option<&ActiveRun>,
        origin: Position,
    ) {
        let label = agent_label(agent, run);
        let working = is_working(run);
        let id = key.strip_prefix(AGENT_PREFIX).unwrap_or(key).to_owned();
        let mut state = self.state();
        let tracked = state.agents.entry(id).or_insert_with(|| Tracked {
            slot: slot_of(origin, &npc),
            label: String::new(),
            working: false,
            agent: agent.clone(),
            npc: npc.clone(),
        });
        tracked.agent = agent.clone();
        tracked.npc = npc;
        if tracked.label != label {
            tracked.npc.set_label(label.clone());
            tracked.label = label;
        }
        if tracked.working != working || tracked.slot == usize::MAX {
            tracked.npc.set_pose(if working {
                NpcPose::Crouching
            } else {
                NpcPose::Standing
            });
            tracked.working = working;
        }
    }

    pub fn target_of(&self, entity_id: i32) -> Option<Clicked> {
        let state = self.state();
        if state
            .captain
            .as_ref()
            .is_some_and(|captain| captain.npc.entity_id == entity_id)
        {
            return Some(Clicked::Captain);
        }
        state
            .agents
            .values()
            .find(|tracked| tracked.npc.entity_id == entity_id)
            .map(|tracked| Clicked::Agent(tracked.agent.clone()))
    }

    pub fn agent_by_click_slug(&self, slug: &str) -> Option<TeamAgent> {
        self.state()
            .agents
            .values()
            .find(|tracked| click_slug(&tracked.agent.slug) == slug)
            .map(|tracked| tracked.agent.clone())
    }

    pub async fn start_task(
        &self,
        agent: &TeamAgent,
        title: &str,
        description: &str,
    ) -> Result<String, ApiError> {
        self.api
            .start_task(&agent.id, title, description)
            .await
            .map(|run| run.run_id)
    }

    pub fn in_chat(&self, player: Uuid) -> bool {
        self.state().chats.contains_key(&player)
    }

    pub fn toggle_chat(&self, player: Uuid) -> bool {
        let mut state = self.state();
        if state.chats.remove(&player).is_some() {
            return false;
        }
        state.chats.insert(player, Chat::default());
        true
    }

    pub fn leave_chat(&self, player: Uuid) {
        self.state().chats.remove(&player);
    }

    pub fn is_quit(text: &str) -> bool {
        QUIT_WORDS.contains(&text.trim().to_lowercase().as_str())
    }

    pub fn begin_turn(&self, player: Uuid) -> Turn {
        let mut state = self.state();
        let Some(chat) = state.chats.get_mut(&player) else {
            return Turn::Busy;
        };
        if chat.busy {
            return Turn::Busy;
        }
        chat.busy = true;
        Turn::Ready(chat.conversation.clone())
    }

    pub fn end_turn(&self, player: Uuid, conversation: Option<String>) {
        if let Some(chat) = self.state().chats.get_mut(&player) {
            chat.busy = false;
            if conversation.is_some() {
                chat.conversation = conversation;
            }
        }
    }

    pub async fn chat(
        &self,
        player: &Player,
        conversation: Option<&str>,
        message: &str,
    ) -> Result<crate::api::ChatOutcome, ApiError> {
        self.api
            .chat(conversation, message, |line| {
                player.message.send(format!("§6{CAPTAIN_NAME}§8: §f{line}"));
            })
            .await
    }
}

fn current_run<'a>(runs: &'a [ActiveRun], agent: &TeamAgent) -> Option<&'a ActiveRun> {
    let mine = || {
        runs.iter()
            .filter(|run| run.run.agent.as_ref().is_some_and(|a| a.id == agent.id))
    };
    mine()
        .find(|run| run.run.status == "running")
        .or_else(|| mine().next())
}

fn slot_position(origin: Position, slot: usize) -> Position {
    Position {
        x: origin.x,
        y: origin.y,
        z: origin.z + SLOT_SPACING * (slot as f64 + 1.0),
    }
}

fn slot_of(origin: Position, npc: &Npc) -> usize {
    let steps = (npc.position.z - origin.z) / SLOT_SPACING;
    if steps >= 1.0 {
        steps.round() as usize - 1
    } else {
        usize::MAX
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{AgentRef, RunSummary};

    fn agent(id: &str) -> TeamAgent {
        TeamAgent {
            id: id.to_owned(),
            slug: id.to_owned(),
            name: id.to_owned(),
            role: None,
            archived: false,
            paused: false,
        }
    }

    fn run(agent: &str, status: &str) -> ActiveRun {
        ActiveRun {
            run: RunSummary {
                id: format!("{agent}-{status}"),
                status: status.to_owned(),
                title: None,
                card_id: None,
                agent: Some(AgentRef {
                    id: agent.to_owned(),
                    name: String::new(),
                }),
            },
            current_step: None,
            steps_done: 0,
            steps_total: 0,
        }
    }

    #[test]
    fn a_running_run_wins_over_a_queued_one_and_other_agents_are_ignored() {
        let runs = [run("a", "queued"), run("b", "running"), run("a", "running")];
        assert_eq!(current_run(&runs, &agent("a")).unwrap().run.id, "a-running");
        assert_eq!(current_run(&runs, &agent("b")).unwrap().run.id, "b-running");
        assert!(current_run(&runs, &agent("c")).is_none());
        let queued = [run("a", "queued")];
        assert_eq!(
            current_run(&queued, &agent("a")).unwrap().run.id,
            "a-queued"
        );
    }

    #[test]
    fn slots_are_spaced_along_z_from_the_origin() {
        let origin = Position {
            x: 3.5,
            y: 64.0,
            z: 0.5,
        };
        assert_eq!(slot_position(origin, 0).z, 2.5);
        assert_eq!(slot_position(origin, 2).z, 6.5);
        assert_eq!(slot_position(origin, 2).x, 3.5);
    }

    #[test]
    fn quit_words_are_recognised_in_any_case() {
        assert!(Bridge::is_quit("Quitter"));
        assert!(Bridge::is_quit("  EXIT "));
        assert!(!Bridge::is_quit("je quitte"));
    }
}

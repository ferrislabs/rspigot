use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct AgentList {
    pub items: Vec<TeamAgent>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct TeamAgent {
    pub id: String,
    pub slug: String,
    pub name: String,
    #[serde(default)]
    pub role: Option<String>,
    #[serde(default)]
    pub archived: bool,
    #[serde(default)]
    pub paused: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ActiveRuns {
    pub items: Vec<ActiveRun>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct ActiveRun {
    pub run: RunSummary,
    #[serde(default)]
    pub current_step: Option<CurrentStep>,
    #[serde(default)]
    pub steps_done: u32,
    #[serde(default)]
    pub steps_total: u32,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct RunSummary {
    pub id: String,
    pub status: String,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub card_id: Option<String>,
    #[serde(default)]
    pub agent: Option<AgentRef>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct AgentRef {
    pub id: String,
    #[serde(default)]
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct CurrentStep {
    pub kind: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CardCreated {
    pub id: String,
    pub version: i64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RunStarted {
    pub run_id: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ConversationCreated {
    pub id: String,
}

use std::time::{Duration, Instant};

use futures_util::StreamExt;
use serde::Deserialize;
use serde_json::json;
use tokio::sync::Mutex;

use crate::config::Config;
use crate::model::{
    ActiveRun, ActiveRuns, AgentList, CardCreated, ConversationCreated, RunStarted, TeamAgent,
};

const TOKEN_MARGIN: Duration = Duration::from_secs(30);
const DEFAULT_TOKEN_LIFETIME: u64 = 60;
const KNOWN_CODES: [&str; 11] = [
    "not_connected",
    "no_provider",
    "execution_already_active",
    "card_without_agent",
    "agent_archived",
    "agent_paused",
    "agent_without_connector",
    "turn_in_progress",
    "conversation_full",
    "captain_not_configured",
    "captain_unavailable",
];

#[derive(Debug, thiserror::Error)]
pub enum ApiError {
    #[error("request failed: {0}")]
    Http(#[from] reqwest::Error),
    #[error("capitainerie answered {status}: {body}")]
    Status { status: u16, body: String },
    #[error("could not get a token: {0}")]
    Token(String),
    #[error("the conversation failed ({code}): {message}")]
    Chat { code: String, message: String },
}

impl ApiError {
    pub fn code(&self) -> Option<&'static str> {
        let text = match self {
            Self::Status { body, .. } => body.as_str(),
            Self::Chat { code, .. } => code.as_str(),
            _ => return None,
        };
        KNOWN_CODES.into_iter().find(|code| text.contains(code))
    }
}

struct Cached {
    token: String,
    expires: Instant,
}

pub struct Api {
    http: reqwest::Client,
    config: Config,
    token: Mutex<Option<Cached>>,
}

#[derive(Deserialize)]
struct TokenResponse {
    access_token: String,
    #[serde(default)]
    expires_in: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChatEvent {
    Delta(String),
    Card { id: String, title: String },
    Done,
    Error { code: String, message: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChatOutcome {
    pub conversation: String,
    pub cards: Vec<(String, String)>,
}

#[derive(Default)]
pub struct SseBuffer {
    pending: String,
}

impl SseBuffer {
    pub fn push(&mut self, chunk: &str) -> Vec<ChatEvent> {
        self.pending.push_str(&chunk.replace("\r\n", "\n"));
        let mut events = Vec::new();
        while let Some(end) = self.pending.find("\n\n") {
            let block: String = self.pending.drain(..end + 2).collect();
            if let Some(event) = parse_block(&block) {
                events.push(event);
            }
        }
        events
    }
}

fn parse_block(block: &str) -> Option<ChatEvent> {
    let mut name = None;
    let mut data = Vec::new();
    for line in block.lines() {
        if let Some(value) = line.strip_prefix("event:") {
            name = Some(value.trim().to_owned());
        } else if let Some(value) = line.strip_prefix("data:") {
            data.push(value.strip_prefix(' ').unwrap_or(value).to_owned());
        }
    }
    let payload: serde_json::Value = serde_json::from_str(&data.join("\n")).ok()?;
    let text = |key: &str| {
        payload
            .get(key)
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default()
            .to_owned()
    };
    match name?.as_str() {
        "delta" => Some(ChatEvent::Delta(text("text"))),
        "card" => Some(ChatEvent::Card {
            id: text("card_id"),
            title: text("title"),
        }),
        "done" => Some(ChatEvent::Done),
        "error" => Some(ChatEvent::Error {
            code: text("code"),
            message: text("message"),
        }),
        _ => None,
    }
}

impl Api {
    pub fn new(config: Config) -> Self {
        Self {
            http: reqwest::Client::new(),
            config,
            token: Mutex::new(None),
        }
    }

    async fn bearer(&self) -> Result<String, ApiError> {
        let mut cached = self.token.lock().await;
        if let Some(current) = cached.as_ref()
            && current.expires > Instant::now() + TOKEN_MARGIN
        {
            return Ok(current.token.clone());
        }
        let mut form = vec![
            ("grant_type", "password"),
            ("client_id", self.config.client_id.as_str()),
            ("username", self.config.username.as_str()),
            ("password", self.config.password.as_str()),
        ];
        if let Some(secret) = &self.config.client_secret {
            form.push(("client_secret", secret));
        }
        if let Some(audience) = &self.config.audience {
            form.push(("audience", audience));
        }
        let response = self
            .http
            .post(&self.config.token_url)
            .form(&form)
            .send()
            .await?;
        if !response.status().is_success() {
            return Err(ApiError::Token(format!("status {}", response.status())));
        }
        let granted: TokenResponse = response.json().await?;
        let lifetime = granted.expires_in.unwrap_or(DEFAULT_TOKEN_LIFETIME);
        let token = granted.access_token;
        *cached = Some(Cached {
            token: token.clone(),
            expires: Instant::now() + Duration::from_secs(lifetime),
        });
        Ok(token)
    }

    fn url(&self, path: &str) -> String {
        format!("{}/api/v1{path}", self.config.base_url)
    }

    async fn checked(response: reqwest::Response) -> Result<reqwest::Response, ApiError> {
        if response.status().is_success() {
            return Ok(response);
        }
        let status = response.status().as_u16();
        let body = response.text().await.unwrap_or_default();
        Err(ApiError::Status {
            status,
            body: body.chars().take(500).collect(),
        })
    }

    async fn get<T: for<'de> Deserialize<'de>>(&self, path: &str) -> Result<T, ApiError> {
        let response = self
            .http
            .get(self.url(path))
            .bearer_auth(self.bearer().await?)
            .send()
            .await?;
        Ok(Self::checked(response).await?.json().await?)
    }

    async fn post<T: for<'de> Deserialize<'de>>(
        &self,
        path: &str,
        body: &serde_json::Value,
    ) -> Result<T, ApiError> {
        let response = self
            .http
            .post(self.url(path))
            .bearer_auth(self.bearer().await?)
            .json(body)
            .send()
            .await?;
        Ok(Self::checked(response).await?.json().await?)
    }

    pub async fn agents(&self) -> Result<Vec<TeamAgent>, ApiError> {
        let list: AgentList = self.get("/team/agents").await?;
        Ok(list.items)
    }

    pub async fn active_runs(&self) -> Result<Vec<ActiveRun>, ApiError> {
        let runs: ActiveRuns = self.get("/runs/active").await?;
        Ok(runs.items)
    }

    pub async fn start_task(
        &self,
        agent_id: &str,
        title: &str,
        description: &str,
    ) -> Result<RunStarted, ApiError> {
        let card: CardCreated = self
            .post(
                "/cards",
                &json!({
                    "title": title,
                    "description": description,
                    "assignee_agent_id": agent_id,
                    "priority": "normal",
                }),
            )
            .await?;
        self.post(
            &format!("/cards/{}/agent-runs", card.id),
            &json!({"expected_version": card.version, "allow_writes": false}),
        )
        .await
    }

    pub async fn chat(
        &self,
        conversation: Option<&str>,
        message: &str,
        mut on_line: impl FnMut(String),
    ) -> Result<ChatOutcome, ApiError> {
        let (conversation, send) = match conversation {
            Some(id) => (id.to_owned(), Some(message)),
            None => {
                let created: ConversationCreated = self
                    .post("/conversations", &json!({"message": message}))
                    .await?;
                (created.id, None)
            }
        };
        let response = self
            .http
            .post(self.url(&format!("/conversations/{conversation}/messages")))
            .bearer_auth(self.bearer().await?)
            .json(&json!({"message": send}))
            .send()
            .await?;
        let mut stream = Self::checked(response).await?.bytes_stream();
        let (mut buffer, mut line, mut cards) = (SseBuffer::default(), String::new(), Vec::new());
        let mut done = false;
        while let Some(chunk) = stream.next().await {
            for event in buffer.push(&String::from_utf8_lossy(&chunk?)) {
                match event {
                    ChatEvent::Delta(text) => {
                        line.push_str(&text);
                        while let Some(end) = line.find('\n') {
                            let complete: String = line.drain(..=end).collect();
                            on_line(complete.trim_end().to_owned());
                        }
                    }
                    ChatEvent::Card { id, title } => cards.push((id, title)),
                    ChatEvent::Done => done = true,
                    ChatEvent::Error { code, message } => {
                        return Err(ApiError::Chat { code, message });
                    }
                }
            }
        }
        if !line.trim().is_empty() {
            on_line(line.trim().to_owned());
        }
        if !done {
            return Err(ApiError::Chat {
                code: "interrupted".to_owned(),
                message: "the answer stopped before the end".to_owned(),
            });
        }
        Ok(ChatOutcome {
            conversation,
            cards,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn events(chunks: &[&str]) -> Vec<ChatEvent> {
        let mut buffer = SseBuffer::default();
        chunks.iter().flat_map(|chunk| buffer.push(chunk)).collect()
    }

    #[test]
    fn events_split_across_chunks_are_reassembled() {
        let got = events(&[
            "event: delta\ndata: {\"te",
            "xt\":\"Bon\"}\n\nevent: delta\ndata: {\"text\":\"jour\"}\n",
            "\nevent: done\ndata: {\"status\":\"complete\"}\n\n",
        ]);
        assert_eq!(
            got,
            [
                ChatEvent::Delta("Bon".to_owned()),
                ChatEvent::Delta("jour".to_owned()),
                ChatEvent::Done
            ]
        );
    }

    #[test]
    fn card_and_error_events_carry_their_fields() {
        let got = events(&[
            "event: card\ndata: {\"card_id\":\"c1\",\"title\":\"Faire\"}\n\n",
            "event: error\ndata: {\"code\":\"turn_in_progress\",\"message\":\"busy\"}\n\n",
        ]);
        assert_eq!(
            got,
            [
                ChatEvent::Card {
                    id: "c1".to_owned(),
                    title: "Faire".to_owned()
                },
                ChatEvent::Error {
                    code: "turn_in_progress".to_owned(),
                    message: "busy".to_owned()
                }
            ]
        );
    }

    #[test]
    fn crlf_tool_events_and_garbage_are_skipped() {
        let got = events(&[
            "event: tool\r\ndata: {\"phase\":\"start\"}\r\n\r\n",
            "data: not json\n\n",
            ": comment\n\n",
            "event: delta\r\ndata: {\"text\":\"ok\"}\r\n\r\n",
        ]);
        assert_eq!(got, [ChatEvent::Delta("ok".to_owned())]);
    }

    #[test]
    fn known_api_codes_are_found_in_the_body() {
        let error = ApiError::Status {
            status: 409,
            body: "{\"code\":\"execution_already_active\"}".to_owned(),
        };
        assert_eq!(error.code(), Some("execution_already_active"));
        let other = ApiError::Status {
            status: 500,
            body: "boom".to_owned(),
        };
        assert_eq!(other.code(), None);
    }
}

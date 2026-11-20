use rspigot_plugin_protocol::v1::{self, answer, question};

use crate::{Error, PlayerAdvancementsHandle};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdvancementProgress {
    pub done: bool,
    pub criteria_done: Vec<String>,
    pub criteria_remaining: Vec<String>,
}

impl PlayerAdvancementsHandle {
    pub async fn get_progress(
        &self,
        advancement: impl Into<String>,
    ) -> Result<AdvancementProgress, Error> {
        let progress = self
            .link
            .ask(
                question::Question::AdvancementProgress(v1::AdvancementQuery {
                    player: self.target.to_string(),
                    advancement: advancement.into(),
                }),
                "an advancement progress",
                |body| match body {
                    answer::Body::AdvancementProgress(progress) => Some(progress),
                    _ => None,
                },
            )
            .await?;
        Ok(AdvancementProgress {
            done: progress.done,
            criteria_done: progress.criteria_done,
            criteria_remaining: progress.criteria_remaining,
        })
    }

    pub async fn get_done(&self) -> Result<Vec<String>, Error> {
        let list = self
            .link
            .ask(
                question::Question::DoneAdvancements(self.target.to_string()),
                "a list of advancements",
                |body| match body {
                    answer::Body::Advancements(list) => Some(list),
                    _ => None,
                },
            )
            .await?;
        Ok(list.ids)
    }
}

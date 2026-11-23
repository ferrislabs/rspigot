use std::sync::Arc;
use std::time::Duration;

use rspigot_plugin_protocol::v1::{
    self, answer, extra_answer, extra_question, from_plugin, player_extra, question,
};

use crate::Error;
use crate::link::Link;

fn ticks_of(duration: Duration) -> i64 {
    i64::try_from(duration.as_millis() * 20 / 1000).unwrap_or(i64::MAX)
}

fn order(target: &Arc<str>, link: &Link, order: player_extra::Order) {
    link.order(from_plugin::Body::Player(v1::PlayerOrder {
        player: target.to_string(),
        order: Some(v1::player_order::Order::Extra(v1::PlayerExtra {
            order: Some(order),
        })),
    }));
}

#[derive(Debug, Clone)]
pub struct PlayerCriteriaHandle {
    target: Arc<str>,
    link: Link,
}

impl PlayerCriteriaHandle {
    pub(crate) fn new(target: Arc<str>, link: Link) -> Self {
        Self { target, link }
    }

    pub fn award(&self, advancement: &str, criterion: &str) {
        order(
            &self.target,
            &self.link,
            player_extra::Order::AwardCriterion(v1::CriterionOrder {
                advancement: advancement.to_owned(),
                criterion: criterion.to_owned(),
            }),
        );
    }

    pub fn revoke(&self, advancement: &str, criterion: &str) {
        order(
            &self.target,
            &self.link,
            player_extra::Order::RevokeCriterion(v1::CriterionOrder {
                advancement: advancement.to_owned(),
                criterion: criterion.to_owned(),
            }),
        );
    }
}

#[derive(Debug, Clone)]
pub struct PlayerViewHandle {
    target: Arc<str>,
    link: Link,
}

impl PlayerViewHandle {
    pub(crate) fn new(target: Arc<str>, link: Link) -> Self {
        Self { target, link }
    }

    pub fn hide_entity(&self, entity_id: i32) {
        order(
            &self.target,
            &self.link,
            player_extra::Order::HideEntity(entity_id),
        );
    }

    pub fn show_entity(&self, entity_id: i32) {
        order(
            &self.target,
            &self.link,
            player_extra::Order::ShowEntity(entity_id),
        );
    }

    fn border(&self, change: v1::world_border_order::Change) {
        order(
            &self.target,
            &self.link,
            player_extra::Order::SetPlayerBorder(v1::WorldBorderOrder {
                change: Some(change),
            }),
        );
    }

    pub fn set_border_size(&self, size: f64) {
        self.resize_border(size, Duration::ZERO);
    }

    pub fn resize_border(&self, size: f64, over: Duration) {
        self.border(v1::world_border_order::Change::Resize(v1::BorderResize {
            size,
            ticks: ticks_of(over),
        }));
    }

    pub fn set_border_center(&self, x: f64, z: f64) {
        self.border(v1::world_border_order::Change::SetCenter(
            v1::BorderCenter { x, z },
        ));
    }

    pub fn set_border_damage_amount(&self, damage: f64) {
        self.border(v1::world_border_order::Change::SetDamageAmount(damage));
    }

    pub fn set_border_damage_buffer(&self, distance: f64) {
        self.border(v1::world_border_order::Change::SetDamageBuffer(distance));
    }

    pub fn set_border_warning_distance(&self, blocks: i32) {
        self.border(v1::world_border_order::Change::SetWarningDistance(blocks));
    }

    pub fn set_border_warning_time(&self, time: Duration) {
        let ticks = i32::try_from(ticks_of(time)).unwrap_or(i32::MAX);
        self.border(v1::world_border_order::Change::SetWarningTimeTicks(ticks));
    }

    pub fn reset_border(&self) {
        order(
            &self.target,
            &self.link,
            player_extra::Order::ResetPlayerBorder(v1::Empty {}),
        );
    }
}

#[derive(Debug, Clone)]
pub struct PlayerCookieHandle {
    target: Arc<str>,
    link: Link,
}

impl PlayerCookieHandle {
    pub(crate) fn new(target: Arc<str>, link: Link) -> Self {
        Self { target, link }
    }

    pub fn store(&self, key: &str, payload: impl Into<Vec<u8>>) {
        order(
            &self.target,
            &self.link,
            player_extra::Order::StoreCookie(v1::CookieStore {
                key: key.to_owned(),
                payload: payload.into(),
            }),
        );
    }

    pub async fn retrieve(&self, key: &str) -> Result<Option<Vec<u8>>, Error> {
        let answer = self
            .link
            .ask(
                question::Question::Extra(v1::ExtraQuestion {
                    query: Some(extra_question::Query::RetrieveCookie(v1::CookieQuery {
                        player: self.target.to_string(),
                        key: key.to_owned(),
                    })),
                }),
                "a cookie",
                |body| match body {
                    answer::Body::Extra(v1::ExtraAnswer {
                        answer: Some(extra_answer::Answer::Cookie(cookie)),
                    }) => Some(cookie),
                    _ => None,
                },
            )
            .await?;
        Ok(answer.payload)
    }
}

#[derive(Debug, Clone)]
pub struct PlayerLinksHandle {
    target: Arc<str>,
    link: Link,
}

impl PlayerLinksHandle {
    pub(crate) fn new(target: Arc<str>, link: Link) -> Self {
        Self { target, link }
    }

    pub fn send(&self, links: &[(&str, &str)]) {
        order(
            &self.target,
            &self.link,
            player_extra::Order::SendLinks(v1::LinkList {
                links: links
                    .iter()
                    .map(|(label, url)| v1::LinkEntry {
                        label: (*label).to_owned(),
                        url: (*url).to_owned(),
                    })
                    .collect(),
            }),
        );
    }
}

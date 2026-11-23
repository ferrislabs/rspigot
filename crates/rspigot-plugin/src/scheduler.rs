use std::future::Future;

use tokio::sync::mpsc;

use crate::link::Link;

#[derive(Debug)]
pub struct Ticker {
    id: u64,
    link: Link,
    fired: mpsc::UnboundedReceiver<()>,
}

impl Ticker {
    pub const fn id(&self) -> u64 {
        self.id
    }

    pub async fn tick(&mut self) -> bool {
        self.fired.recv().await.is_some()
    }

    pub fn cancel(&self) {
        self.link.cancel_task(self.id);
    }
}

#[derive(Debug, Clone)]
pub struct Task {
    id: u64,
    link: Link,
}

impl Task {
    pub const fn id(&self) -> u64 {
        self.id
    }

    pub fn cancel(&self) {
        self.link.cancel_task(self.id);
    }
}

#[derive(Debug, Clone)]
pub struct Scheduler {
    link: Link,
}

impl Scheduler {
    pub(crate) fn new(link: Link) -> Self {
        Self { link }
    }

    pub fn ticker(&self, delay: u32, period: u32) -> Ticker {
        let (id, fired) = self.link.schedule_task(delay, period);
        Ticker {
            id,
            link: self.link.clone(),
            fired,
        }
    }

    pub fn run_later<F, Fut>(&self, delay: u32, task: F) -> Task
    where
        F: FnOnce() -> Fut + Send + 'static,
        Fut: Future<Output = ()> + Send + 'static,
    {
        let mut ticker = self.ticker(delay, 0);
        let handle = Task {
            id: ticker.id,
            link: self.link.clone(),
        };
        tokio::spawn(async move {
            if ticker.tick().await {
                task().await;
            }
        });
        handle
    }

    pub fn run_timer<F, Fut>(&self, delay: u32, period: u32, mut task: F) -> Task
    where
        F: FnMut() -> Fut + Send + 'static,
        Fut: Future<Output = ()> + Send + 'static,
    {
        let mut ticker = self.ticker(delay, period);
        let handle = Task {
            id: ticker.id,
            link: self.link.clone(),
        };
        tokio::spawn(async move {
            while ticker.tick().await {
                task().await;
            }
        });
        handle
    }
}

#[cfg(test)]
mod tests {
    use rspigot_plugin_protocol::v1::{self, from_plugin, server_order};

    use super::*;

    fn connected() -> (
        Link,
        crate::link::Attached,
        mpsc::UnboundedReceiver<v1::FromPlugin>,
    ) {
        let link = Link::default();
        let (outbound, sent) = mpsc::unbounded_channel();
        let attached = link.attach(outbound);
        (link, attached, sent)
    }

    fn order_of(message: v1::FromPlugin) -> server_order::Order {
        match message.body {
            Some(from_plugin::Body::Server(v1::ServerOrder { order: Some(order) })) => order,
            other => panic!("expected a server order, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn a_delayed_ticker_fires_once_and_then_ends() {
        let (link, _attached, mut sent) = connected();
        let mut ticker = Scheduler::new(link.clone()).ticker(3, 0);

        assert_eq!(
            order_of(sent.try_recv().unwrap()),
            server_order::Order::ScheduleTask(v1::ScheduleTask {
                id: ticker.id(),
                delay: 3,
                period: 0
            })
        );
        link.task_due(ticker.id());
        link.task_due(ticker.id());
        assert!(ticker.tick().await);
        assert!(!ticker.tick().await);
    }

    #[tokio::test]
    async fn a_repeating_ticker_keeps_firing_until_cancelled() {
        let (link, _attached, mut sent) = connected();
        let mut ticker = Scheduler::new(link.clone()).ticker(1, 4);
        sent.try_recv().unwrap();

        for _ in 0..3 {
            link.task_due(ticker.id());
            assert!(ticker.tick().await);
        }
        ticker.cancel();

        assert_eq!(
            order_of(sent.try_recv().unwrap()),
            server_order::Order::CancelTask(ticker.id())
        );
        assert!(!ticker.tick().await);
        ticker.cancel();
        assert!(sent.try_recv().is_err());
    }

    #[tokio::test]
    async fn tasks_end_when_the_connection_is_dropped_and_unloading_cancels_them_all() {
        let (link, attached, mut sent) = connected();
        let scheduler = Scheduler::new(link.clone());
        let mut first = scheduler.ticker(1, 1);
        let mut second = scheduler.ticker(2, 2);
        sent.try_recv().unwrap();
        sent.try_recv().unwrap();

        link.cancel_all_tasks();
        let mut cancelled = vec![
            order_of(sent.try_recv().unwrap()),
            order_of(sent.try_recv().unwrap()),
        ];
        cancelled.sort_by_key(|order| match order {
            server_order::Order::CancelTask(id) => *id,
            _ => 0,
        });
        assert_eq!(
            cancelled,
            vec![
                server_order::Order::CancelTask(first.id()),
                server_order::Order::CancelTask(second.id())
            ]
        );
        assert!(!first.tick().await);

        let mut third = scheduler.ticker(1, 1);
        drop(attached);
        assert!(!third.tick().await);
        assert!(!second.tick().await);
    }
}

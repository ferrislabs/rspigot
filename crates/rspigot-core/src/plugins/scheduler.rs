use std::collections::BTreeMap;

use crate::plugins::ToPlugin;
use crate::ports::ClientLink;
use crate::server::Server;

pub const MAX_PLUGIN_TASKS: usize = 4096;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SchedulerOrder {
    Schedule { id: u64, delay: u32, period: u32 },
    Cancel(u64),
}

impl SchedulerOrder {
    pub const fn name(&self) -> &'static str {
        match self {
            Self::Schedule { .. } => "scheduler.schedule",
            Self::Cancel(_) => "scheduler.cancel",
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct Task {
    next: u64,
    period: u32,
}

#[derive(Debug, Default)]
pub(crate) struct Tasks {
    held: BTreeMap<(u64, u64), Task>,
}

impl Tasks {
    fn held_by(&self, connection: u64) -> usize {
        self.held
            .range((connection, 0)..=(connection, u64::MAX))
            .count()
    }

    pub(crate) fn len(&self) -> usize {
        self.held.len()
    }
}

impl<L: ClientLink> Server<L> {
    pub(super) fn scheduler_order(&mut self, connection: u64, order: SchedulerOrder) {
        match order {
            SchedulerOrder::Schedule { id, delay, period } => {
                let tasks = &mut self.plugins.tasks;
                let replaced = tasks.held.contains_key(&(connection, id));
                if !replaced && tasks.held_by(connection) >= MAX_PLUGIN_TASKS {
                    self.warn_plugin(
                        connection,
                        order.name(),
                        format!("the plugin already holds {MAX_PLUGIN_TASKS} tasks"),
                    );
                    return;
                }
                let next = self.current_tick() + u64::from(delay.max(1));
                self.plugins
                    .tasks
                    .held
                    .insert((connection, id), Task { next, period });
            }
            SchedulerOrder::Cancel(id) => {
                self.plugins.tasks.held.remove(&(connection, id));
            }
        }
    }

    pub(crate) fn tick_plugin_tasks(&mut self) {
        if self.plugins.tasks.held.is_empty() {
            return;
        }
        let now = self.current_tick();
        let mut due = Vec::new();
        self.plugins.tasks.held.retain(|key, task| {
            if task.next > now {
                return true;
            }
            due.push(*key);
            if task.period == 0 {
                return false;
            }
            task.next = now + u64::from(task.period);
            true
        });
        for (connection, id) in due {
            self.plugins.send(connection, ToPlugin::TaskDue(id));
        }
    }

    pub(crate) fn forget_plugin_tasks(&mut self, connection: u64) {
        self.plugins
            .tasks
            .held
            .retain(|(owner, _), _| *owner != connection);
    }

    pub fn plugin_task_count(&self) -> usize {
        self.plugins.tasks.len()
    }
}

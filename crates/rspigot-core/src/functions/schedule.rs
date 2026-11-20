use rspigot_domain::Identifier;
use rspigot_nbt::{Compound, Tag};

pub const SCHEDULED_DATA: &str = "scheduled_events";
const FUNCTION_TYPE: &str = "minecraft:function";
const TAG_TYPE: &str = "minecraft:function_tag";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Callback {
    Function(Identifier),
    Tag(Identifier),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScheduledEvent {
    pub name: String,
    pub trigger_time: i64,
    pub callback: Callback,
}

#[derive(Debug, Clone, Default)]
pub struct ScheduledEvents {
    events: Vec<ScheduledEvent>,
}

impl ScheduledEvents {
    pub fn add(&mut self, event: ScheduledEvent, replace: bool) {
        if replace {
            self.remove(&event.name);
        }
        let position = self
            .events
            .partition_point(|queued| queued.trigger_time <= event.trigger_time);
        self.events.insert(position, event);
    }

    pub fn remove(&mut self, name: &str) -> usize {
        let before = self.events.len();
        self.events.retain(|event| event.name != name);
        before - self.events.len()
    }

    pub fn take_due(&mut self, game_time: i64) -> Vec<ScheduledEvent> {
        let due = self
            .events
            .partition_point(|event| event.trigger_time <= game_time);
        self.events.drain(..due).collect()
    }

    pub fn names(&self) -> Vec<&str> {
        let mut names: Vec<&str> = self
            .events
            .iter()
            .map(|event| event.name.as_str())
            .collect();
        names.sort_unstable();
        names.dedup();
        names
    }

    pub fn events(&self) -> &[ScheduledEvent] {
        &self.events
    }

    pub fn len(&self) -> usize {
        self.events.len()
    }

    pub fn is_empty(&self) -> bool {
        self.events.is_empty()
    }

    pub fn to_nbt(&self) -> Compound {
        let events = self
            .events
            .iter()
            .map(|event| {
                let (kind, id) = match &event.callback {
                    Callback::Function(id) => (FUNCTION_TYPE, id),
                    Callback::Tag(id) => (TAG_TYPE, id),
                };
                Tag::Compound(
                    Compound::new()
                        .with("id", event.name.as_str())
                        .with("trigger_time", event.trigger_time)
                        .with(
                            "callback",
                            Compound::new()
                                .with("type", kind)
                                .with("id", id.to_string()),
                        ),
                )
            })
            .collect();
        Compound::new().with("events", Tag::List(events))
    }

    pub fn from_nbt(data: &Compound) -> Self {
        let mut events = Self::default();
        for entry in data.get_list("events").unwrap_or_default() {
            let Some(entry) = entry.as_compound() else {
                continue;
            };
            let (Some(name), Some(trigger_time), Some(callback)) = (
                entry.get_str("id"),
                entry.get_i64("trigger_time"),
                entry.get_compound("callback"),
            ) else {
                continue;
            };
            let Some(target) = callback
                .get_str("id")
                .and_then(|id| Identifier::parse(id).ok())
            else {
                continue;
            };
            let callback = match callback.get_str("type") {
                Some(FUNCTION_TYPE) => Callback::Function(target),
                Some(TAG_TYPE) => Callback::Tag(target),
                _ => continue,
            };
            events.add(
                ScheduledEvent {
                    name: name.to_owned(),
                    trigger_time,
                    callback,
                },
                false,
            );
        }
        events
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn event(name: &str, time: i64) -> ScheduledEvent {
        ScheduledEvent {
            name: name.to_owned(),
            trigger_time: time,
            callback: Callback::Function(Identifier::parse(name).unwrap()),
        }
    }

    fn names(events: &[ScheduledEvent]) -> Vec<&str> {
        events.iter().map(|event| event.name.as_str()).collect()
    }

    #[test]
    fn due_events_come_out_in_time_then_insertion_order() {
        let mut queue = ScheduledEvents::default();
        queue.add(event("t:b", 10), false);
        queue.add(event("t:a", 5), false);
        queue.add(event("t:c", 10), false);
        queue.add(event("t:late", 50), false);
        assert!(queue.take_due(4).is_empty());
        assert_eq!(names(&queue.take_due(10)), ["t:a", "t:b", "t:c"]);
        assert_eq!(queue.len(), 1);
    }

    #[test]
    fn replace_drops_same_name_and_append_keeps_it() {
        let mut queue = ScheduledEvents::default();
        queue.add(event("t:a", 5), false);
        queue.add(event("t:a", 6), false);
        assert_eq!(queue.len(), 2);
        queue.add(event("t:a", 9), true);
        assert_eq!(queue.events(), [event("t:a", 9)]);
        assert_eq!(queue.remove("t:a"), 1);
        assert_eq!(queue.remove("t:a"), 0);
    }

    #[test]
    fn events_round_trip_through_nbt() {
        let mut queue = ScheduledEvents::default();
        queue.add(event("t:a", 5), false);
        queue.add(
            ScheduledEvent {
                name: "#t:group".to_owned(),
                trigger_time: 7,
                callback: Callback::Tag(Identifier::parse("t:group").unwrap()),
            },
            false,
        );
        let restored = ScheduledEvents::from_nbt(&queue.to_nbt());
        assert_eq!(restored.events(), queue.events());
    }
}

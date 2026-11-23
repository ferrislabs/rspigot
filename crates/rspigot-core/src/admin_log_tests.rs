use std::sync::{Arc, Mutex};

use rspigot_domain::text::Component;

use crate::end_tests::arrive;

#[derive(Default, Clone)]
struct Lines(Arc<Mutex<Vec<String>>>);

impl tracing::Subscriber for Lines {
    fn enabled(&self, metadata: &tracing::Metadata<'_>) -> bool {
        *metadata.level() <= tracing::Level::INFO
    }
    fn new_span(&self, _: &tracing::span::Attributes<'_>) -> tracing::span::Id {
        tracing::span::Id::from_u64(1)
    }
    fn record(&self, _: &tracing::span::Id, _: &tracing::span::Record<'_>) {}
    fn record_follows_from(&self, _: &tracing::span::Id, _: &tracing::span::Id) {}
    fn event(&self, event: &tracing::Event<'_>) {
        struct Message(String);
        impl tracing::field::Visit for Message {
            fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
                if field.name() == "message" {
                    self.0 = format!("{value:?}");
                }
            }
        }
        let mut message = Message(String::new());
        event.record(&mut message);
        self.0.lock().unwrap().push(message.0);
    }
    fn enter(&self, _: &tracing::span::Id) {}
    fn exit(&self, _: &tracing::span::Id) {}
}

fn logged(rule: bool) -> Vec<String> {
    let (mut server, _, _) = arrive();
    server.config.rules.log_admin_commands = rule;
    let lines = Lines::default();
    tracing::subscriber::with_default(lines.clone(), || {
        server.inform_operators("Block", Component::text("done"));
    });
    std::mem::take(&mut *lines.0.lock().unwrap())
}

#[test]
fn the_admin_broadcast_reaches_the_console_log_when_the_rule_is_on() {
    assert!(logged(true).iter().any(|line| line.contains("done")));
}

#[test]
fn the_admin_broadcast_stays_out_of_the_console_log_when_the_rule_is_off() {
    assert!(logged(false).is_empty());
}

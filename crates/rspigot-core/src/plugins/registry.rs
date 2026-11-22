use std::collections::BTreeMap;

use crate::config::PluginSettings;
use crate::plugins::{
    Access, CommandSpec, EventKind, HandlerSpec, IntroError, PluginInfo, PluginIntro, Priority,
    ToPlugin,
};
use crate::ports::PluginOutbox;

struct Connected {
    intro: PluginIntro,
    outbox: Box<dyn PluginOutbox>,
    since: u64,
}

impl Connected {
    fn info(&self) -> PluginInfo {
        PluginInfo {
            name: self.intro.name.clone(),
            version: self.intro.version.clone(),
        }
    }

    fn departed(self, connection: u64) -> Departed {
        Departed {
            connection,
            outbox: self.outbox,
            commands: !self.intro.commands.is_empty(),
            plugin: PluginInfo {
                name: self.intro.name,
                version: self.intro.version,
            },
        }
    }
}

pub(crate) struct Departed {
    pub(crate) connection: u64,
    pub(crate) outbox: Box<dyn PluginOutbox>,
    pub(crate) commands: bool,
    pub(crate) plugin: PluginInfo,
}

#[derive(Default)]
pub(crate) struct Registry {
    pub(crate) attachments: crate::plugins::attachments::Attachments,
    pub(crate) views: crate::plugins::extras::PlayerViews,
    connected: BTreeMap<u64, Connected>,
    last_event: u64,
}

impl Registry {
    pub(crate) fn connect(
        &mut self,
        connection: u64,
        intro: PluginIntro,
        outbox: Box<dyn PluginOutbox>,
        tick: u64,
    ) -> Option<Departed> {
        let taken = self
            .connected
            .iter()
            .find(|(_, plugin)| plugin.intro.name == intro.name)
            .map(|(id, _)| *id);
        let replaced =
            taken.and_then(|id| self.connected.remove(&id).map(|plugin| plugin.departed(id)));
        self.connected.insert(
            connection,
            Connected {
                intro,
                outbox,
                since: tick,
            },
        );
        replaced
    }

    pub(crate) fn disconnect(&mut self, connection: u64) -> Option<(PluginInfo, bool)> {
        self.connected.remove(&connection).map(|plugin| {
            let departed = plugin.departed(connection);
            (departed.plugin, departed.commands)
        })
    }

    pub(crate) fn connected(&self) -> Vec<(u64, PluginInfo)> {
        self.connected
            .iter()
            .map(|(connection, plugin)| (*connection, plugin.info()))
            .collect()
    }

    pub(crate) fn commands(&self) -> impl Iterator<Item = (u64, &CommandSpec)> {
        self.connected.iter().flat_map(|(connection, plugin)| {
            plugin
                .intro
                .commands
                .iter()
                .map(|command| (*connection, command))
        })
    }

    pub(crate) fn add_command(
        &mut self,
        connection: u64,
        command: CommandSpec,
    ) -> Result<(), IntroError> {
        command.validate()?;
        let plugin = self.connected.get_mut(&connection);
        let Some(plugin) = plugin else {
            return Ok(());
        };
        if plugin
            .intro
            .commands
            .iter()
            .any(|known| known.id == command.id)
        {
            return Err(IntroError::DuplicateCommand(command.id));
        }
        plugin.intro.commands.push(command);
        Ok(())
    }

    pub(crate) fn remove_command(&mut self, connection: u64, name: &str) -> bool {
        let Some(plugin) = self.connected.get_mut(&connection) else {
            return false;
        };
        let before = plugin.intro.commands.len();
        plugin
            .intro
            .commands
            .retain(|command| !command.name.eq_ignore_ascii_case(name));
        before != plugin.intro.commands.len()
    }

    pub(crate) fn next_event_id(&mut self) -> u64 {
        self.last_event += 1;
        self.last_event
    }

    pub(crate) fn issued(&self, event: u64) -> bool {
        (1..=self.last_event).contains(&event)
    }

    pub(crate) fn name(&self, connection: u64) -> Option<&str> {
        self.connected
            .get(&connection)
            .map(|plugin| plugin.intro.name.as_str())
    }

    pub(crate) fn is_current(&self, connection: u64) -> bool {
        self.connected.contains_key(&connection)
    }

    pub(crate) fn listens(&self, event: EventKind, access: Access) -> bool {
        let receivers = event.receivers();
        self.connected.values().any(|plugin| {
            plugin
                .intro
                .handlers
                .iter()
                .any(|spec| receivers.contains(&spec.event) && spec.access == access)
        })
    }

    pub(crate) fn listeners(
        &self,
        event: EventKind,
        access: Access,
        settings: &PluginSettings,
    ) -> Vec<(u64, HandlerSpec)> {
        let receivers = event.receivers();
        let mut found = Vec::new();
        for (connection, plugin) in &self.connected {
            let name = plugin.intro.name.as_str();
            let rank = rank(name, settings);
            for spec in &plugin.intro.handlers {
                if receivers.contains(&spec.event) && spec.access == access {
                    let priority = effective_priority(name, spec, settings);
                    found.push(((priority, rank, spec.id, *connection), *spec));
                }
            }
        }
        found.sort_by(|a, b| a.0.cmp(&b.0));
        found
            .into_iter()
            .map(|((_, _, _, connection), spec)| (connection, spec))
            .collect()
    }

    pub(crate) fn send(&self, connection: u64, message: ToPlugin) {
        if let Some(plugin) = self.connected.get(&connection) {
            plugin.outbox.send(message);
        }
    }

    pub(crate) fn summary(&self, settings: &PluginSettings) -> Vec<(String, String, u64)> {
        let mut plugins: Vec<&Connected> = self.connected.values().collect();
        plugins.sort_by(|a, b| rank(&a.intro.name, settings).cmp(&rank(&b.intro.name, settings)));
        plugins
            .into_iter()
            .map(|plugin| {
                (
                    plugin.intro.name.clone(),
                    plugin.intro.version.clone(),
                    plugin.since,
                )
            })
            .collect()
    }
}

fn rank<'a>(name: &'a str, settings: &PluginSettings) -> (usize, &'a str) {
    match settings.order.iter().position(|listed| listed == name) {
        Some(index) => (index, ""),
        None => (settings.order.len(), name),
    }
}

fn effective_priority(plugin: &str, spec: &HandlerSpec, settings: &PluginSettings) -> Priority {
    let own = || {
        settings
            .overrides
            .iter()
            .filter(|candidate| candidate.plugin == plugin)
    };
    own()
        .rfind(|candidate| candidate.event == Some(spec.event))
        .or_else(|| own().rfind(|candidate| candidate.event.is_none()))
        .map_or(spec.priority, |found| found.priority)
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use super::*;
    use crate::config::{PluginSettings, PriorityOverride};
    use crate::plugins::{
        Access, EventKind, HandlerSpec, PluginInfo, PluginIntro, Priority, ToPlugin,
    };
    use crate::ports::PluginOutbox;

    #[derive(Clone, Default)]
    struct Mailbox(Arc<Mutex<Vec<ToPlugin>>>);

    impl Mailbox {
        fn received(&self) -> Vec<ToPlugin> {
            self.0.lock().map(|m| m.clone()).unwrap_or_default()
        }
    }

    impl PluginOutbox for Mailbox {
        fn send(&self, message: ToPlugin) {
            if let Ok(mut inbox) = self.0.lock() {
                inbox.push(message);
            }
        }
    }

    fn handler(id: u32, event: EventKind, access: Access, priority: Priority) -> HandlerSpec {
        HandlerSpec {
            id,
            event,
            access,
            priority,
            ignore_cancelled: false,
        }
    }

    fn intro(name: &str, handlers: Vec<HandlerSpec>) -> PluginIntro {
        PluginIntro {
            commands: Vec::new(),
            name: name.to_owned(),
            version: "1.0.0".to_owned(),
            handlers,
        }
    }

    fn mover(name: &str, priority: Priority) -> PluginIntro {
        intro(
            name,
            vec![handler(1, EventKind::PlayerMove, Access::Observe, priority)],
        )
    }

    fn connect(registry: &mut Registry, connection: u64, intro: PluginIntro) -> Mailbox {
        let mailbox = Mailbox::default();
        registry.connect(connection, intro, Box::new(mailbox.clone()), connection);
        mailbox
    }

    fn sequence(
        registry: &Registry,
        event: EventKind,
        access: Access,
        settings: &PluginSettings,
        names: &[&'static str],
    ) -> Vec<&'static str> {
        registry
            .listeners(event, access, settings)
            .into_iter()
            .map(|(connection, _)| names[connection as usize - 1])
            .collect()
    }

    fn mixed_registry() -> Registry {
        let mut registry = Registry::default();
        for (connection, (name, priority)) in [
            ("b", Priority::High),
            ("a", Priority::Normal),
            ("c", Priority::Normal),
        ]
        .into_iter()
        .enumerate()
        {
            let handlers = vec![
                handler(1, EventKind::PlayerMove, Access::Observe, priority),
                handler(2, EventKind::PlayerJoin, Access::Observe, priority),
            ];
            connect(&mut registry, connection as u64 + 1, intro(name, handlers));
        }
        registry
    }

    const NAMES: [&str; 3] = ["b", "a", "c"];

    #[test]
    fn listeners_follow_priority_then_the_configured_order_then_the_name() {
        let mut registry = Registry::default();
        connect(&mut registry, 1, mover("b", Priority::High));
        connect(&mut registry, 2, mover("a", Priority::Normal));
        connect(&mut registry, 3, mover("c", Priority::Normal));
        let settings = PluginSettings {
            order: vec!["c".to_owned()],
            ..PluginSettings::default()
        };
        assert_eq!(
            sequence(
                &registry,
                EventKind::PlayerMove,
                Access::Observe,
                &settings,
                &NAMES
            ),
            ["c", "a", "b"]
        );
        assert_eq!(
            sequence(
                &registry,
                EventKind::PlayerMove,
                Access::Observe,
                &PluginSettings::default(),
                &NAMES
            ),
            ["a", "c", "b"]
        );
    }

    #[test]
    fn several_listed_plugins_follow_the_order_they_are_listed_in() {
        let mut registry = Registry::default();
        connect(&mut registry, 1, mover("a", Priority::Normal));
        connect(&mut registry, 2, mover("z", Priority::Normal));
        let names = ["a", "z"];
        let listed = PluginSettings {
            order: vec!["z".to_owned(), "a".to_owned()],
            ..PluginSettings::default()
        };
        assert_eq!(
            sequence(
                &registry,
                EventKind::PlayerMove,
                Access::Observe,
                &listed,
                &names
            ),
            ["z", "a"]
        );
        assert_eq!(
            sequence(
                &registry,
                EventKind::PlayerMove,
                Access::Observe,
                &PluginSettings::default(),
                &names
            ),
            ["a", "z"]
        );
    }

    #[test]
    fn handlers_of_one_plugin_at_equal_priority_settle_by_id() {
        let mut registry = Registry::default();
        let handlers = vec![
            handler(5, EventKind::PlayerMove, Access::Observe, Priority::Normal),
            handler(2, EventKind::PlayerMove, Access::Observe, Priority::Normal),
        ];
        connect(&mut registry, 1, intro("welcome", handlers));
        let settled = registry.listeners(
            EventKind::PlayerMove,
            Access::Observe,
            &PluginSettings::default(),
        );
        assert_eq!(
            settled.iter().map(|(c, h)| (*c, h.id)).collect::<Vec<_>>(),
            [(1, 2), (1, 5)]
        );
    }

    #[test]
    fn an_override_changes_a_plugin_or_one_of_its_events() {
        let registry = mixed_registry();
        let whole_plugin = PluginSettings {
            overrides: vec![PriorityOverride {
                plugin: "a".to_owned(),
                event: None,
                priority: Priority::Highest,
            }],
            ..PluginSettings::default()
        };
        for event in [EventKind::PlayerMove, EventKind::PlayerJoin] {
            assert_eq!(
                sequence(&registry, event, Access::Observe, &whole_plugin, &NAMES),
                ["c", "b", "a"]
            );
        }

        let one_event = PluginSettings {
            overrides: vec![PriorityOverride {
                plugin: "a".to_owned(),
                event: Some(EventKind::PlayerJoin),
                priority: Priority::Highest,
            }],
            ..PluginSettings::default()
        };
        assert_eq!(
            sequence(
                &registry,
                EventKind::PlayerMove,
                Access::Observe,
                &one_event,
                &NAMES
            ),
            ["a", "c", "b"]
        );
        assert_eq!(
            sequence(
                &registry,
                EventKind::PlayerJoin,
                Access::Observe,
                &one_event,
                &NAMES
            ),
            ["c", "b", "a"]
        );
    }

    #[test]
    fn an_override_for_one_event_wins_over_an_override_for_the_whole_plugin() {
        let registry = mixed_registry();
        let settings = PluginSettings {
            overrides: vec![
                PriorityOverride {
                    plugin: "a".to_owned(),
                    event: Some(EventKind::PlayerMove),
                    priority: Priority::Lowest,
                },
                PriorityOverride {
                    plugin: "a".to_owned(),
                    event: None,
                    priority: Priority::Highest,
                },
            ],
            ..PluginSettings::default()
        };
        assert_eq!(
            sequence(
                &registry,
                EventKind::PlayerMove,
                Access::Observe,
                &settings,
                &NAMES
            ),
            ["a", "c", "b"]
        );
        assert_eq!(
            sequence(
                &registry,
                EventKind::PlayerJoin,
                Access::Observe,
                &settings,
                &NAMES
            ),
            ["c", "b", "a"]
        );
    }

    #[test]
    fn listeners_keep_only_the_requested_event_and_access() {
        let mut registry = Registry::default();
        let handlers = vec![
            handler(3, EventKind::PlayerMove, Access::Observe, Priority::Normal),
            handler(1, EventKind::PlayerMove, Access::Decide, Priority::Normal),
            handler(2, EventKind::PlayerJoin, Access::Observe, Priority::Normal),
            handler(4, EventKind::PlayerMove, Access::Observe, Priority::Normal),
        ];
        connect(&mut registry, 7, intro("welcome", handlers));
        let settings = PluginSettings::default();
        let observers = registry.listeners(EventKind::PlayerMove, Access::Observe, &settings);
        assert_eq!(
            observers
                .iter()
                .map(|(c, h)| (*c, h.id))
                .collect::<Vec<_>>(),
            [(7, 3), (7, 4)]
        );
        let deciders = registry.listeners(EventKind::PlayerMove, Access::Decide, &settings);
        assert_eq!(
            deciders.iter().map(|(c, h)| (*c, h.id)).collect::<Vec<_>>(),
            [(7, 1)]
        );
        assert!(
            registry
                .listeners(EventKind::PlayerLeave, Access::Observe, &settings)
                .is_empty()
        );
        assert!(registry.listens(EventKind::PlayerMove, Access::Decide));
        assert!(registry.listens(EventKind::PlayerJoin, Access::Observe));
        assert!(!registry.listens(EventKind::PlayerJoin, Access::Decide));
        assert!(!registry.listens(EventKind::PlayerLeave, Access::Observe));
    }

    #[test]
    fn a_plugin_with_a_taken_name_replaces_the_old_connection() {
        let mut registry = Registry::default();
        let first = Mailbox::default();
        let replaced = registry.connect(
            1,
            mover("welcome", Priority::Normal),
            Box::new(first.clone()),
            5,
        );
        assert!(replaced.is_none());
        let second = Mailbox::default();
        let replaced = registry.connect(
            2,
            mover("welcome", Priority::Normal),
            Box::new(second.clone()),
            9,
        );
        let replaced = replaced.expect("the first outbox is handed back");
        assert_eq!(replaced.connection, 1);
        assert_eq!(
            replaced.plugin,
            PluginInfo {
                name: "welcome".to_owned(),
                version: "1.0.0".to_owned(),
            }
        );
        replaced.outbox.send(ToPlugin::Replaced);
        assert_eq!(first.received(), [ToPlugin::Replaced]);
        assert!(second.received().is_empty());
        assert!(!registry.is_current(1));
        assert!(registry.is_current(2));
        assert_eq!(
            registry.summary(&PluginSettings::default()),
            [("welcome".to_owned(), "1.0.0".to_owned(), 9)]
        );
        let settings = PluginSettings::default();
        assert_eq!(
            registry
                .listeners(EventKind::PlayerMove, Access::Observe, &settings)
                .len(),
            1
        );
    }

    #[test]
    fn disconnecting_a_replaced_connection_keeps_the_new_one() {
        let mut registry = Registry::default();
        connect(&mut registry, 1, mover("welcome", Priority::Normal));
        connect(&mut registry, 2, mover("welcome", Priority::Normal));
        assert_eq!(registry.disconnect(1), None);
        assert!(registry.is_current(2));
        assert_eq!(registry.summary(&PluginSettings::default()).len(), 1);
        assert_eq!(
            registry.disconnect(2),
            Some((
                PluginInfo {
                    name: "welcome".to_owned(),
                    version: "1.0.0".to_owned(),
                },
                false
            ))
        );
        assert!(!registry.is_current(2));
        assert!(registry.summary(&PluginSettings::default()).is_empty());
        assert_eq!(registry.disconnect(2), None);
    }

    #[test]
    fn a_disconnected_plugin_loses_its_handlers() {
        let mut registry = Registry::default();
        connect(&mut registry, 1, mover("welcome", Priority::Normal));
        connect(&mut registry, 2, mover("guard", Priority::Normal));
        assert!(registry.disconnect(1).is_some());
        let settings = PluginSettings::default();
        let remaining = registry.listeners(EventKind::PlayerMove, Access::Observe, &settings);
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].0, 2);
    }

    #[test]
    fn the_summary_lists_plugins_in_the_configured_order_then_by_name() {
        let mut registry = Registry::default();
        connect(&mut registry, 1, mover("zeta", Priority::Normal));
        connect(&mut registry, 2, mover("beta", Priority::Normal));
        connect(&mut registry, 3, mover("alpha", Priority::Normal));
        let settings = PluginSettings {
            order: vec!["zeta".to_owned(), "missing".to_owned()],
            ..PluginSettings::default()
        };
        let names: Vec<String> = registry
            .summary(&settings)
            .into_iter()
            .map(|(name, _, _)| name)
            .collect();
        assert_eq!(names, ["zeta", "alpha", "beta"]);
    }

    #[test]
    fn the_connected_plugins_are_listed_in_connection_order() {
        let mut registry = Registry::default();
        connect(&mut registry, 2, mover("zeta", Priority::Normal));
        connect(&mut registry, 5, mover("alpha", Priority::Normal));
        connect(&mut registry, 9, mover("beta", Priority::Normal));
        registry.disconnect(5);
        let listed: Vec<(u64, String)> = registry
            .connected()
            .into_iter()
            .map(|(connection, plugin)| (connection, plugin.name))
            .collect();
        assert_eq!(listed, [(2, "zeta".to_owned()), (9, "beta".to_owned())]);
    }

    #[test]
    fn event_ids_start_at_one_and_never_repeat() {
        let mut registry = Registry::default();
        assert!(!registry.issued(1));
        assert_eq!(registry.next_event_id(), 1);
        assert_eq!(registry.next_event_id(), 2);
        assert!(registry.issued(1) && registry.issued(2));
        assert!(!registry.issued(0) && !registry.issued(3));
        connect(&mut registry, 1, mover("welcome", Priority::Normal));
        registry.disconnect(1);
        assert_eq!(registry.next_event_id(), 3);
    }

    #[test]
    fn a_message_reaches_only_the_addressed_connection() {
        let mut registry = Registry::default();
        let welcome = connect(&mut registry, 1, mover("welcome", Priority::Normal));
        let guard = connect(&mut registry, 2, mover("guard", Priority::Normal));
        registry.send(2, ToPlugin::Accepted);
        registry.send(9, ToPlugin::Replaced);
        assert!(welcome.received().is_empty());
        assert_eq!(guard.received(), [ToPlugin::Accepted]);
    }

    #[test]
    fn a_fired_event_reaches_the_handlers_of_its_family_in_registry_order() {
        let mut registry = Registry::default();
        let handlers = vec![
            handler(
                1,
                EventKind::EntityDamageByEntity,
                Access::Decide,
                Priority::Normal,
            ),
            handler(2, EventKind::EntityDamage, Access::Decide, Priority::Low),
            handler(3, EventKind::PlayerMove, Access::Observe, Priority::Normal),
            handler(
                4,
                EventKind::PlayerTeleport,
                Access::Observe,
                Priority::Normal,
            ),
        ];
        connect(&mut registry, 1, intro("guard", handlers));
        let settings = PluginSettings::default();
        let ids = |event, access| {
            registry
                .listeners(event, access, &settings)
                .into_iter()
                .map(|(_, spec)| (spec.id, spec.event))
                .collect::<Vec<_>>()
        };
        assert_eq!(
            ids(EventKind::EntityDamageByEntity, Access::Decide),
            [
                (2, EventKind::EntityDamage),
                (1, EventKind::EntityDamageByEntity)
            ]
        );
        assert_eq!(
            ids(EventKind::EntityDamage, Access::Decide),
            [(2, EventKind::EntityDamage)]
        );
        assert_eq!(
            ids(EventKind::PlayerTeleport, Access::Observe),
            [(4, EventKind::PlayerTeleport)]
        );
        assert_eq!(
            ids(EventKind::PlayerMove, Access::Observe),
            [(3, EventKind::PlayerMove)]
        );
        assert!(registry.listens(EventKind::EntityDamageByBlock, Access::Decide));
        assert!(!registry.listens(EventKind::EntityDamageByBlock, Access::Observe));
    }
}

use rspigot_domain::text::{Component, NamedColor};

pub const GROUP_MINECRAFT: &str = "Minecraft";
pub const GROUP_BUKKIT: &str = "Bukkit";
pub const PLAYER_PAGE: usize = 9;
pub const CONSOLE_PAGE: usize = 19;
pub const DEFAULT_INDEX: &str = "Default";
pub const IGNORE_ALL: &str = "All";
const RULE: &str = "---------";
const TAIL: &str = "-----------------------";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TopicKind {
    #[default]
    Command,
    General,
    Index,
    Group,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct HelpTopic {
    pub name: String,
    pub short: String,
    pub full: String,
    pub group: Option<String>,
    pub permission: Option<String>,
    pub commands: Vec<String>,
    pub kind: TopicKind,
}

impl HelpTopic {
    pub fn command(name: &str, short: &str, group: &str) -> Self {
        Self {
            name: format!("/{}", name.trim_start_matches('/')),
            short: short.to_owned(),
            full: short.to_owned(),
            group: Some(group.to_owned()),
            ..Self::default()
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Amendment {
    pub name: String,
    pub short: Option<String>,
    pub full: Option<String>,
    pub permission: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct HelpConfig {
    pub command_topics_in_master_index: bool,
    pub general: Vec<HelpTopic>,
    pub indexes: Vec<HelpTopic>,
    pub amended: Vec<Amendment>,
    pub ignore_plugins: Vec<String>,
}

const ALIASES: &[&str] = &["?", "ver", "about", "pl", "tm", "w", "tell", "xp", "tp"];

fn bukkit_short(label: &str) -> Option<&'static str> {
    Some(match label {
        "help" => "Shows the help menu",
        "version" => "Gets the version of this server including any plugins in use",
        "plugins" => "Gets a list of plugins running on the server",
        "reload" => "Reloads the server configuration and plugins",
        "tps" => "Gets the current ticks per second for the server",
        "timings" => "Records timings for all plugin events",
        "restart" => "Restarts the server",
        "spigot" => "Spigot related commands",
        _ => return None,
    })
}

pub(crate) fn builtin_topics(level: u8, decision: impl Fn(&str) -> Option<bool>) -> Vec<HelpTopic> {
    super::permission::labels()
        .filter(|label| !ALIASES.contains(label))
        .filter(|label| {
            let node = super::permission::node_of(label);
            super::permission::allowed(label, level, node.and_then(|node| decision(&node)))
        })
        .map(|label| match bukkit_short(label) {
            Some(short) => HelpTopic::command(label, short, GROUP_BUKKIT),
            None => HelpTopic::command(label, "A Mojang provided command.", GROUP_MINECRAFT),
        })
        .collect()
}

pub fn legacy(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '&'
            && chars
                .peek()
                .is_some_and(|next| next.is_ascii_hexdigit() || "klmnorKLMNOR".contains(*next))
        {
            out.push('§');
        } else {
            out.push(c);
        }
    }
    out
}

fn header(title: &str) -> Component {
    Component::text(format!("{RULE} "))
        .color(NamedColor::Yellow)
        .append(Component::text(format!("{title} ")).color(NamedColor::White))
        .append(Component::text(TAIL).color(NamedColor::Yellow))
}

fn entry(name: &str, short: &str) -> Component {
    Component::text(format!("{name}: "))
        .color(NamedColor::Gold)
        .append(Component::text(legacy(short)).color(NamedColor::White))
}

fn ignored(config: &HelpConfig, group: &str) -> bool {
    config
        .ignore_plugins
        .iter()
        .any(|name| name == IGNORE_ALL || name.eq_ignore_ascii_case(group))
}

fn amended(mut topic: HelpTopic, config: &HelpConfig) -> HelpTopic {
    let Some(change) = config
        .amended
        .iter()
        .find(|change| change.name.eq_ignore_ascii_case(&topic.name))
    else {
        return topic;
    };
    if let Some(short) = &change.short {
        topic.short = short.replace("<text>", &topic.short);
    }
    if let Some(full) = &change.full {
        topic.full = full.replace("<text>", &topic.full);
    }
    if change.permission.is_some() {
        topic.permission.clone_from(&change.permission);
    }
    topic
}

pub fn visible_topics(
    commands: Vec<HelpTopic>,
    config: &HelpConfig,
    allows: impl Fn(&str) -> bool,
) -> Vec<HelpTopic> {
    let permitted = |topic: &HelpTopic| topic.permission.as_deref().is_none_or(&allows);
    let mut topics: Vec<HelpTopic> = Vec::new();
    for command in commands {
        let group_ignored = command
            .group
            .as_deref()
            .is_some_and(|group| ignored(config, group));
        if group_ignored {
            continue;
        }
        let command = amended(command, config);
        if permitted(&command) {
            topics.push(command);
        }
    }
    let mut groups: Vec<String> = topics.iter().filter_map(|t| t.group.clone()).collect();
    groups.sort_by_key(|group| group.to_lowercase());
    groups.dedup();
    for group in groups {
        let members: Vec<String> = topics
            .iter()
            .filter(|t| t.group.as_deref() == Some(&group))
            .map(|t| t.name.clone())
            .collect();
        topics.push(HelpTopic {
            short: format!("All commands for {group}"),
            full: format!("All commands for {group}"),
            commands: members,
            name: group,
            kind: TopicKind::Group,
            ..HelpTopic::default()
        });
    }
    for general in &config.general {
        if permitted(general) {
            topics.push(general.clone());
        }
    }
    for index in &config.indexes {
        if permitted(index) {
            topics.push(index.clone());
        }
    }
    topics
}

fn find<'a>(topics: &'a [HelpTopic], name: &str) -> Option<&'a HelpTopic> {
    let bare = name.trim_start_matches('/');
    topics.iter().find(|topic| {
        topic.name.eq_ignore_ascii_case(name)
            || (topic.kind == TopicKind::Command
                && topic
                    .name
                    .trim_start_matches('/')
                    .eq_ignore_ascii_case(bare))
    })
}

fn paginate(lines: Vec<Component>, page: usize, height: usize) -> (Vec<Component>, usize, usize) {
    let height = height.max(1);
    let pages = lines.len().div_ceil(height).max(1);
    let page = page.clamp(1, pages);
    let shown = lines
        .into_iter()
        .skip((page - 1) * height)
        .take(height)
        .collect();
    (shown, page, pages)
}

fn listing(
    title: &str,
    preamble: Option<Component>,
    rows: Vec<Component>,
    page: usize,
    height: usize,
) -> Vec<Component> {
    let (shown, page, pages) = paginate(rows, page, height);
    let mut out = vec![header(&format!("Help: {title} ({page}/{pages})"))];
    out.extend(preamble);
    out.extend(shown);
    out
}

fn master_rows(topics: &[HelpTopic], config: &HelpConfig) -> Vec<Component> {
    let mut shown: Vec<&HelpTopic> = topics
        .iter()
        .filter(|topic| topic.kind != TopicKind::Command || config.command_topics_in_master_index)
        .collect();
    shown.sort_by_key(|topic| topic.name.to_lowercase());
    shown
        .into_iter()
        .map(|topic| entry(&topic.name, &topic.short))
        .collect()
}

fn index_rows(topic: &HelpTopic, topics: &[HelpTopic]) -> Vec<Component> {
    topic
        .commands
        .iter()
        .filter_map(|name| find(topics, name))
        .map(|member| entry(&member.name, &member.short))
        .collect()
}

pub fn help_lines(
    request: &str,
    topics: &[HelpTopic],
    config: &HelpConfig,
    height: usize,
    usage: impl Fn(&str) -> Vec<String>,
) -> Result<Vec<Component>, String> {
    let request = request.trim();
    let hint = Component::text("Use /help [n] to get page n of help.").color(NamedColor::Gold);
    let default_index = topics
        .iter()
        .find(|topic| topic.kind == TopicKind::Index && topic.name == DEFAULT_INDEX);
    let master = |page: usize| match default_index {
        Some(custom) => listing(
            "Index",
            Some(hint.clone()),
            index_rows(custom, topics),
            page,
            height,
        ),
        None => listing(
            "Index",
            Some(hint.clone()),
            master_rows(topics, config),
            page,
            height,
        ),
    };
    if request.is_empty() {
        return Ok(master(1));
    }
    if let Ok(page) = request.parse::<usize>() {
        return Ok(master(page));
    }
    let (name, page) = match request.rsplit_once(' ') {
        Some((name, last)) if last.parse::<usize>().is_ok() && find(topics, name).is_some() => {
            (name, last.parse::<usize>().unwrap_or(1))
        }
        _ => (request, 1),
    };
    let Some(topic) = find(topics, name) else {
        return Err(format!("No help for {request}"));
    };
    Ok(match topic.kind {
        TopicKind::Group => listing(&topic.name, None, index_rows(topic, topics), page, height),
        TopicKind::Index => listing(
            &topic.name,
            (!topic.full.is_empty())
                .then(|| Component::text(legacy(&topic.full)).color(NamedColor::Gold)),
            index_rows(topic, topics),
            page,
            height,
        ),
        TopicKind::General => vec![
            header(&format!("Help: {}", topic.name)),
            Component::text(legacy(if topic.full.is_empty() {
                &topic.short
            } else {
                &topic.full
            })),
        ],
        TopicKind::Command => {
            let mut out = vec![
                header(&format!("Help: {}", topic.name)),
                entry("Description", &topic.short),
            ];
            if !topic.full.is_empty() && topic.full != topic.short {
                out.push(Component::text(legacy(&topic.full)));
            }
            let lines = usage(topic.name.trim_start_matches('/'));
            if !lines.is_empty() {
                out.push(entry("Usage", &lines.join(", ")));
            }
            out
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn commands() -> Vec<HelpTopic> {
        let mut topics: Vec<HelpTopic> = (0..12)
            .map(|n| {
                HelpTopic::command(
                    &format!("cmd{n:02}"),
                    "A Mojang provided command.",
                    GROUP_MINECRAFT,
                )
            })
            .collect();
        topics.push(HelpTopic::command(
            "version",
            "Gets the version",
            GROUP_BUKKIT,
        ));
        topics.push(HelpTopic::command("shop", "Open the shop", "Shopkeeper"));
        topics
    }

    fn texts(lines: &[Component]) -> Vec<String> {
        lines.iter().map(Component::plain_text).collect()
    }

    fn no_usage(_: &str) -> Vec<String> {
        Vec::new()
    }

    #[test]
    fn the_index_lists_one_topic_per_plugin_group() {
        let config = HelpConfig::default();
        let topics = visible_topics(commands(), &config, |_| true);
        let lines = help_lines("", &topics, &config, PLAYER_PAGE, no_usage).unwrap();
        let text = texts(&lines);
        assert!(text[0].contains("Help: Index (1/1)"));
        assert!(
            text.iter()
                .any(|l| l.starts_with("Shopkeeper: All commands for Shopkeeper"))
        );
        assert!(text.iter().any(|l| l.starts_with("Bukkit: ")));
        assert!(text.iter().any(|l| l.starts_with("Minecraft: ")));
        assert!(!text.iter().any(|l| l.starts_with("/cmd00")));
    }

    #[test]
    fn command_topics_join_the_master_index_when_asked_and_paginate() {
        let config = HelpConfig {
            command_topics_in_master_index: true,
            ..HelpConfig::default()
        };
        let topics = visible_topics(commands(), &config, |_| true);
        let first = texts(&help_lines("1", &topics, &config, PLAYER_PAGE, no_usage).unwrap());
        let second = texts(&help_lines("2", &topics, &config, PLAYER_PAGE, no_usage).unwrap());
        assert!(first[0].contains("(1/"));
        assert!(second[0].contains("(2/"));
        assert_ne!(first[2..], second[2..]);
        assert_eq!(first.len(), PLAYER_PAGE + 2);
    }

    #[test]
    fn a_page_past_the_end_is_clamped() {
        let config = HelpConfig::default();
        let topics = visible_topics(commands(), &config, |_| true);
        let lines = texts(&help_lines("99", &topics, &config, PLAYER_PAGE, no_usage).unwrap());
        assert!(lines[0].contains("(1/1)"));
    }

    #[test]
    fn a_command_topic_shows_description_and_usage() {
        let config = HelpConfig::default();
        let topics = visible_topics(commands(), &config, |_| true);
        let lines = texts(
            &help_lines("/version", &topics, &config, PLAYER_PAGE, |name| {
                vec![format!("/{name} [plugin]")]
            })
            .unwrap(),
        );
        assert!(lines[0].contains("Help: /version"));
        assert!(lines.iter().any(|l| l == "Description: Gets the version"));
        assert!(lines.iter().any(|l| l == "Usage: /version [plugin]"));
        let bare = help_lines("version", &topics, &config, PLAYER_PAGE, no_usage);
        assert!(bare.is_ok());
    }

    #[test]
    fn a_group_topic_lists_its_commands() {
        let config = HelpConfig::default();
        let topics = visible_topics(commands(), &config, |_| true);
        let lines =
            texts(&help_lines("Shopkeeper", &topics, &config, PLAYER_PAGE, no_usage).unwrap());
        assert!(lines.iter().any(|l| l == "/shop: Open the shop"));
    }

    #[test]
    fn an_unknown_topic_is_reported() {
        let config = HelpConfig::default();
        let topics = visible_topics(commands(), &config, |_| true);
        assert_eq!(
            help_lines("nothing", &topics, &config, PLAYER_PAGE, no_usage),
            Err("No help for nothing".to_owned())
        );
    }

    #[test]
    fn ignored_plugins_and_all_remove_the_generated_topics() {
        let config = HelpConfig {
            ignore_plugins: vec!["Bukkit".to_owned()],
            ..HelpConfig::default()
        };
        let topics = visible_topics(commands(), &config, |_| true);
        assert!(find(&topics, "/version").is_none());
        assert!(find(&topics, "/shop").is_some());
        let none = HelpConfig {
            ignore_plugins: vec![IGNORE_ALL.to_owned()],
            ..HelpConfig::default()
        };
        assert!(visible_topics(commands(), &none, |_| true).is_empty());
    }

    #[test]
    fn permissions_hide_topics_and_amendments_rewrite_them() {
        let config = HelpConfig {
            general: vec![HelpTopic {
                name: "Rules".to_owned(),
                short: "Rules of the server".to_owned(),
                full: "&61. Be kind.".to_owned(),
                permission: Some("topics.rules".to_owned()),
                kind: TopicKind::General,
                ..HelpTopic::default()
            }],
            amended: vec![Amendment {
                name: "/shop".to_owned(),
                short: Some("<text> today".to_owned()),
                full: None,
                permission: None,
            }],
            ..HelpConfig::default()
        };
        let hidden = visible_topics(commands(), &config, |node| node != "topics.rules");
        assert!(find(&hidden, "Rules").is_none());
        let shown = visible_topics(commands(), &config, |_| true);
        let rules = texts(&help_lines("Rules", &shown, &config, PLAYER_PAGE, no_usage).unwrap());
        assert!(rules.iter().any(|l| l.contains("§61. Be kind.")));
        assert_eq!(find(&shown, "/shop").unwrap().short, "Open the shop today");
    }

    #[test]
    fn a_default_index_topic_replaces_the_master_index() {
        let config = HelpConfig {
            indexes: vec![HelpTopic {
                name: DEFAULT_INDEX.to_owned(),
                short: "Mine".to_owned(),
                commands: vec!["/shop".to_owned()],
                kind: TopicKind::Index,
                ..HelpTopic::default()
            }],
            ..HelpConfig::default()
        };
        let topics = visible_topics(commands(), &config, |_| true);
        let lines = texts(&help_lines("", &topics, &config, PLAYER_PAGE, no_usage).unwrap());
        assert!(lines.iter().any(|l| l == "/shop: Open the shop"));
        assert!(!lines.iter().any(|l| l.starts_with("Minecraft")));
    }

    #[test]
    fn color_codes_use_the_ampersand_form() {
        assert_eq!(legacy("&6gold &bAqua &zplain"), "§6gold §bAqua &zplain");
    }
}

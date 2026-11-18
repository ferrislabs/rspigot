use std::collections::HashSet;

use crate::model::{ActiveRun, TeamAgent};

pub const CAPTAIN_NAME: &str = "Capitaine";
pub const MAX_NPC_NAME: usize = 16;
const BAR_WIDTH: u32 = 5;
const TITLE_WIDTH: usize = 28;

pub fn npc_name(source: &str, taken: &HashSet<String>) -> String {
    let base: String = source
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect();
    let base = if base.trim_matches('_').is_empty() {
        "Agent".to_owned()
    } else {
        base
    };
    let fits = |name: &str| name.chars().count() <= MAX_NPC_NAME;
    let clipped: String = base.chars().take(MAX_NPC_NAME).collect();
    if !taken.contains(&clipped.to_ascii_lowercase()) {
        return clipped;
    }
    (2u32..)
        .map(|n| {
            let suffix = n.to_string();
            let room = MAX_NPC_NAME - suffix.len();
            format!("{}{suffix}", base.chars().take(room).collect::<String>())
        })
        .find(|name| fits(name) && !taken.contains(&name.to_ascii_lowercase()))
        .unwrap_or(clipped)
}

pub fn click_slug(slug: &str) -> String {
    slug.chars()
        .map(|c| {
            let c = c.to_ascii_lowercase();
            if c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_' {
                c
            } else {
                '-'
            }
        })
        .collect()
}

pub fn step_name(kind: &str) -> &str {
    match kind {
        "extract" => "extraction",
        "deduplicate" => "dédoublonnage",
        "write" => "rédaction",
        "execute" => "exécution",
        "intervene" => "intervention",
        "agent_task" => "tâche",
        other => other,
    }
}

pub fn progress_bar(done: u32, total: u32) -> String {
    let filled = if total == 0 || done == 0 {
        0
    } else if done >= total {
        BAR_WIDTH
    } else {
        (done * BAR_WIDTH).div_ceil(total).clamp(1, BAR_WIDTH - 1)
    };
    format!(
        "§a{}§8{}",
        "▰".repeat(filled as usize),
        "▱".repeat((BAR_WIDTH - filled) as usize)
    )
}

fn clip(text: &str) -> String {
    if text.chars().count() <= TITLE_WIDTH {
        return text.to_owned();
    }
    let head: String = text.chars().take(TITLE_WIDTH - 1).collect();
    format!("{head}…")
}

pub fn agent_label(agent: &TeamAgent, run: Option<&ActiveRun>) -> String {
    let head = format!("§l{}", agent.name);
    if agent.paused {
        return format!("{head}\n§cEn pause");
    }
    let Some(run) = run else {
        return format!("{head}\n§7Disponible");
    };
    let mut lines = vec![head];
    let step = run
        .current_step
        .as_ref()
        .map(|step| format!(" §7· {}", step_name(&step.kind)))
        .unwrap_or_default();
    let state = if run.run.status == "queued" {
        "§eEn attente".to_owned()
    } else {
        format!("§eEn cours{step}")
    };
    lines.push(state);
    if run.steps_total > 0 {
        lines.push(format!(
            "{} §7{}/{}",
            progress_bar(run.steps_done, run.steps_total),
            run.steps_done,
            run.steps_total
        ));
    }
    if let Some(title) = run.run.title.as_deref().filter(|title| !title.is_empty()) {
        lines.push(format!("§f{}", clip(title)));
    }
    lines.join("\n")
}

pub fn captain_label(online: bool) -> String {
    if online {
        format!("§6§l{CAPTAIN_NAME}\n§7Clic droit pour discuter")
    } else {
        format!("§6§l{CAPTAIN_NAME}\n§cCapitainerie injoignable")
    }
}

pub fn is_working(run: Option<&ActiveRun>) -> bool {
    run.is_some_and(|run| run.run.status == "running")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{AgentRef, CurrentStep, RunSummary};

    fn agent(name: &str, paused: bool) -> TeamAgent {
        TeamAgent {
            id: "a1".to_owned(),
            slug: "scribe".to_owned(),
            name: name.to_owned(),
            role: None,
            archived: false,
            paused,
        }
    }

    fn run(
        status: &str,
        step: Option<&str>,
        done: u32,
        total: u32,
        title: Option<&str>,
    ) -> ActiveRun {
        ActiveRun {
            run: RunSummary {
                id: "r1".to_owned(),
                status: status.to_owned(),
                title: title.map(str::to_owned),
                card_id: None,
                agent: Some(AgentRef {
                    id: "a1".to_owned(),
                    name: "Scribe".to_owned(),
                }),
            },
            current_step: step.map(|kind| CurrentStep {
                kind: kind.to_owned(),
            }),
            steps_done: done,
            steps_total: total,
        }
    }

    #[test]
    fn a_name_is_word_characters_within_sixteen() {
        let none = HashSet::new();
        assert_eq!(npc_name("scribe", &none), "scribe");
        assert_eq!(npc_name("revue de code", &none), "revue_de_code");
        assert_eq!(
            npc_name("un-nom-vraiment-trop-long", &none),
            "un_nom_vraiment_"
        );
        assert_eq!(npc_name("é", &none), "Agent");
        assert_eq!(npc_name("", &none), "Agent");
    }

    #[test]
    fn a_taken_name_gets_a_number_that_still_fits() {
        let taken: HashSet<String> = ["scribe", "scribe2"].map(str::to_owned).into();
        assert_eq!(npc_name("Scribe", &taken), "Scribe3");
        let long: HashSet<String> = ["un_nom_vraiment_".to_owned()].into();
        let name = npc_name("un-nom-vraiment-trop-long", &long);
        assert_eq!(name, "un_nom_vraiment2");
        assert!(name.chars().count() <= MAX_NPC_NAME);
    }

    #[test]
    fn a_click_slug_is_a_valid_identifier_path() {
        assert_eq!(click_slug("Revue De Code!"), "revue-de-code-");
        assert_eq!(click_slug("scribe_2"), "scribe_2");
    }

    #[test]
    fn the_bar_fills_with_progress_and_never_looks_finished_early() {
        assert_eq!(progress_bar(0, 3), "§a§8▱▱▱▱▱");
        assert_eq!(progress_bar(1, 3), "§a▰▰§8▱▱▱");
        assert_eq!(progress_bar(2, 3), "§a▰▰▰▰§8▱");
        assert_eq!(progress_bar(3, 3), "§a▰▰▰▰▰§8");
        assert_eq!(progress_bar(0, 0), "§a§8▱▱▱▱▱");
    }

    #[test]
    fn an_idle_agent_is_available_and_a_paused_one_says_so() {
        assert_eq!(
            agent_label(&agent("Scribe", false), None),
            "§lScribe\n§7Disponible"
        );
        assert_eq!(
            agent_label(&agent("Scribe", true), None),
            "§lScribe\n§cEn pause"
        );
    }

    #[test]
    fn a_running_agent_shows_step_progress_and_task() {
        let label = agent_label(
            &agent("Scribe", false),
            Some(&run("running", Some("extract"), 1, 3, Some("Compte rendu"))),
        );
        assert_eq!(
            label,
            "§lScribe\n§eEn cours §7· extraction\n§a▰▰§8▱▱▱ §71/3\n§fCompte rendu"
        );
        let queued = agent_label(
            &agent("Scribe", false),
            Some(&run("queued", None, 0, 3, None)),
        );
        assert!(queued.contains("§eEn attente"));
        assert!(!queued.contains("§f"));
    }

    #[test]
    fn a_long_title_is_clipped() {
        let title = "x".repeat(60);
        let label = agent_label(
            &agent("S", false),
            Some(&run("running", None, 0, 0, Some(&title))),
        );
        let last = label.lines().last().unwrap().to_owned();
        assert_eq!(last.chars().count(), 2 + TITLE_WIDTH);
        assert!(last.ends_with('…'));
    }

    #[test]
    fn only_a_running_run_counts_as_working() {
        assert!(is_working(Some(&run("running", None, 0, 1, None))));
        assert!(!is_working(Some(&run("queued", None, 0, 1, None))));
        assert!(!is_working(None));
    }
}

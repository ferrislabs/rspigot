pub const SCAN_DEPTH: i32 = 120;
pub const LOBBY_SCAN_LIFT: i32 = 20;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Probe {
    Air,
    Solid(String),
    Unloaded,
}

impl Probe {
    pub fn of_key(key: &str) -> Self {
        match key {
            "minecraft:air" | "minecraft:cave_air" | "minecraft:void_air" => Self::Air,
            other => Self::Solid(other.to_owned()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Descent {
    Hit { y: i32, depth: i32, block: String },
    Empty,
    Unloaded { y: i32 },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Column {
    pub feet: Probe,
    pub below: Probe,
    pub ground: Descent,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Probes {
    pub player: Option<Column>,
    pub lobby: Descent,
    pub lobby_top: i32,
}

fn probe_text(probe: &Probe) -> String {
    match probe {
        Probe::Air => "§7air".to_owned(),
        Probe::Solid(key) => format!("§f{key}"),
        Probe::Unloaded => "§cchunk non chargé".to_owned(),
    }
}

fn descent_lines(label: &str, descent: &Descent, reference: &str) -> Vec<String> {
    match descent {
        Descent::Hit { y, depth, block } => vec![format!(
            "§e{label} : §f{block} §7en y={y} ({depth} blocs sous {reference})"
        )],
        Descent::Empty => vec![format!(
            "§c{label} : aucun bloc sur {SCAN_DEPTH} blocs sous {reference}"
        )],
        Descent::Unloaded { y } => vec![format!(
            "§c{label} : chunk non chargé en y={y}, le serveur n'a pas ces blocs"
        )],
    }
}

pub fn describe(
    player_world: &str,
    arena_world: &str,
    position: (f64, f64, f64),
    probes: &Probes,
) -> Vec<String> {
    let mut lines = Vec::new();
    if player_world == arena_world {
        lines.push("§aVous êtes dans l'arène".to_owned());
    } else {
        lines.push("§cVous n'êtes PAS dans le monde de l'arène".to_owned());
    }
    lines.push(format!(
        "§7Monde : §f{player_world} §7(arène : §f{arena_world}§7)"
    ));
    lines.push(format!(
        "§7Position : §f{:.1} {:.1} {:.1}",
        position.0, position.1, position.2
    ));
    match &probes.player {
        Some(column) => {
            lines.push(format!("§7Pieds : {}", probe_text(&column.feet)));
            lines.push(format!("§7Dessous : {}", probe_text(&column.below)));
            lines.extend(descent_lines("Sol", &column.ground, "vos pieds"));
        }
        None => lines.push("§7Colonne du joueur non sondée (hors arène)".to_owned()),
    }
    lines.extend(descent_lines(
        "Lobby",
        &probes.lobby,
        &format!("y={}", probes.lobby_top),
    ));
    let unloaded = probes
        .player
        .iter()
        .any(|c| c.feet == Probe::Unloaded || c.below == Probe::Unloaded)
        || matches!(
            probes.player.as_ref().map(|c| &c.ground),
            Some(Descent::Unloaded { .. })
        )
        || matches!(probes.lobby, Descent::Unloaded { .. });
    if unloaded {
        lines.push("§cUn chunk sondé n'est pas chargé côté serveur".to_owned());
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    fn column(ground: Descent) -> Column {
        Column {
            feet: Probe::Air,
            below: Probe::Solid("minecraft:stone".to_owned()),
            ground,
        }
    }

    fn lobby_hit() -> Descent {
        Descent::Hit {
            y: 155,
            depth: 21,
            block: "minecraft:iron_block".to_owned(),
        }
    }

    fn probes(player: Option<Column>, lobby: Descent) -> Probes {
        Probes {
            player,
            lobby,
            lobby_top: 176,
        }
    }

    fn joined(lines: &[String]) -> String {
        lines.join("\n")
    }

    #[test]
    fn in_arena_verdict() {
        let lines = describe(
            "w:arena",
            "w:arena",
            (1.0, 2.0, 3.0),
            &probes(Some(column(Descent::Empty)), lobby_hit()),
        );
        assert_eq!(lines[0], "§aVous êtes dans l'arène");
    }

    #[test]
    fn not_in_arena_verdict() {
        let lines = describe(
            "minecraft:overworld",
            "w:arena",
            (1.0, 2.0, 3.0),
            &probes(None, lobby_hit()),
        );
        assert_eq!(lines[0], "§cVous n'êtes PAS dans le monde de l'arène");
        assert!(joined(&lines).contains("minecraft:overworld"));
        assert!(joined(&lines).contains("hors arène"));
    }

    #[test]
    fn coordinates_are_rounded_to_one_decimal() {
        let lines = describe(
            "a",
            "a",
            (-46.54, 156.0, 24.46),
            &probes(Some(column(Descent::Empty)), lobby_hit()),
        );
        assert!(joined(&lines).contains("-46.5 156.0 24.5"));
    }

    #[test]
    fn solid_found_at_a_depth() {
        let ground = Descent::Hit {
            y: 150,
            depth: 6,
            block: "minecraft:stone".to_owned(),
        };
        let lines = describe(
            "a",
            "a",
            (0.0, 156.0, 0.0),
            &probes(Some(column(ground)), lobby_hit()),
        );
        let text = joined(&lines);
        assert!(text.contains("minecraft:stone §7en y=150 (6 blocs sous vos pieds)"));
        assert!(text.contains("§7Pieds : §7air"));
        assert!(text.contains("§7Dessous : §fminecraft:stone"));
    }

    #[test]
    fn no_solid_within_range() {
        let lines = describe(
            "a",
            "a",
            (0.0, 156.0, 0.0),
            &probes(Some(column(Descent::Empty)), lobby_hit()),
        );
        assert!(joined(&lines).contains("Sol : aucun bloc sur 120 blocs sous vos pieds"));
    }

    #[test]
    fn unloaded_chunk_is_reported_not_air() {
        let mut col = column(Descent::Unloaded { y: 156 });
        col.feet = Probe::Unloaded;
        let lines = describe("a", "a", (0.0, 156.0, 0.0), &probes(Some(col), lobby_hit()));
        let text = joined(&lines);
        assert!(text.contains("Pieds : §cchunk non chargé"));
        assert!(text.contains("chunk non chargé en y=156"));
        assert!(text.contains("n'est pas chargé côté serveur"));
    }

    #[test]
    fn nothing_unloaded_adds_no_warning() {
        let lines = describe(
            "a",
            "a",
            (0.0, 156.0, 0.0),
            &probes(Some(column(Descent::Empty)), lobby_hit()),
        );
        assert!(!joined(&lines).contains("n'est pas chargé côté serveur"));
    }

    #[test]
    fn lobby_column_lines() {
        let lines = describe("a", "a", (0.0, 0.0, 0.0), &probes(None, lobby_hit()));
        assert!(
            joined(&lines)
                .contains("Lobby : §fminecraft:iron_block §7en y=155 (21 blocs sous y=176)")
        );
        let empty = describe("a", "a", (0.0, 0.0, 0.0), &probes(None, Descent::Empty));
        assert!(joined(&empty).contains("Lobby : aucun bloc sur 120 blocs sous y=176"));
        let unloaded = describe(
            "a",
            "a",
            (0.0, 0.0, 0.0),
            &probes(None, Descent::Unloaded { y: 170 }),
        );
        assert!(joined(&unloaded).contains("Lobby : chunk non chargé en y=170"));
    }

    #[test]
    fn air_keys_are_air() {
        assert_eq!(Probe::of_key("minecraft:cave_air"), Probe::Air);
        assert_eq!(
            Probe::of_key("minecraft:stone"),
            Probe::Solid("minecraft:stone".to_owned())
        );
    }
}

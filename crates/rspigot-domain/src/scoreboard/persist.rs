use rspigot_nbt::{Compound, Tag};

use crate::text::{Color, Component, Content, NamedColor, Style};

use super::board::{Objective, Score, Scoreboard};
use super::{CollisionRule, Criteria, DisplaySlot, NumberFormat, RenderType, Team, Visibility};

const TYPE_BLANK: &str = "minecraft:blank";
const TYPE_FIXED: &str = "minecraft:fixed";
const TYPE_STYLED: &str = "minecraft:styled";

fn style_into(style: &Style, out: &mut Compound) {
    if let Some(color) = style.color {
        out.insert("color", color.serialize());
    }
    for (key, value) in [
        ("bold", style.bold),
        ("italic", style.italic),
        ("underlined", style.underlined),
        ("strikethrough", style.strikethrough),
        ("obfuscated", style.obfuscated),
    ] {
        if let Some(value) = value {
            out.insert(key, value);
        }
    }
}

fn style_from(tag: &Compound) -> Style {
    let flag = |key: &str| tag.get_bool(key);
    Style {
        click: None,
        hover: None,
        color: tag.get_str("color").and_then(Color::parse),
        bold: flag("bold"),
        italic: flag("italic"),
        underlined: flag("underlined"),
        strikethrough: flag("strikethrough"),
        obfuscated: flag("obfuscated"),
    }
}

fn is_plain(component: &Component) -> bool {
    component.style == Style::default()
        && component.extra.is_empty()
        && matches!(component.content, Content::Text(_))
}

pub(crate) fn component_to_tag(component: &Component) -> Tag {
    if is_plain(component)
        && let Content::Text(text) = &component.content
    {
        return Tag::String(text.clone());
    }
    let mut out = Compound::new();
    match &component.content {
        Content::Text(text) => {
            out.insert("text", text.as_str());
        }
        Content::Translatable { key, args } => {
            out.insert("translate", key.as_str());
            if !args.is_empty() {
                out.insert(
                    "with",
                    Tag::List(args.iter().map(component_to_tag).collect()),
                );
            }
        }
    }
    style_into(&component.style, &mut out);
    if !component.extra.is_empty() {
        out.insert(
            "extra",
            Tag::List(component.extra.iter().map(component_to_tag).collect()),
        );
    }
    Tag::Compound(out)
}

pub(crate) fn component_from_tag(tag: &Tag) -> Option<Component> {
    match tag {
        Tag::String(text) => Some(Component::text(text.clone())),
        Tag::List(items) => {
            let mut parts = items.iter().filter_map(component_from_tag);
            let mut head = parts.next()?;
            for part in parts {
                head = head.append(part);
            }
            Some(head)
        }
        Tag::Compound(compound) => {
            let mut component = if let Some(text) = compound.get_str("text") {
                Component::text(text)
            } else {
                let key = compound.get_str("translate")?;
                let args = compound
                    .get_list("with")
                    .map(|items| items.iter().filter_map(component_from_tag).collect())
                    .unwrap_or_default();
                Component::translatable(key, args)
            };
            component.style = style_from(compound);
            if let Some(extra) = compound.get_list("extra") {
                component.extra = extra.iter().filter_map(component_from_tag).collect();
            }
            Some(component)
        }
        _ => None,
    }
}

pub(super) fn number_format_to_tag(format: &NumberFormat) -> Tag {
    let mut out = Compound::new();
    match format {
        NumberFormat::Blank => {
            out.insert("type", TYPE_BLANK);
        }
        NumberFormat::Fixed(component) => {
            out.insert("type", TYPE_FIXED);
            out.insert("value", component_to_tag(component));
        }
        NumberFormat::Styled(style) => {
            out.insert("type", TYPE_STYLED);
            style_into(style, &mut out);
        }
    }
    Tag::Compound(out)
}

pub(super) fn number_format_from_tag(tag: &Tag) -> Option<NumberFormat> {
    let compound = tag.as_compound()?;
    match compound.get_str("type")? {
        TYPE_BLANK => Some(NumberFormat::Blank),
        TYPE_FIXED => compound
            .get("value")
            .and_then(component_from_tag)
            .map(NumberFormat::Fixed),
        TYPE_STYLED => Some(NumberFormat::Styled(style_from(compound))),
        _ => None,
    }
}

fn insert_format(out: &mut Compound, format: Option<&NumberFormat>) {
    if let Some(format) = format {
        out.insert("format", number_format_to_tag(format));
    }
}

fn read_format(compound: &Compound) -> Option<NumberFormat> {
    compound.get("format").and_then(number_format_from_tag)
}

fn objective_to_tag(objective: &Objective) -> Tag {
    let mut out = Compound::new()
        .with("Name", objective.name.as_str())
        .with("CriteriaName", objective.criteria.name())
        .with("DisplayName", component_to_tag(&objective.display_name))
        .with("RenderType", objective.render_type.name())
        .with("display_auto_update", objective.display_auto_update);
    insert_format(&mut out, objective.number_format.as_ref());
    Tag::Compound(out)
}

fn objective_from_tag(compound: &Compound) -> Option<Objective> {
    let name = compound.get_str("Name")?;
    let criteria = Criteria::parse(
        compound.get_str("CriteriaName").unwrap_or("dummy"),
        |_, _| true,
    )?;
    Some(Objective {
        name: name.to_owned(),
        display_name: compound
            .get("DisplayName")
            .and_then(component_from_tag)
            .unwrap_or_else(|| Component::text(name)),
        render_type: compound
            .get_str("RenderType")
            .and_then(RenderType::from_name)
            .unwrap_or(RenderType::Integer),
        display_auto_update: compound.get_bool("display_auto_update").unwrap_or(false),
        number_format: read_format(compound),
        criteria,
    })
}

fn score_to_tag(holder: &str, objective: &str, score: &Score) -> Tag {
    let mut out = Compound::new()
        .with("Name", holder)
        .with("Objective", objective)
        .with("Score", score.value)
        .with("Locked", score.locked);
    if let Some(display) = &score.display {
        out.insert("display", component_to_tag(display));
    }
    insert_format(&mut out, score.number_format.as_ref());
    Tag::Compound(out)
}

fn team_to_tag(team: &Team, members: &[&str]) -> Tag {
    let mut out = Compound::new()
        .with("Name", team.name.as_str())
        .with("DisplayName", component_to_tag(&team.display_name));
    if let Some(color) = team.color {
        out.insert("TeamColor", color.name());
    }
    out.insert("AllowFriendlyFire", team.friendly_fire);
    out.insert("SeeFriendlyInvisibles", team.see_friendly_invisibles);
    out.insert("MemberNamePrefix", component_to_tag(&team.prefix));
    out.insert("MemberNameSuffix", component_to_tag(&team.suffix));
    out.insert("NameTagVisibility", team.nametag_visibility.name());
    out.insert(
        "DeathMessageVisibility",
        team.death_message_visibility.name(),
    );
    out.insert("CollisionRule", team.collision_rule.name());
    out.insert(
        "Players",
        Tag::List(
            members
                .iter()
                .map(|name| Tag::String((*name).to_owned()))
                .collect(),
        ),
    );
    Tag::Compound(out)
}

fn team_from_tag(compound: &Compound) -> Option<(Team, Vec<String>)> {
    let name = compound.get_str("Name")?;
    let mut team = Team::new(name, Component::text(name));
    if let Some(display) = compound.get("DisplayName").and_then(component_from_tag) {
        team.display_name = display;
    }
    team.color = compound
        .get_str("TeamColor")
        .and_then(NamedColor::from_name);
    team.friendly_fire = compound.get_bool("AllowFriendlyFire").unwrap_or(true);
    team.see_friendly_invisibles = compound.get_bool("SeeFriendlyInvisibles").unwrap_or(true);
    if let Some(prefix) = compound
        .get("MemberNamePrefix")
        .and_then(component_from_tag)
    {
        team.prefix = prefix;
    }
    if let Some(suffix) = compound
        .get("MemberNameSuffix")
        .and_then(component_from_tag)
    {
        team.suffix = suffix;
    }
    team.nametag_visibility = compound
        .get_str("NameTagVisibility")
        .and_then(Visibility::from_name)
        .unwrap_or_default();
    team.death_message_visibility = compound
        .get_str("DeathMessageVisibility")
        .and_then(Visibility::from_name)
        .unwrap_or_default();
    team.collision_rule = compound
        .get_str("CollisionRule")
        .and_then(CollisionRule::from_name)
        .unwrap_or_default();
    let members = compound
        .get_list("Players")
        .map(|players| {
            players
                .iter()
                .filter_map(|player| player.as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default();
    Some((team, members))
}

impl Scoreboard {
    pub fn to_nbt(&self) -> Compound {
        let objectives = self.objectives.values().map(objective_to_tag).collect();
        let scores = self
            .scores
            .iter()
            .flat_map(|(holder, scores)| {
                scores
                    .iter()
                    .map(|(objective, score)| score_to_tag(holder, objective, score))
            })
            .collect();
        let mut slots = Compound::new();
        for slot in DisplaySlot::all() {
            if let Some(objective) = &self.display[slot.index()] {
                slots.insert(slot.name(), objective.as_str());
            }
        }
        let teams = self
            .teams
            .values()
            .map(|team| team_to_tag(team, &self.team_members(&team.name)))
            .collect();
        Compound::new()
            .with("Objectives", Tag::List(objectives))
            .with("PlayerScores", Tag::List(scores))
            .with("DisplaySlots", slots)
            .with("Teams", Tag::List(teams))
    }

    pub fn from_nbt(data: &Compound) -> Self {
        let mut board = Self::new();
        for compound in compounds(data.get_list("Objectives")) {
            if let Some(objective) = objective_from_tag(compound) {
                board.objectives.insert(objective.name.clone(), objective);
            }
        }
        for compound in compounds(data.get_list("PlayerScores")) {
            let (Some(holder), Some(objective)) =
                (compound.get_str("Name"), compound.get_str("Objective"))
            else {
                continue;
            };
            if !board.objectives.contains_key(objective) {
                continue;
            }
            let score = Score {
                value: compound.get_i32("Score").unwrap_or(0),
                locked: compound.get_bool("Locked").unwrap_or(false),
                display: compound.get("display").and_then(component_from_tag),
                number_format: read_format(compound),
            };
            board
                .scores
                .entry(holder.to_owned())
                .or_default()
                .insert(objective.to_owned(), score);
        }
        if let Some(slots) = data.get_compound("DisplaySlots") {
            for (name, objective) in slots.iter() {
                let (Some(slot), Some(objective)) =
                    (DisplaySlot::from_name(name), objective.as_str())
                else {
                    continue;
                };
                if board.objectives.contains_key(objective) {
                    board.display[slot.index()] = Some(objective.to_owned());
                }
            }
        }
        for compound in compounds(data.get_list("Teams")) {
            if let Some((team, members)) = team_from_tag(compound) {
                for member in members {
                    board.membership.insert(member, team.name.clone());
                }
                board.teams.insert(team.name.clone(), team);
            }
        }
        board
    }
}

fn compounds(list: Option<&[Tag]>) -> impl Iterator<Item = &Compound> {
    list.unwrap_or_default().iter().filter_map(Tag::as_compound)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn populated() -> Scoreboard {
        let mut board = Scoreboard::new();
        let mut objective = Objective::new(
            "hp",
            Criteria::Health,
            Component::text("Health").color(NamedColor::Red),
        );
        objective.number_format = Some(NumberFormat::Fixed(Component::text("x")));
        objective.display_auto_update = true;
        board.add_objective(objective).unwrap();
        board
            .add_objective(Objective::new("t", Criteria::Trigger, Component::text("t")))
            .unwrap();
        board.set_score("alex", "hp", 17, None).unwrap();
        board.set_locked("alex", "t", false).unwrap();
        board
            .set_score_display(
                "alex",
                "hp",
                Some(Component::translatable("k", vec![Component::text("a")])),
            )
            .unwrap();
        board
            .set_score_number_format(
                "alex",
                "hp",
                Some(NumberFormat::Styled(Style {
                    color: Some(Color::Named(NamedColor::Gold)),
                    bold: Some(true),
                    ..Style::default()
                })),
            )
            .unwrap();
        board.set_display(DisplaySlot::Sidebar, Some("hp")).unwrap();
        board
            .set_display(DisplaySlot::Team(NamedColor::Red), Some("t"))
            .unwrap();
        let mut team = Team::new("red", Component::text("Red Team"));
        team.color = Some(NamedColor::Red);
        team.friendly_fire = false;
        team.prefix = Component::text("[R] ");
        team.nametag_visibility = Visibility::HideForOtherTeams;
        team.death_message_visibility = Visibility::Never;
        team.collision_rule = CollisionRule::PushOwnTeam;
        board.add_team(team).unwrap();
        board.join_team("alex", "red").unwrap();
        board
    }

    #[test]
    fn a_populated_scoreboard_round_trips_through_nbt() {
        let board = populated();
        let restored = Scoreboard::from_nbt(&board.to_nbt());
        assert_eq!(restored.to_nbt(), board.to_nbt());
        assert_eq!(restored.objective("hp"), board.objective("hp"));
        assert_eq!(restored.score("alex", "hp"), board.score("alex", "hp"));
        assert_eq!(restored.team("red"), board.team("red"));
        assert_eq!(
            restored.team_of("alex").map(|t| t.name.as_str()),
            Some("red")
        );
        assert_eq!(restored.display(DisplaySlot::Sidebar), Some("hp"));
        assert!(!restored.score("alex", "t").unwrap().locked);
    }

    #[test]
    fn the_layout_matches_the_vanilla_codecs() {
        let nbt = populated().to_nbt();
        let objective = nbt.get_list("Objectives").unwrap()[0]
            .as_compound()
            .unwrap();
        assert_eq!(objective.get_str("CriteriaName"), Some("health"));
        assert_eq!(objective.get_str("RenderType"), Some("hearts"));
        assert_eq!(objective.get_bool("display_auto_update"), Some(true));
        let format = objective.get_compound("format").unwrap();
        assert_eq!(format.get_str("type"), Some("minecraft:fixed"));
        let score = nbt.get_list("PlayerScores").unwrap()[0]
            .as_compound()
            .unwrap();
        assert_eq!(score.get_str("Name"), Some("alex"));
        assert_eq!(score.get_i32("Score"), Some(17));
        assert_eq!(
            nbt.get_compound("DisplaySlots").unwrap().get_str("sidebar"),
            Some("hp")
        );
        let team = nbt.get_list("Teams").unwrap()[0].as_compound().unwrap();
        assert_eq!(team.get_str("TeamColor"), Some("red"));
        assert_eq!(team.get_str("NameTagVisibility"), Some("hideForOtherTeams"));
        assert_eq!(team.get_str("CollisionRule"), Some("pushOwnTeam"));
        assert_eq!(team.get_list("Players").unwrap().len(), 1);
    }

    #[test]
    fn scores_and_slots_of_missing_objectives_are_dropped() {
        let nbt = Compound::new()
            .with(
                "PlayerScores",
                Tag::List(vec![Tag::Compound(
                    Compound::new()
                        .with("Name", "a")
                        .with("Objective", "gone")
                        .with("Score", 1),
                )]),
            )
            .with("DisplaySlots", Compound::new().with("sidebar", "gone"));
        let board = Scoreboard::from_nbt(&nbt);
        assert!(board.holders().is_empty());
        assert_eq!(board.display(DisplaySlot::Sidebar), None);
    }

    #[test]
    fn components_round_trip_plain_translatable_and_styled() {
        let styled = Component::text("a").color(NamedColor::Gold).bold();
        let nested =
            Component::translatable("k", vec![styled.clone()]).append(Component::text("z"));
        for component in [Component::text("plain"), styled, nested] {
            assert_eq!(
                component_from_tag(&component_to_tag(&component)),
                Some(component)
            );
        }
    }
}

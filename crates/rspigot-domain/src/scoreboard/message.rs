use crate::text::{Component, NamedColor};

use super::{CollisionRule, DisplaySlot, NumberFormat, RenderType, Team, Visibility};

#[derive(Debug, Clone, PartialEq)]
pub struct ObjectiveView {
    pub name: String,
    pub display_name: Component,
    pub render_type: RenderType,
    pub number_format: Option<NumberFormat>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TeamView {
    pub display_name: Component,
    pub prefix: Component,
    pub suffix: Component,
    pub nametag_visibility: Visibility,
    pub collision_rule: CollisionRule,
    pub color: Option<NamedColor>,
    pub options: u8,
}

impl TeamView {
    pub fn of(team: &Team) -> Self {
        Self {
            display_name: team.display_name.clone(),
            prefix: team.prefix.clone(),
            suffix: team.suffix.clone(),
            nametag_visibility: team.nametag_visibility,
            collision_rule: team.collision_rule,
            color: team.color,
            options: team.options(),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum TeamOp {
    Add {
        view: TeamView,
        members: Vec<String>,
    },
    Modify(TeamView),
    Remove,
    Join(Vec<String>),
    Leave(Vec<String>),
}

#[derive(Debug, Clone, PartialEq)]
pub enum ScoreboardMessage {
    ObjectiveAdd(ObjectiveView),
    ObjectiveChange(ObjectiveView),
    ObjectiveRemove(String),
    DisplayObjective {
        slot: DisplaySlot,
        objective: Option<String>,
    },
    Score {
        holder: String,
        objective: String,
        value: i32,
        display: Option<Component>,
        number_format: Option<NumberFormat>,
    },
    ResetScore {
        holder: String,
        objective: Option<String>,
    },
    Team {
        name: String,
        op: TeamOp,
    },
}

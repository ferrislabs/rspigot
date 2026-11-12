use rspigot_domain::entity::Aabb;

use crate::action::MobAction;
use crate::brain::AiContext;
use crate::world::{AiWorld, TargetKind};

const ALERT_RANGE_Y: f64 = 10.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResetUniversalAngerGoal {
    alert_others: bool,
    last_hurt_by_player_at: i32,
}

impl ResetUniversalAngerGoal {
    pub const fn new(alert_others: bool) -> Self {
        Self {
            alert_others,
            last_hurt_by_player_at: 0,
        }
    }

    pub(crate) fn can_use<W: AiWorld>(&mut self, context: &mut AiContext<'_, W>) -> bool {
        if !context.world.universal_anger() {
            return false;
        }
        let mob = context.mob;
        mob.last_hurt_by_mob
            .and_then(|id| context.world.living(id))
            .is_some_and(|attacker| attacker.player)
            && mob.last_hurt_by_mob_at > self.last_hurt_by_player_at
    }

    pub(crate) fn start<W: AiWorld>(&mut self, context: &mut AiContext<'_, W>) {
        let mob = context.mob;
        self.last_hurt_by_player_at = mob.last_hurt_by_mob_at;
        context.act(MobAction::ForgetAndRefreshAnger(mob.id));
        if !self.alert_others {
            return;
        }
        let follow = mob.follow_range;
        let corner = mob.position;
        let area = Aabb::new(
            corner.x - follow,
            corner.y - ALERT_RANGE_Y,
            corner.z - follow,
            corner.x + 1.0 + follow,
            corner.y + 1.0 + ALERT_RANGE_Y,
            corner.z + 1.0 + follow,
        );
        let nearby = context
            .world
            .living_of(TargetKind::Class(mob.kind.class()), &area);
        for other in nearby {
            if other.id != mob.id && !other.spectator {
                context.act(MobAction::ForgetAndRefreshAnger(other.id));
            }
        }
    }
}

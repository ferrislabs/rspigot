use rspigot_domain::Vec3;
use rspigot_domain::entity::leash;
use rspigot_entities::EntityKind;

use crate::brain::AiContext;
use crate::world::AiWorld;

pub const DEFAULT_FOLLOW_SPEED: f64 = 1.0;
pub const LLAMA_FOLLOW_SPEED: f64 = 2.0;
const KEEP_DISTANCE: f32 = 2.0;

pub fn follow_speed(kind: EntityKind) -> f64 {
    if kind.extends("Llama") {
        LLAMA_FOLLOW_SPEED
    } else {
        DEFAULT_FOLLOW_SPEED
    }
}

pub fn stays_close_to_holder(kind: EntityKind) -> bool {
    kind.extends("PathfinderMob") && !matches!(kind, EntityKind::Allay | EntityKind::HappyGhast)
}

pub(crate) fn follow_holder<W: AiWorld>(context: &mut AiContext<'_, W>) {
    let mob = context.mob;
    if !stays_close_to_holder(mob.kind) {
        return;
    }
    let Some(holder) = mob.leash_holder.and_then(|id| context.world.living(id)) else {
        return;
    };
    let holder_width = (holder.bounds.max_x - holder.bounds.min_x) as f32;
    let center_distance = leash::center_distance(mob.bounds().center(), holder.bounds.center());
    if leash::must_pull(center_distance, mob.width, holder_width) {
        return;
    }
    let distance = mob.position.distance_squared(holder.position).sqrt() as f32;
    let direction = Vec3::new(
        holder.position.x - mob.position.x,
        holder.position.y - mob.position.y,
        holder.position.z - mob.position.z,
    )
    .normalize()
    .scale(f64::from((distance - KEEP_DISTANCE).max(0.0)));
    context.move_to_pos(
        Vec3::new(
            mob.position.x + direction.x,
            mob.position.y + direction.y,
            mob.position.z + direction.z,
        ),
        follow_speed(mob.kind),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mock::{Harness, Mock, living};

    fn leashed_llama(holder_at: Vec3) -> Harness {
        let world = Mock {
            others: vec![living(2, EntityKind::WanderingTrader, holder_at)],
            ..Mock::default()
        };
        let mut harness =
            Harness::new(EntityKind::TraderLlama, Vec3::new(0.5, 64.0, 0.5), world, 3);
        harness.mob.leashed = true;
        harness.mob.leash_holder = Some(rspigot_domain::EntityId(2));
        harness
    }

    #[test]
    fn a_leashed_llama_walks_toward_its_holder_at_twice_the_speed_when_the_lead_is_slack() {
        assert_eq!(follow_speed(EntityKind::TraderLlama), 2.0);
        assert_eq!(follow_speed(EntityKind::Cat), 1.0);
        let mut near = leashed_llama(Vec3::new(4.5, 64.0, 0.5));
        near.run(follow_holder);
        assert!(!near.brain.navigation.is_done());
    }

    #[test]
    fn a_taut_lead_is_left_to_the_elastic_pull_and_unleashed_mobs_stay_put() {
        let mut taut = leashed_llama(Vec3::new(9.5, 64.0, 0.5));
        taut.run(follow_holder);
        assert!(taut.brain.navigation.is_done());
        let mut free = leashed_llama(Vec3::new(4.5, 64.0, 0.5));
        free.mob.leash_holder = None;
        free.run(follow_holder);
        assert!(free.brain.navigation.is_done());
        let mut flyer = Harness::new(
            EntityKind::Allay,
            Vec3::new(0.5, 64.0, 0.5),
            Mock::default(),
            3,
        );
        flyer.mob.leash_holder = Some(rspigot_domain::EntityId(2));
        flyer.run(follow_holder);
        assert!(flyer.brain.navigation.is_done());
    }
}

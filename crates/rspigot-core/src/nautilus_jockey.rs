#[cfg(test)]
mod tests {
    use rspigot_domain::entity::spawn::Jockey;
    use rspigot_domain::{EntityId, Vec3};
    use rspigot_entities::EntityKind as Kind;

    use crate::mob_actions::new_mob;
    use crate::quiet_server::support::{Quiet, server};
    use crate::server::Server;

    fn drowned_with(server: &mut Server<Quiet>, jockey: Option<Jockey>, at: Vec3) -> EntityId {
        let mut base = server.new_entity_base(Kind::Drowned);
        base.position = at;
        let mut drowned = new_mob(base);
        if let Some(mob) = drowned.living_mut().and_then(|living| living.mob.as_mut()) {
            mob.jockey = jockey;
        }
        server.place_entity(drowned).expect("the drowned is placed")
    }

    #[test]
    fn a_drowned_planned_as_a_jockey_rides_a_fresh_zombie_nautilus() {
        let mut server = server();
        let drowned = drowned_with(
            &mut server,
            Some(Jockey::NautilusRider),
            Vec3::new(0.5, 64.0, 0.5),
        );
        server.tick();
        let mounts = server.entities_of(Kind::ZombieNautilus);
        assert_eq!(mounts.len(), 1);
        assert_eq!(
            server.entities.rides.passengers_of(mounts[0].id),
            &[drowned]
        );
    }

    #[test]
    fn a_drowned_without_a_plan_rides_nothing() {
        let mut server = server();
        drowned_with(&mut server, None, Vec3::new(0.5, 64.0, 0.5));
        server.tick();
        assert!(server.entities_of(Kind::ZombieNautilus).is_empty());
    }
}

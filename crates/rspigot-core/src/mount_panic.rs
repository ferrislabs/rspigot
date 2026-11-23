#[cfg(test)]
mod tests {
    use rspigot_domain::damage::DamageType;
    use rspigot_domain::{EntityId, Vec3};
    use rspigot_entities::EntityKind as Kind;

    use crate::mob_actions::new_mob;
    use crate::quiet_server::support::{Quiet, server};
    use crate::server::Server;

    fn place(server: &mut Server<Quiet>, kind: Kind, at: Vec3) -> EntityId {
        let mut base = server.new_entity_base(kind);
        base.position = at;
        server
            .place_entity(new_mob(base))
            .expect("the entity is placed")
    }

    fn hurt(server: &mut Server<Quiet>, id: EntityId) {
        let now = server.world.level.game_time;
        if let Some(living) = server
            .entities
            .store
            .get_mut(id)
            .and_then(|entity| entity.living_mut())
        {
            living.damage.last_source = Some((DamageType::MobAttack, now));
        }
    }

    #[test]
    fn a_hurt_horse_panics() {
        let mut server = server();
        let horse = place(&mut server, Kind::Horse, Vec3::new(0.5, 64.0, 0.5));
        hurt(&mut server, horse);
        assert!(server.mob_view(horse).unwrap().panic);
    }

    #[test]
    fn a_hurt_horse_carrying_a_mob_rider_does_not_panic() {
        let mut server = server();
        let horse = place(&mut server, Kind::Horse, Vec3::new(0.5, 64.0, 0.5));
        let rider = place(&mut server, Kind::Zombie, Vec3::new(0.5, 64.0, 0.5));
        server.start_riding(rider, horse, true, true);
        hurt(&mut server, horse);
        assert!(!server.mob_view(horse).unwrap().panic);
    }

    #[test]
    fn a_hurt_cow_carrying_a_mob_rider_still_panics() {
        let mut server = server();
        let cow = place(&mut server, Kind::Cow, Vec3::new(0.5, 64.0, 0.5));
        let rider = place(&mut server, Kind::Zombie, Vec3::new(0.5, 64.0, 0.5));
        server.start_riding(rider, cow, true, true);
        hurt(&mut server, cow);
        assert!(server.mob_view(cow).unwrap().panic);
    }
}

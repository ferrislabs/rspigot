use rspigot_domain::EntityId;

use crate::ports::ClientLink;
use crate::server::Server;

impl<L: ClientLink> Server<L> {
    pub fn entity_data_int(&self, id: EntityId, index: u8) -> Option<i32> {
        Some(self.entities.store.get(id)?.base.data.int(index))
    }

    pub fn entity_data_byte(&self, id: EntityId, index: u8) -> Option<i8> {
        Some(self.entities.store.get(id)?.base.data.byte(index))
    }
}

impl<L: ClientLink> Server<L> {
    pub fn entity_data_bool(&self, id: EntityId, index: u8) -> Option<bool> {
        Some(self.entities.store.get(id)?.base.data.boolean(index))
    }
}

impl<L: ClientLink> Server<L> {
    pub fn player_shoulder_entities(&self, id: rspigot_domain::player::PlayerId) -> (bool, bool) {
        self.players.get(&id).map_or((false, false), |player| {
            (
                player.shoulders.left.is_some(),
                player.shoulders.right.is_some(),
            )
        })
    }
}

impl<L: ClientLink> Server<L> {
    pub fn seed_village(&mut self, center: rspigot_domain::BlockPos, homes: usize) {
        use rspigot_domain::poi::PoiType;
        let poi = self.world.chunks.poi_mut();
        poi.add(center, PoiType::Meeting);
        for index in 0..homes {
            let pos = center.offset(index as i32, 1, 0);
            poi.add(pos, PoiType::Home);
            poi.take(|kind| kind == PoiType::Home, |_, at| at == pos, pos, 1);
        }
    }
}

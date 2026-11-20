use rspigot_plugin::prelude::*;

const KIND: &str = "factions:kind";

#[derive(Default, Module)]
#[module(own)]
pub struct Mobs {
    #[module]
    passive: PassiveMobs,
    #[module]
    aggressive: AggressiveMobs,
}

#[listeners]
impl Mobs {
    async fn on_enable(&self, server: &Server) {
        let _ = server;
        tracing::info!("Tagging passive and aggressive mobs as they spawn");
    }
}

#[derive(Default)]
pub struct PassiveMobs;

#[listeners]
impl PassiveMobs {
    async fn on_creature_spawn(&self, event: &CreatureSpawn) {
        if matches!(
            event.entity_type(),
            EntityType::Cow | EntityType::Sheep | EntityType::Pig | EntityType::Chicken
        ) {
            event.entity().meta.set(KIND, "passive");
        }
    }
}

#[derive(Default)]
pub struct AggressiveMobs;

#[listeners]
impl AggressiveMobs {
    async fn on_creature_spawn(&self, event: &CreatureSpawn) {
        if matches!(
            event.entity_type(),
            EntityType::Zombie | EntityType::Skeleton | EntityType::Spider | EntityType::Creeper
        ) {
            event.entity().meta.set(KIND, "aggressive");
        }
    }
}

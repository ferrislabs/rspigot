use std::collections::BTreeSet;

use rspigot_blocks::BlockKind;
use rspigot_domain::item::Slot;
use rspigot_domain::player::PlayerId;
use rspigot_domain::potion::PotionContents;
use rspigot_domain::{BlockPos, EntityId, Identifier, Vec3};
use rspigot_entities::EntityKind as Kind;
use rspigot_items::{EquipmentSlot, ItemId, ItemKind};

use crate::advancement_conditions::{DamageFacts, EntityFacts};
use crate::advancements::AdvancementEvent;
use crate::health::DamageSource;
use crate::ports::ClientLink;
use crate::server::Server;

const SUMMON_RANGE: f64 = 50.0;
const LIGHTNING_RANGE: f64 = 256.0;
const BYSTANDER_RANGE: f64 = 16.0;
const OVERWORLD: &str = "minecraft:overworld";
const NETHER: &str = "minecraft:the_nether";
const CRAFTER_RANGE: f64 = 17.0;
const POTION_SLOTS: usize = 3;
const HALF_WIDTH: f64 = 0.3;
const HEIGHT: f64 = 1.8;
const EDGE: f64 = 1.0e-7;

fn is_potion(item: ItemKind) -> bool {
    matches!(
        item,
        ItemKind::Potion | ItemKind::SplashPotion | ItemKind::LingeringPotion
    )
}

fn has_potion_type(stack: &rspigot_domain::item::ItemStack) -> bool {
    stack.item.kind().is_some_and(is_potion) && PotionContents::of(stack).potion.is_some()
}

impl<L: ClientLink> Server<L> {
    pub(crate) fn criterion_damage(
        &self,
        source: &DamageSource,
        dealt: f32,
        taken: f32,
        blocked: bool,
        from: Vec3,
    ) -> DamageFacts<'static> {
        let describe = |entity: &crate::health::SourceEntity| {
            let mut facts = self
                .facts_of(entity.entity, from)
                .unwrap_or_else(|| EntityFacts::of(entity.kind.name(), false));
            facts.mainhand = entity
                .player
                .and_then(|player| self.players.get(&player))
                .and_then(|player| player.inventory.held())
                .map(|stack| stack.item.0);
            facts
        };
        DamageFacts {
            kind: source.kind,
            dealt: f64::from(dealt),
            taken: f64::from(taken),
            blocked,
            direct: source.direct.as_ref().map(describe),
            causing: source.causing.as_ref().map(describe),
        }
    }

    pub(crate) fn trigger_summoned_near(&mut self, summoned: Option<EntityId>) {
        let Some((id, at)) = summoned.and_then(|id| Some((id, self.entity_position(id)?))) else {
            return;
        };
        let nearby: Vec<PlayerId> = self
            .players
            .values()
            .filter(|player| {
                (player.position.x - at.x).abs() <= SUMMON_RANGE
                    && (player.position.y - at.y).abs() <= SUMMON_RANGE
                    && (player.position.z - at.z).abs() <= SUMMON_RANGE
            })
            .map(|player| player.id)
            .collect();
        for player in nearby {
            let from = self.players.get(&player).map_or(at, |p| p.position);
            let mut event = AdvancementEvent::named("minecraft:summoned_entity");
            event.entities = self.slot_near("entity", id, from).into_iter().collect();
            self.trigger_event(player, event);
        }
    }

    pub(crate) fn trigger_target_hit(&mut self, projectile: EntityId, at: Vec3, strength: i32) {
        let Some(entity) = self.entities.store.get(projectile) else {
            return;
        };
        let owner = EntityId(self.projectile_owner_id(entity));
        let Some(player) = self.player_of_entity(owner) else {
            return;
        };
        let Some(from) = self.players.get(&player).map(|p| p.position) else {
            return;
        };
        let mut event = AdvancementEvent::named("minecraft:target_hit");
        event.entities = self
            .slot_near("projectile", projectile, from)
            .into_iter()
            .collect();
        event.amounts.push(("signal_strength", f64::from(strength)));
        event.at = Some(at.block());
        self.trigger_event(player, event);
    }

    pub(crate) fn trigger_fishing_rod_hooked(&mut self, player: PlayerId, caught: Option<u16>) {
        let mut event = AdvancementEvent::named("minecraft:fishing_rod_hooked");
        event.item = caught;
        self.trigger_event(player, event);
    }

    pub(crate) fn trigger_started_riding(&mut self, rider: EntityId) {
        if let Some(player) = self.player_of_entity(rider) {
            self.trigger_event(player, AdvancementEvent::named("minecraft:started_riding"));
        }
    }

    pub(crate) fn trigger_fall_from_height(&mut self, id: PlayerId, distance: f64, at: Vec3) {
        if distance <= 0.0 || self.players.get(&id).is_none_or(|p| p.vitals.is_dead()) {
            return;
        }
        let mut event = AdvancementEvent::named("minecraft:fall_from_height");
        event.amounts.push(("distance.y", distance));
        event.amounts.push(("start_position.y", at.y + distance));
        self.trigger_event(id, event);
    }

    pub(crate) fn track_nether_travel(
        &mut self,
        id: PlayerId,
        destination: &Identifier,
        arrival: Vec3,
    ) {
        let Some(player) = self.players.get_mut(&id) else {
            return;
        };
        let from = player.dim.to_string();
        let to = destination.to_string();
        if from == OVERWORLD && to == NETHER {
            let here = player.position.block();
            player.advancements.set_nether_entry(Some(here));
            return;
        }
        if from != NETHER || to != OVERWORLD {
            return;
        }
        let Some(entry) = player.advancements.take_nether_entry() else {
            return;
        };
        let (dx, dy, dz) = (
            arrival.x - f64::from(entry.x),
            arrival.y - f64::from(entry.y),
            arrival.z - f64::from(entry.z),
        );
        let mut event = AdvancementEvent::named("minecraft:nether_travel");
        event.amounts.push(("distance.horizontal", dx.hypot(dz)));
        event.amounts.push(("distance.y", dy.abs()));
        event
            .amounts
            .push(("distance.absolute", (dx * dx + dy * dy + dz * dz).sqrt()));
        self.trigger_event(id, event);
    }

    pub(crate) fn trigger_player_killed_entity(
        &mut self,
        victim: EntityId,
        killer: Option<PlayerId>,
        source: &DamageSource,
    ) {
        let Some(player) = killer else {
            return;
        };
        let Some(from) = self.players.get(&player).map(|p| p.position) else {
            return;
        };
        let Some(mut target) = self.facts_of(victim, from) else {
            return;
        };
        target.ominous_banner = self
            .entities
            .store
            .get(victim)
            .and_then(|entity| entity.living())
            .and_then(|living| living.equipment(EquipmentSlot::Head))
            .is_some_and(crate::raids::is_ominous_banner);
        let damage = self.criterion_damage(source, 0.0, 0.0, false, from);
        let mut event = AdvancementEvent::named("minecraft:player_killed_entity");
        event.entities.push(("entity", target));
        event.damage = Some(damage);
        self.trigger_event(player, event);
        self.trigger_killed_by_arrow(player, target, source);
    }

    fn trigger_killed_by_arrow(
        &mut self,
        player: PlayerId,
        victim: EntityFacts<'static>,
        source: &DamageSource,
    ) {
        let Some(direct) = source.direct.as_ref() else {
            return;
        };
        if !matches!(direct.kind, Kind::Arrow | Kind::SpectralArrow) {
            return;
        }
        let weapon = self
            .entities
            .store
            .get(direct.entity)
            .and_then(|entity| match &entity.state {
                rspigot_domain::entity::EntityState::Arrow(state) => state.weapon.as_ref(),
                _ => None,
            })
            .map(|stack| stack.item.0);
        let mut event = AdvancementEvent::named("minecraft:killed_by_arrow");
        event.weapon = weapon;
        event.victims.push(victim);
        event.amounts.push(("unique_entity_types", 1.0));
        self.trigger_event(player, event);
    }

    pub(crate) fn trigger_player_hurt_entity(
        &mut self,
        victim: EntityId,
        source: &DamageSource,
        dealt: f32,
        taken: f32,
    ) {
        let Some(player) = source.attacker() else {
            return;
        };
        let Some(from) = self.players.get(&player).map(|p| p.position) else {
            return;
        };
        let mut event = AdvancementEvent::named("minecraft:player_hurt_entity");
        event.entities = self.slot_near("entity", victim, from).into_iter().collect();
        event.damage = Some(self.criterion_damage(source, dealt, taken, false, from));
        self.trigger_event(player, event);
    }

    pub(crate) fn trigger_entity_hurt_player(
        &mut self,
        id: PlayerId,
        source: &DamageSource,
        dealt: f32,
        taken: f32,
        blocked: bool,
    ) {
        let Some(from) = self.players.get(&id).map(|p| p.position) else {
            return;
        };
        let mut event = AdvancementEvent::named("minecraft:entity_hurt_player");
        event.damage = Some(self.criterion_damage(source, dealt, taken, blocked, from));
        self.trigger_event(id, event);
    }

    pub(crate) fn trigger_effects_changed(&mut self, id: PlayerId) {
        self.trigger_event(id, AdvancementEvent::named("minecraft:effects_changed"));
    }

    pub(crate) fn trigger_loot_generated(&mut self, id: PlayerId, table: &str) {
        let mut event = AdvancementEvent::named("minecraft:player_generates_container_loot");
        event.loot_table = Some(table.to_owned());
        self.trigger_event(id, event);
    }

    pub(crate) fn trigger_brewed_potion(
        &mut self,
        id: PlayerId,
        stand: BlockKind,
        before: &[Slot],
        after: &[Slot],
    ) {
        if stand != BlockKind::BrewingStand {
            return;
        }
        let taken = (0..POTION_SLOTS).any(|slot| {
            before
                .get(slot)
                .and_then(Option::as_ref)
                .is_some_and(has_potion_type)
                && before.get(slot) != after.get(slot)
        });
        if taken {
            self.trigger_event(id, AdvancementEvent::named("minecraft:brewed_potion"));
        }
    }

    pub(crate) fn trigger_lightning_strike(&mut self, bolt: EntityId) {
        let Some(at) = self.entity_position(bolt) else {
            return;
        };
        let origin = at.block();
        let mut fires = 0;
        for dx in -1..=1 {
            for dy in -1..=1 {
                for dz in -1..=1 {
                    let burning = self
                        .world
                        .chunks
                        .block(origin.offset(dx, dy, dz))
                        .and_then(|state| state.kind())
                        .is_some_and(|kind| matches!(kind, BlockKind::Fire | BlockKind::SoulFire));
                    fires += i32::from(burning);
                }
            }
        }
        let bystanders: Vec<EntityId> = self
            .entities
            .store
            .iter()
            .filter(|entity| {
                entity.base.kind != Kind::LightningBolt
                    && (entity.base.position.x - at.x).hypot(entity.base.position.z - at.z)
                        <= BYSTANDER_RANGE
            })
            .map(|entity| entity.base.id)
            .collect();
        let nearby: Vec<(PlayerId, Vec3)> = self
            .players
            .values()
            .filter(|player| (player.position - at).length() <= LIGHTNING_RANGE)
            .map(|player| (player.id, player.position))
            .collect();
        for (player, from) in nearby {
            let mut strike = EntityFacts::of(Kind::LightningBolt.name(), false);
            strike.lightning_fire = Some(fires);
            strike.distance = Some(crate::advancement_conditions::Distance {
                horizontal: (from.x - at.x).hypot(from.z - at.z),
                absolute: (from - at).length(),
            });
            let mut event = AdvancementEvent::named("minecraft:lightning_strike");
            event.entities.push(("lightning", strike));
            for bystander in &bystanders {
                if let Some(facts) = self.slot_near("bystander", *bystander, from) {
                    event.entities.push(facts);
                }
            }
            self.trigger_event(player, event);
        }
    }

    pub(crate) fn tick_levitation(&mut self, id: PlayerId) {
        let Some(player) = self.players.get_mut(&id) else {
            return;
        };
        let levitating = player.effects.has("minecraft:levitation");
        let y = player.position.y;
        let Some(rise) = player.advancements.levitation_height(levitating, y) else {
            return;
        };
        let mut event = AdvancementEvent::named("minecraft:levitation");
        event.amounts.push(("distance.y", rise));
        event.amounts.push(("distance.absolute", rise.abs()));
        self.trigger_event(id, event);
    }

    pub(crate) fn tick_inside_blocks(&mut self, id: PlayerId) {
        let Some(player) = self.players.get(&id) else {
            return;
        };
        let at = player.position;
        let mut now: BTreeSet<u16> = BTreeSet::new();
        let mut places: Vec<(u16, BlockPos)> = Vec::new();
        let low = |value: f64| (value - HALF_WIDTH).floor() as i32;
        let high = |value: f64| (value + HALF_WIDTH - EDGE).floor() as i32;
        for x in low(at.x)..=high(at.x) {
            for y in at.y.floor() as i32..=((at.y + HEIGHT - EDGE).floor() as i32) {
                for z in low(at.z)..=high(at.z) {
                    let pos = BlockPos::new(x, y, z);
                    let Some(state) = self.world.chunks.block(pos).filter(|s| !s.is_air()) else {
                        continue;
                    };
                    if let Some(kind) = state.kind()
                        && now.insert(kind.id().0)
                    {
                        places.push((kind.id().0, pos));
                    }
                }
            }
        }
        let Some(player) = self.players.get_mut(&id) else {
            return;
        };
        for entered in player.advancements.entered_blocks(now) {
            if let Some((_, pos)) = places.iter().find(|(kind, _)| *kind == entered) {
                let mut event = AdvancementEvent::named("minecraft:enter_block");
                event.at = Some(*pos);
                self.trigger_event(id, event);
            }
        }
    }

    pub(crate) fn trigger_item_durability(&mut self, id: PlayerId, item: u16, delta: i32) {
        let mut event = AdvancementEvent::named("minecraft:item_durability_changed");
        event.item = Some(item);
        event.amounts.push(("delta", f64::from(-delta)));
        self.trigger_event(id, event);
    }

    pub(crate) fn tick_lava_ride(&mut self, id: PlayerId) {
        let Some(player) = self.players.get(&id) else {
            return;
        };
        let in_lava = self
            .entities
            .rides
            .vehicle_of(player.entity_id)
            .and_then(|vehicle| self.entity_position(vehicle))
            .and_then(|at| self.world.chunks.block(at.block()))
            .is_some_and(|state| state.kind() == Some(BlockKind::Lava));
        let (x, z) = (player.position.x, player.position.z);
        let Some(player) = self.players.get_mut(&id) else {
            return;
        };
        let Some(travelled) = player.advancements.lava_distance(in_lava, x, z) else {
            return;
        };
        let mut event = AdvancementEvent::named("minecraft:ride_entity_in_lava");
        event.amounts.push(("distance.horizontal", travelled));
        self.trigger_event(id, event);
    }

    pub(crate) fn trigger_recipe_crafted(
        &mut self,
        id: PlayerId,
        recipe: Option<&str>,
        ingredients: &[ItemId],
    ) {
        let Some(recipe) = recipe else {
            return;
        };
        let mut event = AdvancementEvent::named("minecraft:recipe_crafted");
        event.recipe = Some(recipe.to_owned());
        event.ingredients = ingredients.iter().map(|item| item.0).collect();
        self.trigger_event(id, event);
    }

    pub(crate) fn trigger_crafter_crafted(
        &mut self,
        pos: rspigot_domain::BlockPos,
        recipe: &str,
        ingredients: &[ItemId],
    ) {
        let near: Vec<PlayerId> = self
            .players
            .values()
            .filter(|player| {
                let at = pos.center();
                (player.position.x - at.x).abs() <= CRAFTER_RANGE
                    && (player.position.y - at.y).abs() <= CRAFTER_RANGE
                    && (player.position.z - at.z).abs() <= CRAFTER_RANGE
            })
            .map(|player| player.id)
            .collect();
        for player in near {
            let mut event = AdvancementEvent::named("minecraft:crafter_recipe_crafted");
            event.recipe = Some(recipe.to_owned());
            event.ingredients = ingredients.iter().map(|item| item.0).collect();
            self.trigger_event(player, event);
        }
    }
}

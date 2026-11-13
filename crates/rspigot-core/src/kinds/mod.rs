use rspigot_ai::MobView;
use rspigot_domain::EntityId;
use rspigot_domain::entity::Entity;
use rspigot_domain::item::ItemStack;
use rspigot_domain::player::PlayerId;
use rspigot_entities::EntityKind as Kind;

use crate::events::Hand;
use crate::health::DamageSource;
use crate::interact::Interaction;
use crate::ports::ClientLink;
use crate::server::Server;

mod air;
mod allay;
mod armadillo;
mod axolotl;
mod bat;
mod bee;
mod blaze;
mod breeze;
mod cat;
pub(crate) mod copper_golem;
mod creaking;
mod dolphin;
mod drowned;
mod enderman;
mod endermite;
mod fish;
mod fox;
mod frog;
mod frogspawn;
mod ghast;
mod goat;
mod guardian;
mod hoglin;
mod iron_golem;
mod nautilus;
mod ocelot;
mod panda;
mod parrot;
mod phantom;
mod piglin;
mod pillager;
mod polar_bear;
mod rabbit;
mod shulker;
mod silverfish;
mod sniffer;
mod snow_golem;
mod squid;
mod tadpole;
mod turtle;
mod vex;
pub(crate) mod villager;
mod warden;
mod witch;
mod wither;
mod wolf;
mod zombie_villager;
mod zombified_piglin;

pub(crate) fn kind_sound_source(entity: &Entity) -> Option<rspigot_entities::SoundSource> {
    match entity.base.kind {
        Kind::Rabbit => Some(rabbit::sound_source(entity)),
        _ => None,
    }
}
pub(crate) use armadillo::ambient_sound as armadillo_ambient_sound;
pub(crate) use drowned::ambient_sound as drowned_ambient_sound;
pub(crate) use enderman::carried_block_drops;
pub(crate) use fish::fish_facts_of;
pub(crate) use goat::{GoatSound, voice as goat_voice};
pub(crate) use sniffer::ambient_sound as sniffer_ambient_sound;
pub(crate) use snow_golem::readiness_for_shearing;
pub(crate) use zombie_villager::{carry_villager_into_zombie, carry_zombie_into_villager};

impl<L: ClientLink> Server<L> {
    pub(crate) fn kind_of(&self, id: EntityId) -> Option<Kind> {
        self.entities.store.get(id).map(|entity| entity.base.kind)
    }

    pub(crate) fn kind_view(&self, entity: &Entity, view: &mut MobView) {
        match entity.base.kind {
            Kind::Wolf => self.wolf_view(entity, view),
            Kind::Cat => self.cat_view(entity, view),
            Kind::Fox => self.fox_view(entity, view),
            Kind::Panda => self.panda_view(entity, view),
            Kind::PolarBear => self.polar_bear_view(entity, view),
            Kind::Enderman => self.enderman_view(entity, view),
            Kind::Endermite => self.endermite_view(entity, view),
            Kind::Silverfish => self.silverfish_view(entity, view),
            Kind::IronGolem => self.iron_golem_view(entity, view),
            Kind::Blaze => self.blaze_view(entity, view),
            Kind::Ghast => self.ghast_view(entity, view),
            Kind::Vex => self.vex_view(entity, view),
            Kind::Wither => self.wither_view(entity, view),
            Kind::Allay => self.allay_view(entity, view),
            Kind::Bat => self.bat_view(entity, view),
            Kind::Cod | Kind::Salmon | Kind::Pufferfish | Kind::TropicalFish => {
                self.fish_view(entity, view);
            }
            Kind::Squid | Kind::GlowSquid => self.squid_view(entity, view),
            Kind::Dolphin => self.dolphin_view(entity, view),
            Kind::Turtle => self.turtle_view(entity, view),
            Kind::Drowned => self.drowned_view(entity, view),
            Kind::Guardian | Kind::ElderGuardian => self.guardian_view(entity, view),
            Kind::Phantom => self.phantom_view(entity, view),
            Kind::Parrot => self.parrot_view(entity, view),
            Kind::Bee => self.bee_view(entity, view),
            Kind::Shulker => self.shulker_view(entity, view),
            Kind::ZombifiedPiglin => self.zombified_piglin_view(entity, view),
            Kind::Rabbit => self.rabbit_view(entity, view),
            Kind::Ocelot => self.ocelot_view(entity, view),
            Kind::Witch => self.witch_view(entity, view),
            Kind::Pillager => self.pillager_view(entity, view),
            Kind::Hoglin | Kind::Zoglin => self.hoglin_view(entity, view),
            Kind::Goat => self.goat_view(entity, view),
            Kind::Armadillo => self.armadillo_view(entity, view),
            Kind::Sniffer => self.sniffer_view(entity, view),
            Kind::Creaking => self.creaking_view(entity, view),
            Kind::Piglin | Kind::PiglinBrute => self.piglin_view(entity, view),
            Kind::Villager => self.villager_view(entity, view),
            Kind::WanderingTrader => self.wandering_trader_view(entity, view),
            _ => {}
        }
    }

    pub(crate) fn kind_registered(&mut self, id: EntityId, ai: &mut rspigot_ai::MobAi) {
        match self.kind_of(id) {
            Some(Kind::Rabbit) => self.rabbit_registered(id, ai),
            Some(Kind::Ocelot) => self.ocelot_registered(id, ai),
            Some(Kind::Piglin | Kind::PiglinBrute) => self.piglin_registered(id, ai),
            Some(Kind::Goat) => self.goat_registered(id, ai),
            Some(Kind::Sniffer) => self.sniffer_registered(id),
            Some(Kind::Frog) => self.frog_registered(id, ai),
            Some(Kind::Axolotl) => self.axolotl_registered(id, ai),
            Some(Kind::Nautilus | Kind::ZombieNautilus) => self.nautilus_registered(id, ai),
            Some(Kind::Villager) => self.villager_registered(id, ai),
            Some(Kind::Warden) => self.warden_registered(id, ai),
            Some(Kind::Wither) => self.wither_registered(id),
            _ => {}
        }
    }

    pub(crate) fn kind_attack_sound(&mut self, id: EntityId) {
        match self.kind_of(id) {
            Some(Kind::Rabbit) => self.rabbit_attack_sound(id),
            Some(Kind::Creaking) => self.creaking_attack_event(id),
            _ => {}
        }
    }

    pub(crate) fn kind_pre_tick(&mut self, id: EntityId) -> bool {
        match self.kind_of(id) {
            Some(Kind::Wolf) => self.wolf_pre_tick(id),
            Some(Kind::Cat) => self.cat_pre_tick(id),
            Some(Kind::Fox) => self.fox_pre_tick(id),
            Some(Kind::Panda) => self.panda_pre_tick(id),
            Some(Kind::PolarBear) => self.polar_bear_pre_tick(id),
            Some(Kind::Enderman) => self.enderman_pre_tick(id),
            Some(Kind::Endermite) => self.endermite_pre_tick(id),
            Some(Kind::Silverfish) => self.silverfish_pre_tick(id),
            Some(Kind::IronGolem) => self.iron_golem_pre_tick(id),
            Some(Kind::Blaze) => self.blaze_pre_tick(id),
            Some(Kind::Ghast) => self.ghast_pre_tick(id),
            Some(Kind::Vex) => self.vex_pre_tick(id),
            Some(Kind::ZombifiedPiglin) => self.zombified_piglin_pre_tick(id),
            Some(Kind::ZombieVillager) => self.zombie_villager_pre_tick(id),
            Some(Kind::Rabbit) => self.rabbit_pre_tick(id),
            Some(Kind::Ocelot) => self.ocelot_pre_tick(id),
            Some(Kind::Sniffer) => self.sniffer_pre_tick(id),
            Some(Kind::Pufferfish) => self.fish_pre_tick(id),
            _ => true,
        }
    }

    pub(crate) fn kind_post_tick(&mut self, id: EntityId) {
        match self.kind_of(id) {
            Some(Kind::Wolf) => self.wolf_post_tick(id),
            Some(Kind::Cat) => self.cat_post_tick(id),
            Some(Kind::Fox) => self.fox_post_tick(id),
            Some(Kind::Panda) => self.panda_post_tick(id),
            Some(Kind::PolarBear) => self.polar_bear_post_tick(id),
            Some(Kind::Enderman) => self.enderman_post_tick(id),
            Some(Kind::Endermite) => self.endermite_post_tick(id),
            Some(Kind::Silverfish) => self.silverfish_post_tick(id),
            Some(Kind::IronGolem) => self.iron_golem_post_tick(id),
            Some(Kind::SnowGolem) => self.snow_golem_post_tick(id),
            Some(Kind::Shulker) => self.shulker_post_tick(id),
            Some(Kind::Blaze) => self.blaze_post_tick(id),
            Some(Kind::Vex) => self.vex_post_tick(id),
            Some(Kind::Bat) => self.bat_post_tick(id),
            Some(Kind::Cod | Kind::Salmon | Kind::Pufferfish | Kind::TropicalFish) => {
                self.fish_post_tick(id);
            }
            Some(Kind::Squid | Kind::GlowSquid) => self.squid_post_tick(id),
            Some(Kind::Dolphin) => self.dolphin_post_tick(id),
            Some(Kind::Drowned) => self.drowned_post_tick(id),
            Some(Kind::Parrot) => self.parrot_post_tick(id),
            Some(Kind::ZombifiedPiglin) => self.zombified_piglin_post_tick(id),
            Some(Kind::Rabbit) => self.rabbit_post_tick(id),
            Some(Kind::Ocelot) => self.ocelot_post_tick(id),
            Some(Kind::Hoglin | Kind::Zoglin) => self.hoglin_post_tick(id),
            Some(Kind::Armadillo) => self.armadillo_post_tick(id),
            Some(Kind::Creaking) => self.creaking_post_tick(id),
            Some(Kind::CopperGolem) => self.copper_golem_post_tick(id),
            Some(Kind::Warden) => self.warden_post_tick(id),
            Some(Kind::Piglin | Kind::PiglinBrute) => self.piglin_post_tick(id),
            _ => {}
        }
    }

    pub(crate) fn kind_ai_step(&mut self, id: EntityId) {
        match self.kind_of(id) {
            Some(Kind::Fox) => self.fox_ai_step(id),
            Some(Kind::Tadpole) => self.tadpole_ai_step(id),
            Some(Kind::Enderman) => self.enderman_ai_step(id),
            Some(Kind::Blaze) => self.blaze_ai_step(id),
            Some(Kind::Witch) => self.witch_ai_step(id),
            Some(Kind::Creaking) => self.creaking_ai_step(id),
            Some(Kind::Parrot) => self.parrot_ai_step(id),
            Some(Kind::Turtle) => self.turtle_ai_step(id),
            Some(Kind::Guardian | Kind::ElderGuardian) => self.guardian_ai_step(id),
            Some(Kind::Bee) => self.bee_ai_step(id),
            Some(Kind::Wither) => self.wither_ai_step(id),
            Some(Kind::Allay) => self.allay_ai_step(id),
            Some(Kind::Cod | Kind::Salmon | Kind::Pufferfish | Kind::TropicalFish) => {
                self.fish_ai_step(id);
            }
            _ => {}
        }
    }

    pub(crate) fn kind_death_item(&mut self, id: EntityId) -> Option<ItemStack> {
        match self.kind_of(id) {
            Some(Kind::Fox) => self.fox_death_item(id),
            _ => None,
        }
    }

    pub(crate) fn kind_death_extra_drops(&mut self, id: EntityId) -> Vec<ItemStack> {
        match self.kind_of(id) {
            Some(Kind::Piglin) => self.piglin_death_drops(id),
            Some(Kind::Wither) => self.wither_death_drops(id),
            Some(Kind::Allay) => self.allay_death_drops(id),
            _ => Vec::new(),
        }
    }

    pub(crate) fn kind_target_cleared(&mut self, id: EntityId) {
        if self.kind_of(id) == Some(Kind::Fox) {
            self.fox_target_cleared(id);
        }
    }

    pub(crate) fn kind_target_changed(&mut self, id: EntityId, target: Option<EntityId>) {
        if let Some(Kind::Enderman) = self.kind_of(id) {
            self.enderman_target_changed(id, target);
        }
    }

    pub(crate) fn play_ambient_sound(&mut self, id: EntityId) {
        match self.kind_of(id) {
            Some(Kind::Fox) => self.fox_ambient(id),
            Some(Kind::Wolf) => self.wolf_ambient_voice(id),
            Some(Kind::Cat) => self.cat_ambient_voice(id),
            Some(Kind::Hoglin | Kind::Zoglin | Kind::Piglin | Kind::PiglinBrute) => {
                self.brain_ambient_voice(id);
            }
            Some(Kind::Villager) => self.villager_ambient_voice(id),
            Some(Kind::Warden) => self.warden_ambient_voice(id),
            Some(Kind::Bat) => self.bat_ambient_voice(id),
            Some(Kind::Parrot) => self.parrot_ambient_voice(id),
            Some(Kind::Allay) => self.allay_ambient_voice(id),
            _ => {
                let game_time = self.world.level.game_time;
                self.play_mob_voice(id, |entity| {
                    crate::mob_life::ambient_sound(entity, game_time)
                });
            }
        }
    }

    pub(crate) fn kind_custom_ai_step(&mut self, id: EntityId) {
        match self.kind_of(id) {
            Some(Kind::Wolf) => self.wolf_custom_ai_step(id),
            Some(Kind::Cat) => self.cat_custom_ai_step(id),
            Some(Kind::Fox) => self.fox_custom_ai_step(id),
            Some(Kind::Panda) => self.panda_custom_ai_step(id),
            Some(Kind::PolarBear) => self.polar_bear_custom_ai_step(id),
            Some(Kind::Enderman) => self.enderman_custom_ai_step(id),
            Some(Kind::Endermite) => self.endermite_custom_ai_step(id),
            Some(Kind::Silverfish) => self.silverfish_custom_ai_step(id),
            Some(Kind::IronGolem) => self.iron_golem_custom_ai_step(id),
            Some(Kind::Blaze) => self.blaze_custom_ai_step(id),
            Some(Kind::ZombifiedPiglin) => self.zombified_piglin_custom_ai_step(id),
            Some(Kind::Rabbit) => self.rabbit_custom_ai_step(id),
            Some(Kind::Ocelot) => self.ocelot_custom_ai_step(id),
            Some(Kind::Hoglin | Kind::Zoglin) => self.hoglin_custom_ai_step(id),
            Some(Kind::Piglin | Kind::PiglinBrute) => self.piglin_custom_ai_step(id),
            Some(Kind::Goat) => self.goat_custom_ai_step(id),
            Some(Kind::Armadillo) => self.armadillo_custom_ai_step(id),
            Some(Kind::Axolotl) => self.axolotl_post_brain(id),
            Some(Kind::Villager) => self.villager_custom_ai_step(id),
            Some(Kind::WanderingTrader) => self.wandering_trader_custom_ai_step(id),
            Some(Kind::Wither) => self.wither_custom_ai_step(id),
            Some(Kind::Bee) => self.bee_custom_ai_step(id),
            Some(Kind::ElderGuardian) => self.elder_guardian_custom_ai_step(id),
            _ => {}
        }
    }

    pub(crate) fn kind_acquired_target(&mut self, id: EntityId) {
        if let Some(Kind::ZombifiedPiglin) = self.kind_of(id) {
            self.zombified_piglin_acquired_target(id);
        }
    }

    pub(crate) fn kind_interact(
        &mut self,
        player: PlayerId,
        target: EntityId,
        hand: Hand,
        held: Option<&ItemStack>,
    ) -> Option<Interaction> {
        match self.kind_of(target) {
            Some(Kind::Wolf) => self.wolf_interact(player, target, hand, held),
            Some(Kind::Cat) => self.cat_interact(player, target, hand, held),
            Some(Kind::Fox) => self.fox_interact(player, target, hand, held),
            Some(Kind::Panda) => self.panda_interact(player, target, hand, held),
            Some(Kind::PolarBear) => self.polar_bear_interact(player, target, hand, held),
            Some(Kind::Enderman) => self.enderman_interact(player, target, hand, held),
            Some(Kind::Endermite) => self.endermite_interact(player, target, hand, held),
            Some(Kind::Silverfish) => self.silverfish_interact(player, target, hand, held),
            Some(Kind::IronGolem) => self.iron_golem_interact(player, target, hand, held),
            Some(Kind::SnowGolem) => self.snow_golem_interact(player, target, hand, held),
            Some(Kind::ZombifiedPiglin) => {
                self.zombified_piglin_interact(player, target, hand, held)
            }
            Some(Kind::Rabbit) => self.rabbit_interact(player, target, hand, held),
            Some(Kind::Ocelot) => self.ocelot_interact(player, target, hand, held),
            Some(Kind::Piglin) => self.piglin_interact(player, target, hand, held),
            Some(Kind::Armadillo) => self.armadillo_interact(player, target, hand, held),
            Some(Kind::Villager) => self.villager_interact(player, target, hand, held),
            Some(Kind::ZombieVillager) => self.zombie_villager_interact(player, target, hand, held),
            Some(Kind::WanderingTrader) => self.trader_interact(player, target, hand, held),
            Some(Kind::CopperGolem) => self.copper_golem_interact(player, target, hand, held),
            Some(Kind::Parrot) => self.parrot_interact(player, target, hand, held),
            Some(Kind::Bee) => self.bee_interact(player, target, hand, held),
            Some(Kind::Allay) => self.allay_interact(player, target, hand, held),
            Some(Kind::Dolphin) => self.dolphin_interact(player, target, hand, held),
            Some(Kind::Tadpole) => self.tadpole_interact(player, target, hand, held),
            Some(Kind::Axolotl) => self.fish_interact(player, target, hand, held),
            Some(Kind::Nautilus | Kind::ZombieNautilus) => {
                self.nautilus_interact(player, target, hand, held)
            }
            Some(Kind::Cod | Kind::Salmon | Kind::Pufferfish | Kind::TropicalFish) => {
                self.fish_interact(player, target, hand, held)
            }
            _ => None,
        }
    }

    pub(crate) fn kind_hurt_amount(
        &mut self,
        id: EntityId,
        source: &DamageSource,
        amount: f32,
    ) -> Option<f32> {
        match self.kind_of(id) {
            Some(Kind::Wolf) => self.wolf_hurt_amount(id, source, amount),
            Some(Kind::Guardian | Kind::ElderGuardian) => {
                self.guardian_hurt_amount(id, source, amount)
            }
            Some(Kind::Cat) => self.cat_hurt_amount(id, source, amount),
            Some(Kind::Fox) => self.fox_hurt_amount(id, source, amount),
            Some(Kind::Panda) => self.panda_hurt_amount(id, source, amount),
            Some(Kind::PolarBear) => self.polar_bear_hurt_amount(id, source, amount),
            Some(Kind::Enderman) => self.enderman_hurt_amount(id, source, amount),
            Some(Kind::Endermite) => self.endermite_hurt_amount(id, source, amount),
            Some(Kind::Silverfish) => self.silverfish_hurt_amount(id, source, amount),
            Some(Kind::IronGolem) => self.iron_golem_hurt_amount(id, source, amount),
            Some(Kind::Ghast) => self.ghast_hurt_amount(id, source, amount),
            Some(Kind::Wither) => self.wither_hurt_amount(id, source, amount),
            Some(Kind::Allay) => self.allay_hurt_amount(id, source, amount),
            Some(Kind::Bat) => self.bat_hurt_amount(id, source, amount),
            Some(Kind::Shulker) => self.shulker_hurt_amount(id, source, amount),
            Some(Kind::ZombifiedPiglin) => self.zombified_piglin_hurt_amount(id, source, amount),
            Some(Kind::Rabbit) => self.rabbit_hurt_amount(id, source, amount),
            Some(Kind::Ocelot) => self.ocelot_hurt_amount(id, source, amount),
            Some(Kind::Armadillo) => Some(self.armadillo_hurt_amount(id, amount)),
            Some(Kind::Axolotl) => self.axolotl_hurt_amount(id, source, amount),
            Some(Kind::Breeze) => self.breeze_hurt_amount(id, source, amount),
            Some(Kind::Creaking) => self.creaking_hurt_amount(id, source, amount),
            Some(Kind::Warden) if self.warden_hurt_amount(id, source) => None,
            _ => Some(amount),
        }
    }

    pub(crate) fn kind_absorbs_hurt(
        &mut self,
        id: EntityId,
        source: &DamageSource,
        amount: f32,
    ) -> bool {
        match self.kind_of(id) {
            Some(Kind::Wolf) => self.wolf_absorbs_hurt(id, source, amount),
            _ => false,
        }
    }

    pub(crate) fn kind_hurt_settled(&mut self, id: EntityId, source: &DamageSource) {
        if let Some(Kind::Enderman) = self.kind_of(id) {
            self.enderman_hurt_settled(id, source);
        }
    }

    pub(crate) fn kind_after_hurt(&mut self, id: EntityId, source: &DamageSource, dead: bool) {
        match self.kind_of(id) {
            Some(Kind::Wolf) => self.wolf_after_hurt(id, source, dead),
            Some(Kind::Cat) => self.cat_after_hurt(id, source, dead),
            Some(Kind::Fox) => self.fox_after_hurt(id, source, dead),
            Some(Kind::Panda) => self.panda_after_hurt(id, source, dead),
            Some(Kind::PolarBear) => self.polar_bear_after_hurt(id, source, dead),
            Some(Kind::Enderman) => self.enderman_after_hurt(id, source, dead),
            Some(Kind::Endermite) => self.endermite_after_hurt(id, source, dead),
            Some(Kind::Silverfish) => self.silverfish_after_hurt(id, source, dead),
            Some(Kind::IronGolem) => self.iron_golem_after_hurt(id, source, dead),
            Some(Kind::ZombifiedPiglin) => self.zombified_piglin_after_hurt(id, source, dead),
            Some(Kind::Rabbit) => self.rabbit_after_hurt(id, source, dead),
            Some(Kind::Ocelot) => self.ocelot_after_hurt(id, source, dead),
            Some(Kind::Shulker) => self.shulker_after_hurt(id, source, dead),
            Some(Kind::Hoglin | Kind::Zoglin) => self.hoglin_after_hurt(id, source, dead),
            Some(Kind::Piglin | Kind::PiglinBrute) => self.piglin_after_hurt(id, source, dead),
            Some(Kind::Armadillo) => self.armadillo_after_hurt(id, source, dead),
            Some(Kind::Nautilus | Kind::ZombieNautilus) => self.nautilus_after_hurt(id, source),
            Some(Kind::Sniffer) if dead => self.sniffer_died(id),
            Some(Kind::Parrot) => self.parrot_after_hurt(id, dead),
            Some(Kind::Bee) => self.bee_after_hurt(id),
            Some(Kind::Squid | Kind::GlowSquid) => self.squid_after_hurt(id, source),
            Some(Kind::Villager) => self.villager_after_hurt(id, source, dead),
            Some(Kind::Warden) if !dead => self.warden_after_hurt(id, source),
            Some(Kind::CopperGolem) => self.copper_golem_after_hurt(id, !dead),
            Some(Kind::WanderingTrader) if dead => self.merchant_stop_trading(id),
            _ => {}
        }
    }

    pub(crate) fn kind_death_sound(&mut self, id: EntityId) -> bool {
        match self.kind_of(id) {
            Some(Kind::Goat) => self.goat_voice_sound(id, GoatSound::Death),
            Some(Kind::CopperGolem) => self.copper_golem_voice(id, copper_golem::GolemVoice::Death),
            _ => false,
        }
    }

    pub(crate) fn kind_hurt_sound(&mut self, id: EntityId) -> bool {
        match self.kind_of(id) {
            Some(Kind::Shulker) => self.shulker_hurt_sound(id),
            Some(Kind::Goat) => self.goat_voice_sound(id, GoatSound::Hurt),
            Some(Kind::CopperGolem) => self.copper_golem_voice(id, copper_golem::GolemVoice::Hurt),
            Some(Kind::Armadillo) => self.armadillo_hurt_sound(id),
            _ => false,
        }
    }
}

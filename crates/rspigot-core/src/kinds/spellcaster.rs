use rspigot_ai::{MobView, SpellAction, SpellFacts};
use rspigot_domain::effect::{EffectDuration, EffectId, EffectInstance};
use rspigot_domain::entity::living;
use rspigot_domain::entity::species::spellcaster::{self, IllagerSpell};
use rspigot_domain::entity::species::{self, Species, evoker, illusioner, vex};
use rspigot_domain::entity::{Entity, EntityState, fangs};
use rspigot_domain::game_event::GameEvent;
use rspigot_domain::random::RandomSource;
use rspigot_domain::spawn::SpawnReason;
use rspigot_domain::{BlockPos, EntityId, Vec3, mth};
use rspigot_entities::EntityKind as Kind;

use crate::game_events::GameEventContext;
use crate::mob_actions::new_mob;
use crate::mobs::ServerRandom;
use crate::plugins::events::{
    CreatureSpawnReason, EntityPotionEffectCause, EntitySpellCast, SpellcasterSpell,
};
use crate::plugins::steps::SpellStep;
use crate::plugins::{Birth, Continuation, EventPayload};
use crate::ports::ClientLink;
use crate::server::Server;

const NORMAL_DIFFICULTY: f32 = 2.0;
const BLINDNESS: &str = "minecraft:blindness";
const INVISIBILITY: &str = "minecraft:invisibility";

pub(crate) const fn cast_sound(kind: Kind) -> &'static str {
    match kind {
        Kind::Illusioner => illusioner::CAST_SOUND,
        _ => evoker::CAST_SOUND,
    }
}

const fn wire_spell(spell: IllagerSpell) -> SpellcasterSpell {
    match spell {
        IllagerSpell::None => SpellcasterSpell::None,
        IllagerSpell::SummonVex => SpellcasterSpell::SummonVex,
        IllagerSpell::Fangs => SpellcasterSpell::Fangs,
        IllagerSpell::Wololo => SpellcasterSpell::Wololo,
        IllagerSpell::Disappear => SpellcasterSpell::Disappear,
        IllagerSpell::Blindness => SpellcasterSpell::Blindness,
    }
}

fn spell_state_mut(
    entity: &mut Entity,
) -> Option<(&mut rspigot_domain::entity::EntityBase, &mut Species)> {
    let Entity {
        base,
        state: EntityState::Living(living),
        ..
    } = entity
    else {
        return None;
    };
    Some((base, &mut living.mob.as_mut()?.species))
}

fn effect(name: &str, ticks: u32) -> Option<EffectInstance> {
    Some(EffectInstance::new(
        EffectId::from_name(name)?,
        EffectDuration::Ticks(ticks),
        0,
        true,
    ))
}

impl<L: ClientLink> Server<L> {
    pub(crate) fn spellcaster_view(&self, entity: &Entity, view: &mut MobView) {
        self.witch_view(entity, view);
        let Some(living) = entity.living() else {
            return;
        };
        let Some(mob) = living.mob.as_ref() else {
            return;
        };
        let Some(state) = mob.species.spellcaster() else {
            return;
        };
        let wololo_target = match mob.species {
            Species::Evoker(evoker) => evoker.wololo_target,
            _ => None,
        };
        let difficulty = self.difficulty_at(entity.base.position.block());
        view.species.spell = SpellFacts {
            casting_ticks: state.casting_ticks,
            wololo_target,
            invisible: living.effect(INVISIBILITY).is_some(),
            harder_than_normal: difficulty.is_harder_than(NORMAL_DIFFICULTY),
        };
    }

    pub(crate) fn spellcaster_custom_ai_step(&mut self, id: EntityId) {
        if let Some((_, species)) = self.entities.store.get_mut(id).and_then(spell_state_mut)
            && let Some(state) = species.spellcaster_mut()
        {
            state.tick();
        }
    }

    pub(crate) fn spell_action(&mut self, id: EntityId, action: SpellAction) {
        match action {
            SpellAction::Begin {
                spell,
                casting_ticks,
            } => self.set_spell(id, spell, Some(casting_ticks)),
            SpellAction::End => self.set_spell(id, IllagerSpell::None, None),
            SpellAction::WololoTarget(target) => {
                if let Some((_, Species::Evoker(state))) =
                    self.entities.store.get_mut(id).and_then(spell_state_mut)
                {
                    state.wololo_target = target;
                }
            }
            SpellAction::Perform(spell) => self.ask_spell_cast(id, spell),
        }
    }

    fn set_spell(&mut self, id: EntityId, spell: IllagerSpell, casting_ticks: Option<i32>) {
        let Some((base, species)) = self.entities.store.get_mut(id).and_then(spell_state_mut)
        else {
            return;
        };
        let Some(state) = species.spellcaster_mut() else {
            return;
        };
        if let Some(ticks) = casting_ticks {
            state.casting_ticks = ticks;
        }
        spellcaster::set_casting(base, state, spell);
    }

    fn ask_spell_cast(&mut self, id: EntityId, spell: IllagerSpell) {
        let lane = self.lane_for(None, id);
        let step = SpellStep { caster: id, spell };
        self.fire(lane, Continuation::SpellCast(step), move |server| {
            let entity = server.entity_view(id)?;
            Some(EventPayload::EntitySpellCast(EntitySpellCast {
                entity_type: entity.kind.clone(),
                entity,
                spell: wire_spell(spell),
            }))
        });
    }

    pub(crate) fn finish_spell_cast(&mut self, step: SpellStep) {
        let SpellStep { caster, spell } = step;
        let Some(kind) = self.kind_of(caster) else {
            return;
        };
        match spell {
            IllagerSpell::SummonVex => self.summon_vexes(caster),
            IllagerSpell::Fangs => self.cast_fangs(caster),
            IllagerSpell::Wololo => self.cast_wololo(caster),
            IllagerSpell::Disappear => {
                if let Some(effect) = effect(INVISIBILITY, illusioner::INVISIBILITY_TICKS) {
                    self.add_effect_to(caster, effect, EntityPotionEffectCause::Illusion);
                }
            }
            IllagerSpell::Blindness => {
                if let Some(target) = self.mob_target(caster)
                    && let Some(effect) = effect(BLINDNESS, illusioner::BLINDNESS_TICKS)
                {
                    self.add_effect_to(target, effect, EntityPotionEffectCause::Attack);
                }
            }
            IllagerSpell::None => {}
        }
        self.entity_sound(caster, cast_sound(kind), 1.0, 1.0);
    }

    fn summon_vexes(&mut self, caster: EntityId) {
        let Some(origin) = self
            .entities
            .store
            .get(caster)
            .map(|entity| entity.base.position.block())
        else {
            return;
        };
        let owner = self.entities.store.get(caster).map(|e| e.base.uuid);
        let mut random = self.mob_random(caster);
        for _ in 0..evoker::SUMMON_VEX_COUNT {
            let position = evoker::summon_position(origin, &mut random);
            let life = self.summon_vex(position, owner, &mut random);
            if life.is_some() {
                self.game_event(
                    GameEvent::EntityPlace,
                    block_center(position),
                    GameEventContext::of(Some(caster)),
                );
            }
        }
        self.store_mob_random(caster, random);
    }

    fn summon_vex(
        &mut self,
        position: BlockPos,
        owner: Option<uuid::Uuid>,
        random: &mut impl RandomSource,
    ) -> Option<()> {
        let mut entity = new_mob(self.new_entity_base(Kind::Vex));
        entity.base.snap_to(
            Vec3::new(
                f64::from(position.x) + 0.5,
                f64::from(position.y),
                f64::from(position.z) + 0.5,
            ),
            0.0,
            0.0,
        );
        let environment = self.spawn_environment(entity.base.position);
        let mut own = rspigot_domain::random::LegacyRandom::new(rand::Rng::random(&mut self.rng));
        rspigot_domain::entity::spawn::finalize_spawn(
            &mut entity,
            &environment,
            SpawnReason::MobSummoned,
            None,
            &mut ServerRandom(&mut self.rng),
            &mut own,
        );
        let life = evoker::summon_life_ticks(random);
        if let Some(Species::Vex(state)) = entity
            .living_mut()
            .and_then(|living| living.mob.as_mut())
            .map(|mob| &mut mob.species)
        {
            state.owner = owner;
            state.bound_origin = Some(position);
            vex::set_limited_life(state, life);
        }
        self.spawn_entity(
            entity,
            Birth::of(CreatureSpawnReason::Spell).with_random(own),
        );
        Some(())
    }

    fn cast_fangs(&mut self, caster: EntityId) {
        let Some(target) = self.mob_target(caster) else {
            return;
        };
        let Some((target_position, _, _)) = self.entity_box(target) else {
            return;
        };
        let Some(source) = self.entities.store.get(caster) else {
            return;
        };
        let origin = source.base.position;
        let owner = source.base.uuid;
        let min_y = target_position.y.min(origin.y);
        let max_y = target_position.y.max(origin.y) + evoker::FANGS_Y_RANGE_TOP;
        let angle = mth::atan2(target_position.z - origin.z, target_position.x - origin.x) as f32;
        let close = origin.distance_squared(target_position) < evoker::FANGS_NEAR_DISTANCE_SQR;
        for spawn in evoker::fang_pattern(origin.x, origin.z, angle, close) {
            let Some(floor) =
                evoker::fang_floor(&self.world.chunks, spawn.x, spawn.z, min_y, max_y)
            else {
                continue;
            };
            let at = Vec3::new(spawn.x, floor, spawn.z);
            self.place_fangs(at, spawn.angle, spawn.delay, owner);
            self.game_event(
                GameEvent::EntityPlace,
                at,
                GameEventContext::of(Some(caster)),
            );
        }
    }

    fn place_fangs(&mut self, at: Vec3, angle: f32, delay: i32, owner: uuid::Uuid) {
        let mut base = self.new_entity_base(Kind::EvokerFangs);
        base.snap_to(at, angle * fangs::RAD_TO_DEG, 0.0);
        base.old_position = at;
        let state = fangs::FangsState::new(delay, Some(owner));
        let entity = Entity::new(base, EntityState::Fangs(state));
        self.add_entity(entity, CreatureSpawnReason::Default);
    }

    fn cast_wololo(&mut self, caster: EntityId) {
        let target = match self.entities.store.get(caster).and_then(|entity| {
            match entity.living()?.mob.as_ref()?.species {
                Species::Evoker(state) => state.wololo_target,
                _ => None,
            }
        }) {
            Some(target) => target,
            None => return,
        };
        let Some(sheep) = self.entities.store.get_mut(target) else {
            return;
        };
        let alive = sheep
            .living()
            .is_some_and(|state| !living::is_dead_or_dying(&sheep.base, state));
        if sheep.base.kind == Kind::Sheep && alive {
            species::set_wool_color(&mut sheep.base, evoker::WOLOLO_TO_COLOR);
        }
    }
}

fn block_center(pos: BlockPos) -> Vec3 {
    Vec3::new(
        f64::from(pos.x) + 0.5,
        f64::from(pos.y) + 0.5,
        f64::from(pos.z) + 0.5,
    )
}

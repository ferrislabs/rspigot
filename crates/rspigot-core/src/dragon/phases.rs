use std::collections::VecDeque;

use rspigot_domain::EntityId;
use rspigot_domain::dragon::phase::{
    DragonAction, DragonPhase, Followup, PhaseChange, PhaseOutput,
};
use rspigot_domain::entity::species::ender_dragon;
use rspigot_domain::entity::{Entity, EntityState, living};
use rspigot_entities::EntityKind as Kind;

use super::env::DragonWorld;
use crate::plugins::events::{
    EnderDragonChangePhase, EnderDragonChangePhasePatch, EnderDragonPhase,
};
use crate::plugins::steps::DragonPhaseStep;
use crate::plugins::{Continuation, EventPayload};
use crate::ports::ClientLink;
use crate::server::Server;

pub(crate) fn bukkit_phase(phase: DragonPhase) -> EnderDragonPhase {
    match phase {
        DragonPhase::HoldingPattern => EnderDragonPhase::Circling,
        DragonPhase::StrafePlayer => EnderDragonPhase::Strafing,
        DragonPhase::LandingApproach => EnderDragonPhase::FlyToPortal,
        DragonPhase::Landing => EnderDragonPhase::LandOnPortal,
        DragonPhase::Takeoff => EnderDragonPhase::LeavePortal,
        DragonPhase::SittingFlaming => EnderDragonPhase::BreathAttack,
        DragonPhase::SittingScanning => EnderDragonPhase::SearchForBreathAttackTarget,
        DragonPhase::SittingAttacking => EnderDragonPhase::RoarBeforeAttack,
        DragonPhase::ChargingPlayer => EnderDragonPhase::ChargePlayer,
        DragonPhase::Dying => EnderDragonPhase::Dying,
        DragonPhase::Hovering => EnderDragonPhase::Hover,
    }
}

pub(crate) fn phase_of_bukkit(phase: EnderDragonPhase) -> DragonPhase {
    match phase {
        EnderDragonPhase::Circling => DragonPhase::HoldingPattern,
        EnderDragonPhase::Strafing => DragonPhase::StrafePlayer,
        EnderDragonPhase::FlyToPortal => DragonPhase::LandingApproach,
        EnderDragonPhase::LandOnPortal => DragonPhase::Landing,
        EnderDragonPhase::LeavePortal => DragonPhase::Takeoff,
        EnderDragonPhase::BreathAttack => DragonPhase::SittingFlaming,
        EnderDragonPhase::SearchForBreathAttackTarget => DragonPhase::SittingScanning,
        EnderDragonPhase::RoarBeforeAttack => DragonPhase::SittingAttacking,
        EnderDragonPhase::ChargePlayer => DragonPhase::ChargingPlayer,
        EnderDragonPhase::Dying => DragonPhase::Dying,
        EnderDragonPhase::Hover => DragonPhase::Hovering,
    }
}

impl<L: ClientLink> Server<L> {
    pub(super) fn dragon_apply_output(&mut self, id: EntityId, output: PhaseOutput) {
        for action in output.actions {
            self.dragon_action(id, action);
        }
        if !output.changes.is_empty() {
            self.dragon_change_phases(id, output.changes.into());
        }
    }

    fn dragon_action(&mut self, id: EntityId, action: DragonAction) {
        match action {
            DragonAction::LevelEvent { event, pos } => self.level_event(event, pos, 0),
            DragonAction::Fireball { from, direction } => {
                self.launch_hurting(Kind::DragonFireball, id, direction.normalize(), from, 1);
            }
            DragonAction::SetHealth(health) => self.dragon_set_health(id, health),
            DragonAction::SpawnFlame { position } => self.dragon_spawn_flame(id, position),
            DragonAction::DiscardFlame => self.dragon_discard_flame(id),
        }
    }

    pub(super) fn dragon_set_health(&mut self, id: EntityId, health: f32) {
        if let Some(Entity {
            base,
            state: EntityState::Living(state),
            ..
        }) = self.entities.store.get_mut(id)
        {
            living::set_health(base, state, health);
        }
    }

    pub(crate) fn dragon_request_phase(&mut self, id: EntityId, phase: DragonPhase) {
        self.dragon_change_phases(id, VecDeque::from([PhaseChange::to(phase)]));
    }

    pub(super) fn dragon_change_with(&mut self, id: EntityId, change: PhaseChange) {
        self.dragon_change_phases(id, VecDeque::from([change]));
    }

    fn dragon_change_phases(&mut self, id: EntityId, mut queue: VecDeque<PhaseChange>) {
        while let Some(change) = queue.pop_front() {
            let Some(current) = self.dragon_phase(id) else {
                return;
            };
            if current == change.to {
                if let Some(followup) = change.then {
                    self.dragon_followup(id, followup);
                }
                continue;
            }
            if let Some(runtime) = self.entities.dragons.get_mut(&id) {
                runtime.pending_phase = true;
            }
            let Some(view) = self.entity_view(id) else {
                return;
            };
            let lane = self.lane_for(None, id);
            let step = DragonPhaseStep {
                dragon: id,
                change,
                rest: queue,
            };
            self.fire(lane, Continuation::DragonPhase(Box::new(step)), move |_| {
                Some(EventPayload::EnderDragonChangePhase(
                    EnderDragonChangePhase {
                        current_phase: Some(bukkit_phase(current)),
                        entity_type: view.kind.clone(),
                        entity: view,
                        new_phase: bukkit_phase(change.to),
                    },
                ))
            });
            return;
        }
    }

    pub(crate) fn finish_dragon_phase(
        &mut self,
        step: DragonPhaseStep,
        cancelled: bool,
        patch: EnderDragonChangePhasePatch,
    ) {
        let DragonPhaseStep {
            dragon,
            change,
            rest,
        } = step;
        if let Some(runtime) = self.entities.dragons.get_mut(&dragon) {
            runtime.pending_phase = false;
        }
        if !cancelled {
            let target = patch.new_phase.map_or(change.to, phase_of_bukkit);
            self.dragon_enter_phase(dragon, target);
        }
        if let Some(followup) = change.then {
            self.dragon_followup(dragon, followup);
        }
        self.dragon_change_phases(dragon, rest);
    }

    fn dragon_enter_phase(&mut self, id: EntityId, phase: DragonPhase) {
        let Some(runtime) = self.entities.dragons.get_mut(&id) else {
            return;
        };
        let actions = runtime.phases.set_phase(phase);
        if let Some(Entity {
            base,
            state: EntityState::Living(living),
            ..
        }) = self.entities.store.get_mut(id)
            && let Some(mob) = living.mob.as_mut()
            && let rspigot_domain::entity::species::Species::EnderDragon(state) = &mut mob.species
        {
            ender_dragon::set_phase(base, state, phase);
        }
        for action in actions {
            self.dragon_action(id, action);
        }
    }

    fn dragon_followup(&mut self, id: EntityId, followup: Followup) {
        let Some(mut runtime) = self.entities.dragons.remove(&id) else {
            return;
        };
        let Some(mut body) = self.dragon_body(id, &runtime) else {
            self.entities.dragons.insert(id, runtime);
            return;
        };
        let crystals = self.dragon_alive_crystals(id);
        let scale = self.dragon_scale(id);
        {
            let mut world = DragonWorld {
                eye: Self::dragon_eye(&body, scale),
                position: body.position,
                server: self,
            };
            runtime.phases.apply(
                followup,
                &mut body,
                &mut runtime.nodes,
                crystals,
                &mut world,
            );
        }
        self.dragon_store_body(id, &body);
        self.entities.dragons.insert(id, runtime);
    }
}

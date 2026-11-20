use rspigot_data::loot::{self, Condition};
use rspigot_domain::{BlockStateId, Vec3};

use crate::commands::Target;
use crate::loot::{LootContext, LootEntity, condition_holds};
use crate::ports::ClientLink;
use crate::server::Server;

const MAX_REFERENCE_DEPTH: usize = 32;

fn offsets(condition: &Condition, out: &mut Vec<[i32; 3]>, depth: usize) {
    match condition {
        Condition::LocationCheck { offset, block, .. } if block.is_some() => {
            if !out.contains(offset) {
                out.push(*offset);
            }
        }
        Condition::Inverted(term) => offsets(term, out, depth),
        Condition::AnyOf(terms) | Condition::AllOf(terms) => {
            terms.iter().for_each(|term| offsets(term, out, depth));
        }
        Condition::Reference(name) if depth < MAX_REFERENCE_DEPTH => {
            if let Some(found) = loot::predicate(name) {
                offsets(found, out, depth + 1);
            }
        }
        _ => {}
    }
}

impl<L: ClientLink> Server<L> {
    fn target_loot_facts(&self, target: Target) -> Option<LootEntity> {
        let entity = match target {
            Target::Player(id) => self.players.get(&id)?.entity_id,
            Target::Entity(id) => id,
        };
        let mut facts = self.entity_loot_facts(entity)?;
        if let Some(holder) = self.score_holder_of(target) {
            let board = &self.scoreboard.board;
            facts.scores = board
                .objectives()
                .filter_map(|objective| {
                    board
                        .score(&holder, &objective.name)
                        .map(|score| (objective.name.clone(), score.value))
                })
                .collect();
        }
        Some(facts)
    }

    pub(crate) fn predicate_result(
        &self,
        id: &str,
        this: Option<Target>,
        origin: Vec3,
    ) -> Option<bool> {
        let condition = loot::predicate(id)?;
        let facts = this.and_then(|target| self.target_loot_facts(target));
        let here = origin.block();
        let mut wanted = Vec::new();
        offsets(condition, &mut wanted, 0);
        let samples: Vec<([i32; 3], BlockStateId)> = wanted
            .into_iter()
            .filter_map(|offset| {
                let pos = rspigot_domain::BlockPos::new(
                    here.x + offset[0],
                    here.y + offset[1],
                    here.z + offset[2],
                );
                self.world.chunks.block(pos).map(|state| (offset, state))
            })
            .collect();
        let biome = rspigot_data::registries::biome_name(self.biome_at(here).0);
        let context = LootContext {
            block: self.world.chunks.block(here),
            this: facts.as_ref(),
            biome,
            day_time: Some(self.world.level.day_time),
            weather: Some((self.world.level.raining, self.world.level.thundering)),
            blocks_at: &samples,
            ..LootContext::default()
        };
        Some(condition_holds(condition, &context, &mut rand::rng()))
    }
}

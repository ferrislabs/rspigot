use rspigot_data::advancements;
use rspigot_domain::player::PlayerId;

use crate::plugins::AdvancementView;
use crate::ports::ClientLink;
use crate::server::Server;

fn namespaced(id: &str) -> String {
    if id.contains(':') {
        id.to_owned()
    } else {
        format!("minecraft:{id}")
    }
}

impl<L: ClientLink> Server<L> {
    pub(super) fn plugin_advancement(
        &mut self,
        id: PlayerId,
        advancement: &str,
        grant: bool,
    ) -> Result<(), String> {
        let key = namespaced(advancement);
        let Some((_, found)) = advancements::find(&key) else {
            return Err(format!("there is no advancement {key}"));
        };
        if grant {
            self.award_advancement(id, &key);
        } else {
            for criterion in found.criteria {
                self.revoke_advancement_criterion(id, &key, criterion.name);
            }
        }
        Ok(())
    }

    pub(super) fn plugin_criterion(
        &mut self,
        id: PlayerId,
        advancement: &str,
        criterion: &str,
        grant: bool,
    ) -> Result<(), String> {
        let key = namespaced(advancement);
        let Some((_, found)) = advancements::find(&key) else {
            return Err(format!("there is no advancement {key}"));
        };
        if !found.criteria.iter().any(|known| known.name == criterion) {
            return Err(format!("{key} has no criterion {criterion}"));
        }
        if grant {
            self.grant_advancement_criterion(id, &key, criterion);
        } else {
            self.revoke_advancement_criterion(id, &key, criterion);
        }
        Ok(())
    }

    pub(super) fn advancement_view(
        &self,
        id: PlayerId,
        advancement: &str,
    ) -> Option<AdvancementView> {
        let key = namespaced(advancement);
        let (_, found) = advancements::find(&key)?;
        if !self.players.contains_key(&id) {
            return None;
        }
        let obtained = self.obtained_criteria(id, &key);
        Some(AdvancementView {
            done: self.advancement_is_done(id, &key),
            criteria_done: obtained.iter().map(|name| (*name).to_owned()).collect(),
            criteria_remaining: found
                .criteria
                .iter()
                .filter(|criterion| !obtained.contains(&criterion.name))
                .map(|criterion| criterion.name.to_owned())
                .collect(),
        })
    }
}

use std::collections::BTreeMap;
use std::sync::{Mutex, PoisonError};

use rspigot_domain::Identifier;

use crate::ports::ClientLink;
use crate::server::Server;

const THE_NETHER: &str = "minecraft:the_nether";

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct GameplayRules {
    pub fast_lava: bool,
    pub water_evaporates: bool,
    pub respawn_anchor_works: bool,
}

static REGISTERED: Mutex<BTreeMap<String, GameplayRules>> = Mutex::new(BTreeMap::new());

pub fn register_gameplay_rules(type_id: &Identifier, rules: GameplayRules) {
    REGISTERED
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .insert(type_id.to_string(), rules);
}

pub fn gameplay_rules(type_id: &Identifier) -> GameplayRules {
    if let Some(rules) = REGISTERED
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .get(type_id.as_str())
        .copied()
    {
        return rules;
    }
    if type_id.as_str() == THE_NETHER {
        GameplayRules {
            fast_lava: true,
            water_evaporates: true,
            respawn_anchor_works: true,
        }
    } else {
        GameplayRules::default()
    }
}

impl<L: ClientLink> Server<L> {
    pub(crate) fn gameplay_rules(&self) -> GameplayRules {
        gameplay_rules(&self.world.dimension.dimension_type)
    }

    pub(crate) fn fast_lava(&self) -> bool {
        self.gameplay_rules().fast_lava
    }

    pub(crate) fn water_evaporates(&self) -> bool {
        self.gameplay_rules().water_evaporates
    }

    pub(crate) fn respawn_anchor_works(&self) -> bool {
        self.gameplay_rules().respawn_anchor_works
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_nether_type_has_the_three_vanilla_attributes_and_the_overworld_none() {
        let nether = gameplay_rules(&Identifier::minecraft("the_nether"));
        assert!(nether.fast_lava && nether.water_evaporates && nether.respawn_anchor_works);
        assert_eq!(
            gameplay_rules(&Identifier::minecraft("overworld")),
            GameplayRules::default()
        );
    }

    #[test]
    fn a_registered_custom_type_carries_its_own_attributes() {
        let id = Identifier::parse("w25_tests:hot").expect("an identifier");
        register_gameplay_rules(
            &id,
            GameplayRules {
                fast_lava: true,
                ..GameplayRules::default()
            },
        );
        let rules = gameplay_rules(&id);
        assert!(rules.fast_lava);
        assert!(!rules.water_evaporates);
    }
}

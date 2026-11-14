use crate::{BlockKind, StateId};

const HOSTS: [(BlockKind, BlockKind); 7] = [
    (BlockKind::Stone, BlockKind::InfestedStone),
    (BlockKind::Cobblestone, BlockKind::InfestedCobblestone),
    (BlockKind::StoneBricks, BlockKind::InfestedStoneBricks),
    (
        BlockKind::MossyStoneBricks,
        BlockKind::InfestedMossyStoneBricks,
    ),
    (
        BlockKind::CrackedStoneBricks,
        BlockKind::InfestedCrackedStoneBricks,
    ),
    (
        BlockKind::ChiseledStoneBricks,
        BlockKind::InfestedChiseledStoneBricks,
    ),
    (BlockKind::Deepslate, BlockKind::InfestedDeepslate),
];

pub fn infested_of(host: BlockKind) -> Option<BlockKind> {
    HOSTS
        .iter()
        .find_map(|(block, infested)| (*block == host).then_some(*infested))
}

pub fn host_of(infested: BlockKind) -> Option<BlockKind> {
    HOSTS
        .iter()
        .find_map(|(host, block)| (*block == infested).then_some(*host))
}

pub fn is_compatible_host_block(state: StateId) -> bool {
    state.kind().and_then(infested_of).is_some()
}

pub fn is_infested(state: StateId) -> bool {
    state.kind().and_then(host_of).is_some()
}

fn with_properties_of(from: StateId, default: BlockKind) -> StateId {
    from.properties()
        .fold(default.default_state(), |state, (name, value)| {
            state.with_property(name, value).unwrap_or(state)
        })
}

pub fn infested_state_by_host(host: StateId) -> Option<StateId> {
    let infested = host.kind().and_then(infested_of)?;
    Some(with_properties_of(host, infested))
}

pub fn host_state_by_infested(infested: StateId) -> Option<StateId> {
    let host = infested.kind().and_then(host_of)?;
    Some(with_properties_of(infested, host))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seven_hosts_are_infestable() {
        for (host, infested) in HOSTS {
            assert!(is_compatible_host_block(host.default_state()));
            assert!(is_infested(infested.default_state()));
            assert!(!is_infested(host.default_state()));
            assert!(!is_compatible_host_block(infested.default_state()));
            assert_eq!(
                infested_state_by_host(host.default_state()),
                Some(infested.default_state())
            );
            assert_eq!(
                host_state_by_infested(infested.default_state()),
                Some(host.default_state())
            );
        }
        assert!(!is_compatible_host_block(
            BlockKind::Andesite.default_state()
        ));
        assert_eq!(
            infested_state_by_host(BlockKind::Dirt.default_state()),
            None
        );
    }

    #[test]
    fn deepslate_keeps_its_axis() {
        let host = BlockKind::Deepslate
            .default_state()
            .with_property("axis", "x")
            .unwrap();
        let infested = infested_state_by_host(host).unwrap();
        assert_eq!(infested.kind(), Some(BlockKind::InfestedDeepslate));
        assert_eq!(infested.property("axis"), Some("x"));
        assert_eq!(host_state_by_infested(infested), Some(host));
    }
}

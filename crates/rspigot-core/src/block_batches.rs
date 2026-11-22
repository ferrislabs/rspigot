use rspigot_domain::{BlockPos, BlockStateId, ChunkPos};

use crate::events::ClientEvent;
use crate::server::Audience;

type SectionKey = (ChunkPos, i32);

fn key_of(
    audience: &Audience,
    event: &ClientEvent,
) -> Option<(SectionKey, BlockPos, BlockStateId)> {
    match (audience, event) {
        (Audience::ChunkViewers(chunk), ClientEvent::BlockUpdate { pos, state }) => {
            Some(((*chunk, pos.y >> 4), *pos, *state))
        }
        _ => None,
    }
}

impl ClientEvent {
    pub fn expanded(self) -> Vec<ClientEvent> {
        match self {
            ClientEvent::SectionBlocks { changes, .. } => changes
                .into_iter()
                .map(|(pos, state)| ClientEvent::BlockUpdate { pos, state })
                .collect(),
            other => vec![other],
        }
    }
}

pub(crate) fn batch_block_updates(outbox: &mut Vec<(Audience, ClientEvent)>) {
    let mut groups: Vec<(SectionKey, Vec<usize>)> = Vec::new();
    for (index, (audience, event)) in outbox.iter().enumerate() {
        let Some((key, _, _)) = key_of(audience, event) else {
            continue;
        };
        match groups.iter_mut().find(|(found, _)| *found == key) {
            Some((_, members)) => members.push(index),
            None => groups.push((key, vec![index])),
        }
    }
    let mut removed = Vec::new();
    for ((chunk, section), members) in groups {
        if members.len() < 2 {
            continue;
        }
        let mut changes: Vec<(BlockPos, BlockStateId)> = Vec::new();
        for index in &members {
            let Some((_, pos, state)) = key_of(&outbox[*index].0, &outbox[*index].1) else {
                continue;
            };
            match changes.iter_mut().find(|(known, _)| *known == pos) {
                Some(entry) => entry.1 = state,
                None => changes.push((pos, state)),
            }
        }
        if changes.len() < 2 {
            continue;
        }
        outbox[members[0]].1 = ClientEvent::SectionBlocks {
            chunk,
            section,
            changes,
        };
        removed.extend(members[1..].iter().copied());
    }
    if removed.is_empty() {
        return;
    }
    let mut index = 0;
    outbox.retain(|_| {
        let keep = !removed.contains(&index);
        index += 1;
        keep
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn update(x: i32, y: i32, z: i32, state: u16) -> (Audience, ClientEvent) {
        let pos = BlockPos::new(x, y, z);
        (
            Audience::ChunkViewers(pos.chunk()),
            ClientEvent::BlockUpdate {
                pos,
                state: BlockStateId(state),
            },
        )
    }

    #[test]
    fn several_changes_in_one_section_become_one_event_at_the_first_position() {
        let mut outbox = vec![
            update(1, 64, 1, 1),
            (Audience::All, ClientEvent::Ping(1)),
            update(2, 65, 1, 2),
            update(1, 64, 1, 3),
        ];
        batch_block_updates(&mut outbox);
        assert_eq!(outbox.len(), 2);
        match &outbox[0].1 {
            ClientEvent::SectionBlocks {
                section, changes, ..
            } => {
                assert_eq!(*section, 4);
                assert_eq!(changes.len(), 2);
                assert_eq!(changes[0].1, BlockStateId(3));
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn a_section_update_expands_back_into_block_updates() {
        let mut outbox = vec![update(1, 64, 1, 1), update(2, 65, 1, 2)];
        batch_block_updates(&mut outbox);
        let event = outbox.remove(0).1;
        assert_eq!(event.expanded().len(), 2);
    }

    #[test]
    fn a_lone_change_or_other_sections_stay_single_updates() {
        let mut outbox = vec![update(1, 64, 1, 1), update(1, 80, 1, 2)];
        batch_block_updates(&mut outbox);
        assert!(
            outbox
                .iter()
                .all(|(_, event)| matches!(event, ClientEvent::BlockUpdate { .. }))
        );
    }
}

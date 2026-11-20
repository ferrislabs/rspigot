use std::collections::{HashSet, VecDeque};

use rspigot_domain::chat_chain::MessageSignature;

pub const CAPACITY: usize = 128;

pub struct SignatureCache {
    entries: Vec<Option<MessageSignature>>,
}

impl Default for SignatureCache {
    fn default() -> Self {
        Self {
            entries: vec![None; CAPACITY],
        }
    }
}

impl SignatureCache {
    pub fn pack(&self, signature: &MessageSignature) -> Option<usize> {
        self.entries
            .iter()
            .position(|entry| entry.as_ref() == Some(signature))
    }

    pub fn push(&mut self, last_seen: &[MessageSignature], signature: &MessageSignature) {
        let mut queue: VecDeque<MessageSignature> = last_seen.iter().copied().collect();
        queue.push_back(*signature);
        let keep: HashSet<MessageSignature> = queue.iter().copied().collect();
        for slot in &mut self.entries {
            let Some(next) = queue.pop_back() else {
                return;
            };
            if let Some(old) = slot.replace(next)
                && !keep.contains(&old)
            {
                queue.push_front(old);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sig(byte: u8) -> MessageSignature {
        MessageSignature([byte; 256])
    }

    #[test]
    fn a_new_signature_takes_index_zero_and_pushes_older_ones_down() {
        let mut cache = SignatureCache::default();
        assert_eq!(cache.pack(&sig(1)), None);
        cache.push(&[], &sig(1));
        assert_eq!(cache.pack(&sig(1)), Some(0));
        cache.push(&[sig(1)], &sig(2));
        assert_eq!(cache.pack(&sig(2)), Some(0));
        assert_eq!(cache.pack(&sig(1)), Some(1));
    }

    #[test]
    fn the_oldest_entry_falls_out_after_the_capacity() {
        let mut cache = SignatureCache::default();
        for byte in 1..=129u8 {
            cache.push(&[], &sig(byte));
        }
        assert_eq!(cache.pack(&sig(129)), Some(0));
        assert_eq!(cache.pack(&sig(2)), Some(127));
        assert_eq!(cache.pack(&sig(1)), None);
    }

    #[test]
    fn entries_the_sender_listed_keep_their_place_ahead_of_unlisted_ones() {
        let mut cache = SignatureCache::default();
        cache.push(&[], &sig(1));
        cache.push(&[], &sig(2));
        cache.push(&[sig(1)], &sig(3));
        assert_eq!(cache.pack(&sig(3)), Some(0));
        assert_eq!(cache.pack(&sig(1)), Some(1));
        assert_eq!(cache.pack(&sig(2)), Some(2));
    }
}

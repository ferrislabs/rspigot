use rspigot_domain::Vec3;

use crate::plugins::resolve::position_of;
use crate::plugins::{EntityView, EventPatch, Location};

#[derive(Debug, Clone, PartialEq)]
pub struct Verdict {
    pub handler: u32,
    pub cancel: bool,
    pub patch: Option<EventPatch>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ListDiff<T, const SET: bool = false> {
    pub removed: Vec<u32>,
    pub replaced: Vec<(u32, T)>,
    pub added: Vec<T>,
}

pub type SetDiff<T> = ListDiff<T, true>;

impl<T, const SET: bool> Default for ListDiff<T, SET> {
    fn default() -> Self {
        Self {
            removed: Vec::new(),
            replaced: Vec::new(),
            added: Vec::new(),
        }
    }
}

impl<T, const SET: bool> ListDiff<T, SET> {
    pub const fn is_empty(&self) -> bool {
        self.removed.is_empty() && self.replaced.is_empty() && self.added.is_empty()
    }

    pub fn keeps(&self, index: usize) -> bool {
        u32::try_from(index).map_or(true, |index| !self.removed.contains(&index))
    }

    pub fn replacement(&self, index: usize) -> Option<&T> {
        let index = u32::try_from(index).ok()?;
        self.replaced
            .iter()
            .find(|(known, _)| *known == index)
            .map(|(_, value)| value)
    }

    #[must_use]
    pub fn then(mut self, later: Self) -> Self {
        for index in later.removed {
            if !self.removed.contains(&index) {
                self.removed.push(index);
            }
        }
        for (index, value) in later.replaced {
            match self.replaced.iter_mut().find(|(known, _)| *known == index) {
                Some((_, known)) => *known = value,
                None => self.replaced.push((index, value)),
            }
        }
        self.added.extend(later.added);
        self
    }

    fn merge<U>(
        self,
        snapshot: Vec<U>,
        mut replace: impl FnMut(U, T) -> Option<U>,
        add: impl FnMut(T) -> Option<U>,
    ) -> Vec<U> {
        if self.is_empty() {
            return snapshot;
        }
        let mut kept = vec![true; snapshot.len()];
        for index in self.removed {
            if let Some(slot) = usize::try_from(index)
                .ok()
                .and_then(|index| kept.get_mut(index))
            {
                *slot = false;
            }
        }
        let mut replacements: Vec<Option<T>> = std::iter::repeat_with(|| None)
            .take(snapshot.len())
            .collect();
        for (index, value) in self.replaced {
            if let Some(slot) = usize::try_from(index)
                .ok()
                .and_then(|index| replacements.get_mut(index))
            {
                *slot = Some(value);
            }
        }
        snapshot
            .into_iter()
            .zip(kept)
            .zip(replacements)
            .filter_map(|((old, kept), new)| match (kept, new) {
                (false, _) => None,
                (true, Some(new)) => replace(old, new),
                (true, None) => Some(old),
            })
            .chain(self.added.into_iter().filter_map(add))
            .collect()
    }

    pub fn refusal(&self, name: &str, len: usize) -> Option<String> {
        self.removed
            .iter()
            .chain(self.replaced.iter().map(|(index, _)| index))
            .find(|index| usize::try_from(**index).map_or(true, |index| index >= len))
            .map(|index| format!("{name} has no element {index}"))
    }

    pub fn slot_refusal(&self, name: &str, len: usize) -> Option<String> {
        if self.removed.is_empty() && self.added.is_empty() {
            self.refusal(name, len)
        } else {
            Some(format!(
                "{name} keeps its length: it only replaces elements in place"
            ))
        }
    }
}

impl<T> ListDiff<T> {
    pub fn apply(self, snapshot: Vec<T>) -> Vec<T> {
        self.merge(snapshot, |_, value| Some(value), Some)
    }

    pub fn apply_with<U>(
        self,
        snapshot: Vec<U>,
        replace: impl FnMut(U, T) -> Option<U>,
        add: impl FnMut(T) -> Option<U>,
    ) -> Vec<U> {
        self.merge(snapshot, replace, add)
    }
}

impl<T: PartialEq> SetDiff<T> {
    pub fn apply(self, snapshot: Vec<T>) -> Vec<T> {
        self.apply_with(snapshot, |_, value| Some(value), Some)
    }

    pub fn apply_with<U: PartialEq>(
        self,
        snapshot: Vec<U>,
        replace: impl FnMut(U, T) -> Option<U>,
        add: impl FnMut(T) -> Option<U>,
    ) -> Vec<U> {
        if self.is_empty() {
            return snapshot;
        }
        let mut unique: Vec<U> = Vec::new();
        for value in self.merge(snapshot, replace, add) {
            if !unique.contains(&value) {
                unique.push(value);
            }
        }
        unique
    }
}

pub(crate) fn location_refusal(location: &Location) -> Option<String> {
    let rotation = location.rotation;
    position_of(location.position).err().or_else(|| {
        (!(rotation.yaw.is_finite() && rotation.pitch.is_finite()))
            .then(|| format!("rotation {} {} is not finite", rotation.yaw, rotation.pitch))
    })
}

pub(crate) fn vector_refusal(name: &str, vector: Vec3) -> Option<String> {
    (!vector.is_finite()).then(|| {
        format!(
            "{name} {} {} {} is not finite",
            vector.x, vector.y, vector.z
        )
    })
}

pub(crate) fn merge_indexed<V>(
    mut earlier: Vec<(EntityView, V)>,
    later: Vec<(EntityView, V)>,
) -> Vec<(EntityView, V)> {
    for (entity, value) in later {
        match earlier.iter_mut().find(|(known, _)| known.id == entity.id) {
            Some(slot) => *slot = (entity, value),
            None => earlier.push((entity, value)),
        }
    }
    earlier
}

pub(crate) fn number_refusal(name: &str, value: f64) -> Option<String> {
    (!value.is_finite()).then(|| format!("{name} {value} is not finite"))
}

const PNG_SIGNATURE: &[u8] = b"\x89PNG\r\n\x1a\n";
const ICON_SIZE: u32 = 64;
const STATUS_CHARS: usize = 32_767;
const ICON_PREFIX: &str = "data:image/png;base64,";

pub(crate) fn text_refusal(name: &str, text: &str, limit: usize) -> Option<String> {
    let length = text.encode_utf16().count();
    (length > limit).then(|| format!("{name} is {length} characters long, more than {limit}"))
}

pub(crate) fn icon_refusal(name: &str, png: &[u8]) -> Option<String> {
    let header = png
        .strip_prefix(PNG_SIGNATURE)
        .and_then(|rest| rest.get(4..16))
        .filter(|header| header.starts_with(b"IHDR"));
    let Some(&[_, _, _, _, w0, w1, w2, w3, h0, h1, h2, h3]) = header else {
        return Some(format!("{name} is not a PNG image"));
    };
    let width = u32::from_be_bytes([w0, w1, w2, w3]);
    let height = u32::from_be_bytes([h0, h1, h2, h3]);
    if width != ICON_SIZE || height != ICON_SIZE {
        return Some(format!(
            "{name} is a {width}x{height} image, not {ICON_SIZE}x{ICON_SIZE}"
        ));
    }
    let encoded = ICON_PREFIX.len() + png.len().div_ceil(3) * 4;
    (encoded > STATUS_CHARS).then(|| {
        format!(
            "{name} takes {encoded} characters once encoded, more than a status answer holds ({STATUS_CHARS})",
        )
    })
}

#[cfg(test)]
mod tests {
    use rspigot_domain::{EntityId, Rotation};
    use uuid::Uuid;

    use super::*;
    use crate::plugins::events::{
        EntityDamageByEntityPatch, EntityDamagePatch, EntityExhaustionPatch, EntityTeleportPatch,
        EventResult, InventoryClickPatch, InventoryDragPatch, PlayerMovePatch, PlayerVelocityPatch,
        PotionSplashPatch, ServerListPingPatch,
    };
    use crate::plugins::events::{MapInitialize, ServerListPing};
    use crate::plugins::{EventKind, EventPayload, ItemSpec};

    fn at(x: f64) -> Location {
        Location {
            world: "minecraft:overworld".to_owned(),
            position: Vec3::new(x, 64.0, 0.0),
            rotation: Rotation::default(),
        }
    }

    const ELSEWHERE: EventPayload = EventPayload::MapInitialize(MapInitialize { map_id: 0 });

    fn moving(to: Option<Location>) -> EventPatch {
        EventPatch::PlayerMove(PlayerMovePatch { to })
    }

    fn diff(
        removed: &[u32],
        replaced: &[(u32, &'static str)],
        added: &[&'static str],
    ) -> ListDiff<&'static str> {
        ListDiff {
            removed: removed.to_vec(),
            replaced: replaced.to_vec(),
            added: added.to_vec(),
        }
    }

    const SNAPSHOT: [&str; 4] = ["alice", "bob", "carol", "dave"];

    #[test]
    fn removals_of_two_handlers_both_apply() {
        let folded = diff(&[1], &[], &[]).then(diff(&[3], &[], &[]));
        assert_eq!(folded.apply(SNAPSHOT.to_vec()), ["alice", "carol"]);
        let folded = diff(&[3], &[], &[]).then(diff(&[1], &[], &[]));
        assert_eq!(folded.apply(SNAPSHOT.to_vec()), ["alice", "carol"]);
    }

    #[test]
    fn additions_append_in_fold_order_after_the_kept_elements() {
        let folded = diff(&[0], &[], &["erin"])
            .then(diff(&[], &[], &["frank", "gina"]))
            .then(diff(&[0, 2], &[], &[]));
        assert_eq!(folded.removed, [0, 2]);
        assert_eq!(
            folded.apply(SNAPSHOT.to_vec()),
            ["bob", "dave", "erin", "frank", "gina"]
        );
    }

    #[test]
    fn an_element_removed_then_added_back_is_present_once() {
        let folded = diff(&[1], &[], &[]).then(diff(&[], &[], &["bob"]));
        assert_eq!(
            folded.apply(SNAPSHOT.to_vec()),
            ["alice", "carol", "dave", "bob"]
        );
    }

    #[test]
    fn an_empty_diff_keeps_the_snapshot() {
        let empty = ListDiff::<&str>::default();
        assert!(empty.is_empty());
        assert!(empty.keeps(0));
        assert_eq!(empty.replacement(0), None);
        assert_eq!(empty.clone().then(empty.clone()), empty);
        assert_eq!(empty.apply(SNAPSHOT.to_vec()), SNAPSHOT);
    }

    #[test]
    fn a_replaced_element_keeps_its_place() {
        let folded = diff(&[], &[(1, "bea")], &["erin"]);
        assert!(!folded.is_empty());
        assert_eq!(folded.replacement(1), Some(&"bea"));
        assert_eq!(
            folded.apply(SNAPSHOT.to_vec()),
            ["alice", "bea", "carol", "dave", "erin"]
        );
    }

    #[test]
    fn a_removal_by_any_handler_wins_over_a_replacement_in_both_orders() {
        let removing = diff(&[0], &[], &[]);
        let replacing = diff(&[], &[(0, "ada")], &[]);
        for folded in [
            removing.clone().then(replacing.clone()),
            replacing.then(removing),
        ] {
            assert!(!folded.keeps(0));
            assert_eq!(folded.apply(SNAPSHOT.to_vec()), ["bob", "carol", "dave"]);
        }
    }

    #[test]
    fn two_replacements_of_one_element_keep_the_later_one() {
        let earlier = diff(&[], &[(2, "cleo"), (3, "dina")], &[]);
        let later = diff(&[], &[(2, "cora")], &[]);
        assert_eq!(
            earlier.clone().then(later.clone()),
            diff(&[], &[(2, "cora"), (3, "dina")], &[])
        );
        assert_eq!(
            later.then(earlier).apply(SNAPSHOT.to_vec()),
            ["alice", "bob", "cleo", "dina"]
        );
    }

    #[test]
    fn a_diff_applies_to_the_server_side_copy_of_the_snapshot() {
        let folded = diff(&[0, 2], &[(1, "bea"), (2, "cleo")], &["erin", "", "frank"]);
        assert!(!folded.keeps(0));
        assert!(folded.keeps(1));
        let lengths = folded.apply_with(
            vec![10, 20, 30, 40],
            |old, name| Some(old + name.len()),
            |name| (!name.is_empty()).then_some(name.len() * 100),
        );
        assert_eq!(lengths, [23, 40, 400, 500]);
    }

    #[test]
    fn a_set_keeps_one_of_each_element_and_a_list_keeps_them_all() {
        let set: SetDiff<&str> = SetDiff {
            removed: vec![0],
            replaced: vec![(1, "carol")],
            added: vec!["erin", "dave", "erin", "alice"],
        };
        assert_eq!(
            set.apply(SNAPSHOT.to_vec()),
            ["carol", "dave", "erin", "alice"]
        );
        let list = diff(&[0], &[(1, "carol")], &["erin", "dave", "erin", "alice"]);
        assert_eq!(
            list.apply(SNAPSHOT.to_vec()),
            ["carol", "carol", "dave", "erin", "dave", "erin", "alice"]
        );
        let ids = SetDiff {
            removed: Vec::new(),
            replaced: Vec::new(),
            added: vec!["bob"],
        };
        assert_eq!(
            ids.apply_with(
                vec![1, 2],
                |_, name| Some(name.len()),
                |name| Some(name.len() - 1)
            ),
            [1, 2]
        );
    }

    #[test]
    fn a_diff_naming_an_element_the_snapshot_lacks_is_refused() {
        assert_eq!(
            diff(&[0, 3], &[(2, "x")], &["erin"]).refusal("recipients", 4),
            None
        );
        assert_eq!(
            diff(&[1, 4], &[], &[]).refusal("recipients", 4),
            Some("recipients has no element 4".to_owned())
        );
        assert_eq!(
            diff(&[], &[(5, "x")], &[]).refusal("recipients", 4),
            Some("recipients has no element 5".to_owned())
        );
    }

    #[test]
    fn an_array_only_takes_replacements() {
        assert_eq!(diff(&[], &[(3, "x")], &[]).slot_refusal("lines", 4), None);
        assert_eq!(
            diff(&[0], &[], &[]).slot_refusal("lines", 4),
            Some("lines keeps its length: it only replaces elements in place".to_owned())
        );
        assert_eq!(
            diff(&[], &[], &["x"]).slot_refusal("lines", 4),
            Some("lines keeps its length: it only replaces elements in place".to_owned())
        );
        assert_eq!(
            diff(&[], &[(4, "x")], &[]).slot_refusal("lines", 4),
            Some("lines has no element 4".to_owned())
        );
    }

    #[test]
    fn a_later_patch_wins_each_field_it_sets_and_keeps_the_others() {
        assert_eq!(
            moving(Some(at(1.0))).then(moving(Some(at(9.0)))),
            moving(Some(at(9.0)))
        );
        assert_eq!(
            moving(Some(at(1.0))).then(moving(None)),
            moving(Some(at(1.0)))
        );
        assert_eq!(
            moving(None).then(moving(Some(at(9.0)))),
            moving(Some(at(9.0)))
        );
    }

    fn png(width: u32, height: u32) -> Vec<u8> {
        let mut png = PNG_SIGNATURE.to_vec();
        png.extend_from_slice(b"\0\0\0\rIHDR");
        png.extend_from_slice(&width.to_be_bytes());
        png.extend_from_slice(&height.to_be_bytes());
        png.extend_from_slice(&[8, 6, 0, 0, 0]);
        png
    }

    fn pinged() -> EventPayload {
        EventPayload::ServerListPing(ServerListPing {
            address: std::net::IpAddr::from([127, 0, 0, 1]),
            hostname: "localhost".to_owned(),
            players: Vec::new(),
            max_players: 20,
            motd: "A Minecraft Server".to_owned(),
            num_players: 0,
            hidden_players: Vec::new(),
        })
    }

    #[test]
    fn a_server_icon_must_be_a_64_pixel_square_png() {
        let pinged = pinged();
        let icon = |png| {
            EventPatch::ServerListPing(ServerListPingPatch {
                server_icon: Some(png),
                ..ServerListPingPatch::default()
            })
        };
        assert_eq!(icon(png(64, 64)).refusal(&pinged), None);
        assert_eq!(
            EventPatch::ServerListPing(ServerListPingPatch::default()).refusal(&pinged),
            None
        );
        assert_eq!(
            icon(png(64, 32)).refusal(&pinged),
            Some("server_icon is a 64x32 image, not 64x64".to_owned())
        );
        let mut heavy = png(64, 64);
        heavy.resize(24_600, 0);
        assert_eq!(
            icon(heavy).refusal(&pinged),
            Some(
                "server_icon takes 32822 characters once encoded, more than a status answer holds (32767)"
                    .to_owned()
            )
        );
        let mut largest = png(64, 64);
        largest.resize(24_558, 0);
        assert_eq!(icon(largest).refusal(&pinged), None);
        let truncated = png(64, 64)[..20].to_vec();
        for broken in [Vec::new(), b"GIF89a".to_vec(), truncated] {
            assert_eq!(
                icon(broken).refusal(&pinged),
                Some("server_icon is not a PNG image".to_owned())
            );
        }
    }

    #[test]
    fn a_patched_motd_longer_than_a_status_answer_is_refused() {
        let motd = |text: String| {
            EventPatch::ServerListPing(ServerListPingPatch {
                motd: Some(text),
                ..ServerListPingPatch::default()
            })
        };
        assert_eq!(motd("é".repeat(32_767)).refusal(&pinged()), None);
        assert_eq!(
            motd("a".repeat(32_768)).refusal(&pinged()),
            Some("motd is 32768 characters long, more than 32767".to_owned())
        );
        assert_eq!(
            motd("🙂".repeat(16_384)).refusal(&pinged()),
            Some("motd is 32768 characters long, more than 32767".to_owned())
        );
    }

    #[test]
    fn a_destination_outside_the_world_is_refused() {
        assert_eq!(moving(Some(at(1.0))).refusal(&ELSEWHERE), None);
        assert_eq!(moving(None).refusal(&ELSEWHERE), None);
        for nowhere in [
            Vec3::new(f64::NAN, 64.0, 0.0),
            Vec3::new(0.0, f64::INFINITY, 0.0),
            Vec3::new(0.0, 64.0, 4.0e7),
        ] {
            let mut location = at(0.0);
            location.position = nowhere;
            assert!(moving(Some(location)).refusal(&ELSEWHERE).is_some());
        }
    }

    #[test]
    fn every_patched_floating_value_must_be_finite() {
        let damage = |damage| {
            EventPatch::EntityDamage(EntityDamagePatch {
                damage: Some(damage),
            })
        };
        assert_eq!(damage(2.5).refusal(&ELSEWHERE), None);
        assert_eq!(
            damage(f64::NAN).refusal(&ELSEWHERE),
            Some("damage NaN is not finite".to_owned())
        );
        let tired = |exhaustion| {
            EventPatch::EntityExhaustion(EntityExhaustionPatch {
                exhaustion: Some(exhaustion),
            })
        };
        assert_eq!(tired(0.5).refusal(&ELSEWHERE), None);
        assert_eq!(
            tired(f32::INFINITY).refusal(&ELSEWHERE),
            Some("exhaustion inf is not finite".to_owned())
        );
        let pushed = |velocity| {
            EventPatch::PlayerVelocity(PlayerVelocityPatch {
                velocity: Some(velocity),
            })
        };
        assert_eq!(pushed(Vec3::new(0.0, 1.0, 0.0)).refusal(&ELSEWHERE), None);
        assert_eq!(
            pushed(Vec3::new(0.0, f64::NAN, 0.0)).refusal(&ELSEWHERE),
            Some("velocity 0 NaN 0 is not finite".to_owned())
        );
        let mut turned = at(1.0);
        turned.rotation = Rotation::new(f32::NAN, 0.0);
        let teleported = |to| EventPatch::EntityTeleport(EntityTeleportPatch { to: Some(to) });
        assert_eq!(teleported(None).refusal(&ELSEWHERE), None);
        assert_eq!(teleported(Some(at(1.0))).refusal(&ELSEWHERE), None);
        assert_eq!(
            teleported(Some(turned)).refusal(&ELSEWHERE),
            Some("rotation NaN 0 is not finite".to_owned())
        );
    }

    fn splashed(id: i32) -> EntityView {
        EntityView {
            id: EntityId(id),
            uuid: Uuid::from_u128(u128::try_from(id).unwrap_or_default()),
            kind: "minecraft:zombie".to_owned(),
            position: Vec3::new(0.0, 64.0, 0.0),
            player: None,
            meta: crate::plugins::MetaSnapshot::default(),
            marks: Default::default(),
        }
    }

    fn intensities(values: &[(i32, f64)]) -> EventPatch {
        EventPatch::PotionSplash(PotionSplashPatch {
            intensities: values
                .iter()
                .map(|&(id, value)| (splashed(id), value))
                .collect(),
        })
    }

    #[test]
    fn an_indexed_patch_keeps_the_last_value_written_for_each_entity() {
        assert_eq!(
            intensities(&[(1, 0.5), (2, 0.2)]).then(intensities(&[(1, 0.9), (3, 1.0)])),
            intensities(&[(1, 0.9), (2, 0.2), (3, 1.0)])
        );
        assert_eq!(intensities(&[(1, 0.5)]).refusal(&ELSEWHERE), None);
        assert_eq!(
            intensities(&[(1, 0.5), (2, f64::NAN)]).refusal(&ELSEWHERE),
            Some("intensities NaN is not finite".to_owned())
        );
    }

    #[test]
    fn a_patch_of_a_parent_event_lifts_into_the_fired_event() {
        let parent = EventPatch::EntityDamage(EntityDamagePatch { damage: Some(2.5) });
        assert_eq!(
            parent.clone().lift(EventKind::EntityDamageByEntity),
            Some(EventPatch::EntityDamageByEntity(
                EntityDamageByEntityPatch { damage: Some(2.5) }
            ))
        );
        assert_eq!(
            parent.clone().lift(EventKind::EntityDamage),
            Some(parent.clone())
        );
        assert_eq!(parent.lift(EventKind::PlayerMove), None);
    }

    #[test]
    fn only_direct_writes_and_results_survive_a_cancel() {
        let stone = ItemSpec::of("minecraft:stone", 1);
        let both = EventPatch::InventoryClick(InventoryClickPatch {
            current_item: Some(None),
            cursor: Some(Some(stone.clone())),
            result: Some(EventResult::Allow),
        });
        assert_eq!(both.clone().when_cancelled(), Some(both));
        assert_eq!(moving(Some(at(1.0))).when_cancelled(), None);
        let dragged = EventPatch::InventoryDrag(InventoryDragPatch {
            cursor: Some(Some(stone)),
            result: Some(EventResult::Deny),
        });
        assert_eq!(
            dragged.when_cancelled(),
            Some(EventPatch::InventoryDrag(InventoryDragPatch {
                cursor: None,
                result: Some(EventResult::Deny),
            }))
        );
        let cursor_only = EventPatch::InventoryDrag(InventoryDragPatch {
            cursor: Some(None),
            ..InventoryDragPatch::default()
        });
        assert_eq!(cursor_only.when_cancelled(), None);
    }
}

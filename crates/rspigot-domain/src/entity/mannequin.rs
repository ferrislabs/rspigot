use rspigot_nbt::{Compound, Tag};

use super::base::EntityBase;
use super::data::{Pose, StateKind, SyncedValue};
use super::save::{component_from_tag, component_to_tag};
use crate::text::Component;

pub const DATA_MAIN_HAND: u8 = 15;
pub const DATA_LAYERS: u8 = 16;
pub const DATA_PROFILE: u8 = 17;
pub const DATA_IMMOVABLE: u8 = 18;
pub const DATA_DESCRIPTION: u8 = 19;
pub const ALL_LAYERS: u8 = 0x7F;
pub const DESCRIPTION_KEY: &str = "entity.minecraft.mannequin.label";
pub const KEYS: &[&str] = &[
    "profile",
    "hidden_layers",
    "main_hand",
    "pose",
    "immovable",
    "hide_description",
    "description",
];

const RIGHT: i32 = 1;
const LEFT: i32 = 0;

fn arm(id: i32) -> SyncedValue {
    SyncedValue::State(StateKind::HumanoidArm, id)
}

pub fn profile(base: &EntityBase) -> Option<Tag> {
    match base.data.get(DATA_PROFILE) {
        Some(SyncedValue::Profile(tag)) if tag.as_compound().is_none_or(|c| !c.is_empty()) => {
            Some(tag.clone())
        }
        _ => None,
    }
}
const LAYERS: [(&str, u8); 7] = [
    ("cape", 0x01),
    ("jacket", 0x02),
    ("left_sleeve", 0x04),
    ("right_sleeve", 0x08),
    ("left_pants_leg", 0x10),
    ("right_pants_leg", 0x20),
    ("hat", 0x40),
];
const POSES: [(&str, Pose); 5] = [
    ("standing", Pose::Standing),
    ("crouching", Pose::Crouching),
    ("swimming", Pose::Swimming),
    ("fall_flying", Pose::FallFlying),
    ("sleeping", Pose::Sleeping),
];

pub fn default_description() -> Component {
    Component::translatable(DESCRIPTION_KEY, Vec::new())
}

pub fn define(base: &mut EntityBase) {
    base.data.define(DATA_MAIN_HAND, arm(RIGHT));
    base.data.define(
        DATA_PROFILE,
        SyncedValue::Profile(Tag::Compound(Compound::new())),
    );
    base.data
        .define(DATA_LAYERS, SyncedValue::Byte(ALL_LAYERS as i8));
    base.data
        .define(DATA_IMMOVABLE, SyncedValue::Boolean(false));
    base.data.define(
        DATA_DESCRIPTION,
        SyncedValue::OptionalComponent(Some(default_description())),
    );
}

pub fn is_immovable(base: &EntityBase) -> bool {
    base.data.boolean(DATA_IMMOVABLE)
}

pub fn set_immovable(base: &mut EntityBase, immovable: bool) {
    base.data
        .set(DATA_IMMOVABLE, SyncedValue::Boolean(immovable));
}

pub fn layers(base: &EntityBase) -> u8 {
    base.data.byte(DATA_LAYERS) as u8
}

pub fn is_left_handed(base: &EntityBase) -> bool {
    base.data.state(DATA_MAIN_HAND) == LEFT
}

pub fn description(base: &EntityBase) -> Option<Component> {
    match base.data.get(DATA_DESCRIPTION) {
        Some(SyncedValue::OptionalComponent(component)) => component.clone(),
        _ => None,
    }
}

pub fn set_description(base: &mut EntityBase, description: Option<Component>) {
    base.data.set(
        DATA_DESCRIPTION,
        SyncedValue::OptionalComponent(description),
    );
}

pub fn pose_by_name(name: &str) -> Option<Pose> {
    POSES
        .iter()
        .find(|(candidate, _)| *candidate == name)
        .map(|(_, pose)| *pose)
}

pub fn pose_name(pose: Pose) -> Option<&'static str> {
    POSES
        .iter()
        .find(|(_, candidate)| *candidate == pose)
        .map(|(name, _)| *name)
}

pub fn read(nbt: &Compound, base: &mut EntityBase) {
    let hidden = nbt.get_list("hidden_layers").map_or(0, |list| {
        list.iter()
            .filter_map(|tag| match tag {
                Tag::String(name) => Some(name.as_str()),
                _ => None,
            })
            .filter_map(|name| LAYERS.iter().find(|(part, _)| *part == name))
            .fold(0, |mask, (_, bit)| mask | bit)
    });
    base.data
        .set(DATA_LAYERS, SyncedValue::Byte((ALL_LAYERS & !hidden) as i8));
    let main = if nbt.get_str("main_hand") == Some("left") {
        LEFT
    } else {
        RIGHT
    };
    base.data.set(DATA_MAIN_HAND, arm(main));
    if let Some(tag) = nbt.get("profile") {
        base.data
            .set(DATA_PROFILE, SyncedValue::Profile(tag.clone()));
    }
    let pose = nbt
        .get_str("pose")
        .and_then(pose_by_name)
        .unwrap_or(Pose::Standing);
    base.set_pose(pose);
    set_immovable(base, nbt.get_bool("immovable").unwrap_or(false));
    let shown = if nbt.get_bool("hide_description").unwrap_or(false) {
        None
    } else {
        Some(
            nbt.get("description")
                .and_then(component_from_tag)
                .unwrap_or_else(default_description),
        )
    };
    set_description(base, shown);
}

pub fn write(base: &EntityBase, out: &mut Compound) {
    let hidden = !layers(base) & ALL_LAYERS;
    out.insert(
        "hidden_layers",
        Tag::List(
            LAYERS
                .iter()
                .filter(|(_, bit)| hidden & bit != 0)
                .map(|(name, _)| Tag::String((*name).to_owned()))
                .collect(),
        ),
    );
    out.insert(
        "main_hand",
        if is_left_handed(base) {
            "left"
        } else {
            "right"
        },
    );
    out.insert("pose", pose_name(base.pose()).unwrap_or("standing"));
    if let Some(tag) = profile(base) {
        out.insert("profile", tag);
    }
    out.insert("immovable", i8::from(is_immovable(base)));
    match description(base) {
        None => {
            out.insert("hide_description", 1i8);
        }
        Some(component) if component != default_description() => {
            out.insert("description", component_to_tag(&component));
        }
        Some(_) => {}
    }
}

#[cfg(test)]
mod tests {
    use rspigot_entities::EntityKind;
    use uuid::Uuid;

    use super::*;
    use crate::EntityId;

    fn mannequin() -> EntityBase {
        let mut base = EntityBase::new(EntityId(1), Uuid::from_u128(1), EntityKind::Mannequin);
        define(&mut base);
        base
    }

    #[test]
    fn a_new_mannequin_shows_all_layers_and_the_default_label() {
        let base = mannequin();
        assert_eq!(layers(&base), ALL_LAYERS);
        assert_eq!(description(&base), Some(default_description()));
        assert!(!is_immovable(&base));
        assert!(!is_left_handed(&base));
    }

    #[test]
    fn every_option_survives_a_read_and_a_write() {
        let mut base = mannequin();
        let nbt = Compound::new()
            .with(
                "hidden_layers",
                Tag::List(vec![
                    Tag::String("cape".to_owned()),
                    Tag::String("hat".to_owned()),
                ]),
            )
            .with("main_hand", "left")
            .with("pose", "crouching")
            .with("immovable", 1i8)
            .with("description", "Guard");
        read(&nbt, &mut base);
        assert_eq!(layers(&base), ALL_LAYERS & !0x41);
        assert!(is_left_handed(&base));
        assert_eq!(base.pose(), Pose::Crouching);
        assert!(is_immovable(&base));
        let mut saved = Compound::new();
        write(&base, &mut saved);
        assert_eq!(saved.get_str("main_hand"), Some("left"));
        assert_eq!(saved.get_str("pose"), Some("crouching"));
        assert_eq!(saved.get_bool("immovable"), Some(true));
        assert_eq!(saved.get_str("description"), Some("Guard"));
        assert_eq!(saved.get_list("hidden_layers").map(<[Tag]>::len), Some(2));
    }

    #[test]
    fn a_hidden_description_is_saved_as_a_flag() {
        let mut base = mannequin();
        read(&Compound::new().with("hide_description", 1i8), &mut base);
        assert_eq!(description(&base), None);
        let mut saved = Compound::new();
        write(&base, &mut saved);
        assert_eq!(saved.get_bool("hide_description"), Some(true));
        assert!(saved.get("description").is_none());
    }

    #[test]
    fn the_profile_is_kept_and_synced_as_a_value() {
        let mut base = mannequin();
        assert!(profile(&base).is_none());
        read(
            &Compound::new().with("profile", Compound::new().with("name", "Notch")),
            &mut base,
        );
        assert!(profile(&base).is_some());
        let mut saved = Compound::new();
        write(&base, &mut saved);
        assert_eq!(
            saved
                .get_compound("profile")
                .and_then(|p| p.get_str("name")),
            Some("Notch")
        );
    }

    #[test]
    fn poses_a_mannequin_cannot_hold_fall_back_to_standing() {
        let mut base = mannequin();
        read(&Compound::new().with("pose", "dying"), &mut base);
        assert_eq!(base.pose(), Pose::Standing);
    }
}

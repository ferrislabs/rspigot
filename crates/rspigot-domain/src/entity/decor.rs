use rspigot_entities::EntityKind;
use rspigot_nbt::{Compound, Tag};
use uuid::Uuid;

use super::base::EntityBase;
use super::data::SyncedValue;
use super::save::{
    block_state_from_nbt, component_from_tag, item_stack_from_nbt, item_stack_to_nbt, read_uuid,
    uuid_tag,
};
use crate::BlockStateId;
use crate::text::Component;

pub const DISPLAY_BILLBOARD: u8 = 15;
pub const DISPLAY_BRIGHTNESS: u8 = 16;
pub const DISPLAY_VIEW_RANGE: u8 = 17;
pub const DISPLAY_SHADOW_RADIUS: u8 = 18;
pub const DISPLAY_SHADOW_STRENGTH: u8 = 19;
pub const DISPLAY_WIDTH: u8 = 20;
pub const DISPLAY_HEIGHT: u8 = 21;
pub const DISPLAY_GLOW_COLOR: u8 = 22;
pub const DISPLAY_TRANSLATION: u8 = 11;
pub const DISPLAY_SCALE: u8 = 12;
pub const DISPLAY_LEFT_ROTATION: u8 = 13;
pub const DISPLAY_RIGHT_ROTATION: u8 = 14;
pub const BLOCK_STATE: u8 = 23;
pub const DISPLAY_START_INTERPOLATION: u8 = 8;
pub const DISPLAY_INTERPOLATION_DURATION: u8 = 9;
pub const DISPLAY_TELEPORT_DURATION: u8 = 10;
pub const TEXT_TEXT: u8 = 23;
pub const TEXT_LINE_WIDTH: u8 = 24;
pub const TEXT_BACKGROUND: u8 = 25;
pub const TEXT_OPACITY: u8 = 26;
pub const TEXT_FLAGS: u8 = 27;
pub const ITEM_STACK: u8 = 23;
pub const ITEM_CONTEXT: u8 = 24;
pub const INTERACTION_WIDTH: u8 = 8;
pub const INTERACTION_HEIGHT: u8 = 9;
pub const INTERACTION_RESPONSE: u8 = 10;
pub const SPAWNER_ITEM: u8 = 8;
pub const DEFAULT_BACKGROUND: i32 = 0x4000_0000;
pub const DEFAULT_LINE_WIDTH: i32 = 200;
pub const ABOUT_TO_SPAWN_LEAD: i64 = 36;
pub const SPAWNER_DELAY: (i32, i32) = (60, 120);

const IDENTITY: [f32; 4] = [0.0, 0.0, 0.0, 1.0];
const SHADOW: i8 = 1;
const SEE_THROUGH: i8 = 2;
const DEFAULT_BACKGROUND_FLAG: i8 = 4;
const ALIGN_LEFT: i8 = 8;
const ALIGN_RIGHT: i8 = 16;
const BILLBOARDS: [&str; 4] = ["fixed", "vertical", "horizontal", "center"];
const CONTEXTS: [&str; 9] = [
    "none",
    "thirdperson_lefthand",
    "thirdperson_righthand",
    "firstperson_lefthand",
    "firstperson_righthand",
    "head",
    "gui",
    "ground",
    "fixed",
];
const INTERACTION_KEYS: &[&str] = &["interaction", "attack"];
const SPAWNER_KEYS: &[&str] = &["item", "spawn_item_after_ticks"];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlayerAction {
    pub player: Uuid,
    pub timestamp: i64,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct DecorState {
    pub spawn_item_after_ticks: i64,
    pub interaction: Option<PlayerAction>,
    pub attack: Option<PlayerAction>,
}

pub fn is_display(kind: EntityKind) -> bool {
    matches!(
        kind,
        EntityKind::TextDisplay | EntityKind::BlockDisplay | EntityKind::ItemDisplay
    )
}

pub fn is_decor(kind: EntityKind) -> bool {
    is_display(kind)
        || matches!(
            kind,
            EntityKind::Interaction | EntityKind::OminousItemSpawner
        )
}

pub fn keys(kind: EntityKind) -> &'static [&'static str] {
    match kind {
        EntityKind::Interaction => INTERACTION_KEYS,
        EntityKind::OminousItemSpawner => SPAWNER_KEYS,
        _ => &[],
    }
}

pub fn define(base: &mut EntityBase) -> DecorState {
    let kind = base.kind;
    if is_display(kind) {
        let data = &mut base.data;
        data.define(DISPLAY_START_INTERPOLATION, SyncedValue::Int(0));
        data.define(DISPLAY_INTERPOLATION_DURATION, SyncedValue::Int(0));
        data.define(DISPLAY_TELEPORT_DURATION, SyncedValue::Int(0));
        data.define(DISPLAY_TRANSLATION, SyncedValue::Vector3([0.0; 3]));
        data.define(DISPLAY_SCALE, SyncedValue::Vector3([1.0; 3]));
        data.define(DISPLAY_LEFT_ROTATION, SyncedValue::Quaternion(IDENTITY));
        data.define(DISPLAY_RIGHT_ROTATION, SyncedValue::Quaternion(IDENTITY));
        data.define(DISPLAY_BILLBOARD, SyncedValue::Byte(0));
        data.define(DISPLAY_BRIGHTNESS, SyncedValue::Int(-1));
        data.define(DISPLAY_VIEW_RANGE, SyncedValue::Float(1.0));
        data.define(DISPLAY_SHADOW_RADIUS, SyncedValue::Float(0.0));
        data.define(DISPLAY_SHADOW_STRENGTH, SyncedValue::Float(1.0));
        data.define(DISPLAY_WIDTH, SyncedValue::Float(0.0));
        data.define(DISPLAY_HEIGHT, SyncedValue::Float(0.0));
        data.define(DISPLAY_GLOW_COLOR, SyncedValue::Int(-1));
    }
    match kind {
        EntityKind::TextDisplay => {
            let data = &mut base.data;
            data.define(TEXT_TEXT, SyncedValue::Component(Component::text("")));
            data.define(TEXT_LINE_WIDTH, SyncedValue::Int(DEFAULT_LINE_WIDTH));
            data.define(TEXT_BACKGROUND, SyncedValue::Int(DEFAULT_BACKGROUND));
            data.define(TEXT_OPACITY, SyncedValue::Byte(-1));
            data.define(TEXT_FLAGS, SyncedValue::Byte(0));
        }
        EntityKind::BlockDisplay => {
            base.data
                .define(BLOCK_STATE, SyncedValue::BlockState(BlockStateId::AIR));
        }
        EntityKind::ItemDisplay => {
            base.data.define(ITEM_STACK, SyncedValue::Item(None));
            base.data.define(ITEM_CONTEXT, SyncedValue::Byte(0));
        }
        EntityKind::Interaction => {
            let data = &mut base.data;
            data.define(INTERACTION_WIDTH, SyncedValue::Float(1.0));
            data.define(INTERACTION_HEIGHT, SyncedValue::Float(1.0));
            data.define(INTERACTION_RESPONSE, SyncedValue::Boolean(false));
        }
        EntityKind::OminousItemSpawner => {
            base.data.define(SPAWNER_ITEM, SyncedValue::Item(None));
        }
        _ => {}
    }
    DecorState::default()
}

fn float(nbt: &Compound, key: &str) -> Option<f32> {
    nbt.get_f64(key).map(|value| value as f32)
}

fn index_of(names: &[&str], value: Option<&str>) -> Option<i8> {
    let value = value?;
    names
        .iter()
        .position(|name| *name == value)
        .map(|index| index as i8)
}

fn read_action(nbt: &Compound, key: &str) -> Option<PlayerAction> {
    let entry = nbt.get_compound(key)?;
    Some(PlayerAction {
        player: read_uuid(entry.get("player")?)?,
        timestamp: entry.get_i64("timestamp")?,
    })
}

fn write_action(out: &mut Compound, key: &str, action: Option<PlayerAction>) {
    if let Some(action) = action {
        out.insert(
            key,
            Compound::new()
                .with("player", uuid_tag(action.player))
                .with("timestamp", action.timestamp),
        );
    }
}

fn floats<const N: usize>(tag: &Tag) -> Option<[f32; N]> {
    let list = tag.as_list()?;
    if list.len() != N {
        return None;
    }
    let mut out = [0.0; N];
    for (slot, value) in out.iter_mut().zip(list) {
        *slot = value.as_f64()? as f32;
    }
    Some(out)
}

fn rotation(tag: &Tag) -> Option<[f32; 4]> {
    if let Some(quaternion) = floats::<4>(tag) {
        return Some(quaternion);
    }
    let compound = tag.as_compound()?;
    let axis = floats::<3>(compound.get("axis")?)?;
    let angle = compound.get_f64("angle")? as f32;
    let (sin, cos) = (angle / 2.0).sin_cos();
    Some([axis[0] * sin, axis[1] * sin, axis[2] * sin, cos])
}

fn read_transformation(nbt: &Compound, base: &mut EntityBase) {
    let Some(transformation) = nbt.get_compound("transformation") else {
        return;
    };
    let data = &mut base.data;
    if let Some(value) = transformation.get("translation").and_then(floats::<3>) {
        data.set(DISPLAY_TRANSLATION, SyncedValue::Vector3(value));
    }
    if let Some(value) = transformation.get("scale").and_then(floats::<3>) {
        data.set(DISPLAY_SCALE, SyncedValue::Vector3(value));
    }
    if let Some(value) = transformation.get("left_rotation").and_then(rotation) {
        data.set(DISPLAY_LEFT_ROTATION, SyncedValue::Quaternion(value));
    }
    if let Some(value) = transformation.get("right_rotation").and_then(rotation) {
        data.set(DISPLAY_RIGHT_ROTATION, SyncedValue::Quaternion(value));
    }
}

fn read_display(nbt: &Compound, base: &mut EntityBase) {
    read_transformation(nbt, base);
    let data = &mut base.data;
    if let Some(value) = nbt.get_i32("start_interpolation") {
        data.set(DISPLAY_START_INTERPOLATION, SyncedValue::Int(value));
    }
    if let Some(value) = nbt.get_i32("interpolation_duration") {
        data.set(DISPLAY_INTERPOLATION_DURATION, SyncedValue::Int(value));
    }
    if let Some(value) = nbt.get_i32("teleport_duration") {
        data.set(DISPLAY_TELEPORT_DURATION, SyncedValue::Int(value));
    }
    if let Some(index) = index_of(&BILLBOARDS, nbt.get_str("billboard")) {
        data.set(DISPLAY_BILLBOARD, SyncedValue::Byte(index));
    }
    if let Some(brightness) = nbt.get_compound("brightness") {
        let sky = brightness.get_i32("sky").unwrap_or(0).clamp(0, 15);
        let block = brightness.get_i32("block").unwrap_or(0).clamp(0, 15);
        data.set(DISPLAY_BRIGHTNESS, SyncedValue::Int(block << 4 | sky << 20));
    }
    for (key, index) in [
        ("view_range", DISPLAY_VIEW_RANGE),
        ("shadow_radius", DISPLAY_SHADOW_RADIUS),
        ("shadow_strength", DISPLAY_SHADOW_STRENGTH),
        ("width", DISPLAY_WIDTH),
        ("height", DISPLAY_HEIGHT),
    ] {
        if let Some(value) = float(nbt, key) {
            data.set(index, SyncedValue::Float(value));
        }
    }
    if let Some(value) = nbt.get_i32("glow_color_override") {
        data.set(DISPLAY_GLOW_COLOR, SyncedValue::Int(value));
    }
}

fn read_text(nbt: &Compound, base: &mut EntityBase) {
    let data = &mut base.data;
    if let Some(text) = nbt.get("text").and_then(component_from_tag) {
        data.set(TEXT_TEXT, SyncedValue::Component(text));
    }
    if let Some(value) = nbt.get_i32("line_width") {
        data.set(TEXT_LINE_WIDTH, SyncedValue::Int(value));
    }
    if let Some(value) = nbt.get_i32("background") {
        data.set(TEXT_BACKGROUND, SyncedValue::Int(value));
    }
    if let Some(value) = nbt.get_i64("text_opacity") {
        data.set(TEXT_OPACITY, SyncedValue::Byte(value as i8));
    }
    let mut flags = 0i8;
    for (key, bit) in [
        ("shadow", SHADOW),
        ("see_through", SEE_THROUGH),
        ("default_background", DEFAULT_BACKGROUND_FLAG),
    ] {
        if nbt.get_bool(key).unwrap_or(false) {
            flags |= bit;
        }
    }
    flags |= match nbt.get_str("alignment") {
        Some("left") => ALIGN_LEFT,
        Some("right") => ALIGN_RIGHT,
        _ => 0,
    };
    data.set(TEXT_FLAGS, SyncedValue::Byte(flags));
}

pub fn read(nbt: &Compound, base: &mut EntityBase, state: &mut DecorState) {
    match base.kind {
        EntityKind::TextDisplay => {
            read_display(nbt, base);
            read_text(nbt, base);
        }
        EntityKind::BlockDisplay => {
            read_display(nbt, base);
            if let Some(state) = nbt.get_compound("block_state") {
                base.data.set(
                    BLOCK_STATE,
                    SyncedValue::BlockState(block_state_from_nbt(state)),
                );
            }
        }
        EntityKind::ItemDisplay => {
            read_display(nbt, base);
            let stack = nbt.get_compound("item").and_then(item_stack_from_nbt);
            base.data.set(ITEM_STACK, SyncedValue::Item(stack));
            if let Some(index) = index_of(&CONTEXTS, nbt.get_str("item_display")) {
                base.data.set(ITEM_CONTEXT, SyncedValue::Byte(index));
            }
        }
        EntityKind::Interaction => {
            if let Some(value) = float(nbt, "width") {
                base.data.set(INTERACTION_WIDTH, SyncedValue::Float(value));
            }
            if let Some(value) = float(nbt, "height") {
                base.data.set(INTERACTION_HEIGHT, SyncedValue::Float(value));
            }
            base.data.set(
                INTERACTION_RESPONSE,
                SyncedValue::Boolean(nbt.get_bool("response").unwrap_or(false)),
            );
            base.dimensions.width = float(nbt, "width").unwrap_or(1.0);
            base.dimensions.height = float(nbt, "height").unwrap_or(1.0);
            state.interaction = read_action(nbt, "interaction");
            state.attack = read_action(nbt, "attack");
        }
        EntityKind::OminousItemSpawner => {
            let stack = nbt.get_compound("item").and_then(item_stack_from_nbt);
            base.data.set(SPAWNER_ITEM, SyncedValue::Item(stack));
            state.spawn_item_after_ticks = nbt.get_i64("spawn_item_after_ticks").unwrap_or(0);
        }
        _ => {}
    }
}

pub fn write(base: &EntityBase, state: &DecorState, out: &mut Compound) {
    match base.kind {
        EntityKind::Interaction => {
            write_action(out, "interaction", state.interaction);
            write_action(out, "attack", state.attack);
        }
        EntityKind::OminousItemSpawner => {
            if let Some(stack) = item(base) {
                out.insert("item", item_stack_to_nbt(&stack));
            }
            out.insert("spawn_item_after_ticks", state.spawn_item_after_ticks);
        }
        _ => {}
    }
}

pub fn item(base: &EntityBase) -> Option<crate::item::ItemStack> {
    match base.data.get(SPAWNER_ITEM) {
        Some(SyncedValue::Item(stack)) => stack.clone(),
        _ => None,
    }
}

pub fn set_item(base: &mut EntityBase, stack: Option<crate::item::ItemStack>) {
    base.data.set(SPAWNER_ITEM, SyncedValue::Item(stack));
}

pub fn responds(base: &EntityBase) -> bool {
    base.data.boolean(INTERACTION_RESPONSE)
}

pub fn delay(random: &mut impl crate::random::RandomSource) -> i64 {
    i64::from(random.next_int_between(SPAWNER_DELAY.0, SPAWNER_DELAY.1))
}

pub fn about_to_spawn(base: &EntityBase, state: &DecorState) -> bool {
    i64::from(base.tick_count) == state.spawn_item_after_ticks - ABOUT_TO_SPAWN_LEAD
}

pub fn due(base: &EntityBase, state: &DecorState) -> bool {
    state.spawn_item_after_ticks <= i64::from(base.tick_count)
}

#[cfg(test)]
mod tests {
    use rspigot_nbt::Tag;

    use super::*;
    use crate::EntityId;
    use crate::random::LegacyRandom;

    fn decor(kind: EntityKind) -> (EntityBase, DecorState) {
        let mut base = EntityBase::new(EntityId(1), Uuid::from_u128(1), kind);
        let state = define(&mut base);
        (base, state)
    }

    #[test]
    fn a_text_display_reads_its_text_and_options() {
        let (mut base, mut state) = decor(EntityKind::TextDisplay);
        let nbt = Compound::new()
            .with("text", "Hello")
            .with("billboard", "center")
            .with("line_width", 120)
            .with("shadow", 1i8)
            .with("alignment", "left")
            .with("view_range", 2.5f32);
        read(&nbt, &mut base, &mut state);
        assert_eq!(
            base.data.get(TEXT_TEXT),
            Some(&SyncedValue::Component(Component::text("Hello")))
        );
        assert_eq!(base.data.byte(DISPLAY_BILLBOARD), 3);
        assert_eq!(base.data.int(TEXT_LINE_WIDTH), 120);
        assert_eq!(base.data.byte(TEXT_FLAGS), SHADOW | ALIGN_LEFT);
        assert_eq!(
            base.data.get(DISPLAY_VIEW_RANGE),
            Some(&SyncedValue::Float(2.5))
        );
    }

    #[test]
    fn a_transformation_fills_the_vectors_and_quaternions() {
        let (mut base, mut state) = decor(EntityKind::BlockDisplay);
        let list = |values: &[f32]| Tag::List(values.iter().map(|v| Tag::Float(*v)).collect());
        let nbt = Compound::new()
            .with(
                "transformation",
                Compound::new()
                    .with("translation", list(&[1.0, 2.0, 3.0]))
                    .with("scale", list(&[2.0, 2.0, 2.0]))
                    .with("left_rotation", list(&[0.0, 1.0, 0.0, 0.0]))
                    .with(
                        "right_rotation",
                        Compound::new()
                            .with("axis", list(&[0.0, 1.0, 0.0]))
                            .with("angle", std::f64::consts::PI),
                    ),
            )
            .with(
                "block_state",
                Compound::new().with("Name", "minecraft:stone"),
            );
        read(&nbt, &mut base, &mut state);
        assert_eq!(
            base.data.get(DISPLAY_TRANSLATION),
            Some(&SyncedValue::Vector3([1.0, 2.0, 3.0]))
        );
        assert_eq!(
            base.data.get(DISPLAY_SCALE),
            Some(&SyncedValue::Vector3([2.0; 3]))
        );
        assert_eq!(
            base.data.get(DISPLAY_LEFT_ROTATION),
            Some(&SyncedValue::Quaternion([0.0, 1.0, 0.0, 0.0]))
        );
        let Some(SyncedValue::Quaternion(right)) = base.data.get(DISPLAY_RIGHT_ROTATION) else {
            panic!("quaternion");
        };
        assert!((right[1] - 1.0).abs() < 1e-6 && right[3].abs() < 1e-6);
        assert!(matches!(
            base.data.get(BLOCK_STATE),
            Some(SyncedValue::BlockState(state)) if *state != BlockStateId::AIR
        ));
    }

    #[test]
    fn brightness_packs_block_and_sky_light() {
        let (mut base, mut state) = decor(EntityKind::BlockDisplay);
        let nbt = Compound::new().with(
            "brightness",
            Compound::new().with("sky", 5).with("block", 7),
        );
        read(&nbt, &mut base, &mut state);
        assert_eq!(base.data.int(DISPLAY_BRIGHTNESS), 7 << 4 | 5 << 20);
    }

    #[test]
    fn an_item_display_holds_its_item_and_context() {
        let (mut base, mut state) = decor(EntityKind::ItemDisplay);
        let nbt = Compound::new()
            .with(
                "item",
                Compound::new()
                    .with("id", "minecraft:apple")
                    .with("count", 1),
            )
            .with("item_display", "head");
        read(&nbt, &mut base, &mut state);
        assert!(matches!(
            base.data.get(ITEM_STACK),
            Some(SyncedValue::Item(Some(_)))
        ));
        assert_eq!(base.data.byte(ITEM_CONTEXT), 5);
    }

    #[test]
    fn an_interaction_reads_its_box_response_and_last_clicks() {
        let (mut base, mut state) = decor(EntityKind::Interaction);
        let player = Uuid::from_u128(9);
        let nbt = Compound::new()
            .with("width", 2.0f32)
            .with("height", 3.0f32)
            .with("response", 1i8)
            .with(
                "interaction",
                Compound::new()
                    .with("player", uuid_tag(player))
                    .with("timestamp", 77i64),
            );
        read(&nbt, &mut base, &mut state);
        assert!(responds(&base));
        assert_eq!(
            state.interaction,
            Some(PlayerAction {
                player,
                timestamp: 77
            })
        );
        let mut saved = Compound::new();
        write(&base, &state, &mut saved);
        assert_eq!(
            saved
                .get_compound("interaction")
                .and_then(|entry| entry.get_i64("timestamp")),
            Some(77)
        );
        assert!(saved.get("attack").is_none());
    }

    #[test]
    fn an_ominous_item_spawner_round_trips_its_item_and_delay() {
        let (mut base, mut state) = decor(EntityKind::OminousItemSpawner);
        let nbt = Compound::new()
            .with(
                "item",
                Compound::new()
                    .with("id", "minecraft:arrow")
                    .with("count", 1),
            )
            .with("spawn_item_after_ticks", 90i64);
        read(&nbt, &mut base, &mut state);
        assert_eq!(state.spawn_item_after_ticks, 90);
        let mut saved = Compound::new();
        write(&base, &state, &mut saved);
        assert_eq!(saved.get_i64("spawn_item_after_ticks"), Some(90));
        assert_eq!(
            saved
                .get_compound("item")
                .and_then(|stack| stack.get_str("id")),
            Some("minecraft:arrow")
        );
        let _: Option<&Tag> = saved.get("item");
    }

    #[test]
    fn the_spawner_warns_36_ticks_ahead_and_fires_at_its_delay() {
        let (mut base, mut state) = decor(EntityKind::OminousItemSpawner);
        state.spawn_item_after_ticks = 80;
        base.tick_count = 44;
        assert!(about_to_spawn(&base, &state));
        assert!(!due(&base, &state));
        base.tick_count = 80;
        assert!(due(&base, &state));
    }

    #[test]
    fn the_delay_is_between_60_and_120_ticks() {
        let mut random = LegacyRandom::new(1);
        for _ in 0..200 {
            assert!((60..=120).contains(&delay(&mut random)));
        }
    }
}

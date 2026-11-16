use rspigot_items::{EquipmentSlot, ModifierOperation};

use crate::effect::{
    ActiveEffects, AttributeModifier, AttributeOperation, AttributeState, effect_attributes,
    player_base,
};
use crate::ids::ItemId;

pub const MAX_HEALTH: &str = "minecraft:max_health";
pub const MAX_ABSORPTION: &str = "minecraft:max_absorption";
pub const ATTACK_DAMAGE: &str = "minecraft:attack_damage";
pub const ATTACK_SPEED: &str = "minecraft:attack_speed";
pub const ATTACK_KNOCKBACK: &str = "minecraft:attack_knockback";
pub const ARMOR: &str = "minecraft:armor";
pub const ARMOR_TOUGHNESS: &str = "minecraft:armor_toughness";
pub const SAFE_FALL_DISTANCE: &str = "minecraft:safe_fall_distance";
pub const FALL_DAMAGE_MULTIPLIER: &str = "minecraft:fall_damage_multiplier";
pub const KNOCKBACK_RESISTANCE: &str = "minecraft:knockback_resistance";
pub const MOVEMENT_SPEED: &str = "minecraft:movement_speed";

const RANGES: [(&str, f64, f64); 11] = [
    (MAX_HEALTH, 1.0, 1024.0),
    (MAX_ABSORPTION, 0.0, 2048.0),
    (ATTACK_DAMAGE, 0.0, 2048.0),
    (ATTACK_SPEED, 0.0, 1024.0),
    (ATTACK_KNOCKBACK, 0.0, 5.0),
    (ARMOR, 0.0, 30.0),
    (ARMOR_TOUGHNESS, 0.0, 20.0),
    (SAFE_FALL_DISTANCE, -1024.0, 1024.0),
    (FALL_DAMAGE_MULTIPLIER, 0.0, 100.0),
    (KNOCKBACK_RESISTANCE, -2.0, 1.0),
    (MOVEMENT_SPEED, 0.0, 1024.0),
];

const SYNCABLE: [&str; 7] = [
    MAX_HEALTH,
    MAX_ABSORPTION,
    ATTACK_SPEED,
    ARMOR,
    ARMOR_TOUGHNESS,
    SAFE_FALL_DISTANCE,
    FALL_DAMAGE_MULTIPLIER,
];

pub fn is_syncable(attribute: &str) -> bool {
    SYNCABLE.contains(&attribute)
}

pub fn sanitize(attribute: &str, value: f64) -> f64 {
    match RANGES.iter().find(|(name, _, _)| *name == attribute) {
        Some((_, min, _)) if value.is_nan() => *min,
        Some((_, min, max)) => value.clamp(*min, *max),
        None => value,
    }
}

pub fn value(state: &AttributeState) -> f64 {
    let operation = |kind: AttributeOperation| {
        state
            .modifiers
            .iter()
            .filter(move |m| m.operation == kind)
            .map(|m| m.amount)
    };
    let base = state.base + operation(AttributeOperation::AddValue).sum::<f64>();
    let mut result = base;
    for amount in operation(AttributeOperation::AddMultipliedBase) {
        result += base * amount;
    }
    for amount in operation(AttributeOperation::AddMultipliedTotal) {
        result *= 1.0 + amount;
    }
    sanitize(state.attribute, result)
}

const fn operation(operation: ModifierOperation) -> AttributeOperation {
    match operation {
        ModifierOperation::AddValue => AttributeOperation::AddValue,
        ModifierOperation::AddMultipliedBase => AttributeOperation::AddMultipliedBase,
        ModifierOperation::AddMultipliedTotal => AttributeOperation::AddMultipliedTotal,
    }
}

pub fn equipment_modifiers(
    slot: EquipmentSlot,
    item: ItemId,
) -> impl Iterator<Item = (&'static str, AttributeModifier)> {
    item.kind()
        .map_or(&[][..], |kind| kind.attribute_modifiers())
        .iter()
        .filter(move |m| m.slot.contains(slot))
        .map(|m| {
            (
                m.attribute,
                AttributeModifier {
                    id: std::borrow::Cow::Borrowed(m.id),
                    amount: m.amount,
                    operation: operation(m.operation),
                },
            )
        })
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Attributes {
    pub states: Vec<AttributeState>,
}

impl Attributes {
    pub fn of(effects: &ActiveEffects, equipment: &[(EquipmentSlot, ItemId)]) -> Self {
        let mut states = effect_attributes(effects);
        for (slot, item) in equipment {
            for (attribute, modifier) in equipment_modifiers(*slot, *item) {
                match states.iter_mut().find(|s| s.attribute == attribute) {
                    Some(state) => state.modifiers.push(modifier),
                    None => states.push(AttributeState {
                        attribute,
                        base: player_base(attribute),
                        modifiers: vec![modifier],
                    }),
                }
            }
        }
        Self { states }
    }

    pub fn with_base(mut self, attribute: &'static str, base: f64) -> Self {
        match self.states.iter_mut().find(|s| s.attribute == attribute) {
            Some(state) => state.base = base,
            None => self.states.push(AttributeState {
                attribute,
                base,
                modifiers: Vec::new(),
            }),
        }
        self
    }

    pub fn with_instance(mut self, edit: &crate::entity::living::AttributeInstance) -> Self {
        match self
            .states
            .iter_mut()
            .find(|s| s.attribute == edit.attribute)
        {
            Some(state) => {
                state.base = edit.base;
                state.modifiers.extend(edit.modifiers.iter().cloned());
            }
            None => self.states.push(AttributeState {
                attribute: edit.attribute,
                base: edit.base,
                modifiers: edit.modifiers.clone(),
            }),
        }
        self
    }

    pub fn get(&self, attribute: &str) -> f64 {
        self.states
            .iter()
            .find(|s| s.attribute == attribute)
            .map_or_else(|| sanitize(attribute, player_base(attribute)), value)
    }

    pub fn state(&self, attribute: &'static str) -> AttributeState {
        self.states
            .iter()
            .find(|s| s.attribute == attribute)
            .cloned()
            .unwrap_or(AttributeState {
                attribute,
                base: player_base(attribute),
                modifiers: Vec::new(),
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::effect::{EffectDuration, EffectId, EffectInstance};

    fn effect(name: &str, amplifier: u8) -> EffectInstance {
        EffectInstance::new(
            EffectId::from_name(name).unwrap(),
            EffectDuration::Ticks(100),
            amplifier,
            true,
        )
    }

    fn item(name: &str) -> ItemId {
        rspigot_items::ItemKind::from_name(name).unwrap().id()
    }

    #[test]
    fn defaults_are_the_player_bases() {
        let attributes = Attributes::default();
        assert_eq!(attributes.get(MAX_HEALTH), 20.0);
        assert_eq!(attributes.get(ATTACK_DAMAGE), 1.0);
        assert_eq!(attributes.get(ATTACK_SPEED), 4.0);
        assert_eq!(attributes.get(SAFE_FALL_DISTANCE), 3.0);
        assert_eq!(attributes.get(FALL_DAMAGE_MULTIPLIER), 1.0);
        assert_eq!(attributes.get(ARMOR), 0.0);
    }

    #[test]
    fn held_sword_and_strength_add_attack_damage() {
        let mut effects = ActiveEffects::default();
        let _ = effects.apply(effect("strength", 1));
        let held = [(EquipmentSlot::Mainhand, item("diamond_sword"))];
        let attributes = Attributes::of(&effects, &held);
        assert_eq!(attributes.get(ATTACK_DAMAGE), 1.0 + 6.0 + 6.0);
        assert!((attributes.get(ATTACK_SPEED) - 1.6).abs() < 1e-6);
        let offhand = [(EquipmentSlot::Offhand, item("diamond_sword"))];
        assert_eq!(
            Attributes::of(&ActiveEffects::default(), &offhand).get(ATTACK_DAMAGE),
            1.0
        );
    }

    #[test]
    fn jump_boost_and_health_boost_raise_their_attributes() {
        let mut effects = ActiveEffects::default();
        let _ = effects.apply(effect("jump_boost", 1));
        let _ = effects.apply(effect("health_boost", 0));
        let attributes = Attributes::of(&effects, &[]);
        assert_eq!(attributes.get(SAFE_FALL_DISTANCE), 5.0);
        assert_eq!(attributes.get(MAX_HEALTH), 24.0);
    }

    #[test]
    fn armor_pieces_stack_and_operations_follow_vanilla_order() {
        let worn = [
            (EquipmentSlot::Head, item("diamond_helmet")),
            (EquipmentSlot::Chest, item("diamond_chestplate")),
            (EquipmentSlot::Mainhand, item("diamond_helmet")),
        ];
        let attributes = Attributes::of(&ActiveEffects::default(), &worn);
        assert_eq!(attributes.get(ARMOR), 3.0 + 8.0);
        assert_eq!(attributes.get(ARMOR_TOUGHNESS), 4.0);
        let state = AttributeState {
            attribute: ATTACK_DAMAGE,
            base: 2.0,
            modifiers: vec![
                AttributeModifier {
                    id: "a".into(),
                    amount: 1.0,
                    operation: AttributeOperation::AddValue,
                },
                AttributeModifier {
                    id: "b".into(),
                    amount: 0.5,
                    operation: AttributeOperation::AddMultipliedBase,
                },
                AttributeModifier {
                    id: "c".into(),
                    amount: 1.0,
                    operation: AttributeOperation::AddMultipliedTotal,
                },
            ],
        };
        assert_eq!(value(&state), 9.0);
        assert_eq!(sanitize(MAX_HEALTH, 0.0), 1.0);
        assert_eq!(sanitize(ARMOR, 99.0), 30.0);
        assert_eq!(sanitize(ARMOR, f64::NAN), 0.0);
    }

    #[test]
    fn a_new_base_keeps_the_modifiers_and_its_range() {
        let mut effects = ActiveEffects::default();
        let _ = effects.apply(effect("speed", 1));
        let attributes = Attributes::of(&effects, &[]).with_base(MOVEMENT_SPEED, 0.2);
        let state = attributes.state(MOVEMENT_SPEED);
        assert_eq!(state.base, 0.2);
        assert_eq!(state.modifiers.len(), 1);
        assert!((attributes.get(MOVEMENT_SPEED) - 0.2 * 1.4).abs() < 1e-6);

        let still = Attributes::default().with_base(MOVEMENT_SPEED, -0.5);
        assert_eq!(still.state(MOVEMENT_SPEED).base, -0.5);
        assert!(still.state(MOVEMENT_SPEED).modifiers.is_empty());
        assert_eq!(still.get(MOVEMENT_SPEED), 0.0);
    }
}

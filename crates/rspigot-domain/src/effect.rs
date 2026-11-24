pub use rspigot_effects::{AttributeOperation, AttributeTemplate, EffectId, EffectKind};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EffectDuration {
    Ticks(u32),
    Infinite,
}

impl EffectDuration {
    pub const fn is_shorter_than(self, other: Self) -> bool {
        match (self, other) {
            (Self::Infinite, _) => false,
            (Self::Ticks(_), Self::Infinite) => true,
            (Self::Ticks(a), Self::Ticks(b)) => a < b,
        }
    }

    pub const fn wire_ticks(self) -> i32 {
        match self {
            Self::Infinite => -1,
            Self::Ticks(ticks) => {
                if ticks > i32::MAX as u32 {
                    i32::MAX
                } else {
                    ticks as i32
                }
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EffectInstance {
    pub effect: EffectId,
    pub amplifier: u8,
    pub duration: EffectDuration,
    pub ambient: bool,
    pub visible: bool,
    pub show_icon: bool,
}

impl EffectInstance {
    pub const fn new(
        effect: EffectId,
        duration: EffectDuration,
        amplifier: u8,
        visible: bool,
    ) -> Self {
        Self {
            effect,
            amplifier,
            duration,
            ambient: false,
            visible,
            show_icon: visible,
        }
    }

    fn update(&mut self, hidden: &mut Vec<Self>, other: &Self) -> bool {
        let mut changed = false;
        if other.amplifier > self.amplifier {
            if other.duration.is_shorter_than(self.duration) {
                hidden.insert(0, *self);
            }
            self.amplifier = other.amplifier;
            self.duration = other.duration;
            changed = true;
        } else if self.duration.is_shorter_than(other.duration) {
            if other.amplifier == self.amplifier {
                self.duration = other.duration;
                changed = true;
            } else if hidden.is_empty() {
                hidden.push(*other);
            } else {
                let mut deeper = hidden.split_off(1);
                hidden[0].update(&mut deeper, other);
                hidden.append(&mut deeper);
            }
        }
        if (!other.ambient && self.ambient) || changed {
            self.ambient = other.ambient;
            changed = true;
        }
        if other.visible != self.visible {
            self.visible = other.visible;
            changed = true;
        }
        if other.show_icon != self.show_icon {
            self.show_icon = other.show_icon;
            changed = true;
        }
        changed
    }

    fn count_down(&mut self) -> bool {
        match &mut self.duration {
            EffectDuration::Infinite => false,
            EffectDuration::Ticks(ticks) => {
                *ticks = ticks.saturating_sub(1);
                *ticks == 0
            }
        }
    }

    fn take_details(&mut self, other: Self) {
        self.amplifier = other.amplifier;
        self.duration = other.duration;
        self.ambient = other.ambient;
        self.visible = other.visible;
        self.show_icon = other.show_icon;
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[must_use]
pub enum ApplyOutcome {
    Added,
    Updated,
    Unchanged,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ActiveEffects {
    effects: Vec<EffectInstance>,
    hidden: Vec<Vec<EffectInstance>>,
    restored: Vec<EffectId>,
}

impl ActiveEffects {
    pub fn apply(&mut self, instance: EffectInstance) -> ApplyOutcome {
        match self
            .effects
            .iter()
            .position(|e| e.effect == instance.effect)
        {
            Some(index) => {
                if self.effects[index].update(&mut self.hidden[index], &instance) {
                    ApplyOutcome::Updated
                } else {
                    ApplyOutcome::Unchanged
                }
            }
            None => {
                self.effects.push(instance);
                self.hidden.push(Vec::new());
                ApplyOutcome::Added
            }
        }
    }

    pub fn get(&self, effect: EffectId) -> Option<&EffectInstance> {
        self.effects.iter().find(|e| e.effect == effect)
    }

    pub fn remove(&mut self, effect: EffectId) -> Option<EffectInstance> {
        let index = self.effects.iter().position(|e| e.effect == effect)?;
        self.hidden.remove(index);
        Some(self.effects.remove(index))
    }

    pub fn clear(&mut self) -> Vec<EffectInstance> {
        self.hidden.clear();
        std::mem::take(&mut self.effects)
    }

    pub fn tick(&mut self) -> Vec<EffectId> {
        let spent = self.countdown();
        let expired: Vec<EffectId> = spent.iter().map(|effect| effect.effect).collect();
        for effect in &expired {
            self.remove(*effect);
        }
        expired
    }

    pub fn countdown(&mut self) -> Vec<EffectInstance> {
        let mut spent = Vec::new();
        for (index, effect) in self.effects.iter_mut().enumerate() {
            let hidden = &mut self.hidden[index];
            for behind in hidden.iter_mut() {
                behind.count_down();
            }
            if effect.count_down() {
                if hidden.is_empty() {
                    spent.push(*effect);
                } else {
                    effect.take_details(hidden.remove(0));
                    self.restored.push(effect.effect);
                }
            }
        }
        spent
    }

    pub fn take_restored(&mut self) -> Vec<EffectId> {
        std::mem::take(&mut self.restored)
    }

    pub fn trial(&self, instance: EffectInstance) -> ApplyOutcome {
        self.clone().apply(instance)
    }

    pub fn iter(&self) -> impl Iterator<Item = &EffectInstance> {
        self.effects.iter()
    }

    pub fn is_empty(&self) -> bool {
        self.effects.is_empty()
    }

    pub fn has(&self, name: &str) -> bool {
        EffectId::from_name(name).is_some_and(|id| self.get(id).is_some())
    }
}

pub fn attribute_template(effect: &str) -> Option<AttributeTemplate> {
    EffectKind::ALL
        .into_iter()
        .find(|kind| kind.name() == effect)?
        .modifiers()
        .first()
        .copied()
}

pub const PLAYER_ATTRIBUTE_BASES: &[(&str, f64)] =
    <rspigot_entities::Player as rspigot_entities::EntityType>::DEFAULT_ATTRIBUTES;

#[derive(Debug, Clone, PartialEq)]
pub struct AttributeModifier {
    pub id: std::borrow::Cow<'static, str>,
    pub amount: f64,
    pub operation: AttributeOperation,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AttributeState {
    pub attribute: &'static str,
    pub base: f64,
    pub modifiers: Vec<AttributeModifier>,
}

pub fn effect_attributes(effects: &ActiveEffects) -> Vec<AttributeState> {
    effects
        .iter()
        .filter_map(|e| Some((e.amplifier, e.effect.kind()?.modifiers())))
        .flat_map(|(amplifier, templates)| templates.iter().map(move |t| (amplifier, t)))
        .fold(
            Vec::<AttributeState>::new(),
            |mut states, (amplifier, template)| {
                let modifier = AttributeModifier {
                    id: std::borrow::Cow::Borrowed(template.modifier),
                    amount: template.amount_for(amplifier),
                    operation: template.operation,
                };
                match states
                    .iter_mut()
                    .find(|s| s.attribute == template.attribute)
                {
                    Some(state) => state.modifiers.push(modifier),
                    None => states.push(AttributeState {
                        attribute: template.attribute,
                        base: player_base(template.attribute),
                        modifiers: vec![modifier],
                    }),
                }
                states
            },
        )
}

pub fn player_base(attribute: &str) -> f64 {
    PLAYER_ATTRIBUTE_BASES
        .iter()
        .find(|(name, _)| *name == attribute)
        .map_or(0.0, |(_, base)| *base)
}

pub fn affected_attribute(effect: EffectId) -> Option<&'static str> {
    effect.kind()?.modifiers().first().map(|t| t.attribute)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn speed() -> EffectId {
        EffectId::from_name("speed").unwrap()
    }

    #[test]
    fn player_bases_follow_vanilla_defaults() {
        assert_eq!(player_base("minecraft:scale"), 1.0);
        assert_eq!(player_base("minecraft:step_height"), 0.6);
        assert_eq!(player_base("minecraft:gravity"), 0.08);
        assert_eq!(player_base("minecraft:block_interaction_range"), 4.5);
        assert_eq!(player_base("minecraft:entity_interaction_range"), 3.0);
        assert_eq!(
            player_base("minecraft:movement_speed"),
            0.100_000_001_490_116_12
        );
        assert_eq!(player_base("minecraft:max_health"), 20.0);
    }

    #[test]
    fn effect_ids_follow_the_registry() {
        assert_eq!(speed(), EffectId(0));
        assert_eq!(EffectId::from_name("minecraft:slowness"), Some(EffectId(1)));
        assert_eq!(
            speed().translation_key().as_deref(),
            Some("effect.minecraft.speed")
        );
        assert_eq!(EffectId::from_name("not_an_effect"), None);
    }

    #[test]
    fn stronger_amplifier_replaces_and_weaker_is_refused() {
        let mut effects = ActiveEffects::default();
        let base = EffectInstance::new(speed(), EffectDuration::Ticks(600), 1, true);
        assert_eq!(effects.apply(base), ApplyOutcome::Added);
        let weaker = EffectInstance::new(speed(), EffectDuration::Ticks(6000), 0, true);
        assert_eq!(effects.apply(weaker), ApplyOutcome::Unchanged);
        let stronger = EffectInstance::new(speed(), EffectDuration::Ticks(20), 3, true);
        assert_eq!(effects.apply(stronger), ApplyOutcome::Updated);
        assert_eq!(
            effects.get(speed()).map(|e| (e.amplifier, e.duration)),
            Some((3, EffectDuration::Ticks(20)))
        );
    }

    #[test]
    fn same_amplifier_only_extends_duration() {
        let mut effects = ActiveEffects::default();
        let _ = effects.apply(EffectInstance::new(
            speed(),
            EffectDuration::Ticks(600),
            0,
            true,
        ));
        assert_eq!(
            effects.apply(EffectInstance::new(
                speed(),
                EffectDuration::Ticks(100),
                0,
                true
            )),
            ApplyOutcome::Unchanged
        );
        assert_eq!(
            effects.apply(EffectInstance::new(
                speed(),
                EffectDuration::Infinite,
                0,
                true
            )),
            ApplyOutcome::Updated
        );
        assert_eq!(
            effects.apply(EffectInstance::new(
                speed(),
                EffectDuration::Ticks(1_000_000),
                0,
                true
            )),
            ApplyOutcome::Unchanged
        );
    }

    #[test]
    fn visibility_change_counts_as_update() {
        let mut effects = ActiveEffects::default();
        let _ = effects.apply(EffectInstance::new(
            speed(),
            EffectDuration::Ticks(600),
            0,
            true,
        ));
        assert_eq!(
            effects.apply(EffectInstance::new(
                speed(),
                EffectDuration::Ticks(600),
                0,
                false
            )),
            ApplyOutcome::Updated
        );
        assert_eq!(
            effects.get(speed()).map(|e| (e.visible, e.show_icon)),
            Some((false, false))
        );
    }

    #[test]
    fn tick_expires_finite_effects_only() {
        let mut effects = ActiveEffects::default();
        let _ = effects.apply(EffectInstance::new(
            speed(),
            EffectDuration::Ticks(2),
            0,
            true,
        ));
        let _ = effects.apply(EffectInstance::new(
            EffectId(1),
            EffectDuration::Infinite,
            0,
            true,
        ));
        assert!(effects.tick().is_empty());
        assert_eq!(effects.tick(), vec![speed()]);
        assert!(effects.get(EffectId(1)).is_some());
        assert_eq!(EffectDuration::Infinite.wire_ticks(), -1);
    }

    #[test]
    fn speed_250_gives_vanilla_modifier() {
        let mut effects = ActiveEffects::default();
        let _ = effects.apply(EffectInstance::new(
            speed(),
            EffectDuration::Ticks(1_999_980),
            250,
            true,
        ));
        let states = effect_attributes(&effects);
        assert_eq!(states.len(), 1);
        assert_eq!(states[0].attribute, "minecraft:movement_speed");
        assert!((states[0].base - 0.1).abs() < 1e-6);
        assert_eq!(states[0].modifiers[0].id, "minecraft:effect.speed");
        assert!((states[0].modifiers[0].amount - 50.2).abs() < 1e-5);
        assert_eq!(
            states[0].modifiers[0].operation,
            AttributeOperation::AddMultipliedTotal
        );
    }

    #[test]
    fn speed_and_slowness_share_one_attribute() {
        let mut effects = ActiveEffects::default();
        let _ = effects.apply(EffectInstance::new(
            speed(),
            EffectDuration::Ticks(10),
            0,
            true,
        ));
        let _ = effects.apply(EffectInstance::new(
            EffectId(1),
            EffectDuration::Ticks(10),
            0,
            true,
        ));
        let states = effect_attributes(&effects);
        assert_eq!(states.len(), 1);
        assert_eq!(states[0].modifiers.len(), 2);
    }

    #[test]
    fn a_stronger_shorter_effect_hides_the_weaker_longer_one_until_it_ends() {
        let mut effects = ActiveEffects::default();
        let weak = EffectInstance::new(speed(), EffectDuration::Ticks(10), 0, true);
        let strong = EffectInstance::new(speed(), EffectDuration::Ticks(3), 2, true);
        let _ = effects.apply(weak);
        assert_eq!(effects.apply(strong), ApplyOutcome::Updated);
        assert_eq!(effects.get(speed()).map(|e| e.amplifier), Some(2));
        assert!(effects.countdown().is_empty());
        assert!(effects.countdown().is_empty());
        assert!(effects.countdown().is_empty());
        assert_eq!(effects.take_restored(), vec![speed()]);
        let back = effects.get(speed()).copied().expect("restored");
        assert_eq!(back.amplifier, 0);
        assert_eq!(back.duration, EffectDuration::Ticks(7));
        assert!(effects.take_restored().is_empty());
    }

    #[test]
    fn a_weaker_longer_effect_waits_behind_the_current_one() {
        let mut effects = ActiveEffects::default();
        let _ = effects.apply(EffectInstance::new(
            speed(),
            EffectDuration::Ticks(2),
            1,
            true,
        ));
        assert_eq!(
            effects.apply(EffectInstance::new(
                speed(),
                EffectDuration::Ticks(9),
                0,
                true
            )),
            ApplyOutcome::Unchanged
        );
        let _ = effects.countdown();
        assert_eq!(effects.countdown(), Vec::new());
        assert_eq!(effects.take_restored(), vec![speed()]);
        assert_eq!(effects.get(speed()).map(|e| e.amplifier), Some(0));
        assert_eq!(
            effects.get(speed()).map(|e| e.duration),
            Some(EffectDuration::Ticks(7))
        );
    }

    #[test]
    fn an_effect_without_a_hidden_one_expires_normally() {
        let mut effects = ActiveEffects::default();
        let _ = effects.apply(EffectInstance::new(
            speed(),
            EffectDuration::Ticks(1),
            0,
            true,
        ));
        assert_eq!(effects.countdown().len(), 1);
        assert!(effects.take_restored().is_empty());
    }
}

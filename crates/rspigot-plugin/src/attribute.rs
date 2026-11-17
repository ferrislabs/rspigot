use rspigot_plugin_protocol::v1::{self, answer, entity_order, from_plugin, question};

use crate::Error;
use crate::link::Link;

macro_rules! attributes {
    ($($name:ident),+ $(,)?) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        pub enum Attribute {
            $($name),+
        }

        impl Attribute {
            pub const ALL: &'static [Self] = &[$(Self::$name),+];

            pub(crate) const fn to_wire(self) -> v1::Attribute {
                match self {
                    $(Self::$name => v1::Attribute::$name),+
                }
            }
        }
    };
}

attributes!(
    AirDragModifier,
    Armor,
    ArmorToughness,
    AttackDamage,
    AttackKnockback,
    AttackSpeed,
    BelowNameDistance,
    BlockBreakSpeed,
    BlockInteractionRange,
    Bounciness,
    BurningTime,
    CameraDistance,
    ExplosionKnockbackResistance,
    EntityInteractionRange,
    FallDamageMultiplier,
    FlyingSpeed,
    FollowRange,
    FrictionModifier,
    Gravity,
    JumpStrength,
    KnockbackResistance,
    Luck,
    MaxAbsorption,
    MaxHealth,
    MiningEfficiency,
    MovementEfficiency,
    MovementSpeed,
    NameTagDistance,
    OxygenBonus,
    SafeFallDistance,
    Scale,
    SneakingSpeed,
    SpawnReinforcements,
    StepHeight,
    SubmergedMiningSpeed,
    SweepingDamageRatio,
    TemptRange,
    WaterMovementEfficiency,
    WaypointTransmitRange,
    WaypointReceiveRange,
);

#[derive(Debug, Clone)]
pub struct EntityAttributesHandle {
    entity_id: i32,
    link: Link,
}

impl EntityAttributesHandle {
    pub(crate) const fn new(entity_id: i32, link: Link) -> Self {
        Self { entity_id, link }
    }

    pub fn set(&self, attribute: Attribute, value: f64) {
        self.link.order(from_plugin::Body::Entity(v1::EntityOrder {
            entity: self.entity_id,
            order: Some(entity_order::Order::SetAttribute(v1::SetAttribute {
                attribute: attribute.to_wire() as i32,
                value,
            })),
        }));
    }

    pub async fn get_value(&self, attribute: Attribute) -> Result<f64, Error> {
        self.link
            .ask(
                question::Question::AttributeValue(v1::AttributeQuery {
                    entity: self.entity_id,
                    attribute: attribute.to_wire() as i32,
                }),
                "an attribute value",
                |body| match body {
                    answer::Body::AttributeValue(value) => Some(value),
                    _ => None,
                },
            )
            .await
    }
}

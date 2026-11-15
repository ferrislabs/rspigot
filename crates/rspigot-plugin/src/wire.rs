use std::net::IpAddr;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use rspigot_plugin_protocol::v1::{self, particle};
use uuid::Uuid;

use crate::link::Link;
use crate::{
    Block, BlockPos, CustomModelData, Effect, EffectType, Enchantment, EntityType, Error,
    ItemStack, Material, Particle, Position, Vector, World,
};

impl Position {
    pub(crate) fn from_wire(vec: Option<v1::Vec3>, field: &str) -> Result<Self, Error> {
        let vec = vec.ok_or_else(|| Error::Malformed(format!("a message without `{field}`")))?;
        Ok(Self {
            x: vec.x,
            y: vec.y,
            z: vec.z,
        })
    }

    pub(crate) const fn to_wire(self) -> v1::Vec3 {
        v1::Vec3 {
            x: self.x,
            y: self.y,
            z: self.z,
        }
    }
}

impl Vector {
    pub(crate) fn from_wire(vec: Option<v1::Vec3>, field: &str) -> Result<Self, Error> {
        let vec = vec.ok_or_else(|| Error::Malformed(format!("a message without `{field}`")))?;
        Ok(Self {
            x: vec.x,
            y: vec.y,
            z: vec.z,
        })
    }

    pub(crate) const fn to_wire(self) -> v1::Vec3 {
        v1::Vec3 {
            x: self.x,
            y: self.y,
            z: self.z,
        }
    }
}

impl BlockPos {
    pub(crate) fn from_wire(pos: Option<v1::BlockPos>, field: &str) -> Result<Self, Error> {
        let pos = pos.ok_or_else(|| Error::Malformed(format!("a message without `{field}`")))?;
        Ok(Self {
            x: pos.x,
            y: pos.y,
            z: pos.z,
        })
    }

    pub(crate) const fn to_wire(self) -> v1::BlockPos {
        v1::BlockPos {
            x: self.x,
            y: self.y,
            z: self.z,
        }
    }
}

impl ItemStack {
    pub(crate) fn from_wire(item: v1::Item) -> Result<Self, Error> {
        let material = Material::from_key(&item.key)
            .ok_or_else(|| Error::Malformed(format!("an unknown item `{}`", item.key)))?;
        let count = u8::try_from(item.count)
            .map_err(|_| Error::Malformed(format!("an item count {}", item.count)))?;
        let enchantments = item
            .enchantments
            .into_iter()
            .map(|enchant| {
                let enchantment = Enchantment::from_key(&enchant.key).ok_or_else(|| {
                    Error::Malformed(format!("an unknown enchantment `{}`", enchant.key))
                })?;
                let level = u32::try_from(enchant.level).map_err(|_| {
                    Error::Malformed(format!("an enchantment level {}", enchant.level))
                })?;
                Ok((enchantment, level))
            })
            .collect::<Result<_, Error>>()?;
        Ok(Self {
            material,
            count,
            name: item.name,
            lore: item.lore,
            enchantments,
            custom_model_data: item.custom_model_data.map(|data| CustomModelData {
                floats: data.floats,
                flags: data.flags,
                strings: data.strings,
                colors: data.colors,
            }),
            components: item.components,
        })
    }

    pub(crate) fn to_wire(&self) -> v1::Item {
        self.clone().into_wire()
    }

    pub(crate) fn into_wire(self) -> v1::Item {
        v1::Item {
            key: self.material.key().to_owned(),
            count: i32::from(self.count),
            name: self.name,
            lore: self.lore,
            enchantments: self
                .enchantments
                .into_iter()
                .map(|(enchantment, level)| v1::Enchant {
                    key: enchantment.key().to_owned(),
                    level: i32::try_from(level).unwrap_or(i32::MAX),
                })
                .collect(),
            components: self.components,
            custom_model_data: self.custom_model_data.map(|data| v1::CustomModelData {
                floats: data.floats,
                flags: data.flags,
                strings: data.strings,
                colors: data.colors,
            }),
        }
    }
}

impl Effect {
    pub(crate) fn from_wire(effect: v1::Effect) -> Result<Self, Error> {
        let kind = EffectType::from_key(&effect.key)
            .ok_or_else(|| Error::Malformed(format!("an unknown effect `{}`", effect.key)))?;
        let amplifier = u8::try_from(effect.amplifier)
            .map_err(|_| Error::Malformed(format!("an effect amplifier {}", effect.amplifier)))?;
        Ok(Self {
            kind,
            duration: effect.duration,
            amplifier,
            ambient: effect.ambient,
            particles: effect.particles,
            icon: effect.icon,
        })
    }

    pub(crate) fn to_wire(self) -> v1::Effect {
        v1::Effect {
            key: self.kind.key().to_owned(),
            duration: self.duration,
            amplifier: i32::from(self.amplifier),
            ambient: self.ambient,
            particles: self.particles,
            icon: self.icon,
        }
    }
}

impl Particle {
    pub(crate) fn to_wire(
        self,
        position: Position,
        count: i32,
        spread: Vector,
        speed: f64,
    ) -> v1::Particle {
        let data = match self {
            Self::Item(material) => Some(particle::Data::Item(material.key().to_owned())),
            Self::Block(block)
            | Self::BlockMarker(block)
            | Self::FallingDust(block)
            | Self::DustPillar(block)
            | Self::BlockCrumble(block) => Some(particle::Data::Block(block.key().to_owned())),
            Self::Dust { color, scale } => Some(particle::Data::Dust(v1::Dust {
                color: color.to_rgb(),
                scale,
            })),
            _ => None,
        };
        v1::Particle {
            key: self.key().to_owned(),
            data,
            position: Some(position.to_wire()),
            count,
            spread: Some(spread.to_wire()),
            speed,
        }
    }
}

pub(crate) fn millis(time: SystemTime) -> i64 {
    match time.duration_since(UNIX_EPOCH) {
        Ok(after) => i64::try_from(after.as_millis()).unwrap_or(i64::MAX),
        Err(before) => i64::try_from(before.duration().as_millis()).map_or(i64::MIN, |ms| -ms),
    }
}

pub(crate) fn from_millis(ms: i64) -> Result<SystemTime, Error> {
    let span = Duration::from_millis(ms.unsigned_abs());
    let time = if ms < 0 {
        UNIX_EPOCH.checked_sub(span)
    } else {
        UNIX_EPOCH.checked_add(span)
    };
    time.ok_or_else(|| Error::Malformed(format!("{ms} ms, which is not a time this system holds")))
}

pub(crate) fn parse_address(text: &str) -> Result<IpAddr, Error> {
    text.parse()
        .map_err(|_| Error::Malformed(format!("`{text}`, which is not an IP address")))
}

pub(crate) fn parse_uuid(text: &str, of: &str) -> Result<Uuid, Error> {
    Uuid::parse_str(text).map_err(|_| Error::Malformed(format!("`{text}`, which is not {of} UUID")))
}

pub(crate) fn required<T>(field: Option<T>, name: &str) -> Result<T, Error> {
    field.ok_or_else(|| Error::Malformed(format!("a message without `{name}`")))
}

pub(crate) fn byte(value: i32) -> Result<i8, Error> {
    i8::try_from(value).map_err(|_| Error::Malformed(format!("{value}, which is not a byte")))
}

pub(crate) fn note(value: i32) -> Result<u8, Error> {
    u8::try_from(value)
        .ok()
        .filter(|note| *note <= 24)
        .ok_or_else(|| Error::Malformed(format!("{value}, which is not a note")))
}

pub(crate) fn uuid(text: String) -> Result<Uuid, Error> {
    parse_uuid(&text, "a")
}

pub(crate) fn address(text: String) -> Result<IpAddr, Error> {
    parse_address(&text)
}

pub(crate) fn world(name: String, link: &Link) -> World {
    World::named(name, link.clone())
}

pub(crate) fn material(key: String) -> Result<Material, Error> {
    Material::from_key(&key).ok_or_else(|| Error::Malformed(format!("an unknown item `{key}`")))
}

pub(crate) fn entity_type(key: String) -> Result<EntityType, Error> {
    EntityType::from_key(&key)
        .ok_or_else(|| Error::Malformed(format!("an unknown entity type `{key}`")))
}

pub(crate) fn enchantment(key: String) -> Result<Enchantment, Error> {
    Enchantment::from_key(&key)
        .ok_or_else(|| Error::Malformed(format!("an unknown enchantment `{key}`")))
}

pub(crate) fn effect_type(key: String) -> Result<EffectType, Error> {
    EffectType::from_key(&key).ok_or_else(|| Error::Malformed(format!("an unknown effect `{key}`")))
}

pub(crate) fn block(pos: v1::BlockPos, link: &Link) -> Block {
    Block::new(
        BlockPos {
            x: pos.x,
            y: pos.y,
            z: pos.z,
        },
        link.clone(),
    )
}

pub(crate) const fn vector(vec: v1::Vec3) -> Vector {
    Vector {
        x: vec.x,
        y: vec.y,
        z: vec.z,
    }
}

pub(crate) fn enchant(enchant: v1::Enchant) -> Result<(Enchantment, u32), Error> {
    let level = u32::try_from(enchant.level)
        .map_err(|_| Error::Malformed(format!("an enchantment level {}", enchant.level)))?;
    Ok((enchantment(enchant.key)?, level))
}

pub(crate) fn slot(slot: v1::Slot) -> Result<(i32, ItemStack), Error> {
    Ok((
        slot.slot,
        ItemStack::from_wire(required(slot.item, "item")?)?,
    ))
}

pub(crate) fn player_entity(player: &v1::PlayerSnapshot) -> v1::EntitySnapshot {
    v1::EntitySnapshot {
        entity_id: player.entity_id,
        uuid: player.uuid.clone(),
        kind: "minecraft:player".to_owned(),
        position: player.position,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn custom_model_data_survives_the_wire() {
        let stack = ItemStack::new(Material::Stick, 1).custom_model_data(CustomModelData {
            floats: vec![1.5],
            flags: vec![true],
            strings: vec!["skin".to_owned()],
            colors: vec![255],
        });
        let wire = stack.to_wire();
        let data = wire.custom_model_data.clone().unwrap();
        assert_eq!(data.floats, [1.5]);
        assert_eq!(data.strings, ["skin"]);
        assert_eq!(ItemStack::from_wire(wire).unwrap(), stack);
    }
}

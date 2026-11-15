use std::time::{Duration, SystemTime, UNIX_EPOCH};

use rspigot_core::plugins::{
    EffectSpec, ItemSpec, ParticleDataSpec, ParticleSpec, SoundSpec, TitleSpec,
};
use rspigot_domain::item::CustomModelData;
use rspigot_domain::{BlockPos, Rotation, Vec3};
use rspigot_plugin_protocol::v1::{self, particle};
use uuid::Uuid;

use super::Malformed;

pub(super) fn uuid(raw: String) -> Result<Uuid, Malformed> {
    Uuid::parse_str(&raw).map_err(|_| Malformed::Uuid(raw))
}

pub(super) fn required<T>(field: Option<T>) -> Result<T, Malformed> {
    field.ok_or(Malformed::Empty)
}

pub(super) fn expiry(ms: Option<i64>) -> Result<Option<SystemTime>, Malformed> {
    ms.map(time).transpose()
}

fn time(ms: i64) -> Result<SystemTime, Malformed> {
    let offset = Duration::from_millis(ms.unsigned_abs());
    let time = if ms < 0 {
        UNIX_EPOCH.checked_sub(offset)
    } else {
        UNIX_EPOCH.checked_add(offset)
    };
    time.ok_or(Malformed::Time(ms))
}

pub(super) fn wire_time(time: SystemTime) -> i64 {
    match time.duration_since(UNIX_EPOCH) {
        Ok(after) => i64::try_from(after.as_millis()).unwrap_or(i64::MAX),
        Err(before) => i64::try_from(before.duration().as_millis()).map_or(i64::MIN, |ms| -ms),
    }
}

pub(super) const fn vec3(vec: v1::Vec3) -> Vec3 {
    Vec3::new(vec.x, vec.y, vec.z)
}

pub(super) const fn wire_vec(vec: Vec3) -> v1::Vec3 {
    v1::Vec3 {
        x: vec.x,
        y: vec.y,
        z: vec.z,
    }
}

pub(super) const fn block_pos(pos: v1::BlockPos) -> BlockPos {
    BlockPos::new(pos.x, pos.y, pos.z)
}

pub(super) const fn wire_block_pos(pos: BlockPos) -> v1::BlockPos {
    v1::BlockPos {
        x: pos.x,
        y: pos.y,
        z: pos.z,
    }
}

pub(super) const fn rotation(rotation: v1::Rotation) -> Rotation {
    Rotation::new(rotation.yaw, rotation.pitch)
}

pub(super) const fn wire_rotation(rotation: Rotation) -> v1::Rotation {
    v1::Rotation {
        yaw: rotation.yaw,
        pitch: rotation.pitch,
    }
}

pub(super) fn effect(effect: v1::Effect) -> EffectSpec {
    EffectSpec {
        key: effect.key,
        duration: effect.duration,
        amplifier: effect.amplifier,
        ambient: effect.ambient,
        particles: effect.particles,
        icon: effect.icon,
    }
}

pub(super) fn wire_effect(effect: EffectSpec) -> v1::Effect {
    v1::Effect {
        key: effect.key,
        duration: effect.duration,
        amplifier: effect.amplifier,
        ambient: effect.ambient,
        particles: effect.particles,
        icon: effect.icon,
    }
}

pub(super) fn particle(particle: v1::Particle) -> Result<ParticleSpec, Malformed> {
    let data = match particle.data {
        None => ParticleDataSpec::None,
        Some(particle::Data::Item(item)) => ParticleDataSpec::Item(item),
        Some(particle::Data::Block(block)) => ParticleDataSpec::Block(block),
        Some(particle::Data::Dust(dust)) => ParticleDataSpec::Dust {
            color: dust.color,
            scale: dust.scale,
        },
    };
    Ok(ParticleSpec {
        key: particle.key,
        data,
        position: vec3(required(particle.position)?),
        count: particle.count,
        spread: vec3(required(particle.spread)?),
        speed: particle.speed,
    })
}

pub(super) fn sound(sound: v1::Sound) -> SoundSpec {
    SoundSpec {
        key: sound.key,
        volume: sound.volume,
        pitch: sound.pitch,
    }
}

pub(super) fn title(title: v1::Title) -> TitleSpec {
    TitleSpec {
        title: title.title,
        subtitle: title.subtitle,
        fade_in: title.fade_in,
        stay: title.stay,
        fade_out: title.fade_out,
    }
}

pub(super) fn item(item: v1::Item) -> ItemSpec {
    ItemSpec {
        key: item.key,
        count: item.count,
        name: item.name,
        lore: item.lore,
        enchantments: item
            .enchantments
            .into_iter()
            .map(|enchant| (enchant.key, enchant.level))
            .collect(),
        custom_model_data: item.custom_model_data.map(|data| CustomModelData {
            floats: data.floats,
            flags: data.flags,
            strings: data.strings,
            colors: data.colors,
        }),
        components: item.components,
    }
}

pub(super) fn wire_item(item: ItemSpec) -> v1::Item {
    v1::Item {
        key: item.key,
        count: item.count,
        name: item.name,
        lore: item.lore,
        enchantments: item
            .enchantments
            .into_iter()
            .map(|(key, level)| v1::Enchant { key, level })
            .collect(),
        components: item.components,
        custom_model_data: item.custom_model_data.map(|data| v1::CustomModelData {
            floats: data.floats,
            flags: data.flags,
            strings: data.strings,
            colors: data.colors,
        }),
    }
}

pub(super) fn byte(value: i32) -> Result<i8, Malformed> {
    i8::try_from(value).map_err(|_| Malformed::OutOfRange {
        field: "byte",
        value: i64::from(value),
    })
}

pub(super) fn wire_enchant((key, level): (String, i32)) -> v1::Enchant {
    v1::Enchant { key, level }
}

pub(super) fn wire_slot((slot, item): (i32, ItemSpec)) -> v1::Slot {
    v1::Slot {
        slot,
        item: Some(wire_item(item)),
    }
}

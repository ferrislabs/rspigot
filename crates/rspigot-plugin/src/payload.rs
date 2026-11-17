use std::time::SystemTime;

use rspigot_plugin_protocol::v1;

use crate::player::{Position, Rotation, Vector};
use crate::wire::millis;
use crate::{
    CustomInventory, Effect, EffectType, GameMode, InventorySlot, ItemStack, Location, Material,
    Particle, Player, Scoreboard, Sound, Title,
};

#[derive(Debug, Clone, PartialEq)]
pub struct SoundCue {
    pub sound: Sound,
    pub volume: f32,
    pub pitch: f32,
    pub position: Option<Position>,
}

impl SoundCue {
    pub const fn new(sound: Sound) -> Self {
        Self {
            sound,
            volume: 1.0,
            pitch: 1.0,
            position: None,
        }
    }

    #[must_use]
    pub const fn volume(mut self, volume: f32) -> Self {
        self.volume = volume;
        self
    }

    #[must_use]
    pub const fn pitch(mut self, pitch: f32) -> Self {
        self.pitch = pitch;
        self
    }

    #[must_use]
    pub const fn at(mut self, position: Position) -> Self {
        self.position = Some(position);
        self
    }
}

impl From<SoundCue> for v1::Sound {
    fn from(cue: SoundCue) -> Self {
        Self {
            key: cue.sound.key().to_owned(),
            volume: cue.volume,
            pitch: cue.pitch,
            position: cue.position.map(Position::to_wire),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ParticleBurst {
    pub particle: Particle,
    pub position: Position,
    pub count: i32,
    pub spread: Vector,
    pub speed: f64,
}

impl ParticleBurst {
    pub fn new(particle: Particle, position: Position) -> Self {
        Self {
            particle,
            position,
            count: 1,
            spread: Vector::default(),
            speed: 0.0,
        }
    }

    #[must_use]
    pub const fn count(mut self, count: i32) -> Self {
        self.count = count;
        self
    }

    #[must_use]
    pub const fn spread(mut self, spread: Vector) -> Self {
        self.spread = spread;
        self
    }

    #[must_use]
    pub const fn speed(mut self, speed: f64) -> Self {
        self.speed = speed;
        self
    }
}

impl From<ParticleBurst> for v1::Particle {
    fn from(burst: ParticleBurst) -> Self {
        burst
            .particle
            .to_wire(burst.position, burst.count, burst.spread, burst.speed)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TabList {
    pub header: String,
    pub footer: String,
}

impl TabList {
    pub fn new(header: impl Into<String>, footer: impl Into<String>) -> Self {
        Self {
            header: header.into(),
            footer: footer.into(),
        }
    }
}

impl From<TabList> for v1::TabList {
    fn from(tab_list: TabList) -> Self {
        Self {
            header: tab_list.header,
            footer: tab_list.footer,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Facing {
    pub position: Position,
    pub rotation: Rotation,
}

impl Facing {
    pub const fn new(position: Position, rotation: Rotation) -> Self {
        Self { position, rotation }
    }
}

impl From<Facing> for v1::Facing {
    fn from(facing: Facing) -> Self {
        Self {
            position: Some(facing.position.to_wire()),
            rotation: Some(facing.rotation.to_wire()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct BanTerms {
    pub reason: Option<String>,
    pub expires: Option<SystemTime>,
}

impl From<BanTerms> for v1::BanTerms {
    fn from(terms: BanTerms) -> Self {
        Self {
            reason: terms.reason,
            expires_ms: terms.expires.map(millis),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ban {
    pub target: String,
    pub reason: Option<String>,
    pub expires: Option<SystemTime>,
}

impl Ban {
    pub fn new(target: impl ToString) -> Self {
        Self {
            target: target.to_string(),
            reason: None,
            expires: None,
        }
    }

    #[must_use]
    pub fn reason(mut self, reason: impl Into<String>) -> Self {
        self.reason = Some(reason.into());
        self
    }

    #[must_use]
    pub const fn expires(mut self, expires: SystemTime) -> Self {
        self.expires = Some(expires);
        self
    }
}

impl From<Ban> for v1::Ban {
    fn from(ban: Ban) -> Self {
        Self {
            target: ban.target,
            reason: ban.reason,
            expires_ms: ban.expires.map(millis),
        }
    }
}

impl From<Position> for v1::Vec3 {
    fn from(position: Position) -> Self {
        position.to_wire()
    }
}

impl From<Vector> for v1::Vec3 {
    fn from(vector: Vector) -> Self {
        vector.to_wire()
    }
}

impl From<GameMode> for v1::GameMode {
    fn from(mode: GameMode) -> Self {
        mode.to_wire()
    }
}

impl From<Effect> for v1::Effect {
    fn from(effect: Effect) -> Self {
        effect.to_wire()
    }
}

impl From<EffectType> for String {
    fn from(kind: EffectType) -> Self {
        kind.key().to_owned()
    }
}

impl From<Title> for v1::Title {
    fn from(title: Title) -> Self {
        title.into_wire()
    }
}

impl From<&Location> for v1::Location {
    fn from(location: &Location) -> Self {
        location.to_wire()
    }
}

impl From<&Player> for String {
    fn from(player: &Player) -> Self {
        player.uuid.to_string()
    }
}

impl From<&Scoreboard> for u32 {
    fn from(scoreboard: &Scoreboard) -> Self {
        scoreboard.id()
    }
}

impl From<&CustomInventory> for u64 {
    fn from(inventory: &CustomInventory) -> Self {
        inventory.id()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct SlotItem {
    pub slot: InventorySlot,
    pub item: Option<ItemStack>,
}

impl SlotItem {
    pub const fn new(slot: InventorySlot, item: Option<ItemStack>) -> Self {
        Self { slot, item }
    }
}

impl From<SlotItem> for v1::SetSlot {
    fn from(content: SlotItem) -> Self {
        Self {
            slot: content.slot.index().unwrap_or(-1),
            item: content.item.map(ItemStack::into_wire),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ItemRemoval {
    pub material: Material,
    pub count: i32,
}

impl ItemRemoval {
    pub const fn new(material: Material, count: i32) -> Self {
        Self { material, count }
    }
}

impl From<ItemRemoval> for v1::RemoveItems {
    fn from(removal: ItemRemoval) -> Self {
        Self {
            key: removal.material.key().to_owned(),
            count: removal.count,
        }
    }
}

impl From<ItemStack> for v1::Item {
    fn from(item: ItemStack) -> Self {
        item.into_wire()
    }
}

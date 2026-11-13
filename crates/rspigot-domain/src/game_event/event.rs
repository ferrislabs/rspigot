use rspigot_data::registries;
use rspigot_data::tags;

pub const REGISTRY: &str = "minecraft:game_event";
pub const DEFAULT_NOTIFICATION_RADIUS: i32 = 16;
pub const MAX_FREQUENCY: i32 = 15;

macro_rules! game_events {
    ($($variant:ident = $name:literal),* $(,)?) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        pub enum GameEvent {
            $($variant,)*
        }

        impl GameEvent {
            pub const ALL: &'static [Self] = &[$(Self::$variant,)*];

            pub const fn name(self) -> &'static str {
                match self {
                    $(Self::$variant => $name,)*
                }
            }
        }
    };
}

game_events! {
    BlockActivate = "minecraft:block_activate",
    BlockAttach = "minecraft:block_attach",
    BlockChange = "minecraft:block_change",
    BlockClose = "minecraft:block_close",
    BlockDeactivate = "minecraft:block_deactivate",
    BlockDestroy = "minecraft:block_destroy",
    BlockDetach = "minecraft:block_detach",
    BlockOpen = "minecraft:block_open",
    BlockPlace = "minecraft:block_place",
    Bounce = "minecraft:bounce",
    ContainerClose = "minecraft:container_close",
    ContainerOpen = "minecraft:container_open",
    Drink = "minecraft:drink",
    Eat = "minecraft:eat",
    ElytraGlide = "minecraft:elytra_glide",
    EntityDamage = "minecraft:entity_damage",
    EntityDie = "minecraft:entity_die",
    EntityDismount = "minecraft:entity_dismount",
    EntityInteract = "minecraft:entity_interact",
    EntityMount = "minecraft:entity_mount",
    EntityPlace = "minecraft:entity_place",
    EntityAction = "minecraft:entity_action",
    Equip = "minecraft:equip",
    Explode = "minecraft:explode",
    Flap = "minecraft:flap",
    FluidPickup = "minecraft:fluid_pickup",
    FluidPlace = "minecraft:fluid_place",
    HitGround = "minecraft:hit_ground",
    InstrumentPlay = "minecraft:instrument_play",
    ItemInteractFinish = "minecraft:item_interact_finish",
    ItemInteractStart = "minecraft:item_interact_start",
    JukeboxPlay = "minecraft:jukebox_play",
    JukeboxStopPlay = "minecraft:jukebox_stop_play",
    LightningStrike = "minecraft:lightning_strike",
    NoteBlockPlay = "minecraft:note_block_play",
    PrimeFuse = "minecraft:prime_fuse",
    ProjectileLand = "minecraft:projectile_land",
    ProjectileShoot = "minecraft:projectile_shoot",
    SculkSensorTendrilsClicking = "minecraft:sculk_sensor_tendrils_clicking",
    Shear = "minecraft:shear",
    Shriek = "minecraft:shriek",
    Splash = "minecraft:splash",
    Step = "minecraft:step",
    Swim = "minecraft:swim",
    Teleport = "minecraft:teleport",
    Unequip = "minecraft:unequip",
    Resonate1 = "minecraft:resonate_1",
    Resonate2 = "minecraft:resonate_2",
    Resonate3 = "minecraft:resonate_3",
    Resonate4 = "minecraft:resonate_4",
    Resonate5 = "minecraft:resonate_5",
    Resonate6 = "minecraft:resonate_6",
    Resonate7 = "minecraft:resonate_7",
    Resonate8 = "minecraft:resonate_8",
    Resonate9 = "minecraft:resonate_9",
    Resonate10 = "minecraft:resonate_10",
    Resonate11 = "minecraft:resonate_11",
    Resonate12 = "minecraft:resonate_12",
    Resonate13 = "minecraft:resonate_13",
    Resonate14 = "minecraft:resonate_14",
    Resonate15 = "minecraft:resonate_15",
}

const RESONANCE: [GameEvent; 15] = [
    GameEvent::Resonate1,
    GameEvent::Resonate2,
    GameEvent::Resonate3,
    GameEvent::Resonate4,
    GameEvent::Resonate5,
    GameEvent::Resonate6,
    GameEvent::Resonate7,
    GameEvent::Resonate8,
    GameEvent::Resonate9,
    GameEvent::Resonate10,
    GameEvent::Resonate11,
    GameEvent::Resonate12,
    GameEvent::Resonate13,
    GameEvent::Resonate14,
    GameEvent::Resonate15,
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GameEventTag {
    Vibrations,
    WardenCanListen,
    ShriekerCanListen,
    IgnoreVibrationsSneaking,
    AllayCanListen,
}

impl GameEventTag {
    pub const fn name(self) -> &'static str {
        match self {
            Self::Vibrations => "minecraft:vibrations",
            Self::WardenCanListen => "minecraft:warden_can_listen",
            Self::ShriekerCanListen => "minecraft:shrieker_can_listen",
            Self::IgnoreVibrationsSneaking => "minecraft:ignore_vibrations_sneaking",
            Self::AllayCanListen => "minecraft:allay_can_listen",
        }
    }
}

impl GameEvent {
    pub fn index(self) -> u32 {
        self as u32
    }

    pub fn from_name(name: &str) -> Option<Self> {
        let index = registries::static_index(REGISTRY, name)?;
        Self::ALL.get(index as usize).copied()
    }

    pub const fn notification_radius(self) -> i32 {
        match self {
            Self::JukeboxPlay | Self::JukeboxStopPlay => 10,
            Self::Shriek => 32,
            _ => DEFAULT_NOTIFICATION_RADIUS,
        }
    }

    pub fn is_in(self, tag: GameEventTag) -> bool {
        tags::tag(REGISTRY, tag.name())
            .is_some_and(|members| members.binary_search(&self.index()).is_ok())
    }

    pub const fn frequency(self) -> i32 {
        match self {
            Self::Step | Self::Swim | Self::Flap => 1,
            Self::ProjectileLand | Self::HitGround | Self::Splash | Self::Bounce => 2,
            Self::ItemInteractFinish | Self::ProjectileShoot | Self::InstrumentPlay => 3,
            Self::EntityAction | Self::ElytraGlide | Self::Unequip => 4,
            Self::EntityDismount | Self::Equip => 5,
            Self::EntityInteract | Self::Shear | Self::EntityMount => 6,
            Self::EntityDamage => 7,
            Self::Drink | Self::Eat => 8,
            Self::ContainerClose | Self::BlockClose | Self::BlockDeactivate | Self::BlockDetach => {
                9
            }
            Self::ContainerOpen
            | Self::BlockOpen
            | Self::BlockActivate
            | Self::BlockAttach
            | Self::PrimeFuse
            | Self::NoteBlockPlay => 10,
            Self::BlockChange => 11,
            Self::BlockDestroy | Self::FluidPickup => 12,
            Self::BlockPlace | Self::FluidPlace => 13,
            Self::EntityPlace | Self::LightningStrike | Self::Teleport => 14,
            Self::EntityDie | Self::Explode => 15,
            Self::ItemInteractStart
            | Self::JukeboxPlay
            | Self::JukeboxStopPlay
            | Self::SculkSensorTendrilsClicking
            | Self::Shriek => 0,
            Self::Resonate1 => 1,
            Self::Resonate2 => 2,
            Self::Resonate3 => 3,
            Self::Resonate4 => 4,
            Self::Resonate5 => 5,
            Self::Resonate6 => 6,
            Self::Resonate7 => 7,
            Self::Resonate8 => 8,
            Self::Resonate9 => 9,
            Self::Resonate10 => 10,
            Self::Resonate11 => 11,
            Self::Resonate12 => 12,
            Self::Resonate13 => 13,
            Self::Resonate14 => 14,
            Self::Resonate15 => 15,
        }
    }

    pub fn resonance_by_frequency(frequency: i32) -> Option<Self> {
        usize::try_from(frequency - 1)
            .ok()
            .and_then(|index| RESONANCE.get(index))
            .copied()
    }
}

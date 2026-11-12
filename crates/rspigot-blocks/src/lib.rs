#[rustfmt::skip]
mod blocks;
pub mod family;
pub mod infested;
#[rustfmt::skip]
mod kind;
#[rustfmt::skip]
mod shapes;
mod state;

pub use blocks::*;
pub use family::{
    BlockBehavior, Face, Gravity, Linked, LinkedProperty, PlaceContext, Placement,
    SCAFFOLDING_MAX_DISTANCE, UseContext, UseResult, scaffolding_settle, speleothem_thickness,
};
pub use kind::{BlockKind, STATE_COUNT};
pub use shapes::{SHAPE_COUNT, SHAPES};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
pub struct StateId(pub u16);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct BlockId(pub u16);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Property {
    pub name: &'static str,
    pub values: &'static [&'static str],
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PerState<T: 'static> {
    Uniform(T),
    Varies(&'static [T]),
}

impl<T: Copy> PerState<T> {
    pub const fn get(&self, offset: u16) -> T {
        match self {
            Self::Uniform(value) => *value,
            Self::Varies(values) => values[offset as usize],
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct StateFlags(pub u16);

impl StateFlags {
    pub const AIR: Self = Self(1);
    pub const BLOCKS_MOTION: Self = Self(1 << 1);
    pub const HAS_COLLISION: Self = Self(1 << 2);
    pub const CAN_OCCLUDE: Self = Self(1 << 3);
    pub const SOLID_RENDER: Self = Self(1 << 4);
    pub const PROPAGATES_SKYLIGHT: Self = Self(1 << 5);
    pub const REPLACEABLE: Self = Self(1 << 6);
    pub const WATER: Self = Self(1 << 7);
    pub const LAVA: Self = Self(1 << 8);
    pub const RANDOMLY_TICKS: Self = Self(1 << 9);
    pub const FULL_COLLISION_CUBE: Self = Self(1 << 10);
    pub const LEGACY_SOLID: Self = Self(1 << 11);
    pub const REDSTONE_CONDUCTOR: Self = Self(1 << 12);
    pub const SIGNAL_SOURCE: Self = Self(1 << 13);
    pub const PATHFINDABLE_LAND: Self = Self(1 << 14);

    pub const fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }

    pub const fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }

    pub const fn has_fluid(self) -> bool {
        self.0 & (Self::WATER.0 | Self::LAVA.0) != 0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum SpawnSurface {
    #[default]
    Default,
    Never,
    Always,
    OcelotOrParrot,
    PolarBear,
    FireImmune,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Aabb {
    pub min_x: f64,
    pub min_y: f64,
    pub min_z: f64,
    pub max_x: f64,
    pub max_y: f64,
    pub max_z: f64,
}

impl Aabb {
    pub const fn new(
        min_x: f64,
        min_y: f64,
        min_z: f64,
        max_x: f64,
        max_y: f64,
        max_z: f64,
    ) -> Self {
        Self {
            min_x,
            min_y,
            min_z,
            max_x,
            max_y,
            max_z,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
pub struct ShapeId(pub u16);

impl ShapeId {
    pub const EMPTY: Self = Self(0);
    pub const FULL: Self = Self(1);

    pub fn boxes(self) -> &'static [Aabb] {
        SHAPES.get(usize::from(self.0)).copied().unwrap_or(&[])
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Support {
    Full,
    Center,
    Rigid,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct FaceSupport(pub u8, pub u8, pub u8);

impl FaceSupport {
    pub const NONE: Self = Self(0, 0, 0);
    pub const ALL: Self = Self(0x3f, 0x3f, 0x3f);

    pub const fn mask(self, support: Support) -> u8 {
        match support {
            Support::Full => self.0,
            Support::Center => self.1,
            Support::Rigid => self.2,
        }
    }

    pub const fn is_sturdy(self, face: Face, support: Support) -> bool {
        self.mask(support) & (1 << face as u8) != 0
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BlockSettings {
    pub hardness: f32,
    pub resistance: f32,
    pub friction: f32,
    pub speed_factor: f32,
    pub jump_factor: f32,
    pub bounce_restitution: f32,
    pub fall_distance_reduction: f32,
    pub requires_correct_tool: bool,
    pub ignited_by_lava: bool,
}

pub trait Block: 'static {
    const NAME: &'static str;
    const ID: BlockId;
    const FIRST_STATE: StateId;
    const STATE_COUNT: u16;
    const DEFAULT_STATE: StateId;
    const PROPERTIES: &'static [Property];
    const ITEM: Option<u16>;
    const WALL_VARIANT: Option<BlockId>;
    const SETTINGS: BlockSettings;
    const LIGHT_EMISSION: PerState<u8>;
    const LIGHT_DAMPENING: PerState<u8>;
    const FLAGS: PerState<StateFlags>;
    const COLLISION: PerState<ShapeId>;
    const FACE_SUPPORT: PerState<FaceSupport>;
    const SPAWN_SURFACE: SpawnSurface = SpawnSurface::Default;
    type Behavior: BlockBehavior;
}

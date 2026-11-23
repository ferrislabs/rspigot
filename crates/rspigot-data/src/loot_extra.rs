use crate::loot::LevelBasedValue;
use crate::loot::{Condition, EntityTarget, Entry, Function, IntProvider, ItemPredicate, ListMode};
use crate::tags::HolderSet;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Aggregate {
    Sum,
    Product,
    Average,
    Minimum,
    Maximum,
    Length,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Binary {
    Difference,
    Quotient,
    FloorQuotient,
    Modulus,
    FloorModulus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Unary {
    Absolute,
    Negate,
    Ceiling,
    Floor,
    Round,
    Truncate,
    Sine,
    Cosine,
    SquareRoot,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScoreHolder {
    Context(EntityTarget),
    Fixed(&'static str),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum IntExpr {
    Constant(i32),
    Uniform {
        min: &'static IntExpr,
        max: &'static IntExpr,
    },
    Binomial {
        n: &'static IntExpr,
        p: &'static FloatExpr,
    },
    Aggregate {
        op: Aggregate,
        inputs: &'static [IntExpr],
    },
    Binary {
        op: Binary,
        left: &'static IntExpr,
        right: &'static IntExpr,
    },
    Unary {
        op: Unary,
        input: &'static IntExpr,
    },
    Power {
        base: &'static IntExpr,
        exponent: &'static IntExpr,
    },
    FromFloat(&'static FloatExpr),
    Conditional {
        condition: &'static Condition,
        on_true: &'static IntExpr,
        on_false: &'static IntExpr,
    },
    Dispatch {
        cases: &'static [(Condition, IntExpr)],
        default: &'static IntExpr,
    },
    Weighted(&'static [(IntExpr, i32)]),
    Score {
        holder: ScoreHolder,
        objective: &'static str,
        fallback: &'static IntExpr,
    },
    Storage {
        storage: &'static str,
        path: &'static str,
        fallback: &'static IntExpr,
    },
    Attribute(&'static str),
    Reference(&'static str),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FloatExpr {
    Constant(f32),
    Uniform {
        min: &'static FloatExpr,
        max: &'static FloatExpr,
    },
    Aggregate {
        op: Aggregate,
        inputs: &'static [FloatExpr],
    },
    Binary {
        op: Binary,
        left: &'static FloatExpr,
        right: &'static FloatExpr,
    },
    Unary {
        op: Unary,
        input: &'static FloatExpr,
    },
    Power {
        base: &'static FloatExpr,
        exponent: &'static FloatExpr,
    },
    FromInt(&'static IntExpr),
    EnchantmentLevel(LevelBasedValue),
    Conditional {
        condition: &'static Condition,
        on_true: &'static FloatExpr,
        on_false: &'static FloatExpr,
    },
    Dispatch {
        cases: &'static [(Condition, FloatExpr)],
        default: &'static FloatExpr,
    },
    Weighted(&'static [(FloatExpr, i32)]),
    Storage {
        storage: &'static str,
        path: &'static str,
        fallback: &'static FloatExpr,
    },
    Attribute(&'static str),
    Reference(&'static str),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AttributeValue {
    Bool(bool),
    Float(f32),
    Text(&'static str),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContainerComponent {
    Container,
    BundleContents,
    ChargedProjectiles,
}

impl ContainerComponent {
    pub const fn component(self) -> &'static str {
        match self {
            Self::Container => "minecraft:container",
            Self::BundleContents => "minecraft:bundle_contents",
            Self::ChargedProjectiles => "minecraft:charged_projectiles",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SlotContainer {
    Entity(EntityTarget),
    BlockEntity,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SlotSource {
    Empty,
    Range {
        container: SlotContainer,
        slots: &'static str,
    },
    Contents {
        source: &'static SlotSource,
        component: ContainerComponent,
    },
    Filtered {
        source: &'static SlotSource,
        filter: ItemPredicate,
    },
    Limit {
        source: &'static SlotSource,
        limit: i32,
    },
    Group(&'static [SlotSource]),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AttributeModifier {
    pub attribute: &'static str,
    pub id: &'static str,
    pub operation: &'static str,
    pub amount: crate::loot::FloatProvider,
    pub slots: &'static [&'static str],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BannerLayer {
    pub pattern: &'static str,
    pub color: &'static str,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ListEdit<T: 'static> {
    pub values: &'static [T],
    pub mode: ListMode,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Explosion {
    pub shape: Option<&'static str>,
    pub colors: Option<&'static [i32]>,
    pub fade_colors: Option<&'static [i32]>,
    pub trail: Option<bool>,
    pub twinkle: Option<bool>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PageKind {
    Writable,
    Written,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CustomModelEdit {
    pub floats: Option<ListEdit<f32>>,
    pub flags: Option<ListEdit<bool>>,
    pub strings: Option<ListEdit<&'static str>>,
    pub colors: Option<ListEdit<i32>>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ExtraFunction {
    Discard,
    CopyName(EntityTarget),
    CopyComponents {
        from: Option<EntityTarget>,
        include: Option<&'static [&'static str]>,
        exclude: &'static [&'static str],
    },
    FillPlayerHead(EntityTarget),
    ModifyContents {
        component: ContainerComponent,
        modifier: &'static [Function],
    },
    SetContents {
        component: ContainerComponent,
        entries: &'static [Entry],
    },
    SetAttributes {
        modifiers: &'static [AttributeModifier],
        replace: bool,
    },
    SetBannerPattern {
        patterns: &'static [BannerLayer],
        append: bool,
    },
    SetBookCover {
        title: Option<&'static str>,
        author: Option<&'static str>,
        generation: Option<i32>,
    },
    SetCustomModelData(CustomModelEdit),
    SetFireworkExplosion(Explosion),
    SetFireworks {
        explosions: Option<ListEdit<Explosion>>,
        flight_duration: Option<i32>,
    },
    SetPages {
        kind: PageKind,
        pages: &'static [&'static str],
        mode: ListMode,
    },
    SetRandomDyes(IntProvider),
    SetRandomPotion(Option<HolderSet>),
    Filtered {
        filter: ItemPredicate,
        on_pass: &'static [Function],
        on_fail: &'static [Function],
    },
}

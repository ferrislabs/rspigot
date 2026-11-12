macro_rules! activities {
    ($($variant:ident = $name:literal;)*) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
        pub enum Activity {
            $($variant,)*
        }

        impl Activity {
            pub const ALL: &'static [Self] = &[$(Self::$variant,)*];

            pub const fn name(self) -> &'static str {
                match self {
                    $(Self::$variant => $name,)*
                }
            }
        }
    };
}

activities! {
    Core = "minecraft:core";
    Idle = "minecraft:idle";
    Work = "minecraft:work";
    Play = "minecraft:play";
    Rest = "minecraft:rest";
    Meet = "minecraft:meet";
    Panic = "minecraft:panic";
    Raid = "minecraft:raid";
    PreRaid = "minecraft:pre_raid";
    Hide = "minecraft:hide";
    Fight = "minecraft:fight";
    Celebrate = "minecraft:celebrate";
    AdmireItem = "minecraft:admire_item";
    Avoid = "minecraft:avoid";
    Ride = "minecraft:ride";
    PlayDead = "minecraft:play_dead";
    LongJump = "minecraft:long_jump";
    Ram = "minecraft:ram";
    Tongue = "minecraft:tongue";
    Swim = "minecraft:swim";
    LaySpawn = "minecraft:lay_spawn";
    Sniff = "minecraft:sniff";
    Investigate = "minecraft:investigate";
    Roar = "minecraft:roar";
    Emerge = "minecraft:emerge";
    Dig = "minecraft:dig";
}

impl Activity {
    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL
            .iter()
            .copied()
            .find(|activity| activity.name() == name)
    }
}

pub const VILLAGER_PERIOD_TICKS: i64 = 24_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Schedule {
    period_ticks: i64,
    keyframes: &'static [(i64, Activity)],
}

impl Schedule {
    pub const VILLAGER: Self = Self {
        period_ticks: VILLAGER_PERIOD_TICKS,
        keyframes: &[
            (10, Activity::Idle),
            (2000, Activity::Work),
            (9000, Activity::Meet),
            (11000, Activity::Idle),
            (12000, Activity::Rest),
        ],
    };

    pub const BABY_VILLAGER: Self = Self {
        period_ticks: VILLAGER_PERIOD_TICKS,
        keyframes: &[
            (10, Activity::Idle),
            (3000, Activity::Play),
            (6000, Activity::Idle),
            (10000, Activity::Play),
            (12000, Activity::Rest),
        ],
    };

    pub fn activity_at(&self, clock_ticks: i64) -> Activity {
        let time = clock_ticks.rem_euclid(self.period_ticks);
        self.keyframes
            .iter()
            .rev()
            .find(|(ticks, _)| *ticks <= time)
            .or_else(|| self.keyframes.last())
            .map_or(Activity::Idle, |(_, activity)| *activity)
    }
}

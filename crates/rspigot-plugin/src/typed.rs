use rspigot_plugin_protocol::v1::{self, entity_order, typed_change::Change};

use crate::{DyeColor, Entity, EntityType};

macro_rules! typed {
    ($($name:ident => $kind:ident, $method:ident);+ $(;)?) => {
        $(
            #[derive(Debug, Clone)]
            pub struct $name<'a> {
                entity: &'a Entity,
            }

            impl<'a> $name<'a> {
                pub const fn entity(&self) -> &'a Entity {
                    self.entity
                }

                fn change(&self, change: Change) {
                    self.entity
                        .order(entity_order::Order::Change(v1::TypedChange {
                            change: Some(change),
                        }));
                }
            }
        )+

        impl Entity {
            $(
                pub fn $method(&self) -> Option<$name<'_>> {
                    (self.kind == EntityType::$kind).then_some($name { entity: self })
                }
            )+
        }
    };
}

typed!(
    Zombie => Zombie, as_zombie;
    Creeper => Creeper, as_creeper;
    Sheep => Sheep, as_sheep;
    Slime => Slime, as_slime;
    Villager => Villager, as_villager;
    Wolf => Wolf, as_wolf;
    Cat => Cat, as_cat;
    Rabbit => Rabbit, as_rabbit;
);

macro_rules! indexed {
    ($name:ident { $($variant:ident = $value:expr),+ $(,)? }) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        pub enum $name {
            $($variant),+
        }

        impl $name {
            pub const ALL: &'static [Self] = &[$(Self::$variant),+];

            pub(crate) const fn wire(self) -> u32 {
                match self {
                    $(Self::$variant => $value),+
                }
            }
        }
    };
}

indexed!(Profession {
    None = 0,
    Armorer = 1,
    Butcher = 2,
    Cartographer = 3,
    Cleric = 4,
    Farmer = 5,
    Fisherman = 6,
    Fletcher = 7,
    Leatherworker = 8,
    Librarian = 9,
    Mason = 10,
    Nitwit = 11,
    Shepherd = 12,
    Toolsmith = 13,
    Weaponsmith = 14,
});

indexed!(RabbitVariant {
    Brown = 0,
    White = 1,
    Black = 2,
    WhiteSplotched = 3,
    Gold = 4,
    Salt = 5,
    Evil = 99,
});

impl Zombie<'_> {
    pub fn set_baby(&self, baby: bool) {
        self.change(Change::ZombieBaby(baby));
    }
}

impl Creeper<'_> {
    pub fn set_powered(&self, powered: bool) {
        self.change(Change::CreeperPowered(powered));
    }
}

impl Sheep<'_> {
    pub fn set_color(&self, color: DyeColor) {
        self.change(Change::SheepColor(color as u32));
    }

    pub fn set_sheared(&self, sheared: bool) {
        self.change(Change::SheepSheared(sheared));
    }
}

impl Slime<'_> {
    pub fn set_size(&self, size: i32) {
        self.change(Change::SlimeSize(size));
    }
}

impl Villager<'_> {
    pub fn set_profession(&self, profession: Profession) {
        self.change(Change::VillagerProfession(profession.wire()));
    }
}

impl Wolf<'_> {
    pub fn set_collar_color(&self, color: DyeColor) {
        self.change(Change::WolfCollar(color as u32));
    }
}

impl Cat<'_> {
    pub fn set_collar_color(&self, color: DyeColor) {
        self.change(Change::CatCollar(color as u32));
    }
}

impl Rabbit<'_> {
    pub fn set_variant(&self, variant: RabbitVariant) {
        self.change(Change::RabbitVariant(variant.wire()));
    }
}

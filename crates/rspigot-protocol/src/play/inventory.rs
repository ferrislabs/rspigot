use crate::codec::{Decode, DecodeError, Encode, MAX_LIST_LEN, decode_len, encode_len};
use crate::ids::play::clientbound as id;
use crate::play::slot::{ComponentValue, ItemStack, ItemStackTemplate};
use crate::play::types::HolderSet;
use crate::play::wire::{List, Opt, Plain, UNBOUNDED, Var, read_var, wire_struct, write_var};
use crate::types::Text;

wire_struct! {
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct ContainerSetContent = id::CONTAINER_SET_CONTENT {
        pub container_id: i32 as Var,
        pub state_id: i32 as Var,
        pub items: Vec<ItemStack> as List<Plain, UNBOUNDED>,
        pub carried: ItemStack,
    }
}

wire_struct! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct ContainerSetData = id::CONTAINER_SET_DATA {
        pub container_id: i32 as Var,
        pub id: i16,
        pub value: i16,
    }
}

wire_struct! {
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct ContainerSetSlot = id::CONTAINER_SET_SLOT {
        pub container_id: i32 as Var,
        pub state_id: i32 as Var,
        pub slot: i16,
        pub item: ItemStack,
    }
}

wire_struct! {
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct SetCursorItem = id::SET_CURSOR_ITEM {
        pub item: ItemStack,
    }
}

wire_struct! {
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct SetPlayerInventory = id::SET_PLAYER_INVENTORY {
        pub slot: i32 as Var,
        pub item: ItemStack,
    }
}

wire_struct! {
    #[derive(Debug, Clone, PartialEq)]
    pub struct OpenScreen = id::OPEN_SCREEN {
        pub container_id: i32 as Var,
        pub menu: i32 as Var,
        pub title: Text,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TradeCost {
    pub item: i32,
    pub count: i32,
    pub components: Vec<ComponentValue>,
}

impl Encode for TradeCost {
    fn encode(&self, out: &mut Vec<u8>) {
        write_var(self.item, out);
        write_var(self.count, out);
        encode_len(self.components.len(), out);
        for component in &self.components {
            write_var(component.type_id, out);
            out.extend_from_slice(&component.data);
        }
    }
}

impl Decode for TradeCost {
    fn decode(input: &mut &[u8]) -> Result<Self, DecodeError> {
        let item = read_var(input)?;
        let count = read_var(input)?;
        let components = decode_len(input, MAX_LIST_LEN)?;
        if components > 0 {
            return Err(DecodeError::InvalidValue {
                field: "trade cost component without length prefix",
                value: components as i64,
            });
        }
        Ok(Self {
            item,
            count,
            components: Vec::new(),
        })
    }
}

wire_struct! {
    #[derive(Debug, Clone, PartialEq)]
    pub struct TradeOffer {
        pub cost_a: TradeCost,
        pub result: ItemStack,
        pub cost_b: Option<TradeCost> as Opt<Plain>,
        pub out_of_stock: bool,
        pub uses: i32,
        pub max_uses: i32,
        pub xp: i32,
        pub special_price: i32,
        pub price_multiplier: f32,
        pub demand: i32,
    }
}

wire_struct! {
    #[derive(Debug, Clone, PartialEq)]
    pub struct MerchantOffers = id::MERCHANT_OFFERS {
        pub container_id: i32 as Var,
        pub offers: Vec<TradeOffer> as List<Plain, UNBOUNDED>,
        pub level: i32 as Var,
        pub xp: i32 as Var,
        pub show_progress: bool,
        pub can_restock: bool,
    }
}

wire_struct! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct ContainerClose = id::CONTAINER_CLOSE {
        pub container_id: i32 as Var,
    }
}

pub mod slot_display {
    pub const EMPTY: i32 = 0;
    pub const ANY_FUEL: i32 = 1;
    pub const WITH_ANY_POTION: i32 = 2;
    pub const ONLY_WITH_COMPONENT: i32 = 3;
    pub const ITEM: i32 = 4;
    pub const ITEM_STACK: i32 = 5;
    pub const TAG: i32 = 6;
    pub const DYED: i32 = 7;
    pub const SMITHING_TRIM: i32 = 8;
    pub const WITH_REMAINDER: i32 = 9;
    pub const COMPOSITE: i32 = 10;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SlotDisplay {
    Empty,
    AnyFuel,
    Item(i32),
    ItemStack(ItemStackTemplate),
    Tag(HolderSet),
}

impl Encode for SlotDisplay {
    fn encode(&self, out: &mut Vec<u8>) {
        match self {
            Self::Empty => write_var(slot_display::EMPTY, out),
            Self::AnyFuel => write_var(slot_display::ANY_FUEL, out),
            Self::Item(item) => {
                write_var(slot_display::ITEM, out);
                write_var(*item, out);
            }
            Self::ItemStack(template) => {
                write_var(slot_display::ITEM_STACK, out);
                template.encode(out);
            }
            Self::Tag(set) => {
                write_var(slot_display::TAG, out);
                set.encode(out);
            }
        }
    }
}

impl Decode for SlotDisplay {
    fn decode(input: &mut &[u8]) -> Result<Self, DecodeError> {
        Ok(match read_var(input)? {
            slot_display::EMPTY => Self::Empty,
            slot_display::ANY_FUEL => Self::AnyFuel,
            slot_display::ITEM => Self::Item(read_var(input)?),
            slot_display::ITEM_STACK => Self::ItemStack(ItemStackTemplate::decode(input)?),
            slot_display::TAG => Self::Tag(HolderSet::decode(input)?),
            other => {
                return Err(DecodeError::InvalidValue {
                    field: "slot display type",
                    value: i64::from(other),
                });
            }
        })
    }
}

wire_struct! {
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct RecipePropertySet {
        pub key: String,
        pub items: Vec<i32> as List<Var, UNBOUNDED>,
    }
}

wire_struct! {
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct StonecutterEntry {
        pub input: HolderSet,
        pub display: SlotDisplay,
    }
}

wire_struct! {
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct UpdateRecipes = id::UPDATE_RECIPES {
        pub property_sets: Vec<RecipePropertySet> as List<Plain, UNBOUNDED>,
        pub stonecutter: Vec<StonecutterEntry> as List<Plain, UNBOUNDED>,
    }
}

wire_struct! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
    pub struct RecipeBookTypeSettings {
        pub open: bool,
        pub filtering: bool,
    }
}

wire_struct! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
    pub struct RecipeBookSettings = id::RECIPE_BOOK_SETTINGS {
        pub crafting: RecipeBookTypeSettings,
        pub furnace: RecipeBookTypeSettings,
        pub blast_furnace: RecipeBookTypeSettings,
        pub smoker: RecipeBookTypeSettings,
    }
}

#[cfg(test)]
mod tests {
    use rspigot_nbt::Tag;

    use super::*;
    use crate::codec::decode_exact;
    use crate::play::slot::DataComponentPatch;
    use crate::play::wire::testing::roundtrip;

    #[test]
    fn container_content_layout() {
        let packet = ContainerSetContent {
            container_id: 0,
            state_id: 3,
            items: vec![ItemStack::EMPTY, ItemStack::new(1, 64)],
            carried: ItemStack::EMPTY,
        };
        assert_eq!(roundtrip(&packet), vec![0, 3, 2, 0, 64, 1, 0, 0, 0]);
        let slot = ContainerSetSlot {
            container_id: -1,
            state_id: 0,
            slot: 36,
            item: ItemStack::new(1, 1),
        };
        assert_eq!(
            roundtrip(&slot),
            vec![0xFF, 0xFF, 0xFF, 0xFF, 0x0F, 0, 0, 36, 1, 1, 0, 0]
        );
    }

    #[test]
    fn single_item_packets() {
        assert_eq!(
            roundtrip(&SetCursorItem {
                item: ItemStack::EMPTY
            }),
            vec![0]
        );
        assert_eq!(
            roundtrip(&SetPlayerInventory {
                slot: 40,
                item: ItemStack::new(1, 2)
            }),
            vec![40, 2, 1, 0, 0]
        );
        assert_eq!(roundtrip(&ContainerClose { container_id: 1 }), vec![1]);
        let screen = OpenScreen {
            container_id: 1,
            menu: 2,
            title: Text(Tag::String("Chest".into())),
        };
        assert_eq!(
            roundtrip(&screen),
            vec![1, 2, 8, 0, 5, b'C', b'h', b'e', b's', b't']
        );
    }

    #[test]
    fn merchant_offers_layout() {
        let offer = TradeOffer {
            cost_a: TradeCost {
                item: 7,
                count: 3,
                components: Vec::new(),
            },
            result: ItemStack::new(9, 2),
            cost_b: Some(TradeCost {
                item: 8,
                count: 1,
                components: Vec::new(),
            }),
            out_of_stock: true,
            uses: 4,
            max_uses: 12,
            xp: 5,
            special_price: -1,
            price_multiplier: 0.5,
            demand: 2,
        };
        let packet = MerchantOffers {
            container_id: 3,
            offers: vec![offer],
            level: 2,
            xp: 17,
            show_progress: true,
            can_restock: false,
        };
        let bytes = roundtrip(&packet);
        assert_eq!(&bytes[..2], &[3, 1]);
        assert_eq!(&bytes[2..6], &[7, 3, 0, 2]);
        assert_eq!(&bytes[6..9], &[9, 0, 0]);
        assert_eq!(&bytes[9..14], &[1, 8, 1, 0, 1]);
        assert_eq!(&bytes[14..18], &[0, 0, 0, 4]);
        assert_eq!(&bytes[18..22], &[0, 0, 0, 12]);
        assert_eq!(&bytes[22..26], &[0, 0, 0, 5]);
        assert_eq!(&bytes[26..30], &[0xFF, 0xFF, 0xFF, 0xFF]);
        assert_eq!(&bytes[30..34], &0.5f32.to_be_bytes());
        assert_eq!(&bytes[34..38], &[0, 0, 0, 2]);
        assert_eq!(&bytes[38..], &[2, 17, 1, 0]);
    }

    #[test]
    fn a_cost_with_components_writes_them_without_a_length() {
        let cost = TradeCost {
            item: 1,
            count: 1,
            components: vec![ComponentValue {
                type_id: 5,
                data: vec![0xAA, 0xBB],
            }],
        };
        let mut out = Vec::new();
        cost.encode(&mut out);
        assert_eq!(out, vec![1, 1, 1, 5, 0xAA, 0xBB]);
        assert!(decode_exact::<TradeCost>(&out).is_err());
    }

    #[test]
    fn recipes_layout() {
        let packet = UpdateRecipes {
            property_sets: vec![RecipePropertySet {
                key: "minecraft:furnace_input".into(),
                items: vec![1, 2],
            }],
            stonecutter: vec![StonecutterEntry {
                input: HolderSet::Ids(vec![1]),
                display: SlotDisplay::ItemStack(ItemStackTemplate {
                    item: 3,
                    count: 2,
                    components: DataComponentPatch::EMPTY,
                }),
            }],
        };
        let bytes = roundtrip(&packet);
        assert_eq!(&bytes[..2], &[1, 23]);
        assert_eq!(&bytes[25..28], &[2, 1, 2]);
        assert_eq!(&bytes[28..], &[1, 2, 1, 5, 3, 2, 0, 0]);
        for display in [
            SlotDisplay::Empty,
            SlotDisplay::AnyFuel,
            SlotDisplay::Item(5),
            SlotDisplay::Tag(HolderSet::Tag("minecraft:planks".into())),
        ] {
            roundtrip(&display);
        }
        assert!(decode_exact::<SlotDisplay>(&[10, 0]).is_err());
    }

    #[test]
    fn recipe_book_settings_are_eight_booleans() {
        let settings = RecipeBookSettings {
            crafting: RecipeBookTypeSettings {
                open: true,
                filtering: false,
            },
            smoker: RecipeBookTypeSettings {
                open: false,
                filtering: true,
            },
            ..RecipeBookSettings::default()
        };
        assert_eq!(roundtrip(&settings), vec![1, 0, 0, 0, 0, 0, 0, 1]);
    }
}

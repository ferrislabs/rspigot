use crate::codec::{Decode, DecodeError, Encode, decode_len, encode_len};
use crate::ids::play::clientbound as id;
use crate::play::wire::{FieldCodec, List, Opt, Plain, UNBOUNDED, Var, wire_struct};
use crate::types::Text;

pub const MAP_COLORS: usize = 128 * 128;

wire_struct! {
    #[derive(Debug, Clone, PartialEq)]
    pub struct MapDecorationData {
        pub decoration_type: i32 as Var,
        pub x: i8,
        pub y: i8,
        pub rot: i8,
        pub name: Option<Text> as Opt<Plain>,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MapPatch {
    pub start_x: u8,
    pub start_y: u8,
    pub width: u8,
    pub height: u8,
    pub colors: Vec<u8>,
}

pub struct PatchField;

impl FieldCodec<Option<MapPatch>> for PatchField {
    fn write(value: &Option<MapPatch>, out: &mut Vec<u8>) {
        match value {
            Some(patch) => {
                patch.width.encode(out);
                patch.height.encode(out);
                patch.start_x.encode(out);
                patch.start_y.encode(out);
                encode_len(patch.colors.len(), out);
                out.extend_from_slice(&patch.colors);
            }
            None => 0u8.encode(out),
        }
    }

    fn read(input: &mut &[u8]) -> Result<Option<MapPatch>, DecodeError> {
        let width = u8::decode(input)?;
        if width == 0 {
            return Ok(None);
        }
        let height = u8::decode(input)?;
        let start_x = u8::decode(input)?;
        let start_y = u8::decode(input)?;
        let len = decode_len(input, MAP_COLORS)?;
        let colors = crate::codec::take(input, len)?.to_vec();
        Ok(Some(MapPatch {
            start_x,
            start_y,
            width,
            height,
            colors,
        }))
    }
}

wire_struct! {
    #[derive(Debug, Clone, PartialEq)]
    pub struct MapItemData = id::MAP_ITEM_DATA {
        pub map_id: i32 as Var,
        pub scale: i8,
        pub locked: bool,
        pub decorations: Option<Vec<MapDecorationData>> as Opt<List<Plain, UNBOUNDED>>,
        pub patch: Option<MapPatch> as PatchField,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Packet;
    use crate::play::wire::testing::{encoded, roundtrip};
    use rspigot_nbt::Tag;

    #[test]
    fn a_full_map_update_matches_vanilla_layout() {
        let packet = MapItemData {
            map_id: 3,
            scale: 2,
            locked: true,
            decorations: Some(vec![MapDecorationData {
                decoration_type: 1,
                x: -10,
                y: 20,
                rot: 2,
                name: None,
            }]),
            patch: Some(MapPatch {
                start_x: 0,
                start_y: 0,
                width: 2,
                height: 1,
                colors: vec![4, 5],
            }),
        };
        let expected = vec![3, 2, 1, 1, 1, 1, 0xF6, 20, 2, 0, 2, 1, 0, 0, 2, 4, 5];
        assert_eq!(encoded(&packet), expected);
        assert_eq!(roundtrip(&packet), expected);
        assert_eq!(<MapItemData as Packet>::ID, 52);
    }

    #[test]
    fn an_update_without_colors_or_decorations_is_three_bytes_and_a_marker() {
        let packet = MapItemData {
            map_id: 200,
            scale: 0,
            locked: false,
            decorations: None,
            patch: None,
        };
        assert_eq!(encoded(&packet), vec![0xC8, 0x01, 0, 0, 0, 0]);
    }

    #[test]
    fn a_named_decoration_carries_its_component() {
        let packet = MapItemData {
            map_id: 1,
            scale: 0,
            locked: false,
            decorations: Some(vec![MapDecorationData {
                decoration_type: 10,
                x: 0,
                y: 0,
                rot: 0,
                name: Some(crate::types::Text(Tag::String("Home".into()))),
            }]),
            patch: None,
        };
        let bytes = encoded(&packet);
        assert_eq!(&bytes[..9], &[1, 0, 0, 1, 1, 10, 0, 0, 0]);
        assert_eq!(bytes[9], 1);
        let mut input = bytes.as_slice();
        let decoded = <MapItemData as crate::codec::Decode>::decode(&mut input).unwrap();
        assert_eq!(decoded, packet);
    }
}

use rspigot_domain::BlockPos;
use rspigot_domain::poi::{PoiChunkData, PoiRecord, PoiSectionData, PoiType};
use rspigot_nbt::{Compound, Tag, read_named, write_named};

use crate::chunk::DATA_VERSION;
use crate::datafix::{self, DataKind};
use crate::error::AnvilError;

pub fn encode(data: &PoiChunkData) -> Vec<u8> {
    let mut sections = Compound::new();
    for section in &data.sections {
        let records = section
            .records
            .iter()
            .map(|record| {
                let pos = record.pos();
                Tag::Compound(
                    Compound::new()
                        .with("pos", Tag::IntArray(vec![pos.x, pos.y, pos.z]))
                        .with("type", record.kind().name())
                        .with("free_tickets", record.free_tickets()),
                )
            })
            .collect();
        sections.insert(
            section.section_y.to_string(),
            Compound::new()
                .with("Valid", section.valid)
                .with("Records", Tag::List(records)),
        );
    }
    let root = Compound::new()
        .with("DataVersion", DATA_VERSION)
        .with("Sections", sections);
    let mut bytes = Vec::with_capacity(256);
    write_named(&mut bytes, "", &root);
    bytes
}

pub fn decode(bytes: &[u8]) -> Result<PoiChunkData, AnvilError> {
    let mut input = bytes;
    let (_, root) = read_named(&mut input)?;
    let root = datafix::upgrade_checked(DataKind::Poi, root)?.compound;
    let mut sections: Vec<PoiSectionData> = root
        .get_compound("Sections")
        .into_iter()
        .flat_map(Compound::iter)
        .filter_map(|(key, tag)| {
            let section_y = key.parse::<i32>().ok()?;
            let section = tag.as_compound()?;
            let records = section
                .get_list("Records")
                .unwrap_or_default()
                .iter()
                .filter_map(Tag::as_compound)
                .filter_map(decode_record)
                .collect();
            Some(PoiSectionData {
                section_y,
                valid: section.get_bool("Valid").unwrap_or(false),
                records,
            })
        })
        .collect();
    sections.sort_unstable_by_key(|section| section.section_y);
    Ok(PoiChunkData { sections })
}

fn decode_record(record: &Compound) -> Option<PoiRecord> {
    let [x, y, z] = record.get_int_array("pos")?[..] else {
        return None;
    };
    let kind = PoiType::from_name(record.get_str("type")?)?;
    Some(match record.get_i32("free_tickets") {
        Some(free) => PoiRecord::with_free_tickets(BlockPos::new(x, y, z), kind, free),
        None => PoiRecord::new(BlockPos::new(x, y, z), kind),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn records_round_trip_with_their_tickets() {
        let data = PoiChunkData {
            sections: vec![
                PoiSectionData {
                    section_y: -2,
                    valid: true,
                    records: vec![PoiRecord::with_free_tickets(
                        BlockPos::new(-5, -30, 7),
                        PoiType::Home,
                        0,
                    )],
                },
                PoiSectionData {
                    section_y: 4,
                    valid: false,
                    records: vec![PoiRecord::new(BlockPos::new(1, 65, 2), PoiType::Meeting)],
                },
            ],
        };
        assert_eq!(decode(&encode(&data)).unwrap(), data);
    }

    #[test]
    fn unknown_types_and_malformed_records_are_skipped() {
        let root = Compound::new().with("DataVersion", DATA_VERSION).with(
            "Sections",
            Compound::new().with(
                "4",
                Compound::new().with("Valid", true).with(
                    "Records",
                    Tag::List(vec![
                        Tag::Compound(
                            Compound::new()
                                .with("pos", Tag::IntArray(vec![1, 2, 3]))
                                .with("type", "minecraft:nope"),
                        ),
                        Tag::Compound(Compound::new().with("type", "minecraft:home")),
                        Tag::Compound(
                            Compound::new()
                                .with("pos", Tag::IntArray(vec![1, 2, 3]))
                                .with("type", "minecraft:home"),
                        ),
                    ]),
                ),
            ),
        );
        let mut bytes = Vec::new();
        write_named(&mut bytes, "", &root);
        let data = decode(&bytes).unwrap();
        assert_eq!(data.sections.len(), 1);
        assert_eq!(
            data.sections[0].records,
            vec![PoiRecord::new(BlockPos::new(1, 2, 3), PoiType::Home)]
        );
    }

    #[test]
    fn newer_data_versions_are_refused() {
        let root = Compound::new().with("DataVersion", DATA_VERSION + 1);
        let mut bytes = Vec::new();
        write_named(&mut bytes, "", &root);
        assert!(matches!(
            decode(&bytes),
            Err(AnvilError::NewerDataVersion { .. })
        ));
    }
}

use rspigot_blocks::BlockKind;

use super::*;
use crate::BiomeId;

fn bed_head() -> StateId {
    BlockKind::RedBed
        .state_with(&[("part", "head")])
        .expect("bed state")
}

fn bed_foot() -> StateId {
    BlockKind::RedBed
        .state_with(&[("part", "foot")])
        .expect("bed state")
}

fn manager() -> PoiManager {
    PoiManager::new(WorldHeight::OVERWORLD)
}

fn column(x: i32, z: i32) -> ChunkColumn {
    ChunkColumn::empty(ChunkPos::new(x, z), WorldHeight::OVERWORLD, BiomeId(0))
}

fn tag_members(tag: &str) -> Vec<PoiType> {
    let ids = rspigot_data::tags::tag("minecraft:point_of_interest_type", tag).expect("tag");
    ids.iter().map(|id| PoiType::ALL[*id as usize]).collect()
}

#[test]
fn types_follow_the_registry_order_and_names() {
    let entries = rspigot_data::registries::static_entries("minecraft:point_of_interest_type")
        .expect("registry");
    assert_eq!(entries.len(), PoiType::ALL.len());
    for (index, poi) in PoiType::ALL.iter().enumerate() {
        assert_eq!(entries[index], poi.name());
    }
}

#[test]
fn tags_match_the_datapack() {
    let village = tag_members("minecraft:village");
    let jobs = tag_members("minecraft:acquirable_job_site");
    let bees = tag_members("minecraft:bee_home");
    for poi in PoiType::ALL {
        assert_eq!(poi.is_village(), village.contains(&poi), "{poi:?}");
        assert_eq!(poi.is_acquirable_job_site(), jobs.contains(&poi), "{poi:?}");
        assert_eq!(poi.is_bee_home(), bees.contains(&poi), "{poi:?}");
    }
}

#[test]
fn blocks_map_to_their_poi_type() {
    let cases = [
        (BlockKind::BlastFurnace, PoiType::Armorer),
        (BlockKind::Smoker, PoiType::Butcher),
        (BlockKind::CartographyTable, PoiType::Cartographer),
        (BlockKind::BrewingStand, PoiType::Cleric),
        (BlockKind::Composter, PoiType::Farmer),
        (BlockKind::Barrel, PoiType::Fisherman),
        (BlockKind::FletchingTable, PoiType::Fletcher),
        (BlockKind::Cauldron, PoiType::Leatherworker),
        (BlockKind::WaterCauldron, PoiType::Leatherworker),
        (BlockKind::LavaCauldron, PoiType::Leatherworker),
        (BlockKind::PowderSnowCauldron, PoiType::Leatherworker),
        (BlockKind::Lectern, PoiType::Librarian),
        (BlockKind::Stonecutter, PoiType::Mason),
        (BlockKind::Loom, PoiType::Shepherd),
        (BlockKind::SmithingTable, PoiType::Toolsmith),
        (BlockKind::Grindstone, PoiType::Weaponsmith),
        (BlockKind::Bell, PoiType::Meeting),
        (BlockKind::Beehive, PoiType::Beehive),
        (BlockKind::BeeNest, PoiType::BeeNest),
        (BlockKind::NetherPortal, PoiType::NetherPortal),
        (BlockKind::Lodestone, PoiType::Lodestone),
        (BlockKind::LightningRod, PoiType::LightningRod),
        (BlockKind::WaxedOxidizedLightningRod, PoiType::LightningRod),
    ];
    for (kind, poi) in cases {
        assert_eq!(
            PoiType::of_state(kind.default_state()),
            Some(poi),
            "{kind:?}"
        );
    }
    assert_eq!(PoiType::of_state(BlockKind::Stone.default_state()), None);
    assert_eq!(PoiType::of_state(StateId::AIR), None);
}

#[test]
fn only_the_head_of_a_bed_is_a_home() {
    assert_eq!(PoiType::of_state(bed_head()), Some(PoiType::Home));
    assert_eq!(PoiType::of_state(bed_foot()), None);
    assert_eq!(PoiType::of_state(BlockKind::StrawBed.default_state()), None);
}

#[test]
fn tickets_and_ranges_follow_the_table() {
    assert_eq!(PoiType::Meeting.max_tickets(), 32);
    assert_eq!(PoiType::Meeting.valid_range(), 6);
    assert_eq!(PoiType::Home.max_tickets(), 1);
    assert_eq!(PoiType::Armorer.valid_range(), 1);
    assert_eq!(PoiType::Beehive.max_tickets(), 0);
    assert_eq!(PoiType::NetherPortal.max_tickets(), 0);
}

#[test]
fn a_loaded_chunk_registers_the_poi_blocks() {
    let mut chunk = column(2, -1);
    chunk.set_block(BlockPos::new(35, 64, -3), BlockKind::Bell.default_state());
    chunk.set_block(BlockPos::new(36, 64, -3), bed_head());
    chunk.set_block(BlockPos::new(37, 64, -3), bed_foot());
    chunk.set_block(BlockPos::new(38, 64, -3), BlockKind::Stone.default_state());
    let mut poi = manager();
    poi.load_chunk(&chunk);
    assert_eq!(poi.record_count(), 2);
    assert_eq!(
        poi.type_at(BlockPos::new(35, 64, -3)),
        Some(PoiType::Meeting)
    );
    assert_eq!(poi.type_at(BlockPos::new(36, 64, -3)), Some(PoiType::Home));
    assert_eq!(poi.type_at(BlockPos::new(37, 64, -3)), None);
    assert!(poi.exists(BlockPos::new(35, 64, -3), PoiType::is_village));
}

#[test]
fn reloading_a_chunk_replaces_its_records() {
    let mut chunk = column(0, 0);
    chunk.set_block(BlockPos::new(1, 64, 1), BlockKind::Bell.default_state());
    let mut poi = manager();
    poi.load_chunk(&chunk);
    chunk.set_block(BlockPos::new(1, 64, 1), StateId::AIR);
    chunk.set_block(BlockPos::new(2, 64, 2), BlockKind::Lectern.default_state());
    poi.load_chunk(&chunk);
    assert_eq!(poi.record_count(), 1);
    assert_eq!(
        poi.type_at(BlockPos::new(2, 64, 2)),
        Some(PoiType::Librarian)
    );
}

#[test]
fn unloading_a_chunk_drops_its_records() {
    let mut chunk = column(0, 0);
    chunk.set_block(BlockPos::new(1, 64, 1), BlockKind::Bell.default_state());
    let mut other = column(1, 0);
    other.set_block(BlockPos::new(17, 64, 1), BlockKind::Bell.default_state());
    let mut poi = manager();
    poi.load_chunk(&chunk);
    poi.load_chunk(&other);
    poi.unload_chunk(ChunkPos::new(0, 0));
    assert_eq!(poi.record_count(), 1);
    assert_eq!(poi.section_count(), 1);
    assert_eq!(poi.type_at(BlockPos::new(1, 64, 1)), None);
    assert_eq!(
        poi.type_at(BlockPos::new(17, 64, 1)),
        Some(PoiType::Meeting)
    );
}

#[test]
fn block_changes_add_and_remove_records() {
    let mut poi = manager();
    let pos = BlockPos::new(5, 70, 5);
    poi.on_block_change(pos, StateId::AIR, BlockKind::Bell.default_state());
    assert_eq!(poi.type_at(pos), Some(PoiType::Meeting));
    poi.on_block_change(pos, BlockKind::Bell.default_state(), StateId::AIR);
    assert_eq!(poi.type_at(pos), None);
    assert_eq!(poi.record_count(), 0);
}

#[test]
fn a_block_change_inside_one_type_keeps_the_record_and_its_tickets() {
    let mut poi = manager();
    let pos = BlockPos::new(5, 70, 5);
    poi.on_block_change(pos, StateId::AIR, bed_head());
    poi.take(|_| true, |_, _| true, pos, 4);
    let blue = BlockKind::BlueBed.state_with(&[("part", "head")]).unwrap();
    poi.on_block_change(pos, bed_head(), blue);
    assert_eq!(poi.record_at(pos).unwrap().free_tickets(), 0);
}

#[test]
fn replacing_a_poi_by_another_type_resets_the_record() {
    let mut poi = manager();
    let pos = BlockPos::new(5, 70, 5);
    poi.on_block_change(pos, StateId::AIR, BlockKind::Lectern.default_state());
    poi.on_block_change(
        pos,
        BlockKind::Lectern.default_state(),
        BlockKind::Loom.default_state(),
    );
    assert_eq!(poi.type_at(pos), Some(PoiType::Shepherd));
}

#[test]
fn a_village_without_tickets_is_not_a_village() {
    let mut poi = manager();
    let bell = BlockPos::new(8, 64, 8);
    poi.on_block_change(bell, StateId::AIR, BlockKind::Bell.default_state());
    poi.on_block_change(bell.offset(1, 0, 0), StateId::AIR, bed_head());
    assert!(!poi.record_at(bell).unwrap().is_occupied());
    assert_eq!(poi.sections_to_village(SectionPos::of(bell)), NO_VILLAGE);
}

#[test]
fn an_occupied_village_poi_makes_its_section_level_zero() {
    let mut poi = manager();
    let bell = BlockPos::new(8, 64, 8);
    poi.on_block_change(bell, StateId::AIR, BlockKind::Bell.default_state());
    assert_eq!(
        poi.take(PoiType::is_village, |_, _| true, bell, 4),
        Some(bell)
    );
    let section = SectionPos::of(bell);
    assert_eq!(poi.sections_to_village(section), 0);
    assert_eq!(poi.sections_to_village(section.offset(1, 0, 0)), 1);
    assert_eq!(poi.sections_to_village(section.offset(2, 1, -3)), 3);
    assert_eq!(poi.sections_to_village(section.offset(6, 6, 6)), 6);
    assert_eq!(poi.sections_to_village(section.offset(7, 0, 0)), NO_VILLAGE);
    assert_eq!(
        poi.sections_to_village(section.offset(0, -7, 0)),
        NO_VILLAGE
    );
}

#[test]
fn only_village_types_count_as_village_centers() {
    let mut poi = manager();
    let pos = BlockPos::new(8, 64, 8);
    poi.on_block_change(pos, StateId::AIR, BlockKind::Lectern.default_state());
    poi.on_block_change(
        pos.offset(1, 0, 0),
        StateId::AIR,
        BlockKind::Beehive.default_state(),
    );
    poi.take(|_| true, |_, _| true, pos, 4);
    assert_eq!(poi.sections_to_village(SectionPos::of(pos)), 0);
    let mut other = manager();
    other.on_block_change(pos, StateId::AIR, BlockKind::Lodestone.default_state());
    assert_eq!(other.take(|_| true, |_, _| true, pos, 4), None);
    assert_eq!(other.sections_to_village(SectionPos::of(pos)), NO_VILLAGE);
}

#[test]
fn releasing_the_ticket_ends_the_village_and_removal_recomputes_the_nearest() {
    let mut poi = manager();
    let near = BlockPos::new(8, 64, 8);
    let far = BlockPos::new(8 + 16 * 5, 64, 8);
    for pos in [near, far] {
        poi.on_block_change(pos, StateId::AIR, BlockKind::Bell.default_state());
        poi.take(|_| true, |_, p| p == pos, pos, 1);
    }
    let probe = SectionPos::of(near).offset(2, 0, 0);
    assert_eq!(poi.sections_to_village(probe), 2);
    assert_eq!(poi.release(near), Some(true));
    assert_eq!(poi.sections_to_village(probe), 3);
    assert_eq!(poi.sections_to_village(SectionPos::of(near)), 5);
    poi.on_block_change(far, BlockKind::Bell.default_state(), StateId::AIR);
    assert_eq!(poi.sections_to_village(probe), NO_VILLAGE);
}

#[test]
fn release_of_an_unknown_poi_is_none_and_of_a_free_one_false() {
    let mut poi = manager();
    let pos = BlockPos::new(1, 64, 1);
    assert_eq!(poi.release(pos), None);
    poi.add(pos, PoiType::Home);
    assert_eq!(poi.release(pos), Some(false));
}

#[test]
fn unloading_a_chunk_with_an_occupied_poi_clears_its_village_levels() {
    let mut chunk = column(0, 0);
    chunk.set_block(BlockPos::new(1, 64, 1), BlockKind::Bell.default_state());
    let mut poi = manager();
    poi.load_chunk(&chunk);
    poi.take(|_| true, |_, _| true, BlockPos::new(1, 64, 1), 2);
    assert_eq!(
        poi.sections_to_village(SectionPos::of(BlockPos::new(1, 64, 1))),
        0
    );
    poi.unload_chunk(ChunkPos::new(0, 0));
    assert_eq!(
        poi.sections_to_village(SectionPos::of(BlockPos::new(1, 64, 1))),
        NO_VILLAGE
    );
}

#[test]
fn range_queries_use_the_square_then_the_sphere() {
    let mut poi = manager();
    let center = BlockPos::new(0, 64, 0);
    poi.add(BlockPos::new(3, 64, 4), PoiType::Home);
    poi.add(BlockPos::new(8, 64, 8), PoiType::Meeting);
    poi.add(BlockPos::new(-20, 64, 0), PoiType::Meeting);
    let square = poi.in_square(|_| true, center, 8, Occupancy::Any);
    assert_eq!(square.len(), 2);
    let range = poi.in_range(|_| true, center, 8, Occupancy::Any);
    assert_eq!(range.len(), 1);
    assert_eq!(range[0].pos(), BlockPos::new(3, 64, 4));
    assert_eq!(poi.count_in_range(|_| true, center, 12, Occupancy::Any), 2);
}

#[test]
fn occupancy_filters_follow_the_ticket_counts() {
    let mut poi = manager();
    let bed = BlockPos::new(1, 64, 1);
    let bell = BlockPos::new(2, 64, 2);
    poi.add(bed, PoiType::Home);
    poi.add(bell, PoiType::Meeting);
    poi.add(BlockPos::new(3, 64, 3), PoiType::Beehive);
    let count = |occupancy| poi.count_in_range(|_| true, bed, 10, occupancy);
    assert_eq!(count(Occupancy::Any), 3);
    assert_eq!(count(Occupancy::HasSpace), 2);
    assert_eq!(count(Occupancy::IsOccupied), 0);
    poi.take(|kind| kind == PoiType::Home, |_, _| true, bed, 4);
    let count = |occupancy| poi.count_in_range(|_| true, bed, 10, occupancy);
    assert_eq!(count(Occupancy::HasSpace), 1);
    assert_eq!(count(Occupancy::IsOccupied), 1);
}

#[test]
fn find_helpers_pick_the_first_filtered_and_the_closest() {
    let mut poi = manager();
    let center = BlockPos::new(0, 64, 0);
    poi.add(BlockPos::new(5, 64, 0), PoiType::Home);
    poi.add(BlockPos::new(2, 64, 0), PoiType::Home);
    poi.add(BlockPos::new(0, 64, 7), PoiType::Meeting);
    assert_eq!(
        poi.find_closest(|_| true, center, 10, Occupancy::Any),
        Some(BlockPos::new(2, 64, 0))
    );
    assert_eq!(
        poi.find(|_| true, |pos| pos.z == 7, center, 10, Occupancy::Any),
        Some(BlockPos::new(0, 64, 7))
    );
    assert_eq!(
        poi.find(|_| true, |_| false, center, 10, Occupancy::Any),
        None
    );
}

#[test]
fn results_come_in_chunk_then_section_then_offset_order() {
    let mut poi = manager();
    poi.add(BlockPos::new(20, 64, 0), PoiType::Home);
    poi.add(BlockPos::new(4, 80, 0), PoiType::Home);
    poi.add(BlockPos::new(4, 64, 0), PoiType::Home);
    poi.add(BlockPos::new(2, 64, 17), PoiType::Home);
    let order: Vec<BlockPos> = poi
        .in_square(|_| true, BlockPos::new(8, 64, 8), 32, Occupancy::Any)
        .into_iter()
        .map(|record| record.pos())
        .collect();
    assert_eq!(
        order,
        vec![
            BlockPos::new(4, 64, 0),
            BlockPos::new(4, 80, 0),
            BlockPos::new(20, 64, 0),
            BlockPos::new(2, 64, 17),
        ]
    );
}

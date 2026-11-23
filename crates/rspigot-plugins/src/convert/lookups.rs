use rspigot_core::plugins::{
    FluidMode, LookupAnswer, LookupQuestion, LootRequest, LootTarget, ProfileInfo, ProfileLookup,
    Question, RayHit, RayTraceQuery, TargetHit,
};
use rspigot_core::ports::{TemplateMirror, TemplateRotation};
use rspigot_plugin_protocol::v1::{self, lookup_answer, lookup_question};

use super::Malformed;
use super::player;
use super::shared::{self, required};

fn fluid(value: i32) -> Result<FluidMode, Malformed> {
    match v1::FluidMode::try_from(value) {
        Ok(v1::FluidMode::Never | v1::FluidMode::Unspecified) => Ok(FluidMode::Never),
        Ok(v1::FluidMode::SourceOnly) => Ok(FluidMode::SourceOnly),
        Ok(v1::FluidMode::Always) => Ok(FluidMode::Always),
        Err(_) => Err(Malformed::Enum {
            field: "fluid mode",
            value,
        }),
    }
}

fn rotation(name: &str) -> Result<TemplateRotation, Malformed> {
    match name {
        "" | "none" => Ok(TemplateRotation::None),
        "clockwise_90" => Ok(TemplateRotation::Clockwise90),
        "180" => Ok(TemplateRotation::Clockwise180),
        "counterclockwise_90" => Ok(TemplateRotation::Counterclockwise90),
        _ => Err(Malformed::Enum {
            field: "structure rotation",
            value: 0,
        }),
    }
}

fn mirror(name: &str) -> Result<TemplateMirror, Malformed> {
    match name {
        "" | "none" => Ok(TemplateMirror::None),
        "left_right" => Ok(TemplateMirror::LeftRight),
        "front_back" => Ok(TemplateMirror::FrontBack),
        _ => Err(Malformed::Enum {
            field: "structure mirror",
            value: 0,
        }),
    }
}

pub(super) fn question(lookup: v1::LookupQuestion) -> Result<Question, Malformed> {
    use lookup_question::Query;
    Ok(Question::Lookup(match required(lookup.query)? {
        Query::Structures(_) => LookupQuestion::Structures,
        Query::Structure(id) => LookupQuestion::Structure(id),
        Query::PlaceStructure(place) => LookupQuestion::PlaceStructure {
            rotation: rotation(&place.rotation)?,
            mirror: mirror(&place.mirror)?,
            template: place.template,
            pos: shared::block_pos(required(place.pos)?),
            integrity: place.integrity,
            seed: place.seed,
        },
        Query::LocateStructure(locate) => LookupQuestion::LocateStructure {
            structure: locate.structure,
            from: shared::vec3(required(locate.from)?),
        },
        Query::GenerateLoot(loot) => LookupQuestion::GenerateLoot(LootRequest {
            table: loot.table,
            position: loot.position.map(shared::vec3),
            looter: loot.looter.map(player::id).transpose()?,
            luck: loot.luck,
            seed: loot.seed,
        }),
        Query::GenerateTree(tree) => LookupQuestion::GenerateTree {
            pos: shared::block_pos(required(tree.pos)?),
            feature: tree.feature,
        },
        Query::RayTrace(ray) => LookupQuestion::RayTrace(RayTraceQuery {
            origin: shared::vec3(required(ray.origin)?),
            direction: shared::vec3(required(ray.direction)?),
            max_distance: ray.max_distance,
            blocks: ray.blocks,
            entities: ray.entities,
            fluid: fluid(ray.fluid)?,
            ignore_passable: ray.ignore_passable,
            ray_size: ray.ray_size,
            ignore_entity: ray.ignore_entity.map(rspigot_domain::EntityId),
        }),
        Query::Profile(profile) => LookupQuestion::Profile(match required(profile.by)? {
            v1::profile_query::By::Name(name) => ProfileLookup::Name(name),
            v1::profile_query::By::Uuid(uuid) => {
                ProfileLookup::Id(uuid::Uuid::parse_str(&uuid).map_err(|_| Malformed::Uuid(uuid))?)
            }
        }),
        Query::Lootable(target) => LookupQuestion::Lootable(match required(target.target)? {
            v1::loot_target::Target::Block(pos) => LootTarget::Block(shared::block_pos(pos)),
            v1::loot_target::Target::Entity(entity) => {
                LootTarget::Entity(rspigot_domain::EntityId(entity))
            }
        }),
        Query::Registries(_) => LookupQuestion::Registries,
        Query::LootTables(_) => LookupQuestion::LootTables,
        Query::RegistryEntries(registry) => LookupQuestion::RegistryEntries(registry),
        Query::Tags(registry) => LookupQuestion::Tags(registry),
        Query::TagMembers(query) => LookupQuestion::TagMembers {
            registry: query.registry,
            tag: query.tag,
        },
    }))
}

pub(super) fn wire_answer(answer: LookupAnswer) -> v1::LookupAnswer {
    use lookup_answer::Answer;
    v1::LookupAnswer {
        answer: Some(match answer {
            LookupAnswer::Names(names) => Answer::Names(v1::NameList { names }),
            LookupAnswer::Structure(size) => Answer::Structure(v1::StructureSize {
                x: size.x,
                y: size.y,
                z: size.z,
                blocks: size.blocks,
            }),
            LookupAnswer::Placed(blocks) => Answer::Placed(v1::StructurePlaced { blocks }),
            LookupAnswer::Located(found) => Answer::Located(match found {
                Some((structure, at)) => v1::StructureLocation {
                    structure: Some(structure),
                    position: Some(shared::wire_vec(at)),
                },
                None => v1::StructureLocation::default(),
            }),
            LookupAnswer::Loot(items) => Answer::Loot(v1::LootItems {
                items: items.into_iter().map(shared::wire_item).collect(),
            }),
            LookupAnswer::RayHit(hit) => Answer::RayHit(wire_ray(hit)),
            LookupAnswer::Profile(info) => Answer::Profile(wire_profile(info)),
            LookupAnswer::Lootable { table, seed } => {
                Answer::Lootable(v1::LootTableSet { table, seed })
            }
        }),
    }
}

fn wire_profile(info: ProfileInfo) -> v1::ProfileInfo {
    v1::ProfileInfo {
        name: info.name,
        uuid: info.uuid.to_string(),
        textures: info
            .textures
            .map(|(value, signature)| v1::ProfileTextures { value, signature }),
        online: info.online,
    }
}

fn wire_ray(hit: RayHit) -> v1::RayHit {
    v1::RayHit {
        hit: hit.hit.map(wire_target),
        position: Some(shared::wire_vec(hit.position)),
        distance: hit.distance,
    }
}

fn wire_target(hit: TargetHit) -> v1::Target {
    super::player::wire_target(Some(hit))
}

#[cfg(test)]
mod tests {
    use rspigot_domain::BlockPos;

    use super::*;

    fn asked(query: lookup_question::Query) -> Result<Question, Malformed> {
        question(v1::LookupQuestion { query: Some(query) })
    }

    #[test]
    fn a_placement_converts_its_rotation_mirror_and_position() {
        let place = v1::PlaceStructure {
            template: "minecraft:igloo/top".to_owned(),
            pos: Some(v1::BlockPos { x: 1, y: 2, z: 3 }),
            rotation: "clockwise_90".to_owned(),
            mirror: "left_right".to_owned(),
            integrity: 0.5,
            seed: 9,
        };
        assert_eq!(
            asked(lookup_question::Query::PlaceStructure(place)).unwrap(),
            Question::Lookup(LookupQuestion::PlaceStructure {
                template: "minecraft:igloo/top".to_owned(),
                pos: BlockPos::new(1, 2, 3),
                rotation: TemplateRotation::Clockwise90,
                mirror: TemplateMirror::LeftRight,
                integrity: 0.5,
                seed: 9,
            })
        );
    }

    #[test]
    fn an_unknown_rotation_a_missing_position_and_an_empty_query_are_malformed() {
        let place = |rotation: &str, pos: Option<v1::BlockPos>| v1::PlaceStructure {
            template: "t".to_owned(),
            pos,
            rotation: rotation.to_owned(),
            ..Default::default()
        };
        assert!(
            asked(lookup_question::Query::PlaceStructure(place(
                "sideways",
                Some(v1::BlockPos::default())
            )))
            .is_err()
        );
        assert!(asked(lookup_question::Query::PlaceStructure(place("", None))).is_err());
        assert!(question(v1::LookupQuestion { query: None }).is_err());
    }

    #[test]
    fn a_loot_request_converts_its_looter_and_context() {
        let loot = v1::LootQuery {
            table: "minecraft:chests/x".to_owned(),
            position: Some(v1::Vec3 {
                x: 1.0,
                y: 2.0,
                z: 3.0,
            }),
            looter: Some("00000000-0000-0000-0000-000000000001".to_owned()),
            luck: 2.0,
            seed: Some(5),
        };
        let Question::Lookup(LookupQuestion::GenerateLoot(request)) =
            asked(lookup_question::Query::GenerateLoot(loot)).unwrap()
        else {
            panic!("a loot request");
        };
        assert_eq!(request.seed, Some(5));
        assert!(request.looter.is_some());
        assert!(
            asked(lookup_question::Query::GenerateLoot(v1::LootQuery {
                looter: Some("nobody".to_owned()),
                ..Default::default()
            }))
            .is_err()
        );
    }

    #[test]
    fn answers_go_back_on_the_wire() {
        let located = wire_answer(LookupAnswer::Located(None));
        assert_eq!(
            located.answer,
            Some(lookup_answer::Answer::Located(
                v1::StructureLocation::default()
            ))
        );
        let names = wire_answer(LookupAnswer::Names(vec!["a".to_owned()]));
        assert_eq!(
            names.answer,
            Some(lookup_answer::Answer::Names(v1::NameList {
                names: vec!["a".to_owned()]
            }))
        );
    }
}

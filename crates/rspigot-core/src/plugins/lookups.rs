use std::collections::BTreeSet;
use std::time::Duration;

use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};
use rspigot_domain::player::PlayerId;
use rspigot_domain::{BlockPos, Vec3};

use crate::chunks::{QueryAnswer, WorldQuery};
use crate::commands::Target;
use crate::loot::{self, LootContext};
use crate::plugins::{Answer, ItemSpec, spec_of};
use crate::ports::{ClientLink, PlaceFailure, PlaceRequest, TemplateMirror, TemplateRotation};
use crate::server::Server;

const QUERY_WAIT: Duration = Duration::from_secs(10);

#[derive(Debug, Clone, PartialEq)]
pub struct LootRequest {
    pub table: String,
    pub position: Option<Vec3>,
    pub looter: Option<PlayerId>,
    pub luck: f32,
    pub seed: Option<i64>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum LookupQuestion {
    Structures,
    Structure(String),
    PlaceStructure {
        template: String,
        pos: BlockPos,
        rotation: TemplateRotation,
        mirror: TemplateMirror,
        integrity: f32,
        seed: i32,
    },
    LocateStructure {
        structure: String,
        from: Vec3,
    },
    GenerateLoot(LootRequest),
    Registries,
    LootTables,
    Profile(ProfileLookup),
    GenerateTree {
        pos: BlockPos,
        feature: String,
    },
    RayTrace(crate::plugins::RayTraceQuery),
    Lootable(crate::plugins::LootTarget),
    RegistryEntries(String),
    Tags(String),
    TagMembers {
        registry: String,
        tag: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProfileLookup {
    Name(String),
    Id(uuid::Uuid),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProfileInfo {
    pub name: String,
    pub uuid: uuid::Uuid,
    pub textures: Option<(String, Option<String>)>,
    pub online: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StructureSize {
    pub x: i32,
    pub y: i32,
    pub z: i32,
    pub blocks: i32,
}

#[derive(Debug, Clone, PartialEq)]
pub enum LookupAnswer {
    Names(Vec<String>),
    Structure(StructureSize),
    Placed(i32),
    Located(Option<(String, Vec3)>),
    Loot(Vec<ItemSpec>),
    RayHit(crate::plugins::RayHit),
    Profile(ProfileInfo),
    Lootable { table: Option<String>, seed: i64 },
}

fn namespaced(key: &str) -> String {
    if key.contains(':') {
        key.to_owned()
    } else {
        format!("minecraft:{key}")
    }
}

fn entries_of(registry: &str) -> Option<&'static [&'static str]> {
    rspigot_data::registries::static_entries(registry)
        .or_else(|| rspigot_data::registries::synced_entries(registry))
}

fn tag_names(registry: &str) -> BTreeSet<String> {
    let mut names = BTreeSet::new();
    if let Some(builtin) = rspigot_data::tags::TAGS
        .iter()
        .find(|candidate| candidate.registry == registry)
    {
        names.extend(builtin.tags.iter().map(|(name, _)| (*name).to_owned()));
    }
    if let Some(overlay) = rspigot_data::tags::published() {
        for (candidate, tags) in overlay.registries() {
            if candidate == registry {
                names.extend(tags.keys().cloned());
            }
        }
    }
    names
}

impl<L: ClientLink> Server<L> {
    pub(super) fn lookup_answer(&mut self, question: LookupQuestion) -> Answer {
        match question {
            LookupQuestion::Structures => {
                let answer = self
                    .workers
                    .request_query(
                        &self.world.dimension,
                        Vec3::new(0.0, 0.0, 0.0),
                        WorldQuery::Templates,
                    )
                    .recv_timeout(QUERY_WAIT);
                match answer {
                    Ok(QueryAnswer::Templates(names)) => Answer::Lookup(LookupAnswer::Names(names)),
                    _ => Answer::Refused("the template list did not finish".to_owned()),
                }
            }
            LookupQuestion::GenerateTree { pos, feature } => {
                let feature = feature.trim_start_matches("minecraft:").to_owned();
                let request = self.grow_request(&feature, pos, Vec::new());
                self.place_blocks(request, pos.center())
            }
            LookupQuestion::Profile(lookup) => self.profile_answer(&lookup),
            LookupQuestion::Lootable(target) => self.lootable_answer(target),
            LookupQuestion::RayTrace(query) => self.ray_trace(&query),
            LookupQuestion::LootTables => {
                Answer::Lookup(LookupAnswer::Names(rspigot_data::loot::table_names()))
            }
            LookupQuestion::Structure(id) => self.structure_size(&namespaced(&id)),
            LookupQuestion::PlaceStructure {
                template,
                pos,
                rotation,
                mirror,
                integrity,
                seed,
            } => self.place_structure(PlaceRequest::Template {
                id: namespaced(&template),
                pos,
                rotation,
                mirror,
                integrity,
                seed,
            }),
            LookupQuestion::LocateStructure { structure, from } => {
                let query = WorldQuery::Structures(vec![namespaced(&structure)]);
                let answer = self
                    .workers
                    .request_query(&self.world.dimension, from, query)
                    .recv_timeout(QUERY_WAIT);
                match answer {
                    Ok(QueryAnswer::Located(found)) => Answer::Lookup(LookupAnswer::Located(found)),
                    _ => Answer::Refused("the structure search did not finish".to_owned()),
                }
            }
            LookupQuestion::GenerateLoot(request) => self.generate_plugin_loot(&request),
            LookupQuestion::Registries => {
                let mut names: BTreeSet<String> = rspigot_data::registries::SYNCED
                    .iter()
                    .map(|registry| registry.id.to_owned())
                    .collect();
                names.extend(rspigot_data::registries::static_registry_ids().map(str::to_owned));
                names.extend(
                    rspigot_data::tags::TAGS
                        .iter()
                        .map(|registry| registry.registry.to_owned()),
                );
                Answer::Lookup(LookupAnswer::Names(names.into_iter().collect()))
            }
            LookupQuestion::RegistryEntries(registry) => {
                entries_of(&namespaced(&registry)).map_or(Answer::NotFound, |entries| {
                    Answer::Lookup(LookupAnswer::Names(
                        entries.iter().map(|entry| (*entry).to_owned()).collect(),
                    ))
                })
            }
            LookupQuestion::Tags(registry) => {
                let names = tag_names(&namespaced(&registry));
                if names.is_empty() {
                    Answer::NotFound
                } else {
                    Answer::Lookup(LookupAnswer::Names(names.into_iter().collect()))
                }
            }
            LookupQuestion::TagMembers { registry, tag } => {
                let registry = namespaced(&registry);
                let tag = namespaced(tag.trim_start_matches('#'));
                let (Some(ids), Some(entries)) = (
                    rspigot_data::tags::tag(&registry, &tag),
                    entries_of(&registry),
                ) else {
                    return Answer::NotFound;
                };
                Answer::Lookup(LookupAnswer::Names(
                    ids.iter()
                        .filter_map(|id| entries.get(usize::try_from(*id).ok()?))
                        .map(|entry| (*entry).to_owned())
                        .collect(),
                ))
            }
        }
    }

    fn profile_answer(&self, lookup: &ProfileLookup) -> Answer {
        let online = self.players.values().find(|player| match lookup {
            ProfileLookup::Name(name) => player.profile.name.eq_ignore_ascii_case(name),
            ProfileLookup::Id(id) => player.profile.id.0 == *id,
        });
        if let Some(player) = online {
            let textures = player
                .profile
                .properties
                .iter()
                .find(|property| property.name == "textures")
                .map(|property| (property.value.clone(), property.signature.clone()));
            return Answer::Lookup(LookupAnswer::Profile(ProfileInfo {
                name: player.profile.name.clone(),
                uuid: player.profile.id.0,
                textures,
                online: true,
            }));
        }
        let now = std::time::SystemTime::now();
        let cached = match lookup {
            ProfileLookup::Name(name) => self.config.user_cache.by_name(name, now),
            ProfileLookup::Id(id) => self.config.user_cache.by_uuid(*id, now),
        };
        cached.map_or(Answer::NotFound, |cached| {
            Answer::Lookup(LookupAnswer::Profile(ProfileInfo {
                name: cached.name,
                uuid: cached.uuid,
                textures: None,
                online: false,
            }))
        })
    }

    fn run_placement(
        &self,
        request: PlaceRequest,
        from: Vec3,
    ) -> Result<Vec<crate::ports::PlacedBlock>, Box<Answer>> {
        let answer = self
            .workers
            .request_query(&self.world.dimension, from, WorldQuery::Place(request))
            .recv_timeout(QUERY_WAIT);
        match answer {
            Ok(QueryAnswer::Placed(Ok(blocks))) => Ok(blocks),
            Ok(QueryAnswer::Placed(Err(PlaceFailure::Unknown))) => Err(Box::new(Answer::NotFound)),
            Ok(_) => Err(Box::new(Answer::Refused(
                "the structure cannot be placed".to_owned(),
            ))),
            Err(_) => Err(Box::new(Answer::Refused(
                "the structure placement did not finish".to_owned(),
            ))),
        }
    }

    fn structure_size(&self, id: &str) -> Answer {
        let answer = self
            .workers
            .request_query(
                &self.world.dimension,
                Vec3::new(0.0, 0.0, 0.0),
                WorldQuery::TemplateInfo(id.to_owned()),
            )
            .recv_timeout(QUERY_WAIT);
        match answer {
            Ok(QueryAnswer::TemplateInfo(Some(info))) => {
                Answer::Lookup(LookupAnswer::Structure(StructureSize {
                    x: info.size[0],
                    y: info.size[1],
                    z: info.size[2],
                    blocks: info.blocks,
                }))
            }
            Ok(QueryAnswer::TemplateInfo(None)) => Answer::NotFound,
            _ => Answer::Refused("the template read did not finish".to_owned()),
        }
    }

    fn place_structure(&mut self, request: PlaceRequest) -> Answer {
        let PlaceRequest::Template { pos, .. } = &request else {
            return Answer::Refused("only templates can be placed".to_owned());
        };
        let from = pos.center();
        self.place_blocks(request, from)
    }

    fn place_blocks(&mut self, request: PlaceRequest, from: Vec3) -> Answer {
        let blocks = match self.run_placement(request, from) {
            Ok(blocks) => blocks,
            Err(answer) => return *answer,
        };
        for block in &blocks {
            self.set_block(block.pos, block.state);
        }
        Answer::Lookup(LookupAnswer::Placed(
            i32::try_from(blocks.len()).unwrap_or(i32::MAX),
        ))
    }

    fn generate_plugin_loot(&mut self, request: &LootRequest) -> Answer {
        let name = namespaced(&request.table);
        let Some(table) = rspigot_data::loot::table(&name) else {
            return Answer::NotFound;
        };
        if !request.luck.is_finite() {
            return Answer::Refused("the luck is not finite".to_owned());
        }
        let seed: u64 = match request.seed {
            Some(seed) => seed.cast_unsigned(),
            None => self.rng.random(),
        };
        let looter = match request.looter {
            Some(player) => match self.target_loot_facts(Target::Player(player)) {
                Some(facts) => Some(facts),
                None => return Answer::NotFound,
            },
            None => None,
        };
        let biome = request.position.and_then(|at| {
            let pos = BlockPos::new(
                at.x.floor() as i32,
                at.y.floor() as i32,
                at.z.floor() as i32,
            );
            rspigot_data::registries::biome_name(self.biome_at(pos).0).map(str::to_owned)
        });
        let world = self.loot_world();
        let context = LootContext {
            this: looter.as_ref(),
            player: looter.as_ref(),
            world: Some(&world),
            luck: request.luck,
            biome: biome.as_deref(),
            exploration_maps: request.position.is_some(),
            ..LootContext::default()
        };
        let stacks = loot::table_drops(table, &context, &mut StdRng::seed_from_u64(seed));
        let stacks = match request.position {
            Some(origin) => self.resolve_loot_maps(origin, stacks),
            None => stacks,
        };
        Answer::Lookup(LookupAnswer::Loot(stacks.iter().map(spec_of).collect()))
    }
}

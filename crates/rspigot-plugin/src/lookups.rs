use rspigot_plugin_protocol::v1::{self, answer, lookup_answer, lookup_question, question};
use uuid::Uuid;

use crate::link::Link;
use crate::{BlockPos, Error, ItemStack, Player, Position};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum StructureRotation {
    #[default]
    None,
    Clockwise90,
    Clockwise180,
    Counterclockwise90,
}

impl StructureRotation {
    const fn key(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Clockwise90 => "clockwise_90",
            Self::Clockwise180 => "180",
            Self::Counterclockwise90 => "counterclockwise_90",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum StructureMirror {
    #[default]
    None,
    LeftRight,
    FrontBack,
}

impl StructureMirror {
    const fn key(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::LeftRight => "left_right",
            Self::FrontBack => "front_back",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TreeKind {
    Oak,
    FancyOak,
    Birch,
    Spruce,
    MegaSpruce,
    MegaPine,
    SmallJungle,
    MegaJungle,
    Acacia,
    DarkOak,
    Mangrove,
    TallMangrove,
    Azalea,
    Cherry,
}

impl TreeKind {
    const fn feature(self) -> &'static str {
        match self {
            Self::Oak => "oak",
            Self::FancyOak => "fancy_oak",
            Self::Birch => "birch",
            Self::Spruce => "spruce",
            Self::MegaSpruce => "mega_spruce",
            Self::MegaPine => "mega_pine",
            Self::SmallJungle => "jungle_tree_no_vine",
            Self::MegaJungle => "mega_jungle_tree",
            Self::Acacia => "acacia",
            Self::DarkOak => "dark_oak",
            Self::Mangrove => "mangrove",
            Self::TallMangrove => "tall_mangrove",
            Self::Azalea => "azalea_tree",
            Self::Cherry => "cherry",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PlaceOptions {
    pub rotation: StructureRotation,
    pub mirror: StructureMirror,
    pub integrity: f32,
    pub seed: i32,
}

impl Default for PlaceOptions {
    fn default() -> Self {
        Self {
            rotation: StructureRotation::None,
            mirror: StructureMirror::None,
            integrity: 1.0,
            seed: 0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StructureSize {
    pub x: i32,
    pub y: i32,
    pub z: i32,
    pub blocks: i32,
}

async fn lookup(
    link: &Link,
    query: lookup_question::Query,
) -> Result<lookup_answer::Answer, Error> {
    let answer = link
        .ask(
            question::Question::Lookup(v1::LookupQuestion { query: Some(query) }),
            "a lookup answer",
            |body| match body {
                answer::Body::Lookup(lookup) => Some(lookup),
                _ => None,
            },
        )
        .await?;
    answer
        .answer
        .ok_or_else(|| Error::Malformed("an empty lookup answer".to_owned()))
}

async fn names(link: &Link, query: lookup_question::Query) -> Result<Vec<String>, Error> {
    match lookup(link, query).await? {
        lookup_answer::Answer::Names(list) => Ok(list.names),
        _ => Err(Error::Malformed(
            "an answer that is not a list of names".to_owned(),
        )),
    }
}

#[derive(Debug, Clone)]
pub struct WorldStructuresHandle {
    link: Link,
}

impl WorldStructuresHandle {
    pub(crate) fn new(link: Link) -> Self {
        Self { link }
    }

    pub async fn list(&self) -> Result<Vec<String>, Error> {
        names(&self.link, lookup_question::Query::Structures(v1::Empty {})).await
    }

    pub async fn read(&self, template: &str) -> Result<StructureSize, Error> {
        match lookup(
            &self.link,
            lookup_question::Query::Structure(template.to_owned()),
        )
        .await?
        {
            lookup_answer::Answer::Structure(size) => Ok(StructureSize {
                x: size.x,
                y: size.y,
                z: size.z,
                blocks: size.blocks,
            }),
            _ => Err(Error::Malformed(
                "an answer that is not a structure size".to_owned(),
            )),
        }
    }

    pub async fn generate_tree(&self, at: BlockPos, tree: TreeKind) -> Result<i32, Error> {
        let query = lookup_question::Query::GenerateTree(v1::GenerateTree {
            pos: Some(at.to_wire()),
            feature: tree.feature().to_owned(),
        });
        match lookup(&self.link, query).await? {
            lookup_answer::Answer::Placed(placed) => Ok(placed.blocks),
            _ => Err(Error::Malformed(
                "an answer that is not a placement".to_owned(),
            )),
        }
    }

    pub async fn place(
        &self,
        template: &str,
        at: BlockPos,
        options: PlaceOptions,
    ) -> Result<i32, Error> {
        let query = lookup_question::Query::PlaceStructure(v1::PlaceStructure {
            template: template.to_owned(),
            pos: Some(at.to_wire()),
            rotation: options.rotation.key().to_owned(),
            mirror: options.mirror.key().to_owned(),
            integrity: options.integrity,
            seed: options.seed,
        });
        match lookup(&self.link, query).await? {
            lookup_answer::Answer::Placed(placed) => Ok(placed.blocks),
            _ => Err(Error::Malformed(
                "an answer that is not a placement".to_owned(),
            )),
        }
    }

    pub async fn locate(
        &self,
        structure: &str,
        from: Position,
    ) -> Result<Option<(String, Position)>, Error> {
        let query = lookup_question::Query::LocateStructure(v1::LocateStructure {
            structure: structure.to_owned(),
            from: Some(from.to_wire()),
        });
        match lookup(&self.link, query).await? {
            lookup_answer::Answer::Located(found) => match (found.structure, found.position) {
                (Some(name), position) => {
                    Ok(Some((name, Position::from_wire(position, "position")?)))
                }
                (None, _) => Ok(None),
            },
            _ => Err(Error::Malformed(
                "an answer that is not a location".to_owned(),
            )),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct LootRequest {
    pub table: String,
    pub position: Option<Position>,
    pub looter: Option<Uuid>,
    pub luck: f32,
    pub seed: Option<i64>,
}

impl LootRequest {
    pub fn new(table: impl Into<String>) -> Self {
        Self {
            table: table.into(),
            position: None,
            looter: None,
            luck: 0.0,
            seed: None,
        }
    }

    #[must_use]
    pub const fn at(mut self, position: Position) -> Self {
        self.position = Some(position);
        self
    }

    #[must_use]
    pub const fn looted_by(mut self, player: &Player) -> Self {
        self.looter = Some(player.uuid);
        self
    }

    #[must_use]
    pub const fn luck(mut self, luck: f32) -> Self {
        self.luck = luck;
        self
    }

    #[must_use]
    pub const fn seed(mut self, seed: i64) -> Self {
        self.seed = Some(seed);
        self
    }
}

#[derive(Debug, Clone)]
pub struct ServerLootHandle {
    link: Link,
}

impl ServerLootHandle {
    pub(crate) fn new(link: Link) -> Self {
        Self { link }
    }

    pub async fn tables(&self) -> Result<Vec<String>, Error> {
        names(&self.link, lookup_question::Query::LootTables(v1::Empty {})).await
    }

    pub async fn generate(&self, request: &LootRequest) -> Result<Vec<ItemStack>, Error> {
        let query = lookup_question::Query::GenerateLoot(v1::LootQuery {
            table: request.table.clone(),
            position: request.position.map(Position::to_wire),
            looter: request.looter.map(|uuid| uuid.to_string()),
            luck: request.luck,
            seed: request.seed,
        });
        match lookup(&self.link, query).await? {
            lookup_answer::Answer::Loot(loot) => {
                loot.items.into_iter().map(ItemStack::from_wire).collect()
            }
            _ => Err(Error::Malformed("an answer that is not loot".to_owned())),
        }
    }
}

pub(crate) async fn lootable(
    link: &Link,
    target: v1::loot_target::Target,
) -> Result<Option<(String, i64)>, Error> {
    let query = lookup_question::Query::Lootable(v1::LootTarget {
        target: Some(target),
    });
    match lookup(link, query).await? {
        lookup_answer::Answer::Lootable(set) => Ok(set.table.map(|table| (table, set.seed))),
        _ => Err(Error::Malformed(
            "an answer that is not a loot table".to_owned(),
        )),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlayerProfile {
    pub name: String,
    pub uuid: Uuid,
    pub textures: Option<(String, Option<String>)>,
    pub online: bool,
}

#[derive(Debug, Clone)]
pub struct ServerProfilesHandle {
    link: Link,
}

impl ServerProfilesHandle {
    pub(crate) fn new(link: Link) -> Self {
        Self { link }
    }

    async fn fetch(&self, by: v1::profile_query::By) -> Result<PlayerProfile, Error> {
        let query = lookup_question::Query::Profile(v1::ProfileQuery { by: Some(by) });
        match lookup(&self.link, query).await? {
            lookup_answer::Answer::Profile(info) => Ok(PlayerProfile {
                uuid: Uuid::parse_str(&info.uuid)
                    .map_err(|_| Error::Malformed(format!("a profile uuid `{}`", info.uuid)))?,
                name: info.name,
                textures: info
                    .textures
                    .map(|textures| (textures.value, textures.signature)),
                online: info.online,
            }),
            _ => Err(Error::Malformed(
                "an answer that is not a profile".to_owned(),
            )),
        }
    }

    pub async fn by_name(&self, name: &str) -> Result<PlayerProfile, Error> {
        self.fetch(v1::profile_query::By::Name(name.to_owned()))
            .await
    }

    pub async fn by_id(&self, id: Uuid) -> Result<PlayerProfile, Error> {
        self.fetch(v1::profile_query::By::Uuid(id.to_string()))
            .await
    }
}

#[derive(Debug, Clone)]
pub struct ServerRegistriesHandle {
    link: Link,
}

impl ServerRegistriesHandle {
    pub(crate) fn new(link: Link) -> Self {
        Self { link }
    }

    pub async fn registries(&self) -> Result<Vec<String>, Error> {
        names(&self.link, lookup_question::Query::Registries(v1::Empty {})).await
    }

    pub async fn entries(&self, registry: &str) -> Result<Vec<String>, Error> {
        names(
            &self.link,
            lookup_question::Query::RegistryEntries(registry.to_owned()),
        )
        .await
    }

    pub async fn tags(&self, registry: &str) -> Result<Vec<String>, Error> {
        names(
            &self.link,
            lookup_question::Query::Tags(registry.to_owned()),
        )
        .await
    }

    pub async fn tag_members(&self, registry: &str, tag: &str) -> Result<Vec<String>, Error> {
        names(
            &self.link,
            lookup_question::Query::TagMembers(v1::TagQuery {
                registry: registry.to_owned(),
                tag: tag.to_owned(),
            }),
        )
        .await
    }

    pub async fn is_tagged(&self, registry: &str, tag: &str, key: &str) -> Result<bool, Error> {
        let wanted = if key.contains(':') {
            key.to_owned()
        } else {
            format!("minecraft:{key}")
        };
        Ok(self.tag_members(registry, tag).await?.contains(&wanted))
    }
}

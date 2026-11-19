use rspigot_plugin_protocol::v1::{self, target};

use crate::block::{Block, BlockPos, block_type};
use crate::entity::Entity;
use crate::events::Face;
use crate::link::Link;
use crate::npc::Npc;
use crate::player::Player;
use crate::{BlockType, Error};

#[derive(Debug, Clone)]
pub enum Target {
    Player(Player),
    Entity(Entity),
    Npc(Npc),
}

impl Target {
    pub(crate) fn from_wire(hit: Option<target::Hit>, link: &Link) -> Result<Option<Self>, Error> {
        match hit {
            None => Ok(None),
            Some(target::Hit::Player(player)) => {
                Ok(Some(Self::Player(Player::from_wire(Some(player), link)?)))
            }
            Some(target::Hit::Entity(entity)) => {
                Ok(Some(Self::Entity(Entity::from_wire(entity, link)?)))
            }
            Some(target::Hit::Npc(npc)) => Ok(Some(Self::Npc(Npc::from_wire(npc, link)?))),
            Some(target::Hit::Block(_)) => Err(Error::Malformed(
                "a block where a player or an entity was expected".to_owned(),
            )),
        }
    }
}

#[derive(Debug, Clone)]
pub struct BlockTarget {
    pub block: Block,
    pub face: Face,
    pub kind: BlockType,
}

impl BlockTarget {
    pub(crate) fn from_wire(hit: Option<target::Hit>, link: &Link) -> Result<Option<Self>, Error> {
        match hit {
            None => Ok(None),
            Some(target::Hit::Block(v1::BlockHit { pos, face, block })) => Ok(Some(Self {
                block: Block::new(BlockPos::from_wire(pos, "pos")?, link.clone()),
                face: Face::from_wire(face)?,
                kind: block_type(&block)?,
            })),
            Some(target::Hit::Player(_) | target::Hit::Entity(_) | target::Hit::Npc(_)) => Err(
                Error::Malformed("a player or an entity where a block was expected".to_owned()),
            ),
        }
    }
}

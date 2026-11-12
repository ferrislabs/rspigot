use rspigot_entities::EntityKind;

use crate::brain::AiContext;
use crate::goal::{Flags, Goal};
use crate::registry::Goals;
use crate::world::AiWorld;

#[derive(Debug, Clone, PartialEq)]
pub enum IronGolemGoal {}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct IronGolemFacts;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IronGolemAction {}

impl<W: AiWorld> Goal<AiContext<'_, W>> for IronGolemGoal {
    fn flags(&self) -> Flags {
        match *self {}
    }

    fn can_use(&mut self, _context: &mut AiContext<'_, W>) -> bool {
        match *self {}
    }
}

pub(crate) fn register(_add: &mut Goals<'_>, _kind: EntityKind) {}

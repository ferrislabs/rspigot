use rspigot_domain::EntityId;
use rspigot_domain::attribute;
use rspigot_domain::entity::species::villager::{self, Profession};
use rspigot_domain::entity::species::{self, cat, cube, rabbit, wolf};
use rspigot_domain::entity::{Entity, EntityState};
use rspigot_entities::EntityKind as Kind;

use crate::commands::{CommandContext, Target};
use crate::plugins::{Answer, EntityTrait};
use crate::ports::ClientLink;
use crate::server::Server;

const DYES: u32 = 16;
const RABBIT_VARIANTS: [i32; 7] = [0, 1, 2, 3, 4, 5, 99];

impl<L: ClientLink> Server<L> {
    fn attribute_target(&self, id: EntityId) -> Option<Target> {
        if let Some(player) = self.player_of_entity(id) {
            return Some(Target::Player(player));
        }
        self.entities.store.get(id).map(|_| Target::Entity(id))
    }

    fn attribute_key(&self, target: Target, attribute: &str) -> Result<&'static str, String> {
        let kind = match target {
            Target::Player(_) => Kind::Player,
            Target::Entity(id) => self
                .entities
                .store
                .get(id)
                .map_or(Kind::Player, |entity| entity.base.kind),
        };
        kind.default_attributes()
            .iter()
            .map(|(name, _)| *name)
            .find(|name| *name == attribute)
            .ok_or_else(|| format!("the entity has no attribute {attribute}"))
    }

    pub(super) fn set_attribute_for_plugin(
        &mut self,
        id: EntityId,
        attribute: &str,
        value: f64,
    ) -> Result<(), String> {
        let target = self
            .attribute_target(id)
            .ok_or_else(|| "the entity is gone".to_owned())?;
        if !value.is_finite() {
            return Err(format!("{value} is not a number"));
        }
        let name = self.attribute_key(target, attribute)?;
        CommandContext::set_attribute_base(self, target, name, value);
        Ok(())
    }

    pub(super) fn attribute_value_of(&mut self, id: EntityId, attribute: &str) -> Answer {
        let Some(target) = self.attribute_target(id) else {
            return Answer::NotFound;
        };
        let name = match self.attribute_key(target, attribute) {
            Ok(name) => name,
            Err(reason) => return Answer::Refused(reason),
        };
        CommandContext::attribute_state(self, target, name).map_or(Answer::NotFound, |state| {
            Answer::AttributeValue(attribute::value(&state))
        })
    }

    pub(super) fn change_entity(
        &mut self,
        id: EntityId,
        change: EntityTrait,
    ) -> Result<(), String> {
        let entity = self
            .entities
            .store
            .get_mut(id)
            .ok_or_else(|| "the entity is gone".to_owned())?;
        let required = match change {
            EntityTrait::ZombieBaby(_) => Kind::Zombie,
            EntityTrait::CreeperPowered(_) => Kind::Creeper,
            EntityTrait::SheepColor(_) | EntityTrait::SheepSheared(_) => Kind::Sheep,
            EntityTrait::SlimeSize(_) => Kind::Slime,
            EntityTrait::VillagerProfession(_) => Kind::Villager,
            EntityTrait::WolfCollar(_) => Kind::Wolf,
            EntityTrait::CatCollar(_) => Kind::Cat,
            EntityTrait::RabbitVariant(_) => Kind::Rabbit,
        };
        if entity.base.kind != required {
            return Err(format!("the entity is not a {}", change.species()));
        }
        let Entity { base, state, .. } = entity;
        let EntityState::Living(living) = state else {
            return Err(format!("the entity is not a {}", change.species()));
        };
        match change {
            EntityTrait::ZombieBaby(baby) => {
                species::set_zombie_baby(base, &mut living.attributes, baby);
            }
            EntityTrait::CreeperPowered(powered) => species::set_powered(base, powered),
            EntityTrait::SheepColor(color) => species::set_wool_color(base, dye(color)?),
            EntityTrait::SheepSheared(sheared) => species::set_sheared(base, sheared),
            EntityTrait::SlimeSize(size) => {
                if size < 1 {
                    return Err(format!("{size} is not a size"));
                }
                cube::set_size(base, living, size, true);
            }
            EntityTrait::VillagerProfession(index) => {
                let profession = usize::try_from(index)
                    .ok()
                    .and_then(|index| Profession::ALL.get(index).copied())
                    .ok_or_else(|| format!("{index} is not a profession"))?;
                let (_, _, level) = villager::villager_data(base);
                match living.mob.as_mut().and_then(|mob| mob.merchant.as_mut()) {
                    Some(merchant) => villager::set_profession(base, merchant, profession, level),
                    None => return Err("the villager cannot trade".to_owned()),
                }
            }
            EntityTrait::WolfCollar(color) => wolf::set_collar_color(base, dye(color)?),
            EntityTrait::CatCollar(color) => cat::set_collar_color(base, dye(color)?),
            EntityTrait::RabbitVariant(id) => {
                let id = i32::try_from(id)
                    .ok()
                    .filter(|id| RABBIT_VARIANTS.contains(id))
                    .ok_or_else(|| format!("{id} is not a rabbit variant"))?;
                rabbit::set_variant(base, &mut living.attributes, rabbit::Variant::by_id(id));
            }
        }
        Ok(())
    }
}

fn dye(color: u32) -> Result<u8, String> {
    if color < DYES {
        Ok(color as u8)
    } else {
        Err(format!("the colour {color} is not a dye"))
    }
}

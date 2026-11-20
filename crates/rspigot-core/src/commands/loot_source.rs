use rspigot_domain::item::ItemStack;
use rspigot_domain::player::PlayerId;
use rspigot_domain::text::Component;
use rspigot_domain::{BlockPos, Vec3};

use super::dispatch::{CommandContext, CommandSender, Target};
use super::slots::{OFFHAND, SlotHolder};
use crate::loot::{self, LootContext, LootEntity};
use crate::ports::ClientLink;
use crate::server::Server;

const PICKUP_DELAY: u16 = 10;

#[derive(Debug, Clone, PartialEq)]
pub enum LootTool {
    Mainhand,
    Offhand,
    Stack(ItemStack),
}

#[derive(Debug, Clone, PartialEq)]
pub enum LootSource {
    Table(String),
    Fish { table: String, tool: LootTool },
    Kill(Target),
    Mine { pos: BlockPos, tool: LootTool },
}

fn missing_table(table: &str) -> Component {
    Component::translatable(
        "argument.resource.not_found",
        vec![
            Component::text(table),
            Component::text("minecraft:loot_table"),
        ],
    )
}

fn namespaced(name: &str) -> String {
    if name.contains(':') {
        name.to_owned()
    } else {
        format!("minecraft:{name}")
    }
}

impl<L: ClientLink> Server<L> {
    fn loot_tool(&self, sender: CommandSender, tool: &LootTool) -> Option<ItemStack> {
        let executor = self.command_source(sender).executor;
        match tool {
            LootTool::Stack(stack) => Some(stack.clone()),
            LootTool::Mainhand => executor.and_then(|target| self.main_hand(target)),
            LootTool::Offhand => executor
                .and_then(|target| self.read_command_slot(SlotHolder::Entity(target), OFFHAND))
                .flatten(),
        }
    }

    pub(crate) fn roll_command_loot(
        &mut self,
        sender: CommandSender,
        source: &LootSource,
    ) -> Result<Vec<ItemStack>, Component> {
        match source {
            LootSource::Table(name) => {
                let name = namespaced(name);
                let table = rspigot_data::loot::table(&name).ok_or_else(|| missing_table(&name))?;
                Ok(loot::table_drops(
                    table,
                    &LootContext::default(),
                    &mut self.rng,
                ))
            }
            LootSource::Fish { table, tool } => {
                let name = namespaced(table);
                let found = rspigot_data::loot::table(&name).ok_or_else(|| missing_table(&name))?;
                let tool = self.loot_tool(sender, tool);
                let context = LootContext {
                    tool: tool.as_ref(),
                    ..LootContext::default()
                };
                Ok(loot::table_drops(found, &context, &mut self.rng))
            }
            LootSource::Mine { pos, tool } => {
                let state = self.block_or_air(*pos);
                let kind = state.kind().ok_or_else(|| {
                    Component::translatable(
                        "commands.drop.no_loot_table.block",
                        vec![Component::text("minecraft:air")],
                    )
                })?;
                let Some(table) = rspigot_data::loot::block_table(kind.id().0) else {
                    return Err(Component::translatable(
                        "commands.drop.no_loot_table.block",
                        vec![Component::text(kind.name())],
                    ));
                };
                let tool = self.loot_tool(sender, tool);
                let data = self.block_entity_data(*pos);
                let breaker = LootEntity::of(rspigot_entities::EntityKind::Player);
                let context = LootContext::block(state, tool.as_ref(), Some(&breaker))
                    .with_block_entity(data.as_ref());
                Ok(loot::table_drops(table, &context, &mut self.rng))
            }
            LootSource::Kill(target) => {
                let Target::Entity(id) = *target else {
                    return Err(self.no_entity_table(*target));
                };
                let Some(entity) = self.entities.store.get(id) else {
                    return Err(self.no_entity_table(*target));
                };
                let table = entity
                    .living()
                    .and_then(|living| living.mob.as_ref())
                    .and_then(|mob| mob.death_loot_table.as_deref())
                    .and_then(rspigot_data::loot::table)
                    .or_else(|| rspigot_data::loot::entity_table(entity.base.kind.id().0));
                let Some(table) = table else {
                    return Err(self.no_entity_table(*target));
                };
                let facts = crate::mobs::loot_facts_of(entity);
                let context = LootContext {
                    this: Some(&facts),
                    ..LootContext::default()
                };
                Ok(loot::table_drops(table, &context, &mut self.rng))
            }
        }
    }

    fn no_entity_table(&self, target: Target) -> Component {
        Component::translatable(
            "commands.drop.no_loot_table",
            vec![self.target_name(target)],
        )
    }

    pub(crate) fn give_command_stack(&mut self, player: PlayerId, stack: ItemStack) {
        let Some(state) = self.players.get_mut(&player) else {
            return;
        };
        let at = state.position;
        let mut rest = Some(stack);
        state.inventory.add(&mut rest, false);
        self.send_inventory(player);
        self.broadcast_held_item(player);
        if let Some(rest) = rest {
            self.drop_command_stack(at, rest);
        }
    }

    pub(crate) fn drop_command_stack(&mut self, at: Vec3, stack: ItemStack) {
        self.spawn_item_entity(at, Vec3::ZERO, PICKUP_DELAY, stack);
    }
}

use rspigot_blocks::BlockKind;
use rspigot_domain::chunk::BlockEntity;
use rspigot_domain::entity::Entity;
use rspigot_domain::entity::physics::has_tag;
use rspigot_domain::entity::save::{component_from_tag, component_to_tag};
use rspigot_domain::entity::species::Species;
use rspigot_domain::entity::species::copper_golem::{self, StatuePose, WeatherState};
use rspigot_domain::game_event::GameEvent;
use rspigot_domain::item::ItemStack;
use rspigot_domain::player::PlayerId;
use rspigot_domain::text::Component;
use rspigot_domain::{BlockPos, BlockStateId, EntityId, Identifier, Vec3};
use rspigot_entities::{EntityKind as Kind, SoundSource};
use rspigot_items::ItemKind;
use rspigot_nbt::Compound;

use crate::chests;
use crate::events::{Direction, Hand};
use crate::game_events::GameEventContext;
use crate::interact::item_tag;
use crate::mob_actions::new_mob;
use crate::plugins::events::CreatureSpawnReason;
use crate::plugins::steps::{Birth, Grief};
use crate::ports::ClientLink;
use crate::server::Server;

const STATUE_ENTITY: &str = "copper_golem_statue";
const COMPONENTS: &str = "components";
const CUSTOM_NAME: &str = "minecraft:custom_name";
const POSE: &str = "copper_golem_pose";
const FACING: &str = "facing";
const BECOME_STATUE_SOUND: &str = "minecraft:entity.copper_golem_become_statue";
const COPPER_TAG: &str = "minecraft:copper";
const STATUE_TAG: &str = "minecraft:copper_golem_statues";
const BLOCK_BREAK_EVENT: i32 = 2001;
const BUILD_HEIGHT_OFFSET: f64 = 0.05;
const HORIZONTAL_FACINGS: [&str; 4] = ["south", "west", "north", "east"];
const FACING_YAWS: [f32; 4] = [0.0, 90.0, 180.0, 270.0];

pub(crate) fn is_statue(state: BlockStateId) -> bool {
    has_tag(state, STATUE_TAG)
}

fn is_waxed_name(name: &str) -> bool {
    name.strip_prefix("minecraft:")
        .unwrap_or(name)
        .starts_with("waxed_")
}

pub(crate) fn statue_age(kind: BlockKind) -> WeatherState {
    let name = kind.name();
    let path = name.strip_prefix("minecraft:").unwrap_or(name);
    let path = path.strip_prefix("waxed_").unwrap_or(path);
    if path.starts_with("exposed_") {
        WeatherState::Exposed
    } else if path.starts_with("weathered_") {
        WeatherState::Weathered
    } else if path.starts_with("oxidized_") {
        WeatherState::Oxidized
    } else {
        WeatherState::Unaffected
    }
}

pub(crate) fn copper_block_age(kind: BlockKind) -> WeatherState {
    let name = kind.name();
    let path = name.strip_prefix("minecraft:").unwrap_or(name);
    let path = path.strip_prefix("waxed_").unwrap_or(path);
    match path {
        "exposed_copper" => WeatherState::Exposed,
        "weathered_copper" => WeatherState::Weathered,
        "oxidized_copper" => WeatherState::Oxidized,
        _ => WeatherState::Unaffected,
    }
}

fn chest_of_copper_block(kind: BlockKind) -> Option<BlockKind> {
    let age = copper_block_age(kind);
    let waxed = if is_waxed_name(kind.name()) {
        "waxed_"
    } else {
        ""
    };
    let stage = match age {
        WeatherState::Unaffected => "",
        WeatherState::Exposed => "exposed_",
        WeatherState::Weathered => "weathered_",
        WeatherState::Oxidized => "oxidized_",
    };
    BlockKind::from_name(&format!("minecraft:{waxed}{stage}copper_chest"))
}

pub(crate) fn facing_of_yaw(yaw: f32) -> &'static str {
    let index = ((f64::from(yaw) / 90.0 + 0.5).floor() as i32) & 3;
    HORIZONTAL_FACINGS[index as usize]
}

fn yaw_of_facing(facing: &str) -> f32 {
    HORIZONTAL_FACINGS
        .iter()
        .position(|name| *name == facing)
        .map_or(0.0, |index| FACING_YAWS[index])
}

pub(crate) fn statue_state(pose: StatuePose, yaw: f32) -> BlockStateId {
    let state = BlockKind::OxidizedCopperGolemStatue.default_state();
    let posed = state.with_property(POSE, pose.name()).unwrap_or(state);
    posed
        .with_property(FACING, facing_of_yaw(yaw))
        .unwrap_or(posed)
}

pub(crate) fn pose_of(state: BlockStateId) -> StatuePose {
    state
        .property(POSE)
        .and_then(StatuePose::from_name)
        .unwrap_or_default()
}

fn statue_name(data: &Compound) -> Option<Component> {
    data.get_compound(COMPONENTS)
        .and_then(|components| components.get(CUSTOM_NAME))
        .and_then(component_from_tag)
}

fn is_pumpkin(state: BlockStateId) -> bool {
    matches!(
        state.kind(),
        Some(BlockKind::CarvedPumpkin | BlockKind::JackOLantern)
    )
}

impl<L: ClientLink> Server<L> {
    pub(crate) fn ensure_statue_entity(&mut self, pos: BlockPos, state: BlockStateId) {
        if !is_statue(state) {
            return;
        }
        let present = self
            .world
            .chunks
            .get(pos.chunk())
            .is_some_and(|chunk| chunk.block_entity(pos).is_some());
        if present {
            return;
        }
        if let Some(chunk) = self.world.chunks.chunk_mut(pos.chunk()) {
            chunk.set_block_entity(BlockEntity {
                pos,
                kind: Identifier::minecraft(STATUE_ENTITY),
                data: Compound::new(),
            });
        }
    }

    pub(crate) fn turn_to_statue(&mut self, id: EntityId) {
        let Some((pos, yaw)) = self
            .entities
            .store
            .get(id)
            .map(|entity| (entity.base.position.block(), entity.base.yaw))
        else {
            return;
        };
        let pose = StatuePose::random(&mut crate::mobs::ServerRandom(&mut self.rng));
        let state = statue_state(pose, yaw);
        let Some(view) = self.entity_view(id) else {
            return;
        };
        self.change_block(
            view,
            pos,
            state,
            Grief::Statue {
                golem: id,
                pos,
                state,
            },
            false,
        );
    }

    pub(crate) fn finish_golem_statue(
        &mut self,
        golem: EntityId,
        pos: BlockPos,
        state: BlockStateId,
    ) {
        let Some(entity) = self.entities.store.get(golem) else {
            return;
        };
        let name = entity.base.custom_name().cloned();
        self.set_block(pos, state);
        self.ensure_statue_entity(pos, state);
        let Some(chunk) = self.world.chunks.chunk_mut(pos.chunk()) else {
            return;
        };
        let Some(existing) = chunk.block_entity(pos).cloned() else {
            return;
        };
        let mut entity = existing;
        if let Some(name) = name {
            let mut components = Compound::new();
            components.insert(CUSTOM_NAME, component_to_tag(&name));
            entity.data.insert(COMPONENTS, components);
        } else {
            entity.data.remove(COMPONENTS);
        }
        chunk.set_block_entity(entity);
        self.drop_preserved_equipment(golem);
        self.play_become_statue_sound(golem);
        self.remove_entity(
            golem,
            rspigot_domain::entity::RemovalReason::Discarded,
            Some(crate::plugins::events::EntityRemoveCause::Despawn),
        );
    }

    fn update_statue_pose(&mut self, player: PlayerId, pos: BlockPos, state: BlockStateId) {
        self.play_sound(
            None,
            pos.center(),
            BECOME_STATUE_SOUND,
            SoundSource::Blocks,
            1.0,
            1.0,
        );
        let next = pose_of(state).next();
        if let Some(changed) = state.with_property(POSE, next.name()) {
            self.set_block(pos, changed);
        }
        let source = self.players.get(&player).map(|player| player.entity_id);
        self.block_game_event(GameEvent::BlockChange, pos, GameEventContext::of(source));
    }

    pub(crate) fn use_golem_statue(
        &mut self,
        player: PlayerId,
        hand: Hand,
        held: Option<&ItemStack>,
        pos: BlockPos,
        clicked: BlockStateId,
    ) -> bool {
        let Some(kind) = clicked.kind().filter(|_| is_statue(clicked)) else {
            return false;
        };
        let axe = held.is_some_and(|stack| item_tag(stack.item, "minecraft:axes"));
        if is_waxed_name(kind.name()) {
            if axe {
                return false;
            }
            self.update_statue_pose(player, pos, clicked);
            return true;
        }
        let has_entity = self
            .world
            .chunks
            .get(pos.chunk())
            .is_some_and(|chunk| chunk.block_entity(pos).is_some());
        if !has_entity {
            return false;
        }
        if axe {
            if statue_age(kind) != WeatherState::Unaffected {
                return false;
            }
            if self.reanimate_statue(pos, clicked) {
                self.hurt_held(player, hand, 1);
                self.set_block(pos, BlockStateId::AIR);
                return true;
            }
            return false;
        }
        if held.is_some_and(|stack| stack.item == ItemKind::Honeycomb.id()) {
            return false;
        }
        self.update_statue_pose(player, pos, clicked);
        true
    }

    fn reanimate_statue(&mut self, pos: BlockPos, state: BlockStateId) -> bool {
        let name = self
            .world
            .chunks
            .get(pos.chunk())
            .and_then(|chunk| chunk.block_entity(pos))
            .and_then(|entity| statue_name(&entity.data));
        let yaw = yaw_of_facing(state.property(FACING).unwrap_or("north"));
        let mut entity = new_mob(self.new_entity_base(Kind::CopperGolem));
        let id = entity.base.id;
        entity.base.snap_to(
            Vec3::new(
                f64::from(pos.x) + 0.5,
                f64::from(pos.y),
                f64::from(pos.z) + 0.5,
            ),
            yaw,
            0.0,
        );
        entity.base.head_yaw = yaw;
        if let Some(living) = entity.living_mut() {
            living.body_yaw = yaw;
        }
        entity.base.set_custom_name(name);
        mark_spawn_sound(&mut entity);
        self.spawn_entity(entity, Birth::of(CreatureSpawnReason::Reanimate));
        self.spawning_now(id)
    }

    pub(crate) fn try_spawn_copper_golem(
        &mut self,
        pos: BlockPos,
        old: BlockStateId,
        new: BlockStateId,
    ) {
        if !is_pumpkin(new) || old.kind() == new.kind() {
            return;
        }
        let below = pos.offset(0, -1, 0);
        let Some(base) = self
            .world
            .chunks
            .block(below)
            .filter(|state| has_tag(*state, COPPER_TAG))
        else {
            return;
        };
        let Some(base_kind) = base.kind() else {
            return;
        };
        let weather = copper_block_age(base_kind);
        let facing = new.property(FACING).unwrap_or("north");
        let mut golem = new_mob(self.new_entity_base(Kind::CopperGolem));
        let id = golem.base.id;
        golem.base.snap_to(
            Vec3::new(
                f64::from(pos.x) + 0.5,
                f64::from(pos.y) + BUILD_HEIGHT_OFFSET,
                f64::from(pos.z) + 0.5,
            ),
            0.0,
            0.0,
        );
        copper_golem::set_weather_state(&mut golem.base, weather);
        mark_spawn_sound(&mut golem);
        self.spawn_entity(golem, Birth::of(CreatureSpawnReason::BuildCoppergolem));
        if !self.spawning_now(id) {
            return;
        }
        for cleared in [pos, below] {
            let state = self
                .world
                .chunks
                .block(cleared)
                .unwrap_or(BlockStateId::AIR);
            self.set_block(cleared, BlockStateId::AIR);
            self.level_event(BLOCK_BREAK_EVENT, cleared, i32::from(state.0));
        }
        if let Some(chest) = chest_of_copper_block(base_kind) {
            let faced = chest
                .default_state()
                .with_property(FACING, facing)
                .unwrap_or_else(|| chest.default_state());
            let chunks = &self.world.chunks;
            let state =
                chests::placed_state(faced, below, Direction::Up, false, |at| chunks.block(at));
            self.set_block(below, state);
        }
    }
}

fn mark_spawn_sound(entity: &mut Entity) {
    if let Some(Species::CopperGolem(state)) = entity
        .living_mut()
        .and_then(|living| living.mob.as_mut())
        .map(|mob| &mut mob.species)
    {
        state.spawn_sound_pending = true;
    }
}

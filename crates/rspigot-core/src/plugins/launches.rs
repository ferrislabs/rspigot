use rand::Rng;
use rspigot_blocks::Face;
use rspigot_domain::entity::firework::{self, FireworkState};
use rspigot_domain::entity::hanging;
use rspigot_domain::entity::hurting;
use rspigot_domain::entity::save::{is_simulated, item_stack_to_nbt, uuid_tag};
use rspigot_domain::entity::thrown;
use rspigot_domain::entity::{Entity, EntityState};
use rspigot_domain::random::LegacyRandom;
use rspigot_domain::spawn::SpawnReason;
use rspigot_domain::{BlockPos, EntityId, Vec3};
use rspigot_entities::EntityKind as Kind;
use rspigot_nbt::{Compound, Tag};
use uuid::Uuid;

use crate::context::SUMMON_FAILED;
use crate::eye_of_ender::{START_SPEED, signal_target};
use crate::plugins::displays::{DisplaySpec, apply_display};
use crate::plugins::entity::refusal;
use crate::plugins::events::CreatureSpawnReason;
use crate::plugins::items::{spec_of, stack_of};
use crate::plugins::resolve;
use crate::plugins::steps::{Birth, Spawned};
use crate::plugins::{ActionError, ActionErrorKind, ItemSpec};
use crate::ports::ClientLink;
use crate::server::Server;
use rspigot_domain::mth;

const MAX_LAUNCH_SPEED: f64 = 100.0;

pub(crate) struct FireworkRequest<'a> {
    pub(crate) position: Vec3,
    pub(crate) item: &'a ItemSpec,
    pub(crate) shooter: Option<EntityId>,
    pub(crate) attached: Option<EntityId>,
    pub(crate) angled: bool,
    pub(crate) velocity: Option<Vec3>,
}

pub(crate) struct HangingRequest<'a> {
    pub(crate) kind: &'a str,
    pub(crate) pos: BlockPos,
    pub(crate) face: &'a str,
    pub(crate) variant: Option<&'a str>,
    pub(crate) item: Option<&'a ItemSpec>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct HangingView {
    pub kind: String,
    pub pos: BlockPos,
    pub facing: String,
    pub item: Option<ItemSpec>,
    pub rotation: i32,
    pub visible: bool,
    pub variant: Option<String>,
}

fn face_name(face: Face) -> &'static str {
    match face {
        Face::North => "north",
        Face::South => "south",
        Face::East => "east",
        Face::West => "west",
        Face::Up => "up",
        Face::Down => "down",
    }
}

fn invalid(reason: impl Into<String>) -> ActionError {
    ActionError::new(ActionErrorKind::InvalidArgument, reason)
}

fn incompatible(reason: impl Into<String>) -> ActionError {
    ActionError::new(ActionErrorKind::IncompatibleState, reason)
}

fn gone() -> ActionError {
    ActionError::new(ActionErrorKind::TargetGone, "the entity is gone")
}

fn kind_of(key: &str) -> Result<Kind, ActionError> {
    Kind::from_name(key).ok_or_else(|| invalid(format!("unknown entity type {key}")))
}

fn clamped(velocity: Vec3) -> Result<Vec3, ActionError> {
    let velocity = resolve::velocity_of(velocity).map_err(invalid)?;
    let clamp = |axis: f64| axis.clamp(-MAX_LAUNCH_SPEED, MAX_LAUNCH_SPEED);
    Ok(Vec3::new(
        clamp(velocity.x),
        clamp(velocity.y),
        clamp(velocity.z),
    ))
}

fn is_projectile(kind: Kind) -> bool {
    (matches!(kind, Kind::Arrow | Kind::Trident)
        || thrown::is_thrown(kind)
        || hurting::is_hurting_projectile(kind))
        && (is_simulated(kind) || matches!(kind, Kind::EnderPearl | Kind::EyeOfEnder))
}

fn is_vehicle(kind: Kind) -> bool {
    kind.extends("AbstractBoat") || kind.extends("AbstractMinecart")
}

fn face_of(name: &str) -> Result<Face, ActionError> {
    match name {
        "north" => Ok(Face::North),
        "south" => Ok(Face::South),
        "east" => Ok(Face::East),
        "west" => Ok(Face::West),
        "up" => Ok(Face::Up),
        "down" => Ok(Face::Down),
        other => Err(invalid(format!("{other} is not a face"))),
    }
}

fn block_pos_tag(pos: BlockPos) -> Tag {
    Tag::IntArray(vec![pos.x, pos.y, pos.z])
}

impl<L: ClientLink> Server<L> {
    pub(crate) fn loaded_position_for(&self, position: Vec3) -> Result<Vec3, ActionError> {
        let position = resolve::position_of(position).map_err(invalid)?;
        if self.world.chunks.is_loaded(position.chunk()) {
            Ok(position)
        } else {
            Err(incompatible(refusal(SUMMON_FAILED)))
        }
    }

    fn uuid_of_entity(&self, id: EntityId) -> Result<Uuid, ActionError> {
        if let Some(player) = self.players.values().find(|player| player.entity_id == id) {
            return Ok(player.profile.id.0);
        }
        self.entities
            .store
            .get(id)
            .map(|entity| entity.base.uuid)
            .ok_or_else(gone)
    }

    fn summoned(
        &mut self,
        kind: Kind,
        position: Vec3,
        nbt: Option<Compound>,
    ) -> Result<(Entity, LegacyRandom), ActionError> {
        self.summon_entity(kind, position, nbt, SpawnReason::Command)
            .map_err(|key| incompatible(refusal(key)))
    }

    pub(crate) fn answer_with_spawn(
        &mut self,
        connection: u64,
        request: u64,
        entity: Entity,
        random: LegacyRandom,
    ) {
        let birth = Birth::of(CreatureSpawnReason::Custom)
            .then(Spawned::Answered {
                connection,
                request,
            })
            .with_random(random);
        self.spawn_entity(entity, birth);
    }

    pub(crate) fn launch_projectile(
        &mut self,
        connection: u64,
        request: u64,
        kind: &str,
        position: Vec3,
        velocity: Vec3,
        shooter: Option<EntityId>,
    ) -> Result<(), ActionError> {
        let kind = kind_of(kind)?;
        if !is_projectile(kind) {
            return Err(invalid(format!("{} is not a projectile", kind.name())));
        }
        let velocity = clamped(velocity)?;
        let position = self.loaded_position_for(position)?;
        if matches!(kind, Kind::EnderPearl | Kind::EyeOfEnder) {
            return self.launch_ender(connection, request, kind, position, velocity, shooter);
        }
        let mut nbt = Compound::new().with("HasBeenShot", true);
        if let Some(shooter) = shooter {
            nbt.insert("Owner", uuid_tag(self.uuid_of_entity(shooter)?));
        }
        let (mut entity, random) = self.summoned(kind, position, Some(nbt))?;
        match &entity.state {
            EntityState::Hurting(state) if velocity != Vec3::ZERO => {
                let power = state.acceleration_power;
                hurting::assign_directional_movement(&mut entity.base, velocity, power);
            }
            _ => {
                entity.base.velocity = velocity;
                entity.base.needs_sync = true;
            }
        }
        self.answer_with_spawn(connection, request, entity, random);
        Ok(())
    }

    pub(crate) fn launch_firework(
        &mut self,
        connection: u64,
        request: u64,
        launch: FireworkRequest<'_>,
    ) -> Result<(), ActionError> {
        let FireworkRequest {
            position,
            item,
            shooter,
            attached,
            angled,
            velocity,
        } = launch;
        let stack = stack_of(item)
            .map_err(invalid)?
            .filter(|stack| stack.item == rspigot_items::ItemKind::FireworkRocket.id())
            .ok_or_else(|| invalid("a firework needs a firework rocket item"))?;
        let velocity = velocity.map(clamped).transpose()?;
        if angled && velocity.is_none() {
            return Err(invalid("a firework shot at an angle needs a velocity"));
        }
        let position = self.loaded_position_for(position)?;
        let owner = shooter.map(|id| self.uuid_of_entity(id)).transpose()?;
        if let Some(target) = attached {
            self.uuid_of_entity(target)?;
        }
        let mut base = self.new_entity_base(Kind::FireworkRocket);
        base.position = position;
        base.old_position = position;
        let seed = self.rng.random::<i64>();
        let single = rspigot_domain::item::ItemStack {
            count: std::num::NonZeroU8::MIN,
            ..stack
        };
        let mut state = FireworkState::new(&mut base, seed, &single);
        state.owner = owner;
        if let Some(velocity) = velocity {
            base.velocity = velocity;
        }
        firework::set_shot_at_angle(&mut base, angled);
        firework::set_attached(&mut base, &mut state, attached);
        let random = state.random.clone();
        let rocket = Entity::new(base, EntityState::Firework(Box::new(state)));
        self.answer_with_spawn(connection, request, rocket, random);
        Ok(())
    }

    fn launch_ender(
        &mut self,
        connection: u64,
        request: u64,
        kind: Kind,
        position: Vec3,
        velocity: Vec3,
        shooter: Option<EntityId>,
    ) -> Result<(), ActionError> {
        let owner = shooter.map(|id| self.uuid_of_entity(id)).transpose()?;
        let owner_is_player = owner.is_some_and(|uuid| {
            self.players
                .values()
                .any(|player| player.profile.id.0 == uuid)
        });
        if kind == Kind::EnderPearl && !owner_is_player {
            return Err(invalid("an ender pearl needs a player as its shooter"));
        }
        let mut base = self.new_entity_base(kind);
        base.position = position;
        base.old_position = position;
        let seed = self.rng.random::<i64>();
        let mut state = thrown::ThrownState::new(&mut base, seed);
        thrown::set_item(&mut base, &thrown::default_item_of(kind));
        state.owner = owner;
        state.has_been_shot = true;
        if kind == Kind::EyeOfEnder {
            let aim = signal_target(position, position + velocity);
            let angle = mth::atan2(aim.z - position.z, aim.x - position.x);
            base.velocity = Vec3::new(angle.cos() * START_SPEED, 0.0, angle.sin() * START_SPEED);
            state.target = Some(aim);
        } else {
            base.velocity = velocity;
        }
        base.needs_sync = true;
        let random = state.random.clone();
        let entity = Entity::new(base, EntityState::Thrown(Box::new(state)));
        self.answer_with_spawn(connection, request, entity, random);
        Ok(())
    }

    pub(crate) fn detonate_firework(&mut self, id: EntityId) -> Result<(), ActionError> {
        let entity = self.entities.store.get_mut(id).ok_or_else(gone)?;
        match &mut entity.state {
            EntityState::Firework(state) => {
                state.life = state.lifetime;
                Ok(())
            }
            _ => Err(invalid("the entity is not a firework rocket")),
        }
    }

    pub(crate) fn spawn_vehicle(
        &mut self,
        connection: u64,
        request: u64,
        kind: &str,
        position: Vec3,
        velocity: Option<Vec3>,
    ) -> Result<(), ActionError> {
        let kind = kind_of(kind)?;
        if !is_vehicle(kind) {
            return Err(invalid(format!(
                "{} is not a boat or a minecart",
                kind.name()
            )));
        }
        let velocity = velocity.map(clamped).transpose()?;
        let position = self.loaded_position_for(position)?;
        let (mut entity, random) = self.summoned(kind, position, None)?;
        if let Some(velocity) = velocity {
            entity.base.velocity = velocity;
            entity.base.needs_sync = true;
        }
        self.answer_with_spawn(connection, request, entity, random);
        Ok(())
    }

    pub(crate) fn set_vehicle_velocity(
        &mut self,
        id: EntityId,
        velocity: Vec3,
    ) -> Result<(), ActionError> {
        let velocity = clamped(velocity)?;
        let entity = self.entities.store.get_mut(id).ok_or_else(gone)?;
        if !matches!(
            entity.state,
            EntityState::Boat(_) | EntityState::Minecart(_)
        ) {
            return Err(invalid("the entity is not a boat or a minecart"));
        }
        entity.base.velocity = velocity;
        entity.base.needs_sync = true;
        Ok(())
    }

    pub(crate) fn spawn_display_for_plugin(
        &mut self,
        connection: u64,
        request: u64,
        position: Vec3,
        spec: &DisplaySpec,
    ) -> Result<(), ActionError> {
        let position = self.loaded_position_for(position)?;
        let (mut entity, random) = self.summoned(spec.kind(), position, None)?;
        apply_display(&mut entity.base, spec)?;
        self.answer_with_spawn(connection, request, entity, random);
        Ok(())
    }

    pub(crate) fn place_hanging(
        &mut self,
        connection: u64,
        request: u64,
        placing: HangingRequest<'_>,
    ) -> Result<(), ActionError> {
        let HangingRequest {
            kind,
            pos,
            face,
            variant,
            item,
        } = placing;
        let kind = kind_of(kind)?;
        if !hanging::is_hanging(kind) {
            return Err(invalid(format!(
                "{} is not an item frame or a painting",
                kind.name()
            )));
        }
        let facing = face_of(face)?;
        let height = self.world.dimension.height;
        if pos.y < height.min_y || pos.y > height.max_y() {
            return Err(invalid(format!("y {} is outside the world", pos.y)));
        }
        let mut nbt = Compound::new().with("block_pos", block_pos_tag(pos));
        if hanging::is_frame(kind) {
            nbt.insert("Facing", hanging::data_3d(facing) as i8);
            if let Some(spec) = item
                && let Some(stack) = stack_of(spec).map_err(invalid)?
            {
                nbt.insert("Item", item_stack_to_nbt(&stack));
            }
        } else {
            if !hanging::is_horizontal(facing) {
                return Err(invalid(
                    "a painting hangs on a wall, not a floor or a ceiling",
                ));
            }
            if item.is_some() {
                return Err(invalid("a painting holds no item"));
            }
            nbt.insert("facing", hanging::data_2d(facing) as i8);
            if let Some(name) = variant {
                let bare = name.strip_prefix("minecraft:").unwrap_or(name);
                if hanging::variant_by_name(bare).is_none() {
                    return Err(invalid(format!("unknown painting {name}")));
                }
                nbt.insert("variant", format!("minecraft:{bare}"));
            }
        }
        let (mut entity, random) = self.summoned(kind, pos.center(), Some(nbt))?;
        let neighbours = self.hanging_neighbours();
        let chunks = &self.world.chunks;
        let Entity { base, state, .. } = &mut entity;
        let EntityState::Hanging(hung) = state else {
            return Err(incompatible("the entity is not a hanging entity"));
        };
        hanging::place(base, hung, pos, facing);
        if kind == Kind::Painting && variant.is_none() {
            let mut best: Option<(i32, u32)> = None;
            for candidate in hanging::placeable_variants() {
                hanging::set_variant(base, candidate);
                hanging::place(base, hung, pos, facing);
                let area = hanging::variant_area(candidate);
                if hanging::survives(chunks, base, hung, &neighbours)
                    && best.is_none_or(|(largest, _)| area > largest)
                {
                    best = Some((area, candidate));
                }
            }
            let (_, chosen) = best.ok_or_else(|| incompatible("no painting fits there"))?;
            hanging::set_variant(base, chosen);
            hanging::place(base, hung, pos, facing);
        }
        if !hanging::survives(chunks, base, hung, &neighbours) {
            return Err(incompatible("there is no free wall to hang it on"));
        }
        self.answer_with_spawn(connection, request, entity, random);
        Ok(())
    }

    pub(crate) fn set_frame_item(
        &mut self,
        id: EntityId,
        item: Option<&ItemSpec>,
    ) -> Result<(), ActionError> {
        let stack = item.map(stack_of).transpose().map_err(invalid)?.flatten();
        let cell = self.with_frame(id, |base, state| {
            hanging::set_item(base, state, stack);
            state.pos
        })?;
        self.update_neighbour_for_output_signal(cell);
        Ok(())
    }

    pub(crate) fn set_frame_rotation(
        &mut self,
        id: EntityId,
        rotation: i32,
    ) -> Result<(), ActionError> {
        let cell = self.with_frame(id, |base, state| {
            hanging::set_rotation(base, rotation);
            state.pos
        })?;
        self.update_neighbour_for_output_signal(cell);
        Ok(())
    }

    pub(crate) fn set_frame_visible(
        &mut self,
        id: EntityId,
        visible: bool,
    ) -> Result<(), ActionError> {
        self.with_frame(id, |base, _| hanging::set_invisible(base, !visible))?;
        Ok(())
    }

    pub(crate) fn hanging_view(&self, id: EntityId) -> Result<HangingView, ActionError> {
        let entity = self.entities.store.get(id).ok_or_else(gone)?;
        let EntityState::Hanging(state) = &entity.state else {
            return Err(invalid("the entity is not an item frame or a painting"));
        };
        let base = &entity.base;
        let frame = hanging::is_frame(base.kind);
        Ok(HangingView {
            kind: base.kind.name().to_owned(),
            pos: state.pos,
            facing: face_name(hanging::facing(base)).to_owned(),
            item: hanging::item(base).map(spec_of),
            rotation: if frame { hanging::rotation(base) } else { 0 },
            visible: !hanging::is_invisible(base),
            variant: (!frame)
                .then(|| hanging::variant_name(hanging::variant(base)))
                .flatten()
                .map(|name| format!("minecraft:{name}")),
        })
    }

    fn with_frame<T>(
        &mut self,
        id: EntityId,
        change: impl FnOnce(&mut rspigot_domain::entity::EntityBase, &mut hanging::HangingState) -> T,
    ) -> Result<T, ActionError> {
        let entity = self.entities.store.get_mut(id).ok_or_else(gone)?;
        let Entity { base, state, .. } = entity;
        match state {
            EntityState::Hanging(state) if hanging::is_frame(base.kind) => Ok(change(base, state)),
            _ => Err(invalid("the entity is not an item frame")),
        }
    }
}

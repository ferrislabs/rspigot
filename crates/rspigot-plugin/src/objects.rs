use std::sync::Arc;

use rspigot_plugin_protocol::v1::{
    self, answer, entity_order, from_plugin, map_decoration_edit, map_edit, object_answer,
    object_entity_order, object_player_order, object_question, object_world_order, player_order,
    question, world_order,
};

use crate::displays::DisplaySpec;
use crate::link::Link;
use crate::pdc::PdcHandle;
use crate::{BlockPos, Entity, EntityType, Error, ItemStack, Position, Trade, Vector};

fn entity_object(entity: i32, link: &Link, order: object_entity_order::Order) {
    link.order(from_plugin::Body::Entity(v1::EntityOrder {
        entity,
        order: Some(entity_order::Order::Object(v1::ObjectEntityOrder {
            order: Some(order),
        })),
    }));
}

#[derive(Debug, Clone)]
pub struct MerchantHandle {
    entity: i32,
    link: Link,
}

impl MerchantHandle {
    pub(crate) fn new(entity: i32, link: Link) -> Self {
        Self { entity, link }
    }

    pub fn set_trades(&self, trades: &[Trade]) {
        entity_object(
            self.entity,
            &self.link,
            object_entity_order::Order::SetTrades(v1::TradeList {
                trades: trades.iter().map(Trade::to_wire).collect(),
            }),
        );
    }

    pub async fn trades(&self) -> Result<Vec<Trade>, Error> {
        let list = self
            .link
            .ask(
                question::Question::Object(v1::ObjectQuestion {
                    query: Some(object_question::Query::Trades(self.entity)),
                }),
                "a trade list",
                |body| match body {
                    answer::Body::Object(v1::ObjectAnswer {
                        answer: Some(object_answer::Answer::Trades(list)),
                    }) => Some(list),
                    _ => None,
                },
            )
            .await?;
        list.trades
            .into_iter()
            .map(|trade| Trade::from_wire(trade, &self.link))
            .collect()
    }
}

#[derive(Debug, Clone)]
pub struct FireworkHandle {
    entity: i32,
    link: Link,
}

impl FireworkHandle {
    pub(crate) fn new(entity: i32, link: Link) -> Self {
        Self { entity, link }
    }

    pub fn detonate(&self) {
        entity_object(
            self.entity,
            &self.link,
            object_entity_order::Order::DetonateFirework(v1::Empty {}),
        );
    }
}

#[derive(Debug, Clone)]
pub struct VehicleHandle {
    entity: i32,
    link: Link,
}

impl VehicleHandle {
    pub(crate) fn new(entity: i32, link: Link) -> Self {
        Self { entity, link }
    }

    pub fn set_velocity(&self, velocity: Vector) {
        entity_object(
            self.entity,
            &self.link,
            object_entity_order::Order::SetVehicleVelocity(velocity.to_wire()),
        );
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HangingFace {
    North,
    South,
    East,
    West,
    Up,
    Down,
}

impl HangingFace {
    const fn wire(self) -> &'static str {
        match self {
            Self::North => "north",
            Self::South => "south",
            Self::East => "east",
            Self::West => "west",
            Self::Up => "up",
            Self::Down => "down",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HangingKind {
    ItemFrame,
    GlowItemFrame,
    Painting,
}

impl HangingKind {
    const fn wire(self) -> &'static str {
        match self {
            Self::ItemFrame => "minecraft:item_frame",
            Self::GlowItemFrame => "minecraft:glow_item_frame",
            Self::Painting => "minecraft:painting",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct HangingState {
    pub kind: String,
    pub pos: BlockPos,
    pub facing: String,
    pub item: Option<ItemStack>,
    pub rotation: i32,
    pub visible: bool,
    pub variant: Option<String>,
}

#[derive(Debug, Clone)]
pub struct FrameHandle {
    entity: i32,
    link: Link,
}

impl FrameHandle {
    pub(crate) fn new(entity: i32, link: Link) -> Self {
        Self { entity, link }
    }

    fn order(&self, order: object_entity_order::Order) {
        entity_object(self.entity, &self.link, order);
    }

    pub fn set_item(&self, item: Option<&ItemStack>) {
        self.order(object_entity_order::Order::SetFrameItem(v1::FrameItem {
            item: item.map(ItemStack::to_wire),
        }));
    }

    pub fn set_rotation(&self, rotation: i32) {
        self.order(object_entity_order::Order::SetFrameRotation(rotation));
    }

    pub fn set_visible(&self, visible: bool) {
        self.order(object_entity_order::Order::SetFrameVisible(visible));
    }

    pub async fn get_state(&self) -> Result<HangingState, Error> {
        let view = self
            .link
            .ask(
                question::Question::Object(v1::ObjectQuestion {
                    query: Some(object_question::Query::Hanging(self.entity)),
                }),
                "a hanging entity",
                |body| match body {
                    answer::Body::Object(v1::ObjectAnswer {
                        answer: Some(object_answer::Answer::Hanging(view)),
                    }) => Some(view),
                    _ => None,
                },
            )
            .await?;
        Ok(HangingState {
            kind: view.kind,
            pos: BlockPos::from_wire(view.pos, "hanging position")?,
            facing: view.facing,
            item: view.item.map(ItemStack::from_wire).transpose()?,
            rotation: view.rotation,
            visible: view.visible,
            variant: view.variant,
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct MapMarker {
    pub kind: String,
    pub x: f64,
    pub z: f64,
    pub rotation: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MapDecoration {
    pub kind: u8,
    pub x: i8,
    pub y: i8,
    pub rotation: i8,
    pub name: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MapView {
    pub id: i32,
    pub dimension: String,
    pub center_x: i32,
    pub center_z: i32,
    pub scale: u8,
    pub locked: bool,
    pub tracking_position: bool,
    pub unlimited_tracking: bool,
    pub colors: Vec<u8>,
    pub decorations: Vec<MapDecoration>,
}

impl MapView {
    fn from_wire(view: v1::MapView) -> Result<Self, Error> {
        let narrow = |value: i32, what: &str| {
            i8::try_from(value).map_err(|_| Error::Malformed(format!("{what} of {value}")))
        };
        Ok(Self {
            id: view.id,
            dimension: view.dimension,
            center_x: view.center_x,
            center_z: view.center_z,
            scale: u8::try_from(view.scale)
                .map_err(|_| Error::Malformed(format!("a map scale of {}", view.scale)))?,
            locked: view.locked,
            tracking_position: view.tracking_position,
            unlimited_tracking: view.unlimited_tracking,
            colors: view.colors,
            decorations: view
                .decorations
                .into_iter()
                .map(|decoration| {
                    Ok(MapDecoration {
                        kind: u8::try_from(decoration.kind).map_err(|_| {
                            Error::Malformed(format!("a marker type {}", decoration.kind))
                        })?,
                        x: narrow(decoration.x, "a marker x")?,
                        y: narrow(decoration.y, "a marker y")?,
                        rotation: narrow(decoration.rotation, "a marker rotation")?,
                        name: decoration.name,
                    })
                })
                .collect::<Result<_, Error>>()?,
        })
    }
}

#[derive(Debug, Clone)]
pub struct MapHandle {
    map: i32,
    link: Link,
}

impl MapHandle {
    pub(crate) fn new(map: i32, link: Link) -> Self {
        Self { map, link }
    }

    fn edit(&self, edit: map_edit::Edit) {
        self.link.order(from_plugin::Body::World(v1::WorldOrder {
            order: Some(world_order::Order::Object(v1::ObjectWorldOrder {
                order: Some(object_world_order::Order::EditMap(v1::MapEdit {
                    map: self.map,
                    edit: Some(edit),
                })),
            })),
        }));
    }

    pub fn set_center(&self, x: i32, z: i32) {
        self.edit(map_edit::Edit::SetCenter(v1::MapCenter { x, z }));
    }

    pub fn set_scale(&self, scale: u8) {
        self.edit(map_edit::Edit::SetScale(u32::from(scale)));
    }

    pub fn set_locked(&self, locked: bool) {
        self.edit(map_edit::Edit::SetLocked(locked));
    }

    pub fn set_tracking_position(&self, tracking: bool) {
        self.edit(map_edit::Edit::SetTrackingPosition(tracking));
    }

    pub fn set_unlimited_tracking(&self, unlimited: bool) {
        self.edit(map_edit::Edit::SetUnlimitedTracking(unlimited));
    }

    pub fn set_pixel(&self, x: i32, y: i32, color: u8) {
        self.edit(map_edit::Edit::SetPixel(v1::MapPixel {
            x,
            y,
            color: u32::from(color),
        }));
    }

    pub fn add_marker(&self, key: &str, marker: &MapMarker, name: Option<&str>) {
        self.edit(map_edit::Edit::AddMarker(v1::MapLiveMarker {
            key: key.to_owned(),
            kind: marker.kind.clone(),
            x: marker.x,
            z: marker.z,
            rotation: marker.rotation,
            name: name.map(str::to_owned),
        }));
    }

    pub fn remove_marker(&self, key: &str) {
        self.edit(map_edit::Edit::RemoveMarker(key.to_owned()));
    }

    pub fn clear_markers(&self) {
        self.edit(map_edit::Edit::ClearMarkers(v1::Empty {}));
    }

    pub fn set_canvas(&self, colors: &[u8]) {
        self.edit(map_edit::Edit::SetCanvas(colors.to_vec()));
    }

    pub async fn get_view(&self) -> Result<MapView, Error> {
        let view = self
            .link
            .ask(
                question::Question::Object(v1::ObjectQuestion {
                    query: Some(object_question::Query::Map(self.map)),
                }),
                "a map",
                |body| match body {
                    answer::Body::Object(v1::ObjectAnswer {
                        answer: Some(object_answer::Answer::Map(view)),
                    }) => Some(view),
                    _ => None,
                },
            )
            .await?;
        MapView::from_wire(view)
    }
}

#[derive(Debug, Clone)]
pub struct PlayerObjectsHandle {
    target: Arc<str>,
    link: Link,
}

impl PlayerObjectsHandle {
    pub(crate) fn new(target: Arc<str>, link: Link) -> Self {
        Self { target, link }
    }

    fn order(&self, order: object_player_order::Order) {
        self.link.order(from_plugin::Body::Player(v1::PlayerOrder {
            player: self.target.to_string(),
            order: Some(player_order::Order::Object(v1::ObjectPlayerOrder {
                order: Some(order),
            })),
        }));
    }

    pub fn open_merchant(&self, merchant: &Entity) {
        self.order(object_player_order::Order::OpenMerchant(merchant.entity_id));
    }

    pub fn item_data(&self, slot: i32) -> PdcHandle {
        PdcHandle::of_item(Arc::clone(&self.target), slot, self.link.clone())
    }

    pub fn set_map_marker(&self, slot: i32, key: &str, marker: &MapMarker) {
        self.order(object_player_order::Order::MapDecoration(
            v1::MapDecorationEdit {
                slot,
                key: key.to_owned(),
                change: Some(map_decoration_edit::Change::Set(v1::MapMarker {
                    kind: marker.kind.clone(),
                    x: marker.x,
                    z: marker.z,
                    rotation: marker.rotation,
                })),
            },
        ));
    }

    pub fn remove_map_marker(&self, slot: i32, key: &str) {
        self.order(object_player_order::Order::MapDecoration(
            v1::MapDecorationEdit {
                slot,
                key: key.to_owned(),
                change: Some(map_decoration_edit::Change::Remove(v1::Empty {})),
            },
        ));
    }
}

#[derive(Debug, Clone)]
pub struct FireworkLaunch {
    pub shooter: Option<i32>,
    pub attached: Option<i32>,
    pub velocity: Option<Vector>,
    pub angled: bool,
}

impl FireworkLaunch {
    pub const fn new() -> Self {
        Self {
            shooter: None,
            attached: None,
            velocity: None,
            angled: false,
        }
    }

    pub fn shooter(mut self, shooter: &Entity) -> Self {
        self.shooter = Some(shooter.entity_id);
        self
    }

    pub fn attached_to(mut self, target: &Entity) -> Self {
        self.attached = Some(target.entity_id);
        self
    }

    pub fn velocity(mut self, velocity: Vector) -> Self {
        self.velocity = Some(velocity);
        self
    }

    pub fn angled(mut self, angled: bool) -> Self {
        self.angled = angled;
        self
    }
}

impl Default for FireworkLaunch {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone)]
pub struct WorldObjectsHandle {
    link: Link,
}

impl WorldObjectsHandle {
    pub(crate) fn new(link: Link) -> Self {
        Self { link }
    }

    async fn created(&self, query: object_question::Query) -> Result<Entity, Error> {
        let snapshot = self
            .link
            .ask(
                question::Question::Object(v1::ObjectQuestion { query: Some(query) }),
                "a created entity",
                |body| match body {
                    answer::Body::Spawned(snapshot) => Some(snapshot),
                    _ => None,
                },
            )
            .await?;
        Entity::from_wire(snapshot, &self.link)
    }

    pub async fn launch_projectile(
        &self,
        kind: EntityType,
        position: Position,
        velocity: Vector,
        shooter: Option<&Entity>,
    ) -> Result<Entity, Error> {
        self.created(object_question::Query::LaunchProjectile(
            v1::LaunchProjectile {
                kind: kind.key().to_owned(),
                position: Some(position.to_wire()),
                velocity: Some(velocity.to_wire()),
                shooter: shooter.map(|entity| entity.entity_id),
            },
        ))
        .await
    }

    pub async fn launch_firework(
        &self,
        position: Position,
        rocket: &ItemStack,
        launch: FireworkLaunch,
    ) -> Result<Entity, Error> {
        self.created(object_question::Query::LaunchFirework(v1::LaunchFirework {
            position: Some(position.to_wire()),
            item: Some(rocket.to_wire()),
            shooter: launch.shooter,
            attached: launch.attached,
            angled: launch.angled,
            velocity: launch.velocity.map(Vector::to_wire),
        }))
        .await
    }

    pub async fn spawn_vehicle(
        &self,
        kind: EntityType,
        position: Position,
        velocity: Option<Vector>,
    ) -> Result<Entity, Error> {
        self.created(object_question::Query::SpawnVehicle(v1::SpawnVehicle {
            kind: kind.key().to_owned(),
            position: Some(position.to_wire()),
            velocity: velocity.map(Vector::to_wire),
        }))
        .await
    }

    async fn hang(
        &self,
        kind: HangingKind,
        pos: BlockPos,
        face: HangingFace,
        variant: Option<&str>,
        item: Option<&ItemStack>,
    ) -> Result<Entity, Error> {
        self.created(object_question::Query::PlaceHanging(v1::PlaceHanging {
            kind: kind.wire().to_owned(),
            pos: Some(pos.to_wire()),
            face: face.wire().to_owned(),
            variant: variant.map(str::to_owned),
            item: item.map(ItemStack::to_wire),
        }))
        .await
    }

    pub async fn place_item_frame(
        &self,
        pos: BlockPos,
        face: HangingFace,
        glowing: bool,
        item: Option<&ItemStack>,
    ) -> Result<Entity, Error> {
        let kind = if glowing {
            HangingKind::GlowItemFrame
        } else {
            HangingKind::ItemFrame
        };
        self.hang(kind, pos, face, None, item).await
    }

    pub async fn place_painting(
        &self,
        pos: BlockPos,
        face: HangingFace,
        variant: Option<&str>,
    ) -> Result<Entity, Error> {
        self.hang(HangingKind::Painting, pos, face, variant, None)
            .await
    }

    pub async fn spawn_display(
        &self,
        position: Position,
        spec: impl Into<DisplaySpec>,
    ) -> Result<Entity, Error> {
        self.created(object_question::Query::SpawnDisplay(v1::SpawnDisplay {
            position: Some(position.to_wire()),
            spec: Some(spec.into().to_wire()),
        }))
        .await
    }

    pub fn map(&self, id: i32) -> MapHandle {
        MapHandle::new(id, self.link.clone())
    }

    pub fn block_data(&self, pos: BlockPos) -> PdcHandle {
        PdcHandle::of_block(pos, self.link.clone())
    }
}

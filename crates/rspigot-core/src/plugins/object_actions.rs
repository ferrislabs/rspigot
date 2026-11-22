use std::collections::BTreeMap;

use rspigot_domain::player::PlayerId;
use rspigot_domain::{BlockPos, EntityId, Vec3};

use crate::plugins::displays::DisplaySpec;
use crate::plugins::launches::{FireworkRequest, HangingRequest, HangingView};
use crate::plugins::map_views::{MapDecorationEdit, MapEdit, MapView};
use crate::plugins::pdc::{PdcChange, PdcHolder, PdcValue};
use crate::plugins::{ActionError, Answer, ItemSpec, Trade};
use crate::ports::ClientLink;
use crate::server::Server;

#[derive(Debug, Clone, PartialEq)]
pub enum ObjectOrder {
    SetTrades(Vec<Trade>),
    EditDisplay(DisplaySpec),
    DetonateFirework,
    SetVehicleVelocity(Vec3),
    SetFrameItem(Option<ItemSpec>),
    SetFrameRotation(i32),
    SetFrameVisible(bool),
    Pdc(PdcChange),
}

impl ObjectOrder {
    pub const fn name(&self) -> &'static str {
        match self {
            Self::SetTrades(_) => "entity.merchant.set_trades",
            Self::EditDisplay(_) => "entity.display.edit",
            Self::DetonateFirework => "entity.firework.detonate",
            Self::SetVehicleVelocity(_) => "entity.vehicle.set_velocity",
            Self::SetFrameItem(_) => "entity.frame.set_item",
            Self::SetFrameRotation(_) => "entity.frame.set_rotation",
            Self::SetFrameVisible(_) => "entity.frame.set_visible",
            Self::Pdc(change) => change.name(),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum PlayerObjectOrder {
    OpenMerchant(EntityId),
    ItemPdc { slot: i32, change: PdcChange },
    MapDecoration(MapDecorationEdit),
}

impl PlayerObjectOrder {
    pub const fn name(&self) -> &'static str {
        match self {
            Self::OpenMerchant(_) => "player.merchant.open",
            Self::ItemPdc { change, .. } => change.name(),
            Self::MapDecoration(_) => "player.map.decoration",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum WorldObjectOrder {
    EditMap { map: i32, edit: MapEdit },
}

impl WorldObjectOrder {
    pub const fn name(&self) -> &'static str {
        match self {
            Self::EditMap { .. } => "world.map.edit",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum ObjectQuestion {
    LaunchProjectile {
        kind: String,
        position: Vec3,
        velocity: Vec3,
        shooter: Option<EntityId>,
    },
    LaunchFirework {
        position: Vec3,
        item: ItemSpec,
        shooter: Option<EntityId>,
        attached: Option<EntityId>,
        angled: bool,
        velocity: Option<Vec3>,
    },
    SpawnVehicle {
        kind: String,
        position: Vec3,
        velocity: Option<Vec3>,
    },
    PlaceHanging {
        kind: String,
        pos: BlockPos,
        face: String,
        variant: Option<String>,
        item: Option<ItemSpec>,
    },
    SpawnDisplay {
        position: Vec3,
        spec: DisplaySpec,
    },
    Trades(EntityId),
    Display(EntityId),
    Map(i32),
    Pdc(PdcHolder),
    Hanging(EntityId),
}

#[derive(Debug, Clone, PartialEq)]
pub enum ObjectAnswer {
    Trades(Vec<Trade>),
    Display(Box<DisplaySpec>),
    Map(Box<MapView>),
    Pdc(BTreeMap<String, PdcValue>),
    Hanging(Box<HangingView>),
}

fn answered<T>(result: Result<T, ActionError>, wrap: impl FnOnce(T) -> ObjectAnswer) -> Answer {
    match result {
        Ok(value) => Answer::Object(wrap(value)),
        Err(error) => Answer::Failed(error),
    }
}

impl<L: ClientLink> Server<L> {
    pub(super) fn object_order(&mut self, id: EntityId, order: ObjectOrder) -> Result<(), String> {
        match order {
            ObjectOrder::SetTrades(trades) => self.set_merchant_trades(id, &trades),
            ObjectOrder::EditDisplay(spec) => self.edit_display(id, &spec),
            ObjectOrder::DetonateFirework => self.detonate_firework(id),
            ObjectOrder::SetVehicleVelocity(velocity) => self.set_vehicle_velocity(id, velocity),
            ObjectOrder::SetFrameItem(item) => self.set_frame_item(id, item.as_ref()),
            ObjectOrder::SetFrameRotation(rotation) => self.set_frame_rotation(id, rotation),
            ObjectOrder::SetFrameVisible(visible) => self.set_frame_visible(id, visible),
            ObjectOrder::Pdc(change) => self.pdc_change(PdcHolder::Entity(id), change),
        }
        .map_err(|error| error.reason)
    }

    pub(super) fn player_object_order(
        &mut self,
        id: PlayerId,
        order: PlayerObjectOrder,
    ) -> Result<(), String> {
        match order {
            PlayerObjectOrder::OpenMerchant(merchant) => self.open_merchant(id, merchant),
            PlayerObjectOrder::ItemPdc { slot, change } => {
                self.pdc_change(PdcHolder::Item { player: id, slot }, change)
            }
            PlayerObjectOrder::MapDecoration(edit) => self.edit_map_decoration(id, edit),
        }
        .map_err(|error| error.reason)
    }

    pub(super) fn world_object_order(&mut self, order: WorldObjectOrder) -> Result<(), String> {
        match order {
            WorldObjectOrder::EditMap { map, edit } => self.edit_map(map, edit),
        }
        .map_err(|error| error.reason)
    }

    pub(super) fn object_answer(
        &mut self,
        connection: u64,
        request: u64,
        question: ObjectQuestion,
    ) -> Option<Answer> {
        let spawned = match question {
            ObjectQuestion::LaunchProjectile {
                kind,
                position,
                velocity,
                shooter,
            } => self.launch_projectile(connection, request, &kind, position, velocity, shooter),
            ObjectQuestion::LaunchFirework {
                position,
                item,
                shooter,
                attached,
                angled,
                velocity,
            } => self.launch_firework(
                connection,
                request,
                FireworkRequest {
                    position,
                    item: &item,
                    shooter,
                    attached,
                    angled,
                    velocity,
                },
            ),
            ObjectQuestion::SpawnVehicle {
                kind,
                position,
                velocity,
            } => self.spawn_vehicle(connection, request, &kind, position, velocity),
            ObjectQuestion::PlaceHanging {
                kind,
                pos,
                face,
                variant,
                item,
            } => self.place_hanging(
                connection,
                request,
                HangingRequest {
                    kind: &kind,
                    pos,
                    face: &face,
                    variant: variant.as_deref(),
                    item: item.as_ref(),
                },
            ),
            ObjectQuestion::SpawnDisplay { position, spec } => {
                self.spawn_display_for_plugin(connection, request, position, &spec)
            }
            ObjectQuestion::Trades(id) => {
                return Some(answered(self.merchant_trades(id), ObjectAnswer::Trades));
            }
            ObjectQuestion::Display(id) => {
                return Some(answered(self.display_of(id), |spec| {
                    ObjectAnswer::Display(Box::new(spec))
                }));
            }
            ObjectQuestion::Map(id) => {
                return Some(answered(self.map_view(id), |view| {
                    ObjectAnswer::Map(Box::new(view))
                }));
            }
            ObjectQuestion::Pdc(holder) => {
                return Some(answered(self.pdc_entries(&holder), ObjectAnswer::Pdc));
            }
            ObjectQuestion::Hanging(id) => {
                return Some(answered(self.hanging_view(id), |view| {
                    ObjectAnswer::Hanging(Box::new(view))
                }));
            }
        };
        spawned.err().map(Answer::Failed)
    }
}

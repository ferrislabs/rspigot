use rspigot_domain::{BlockPos, Rotation, Vec3};

use crate::events::{ClientEvent, GameEvent};
use crate::plugins::decisions::{Continuation, Lane};
use crate::plugins::events::{
    SpawnChange, ThunderChange, TimeSkip, TimeSkipPatch, TimeSkipReason, WeatherChange, WorldInit,
    WorldLoad, WorldSave,
};
use crate::plugins::steps::{Sky, TimeStep, WeatherStep};
use crate::plugins::{Access, EventKind, EventPayload, Location, Phase};
use crate::ports::ClientLink;
use crate::server::{Audience, Server};

impl<L: ClientLink> Server<L> {
    pub(crate) fn world_initialised(&mut self) {
        self.drain_startup();
        self.fire(
            Lane::World,
            Continuation::Notice(EventKind::WorldInit),
            |server| {
                Some(EventPayload::WorldInit(WorldInit {
                    world: server.world_name(),
                }))
            },
        );
    }

    pub fn world_loaded(&mut self) {
        self.drain_startup();
        self.fire(
            Lane::World,
            Continuation::Notice(EventKind::WorldLoad),
            |server| {
                Some(EventPayload::WorldLoad(WorldLoad {
                    world: server.world_name(),
                }))
            },
        );
    }

    pub(crate) fn world_saving(&mut self) {
        self.fire(
            Lane::World,
            Continuation::Notice(EventKind::WorldSave),
            |server| {
                Some(EventPayload::WorldSave(WorldSave {
                    world: server.world_name(),
                }))
            },
        );
    }

    pub(crate) fn world_saving_at_shutdown(&mut self) {
        if !self.plugins.listens(EventKind::WorldSave, Access::Observe) {
            return;
        }
        let payload = EventPayload::WorldSave(WorldSave {
            world: self.world_name(),
        });
        let event = self.next_event_id();
        self.notify_plugins(event, &payload, false, Phase::Settled);
    }

    pub(crate) fn change_day_time(&mut self, time: i64, reason: Option<TimeSkipReason>) {
        let skipped = time.wrapping_sub(self.world.level.day_time);
        let Some(reason) = reason.filter(|_| skipped != 0) else {
            self.move_day_time(skipped);
            return;
        };
        let applied = !self.plugins.listens(EventKind::TimeSkip, Access::Decide);
        if applied {
            self.move_day_time(skipped);
        }
        self.fire(
            Lane::World,
            Continuation::TimeSkip(TimeStep { skipped, applied }),
            move |server| {
                Some(EventPayload::TimeSkip(TimeSkip {
                    skip_amount: skipped,
                    skip_reason: reason,
                    world: server.world_name(),
                }))
            },
        );
    }

    fn move_day_time(&mut self, amount: i64) {
        let level = &mut self.world.level;
        level.day_time = level.day_time.wrapping_add(amount);
        let update = self.time_event();
        self.push(
            Audience::Dimension(self.world.dimension.key.clone()),
            update,
        );
    }

    pub(crate) fn finish_time_skip(&mut self, step: TimeStep, patch: TimeSkipPatch) {
        if !step.applied {
            self.move_day_time(patch.skip_amount.unwrap_or(step.skipped));
        }
    }

    pub(crate) fn change_spawn(&mut self, pos: BlockPos) {
        let previous = self.world.spawn.block();
        if previous == pos {
            return;
        }
        self.world.level.spawn = Some(pos);
        self.world.spawn = pos.center();
        let yaw = self.world.level.spawn_angle;
        self.fire(
            Lane::World,
            Continuation::Notice(EventKind::SpawnChange),
            move |server| {
                let world = server.world_name();
                Some(EventPayload::SpawnChange(SpawnChange {
                    previous_location: Location {
                        world: world.clone(),
                        position: Vec3::new(
                            f64::from(previous.x),
                            f64::from(previous.y),
                            f64::from(previous.z),
                        ),
                        rotation: Rotation::new(yaw, 0.0),
                    },
                    world,
                }))
            },
        );
        self.push(
            Audience::Dimension(self.world.dimension.key.clone()),
            ClientEvent::SpawnPosition {
                dimension: self.world.dimension.key.clone(),
                pos,
                yaw,
                pitch: 0.0,
            },
        );
    }

    pub(crate) fn change_weather(&mut self, raining: bool, thundering: bool) {
        self.fire_deferred(
            Lane::World,
            Continuation::SetWeather(Sky {
                raining,
                thundering,
            }),
        );
    }

    pub(crate) fn weather_at_head(&mut self, sky: Sky) -> Option<Continuation> {
        let level = &self.world.level;
        let step = WeatherStep {
            rain: (level.raining != sky.raining).then_some(sky.raining),
            thunder: (level.thundering != sky.thundering).then_some(sky.thundering),
        };
        if step.rain.is_some() {
            return Some(Continuation::Weather(step));
        }
        step.thunder.map(Continuation::Thunder)
    }

    pub(crate) fn weather_payload(&self, then: &Continuation) -> Option<EventPayload> {
        let world = self.world_name();
        match then {
            Continuation::Weather(step) => Some(EventPayload::WeatherChange(WeatherChange {
                to_weather_state: step.rain?,
                world,
            })),
            Continuation::Thunder(to) => Some(EventPayload::ThunderChange(ThunderChange {
                to_thunder_state: *to,
                world,
            })),
            _ => None,
        }
    }

    pub(crate) fn finish_weather(&mut self, step: WeatherStep, cancelled: bool) {
        if !cancelled
            && let Some(to) = step.rain
            && self.world.level.raining != to
        {
            self.world.level.raining = to;
            let start = if to {
                GameEvent::StartRaining
            } else {
                GameEvent::StopRaining
            };
            self.push(
                Audience::Dimension(self.world.dimension.key.clone()),
                ClientEvent::GameEvent(start),
            );
            self.push(
                Audience::Dimension(self.world.dimension.key.clone()),
                ClientEvent::GameEvent(GameEvent::RainLevelChange(level_of(to))),
            );
        }
        self.change_thunder(step.thunder);
    }

    fn change_thunder(&mut self, to: Option<bool>) {
        let Some(to) = to else {
            return;
        };
        self.fire(Lane::World, Continuation::Thunder(to), move |server| {
            Some(EventPayload::ThunderChange(ThunderChange {
                to_thunder_state: to,
                world: server.world_name(),
            }))
        });
    }

    pub(crate) fn finish_thunder(&mut self, to: bool, cancelled: bool) {
        if cancelled || self.world.level.thundering == to {
            return;
        }
        self.world.level.thundering = to;
        self.push(
            Audience::Dimension(self.world.dimension.key.clone()),
            ClientEvent::GameEvent(GameEvent::ThunderLevelChange(level_of(to))),
        );
    }
}

const fn level_of(on: bool) -> f32 {
    if on { 1.0 } else { 0.0 }
}

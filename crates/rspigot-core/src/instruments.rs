use rspigot_domain::game_event::GameEvent;
use rspigot_domain::player::PlayerId;
use rspigot_domain::stack_components;
use rspigot_entities::SoundSource;

use crate::bow::UsingItem;
use crate::events::Hand;
use crate::game_events::GameEventContext;
use crate::ports::ClientLink;
use crate::server::Server;

const RANGE_PER_VOLUME: f32 = 16.0;
const TICKS_PER_SECOND: f32 = 20.0;

pub(crate) fn instrument_use_ticks(stack: &rspigot_domain::item::ItemStack) -> Option<u64> {
    stack_components::instrument(stack)
        .map(|instrument| (instrument.use_seconds * TICKS_PER_SECOND) as u64)
}

impl<L: ClientLink> Server<L> {
    pub(crate) fn use_instrument(&mut self, id: PlayerId, hand: Hand) -> bool {
        let Some(stack) = self.held(id, hand) else {
            return false;
        };
        let Some(instrument) = stack_components::instrument(&stack) else {
            return false;
        };
        let Some(player) = self.players.get(&id) else {
            return false;
        };
        let (position, entity) = (player.position, player.entity_id);
        self.play_named_sound(
            Some(id),
            position,
            &instrument.sound,
            SoundSource::Records,
            instrument.range / RANGE_PER_VOLUME,
            1.0,
        );
        self.game_event(
            GameEvent::InstrumentPlay,
            position,
            GameEventContext::of(Some(entity)),
        );
        if let Some(group) = stack_components::cooldown_group(&stack) {
            let ticks = (instrument.use_seconds * TICKS_PER_SECOND) as i32;
            self.start_group_cooldown(id, group, ticks);
        }
        self.count_used(id, &stack);
        let since = self.current_tick();
        if let Some(player) = self.players.get_mut(&id)
            && player.using_item.is_none()
        {
            player.using_item = Some(UsingItem { hand, since });
            self.show_use_status(id);
        }
        true
    }
}

use crate::events::ClientEvent;
use crate::ports::ClientLink;
use crate::server::{Audience, Server};

impl<L: ClientLink> Server<L> {
    pub(crate) fn send_light_updates(&mut self) {
        for pos in self.world.chunks.relight_chunks() {
            let Some(chunk) = self.world.chunks.get(pos) else {
                continue;
            };
            let light = Box::new(chunk.light.clone());
            self.push(
                Audience::ChunkViewers(pos),
                ClientEvent::LightUpdate { chunk: pos, light },
            );
        }
    }
}

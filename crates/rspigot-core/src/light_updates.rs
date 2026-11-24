use crate::events::ClientEvent;
use crate::lighting::LightPatch;
use crate::ports::ClientLink;
use crate::server::{Audience, Server};

impl<L: ClientLink> Server<L> {
    pub(crate) fn send_light_updates(&mut self) {
        for (pos, sections) in self.world.chunks.relight_chunks().into_sorted() {
            let Some(chunk) = self.world.chunks.get(pos) else {
                continue;
            };
            let light = Box::new(LightPatch::of(&chunk.light, &sections));
            self.push(
                Audience::ChunkViewers(pos),
                ClientEvent::LightUpdate { chunk: pos, light },
            );
        }
    }
}

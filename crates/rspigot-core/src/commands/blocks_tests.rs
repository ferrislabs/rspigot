use rspigot_blocks::BlockKind;
use rspigot_domain::BlockPos;
use rspigot_domain::player::PlayerId;

use super::{CommandOutcome, CommandSender, execute};
use crate::end_tests::{Tap, arrive};
use crate::events::ClientEvent;
use crate::server::Server;

struct Rig {
    server: Server<Tap>,
    id: PlayerId,
    link: Tap,
}

fn rig() -> Rig {
    let (mut server, id, link) = arrive();
    if let Some(player) = server.players.get_mut(&id) {
        player.operator = true;
    }
    Rig { server, id, link }
}

impl Rig {
    fn run(&mut self, line: &str) -> CommandOutcome {
        let outcome = execute(&mut self.server, CommandSender::Player(self.id), line);
        self.server.deliver();
        outcome
    }

    fn texts(&self) -> Vec<String> {
        self.link
            .take()
            .into_iter()
            .filter_map(|event| match event {
                ClientEvent::SystemChat { message, .. } => Some(message.plain_text()),
                _ => None,
            })
            .collect()
    }

    fn kind(&self, x: i32, y: i32, z: i32) -> Option<BlockKind> {
        self.server
            .block_at(BlockPos::new(x, y, z))
            .and_then(|s| s.kind())
    }

    fn origin(&self) -> BlockPos {
        self.server.players[&self.id].position.block()
    }
}

#[test]
fn setblock_places_and_reports() {
    let mut rig = rig();
    let at = rig.origin();
    let line = format!(
        "setblock {} {} {} minecraft:stone",
        at.x + 3,
        at.y + 5,
        at.z
    );
    rig.texts();
    assert_eq!(rig.run(&line), CommandOutcome::Success);
    assert_eq!(rig.kind(at.x + 3, at.y + 5, at.z), Some(BlockKind::Stone));
    assert_eq!(
        rig.texts(),
        [format!(
            "commands.setblock.success {} {} {}",
            at.x + 3,
            at.y + 5,
            at.z
        )]
    );
    assert_eq!(rig.run(&line), CommandOutcome::Failure);
    assert_eq!(rig.texts(), ["commands.setblock.failed"]);
}

#[test]
fn setblock_keep_refuses_to_replace_and_checks_bounds() {
    let mut rig = rig();
    let at = rig.origin();
    let target = (at.x + 3, at.y + 5, at.z);
    rig.run(&format!(
        "setblock {} {} {} stone",
        target.0, target.1, target.2
    ));
    assert_eq!(
        rig.run(&format!(
            "setblock {} {} {} dirt keep",
            target.0, target.1, target.2
        )),
        CommandOutcome::Failure
    );
    assert_eq!(
        rig.kind(target.0, target.1, target.2),
        Some(BlockKind::Stone)
    );
    rig.texts();
    assert_eq!(
        rig.run(&format!("setblock {} 100000 {} stone", at.x, at.z)),
        CommandOutcome::Failure
    );
    assert_eq!(rig.texts(), ["argument.pos.outofworld"]);
    assert_eq!(
        rig.run("setblock ~ ~ ~ minecraft:nothing"),
        CommandOutcome::Failure
    );
    assert_eq!(rig.texts(), ["argument.block.id.invalid minecraft:nothing"]);
}

#[test]
fn setblock_applies_block_states() {
    let mut rig = rig();
    let at = rig.origin();
    let (x, y, z) = (at.x + 2, at.y + 6, at.z);
    rig.run(&format!(
        "setblock {x} {y} {z} minecraft:oak_stairs[facing=east,half=top]"
    ));
    let state = rig.server.block_at(BlockPos::new(x, y, z)).expect("loaded");
    assert_eq!(state.property("facing"), Some("east"));
    assert_eq!(state.property("half"), Some("top"));
}

#[test]
fn fill_modes_shape_the_region() {
    let mut rig = rig();
    let at = rig.origin();
    let (x, y, z) = (at.x + 2, at.y + 6, at.z + 2);
    let span = |extra: &str, block: &str| {
        format!(
            "fill {x} {y} {z} {} {} {} {block} {extra}",
            x + 2,
            y + 2,
            z + 2
        )
    };
    assert_eq!(rig.run(&span("", "stone")), CommandOutcome::Success);
    assert_eq!(
        rig.texts().last().map(String::as_str),
        Some("commands.fill.success 27")
    );
    assert_eq!(rig.run(&span("hollow", "dirt")), CommandOutcome::Success);
    assert_eq!(rig.kind(x, y, z), Some(BlockKind::Dirt));
    assert_eq!(rig.kind(x + 1, y + 1, z + 1), Some(BlockKind::Air));
    assert_eq!(rig.run(&span("outline", "glass")), CommandOutcome::Success);
    assert_eq!(rig.kind(x, y, z), Some(BlockKind::Glass));
    assert_eq!(rig.kind(x + 1, y + 1, z + 1), Some(BlockKind::Air));
    assert_eq!(
        rig.run(&span("keep", "gold_block")),
        CommandOutcome::Success
    );
    assert_eq!(rig.kind(x, y, z), Some(BlockKind::Glass));
    assert_eq!(rig.kind(x + 1, y + 1, z + 1), Some(BlockKind::GoldBlock));
    assert_eq!(
        rig.run(&span("replace glass", "stone")),
        CommandOutcome::Success
    );
    assert_eq!(rig.kind(x, y, z), Some(BlockKind::Stone));
    assert_eq!(rig.kind(x + 1, y + 1, z + 1), Some(BlockKind::GoldBlock));
    rig.texts();
    assert_eq!(
        rig.run(&span("replace glass", "stone")),
        CommandOutcome::Failure
    );
    assert_eq!(rig.texts(), ["commands.fill.failed"]);
}

#[test]
fn fill_refuses_huge_regions() {
    let mut rig = rig();
    let at = rig.origin();
    let line = format!(
        "fill {} {} {} {} {} {} stone",
        at.x,
        at.y,
        at.z,
        at.x + 40,
        at.y + 39,
        at.z + 40
    );
    rig.texts();
    assert_eq!(rig.run(&line), CommandOutcome::Failure);
    assert_eq!(rig.texts(), ["commands.fill.toobig 32768 67240"]);
}

#[test]
fn clone_copies_moves_and_guards_overlap() {
    let mut rig = rig();
    let at = rig.origin();
    let (x, y, z) = (at.x + 2, at.y + 6, at.z);
    rig.run(&format!("fill {x} {y} {z} {} {} {z} stone", x + 1, y + 1));
    let line = |tail: &str| {
        format!(
            "clone {x} {y} {z} {} {} {z} {} {y} {z} {tail}",
            x + 1,
            y + 1,
            x + 5
        )
    };
    rig.texts();
    assert_eq!(rig.run(&line("")), CommandOutcome::Success);
    assert_eq!(rig.texts(), ["commands.clone.success 4"]);
    assert_eq!(rig.kind(x + 5, y, z), Some(BlockKind::Stone));
    assert_eq!(rig.kind(x + 6, y + 1, z), Some(BlockKind::Stone));
    let overlap = format!(
        "clone {x} {y} {z} {} {} {z} {} {y} {z}",
        x + 1,
        y + 1,
        x + 1
    );
    assert_eq!(rig.run(&overlap), CommandOutcome::Failure);
    assert_eq!(rig.texts(), ["commands.clone.overlap"]);
    let moved = format!(
        "clone {x} {y} {z} {} {} {z} {} {y} {z} replace move",
        x + 1,
        y + 1,
        x + 10
    );
    assert_eq!(rig.run(&moved), CommandOutcome::Success);
    assert_eq!(rig.kind(x, y, z), Some(BlockKind::Air));
    assert_eq!(rig.kind(x + 10, y, z), Some(BlockKind::Stone));
    let masked = format!(
        "clone {x} {y} {z} {} {} {z} {} {y} {z} masked",
        x + 1,
        y + 1,
        x + 5
    );
    assert_eq!(rig.run(&masked), CommandOutcome::Failure);
    assert_eq!(rig.kind(x + 5, y, z), Some(BlockKind::Stone));
}

#[test]
fn setblock_inside_execute_uses_the_new_position() {
    let mut rig = rig();
    let at = rig.origin();
    let line = format!(
        "execute positioned {}.5 {}.0 {}.5 run setblock ~ ~6 ~ minecraft:stone",
        at.x + 2,
        at.y,
        at.z
    );
    assert_eq!(rig.run(&line), CommandOutcome::Success);
    assert_eq!(rig.kind(at.x + 2, at.y + 6, at.z), Some(BlockKind::Stone));
}

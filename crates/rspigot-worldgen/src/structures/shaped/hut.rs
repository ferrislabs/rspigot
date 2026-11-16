use rspigot_blocks::BlockKind as Kind;
use rspigot_domain::BlockStateId;
use serde_json::json;

use super::frame::{Frame, Guard, plain, with};
use super::scatter::Piece;
use crate::overworld::features::place::Run;
use crate::structures::bbox::Bbox;

fn shaped(state: BlockStateId, shape: &str) -> BlockStateId {
    state.with_property("shape", shape).unwrap_or(state)
}

fn body(f: &mut Frame) {
    f.bounds(
        (1, 1, 1),
        (5, 1, 7),
        plain(Kind::SprucePlanks),
        plain(Kind::SprucePlanks),
        false,
    );
    f.bounds(
        (1, 4, 2),
        (5, 4, 7),
        plain(Kind::SprucePlanks),
        plain(Kind::SprucePlanks),
        false,
    );
    f.bounds(
        (2, 1, 0),
        (4, 1, 0),
        plain(Kind::SprucePlanks),
        plain(Kind::SprucePlanks),
        false,
    );
    f.bounds(
        (2, 2, 2),
        (3, 3, 2),
        plain(Kind::SprucePlanks),
        plain(Kind::SprucePlanks),
        false,
    );
    f.bounds(
        (1, 2, 3),
        (1, 3, 6),
        plain(Kind::SprucePlanks),
        plain(Kind::SprucePlanks),
        false,
    );
    f.bounds(
        (5, 2, 3),
        (5, 3, 6),
        plain(Kind::SprucePlanks),
        plain(Kind::SprucePlanks),
        false,
    );
    f.bounds(
        (2, 2, 7),
        (4, 3, 7),
        plain(Kind::SprucePlanks),
        plain(Kind::SprucePlanks),
        false,
    );
    f.bounds(
        (1, 0, 2),
        (1, 3, 2),
        plain(Kind::OakLog),
        plain(Kind::OakLog),
        false,
    );
    f.bounds(
        (5, 0, 2),
        (5, 3, 2),
        plain(Kind::OakLog),
        plain(Kind::OakLog),
        false,
    );
    f.bounds(
        (1, 0, 7),
        (1, 3, 7),
        plain(Kind::OakLog),
        plain(Kind::OakLog),
        false,
    );
    f.bounds(
        (5, 0, 7),
        (5, 3, 7),
        plain(Kind::OakLog),
        plain(Kind::OakLog),
        false,
    );
    f.place(plain(Kind::OakFence), 2, 3, 2);
    f.place(plain(Kind::OakFence), 3, 3, 7);
    f.place(plain(Kind::Air), 1, 3, 4);
    f.place(plain(Kind::Air), 5, 3, 4);
    f.place(plain(Kind::Air), 5, 3, 5);
    f.place(plain(Kind::PottedRedMushroom), 1, 3, 5);
    f.place(plain(Kind::CraftingTable), 3, 2, 6);
    f.place(plain(Kind::Cauldron), 4, 2, 6);
    f.place(plain(Kind::OakFence), 1, 2, 1);
    f.place(plain(Kind::OakFence), 5, 2, 1);
    let north = with(Kind::SpruceStairs, &[("facing", "north")]);
    let east = with(Kind::SpruceStairs, &[("facing", "east")]);
    let west = with(Kind::SpruceStairs, &[("facing", "west")]);
    let south = with(Kind::SpruceStairs, &[("facing", "south")]);
    f.bounds((0, 4, 1), (6, 4, 1), north, north, false);
    f.bounds((0, 4, 2), (0, 4, 7), east, east, false);
    f.bounds((6, 4, 2), (6, 4, 7), west, west, false);
    f.bounds((0, 4, 8), (6, 4, 8), south, south, false);
    f.place(shaped(north, "outer_right"), 0, 4, 1);
    f.place(shaped(north, "outer_left"), 6, 4, 1);
    f.place(shaped(south, "outer_left"), 0, 4, 8);
    f.place(shaped(south, "outer_right"), 6, 4, 8);
    let mut v12 = 2;
    while v12 <= 7 {
        let mut v13 = 1;
        while v13 <= 5 {
            f.column_down(plain(Kind::OakLog), v13, -1, v12);
            v13 += 4;
        }
        v12 += 5;
    }
}

fn spawn(f: &mut Frame, id: &str, extra: Option<(&str, &str)>) {
    let at = f.world_pos(2, 2, 5);
    if !f.chunk.contains(at) {
        return;
    }
    let mut entity = json!({
        "id": id,
        "Pos": [f64::from(at.0) + 0.5, f64::from(at.1), f64::from(at.2) + 0.5],
        "Motion": [0.0, 0.0, 0.0],
        "Rotation": [0.0f32, 0.0f32],
        "PersistenceRequired": { "$b": 1 },
    });
    if let (Some((key, value)), Some(map)) = (extra, entity.as_object_mut()) {
        map.insert(key.to_owned(), json!(value));
    }
    f.run.region.entities.push(entity);
}

impl Piece {
    pub(crate) fn place_hut(&self, run: &mut Run, write: &Bbox) {
        let Some(height) = self.average(run) else {
            return;
        };
        let bbox = self.moved(height, 0);
        let mut f = Frame::new(run, bbox, Some(self.facing), *write, Guard::Open).oriented();
        body(&mut f);
        spawn(&mut f, "minecraft:witch", None);
        spawn(
            &mut f,
            "minecraft:cat",
            Some(("variant", "minecraft:all_black")),
        );
        f.finish();
    }
}

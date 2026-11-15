use std::sync::Arc;

use super::place::{Placed, Run};
use super::prov::Pos;

type Step = (i32, i32, i32);

const HORIZONTAL: [Step; 4] = [(0, 0, -1), (1, 0, 0), (0, 0, 1), (-1, 0, 0)];
const UP: Step = (0, 1, 0);

fn shuffle<T>(run: &mut Run, items: &mut [T]) {
    for index in (2..=items.len()).rev() {
        let other = run.rng.below(index as i32) as usize;
        items.swap(index - 1, other);
    }
}

fn moved((x, y, z): Pos, (dx, dy, dz): Step) -> Pos {
    (x + dx, y + dy, z + dz)
}

fn opposite((dx, dy, dz): Step) -> Step {
    (-dx, -dy, -dz)
}

pub(crate) fn reach_of_tree(feature: &Placed) -> i32 {
    4 + feature.margin
}

pub(crate) fn reach_of_claw(feature: &Placed) -> i32 {
    5 + feature.margin
}

pub(crate) fn tree(feature: &Arc<Placed>, run: &mut Run, origin: Pos) -> bool {
    let mut at = origin;
    let trunk = run.rng.below(3) + 1;
    for _ in 0..trunk {
        if !feature.place(run, at) {
            return true;
        }
        at = moved(at, UP);
    }
    let top = at;
    let branches = (run.rng.below(3) + 2) as usize;
    let mut directions = HORIZONTAL;
    shuffle(run, &mut directions);
    for direction in &directions[..branches] {
        at = moved(top, *direction);
        let length = run.rng.below(5) + 2;
        let mut sideways = 0;
        for step in 0..length {
            if !feature.place(run, at) {
                break;
            }
            sideways += 1;
            at = moved(at, UP);
            if step == 0 || (sideways >= 2 && run.rng.next_float() < 0.25) {
                at = moved(at, *direction);
                sideways = 0;
            }
        }
    }
    true
}

pub(crate) fn claw(feature: &Arc<Placed>, run: &mut Run, origin: Pos) -> bool {
    if !feature.place(run, origin) {
        return false;
    }
    let main_index = run.rng.below(4) as usize;
    let main = HORIZONTAL[main_index];
    let count = (run.rng.below(2) + 2) as usize;
    let mut directions = [
        main,
        HORIZONTAL[(main_index + 1) % 4],
        HORIZONTAL[(main_index + 3) % 4],
    ];
    shuffle(run, &mut directions);
    for direction in &directions[..count] {
        let mut at = origin;
        let first = run.rng.below(2) + 1;
        at = moved(at, *direction);
        let (along, second) = if *direction == main {
            (main, run.rng.below(3) + 2)
        } else {
            at = moved(at, UP);
            let pick = [*direction, UP][run.rng.below(2) as usize];
            (pick, run.rng.below(3) + 3)
        };
        for _ in 0..first {
            if !feature.place(run, at) {
                break;
            }
            at = moved(at, along);
        }
        at = moved(at, opposite(along));
        at = moved(at, UP);
        for _ in 0..second {
            at = moved(at, main);
            if !feature.place(run, at) {
                break;
            }
            if run.rng.next_float() < 0.25 {
                at = moved(at, UP);
            }
        }
    }
    true
}

#[cfg(test)]
mod tests;

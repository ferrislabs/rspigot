mod reactions;
mod resolver;

pub use resolver::{
    Limits, PUSH_LIMIT, Reaction, Resolver, can_stick_to_each_other, is_pushable, is_sticky,
    reaction_of,
};

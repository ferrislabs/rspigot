mod connect;
mod rails;
mod replace;
mod shape;
mod support;
mod survive;

pub use connect::{connection_state, stairs_shape};
pub use rails::{RailChange, is_rail, rail_placed, rail_removal_needed};
pub use replace::{Adjusted, Click, Look, adjust, can_be_replaced};
pub use shape::{Shaped, update_shape};
pub use survive::{can_survive, is_deferred};

#[cfg(test)]
mod tests;

pub const PROTOCOL_VERSION: u32 = 5;

#[allow(clippy::all)]
pub mod v1 {
    include!("generated/rspigot.plugin.v1.rs");
}

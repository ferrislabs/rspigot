pub const PROTOCOL_VERSION: u32 = 5;

#[allow(clippy::all)]
pub mod v1 {
    include!("generated/rspigot.plugin.v1.rs");
}

pub mod proxy {
    pub const PROTOCOL_VERSION: u32 = 1;

    #[allow(clippy::all)]
    pub mod v1 {
        include!("generated/rspigot.proxy.v1.rs");
    }
}

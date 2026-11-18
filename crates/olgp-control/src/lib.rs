//! Open Local GPU Protocol control plane.

pub mod api;
pub mod auth;
pub mod bench;
pub mod config;
pub mod discovery;
pub mod gateway;
pub mod household;
pub mod images;
pub mod rag;
pub mod recordings;
pub mod speaker;
pub mod product;
pub mod memory;
pub mod pairing;
pub mod sched;
pub mod netlink;
pub mod registry;
pub mod telemetry;
pub mod voice;
pub mod venus;

pub mod shim;
/// Protocol version implemented by this crate.
pub const PROTOCOL_VERSION: u8 = 1;

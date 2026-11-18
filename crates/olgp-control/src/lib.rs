//! Open Local GPU Protocol control plane.

pub mod api;
pub mod bench;
pub mod config;
pub mod discovery;
pub mod gateway;
pub mod pairing;
pub mod sched;
pub mod telemetry;
pub mod voice;
pub mod venus;

pub mod shim;
/// Protocol version implemented by this crate.
pub const PROTOCOL_VERSION: u8 = 1;

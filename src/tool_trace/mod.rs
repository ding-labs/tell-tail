//! Source-independent, bounded per-tool diagnostic evidence.
//! A replay contract is not a claim of desktop capture support.
mod correlate;
pub(crate) mod identity;
pub mod model;
pub mod replay;
pub mod store;

pub mod live;
pub mod otlp;
#[cfg(feature = "web")]
pub mod receiver;

#[cfg(feature = "web")]
mod setup_config;

#[cfg(feature = "web")]
pub mod service;

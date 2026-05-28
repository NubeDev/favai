mod config;
pub mod sync;
mod watch;
mod agent;

pub mod error;
pub mod git;
pub mod builder;
pub mod mcp_bridge;

pub use config::FavaiConfig;
pub use agent::{FavaiAgent, ReloadEvent, ReloadTrigger, SourceStatus, SyncReport};
pub use builder::apply_to_builder;
pub use error::FavaiError;

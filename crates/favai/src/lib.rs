pub mod config;
pub mod sync;
mod agent;

pub mod error;
pub mod git;
pub mod builder;
pub mod mcp_bridge;

pub use config::FavaiConfig;
pub use agent::{FavaiAgent, ReloadEvent, SourceStatus, SyncReport};
pub use builder::{apply_to_builder, sources_root};
pub use error::FavaiError;

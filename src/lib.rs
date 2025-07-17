//! OffGridComm - Decentralized Amateur Radio Communications Platform
//! 
//! A comprehensive messaging system designed for ham radio operators, featuring:
//! - Direct messaging between callsigns
//! - Group/bulletin board discussions  
//! - RF transmission via multiple protocols (AX.25, VARA, etc.)
//! - Web and terminal interfaces
//! - Message store-and-forward networking
//! - FCC Part 97 compliant (no encryption, callsign authentication)

#![warn(missing_docs)]
#![warn(rust_2018_idioms)]

pub mod auth;
pub mod config;
pub mod database;
pub mod messaging;
pub mod network;
pub mod utils;

#[cfg(feature = "web")]
pub mod web;

#[cfg(feature = "terminal")]
pub mod terminal;

/// Re-export common types for convenience
pub use auth::{User, UserRole, UserStatus};
pub use config::Settings;
pub use database::Database;
pub use messaging::{MessageSystem, MessageType, MessagePriority};

/// Library version information
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Library name
pub const NAME: &str = env!("CARGO_PKG_NAME");

/// Initialize logging for the library
pub fn init_logging(level: tracing::Level) -> anyhow::Result<()> {
    use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};
    
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| format!("{}={}", NAME, level).into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();
    
    Ok(())
}

/// Common error type for the library
pub type Result<T> = anyhow::Result<T>;

/// Common error types that can occur across the library
#[derive(Debug, thiserror::Error)]
pub enum OffGridCommError {
    #[error("Configuration error: {0}")]
    Config(#[from] config::ConfigError),
    
    #[error("Database error: {0}")]
    Database(#[from] anyhow::Error),
    
    #[error("Authentication error: {0}")]
    Auth(#[from] auth::AuthError),
    
    #[error("Messaging error: {0}")]
    Messaging(#[from] messaging::MessageError),
    
    #[error("Network error: {0}")]
    Network(String),
    
    #[error("Invalid callsign: {0}")]
    InvalidCallsign(String),
    
    #[error("Permission denied: {0}")]
    PermissionDenied(String),
    
    #[error("Resource not found: {0}")]
    NotFound(String),
    
    #[error("System unavailable: {0}")]
    Unavailable(String),
}

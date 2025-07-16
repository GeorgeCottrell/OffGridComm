use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

pub mod settings;

pub use settings::Settings;

/// Configuration loader and validator
pub struct ConfigLoader {
    config_path: Option<PathBuf>,
    data_dir: Option<PathBuf>,
}

impl ConfigLoader {
    /// Create a new configuration loader
    pub fn new() -> Self {
        Self {
            config_path: None,
            data_dir: None,
        }
    }

    /// Set the configuration file path
    pub fn with_config_path(mut self, path: PathBuf) -> Self {
        self.config_path = Some(path);
        self
    }

    /// Set the data directory path
    pub fn with_data_dir(mut self, dir: PathBuf) -> Self {
        self.data_dir = Some(dir);
        self
    }

    /// Load and validate configuration
    pub fn load(self) -> Result<Settings> {
        let mut builder = config::Config::builder();

        // Load default configuration
        builder = builder.add_source(config::File::from_str(
            DEFAULT_CONFIG,
            config::FileFormat::Toml,
        ));

        // Load user configuration file if it exists
        if let Some(config_path) = &self.config_path {
            if config_path.exists() {
                builder = builder.add_source(config::File::from(config_path.clone()));
            } else {
                tracing::warn!("Configuration file not found: {:?}", config_path);
            }
        } else {
            // Try to load from standard locations
            let possible_paths = [
                "offgridcomm.toml",
                "config/offgridcomm.toml",
                "/etc/offgridcomm/config.toml",
                "~/.config/offgridcomm/config.toml",
            ];

            for path in &possible_paths {
                let path = PathBuf::from(path);
                if path.exists() {
                    builder = builder.add_source(config::File::from(path));
                    break;
                }
            }
        }

        // Override with environment variables (OGC_ prefix)
        builder = builder.add_source(
            config::Environment::with_prefix("OGC")
                .try_parsing(true)
                .separator("_")
                .list_separator(",")
        );

        let config = builder.build()?;
        let mut settings: Settings = config.try_deserialize()?;

        // Override data directory if provided
        if let Some(data_dir) = self.data_dir {
            settings.data_dir = data_dir;
        }

        // Validate and ensure required directories exist
        settings.validate_and_create_dirs()?;

        Ok(settings)
    }
}

impl Default for ConfigLoader {
    fn default() -> Self {
        Self::new()
    }
}

/// Default configuration embedded in binary
const DEFAULT_CONFIG: &str = r#"
# OffGridComm Default Configuration

# Node identification
[node]
callsign = ""
location = ""
description = "OffGridComm Node"
timezone = "UTC"

# Data storage paths
data_dir = "./data"

# Database configuration
[database]
type = "sqlite"
path = "offgridcomm.db"
max_connections = 10
connection_timeout = 30
query_timeout = 10

# Message system configuration
[messaging]
max_message_size = 1024
max_messages_per_user = 1000
message_retention_days = 30
enable_groups = true
enable_direct_messages = true

# Network configuration
[network]
node_id = ""
listen_port = 14580
max_connections = 50
connection_timeout = 30
heartbeat_interval = 60
sync_interval = 300

# RF interface configuration
[rf]
enabled = true
interface = "serial"
device = "/dev/ttyUSB0"
baud_rate = 9600
protocol = "ax25"
callsign = ""
ssid = 1

# AX.25 specific settings
[rf.ax25]
digipeater_path = []
beacon_interval = 600
beacon_text = "OffGridComm Node"

# Web interface configuration
[web]
enabled = true
host = "0.0.0.0"
port = 8080
static_dir = "./static"
session_timeout = 3600
max_upload_size = 1048576
cors_origins = ["http://localhost:3000"]

# Terminal interface configuration
[terminal]
enabled = true
host = "0.0.0.0"
port = 2323
max_connections = 10
idle_timeout = 1800
welcome_message = "Welcome to OffGridComm"

# Compression settings
[compression]
enabled = true
algorithm = "lz4"
level = 1
min_size = 128

# Security settings
[security]
password_min_length = 8
session_timeout = 3600
max_login_attempts = 5
lockout_duration = 900

# Logging configuration
[logging]
level = "info"
file = "offgridcomm.log"
max_size = 10485760
max_files = 5
console = true

# Performance settings
[performance]
worker_threads = 4
max_blocking_threads = 512
event_interval = 100
stack_size = 2097152

# Features configuration
[features]
web_interface = true
terminal_interface = true
rf_interface = true
compression = true
callsign_validation = true
message_encryption = false
"#;

/// Configuration validation errors
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("Invalid callsign format: {0}")]
    InvalidCallsign(String),
    
    #[error("Invalid directory path: {0}")]
    InvalidDirectory(String),
    
    #[error("Invalid port number: {0}")]
    InvalidPort(u16),
    
    #[error("Invalid compression algorithm: {0}")]
    InvalidCompression(String),
    
    #[error("Invalid RF protocol: {0}")]
    InvalidRfProtocol(String),
    
    #[error("Configuration file error: {0}")]
    FileError(String),
    
    #[error("Environment variable error: {0}")]
    EnvError(String),
}

/// Configuration validation utilities
pub mod validation {
    use super::ConfigError;
    use regex::Regex;
    use once_cell::sync::Lazy;
    
    // Amateur radio callsign validation regex
    static CALLSIGN_REGEX: Lazy<Regex> = Lazy::new(|| {
        Regex::new(r"^[A-Z]{1,2}[0-9][A-Z]{1,3}$").unwrap()
    });
    
    /// Validate amateur radio callsign format
    pub fn validate_callsign(callsign: &str) -> Result<(), ConfigError> {
        if callsign.is_empty() {
            return Ok(()); // Empty callsign is allowed during setup
        }
        
        if !CALLSIGN_REGEX.is_match(callsign) {
            return Err(ConfigError::InvalidCallsign(callsign.to_string()));
        }
        
        Ok(())
    }
    
    /// Validate port number
    pub fn validate_port(port: u16) -> Result<(), ConfigError> {
        if port < 1024 {
            return Err(ConfigError::InvalidPort(port));
        }
        Ok(())
    }
    
    /// Validate compression algorithm
    pub fn validate_compression(algorithm: &str) -> Result<(), ConfigError> {
        match algorithm {
            "none" | "lz4" | "zstd" | "gzip" => Ok(()),
            _ => Err(ConfigError::InvalidCompression(algorithm.to_string())),
        }
    }
    
    /// Validate RF protocol
    pub fn validate_rf_protocol(protocol: &str) -> Result<(), ConfigError> {
        match protocol {
            "ax25" | "kiss" | "vara" | "ardop" => Ok(()),
            _ => Err(ConfigError::InvalidRfProtocol(protocol.to_string())),
        }
    }
    
    /// Validate directory path and create if needed
    pub fn validate_and_create_dir(path: &std::path::Path) -> Result<(), ConfigError> {
        if let Err(e) = std::fs::create_dir_all(path) {
            return Err(ConfigError::InvalidDirectory(format!("{}: {}", path.display(), e)));
        }
        Ok(())
    }
}

/// Environment variable helpers
pub mod env {
    use std::env;
    use std::str::FromStr;
    
    /// Get environment variable with default value
    pub fn get_env_or<T>(key: &str, default: T) -> T
    where
        T: FromStr,
        T::Err: std::fmt::Debug,
    {
        env::var(key)
            .ok()
            .and_then(|val| val.parse().ok())
            .unwrap_or(default)
    }
    
    /// Get required environment variable
    pub fn get_env<T>(key: &str) -> Result<T, String>
    where
        T: FromStr,
        T::Err: std::fmt::Debug,
    {
        env::var(key)
            .map_err(|_| format!("Environment variable {} not found", key))?
            .parse()
            .map_err(|e| format!("Failed to parse {}: {:?}", key, e))
    }
    
    /// Check if running in Docker container
    pub fn is_docker() -> bool {
        std::path::Path::new("/.dockerenv").exists() ||
        env::var("DOCKER_CONTAINER").is_ok()
    }
    
    /// Get Docker-specific defaults
    pub fn docker_defaults() -> std::collections::HashMap<String, String> {
        let mut defaults = std::collections::HashMap::new();
        defaults.insert("OGC_WEB_HOST".to_string(), "0.0.0.0".to_string());
        defaults.insert("OGC_TERMINAL_HOST".to_string(), "0.0.0.0".to_string());
        defaults.insert("OGC_DATA_DIR".to_string(), "/data".to_string());
        defaults.insert("OGC_DATABASE_PATH".to_string(), "/data/offgridcomm.db".to_string());
        defaults.insert("OGC_LOGGING_FILE".to_string(), "/data/offgridcomm.log".to_string());
        defaults
    }
}

/// Configuration file templates
pub mod templates {
    /// Generate a basic configuration file template
    pub fn basic_config(callsign: &str, location: Option<&str>) -> String {
        format!(
            r#"# OffGridComm Configuration

[node]
callsign = "{}"
location = "{}"
description = "OffGridComm Node"

[database]
path = "./data/offgridcomm.db"

[rf]
enabled = true
callsign = "{}"
device = "/dev/ttyUSB0"

[web]
port = 8080

[terminal]
port = 2323

[logging]
level = "info"
"#,
            callsign,
            location.unwrap_or(""),
            callsign
        )
    }
    
    /// Generate a Docker configuration template
    pub fn docker_config(callsign: &str) -> String {
        format!(
            r#"# OffGridComm Docker Configuration

[node]
callsign = "{}"
description = "OffGridComm Docker Node"

[database]
path = "/data/offgridcomm.db"

[web]
host = "0.0.0.0"
port = 8080

[terminal]
host = "0.0.0.0"
port = 2323

[rf]
enabled = true
callsign = "{}"
device = "/dev/ttyUSB0"

[logging]
level = "info"
file = "/data/offgridcomm.log"
"#,
            callsign, callsign
        )
    }
    
    /// Generate a minimal configuration for testing
    pub fn test_config() -> String {
        r#"# OffGridComm Test Configuration

[node]
callsign = "W1TEST"
description = "Test Node"

[database]
path = ":memory:"

[rf]
enabled = false

[web]
port = 8080

[terminal]
port = 2323

[logging]
level = "debug"
console = true
"#.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;
    
    #[test]
    fn test_default_config_loading() {
        let loader = ConfigLoader::new();
        let settings = loader.load().unwrap();
        
        assert_eq!(settings.web.port, 8080);
        assert_eq!(settings.terminal.port, 2323);
        assert!(settings.database.path.ends_with("offgridcomm.db"));
    }
    
    #[test]
    fn test_callsign_validation() {
        use validation::validate_callsign;
        
        assert!(validate_callsign("W1AW").is_ok());
        assert!(validate_callsign("K2ABC").is_ok());
        assert!(validate_callsign("VE3XYZ").is_ok());
        assert!(validate_callsign("").is_ok()); // Empty is allowed
        
        assert!(validate_callsign("INVALID").is_err());
        assert!(validate_callsign("W1").is_err());
        assert!(validate_callsign("123ABC").is_err());
    }
    
    #[test]
    fn test_config_file_override() {
        let temp_dir = tempdir().unwrap();
        let config_path = temp_dir.path().join("test.toml");
        
        std::fs::write(&config_path, r#"
[web]
port = 9090

[node]
callsign = "W1TEST"
"#).unwrap();
        
        let loader = ConfigLoader::new().with_config_path(config_path);
        let settings = loader.load().unwrap();
        
        assert_eq!(settings.web.port, 9090);
        assert_eq!(settings.node.callsign, "W1TEST");
    }
    
    #[test]
    fn test_environment_override() {
        std::env::set_var("OGC_WEB_PORT", "7070");
        std::env::set_var("OGC_NODE_CALLSIGN", "W1ENV");
        
        let loader = ConfigLoader::new();
        let settings = loader.load().unwrap();
        
        assert_eq!(settings.web.port, 7070);
        assert_eq!(settings.node.callsign, "W1ENV");
        
        std::env::remove_var("OGC_WEB_PORT");
        std::env::remove_var("OGC_NODE_CALLSIGN");
    }
}
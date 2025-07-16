use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::time::Duration;

use crate::config::validation::{
    validate_callsign, validate_port, validate_compression, 
    validate_rf_protocol, validate_and_create_dir
};

/// Main configuration structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    /// Node identification and basic info
    pub node: NodeConfig,
    
    /// Data directory path (base for all data storage)
    pub data_dir: PathBuf,
    
    /// Database configuration
    pub database: DatabaseConfig,
    
    /// Message system configuration
    pub messaging: MessagingConfig,
    
    /// Network configuration
    pub network: NetworkConfig,
    
    /// RF interface configuration
    pub rf: RfConfig,
    
    /// Web interface configuration
    pub web: WebConfig,
    
    /// Terminal interface configuration
    pub terminal: TerminalConfig,
    
    /// Compression settings
    pub compression: CompressionConfig,
    
    /// Security settings
    pub security: SecurityConfig,
    
    /// Logging configuration
    pub logging: LoggingConfig,
    
    /// Performance settings
    pub performance: PerformanceConfig,
    
    /// Feature flags
    pub features: FeaturesConfig,
}

/// Node identification and metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeConfig {
    /// Amateur radio callsign (must be valid)
    pub callsign: String,
    
    /// Geographic location (grid square, city, etc.)
    pub location: String,
    
    /// Human-readable description
    pub description: String,
    
    /// Timezone for timestamps
    pub timezone: String,
    
    /// Node version (auto-populated)
    #[serde(default = "default_version")]
    pub version: String,
}

/// Database configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatabaseConfig {
    /// Database type (sqlite, postgres, etc.)
    #[serde(default = "default_db_type")]
    pub db_type: String,
    
    /// Database file path (relative to data_dir)
    pub path: PathBuf,
    
    /// Maximum number of connections
    #[serde(default = "default_max_connections")]
    pub max_connections: u32,
    
    /// Connection timeout in seconds
    #[serde(default = "default_connection_timeout")]
    pub connection_timeout: u64,
    
    /// Query timeout in seconds
    #[serde(default = "default_query_timeout")]
    pub query_timeout: u64,
    
    /// Enable WAL mode for SQLite
    #[serde(default = "default_wal_mode")]
    pub wal_mode: bool,
    
    /// Database backup interval (hours)
    #[serde(default = "default_backup_interval")]
    pub backup_interval: u64,
}

/// Message system configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessagingConfig {
    /// Maximum message size in bytes
    #[serde(default = "default_max_message_size")]
    pub max_message_size: usize,
    
    /// Maximum messages per user
    #[serde(default = "default_max_messages_per_user")]
    pub max_messages_per_user: u32,
    
    /// Message retention period in days
    #[serde(default = "default_message_retention_days")]
    pub message_retention_days: u32,
    
    /// Enable group/board messages
    #[serde(default = "default_true")]
    pub enable_groups: bool,
    
    /// Enable direct messages
    #[serde(default = "default_true")]
    pub enable_direct_messages: bool,
    
    /// Enable message threading
    #[serde(default = "default_true")]
    pub enable_threading: bool,
    
    /// Maximum thread depth
    #[serde(default = "default_max_thread_depth")]
    pub max_thread_depth: u32,
    
    /// Auto-prune old messages
    #[serde(default = "default_true")]
    pub auto_prune: bool,
}

/// Network communication configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkConfig {
    /// Unique node identifier (auto-generated if empty)
    pub node_id: String,
    
    /// Port for node-to-node communication
    #[serde(default = "default_network_port")]
    pub listen_port: u16,
    
    /// Maximum concurrent connections
    #[serde(default = "default_max_network_connections")]
    pub max_connections: u32,
    
    /// Connection timeout in seconds
    #[serde(default = "default_connection_timeout")]
    pub connection_timeout: u64,
    
    /// Heartbeat interval in seconds
    #[serde(default = "default_heartbeat_interval")]
    pub heartbeat_interval: u64,
    
    /// Sync interval in seconds
    #[serde(default = "default_sync_interval")]
    pub sync_interval: u64,
    
    /// Known peer nodes
    #[serde(default)]
    pub peers: Vec<PeerConfig>,
    
    /// Enable mesh networking
    #[serde(default = "default_true")]
    pub enable_mesh: bool,
    
    /// Maximum hops for message forwarding
    #[serde(default = "default_max_hops")]
    pub max_hops: u8,
}

/// Peer node configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeerConfig {
    /// Peer callsign
    pub callsign: String,
    
    /// Peer address (IP:port or hostname:port)
    pub address: String,
    
    /// Trust level (0-100)
    #[serde(default = "default_trust_level")]
    pub trust_level: u8,
    
    /// Auto-connect to this peer
    #[serde(default = "default_true")]
    pub auto_connect: bool,
}

/// RF interface configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RfConfig {
    /// Enable RF interface
    #[serde(default = "default_true")]
    pub enabled: bool,
    
    /// Interface type (serial, tcp, etc.)
    #[serde(default = "default_rf_interface")]
    pub interface: String,
    
    /// Device path or address
    #[serde(default = "default_rf_device")]
    pub device: String,
    
    /// Baud rate for serial connections
    #[serde(default = "default_baud_rate")]
    pub baud_rate: u32,
    
    /// RF protocol (ax25, kiss, vara, etc.)
    #[serde(default = "default_rf_protocol")]
    pub protocol: String,
    
    /// RF callsign (can be different from node callsign)
    pub callsign: String,
    
    /// SSID for RF transmissions
    #[serde(default = "default_ssid")]
    pub ssid: u8,
    
    /// AX.25 specific configuration
    pub ax25: Ax25Config,
    
    /// KISS TNC configuration
    pub kiss: KissConfig,
    
    /// TX delay in milliseconds
    #[serde(default = "default_tx_delay")]
    pub tx_delay: u16,
    
    /// Maximum packet size
    #[serde(default = "default_max_packet_size")]
    pub max_packet_size: usize,
}

/// AX.25 protocol configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Ax25Config {
    /// Digipeater path
    #[serde(default)]
    pub digipeater_path: Vec<String>,
    
    /// Beacon interval in seconds
    #[serde(default = "default_beacon_interval")]
    pub beacon_interval: u64,
    
    /// Beacon text
    #[serde(default = "default_beacon_text")]
    pub beacon_text: String,
    
    /// Enable position beacons
    #[serde(default = "default_false")]
    pub position_beacon: bool,
    
    /// Maximum retries for packet transmission
    #[serde(default = "default_max_retries")]
    pub max_retries: u8,
}

/// KISS TNC configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KissConfig {
    /// KISS port number
    #[serde(default = "default_kiss_port")]
    pub port: u8,
    
    /// TX delay parameter
    #[serde(default = "default_kiss_txdelay")]
    pub txdelay: u8,
    
    /// Persistence parameter
    #[serde(default = "default_kiss_persistence")]
    pub persistence: u8,
    
    /// Slot time parameter
    #[serde(default = "default_kiss_slottime")]
    pub slottime: u8,
    
    /// Duplex mode
    #[serde(default = "default_false")]
    pub duplex: bool,
}

/// Web interface configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebConfig {
    /// Enable web interface
    #[serde(default = "default_true")]
    pub enabled: bool,
    
    /// Bind address
    #[serde(default = "default_web_host")]
    pub host: String,
    
    /// HTTP port
    #[serde(default = "default_web_port")]
    pub port: u16,
    
    /// Static files directory
    #[serde(default = "default_static_dir")]
    pub static_dir: PathBuf,
    
    /// Session timeout in seconds
    #[serde(default = "default_session_timeout")]
    pub session_timeout: u64,
    
    /// Maximum upload size in bytes
    #[serde(default = "default_max_upload_size")]
    pub max_upload_size: usize,
    
    /// CORS allowed origins
    #[serde(default)]
    pub cors_origins: Vec<String>,
    
    /// Enable gzip compression
    #[serde(default = "default_true")]
    pub gzip_compression: bool,
    
    /// Rate limiting (requests per minute)
    #[serde(default = "default_rate_limit")]
    pub rate_limit: u32,
}

/// Terminal interface configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TerminalConfig {
    /// Enable terminal interface
    #[serde(default = "default_true")]
    pub enabled: bool,
    
    /// Bind address
    #[serde(default = "default_terminal_host")]
    pub host: String,
    
    /// Telnet port
    #[serde(default = "default_terminal_port")]
    pub port: u16,
    
    /// Maximum concurrent connections
    #[serde(default = "default_max_terminal_connections")]
    pub max_connections: u32,
    
    /// Idle timeout in seconds
    #[serde(default = "default_idle_timeout")]
    pub idle_timeout: u64,
    
    /// Welcome message
    #[serde(default = "default_welcome_message")]
    pub welcome_message: String,
    
    /// Enable ANSI colors
    #[serde(default = "default_true")]
    pub ansi_colors: bool,
    
    /// Enable mouse support
    #[serde(default = "default_true")]
    pub mouse_support: bool,
    
    /// Terminal width for layout
    #[serde(default = "default_terminal_width")]
    pub terminal_width: u16,
    
    /// Terminal height for layout
    #[serde(default = "default_terminal_height")]
    pub terminal_height: u16,
}

/// Compression configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompressionConfig {
    /// Enable compression
    #[serde(default = "default_true")]
    pub enabled: bool,
    
    /// Compression algorithm (lz4, zstd, gzip, none)
    #[serde(default = "default_compression_algorithm")]
    pub algorithm: String,
    
    /// Compression level (1-9)
    #[serde(default = "default_compression_level")]
    pub level: u8,
    
    /// Minimum size to compress (bytes)
    #[serde(default = "default_min_compression_size")]
    pub min_size: usize,
    
    /// Compress RF transmissions
    #[serde(default = "default_true")]
    pub compress_rf: bool,
    
    /// Compress database storage
    #[serde(default = "default_false")]
    pub compress_storage: bool,
}

/// Security configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityConfig {
    /// Minimum password length
    #[serde(default = "default_password_min_length")]
    pub password_min_length: usize,
    
    /// Session timeout in seconds
    #[serde(default = "default_session_timeout")]
    pub session_timeout: u64,
    
    /// Maximum login attempts before lockout
    #[serde(default = "default_max_login_attempts")]
    pub max_login_attempts: u8,
    
    /// Lockout duration in seconds
    #[serde(default = "default_lockout_duration")]
    pub lockout_duration: u64,
    
    /// Require callsign verification
    #[serde(default = "default_true")]
    pub require_callsign_verification: bool,
    
    /// Enable rate limiting
    #[serde(default = "default_true")]
    pub enable_rate_limiting: bool,
}

/// Logging configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoggingConfig {
    /// Log level (trace, debug, info, warn, error)
    #[serde(default = "default_log_level")]
    pub level: String,
    
    /// Log file path (relative to data_dir)
    #[serde(default = "default_log_file")]
    pub file: PathBuf,
    
    /// Maximum log file size in bytes
    #[serde(default = "default_max_log_size")]
    pub max_size: u64,
    
    /// Maximum number of log files to keep
    #[serde(default = "default_max_log_files")]
    pub max_files: u32,
    
    /// Enable console logging
    #[serde(default = "default_true")]
    pub console: bool,
    
    /// Enable structured JSON logging
    #[serde(default = "default_false")]
    pub json_format: bool,
}

/// Performance configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceConfig {
    /// Number of worker threads
    #[serde(default = "default_worker_threads")]
    pub worker_threads: usize,
    
    /// Maximum blocking threads
    #[serde(default = "default_max_blocking_threads")]
    pub max_blocking_threads: usize,
    
    /// Event loop interval in milliseconds
    #[serde(default = "default_event_interval")]
    pub event_interval: u64,
    
    /// Stack size for threads
    #[serde(default = "default_stack_size")]
    pub stack_size: usize,
    
    /// Buffer sizes for various components
    pub buffers: BufferConfig,
}

/// Buffer size configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BufferConfig {
    /// Network receive buffer size
    #[serde(default = "default_network_buffer")]
    pub network_rx: usize,
    
    /// Network transmit buffer size
    #[serde(default = "default_network_buffer")]
    pub network_tx: usize,
    
    /// RF receive buffer size
    #[serde(default = "default_rf_buffer")]
    pub rf_rx: usize,
    
    /// RF transmit buffer size
    #[serde(default = "default_rf_buffer")]
    pub rf_tx: usize,
    
    /// Message queue size
    #[serde(default = "default_message_buffer")]
    pub message_queue: usize,
}

/// Feature flags configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeaturesConfig {
    /// Enable web interface
    #[serde(default = "default_true")]
    pub web_interface: bool,
    
    /// Enable terminal interface
    #[serde(default = "default_true")]
    pub terminal_interface: bool,
    
    /// Enable RF interface
    #[serde(default = "default_true")]
    pub rf_interface: bool,
    
    /// Enable compression
    #[serde(default = "default_true")]
    pub compression: bool,
    
    /// Enable callsign validation
    #[serde(default = "default_true")]
    pub callsign_validation: bool,
    
    /// Enable message encryption (NOTE: Must be disabled for FCC compliance)
    #[serde(default = "default_false")]
    pub message_encryption: bool,
    
    /// Enable experimental features
    #[serde(default = "default_false")]
    pub experimental: bool,
}

impl Settings {
    /// Validate configuration and create required directories
    pub fn validate_and_create_dirs(&self) -> Result<()> {
        // Validate node callsign
        validate_callsign(&self.node.callsign)?;
        
        // Validate RF callsign
        if self.rf.enabled {
            validate_callsign(&self.rf.callsign)?;
        }
        
        // Validate ports
        if self.web.enabled {
            validate_port(self.web.port)?;
        }
        if self.terminal.enabled {
            validate_port(self.terminal.port)?;
        }
        validate_port(self.network.listen_port)?;
        
        // Validate compression
        validate_compression(&self.compression.algorithm)?;
        
        // Validate RF protocol
        if self.rf.enabled {
            validate_rf_protocol(&self.rf.protocol)?;
        }
        
        // Create required directories
        validate_and_create_dir(&self.data_dir)?;
        
        let db_dir = self.data_dir.join(&self.database.path).parent()
            .unwrap_or(&self.data_dir);
        validate_and_create_dir(db_dir)?;
        
        let log_dir = self.data_dir.join(&self.logging.file).parent()
            .unwrap_or(&self.data_dir);
        validate_and_create_dir(log_dir)?;
        
        let static_dir = if self.web.static_dir.is_relative() {
            self.data_dir.join(&self.web.static_dir)
        } else {
            self.web.static_dir.clone()
        };
        validate_and_create_dir(&static_dir)?;
        
        Ok(())
    }
    
    /// Get absolute path for database
    pub fn database_path(&self) -> PathBuf {
        if self.database.path.is_absolute() {
            self.database.path.clone()
        } else {
            self.data_dir.join(&self.database.path)
        }
    }
    
    /// Get absolute path for log file
    pub fn log_path(&self) -> PathBuf {
        if self.logging.file.is_absolute() {
            self.logging.file.clone()
        } else {
            self.data_dir.join(&self.logging.file)
        }
    }
    
    /// Get connection timeout as Duration
    pub fn connection_timeout(&self) -> Duration {
        Duration::from_secs(self.database.connection_timeout)
    }
    
    /// Get session timeout as Duration
    pub fn session_timeout(&self) -> Duration {
        Duration::from_secs(self.security.session_timeout)
    }
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            node: NodeConfig::default(),
            data_dir: PathBuf::from("./data"),
            database: DatabaseConfig::default(),
            messaging: MessagingConfig::default(),
            network: NetworkConfig::default(),
            rf: RfConfig::default(),
            web: WebConfig::default(),
            terminal: TerminalConfig::default(),
            compression: CompressionConfig::default(),
            security: SecurityConfig::default(),
            logging: LoggingConfig::default(),
            performance: PerformanceConfig::default(),
            features: FeaturesConfig::default(),
        }
    }
}

// Default value functions for serde
fn default_version() -> String { env!("CARGO_PKG_VERSION").to_string() }
fn default_db_type() -> String { "sqlite".to_string() }
fn default_max_connections() -> u32 { 10 }
fn default_connection_timeout() -> u64 { 30 }
fn default_query_timeout() -> u64 { 10 }
fn default_wal_mode() -> bool { true }
fn default_backup_interval() -> u64 { 24 }
fn default_max_message_size() -> usize { 1024 }
fn default_max_messages_per_user() -> u32 { 1000 }
fn default_message_retention_days() -> u32 { 30 }
fn default_max_thread_depth() -> u32 { 10 }
fn default_network_port() -> u16 { 14580 }
fn default_max_network_connections() -> u32 { 50 }
fn default_heartbeat_interval() -> u64 { 60 }
fn default_sync_interval() -> u64 { 300 }
fn default_max_hops() -> u8 { 7 }
fn default_trust_level() -> u8 { 50 }
fn default_rf_interface() -> String { "serial".to_string() }
fn default_rf_device() -> String { "/dev/ttyUSB0".to_string() }
fn default_baud_rate() -> u32 { 9600 }
fn default_rf_protocol() -> String { "ax25".to_string() }
fn default_ssid() -> u8 { 1 }
fn default_tx_delay() -> u16 { 300 }
fn default_max_packet_size() -> usize { 256 }
fn default_beacon_interval() -> u64 { 600 }
fn default_beacon_text() -> String { "OffGridComm Node".to_string() }
fn default_max_retries() -> u8 { 3 }
fn default_kiss_port() -> u8 { 0 }
fn default_kiss_txdelay() -> u8 { 30 }
fn default_kiss_persistence() -> u8 { 63 }
fn default_kiss_slottime() -> u8 { 10 }
fn default_web_host() -> String { "127.0.0.1".to_string() }
fn default_web_port() -> u16 { 8080 }
fn default_static_dir() -> PathBuf { PathBuf::from("./static") }
fn default_session_timeout() -> u64 { 3600 }
fn default_max_upload_size() -> usize { 1048576 }
fn default_rate_limit() -> u32 { 60 }
fn default_terminal_host() -> String { "127.0.0.1".to_string() }
fn default_terminal_port() -> u16 { 2323 }
fn default_max_terminal_connections() -> u32 { 10 }
fn default_idle_timeout() -> u64 { 1800 }
fn default_welcome_message() -> String { "Welcome to OffGridComm".to_string() }
fn default_terminal_width() -> u16 { 80 }
fn default_terminal_height() -> u16 { 24 }
fn default_compression_algorithm() -> String { "lz4".to_string() }
fn default_compression_level() -> u8 { 1 }
fn default_min_compression_size() -> usize { 128 }
fn default_password_min_length() -> usize { 8 }
fn default_max_login_attempts() -> u8 { 5 }
fn default_lockout_duration() -> u64 { 900 }
fn default_log_level() -> String { "info".to_string() }
fn default_log_file() -> PathBuf { PathBuf::from("offgridcomm.log") }
fn default_max_log_size() -> u64 { 10485760 }
fn default_max_log_files() -> u32 { 5 }
fn default_worker_threads() -> usize { 4 }
fn default_max_blocking_threads() -> usize { 512 }
fn default_event_interval() -> u64 { 100 }
fn default_stack_size() -> usize { 2097152 }
fn default_network_buffer() -> usize { 8192 }
fn default_rf_buffer() -> usize { 2048 }
fn default_message_buffer() -> usize { 1000 }
fn default_true() -> bool { true }
fn default_false() -> bool { false }

// Default implementations for nested structs
impl Default for NodeConfig {
    fn default() -> Self {
        Self {
            callsign: String::new(),
            location: String::new(),
            description: "OffGridComm Node".to_string(),
            timezone: "UTC".to_string(),
            version: default_version(),
        }
    }
}

impl Default for DatabaseConfig {
    fn default() -> Self {
        Self {
            db_type: default_db_type(),
            path: PathBuf::from("offgridcomm.db"),
            max_connections: default_max_connections(),
            connection_timeout: default_connection_timeout(),
            query_timeout: default_query_timeout(),
            wal_mode: default_wal_mode(),
            backup_interval: default_backup_interval(),
        }
    }
}

impl Default for MessagingConfig {
    fn default() -> Self {
        Self {
            max_message_size: default_max_message_size(),
            max_messages_per_user: default_max_messages_per_user(),
            message_retention_days: default_message_retention_days(),
            enable_groups: default_true(),
            enable_direct_messages: default_true(),
            enable_threading: default_true(),
            max_thread_depth: default_max_thread_depth(),
            auto_prune: default_true(),
        }
    }
}

impl Default for NetworkConfig {
    fn default() -> Self {
        Self {
            node_id: String::new(),
            listen_port: default_network_port(),
            max_connections: default_max_network_connections(),
            connection_timeout: default_connection_timeout(),
            heartbeat_interval: default_heartbeat_interval(),
            sync_interval: default_sync_interval(),
            peers: Vec::new(),
            enable_mesh: default_true(),
            max_hops: default_max_hops(),
        }
    }
}

impl Default for RfConfig {
    fn default() -> Self {
        Self {
            enabled: default_true(),
            interface: default_rf_interface(),
            device: default_rf_device(),
            baud_rate: default_baud_rate(),
            protocol: default_rf_protocol(),
            callsign: String::new(),
            ssid: default_ssid(),
            ax25: Ax25Config::default(),
            kiss: KissConfig::default(),
            tx_delay: default_tx_delay(),
            max_packet_size: default_max_packet_size(),
        }
    }
}

impl Default for Ax25Config {
    fn default() -> Self {
        Self {
            digipeater_path: Vec::new(),
            beacon_interval: default_beacon_interval(),
            beacon_text: default_beacon_text(),
            position_beacon: default_false(),
            max_retries: default_max_retries(),
        }
    }
}

impl Default for KissConfig {
    fn default() -> Self {
        Self {
            port: default_kiss_port(),
            txdelay: default_kiss_txdelay(),
            persistence: default_kiss_persistence(),
            slottime: default_kiss_slottime(),
            duplex: default_false(),
        }
    }
}

impl Default for WebConfig {
    fn default() -> Self {
        Self {
            enabled: default_true(),
            host: default_web_host(),
            port: default_web_port(),
            static_dir: default_static_dir(),
            session_timeout: default_session_timeout(),
            max_upload_size: default_max_upload_size(),
            cors_origins: Vec::new(),
            gzip_compression: default_true(),
            rate_limit: default_rate_limit(),
        }
    }
}

impl Default for TerminalConfig {
    fn default() -> Self {
        Self {
            enabled: default_true(),
            host: default_terminal_host(),
            port: default_terminal_port(),
            max_connections: default_max_terminal_connections(),
            idle_timeout: default_idle_timeout(),
            welcome_message: default_welcome_message(),
            ansi_colors: default_true(),
            mouse_support: default_true(),
            terminal_width: default_terminal_width(),
            terminal_height: default_terminal_height(),
        }
    }
}

impl Default for CompressionConfig {
    fn default() -> Self {
        Self {
            enabled: default_true(),
            algorithm: default_compression_algorithm(),
            level: default_compression_level(),
            min_size: default_min_compression_size(),
            compress_rf: default_true(),
            compress_storage: default_false(),
        }
    }
}

impl Default for SecurityConfig {
    fn default() -> Self {
        Self {
            password_min_length: default_password_min_length(),
            session_timeout: default_session_timeout(),
            max_login_attempts: default_max_login_attempts(),
            lockout_duration: default_lockout_duration(),
            require_callsign_verification: default_true(),
            enable_rate_limiting: default_true(),
        }
    }
}

impl Default for LoggingConfig {
    fn default() -> Self {
        Self {
            level: default_log_level(),
            file: default_log_file(),
            max_size: default_max_log_size(),
            max_files: default_max_log_files(),
            console: default_true(),
            json_format: default_false(),
        }
    }
}

impl Default for PerformanceConfig {
    fn default() -> Self {
        Self {
            worker_threads: default_worker_threads(),
            max_blocking_threads: default_max_blocking_threads(),
            event_interval: default_event_interval(),
            stack_size: default_stack_size(),
            buffers: BufferConfig::default(),
        }
    }
}

impl Default for BufferConfig {
    fn default() -> Self {
        Self {
            network_rx: default_network_buffer(),
            network_tx: default_network_buffer(),
            rf_rx: default_rf_buffer(),
            rf_tx: default_rf_buffer(),
            message_queue: default_message_buffer(),
        }
    }
}

impl Default for FeaturesConfig {
    fn default() -> Self {
        Self {
            web_interface: default_true(),
            terminal_interface: default_true(),
            rf_interface: default_true(),
            compression: default_true(),
            callsign_validation: default_true(),
            message_encryption: default_false(),
            experimental: default_false(),
        }
    }
}

impl Default for PeerConfig {
    fn default() -> Self {
        Self {
            callsign: String::new(),
            address: String::new(),
            trust_level: default_trust_level(),
            auto_connect: default_true(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;
    
    #[test]
    fn test_settings_default() {
        let settings = Settings::default();
        
        assert_eq!(settings.web.port, 8080);
        assert_eq!(settings.terminal.port, 2323);
        assert_eq!(settings.network.listen_port, 14580);
        assert_eq!(settings.compression.algorithm, "lz4");
        assert_eq!(settings.rf.protocol, "ax25");
        assert!(settings.features.web_interface);
        assert!(settings.features.terminal_interface);
        assert!(!settings.features.message_encryption); // Must be false for FCC compliance
    }
    
    #[test]
    fn test_settings_validation() {
        let temp_dir = tempdir().unwrap();
        let mut settings = Settings::default();
        settings.data_dir = temp_dir.path().to_path_buf();
        settings.node.callsign = "W1TEST".to_string();
        settings.rf.callsign = "W1TEST".to_string();
        
        assert!(settings.validate_and_create_dirs().is_ok());
    }
    
    #[test]
    fn test_invalid_callsign() {
        let temp_dir = tempdir().unwrap();
        let mut settings = Settings::default();
        settings.data_dir = temp_dir.path().to_path_buf();
        settings.node.callsign = "INVALID".to_string();
        
        assert!(settings.validate_and_create_dirs().is_err());
    }
    
    #[test]
    fn test_invalid_port() {
        let temp_dir = tempdir().unwrap();
        let mut settings = Settings::default();
        settings.data_dir = temp_dir.path().to_path_buf();
        settings.node.callsign = "W1TEST".to_string();
        settings.rf.callsign = "W1TEST".to_string();
        settings.web.port = 80; // Privileged port
        
        assert!(settings.validate_and_create_dirs().is_err());
    }
    
    #[test]
    fn test_path_resolution() {
        let temp_dir = tempdir().unwrap();
        let mut settings = Settings::default();
        settings.data_dir = temp_dir.path().to_path_buf();
        settings.database.path = PathBuf::from("test.db");
        settings.logging.file = PathBuf::from("logs/test.log");
        
        let db_path = settings.database_path();
        let log_path = settings.log_path();
        
        assert!(db_path.starts_with(&settings.data_dir));
        assert!(log_path.starts_with(&settings.data_dir));
        assert!(db_path.ends_with("test.db"));
        assert!(log_path.ends_with("logs/test.log"));
    }
    
    #[test]
    fn test_duration_conversion() {
        let settings = Settings::default();
        
        let conn_timeout = settings.connection_timeout();
        let session_timeout = settings.session_timeout();
        
        assert_eq!(conn_timeout.as_secs(), settings.database.connection_timeout);
        assert_eq!(session_timeout.as_secs(), settings.security.session_timeout);
    }
    
    #[test]
    fn test_compression_validation() {
        use crate::config::validation::validate_compression;
        
        assert!(validate_compression("lz4").is_ok());
        assert!(validate_compression("zstd").is_ok());
        assert!(validate_compression("gzip").is_ok());
        assert!(validate_compression("none").is_ok());
        assert!(validate_compression("invalid").is_err());
    }
    
    #[test]
    fn test_rf_protocol_validation() {
        use crate::config::validation::validate_rf_protocol;
        
        assert!(validate_rf_protocol("ax25").is_ok());
        assert!(validate_rf_protocol("kiss").is_ok());
        assert!(validate_rf_protocol("vara").is_ok());
        assert!(validate_rf_protocol("ardop").is_ok());
        assert!(validate_rf_protocol("invalid").is_err());
    }
    
    #[test]
    fn test_serialization() {
        let settings = Settings::default();
        
        // Test TOML serialization
        let toml_str = toml::to_string(&settings).unwrap();
        let deserialized: Settings = toml::from_str(&toml_str).unwrap();
        
        assert_eq!(settings.web.port, deserialized.web.port);
        assert_eq!(settings.node.description, deserialized.node.description);
    }
    
    #[test]
    fn test_feature_flags() {
        let mut settings = Settings::default();
        
        // Test disabling features
        settings.features.web_interface = false;
        settings.features.rf_interface = false;
        
        assert!(!settings.features.web_interface);
        assert!(!settings.features.rf_interface);
        assert!(settings.features.terminal_interface);
        
        // Ensure encryption is disabled by default (FCC compliance)
        assert!(!settings.features.message_encryption);
    }
    
    #[test]
    fn test_peer_config() {
        let peer = PeerConfig {
            callsign: "W1XYZ".to_string(),
            address: "192.168.1.100:14580".to_string(),
            trust_level: 75,
            auto_connect: true,
        };
        
        assert_eq!(peer.callsign, "W1XYZ");
        assert_eq!(peer.trust_level, 75);
        assert!(peer.auto_connect);
    }
    
    #[test]
    fn test_buffer_sizes() {
        let settings = Settings::default();
        
        assert_eq!(settings.performance.buffers.network_rx, 8192);
        assert_eq!(settings.performance.buffers.rf_rx, 2048);
        assert_eq!(settings.performance.buffers.message_queue, 1000);
    }
    
    #[test]
    fn test_ax25_config() {
        let mut ax25 = Ax25Config::default();
        ax25.digipeater_path = vec!["WIDE1-1".to_string(), "WIDE2-1".to_string()];
        ax25.beacon_interval = 1200;
        ax25.position_beacon = true;
        
        assert_eq!(ax25.digipeater_path.len(), 2);
        assert_eq!(ax25.beacon_interval, 1200);
        assert!(ax25.position_beacon);
    }
    
    #[test]
    fn test_kiss_config() {
        let mut kiss = KissConfig::default();
        kiss.port = 1;
        kiss.txdelay = 50;
        kiss.duplex = true;
        
        assert_eq!(kiss.port, 1);
        assert_eq!(kiss.txdelay, 50);
        assert!(kiss.duplex);
    }
}
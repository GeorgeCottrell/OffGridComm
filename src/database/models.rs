use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::SystemTime;
use uuid::Uuid;

/// Message types for different communication purposes
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash, sqlx::Type)]
#[repr(i32)]
pub enum MessageType {
    /// Direct person-to-person message
    Direct = 0,
    /// Group/board message
    Group = 1,
    /// System notification message
    System = 2,
    /// Emergency message (highest priority)
    Emergency = 3,
    /// Public bulletin message
    Bulletin = 4,
}

/// Message priority levels
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash, sqlx::Type)]
#[repr(i32)]
pub enum MessagePriority {
    /// Low priority message
    Low = 0,
    /// Normal priority message
    Normal = 1,
    /// High priority message
    High = 2,
    /// Emergency priority message
    Emergency = 3,
}

/// Message delivery and read status
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash, sqlx::Type)]
#[repr(i32)]
pub enum MessageStatus {
    /// Message pending delivery
    Pending = 0,
    /// Message sent successfully
    Sent = 1,
    /// Message delivered to recipient
    Delivered = 2,
    /// Message read by recipient
    Read = 3,
    /// Message failed to deliver
    Failed = 4,
    /// Message deleted
    Deleted = 5,
}

/// Core message structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    /// Unique message identifier
    pub id: Uuid,
    
    /// Type of message
    pub message_type: MessageType,
    
    /// ID of user who sent the message
    pub sender_id: Uuid,
    
    /// Callsign of sender
    pub sender_callsign: String,
    
    /// ID of recipient user (for direct messages)
    pub recipient_id: Option<Uuid>,
    
    /// Callsign of recipient (for direct messages)
    pub recipient_callsign: Option<String>,
    
    /// ID of group (for group messages)
    pub group_id: Option<Uuid>,
    
    /// Message subject/title (optional)
    pub subject: Option<String>,
    
    /// Message body content
    pub body: String,
    
    /// Message priority level
    pub priority: MessagePriority,
    
    /// When message was created
    pub created_at: SystemTime,
    
    /// When message was delivered (if applicable)
    pub delivered_at: Option<SystemTime>,
    
    /// When message was read (if applicable)
    pub read_at: Option<SystemTime>,
    
    /// Parent message ID (for replies)
    pub parent_id: Option<Uuid>,
    
    /// Thread ID for conversation grouping
    pub thread_id: Option<Uuid>,
    
    /// Current message status
    pub status: MessageStatus,
    
    /// ID of originating node
    pub origin_node_id: Uuid,
    
    /// Hash for deduplication
    pub message_hash: String,
    
    /// RF transmission metadata
    pub rf_metadata: Option<RfMetadata>,
    
    /// Message flags
    pub flags: MessageFlags,
}

/// RF transmission metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RfMetadata {
    /// Transmission frequency
    pub frequency: Option<f64>,
    
    /// Signal-to-noise ratio
    pub snr: Option<f32>,
    
    /// Received signal strength indicator
    pub rssi: Option<f32>,
    
    /// Transmission mode (PSK31, VARA, etc.)
    pub mode: Option<String>,
    
    /// Bandwidth used
    pub bandwidth: Option<u32>,
    
    /// Error correction info
    pub error_correction: Option<String>,
    
    /// Number of transmission attempts
    pub retry_count: Option<u8>,
    
    /// Path taken through network
    pub hop_path: Vec<String>,
}

/// Message flags for special handling
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageFlags {
    /// Message requires acknowledgment
    pub requires_ack: bool,
    
    /// Message is compressed
    pub compressed: bool,
    
    /// Message is important/urgent
    pub important: bool,
    
    /// Message is part of emergency traffic
    pub emergency_traffic: bool,
    
    /// Message should be retained longer
    pub retain_longer: bool,
    
    /// Message was auto-generated
    pub auto_generated: bool,
}

impl Default for MessageFlags {
    fn default() -> Self {
        Self {
            requires_ack: false,
            compressed: false,
            important: false,
            emergency_traffic: false,
            retain_longer: false,
            auto_generated: false,
        }
    }
}

/// Group types for different purposes
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash, sqlx::Type)]
#[repr(i32)]
pub enum GroupType {
    /// Public group (anyone can join)
    Public = 0,
    /// Private group (invitation only)
    Private = 1,
    /// Announcement group (read-only for most members)
    Announcement = 2,
    /// Emergency group (for emergency communications)
    Emergency = 3,
    /// Local area group
    Local = 4,
}

/// Group status
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, sqlx::Type)]
#[repr(i32)]
pub enum GroupStatus {
    /// Active group
    Active = 0,
    /// Archived group
    Archived = 1,
    /// Suspended group
    Suspended = 2,
    /// Deleted group
    Deleted = 3,
}

/// Group/Board structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Group {
    /// Unique group identifier
    pub id: Uuid,
    
    /// Group name
    pub name: String,
    
    /// Group description
    pub description: Option<String>,
    
    /// Type of group
    pub group_type: GroupType,
    
    /// ID of user who created the group
    pub created_by: Uuid,
    
    /// When group was created
    pub created_at: SystemTime,
    
    /// Current group status
    pub status: GroupStatus,
    
    /// Group settings
    pub settings: GroupSettings,
    
    /// Group statistics
    pub stats: GroupStats,
}

/// Group configuration settings
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroupSettings {
    /// Maximum number of members (None = unlimited)
    pub max_members: Option<u32>,
    
    /// Require approval to join
    pub require_approval: bool,
    
    /// Group is read-only
    pub read_only: bool,
    
    /// Allow message threading
    pub allow_threading: bool,
    
    /// Message types allowed in this group
    pub allowed_message_types: Vec<MessageType>,
    
    /// Auto-prune messages older than days (None = no auto-pruning)
    pub auto_prune_days: Option<u32>,
    
    /// Welcome message for new members
    pub welcome_message: Option<String>,
}

impl Default for GroupSettings {
    fn default() -> Self {
        Self {
            max_members: None,
            require_approval: false,
            read_only: false,
            allow_threading: true,
            allowed_message_types: vec![MessageType::Group, MessageType::Bulletin],
            auto_prune_days: Some(30), // Default 30-day retention
            welcome_message: None,
        }
    }
}

/// Group statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroupStats {
    /// Total number of members
    pub member_count: u32,
    
    /// Total number of messages
    pub message_count: u64,
    
    /// Last message timestamp
    pub last_message_at: Option<SystemTime>,
    
    /// Most active member callsign
    pub most_active_member: Option<String>,
    
    /// Messages per day average
    pub daily_message_average: f64,
}

impl Default for GroupStats {
    fn default() -> Self {
        Self {
            member_count: 0,
            message_count: 0,
            last_message_at: None,
            most_active_member: None,
            daily_message_average: 0.0,
        }
    }
}

/// Group member roles
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, sqlx::Type)]
#[repr(i32)]
pub enum GroupRole {
    /// Regular member
    Member = 0,
    /// Group moderator
    Moderator = 1,
    /// Group administrator
    Administrator = 2,
    /// Group owner
    Owner = 3,
}

impl GroupRole {
    /// Check if this role can moderate the group
    pub fn can_moderate(&self) -> bool {
        matches!(self, GroupRole::Moderator | GroupRole::Administrator | GroupRole::Owner)
    }
    
    /// Check if this role can administer the group
    pub fn can_admin(&self) -> bool {
        matches!(self, GroupRole::Administrator | GroupRole::Owner)
    }
    
    /// Check if this role is owner
    pub fn is_owner(&self) -> bool {
        matches!(self, GroupRole::Owner)
    }
}

/// Group membership status
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, sqlx::Type)]
#[repr(i32)]
pub enum MembershipStatus {
    /// Active member
    Active = 0,
    /// Pending approval
    Pending = 1,
    /// Banned from group
    Banned = 2,
    /// Left the group
    Left = 3,
}

/// Group membership information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroupMembership {
    /// Group ID
    pub group_id: Uuid,
    
    /// User ID
    pub user_id: Uuid,
    
    /// User callsign
    pub user_callsign: String,
    
    /// Member role in group
    pub role: GroupRole,
    
    /// When user joined group
    pub joined_at: SystemTime,
    
    /// Last activity in group
    pub last_activity: Option<SystemTime>,
    
    /// Membership status
    pub status: MembershipStatus,
    
    /// Who invited this member
    pub invited_by: Option<Uuid>,
    
    /// Member-specific settings
    pub settings: MemberSettings,
}

/// Member-specific settings
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemberSettings {
    /// Receive notifications for this group
    pub notifications_enabled: bool,
    
    /// Email notifications
    pub email_notifications: bool,
    
    /// Custom nickname in this group
    pub nickname: Option<String>,
    
    /// Mute notifications until
    pub muted_until: Option<SystemTime>,
}

impl Default for MemberSettings {
    fn default() -> Self {
        Self {
            notifications_enabled: true,
            email_notifications: false,
            nickname: None,
            muted_until: None,
        }
    }
}

/// Network node status
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, sqlx::Type)]
#[repr(i32)]
pub enum NodeStatus {
    /// Node is offline
    Offline = 0,
    /// Node is online and reachable
    Online = 1,
    /// Node is in maintenance mode
    Maintenance = 2,
    /// Node is unreachable
    Unreachable = 3,
    /// Unknown status
    Unknown = -1,
}

/// Network node information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkNode {
    /// Unique node identifier
    pub id: Uuid,
    
    /// Node callsign
    pub callsign: String,
    
    /// Node description
    pub description: String,
    
    /// Geographic location
    pub location: Option<String>,
    
    /// Operator callsign
    pub operator_callsign: Option<String>,
    
    /// When node was first seen
    pub created_at: SystemTime,
    
    /// Last time node was seen
    pub last_seen: SystemTime,
    
    /// Current node status
    pub status: NodeStatus,
    
    /// Node software version
    pub version: String,
    
    /// Network configuration
    pub network_config: NetworkConfig,
    
    /// Node capabilities
    pub capabilities: NodeCapabilities,
    
    /// Node statistics
    pub stats: NodeStats,
}

/// Network configuration for a node
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkConfig {
    /// Supported protocols
    pub protocols: Vec<String>,
    
    /// RF interfaces available
    pub rf_interfaces: Vec<RfInterface>,
    
    /// Network interfaces
    pub network_interfaces: Vec<NetworkInterface>,
    
    /// Maximum hop count
    pub max_hops: u8,
    
    /// Sync interval in seconds
    pub sync_interval: u64,
    
    /// Compression enabled
    pub compression_enabled: bool,
}

impl Default for NetworkConfig {
    fn default() -> Self {
        Self {
            protocols: vec!["AX.25".to_string(), "TCP/IP".to_string()],
            rf_interfaces: Vec::new(),
            network_interfaces: Vec::new(),
            max_hops: 7,
            sync_interval: 300,
            compression_enabled: true,
        }
    }
}

/// RF interface information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RfInterface {
    /// Interface name
    pub name: String,
    
    /// Device path or identifier
    pub device: String,
    
    /// Operating frequency
    pub frequency: f64,
    
    /// Transmission mode
    pub mode: String,
    
    /// Baud rate
    pub baud_rate: u32,
    
    /// Maximum power output (watts)
    pub power: f32,
    
    /// Current signal quality
    pub signal_quality: Option<SignalQuality>,
}

/// Network interface information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkInterface {
    /// Interface name
    pub name: String,
    
    /// IP address
    pub ip_address: String,
    
    /// Port number
    pub port: u16,
    
    /// Interface type (TCP, UDP, etc.)
    pub interface_type: String,
}

/// Signal quality metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignalQuality {
    /// Signal strength (dBm)
    pub signal_strength: f32,
    
    /// Signal-to-noise ratio (dB)
    pub snr: f32,
    
    /// Bit error rate
    pub ber: f32,
    
    /// Last measured
    pub measured_at: SystemTime,
}

/// Node capabilities
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeCapabilities {
    /// Supported message types
    pub message_types: Vec<MessageType>,
    
    /// Maximum message size
    pub max_message_size: usize,
    
    /// Compression algorithms supported
    pub compression_algorithms: Vec<String>,
    
    /// RF modes supported
    pub rf_modes: Vec<String>,
    
    /// Can act as relay
    pub relay_capable: bool,
    
    /// Can store and forward messages
    pub store_forward: bool,
}

impl Default for NodeCapabilities {
    fn default() -> Self {
        Self {
            message_types: vec![
                MessageType::Direct,
                MessageType::Group,
                MessageType::System,
                MessageType::Emergency,
                MessageType::Bulletin,
            ],
            max_message_size: 1024,
            compression_algorithms: vec!["lz4".to_string(), "zstd".to_string()],
            rf_modes: vec!["AX.25".to_string(), "PSK31".to_string()],
            relay_capable: true,
            store_forward: true,
        }
    }
}

/// Node performance statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeStats {
    /// Total messages sent
    pub messages_sent: u64,
    
    /// Total messages received
    pub messages_received: u64,
    
    /// Total bytes transmitted
    pub bytes_transmitted: u64,
    
    /// Total bytes received
    pub bytes_received: u64,
    
    /// Average transmission rate (bytes/sec)
    pub avg_tx_rate: f64,
    
    /// Average reception rate (bytes/sec)
    pub avg_rx_rate: f64,
    
    /// Number of connected peers
    pub connected_peers: u32,
    
    /// Uptime in seconds
    pub uptime_seconds: u64,
    
    /// Last statistics update
    pub last_updated: SystemTime,
}

impl Default for NodeStats {
    fn default() -> Self {
        Self {
            messages_sent: 0,
            messages_received: 0,
            bytes_transmitted: 0,
            bytes_received: 0,
            avg_tx_rate: 0.0,
            avg_rx_rate: 0.0,
            connected_peers: 0,
            uptime_seconds: 0,
            last_updated: SystemTime::now(),
        }
    }
}

/// System statistics for monitoring
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemStats {
    /// Node information
    pub node: NetworkNode,
    
    /// Database statistics
    pub database: DatabaseStats,
    
    /// Network statistics
    pub network: NetworkStats,
    
    /// RF statistics
    pub rf: RfStats,
    
    /// System resource usage
    pub resources: ResourceStats,
}

/// Database statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatabaseStats {
    /// Total number of messages
    pub total_messages: u64,
    
    /// Total number of users
    pub total_users: u64,
    
    /// Total number of groups
    pub total_groups: u64,
    
    /// Database size in bytes
    pub size_bytes: u64,
    
    /// Messages by type
    pub messages_by_type: HashMap<MessageType, u64>,
    
    /// Messages by status
    pub messages_by_status: HashMap<MessageStatus, u64>,
    
    /// Last pruning operation
    pub last_pruned: Option<SystemTime>,
}

/// Network statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkStats {
    /// Number of connected nodes
    pub connected_nodes: u32,
    
    /// Number of nodes in sync
    pub nodes_in_sync: u32,
    
    /// Number of nodes out of sync
    pub nodes_out_of_sync: u32,
    
    /// Total network traffic (bytes)
    pub total_traffic_bytes: u64,
    
    /// Current transmission rate (bytes/sec)
    pub current_tx_rate: f64,
    
    /// Current reception rate (bytes/sec)
    pub current_rx_rate: f64,
    
    /// Network latency (ms)
    pub avg_latency_ms: f64,
    
    /// Packet loss rate (%)
    pub packet_loss_rate: f32,
}

/// RF statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RfStats {
    /// Active RF interfaces
    pub active_interfaces: Vec<RfInterface>,
    
    /// Total RF transmissions
    pub total_transmissions: u64,
    
    /// Total RF receptions
    pub total_receptions: u64,
    
    /// Current frequency usage
    pub frequency_usage: HashMap<String, f64>,
    
    /// Average signal quality
    pub avg_signal_quality: Option<SignalQuality>,
    
    /// Transmission errors
    pub transmission_errors: u64,
    
    /// Reception errors
    pub reception_errors: u64,
}

/// System resource statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceStats {
    /// CPU usage percentage
    pub cpu_usage: f32,
    
    /// Memory usage in bytes
    pub memory_usage: u64,
    
    /// Total memory in bytes
    pub memory_total: u64,
    
    /// Disk usage in bytes
    pub disk_usage: u64,
    
    /// Total disk space in bytes
    pub disk_total: u64,
    
    /// Network bandwidth usage
    pub network_bandwidth: f64,
    
    /// System load average
    pub load_average: f32,
    
    /// System uptime in seconds
    pub uptime: u64,
}

/// User profile for public pages
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserProfile {
    /// User ID
    pub user_id: Uuid,
    
    /// Callsign
    pub callsign: String,
    
    /// Display name
    pub display_name: Option<String>,
    
    /// Location information
    pub location: Option<String>,
    
    /// Bio/description
    pub bio: Option<String>,
    
    /// Contact information
    pub contact_info: ContactInfo,
    
    /// Public statistics
    pub stats: UserStats,
    
    /// Recent activity
    pub recent_activity: Vec<ActivityEntry>,
    
    /// Profile settings
    pub settings: ProfileSettings,
}

/// Contact information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContactInfo {
    /// Email address (if public)
    pub email: Option<String>,
    
    /// QTH (location)
    pub qth: Option<String>,
    
    /// Grid square
    pub grid_square: Option<String>,
    
    /// Other contact methods
    pub other_contacts: HashMap<String, String>,
}

/// User statistics for public view
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserStats {
    /// Total messages sent
    pub messages_sent: u64,
    
    /// Total messages received
    pub messages_received: u64,
    
    /// Groups joined
    pub groups_joined: u32,
    
    /// Member since
    pub member_since: SystemTime,
    
    /// Last activity
    pub last_activity: Option<SystemTime>,
    
    /// Online status
    pub online: bool,
}

/// Activity entry for user activity feed
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActivityEntry {
    /// Activity ID
    pub id: Uuid,
    
    /// Activity type
    pub activity_type: ActivityType,
    
    /// Activity description
    pub description: String,
    
    /// When activity occurred
    pub timestamp: SystemTime,
    
    /// Related entity ID (message, group, etc.)
    pub entity_id: Option<Uuid>,
    
    /// Additional activity data
    pub metadata: HashMap<String, serde_json::Value>,
}

/// Types of user activities
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum ActivityType {
    /// Posted a message
    MessagePosted,
    /// Joined a group
    GroupJoined,
    /// Created a group
    GroupCreated,
    /// Profile updated
    ProfileUpdated,
    /// Came online
    CameOnline,
    /// Went offline
    WentOffline,
}

/// Profile settings for privacy control
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfileSettings {
    /// Profile is public
    pub public: bool,
    
    /// Show online status
    pub show_online_status: bool,
    
    /// Show activity feed
    pub show_activity: bool,
    
    /// Show contact information
    pub show_contact_info: bool,
    
    /// Show statistics
    pub show_stats: bool,
    
    /// Allow direct messages from anyone
    pub allow_dm_from_anyone: bool,
}

impl Default for ProfileSettings {
    fn default() -> Self {
        Self {
            public: true,
            show_online_status: true,
            show_activity: true,
            show_contact_info: false,
            show_stats: true,
            allow_dm_from_anyone: true,
        }
    }
}

/// Setup configuration for initial node setup
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SetupConfig {
    /// Node operator information
    pub operator: OperatorInfo,
    
    /// RF hardware configuration
    pub rf_config: RfSetupConfig,
    
    /// Network configuration
    pub network_config: NetworkSetupConfig,
    
    /// Initial groups to create
    pub initial_groups: Vec<String>,
    
    /// Setup completion status
    pub completed: bool,
    
    /// Setup timestamp
    pub setup_at: SystemTime,
}

/// Operator information for setup
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OperatorInfo {
    /// Amateur radio callsign
    pub callsign: String,
    
    /// Full name
    pub full_name: String,
    
    /// Physical address
    pub address: Address,
    
    /// License class
    pub license_class: Option<String>,
    
    /// Contact information
    pub contact: ContactInfo,
}

/// Physical address
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Address {
    /// Street address
    pub street: String,
    
    /// City
    pub city: String,
    
    /// State/province
    pub state: String,
    
    /// Postal/ZIP code
    pub postal_code: String,
    
    /// Country
    pub country: String,
    
    /// Grid square (amateur radio)
    pub grid_square: Option<String>,
}

/// RF setup configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RfSetupConfig {
    /// Detected RF interfaces
    pub detected_interfaces: Vec<DetectedRfInterface>,
    
    /// Selected interface for use
    pub selected_interface: Option<String>,
    
    /// Auto-detected protocols
    pub detected_protocols: Vec<String>,
    
    /// Selected protocol
    pub selected_protocol: Option<String>,
}

/// Detected RF interface during setup
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DetectedRfInterface {
    /// Device path
    pub device: String,
    
    /// Device description
    pub description: String,
    
    /// Supported modes
    pub supported_modes: Vec<String>,
    
    /// Recommended settings
    pub recommended_settings: HashMap<String, serde_json::Value>,
}

/// Network setup configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkSetupConfig {
    /// Node description
    pub node_description: String,
    
    /// Enable mesh networking
    pub enable_mesh: bool,
    
    /// Enable RF interface
    pub enable_rf: bool,
    
    /// Enable web interface
    pub enable_web: bool,
    
    /// Enable terminal interface
    pub enable_terminal: bool,
    
    /// Compression settings
    pub compression_enabled: bool,
}

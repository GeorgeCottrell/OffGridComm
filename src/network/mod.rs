use anyhow::Result;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, SystemTime};
use tokio::sync::RwLock;
use uuid::Uuid;

pub mod protocol;
pub mod rf;
pub mod sync;
pub mod compression;
pub mod delta;

use crate::config::NetworkConfig;
use crate::database::Database;
use crate::messaging::MessageSystem;

/// Network manager coordinates all network communications
#[derive(Clone)]
pub struct NetworkManager {
    /// Database for node storage
    database: Database,
    
    /// Message system for routing
    message_system: MessageSystem,
    
    /// Network configuration
    config: NetworkConfig,
    
    /// Connected peers
    peers: Arc<RwLock<HashMap<Uuid, PeerConnection>>>,
    
    /// RF interface (if enabled)
    rf_interface: Option<rf::RfInterface>,
    
    /// Protocol handler
    protocol: protocol::ProtocolHandler,
    
    /// Sync manager
    sync_manager: sync::SyncManager,
    
    /// Compression handler
    compression: compression::CompressionHandler,
    
    /// Network statistics
    stats: Arc<RwLock<NetworkStats>>,
    
    /// Node ID
    node_id: Uuid,
    
    /// Enable RF communications
    rf_enabled: bool,
}

/// Peer connection information
#[derive(Debug, Clone)]
pub struct PeerConnection {
    /// Peer node ID
    pub node_id: Uuid,
    
    /// Peer callsign
    pub callsign: String,
    
    /// Connection type
    pub connection_type: ConnectionType,
    
    /// Connection established time
    pub connected_at: SystemTime,
    
    /// Last activity time
    pub last_activity: SystemTime,
    
    /// Connection status
    pub status: ConnectionStatus,
    
    /// Peer capabilities
    pub capabilities: PeerCapabilities,
    
    /// Connection quality metrics
    pub quality: ConnectionQuality,
}

/// Connection types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectionType {
    /// TCP/IP connection
    Internet,
    
    /// RF connection via AX.25
    Ax25,
    
    /// RF connection via VARA
    Vara,
    
    /// RF connection via other protocol
    RfOther,
    
    /// Local network connection
    Local,
}

/// Connection status
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectionStatus {
    /// Connection is active
    Connected,
    
    /// Connection is being established
    Connecting,
    
    /// Connection lost, attempting reconnect
    Reconnecting,
    
    /// Connection failed
    Failed,
    
    /// Deliberately disconnected
    Disconnected,
}

/// Peer capabilities
#[derive(Debug, Clone)]
pub struct PeerCapabilities {
    /// Supported protocols
    pub protocols: Vec<String>,
    
    /// Maximum message size
    pub max_message_size: usize,
    
    /// Supports compression
    pub compression: bool,
    
    /// Supports store-and-forward
    pub store_forward: bool,
    
    /// RF interfaces available
    pub rf_interfaces: Vec<String>,
    
    /// Software version
    pub version: String,
}

/// Connection quality metrics
#[derive(Debug, Clone)]
pub struct ConnectionQuality {
    /// Round-trip time (ms)
    pub rtt_ms: f64,
    
    /// Packet loss rate (0.0-1.0)
    pub packet_loss: f64,
    
    /// Bandwidth estimate (bytes/sec)
    pub bandwidth_bps: f64,
    
    /// Signal quality (RF only, dB)
    pub signal_db: Option<f64>,
    
    /// Connection reliability (0.0-1.0)
    pub reliability: f64,
}

/// Network statistics
#[derive(Debug, Clone)]
pub struct NetworkStats {
    /// Total messages sent
    pub messages_sent: u64,
    
    /// Total messages received
    pub messages_received: u64,
    
    /// Total bytes transmitted
    pub bytes_transmitted: u64,
    
    /// Total bytes received
    pub bytes_received: u64,
    
    /// Currently connected peers
    pub connected_peers: usize,
    
    /// Total connection attempts
    pub connection_attempts: u64,
    
    /// Successful connections
    pub successful_connections: u64,
    
    /// Failed connections
    pub failed_connections: u64,
    
    /// Average message latency (ms)
    pub avg_latency_ms: f64,
    
    /// Network uptime
    pub uptime: Duration,
    
    /// Last statistics update
    pub last_updated: SystemTime,
}

impl NetworkManager {
    /// Create a new network manager
    pub async fn new(
        database: Database,
        message_system: MessageSystem,
        config: &NetworkConfig,
        rf_enabled: bool,
    ) -> Result<Self> {
        let node_id = if config.node_id.is_empty() {
            Uuid::new_v4()
        } else {
            Uuid::parse_str(&config.node_id)?
        };
        
        let rf_interface = if rf_enabled {
            Some(rf::RfInterface::new(&config).await?)
        } else {
            None
        };
        
        let protocol = protocol::ProtocolHandler::new();
        let sync_manager = sync::SyncManager::new(database.clone()).await?;
        let compression = compression::CompressionHandler::new();
        
        Ok(Self {
            database,
            message_system,
            config: config.clone(),
            peers: Arc::new(RwLock::new(HashMap::new())),
            rf_interface,
            protocol,
            sync_manager,
            compression,
            stats: Arc::new(RwLock::new(NetworkStats {
                messages_sent: 0,
                messages_received: 0,
                bytes_transmitted: 0,
                bytes_received: 0,
                connected_peers: 0,
                connection_attempts: 0,
                successful_connections: 0,
                failed_connections: 0,
                avg_latency_ms: 0.0,
                uptime: Duration::from_secs(0),
                last_updated: SystemTime::now(),
            })),
            node_id,
            rf_enabled,
        })
    }
    
    /// Start the network manager
    pub async fn run(&self) -> Result<()> {
        tracing::info!("Starting network manager for node {}", self.node_id);
        
        let start_time = SystemTime::now();
        
        // Start RF interface if enabled
        if let Some(rf) = &self.rf_interface {
            let rf_handle = {
                let rf = rf.clone();
                let manager = self.clone();
                tokio::spawn(async move {
                    if let Err(e) = rf.run().await {
                        tracing::error!("RF interface error: {}", e);
                    }
                })
            };
            
            // Start message handler
            let message_handle = {
                let manager = self.clone();
                tokio::spawn(async move {
                    manager.message_handler().await;
                })
            };
            
            // Start peer management
            let peer_handle = {
                let manager = self.clone();
                tokio::spawn(async move {
                    manager.peer_manager().await;
                })
            };
            
            // Start sync manager
            let sync_handle = {
                let sync_manager = self.sync_manager.clone();
                tokio::spawn(async move {
                    if let Err(e) = sync_manager.run().await {
                        tracing::error!("Sync manager error: {}", e);
                    }
                })
            };
            
            // Start statistics updater
            let stats_handle = {
                let manager = self.clone();
                tokio::spawn(async move {
                    manager.update_statistics_loop(start_time).await;
                })
            };
            
            // Wait for any task to complete (which shouldn't happen in normal operation)
            tokio::select! {
                _ = rf_handle => tracing::warn!("RF interface task completed"),
                _ = message_handle => tracing::warn!("Message handler task completed"),
                _ = peer_handle => tracing::warn!("Peer manager task completed"),
                _ = sync_handle => tracing::warn!("Sync manager task completed"),
                _ = stats_handle => tracing::warn!("Statistics updater task completed"),
            }
        } else {
            tracing::info!("RF interface disabled, running in IP-only mode");
            
            // Start IP-only services
            let message_handle = {
                let manager = self.clone();
                tokio::spawn(async move {
                    manager.message_handler().await;
                })
            };
            
            let stats_handle = {
                let manager = self.clone();
                tokio::spawn(async move {
                    manager.update_statistics_loop(start_time).await;
                })
            };
            
            tokio::select! {
                _ = message_handle => tracing::warn!("Message handler task completed"),
                _ = stats_handle => tracing::warn!("Statistics updater task completed"),
            }
        }
        
        Ok(())
    }
    
    /// Handle incoming messages
    async fn message_handler(&self) {
        let mut interval = tokio::time::interval(Duration::from_millis(100));
        
        loop {
            interval.tick().await;
            
            // Process pending messages
            if let Err(e) = self.process_pending_messages().await {
                tracing::error!("Error processing messages: {}", e);
            }
        }
    }
    
    /// Manage peer connections
    async fn peer_manager(&self) {
        let mut interval = tokio::time::interval(Duration::from_secs(30));
        
        loop {
            interval.tick().await;
            
            // Check peer health
            if let Err(e) = self.check_peer_health().await {
                tracing::error!("Error checking peer health: {}", e);
            }
            
            // Attempt connections to configured peers
            if let Err(e) = self.connect_to_peers().await {
                tracing::error!("Error connecting to peers: {}", e);
            }
        }
    }
    
    /// Update statistics periodically
    async fn update_statistics_loop(&self, start_time: SystemTime) {
        let mut interval = tokio::time::interval(Duration::from_secs(60));
        
        loop {
            interval.tick().await;
            
            let mut stats = self.stats.write().await;
            stats.uptime = start_time.elapsed().unwrap_or_default();
            stats.last_updated = SystemTime::now();
            
            // Update connected peers count
            let peers = self.peers.read().await;
            stats.connected_peers = peers.values()
                .filter(|p| p.status == ConnectionStatus::Connected)
                .count();
        }
    }
    
    /// Process pending messages
    async fn process_pending_messages(&self) -> Result<()> {
        // This would process outgoing messages and route them appropriately
        // Implementation would depend on message queue system
        Ok(())
    }
    
    /// Check health of peer connections
    async fn check_peer_health(&self) -> Result<()> {
        let mut peers = self.peers.write().await;
        let now = SystemTime::now();
        
        for (peer_id, peer) in peers.iter_mut() {
            // Check if peer has been inactive too long
            if let Ok(inactive_duration) = now.duration_since(peer.last_activity) {
                if inactive_duration > Duration::from_secs(300) { // 5 minutes
                    tracing::warn!("Peer {} inactive for {:?}", peer.callsign, inactive_duration);
                    
                    if peer.status == ConnectionStatus::Connected {
                        peer.status = ConnectionStatus::Reconnecting;
                    }
                }
            }
        }
        
        Ok(())
    }
    
    /// Connect to configured peers
    async fn connect_to_peers(&self) -> Result<()> {
        for peer_config in &self.config.peers {
            // Check if we're already connected to this peer
            let peers = self.peers.read().await;
            let already_connected = peers.values()
                .any(|p| p.callsign == peer_config.callsign && p.status == ConnectionStatus::Connected);
            
            if !already_connected && peer_config.auto_connect {
                drop(peers);
                
                // Attempt connection
                if let Err(e) = self.connect_to_peer(peer_config).await {
                    tracing::debug!("Failed to connect to {}: {}", peer_config.callsign, e);
                }
            }
        }
        
        Ok(())
    }
    
    /// Connect to a specific peer
    async fn connect_to_peer(&self, peer_config: &crate::config::PeerConfig) -> Result<()> {
        tracing::info!("Attempting to connect to peer {}", peer_config.callsign);
        
        let mut stats = self.stats.write().await;
        stats.connection_attempts += 1;
        drop(stats);
        
        // Parse peer address
        let socket_addr = crate::utils::network::parse_socket_addr(&peer_config.address, 14580)?;
        
        // Attempt TCP connection
        match tokio::net::TcpStream::connect(socket_addr).await {
            Ok(stream) => {
                tracing::info!("Connected to peer {} at {}", peer_config.callsign, socket_addr);
                
                // Create peer connection
                let peer = PeerConnection {
                    node_id: Uuid::new_v4(), // Would be received during handshake
                    callsign: peer_config.callsign.clone(),
                    connection_type: ConnectionType::Internet,
                    connected_at: SystemTime::now(),
                    last_activity: SystemTime::now(),
                    status: ConnectionStatus::Connected,
                    capabilities: PeerCapabilities {
                        protocols: vec!["TCP".to_string()],
                        max_message_size: 1024,
                        compression: true,
                        store_forward: true,
                        rf_interfaces: Vec::new(),
                        version: "unknown".to_string(),
                    },
                    quality: ConnectionQuality {
                        rtt_ms: 0.0,
                        packet_loss: 0.0,
                        bandwidth_bps: 0.0,
                        signal_db: None,
                        reliability: 1.0,
                    },
                };
                
                // Add to peers
                let mut peers = self.peers.write().await;
                peers.insert(peer.node_id, peer);
                
                // Update statistics
                let mut stats = self.stats.write().await;
                stats.successful_connections += 1;
                
                // TODO: Handle the actual TCP stream for communication
                // This would involve protocol negotiation, authentication, etc.
                
                Ok(())
            }
            Err(e) => {
                tracing::warn!("Failed to connect to {}: {}", peer_config.callsign, e);
                
                let mut stats = self.stats.write().await;
                stats.failed_connections += 1;
                
                Err(anyhow::anyhow!("Connection failed: {}", e))
            }
        }
    }
    
    /// Send message to specific peer
    pub async fn send_to_peer(&self, peer_id: &Uuid, message: &[u8]) -> Result<()> {
        let peers = self.peers.read().await;
        if let Some(peer) = peers.get(peer_id) {
            if peer.status != ConnectionStatus::Connected {
                return Err(anyhow::anyhow!("Peer not connected"));
            }
            
            // Compress message if beneficial
            let compressed = if message.len() > 128 && peer.capabilities.compression {
                self.compression.compress(message)?
            } else {
                message.to_vec()
            };
            
            // TODO: Actually send the message via the connection
            // This would use the stored connection handle
            
            // Update statistics
            let mut stats = self.stats.write().await;
            stats.messages_sent += 1;
            stats.bytes_transmitted += compressed.len() as u64;
            
            tracing::debug!("Sent {} bytes to peer {}", compressed.len(), peer.callsign);
            
            Ok(())
        } else {
            Err(anyhow::anyhow!("Peer not found"))
        }
    }
    
    /// Broadcast message to all connected peers
    pub async fn broadcast_message(&self, message: &[u8]) -> Result<usize> {
        let peers = self.peers.read().await;
        let connected_peers: Vec<Uuid> = peers.values()
            .filter(|p| p.status == ConnectionStatus::Connected)
            .map(|p| p.node_id)
            .collect();
        
        drop(peers);
        
        let mut sent_count = 0;
        for peer_id in connected_peers {
            if self.send_to_peer(&peer_id, message).await.is_ok() {
                sent_count += 1;
            }
        }
        
        Ok(sent_count)
    }
    
    /// Get network statistics
    pub async fn get_stats(&self) -> NetworkStats {
        self.stats.read().await.clone()
    }
    
    /// Get connected peers
    pub async fn get_connected_peers(&self) -> Vec<PeerConnection> {
        let peers = self.peers.read().await;
        peers.values()
            .filter(|p| p.status == ConnectionStatus::Connected)
            .cloned()
            .collect()
    }
    
    /// Disconnect from peer
    pub async fn disconnect_peer(&self, peer_id: &Uuid) -> Result<()> {
        let mut peers = self.peers.write().await;
        if let Some(peer) = peers.get_mut(peer_id) {
            peer.status = ConnectionStatus::Disconnected;
            tracing::info!("Disconnected from peer {}", peer.callsign);
            Ok(())
        } else {
            Err(anyhow::anyhow!("Peer not found"))
        }
    }
    
    /// Get node ID
    pub fn node_id(&self) -> Uuid {
        self.node_id
    }
    
    /// Check if RF is enabled
    pub fn is_rf_enabled(&self) -> bool {
        self.rf_enabled
    }
}

impl Default for PeerCapabilities {
    fn default() -> Self {
        Self {
            protocols: vec!["TCP".to_string()],
            max_message_size: 1024,
            compression: true,
            store_forward: true,
            rf_interfaces: Vec::new(),
            version: env!("CARGO_PKG_VERSION").to_string(),
        }
    }
}

impl Default for ConnectionQuality {
    fn default() -> Self {
        Self {
            rtt_ms: 0.0,
            packet_loss: 0.0,
            bandwidth_bps: 0.0,
            signal_db: None,
            reliability: 1.0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{NetworkConfig, PeerConfig};
    use crate::database::Database;
    use crate::messaging::MessageSystem;
    use tempfile::tempdir;
    
    async fn create_test_network_manager() -> NetworkManager {
        let temp_dir = tempdir().unwrap();
        let db_path = temp_dir.path().join("test.db");
        
        let mut db_config = crate::config::DatabaseConfig::default();
        db_config.path = db_path;
        
        let database = Database::new(&db_config).await.unwrap();
        database.migrate().await.unwrap();
        
        let messaging_config = crate::config::MessagingConfig::default();
        let message_system = MessageSystem::new(database.clone(), &messaging_config).await.unwrap();
        
        let network_config = NetworkConfig::default();
        
        NetworkManager::new(database, message_system, &network_config, false).await.unwrap()
    }
    
    #[tokio::test]
    async fn test_network_manager_creation() {
        let manager = create_test_network_manager().await;
        
        assert!(!manager.is_rf_enabled());
        assert_eq!(manager.get_connected_peers().await.len(), 0);
        
        let stats = manager.get_stats().await;
        assert_eq!(stats.connected_peers, 0);
        assert_eq!(stats.messages_sent, 0);
    }
    
    #[tokio::test]
    async fn test_peer_management() {
        let manager = create_test_network_manager().await;
        
        // Create a test peer connection
        let peer = PeerConnection {
            node_id: Uuid::new_v4(),
            callsign: "W1TEST".to_string(),
            connection_type: ConnectionType::Internet,
            connected_at: SystemTime::now(),
            last_activity: SystemTime::now(),
            status: ConnectionStatus::Connected,
            capabilities: PeerCapabilities::default(),
            quality: ConnectionQuality::default(),
        };
        
        // Add peer manually for testing
        {
            let mut peers = manager.peers.write().await;
            peers.insert(peer.node_id, peer.clone());
        }
        
        let connected_peers = manager.get_connected_peers().await;
        assert_eq!(connected_peers.len(), 1);
        assert_eq!(connected_peers[0].callsign, "W1TEST");
    }
    
    #[tokio::test]
    async fn test_statistics_tracking() {
        let manager = create_test_network_manager().await;
        
        // Simulate sending messages
        {
            let mut stats = manager.stats.write().await;
            stats.messages_sent = 10;
            stats.bytes_transmitted = 1024;
        }
        
        let stats = manager.get_stats().await;
        assert_eq!(stats.messages_sent, 10);
        assert_eq!(stats.bytes_transmitted, 1024);
    }
}

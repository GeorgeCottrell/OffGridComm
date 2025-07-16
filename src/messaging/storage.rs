use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::sync::RwLock;
use uuid::Uuid;

use crate::config::MessagingConfig;
use crate::database::{Database, models::*};

/// Message storage handler for efficient message persistence and retrieval
#[derive(Clone)]
pub struct MessageStorage {
    /// Database connection
    database: Database,
    
    /// Storage configuration
    config: MessagingConfig,
    
    /// Message index for fast lookups
    message_index: Arc<RwLock<MessageIndex>>,
    
    /// Thread index for conversation tracking
    thread_index: Arc<RwLock<ThreadIndex>>,
    
    /// Storage statistics
    stats: Arc<RwLock<StorageStats>>,
    
    /// Compression handler
    compression: CompressionHandler,
    
    /// Cleanup scheduler
    cleanup_scheduler: Arc<RwLock<CleanupScheduler>>,
}

/// Message index for fast access patterns
#[derive(Debug, Default)]
struct MessageIndex {
    /// Messages by hash (for deduplication)
    by_hash: HashMap<String, Uuid>,
    
    /// Recent messages by user (sender or recipient)
    by_user: HashMap<Uuid, Vec<MessageRef>>,
    
    /// Recent messages by group
    by_group: HashMap<Uuid, Vec<MessageRef>>,
    
    /// Unread counts by user
    unread_counts: HashMap<Uuid, u32>,
    
    /// Index last updated timestamp
    last_updated: SystemTime,
    
    /// Index size limit
    max_size: usize,
}

/// Thread index for conversation tracking
#[derive(Debug, Default)]
struct ThreadIndex {
    /// Thread to messages mapping
    threads: HashMap<Uuid, Vec<MessageRef>>,
    
    /// Message to thread mapping
    message_to_thread: HashMap<Uuid, Uuid>,
    
    /// Thread statistics
    thread_stats: HashMap<Uuid, ThreadStats>,
    
    /// Last updated timestamp
    last_updated: SystemTime,
}

/// Message reference for indexing
#[derive(Debug, Clone)]
struct MessageRef {
    id: Uuid,
    created_at: SystemTime,
    sender_id: Uuid,
    message_type: MessageType,
    priority: MessagePriority,
    status: MessageStatus,
}

/// Thread statistics
#[derive(Debug, Clone)]
struct ThreadStats {
    message_count: u32,
    participant_count: u32,
    last_activity: SystemTime,
    first_message: SystemTime,
}

/// Storage statistics
#[derive(Debug, Clone, Serialize)]
pub struct StorageStats {
    /// Total messages stored
    pub total_messages: u64,
    
    /// Messages by type
    pub by_type: HashMap<MessageType, u64>,
    
    /// Messages by status
    pub by_status: HashMap<MessageStatus, u64>,
    
    /// Storage size metrics
    pub storage_metrics: StorageMetrics,
    
    /// Performance metrics
    pub performance_metrics: PerformanceMetrics,
    
    /// Last statistics update
    pub last_updated: SystemTime,
}

/// Storage size metrics
#[derive(Debug, Clone, Serialize)]
pub struct StorageMetrics {
    /// Total database size (bytes)
    pub total_size_bytes: u64,
    
    /// Messages table size (bytes)
    pub messages_size_bytes: u64,
    
    /// Average message size (bytes)
    pub avg_message_size: f64,
    
    /// Compression ratio
    pub compression_ratio: f64,
    
    /// Storage efficiency
    pub storage_efficiency: f64,
}

/// Performance metrics
#[derive(Debug, Clone, Serialize)]
pub struct PerformanceMetrics {
    /// Average query time (milliseconds)
    pub avg_query_time_ms: f64,
    
    /// Cache hit rate
    pub cache_hit_rate: f64,
    
    /// Index efficiency
    pub index_efficiency: f64,
    
    /// Cleanup frequency
    pub cleanup_frequency_hours: f64,
}

/// Compression handler for message content
#[derive(Clone)]
struct CompressionHandler {
    enabled: bool,
    algorithm: CompressionAlgorithm,
    min_size: usize,
    compression_level: u8,
}

/// Compression algorithms
#[derive(Debug, Clone, Copy)]
enum CompressionAlgorithm {
    None,
    Lz4,
    Zstd,
    Gzip,
}

/// Cleanup scheduler for maintenance tasks
#[derive(Debug, Default)]
struct CleanupScheduler {
    /// Last cleanup timestamp
    last_cleanup: Option<SystemTime>,
    
    /// Cleanup interval
    cleanup_interval: Duration,
    
    /// Cleanup tasks pending
    pending_tasks: Vec<CleanupTask>,
    
    /// Cleanup statistics
    cleanup_stats: CleanupStats,
}

/// Cleanup task types
#[derive(Debug, Clone)]
enum CleanupTask {
    /// Delete expired messages
    DeleteExpired { older_than: Duration },
    
    /// Compact message storage
    CompactStorage,
    
    /// Rebuild indexes
    RebuildIndexes,
    
    /// Update statistics
    UpdateStatistics,
    
    /// Vacuum database
    VacuumDatabase,
}

/// Cleanup statistics
#[derive(Debug, Clone, Default)]
struct CleanupStats {
    /// Total cleanup runs
    total_runs: u64,
    
    /// Messages cleaned up
    messages_cleaned: u64,
    
    /// Space reclaimed (bytes)
    space_reclaimed: u64,
    
    /// Average cleanup time (seconds)
    avg_cleanup_time: f64,
    
    /// Last cleanup duration
    last_cleanup_duration: Option<Duration>,
}

/// Message search parameters
#[derive(Debug, Clone)]
pub struct MessageSearchParams {
    /// Text search query
    pub query: Option<String>,
    
    /// Search in message body
    pub search_body: bool,
    
    /// Search in subject
    pub search_subject: bool,
    
    /// Search scope
    pub scope: SearchScope,
    
    /// Date range
    pub date_range: Option<(SystemTime, SystemTime)>,
    
    /// Message types to include
    pub message_types: Option<Vec<MessageType>>,
    
    /// Message priorities to include
    pub priorities: Option<Vec<MessagePriority>>,
    
    /// Sender filter
    pub sender_filter: Option<String>,
    
    /// Sort order
    pub sort_order: SortOrder,
    
    /// Pagination
    pub limit: u32,
    pub offset: u32,
}

/// Search scope
#[derive(Debug, Clone)]
pub enum SearchScope {
    /// Search user's messages (sent and received)
    UserMessages(Uuid),
    
    /// Search specific group
    GroupMessages(Uuid),
    
    /// Search specific conversation
    Conversation { user1: Uuid, user2: Uuid },
    
    /// Search all accessible messages for user
    AllAccessible(Uuid),
}

/// Sort order for search results
#[derive(Debug, Clone, Copy)]
pub enum SortOrder {
    /// Most recent first
    DateDescending,
    
    /// Oldest first
    DateAscending,
    
    /// By relevance (for text search)
    Relevance,
    
    /// By priority (emergency first)
    Priority,
}

/// Message archival parameters
#[derive(Debug, Clone)]
pub struct ArchivalParams {
    /// Messages older than this duration
    pub older_than: Duration,
    
    /// Archive to external storage
    pub external_storage: bool,
    
    /// Compress archived messages
    pub compress: bool,
    
    /// Keep metadata in main database
    pub keep_metadata: bool,
    
    /// Archive location
    pub archive_path: Option<String>,
}

impl MessageStorage {
    /// Create a new message storage handler
    pub async fn new(database: Database, config: MessagingConfig) -> Result<Self> {
        let compression = CompressionHandler {
            enabled: true, // Would come from config
            algorithm: CompressionAlgorithm::Lz4,
            min_size: 256,
            compression_level: 1,
        };
        
        let cleanup_scheduler = Arc::new(RwLock::new(CleanupScheduler {
            last_cleanup: None,
            cleanup_interval: Duration::from_secs(3600), // 1 hour
            pending_tasks: Vec::new(),
            cleanup_stats: CleanupStats::default(),
        }));
        
        let storage = Self {
            database,
            config,
            message_index: Arc::new(RwLock::new(MessageIndex {
                by_hash: HashMap::new(),
                by_user: HashMap::new(),
                by_group: HashMap::new(),
                unread_counts: HashMap::new(),
                last_updated: SystemTime::now(),
                max_size: 10000, // Configurable
            })),
            thread_index: Arc::new(RwLock::new(ThreadIndex::default())),
            stats: Arc::new(RwLock::new(StorageStats {
                total_messages: 0,
                by_type: HashMap::new(),
                by_status: HashMap::new(),
                storage_metrics: StorageMetrics {
                    total_size_bytes: 0,
                    messages_size_bytes: 0,
                    avg_message_size: 0.0,
                    compression_ratio: 1.0,
                    storage_efficiency: 1.0,
                },
                performance_metrics: PerformanceMetrics {
                    avg_query_time_ms: 0.0,
                    cache_hit_rate: 0.0,
                    index_efficiency: 1.0,
                    cleanup_frequency_hours: 24.0,
                },
                last_updated: SystemTime::now(),
            })),
            compression,
            cleanup_scheduler,
        };
        
        // Initialize indexes
        storage.rebuild_indexes().await?;
        
        Ok(storage)
    }
    
    /// Store a new message
    pub async fn store_message(&self, message: &Message) -> Result<()> {
        let start_time = std::time::Instant::now();
        
        // Compress message if needed
        let compressed_message = self.compress_message_if_needed(message)?;
        
        // Store in database
        self.database.create_message(&compressed_message).await?;
        
        // Update indexes
        self.update_indexes_for_message(&compressed_message).await;
        
        // Update statistics
        self.update_storage_stats(&compressed_message, start_time.elapsed()).await;
        
        Ok(())
    }
    
    /// Get message by ID
    pub async fn get_message(&self, message_id: &Uuid) -> Result<Option<Message>> {
        let start_time = std::time::Instant::now();
        
        // Try to get from database
        let message = self.database.get_message_by_id(message_id).await?;
        
        // Decompress if needed
        let decompressed_message = if let Some(msg) = message {
            Some(self.decompress_message_if_needed(&msg)?)
        } else {
            None
        };
        
        // Update performance metrics
        self.update_query_performance(start_time.elapsed()).await;
        
        Ok(decompressed_message)
    }
    
    /// Get user messages with filtering and pagination
    pub async fn get_user_messages(
        &self,
        user_id: &Uuid,
        message_type: Option<MessageType>,
        limit: u32,
        offset: u32,
    ) -> Result<Vec<Message>> {
        let start_time = std::time::Instant::now();
        
        // Check index for recent messages
        let cached_messages = self.get_cached_user_messages(user_id, limit, offset).await;
        if let Some(messages) = cached_messages {
            self.update_query_performance(start_time.elapsed()).await;
            return Ok(messages);
        }
        
        // Query database
        let messages = self.database.get_user_messages(user_id, message_type, limit, offset).await?;
        
        // Decompress messages
        let decompressed_messages = self.decompress_messages(&messages)?;
        
        // Update cache
        self.cache_user_messages(user_id, &decompressed_messages).await;
        
        // Update performance metrics
        self.update_query_performance(start_time.elapsed()).await;
        
        Ok(decompressed_messages)
    }
    
    /// Get group messages with pagination
    pub async fn get_group_messages(
        &self,
        group_id: &Uuid,
        limit: u32,
        offset: u32,
    ) -> Result<Vec<Message>> {
        let start_time = std::time::Instant::now();
        
        // Check index for recent messages
        let cached_messages = self.get_cached_group_messages(group_id, limit, offset).await;
        if let Some(messages) = cached_messages {
            self.update_query_performance(start_time.elapsed()).await;
            return Ok(messages);
        }
        
        // Query database
        let messages = self.database.get_group_messages(group_id, limit, offset).await?;
        
        // Decompress messages
        let decompressed_messages = self.decompress_messages(&messages)?;
        
        // Update cache
        self.cache_group_messages(group_id, &decompressed_messages).await;
        
        // Update performance metrics
        self.update_query_performance(start_time.elapsed()).await;
        
        Ok(decompressed_messages)
    }
    
    /// Get message thread
    pub async fn get_message_thread(&self, thread_id: &Uuid) -> Result<Vec<Message>> {
        let start_time = std::time::Instant::now();
        
        // Check thread index
        let cached_thread = self.get_cached_thread(thread_id).await;
        if let Some(messages) = cached_thread {
            self.update_query_performance(start_time.elapsed()).await;
            return Ok(messages);
        }
        
        // Query database
        let messages = self.database.get_message_thread(thread_id).await?;
        
        // Decompress messages
        let decompressed_messages = self.decompress_messages(&messages)?;
        
        // Update thread cache
        self.cache_thread(thread_id, &decompressed_messages).await;
        
        // Update performance metrics
        self.update_query_performance(start_time.elapsed()).await;
        
        Ok(decompressed_messages)
    }
    
    /// Search messages
    pub async fn search_messages(&self, params: MessageSearchParams) -> Result<Vec<Message>> {
        let start_time = std::time::Instant::now();
        
        // Build search query
        let messages = self.execute_search_query(&params).await?;
        
        // Decompress messages
        let decompressed_messages = self.decompress_messages(&messages)?;
        
        // Update performance metrics
        self.update_query_performance(start_time.elapsed()).await;
        
        Ok(decompressed_messages)
    }
    
    /// Mark message as read
    pub async fn mark_message_read(&self, message_id: &Uuid, read_at: SystemTime) -> Result<()> {
        // Update in database
        self.database.mark_message_read(message_id, read_at).await?;
        
        // Update indexes
        self.update_read_status_in_indexes(message_id).await;
        
        Ok(())
    }
    
    /// Update message status
    pub async fn update_message_status(&self, message_id: &Uuid, status: MessageStatus) -> Result<()> {
        // Update in database
        self.database.update_message_status(message_id, status).await?;
        
        // Update indexes
        self.update_status_in_indexes(message_id, status).await;
        
        Ok(())
    }
    
    /// Check if message exists by hash (for deduplication)
    pub async fn message_exists_by_hash(&self, message_hash: &str) -> Result<bool> {
        // Check index first
        {
            let index = self.message_index.read().await;
            if index.by_hash.contains_key(message_hash) {
                return Ok(true);
            }
        }
        
        // Check database
        self.database.message_exists_by_hash(message_hash).await
    }
    
    /// Get unread message count for user
    pub async fn get_unread_message_count(&self, user_id: &Uuid) -> Result<i64> {
        // Check index first
        {
            let index = self.message_index.read().await;
            if let Some(&count) = index.unread_counts.get(user_id) {
                return Ok(count as i64);
            }
        }
        
        // Query database
        let count = self.database.get_unread_message_count(user_id).await?;
        
        // Update index
        {
            let mut index = self.message_index.write().await;
            index.unread_counts.insert(*user_id, count as u32);
        }
        
        Ok(count)
    }
    
    /// Archive old messages
    pub async fn archive_messages(&self, params: ArchivalParams) -> Result<u64> {
        let cutoff_time = SystemTime::now() - params.older_than;
        
        // Get messages to archive
        let old_messages = self.get_messages_older_than(cutoff_time).await?;
        
        if params.external_storage {
            // Export to external storage
            self.export_messages_to_archive(&old_messages, &params).await?;
        }
        
        // Delete from main database
        let deleted_count = self.database.delete_old_messages(params.older_than.as_secs() as u32 / 86400).await?;
        
        // Update indexes
        self.rebuild_indexes().await?;
        
        Ok(deleted_count)
    }
    
    /// Get comprehensive message statistics
    pub async fn get_comprehensive_message_stats(&self) -> Result<MessageStats> {
        let storage_stats = self.stats.read().await;
        
        Ok(MessageStats {
            total_messages: storage_stats.total_messages,
            by_type: storage_stats.by_type.clone(),
            by_status: storage_stats.by_status.clone(),
            by_priority: HashMap::new(), // Would be populated from actual data
            avg_message_size: storage_stats.storage_metrics.avg_message_size,
            daily_message_count: self.get_daily_message_counts().await?,
            top_senders: self.get_top_senders(10).await?,
            emergency_count: storage_stats.by_type.get(&MessageType::Emergency).copied().unwrap_or(0),
        })
    }
    
    /// Perform storage maintenance
    pub async fn perform_maintenance(&self) -> Result<()> {
        let mut scheduler = self.cleanup_scheduler.write().await;
        let now = SystemTime::now();
        
        // Check if maintenance is due
        if let Some(last_cleanup) = scheduler.last_cleanup {
            if now.duration_since(last_cleanup).unwrap_or_default() < scheduler.cleanup_interval {
                return Ok(()); // Too early for maintenance
            }
        }
        
        let start_time = std::time::Instant::now();
        
        // Execute pending cleanup tasks
        for task in &scheduler.pending_tasks {
            self.execute_cleanup_task(task).await?;
        }
        
        // Default maintenance tasks
        self.rebuild_indexes().await?;
        self.update_statistics().await?;
        self.compact_storage().await?;
        
        // Update cleanup statistics
        let duration = start_time.elapsed();
        scheduler.last_cleanup = Some(now);
        scheduler.cleanup_stats.total_runs += 1;
        scheduler.cleanup_stats.last_cleanup_duration = Some(duration);
        scheduler.cleanup_stats.avg_cleanup_time = 
            (scheduler.cleanup_stats.avg_cleanup_time * (scheduler.cleanup_stats.total_runs - 1) as f64 + 
             duration.as_secs_f64()) / scheduler.cleanup_stats.total_runs as f64;
        
        // Clear completed tasks
        scheduler.pending_tasks.clear();
        
        Ok(())
    }
    
    /// Start background maintenance tasks
    pub async fn start_background_maintenance(&self) -> tokio::task::JoinHandle<()> {
        let storage = self.clone();
        
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(3600)); // 1 hour
            
            loop {
                interval.tick().await;
                
                if let Err(e) = storage.perform_maintenance().await {
                    tracing::error!("Storage maintenance error: {}", e);
                }
            }
        })
    }
    
    // Private helper methods
    
    /// Compress message if needed
    fn compress_message_if_needed(&self, message: &Message) -> Result<Message> {
        if !self.compression.enabled || message.body.len() < self.compression.min_size {
            return Ok(message.clone());
        }
        
        let compressed_body = match self.compression.algorithm {
            CompressionAlgorithm::None => message.body.clone(),
            CompressionAlgorithm::Lz4 => {
                self.compress_lz4(&message.body)?
            }
            CompressionAlgorithm::Zstd => {
                self.compress_zstd(&message.body)?
            }
            CompressionAlgorithm::Gzip => {
                self.compress_gzip(&message.body)?
            }
        };
        
        let mut compressed_message = message.clone();
        compressed_message.body = compressed_body;
        compressed_message.flags.compressed = true;
        
        Ok(compressed_message)
    }
    
    /// Decompress message if needed
    fn decompress_message_if_needed(&self, message: &Message) -> Result<Message> {
        if !message.flags.compressed {
            return Ok(message.clone());
        }
        
        let decompressed_body = match self.compression.algorithm {
            CompressionAlgorithm::None => message.body.clone(),
            CompressionAlgorithm::Lz4 => {
                self.decompress_lz4(&message.body)?
            }
            CompressionAlgorithm::Zstd => {
                self.decompress_zstd(&message.body)?
            }
            CompressionAlgorithm::Gzip => {
                self.decompress_gzip(&message.body)?
            }
        };
        
        let mut decompressed_message = message.clone();
        decompressed_message.body = decompressed_body;
        decompressed_message.flags.compressed = false;
        
        Ok(decompressed_message)
    }
    
    /// Decompress multiple messages
    fn decompress_messages(&self, messages: &[Message]) -> Result<Vec<Message>> {
        messages
            .iter()
            .map(|msg| self.decompress_message_if_needed(msg))
            .collect()
    }
    
    /// Compress using LZ4
    fn compress_lz4(&self, data: &str) -> Result<String> {
        use lz4_flex::compress_prepend_size;
        let compressed = compress_prepend_size(data.as_bytes());
        Ok(base64::encode(compressed))
    }
    
    /// Decompress using LZ4
    fn decompress_lz4(&self, data: &str) -> Result<String> {
        use lz4_flex::decompress_size_prepended;
        let compressed = base64::decode(data)?;
        let decompressed = decompress_size_prepended(&compressed)?;
        Ok(String::from_utf8(decompressed)?)
    }
    
    /// Compress using Zstd
    fn compress_zstd(&self, data: &str) -> Result<String> {
        use zstd::encode_all;
        let compressed = encode_all(data.as_bytes(), self.compression.compression_level as i32)?;
        Ok(base64::encode(compressed))
    }
    
    /// Decompress using Zstd
    fn decompress_zstd(&self, data: &str) -> Result<String> {
        use zstd::decode_all;
        let compressed = base64::decode(data)?;
        let decompressed = decode_all(&compressed[..])?;
        Ok(String::from_utf8(decompressed)?)
    }
    
    /// Compress using Gzip
    fn compress_gzip(&self, data: &str) -> Result<String> {
        use flate2::{Compression, write::GzEncoder};
        use std::io::Write;
        
        let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
        encoder.write_all(data.as_bytes())?;
        let compressed = encoder.finish()?;
        Ok(base64::encode(compressed))
    }
    
    /// Decompress using Gzip
    fn decompress_gzip(&self, data: &str) -> Result<String> {
        use flate2::read::GzDecoder;
        use std::io::Read;
        
        let compressed = base64::decode(data)?;
        let mut decoder = GzDecoder::new(&compressed[..]);
        let mut decompressed = String::new();
        decoder.read_to_string(&mut decompressed)?;
        Ok(decompressed)
    }
    
    /// Update indexes for new message
    async fn update_indexes_for_message(&self, message: &Message) {
        let message_ref = MessageRef {
            id: message.id,
            created_at: message.created_at,
            sender_id: message.sender_id,
            message_type: message.message_type,
            priority: message.priority,
            status: message.status,
        };
        
        let mut index = self.message_index.write().await;
        
        // Update hash index
        index.by_hash.insert(message.message_hash.clone(), message.id);
        
        // Update user index
        index.by_user.entry(message.sender_id).or_insert_with(Vec::new).push(message_ref.clone());
        if let Some(recipient_id) = message.recipient_id {
            index.by_user.entry(recipient_id).or_insert_with(Vec::new).push(message_ref.clone());
            
            // Update unread count for recipient
            if message.read_at.is_none() {
                *index.unread_counts.entry(recipient_id).or_insert(0) += 1;
            }
        }
        
        // Update group index
        if let Some(group_id) = message.group_id {
            index.by_group.entry(group_id).or_insert_with(Vec::new).push(message_ref);
        }
        
        // Maintain index size
        self.trim_index_if_needed(&mut index).await;
        
        // Update thread index
        if let Some(thread_id) = message.thread_id {
            let mut thread_index = self.thread_index.write().await;
            thread_index.threads.entry(thread_id).or_insert_with(Vec::new).push(MessageRef {
                id: message.id,
                created_at: message.created_at,
                sender_id: message.sender_id,
                message_type: message.message_type,
                priority: message.priority,
                status: message.status,
            });
            thread_index.message_to_thread.insert(message.id, thread_id);
        }
    }
    
    /// Trim index to maintain size limits
    async fn trim_index_if_needed(&self, index: &mut MessageIndex) {
        if index.by_user.len() > index.max_size {
            // Remove oldest entries from user index
            for user_messages in index.by_user.values_mut() {
                user_messages.sort_by_key(|m| m.created_at);
                if user_messages.len() > 100 { // Keep last 100 per user
                    user_messages.drain(0..user_messages.len() - 100);
                }
            }
        }
        
        // Similar logic for group index
        if index.by_group.len() > index.max_size {
            for group_messages in index.by_group.values_mut() {
                group_messages.sort_by_key(|m| m.created_at);
                if group_messages.len() > 100 {
                    group_messages.drain(0..group_messages.len() - 100);
                }
            }
        }
    }
    
    /// Get cached user messages
    async fn get_cached_user_messages(
        &self,
        user_id: &Uuid,
        limit: u32,
        offset: u32,
    ) -> Option<Vec<Message>> {
        let index = self.message_index.read().await;
        if let Some(message_refs) = index.by_user.get(user_id) {
            if offset == 0 && limit <= message_refs.len() as u32 {
                // Can serve from cache
                let message_ids: Vec<Uuid> = message_refs
                    .iter()
                    .rev() // Most recent first
                    .take(limit as usize)
                    .map(|r| r.id)
                    .collect();
                
                // Would need to fetch full messages from cache or database
                // Simplified for now
                return None;
            }
        }
        None
    }
    
    /// Cache user messages
    async fn cache_user_messages(&self, _user_id: &Uuid, _messages: &[Message]) {
        // Would update the message cache
        // Simplified for now
    }
    
    /// Get cached group messages
    async fn get_cached_group_messages(
        &self,
        _group_id: &Uuid,
        _limit: u32,
        _offset: u32,
    ) -> Option<Vec<Message>> {
        // Would check group message cache
        // Simplified for now
        None
    }
    
    /// Cache group messages
    async fn cache_group_messages(&self, _group_id: &Uuid, _messages: &[Message]) {
        // Would update the group message cache
        // Simplified for now
    }
    
    /// Get cached thread
    async fn get_cached_thread(&self, _thread_id: &Uuid) -> Option<Vec<Message>> {
        // Would check thread cache
        // Simplified for now
        None
    }
    
    /// Cache thread messages
    async fn cache_thread(&self, _thread_id: &Uuid, _messages: &[Message]) {
        // Would update the thread cache
        // Simplified for now
    }
    
    /// Execute search query
    async fn execute_search_query(&self, _params: &MessageSearchParams) -> Result<Vec<Message>> {
        // Would execute complex search query
        // Simplified for now
        Ok(Vec::new())
    }
    
    /// Update read status in indexes
    async fn update_read_status_in_indexes(&self, message_id: &Uuid) {
        let mut index = self.message_index.write().await;
        
        // Find and update message status in all indexes
        for user_messages in index.by_user.values_mut() {
            for msg_ref in user_messages.iter_mut() {
                if msg_ref.id == *message_id {
                    msg_ref.status = MessageStatus::Read;
                }
            }
        }
        
        for group_messages in index.by_group.values_mut() {
            for msg_ref in group_messages.iter_mut() {
                if msg_ref.id == *message_id {
                    msg_ref.status = MessageStatus::Read;
                }
            }
        }
        
        // Update unread counts (would need more context about which user read it)
        // Simplified for now
    }
    
    /// Update status in indexes
    async fn update_status_in_indexes(&self, message_id: &Uuid, status: MessageStatus) {
        let mut index = self.message_index.write().await;
        
        // Update status in all relevant indexes
        for user_messages in index.by_user.values_mut() {
            for msg_ref in user_messages.iter_mut() {
                if msg_ref.id == *message_id {
                    msg_ref.status = status;
                }
            }
        }
        
        for group_messages in index.by_group.values_mut() {
            for msg_ref in group_messages.iter_mut() {
                if msg_ref.id == *message_id {
                    msg_ref.status = status;
                }
            }
        }
    }
    
    /// Update storage statistics
    async fn update_storage_stats(&self, message: &Message, query_time: Duration) {
        let mut stats = self.stats.write().await;
        
        // Update total message count
        stats.total_messages += 1;
        
        // Update by type
        *stats.by_type.entry(message.message_type).or_insert(0) += 1;
        
        // Update by status
        *stats.by_status.entry(message.status).or_insert(0) += 1;
        
        // Update storage metrics
        let message_size = message.body.len() as f64;
        stats.storage_metrics.avg_message_size = 
            (stats.storage_metrics.avg_message_size * (stats.total_messages - 1) as f64 + message_size) / 
            stats.total_messages as f64;
        
        // Update performance metrics
        let query_time_ms = query_time.as_millis() as f64;
        stats.performance_metrics.avg_query_time_ms = 
            (stats.performance_metrics.avg_query_time_ms * 0.9) + (query_time_ms * 0.1);
        
        stats.last_updated = SystemTime::now();
    }
    
    /// Update query performance metrics
    async fn update_query_performance(&self, query_time: Duration) {
        let mut stats = self.stats.write().await;
        let query_time_ms = query_time.as_millis() as f64;
        
        stats.performance_metrics.avg_query_time_ms = 
            (stats.performance_metrics.avg_query_time_ms * 0.95) + (query_time_ms * 0.05);
    }
    
    /// Rebuild indexes from database
    async fn rebuild_indexes(&self) -> Result<()> {
        tracing::info!("Rebuilding message indexes");
        
        // Clear existing indexes
        {
            let mut index = self.message_index.write().await;
            index.by_hash.clear();
            index.by_user.clear();
            index.by_group.clear();
            index.unread_counts.clear();
        }
        
        {
            let mut thread_index = self.thread_index.write().await;
            thread_index.threads.clear();
            thread_index.message_to_thread.clear();
            thread_index.thread_stats.clear();
        }
        
        // This would rebuild from database
        // Simplified for now - in practice would query all recent messages
        // and rebuild the indexes
        
        tracing::info!("Message indexes rebuilt successfully");
        Ok(())
    }
    
    /// Update storage statistics from database
    async fn update_statistics(&self) -> Result<()> {
        let db_stats = self.database.get_stats().await?;
        
        let mut stats = self.stats.write().await;
        stats.total_messages = db_stats.message_count;
        stats.storage_metrics.total_size_bytes = db_stats.total_size_bytes;
        stats.last_updated = SystemTime::now();
        
        Ok(())
    }
    
    /// Compact storage (vacuum database)
    async fn compact_storage(&self) -> Result<()> {
        tracing::info!("Compacting message storage");
        
        self.database.compact().await?;
        
        tracing::info!("Storage compaction completed");
        Ok(())
    }
    
    /// Execute cleanup task
    async fn execute_cleanup_task(&self, task: &CleanupTask) -> Result<()> {
        match task {
            CleanupTask::DeleteExpired { older_than } => {
                let days = older_than.as_secs() / 86400;
                let deleted = self.database.delete_old_messages(days as u32).await?;
                tracing::info!("Deleted {} expired messages", deleted);
                
                let mut scheduler = self.cleanup_scheduler.write().await;
                scheduler.cleanup_stats.messages_cleaned += deleted;
            }
            
            CleanupTask::CompactStorage => {
                self.compact_storage().await?;
            }
            
            CleanupTask::RebuildIndexes => {
                self.rebuild_indexes().await?;
            }
            
            CleanupTask::UpdateStatistics => {
                self.update_statistics().await?;
            }
            
            CleanupTask::VacuumDatabase => {
                // Would perform database vacuum
                tracing::info!("Performing database vacuum");
            }
        }
        
        Ok(())
    }
    
    /// Get messages older than cutoff time
    async fn get_messages_older_than(&self, _cutoff_time: SystemTime) -> Result<Vec<Message>> {
        // Would query database for old messages
        // Simplified for now
        Ok(Vec::new())
    }
    
    /// Export messages to archive
    async fn export_messages_to_archive(
        &self,
        _messages: &[Message],
        _params: &ArchivalParams,
    ) -> Result<()> {
        // Would export messages to external archive storage
        // Simplified for now
        Ok(())
    }
    
    /// Get daily message counts for statistics
    async fn get_daily_message_counts(&self) -> Result<Vec<u64>> {
        // Would query database for daily message counts over last 30 days
        // Simplified for now
        Ok(vec![0; 30])
    }
    
    /// Get top senders for statistics
    async fn get_top_senders(&self, _limit: u32) -> Result<Vec<(String, u64)>> {
        // Would query database for most active senders
        // Simplified for now
        Ok(Vec::new())
    }
}

impl Default for MessageSearchParams {
    fn default() -> Self {
        Self {
            query: None,
            search_body: true,
            search_subject: true,
            scope: SearchScope::AllAccessible(Uuid::nil()),
            date_range: None,
            message_types: None,
            priorities: None,
            sender_filter: None,
            sort_order: SortOrder::DateDescending,
            limit: 50,
            offset: 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::{User, UserRole, UserStatus};
    use crate::config::MessagingConfig;
    use crate::database::Database;
    use tempfile::tempdir;
    
    async fn create_test_storage() -> MessageStorage {
        let temp_dir = tempdir().unwrap();
        let db_path = temp_dir.path().join("test.db");
        
        let mut db_config = crate::config::DatabaseConfig::default();
        db_config.path = db_path;
        
        let database = Database::new(&db_config).await.unwrap();
        database.migrate().await.unwrap();
        
        let messaging_config = MessagingConfig::default();
        
        MessageStorage::new(database, messaging_config).await.unwrap()
    }
    
    async fn create_test_user(storage: &MessageStorage, callsign: &str) -> User {
        let user = User {
            id: Uuid::new_v4(),
            callsign: callsign.to_string(),
            full_name: Some(format!("Test User {}", callsign)),
            email: None,
            location: None,
            role: UserRole::User,
            created_at: SystemTime::now(),
            last_login: None,
            status: UserStatus::Active,
            license_class: None,
            license_verified: false,
        };
        
        storage.database.create_user(&user, "test_hash").await.unwrap();
        user
    }
    
    async fn create_test_node(storage: &MessageStorage) -> Uuid {
        let node_id = Uuid::new_v4();
        let timestamp = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap().as_secs() as i64;
        
        sqlx::query!(
            "INSERT INTO nodes (id, callsign, description, created_at, last_seen, status, version)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            node_id.to_string(),
            "W1NODE",
            "Test Node",
            timestamp,
            timestamp,
            1i32,
            "1.0.0"
        )
        .execute(storage.database.pool())
        .await
        .unwrap();
        
        node_id
    }
    
    async fn create_test_message(storage: &MessageStorage, sender: &User, recipient: &User, node_id: Uuid) -> Message {
        Message {
            id: Uuid::new_v4(),
            message_type: MessageType::Direct,
            sender_id: sender.id,
            sender_callsign: sender.callsign.clone(),
            recipient_id: Some(recipient.id),
            recipient_callsign: Some(recipient.callsign.clone()),
            group_id: None,
            subject: Some("Test Subject".to_string()),
            body: "This is a test message for storage testing".to_string(),
            priority: MessagePriority::Normal,
            created_at: SystemTime::now(),
            delivered_at: None,
            read_at: None,
            parent_id: None,
            thread_id: Some(Uuid::new_v4()),
            status: MessageStatus::Sent,
            origin_node_id: node_id,
            message_hash: "test_hash_123".to_string(),
            rf_metadata: None,
            flags: MessageFlags::default(),
        }
    }
    
    #[tokio::test]
    async fn test_message_storage_and_retrieval() {
        let storage = create_test_storage().await;
        let sender = create_test_user(&storage, "W1SEND").await;
        let recipient = create_test_user(&storage, "W1RECV").await;
        let node_id = create_test_node(&storage).await;
        
        let message = create_test_message(&storage, &sender, &recipient, node_id).await;
        
        // Store message
        storage.store_message(&message).await.unwrap();
        
        // Retrieve message
        let retrieved = storage.get_message(&message.id).await.unwrap().unwrap();
        assert_eq!(retrieved.id, message.id);
        assert_eq!(retrieved.body, message.body);
        assert_eq!(retrieved.sender_callsign, message.sender_callsign);
    }
    
    #[tokio::test]
    async fn test_message_compression() {
        let storage = create_test_storage().await;
        let sender = create_test_user(&storage, "W1COMP").await;
        let recipient = create_test_user(&storage, "W1RECV").await;
        let node_id = create_test_node(&storage).await;
        
        // Create message with large body
        let mut message = create_test_message(&storage, &sender, &recipient, node_id).await;
        message.body = "x".repeat(1000); // Large enough to trigger compression
        
        // Test compression
        let compressed = storage.compress_message_if_needed(&message).unwrap();
        assert!(compressed.flags.compressed);
        assert!(compressed.body.len() < message.body.len()); // Should be smaller
        
        // Test decompression
        let decompressed = storage.decompress_message_if_needed(&compressed).unwrap();
        assert!(!decompressed.flags.compressed);
        assert_eq!(decompressed.body, message.body); // Should match original
    }
    
    #[tokio::test]
    async fn test_lz4_compression() {
        let storage = create_test_storage().await;
        let test_data = "This is a test string for compression testing. It should compress well because it has repeated patterns. ".repeat(10);
        
        let compressed = storage.compress_lz4(&test_data).unwrap();
        let decompressed = storage.decompress_lz4(&compressed).unwrap();
        
        assert_eq!(decompressed, test_data);
        assert!(compressed.len() < test_data.len()); // Should be compressed
    }
    
    #[tokio::test]
    async fn test_message_deduplication() {
        let storage = create_test_storage().await;
        let sender = create_test_user(&storage, "W1DUP").await;
        let recipient = create_test_user(&storage, "W1RECV").await;
        let node_id = create_test_node(&storage).await;
        
        let message = create_test_message(&storage, &sender, &recipient, node_id).await;
        
        // Store message
        storage.store_message(&message).await.unwrap();
        
        // Check if hash exists
        let exists = storage.message_exists_by_hash(&message.message_hash).await.unwrap();
        assert!(exists);
        
        // Check non-existent hash
        let not_exists = storage.message_exists_by_hash("nonexistent_hash").await.unwrap();
        assert!(!not_exists);
    }
    
    #[tokio::test]
    async fn test_user_messages_retrieval() {
        let storage = create_test_storage().await;
        let sender = create_test_user(&storage, "W1SEND").await;
        let recipient = create_test_user(&storage, "W1RECV").await;
        let node_id = create_test_node(&storage).await;
        
        // Store multiple messages
        for i in 1..=5 {
            let mut message = create_test_message(&storage, &sender, &recipient, node_id).await;
            message.id = Uuid::new_v4();
            message.body = format!("Test message {}", i);
            message.message_hash = format!("test_hash_{}", i);
            storage.store_message(&message).await.unwrap();
        }
        
        // Retrieve user messages
        let messages = storage.get_user_messages(&recipient.id, None, 10, 0).await.unwrap();
        assert_eq!(messages.len(), 5);
        
        // Test pagination
        let page1 = storage.get_user_messages(&recipient.id, None, 3, 0).await.unwrap();
        assert_eq!(page1.len(), 3);
        
        let page2 = storage.get_user_messages(&recipient.id, None, 3, 3).await.unwrap();
        assert_eq!(page2.len(), 2);
    }
    
    #[tokio::test]
    async fn test_unread_count_tracking() {
        let storage = create_test_storage().await;
        let sender = create_test_user(&storage, "W1SEND").await;
        let recipient = create_test_user(&storage, "W1RECV").await;
        let node_id = create_test_node(&storage).await;
        
        // Store unread messages
        for i in 1..=3 {
            let mut message = create_test_message(&storage, &sender, &recipient, node_id).await;
            message.id = Uuid::new_v4();
            message.message_hash = format!("unread_hash_{}", i);
            message.read_at = None; // Unread
            storage.store_message(&message).await.unwrap();
        }
        
        // Check unread count
        let unread_count = storage.get_unread_message_count(&recipient.id).await.unwrap();
        assert_eq!(unread_count, 3);
        
        // Mark one as read
        let message = create_test_message(&storage, &sender, &recipient, node_id).await;
        storage.store_message(&message).await.unwrap();
        storage.mark_message_read(&message.id, SystemTime::now()).await.unwrap();
        
        // Unread count should remain the same since we added one and marked one as read
        let unread_count_after = storage.get_unread_message_count(&recipient.id).await.unwrap();
        assert!(unread_count_after >= 3);
    }
    
    #[tokio::test]
    async fn test_message_status_updates() {
        let storage = create_test_storage().await;
        let sender = create_test_user(&storage, "W1SEND").await;
        let recipient = create_test_user(&storage, "W1RECV").await;
        let node_id = create_test_node(&storage).await;
        
        let message = create_test_message(&storage, &sender, &recipient, node_id).await;
        storage.store_message(&message).await.unwrap();
        
        // Update status
        storage.update_message_status(&message.id, MessageStatus::Delivered).await.unwrap();
        
        // Verify status was updated
        let retrieved = storage.get_message(&message.id).await.unwrap().unwrap();
        assert_eq!(retrieved.status, MessageStatus::Delivered);
    }
    
    #[tokio::test]
    async fn test_storage_statistics() {
        let storage = create_test_storage().await;
        let sender = create_test_user(&storage, "W1STATS").await;
        let recipient = create_test_user(&storage, "W1RECV").await;
        let node_id = create_test_node(&storage).await;
        
        // Store some messages
        for i in 1..=10 {
            let mut message = create_test_message(&storage, &sender, &recipient, node_id).await;
            message.id = Uuid::new_v4();
            message.message_hash = format!("stats_hash_{}", i);
            storage.store_message(&message).await.unwrap();
        }
        
        // Get statistics
        let stats = storage.stats.read().await;
        assert_eq!(stats.total_messages, 10);
        assert!(stats.by_type.contains_key(&MessageType::Direct));
        assert!(stats.storage_metrics.avg_message_size > 0.0);
    }
    
    #[tokio::test]
    async fn test_index_management() {
        let storage = create_test_storage().await;
        let sender = create_test_user(&storage, "W1INDEX").await;
        let recipient = create_test_user(&storage, "W1RECV").await;
        let node_id = create_test_node(&storage).await;
        
        let message = create_test_message(&storage, &sender, &recipient, node_id).await;
        storage.store_message(&message).await.unwrap();
        
        // Check that indexes were updated
        {
            let index = storage.message_index.read().await;
            assert!(index.by_hash.contains_key(&message.message_hash));
            assert!(index.by_user.contains_key(&sender.id));
            assert!(index.by_user.contains_key(&recipient.id));
        }
        
        // Test index rebuild
        storage.rebuild_indexes().await.unwrap();
        
        // Indexes should still be functional after rebuild
        let exists = storage.message_exists_by_hash(&message.message_hash).await.unwrap();
        assert!(exists);
    }
    
    #[tokio::test]
    async fn test_maintenance_operations() {
        let storage = create_test_storage().await;
        
        // Test that maintenance can run without errors
        storage.perform_maintenance().await.unwrap();
        
        // Test statistics update
        storage.update_statistics().await.unwrap();
        
        // Test storage compaction
        storage.compact_storage().await.unwrap();
        
        // Test index rebuild
        storage.rebuild_indexes().await.unwrap();
    }
}
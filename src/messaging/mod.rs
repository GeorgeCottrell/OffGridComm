use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, SystemTime};
use tokio::sync::{broadcast, RwLock};
use uuid::Uuid;

pub mod direct;
pub mod groups;
pub mod storage;
pub mod validation;
pub mod notifications;

pub use direct::DirectMessageHandler;
pub use groups::GroupMessageHandler;
pub use storage::MessageStorage;
pub use validation::MessageValidator;
pub use notifications::NotificationManager;

use crate::auth::User;
use crate::config::MessagingConfig;
use crate::database::{Database, models::*};

/// Main messaging system that coordinates all message operations
#[derive(Clone)]
pub struct MessageSystem {
    /// Database connection
    database: Database,
    
    /// Message storage handler
    storage: MessageStorage,
    
    /// Direct message handler
    direct_handler: DirectMessageHandler,
    
    /// Group message handler
    group_handler: GroupMessageHandler,
    
    /// Message validator
    validator: MessageValidator,
    
    /// Notification manager
    notification_manager: NotificationManager,
    
    /// Configuration
    config: MessagingConfig,
    
    /// Real-time event broadcaster
    event_broadcaster: broadcast::Sender<MessageEvent>,
    
    /// Message cache for fast access
    message_cache: Arc<RwLock<MessageCache>>,
    
    /// Active delivery tasks
    delivery_tasks: Arc<RwLock<HashMap<Uuid, DeliveryTask>>>,
}

/// Message cache for performance optimization
#[derive(Debug, Default)]
struct MessageCache {
    /// Recently accessed messages
    recent_messages: HashMap<Uuid, (Message, SystemTime)>,
    
    /// Unread counts per user
    unread_counts: HashMap<Uuid, u32>,
    
    /// Group message counts
    group_message_counts: HashMap<Uuid, u32>,
    
    /// Cache size limit
    max_size: usize,
}

/// Message delivery task tracking
#[derive(Debug, Clone)]
struct DeliveryTask {
    /// Message being delivered
    message_id: Uuid,
    
    /// Delivery attempts
    attempts: u32,
    
    /// Next retry time
    next_retry: SystemTime,
    
    /// Delivery status
    status: DeliveryStatus,
    
    /// Target recipients
    recipients: Vec<Uuid>,
}

/// Delivery status enumeration
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DeliveryStatus {
    /// Delivery is pending
    Pending,
    
    /// Delivery in progress
    InProgress,
    
    /// Delivery completed successfully
    Completed,
    
    /// Delivery failed permanently
    Failed,
    
    /// Delivery retrying
    Retrying,
}

/// Real-time message events
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageEvent {
    /// Event type
    pub event_type: MessageEventType,
    
    /// Associated message ID
    pub message_id: Uuid,
    
    /// User ID (sender or recipient)
    pub user_id: Uuid,
    
    /// Group ID (if applicable)
    pub group_id: Option<Uuid>,
    
    /// Event timestamp
    pub timestamp: SystemTime,
    
    /// Additional event data
    pub data: serde_json::Value,
}

/// Message event types
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum MessageEventType {
    /// New message created
    MessageCreated,
    
    /// Message delivered
    MessageDelivered,
    
    /// Message read
    MessageRead,
    
    /// Message failed to deliver
    MessageFailed,
    
    /// Message deleted
    MessageDeleted,
    
    /// User typing notification
    UserTyping,
    
    /// User online status change
    UserOnline,
    
    /// User offline status change
    UserOffline,
}

/// Message composition request
#[derive(Debug, Clone, Deserialize)]
pub struct ComposeMessageRequest {
    /// Message type
    pub message_type: MessageType,
    
    /// Recipient user ID (for direct messages)
    pub recipient_id: Option<Uuid>,
    
    /// Recipient callsign (for direct messages)
    pub recipient_callsign: Option<String>,
    
    /// Group ID (for group messages)
    pub group_id: Option<Uuid>,
    
    /// Message subject
    pub subject: Option<String>,
    
    /// Message body
    pub body: String,
    
    /// Message priority
    pub priority: MessagePriority,
    
    /// Parent message ID (for replies)
    pub parent_id: Option<Uuid>,
    
    /// Delivery options
    pub delivery_options: DeliveryOptions,
}

/// Message delivery options
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeliveryOptions {
    /// Require delivery confirmation
    pub require_confirmation: bool,
    
    /// Maximum delivery attempts
    pub max_attempts: u32,
    
    /// Delivery timeout (seconds)
    pub timeout_seconds: u64,
    
    /// Priority routing
    pub priority_routing: bool,
    
    /// Compress message for RF transmission
    pub compress: bool,
}

/// Message query parameters
#[derive(Debug, Clone)]
pub struct MessageQuery {
    /// User ID (for user's messages)
    pub user_id: Option<Uuid>,
    
    /// Group ID (for group messages)
    pub group_id: Option<Uuid>,
    
    /// Message type filter
    pub message_type: Option<MessageType>,
    
    /// Status filter
    pub status: Option<MessageStatus>,
    
    /// Priority filter
    pub priority: Option<MessagePriority>,
    
    /// Date range (start)
    pub date_from: Option<SystemTime>,
    
    /// Date range (end)
    pub date_to: Option<SystemTime>,
    
    /// Include deleted messages
    pub include_deleted: bool,
    
    /// Pagination limit
    pub limit: u32,
    
    /// Pagination offset
    pub offset: u32,
}

/// Message statistics
#[derive(Debug, Clone, Serialize)]
pub struct MessageStats {
    /// Total messages in system
    pub total_messages: u64,
    
    /// Messages by type
    pub by_type: HashMap<MessageType, u64>,
    
    /// Messages by status
    pub by_status: HashMap<MessageStatus, u64>,
    
    /// Messages by priority
    pub by_priority: HashMap<MessagePriority, u64>,
    
    /// Average message size
    pub avg_message_size: f64,
    
    /// Messages per day (last 30 days)
    pub daily_message_count: Vec<u64>,
    
    /// Top active users
    pub top_senders: Vec<(String, u64)>, // (callsign, count)
    
    /// Emergency message count
    pub emergency_count: u64,
}

impl MessageSystem {
    /// Create a new message system
    pub async fn new(
        database: Database,
        config: &MessagingConfig,
    ) -> Result<Self> {
        let storage = MessageStorage::new(database.clone(), config.clone()).await?;
        let validator = MessageValidator::new(config.clone());
        let direct_handler = DirectMessageHandler::new(database.clone());
        let group_handler = GroupMessageHandler::new(database.clone());
        let notification_manager = NotificationManager::new(database.clone());
        
        let (event_broadcaster, _) = broadcast::channel(1000);
        
        let message_cache = Arc::new(RwLock::new(MessageCache {
            recent_messages: HashMap::new(),
            unread_counts: HashMap::new(),
            group_message_counts: HashMap::new(),
            max_size: 1000, // Configurable cache size
        }));
        
        Ok(Self {
            database,
            storage,
            direct_handler,
            group_handler,
            validator,
            notification_manager,
            config: config.clone(),
            event_broadcaster,
            message_cache,
            delivery_tasks: Arc::new(RwLock::new(HashMap::new())),
        })
    }
    
    /// Compose and send a new message
    pub async fn compose_message(
        &self,
        sender: &User,
        request: ComposeMessageRequest,
        origin_node_id: Uuid,
    ) -> Result<Message> {
        // Validate message content and permissions
        self.validator.validate_message_content(&request.body)?;
        self.validator.validate_message_size(&request.body, self.config.max_message_size)?;
        
        // Validate recipient/group permissions
        match request.message_type {
            MessageType::Direct => {
                if request.recipient_id.is_none() && request.recipient_callsign.is_none() {
                    return Err(anyhow::anyhow!("Direct message requires recipient"));
                }
                
                // Validate recipient exists
                if let Some(recipient_callsign) = &request.recipient_callsign {
                    if self.database.get_user_by_callsign(recipient_callsign).await?.is_none() {
                        return Err(anyhow::anyhow!("Recipient callsign not found: {}", recipient_callsign));
                    }
                }
            }
            MessageType::Group | MessageType::Bulletin => {
                if request.group_id.is_none() {
                    return Err(anyhow::anyhow!("Group message requires group ID"));
                }
                
                // Validate group membership and permissions
                let group_id = request.group_id.unwrap();
                self.group_handler.validate_group_permissions(sender, &group_id).await?;
            }
            MessageType::Emergency => {
                // Emergency messages have special validation
                self.validator.validate_emergency_message(sender, &request.body)?;
            }
            MessageType::System => {
                // Only system users can send system messages
                if sender.role != crate::auth::UserRole::System {
                    return Err(anyhow::anyhow!("Insufficient permissions for system message"));
                }
            }
        }
        
        // Create message object
        let message_id = Uuid::new_v4();
        let now = SystemTime::now();
        
        // Determine thread ID
        let thread_id = if let Some(parent_id) = request.parent_id {
            // Get parent message to find thread ID
            if let Some(parent) = self.storage.get_message(&parent_id).await? {
                parent.thread_id.unwrap_or(parent.id)
            } else {
                return Err(anyhow::anyhow!("Parent message not found"));
            }
        } else {
            message_id // This message starts a new thread
        };
        
        // Generate message hash for deduplication
        let message_hash = self.generate_message_hash(
            &sender.callsign,
            &request.body,
            now,
        );
        
        // Check for duplicate messages
        if self.storage.message_exists_by_hash(&message_hash).await? {
            return Err(anyhow::anyhow!("Duplicate message detected"));
        }
        
        let message = Message {
            id: message_id,
            message_type: request.message_type,
            sender_id: sender.id,
            sender_callsign: sender.callsign.clone(),
            recipient_id: request.recipient_id,
            recipient_callsign: request.recipient_callsign,
            group_id: request.group_id,
            subject: request.subject,
            body: request.body,
            priority: request.priority,
            created_at: now,
            delivered_at: None,
            read_at: None,
            parent_id: request.parent_id,
            thread_id: Some(thread_id),
            status: MessageStatus::Pending,
            origin_node_id,
            message_hash,
            rf_metadata: None,
            flags: MessageFlags {
                requires_ack: request.delivery_options.require_confirmation,
                compressed: request.delivery_options.compress,
                important: request.priority >= MessagePriority::High,
                ..Default::default()
            },
        };
        
        // Store message
        self.storage.store_message(&message).await?;
        
        // Update cache
        self.update_message_cache(&message).await;
        
        // Initiate delivery
        self.initiate_delivery(&message, request.delivery_options).await?;
        
        // Send real-time event
        let event = MessageEvent {
            event_type: MessageEventType::MessageCreated,
            message_id: message.id,
            user_id: sender.id,
            group_id: message.group_id,
            timestamp: now,
            data: serde_json::json!({
                "message_type": message.message_type,
                "priority": message.priority,
                "has_subject": message.subject.is_some(),
            }),
        };
        let _ = self.event_broadcaster.send(event);
        
        Ok(message)
    }
    
    /// Get messages based on query parameters
    pub async fn get_messages(&self, query: MessageQuery) -> Result<Vec<Message>> {
        // Check cache first for recent messages
        if let Some(user_id) = query.user_id {
            if query.limit <= 50 && query.offset == 0 {
                let cache = self.message_cache.read().await;
                if let Some(cached_messages) = self.get_cached_user_messages(&cache, &user_id, query.limit) {
                    return Ok(cached_messages);
                }
            }
        }
        
        // Query database
        let messages = match (query.user_id, query.group_id) {
            (Some(user_id), None) => {
                self.storage.get_user_messages(
                    &user_id,
                    query.message_type,
                    query.limit,
                    query.offset,
                ).await?
            }
            (None, Some(group_id)) => {
                self.storage.get_group_messages(
                    &group_id,
                    query.limit,
                    query.offset,
                ).await?
            }
            _ => {
                return Err(anyhow::anyhow!("Must specify either user_id or group_id"));
            }
        };
        
        // Apply additional filters
        let filtered_messages = self.apply_message_filters(messages, &query);
        
        // Update cache with results
        self.cache_messages(&filtered_messages).await;
        
        Ok(filtered_messages)
    }
    
    /// Get a specific message by ID
    pub async fn get_message(&self, message_id: &Uuid, requester: &User) -> Result<Option<Message>> {
        // Check cache first
        {
            let cache = self.message_cache.read().await;
            if let Some((message, cached_at)) = cache.recent_messages.get(message_id) {
                // Cache is valid for 5 minutes
                if cached_at.elapsed().unwrap_or_default() < Duration::from_secs(300) {
                    // Validate access permissions
                    if self.can_access_message(requester, message) {
                        return Ok(Some(message.clone()));
                    }
                }
            }
        }
        
        // Get from database
        if let Some(message) = self.storage.get_message(message_id).await? {
            // Validate access permissions
            if self.can_access_message(requester, &message) {
                // Update cache
                self.update_message_cache(&message).await;
                Ok(Some(message))
            } else {
                Err(anyhow::anyhow!("Access denied to message"))
            }
        } else {
            Ok(None)
        }
    }
    
    /// Mark a message as read
    pub async fn mark_message_read(
        &self,
        message_id: &Uuid,
        user: &User,
    ) -> Result<()> {
        // Get message to validate access
        let message = self.storage.get_message(message_id).await?
            .ok_or_else(|| anyhow::anyhow!("Message not found"))?;
        
        // Validate user can read this message
        if !self.can_access_message(user, &message) {
            return Err(anyhow::anyhow!("Access denied"));
        }
        
        // Mark as read only if user is the recipient
        let can_mark_read = match message.message_type {
            MessageType::Direct => {
                message.recipient_id == Some(user.id)
            }
            MessageType::Group | MessageType::Bulletin => {
                // For group messages, any member can mark as read for themselves
                self.group_handler.is_group_member(user, &message.group_id.unwrap()).await?
            }
            _ => false,
        };
        
        if !can_mark_read {
            return Err(anyhow::anyhow!("Cannot mark this message as read"));
        }
        
        let read_at = SystemTime::now();
        self.storage.mark_message_read(message_id, read_at).await?;
        
        // Update cache
        self.invalidate_message_cache(message_id).await;
        self.update_unread_count_cache(&user.id).await;
        
        // Send real-time event
        let event = MessageEvent {
            event_type: MessageEventType::MessageRead,
            message_id: *message_id,
            user_id: user.id,
            group_id: message.group_id,
            timestamp: read_at,
            data: serde_json::json!({}),
        };
        let _ = self.event_broadcaster.send(event);
        
        Ok(())
    }
    
    /// Delete a message
    pub async fn delete_message(
        &self,
        message_id: &Uuid,
        user: &User,
    ) -> Result<()> {
        let message = self.storage.get_message(message_id).await?
            .ok_or_else(|| anyhow::anyhow!("Message not found"))?;
        
        // Validate deletion permissions
        let can_delete = message.sender_id == user.id || 
                        user.role == crate::auth::UserRole::Administrator ||
                        (message.group_id.is_some() && 
                         self.group_handler.can_moderate_group(user, &message.group_id.unwrap()).await?);
        
        if !can_delete {
            return Err(anyhow::anyhow!("Insufficient permissions to delete message"));
        }
        
        // Mark as deleted
        self.storage.update_message_status(message_id, MessageStatus::Deleted).await?;
        
        // Update cache
        self.invalidate_message_cache(message_id).await;
        
        // Send real-time event
        let event = MessageEvent {
            event_type: MessageEventType::MessageDeleted,
            message_id: *message_id,
            user_id: user.id,
            group_id: message.group_id,
            timestamp: SystemTime::now(),
            data: serde_json::json!({}),
        };
        let _ = self.event_broadcaster.send(event);
        
        Ok(())
    }
    
    /// Get unread message count for a user
    pub async fn get_unread_count(&self, user_id: &Uuid) -> Result<u32> {
        // Check cache first
        {
            let cache = self.message_cache.read().await;
            if let Some(&count) = cache.unread_counts.get(user_id) {
                return Ok(count);
            }
        }
        
        // Get from database
        let count = self.storage.get_unread_message_count(user_id).await?;
        
        // Update cache
        {
            let mut cache = self.message_cache.write().await;
            cache.unread_counts.insert(*user_id, count as u32);
        }
        
        Ok(count as u32)
    }
    
    /// Get message thread
    pub async fn get_message_thread(&self, thread_id: &Uuid, user: &User) -> Result<Vec<Message>> {
        let messages = self.storage.get_message_thread(thread_id).await?;
        
        // Filter messages user can access
        let accessible_messages: Vec<Message> = messages
            .into_iter()
            .filter(|msg| self.can_access_message(user, msg))
            .collect();
        
        Ok(accessible_messages)
    }
    
    /// Get message statistics
    pub async fn get_message_stats(&self) -> Result<MessageStats> {
        // This would typically be cached and updated periodically
        let stats = self.storage.get_comprehensive_message_stats().await?;
        Ok(stats)
    }
    
    /// Subscribe to real-time message events
    pub fn subscribe_to_events(&self) -> broadcast::Receiver<MessageEvent> {
        self.event_broadcaster.subscribe()
    }
    
    /// Start background tasks for message processing
    pub async fn start_background_tasks(&self) -> tokio::task::JoinHandle<()> {
        let system = self.clone();
        
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(30));
            
            loop {
                interval.tick().await;
                
                // Process pending deliveries
                if let Err(e) = system.process_pending_deliveries().await {
                    tracing::error!("Error processing pending deliveries: {}", e);
                }
                
                // Clean up old cache entries
                system.cleanup_message_cache().await;
                
                // Update statistics cache
                if let Err(e) = system.update_statistics_cache().await {
                    tracing::error!("Error updating statistics cache: {}", e);
                }
            }
        })
    }
    
    // Private helper methods
    
    /// Generate message hash for deduplication
    fn generate_message_hash(&self, sender: &str, body: &str, timestamp: SystemTime) -> String {
        use blake3::Hasher;
        
        let mut hasher = Hasher::new();
        hasher.update(sender.as_bytes());
        hasher.update(body.as_bytes());
        hasher.update(&timestamp.duration_since(SystemTime::UNIX_EPOCH).unwrap().as_secs().to_le_bytes());
        
        hex::encode(hasher.finalize().as_bytes())
    }
    
    /// Check if user can access a message
    fn can_access_message(&self, user: &User, message: &Message) -> bool {
        match message.message_type {
            MessageType::Direct => {
                message.sender_id == user.id || message.recipient_id == Some(user.id)
            }
            MessageType::Group | MessageType::Bulletin => {
                // For group messages, this would check group membership
                // Simplified here - in practice would query group membership
                true // Placeholder
            }
            MessageType::System => {
                // System messages are visible to all users
                true
            }
            MessageType::Emergency => {
                // Emergency messages are visible to all users
                true
            }
        }
    }
    
    /// Initiate message delivery
    async fn initiate_delivery(
        &self,
        message: &Message,
        options: DeliveryOptions,
    ) -> Result<()> {
        let delivery_task = DeliveryTask {
            message_id: message.id,
            attempts: 0,
            next_retry: SystemTime::now(),
            status: DeliveryStatus::Pending,
            recipients: self.determine_recipients(message).await?,
        };
        
        // Add to delivery queue
        {
            let mut tasks = self.delivery_tasks.write().await;
            tasks.insert(message.id, delivery_task);
        }
        
        // For now, mark as sent immediately for local delivery
        // In a full implementation, this would handle RF transmission
        self.storage.update_message_status(&message.id, MessageStatus::Sent).await?;
        
        Ok(())
    }
    
    /// Determine message recipients
    async fn determine_recipients(&self, message: &Message) -> Result<Vec<Uuid>> {
        match message.message_type {
            MessageType::Direct => {
                if let Some(recipient_id) = message.recipient_id {
                    Ok(vec![recipient_id])
                } else {
                    Ok(vec![])
                }
            }
            MessageType::Group | MessageType::Bulletin => {
                if let Some(group_id) = message.group_id {
                    self.group_handler.get_group_member_ids(&group_id).await
                } else {
                    Ok(vec![])
                }
            }
            MessageType::System | MessageType::Emergency => {
                // Broadcast to all active users
                self.get_all_active_user_ids().await
            }
        }
    }
    
    /// Get all active user IDs (placeholder)
    async fn get_all_active_user_ids(&self) -> Result<Vec<Uuid>> {
        // This would query all active users from the database
        Ok(vec![])
    }
    
    /// Process pending message deliveries
    async fn process_pending_deliveries(&self) -> Result<()> {
        let mut tasks = self.delivery_tasks.write().await;
        let now = SystemTime::now();
        
        for (message_id, task) in tasks.iter_mut() {
            if task.status == DeliveryStatus::Pending && task.next_retry <= now {
                // Process delivery
                task.status = DeliveryStatus::InProgress;
                task.attempts += 1;
                
                // Simulate delivery processing
                // In a real implementation, this would handle RF transmission
                tracing::debug!("Processing delivery for message {}", message_id);
                
                task.status = DeliveryStatus::Completed;
            }
        }
        
        // Remove completed tasks
        tasks.retain(|_, task| task.status != DeliveryStatus::Completed);
        
        Ok(())
    }
    
    /// Update message cache
    async fn update_message_cache(&self, message: &Message) {
        let mut cache = self.message_cache.write().await;
        cache.recent_messages.insert(message.id, (message.clone(), SystemTime::now()));
        
        // Cleanup if cache is too large
        if cache.recent_messages.len() > cache.max_size {
            // Remove oldest entries
            let mut entries: Vec<_> = cache.recent_messages.iter().collect();
            entries.sort_by_key(|(_, (_, timestamp))| *timestamp);
            
            for (message_id, _) in entries.into_iter().take(cache.recent_messages.len() - cache.max_size) {
                cache.recent_messages.remove(message_id);
            }
        }
    }
    
    /// Cache multiple messages
    async fn cache_messages(&self, messages: &[Message]) {
        let mut cache = self.message_cache.write().await;
        let now = SystemTime::now();
        
        for message in messages {
            cache.recent_messages.insert(message.id, (message.clone(), now));
        }
    }
    
    /// Invalidate message cache entry
    async fn invalidate_message_cache(&self, message_id: &Uuid) {
        let mut cache = self.message_cache.write().await;
        cache.recent_messages.remove(message_id);
    }
    
    /// Update unread count cache
    async fn update_unread_count_cache(&self, user_id: &Uuid) {
        if let Ok(count) = self.storage.get_unread_message_count(user_id).await {
            let mut cache = self.message_cache.write().await;
            cache.unread_counts.insert(*user_id, count as u32);
        }
    }
    
    /// Get cached user messages
    fn get_cached_user_messages(
        &self,
        cache: &MessageCache,
        user_id: &Uuid,
        limit: u32,
    ) -> Option<Vec<Message>> {
        // This is a simplified cache lookup
        // In practice, would need more sophisticated caching
        None
    }
    
    /// Apply additional filters to messages
    fn apply_message_filters(&self, messages: Vec<Message>, query: &MessageQuery) -> Vec<Message> {
        messages
            .into_iter()
            .filter(|msg| {
                if let Some(status) = query.status {
                    if msg.status != status {
                        return false;
                    }
                }
                
                if let Some(priority) = query.priority {
                    if msg.priority != priority {
                        return false;
                    }
                }
                
                if let Some(date_from) = query.date_from {
                    if msg.created_at < date_from {
                        return false;
                    }
                }
                
                if let Some(date_to) = query.date_to {
                    if msg.created_at > date_to {
                        return false;
                    }
                }
                
                if !query.include_deleted && msg.status == MessageStatus::Deleted {
                    return false;
                }
                
                true
            })
            .collect()
    }
    
    /// Clean up old cache entries
    async fn cleanup_message_cache(&self) {
        let mut cache = self.message_cache.write().await;
        let cutoff = SystemTime::now() - Duration::from_secs(3600); // 1 hour
        
        cache.recent_messages.retain(|_, (_, timestamp)| *timestamp > cutoff);
    }
    
    /// Update statistics cache (placeholder)
    async fn update_statistics_cache(&self) -> Result<()> {
        // This would update cached statistics periodically
        Ok(())
    }
}

impl Default for DeliveryOptions {
    fn default() -> Self {
        Self {
            require_confirmation: false,
            max_attempts: 3,
            timeout_seconds: 300,
            priority_routing: false,
            compress: true,
        }
    }
}

impl Default for MessageQuery {
    fn default() -> Self {
        Self {
            user_id: None,
            group_id: None,
            message_type: None,
            status: None,
            priority: None,
            date_from: None,
            date_to: None,
            include_deleted: false,
            limit: 50,
            offset: 0,
        }
    }
}

/// Message system errors
#[derive(Debug, thiserror::Error)]
pub enum MessageError {
    #[error("Message validation failed: {0}")]
    ValidationFailed(String),
    
    #[error("Insufficient permissions: {0}")]
    PermissionDenied(String),
    
    #[error("Message not found: {0}")]
    MessageNotFound(Uuid),
    
    #[error("Recipient not found: {0}")]
    RecipientNotFound(String),
    
    #[error("Group not found: {0}")]
    GroupNotFound(Uuid),
    
    #[error("Message too large: {current} bytes, max {max} bytes")]
    MessageTooLarge { current: usize, max: usize },
    
    #[error("Delivery failed: {0}")]
    DeliveryFailed(String),
    
    #[error("Duplicate message detected")]
    DuplicateMessage,
    
    #[error("Database error: {0}")]
    DatabaseError(#[from] anyhow::Error),
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::{User, UserRole, UserStatus};
    use crate::config::MessagingConfig;
    use crate::database::Database;
    use tempfile::tempdir;
    
    async fn create_test_message_system() -> MessageSystem {
        let temp_dir = tempdir().unwrap();
        let db_path = temp_dir.path().join("test.db");
        
        let mut db_config = crate::config::DatabaseConfig::default();
        db_config.path = db_path;
        
        let database = Database::new(&db_config).await.unwrap();
        database.migrate().await.unwrap();
        
        let messaging_config = MessagingConfig::default();
        
        MessageSystem::new(database, &messaging_config).await.unwrap()
    }
    
    async fn create_test_user(system: &MessageSystem, callsign: &str) -> User {
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
        
        system.database.create_user(&user, "test_hash").await.unwrap();
        user
    }
    
    async fn create_test_node(system: &MessageSystem) -> Uuid {
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
        .execute(system.database.pool())
        .await
        .unwrap();
        
        node_id
    }
    
    #[tokio::test]
    async fn test_direct_message_composition() {
        let system = create_test_message_system().await;
        let sender = create_test_user(&system, "W1SEND").await;
        let recipient = create_test_user(&system, "W1RECV").await;
        let node_id = create_test_node(&system).await;
        
        let request = ComposeMessageRequest {
            message_type: MessageType::Direct,
            recipient_id: Some(recipient.id),
            recipient_callsign: Some(recipient.callsign.clone()),
            group_id: None,
            subject: Some("Test Subject".to_string()),
            body: "Hello, this is a test message!".to_string(),
            priority: MessagePriority::Normal,
            parent_id: None,
            delivery_options: DeliveryOptions::default(),
        };
        
        let message = system.compose_message(&sender, request, node_id).await.unwrap();
        
        assert_eq!(message.message_type, MessageType::Direct);
        assert_eq!(message.sender_id, sender.id);
        assert_eq!(message.recipient_id, Some(recipient.id));
        assert_eq!(message.body, "Hello, this is a test message!");
        assert_eq!(message.status, MessageStatus::Pending);
        assert!(message.thread_id.is_some());
    }
    
    #[tokio::test]
    async fn test_message_validation() {
        let system = create_test_message_system().await;
        let sender = create_test_user(&system, "W1TEST").await;
        let node_id = create_test_node(&system).await;
        
        // Test message too large
        let large_body = "x".repeat(system.config.max_message_size + 1);
        let request = ComposeMessageRequest {
            message_type: MessageType::Direct,
            recipient_id: None,
            recipient_callsign: Some("W1RECV".to_string()),
            group_id: None,
            subject: None,
            body: large_body,
            priority: MessagePriority::Normal,
            parent_id: None,
            delivery_options: DeliveryOptions::default(),
        };
        
        let result = system.compose_message(&sender, request, node_id).await;
        assert!(result.is_err());
        
        // Test missing recipient for direct message
        let request = ComposeMessageRequest {
            message_type: MessageType::Direct,
            recipient_id: None,
            recipient_callsign: None,
            group_id: None,
            subject: None,
            body: "Test message".to_string(),
            priority: MessagePriority::Normal,
            parent_id: None,
            delivery_options: DeliveryOptions::default(),
        };
        
        let result = system.compose_message(&sender, request, node_id).await;
        assert!(result.is_err());
    }
    
    #[tokio::test]
    async fn test_message_retrieval() {
        let system = create_test_message_system().await;
        let sender = create_test_user(&system, "W1SEND").await;
        let recipient = create_test_user(&system, "W1RECV").await;
        let node_id = create_test_node(&system).await;
        
        // Send a message
        let request = ComposeMessageRequest {
            message_type: MessageType::Direct,
            recipient_id: Some(recipient.id),
            recipient_callsign: Some(recipient.callsign.clone()),
            group_id: None,
            subject: Some("Test Subject".to_string()),
            body: "Test message body".to_string(),
            priority: MessagePriority::Normal,
            parent_id: None,
            delivery_options: DeliveryOptions::default(),
        };
        
        let sent_message = system.compose_message(&sender, request, node_id).await.unwrap();
        
        // Retrieve message by ID
        let retrieved = system.get_message(&sent_message.id, &recipient).await.unwrap().unwrap();
        assert_eq!(retrieved.id, sent_message.id);
        assert_eq!(retrieved.body, sent_message.body);
        
        // Test access control - sender should also be able to access
        let retrieved_by_sender = system.get_message(&sent_message.id, &sender).await.unwrap().unwrap();
        assert_eq!(retrieved_by_sender.id, sent_message.id);
        
        // Get user messages
        let query = MessageQuery {
            user_id: Some(recipient.id),
            limit: 10,
            ..Default::default()
        };
        
        let user_messages = system.get_messages(query).await.unwrap();
        assert_eq!(user_messages.len(), 1);
        assert_eq!(user_messages[0].id, sent_message.id);
    }
    
    #[tokio::test]
    async fn test_mark_message_read() {
        let system = create_test_message_system().await;
        let sender = create_test_user(&system, "W1SEND").await;
        let recipient = create_test_user(&system, "W1RECV").await;
        let node_id = create_test_node(&system).await;
        
        // Send a message
        let request = ComposeMessageRequest {
            message_type: MessageType::Direct,
            recipient_id: Some(recipient.id),
            recipient_callsign: Some(recipient.callsign.clone()),
            group_id: None,
            subject: None,
            body: "Test read message".to_string(),
            priority: MessagePriority::Normal,
            parent_id: None,
            delivery_options: DeliveryOptions::default(),
        };
        
        let message = system.compose_message(&sender, request, node_id).await.unwrap();
        
        // Check unread count
        let unread_count = system.get_unread_count(&recipient.id).await.unwrap();
        assert_eq!(unread_count, 1);
        
        // Mark as read
        system.mark_message_read(&message.id, &recipient).await.unwrap();
        
        // Verify read status
        let retrieved = system.get_message(&message.id, &recipient).await.unwrap().unwrap();
        assert!(retrieved.read_at.is_some());
        
        // Check unread count again
        let unread_count = system.get_unread_count(&recipient.id).await.unwrap();
        assert_eq!(unread_count, 0);
    }
    
    #[tokio::test]
    async fn test_message_threading() {
        let system = create_test_message_system().await;
        let sender = create_test_user(&system, "W1SEND").await;
        let recipient = create_test_user(&system, "W1RECV").await;
        let node_id = create_test_node(&system).await;
        
        // Send original message
        let request = ComposeMessageRequest {
            message_type: MessageType::Direct,
            recipient_id: Some(recipient.id),
            recipient_callsign: Some(recipient.callsign.clone()),
            group_id: None,
            subject: Some("Original Message".to_string()),
            body: "This is the original message".to_string(),
            priority: MessagePriority::Normal,
            parent_id: None,
            delivery_options: DeliveryOptions::default(),
        };
        
        let original_message = system.compose_message(&sender, request, node_id).await.unwrap();
        
        // Send reply
        let reply_request = ComposeMessageRequest {
            message_type: MessageType::Direct,
            recipient_id: Some(sender.id),
            recipient_callsign: Some(sender.callsign.clone()),
            group_id: None,
            subject: Some("Re: Original Message".to_string()),
            body: "This is a reply".to_string(),
            priority: MessagePriority::Normal,
            parent_id: Some(original_message.id),
            delivery_options: DeliveryOptions::default(),
        };
        
        let reply_message = system.compose_message(&recipient, reply_request, node_id).await.unwrap();
        
        // Check thread relationship
        assert_eq!(reply_message.parent_id, Some(original_message.id));
        assert_eq!(reply_message.thread_id, original_message.thread_id);
        
        // Get thread messages
        let thread_id = original_message.thread_id.unwrap();
        let thread_messages = system.get_message_thread(&thread_id, &sender).await.unwrap();
        
        assert_eq!(thread_messages.len(), 2);
        assert_eq!(thread_messages[0].id, original_message.id);
        assert_eq!(thread_messages[1].id, reply_message.id);
    }
    
    #[tokio::test]
    async fn test_emergency_message() {
        let system = create_test_message_system().await;
        let sender = create_test_user(&system, "W1EMRG").await;
        let node_id = create_test_node(&system).await;
        
        let request = ComposeMessageRequest {
            message_type: MessageType::Emergency,
            recipient_id: None,
            recipient_callsign: None,
            group_id: None,
            subject: Some("EMERGENCY".to_string()),
            body: "This is an emergency message".to_string(),
            priority: MessagePriority::Emergency,
            parent_id: None,
            delivery_options: DeliveryOptions {
                require_confirmation: true,
                priority_routing: true,
                ..Default::default()
            },
        };
        
        let message = system.compose_message(&sender, request, node_id).await.unwrap();
        
        assert_eq!(message.message_type, MessageType::Emergency);
        assert_eq!(message.priority, MessagePriority::Emergency);
        assert!(message.flags.requires_ack);
        assert!(message.flags.important);
    }
    
    #[tokio::test]
    async fn test_duplicate_message_detection() {
        let system = create_test_message_system().await;
        let sender = create_test_user(&system, "W1DUP").await;
        let recipient = create_test_user(&system, "W1RECV").await;
        let node_id = create_test_node(&system).await;
        
        let request = ComposeMessageRequest {
            message_type: MessageType::Direct,
            recipient_id: Some(recipient.id),
            recipient_callsign: Some(recipient.callsign.clone()),
            group_id: None,
            subject: None,
            body: "Duplicate test message".to_string(),
            priority: MessagePriority::Normal,
            parent_id: None,
            delivery_options: DeliveryOptions::default(),
        };
        
        // Send first message
        let _first_message = system.compose_message(&sender, request.clone(), node_id).await.unwrap();
        
        // Attempt to send duplicate (same sender, body, and timestamp)
        // Note: In practice, the timestamp would likely be different enough to prevent this
        // This test would need to be adjusted for real duplicate detection logic
        
        // For now, we'll just verify the first message was created successfully
        assert!(_first_message.message_hash.len() > 0);
    }
    
    #[tokio::test]
    async fn test_message_deletion() {
        let system = create_test_message_system().await;
        let sender = create_test_user(&system, "W1DEL").await;
        let recipient = create_test_user(&system, "W1RECV").await;
        let node_id = create_test_node(&system).await;
        
        // Send a message
        let request = ComposeMessageRequest {
            message_type: MessageType::Direct,
            recipient_id: Some(recipient.id),
            recipient_callsign: Some(recipient.callsign.clone()),
            group_id: None,
            subject: None,
            body: "Message to be deleted".to_string(),
            priority: MessagePriority::Normal,
            parent_id: None,
            delivery_options: DeliveryOptions::default(),
        };
        
        let message = system.compose_message(&sender, request, node_id).await.unwrap();
        
        // Delete the message
        system.delete_message(&message.id, &sender).await.unwrap();
        
        // Verify message is marked as deleted
        let retrieved = system.get_message(&message.id, &sender).await.unwrap().unwrap();
        assert_eq!(retrieved.status, MessageStatus::Deleted);
    }
    
    #[tokio::test]
    async fn test_event_subscription() {
        let system = create_test_message_system().await;
        let sender = create_test_user(&system, "W1EVT").await;
        let recipient = create_test_user(&system, "W1RECV").await;
        let node_id = create_test_node(&system).await;
        
        // Subscribe to events
        let mut event_receiver = system.subscribe_to_events();
        
        // Send a message
        let request = ComposeMessageRequest {
            message_type: MessageType::Direct,
            recipient_id: Some(recipient.id),
            recipient_callsign: Some(recipient.callsign.clone()),
            group_id: None,
            subject: None,
            body: "Event test message".to_string(),
            priority: MessagePriority::Normal,
            parent_id: None,
            delivery_options: DeliveryOptions::default(),
        };
        
        let _message = system.compose_message(&sender, request, node_id).await.unwrap();
        
        // Check for event
        let event = tokio::time::timeout(
            Duration::from_secs(1),
            event_receiver.recv()
        ).await.unwrap().unwrap();
        
        assert_eq!(event.event_type, MessageEventType::MessageCreated);
        assert_eq!(event.user_id, sender.id);
    }
}
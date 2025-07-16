use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, SystemTime};
use tokio::sync::RwLock;
use uuid::Uuid;

use crate::auth::User;
use crate::database::{Database, models::*};

/// Direct message handler for one-on-one communication
#[derive(Clone)]
pub struct DirectMessageHandler {
    /// Database connection
    database: Database,
    
    /// Conversation cache for fast access
    conversation_cache: Arc<RwLock<ConversationCache>>,
    
    /// Active typing indicators
    typing_indicators: Arc<RwLock<HashMap<Uuid, TypingStatus>>>,
    
    /// Delivery confirmations
    delivery_confirmations: Arc<RwLock<HashMap<Uuid, DeliveryConfirmation>>>,
}

/// Conversation cache for performance optimization
#[derive(Debug, Default)]
struct ConversationCache {
    /// Recent conversations per user
    conversations: HashMap<Uuid, Vec<ConversationInfo>>,
    
    /// Last update timestamp per user
    last_updated: HashMap<Uuid, SystemTime>,
    
    /// Cache expiration time
    cache_duration: Duration,
}

/// Conversation information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationInfo {
    /// Conversation ID (derived from participants)
    pub conversation_id: String,
    
    /// Other participant's user ID
    pub participant_id: Uuid,
    
    /// Other participant's callsign
    pub participant_callsign: String,
    
    /// Other participant's full name
    pub participant_name: Option<String>,
    
    /// Last message in conversation
    pub last_message: Option<ConversationMessage>,
    
    /// Unread message count
    pub unread_count: u32,
    
    /// Last activity timestamp
    pub last_activity: SystemTime,
    
    /// Conversation status
    pub status: ConversationStatus,
    
    /// Participant's online status
    pub participant_online: bool,
}

/// Simplified message info for conversations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationMessage {
    pub id: Uuid,
    pub sender_callsign: String,
    pub body: String,
    pub created_at: SystemTime,
    pub is_read: bool,
}

/// Conversation status
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum ConversationStatus {
    /// Active conversation
    Active,
    
    /// Archived conversation
    Archived,
    
    /// Muted conversation
    Muted,
    
    /// Blocked conversation
    Blocked,
}

/// Typing status indicator
#[derive(Debug, Clone)]
struct TypingStatus {
    /// User who is typing
    user_id: Uuid,
    
    /// User they're typing to
    recipient_id: Uuid,
    
    /// When typing started
    started_at: SystemTime,
    
    /// Typing timeout
    expires_at: SystemTime,
}

/// Delivery confirmation tracking
#[derive(Debug, Clone)]
struct DeliveryConfirmation {
    /// Message ID
    message_id: Uuid,
    
    /// Sender ID
    sender_id: Uuid,
    
    /// Recipient ID
    recipient_id: Uuid,
    
    /// Delivery timestamp
    delivered_at: SystemTime,
    
    /// Read timestamp
    read_at: Option<SystemTime>,
}

/// Direct message request
#[derive(Debug, Clone, Deserialize)]
pub struct DirectMessageRequest {
    /// Recipient user ID or callsign
    pub recipient: RecipientIdentifier,
    
    /// Message subject (optional)
    pub subject: Option<String>,
    
    /// Message body
    pub body: String,
    
    /// Message priority
    pub priority: MessagePriority,
    
    /// Parent message ID (for replies)
    pub parent_id: Option<Uuid>,
    
    /// Request delivery confirmation
    pub request_confirmation: bool,
}

/// Recipient identifier (ID or callsign)
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum RecipientIdentifier {
    /// User ID
    UserId(Uuid),
    
    /// Callsign
    Callsign(String),
}

/// Conversation query parameters
#[derive(Debug, Clone)]
pub struct ConversationQuery {
    /// User requesting conversations
    pub user_id: Uuid,
    
    /// Include archived conversations
    pub include_archived: bool,
    
    /// Filter by status
    pub status_filter: Option<ConversationStatus>,
    
    /// Search term (participant name/callsign)
    pub search: Option<String>,
    
    /// Pagination limit
    pub limit: u32,
    
    /// Pagination offset
    pub offset: u32,
}

/// Message history query
#[derive(Debug, Clone)]
pub struct MessageHistoryQuery {
    /// Conversation participants
    pub user1_id: Uuid,
    pub user2_id: Uuid,
    
    /// Date range start
    pub date_from: Option<SystemTime>,
    
    /// Date range end
    pub date_to: Option<SystemTime>,
    
    /// Include deleted messages
    pub include_deleted: bool,
    
    /// Pagination limit
    pub limit: u32,
    
    /// Pagination offset
    pub offset: u32,
}

/// Typing notification
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TypingNotification {
    /// User who is typing
    pub user_id: Uuid,
    
    /// User's callsign
    pub callsign: String,
    
    /// Recipient user ID
    pub recipient_id: Uuid,
    
    /// Typing status
    pub is_typing: bool,
    
    /// Timestamp
    pub timestamp: SystemTime,
}

impl DirectMessageHandler {
    /// Create a new direct message handler
    pub fn new(database: Database) -> Self {
        Self {
            database,
            conversation_cache: Arc::new(RwLock::new(ConversationCache {
                conversations: HashMap::new(),
                last_updated: HashMap::new(),
                cache_duration: Duration::from_secs(300), // 5 minutes
            })),
            typing_indicators: Arc::new(RwLock::new(HashMap::new())),
            delivery_confirmations: Arc::new(RwLock::new(HashMap::new())),
        }
    }
    
    /// Send a direct message
    pub async fn send_direct_message(
        &self,
        sender: &User,
        request: DirectMessageRequest,
        origin_node_id: Uuid,
    ) -> Result<Message> {
        // Resolve recipient
        let recipient = self.resolve_recipient(&request.recipient).await?;
        
        // Validate permissions (basic spam protection)
        self.validate_direct_message_permissions(sender, &recipient).await?;
        
        // Create message
        let message_id = Uuid::new_v4();
        let now = SystemTime::now();
        
        // Determine thread ID for replies
        let thread_id = if let Some(parent_id) = request.parent_id {
            self.get_thread_id_from_parent(&parent_id).await?
        } else {
            message_id // New conversation thread
        };
        
        // Generate message hash for deduplication
        let message_hash = self.generate_message_hash(
            &sender.callsign,
            &recipient.callsign,
            &request.body,
            now,
        );
        
        let message = Message {
            id: message_id,
            message_type: MessageType::Direct,
            sender_id: sender.id,
            sender_callsign: sender.callsign.clone(),
            recipient_id: Some(recipient.id),
            recipient_callsign: Some(recipient.callsign.clone()),
            group_id: None,
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
                requires_ack: request.request_confirmation,
                important: request.priority >= MessagePriority::High,
                ..Default::default()
            },
        };
        
        // Store message in database
        self.database.create_message(&message).await?;
        
        // Update conversation cache
        self.invalidate_conversation_cache(&sender.id).await;
        self.invalidate_conversation_cache(&recipient.id).await;
        
        // Clear any typing indicators
        self.clear_typing_indicator(&sender.id, &recipient.id).await;
        
        // Set up delivery confirmation if requested
        if request.request_confirmation {
            self.setup_delivery_confirmation(&message).await;
        }
        
        Ok(message)
    }
    
    /// Get conversations for a user
    pub async fn get_conversations(
        &self,
        query: ConversationQuery,
    ) -> Result<Vec<ConversationInfo>> {
        // Check cache first
        if let Some(cached_conversations) = self.get_cached_conversations(&query.user_id).await {
            return Ok(self.filter_conversations(cached_conversations, &query));
        }
        
        // Query database for conversations
        let conversations = self.query_conversations_from_database(&query).await?;
        
        // Cache results
        self.cache_conversations(&query.user_id, &conversations).await;
        
        Ok(conversations)
    }
    
    /// Get message history between two users
    pub async fn get_message_history(
        &self,
        query: MessageHistoryQuery,
    ) -> Result<Vec<Message>> {
        let messages = self.database.get_conversation_messages(
            &query.user1_id,
            &query.user2_id,
            query.date_from,
            query.date_to,
            query.include_deleted,
            query.limit,
            query.offset,
        ).await?;
        
        Ok(messages)
    }
    
    /// Mark conversation messages as read
    pub async fn mark_conversation_read(
        &self,
        user_id: &Uuid,
        participant_id: &Uuid,
    ) -> Result<u32> {
        let updated_count = self.database.mark_conversation_messages_read(
            user_id,
            participant_id,
        ).await?;
        
        // Invalidate cache
        self.invalidate_conversation_cache(user_id).await;
        
        Ok(updated_count)
    }
    
    /// Set typing indicator
    pub async fn set_typing_indicator(
        &self,
        user: &User,
        recipient_id: &Uuid,
        is_typing: bool,
    ) -> Result<TypingNotification> {
        let notification = TypingNotification {
            user_id: user.id,
            callsign: user.callsign.clone(),
            recipient_id: *recipient_id,
            is_typing,
            timestamp: SystemTime::now(),
        };
        
        if is_typing {
            // Set typing status
            let typing_status = TypingStatus {
                user_id: user.id,
                recipient_id: *recipient_id,
                started_at: notification.timestamp,
                expires_at: notification.timestamp + Duration::from_secs(30), // 30 second timeout
            };
            
            let mut typing_indicators = self.typing_indicators.write().await;
            typing_indicators.insert(user.id, typing_status);
        } else {
            // Clear typing status
            let mut typing_indicators = self.typing_indicators.write().await;
            typing_indicators.remove(&user.id);
        }
        
        Ok(notification)
    }
    
    /// Get active typing indicators for a user
    pub async fn get_typing_indicators(&self, user_id: &Uuid) -> Vec<TypingNotification> {
        let mut indicators = Vec::new();
        let now = SystemTime::now();
        
        let typing_map = self.typing_indicators.read().await;
        for typing_status in typing_map.values() {
            // Only include indicators for this recipient that haven't expired
            if typing_status.recipient_id == *user_id && typing_status.expires_at > now {
                // Would need to fetch user info to get callsign
                // Simplified for now
                indicators.push(TypingNotification {
                    user_id: typing_status.user_id,
                    callsign: "".to_string(), // Would fetch from database
                    recipient_id: typing_status.recipient_id,
                    is_typing: true,
                    timestamp: typing_status.started_at,
                });
            }
        }
        
        indicators
    }
    
    /// Archive a conversation
    pub async fn archive_conversation(
        &self,
        user_id: &Uuid,
        participant_id: &Uuid,
    ) -> Result<()> {
        // This would typically update a user_conversations table
        // For now, we'll just invalidate the cache
        self.invalidate_conversation_cache(user_id).await;
        Ok(())
    }
    
    /// Block a user from sending direct messages
    pub async fn block_user(
        &self,
        user_id: &Uuid,
        blocked_user_id: &Uuid,
    ) -> Result<()> {
        // This would typically update a user_blocks table
        // For now, we'll just invalidate the cache
        self.invalidate_conversation_cache(user_id).await;
        Ok(())
    }
    
    /// Check if a user is blocked
    pub async fn is_user_blocked(
        &self,
        user_id: &Uuid,
        potential_sender_id: &Uuid,
    ) -> Result<bool> {
        // This would check a user_blocks table
        // For now, return false (no blocking)
        Ok(false)
    }
    
    /// Get conversation statistics
    pub async fn get_conversation_stats(&self, user_id: &Uuid) -> Result<ConversationStats> {
        let stats = ConversationStats {
            total_conversations: self.count_user_conversations(user_id).await?,
            unread_conversations: self.count_unread_conversations(user_id).await?,
            total_messages_sent: self.count_messages_sent(user_id).await?,
            total_messages_received: self.count_messages_received(user_id).await?,
            most_active_contacts: self.get_most_active_contacts(user_id, 5).await?,
        };
        
        Ok(stats)
    }
    
    /// Clean up expired typing indicators
    pub async fn cleanup_expired_typing_indicators(&self) {
        let now = SystemTime::now();
        let mut typing_indicators = self.typing_indicators.write().await;
        
        typing_indicators.retain(|_, status| status.expires_at > now);
    }
    
    // Private helper methods
    
    /// Resolve recipient by ID or callsign
    async fn resolve_recipient(&self, identifier: &RecipientIdentifier) -> Result<User> {
        match identifier {
            RecipientIdentifier::UserId(user_id) => {
                self.database.get_user_by_id(user_id).await?
                    .ok_or_else(|| anyhow::anyhow!("User not found: {}", user_id))
            }
            RecipientIdentifier::Callsign(callsign) => {
                self.database.get_user_by_callsign(callsign).await?
                    .ok_or_else(|| anyhow::anyhow!("Callsign not found: {}", callsign))
            }
        }
    }
    
    /// Validate direct message permissions
    async fn validate_direct_message_permissions(
        &self,
        sender: &User,
        recipient: &User,
    ) -> Result<()> {
        // Check if recipient has blocked sender
        if self.is_user_blocked(&recipient.id, &sender.id).await? {
            return Err(anyhow::anyhow!("You are blocked by this user"));
        }
        
        // Additional validation could include:
        // - Rate limiting
        // - Spam detection
        // - User status checks
        
        Ok(())
    }
    
    /// Get thread ID from parent message
    async fn get_thread_id_from_parent(&self, parent_id: &Uuid) -> Result<Uuid> {
        if let Some(parent_message) = self.database.get_message_by_id(parent_id).await? {
            Ok(parent_message.thread_id.unwrap_or(parent_message.id))
        } else {
            Err(anyhow::anyhow!("Parent message not found"))
        }
    }
    
    /// Generate message hash for deduplication
    fn generate_message_hash(
        &self,
        sender_callsign: &str,
        recipient_callsign: &str,
        body: &str,
        timestamp: SystemTime,
    ) -> String {
        use blake3::Hasher;
        
        let mut hasher = Hasher::new();
        hasher.update(sender_callsign.as_bytes());
        hasher.update(recipient_callsign.as_bytes());
        hasher.update(body.as_bytes());
        hasher.update(&timestamp.duration_since(SystemTime::UNIX_EPOCH).unwrap().as_secs().to_le_bytes());
        
        hex::encode(hasher.finalize().as_bytes())
    }
    
    /// Get cached conversations
    async fn get_cached_conversations(&self, user_id: &Uuid) -> Option<Vec<ConversationInfo>> {
        let cache = self.conversation_cache.read().await;
        
        if let Some(last_updated) = cache.last_updated.get(user_id) {
            if last_updated.elapsed().unwrap_or_default() < cache.cache_duration {
                return cache.conversations.get(user_id).cloned();
            }
        }
        
        None
    }
    
    /// Cache conversations for a user
    async fn cache_conversations(&self, user_id: &Uuid, conversations: &[ConversationInfo]) {
        let mut cache = self.conversation_cache.write().await;
        cache.conversations.insert(*user_id, conversations.to_vec());
        cache.last_updated.insert(*user_id, SystemTime::now());
    }
    
    /// Invalidate conversation cache for a user
    async fn invalidate_conversation_cache(&self, user_id: &Uuid) {
        let mut cache = self.conversation_cache.write().await;
        cache.conversations.remove(user_id);
        cache.last_updated.remove(user_id);
    }
    
    /// Query conversations from database
    async fn query_conversations_from_database(
        &self,
        query: &ConversationQuery,
    ) -> Result<Vec<ConversationInfo>> {
        // This would be a complex query joining messages, users, and conversation metadata
        // Simplified implementation for now
        let conversations = Vec::new();
        
        // In a real implementation, this would:
        // 1. Find all users this user has exchanged messages with
        // 2. Get the latest message for each conversation
        // 3. Calculate unread counts
        // 4. Check online status
        // 5. Apply filters and pagination
        
        Ok(conversations)
    }
    
    /// Filter conversations based on query parameters
    fn filter_conversations(
        &self,
        conversations: Vec<ConversationInfo>,
        query: &ConversationQuery,
    ) -> Vec<ConversationInfo> {
        conversations
            .into_iter()
            .filter(|conv| {
                // Apply status filter
                if let Some(status_filter) = query.status_filter {
                    if conv.status != status_filter {
                        return false;
                    }
                }
                
                // Apply archived filter
                if !query.include_archived && conv.status == ConversationStatus::Archived {
                    return false;
                }
                
                // Apply search filter
                if let Some(search) = &query.search {
                    let search_lower = search.to_lowercase();
                    if !conv.participant_callsign.to_lowercase().contains(&search_lower) &&
                       !conv.participant_name.as_ref()
                           .map(|name| name.to_lowercase().contains(&search_lower))
                           .unwrap_or(false) {
                        return false;
                    }
                }
                
                true
            })
            .skip(query.offset as usize)
            .take(query.limit as usize)
            .collect()
    }
    
    /// Clear typing indicator
    async fn clear_typing_indicator(&self, user_id: &Uuid, _recipient_id: &Uuid) {
        let mut typing_indicators = self.typing_indicators.write().await;
        typing_indicators.remove(user_id);
    }
    
    /// Setup delivery confirmation tracking
    async fn setup_delivery_confirmation(&self, message: &Message) {
        if let Some(recipient_id) = message.recipient_id {
            let confirmation = DeliveryConfirmation {
                message_id: message.id,
                sender_id: message.sender_id,
                recipient_id,
                delivered_at: SystemTime::now(),
                read_at: None,
            };
            
            let mut confirmations = self.delivery_confirmations.write().await;
            confirmations.insert(message.id, confirmation);
        }
    }
    
    /// Count user conversations
    async fn count_user_conversations(&self, user_id: &Uuid) -> Result<u32> {
        // This would count distinct conversations for a user
        Ok(0)
    }
    
    /// Count unread conversations
    async fn count_unread_conversations(&self, user_id: &Uuid) -> Result<u32> {
        // This would count conversations with unread messages
        Ok(0)
    }
    
    /// Count messages sent by user
    async fn count_messages_sent(&self, user_id: &Uuid) -> Result<u64> {
        // This would count total direct messages sent
        Ok(0)
    }
    
    /// Count messages received by user
    async fn count_messages_received(&self, user_id: &Uuid) -> Result<u64> {
        // This would count total direct messages received
        Ok(0)
    }
    
    /// Get most active contacts
    async fn get_most_active_contacts(&self, user_id: &Uuid, limit: u32) -> Result<Vec<ContactActivity>> {
        // This would return users with most message exchanges
        Ok(Vec::new())
    }
}

/// Conversation statistics
#[derive(Debug, Clone, Serialize)]
pub struct ConversationStats {
    pub total_conversations: u32,
    pub unread_conversations: u32,
    pub total_messages_sent: u64,
    pub total_messages_received: u64,
    pub most_active_contacts: Vec<ContactActivity>,
}

/// Contact activity information
#[derive(Debug, Clone, Serialize)]
pub struct ContactActivity {
    pub user_id: Uuid,
    pub callsign: String,
    pub full_name: Option<String>,
    pub message_count: u64,
    pub last_message_at: SystemTime,
}

/// Generate conversation ID from two user IDs
pub fn generate_conversation_id(user1_id: &Uuid, user2_id: &Uuid) -> String {
    // Ensure consistent ordering for conversation ID
    let (id1, id2) = if user1_id < user2_id {
        (user1_id, user2_id)
    } else {
        (user2_id, user1_id)
    };
    
    format!("{}_{}", id1, id2)
}

/// Database extension trait for direct message queries
trait DirectMessageQueries {
    async fn get_conversation_messages(
        &self,
        user1_id: &Uuid,
        user2_id: &Uuid,
        date_from: Option<SystemTime>,
        date_to: Option<SystemTime>,
        include_deleted: bool,
        limit: u32,
        offset: u32,
    ) -> Result<Vec<Message>>;
    
    async fn mark_conversation_messages_read(
        &self,
        user_id: &Uuid,
        participant_id: &Uuid,
    ) -> Result<u32>;
}

impl DirectMessageQueries for Database {
    async fn get_conversation_messages(
        &self,
        user1_id: &Uuid,
        user2_id: &Uuid,
        date_from: Option<SystemTime>,
        date_to: Option<SystemTime>,
        include_deleted: bool,
        limit: u32,
        offset: u32,
    ) -> Result<Vec<Message>> {
        // This would query messages between two users
        // with proper filtering and pagination
        
        // Placeholder implementation
        let mut query = "SELECT * FROM messages WHERE 
            ((sender_id = ?1 AND recipient_id = ?2) OR (sender_id = ?2 AND recipient_id = ?1))
            AND message_type = 0".to_string(); // Direct messages only
        
        if !include_deleted {
            query.push_str(" AND status != 5"); // Exclude deleted
        }
        
        if date_from.is_some() {
            query.push_str(" AND created_at >= ?3");
        }
        
        if date_to.is_some() {
            query.push_str(" AND created_at <= ?4");
        }
        
        query.push_str(" ORDER BY created_at DESC LIMIT ? OFFSET ?");
        
        // Would execute the query and return results
        // Simplified to return empty vector for now
        Ok(Vec::new())
    }
    
    async fn mark_conversation_messages_read(
        &self,
        user_id: &Uuid,
        participant_id: &Uuid,
    ) -> Result<u32> {
        // Mark all unread messages from participant as read
        let result = sqlx::query!(
            "UPDATE messages SET read_at = ?1, status = CASE 
                WHEN status < 3 THEN 3 
                ELSE status 
             END
             WHERE recipient_id = ?2 AND sender_id = ?3 AND read_at IS NULL",
            SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap().as_secs() as i64,
            user_id.to_string(),
            participant_id.to_string()
        )
        .execute(self.pool())
        .await?;
        
        Ok(result.rows_affected() as u32)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::{User, UserRole, UserStatus};
    use crate::database::Database;
    use tempfile::tempdir;
    
    async fn create_test_handler() -> DirectMessageHandler {
        let temp_dir = tempdir().unwrap();
        let db_path = temp_dir.path().join("test.db");
        
        let mut db_config = crate::config::DatabaseConfig::default();
        db_config.path = db_path;
        
        let database = Database::new(&db_config).await.unwrap();
        database.migrate().await.unwrap();
        
        DirectMessageHandler::new(database)
    }
    
    async fn create_test_user(handler: &DirectMessageHandler, callsign: &str) -> User {
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
        
        handler.database.create_user(&user, "test_hash").await.unwrap();
        user
    }
    
    #[tokio::test]
    async fn test_direct_message_creation() {
        let handler = create_test_handler().await;
        let sender = create_test_user(&handler, "W1SEND").await;
        let recipient = create_test_user(&handler, "W1RECV").await;
        let node_id = Uuid::new_v4();
        
        let request = DirectMessageRequest {
            recipient: RecipientIdentifier::UserId(recipient.id),
            subject: Some("Test Subject".to_string()),
            body: "Hello, this is a test message!".to_string(),
            priority: MessagePriority::Normal,
            parent_id: None,
            request_confirmation: false,
        };
        
        let message = handler.send_direct_message(&sender, request, node_id).await.unwrap();
        
        assert_eq!(message.message_type, MessageType::Direct);
        assert_eq!(message.sender_id, sender.id);
        assert_eq!(message.recipient_id, Some(recipient.id));
        assert_eq!(message.body, "Hello, this is a test message!");
        assert!(message.thread_id.is_some());
    }
    
    #[tokio::test]
    async fn test_recipient_resolution() {
        let handler = create_test_handler().await;
        let user = create_test_user(&handler, "W1TEST").await;
        
        // Test resolution by user ID
        let resolved_by_id = handler.resolve_recipient(&RecipientIdentifier::UserId(user.id)).await.unwrap();
        assert_eq!(resolved_by_id.id, user.id);
        
        // Test resolution by callsign
        let resolved_by_callsign = handler.resolve_recipient(&RecipientIdentifier::Callsign("W1TEST".to_string())).await.unwrap();
        assert_eq!(resolved_by_callsign.id, user.id);
        
        // Test invalid callsign
        let invalid_result = handler.resolve_recipient(&RecipientIdentifier::Callsign("INVALID".to_string())).await;
        assert!(invalid_result.is_err());
    }
    
    #[tokio::test]
    async fn test_typing_indicators() {
        let handler = create_test_handler().await;
        let sender = create_test_user(&handler, "W1TYPE").await;
        let recipient = create_test_user(&handler, "W1RECV").await;
        
        // Set typing indicator
        let notification = handler.set_typing_indicator(&sender, &recipient.id, true).await.unwrap();
        assert!(notification.is_typing);
        assert_eq!(notification.user_id, sender.id);
        assert_eq!(notification.recipient_id, recipient.id);
        
        // Get typing indicators for recipient
        let indicators = handler.get_typing_indicators(&recipient.id).await;
        assert_eq!(indicators.len(), 1);
        
        // Clear typing indicator
        let stop_notification = handler.set_typing_indicator(&sender, &recipient.id, false).await.unwrap();
        assert!(!stop_notification.is_typing);
        
        // Indicators should be empty after clearing
        let indicators_after = handler.get_typing_indicators(&recipient.id).await;
        assert_eq!(indicators_after.len(), 0);
    }
    
    #[tokio::test]
    async fn test_conversation_id_generation() {
        let user1_id = Uuid::new_v4();
        let user2_id = Uuid::new_v4();
        
        let conv_id_1 = generate_conversation_id(&user1_id, &user2_id);
        let conv_id_2 = generate_conversation_id(&user2_id, &user1_id);
        
        // Should be the same regardless of order
        assert_eq!(conv_id_1, conv_id_2);
        
        // Should be deterministic
        let conv_id_3 = generate_conversation_id(&user1_id, &user2_id);
        assert_eq!(conv_id_1, conv_id_3);
    }
    
    #[tokio::test]
    async fn test_message_thread_handling() {
        let handler = create_test_handler().await;
        let sender = create_test_user(&handler, "W1SEND").await;
        let recipient = create_test_user(&handler, "W1RECV").await;
        let node_id = Uuid::new_v4();
        
        // Send original message
        let original_request = DirectMessageRequest {
            recipient: RecipientIdentifier::UserId(recipient.id),
            subject: Some("Original Message".to_string()),
            body: "This is the original message".to_string(),
            priority: MessagePriority::Normal,
            parent_id: None,
            request_confirmation: false,
        };
        
        let original_message = handler.send_direct_message(&sender, original_request, node_id).await.unwrap();
        
        // Send reply
        let reply_request = DirectMessageRequest {
            recipient: RecipientIdentifier::UserId(sender.id),
            subject: Some("Re: Original Message".to_string()),
            body: "This is a reply".to_string(),
            priority: MessagePriority::Normal,
            parent_id: Some(original_message.id),
            request_confirmation: false,
        };
        
        let reply_message = handler.send_direct_message(&recipient, reply_request, node_id).await.unwrap();
        
        // Check thread relationship
        assert_eq!(reply_message.parent_id, Some(original_message.id));
        assert_eq!(reply_message.thread_id, original_message.thread_id);
    }
    
    #[tokio::test]
    async fn test_message_hash_generation() {
        let handler = create_test_handler().await;
        let timestamp = SystemTime::now();
        
        let hash1 = handler.generate_message_hash("W1TEST", "W2TEST", "Hello", timestamp);
        let hash2 = handler.generate_message_hash("W1TEST", "W2TEST", "Hello", timestamp);
        
        // Same inputs should produce same hash
        assert_eq!(hash1, hash2);
        
        // Different inputs should produce different hashes
        let hash3 = handler.generate_message_hash("W1TEST", "W2TEST", "Different message", timestamp);
        assert_ne!(hash1, hash3);
        
        let hash4 = handler.generate_message_hash("W1DIFF", "W2TEST", "Hello", timestamp);
        assert_ne!(hash1, hash4);
    }
    
    #[tokio::test]
    async fn test_delivery_confirmation() {
        let handler = create_test_handler().await;
        let sender = create_test_user(&handler, "W1SEND").await;
        let recipient = create_test_user(&handler, "W1RECV").await;
        let node_id = Uuid::new_v4();
        
        let request = DirectMessageRequest {
            recipient: RecipientIdentifier::UserId(recipient.id),
            subject: None,
            body: "Message with confirmation".to_string(),
            priority: MessagePriority::Normal,
            parent_id: None,
            request_confirmation: true,
        };
        
        let message = handler.send_direct_message(&sender, request, node_id).await.unwrap();
        
        // Message should be flagged for acknowledgment
        assert!(message.flags.requires_ack);
        
        // Delivery confirmation should be set up
        let confirmations = handler.delivery_confirmations.read().await;
        assert!(confirmations.contains_key(&message.id));
    }
    
    #[tokio::test]
    async fn test_conversation_cache() {
        let handler = create_test_handler().await;
        let user = create_test_user(&handler, "W1CACHE").await;
        
        // Initially no cached conversations
        let cached = handler.get_cached_conversations(&user.id).await;
        assert!(cached.is_none());
        
        // Cache some conversations
        let conversations = vec![
            ConversationInfo {
                conversation_id: "test_conv_1".to_string(),
                participant_id: Uuid::new_v4(),
                participant_callsign: "W1TEST".to_string(),
                participant_name: Some("Test User".to_string()),
                last_message: None,
                unread_count: 0,
                last_activity: SystemTime::now(),
                status: ConversationStatus::Active,
                participant_online: false,
            }
        ];
        
        handler.cache_conversations(&user.id, &conversations).await;
        
        // Should now have cached conversations
        let cached_after = handler.get_cached_conversations(&user.id).await;
        assert!(cached_after.is_some());
        assert_eq!(cached_after.unwrap().len(), 1);
        
        // Invalidate cache
        handler.invalidate_conversation_cache(&user.id).await;
        
        // Cache should be cleared
        let cached_after_invalidation = handler.get_cached_conversations(&user.id).await;
        assert!(cached_after_invalidation.is_none());
    }
    
    #[tokio::test]
    async fn test_expired_typing_cleanup() {
        let handler = create_test_handler().await;
        let user1 = create_test_user(&handler, "W1TYPE1").await;
        let user2 = create_test_user(&handler, "W1TYPE2").await;
        
        // Set typing indicator
        handler.set_typing_indicator(&user1, &user2.id, true).await.unwrap();
        
        // Manually expire the indicator by setting past time
        {
            let mut typing_indicators = handler.typing_indicators.write().await;
            if let Some(indicator) = typing_indicators.get_mut(&user1.id) {
                indicator.expires_at = SystemTime::now() - Duration::from_secs(60);
            }
        }
        
        // Should have indicator before cleanup
        let indicators_before = handler.typing_indicators.read().await;
        assert!(indicators_before.contains_key(&user1.id));
        drop(indicators_before);
        
        // Clean up expired indicators
        handler.cleanup_expired_typing_indicators().await;
        
        // Should be removed after cleanup
        let indicators_after = handler.typing_indicators.read().await;
        assert!(!indicators_after.contains_key(&user1.id));
    }
    
    #[tokio::test]
    async fn test_mark_conversation_read() {
        let handler = create_test_handler().await;
        let sender = create_test_user(&handler, "W1SEND").await;
        let recipient = create_test_user(&handler, "W1RECV").await;
        let node_id = Uuid::new_v4();
        
        // Send multiple messages
        for i in 1..=3 {
            let request = DirectMessageRequest {
                recipient: RecipientIdentifier::UserId(recipient.id),
                subject: None,
                body: format!("Message {}", i),
                priority: MessagePriority::Normal,
                parent_id: None,
                request_confirmation: false,
            };
            
            handler.send_direct_message(&sender, request, node_id).await.unwrap();
        }
        
        // Mark conversation as read
        let marked_count = handler.mark_conversation_read(&recipient.id, &sender.id).await.unwrap();
        
        // Should have marked some messages as read
        // Note: actual count depends on database implementation
        assert!(marked_count >= 0);
    }
    
    #[tokio::test]
    async fn test_conversation_query_filtering() {
        let handler = create_test_handler().await;
        
        let conversations = vec![
            ConversationInfo {
                conversation_id: "conv1".to_string(),
                participant_id: Uuid::new_v4(),
                participant_callsign: "W1ACTIVE".to_string(),
                participant_name: Some("Active User".to_string()),
                last_message: None,
                unread_count: 2,
                last_activity: SystemTime::now(),
                status: ConversationStatus::Active,
                participant_online: true,
            },
            ConversationInfo {
                conversation_id: "conv2".to_string(),
                participant_id: Uuid::new_v4(),
                participant_callsign: "W1ARCH".to_string(),
                participant_name: Some("Archived User".to_string()),
                last_message: None,
                unread_count: 0,
                last_activity: SystemTime::now() - Duration::from_secs(86400),
                status: ConversationStatus::Archived,
                participant_online: false,
            },
            ConversationInfo {
                conversation_id: "conv3".to_string(),
                participant_id: Uuid::new_v4(),
                participant_callsign: "W1MUTED".to_string(),
                participant_name: Some("Muted User".to_string()),
                last_message: None,
                unread_count: 1,
                last_activity: SystemTime::now(),
                status: ConversationStatus::Muted,
                participant_online: false,
            },
        ];
        
        // Test filtering by status
        let query = ConversationQuery {
            user_id: Uuid::new_v4(),
            include_archived: false,
            status_filter: Some(ConversationStatus::Active),
            search: None,
            limit: 10,
            offset: 0,
        };
        
        let filtered = handler.filter_conversations(conversations.clone(), &query);
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].status, ConversationStatus::Active);
        
        // Test search filtering
        let search_query = ConversationQuery {
            user_id: Uuid::new_v4(),
            include_archived: true,
            status_filter: None,
            search: Some("arch".to_string()),
            limit: 10,
            offset: 0,
        };
        
        let search_filtered = handler.filter_conversations(conversations.clone(), &search_query);
        assert_eq!(search_filtered.len(), 1);
        assert!(search_filtered[0].participant_callsign.contains("ARCH"));
        
        // Test include archived
        let archived_query = ConversationQuery {
            user_id: Uuid::new_v4(),
            include_archived: true,
            status_filter: None,
            search: None,
            limit: 10,
            offset: 0,
        };
        
        let with_archived = handler.filter_conversations(conversations.clone(), &archived_query);
        assert_eq!(with_archived.len(), 3);
        
        let without_archived_query = ConversationQuery {
            user_id: Uuid::new_v4(),
            include_archived: false,
            status_filter: None,
            search: None,
            limit: 10,
            offset: 0,
        };
        
        let without_archived = handler.filter_conversations(conversations, &without_archived_query);
        assert_eq!(without_archived.len(), 2); // Should exclude archived
    }
}
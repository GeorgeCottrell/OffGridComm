use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, SystemTime};
use tokio::sync::{broadcast, RwLock};
use uuid::Uuid;

use crate::auth::User;
use crate::database::{Database, models::*};

/// Notification manager for real-time user notifications
#[derive(Clone)]
pub struct NotificationManager {
    /// Database connection
    database: Database,
    
    /// Real-time notification broadcaster
    broadcaster: broadcast::Sender<Notification>,
    
    /// User notification preferences
    preferences: Arc<RwLock<HashMap<Uuid, NotificationPreferences>>>,
    
    /// Active notification subscriptions
    subscriptions: Arc<RwLock<HashMap<Uuid, Vec<NotificationSubscription>>>>,
    
    /// Notification history for undelivered notifications
    history: Arc<RwLock<HashMap<Uuid, Vec<PendingNotification>>>>,
}

/// Notification types
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum NotificationType {
    /// New direct message received
    NewDirectMessage,
    
    /// New group message in subscribed group
    NewGroupMessage,
    
    /// Emergency message broadcast
    EmergencyMessage,
    
    /// System announcement
    SystemAnnouncement,
    
    /// User mentioned in message
    UserMentioned,
    
    /// Message delivery confirmation
    DeliveryConfirmation,
    
    /// User typing indicator
    UserTyping,
    
    /// User online/offline status
    UserStatusChange,
    
    /// Group invitation received
    GroupInvitation,
    
    /// Message failed to deliver
    DeliveryFailed,
}

/// Notification data structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Notification {
    /// Unique notification ID
    pub id: Uuid,
    
    /// Type of notification
    pub notification_type: NotificationType,
    
    /// Target user ID
    pub user_id: Uuid,
    
    /// Notification title
    pub title: String,
    
    /// Notification message
    pub message: String,
    
    /// Related message ID (if applicable)
    pub message_id: Option<Uuid>,
    
    /// Related group ID (if applicable)
    pub group_id: Option<Uuid>,
    
    /// Sender information
    pub sender: Option<NotificationSender>,
    
    /// Notification priority
    pub priority: NotificationPriority,
    
    /// Creation timestamp
    pub created_at: SystemTime,
    
    /// Expiration time (if applicable)
    pub expires_at: Option<SystemTime>,
    
    /// Additional data
    pub data: serde_json::Value,
    
    /// Delivery channels
    pub channels: Vec<NotificationChannel>,
}

/// Notification sender information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotificationSender {
    /// Sender user ID
    pub user_id: Uuid,
    
    /// Sender callsign
    pub callsign: String,
    
    /// Sender display name
    pub display_name: Option<String>,
}

/// Notification priority levels
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub enum NotificationPriority {
    /// Low priority notification
    Low = 1,
    
    /// Normal priority notification
    Normal = 2,
    
    /// High priority notification
    High = 3,
    
    /// Emergency priority notification
    Emergency = 4,
}

/// Notification delivery channels
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum NotificationChannel {
    /// Real-time in-app notification
    InApp,
    
    /// Email notification
    Email,
    
    /// System notification (desktop)
    System,
    
    /// RF transmission notification
    RF,
}

/// User notification preferences
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotificationPreferences {
    /// User ID
    pub user_id: Uuid,
    
    /// Enabled notification types
    pub enabled_types: HashMap<NotificationType, bool>,
    
    /// Enabled delivery channels
    pub enabled_channels: HashMap<NotificationChannel, bool>,
    
    /// Quiet hours (no notifications)
    pub quiet_hours: Option<QuietHours>,
    
    /// Maximum notification frequency
    pub max_frequency: NotificationFrequency,
    
    /// Priority threshold
    pub priority_threshold: NotificationPriority,
    
    /// Group-specific preferences
    pub group_preferences: HashMap<Uuid, GroupNotificationPreferences>,
}

/// Quiet hours configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuietHours {
    /// Start time (hour 0-23)
    pub start_hour: u8,
    
    /// End time (hour 0-23)
    pub end_hour: u8,
    
    /// Days of week (0=Sunday, 6=Saturday)
    pub days: Vec<u8>,
    
    /// Emergency notifications override quiet hours
    pub emergency_override: bool,
}

/// Notification frequency limits
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum NotificationFrequency {
    /// No limit
    Unlimited,
    
    /// Maximum per minute
    PerMinute(u32),
    
    /// Maximum per hour
    PerHour(u32),
    
    /// Digest mode (batch notifications)
    Digest(Duration),
}

/// Group-specific notification preferences
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroupNotificationPreferences {
    /// Enable notifications for this group
    pub enabled: bool,
    
    /// Only notify on mentions
    pub mentions_only: bool,
    
    /// Notification channels for this group
    pub channels: Vec<NotificationChannel>,
}

/// Notification subscription
#[derive(Debug, Clone)]
pub struct NotificationSubscription {
    /// Subscription ID
    pub id: Uuid,
    
    /// User ID
    pub user_id: Uuid,
    
    /// Subscription type
    pub subscription_type: SubscriptionType,
    
    /// Real-time channel sender
    pub sender: broadcast::Sender<Notification>,
    
    /// Created timestamp
    pub created_at: SystemTime,
    
    /// Last activity
    pub last_activity: SystemTime,
}

/// Subscription types
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SubscriptionType {
    /// Real-time WebSocket connection
    WebSocket,
    
    /// Server-sent events
    ServerSentEvents,
    
    /// Polling subscription
    Polling,
}

/// Pending notification for offline users
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingNotification {
    /// Notification data
    pub notification: Notification,
    
    /// Delivery attempts
    pub attempts: u32,
    
    /// Next retry time
    pub next_retry: SystemTime,
    
    /// Maximum retries
    pub max_retries: u32,
}

impl NotificationManager {
    /// Create a new notification manager
    pub fn new(database: Database) -> Self {
        let (broadcaster, _) = broadcast::channel(1000);
        
        Self {
            database,
            broadcaster,
            preferences: Arc::new(RwLock::new(HashMap::new())),
            subscriptions: Arc::new(RwLock::new(HashMap::new())),
            history: Arc::new(RwLock::new(HashMap::new())),
        }
    }
    
    /// Send a notification to a user
    pub async fn send_notification(&self, notification: Notification) -> Result<()> {
        // Check user preferences
        let preferences = self.get_user_preferences(&notification.user_id).await?;
        
        // Check if notification type is enabled
        if !preferences.enabled_types.get(&notification.notification_type).unwrap_or(&true) {
            return Ok(()); // Notification disabled
        }
        
        // Check priority threshold
        if notification.priority < preferences.priority_threshold {
            return Ok(()); // Below threshold
        }
        
        // Check quiet hours
        if self.is_quiet_hours(&preferences).await && !self.is_emergency_override(&notification, &preferences) {
            // Queue for later delivery
            self.queue_notification(notification).await?;
            return Ok(());
        }
        
        // Check frequency limits
        if !self.check_frequency_limit(&notification.user_id, &preferences).await? {
            // Queue or drop based on priority
            if notification.priority >= NotificationPriority::High {
                self.queue_notification(notification).await?;
            }
            return Ok(());
        }
        
        // Deliver notification
        self.deliver_notification(notification).await?;
        
        Ok(())
    }
    
    /// Deliver notification through enabled channels
    async fn deliver_notification(&self, notification: Notification) -> Result<()> {
        let preferences = self.get_user_preferences(&notification.user_id).await?;
        
        // Determine delivery channels
        let channels = if notification.channels.is_empty() {
            // Use default channels from preferences
            preferences.enabled_channels.iter()
                .filter_map(|(channel, enabled)| if *enabled { Some(channel.clone()) } else { None })
                .collect()
        } else {
            // Use specified channels filtered by preferences
            notification.channels.iter()
                .filter(|channel| preferences.enabled_channels.get(channel).unwrap_or(&true))
                .cloned()
                .collect()
        };
        
        // Deliver via real-time channel
        if channels.contains(&NotificationChannel::InApp) {
            if let Err(e) = self.broadcaster.send(notification.clone()) {
                tracing::warn!("Failed to broadcast notification: {}", e);
            }
        }
        
        // Deliver via email (if configured)
        if channels.contains(&NotificationChannel::Email) {
            self.send_email_notification(&notification).await?;
        }
        
        // Deliver via system notification
        if channels.contains(&NotificationChannel::System) {
            self.send_system_notification(&notification).await?;
        }
        
        // Deliver via RF (for emergency notifications)
        if channels.contains(&NotificationChannel::RF) && notification.priority >= NotificationPriority::Emergency {
            self.send_rf_notification(&notification).await?;
        }
        
        tracing::debug!("Delivered notification {} to user {}", notification.id, notification.user_id);
        
        Ok(())
    }
    
    /// Queue notification for later delivery
    async fn queue_notification(&self, notification: Notification) -> Result<()> {
        let pending = PendingNotification {
            notification: notification.clone(),
            attempts: 0,
            next_retry: SystemTime::now() + Duration::from_secs(300), // 5 minutes
            max_retries: 3,
        };
        
        let mut history = self.history.write().await;
        history.entry(notification.user_id)
            .or_insert_with(Vec::new)
            .push(pending);
        
        Ok(())
    }
    
    /// Create notification subscription for real-time delivery
    pub async fn subscribe(&self, user_id: &Uuid, subscription_type: SubscriptionType) -> broadcast::Receiver<Notification> {
        let (sender, receiver) = broadcast::channel(100);
        
        let subscription = NotificationSubscription {
            id: Uuid::new_v4(),
            user_id: *user_id,
            subscription_type,
            sender,
            created_at: SystemTime::now(),
            last_activity: SystemTime::now(),
        };
        
        let mut subscriptions = self.subscriptions.write().await;
        subscriptions.entry(*user_id)
            .or_insert_with(Vec::new)
            .push(subscription);
        
        receiver
    }
    
    /// Unsubscribe from notifications
    pub async fn unsubscribe(&self, user_id: &Uuid, subscription_id: &Uuid) {
        let mut subscriptions = self.subscriptions.write().await;
        if let Some(user_subscriptions) = subscriptions.get_mut(user_id) {
            user_subscriptions.retain(|s| s.id != *subscription_id);
        }
    }
    
    /// Get user notification preferences
    pub async fn get_user_preferences(&self, user_id: &Uuid) -> Result<NotificationPreferences> {
        // Check cache first
        {
            let preferences = self.preferences.read().await;
            if let Some(prefs) = preferences.get(user_id) {
                return Ok(prefs.clone());
            }
        }
        
        // Load from database or create defaults
        let prefs = self.load_user_preferences(user_id).await?
            .unwrap_or_else(|| self.default_preferences(*user_id));
        
        // Cache preferences
        {
            let mut preferences = self.preferences.write().await;
            preferences.insert(*user_id, prefs.clone());
        }
        
        Ok(prefs)
    }
    
    /// Update user notification preferences
    pub async fn update_user_preferences(
        &self,
        user_id: &Uuid,
        preferences: NotificationPreferences,
    ) -> Result<()> {
        // Store in database
        self.save_user_preferences(&preferences).await?;
        
        // Update cache
        {
            let mut cached_preferences = self.preferences.write().await;
            cached_preferences.insert(*user_id, preferences);
        }
        
        Ok(())
    }
    
    /// Check if current time is in quiet hours
    async fn is_quiet_hours(&self, preferences: &NotificationPreferences) -> bool {
        if let Some(quiet_hours) = &preferences.quiet_hours {
            let now = chrono::Utc::now();
            let current_hour = now.hour() as u8;
            let current_day = now.weekday().num_days_from_sunday() as u8;
            
            if quiet_hours.days.contains(&current_day) {
                if quiet_hours.start_hour <= quiet_hours.end_hour {
                    // Same day range
                    current_hour >= quiet_hours.start_hour && current_hour < quiet_hours.end_hour
                } else {
                    // Overnight range
                    current_hour >= quiet_hours.start_hour || current_hour < quiet_hours.end_hour
                }
            } else {
                false
            }
        } else {
            false
        }
    }
    
    /// Check if emergency override applies
    fn is_emergency_override(&self, notification: &Notification, preferences: &NotificationPreferences) -> bool {
        notification.priority >= NotificationPriority::Emergency &&
            preferences.quiet_hours.as_ref()
                .map(|qh| qh.emergency_override)
                .unwrap_or(true)
    }
    
    /// Check frequency limits
    async fn check_frequency_limit(&self, user_id: &Uuid, preferences: &NotificationPreferences) -> Result<bool> {
        match preferences.max_frequency {
            NotificationFrequency::Unlimited => Ok(true),
            NotificationFrequency::PerMinute(limit) => {
                self.check_rate_limit(user_id, Duration::from_secs(60), limit).await
            }
            NotificationFrequency::PerHour(limit) => {
                self.check_rate_limit(user_id, Duration::from_secs(3600), limit).await
            }
            NotificationFrequency::Digest(_) => {
                // Digest mode always allows individual notifications
                Ok(true)
            }
        }
    }
    
    /// Check rate limit for user
    async fn check_rate_limit(&self, _user_id: &Uuid, _duration: Duration, _limit: u32) -> Result<bool> {
        // This would implement rate limiting logic
        // For now, always allow
        Ok(true)
    }
    
    /// Send email notification
    async fn send_email_notification(&self, _notification: &Notification) -> Result<()> {
        // This would integrate with email service
        // For now, just log
        tracing::info!("Would send email notification: {}", _notification.title);
        Ok(())
    }
    
    /// Send system notification
    async fn send_system_notification(&self, _notification: &Notification) -> Result<()> {
        // This would send OS-level notifications
        tracing::info!("Would send system notification: {}", _notification.title);
        Ok(())
    }
    
    /// Send RF notification
    async fn send_rf_notification(&self, _notification: &Notification) -> Result<()> {
        // This would send emergency notifications via RF
        tracing::info!("Would send RF notification: {}", _notification.title);
        Ok(())
    }
    
    /// Load user preferences from database
    async fn load_user_preferences(&self, _user_id: &Uuid) -> Result<Option<NotificationPreferences>> {
        // This would load from database
        // For now, return None to use defaults
        Ok(None)
    }
    
    /// Save user preferences to database
    async fn save_user_preferences(&self, _preferences: &NotificationPreferences) -> Result<()> {
        // This would save to database
        Ok(())
    }
    
    /// Create default notification preferences
    fn default_preferences(&self, user_id: Uuid) -> NotificationPreferences {
        let mut enabled_types = HashMap::new();
        enabled_types.insert(NotificationType::NewDirectMessage, true);
        enabled_types.insert(NotificationType::EmergencyMessage, true);
        enabled_types.insert(NotificationType::SystemAnnouncement, true);
        enabled_types.insert(NotificationType::UserMentioned, true);
        enabled_types.insert(NotificationType::DeliveryConfirmation, false);
        enabled_types.insert(NotificationType::GroupInvitation, true);
        enabled_types.insert(NotificationType::NewGroupMessage, false);
        enabled_types.insert(NotificationType::UserTyping, false);
        enabled_types.insert(NotificationType::UserStatusChange, false);
        enabled_types.insert(NotificationType::DeliveryFailed, true);
        
        let mut enabled_channels = HashMap::new();
        enabled_channels.insert(NotificationChannel::InApp, true);
        enabled_channels.insert(NotificationChannel::Email, false);
        enabled_channels.insert(NotificationChannel::System, true);
        enabled_channels.insert(NotificationChannel::RF, false);
        
        NotificationPreferences {
            user_id,
            enabled_types,
            enabled_channels,
            quiet_hours: None,
            max_frequency: NotificationFrequency::PerMinute(10),
            priority_threshold: NotificationPriority::Low,
            group_preferences: HashMap::new(),
        }
    }
    
    /// Process pending notifications
    pub async fn process_pending_notifications(&self) -> Result<()> {
        let mut history = self.history.write().await;
        let now = SystemTime::now();
        
        for (user_id, pending_notifications) in history.iter_mut() {
            pending_notifications.retain_mut(|pending| {
                if pending.next_retry <= now && pending.attempts < pending.max_retries {
                    // Retry delivery
                    pending.attempts += 1;
                    pending.next_retry = now + Duration::from_secs(300 * pending.attempts as u64);
                    
                    // Try to deliver
                    let notification = pending.notification.clone();
                    let manager = self.clone();
                    tokio::spawn(async move {
                        if let Err(e) = manager.deliver_notification(notification).await {
                            tracing::warn!("Failed to deliver pending notification: {}", e);
                        }
                    });
                    
                    // Keep in queue for now
                    true
                } else if pending.attempts >= pending.max_retries {
                    tracing::warn!("Dropping notification {} after {} attempts", 
                                  pending.notification.id, pending.attempts);
                    false // Remove from queue
                } else {
                    true // Keep in queue
                }
            });
        }
        
        Ok(())
    }
    
    /// Clean up old subscriptions
    pub async fn cleanup_subscriptions(&self) {
        let mut subscriptions = self.subscriptions.write().await;
        let cutoff = SystemTime::now() - Duration::from_secs(3600); // 1 hour
        
        for user_subscriptions in subscriptions.values_mut() {
            user_subscriptions.retain(|s| s.last_activity > cutoff);
        }
        
        // Remove empty user entries
        subscriptions.retain(|_, subs| !subs.is_empty());
    }
}

/// Convenience functions for creating notifications
impl NotificationManager {
    /// Create new direct message notification
    pub async fn notify_new_direct_message(
        &self,
        recipient_id: Uuid,
        sender: &User,
        message: &Message,
    ) -> Result<()> {
        let notification = Notification {
            id: Uuid::new_v4(),
            notification_type: NotificationType::NewDirectMessage,
            user_id: recipient_id,
            title: format!("New message from {}", sender.callsign),
            message: if message.subject.is_some() {
                format!("Subject: {}", message.subject.as_ref().unwrap())
            } else {
                "New direct message".to_string()
            },
            message_id: Some(message.id),
            group_id: None,
            sender: Some(NotificationSender {
                user_id: sender.id,
                callsign: sender.callsign.clone(),
                display_name: sender.full_name.clone(),
            }),
            priority: if message.priority >= MessagePriority::High {
                NotificationPriority::High
            } else {
                NotificationPriority::Normal
            },
            created_at: SystemTime::now(),
            expires_at: Some(SystemTime::now() + Duration::from_secs(86400)), // 24 hours
            data: serde_json::json!({
                "typing": true,
                "group_id": group_id,
            }),
            channels: vec![NotificationChannel::InApp],
        };
        
        self.send_notification(notification).await
    }
    
    /// Create user status change notification
    pub async fn notify_user_status_change(
        &self,
        recipient_id: Uuid,
        user: &User,
        status: UserStatus,
    ) -> Result<()> {
        let status_text = match status {
            UserStatus::Online => "came online",
            UserStatus::Offline => "went offline",
            UserStatus::Away => "is away",
            UserStatus::Busy => "is busy",
        };
        
        let notification = Notification {
            id: Uuid::new_v4(),
            notification_type: NotificationType::UserStatusChange,
            user_id: recipient_id,
            title: format!("{} {}", user.callsign, status_text),
            message: "".to_string(),
            message_id: None,
            group_id: None,
            sender: Some(NotificationSender {
                user_id: user.id,
                callsign: user.callsign.clone(),
                display_name: user.full_name.clone(),
            }),
            priority: NotificationPriority::Low,
            created_at: SystemTime::now(),
            expires_at: Some(SystemTime::now() + Duration::from_secs(3600)), // 1 hour
            data: serde_json::json!({
                "status_change": true,
                "new_status": status,
            }),
            channels: vec![NotificationChannel::InApp],
        };
        
        self.send_notification(notification).await
    }
    
    /// Create group invitation notification
    pub async fn notify_group_invitation(
        &self,
        recipient_id: Uuid,
        inviter: &User,
        group: &Group,
        invitation_id: Uuid,
    ) -> Result<()> {
        let notification = Notification {
            id: Uuid::new_v4(),
            notification_type: NotificationType::GroupInvitation,
            user_id: recipient_id,
            title: format!("Group invitation from {}", inviter.callsign),
            message: format!("{} invited you to join group: {}", 
                           inviter.callsign, group.name),
            message_id: None,
            group_id: Some(group.id),
            sender: Some(NotificationSender {
                user_id: inviter.id,
                callsign: inviter.callsign.clone(),
                display_name: inviter.full_name.clone(),
            }),
            priority: NotificationPriority::Normal,
            created_at: SystemTime::now(),
            expires_at: Some(SystemTime::now() + Duration::from_secs(604800)), // 7 days
            data: serde_json::json!({
                "invitation_id": invitation_id,
                "group_id": group.id,
                "group_name": group.name,
                "inviter_id": inviter.id,
            }),
            channels: vec![NotificationChannel::InApp, NotificationChannel::Email],
        };
        
        self.send_notification(notification).await
    }
    
    /// Create delivery failed notification
    pub async fn notify_delivery_failed(
        &self,
        recipient_id: Uuid,
        message_id: Uuid,
        reason: &str,
    ) -> Result<()> {
        let notification = Notification {
            id: Uuid::new_v4(),
            notification_type: NotificationType::DeliveryFailed,
            user_id: recipient_id,
            title: "Message delivery failed".to_string(),
            message: format!("Failed to deliver message: {}", reason),
            message_id: Some(message_id),
            group_id: None,
            sender: None,
            priority: NotificationPriority::High,
            created_at: SystemTime::now(),
            expires_at: Some(SystemTime::now() + Duration::from_secs(86400)), // 24 hours
            data: serde_json::json!({
                "message_id": message_id,
                "failure_reason": reason,
                "delivery_failed": true,
            }),
            channels: vec![NotificationChannel::InApp, NotificationChannel::System],
        };
        
        self.send_notification(notification).await
    }
    
    /// Broadcast notification to all users
    pub async fn broadcast_notification(
        &self,
        notification_type: NotificationType,
        title: String,
        message: String,
        priority: NotificationPriority,
        sender: Option<NotificationSender>,
    ) -> Result<()> {
        // Get all active users from database
        let users = self.database.get_all_active_users().await?;
        
        for user in users {
            let notification = Notification {
                id: Uuid::new_v4(),
                notification_type: notification_type.clone(),
                user_id: user.id,
                title: title.clone(),
                message: message.clone(),
                message_id: None,
                group_id: None,
                sender: sender.clone(),
                priority,
                created_at: SystemTime::now(),
                expires_at: Some(SystemTime::now() + Duration::from_secs(604800)), // 7 days
                data: serde_json::json!({
                    "broadcast": true,
                }),
                channels: vec![
                    NotificationChannel::InApp,
                    NotificationChannel::System,
                    NotificationChannel::Email,
                ],
            };
            
            // Send asynchronously
            let manager = self.clone();
            tokio::spawn(async move {
                if let Err(e) = manager.send_notification(notification).await {
                    tracing::warn!("Failed to send broadcast notification: {}", e);
                }
            });
        }
        
        Ok(())
    }
    
    /// Get notification history for a user
    pub async fn get_notification_history(
        &self,
        user_id: &Uuid,
        limit: Option<usize>,
        before: Option<SystemTime>,
    ) -> Result<Vec<Notification>> {
        // This would load from database
        // For now, return empty vector
        Ok(vec![])
    }
    
    /// Mark notifications as read
    pub async fn mark_notifications_read(
        &self,
        user_id: &Uuid,
        notification_ids: &[Uuid],
    ) -> Result<()> {
        // This would update database
        tracing::debug!("Marking {} notifications as read for user {}", 
                       notification_ids.len(), user_id);
        Ok(())
    }
    
    /// Delete notifications
    pub async fn delete_notifications(
        &self,
        user_id: &Uuid,
        notification_ids: &[Uuid],
    ) -> Result<()> {
        // This would delete from database
        tracing::debug!("Deleting {} notifications for user {}", 
                       notification_ids.len(), user_id);
        Ok(())
    }
    
    /// Get notification statistics
    pub async fn get_notification_stats(&self, user_id: &Uuid) -> Result<NotificationStats> {
        // This would query database for stats
        Ok(NotificationStats {
            total_notifications: 0,
            unread_notifications: 0,
            notifications_by_type: HashMap::new(),
            last_notification: None,
        })
    }
}

/// Notification statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotificationStats {
    /// Total notifications received
    pub total_notifications: u64,
    
    /// Unread notifications count
    pub unread_notifications: u64,
    
    /// Notifications by type
    pub notifications_by_type: HashMap<NotificationType, u64>,
    
    /// Last notification timestamp
    pub last_notification: Option<SystemTime>,
}

/// Background task for processing notifications
pub async fn notification_processor(manager: NotificationManager) {
    let mut interval = tokio::time::interval(Duration::from_secs(60)); // Run every minute
    
    loop {
        interval.tick().await;
        
        // Process pending notifications
        if let Err(e) = manager.process_pending_notifications().await {
            tracing::error!("Failed to process pending notifications: {}", e);
        }
        
        // Clean up old subscriptions
        manager.cleanup_subscriptions().await;
        
        // Clean up expired notifications
        // This would remove expired notifications from database
        tracing::debug!("Notification processor completed cycle");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::Database;
    
    #[tokio::test]
    async fn test_notification_manager_creation() {
        let database = Database::new_test().await;
        let manager = NotificationManager::new(database);
        
        assert!(manager.preferences.read().await.is_empty());
        assert!(manager.subscriptions.read().await.is_empty());
        assert!(manager.history.read().await.is_empty());
    }
    
    #[tokio::test]
    async fn test_default_preferences() {
        let database = Database::new_test().await;
        let manager = NotificationManager::new(database);
        let user_id = Uuid::new_v4();
        
        let prefs = manager.default_preferences(user_id);
        
        assert_eq!(prefs.user_id, user_id);
        assert!(prefs.enabled_types[&NotificationType::NewDirectMessage]);
        assert!(prefs.enabled_types[&NotificationType::EmergencyMessage]);
        assert!(!prefs.enabled_types[&NotificationType::UserTyping]);
        assert_eq!(prefs.priority_threshold, NotificationPriority::Low);
    }
    
    #[tokio::test]
    async fn test_notification_subscription() {
        let database = Database::new_test().await;
        let manager = NotificationManager::new(database);
        let user_id = Uuid::new_v4();
        
        let _receiver = manager.subscribe(&user_id, SubscriptionType::WebSocket).await;
        
        let subscriptions = manager.subscriptions.read().await;
        assert!(subscriptions.contains_key(&user_id));
        assert_eq!(subscriptions[&user_id].len(), 1);
    }
    
    #[tokio::test]
    async fn test_quiet_hours_check() {
        let database = Database::new_test().await;
        let manager = NotificationManager::new(database);
        let user_id = Uuid::new_v4();
        
        let mut prefs = manager.default_preferences(user_id);
        prefs.quiet_hours = Some(QuietHours {
            start_hour: 22, // 10 PM
            end_hour: 6,    // 6 AM
            days: vec![0, 1, 2, 3, 4, 5, 6], // All days
            emergency_override: true,
        });
        
        // This test would need to mock the current time
        // For now, just verify the structure
        assert!(prefs.quiet_hours.is_some());
    }
}erde_json::json!({
                "message_id": message.id,
                "message_type": message.message_type,
            }),
            channels: vec![NotificationChannel::InApp, NotificationChannel::System],
        };
        
        self.send_notification(notification).await
    }
    
    /// Create emergency message notification
    pub async fn notify_emergency_message(
        &self,
        recipient_id: Uuid,
        message: &Message,
    ) -> Result<()> {
        let notification = Notification {
            id: Uuid::new_v4(),
            notification_type: NotificationType::EmergencyMessage,
            user_id: recipient_id,
            title: "EMERGENCY MESSAGE".to_string(),
            message: format!("Emergency from {}: {}", 
                           message.sender_callsign,
                           message.body.chars().take(100).collect::<String>()),
            message_id: Some(message.id),
            group_id: None,
            sender: Some(NotificationSender {
                user_id: message.sender_id,
                callsign: message.sender_callsign.clone(),
                display_name: None,
            }),
            priority: NotificationPriority::Emergency,
            created_at: SystemTime::now(),
            expires_at: Some(SystemTime::now() + Duration::from_secs(3600)), // 1 hour
            data: serde_json::json!({
                "message_id": message.id,
                "emergency": true,
            }),
            channels: vec![
                NotificationChannel::InApp, 
                NotificationChannel::System,
                NotificationChannel::Email,
                NotificationChannel::RF,
            ],
        };
        
        self.send_notification(notification).await
    }
    
    /// Create group message notification
    pub async fn notify_group_message(
        &self,
        recipient_id: Uuid,
        sender: &User,
        message: &Message,
        group: &Group,
    ) -> Result<()> {
        let preferences = self.get_user_preferences(&recipient_id).await?;
        
        // Check group-specific preferences
        if let Some(group_prefs) = preferences.group_preferences.get(&group.id) {
            if !group_prefs.enabled {
                return Ok(()); // Group notifications disabled
            }
            
            // Check if user is mentioned or if mentions_only is enabled
            if group_prefs.mentions_only && !message.body.contains(&format!("@{}", preferences.user_id)) {
                return Ok(()); // Only notify on mentions
            }
        }
        
        let notification = Notification {
            id: Uuid::new_v4(),
            notification_type: NotificationType::NewGroupMessage,
            user_id: recipient_id,
            title: format!("New message in {}", group.name),
            message: format!("{}: {}", 
                           sender.callsign,
                           message.body.chars().take(100).collect::<String>()),
            message_id: Some(message.id),
            group_id: Some(group.id),
            sender: Some(NotificationSender {
                user_id: sender.id,
                callsign: sender.callsign.clone(),
                display_name: sender.full_name.clone(),
            }),
            priority: if message.priority >= MessagePriority::High {
                NotificationPriority::High
            } else {
                NotificationPriority::Normal
            },
            created_at: SystemTime::now(),
            expires_at: Some(SystemTime::now() + Duration::from_secs(86400)), // 24 hours
            data: serde_json::json!({
                "message_id": message.id,
                "group_id": group.id,
                "group_name": group.name,
            }),
            channels: vec![NotificationChannel::InApp],
        };
        
        self.send_notification(notification).await
    }
    
    /// Create user mentioned notification
    pub async fn notify_user_mentioned(
        &self,
        recipient_id: Uuid,
        sender: &User,
        message: &Message,
        group: Option<&Group>,
    ) -> Result<()> {
        let context = if let Some(group) = group {
            format!("in {}", group.name)
        } else {
            "in direct message".to_string()
        };
        
        let notification = Notification {
            id: Uuid::new_v4(),
            notification_type: NotificationType::UserMentioned,
            user_id: recipient_id,
            title: format!("You were mentioned by {}", sender.callsign),
            message: format!("Mentioned {} {}: {}", 
                           context,
                           sender.callsign,
                           message.body.chars().take(100).collect::<String>()),
            message_id: Some(message.id),
            group_id: group.map(|g| g.id),
            sender: Some(NotificationSender {
                user_id: sender.id,
                callsign: sender.callsign.clone(),
                display_name: sender.full_name.clone(),
            }),
            priority: NotificationPriority::High,
            created_at: SystemTime::now(),
            expires_at: Some(SystemTime::now() + Duration::from_secs(86400)), // 24 hours
            data: serde_json::json!({
                "message_id": message.id,
                "group_id": group.map(|g| g.id),
                "mention": true,
            }),
            channels: vec![NotificationChannel::InApp, NotificationChannel::System],
        };
        
        self.send_notification(notification).await
    }
    
    /// Create system announcement notification
    pub async fn notify_system_announcement(
        &self,
        recipient_id: Uuid,
        title: String,
        message: String,
        priority: NotificationPriority,
    ) -> Result<()> {
        let notification = Notification {
            id: Uuid::new_v4(),
            notification_type: NotificationType::SystemAnnouncement,
            user_id: recipient_id,
            title,
            message,
            message_id: None,
            group_id: None,
            sender: None,
            priority,
            created_at: SystemTime::now(),
            expires_at: Some(SystemTime::now() + Duration::from_secs(604800)), // 7 days
            data: serde_json::json!({
                "system_announcement": true,
            }),
            channels: vec![
                NotificationChannel::InApp,
                NotificationChannel::System,
                NotificationChannel::Email,
            ],
        };
        
        self.send_notification(notification).await
    }
    
    /// Create delivery confirmation notification
    pub async fn notify_delivery_confirmation(
        &self,
        recipient_id: Uuid,
        message_id: Uuid,
        delivered_to: &str,
        delivered_at: SystemTime,
    ) -> Result<()> {
        let notification = Notification {
            id: Uuid::new_v4(),
            notification_type: NotificationType::DeliveryConfirmation,
            user_id: recipient_id,
            title: "Message delivered".to_string(),
            message: format!("Message delivered to {} at {}", 
                           delivered_to,
                           chrono::DateTime::<chrono::Utc>::from(delivered_at)
                               .format("%Y-%m-%d %H:%M:%S UTC")),
            message_id: Some(message_id),
            group_id: None,
            sender: None,
            priority: NotificationPriority::Low,
            created_at: SystemTime::now(),
            expires_at: Some(SystemTime::now() + Duration::from_secs(86400)), // 24 hours
            data: serde_json::json!({
                "message_id": message_id,
                "delivered_to": delivered_to,
                "delivered_at": delivered_at.duration_since(std::time::UNIX_EPOCH).unwrap().as_secs(),
            }),
            channels: vec![NotificationChannel::InApp],
        };
        
        self.send_notification(notification).await
    }
    
    /// Create user typing notification
    pub async fn notify_user_typing(
        &self,
        recipient_id: Uuid,
        sender: &User,
        group_id: Option<Uuid>,
    ) -> Result<()> {
        let notification = Notification {
            id: Uuid::new_v4(),
            notification_type: NotificationType::UserTyping,
            user_id: recipient_id,
            title: format!("{} is typing...", sender.callsign),
            message: "".to_string(),
            message_id: None,
            group_id,
            sender: Some(NotificationSender {
                user_id: sender.id,
                callsign: sender.callsign.clone(),
                display_name: sender.full_name.clone(),
            }),
            priority: NotificationPriority::Low,
            created_at: SystemTime::now(),
            expires_at: Some(SystemTime::now() + Duration::from_secs(30)), // 30 seconds
            data: s

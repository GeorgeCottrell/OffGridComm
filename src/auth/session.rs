use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::net::IpAddr;
use std::str::FromStr;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::sync::RwLock;
use uuid::Uuid;

/// Session token type - uses UUID for security and uniqueness
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SessionToken(Uuid);

/// User session information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    /// Unique session token
    pub token: SessionToken,
    
    /// User ID this session belongs to
    pub user_id: Uuid,
    
    /// User's callsign
    pub callsign: String,
    
    /// Session creation time
    pub created_at: SystemTime,
    
    /// Session expiration time
    pub expires_at: SystemTime,
    
    /// Last activity time
    pub last_activity: SystemTime,
    
    /// IP address of the client
    pub client_ip: Option<IpAddr>,
    
    /// User agent string
    pub user_agent: Option<String>,
    
    /// Session type (web, terminal, api)
    pub session_type: SessionType,
    
    /// Additional session metadata
    pub metadata: SessionMetadata,
    
    /// Whether this is a "remember me" session (longer duration)
    pub persistent: bool,
    
    /// Session flags
    pub flags: SessionFlags,
}

/// Types of sessions
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum SessionType {
    /// Web browser session
    Web,
    
    /// Terminal/Telnet session
    Terminal,
    
    /// API access session
    Api,
    
    /// Mobile app session
    Mobile,
    
    /// System/internal session
    System,
}

/// Session metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionMetadata {
    /// Browser/client information
    pub client_info: Option<String>,
    
    /// Geographic location (if available)
    pub location: Option<String>,
    
    /// Device identifier
    pub device_id: Option<String>,
    
    /// Application version
    pub app_version: Option<String>,
    
    /// Custom key-value pairs
    pub custom: HashMap<String, String>,
}

/// Session flags for special behaviors
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionFlags {
    /// Session is read-only
    pub read_only: bool,
    
    /// Session requires additional verification
    pub requires_verification: bool,
    
    /// Session is from a trusted device
    pub trusted_device: bool,
    
    /// Session has elevated privileges
    pub elevated: bool,
    
    /// Session is being monitored for security
    pub monitored: bool,
}

/// Session manager for handling user sessions
#[derive(Clone)]
pub struct SessionManager {
    /// In-memory session storage
    sessions: RwLock<HashMap<SessionToken, Session>>,
    
    /// Default session duration
    default_duration: Duration,
    
    /// Maximum session duration
    max_duration: Duration,
    
    /// Cleanup interval
    cleanup_interval: Duration,
    
    /// Maximum sessions per user
    max_sessions_per_user: usize,
}

/// Session validation result
#[derive(Debug, Clone)]
pub struct SessionValidation {
    /// Whether the session is valid
    pub valid: bool,
    
    /// The session (if valid)
    pub session: Option<Session>,
    
    /// Validation error (if invalid)
    pub error: Option<SessionError>,
    
    /// Whether the session was refreshed
    pub refreshed: bool,
}

/// Session creation parameters
#[derive(Debug, Clone)]
pub struct SessionParams {
    /// User ID
    pub user_id: Uuid,
    
    /// User's callsign
    pub callsign: String,
    
    /// Session duration (None = use default)
    pub duration: Option<Duration>,
    
    /// Client IP address
    pub client_ip: Option<IpAddr>,
    
    /// User agent string
    pub user_agent: Option<String>,
    
    /// Session type
    pub session_type: SessionType,
    
    /// Whether this is a persistent session
    pub persistent: bool,
    
    /// Session metadata
    pub metadata: SessionMetadata,
    
    /// Session flags
    pub flags: SessionFlags,
}

/// Session statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionStats {
    /// Total active sessions
    pub total_sessions: usize,
    
    /// Sessions by type
    pub by_type: HashMap<SessionType, usize>,
    
    /// Sessions by user
    pub by_user: HashMap<Uuid, usize>,
    
    /// Average session duration
    pub avg_duration_minutes: f64,
    
    /// Sessions created in last hour
    pub recent_sessions: usize,
    
    /// Sessions expiring in next hour
    pub expiring_soon: usize,
}

impl SessionToken {
    /// Generate a new random session token
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
    
    /// Get the token as a string
    pub fn as_str(&self) -> &str {
        self.0.as_hyphenated().as_str()
    }
    
    /// Get the inner UUID
    pub fn uuid(&self) -> Uuid {
        self.0
    }
}

impl FromStr for SessionToken {
    type Err = uuid::Error;
    
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self(Uuid::parse_str(s)?))
    }
}

impl std::fmt::Display for SessionToken {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl SessionManager {
    /// Create a new session manager
    pub fn new(default_duration: Duration) -> Self {
        Self {
            sessions: RwLock::new(HashMap::new()),
            default_duration,
            max_duration: Duration::from_secs(30 * 24 * 3600), // 30 days max
            cleanup_interval: Duration::from_secs(300), // 5 minutes
            max_sessions_per_user: 10, // Reasonable limit
        }
    }
    
    /// Create a new session
    pub async fn create_session(
        &self,
        user_id: Uuid,
        callsign: String,
        duration: Duration,
    ) -> Result<Session> {
        let params = SessionParams {
            user_id,
            callsign,
            duration: Some(duration),
            client_ip: None,
            user_agent: None,
            session_type: SessionType::Web,
            persistent: false,
            metadata: SessionMetadata::default(),
            flags: SessionFlags::default(),
        };
        
        self.create_session_with_params(params).await
    }
    
    /// Create a session with detailed parameters
    pub async fn create_session_with_params(
        &self,
        params: SessionParams,
    ) -> Result<Session> {
        let token = SessionToken::new();
        let now = SystemTime::now();
        
        // Use provided duration or default
        let duration = params.duration.unwrap_or(self.default_duration);
        let duration = duration.min(self.max_duration); // Cap at maximum
        
        let session = Session {
            token: token.clone(),
            user_id: params.user_id,
            callsign: params.callsign,
            created_at: now,
            expires_at: now + duration,
            last_activity: now,
            client_ip: params.client_ip,
            user_agent: params.user_agent,
            session_type: params.session_type,
            metadata: params.metadata,
            persistent: params.persistent,
            flags: params.flags,
        };
        
        // Check session limits per user
        {
            let sessions = self.sessions.read().await;
            let user_sessions = sessions.values()
                .filter(|s| s.user_id == params.user_id)
                .count();
            
            if user_sessions >= self.max_sessions_per_user {
                return Err(anyhow::anyhow!("Maximum sessions per user exceeded"));
            }
        }
        
        // Store the session
        {
            let mut sessions = self.sessions.write().await;
            sessions.insert(token, session.clone());
        }
        
        Ok(session)
    }
    
    /// Validate and optionally refresh a session
    pub async fn validate_session(&self, token: &str) -> Result<Option<Session>> {
        let token = match SessionToken::from_str(token) {
            Ok(t) => t,
            Err(_) => return Ok(None),
        };
        
        let mut sessions = self.sessions.write().await;
        
        if let Some(session) = sessions.get_mut(&token) {
            let now = SystemTime::now();
            
            // Check if session has expired
            if session.expires_at <= now {
                sessions.remove(&token);
                return Ok(None);
            }
            
            // Update last activity
            session.last_activity = now;
            
            // For persistent sessions, extend expiration on activity
            if session.persistent {
                let remaining = session.expires_at.duration_since(now).unwrap_or_default();
                let extension = self.default_duration / 2; // Extend by half default duration
                
                if remaining < extension {
                    session.expires_at = now + self.default_duration;
                }
            }
            
            Ok(Some(session.clone()))
        } else {
            Ok(None)
        }
    }
    
    /// Invalidate a specific session
    pub async fn invalidate_session(&self, token: &str) -> Result<bool> {
        let token = match SessionToken::from_str(token) {
            Ok(t) => t,
            Err(_) => return Ok(false),
        };
        
        let mut sessions = self.sessions.write().await;
        Ok(sessions.remove(&token).is_some())
    }
    
    /// Invalidate all sessions for a user
    pub async fn invalidate_user_sessions(&self, user_id: &Uuid) -> Result<usize> {
        let mut sessions = self.sessions.write().await;
        let tokens_to_remove: Vec<SessionToken> = sessions
            .iter()
            .filter(|(_, session)| session.user_id == *user_id)
            .map(|(token, _)| token.clone())
            .collect();
        
        let count = tokens_to_remove.len();
        for token in tokens_to_remove {
            sessions.remove(&token);
        }
        
        Ok(count)
    }
    
    /// Invalidate all sessions except the current one
    pub async fn invalidate_other_sessions(
        &self,
        user_id: &Uuid,
        current_token: &str,
    ) -> Result<usize> {
        let current_token = SessionToken::from_str(current_token)?;
        let mut sessions = self.sessions.write().await;
        
        let tokens_to_remove: Vec<SessionToken> = sessions
            .iter()
            .filter(|(token, session)| {
                session.user_id == *user_id && **token != current_token
            })
            .map(|(token, _)| token.clone())
            .collect();
        
        let count = tokens_to_remove.len();
        for token in tokens_to_remove {
            sessions.remove(&token);
        }
        
        Ok(count)
    }
    
    /// Get all sessions for a user
    pub async fn get_user_sessions(&self, user_id: &Uuid) -> Result<Vec<Session>> {
        let sessions = self.sessions.read().await;
        let user_sessions: Vec<Session> = sessions
            .values()
            .filter(|session| session.user_id == *user_id)
            .cloned()
            .collect();
        
        Ok(user_sessions)
    }
    
    /// Update session metadata
    pub async fn update_session_metadata(
        &self,
        token: &str,
        metadata: SessionMetadata,
    ) -> Result<bool> {
        let token = SessionToken::from_str(token)?;
        let mut sessions = self.sessions.write().await;
        
        if let Some(session) = sessions.get_mut(&token) {
            session.metadata = metadata;
            Ok(true)
        } else {
            Ok(false)
        }
    }
    
    /// Extend session expiration
    pub async fn extend_session(
        &self,
        token: &str,
        additional_duration: Duration,
    ) -> Result<bool> {
        let token = SessionToken::from_str(token)?;
        let mut sessions = self.sessions.write().await;
        
        if let Some(session) = sessions.get_mut(&token) {
            let now = SystemTime::now();
            let new_expiration = session.expires_at + additional_duration;
            let max_expiration = now + self.max_duration;
            
            session.expires_at = new_expiration.min(max_expiration);
            Ok(true)
        } else {
            Ok(false)
        }
    }
    
    /// Clean up expired sessions
    pub async fn cleanup_expired(&self) -> Result<usize> {
        let mut sessions = self.sessions.write().await;
        let now = SystemTime::now();
        
        let expired_tokens: Vec<SessionToken> = sessions
            .iter()
            .filter(|(_, session)| session.expires_at <= now)
            .map(|(token, _)| token.clone())
            .collect();
        
        let count = expired_tokens.len();
        for token in expired_tokens {
            sessions.remove(&token);
        }
        
        Ok(count)
    }
    
    /// Get session statistics
    pub async fn get_stats(&self) -> SessionStats {
        let sessions = self.sessions.read().await;
        let now = SystemTime::now();
        let one_hour_ago = now - Duration::from_secs(3600);
        let one_hour_later = now + Duration::from_secs(3600);
        
        let mut by_type = HashMap::new();
        let mut by_user = HashMap::new();
        let mut total_duration = Duration::new(0, 0);
        let mut recent_sessions = 0;
        let mut expiring_soon = 0;
        
        for session in sessions.values() {
            // Count by type
            *by_type.entry(session.session_type).or_insert(0) += 1;
            
            // Count by user
            *by_user.entry(session.user_id).or_insert(0) += 1;
            
            // Calculate duration
            let duration = now.duration_since(session.created_at).unwrap_or_default();
            total_duration += duration;
            
            // Count recent sessions
            if session.created_at >= one_hour_ago {
                recent_sessions += 1;
            }
            
            // Count expiring soon
            if session.expires_at <= one_hour_later {
                expiring_soon += 1;
            }
        }
        
        let avg_duration_minutes = if !sessions.is_empty() {
            total_duration.as_secs() as f64 / sessions.len() as f64 / 60.0
        } else {
            0.0
        };
        
        SessionStats {
            total_sessions: sessions.len(),
            by_type,
            by_user,
            avg_duration_minutes,
            recent_sessions,
            expiring_soon,
        }
    }
    
    /// Check if a session exists
    pub async fn session_exists(&self, token: &str) -> bool {
        if let Ok(token) = SessionToken::from_str(token) {
            let sessions = self.sessions.read().await;
            sessions.contains_key(&token)
        } else {
            false
        }
    }
    
    /// Get session count for user
    pub async fn get_user_session_count(&self, user_id: &Uuid) -> usize {
        let sessions = self.sessions.read().await;
        sessions.values()
            .filter(|session| session.user_id == *user_id)
            .count()
    }
    
    /// Start automatic cleanup task
    pub async fn start_cleanup_task(&self) -> tokio::task::JoinHandle<()> {
        let session_manager = self.clone();
        let interval = self.cleanup_interval;
        
        tokio::spawn(async move {
            let mut cleanup_interval = tokio::time::interval(interval);
            
            loop {
                cleanup_interval.tick().await;
                
                if let Ok(cleaned) = session_manager.cleanup_expired().await {
                    if cleaned > 0 {
                        tracing::debug!("Cleaned up {} expired sessions", cleaned);
                    }
                }
            }
        })
    }
}

impl Default for SessionMetadata {
    fn default() -> Self {
        Self {
            client_info: None,
            location: None,
            device_id: None,
            app_version: None,
            custom: HashMap::new(),
        }
    }
}

impl Default for SessionFlags {
    fn default() -> Self {
        Self {
            read_only: false,
            requires_verification: false,
            trusted_device: false,
            elevated: false,
            monitored: false,
        }
    }
}

/// Session-related errors
#[derive(Debug, thiserror::Error)]
pub enum SessionError {
    #[error("Invalid session token format")]
    InvalidToken,
    
    #[error("Session expired")]
    Expired,
    
    #[error("Session not found")]
    NotFound,
    
    #[error("Maximum sessions per user exceeded")]
    TooManySessions,
    
    #[error("Session validation failed: {0}")]
    ValidationFailed(String),
    
    #[error("Database error: {0}")]
    DatabaseError(#[from] anyhow::Error),
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_session_creation() {
        let manager = SessionManager::new(Duration::from_secs(3600));
        let user_id = Uuid::new_v4();
        let callsign = "W1TEST".to_string();
        
        let session = manager.create_session(
            user_id,
            callsign.clone(),
            Duration::from_secs(1800),
        ).await.unwrap();
        
        assert_eq!(session.user_id, user_id);
        assert_eq!(session.callsign, callsign);
        assert!(session.expires_at > session.created_at);
    }
    
    #[tokio::test]
    async fn test_session_validation() {
        let manager = SessionManager::new(Duration::from_secs(3600));
        let user_id = Uuid::new_v4();
        
        let session = manager.create_session(
            user_id,
            "W1TEST".to_string(),
            Duration::from_secs(1800),
        ).await.unwrap();
        
        let token_str = session.token.to_string();
        
        // Valid session should be found
        let validated = manager.validate_session(&token_str).await.unwrap();
        assert!(validated.is_some());
        
        // Invalid token should return None
        let invalid = manager.validate_session("invalid-token").await.unwrap();
        assert!(invalid.is_none());
    }
    
    #[tokio::test]
    async fn test_session_invalidation() {
        let manager = SessionManager::new(Duration::from_secs(3600));
        let user_id = Uuid::new_v4();
        
        let session = manager.create_session(
            user_id,
            "W1TEST".to_string(),
            Duration::from_secs(1800),
        ).await.unwrap();
        
        let token_str = session.token.to_string();
        
        // Session should exist
        assert!(manager.session_exists(&token_str).await);
        
        // Invalidate session
        let invalidated = manager.invalidate_session(&token_str).await.unwrap();
        assert!(invalidated);
        
        // Session should no longer exist
        assert!(!manager.session_exists(&token_str).await);
    }
    
    #[tokio::test]
    async fn test_user_session_management() {
        let manager = SessionManager::new(Duration::from_secs(3600));
        let user_id = Uuid::new_v4();
        
        // Create multiple sessions for the same user
        let session1 = manager.create_session(
            user_id,
            "W1TEST".to_string(),
            Duration::from_secs(1800),
        ).await.unwrap();
        
        let session2 = manager.create_session(
            user_id,
            "W1TEST".to_string(),
            Duration::from_secs(1800),
        ).await.unwrap();
        
        // Should have 2 sessions for user
        let user_sessions = manager.get_user_sessions(&user_id).await.unwrap();
        assert_eq!(user_sessions.len(), 2);
        
        // Invalidate all user sessions
        let invalidated = manager.invalidate_user_sessions(&user_id).await.unwrap();
        assert_eq!(invalidated, 2);
        
        // Should have no sessions for user
        let user_sessions = manager.get_user_sessions(&user_id).await.unwrap();
        assert_eq!(user_sessions.len(), 0);
    }
    
    #[tokio::test]
    async fn test_session_expiration() {
        let manager = SessionManager::new(Duration::from_secs(1)); // 1 second duration
        let user_id = Uuid::new_v4();
        
        let session = manager.create_session(
            user_id,
            "W1TEST".to_string(),
            Duration::from_millis(100), // Very short duration
        ).await.unwrap();
        
        let token_str = session.token.to_string();
        
        // Wait for expiration
        tokio::time::sleep(Duration::from_millis(200)).await;
        
        // Session should be expired and return None
        let validated = manager.validate_session(&token_str).await.unwrap();
        assert!(validated.is_none());
    }
    
    #[tokio::test]
    async fn test_session_extension() {
        let manager = SessionManager::new(Duration::from_secs(3600));
        let user_id = Uuid::new_v4();
        
        let session = manager.create_session(
            user_id,
            "W1TEST".to_string(),
            Duration::from_secs(1800),
        ).await.unwrap();
        
        let token_str = session.token.to_string();
        let original_expiration = session.expires_at;
        
        // Extend session
        let extended = manager.extend_session(
            &token_str,
            Duration::from_secs(3600),
        ).await.unwrap();
        assert!(extended);
        
        // Get updated session
        let updated_session = manager.validate_session(&token_str).await.unwrap().unwrap();
        assert!(updated_session.expires_at > original_expiration);
    }
    
    #[tokio::test]
    async fn test_session_stats() {
        let manager = SessionManager::new(Duration::from_secs(3600));
        let user_id = Uuid::new_v4();
        
        // Create sessions of different types
        let params1 = SessionParams {
            user_id,
            callsign: "W1TEST".to_string(),
            duration: Some(Duration::from_secs(1800)),
            session_type: SessionType::Web,
            ..Default::default()
        };
        
        let params2 = SessionParams {
            user_id,
            callsign: "W1TEST".to_string(),
            duration: Some(Duration::from_secs(1800)),
            session_type: SessionType::Terminal,
            ..Default::default()
        };
        
        manager.create_session_with_params(params1).await.unwrap();
        manager.create_session_with_params(params2).await.unwrap();
        
        let stats = manager.get_stats().await;
        assert_eq!(stats.total_sessions, 2);
        assert_eq!(stats.by_type.get(&SessionType::Web), Some(&1));
        assert_eq!(stats.by_type.get(&SessionType::Terminal), Some(&1));
        assert_eq!(stats.by_user.get(&user_id), Some(&2));
    }
    
    #[test]
    fn test_session_token() {
        let token = SessionToken::new();
        let token_str = token.to_string();
        
        // Should be valid UUID format
        assert!(Uuid::parse_str(&token_str).is_ok());
        
        // Should be able to parse back
        let parsed_token = SessionToken::from_str(&token_str).unwrap();
        assert_eq!(token, parsed_token);
    }
}
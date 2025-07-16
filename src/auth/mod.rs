use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::sync::RwLock;
use uuid::Uuid;

pub mod callsign;
pub mod password;
pub mod session;

pub use callsign::{CallsignValidator, validate_callsign};
pub use password::{PasswordManager, hash_password, verify_password};
pub use session::{SessionManager, Session, SessionToken};

use crate::config::SecurityConfig;
use crate::database::Database;

/// Authentication system for OffGridComm
/// 
/// Handles callsign-based identity verification, password management,
/// and session tracking. Designed for FCC Part 97 compliance.
#[derive(Clone)]
pub struct AuthSystem {
    database: Database,
    session_manager: SessionManager,
    password_manager: PasswordManager,
    callsign_validator: CallsignValidator,
    login_attempts: RwLock<HashMap<String, LoginAttempts>>,
    config: SecurityConfig,
}

/// User authentication information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct User {
    /// Unique user ID
    pub id: Uuid,
    
    /// Amateur radio callsign (primary identifier)
    pub callsign: String,
    
    /// User's full name (optional)
    pub full_name: Option<String>,
    
    /// Email address (optional, for notifications)
    pub email: Option<String>,
    
    /// Geographic location/grid square
    pub location: Option<String>,
    
    /// User role/permissions
    pub role: UserRole,
    
    /// Account creation timestamp
    pub created_at: SystemTime,
    
    /// Last login timestamp
    pub last_login: Option<SystemTime>,
    
    /// Account status
    pub status: UserStatus,
    
    /// License class (if verified)
    pub license_class: Option<LicenseClass>,
    
    /// License verification status
    pub license_verified: bool,
}

/// User roles and permissions
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum UserRole {
    /// Regular user (can send/receive messages)
    User,
    
    /// Operator (can moderate groups)
    Operator,
    
    /// Administrator (full system access)
    Administrator,
    
    /// System (internal system user)
    System,
}

/// User account status
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum UserStatus {
    /// Active account
    Active,
    
    /// Pending verification
    Pending,
    
    /// Temporarily suspended
    Suspended,
    
    /// Permanently banned
    Banned,
    
    /// Account disabled by user
    Disabled,
}

/// Amateur radio license classes
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum LicenseClass {
    /// Technician class
    Technician,
    
    /// General class
    General,
    
    /// Amateur Extra class
    Extra,
    
    /// Advanced class (legacy)
    Advanced,
    
    /// Novice class (legacy)
    Novice,
}

/// Login attempt tracking
#[derive(Debug, Clone)]
struct LoginAttempts {
    count: u32,
    last_attempt: SystemTime,
    locked_until: Option<SystemTime>,
}

/// Authentication request
#[derive(Debug, Deserialize)]
pub struct AuthRequest {
    pub callsign: String,
    pub password: String,
    pub remember_me: Option<bool>,
}

/// Authentication response
#[derive(Debug, Serialize)]
pub struct AuthResponse {
    pub success: bool,
    pub session_token: Option<String>,
    pub user: Option<User>,
    pub error: Option<String>,
    pub locked_until: Option<u64>, // Unix timestamp
}

/// Registration request
#[derive(Debug, Deserialize)]
pub struct RegistrationRequest {
    pub callsign: String,
    pub password: String,
    pub full_name: Option<String>,
    pub email: Option<String>,
    pub location: Option<String>,
}

/// Registration response
#[derive(Debug, Serialize)]
pub struct RegistrationResponse {
    pub success: bool,
    pub user_id: Option<Uuid>,
    pub error: Option<String>,
    pub verification_required: bool,
}

/// Permission check result
#[derive(Debug, Clone)]
pub struct PermissionCheck {
    pub allowed: bool,
    pub reason: Option<String>,
    pub required_role: Option<UserRole>,
}

impl AuthSystem {
    /// Create a new authentication system
    pub async fn new(
        database: Database,
        config: SecurityConfig,
    ) -> Result<Self> {
        let session_manager = SessionManager::new(
            Duration::from_secs(config.session_timeout),
        );
        
        let password_manager = PasswordManager::new(
            config.password_min_length,
        );
        
        let callsign_validator = CallsignValidator::new(database.clone()).await?;
        
        Ok(Self {
            database,
            session_manager,
            password_manager,
            callsign_validator,
            login_attempts: RwLock::new(HashMap::new()),
            config,
        })
    }
    
    /// Authenticate a user with callsign and password
    pub async fn authenticate(
        &self,
        request: AuthRequest,
    ) -> Result<AuthResponse> {
        // Normalize callsign
        let callsign = request.callsign.to_uppercase();
        
        // Check if account is locked
        if let Some(locked_until) = self.is_account_locked(&callsign).await {
            return Ok(AuthResponse {
                success: false,
                session_token: None,
                user: None,
                error: Some("Account temporarily locked due to too many failed attempts".to_string()),
                locked_until: Some(locked_until.duration_since(UNIX_EPOCH)?.as_secs()),
            });
        }
        
        // Validate callsign format
        if let Err(e) = validate_callsign(&callsign) {
            self.record_failed_attempt(&callsign).await;
            return Ok(AuthResponse {
                success: false,
                session_token: None,
                user: None,
                error: Some(format!("Invalid callsign format: {}", e)),
                locked_until: None,
            });
        }
        
        // Get user from database
        let user = match self.database.get_user_by_callsign(&callsign).await? {
            Some(user) => user,
            None => {
                self.record_failed_attempt(&callsign).await;
                return Ok(AuthResponse {
                    success: false,
                    session_token: None,
                    user: None,
                    error: Some("Invalid callsign or password".to_string()),
                    locked_until: None,
                });
            }
        };
        
        // Check user status
        if user.status != UserStatus::Active {
            return Ok(AuthResponse {
                success: false,
                session_token: None,
                user: None,
                error: Some(match user.status {
                    UserStatus::Pending => "Account pending verification".to_string(),
                    UserStatus::Suspended => "Account suspended".to_string(),
                    UserStatus::Banned => "Account banned".to_string(),
                    UserStatus::Disabled => "Account disabled".to_string(),
                    _ => "Account not available".to_string(),
                }),
                locked_until: None,
            });
        }
        
        // Verify password
        let password_hash = self.database.get_user_password_hash(&user.id).await?;
        if !self.password_manager.verify_password(&request.password, &password_hash)? {
            self.record_failed_attempt(&callsign).await;
            return Ok(AuthResponse {
                success: false,
                session_token: None,
                user: None,
                error: Some("Invalid callsign or password".to_string()),
                locked_until: None,
            });
        }
        
        // Clear failed attempts on successful login
        self.clear_failed_attempts(&callsign).await;
        
        // Create session
        let session_duration = if request.remember_me.unwrap_or(false) {
            Duration::from_secs(self.config.session_timeout * 24) // 24x longer for "remember me"
        } else {
            Duration::from_secs(self.config.session_timeout)
        };
        
        let session = self.session_manager.create_session(
            user.id,
            callsign.clone(),
            session_duration,
        ).await?;
        
        // Update last login time
        self.database.update_last_login(&user.id).await?;
        
        Ok(AuthResponse {
            success: true,
            session_token: Some(session.token.to_string()),
            user: Some(user),
            error: None,
            locked_until: None,
        })
    }
    
    /// Register a new user
    pub async fn register(
        &self,
        request: RegistrationRequest,
    ) -> Result<RegistrationResponse> {
        // Normalize callsign
        let callsign = request.callsign.to_uppercase();
        
        // Validate callsign format
        if let Err(e) = validate_callsign(&callsign) {
            return Ok(RegistrationResponse {
                success: false,
                user_id: None,
                error: Some(format!("Invalid callsign format: {}", e)),
                verification_required: false,
            });
        }
        
        // Check if callsign already exists
        if self.database.get_user_by_callsign(&callsign).await?.is_some() {
            return Ok(RegistrationResponse {
                success: false,
                user_id: None,
                error: Some("Callsign already registered".to_string()),
                verification_required: false,
            });
        }
        
        // Validate password
        if let Err(e) = self.password_manager.validate_password(&request.password) {
            return Ok(RegistrationResponse {
                success: false,
                user_id: None,
                error: Some(format!("Password validation failed: {}", e)),
                verification_required: false,
            });
        }
        
        // Verify callsign against license database (if enabled)
        let verification_required = if self.config.require_callsign_verification {
            match self.callsign_validator.verify_callsign(&callsign).await {
                Ok(verified) => !verified,
                Err(_) => true, // Require verification if lookup fails
            }
        } else {
            false
        };
        
        // Hash password
        let password_hash = self.password_manager.hash_password(&request.password)?;
        
        // Create user
        let user = User {
            id: Uuid::new_v4(),
            callsign: callsign.clone(),
            full_name: request.full_name,
            email: request.email,
            location: request.location,
            role: UserRole::User,
            created_at: SystemTime::now(),
            last_login: None,
            status: if verification_required {
                UserStatus::Pending
            } else {
                UserStatus::Active
            },
            license_class: None,
            license_verified: false,
        };
        
        // Store user in database
        self.database.create_user(&user, &password_hash).await?;
        
        Ok(RegistrationResponse {
            success: true,
            user_id: Some(user.id),
            error: None,
            verification_required,
        })
    }
    
    /// Validate a session token
    pub async fn validate_session(&self, token: &str) -> Result<Option<User>> {
        match self.session_manager.validate_session(token).await? {
            Some(session) => {
                // Get current user data from database
                Ok(self.database.get_user_by_id(&session.user_id).await?)
            }
            None => Ok(None),
        }
    }
    
    /// Logout a user (invalidate session)
    pub async fn logout(&self, token: &str) -> Result<bool> {
        self.session_manager.invalidate_session(token).await
    }
    
    /// Check if user has required permission
    pub async fn check_permission(
        &self,
        user: &User,
        required_role: UserRole,
    ) -> PermissionCheck {
        // System role has all permissions
        if user.role == UserRole::System {
            return PermissionCheck {
                allowed: true,
                reason: None,
                required_role: None,
            };
        }
        
        // Check role hierarchy
        let allowed = match required_role {
            UserRole::User => matches!(user.role, UserRole::User | UserRole::Operator | UserRole::Administrator),
            UserRole::Operator => matches!(user.role, UserRole::Operator | UserRole::Administrator),
            UserRole::Administrator => user.role == UserRole::Administrator,
            UserRole::System => user.role == UserRole::System,
        };
        
        PermissionCheck {
            allowed,
            reason: if !allowed {
                Some(format!("Requires {:?} role or higher", required_role))
            } else {
                None
            },
            required_role: if !allowed { Some(required_role) } else { None },
        }
    }
    
    /// Change user password
    pub async fn change_password(
        &self,
        user_id: &Uuid,
        current_password: &str,
        new_password: &str,
    ) -> Result<bool> {
        // Get current password hash
        let current_hash = self.database.get_user_password_hash(user_id).await?;
        
        // Verify current password
        if !self.password_manager.verify_password(current_password, &current_hash)? {
            return Ok(false);
        }
        
        // Validate new password
        self.password_manager.validate_password(new_password)?;
        
        // Hash new password
        let new_hash = self.password_manager.hash_password(new_password)?;
        
        // Update password in database
        self.database.update_user_password(user_id, &new_hash).await?;
        
        // Invalidate all sessions for this user (force re-login)
        self.session_manager.invalidate_user_sessions(user_id).await?;
        
        Ok(true)
    }
    
    /// Update user profile
    pub async fn update_profile(
        &self,
        user_id: &Uuid,
        full_name: Option<String>,
        email: Option<String>,
        location: Option<String>,
    ) -> Result<()> {
        self.database.update_user_profile(user_id, full_name, email, location).await
    }
    
    /// Get user by callsign
    pub async fn get_user_by_callsign(&self, callsign: &str) -> Result<Option<User>> {
        let callsign = callsign.to_uppercase();
        self.database.get_user_by_callsign(&callsign).await
    }
    
    /// Get user by ID
    pub async fn get_user_by_id(&self, user_id: &Uuid) -> Result<Option<User>> {
        self.database.get_user_by_id(user_id).await
    }
    
    /// List all users (admin only)
    pub async fn list_users(
        &self,
        requester: &User,
        offset: u32,
        limit: u32,
    ) -> Result<Vec<User>> {
        let permission = self.check_permission(requester, UserRole::Administrator).await;
        if !permission.allowed {
            return Err(anyhow::anyhow!("Insufficient permissions"));
        }
        
        self.database.list_users(offset, limit).await
    }
    
    /// Update user status (admin only)
    pub async fn update_user_status(
        &self,
        requester: &User,
        user_id: &Uuid,
        new_status: UserStatus,
    ) -> Result<()> {
        let permission = self.check_permission(requester, UserRole::Administrator).await;
        if !permission.allowed {
            return Err(anyhow::anyhow!("Insufficient permissions"));
        }
        
        self.database.update_user_status(user_id, new_status).await
    }
    
    /// Update user role (admin only)
    pub async fn update_user_role(
        &self,
        requester: &User,
        user_id: &Uuid,
        new_role: UserRole,
    ) -> Result<()> {
        let permission = self.check_permission(requester, UserRole::Administrator).await;
        if !permission.allowed {
            return Err(anyhow::anyhow!("Insufficient permissions"));
        }
        
        // Prevent removing the last administrator
        if new_role != UserRole::Administrator {
            let admin_count = self.database.count_administrators().await?;
            if admin_count <= 1 {
                let current_user = self.database.get_user_by_id(user_id).await?;
                if let Some(user) = current_user {
                    if user.role == UserRole::Administrator {
                        return Err(anyhow::anyhow!("Cannot remove the last administrator"));
                    }
                }
            }
        }
        
        self.database.update_user_role(user_id, new_role).await
    }
    
    /// Clean up expired sessions and login attempts
    pub async fn cleanup(&self) -> Result<()> {
        // Clean up expired sessions
        self.session_manager.cleanup_expired().await?;
        
        // Clean up old login attempts
        let mut attempts = self.login_attempts.write().await;
        let now = SystemTime::now();
        attempts.retain(|_, attempt| {
            // Keep attempts that are still within lockout period
            if let Some(locked_until) = attempt.locked_until {
                locked_until > now
            } else {
                // Keep recent attempts (within 1 hour)
                now.duration_since(attempt.last_attempt)
                    .map(|d| d.as_secs() < 3600)
                    .unwrap_or(false)
            }
        });
        
        Ok(())
    }
    
    /// Check if account is locked due to failed attempts
    async fn is_account_locked(&self, callsign: &str) -> Option<SystemTime> {
        let attempts = self.login_attempts.read().await;
        if let Some(attempt) = attempts.get(callsign) {
            if let Some(locked_until) = attempt.locked_until {
                if locked_until > SystemTime::now() {
                    return Some(locked_until);
                }
            }
        }
        None
    }
    
    /// Record a failed login attempt
    async fn record_failed_attempt(&self, callsign: &str) {
        let mut attempts = self.login_attempts.write().await;
        let now = SystemTime::now();
        
        let entry = attempts.entry(callsign.to_string()).or_insert(LoginAttempts {
            count: 0,
            last_attempt: now,
            locked_until: None,
        });
        
        entry.count += 1;
        entry.last_attempt = now;
        
        // Lock account if too many attempts
        if entry.count >= self.config.max_login_attempts {
            entry.locked_until = Some(now + Duration::from_secs(self.config.lockout_duration));
        }
    }
    
    /// Clear failed login attempts for successful login
    async fn clear_failed_attempts(&self, callsign: &str) {
        let mut attempts = self.login_attempts.write().await;
        attempts.remove(callsign);
    }
}

/// Authentication errors
#[derive(Debug, thiserror::Error)]
pub enum AuthError {
    #[error("Invalid callsign format: {0}")]
    InvalidCallsign(String),
    
    #[error("Invalid password: {0}")]
    InvalidPassword(String),
    
    #[error("Authentication failed")]
    AuthenticationFailed,
    
    #[error("Account locked until {0:?}")]
    AccountLocked(SystemTime),
    
    #[error("Account not active: {0:?}")]
    AccountNotActive(UserStatus),
    
    #[error("Session expired or invalid")]
    InvalidSession,
    
    #[error("Insufficient permissions")]
    InsufficientPermissions,
    
    #[error("Callsign already registered")]
    CallsignExists,
    
    #[error("Database error: {0}")]
    DatabaseError(#[from] anyhow::Error),
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::Database;
    use tempfile::tempdir;
    
    async fn create_test_auth_system() -> AuthSystem {
        let temp_dir = tempdir().unwrap();
        let db_path = temp_dir.path().join("test.db");
        
        let mut db_config = crate::config::DatabaseConfig::default();
        db_config.path = db_path;
        
        let database = Database::new(&db_config).await.unwrap();
        database.migrate().await.unwrap();
        
        let security_config = SecurityConfig::default();
        
        AuthSystem::new(database, security_config).await.unwrap()
    }
    
    #[tokio::test]
    async fn test_user_registration() {
        let auth = create_test_auth_system().await;
        
        let request = RegistrationRequest {
            callsign: "W1TEST".to_string(),
            password: "testpassword123".to_string(),
            full_name: Some("Test User".to_string()),
            email: Some("test@example.com".to_string()),
            location: Some("FN42".to_string()),
        };
        
        let response = auth.register(request).await.unwrap();
        assert!(response.success);
        assert!(response.user_id.is_some());
    }
    
    #[tokio::test]
    async fn test_authentication() {
        let auth = create_test_auth_system().await;
        
        // Register user first
        let reg_request = RegistrationRequest {
            callsign: "W1TEST".to_string(),
            password: "testpassword123".to_string(),
            full_name: None,
            email: None,
            location: None,
        };
        
        let reg_response = auth.register(reg_request).await.unwrap();
        assert!(reg_response.success);
        
        // Authenticate
        let auth_request = AuthRequest {
            callsign: "W1TEST".to_string(),
            password: "testpassword123".to_string(),
            remember_me: None,
        };
        
        let auth_response = auth.authenticate(auth_request).await.unwrap();
        assert!(auth_response.success);
        assert!(auth_response.session_token.is_some());
        assert!(auth_response.user.is_some());
    }
    
    #[tokio::test]
    async fn test_invalid_callsign() {
        let auth = create_test_auth_system().await;
        
        let request = RegistrationRequest {
            callsign: "INVALID".to_string(),
            password: "testpassword123".to_string(),
            full_name: None,
            email: None,
            location: None,
        };
        
        let response = auth.register(request).await.unwrap();
        assert!(!response.success);
        assert!(response.error.is_some());
    }
    
    #[tokio::test]
    async fn test_weak_password() {
        let auth = create_test_auth_system().await;
        
        let request = RegistrationRequest {
            callsign: "W1TEST".to_string(),
            password: "123".to_string(), // Too short
            full_name: None,
            email: None,
            location: None,
        };
        
        let response = auth.register(request).await.unwrap();
        assert!(!response.success);
        assert!(response.error.is_some());
    }
    
    #[tokio::test]
    async fn test_permission_check() {
        let auth = create_test_auth_system().await;
        
        let user = User {
            id: Uuid::new_v4(),
            callsign: "W1TEST".to_string(),
            full_name: None,
            email: None,
            location: None,
            role: UserRole::User,
            created_at: SystemTime::now(),
            last_login: None,
            status: UserStatus::Active,
            license_class: None,
            license_verified: false,
        };
        
        // User should have User permissions
        let check = auth.check_permission(&user, UserRole::User).await;
        assert!(check.allowed);
        
        // User should not have Admin permissions
        let check = auth.check_permission(&user, UserRole::Administrator).await;
        assert!(!check.allowed);
    }
}
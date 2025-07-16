use anyhow::Result;
use sqlx::{Pool, Sqlite, SqlitePool, Row};
use std::path::Path;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::sync::RwLock;
use uuid::Uuid;

pub mod models;
pub mod migrations;
pub mod queries;

pub use models::*;
pub use migrations::Migration;

use crate::config::DatabaseConfig;
use crate::auth::{User, UserRole, UserStatus, LicenseInfo};

/// Main database interface for OffGridComm
/// 
/// Provides async database operations using SQLite with connection pooling.
/// Handles all persistent data including users, messages, nodes, and callsign verification.
#[derive(Clone)]
pub struct Database {
    /// SQLite connection pool
    pool: SqlitePool,
    
    /// Database configuration
    config: DatabaseConfig,
    
    /// In-memory cache for frequently accessed data
    cache: RwLock<DatabaseCache>,
}

/// In-memory cache for performance optimization
#[derive(Debug, Default)]
struct DatabaseCache {
    /// User cache (user_id -> User)
    users: std::collections::HashMap<Uuid, CachedUser>,
    
    /// Callsign to user ID mapping
    callsign_map: std::collections::HashMap<String, Uuid>,
    
    /// License info cache
    license_cache: std::collections::HashMap<String, LicenseInfo>,
    
    /// Node information cache
    node_cache: Option<NodeInfo>,
    
    /// Cache timestamps for expiration
    cache_timestamps: std::collections::HashMap<String, SystemTime>,
}

/// Cached user data
#[derive(Debug, Clone)]
struct CachedUser {
    user: User,
    password_hash: String,
    cached_at: SystemTime,
}

/// Node information
#[derive(Debug, Clone)]
pub struct NodeInfo {
    pub id: Uuid,
    pub callsign: String,
    pub location: Option<String>,
    pub description: String,
    pub created_at: SystemTime,
    pub last_seen: SystemTime,
    pub status: NodeStatus,
    pub version: String,
}

/// Node status
#[derive(Debug, Clone, Copy, sqlx::Type)]
#[repr(i32)]
pub enum NodeStatus {
    Online = 1,
    Offline = 0,
    Maintenance = 2,
    Unknown = -1,
}

/// Database statistics
#[derive(Debug, Clone)]
pub struct DatabaseStats {
    pub total_size_bytes: u64,
    pub user_count: u64,
    pub message_count: u64,
    pub group_count: u64,
    pub node_count: u64,
    pub license_cache_count: u64,
    pub last_backup: Option<SystemTime>,
    pub last_vacuum: Option<SystemTime>,
}

/// Database connection info
#[derive(Debug, Clone)]
pub struct ConnectionInfo {
    pub database_path: String,
    pub connection_count: u32,
    pub max_connections: u32,
    pub busy_timeout: Duration,
    pub wal_mode: bool,
    pub foreign_keys: bool,
}

impl Database {
    /// Create a new database instance
    pub async fn new(config: &DatabaseConfig) -> Result<Self> {
        let database_url = if config.path.to_string_lossy() == ":memory:" {
            "sqlite::memory:".to_string()
        } else {
            // Ensure parent directory exists
            if let Some(parent) = config.path.parent() {
                tokio::fs::create_dir_all(parent).await?;
            }
            format!("sqlite:{}?mode=rwc", config.path.display())
        };
        
        // Configure connection pool
        let pool = SqlitePool::connect_with(
            sqlx::sqlite::SqliteConnectOptions::new()
                .filename(&config.path)
                .create_if_missing(true)
                .journal_mode(if config.wal_mode {
                    sqlx::sqlite::SqliteJournalMode::Wal
                } else {
                    sqlx::sqlite::SqliteJournalMode::Delete
                })
                .synchronous(sqlx::sqlite::SqliteSynchronous::Normal)
                .busy_timeout(Duration::from_secs(config.connection_timeout))
                .foreign_keys(true)
        ).await?;
        
        // Set pool options
        let pool = SqlitePool::connect_with(
            sqlx::sqlite::SqliteConnectOptions::new()
                .filename(&config.path)
                .create_if_missing(true)
                .journal_mode(if config.wal_mode {
                    sqlx::sqlite::SqliteJournalMode::Wal
                } else {
                    sqlx::sqlite::SqliteJournalMode::Delete
                })
                .synchronous(sqlx::sqlite::SqliteSynchronous::Normal)
                .busy_timeout(Duration::from_secs(config.connection_timeout))
                .foreign_keys(true)
        ).await?;
        
        Ok(Self {
            pool,
            config: config.clone(),
            cache: RwLock::new(DatabaseCache::default()),
        })
    }
    
    /// Initialize database with required tables
    pub async fn initialize(&self) -> Result<()> {
        self.migrate().await
    }
    
    /// Run database migrations
    pub async fn migrate(&self) -> Result<()> {
        migrations::run_migrations(&self.pool).await
    }
    
    /// Check if node exists and is initialized
    pub async fn node_exists(&self) -> Result<bool> {
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM nodes")
            .fetch_one(&self.pool)
            .await?;
        Ok(count > 0)
    }
    
    /// Initialize node information
    pub async fn initialize_node(&self, callsign: &str, location: Option<&str>) -> Result<()> {
        let node_id = Uuid::new_v4();
        let now = SystemTime::now();
        let timestamp = now.duration_since(UNIX_EPOCH)?.as_secs() as i64;
        
        sqlx::query(
            "INSERT INTO nodes (id, callsign, location, description, created_at, last_seen, status, version)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)"
        )
        .bind(node_id.to_string())
        .bind(callsign)
        .bind(location)
        .bind("OffGridComm Node")
        .bind(timestamp)
        .bind(timestamp)
        .bind(NodeStatus::Online as i32)
        .bind(env!("CARGO_PKG_VERSION"))
        .execute(&self.pool)
        .await?;
        
        // Cache the node info
        let mut cache = self.cache.write().await;
        cache.node_cache = Some(NodeInfo {
            id: node_id,
            callsign: callsign.to_string(),
            location: location.map(|s| s.to_string()),
            description: "OffGridComm Node".to_string(),
            created_at: now,
            last_seen: now,
            status: NodeStatus::Online,
            version: env!("CARGO_PKG_VERSION").to_string(),
        });
        
        Ok(())
    }
    
    /// Get node information
    pub async fn get_node_info(&self) -> Result<Option<NodeInfo>> {
        // Check cache first
        {
            let cache = self.cache.read().await;
            if let Some(node_info) = &cache.node_cache {
                return Ok(Some(node_info.clone()));
            }
        }
        
        // Query database
        let row = sqlx::query(
            "SELECT id, callsign, location, description, created_at, last_seen, status, version
             FROM nodes LIMIT 1"
        )
        .fetch_optional(&self.pool)
        .await?;
        
        if let Some(row) = row {
            let node_info = NodeInfo {
                id: Uuid::parse_str(&row.get::<String, _>("id"))?,
                callsign: row.get("callsign"),
                location: row.get("location"),
                description: row.get("description"),
                created_at: UNIX_EPOCH + Duration::from_secs(row.get::<i64, _>("created_at") as u64),
                last_seen: UNIX_EPOCH + Duration::from_secs(row.get::<i64, _>("last_seen") as u64),
                status: match row.get::<i32, _>("status") {
                    1 => NodeStatus::Online,
                    0 => NodeStatus::Offline,
                    2 => NodeStatus::Maintenance,
                    _ => NodeStatus::Unknown,
                },
                version: row.get("version"),
            };
            
            // Cache the result
            let mut cache = self.cache.write().await;
            cache.node_cache = Some(node_info.clone());
            
            Ok(Some(node_info))
        } else {
            Ok(None)
        }
    }
    
    /// Update node last seen timestamp
    pub async fn update_node_last_seen(&self) -> Result<()> {
        let timestamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs() as i64;
        
        sqlx::query("UPDATE nodes SET last_seen = ?1")
            .bind(timestamp)
            .execute(&self.pool)
            .await?;
        
        // Update cache
        let mut cache = self.cache.write().await;
        if let Some(node_info) = &mut cache.node_cache {
            node_info.last_seen = UNIX_EPOCH + Duration::from_secs(timestamp as u64);
        }
        
        Ok(())
    }
    
    /// Create a new user
    pub async fn create_user(&self, user: &User, password_hash: &str) -> Result<()> {
        let timestamp = user.created_at.duration_since(UNIX_EPOCH)?.as_secs() as i64;
        let last_login_timestamp = user.last_login
            .map(|t| t.duration_since(UNIX_EPOCH).map(|d| d.as_secs() as i64))
            .transpose()?;
        
        sqlx::query(
            "INSERT INTO users (id, callsign, full_name, email, location, role, created_at, 
             last_login, status, license_class, license_verified, password_hash)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)"
        )
        .bind(user.id.to_string())
        .bind(&user.callsign)
        .bind(&user.full_name)
        .bind(&user.email)
        .bind(&user.location)
        .bind(user.role as i32)
        .bind(timestamp)
        .bind(last_login_timestamp)
        .bind(user.status as i32)
        .bind(user.license_class.map(|lc| lc as i32))
        .bind(user.license_verified)
        .bind(password_hash)
        .execute(&self.pool)
        .await?;
        
        // Update cache
        let mut cache = self.cache.write().await;
        cache.users.insert(user.id, CachedUser {
            user: user.clone(),
            password_hash: password_hash.to_string(),
            cached_at: SystemTime::now(),
        });
        cache.callsign_map.insert(user.callsign.clone(), user.id);
        
        Ok(())
    }
    
    /// Get user by ID
    pub async fn get_user_by_id(&self, user_id: &Uuid) -> Result<Option<User>> {
        // Check cache first
        {
            let cache = self.cache.read().await;
            if let Some(cached) = cache.users.get(user_id) {
                // Check if cache is still fresh (5 minutes)
                if cached.cached_at.elapsed().unwrap_or_default() < Duration::from_secs(300) {
                    return Ok(Some(cached.user.clone()));
                }
            }
        }
        
        // Query database
        let row = sqlx::query(
            "SELECT id, callsign, full_name, email, location, role, created_at, 
             last_login, status, license_class, license_verified
             FROM users WHERE id = ?1"
        )
        .bind(user_id.to_string())
        .fetch_optional(&self.pool)
        .await?;
        
        if let Some(row) = row {
            let user = User {
                id: *user_id,
                callsign: row.get("callsign"),
                full_name: row.get("full_name"),
                email: row.get("email"),
                location: row.get("location"),
                role: match row.get::<i32, _>("role") {
                    0 => UserRole::User,
                    1 => UserRole::Operator,
                    2 => UserRole::Administrator,
                    3 => UserRole::System,
                    _ => UserRole::User,
                },
                created_at: UNIX_EPOCH + Duration::from_secs(row.get::<i64, _>("created_at") as u64),
                last_login: row.get::<Option<i64>, _>("last_login")
                    .map(|t| UNIX_EPOCH + Duration::from_secs(t as u64)),
                status: match row.get::<i32, _>("status") {
                    0 => UserStatus::Active,
                    1 => UserStatus::Pending,
                    2 => UserStatus::Suspended,
                    3 => UserStatus::Banned,
                    4 => UserStatus::Disabled,
                    _ => UserStatus::Active,
                },
                license_class: row.get::<Option<i32>, _>("license_class")
                    .and_then(|lc| match lc {
                        0 => Some(crate::auth::LicenseClass::Technician),
                        1 => Some(crate::auth::LicenseClass::General),
                        2 => Some(crate::auth::LicenseClass::Extra),
                        3 => Some(crate::auth::LicenseClass::Advanced),
                        4 => Some(crate::auth::LicenseClass::Novice),
                        _ => None,
                    }),
                license_verified: row.get("license_verified"),
            };
            
            Ok(Some(user))
        } else {
            Ok(None)
        }
    }
    
    /// Get user by callsign
    pub async fn get_user_by_callsign(&self, callsign: &str) -> Result<Option<User>> {
        let callsign = callsign.to_uppercase();
        
        // Check cache first
        {
            let cache = self.cache.read().await;
            if let Some(user_id) = cache.callsign_map.get(&callsign) {
                if let Some(cached) = cache.users.get(user_id) {
                    if cached.cached_at.elapsed().unwrap_or_default() < Duration::from_secs(300) {
                        return Ok(Some(cached.user.clone()));
                    }
                }
            }
        }
        
        // Query database
        let row = sqlx::query(
            "SELECT id, callsign, full_name, email, location, role, created_at, 
             last_login, status, license_class, license_verified
             FROM users WHERE UPPER(callsign) = ?1"
        )
        .bind(&callsign)
        .fetch_optional(&self.pool)
        .await?;
        
        if let Some(row) = row {
            let user_id = Uuid::parse_str(&row.get::<String, _>("id"))?;
            let user = User {
                id: user_id,
                callsign: row.get("callsign"),
                full_name: row.get("full_name"),
                email: row.get("email"),
                location: row.get("location"),
                role: match row.get::<i32, _>("role") {
                    0 => UserRole::User,
                    1 => UserRole::Operator,
                    2 => UserRole::Administrator,
                    3 => UserRole::System,
                    _ => UserRole::User,
                },
                created_at: UNIX_EPOCH + Duration::from_secs(row.get::<i64, _>("created_at") as u64),
                last_login: row.get::<Option<i64>, _>("last_login")
                    .map(|t| UNIX_EPOCH + Duration::from_secs(t as u64)),
                status: match row.get::<i32, _>("status") {
                    0 => UserStatus::Active,
                    1 => UserStatus::Pending,
                    2 => UserStatus::Suspended,
                    3 => UserStatus::Banned,
                    4 => UserStatus::Disabled,
                    _ => UserStatus::Active,
                },
                license_class: row.get::<Option<i32>, _>("license_class")
                    .and_then(|lc| match lc {
                        0 => Some(crate::auth::LicenseClass::Technician),
                        1 => Some(crate::auth::LicenseClass::General),
                        2 => Some(crate::auth::LicenseClass::Extra),
                        3 => Some(crate::auth::LicenseClass::Advanced),
                        4 => Some(crate::auth::LicenseClass::Novice),
                        _ => None,
                    }),
                license_verified: row.get("license_verified"),
            };
            
            // Update cache
            let mut cache = self.cache.write().await;
            cache.callsign_map.insert(callsign, user_id);
            
            Ok(Some(user))
        } else {
            Ok(None)
        }
    }
    
    /// Get user password hash
    pub async fn get_user_password_hash(&self, user_id: &Uuid) -> Result<String> {
        // Check cache first
        {
            let cache = self.cache.read().await;
            if let Some(cached) = cache.users.get(user_id) {
                if cached.cached_at.elapsed().unwrap_or_default() < Duration::from_secs(300) {
                    return Ok(cached.password_hash.clone());
                }
            }
        }
        
        // Query database
        let password_hash: String = sqlx::query_scalar(
            "SELECT password_hash FROM users WHERE id = ?1"
        )
        .bind(user_id.to_string())
        .fetch_one(&self.pool)
        .await?;
        
        Ok(password_hash)
    }
    
    /// Update user password hash
    pub async fn update_user_password(&self, user_id: &Uuid, password_hash: &str) -> Result<()> {
        sqlx::query("UPDATE users SET password_hash = ?1 WHERE id = ?2")
            .bind(password_hash)
            .bind(user_id.to_string())
            .execute(&self.pool)
            .await?;
        
        // Update cache
        let mut cache = self.cache.write().await;
        if let Some(cached) = cache.users.get_mut(user_id) {
            cached.password_hash = password_hash.to_string();
            cached.cached_at = SystemTime::now();
        }
        
        Ok(())
    }
    
    /// Update user last login timestamp
    pub async fn update_last_login(&self, user_id: &Uuid) -> Result<()> {
        let timestamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs() as i64;
        
        sqlx::query("UPDATE users SET last_login = ?1 WHERE id = ?2")
            .bind(timestamp)
            .bind(user_id.to_string())
            .execute(&self.pool)
            .await?;
        
        // Update cache
        let mut cache = self.cache.write().await;
        if let Some(cached) = cache.users.get_mut(user_id) {
            cached.user.last_login = Some(UNIX_EPOCH + Duration::from_secs(timestamp as u64));
            cached.cached_at = SystemTime::now();
        }
        
        Ok(())
    }
    
    /// Update user profile information
    pub async fn update_user_profile(
        &self,
        user_id: &Uuid,
        full_name: Option<String>,
        email: Option<String>,
        location: Option<String>,
    ) -> Result<()> {
        sqlx::query(
            "UPDATE users SET full_name = ?1, email = ?2, location = ?3 WHERE id = ?4"
        )
        .bind(&full_name)
        .bind(&email)
        .bind(&location)
        .bind(user_id.to_string())
        .execute(&self.pool)
        .await?;
        
        // Update cache
        let mut cache = self.cache.write().await;
        if let Some(cached) = cache.users.get_mut(user_id) {
            cached.user.full_name = full_name;
            cached.user.email = email;
            cached.user.location = location;
            cached.cached_at = SystemTime::now();
        }
        
        Ok(())
    }
    
    /// Update user status
    pub async fn update_user_status(&self, user_id: &Uuid, status: UserStatus) -> Result<()> {
        sqlx::query("UPDATE users SET status = ?1 WHERE id = ?2")
            .bind(status as i32)
            .bind(user_id.to_string())
            .execute(&self.pool)
            .await?;
        
        // Invalidate cache entry to force refresh
        let mut cache = self.cache.write().await;
        cache.users.remove(user_id);
        
        Ok(())
    }
    
    /// Update user role
    pub async fn update_user_role(&self, user_id: &Uuid, role: UserRole) -> Result<()> {
        sqlx::query("UPDATE users SET role = ?1 WHERE id = ?2")
            .bind(role as i32)
            .bind(user_id.to_string())
            .execute(&self.pool)
            .await?;
        
        // Invalidate cache entry to force refresh
        let mut cache = self.cache.write().await;
        cache.users.remove(user_id);
        
        Ok(())
    }
    
    /// List users with pagination
    pub async fn list_users(&self, offset: u32, limit: u32) -> Result<Vec<User>> {
        let rows = sqlx::query(
            "SELECT id, callsign, full_name, email, location, role, created_at, 
             last_login, status, license_class, license_verified
             FROM users ORDER BY created_at DESC LIMIT ?1 OFFSET ?2"
        )
        .bind(limit as i64)
        .bind(offset as i64)
        .fetch_all(&self.pool)
        .await?;
        
        let mut users = Vec::new();
        for row in rows {
            let user = User {
                id: Uuid::parse_str(&row.get::<String, _>("id"))?,
                callsign: row.get("callsign"),
                full_name: row.get("full_name"),
                email: row.get("email"),
                location: row.get("location"),
                role: match row.get::<i32, _>("role") {
                    0 => UserRole::User,
                    1 => UserRole::Operator,
                    2 => UserRole::Administrator,
                    3 => UserRole::System,
                    _ => UserRole::User,
                },
                created_at: UNIX_EPOCH + Duration::from_secs(row.get::<i64, _>("created_at") as u64),
                last_login: row.get::<Option<i64>, _>("last_login")
                    .map(|t| UNIX_EPOCH + Duration::from_secs(t as u64)),
                status: match row.get::<i32, _>("status") {
                    0 => UserStatus::Active,
                    1 => UserStatus::Pending,
                    2 => UserStatus::Suspended,
                    3 => UserStatus::Banned,
                    4 => UserStatus::Disabled,
                    _ => UserStatus::Active,
                },
                license_class: row.get::<Option<i32>, _>("license_class")
                    .and_then(|lc| match lc {
                        0 => Some(crate::auth::LicenseClass::Technician),
                        1 => Some(crate::auth::LicenseClass::General),
                        2 => Some(crate::auth::LicenseClass::Extra),
                        3 => Some(crate::auth::LicenseClass::Advanced),
                        4 => Some(crate::auth::LicenseClass::Novice),
                        _ => None,
                    }),
                license_verified: row.get("license_verified"),
            };
            users.push(user);
        }
        
        Ok(users)
    }
    
    /// Count administrators
    pub async fn count_administrators(&self) -> Result<i64> {
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM users WHERE role = ?1"
        )
        .bind(UserRole::Administrator as i32)
        .fetch_one(&self.pool)
        .await?;
        
        Ok(count)
    }
    
    /// Cache license information
    pub async fn cache_license_info(&self, license_info: &LicenseInfo) -> Result<()> {
        let timestamp = license_info.verified_at.duration_since(UNIX_EPOCH)?.as_secs() as i64;
        let expires_timestamp = license_info.expires
            .map(|t| t.duration_since(UNIX_EPOCH).map(|d| d.as_secs() as i64))
            .transpose()?;
        
        sqlx::query(
            "INSERT OR REPLACE INTO license_cache 
             (callsign, name, license_class, status, expires, country, verified_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)"
        )
        .bind(&license_info.callsign)
        .bind(&license_info.name)
        .bind(&license_info.license_class)
        .bind(license_info.status as i32)
        .bind(expires_timestamp)
        .bind(&license_info.country)
        .bind(timestamp)
        .execute(&self.pool)
        .await?;
        
        // Update memory cache
        let mut cache = self.cache.write().await;
        cache.license_cache.insert(license_info.callsign.clone(), license_info.clone());
        
        Ok(())
    }
    
    /// Get cached license information
    pub async fn get_cached_license_info(&self, callsign: &str) -> Result<Option<LicenseInfo>> {
        let callsign = callsign.to_uppercase();
        
        // Check memory cache first
        {
            let cache = self.cache.read().await;
            if let Some(license_info) = cache.license_cache.get(&callsign) {
                return Ok(Some(license_info.clone()));
            }
        }
        
        // Query database
        let row = sqlx::query(
            "SELECT callsign, name, license_class, status, expires, country, verified_at
             FROM license_cache WHERE UPPER(callsign) = ?1"
        )
        .bind(&callsign)
        .fetch_optional(&self.pool)
        .await?;
        
        if let Some(row) = row {
            let license_info = LicenseInfo {
                callsign: row.get("callsign"),
                name: row.get("name"),
                license_class: row.get("license_class"),
                status: match row.get::<i32, _>("status") {
                    0 => crate::auth::LicenseStatus::Active,
                    1 => crate::auth::LicenseStatus::Expired,
                    2 => crate::auth::LicenseStatus::Canceled,
                    3 => crate::auth::LicenseStatus::Suspended,
                    _ => crate::auth::LicenseStatus::Unknown,
                },
                expires: row.get::<Option<i64>, _>("expires")
                    .map(|t| UNIX_EPOCH + Duration::from_secs(t as u64)),
                address: None, // Not stored in cache table
                country: row.get("country"),
                verified_at: UNIX_EPOCH + Duration::from_secs(row.get::<i64, _>("verified_at") as u64),
            };
            
            // Update memory cache
            let mut cache = self.cache.write().await;
            cache.license_cache.insert(callsign, license_info.clone());
            
            Ok(Some(license_info))
        } else {
            Ok(None)
        }
    }
    
    /// Import FCC callsign database
    pub async fn import_fcc_callsigns(&self, _file_path: &Path) -> Result<()> {
        // TODO: Implement FCC database import
        // This would parse the FCC ULS database files and import them
        Ok(())
    }
    
    /// Import Industry Canada callsign database
    pub async fn import_ic_callsigns(&self, _file_path: &Path) -> Result<()> {
        // TODO: Implement IC database import
        Ok(())
    }
    
    /// Get database statistics
    pub async fn get_stats(&self) -> Result<DatabaseStats> {
        let user_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users")
            .fetch_one(&self.pool)
            .await?;
        
        let message_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM messages")
            .fetch_optional(&self.pool)
            .await?
            .unwrap_or(0);
        
        let group_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM groups")
            .fetch_optional(&self.pool)
            .await?
            .unwrap_or(0);
        
        let node_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM nodes")
            .fetch_one(&self.pool)
            .await?;
        
        let license_cache_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM license_cache")
            .fetch_one(&self.pool)
            .await?;
        
        // Get database file size (if not in-memory)
        let total_size_bytes = if self.config.path.to_string_lossy() != ":memory:" {
            tokio::fs::metadata(&self.config.path)
                .await
                .map(|meta| meta.len())
                .unwrap_or(0)
        } else {
            0
        };
        
        Ok(DatabaseStats {
            total_size_bytes,
            user_count: user_count as u64,
            message_count: message_count as u64,
            group_count: group_count as u64,
            node_count: node_count as u64,
            license_cache_count: license_cache_count as u64,
            last_backup: None, // TODO: Track backup timestamps
            last_vacuum: None, // TODO: Track vacuum timestamps
        })
    }
    
    /// Compact/optimize the database
    pub async fn compact(&self) -> Result<()> {
        // Run VACUUM to reclaim space and optimize
        sqlx::query("VACUUM").execute(&self.pool).await?;
        
        // Analyze to update statistics
        sqlx::query("ANALYZE").execute(&self.pool).await?;
        
        Ok(())
    }
    
    /// Get connection information
    pub async fn get_connection_info(&self) -> Result<ConnectionInfo> {
        Ok(ConnectionInfo {
            database_path: self.config.path.to_string_lossy().to_string(),
            connection_count: self.pool.size(),
            max_connections: self.config.max_connections,
            busy_timeout: Duration::from_secs(self.config.connection_timeout),
            wal_mode: self.config.wal_mode,
            foreign_keys: true,
        })
    }
    
    /// Clear all caches
    pub async fn clear_cache(&self) {
        let mut cache = self.cache.write().await;
        cache.users.clear();
        cache.callsign_map.clear();
        cache.license_cache.clear();
        cache.cache_timestamps.clear();
    }
    
    /// Get cache statistics
    pub async fn get_cache_stats(&self) -> CacheStats {
        let cache = self.cache.read().await;
        CacheStats {
            user_cache_size: cache.users.len(),
            callsign_map_size: cache.callsign_map.len(),
            license_cache_size: cache.license_cache.len(),
            node_cached: cache.node_cache.is_some(),
        }
    }
    
    /// Backup database to file
    pub async fn backup(&self, backup_path: &Path) -> Result<()> {
        if self.config.path.to_string_lossy() == ":memory:" {
            return Err(anyhow::anyhow!("Cannot backup in-memory database"));
        }
        
        // Create backup directory if it doesn't exist
        if let Some(parent) = backup_path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        
        // For SQLite, we can use the backup API or simple file copy
        // Using file copy for simplicity (should use WAL checkpoint for production)
        tokio::fs::copy(&self.config.path, backup_path).await?;
        
        Ok(())
    }
    
    /// Restore database from backup
    pub async fn restore(&self, backup_path: &Path) -> Result<()> {
        if self.config.path.to_string_lossy() == ":memory:" {
            return Err(anyhow::anyhow!("Cannot restore to in-memory database"));
        }
        
        if !backup_path.exists() {
            return Err(anyhow::anyhow!("Backup file does not exist"));
        }
        
        // Close existing connections (would need connection pool restart in practice)
        // For now, just copy the file
        tokio::fs::copy(backup_path, &self.config.path).await?;
        
        // Clear caches after restore
        self.clear_cache().await;
        
        Ok(())
    }
    
    /// Execute a raw SQL query (for administrative purposes)
    pub async fn execute_raw(&self, sql: &str) -> Result<u64> {
        let result = sqlx::query(sql).execute(&self.pool).await?;
        Ok(result.rows_affected())
    }
    
    /// Execute a raw SQL query and return results
    pub async fn query_raw(&self, sql: &str) -> Result<Vec<std::collections::HashMap<String, sqlx::types::Json<serde_json::Value>>>> {
        let rows = sqlx::query(sql).fetch_all(&self.pool).await?;
        
        let mut results = Vec::new();
        for row in rows {
            let mut map = std::collections::HashMap::new();
            for (i, column) in row.columns().iter().enumerate() {
                let value: serde_json::Value = row.try_get(i)?;
                map.insert(column.name().to_string(), sqlx::types::Json(value));
            }
            results.push(map);
        }
        
        Ok(results)
    }
    
    /// Check database integrity
    pub async fn check_integrity(&self) -> Result<bool> {
        let result: String = sqlx::query_scalar("PRAGMA integrity_check")
            .fetch_one(&self.pool)
            .await?;
        
        Ok(result == "ok")
    }
    
    /// Get database schema version
    pub async fn get_schema_version(&self) -> Result<i32> {
        // Try to get version from metadata table
        let version: Option<i32> = sqlx::query_scalar(
            "SELECT version FROM schema_metadata WHERE key = 'version'"
        )
        .fetch_optional(&self.pool)
        .await?;
        
        Ok(version.unwrap_or(0))
    }
    
    /// Set database schema version
    pub async fn set_schema_version(&self, version: i32) -> Result<()> {
        sqlx::query(
            "INSERT OR REPLACE INTO schema_metadata (key, version, updated_at) VALUES ('version', ?1, ?2)"
        )
        .bind(version)
        .bind(SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs() as i64)
        .execute(&self.pool)
        .await?;
        
        Ok(())
    }
    
    /// Close database connections
    pub async fn close(&self) {
        self.pool.close().await;
    }
}

/// Cache statistics
#[derive(Debug, Clone)]
pub struct CacheStats {
    pub user_cache_size: usize,
    pub callsign_map_size: usize,
    pub license_cache_size: usize,
    pub node_cached: bool,
}

/// Database errors
#[derive(Debug, thiserror::Error)]
pub enum DatabaseError {
    #[error("Database connection failed: {0}")]
    ConnectionFailed(String),
    
    #[error("Migration failed: {0}")]
    MigrationFailed(String),
    
    #[error("Query failed: {0}")]
    QueryFailed(String),
    
    #[error("Data validation failed: {0}")]
    ValidationFailed(String),
    
    #[error("Constraint violation: {0}")]
    ConstraintViolation(String),
    
    #[error("Cache error: {0}")]
    CacheError(String),
    
    #[error("Backup/restore error: {0}")]
    BackupError(String),
    
    #[error("Database integrity check failed")]
    IntegrityCheckFailed,
    
    #[error("Unsupported operation: {0}")]
    UnsupportedOperation(String),
}

impl From<sqlx::Error> for DatabaseError {
    fn from(err: sqlx::Error) -> Self {
        match err {
            sqlx::Error::RowNotFound => DatabaseError::QueryFailed("Row not found".to_string()),
            sqlx::Error::Database(db_err) => {
                if db_err.is_unique_violation() {
                    DatabaseError::ConstraintViolation("Unique constraint violation".to_string())
                } else if db_err.is_foreign_key_violation() {
                    DatabaseError::ConstraintViolation("Foreign key constraint violation".to_string())
                } else {
                    DatabaseError::QueryFailed(db_err.to_string())
                }
            }
            _ => DatabaseError::QueryFailed(err.to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;
    use crate::auth::{User, UserRole, UserStatus};
    
    async fn create_test_database() -> Database {
        let temp_dir = tempdir().unwrap();
        let db_path = temp_dir.path().join("test.db");
        
        let mut config = DatabaseConfig::default();
        config.path = db_path;
        
        let db = Database::new(&config).await.unwrap();
        db.migrate().await.unwrap();
        
        db
    }
    
    #[tokio::test]
    async fn test_database_creation() {
        let db = create_test_database().await;
        
        // Database should be created and migrated
        assert!(db.get_schema_version().await.unwrap() > 0);
    }
    
    #[tokio::test]
    async fn test_node_operations() {
        let db = create_test_database().await;
        
        // Node should not exist initially
        assert!(!db.node_exists().await.unwrap());
        
        // Initialize node
        db.initialize_node("W1TEST", Some("FN42")).await.unwrap();
        
        // Node should now exist
        assert!(db.node_exists().await.unwrap());
        
        // Get node info
        let node_info = db.get_node_info().await.unwrap().unwrap();
        assert_eq!(node_info.callsign, "W1TEST");
        assert_eq!(node_info.location, Some("FN42".to_string()));
    }
    
    #[tokio::test]
    async fn test_user_operations() {
        let db = create_test_database().await;
        
        let user = User {
            id: Uuid::new_v4(),
            callsign: "W1TEST".to_string(),
            full_name: Some("Test User".to_string()),
            email: Some("test@example.com".to_string()),
            location: Some("FN42".to_string()),
            role: UserRole::User,
            created_at: SystemTime::now(),
            last_login: None,
            status: UserStatus::Active,
            license_class: Some(crate::auth::LicenseClass::General),
            license_verified: true,
        };
        
        let password_hash = "test_hash";
        
        // Create user
        db.create_user(&user, password_hash).await.unwrap();
        
        // Get user by ID
        let retrieved_user = db.get_user_by_id(&user.id).await.unwrap().unwrap();
        assert_eq!(retrieved_user.callsign, user.callsign);
        assert_eq!(retrieved_user.email, user.email);
        
        // Get user by callsign
        let retrieved_user = db.get_user_by_callsign("W1TEST").await.unwrap().unwrap();
        assert_eq!(retrieved_user.id, user.id);
        
        // Get password hash
        let retrieved_hash = db.get_user_password_hash(&user.id).await.unwrap();
        assert_eq!(retrieved_hash, password_hash);
        
        // Update password
        let new_hash = "new_test_hash";
        db.update_user_password(&user.id, new_hash).await.unwrap();
        let updated_hash = db.get_user_password_hash(&user.id).await.unwrap();
        assert_eq!(updated_hash, new_hash);
        
        // Update profile
        db.update_user_profile(
            &user.id,
            Some("Updated Name".to_string()),
            Some("updated@example.com".to_string()),
            Some("FM29".to_string()),
        ).await.unwrap();
        
        let updated_user = db.get_user_by_id(&user.id).await.unwrap().unwrap();
        assert_eq!(updated_user.full_name, Some("Updated Name".to_string()));
        assert_eq!(updated_user.email, Some("updated@example.com".to_string()));
        assert_eq!(updated_user.location, Some("FM29".to_string()));
    }
    
    #[tokio::test]
    async fn test_license_cache() {
        let db = create_test_database().await;
        
        let license_info = LicenseInfo {
            callsign: "W1AW".to_string(),
            name: Some("ARRL".to_string()),
            license_class: Some("Extra".to_string()),
            status: crate::auth::LicenseStatus::Active,
            expires: Some(SystemTime::now() + Duration::from_secs(365 * 24 * 3600)),
            address: None,
            country: "United States".to_string(),
            verified_at: SystemTime::now(),
        };
        
        // Cache license info
        db.cache_license_info(&license_info).await.unwrap();
        
        // Retrieve cached info
        let cached_info = db.get_cached_license_info("W1AW").await.unwrap().unwrap();
        assert_eq!(cached_info.callsign, license_info.callsign);
        assert_eq!(cached_info.name, license_info.name);
        assert_eq!(cached_info.status, license_info.status);
    }
    
    #[tokio::test]
    async fn test_database_stats() {
        let db = create_test_database().await;
        
        // Initialize node and create a user
        db.initialize_node("W1TEST", None).await.unwrap();
        
        let user = User {
            id: Uuid::new_v4(),
            callsign: "W1USER".to_string(),
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
        
        db.create_user(&user, "test_hash").await.unwrap();
        
        // Get stats
        let stats = db.get_stats().await.unwrap();
        assert_eq!(stats.user_count, 1);
        assert_eq!(stats.node_count, 1);
        assert!(stats.total_size_bytes > 0);
    }
    
    #[tokio::test]
    async fn test_database_integrity() {
        let db = create_test_database().await;
        
        // Check integrity
        let is_ok = db.check_integrity().await.unwrap();
        assert!(is_ok);
    }
    
    #[tokio::test]
    async fn test_cache_operations() {
        let db = create_test_database().await;
        
        let user = User {
            id: Uuid::new_v4(),
            callsign: "W1CACHE".to_string(),
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
        
        db.create_user(&user, "test_hash").await.unwrap();
        
        // First call should populate cache
        let user1 = db.get_user_by_id(&user.id).await.unwrap().unwrap();
        assert_eq!(user1.callsign, "W1CACHE");
        
        // Second call should use cache
        let user2 = db.get_user_by_id(&user.id).await.unwrap().unwrap();
        assert_eq!(user2.callsign, "W1CACHE");
        
        // Get cache stats
        let cache_stats = db.get_cache_stats().await;
        assert!(cache_stats.user_cache_size > 0);
        assert!(cache_stats.callsign_map_size > 0);
        
        // Clear cache
        db.clear_cache().await;
        
        let cache_stats = db.get_cache_stats().await;
        assert_eq!(cache_stats.user_cache_size, 0);
        assert_eq!(cache_stats.callsign_map_size, 0);
    }
    
    #[tokio::test]
    async fn test_backup_restore() {
        let db = create_test_database().await;
        let temp_dir = tempdir().unwrap();
        let backup_path = temp_dir.path().join("backup.db");
        
        // Initialize some data
        db.initialize_node("W1BACKUP", None).await.unwrap();
        
        // Backup database
        db.backup(&backup_path).await.unwrap();
        assert!(backup_path.exists());
        
        // Verify backup file size
        let backup_size = tokio::fs::metadata(&backup_path).await.unwrap().len();
        assert!(backup_size > 0);
    }
}
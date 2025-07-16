use anyhow::Result;
use sqlx::{Pool, Sqlite, Row};
use std::time::{SystemTime, UNIX_EPOCH};
use tracing::{info, warn, debug};

/// Database migration structure
#[derive(Debug, Clone)]
pub struct Migration {
    /// Migration version number
    pub version: i32,
    
    /// Migration name/description
    pub name: String,
    
    /// SQL statements to apply migration
    pub up_sql: Vec<String>,
    
    /// SQL statements to rollback migration (optional)
    pub down_sql: Option<Vec<String>>,
    
    /// Whether this migration is reversible
    pub reversible: bool,
}

/// Migration execution result
#[derive(Debug)]
pub struct MigrationResult {
    /// Migrations that were applied
    pub applied: Vec<i32>,
    
    /// Current schema version after migrations
    pub current_version: i32,
    
    /// Total time taken for migrations
    pub duration: std::time::Duration,
}

/// Run all pending migrations
pub async fn run_migrations(pool: &Pool<Sqlite>) -> Result<MigrationResult> {
    let start_time = std::time::Instant::now();
    info!("Starting database migrations");
    
    // Ensure migration tracking table exists
    ensure_migration_table(pool).await?;
    
    // Get current schema version
    let current_version = get_current_version(pool).await?;
    debug!("Current schema version: {}", current_version);
    
    // Get all available migrations
    let all_migrations = get_all_migrations();
    
    // Filter to pending migrations
    let pending_migrations: Vec<&Migration> = all_migrations
        .iter()
        .filter(|m| m.version > current_version)
        .collect();
    
    if pending_migrations.is_empty() {
        info!("No pending migrations");
        return Ok(MigrationResult {
            applied: vec![],
            current_version,
            duration: start_time.elapsed(),
        });
    }
    
    info!("Applying {} pending migration(s)", pending_migrations.len());
    
    let mut applied_versions = Vec::new();
    
    // Apply each migration in a transaction
    for migration in pending_migrations {
        info!("Applying migration {}: {}", migration.version, migration.name);
        
        let mut tx = pool.begin().await?;
        
        // Execute migration SQL
        for sql in &migration.up_sql {
            debug!("Executing SQL: {}", sql);
            sqlx::query(sql).execute(&mut *tx).await?;
        }
        
        // Record migration in tracking table
        let timestamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs() as i64;
        sqlx::query(
            "INSERT INTO schema_migrations (version, name, applied_at) VALUES (?1, ?2, ?3)"
        )
        .bind(migration.version)
        .bind(&migration.name)
        .bind(timestamp)
        .execute(&mut *tx)
        .await?;
        
        // Update schema version in metadata
        sqlx::query(
            "INSERT OR REPLACE INTO schema_metadata (key, version, updated_at) VALUES ('version', ?1, ?2)"
        )
        .bind(migration.version)
        .bind(timestamp)
        .execute(&mut *tx)
        .await?;
        
        tx.commit().await?;
        
        applied_versions.push(migration.version);
        info!("Successfully applied migration {}", migration.version);
    }
    
    let final_version = applied_versions.last().copied().unwrap_or(current_version);
    let duration = start_time.elapsed();
    
    info!("Migrations completed. Schema version: {} -> {}, Duration: {:?}", 
          current_version, final_version, duration);
    
    Ok(MigrationResult {
        applied: applied_versions,
        current_version: final_version,
        duration,
    })
}

/// Rollback migrations to a specific version
pub async fn rollback_to_version(pool: &Pool<Sqlite>, target_version: i32) -> Result<()> {
    let current_version = get_current_version(pool).await?;
    
    if target_version >= current_version {
        return Err(anyhow::anyhow!("Target version {} is not lower than current version {}", 
                                  target_version, current_version));
    }
    
    let all_migrations = get_all_migrations();
    
    // Get migrations to rollback (in reverse order)
    let mut rollback_migrations: Vec<&Migration> = all_migrations
        .iter()
        .filter(|m| m.version > target_version && m.version <= current_version)
        .collect();
    
    rollback_migrations.sort_by(|a, b| b.version.cmp(&a.version)); // Reverse order
    
    info!("Rolling back {} migration(s) from version {} to {}", 
          rollback_migrations.len(), current_version, target_version);
    
    for migration in rollback_migrations {
        if !migration.reversible {
            return Err(anyhow::anyhow!("Migration {} is not reversible", migration.version));
        }
        
        if let Some(down_sql) = &migration.down_sql {
            info!("Rolling back migration {}: {}", migration.version, migration.name);
            
            let mut tx = pool.begin().await?;
            
            // Execute rollback SQL
            for sql in down_sql {
                sqlx::query(sql).execute(&mut *tx).await?;
            }
            
            // Remove migration record
            sqlx::query("DELETE FROM schema_migrations WHERE version = ?1")
                .bind(migration.version)
                .execute(&mut *tx)
                .await?;
            
            tx.commit().await?;
            
            info!("Successfully rolled back migration {}", migration.version);
        }
    }
    
    // Update schema version
    let timestamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs() as i64;
    sqlx::query(
        "INSERT OR REPLACE INTO schema_metadata (key, version, updated_at) VALUES ('version', ?1, ?2)"
    )
    .bind(target_version)
    .bind(timestamp)
    .execute(pool)
    .await?;
    
    info!("Rollback completed to version {}", target_version);
    Ok(())
}

/// Get current schema version
async fn get_current_version(pool: &Pool<Sqlite>) -> Result<i32> {
    // Try to get from metadata table first
    let version: Option<i32> = sqlx::query_scalar(
        "SELECT version FROM schema_metadata WHERE key = 'version'"
    )
    .fetch_optional(pool)
    .await?;
    
    if let Some(v) = version {
        return Ok(v);
    }
    
    // Fallback: get highest version from migrations table
    let version: Option<i32> = sqlx::query_scalar(
        "SELECT MAX(version) FROM schema_migrations"
    )
    .fetch_optional(pool)
    .await?;
    
    Ok(version.unwrap_or(0))
}

/// Ensure migration tracking tables exist
async fn ensure_migration_table(pool: &Pool<Sqlite>) -> Result<()> {
    // Create schema_migrations table
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS schema_migrations (
            version INTEGER PRIMARY KEY,
            name TEXT NOT NULL,
            applied_at INTEGER NOT NULL
        )"
    )
    .execute(pool)
    .await?;
    
    // Create schema_metadata table
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS schema_metadata (
            key TEXT PRIMARY KEY,
            version INTEGER,
            updated_at INTEGER NOT NULL
        )"
    )
    .execute(pool)
    .await?;
    
    Ok(())
}

/// Get all available migrations in order
fn get_all_migrations() -> Vec<Migration> {
    vec![
        // Migration 1: Initial schema
        Migration {
            version: 1,
            name: "Initial schema".to_string(),
            up_sql: vec![
                // Users table
                "CREATE TABLE users (
                    id TEXT PRIMARY KEY,
                    callsign TEXT UNIQUE NOT NULL,
                    full_name TEXT,
                    email TEXT,
                    location TEXT,
                    role INTEGER NOT NULL DEFAULT 0,
                    created_at INTEGER NOT NULL,
                    last_login INTEGER,
                    status INTEGER NOT NULL DEFAULT 0,
                    license_class INTEGER,
                    license_verified BOOLEAN NOT NULL DEFAULT FALSE,
                    password_hash TEXT NOT NULL
                )".to_string(),
                
                // Nodes table
                "CREATE TABLE nodes (
                    id TEXT PRIMARY KEY,
                    callsign TEXT UNIQUE NOT NULL,
                    location TEXT,
                    description TEXT NOT NULL,
                    created_at INTEGER NOT NULL,
                    last_seen INTEGER NOT NULL,
                    status INTEGER NOT NULL DEFAULT 0,
                    version TEXT NOT NULL
                )".to_string(),
                
                // License cache table
                "CREATE TABLE license_cache (
                    callsign TEXT PRIMARY KEY,
                    name TEXT,
                    license_class TEXT,
                    status INTEGER NOT NULL,
                    expires INTEGER,
                    country TEXT NOT NULL,
                    verified_at INTEGER NOT NULL
                )".to_string(),
                
                // Create indexes
                "CREATE INDEX idx_users_callsign ON users(callsign)".to_string(),
                "CREATE INDEX idx_users_email ON users(email)".to_string(),
                "CREATE INDEX idx_users_status ON users(status)".to_string(),
                "CREATE INDEX idx_nodes_callsign ON nodes(callsign)".to_string(),
                "CREATE INDEX idx_nodes_status ON nodes(status)".to_string(),
                "CREATE INDEX idx_license_cache_verified_at ON license_cache(verified_at)".to_string(),
            ],
            down_sql: Some(vec![
                "DROP INDEX IF EXISTS idx_license_cache_verified_at".to_string(),
                "DROP INDEX IF EXISTS idx_nodes_status".to_string(),
                "DROP INDEX IF EXISTS idx_nodes_callsign".to_string(),
                "DROP INDEX IF EXISTS idx_users_status".to_string(),
                "DROP INDEX IF EXISTS idx_users_email".to_string(),
                "DROP INDEX IF EXISTS idx_users_callsign".to_string(),
                "DROP TABLE IF EXISTS license_cache".to_string(),
                "DROP TABLE IF EXISTS nodes".to_string(),
                "DROP TABLE IF EXISTS users".to_string(),
            ]),
            reversible: true,
        },
        
        // Migration 2: Messages and groups
        Migration {
            version: 2,
            name: "Messages and groups".to_string(),
            up_sql: vec![
                // Messages table
                "CREATE TABLE messages (
                    id TEXT PRIMARY KEY,
                    message_type INTEGER NOT NULL,
                    sender_id TEXT NOT NULL,
                    sender_callsign TEXT NOT NULL,
                    recipient_id TEXT,
                    recipient_callsign TEXT,
                    group_id TEXT,
                    subject TEXT,
                    body TEXT NOT NULL,
                    priority INTEGER NOT NULL DEFAULT 1,
                    created_at INTEGER NOT NULL,
                    delivered_at INTEGER,
                    read_at INTEGER,
                    parent_id TEXT,
                    thread_id TEXT,
                    status INTEGER NOT NULL DEFAULT 0,
                    origin_node_id TEXT NOT NULL,
                    message_hash TEXT NOT NULL,
                    rf_metadata TEXT,
                    flags TEXT,
                    FOREIGN KEY (sender_id) REFERENCES users(id),
                    FOREIGN KEY (recipient_id) REFERENCES users(id),
                    FOREIGN KEY (parent_id) REFERENCES messages(id),
                    FOREIGN KEY (origin_node_id) REFERENCES nodes(id)
                )".to_string(),
                
                // Groups table
                "CREATE TABLE groups (
                    id TEXT PRIMARY KEY,
                    name TEXT NOT NULL,
                    description TEXT,
                    group_type INTEGER NOT NULL DEFAULT 0,
                    created_by TEXT NOT NULL,
                    created_at INTEGER NOT NULL,
                    status INTEGER NOT NULL DEFAULT 0,
                    settings TEXT,
                    stats TEXT,
                    FOREIGN KEY (created_by) REFERENCES users(id)
                )".to_string(),
                
                // Group memberships table
                "CREATE TABLE group_memberships (
                    group_id TEXT NOT NULL,
                    user_id TEXT NOT NULL,
                    user_callsign TEXT NOT NULL,
                    role INTEGER NOT NULL DEFAULT 0,
                    joined_at INTEGER NOT NULL,
                    last_activity INTEGER,
                    status INTEGER NOT NULL DEFAULT 0,
                    invited_by TEXT,
                    settings TEXT,
                    PRIMARY KEY (group_id, user_id),
                    FOREIGN KEY (group_id) REFERENCES groups(id),
                    FOREIGN KEY (user_id) REFERENCES users(id),
                    FOREIGN KEY (invited_by) REFERENCES users(id)
                )".to_string(),
                
                // Message indexes
                "CREATE INDEX idx_messages_sender ON messages(sender_id)".to_string(),
                "CREATE INDEX idx_messages_recipient ON messages(recipient_id)".to_string(),
                "CREATE INDEX idx_messages_group ON messages(group_id)".to_string(),
                "CREATE INDEX idx_messages_created_at ON messages(created_at)".to_string(),
                "CREATE INDEX idx_messages_status ON messages(status)".to_string(),
                "CREATE INDEX idx_messages_thread ON messages(thread_id)".to_string(),
                "CREATE INDEX idx_messages_hash ON messages(message_hash)".to_string(),
                
                // Group indexes
                "CREATE INDEX idx_groups_name ON groups(name)".to_string(),
                "CREATE INDEX idx_groups_type ON groups(group_type)".to_string(),
                "CREATE INDEX idx_groups_status ON groups(status)".to_string(),
                "CREATE INDEX idx_group_memberships_user ON group_memberships(user_id)".to_string(),
                "CREATE INDEX idx_group_memberships_status ON group_memberships(status)".to_string(),
            ],
            down_sql: Some(vec![
                "DROP INDEX IF EXISTS idx_group_memberships_status".to_string(),
                "DROP INDEX IF EXISTS idx_group_memberships_user".to_string(),
                "DROP INDEX IF EXISTS idx_groups_status".to_string(),
                "DROP INDEX IF EXISTS idx_groups_type".to_string(),
                "DROP INDEX IF EXISTS idx_groups_name".to_string(),
                "DROP INDEX IF EXISTS idx_messages_hash".to_string(),
                "DROP INDEX IF EXISTS idx_messages_thread".to_string(),
                "DROP INDEX IF EXISTS idx_messages_status".to_string(),
                "DROP INDEX IF EXISTS idx_messages_created_at".to_string(),
                "DROP INDEX IF EXISTS idx_messages_group".to_string(),
                "DROP INDEX IF EXISTS idx_messages_recipient".to_string(),
                "DROP INDEX IF EXISTS idx_messages_sender".to_string(),
                "DROP TABLE IF EXISTS group_memberships".to_string(),
                "DROP TABLE IF EXISTS groups".to_string(),
                "DROP TABLE IF EXISTS messages".to_string(),
            ]),
            reversible: true,
        },
        
        // Migration 3: Network nodes and routing
        Migration {
            version: 3,
            name: "Network nodes and routing".to_string(),
            up_sql: vec![
                // Network nodes table (detailed node information)
                "CREATE TABLE network_nodes (
                    id TEXT PRIMARY KEY,
                    callsign TEXT UNIQUE NOT NULL,
                    description TEXT NOT NULL,
                    location TEXT,
                    operator_callsign TEXT,
                    created_at INTEGER NOT NULL,
                    last_seen INTEGER NOT NULL,
                    status INTEGER NOT NULL DEFAULT 0,
                    version TEXT NOT NULL,
                    network_config TEXT,
                    capabilities TEXT,
                    stats TEXT
                )".to_string(),
                
                // Message routes table
                "CREATE TABLE message_routes (
                    message_id TEXT NOT NULL,
                    source_node TEXT NOT NULL,
                    destination_node TEXT NOT NULL,
                    next_hop TEXT NOT NULL,
                    hop_count INTEGER NOT NULL,
                    cost INTEGER NOT NULL,
                    created_at INTEGER NOT NULL,
                    expires_at INTEGER NOT NULL,
                    status INTEGER NOT NULL DEFAULT 0,
                    PRIMARY KEY (message_id, source_node, destination_node),
                    FOREIGN KEY (message_id) REFERENCES messages(id),
                    FOREIGN KEY (source_node) REFERENCES network_nodes(id),
                    FOREIGN KEY (destination_node) REFERENCES network_nodes(id),
                    FOREIGN KEY (next_hop) REFERENCES network_nodes(id)
                )".to_string(),
                
                // Sync state table
                "CREATE TABLE sync_state (
                    entity_type TEXT NOT NULL,
                    entity_id TEXT NOT NULL,
                    last_sync INTEGER NOT NULL,
                    entity_hash TEXT NOT NULL,
                    updated_by TEXT NOT NULL,
                    status INTEGER NOT NULL DEFAULT 0,
                    conflict_data TEXT,
                    PRIMARY KEY (entity_type, entity_id),
                    FOREIGN KEY (updated_by) REFERENCES network_nodes(id)
                )".to_string(),
                
                // Indexes
                "CREATE INDEX idx_network_nodes_callsign ON network_nodes(callsign)".to_string(),
                "CREATE INDEX idx_network_nodes_status ON network_nodes(status)".to_string(),
                "CREATE INDEX idx_network_nodes_last_seen ON network_nodes(last_seen)".to_string(),
                "CREATE INDEX idx_message_routes_expires ON message_routes(expires_at)".to_string(),
                "CREATE INDEX idx_sync_state_last_sync ON sync_state(last_sync)".to_string(),
                "CREATE INDEX idx_sync_state_status ON sync_state(status)".to_string(),
            ],
            down_sql: Some(vec![
                "DROP INDEX IF EXISTS idx_sync_state_status".to_string(),
                "DROP INDEX IF EXISTS idx_sync_state_last_sync".to_string(),
                "DROP INDEX IF EXISTS idx_message_routes_expires".to_string(),
                "DROP INDEX IF EXISTS idx_network_nodes_last_seen".to_string(),
                "DROP INDEX IF EXISTS idx_network_nodes_status".to_string(),
                "DROP INDEX IF EXISTS idx_network_nodes_callsign".to_string(),
                "DROP TABLE IF EXISTS sync_state".to_string(),
                "DROP TABLE IF EXISTS message_routes".to_string(),
                "DROP TABLE IF EXISTS network_nodes".to_string(),
            ]),
            reversible: true,
        },
        
        // Migration 4: Event logging and configuration
        Migration {
            version: 4,
            name: "Event logging and configuration".to_string(),
            up_sql: vec![
                // Event log table
                "CREATE TABLE event_log (
                    id TEXT PRIMARY KEY,
                    event_type INTEGER NOT NULL,
                    user_id TEXT,
                    node_id TEXT NOT NULL,
                    timestamp INTEGER NOT NULL,
                    severity INTEGER NOT NULL,
                    description TEXT NOT NULL,
                    data TEXT,
                    entity_type TEXT,
                    entity_id TEXT,
                    FOREIGN KEY (user_id) REFERENCES users(id),
                    FOREIGN KEY (node_id) REFERENCES nodes(id)
                )".to_string(),
                
                // Configuration settings table
                "CREATE TABLE config_settings (
                    key TEXT PRIMARY KEY,
                    value TEXT NOT NULL,
                    category TEXT NOT NULL,
                    description TEXT,
                    updated_at INTEGER NOT NULL,
                    updated_by TEXT,
                    encrypted BOOLEAN NOT NULL DEFAULT FALSE,
                    FOREIGN KEY (updated_by) REFERENCES users(id)
                )".to_string(),
                
                // Indexes
                "CREATE INDEX idx_event_log_timestamp ON event_log(timestamp)".to_string(),
                "CREATE INDEX idx_event_log_severity ON event_log(severity)".to_string(),
                "CREATE INDEX idx_event_log_type ON event_log(event_type)".to_string(),
                "CREATE INDEX idx_event_log_user ON event_log(user_id)".to_string(),
                "CREATE INDEX idx_config_settings_category ON config_settings(category)".to_string(),
            ],
            down_sql: Some(vec![
                "DROP INDEX IF EXISTS idx_config_settings_category".to_string(),
                "DROP INDEX IF EXISTS idx_event_log_user".to_string(),
                "DROP INDEX IF EXISTS idx_event_log_type".to_string(),
                "DROP INDEX IF EXISTS idx_event_log_severity".to_string(),
                "DROP INDEX IF EXISTS idx_event_log_timestamp".to_string(),
                "DROP TABLE IF EXISTS config_settings".to_string(),
                "DROP TABLE IF EXISTS event_log".to_string(),
            ]),
            reversible: true,
        },
        
        // Migration 5: File attachments
        Migration {
            version: 5,
            name: "File attachments".to_string(),
            up_sql: vec![
                // Attachments table
                "CREATE TABLE attachments (
                    id TEXT PRIMARY KEY,
                    message_id TEXT NOT NULL,
                    filename TEXT NOT NULL,
                    mime_type TEXT NOT NULL,
                    size INTEGER NOT NULL,
                    hash TEXT NOT NULL,
                    storage_path TEXT NOT NULL,
                    created_at INTEGER NOT NULL,
                    uploaded_by TEXT NOT NULL,
                    metadata TEXT,
                    FOREIGN KEY (message_id) REFERENCES messages(id),
                    FOREIGN KEY (uploaded_by) REFERENCES users(id)
                )".to_string(),
                
                // Indexes
                "CREATE INDEX idx_attachments_message ON attachments(message_id)".to_string(),
                "CREATE INDEX idx_attachments_hash ON attachments(hash)".to_string(),
                "CREATE INDEX idx_attachments_uploaded_by ON attachments(uploaded_by)".to_string(),
            ],
            down_sql: Some(vec![
                "DROP INDEX IF EXISTS idx_attachments_uploaded_by".to_string(),
                "DROP INDEX IF EXISTS idx_attachments_hash".to_string(),
                "DROP INDEX IF EXISTS idx_attachments_message".to_string(),
                "DROP TABLE IF EXISTS attachments".to_string(),
            ]),
            reversible: true,
        },
        
        // Migration 6: Performance optimizations
        Migration {
            version: 6,
            name: "Performance optimizations".to_string(),
            up_sql: vec![
                // Additional indexes for common queries
                "CREATE INDEX idx_messages_sender_created ON messages(sender_id, created_at)".to_string(),
                "CREATE INDEX idx_messages_recipient_status ON messages(recipient_id, status)".to_string(),
                "CREATE INDEX idx_users_status_role ON users(status, role)".to_string(),
                "CREATE INDEX idx_groups_status_type ON groups(status, group_type)".to_string(),
                
                // Optimize foreign key constraints
                "PRAGMA foreign_keys = ON".to_string(),
                
                // Update table statistics
                "ANALYZE".to_string(),
            ],
            down_sql: Some(vec![
                "DROP INDEX IF EXISTS idx_groups_status_type".to_string(),
                "DROP INDEX IF EXISTS idx_users_status_role".to_string(),
                "DROP INDEX IF EXISTS idx_messages_recipient_status".to_string(),
                "DROP INDEX IF EXISTS idx_messages_sender_created".to_string(),
            ]),
            reversible: true,
        },
    ]
}

/// Check if a specific migration has been applied
pub async fn is_migration_applied(pool: &Pool<Sqlite>, version: i32) -> Result<bool> {
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM schema_migrations WHERE version = ?1"
    )
    .bind(version)
    .fetch_one(pool)
    .await?;
    
    Ok(count > 0)
}

/// Get list of applied migrations
pub async fn get_applied_migrations(pool: &Pool<Sqlite>) -> Result<Vec<(i32, String, i64)>> {
    let rows = sqlx::query(
        "SELECT version, name, applied_at FROM schema_migrations ORDER BY version"
    )
    .fetch_all(pool)
    .await?;
    
    let mut migrations = Vec::new();
    for row in rows {
        migrations.push((
            row.get("version"),
            row.get("name"),
            row.get("applied_at"),
        ));
    }
    
    Ok(migrations)
}

/// Validate database schema integrity
pub async fn validate_schema(pool: &Pool<Sqlite>) -> Result<bool> {
    // Check that all expected tables exist
    let expected_tables = vec![
        "users", "nodes", "license_cache", "messages", "groups", 
        "group_memberships", "network_nodes", "message_routes", 
        "sync_state", "event_log", "config_settings", "attachments",
        "schema_migrations", "schema_metadata"
    ];
    
    for table in expected_tables {
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name=?1"
        )
        .bind(table)
        .fetch_one(pool)
        .await?;
        
        if count == 0 {
            warn!("Missing expected table: {}", table);
            return Ok(false);
        }
    }
    
    // Run integrity check
    let integrity: String = sqlx::query_scalar("PRAGMA integrity_check")
        .fetch_one(pool)
        .await?;
    
    if integrity != "ok" {
        warn!("Database integrity check failed: {}", integrity);
        return Ok(false);
    }
    
    info!("Database schema validation passed");
    Ok(true)
}

/// Reset database (drop all tables and start fresh)
pub async fn reset_database(pool: &Pool<Sqlite>) -> Result<()> {
    warn!("Resetting database - all data will be lost!");
    
    // Get all table names
    let rows = sqlx::query(
        "SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%'"
    )
    .fetch_all(pool)
    .await?;
    
    // Drop all tables
    for row in rows {
        let table_name: String = row.get("name");
        let sql = format!("DROP TABLE IF EXISTS {}", table_name);
        sqlx::query(&sql).execute(pool).await?;
    }
    
    // Run migrations to recreate schema
    run_migrations(pool).await?;
    
    info!("Database reset completed");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::SqlitePool;
    use tempfile::tempdir;
    
    async fn create_test_pool() -> SqlitePool {
        SqlitePool::connect(":memory:").await.unwrap()
    }
    
    #[tokio::test]
    async fn test_migration_table_creation() {
        let pool = create_test_pool().await;
        
        ensure_migration_table(&pool).await.unwrap();
        
        // Check that tables were created
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='schema_migrations'"
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        
        assert_eq!(count, 1);
    }
    
    #[tokio::test]
    async fn test_run_migrations() {
        let pool = create_test_pool().await;
        
        let result = run_migrations(&pool).await.unwrap();
        
        assert!(!result.applied.is_empty());
        assert!(result.current_version > 0);
        
        // Check that all expected tables exist
        let table_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table'"
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        
        assert!(table_count >= 10); // Should have many tables
    }
    
    #[tokio::test]
    async fn test_schema_validation() {
        let pool = create_test_pool().await;
        
        // Should fail before migrations
        let valid = validate_schema(&pool).await.unwrap();
        assert!(!valid);
        
        // Run migrations
        run_migrations(&pool).await.unwrap();
        
        // Should pass after migrations
        let valid = validate_schema(&pool).await.unwrap();
        assert!(valid);
    }
    
    #[tokio::test]
    async fn test_migration_tracking() {
        let pool = create_test_pool().await;
        
        run_migrations(&pool).await.unwrap();
        
        let applied = get_applied_migrations(&pool).await.unwrap();
        assert!(!applied.is_empty());
        
        // Check specific migration
        let is_applied = is_migration_applied(&pool, 1).await.unwrap();
        assert!(is_applied);
        
        let not_applied = is_migration_applied(&pool, 999).await.unwrap();
        assert!(!not_applied);
    }
    
    #[tokio::test]
    async fn test_current_version() {
        let pool = create_test_pool().await;
        
        // Should be 0 initially
        let version = get_current_version(&pool).await.unwrap();
        assert_eq!(version, 0);
        
        // Run migrations
        run_migrations(&pool).await.unwrap();
        
        // Should be latest version
        let version = get_current_version(&pool).await.unwrap();
        assert!(version > 0);
    }
    
    #[tokio::test]
    async fn test_idempotent_migrations() {
        let pool = create_test_pool().await;
        
        // Run migrations twice
        let result1 = run_migrations(&pool).await.unwrap();
        let result2 = run_migrations(&pool).await.unwrap();
        
        // Second run should apply no migrations
        assert!(result2.applied.is_empty());
        assert_eq!(result1.current_version, result2.current_version);
    }
}
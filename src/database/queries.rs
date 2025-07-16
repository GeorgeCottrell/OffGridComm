/// Get message thread
    pub async fn get_message_thread(&self, thread_id: &Uuid) -> Result<Vec<Message>> {
        let rows = sqlx::query!(
            "SELECT id, message_type, sender_id, sender_callsign, recipient_id, 
             recipient_callsign, group_id, subject, body, priority, created_at, 
             delivered_at, read_at, parent_id, thread_id, status, origin_node_id, 
             message_hash, rf_metadata, flags
             FROM messages 
             WHERE thread_id = ?1 AND status != 5
             ORDER BY created_at ASC",
            thread_id.to_string()
        )
        .fetch_all(&self.pool)
        .await?;

        let mut messages = Vec::new();
        for row in rows {
            let rf_metadata = row.rf_metadata.and_then(|rf| 
                serde_json::from_str(&rf).ok()
            );
            let flags: MessageFlags = row.flags.as_ref()
                .and_then(|f| serde_json::from_str(f).ok())
                .unwrap_or_default();

            let message = Message {
                id: Uuid::parse_str(&row.id)?,
                message_type: match row.message_type {
                    0 => MessageType::Direct,
                    1 => MessageType::Group,
                    2 => MessageType::System,
                    3 => MessageType::Emergency,
                    4 => MessageType::Bulletin,
                    _ => MessageType::Direct,
                },
                sender_id: Uuid::parse_str(&row.sender_id)?,
                sender_callsign: row.sender_callsign,
                recipient_id: row.recipient_id.as_ref().and_then(|id| Uuid::parse_str(id).ok()),
                recipient_callsign: row.recipient_callsign,
                group_id: row.group_id.as_ref().and_then(|id| Uuid::parse_str(id).ok()),
                subject: row.subject,
                body: row.body,
                priority: match row.priority {
                    0 => MessagePriority::Low,
                    1 => MessagePriority::Normal,
                    2 => MessagePriority::High,
                    3 => MessagePriority::Emergency,
                    _ => MessagePriority::Normal,
                },
                created_at: UNIX_EPOCH + Duration::from_secs(row.created_at as u64),
                delivered_at: row.delivered_at
                    .map(|t| UNIX_EPOCH + Duration::from_secs(t as u64)),
                read_at: row.read_at
                    .map(|t| UNIX_EPOCH + Duration::from_secs(t as u64)),
                parent_id: row.parent_id.as_ref().and_then(|id| Uuid::parse_str(id).ok()),
                thread_id: row.thread_id.as_ref().and_then(|id| Uuid::parse_str(id).ok()),
                status: match row.status {
                    0 => MessageStatus::Pending,
                    1 => MessageStatus::Sent,
                    2 => MessageStatus::Delivered,
                    3 => MessageStatus::Read,
                    4 => MessageStatus::Failed,
                    5 => MessageStatus::Deleted,
                    _ => MessageStatus::Pending,
                },
                origin_node_id: Uuid::parse_str(&row.origin_node_id)?,
                message_hash: row.message_hash,
                rf_metadata,
                flags,
            };
            messages.push(message);
        }

        Ok(messages)
    }

    /// Check for duplicate messages by hash
    pub async fn message_exists_by_hash(&self, message_hash: &str) -> Result<bool> {
        let row = sqlx::query!(
            "SELECT COUNT(*) as count FROM messages WHERE message_hash = ?1",
            message_hash
        )
        .fetch_one(&self.pool)
        .await?;

        Ok(row.count > 0)
    }

    /// Delete old messages (cleanup)
    pub async fn delete_old_messages(&self, older_than_days: u32) -> Result<u64> {
        let cutoff_timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)?
            .as_secs() as i64 - (older_than_days as i64 * 24 * 3600);

        let result = sqlx::query!(
            "DELETE FROM messages WHERE created_at < ?1 AND status != 3", // Don't delete unread messages
            cutoff_timestamp
        )
        .execute(&self.pool)
        .await?;

        Ok(result.rows_affected())
    }
}

// =============================================================================
// GROUP QUERIES
// =============================================================================

impl Queries {
    /// Create a new group
    pub async fn create_group(&self, group: &Group) -> Result<()> {
        let created_at = group.created_at.duration_since(UNIX_EPOCH)?.as_secs() as i64;
        let settings_json = serde_json::to_string(&group.settings)?;
        let stats_json = serde_json::to_string(&group.stats)?;

        sqlx::query!(
            "INSERT INTO groups (id, name, description, group_type, created_by, 
             created_at, status, settings, stats)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            group.id.to_string(),
            group.name,
            group.description,
            group.group_type as i32,
            group.created_by.to_string(),
            created_at,
            group.status as i32,
            settings_json,
            stats_json
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Get group by ID
    pub async fn get_group_by_id(&self, group_id: &Uuid) -> Result<Option<Group>> {
        let row = sqlx::query!(
            "SELECT id, name, description, group_type, created_by, created_at, 
             status, settings, stats
             FROM groups WHERE id = ?1",
            group_id.to_string()
        )
        .fetch_optional(&self.pool)
        .await?;

        if let Some(row) = row {
            let settings: GroupSettings = row.settings.as_ref()
                .and_then(|s| serde_json::from_str(s).ok())
                .unwrap_or_default();
            let stats: GroupStats = row.stats.as_ref()
                .and_then(|s| serde_json::from_str(s).ok())
                .unwrap_or_default();

            Ok(Some(Group {
                id: Uuid::parse_str(&row.id)?,
                name: row.name,
                description: row.description,
                group_type: match row.group_type {
                    0 => GroupType::Public,
                    1 => GroupType::Private,
                    2 => GroupType::Announcement,
                    3 => GroupType::Emergency,
                    4 => GroupType::Local,
                    _ => GroupType::Public,
                },
                created_by: Uuid::parse_str(&row.created_by)?,
                created_at: UNIX_EPOCH + Duration::from_secs(row.created_at as u64),
                status: match row.status {
                    0 => GroupStatus::Active,
                    1 => GroupStatus::Archived,
                    2 => GroupStatus::Suspended,
                    3 => GroupStatus::Deleted,
                    _ => GroupStatus::Active,
                },
                settings,
                stats,
            }))
        } else {
            Ok(None)
        }
    }

    /// List groups with filters
    pub async fn list_groups(
        &self,
        group_type: Option<GroupType>,
        status: Option<GroupStatus>,
        limit: u32,
        offset: u32,
    ) -> Result<Vec<Group>> {
        let mut query = "SELECT id, name, description, group_type, created_by, created_at, 
                         status, settings, stats
                         FROM groups WHERE 1=1".to_string();
        
        let mut params = Vec::new();
        
        if let Some(gtype) = group_type {
            query.push_str(" AND group_type = ?");
            params.push(gtype as i32);
        }
        
        if let Some(gstatus) = status {
            query.push_str(" AND status = ?");
            params.push(gstatus as i32);
        }
        
        query.push_str(" ORDER BY created_at DESC LIMIT ? OFFSET ?");
        params.push(limit as i32);
        params.push(offset as i32);

        let mut query_builder = sqlx::query(&query);
        for param in params {
            query_builder = query_builder.bind(param);
        }

        let rows = query_builder.fetch_all(&self.pool).await?;
        
        let mut groups = Vec::new();
        for row in rows {
            let settings: GroupSettings = row.get::<Option<String>, _>("settings")
                .and_then(|s| serde_json::from_str(&s).ok())
                .unwrap_or_default();
            let stats: GroupStats = row.get::<Option<String>, _>("stats")
                .and_then(|s| serde_json::from_str(&s).ok())
                .unwrap_or_default();

            let group = Group {
                id: Uuid::parse_str(&row.get::<String, _>("id"))?,
                name: row.get("name"),
                description: row.get("description"),
                group_type: match row.get::<i32, _>("group_type") {
                    0 => GroupType::Public,
                    1 => GroupType::Private,
                    2 => GroupType::Announcement,
                    3 => GroupType::Emergency,
                    4 => GroupType::Local,
                    _ => GroupType::Public,
                },
                created_by: Uuid::parse_str(&row.get::<String, _>("created_by"))?,
                created_at: UNIX_EPOCH + Duration::from_secs(row.get::<i64, _>("created_at") as u64),
                status: match row.get::<i32, _>("status") {
                    0 => GroupStatus::Active,
                    1 => GroupStatus::Archived,
                    2 => GroupStatus::Suspended,
                    3 => GroupStatus::Deleted,
                    _ => GroupStatus::Active,
                },
                settings,
                stats,
            };
            groups.push(group);
        }

        Ok(groups)
    }

    /// Add user to group
    pub async fn add_group_member(&self, membership: &GroupMembership) -> Result<()> {
        let joined_at = membership.joined_at.duration_since(UNIX_EPOCH)?.as_secs() as i64;
        let last_activity = membership.last_activity
            .map(|t| t.duration_since(UNIX_EPOCH).map(|d| d.as_secs() as i64))
            .transpose()?;
        let settings_json = serde_json::to_string(&membership.settings)?;

        sqlx::query!(
            "INSERT INTO group_memberships (group_id, user_id, user_callsign, role, 
             joined_at, last_activity, status, invited_by, settings)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            membership.group_id.to_string(),
            membership.user_id.to_string(),
            membership.user_callsign,
            membership.role as i32,
            joined_at,
            last_activity,
            membership.status as i32,
            membership.invited_by.map(|id| id.to_string()),
            settings_json
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Get group membership
    pub async fn get_group_membership(
        &self,
        group_id: &Uuid,
        user_id: &Uuid,
    ) -> Result<Option<GroupMembership>> {
        let row = sqlx::query!(
            "SELECT group_id, user_id, user_callsign, role, joined_at, last_activity, 
             status, invited_by, settings
             FROM group_memberships WHERE group_id = ?1 AND user_id = ?2",
            group_id.to_string(),
            user_id.to_string()
        )
        .fetch_optional(&self.pool)
        .await?;

        if let Some(row) = row {
            let settings: MemberSettings = row.settings.as_ref()
                .and_then(|s| serde_json::from_str(s).ok())
                .unwrap_or_default();

            Ok(Some(GroupMembership {
                group_id: Uuid::parse_str(&row.group_id)?,
                user_id: Uuid::parse_str(&row.user_id)?,
                user_callsign: row.user_callsign,
                role: match row.role {
                    0 => GroupRole::Member,
                    1 => GroupRole::Moderator,
                    2 => GroupRole::Administrator,
                    3 => GroupRole::Owner,
                    _ => GroupRole::Member,
                },
                joined_at: UNIX_EPOCH + Duration::from_secs(row.joined_at as u64),
                last_activity: row.last_activity
                    .map(|t| UNIX_EPOCH + Duration::from_secs(t as u64)),
                status: match row.status {
                    0 => MembershipStatus::Active,
                    1 => MembershipStatus::Pending,
                    2 => MembershipStatus::Banned,
                    3 => MembershipStatus::Left,
                    _ => MembershipStatus::Active,
                },
                invited_by: row.invited_by.as_ref().and_then(|id| Uuid::parse_str(id).ok()),
                settings,
            }))
        } else {
            Ok(None)
        }
    }

    /// List group members
    pub async fn list_group_members(&self, group_id: &Uuid) -> Result<Vec<GroupMembership>> {
        let rows = sqlx::query!(
            "SELECT group_id, user_id, user_callsign, role, joined_at, last_activity, 
             status, invited_by, settings
             FROM group_memberships 
             WHERE group_id = ?1 AND status = 0
             ORDER BY joined_at ASC",
            group_id.to_string()
        )
        .fetch_all(&self.pool)
        .await?;

        let mut members = Vec::new();
        for row in rows {
            let settings: MemberSettings = row.settings.as_ref()
                .and_then(|s| serde_json::from_str(s).ok())
                .unwrap_or_default();

            let membership = GroupMembership {
                group_id: Uuid::parse_str(&row.group_id)?,
                user_id: Uuid::parse_str(&row.user_id)?,
                user_callsign: row.user_callsign,
                role: match row.role {
                    0 => GroupRole::Member,
                    1 => GroupRole::Moderator,
                    2 => GroupRole::Administrator,
                    3 => GroupRole::Owner,
                    _ => GroupRole::Member,
                },
                joined_at: UNIX_EPOCH + Duration::from_secs(row.joined_at as u64),
                last_activity: row.last_activity
                    .map(|t| UNIX_EPOCH + Duration::from_secs(t as u64)),
                status: match row.status {
                    0 => MembershipStatus::Active,
                    1 => MembershipStatus::Pending,
                    2 => MembershipStatus::Banned,
                    3 => MembershipStatus::Left,
                    _ => MembershipStatus::Active,
                },
                invited_by: row.invited_by.as_ref().and_then(|id| Uuid::parse_str(id).ok()),
                settings,
            };
            members.push(membership);
        }

        Ok(members)
    }

    /// Get user's group memberships
    pub async fn get_user_groups(&self, user_id: &Uuid) -> Result<Vec<GroupMembership>> {
        let rows = sqlx::query!(
            "SELECT group_id, user_id, user_callsign, role, joined_at, last_activity, 
             status, invited_by, settings
             FROM group_memberships 
             WHERE user_id = ?1 AND status = 0
             ORDER BY joined_at DESC",
            user_id.to_string()
        )
        .fetch_all(&self.pool)
        .await?;

        let mut memberships = Vec::new();
        for row in rows {
            let settings: MemberSettings = row.settings.as_ref()
                .and_then(|s| serde_json::from_str(s).ok())
                .unwrap_or_default();

            let membership = GroupMembership {
                group_id: Uuid::parse_str(&row.group_id)?,
                user_id: Uuid::parse_str(&row.user_id)?,
                user_callsign: row.user_callsign,
                role: match row.role {
                    0 => GroupRole::Member,
                    1 => GroupRole::Moderator,
                    2 => GroupRole::Administrator,
                    3 => GroupRole::Owner,
                    _ => GroupRole::Member,
                },
                joined_at: UNIX_EPOCH + Duration::from_secs(row.joined_at as u64),
                last_activity: row.last_activity
                    .map(|t| UNIX_EPOCH + Duration::from_secs(t as u64)),
                status: match row.status {
                    0 => MembershipStatus::Active,
                    1 => MembershipStatus::Pending,
                    2 => MembershipStatus::Banned,
                    3 => MembershipStatus::Left,
                    _ => MembershipStatus::Active,
                },
                invited_by: row.invited_by.as_ref().and_then(|id| Uuid::parse_str(id).ok()),
                settings,
            };
            memberships.push(membership);
        }

        Ok(memberships)
    }

    /// Update group statistics
    pub async fn update_group_stats(&self, group_id: &Uuid, stats: &GroupStats) -> Result<()> {
        let stats_json = serde_json::to_string(stats)?;
        
        sqlx::query!(
            "UPDATE groups SET stats = ?1 WHERE id = ?2",
            stats_json,
            group_id.to_string()
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }
}

// =============================================================================
// LICENSE CACHE QUERIES
// =============================================================================

impl Queries {
    /// Cache license information
    pub async fn cache_license_info(&self, license_info: &LicenseInfo) -> Result<()> {
        let verified_at = license_info.verified_at.duration_since(UNIX_EPOCH)?.as_secs() as i64;
        let expires = license_info.expires
            .map(|t| t.duration_since(UNIX_EPOCH).map(|d| d.as_secs() as i64))
            .transpose()?;

        sqlx::query!(
            "INSERT OR REPLACE INTO license_cache 
             (callsign, name, license_class, status, expires, country, verified_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            license_info.callsign,
            license_info.name,
            license_info.license_class,
            license_info.status as i32,
            expires,
            license_info.country,
            verified_at
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Get cached license information
    pub async fn get_cached_license_info(&self, callsign: &str) -> Result<Option<LicenseInfo>> {
        let callsign = callsign.to_uppercase();
        
        let row = sqlx::query!(
            "SELECT callsign, name, license_class, status, expires, country, verified_at
             FROM license_cache WHERE UPPER(callsign) = ?1",
            callsign
        )
        .fetch_optional(&self.pool)
        .await?;

        if let Some(row) = row {
            Ok(Some(LicenseInfo {
                callsign: row.callsign,
                name: row.name,
                license_class: row.license_class,
                status: match row.status {
                    0 => LicenseStatus::Active,
                    1 => LicenseStatus::Expired,
                    2 => LicenseStatus::Canceled,
                    3 => LicenseStatus::Suspended,
                    _ => LicenseStatus::Unknown,
                },
                expires: row.expires
                    .map(|t| UNIX_EPOCH + Duration::from_secs(t as u64)),
                address: None, // Not stored in cache
                country: row.country,
                verified_at: UNIX_EPOCH + Duration::from_secs(row.verified_at as u64),
            }))
        } else {
            Ok(None)
        }
    }

    /// Clean up old license cache entries
    pub async fn cleanup_license_cache(&self, older_than_days: u32) -> Result<u64> {
        let cutoff_timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)?
            .as_secs() as i64 - (older_than_days as i64 * 24 * 3600);

        let result = sqlx::query!(
            "DELETE FROM license_cache WHERE verified_at < ?1",
            cutoff_timestamp
        )
        .execute(&self.pool)
        .await?;

        Ok(result.rows_affected())
    }
}

// =============================================================================
// NETWORK NODE QUERIES
// =============================================================================

impl Queries {
    /// Create or update network node
    pub async fn upsert_network_node(&self, node: &NetworkNode) -> Result<()> {
        let created_at = node.created_at.duration_since(UNIX_EPOCH)?.as_secs() as i64;
        let last_seen = node.last_seen.duration_since(UNIX_EPOCH)?.as_secs() as i64;
        let network_config_json = serde_json::to_string(&node.network_config)?;
        let capabilities_json = serde_json::to_string(&node.capabilities)?;
        let stats_json = serde_json::to_string(&node.stats)?;

        sqlx::query!(
            "INSERT OR REPLACE INTO network_nodes 
             (id, callsign, description, location, operator_callsign, created_at, 
              last_seen, status, version, network_config, capabilities, stats)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
            node.id.to_string(),
            node.callsign,
            node.description,
            node.location,
            node.operator_callsign,
            created_at,
            last_seen,
            node.status as i32,
            node.version,
            network_config_json,
            capabilities_json,
            stats_json
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Get network node by ID
    pub async fn get_network_node_by_id(&self, node_id: &Uuid) -> Result<Option<NetworkNode>> {
        let row = sqlx::query!(
            "SELECT id, callsign, description, location, operator_callsign, created_at, 
             last_seen, status, version, network_config, capabilities, stats
             FROM network_nodes WHERE id = ?1",
            node_id.to_string()
        )
        .fetch_optional(&self.pool)
        .await?;

        if let Some(row) = row {
            let network_config: NetworkConfig = row.network_config.as_ref()
                .and_then(|s| serde_json::from_str(s).ok())
                .unwrap_or_default();
            let capabilities: NodeCapabilities = row.capabilities.as_ref()
                .and_then(|s| serde_json::from_str(s).ok())
                .unwrap_or_default();
            let stats: NodeStats = row.stats.as_ref()
                .and_then(|s| serde_json::from_str(s).ok())
                .unwrap_or_default();

            Ok(Some(NetworkNode {
                id: Uuid::parse_str(&row.id)?,
                callsign: row.callsign,
                description: row.description,
                location: row.location,
                operator_callsign: row.operator_callsign,
                created_at: UNIX_EPOCH + Duration::from_secs(row.created_at as u64),
                last_seen: UNIX_EPOCH + Duration::from_secs(row.last_seen as u64),
                status: match row.status {
                    1 => NodeStatus::Online,
                    0 => NodeStatus::Offline,
                    2 => NodeStatus::Maintenance,
                    3 => NodeStatus::Unreachable,
                    _ => NodeStatus::Unknown,
                },
                version: row.version,
                network_config,
                capabilities,
                stats,
            }))
        } else {
            Ok(None)
        }
    }

    /// List active network nodes
    pub async fn list_active_nodes(&self) -> Result<Vec<NetworkNode>> {
        let rows = sqlx::query!(
            "SELECT id, callsign, description, location, operator_callsign, created_at, 
             last_seen, status, version, network_config, capabilities, stats
             FROM network_nodes 
             WHERE status IN (1, 2) 
             ORDER BY last_seen DESC"
        )
        .fetch_all(&self.pool)
        .await?;

        let mut nodes = Vec::new();
        for row in rows {
            let network_config: NetworkConfig = row.network_config.as_ref()
                .and_then(|s| serde_json::from_str(s).ok())
                .unwrap_or_default();
            let capabilities: NodeCapabilities = row.capabilities.as_ref()
                .and_then(|s| serde_json::from_str(s).ok())
                .unwrap_or_default();
            let stats: NodeStats = row.stats.as_ref()
                .and_then(|s| serde_json::from_str(s).ok())
                .unwrap_or_default();

            let node = NetworkNode {
                id: Uuid::parse_str(&row.id)?,
                callsign: row.callsign,
                description: row.description,
                location: row.location,
                operator_callsign: row.operator_callsign,
                created_at: UNIX_EPOCH + Duration::from_secs(row.created_at as u64),
                last_seen: UNIX_EPOCH + Duration::from_secs(row.last_seen as u64),
                status: match row.status {
                    1 => NodeStatus::Online,
                    0 => NodeStatus::Offline,
                    2 => NodeStatus::Maintenance,
                    3 => NodeStatus::Unreachable,
                    _ => NodeStatus::Unknown,
                },
                version: row.version,
                network_config,
                capabilities,
                stats,
            };
            nodes.push(node);
        }

        Ok(nodes)
    }

    /// Update node last seen timestamp
    pub async fn update_node_last_seen(&self, node_id: &Uuid) -> Result<()> {
        let timestamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs() as i64;
        
        sqlx::query!(
            "UPDATE network_nodes SET last_seen = ?1 WHERE id = ?2",
            timestamp,
            node_id.to_string()
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Mark nodes as offline if not seen recently
    pub async fn mark_stale_nodes_offline(&self, timeout_minutes: u32) -> Result<u64> {
        let cutoff_timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)?
            .as_secs() as i64 - (timeout_minutes as i64 * 60);

        let result = sqlx::query!(
            "UPDATE network_nodes 
             SET status = 0 
             WHERE last_seen < ?1 AND status = 1",
            cutoff_timestamp
        )
        .execute(&self.pool)
        .await?;

        Ok(result.rows_affected())
    }
}

// =============================================================================
// STATISTICS AND REPORTING QUERIES
// =============================================================================

impl Queries {
    /// Get comprehensive database statistics
    pub async fn get_database_stats(&self) -> Result<DatabaseStats> {
        let user_count = sqlx::query_scalar!("SELECT COUNT(*) FROM users")
            .fetch_one(&self.pool)
            .await?;

        let message_count = sqlx::query_scalar!("SELECT COUNT(*) FROM messages")
            .fetch_one(&self.pool)
            .await?
            .unwrap_or(0);

        let group_count = sqlx::query_scalar!("SELECT COUNT(*) FROM groups")
            .fetch_one(&self.pool)
            .await?
            .unwrap_or(0);

        let node_count = sqlx::query_scalar!("SELECT COUNT(*) FROM nodes")
            .fetch_one(&self.pool)
            .await?;

        let license_cache_count = sqlx::query_scalar!("SELECT COUNT(*) FROM license_cache")
            .fetch_one(&self.pool)
            .await?;

        Ok(crate::database::DatabaseStats {
            total_size_bytes: 0, // Would need file system check
            user_count: user_count as u64,
            message_count: message_count as u64,
            group_count: group_count as u64,
            node_count: node_count as u64,
            license_cache_count: license_cache_count as u64,
            last_backup: None,
            last_vacuum: None,
        })
    }

    /// Get message statistics for a user
    pub async fn get_user_message_stats(&self, user_id: &Uuid) -> Result<UserMessageStats> {
        let sent_count = sqlx::query_scalar!(
            "SELECT COUNT(*) FROM messages WHERE sender_id = ?1",
            user_id.to_string()
        )
        .fetch_one(&self.pool)
        .await?;

        let received_count = sqlx::query_scalar!(
            "SELECT COUNT(*) FROM messages WHERE recipient_id = ?1",
            user_id.to_string()
        )
        .fetch_one(&self.pool)
        .await?;

        let unread_count = sqlx::query_scalar!(
            "SELECT COUNT(*) FROM messages WHERE recipient_id = ?1 AND read_at IS NULL",
            user_id.to_string()
        )
        .fetch_one(&self.pool)
        .await?;

        Ok(UserMessageStats {
            sent_count: sent_count as u32,
            received_count: received_count as u32,
            unread_count: unread_count as u32,
        })
    }

    /// Get recent activity summary
    pub async fn get_recent_activity(&self, hours: u32) -> Result<ActivityStats> {
        let cutoff_timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)?
            .as_secs() as i64 - (hours as i64 * 3600);

        let new_messages = sqlx::query_scalar!(
            "SELECT COUNT(*) FROM messages WHERE created_at > ?1",
            cutoff_timestamp
        )
        .fetch_one(&self.pool)
        .await?;

        let new_users = sqlx::query_scalar!(
            "SELECT COUNT(*) FROM users WHERE created_at > ?1",
            cutoff_timestamp
        )
        .fetch_one(&self.pool)
        .await?;

        let active_users = sqlx::query_scalar!(
            "SELECT COUNT(DISTINCT sender_id) FROM messages WHERE created_at > ?1",
            cutoff_timestamp
        )
        .fetch_one(&self.pool)
        .await?;

        Ok(ActivityStats {
            new_messages: new_messages as u32,
            new_users: new_users as u32,
            active_users: active_users as u32,
            timeframe_hours: hours,
        })
    }

    /// Get top active users by message count
    pub async fn get_top_active_users(&self, limit: u32, days: u32) -> Result<Vec<UserActivity>> {
        let cutoff_timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)?
            .as_secs() as i64 - (days as i64 * 24 * 3600);

        let rows = sqlx::query!(
            "SELECT u.id, u.callsign, u.full_name, COUNT(m.id) as message_count
             FROM users u
             LEFT JOIN messages m ON u.id = m.sender_id AND m.created_at > ?1
             GROUP BY u.id, u.callsign, u.full_name
             ORDER BY message_count DESC
             LIMIT ?2",
            cutoff_timestamp,
            limit as i64
        )
        .fetch_all(&self.pool)
        .await?;

        let mut activities = Vec::new();
        for row in rows {
            activities.push(UserActivity {
                user_id: Uuid::parse_str(&row.id)?,
                callsign: row.callsign,
                full_name: row.full_name,
                message_count: row.message_count as u32,
            });
        }

        Ok(activities)
    }

    /// Get emergency messages (high priority)
    pub async fn get_emergency_messages(&self, limit: u32) -> Result<Vec<Message>> {
        let rows = sqlx::query!(
            "SELECT id, message_type, sender_id, sender_callsign, recipient_id, 
             recipient_callsign, group_id, subject, body, priority, created_at, 
             delivered_at, read_at, parent_id, thread_id, status, origin_node_id, 
             message_hash, rf_metadata, flags
             FROM messages 
             WHERE (priority = 3 OR message_type = 3) AND status != 5
             ORDER BY created_at DESC 
             LIMIT ?1",
            limit as i64
        )
        .fetch_all(&self.pool)
        .await?;

        let mut messages = Vec::new();
        for row in rows {
            let rf_metadata = row.rf_metadata.and_then(|rf| 
                serde_json::from_str(&rf).ok()
            );
            let flags: MessageFlags = row.flags.as_ref()
                .and_then(|f| serde_json::from_str(f).ok())
                .unwrap_or_default();

            let message = Message {
                id: Uuid::parse_str(&row.id)?,
                message_type: match row.message_type {
                    0 => MessageType::Direct,
                    1 => MessageType::Group,
                    2 => MessageType::System,
                    3 => MessageType::Emergency,
                    4 => MessageType::Bulletin,
                    _ => MessageType::Emergency,
                },
                sender_id: Uuid::parse_str(&row.sender_id)?,
                sender_callsign: row.sender_callsign,
                recipient_id: row.recipient_id.as_ref().and_then(|id| Uuid::parse_str(id).ok()),
                recipient_callsign: row.recipient_callsign,
                group_id: row.group_id.as_ref().and_then(|id| Uuid::parse_str(id).ok()),
                subject: row.subject,
                body: row.body,
                priority: match row.priority {
                    0 => MessagePriority::Low,
                    1 => MessagePriority::Normal,
                    2 => MessagePriority::High,
                    3 => MessagePriority::Emergency,
                    _ => MessagePriority::Emergency,
                },
                created_at: UNIX_EPOCH + Duration::from_secs(row.created_at as u64),
                delivered_at: row.delivered_at
                    .map(|t| UNIX_EPOCH + Duration::from_secs(t as u64)),
                read_at: row.read_at
                    .map(|t| UNIX_EPOCH + Duration::from_secs(t as u64)),
                parent_id: row.parent_id.as_ref().and_then(|id| Uuid::parse_str(id).ok()),
                thread_id: row.thread_id.as_ref().and_then(|id| Uuid::parse_str(id).ok()),
                status: match row.status {
                    0 => MessageStatus::Pending,
                    1 => MessageStatus::Sent,
                    2 => MessageStatus::Delivered,
                    3 => MessageStatus::Read,
                    4 => MessageStatus::Failed,
                    5 => MessageStatus::Deleted,
                    _ => MessageStatus::Pending,
                },
                origin_node_id: Uuid::parse_str(&row.origin_node_id)?,
                message_hash: row.message_hash,
                rf_metadata,
                flags,
            };
            messages.push(message);
        }

        Ok(messages)
    }
}

// =============================================================================
// UTILITY STRUCTS FOR STATISTICS
// =============================================================================

/// User message statistics
#[derive(Debug, Clone)]
pub struct UserMessageStats {
    pub sent_count: u32,
    pub received_count: u32,
    pub unread_count: u32,
}

/// Recent activity statistics
#[derive(Debug, Clone)]
pub struct ActivityStats {
    pub new_messages: u32,
    pub new_users: u32,
    pub active_users: u32,
    pub timeframe_hours: u32,
}

/// User activity information
#[derive(Debug, Clone)]
pub struct UserActivity {
    pub user_id: Uuid,
    pub callsign: String,
    pub full_name: Option<String>,
    pub message_count: u32,
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::SqlitePool;
    use tempfile::tempdir;
    use crate::database::migrations::run_migrations;
    use crate::auth::{User, UserRole, UserStatus};

    async fn create_test_queries() -> Queries {
        let pool = SqlitePool::connect(":memory:").await.unwrap();
        run_migrations(&pool).await.unwrap();
        Queries::new(pool)
    }

    #[tokio::test]
    async fn test_user_queries() {
        let queries = create_test_queries().await;

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
            license_class: Some(LicenseClass::General),
            license_verified: true,
        };

        // Create user
        queries.create_user(&user, "test_hash").await.unwrap();

        // Get user by ID
        let retrieved = queries.get_user_by_id(&user.id).await.unwrap().unwrap();
        assert_eq!(retrieved.callsign, user.callsign);

        // Get user by callsign
        let retrieved = queries.get_user_by_callsign("W1TEST").await.unwrap().unwrap();
        assert_eq!(retrieved.id, user.id);

        // Search users
        let results = queries.search_users_by_callsign("W1", 10).await.unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].callsign, "W1TEST");

        // Update last login
        queries.update_last_login(&user.id).await.unwrap();

        // Get password hash
        let hash = queries.get_user_password_hash(&user.id).await.unwrap();
        assert_eq!(hash, "test_hash");
    }

    #[tokio::test]
    async fn test_message_queries() {
        let queries = create_test_queries().await;

        // Create test user first
        let user = User {
            id: Uuid::new_v4(),
            callsign: "W1MSG".to_string(),
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
        queries.create_user(&user, "hash").await.unwrap();

        // Create test node
        let node_id = Uuid::new_v4();
        sqlx::query!(
            "INSERT INTO nodes (id, callsign, description, created_at, last_seen, status, version)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            node_id.to_string(),
            "W1NODE",
            "Test Node",
            SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs() as i64,
            SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs() as i64,
            1i32,
            "1.0.0"
        )
        .execute(queries.pool())
        .await
        .unwrap();

        let message = Message {
            id: Uuid::new_v4(),
            message_type: MessageType::Direct,
            sender_id: user.id,
            sender_callsign: user.callsign.clone(),
            recipient_id: Some(user.id),
            recipient_callsign: Some(user.callsign.clone()),
            group_id: None,
            subject: Some("Test Subject".to_string()),
            body: "Test message body".to_string(),
            priority: MessagePriority::Normal,
            created_at: SystemTime::now(),
            delivered_at: None,
            read_at: None,
            parent_id: None,
            thread_id: None,
            status: MessageStatus::Sent,
            origin_node_id: node_id,
            message_hash: "test_hash".to_string(),
            rf_metadata: None,
            flags: MessageFlags::default(),
        };

        // Create message
        queries.create_message(&message).await.unwrap();

        // Get message by ID
        let retrieved = queries.get_message_by_id(&message.id).await.unwrap().unwrap();
        assert_eq!(retrieved.body, message.body);
        assert_eq!(retrieved.sender_callsign, message.sender_callsign);

        // Get user messages
        let user_messages = queries.get_user_messages(&user.id, None, 10, 0).await.unwrap();
        assert_eq!(user_messages.len(), 1);

        // Mark as read
        queries.mark_message_read(&message.id, SystemTime::now()).await.unwrap();

        // Check unread count
        let unread_count = queries.get_unread_message_count(&user.id).await.unwrap();
        assert_eq!(unread_count, 0);

        // Check for duplicates
        let exists = queries.message_exists_by_hash("test_hash").await.unwrap();
        assert!(exists);

        let not_exists = queries.message_exists_by_hash("nonexistent").await.unwrap();
        assert!(!not_exists);
    }

    #[tokio::test]
    async fn test_license_cache_queries() {
        let queries = create_test_queries().await;

        let license_info = LicenseInfo {
            callsign: "W1TEST".to_string(),
            name: Some("Test Ham".to_string()),
            license_class: Some("Extra".to_string()),
            status: LicenseStatus::Active,
            expires: Some(SystemTime::now() + Duration::from_secs(365 * 24 * 3600)),
            address: None,
            country: "United States".to_string(),
            verified_at: SystemTime::now(),
        };

        // Cache license info
        queries.cache_license_info(&license_info).await.unwrap();

        // Retrieve cached info
        let cached = queries.get_cached_license_info("W1TEST").await.unwrap().unwrap();
        assert_eq!(cached.callsign, license_info.callsign);
        assert_eq!(cached.name, license_info.name);
        assert_eq!(cached.status, license_info.status);

        // Test case insensitive lookup
        let cached_lower = queries.get_cached_license_info("w1test").await.unwrap().unwrap();
        assert_eq!(cached_lower.callsign, license_info.callsign);
    }

    #[tokio::test]
    async fn test_statistics_queries() {
        let queries = create_test_queries().await;

        // Create test data
        let user = User {
            id: Uuid::new_v4(),
            callsign: "W1STAT".to_string(),
            full_name: Some("Stats User".to_string()),
            email: None,
            location: None,
            role: UserRole::User,
            created_at: SystemTime::now(),
            last_login: None,
            status: UserStatus::Active,
            license_class: None,
            license_verified: false,
        };
        queries.create_user(&user, "hash").await.unwrap();

        // Get database stats
        let stats = queries.get_database_stats().await.unwrap();
        assert_eq!(stats.user_count, 1);

        // Get user message stats
        let user_stats = queries.get_user_message_stats(&user.id).await.unwrap();
        assert_eq!(user_stats.sent_count, 0);
        assert_eq!(user_stats.received_count, 0);

        // Get recent activity
        let activity = queries.get_recent_activity(24).await.unwrap();
        assert_eq!(activity.new_users, 1);
        assert_eq!(activity.timeframe_hours, 24);

        // Get top active users
        let top_users = queries.get_top_active_users(10, 7).await.unwrap();
        assert_eq!(top_users.len(), 1);
        assert_eq!(top_users[0].callsign, "W1STAT");
    }

    #[tokio::test]
    async fn test_network_node_queries() {
        let queries = create_test_queries().await;

        let node = NetworkNode {
            id: Uuid::new_v4(),
            callsign: "W1NODE".to_string(),
            description: "Test Node".to_string(),
            location: Some("FN42".to_string()),
            operator_callsign: Some("W1OP".to_string()),
            created_at: SystemTime::now(),
            last_seen: SystemTime::now(),
            status: NodeStatus::Online,
            version: "1.0.0".to_string(),
            network_config: NetworkConfig::default(),
            capabilities: NodeCapabilities::default(),
            stats: NodeStats::default(),
        };

        // Create/update node
        queries.upsert_network_node(&node).await.unwrap();

        // Get node by ID
        let retrieved = queries.get_network_node_by_id(&node.id).await.unwrap().unwrap();
        assert_eq!(retrieved.callsign, node.callsign);
        assert_eq!(retrieved.status, NodeStatus::Online);

        // List active nodes
        let active_nodes = queries.list_active_nodes().await.unwrap();
        assert_eq!(active_nodes.len(), 1);

        // Update last seen
        queries.update_node_last_seen(&node.id).await.unwrap();

        // Test marking stale nodes offline
        let marked_offline = queries.mark_stale_nodes_offline(0).await.unwrap();
        assert_eq!(marked_offline, 0); // Should be 0 since we just updated last_seen
    }
}use anyhow::Result;
use sqlx::{Pool, Sqlite, Row};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use uuid::Uuid;

use crate::auth::{User, UserRole, UserStatus, LicenseClass, LicenseInfo, LicenseStatus};
use crate::database::models::*;

/// Database queries organized by domain
pub struct Queries {
    pool: Pool<Sqlite>,
}

impl Queries {
    /// Create new queries instance
    pub fn new(pool: Pool<Sqlite>) -> Self {
        Self { pool }
    }

    /// Get reference to the connection pool
    pub fn pool(&self) -> &Pool<Sqlite> {
        &self.pool
    }
}

// =============================================================================
// USER QUERIES
// =============================================================================

impl Queries {
    /// Create a new user with optimized insert
    pub async fn create_user(&self, user: &User, password_hash: &str) -> Result<()> {
        let timestamp = user.created_at.duration_since(UNIX_EPOCH)?.as_secs() as i64;
        let last_login_timestamp = user.last_login
            .map(|t| t.duration_since(UNIX_EPOCH).map(|d| d.as_secs() as i64))
            .transpose()?;

        sqlx::query!(
            "INSERT INTO users (id, callsign, full_name, email, location, role, created_at, 
             last_login, status, license_class, license_verified, password_hash)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
            user.id.to_string(),
            user.callsign,
            user.full_name,
            user.email,
            user.location,
            user.role as i32,
            timestamp,
            last_login_timestamp,
            user.status as i32,
            user.license_class.map(|lc| lc as i32),
            user.license_verified,
            password_hash
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Get user by ID with optimized query
    pub async fn get_user_by_id(&self, user_id: &Uuid) -> Result<Option<User>> {
        let row = sqlx::query!(
            "SELECT id, callsign, full_name, email, location, role, created_at, 
             last_login, status, license_class, license_verified
             FROM users WHERE id = ?1",
            user_id.to_string()
        )
        .fetch_optional(&self.pool)
        .await?;

        if let Some(row) = row {
            Ok(Some(User {
                id: Uuid::parse_str(&row.id)?,
                callsign: row.callsign,
                full_name: row.full_name,
                email: row.email,
                location: row.location,
                role: match row.role {
                    0 => UserRole::User,
                    1 => UserRole::Operator,
                    2 => UserRole::Administrator,
                    3 => UserRole::System,
                    _ => UserRole::User,
                },
                created_at: UNIX_EPOCH + Duration::from_secs(row.created_at as u64),
                last_login: row.last_login
                    .map(|t| UNIX_EPOCH + Duration::from_secs(t as u64)),
                status: match row.status {
                    0 => UserStatus::Active,
                    1 => UserStatus::Pending,
                    2 => UserStatus::Suspended,
                    3 => UserStatus::Banned,
                    4 => UserStatus::Disabled,
                    _ => UserStatus::Active,
                },
                license_class: row.license_class.and_then(|lc| match lc {
                    0 => Some(LicenseClass::Technician),
                    1 => Some(LicenseClass::General),
                    2 => Some(LicenseClass::Extra),
                    3 => Some(LicenseClass::Advanced),
                    4 => Some(LicenseClass::Novice),
                    _ => None,
                }),
                license_verified: row.license_verified,
            }))
        } else {
            Ok(None)
        }
    }

    /// Get user by callsign (case-insensitive)
    pub async fn get_user_by_callsign(&self, callsign: &str) -> Result<Option<User>> {
        let callsign = callsign.to_uppercase();
        
        let row = sqlx::query!(
            "SELECT id, callsign, full_name, email, location, role, created_at, 
             last_login, status, license_class, license_verified
             FROM users WHERE UPPER(callsign) = ?1",
            callsign
        )
        .fetch_optional(&self.pool)
        .await?;

        if let Some(row) = row {
            Ok(Some(User {
                id: Uuid::parse_str(&row.id)?,
                callsign: row.callsign,
                full_name: row.full_name,
                email: row.email,
                location: row.location,
                role: match row.role {
                    0 => UserRole::User,
                    1 => UserRole::Operator,
                    2 => UserRole::Administrator,
                    3 => UserRole::System,
                    _ => UserRole::User,
                },
                created_at: UNIX_EPOCH + Duration::from_secs(row.created_at as u64),
                last_login: row.last_login
                    .map(|t| UNIX_EPOCH + Duration::from_secs(t as u64)),
                status: match row.status {
                    0 => UserStatus::Active,
                    1 => UserStatus::Pending,
                    2 => UserStatus::Suspended,
                    3 => UserStatus::Banned,
                    4 => UserStatus::Disabled,
                    _ => UserStatus::Active,
                },
                license_class: row.license_class.and_then(|lc| match lc {
                    0 => Some(LicenseClass::Technician),
                    1 => Some(LicenseClass::General),
                    2 => Some(LicenseClass::Extra),
                    3 => Some(LicenseClass::Advanced),
                    4 => Some(LicenseClass::Novice),
                    _ => None,
                }),
                license_verified: row.license_verified,
            }))
        } else {
            Ok(None)
        }
    }

    /// Get user password hash
    pub async fn get_user_password_hash(&self, user_id: &Uuid) -> Result<String> {
        let row = sqlx::query!(
            "SELECT password_hash FROM users WHERE id = ?1",
            user_id.to_string()
        )
        .fetch_one(&self.pool)
        .await?;

        Ok(row.password_hash)
    }

    /// Update user password hash
    pub async fn update_user_password(&self, user_id: &Uuid, password_hash: &str) -> Result<()> {
        sqlx::query!(
            "UPDATE users SET password_hash = ?1 WHERE id = ?2",
            password_hash,
            user_id.to_string()
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Update user last login timestamp
    pub async fn update_last_login(&self, user_id: &Uuid) -> Result<()> {
        let timestamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs() as i64;
        
        sqlx::query!(
            "UPDATE users SET last_login = ?1 WHERE id = ?2",
            timestamp,
            user_id.to_string()
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// List users with pagination and filters
    pub async fn list_users(
        &self,
        offset: u32,
        limit: u32,
        status_filter: Option<UserStatus>,
        role_filter: Option<UserRole>,
    ) -> Result<Vec<User>> {
        let mut query = "SELECT id, callsign, full_name, email, location, role, created_at, 
                         last_login, status, license_class, license_verified
                         FROM users WHERE 1=1".to_string();
        
        let mut params = Vec::new();
        
        if let Some(status) = status_filter {
            query.push_str(" AND status = ?");
            params.push(status as i32);
        }
        
        if let Some(role) = role_filter {
            query.push_str(" AND role = ?");
            params.push(role as i32);
        }
        
        query.push_str(" ORDER BY created_at DESC LIMIT ? OFFSET ?");
        params.push(limit as i32);
        params.push(offset as i32);

        let mut query_builder = sqlx::query(&query);
        for param in params {
            query_builder = query_builder.bind(param);
        }

        let rows = query_builder.fetch_all(&self.pool).await?;
        
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
                        0 => Some(LicenseClass::Technician),
                        1 => Some(LicenseClass::General),
                        2 => Some(LicenseClass::Extra),
                        3 => Some(LicenseClass::Advanced),
                        4 => Some(LicenseClass::Novice),
                        _ => None,
                    }),
                license_verified: row.get("license_verified"),
            };
            users.push(user);
        }

        Ok(users)
    }

    /// Search users by callsign prefix
    pub async fn search_users_by_callsign(&self, prefix: &str, limit: u32) -> Result<Vec<User>> {
        let prefix = format!("{}%", prefix.to_uppercase());
        
        let rows = sqlx::query!(
            "SELECT id, callsign, full_name, email, location, role, created_at, 
             last_login, status, license_class, license_verified
             FROM users 
             WHERE UPPER(callsign) LIKE ?1 AND status = 0
             ORDER BY callsign 
             LIMIT ?2",
            prefix,
            limit as i64
        )
        .fetch_all(&self.pool)
        .await?;

        let mut users = Vec::new();
        for row in rows {
            let user = User {
                id: Uuid::parse_str(&row.id)?,
                callsign: row.callsign,
                full_name: row.full_name,
                email: row.email,
                location: row.location,
                role: match row.role {
                    0 => UserRole::User,
                    1 => UserRole::Operator,
                    2 => UserRole::Administrator,
                    3 => UserRole::System,
                    _ => UserRole::User,
                },
                created_at: UNIX_EPOCH + Duration::from_secs(row.created_at as u64),
                last_login: row.last_login
                    .map(|t| UNIX_EPOCH + Duration::from_secs(t as u64)),
                status: match row.status {
                    0 => UserStatus::Active,
                    1 => UserStatus::Pending,
                    2 => UserStatus::Suspended,
                    3 => UserStatus::Banned,
                    4 => UserStatus::Disabled,
                    _ => UserStatus::Active,
                },
                license_class: row.license_class.and_then(|lc| match lc {
                    0 => Some(LicenseClass::Technician),
                    1 => Some(LicenseClass::General),
                    2 => Some(LicenseClass::Extra),
                    3 => Some(LicenseClass::Advanced),
                    4 => Some(LicenseClass::Novice),
                    _ => None,
                }),
                license_verified: row.license_verified,
            };
            users.push(user);
        }

        Ok(users)
    }

    /// Count administrators
    pub async fn count_administrators(&self) -> Result<i64> {
        let row = sqlx::query!(
            "SELECT COUNT(*) as count FROM users WHERE role = ?1",
            UserRole::Administrator as i32
        )
        .fetch_one(&self.pool)
        .await?;

        Ok(row.count)
    }
}

// =============================================================================
// MESSAGE QUERIES
// =============================================================================

impl Queries {
    /// Create a new message
    pub async fn create_message(&self, message: &Message) -> Result<()> {
        let created_at = message.created_at.duration_since(UNIX_EPOCH)?.as_secs() as i64;
        let delivered_at = message.delivered_at
            .map(|t| t.duration_since(UNIX_EPOCH).map(|d| d.as_secs() as i64))
            .transpose()?;
        let read_at = message.read_at
            .map(|t| t.duration_since(UNIX_EPOCH).map(|d| d.as_secs() as i64))
            .transpose()?;
        
        let rf_metadata_json = message.rf_metadata.as_ref()
            .map(|rf| serde_json::to_string(rf).unwrap_or_default());
        let flags_json = serde_json::to_string(&message.flags).unwrap_or_default();

        sqlx::query!(
            "INSERT INTO messages (id, message_type, sender_id, sender_callsign, 
             recipient_id, recipient_callsign, group_id, subject, body, priority, 
             created_at, delivered_at, read_at, parent_id, thread_id, status, 
             origin_node_id, message_hash, rf_metadata, flags)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20)",
            message.id.to_string(),
            message.message_type as i32,
            message.sender_id.to_string(),
            message.sender_callsign,
            message.recipient_id.map(|id| id.to_string()),
            message.recipient_callsign,
            message.group_id.map(|id| id.to_string()),
            message.subject,
            message.body,
            message.priority as i32,
            created_at,
            delivered_at,
            read_at,
            message.parent_id.map(|id| id.to_string()),
            message.thread_id.map(|id| id.to_string()),
            message.status as i32,
            message.origin_node_id.to_string(),
            message.message_hash,
            rf_metadata_json,
            flags_json
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Get message by ID
    pub async fn get_message_by_id(&self, message_id: &Uuid) -> Result<Option<Message>> {
        let row = sqlx::query!(
            "SELECT id, message_type, sender_id, sender_callsign, recipient_id, 
             recipient_callsign, group_id, subject, body, priority, created_at, 
             delivered_at, read_at, parent_id, thread_id, status, origin_node_id, 
             message_hash, rf_metadata, flags
             FROM messages WHERE id = ?1",
            message_id.to_string()
        )
        .fetch_optional(&self.pool)
        .await?;

        if let Some(row) = row {
            let rf_metadata = row.rf_metadata.and_then(|rf| 
                serde_json::from_str(&rf).ok()
            );
            let flags: MessageFlags = row.flags.as_ref()
                .and_then(|f| serde_json::from_str(f).ok())
                .unwrap_or_default();

            Ok(Some(Message {
                id: Uuid::parse_str(&row.id)?,
                message_type: match row.message_type {
                    0 => MessageType::Direct,
                    1 => MessageType::Group,
                    2 => MessageType::System,
                    3 => MessageType::Emergency,
                    4 => MessageType::Bulletin,
                    _ => MessageType::Direct,
                },
                sender_id: Uuid::parse_str(&row.sender_id)?,
                sender_callsign: row.sender_callsign,
                recipient_id: row.recipient_id.as_ref().and_then(|id| Uuid::parse_str(id).ok()),
                recipient_callsign: row.recipient_callsign,
                group_id: row.group_id.as_ref().and_then(|id| Uuid::parse_str(id).ok()),
                subject: row.subject,
                body: row.body,
                priority: match row.priority {
                    0 => MessagePriority::Low,
                    1 => MessagePriority::Normal,
                    2 => MessagePriority::High,
                    3 => MessagePriority::Emergency,
                    _ => MessagePriority::Normal,
                },
                created_at: UNIX_EPOCH + Duration::from_secs(row.created_at as u64),
                delivered_at: row.delivered_at
                    .map(|t| UNIX_EPOCH + Duration::from_secs(t as u64)),
                read_at: row.read_at
                    .map(|t| UNIX_EPOCH + Duration::from_secs(t as u64)),
                parent_id: row.parent_id.as_ref().and_then(|id| Uuid::parse_str(id).ok()),
                thread_id: row.thread_id.as_ref().and_then(|id| Uuid::parse_str(id).ok()),
                status: match row.status {
                    0 => MessageStatus::Pending,
                    1 => MessageStatus::Sent,
                    2 => MessageStatus::Delivered,
                    3 => MessageStatus::Read,
                    4 => MessageStatus::Failed,
                    5 => MessageStatus::Deleted,
                    _ => MessageStatus::Pending,
                },
                origin_node_id: Uuid::parse_str(&row.origin_node_id)?,
                message_hash: row.message_hash,
                rf_metadata,
                flags,
            }))
        } else {
            Ok(None)
        }
    }

    /// Get messages for a user (inbox)
    pub async fn get_user_messages(
        &self,
        user_id: &Uuid,
        message_type: Option<MessageType>,
        limit: u32,
        offset: u32,
    ) -> Result<Vec<Message>> {
        let mut query = "SELECT id, message_type, sender_id, sender_callsign, recipient_id, 
                         recipient_callsign, group_id, subject, body, priority, created_at, 
                         delivered_at, read_at, parent_id, thread_id, status, origin_node_id, 
                         message_hash, rf_metadata, flags
                         FROM messages 
                         WHERE (recipient_id = ?1 OR sender_id = ?1) 
                         AND status != 5".to_string(); // Exclude deleted messages
        
        let mut params = vec![user_id.to_string()];
        
        if let Some(msg_type) = message_type {
            query.push_str(" AND message_type = ?");
            params.push((msg_type as i32).to_string());
        }
        
        query.push_str(" ORDER BY created_at DESC LIMIT ? OFFSET ?");
        params.push(limit.to_string());
        params.push(offset.to_string());

        let mut query_builder = sqlx::query(&query);
        for param in params {
            query_builder = query_builder.bind(param);
        }

        let rows = query_builder.fetch_all(&self.pool).await?;
        
        let mut messages = Vec::new();
        for row in rows {
            let rf_metadata = row.get::<Option<String>, _>("rf_metadata")
                .and_then(|rf| serde_json::from_str(&rf).ok());
            let flags: MessageFlags = row.get::<Option<String>, _>("flags")
                .and_then(|f| serde_json::from_str(&f).ok())
                .unwrap_or_default();

            let message = Message {
                id: Uuid::parse_str(&row.get::<String, _>("id"))?,
                message_type: match row.get::<i32, _>("message_type") {
                    0 => MessageType::Direct,
                    1 => MessageType::Group,
                    2 => MessageType::System,
                    3 => MessageType::Emergency,
                    4 => MessageType::Bulletin,
                    _ => MessageType::Direct,
                },
                sender_id: Uuid::parse_str(&row.get::<String, _>("sender_id"))?,
                sender_callsign: row.get("sender_callsign"),
                recipient_id: row.get::<Option<String>, _>("recipient_id")
                    .and_then(|id| Uuid::parse_str(&id).ok()),
                recipient_callsign: row.get("recipient_callsign"),
                group_id: row.get::<Option<String>, _>("group_id")
                    .and_then(|id| Uuid::parse_str(&id).ok()),
                subject: row.get("subject"),
                body: row.get("body"),
                priority: match row.get::<i32, _>("priority") {
                    0 => MessagePriority::Low,
                    1 => MessagePriority::Normal,
                    2 => MessagePriority::High,
                    3 => MessagePriority::Emergency,
                    _ => MessagePriority::Normal,
                },
                created_at: UNIX_EPOCH + Duration::from_secs(row.get::<i64, _>("created_at") as u64),
                delivered_at: row.get::<Option<i64>, _>("delivered_at")
                    .map(|t| UNIX_EPOCH + Duration::from_secs(t as u64)),
                read_at: row.get::<Option<i64>, _>("read_at")
                    .map(|t| UNIX_EPOCH + Duration::from_secs(t as u64)),
                parent_id: row.get::<Option<String>, _>("parent_id")
                    .and_then(|id| Uuid::parse_str(&id).ok()),
                thread_id: row.get::<Option<String>, _>("thread_id")
                    .and_then(|id| Uuid::parse_str(&id).ok()),
                status: match row.get::<i32, _>("status") {
                    0 => MessageStatus::Pending,
                    1 => MessageStatus::Sent,
                    2 => MessageStatus::Delivered,
                    3 => MessageStatus::Read,
                    4 => MessageStatus::Failed,
                    5 => MessageStatus::Deleted,
                    _ => MessageStatus::Pending,
                },
                origin_node_id: Uuid::parse_str(&row.get::<String, _>("origin_node_id"))?,
                message_hash: row.get("message_hash"),
                rf_metadata,
                flags,
            };
            messages.push(message);
        }

        Ok(messages)
    }

    /// Get group messages with pagination
    pub async fn get_group_messages(
        &self,
        group_id: &Uuid,
        limit: u32,
        offset: u32,
    ) -> Result<Vec<Message>> {
        let rows = sqlx::query!(
            "SELECT id, message_type, sender_id, sender_callsign, recipient_id, 
             recipient_callsign, group_id, subject, body, priority, created_at, 
             delivered_at, read_at, parent_id, thread_id, status, origin_node_id, 
             message_hash, rf_metadata, flags
             FROM messages 
             WHERE group_id = ?1 AND status != 5
             ORDER BY created_at DESC 
             LIMIT ?2 OFFSET ?3",
            group_id.to_string(),
            limit as i64,
            offset as i64
        )
        .fetch_all(&self.pool)
        .await?;

        let mut messages = Vec::new();
        for row in rows {
            let rf_metadata = row.rf_metadata.and_then(|rf| 
                serde_json::from_str(&rf).ok()
            );
            let flags: MessageFlags = row.flags.as_ref()
                .and_then(|f| serde_json::from_str(f).ok())
                .unwrap_or_default();

            let message = Message {
                id: Uuid::parse_str(&row.id)?,
                message_type: match row.message_type {
                    0 => MessageType::Direct,
                    1 => MessageType::Group,
                    2 => MessageType::System,
                    3 => MessageType::Emergency,
                    4 => MessageType::Bulletin,
                    _ => MessageType::Group,
                },
                sender_id: Uuid::parse_str(&row.sender_id)?,
                sender_callsign: row.sender_callsign,
                recipient_id: row.recipient_id.as_ref().and_then(|id| Uuid::parse_str(id).ok()),
                recipient_callsign: row.recipient_callsign,
                group_id: row.group_id.as_ref().and_then(|id| Uuid::parse_str(id).ok()),
                subject: row.subject,
                body: row.body,
                priority: match row.priority {
                    0 => MessagePriority::Low,
                    1 => MessagePriority::Normal,
                    2 => MessagePriority::High,
                    3 => MessagePriority::Emergency,
                    _ => MessagePriority::Normal,
                },
                created_at: UNIX_EPOCH + Duration::from_secs(row.created_at as u64),
                delivered_at: row.delivered_at
                    .map(|t| UNIX_EPOCH + Duration::from_secs(t as u64)),
                read_at: row.read_at
                    .map(|t| UNIX_EPOCH + Duration::from_secs(t as u64)),
                parent_id: row.parent_id.as_ref().and_then(|id| Uuid::parse_str(id).ok()),
                thread_id: row.thread_id.as_ref().and_then(|id| Uuid::parse_str(id).ok()),
                status: match row.status {
                    0 => MessageStatus::Pending,
                    1 => MessageStatus::Sent,
                    2 => MessageStatus::Delivered,
                    3 => MessageStatus::Read,
                    4 => MessageStatus::Failed,
                    5 => MessageStatus::Deleted,
                    _ => MessageStatus::Pending,
                },
                origin_node_id: Uuid::parse_str(&row.origin_node_id)?,
                message_hash: row.message_hash,
                rf_metadata,
                flags,
            };
            messages.push(message);
        }

        Ok(messages)
    }

    /// Mark message as read
    pub async fn mark_message_read(&self, message_id: &Uuid, read_at: SystemTime) -> Result<()> {
        let timestamp = read_at.duration_since(UNIX_EPOCH)?.as_secs() as i64;
        
        sqlx::query!(
            "UPDATE messages SET read_at = ?1, status = CASE 
                WHEN status < 3 THEN 3 
                ELSE status 
             END
             WHERE id = ?2",
            timestamp,
            message_id.to_string()
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Update message status
    pub async fn update_message_status(&self, message_id: &Uuid, status: MessageStatus) -> Result<()> {
        sqlx::query!(
            "UPDATE messages SET status = ?1 WHERE id = ?2",
            status as i32,
            message_id.to_string()
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Get unread message count for user
    pub async fn get_unread_message_count(&self, user_id: &Uuid) -> Result<i64> {
        let row = sqlx::query!(
            "SELECT COUNT(*) as count FROM messages 
             WHERE recipient_id = ?1 AND read_at IS NULL AND status != 5",
            user_id.to_string()
        )
        .fetch_one(&self.pool)
        .await?;

        Ok(row.count)
    }

    /// Get message thread
    pub async fn get_message_thread(&self, thread_id: &Uuid) -> Result<Vec<Message>> {
        let rows = sqlx::query!(
            "SELECT id, message_type, sender_id, sender_callsign, recipient_id, 
             recipient_callsign, group_id, subject, body, priority, created_at, 
             delivered_at, read_at, parent_id, thread_id, status, origin_node_id, 
             message_hash, rf_metadata, flags
             FROM messages 
             WHERE thread_id = ?1 AND
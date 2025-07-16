use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, SystemTime};
use tokio::sync::RwLock;
use uuid::Uuid;

use crate::auth::User;
use crate::database::{Database, models::*};

/// Group message handler for bulletin boards and discussion groups
#[derive(Clone)]
pub struct GroupMessageHandler {
    /// Database connection
    database: Database,
    
    /// Group cache for fast access
    group_cache: Arc<RwLock<GroupCache>>,
    
    /// Membership cache
    membership_cache: Arc<RwLock<MembershipCache>>,
    
    /// Group statistics cache
    stats_cache: Arc<RwLock<HashMap<Uuid, GroupStats>>>,
}

/// Group cache for performance optimization
#[derive(Debug, Default)]
struct GroupCache {
    /// Groups by ID
    groups: HashMap<Uuid, Group>,
    
    /// Group lists by type
    groups_by_type: HashMap<GroupType, Vec<Uuid>>,
    
    /// Public groups list
    public_groups: Vec<Uuid>,
    
    /// Cache timestamps
    cache_timestamps: HashMap<Uuid, SystemTime>,
    
    /// Cache duration
    cache_duration: Duration,
}

/// Membership cache
#[derive(Debug, Default)]
struct MembershipCache {
    /// User memberships by user ID
    user_memberships: HashMap<Uuid, Vec<GroupMembership>>,
    
    /// Group members by group ID
    group_members: HashMap<Uuid, Vec<GroupMembership>>,
    
    /// Cache timestamps
    cache_timestamps: HashMap<Uuid, SystemTime>,
    
    /// Cache duration
    cache_duration: Duration,
}

/// Group creation request
#[derive(Debug, Clone, Deserialize)]
pub struct CreateGroupRequest {
    /// Group name
    pub name: String,
    
    /// Group description
    pub description: Option<String>,
    
    /// Group type
    pub group_type: GroupType,
    
    /// Group settings
    pub settings: GroupSettings,
}

/// Group message request
#[derive(Debug, Clone, Deserialize)]
pub struct GroupMessageRequest {
    /// Target group ID
    pub group_id: Uuid,
    
    /// Message subject
    pub subject: Option<String>,
    
    /// Message body
    pub body: String,
    
    /// Message priority
    pub priority: MessagePriority,
    
    /// Parent message ID (for replies)
    pub parent_id: Option<Uuid>,
    
    /// Message type (Group or Bulletin)
    pub message_type: MessageType,
}

/// Group membership request
#[derive(Debug, Clone, Deserialize)]
pub struct JoinGroupRequest {
    /// Group ID to join
    pub group_id: Uuid,
    
    /// Optional join message
    pub join_message: Option<String>,
}

/// Group invitation request
#[derive(Debug, Clone, Deserialize)]
pub struct InviteUserRequest {
    /// Group ID
    pub group_id: Uuid,
    
    /// User to invite (ID or callsign)
    pub user_identifier: UserIdentifier,
    
    /// Role to assign
    pub role: GroupRole,
    
    /// Invitation message
    pub invitation_message: Option<String>,
}

/// User identifier for invitations
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum UserIdentifier {
    /// User ID
    UserId(Uuid),
    
    /// Callsign
    Callsign(String),
}

/// Group query parameters
#[derive(Debug, Clone)]
pub struct GroupQuery {
    /// Filter by group type
    pub group_type: Option<GroupType>,
    
    /// Filter by status
    pub status: Option<GroupStatus>,
    
    /// Search term (name/description)
    pub search: Option<String>,
    
    /// Only show groups user can join
    pub joinable_only: bool,
    
    /// Requesting user ID
    pub user_id: Option<Uuid>,
    
    /// Pagination limit
    pub limit: u32,
    
    /// Pagination offset
    pub offset: u32,
}

/// Group membership query
#[derive(Debug, Clone)]
pub struct MembershipQuery {
    /// Group ID
    pub group_id: Uuid,
    
    /// Filter by role
    pub role_filter: Option<GroupRole>,
    
    /// Filter by status
    pub status_filter: Option<MembershipStatus>,
    
    /// Search term (callsign/name)
    pub search: Option<String>,
    
    /// Include pending members
    pub include_pending: bool,
    
    /// Pagination limit
    pub limit: u32,
    
    /// Pagination offset
    pub offset: u32,
}

/// Group activity summary
#[derive(Debug, Clone, Serialize)]
pub struct GroupActivity {
    /// Group information
    pub group: Group,
    
    /// Recent messages
    pub recent_messages: Vec<Message>,
    
    /// Active members count
    pub active_members: u32,
    
    /// Unread count for user
    pub unread_count: u32,
    
    /// User's membership info
    pub membership: Option<GroupMembership>,
}

/// Group management response
#[derive(Debug, Clone, Serialize)]
pub struct GroupManagementResponse {
    /// Operation success
    pub success: bool,
    
    /// Result message
    pub message: String,
    
    /// Updated group (if applicable)
    pub group: Option<Group>,
    
    /// Updated membership (if applicable)
    pub membership: Option<GroupMembership>,
}

impl GroupMessageHandler {
    /// Create a new group message handler
    pub fn new(database: Database) -> Self {
        Self {
            database,
            group_cache: Arc::new(RwLock::new(GroupCache {
                groups: HashMap::new(),
                groups_by_type: HashMap::new(),
                public_groups: Vec::new(),
                cache_timestamps: HashMap::new(),
                cache_duration: Duration::from_secs(300), // 5 minutes
            })),
            membership_cache: Arc::new(RwLock::new(MembershipCache {
                user_memberships: HashMap::new(),
                group_members: HashMap::new(),
                cache_timestamps: HashMap::new(),
                cache_duration: Duration::from_secs(180), // 3 minutes
            })),
            stats_cache: Arc::new(RwLock::new(HashMap::new())),
        }
    }
    
    /// Create a new group
    pub async fn create_group(
        &self,
        creator: &User,
        request: CreateGroupRequest,
    ) -> Result<Group> {
        // Validate group name
        self.validate_group_name(&request.name)?;
        
        // Check for duplicate names
        if self.group_name_exists(&request.name).await? {
            return Err(anyhow::anyhow!("Group name already exists"));
        }
        
        // Validate permissions
        self.validate_group_creation_permissions(creator, &request)?;
        
        let group_id = Uuid::new_v4();
        let now = SystemTime::now();
        
        let group = Group {
            id: group_id,
            name: request.name,
            description: request.description,
            group_type: request.group_type,
            created_by: creator.id,
            created_at: now,
            status: GroupStatus::Active,
            settings: request.settings,
            stats: GroupStats::default(),
        };
        
        // Store group in database
        self.database.create_group(&group).await?;
        
        // Add creator as owner
        let owner_membership = GroupMembership {
            group_id,
            user_id: creator.id,
            user_callsign: creator.callsign.clone(),
            role: GroupRole::Owner,
            joined_at: now,
            last_activity: Some(now),
            status: MembershipStatus::Active,
            invited_by: None,
            settings: MemberSettings::default(),
        };
        
        self.database.add_group_member(&owner_membership).await?;
        
        // Update caches
        self.invalidate_group_cache(&group_id).await;
        self.invalidate_membership_cache(&creator.id).await;
        
        Ok(group)
    }
    
    /// Send a message to a group
    pub async fn send_group_message(
        &self,
        sender: &User,
        request: GroupMessageRequest,
        origin_node_id: Uuid,
    ) -> Result<Message> {
        // Validate group membership and permissions
        let membership = self.get_user_membership(&request.group_id, &sender.id).await?
            .ok_or_else(|| anyhow::anyhow!("User is not a member of this group"))?;
        
        if membership.status != MembershipStatus::Active {
            return Err(anyhow::anyhow!("User membership is not active"));
        }
        
        // Get group to check settings
        let group = self.get_group(&request.group_id).await?
            .ok_or_else(|| anyhow::anyhow!("Group not found"))?;
        
        // Validate posting permissions
        self.validate_posting_permissions(&group, &membership, &request)?;
        
        // Create message
        let message_id = Uuid::new_v4();
        let now = SystemTime::now();
        
        // Determine thread ID for replies
        let thread_id = if let Some(parent_id) = request.parent_id {
            self.get_thread_id_from_parent(&parent_id).await?
        } else {
            message_id // New thread
        };
        
        // Generate message hash
        let message_hash = self.generate_group_message_hash(
            &sender.callsign,
            &request.group_id,
            &request.body,
            now,
        );
        
        let message = Message {
            id: message_id,
            message_type: request.message_type,
            sender_id: sender.id,
            sender_callsign: sender.callsign.clone(),
            recipient_id: None,
            recipient_callsign: None,
            group_id: Some(request.group_id),
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
                important: request.priority >= MessagePriority::High,
                ..Default::default()
            },
        };
        
        // Store message
        self.database.create_message(&message).await?;
        
        // Update member last activity
        self.update_member_activity(&request.group_id, &sender.id).await?;
        
        // Update group statistics
        self.update_group_message_stats(&request.group_id).await?;
        
        Ok(message)
    }
    
    /// Join a group
    pub async fn join_group(
        &self,
        user: &User,
        request: JoinGroupRequest,
    ) -> Result<GroupManagementResponse> {
        let group = self.get_group(&request.group_id).await?
            .ok_or_else(|| anyhow::anyhow!("Group not found"))?;
        
        // Check if user is already a member
        if let Some(existing_membership) = self.get_user_membership(&request.group_id, &user.id).await? {
            match existing_membership.status {
                MembershipStatus::Active => {
                    return Ok(GroupManagementResponse {
                        success: false,
                        message: "Already a member of this group".to_string(),
                        group: Some(group),
                        membership: Some(existing_membership),
                    });
                }
                MembershipStatus::Banned => {
                    return Ok(GroupManagementResponse {
                        success: false,
                        message: "You are banned from this group".to_string(),
                        group: Some(group),
                        membership: None,
                    });
                }
                _ => {
                    // Rejoin if left or pending
                }
            }
        }
        
        // Validate join permissions
        self.validate_join_permissions(&group, user)?;
        
        let now = SystemTime::now();
        let membership_status = if group.settings.require_approval {
            MembershipStatus::Pending
        } else {
            MembershipStatus::Active
        };
        
        let membership = GroupMembership {
            group_id: request.group_id,
            user_id: user.id,
            user_callsign: user.callsign.clone(),
            role: GroupRole::Member,
            joined_at: now,
            last_activity: Some(now),
            status: membership_status,
            invited_by: None,
            settings: MemberSettings::default(),
        };
        
        // Add or update membership
        self.database.add_group_member(&membership).await?;
        
        // Update caches
        self.invalidate_membership_cache(&user.id).await;
        self.invalidate_group_member_cache(&request.group_id).await;
        
        let message = if membership_status == MembershipStatus::Pending {
            "Join request submitted for approval".to_string()
        } else {
            "Successfully joined group".to_string()
        };
        
        Ok(GroupManagementResponse {
            success: true,
            message,
            group: Some(group),
            membership: Some(membership),
        })
    }
    
    /// Leave a group
    pub async fn leave_group(
        &self,
        user: &User,
        group_id: &Uuid,
    ) -> Result<GroupManagementResponse> {
        let membership = self.get_user_membership(group_id, &user.id).await?
            .ok_or_else(|| anyhow::anyhow!("User is not a member of this group"))?;
        
        // Check if user is the only owner
        if membership.role == GroupRole::Owner {
            let owner_count = self.count_group_owners(group_id).await?;
            if owner_count <= 1 {
                return Ok(GroupManagementResponse {
                    success: false,
                    message: "Cannot leave group as the only owner. Transfer ownership first.".to_string(),
                    group: None,
                    membership: Some(membership),
                });
            }
        }
        
        // Update membership status to Left
        self.database.update_group_membership_status(
            group_id,
            &user.id,
            MembershipStatus::Left,
        ).await?;
        
        // Update caches
        self.invalidate_membership_cache(&user.id).await;
        self.invalidate_group_member_cache(group_id).await;
        
        Ok(GroupManagementResponse {
            success: true,
            message: "Successfully left group".to_string(),
            group: None,
            membership: None,
        })
    }
    
    /// Invite a user to a group
    pub async fn invite_user(
        &self,
        inviter: &User,
        request: InviteUserRequest,
    ) -> Result<GroupManagementResponse> {
        // Validate inviter permissions
        let inviter_membership = self.get_user_membership(&request.group_id, &inviter.id).await?
            .ok_or_else(|| anyhow::anyhow!("You are not a member of this group"))?;
        
        if !self.can_invite_users(&inviter_membership) {
            return Ok(GroupManagementResponse {
                success: false,
                message: "Insufficient permissions to invite users".to_string(),
                group: None,
                membership: None,
            });
        }
        
        // Resolve invited user
        let invited_user = self.resolve_user(&request.user_identifier).await?;
        
        // Check if user is already a member
        if let Some(existing) = self.get_user_membership(&request.group_id, &invited_user.id).await? {
            if existing.status == MembershipStatus::Active {
                return Ok(GroupManagementResponse {
                    success: false,
                    message: "User is already a member".to_string(),
                    group: None,
                    membership: Some(existing),
                });
            }
        }
        
        let now = SystemTime::now();
        let membership = GroupMembership {
            group_id: request.group_id,
            user_id: invited_user.id,
            user_callsign: invited_user.callsign.clone(),
            role: request.role,
            joined_at: now,
            last_activity: Some(now),
            status: MembershipStatus::Pending,
            invited_by: Some(inviter.id),
            settings: MemberSettings::default(),
        };
        
        self.database.add_group_member(&membership).await?;
        
        // Update caches
        self.invalidate_membership_cache(&invited_user.id).await;
        self.invalidate_group_member_cache(&request.group_id).await;
        
        Ok(GroupManagementResponse {
            success: true,
            message: format!("Invitation sent to {}", invited_user.callsign),
            group: None,
            membership: Some(membership),
        })
    }
    
    /// Get groups (public or user's groups)
    pub async fn get_groups(&self, query: GroupQuery) -> Result<Vec<Group>> {
        // Check cache for public groups
        if query.group_type.is_none() && query.search.is_none() && query.user_id.is_none() {
            if let Some(cached_groups) = self.get_cached_public_groups().await {
                return Ok(self.filter_groups(cached_groups, &query).await);
            }
        }
        
        // Query database
        let groups = self.database.list_groups(
            query.group_type,
            query.status,
            query.limit,
            query.offset,
        ).await?;
        
        // Apply additional filters
        let filtered_groups = self.filter_groups(groups, &query).await;
        
        // Cache public groups
        if query.group_type.is_none() && query.search.is_none() {
            self.cache_public_groups(&filtered_groups).await;
        }
        
        Ok(filtered_groups)
    }
    
    /// Get group by ID
    pub async fn get_group(&self, group_id: &Uuid) -> Result<Option<Group>> {
        // Check cache first
        if let Some(cached_group) = self.get_cached_group(group_id).await {
            return Ok(Some(cached_group));
        }
        
        // Get from database
        if let Some(group) = self.database.get_group_by_id(group_id).await? {
            // Cache the group
            self.cache_group(&group).await;
            Ok(Some(group))
        } else {
            Ok(None)
        }
    }
    
    /// Get group members
    pub async fn get_group_members(&self, query: MembershipQuery) -> Result<Vec<GroupMembership>> {
        // Check cache first
        if let Some(cached_members) = self.get_cached_group_members(&query.group_id).await {
            return Ok(self.filter_memberships(cached_members, &query));
        }
        
        // Query database
        let members = self.database.list_group_members(&query.group_id).await?;
        
        // Apply filters
        let filtered_members = self.filter_memberships(members, &query);
        
        // Cache results
        self.cache_group_members(&query.group_id, &filtered_members).await;
        
        Ok(filtered_members)
    }
    
    /// Get user's group memberships
    pub async fn get_user_groups(&self, user_id: &Uuid) -> Result<Vec<GroupMembership>> {
        // Check cache first
        if let Some(cached_memberships) = self.get_cached_user_memberships(user_id).await {
            return Ok(cached_memberships);
        }
        
        // Query database
        let memberships = self.database.get_user_groups(user_id).await?;
        
        // Cache results
        self.cache_user_memberships(user_id, &memberships).await;
        
        Ok(memberships)
    }
    
    /// Get group activity summary
    pub async fn get_group_activity(
        &self,
        group_id: &Uuid,
        user_id: &Uuid,
        message_limit: u32,
    ) -> Result<GroupActivity> {
        let group = self.get_group(group_id).await?
            .ok_or_else(|| anyhow::anyhow!("Group not found"))?;
        
        let membership = self.get_user_membership(group_id, user_id).await?;
        
        let recent_messages = self.database.get_group_messages(
            group_id,
            message_limit,
            0,
        ).await?;
        
        let active_members = self.count_active_group_members(group_id).await?;
        let unread_count = self.get_user_unread_count_for_group(group_id, user_id).await?;
        
        Ok(GroupActivity {
            group,
            recent_messages,
            active_members,
            unread_count,
            membership,
        })
    }
    
    /// Validate group permissions for a user
    pub async fn validate_group_permissions(
        &self,
        user: &User,
        group_id: &Uuid,
    ) -> Result<()> {
        let membership = self.get_user_membership(group_id, &user.id).await?
            .ok_or_else(|| anyhow::anyhow!("User is not a member of this group"))?;
        
        if membership.status != MembershipStatus::Active {
            return Err(anyhow::anyhow!("User membership is not active"));
        }
        
        Ok(())
    }
    
    /// Check if user is a group member
    pub async fn is_group_member(&self, user: &User, group_id: &Uuid) -> Result<bool> {
        if let Some(membership) = self.get_user_membership(group_id, &user.id).await? {
            Ok(membership.status == MembershipStatus::Active)
        } else {
            Ok(false)
        }
    }
    
    /// Check if user can moderate a group
    pub async fn can_moderate_group(&self, user: &User, group_id: &Uuid) -> Result<bool> {
        if let Some(membership) = self.get_user_membership(group_id, &user.id).await? {
            Ok(membership.can_moderate() && membership.status == MembershipStatus::Active)
        } else {
            Ok(false)
        }
    }
    
    /// Get group member IDs (for message delivery)
    pub async fn get_group_member_ids(&self, group_id: &Uuid) -> Result<Vec<Uuid>> {
        let members = self.database.list_group_members(group_id).await?;
        Ok(members
            .into_iter()
            .filter(|m| m.status == MembershipStatus::Active)
            .map(|m| m.user_id)
            .collect())
    }
    
    // Private helper methods
    
    /// Get user membership in a group
    async fn get_user_membership(
        &self,
        group_id: &Uuid,
        user_id: &Uuid,
    ) -> Result<Option<GroupMembership>> {
        self.database.get_group_membership(group_id, user_id).await
    }
    
    /// Validate group name
    fn validate_group_name(&self, name: &str) -> Result<()> {
        if name.trim().is_empty() {
            return Err(anyhow::anyhow!("Group name cannot be empty"));
        }
        
        if name.len() > 100 {
            return Err(anyhow::anyhow!("Group name too long (max 100 characters)"));
        }
        
        // Check for invalid characters
        if name.contains(['<', '>', '&', '"', '\'']) {
            return Err(anyhow::anyhow!("Group name contains invalid characters"));
        }
        
        Ok(())
    }
    
    /// Check if group name exists
    async fn group_name_exists(&self, name: &str) -> Result<bool> {
        // This would query the database to check for existing group names
        // Simplified for now
        Ok(false)
    }
    
    /// Validate group creation permissions
    fn validate_group_creation_permissions(
        &self,
        creator: &User,
        request: &CreateGroupRequest,
    ) -> Result<()> {
        // Check user status
        if creator.status != crate::auth::UserStatus::Active {
            return Err(anyhow::anyhow!("User account is not active"));
        }
        
        // Special groups may require higher permissions
        match request.group_type {
            GroupType::Emergency => {
                if creator.role != crate::auth::UserRole::Administrator &&
                   creator.role != crate::auth::UserRole::Operator {
                    return Err(anyhow::anyhow!("Insufficient permissions to create emergency group"));
                }
            }
            GroupType::Announcement => {
                if creator.role != crate::auth::UserRole::Administrator &&
                   creator.role != crate::auth::UserRole::Operator {
                    return Err(anyhow::anyhow!("Insufficient permissions to create announcement group"));
                }
            }
            _ => {} // Public and Private groups can be created by any user
        }
        
        Ok(())
    }
    
    /// Validate posting permissions
    fn validate_posting_permissions(
        &self,
        group: &Group,
        membership: &GroupMembership,
        request: &GroupMessageRequest,
    ) -> Result<()> {
        // Check if group allows posting
        if group.settings.read_only && membership.role != GroupRole::Owner &&
           membership.role != GroupRole::Administrator && membership.role != GroupRole::Moderator {
            return Err(anyhow::anyhow!("Group is read-only"));
        }
        
        // Check message type permissions
        if !group.settings.allowed_message_types.contains(&request.message_type) {
            return Err(anyhow::anyhow!("Message type not allowed in this group"));
        }
        
        Ok(())
    }
    
    /// Validate join permissions
    fn validate_join_permissions(&self, group: &Group, user: &User) -> Result<()> {
        // Check group status
        if group.status != GroupStatus::Active {
            return Err(anyhow::anyhow!("Group is not active"));
        }
        
        // Check if group is private
        if group.group_type == GroupType::Private {
            return Err(anyhow::anyhow!("Private groups require invitation"));
        }
        
        // Check member limits
        if let Some(max_members) = group.settings.max_members {
            // Would need to check current member count
            // Simplified for now
        }
        
        Ok(())
    }
    
    /// Check if user can invite others
    fn can_invite_users(&self, membership: &GroupMembership) -> bool {
        matches!(membership.role, 
                GroupRole::Owner | GroupRole::Administrator | GroupRole::Moderator)
    }
    
    /// Resolve user by identifier
    async fn resolve_user(&self, identifier: &UserIdentifier) -> Result<User> {
        match identifier {
            UserIdentifier::UserId(user_id) => {
                self.database.get_user_by_id(user_id).await?
                    .ok_or_else(|| anyhow::anyhow!("User not found"))
            }
            UserIdentifier::Callsign(callsign) => {
                self.database.get_user_by_callsign(callsign).await?
                    .ok_or_else(|| anyhow::anyhow!("Callsign not found"))
            }
        }
    }
    
    /// Get thread ID from parent message
    async fn get_thread_id_from_parent(&self, parent_id: &Uuid) -> Result<Uuid> {
        if let Some(parent) = self.database.get_message_by_id(parent_id).await? {
            Ok(parent.thread_id.unwrap_or(parent.id))
        } else {
            Err(anyhow::anyhow!("Parent message not found"))
        }
    }
    
    /// Generate group message hash
    fn generate_group_message_hash(
        &self,
        sender_callsign: &str,
        group_id: &Uuid,
        body: &str,
        timestamp: SystemTime,
    ) -> String {
        use blake3::Hasher;
        
        let mut hasher = Hasher::new();
        hasher.update(sender_callsign.as_bytes());
        hasher.update(group_id.as_bytes());
        hasher.update(body.as_bytes());
        hasher.update(&timestamp.duration_since(SystemTime::UNIX_EPOCH).unwrap().as_secs().to_le_bytes());
        
        hex::encode(hasher.finalize().as_bytes())
    }
    
    /// Update member activity timestamp
    async fn update_member_activity(&self, group_id: &Uuid, user_id: &Uuid) -> Result<()> {
        // This would update the last_activity timestamp for the member
        // Simplified for now
        Ok(())
    }
    
    /// Update group message statistics
    async fn update_group_message_stats(&self, group_id: &Uuid) -> Result<()> {
        // This would increment message counts and update last_message_at
        // Simplified for now
        Ok(())
    }
    
    /// Count group owners
    async fn count_group_owners(&self, group_id: &Uuid) -> Result<u32> {
        let members = self.database.list_group_members(group_id).await?;
        Ok(members
            .into_iter()
            .filter(|m| m.role == GroupRole::Owner && m.status == MembershipStatus::Active)
            .count() as u32)
    }
    
    /// Count active group members
    async fn count_active_group_members(&self, group_id: &Uuid) -> Result<u32> {
        let members = self.database.list_group_members(group_id).await?;
        Ok(members
            .into_iter()
            .filter(|m| m.status == MembershipStatus::Active)
            .count() as u32)
    }
    
    /// Get user's unread count for a group
    async fn get_user_unread_count_for_group(
        &self,
        group_id: &Uuid,
        user_id: &Uuid,
    ) -> Result<u32> {
        // This would count unread messages in the group for the user
        // Simplified for now
        Ok(0)
    }
    
    // Cache management methods
    
    /// Get cached group
    async fn get_cached_group(&self, group_id: &Uuid) -> Option<Group> {
        let cache = self.group_cache.read().await;
        if let Some(timestamp) = cache.cache_timestamps.get(group_id) {
            if timestamp.elapsed().unwrap_or_default() < cache.cache_duration {
                return cache.groups.get(group_id).cloned();
            }
        }
        None
    }
    
    /// Cache a group
    async fn cache_group(&self, group: &Group) {
        let mut cache = self.group_cache.write().await;
        cache.groups.insert(group.id, group.clone());
        cache.cache_timestamps.insert(group.id, SystemTime::now());
        
        // Update type-based cache
        cache.groups_by_type
            .entry(group.group_type)
            .or_insert_with(Vec::new)
            .push(group.id);
        
        // Update public groups cache
        if group.group_type == GroupType::Public {
            if !cache.public_groups.contains(&group.id) {
                cache.public_groups.push(group.id);
            }
        }
    }
    
    /// Get cached public groups
    async fn get_cached_public_groups(&self) -> Option<Vec<Group>> {
        let cache = self.group_cache.read().await;
        let mut groups = Vec::new();
        
        for group_id in &cache.public_groups {
            if let Some(timestamp) = cache.cache_timestamps.get(group_id) {
                if timestamp.elapsed().unwrap_or_default() < cache.cache_duration {
                    if let Some(group) = cache.groups.get(group_id) {
                        groups.push(group.clone());
                    }
                } else {
                    return None; // Cache expired
                }
            } else {
                return None; // No timestamp
            }
        }
        
        if groups.is_empty() {
            None
        } else {
            Some(groups)
        }
    }
    
    /// Cache public groups
    async fn cache_public_groups(&self, groups: &[Group]) {
        let mut cache = self.group_cache.write().await;
        let now = SystemTime::now();
        
        cache.public_groups.clear();
        for group in groups {
            if group.group_type == GroupType::Public {
                cache.groups.insert(group.id, group.clone());
                cache.cache_timestamps.insert(group.id, now);
                cache.public_groups.push(group.id);
            }
        }
    }
    
    /// Invalidate group cache
    async fn invalidate_group_cache(&self, group_id: &Uuid) {
        let mut cache = self.group_cache.write().await;
        cache.groups.remove(group_id);
        cache.cache_timestamps.remove(group_id);
        
        // Remove from type-based caches
        for group_list in cache.groups_by_type.values_mut() {
            group_list.retain(|id| id != group_id);
        }
        
        // Remove from public groups cache
        cache.public_groups.retain(|id| id != group_id);
    }
    
    /// Get cached user memberships
    async fn get_cached_user_memberships(&self, user_id: &Uuid) -> Option<Vec<GroupMembership>> {
        let cache = self.membership_cache.read().await;
        if let Some(timestamp) = cache.cache_timestamps.get(user_id) {
            if timestamp.elapsed().unwrap_or_default() < cache.cache_duration {
                return cache.user_memberships.get(user_id).cloned();
            }
        }
        None
    }
    
    /// Cache user memberships
    async fn cache_user_memberships(&self, user_id: &Uuid, memberships: &[GroupMembership]) {
        let mut cache = self.membership_cache.write().await;
        cache.user_memberships.insert(*user_id, memberships.to_vec());
        cache.cache_timestamps.insert(*user_id, SystemTime::now());
    }
    
    /// Get cached group members
    async fn get_cached_group_members(&self, group_id: &Uuid) -> Option<Vec<GroupMembership>> {
        let cache = self.membership_cache.read().await;
        if let Some(timestamp) = cache.cache_timestamps.get(group_id) {
            if timestamp.elapsed().unwrap_or_default() < cache.cache_duration {
                return cache.group_members.get(group_id).cloned();
            }
        }
        None
    }
    
    /// Cache group members
    async fn cache_group_members(&self, group_id: &Uuid, members: &[GroupMembership]) {
        let mut cache = self.membership_cache.write().await;
        cache.group_members.insert(*group_id, members.to_vec());
        cache.cache_timestamps.insert(*group_id, SystemTime::now());
    }
    
    /// Invalidate membership cache for user
    async fn invalidate_membership_cache(&self, user_id: &Uuid) {
        let mut cache = self.membership_cache.write().await;
        cache.user_memberships.remove(user_id);
        cache.cache_timestamps.remove(user_id);
    }
    
    /// Invalidate group member cache
    async fn invalidate_group_member_cache(&self, group_id: &Uuid) {
        let mut cache = self.membership_cache.write().await;
        cache.group_members.remove(group_id);
        cache.cache_timestamps.remove(group_id);
    }
    
    /// Filter groups based on query
    async fn filter_groups(&self, groups: Vec<Group>, query: &GroupQuery) -> Vec<Group> {
        let mut filtered: Vec<Group> = groups
            .into_iter()
            .filter(|group| {
                // Filter by status
                if let Some(status) = query.status {
                    if group.status != status {
                        return false;
                    }
                }
                
                // Filter by type
                if let Some(group_type) = query.group_type {
                    if group.group_type != group_type {
                        return false;
                    }
                }
                
                // Search filter
                if let Some(search) = &query.search {
                    let search_lower = search.to_lowercase();
                    if !group.name.to_lowercase().contains(&search_lower) &&
                       !group.description.as_ref()
                           .map(|desc| desc.to_lowercase().contains(&search_lower))
                           .unwrap_or(false) {
                        return false;
                    }
                }
                
                true
            })
            .collect();
        
        // Apply pagination
        filtered.sort_by(|a, b| a.name.cmp(&b.name));
        filtered
            .into_iter()
            .skip(query.offset as usize)
            .take(query.limit as usize)
            .collect()
    }
    
    /// Filter memberships based on query
    fn filter_memberships(&self, memberships: Vec<GroupMembership>, query: &MembershipQuery) -> Vec<GroupMembership> {
        memberships
            .into_iter()
            .filter(|membership| {
                // Filter by role
                if let Some(role) = query.role_filter {
                    if membership.role != role {
                        return false;
                    }
                }
                
                // Filter by status
                if let Some(status) = query.status_filter {
                    if membership.status != status {
                        return false;
                    }
                }
                
                // Include pending filter
                if !query.include_pending && membership.status == MembershipStatus::Pending {
                    return false;
                }
                
                // Search filter
                if let Some(search) = &query.search {
                    let search_lower = search.to_lowercase();
                    if !membership.user_callsign.to_lowercase().contains(&search_lower) {
                        return false;
                    }
                }
                
                true
            })
            .skip(query.offset as usize)
            .take(query.limit as usize)
            .collect()
    }
}

/// Database extension trait for group management queries
trait GroupQueries {
    async fn update_group_membership_status(
        &self,
        group_id: &Uuid,
        user_id: &Uuid,
        status: MembershipStatus,
    ) -> Result<()>;
}

impl GroupQueries for Database {
    async fn update_group_membership_status(
        &self,
        group_id: &Uuid,
        user_id: &Uuid,
        status: MembershipStatus,
    ) -> Result<()> {
        sqlx::query!(
            "UPDATE group_memberships SET status = ?1 WHERE group_id = ?2 AND user_id = ?3",
            status as i32,
            group_id.to_string(),
            user_id.to_string()
        )
        .execute(self.pool())
        .await?;
        
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::{User, UserRole, UserStatus};
    use crate::database::Database;
    use tempfile::tempdir;
    
    async fn create_test_handler() -> GroupMessageHandler {
        let temp_dir = tempdir().unwrap();
        let db_path = temp_dir.path().join("test.db");
        
        let mut db_config = crate::config::DatabaseConfig::default();
        db_config.path = db_path;
        
        let database = Database::new(&db_config).await.unwrap();
        database.migrate().await.unwrap();
        
        GroupMessageHandler::new(database)
    }
    
    async fn create_test_user(handler: &GroupMessageHandler, callsign: &str) -> User {
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
    async fn test_group_creation() {
        let handler = create_test_handler().await;
        let creator = create_test_user(&handler, "W1CREATE").await;
        
        let request = CreateGroupRequest {
            name: "Test Group".to_string(),
            description: Some("A test group for testing".to_string()),
            group_type: GroupType::Public,
            settings: GroupSettings::default(),
        };
        
        let group = handler.create_group(&creator, request).await.unwrap();
        
        assert_eq!(group.name, "Test Group");
        assert_eq!(group.group_type, GroupType::Public);
        assert_eq!(group.created_by, creator.id);
        assert_eq!(group.status, GroupStatus::Active);
    }
    
    #[tokio::test]
    async fn test_group_name_validation() {
        let handler = create_test_handler().await;
        let creator = create_test_user(&handler, "W1VALID").await;
        
        // Test empty name
        let empty_request = CreateGroupRequest {
            name: "".to_string(),
            description: None,
            group_type: GroupType::Public,
            settings: GroupSettings::default(),
        };
        
        let result = handler.create_group(&creator, empty_request).await;
        assert!(result.is_err());
        
        // Test name too long
        let long_request = CreateGroupRequest {
            name: "x".repeat(101),
            description: None,
            group_type: GroupType::Public,
            settings: GroupSettings::default(),
        };
        
        let result = handler.create_group(&creator, long_request).await;
        assert!(result.is_err());
        
        // Test invalid characters
        let invalid_request = CreateGroupRequest {
            name: "Test<Group>".to_string(),
            description: None,
            group_type: GroupType::Public,
            settings: GroupSettings::default(),
        };
        
        let result = handler.create_group(&creator, invalid_request).await;
        assert!(result.is_err());
    }
    
    #[tokio::test]
    async fn test_emergency_group_permissions() {
        let handler = create_test_handler().await;
        let regular_user = create_test_user(&handler, "W1USER").await;
        
        let mut admin_user = create_test_user(&handler, "W1ADMIN").await;
        admin_user.role = UserRole::Administrator;
        handler.database.create_user(&admin_user, "admin_hash").await.unwrap();
        
        // Regular user should not be able to create emergency group
        let emergency_request = CreateGroupRequest {
            name: "Emergency Test".to_string(),
            description: None,
            group_type: GroupType::Emergency,
            settings: GroupSettings::default(),
        };
        
        let result = handler.create_group(&regular_user, emergency_request.clone()).await;
        assert!(result.is_err());
        
        // Admin should be able to create emergency group
        let result = handler.create_group(&admin_user, emergency_request).await;
        assert!(result.is_ok());
    }
    
    #[tokio::test]
    async fn test_join_group() {
        let handler = create_test_handler().await;
        let creator = create_test_user(&handler, "W1CREATE").await;
        let joiner = create_test_user(&handler, "W1JOIN").await;
        
        // Create a public group
        let group_request = CreateGroupRequest {
            name: "Joinable Group".to_string(),
            description: None,
            group_type: GroupType::Public,
            settings: GroupSettings::default(),
        };
        
        let group = handler.create_group(&creator, group_request).await.unwrap();
        
        // Join the group
        let join_request = JoinGroupRequest {
            group_id: group.id,
            join_message: Some("Hello, I'd like to join!".to_string()),
        };
        
        let response = handler.join_group(&joiner, join_request).await.unwrap();
        
        assert!(response.success);
        assert!(response.membership.is_some());
        
        let membership = response.membership.unwrap();
        assert_eq!(membership.user_id, joiner.id);
        assert_eq!(membership.role, GroupRole::Member);
        assert_eq!(membership.status, MembershipStatus::Active);
    }
    
    #[tokio::test]
    async fn test_group_message_posting() {
        let handler = create_test_handler().await;
        let creator = create_test_user(&handler, "W1CREATE").await;
        let node_id = Uuid::new_v4();
        
        // Create a group
        let group_request = CreateGroupRequest {
            name: "Message Test Group".to_string(),
            description: None,
            group_type: GroupType::Public,
            settings: GroupSettings::default(),
        };
        
        let group = handler.create_group(&creator, group_request).await.unwrap();
        
        // Post a message
        let message_request = GroupMessageRequest {
            group_id: group.id,
            subject: Some("Test Subject".to_string()),
            body: "This is a test message to the group".to_string(),
            priority: MessagePriority::Normal,
            parent_id: None,
            message_type: MessageType::Group,
        };
        
        let message = handler.send_group_message(&creator, message_request, node_id).await.unwrap();
        
        assert_eq!(message.message_type, MessageType::Group);
        assert_eq!(message.sender_id, creator.id);
        assert_eq!(message.group_id, Some(group.id));
        assert_eq!(message.body, "This is a test message to the group");
    }
    
    #[tokio::test]
    async fn test_user_invitation() {
        let handler = create_test_handler().await;
        let creator = create_test_user(&handler, "W1CREATE").await;
        let invitee = create_test_user(&handler, "W1INVITE").await;
        
        // Create a private group
        let group_request = CreateGroupRequest {
            name: "Private Group".to_string(),
            description: None,
            group_type: GroupType::Private,
            settings: GroupSettings::default(),
        };
        
        let group = handler.create_group(&creator, group_request).await.unwrap();
        
        // Invite user by callsign
        let invite_request = InviteUserRequest {
            group_id: group.id,
            user_identifier: UserIdentifier::Callsign("W1INVITE".to_string()),
            role: GroupRole::Member,
            invitation_message: Some("Welcome to our private group!".to_string()),
        };
        
        let response = handler.invite_user(&creator, invite_request).await.unwrap();
        
        assert!(response.success);
        assert!(response.membership.is_some());
        
        let membership = response.membership.unwrap();
        assert_eq!(membership.user_id, invitee.id);
        assert_eq!(membership.status, MembershipStatus::Pending);
        assert_eq!(membership.invited_by, Some(creator.id));
    }
    
    #[tokio::test]
    async fn test_leave_group() {
        let handler = create_test_handler().await;
        let creator = create_test_user(&handler, "W1CREATE").await;
        let member = create_test_user(&handler, "W1MEMBER").await;
        
        // Create group and add member
        let group_request = CreateGroupRequest {
            name: "Leave Test Group".to_string(),
            description: None,
            group_type: GroupType::Public,
            settings: GroupSettings::default(),
        };
        
        let group = handler.create_group(&creator, group_request).await.unwrap();
        
        let join_request = JoinGroupRequest {
            group_id: group.id,
            join_message: None,
        };
        
        handler.join_group(&member, join_request).await.unwrap();
        
        // Member leaves group
        let response = handler.leave_group(&member, &group.id).await.unwrap();
        
        assert!(response.success);
        assert_eq!(response.message, "Successfully left group");
    }
    
    #[tokio::test]
    async fn test_owner_cannot_leave_alone() {
        let handler = create_test_handler().await;
        let creator = create_test_user(&handler, "W1OWNER").await;
        
        // Create group (creator becomes owner)
        let group_request = CreateGroupRequest {
            name: "Owner Test Group".to_string(),
            description: None,
            group_type: GroupType::Public,
            settings: GroupSettings::default(),
        };
        
        let group = handler.create_group(&creator, group_request).await.unwrap();
        
        // Owner tries to leave (should fail as only owner)
        let response = handler.leave_group(&creator, &group.id).await.unwrap();
        
        assert!(!response.success);
        assert!(response.message.contains("only owner"));
    }
    
    #[tokio::test]
    async fn test_group_cache() {
        let handler = create_test_handler().await;
        let creator = create_test_user(&handler, "W1CACHE").await;
        
        let group_request = CreateGroupRequest {
            name: "Cache Test Group".to_string(),
            description: None,
            group_type: GroupType::Public,
            settings: GroupSettings::default(),
        };
        
        let group = handler.create_group(&creator, group_request).await.unwrap();
        
        // First access should cache the group
        let cached_group = handler.get_group(&group.id).await.unwrap().unwrap();
        assert_eq!(cached_group.name, group.name);
        
        // Should get from cache on second access
        let cached_again = handler.get_cached_group(&group.id).await;
        assert!(cached_again.is_some());
        assert_eq!(cached_again.unwrap().name, group.name);
        
        // Invalidate cache
        handler.invalidate_group_cache(&group.id).await;
        
        // Should be gone from cache
        let after_invalidation = handler.get_cached_group(&group.id).await;
        assert!(after_invalidation.is_none());
    }
    
    #[tokio::test]
    async fn test_group_filtering() {
        let handler = create_test_handler().await;
        
        let groups = vec![
            Group {
                id: Uuid::new_v4(),
                name: "Public Group".to_string(),
                description: Some("A public group".to_string()),
                group_type: GroupType::Public,
                created_by: Uuid::new_v4(),
                created_at: SystemTime::now(),
                status: GroupStatus::Active,
                settings: GroupSettings::default(),
                stats: GroupStats::default(),
            },
            Group {
                id: Uuid::new_v4(),
                name: "Private Group".to_string(),
                description: Some("A private group".to_string()),
                group_type: GroupType::Private,
                created_by: Uuid::new_v4(),
                created_at: SystemTime::now(),
                status: GroupStatus::Active,
                settings: GroupSettings::default(),
                stats: GroupStats::default(),
            },
            Group {
                id: Uuid::new_v4(),
                name: "Emergency Group".to_string(),
                description: Some("An emergency group".to_string()),
                group_type: GroupType::Emergency,
                created_by: Uuid::new_v4(),
                created_at: SystemTime::now(),
                status: GroupStatus::Active,
                settings: GroupSettings::default(),
                stats: GroupStats::default(),
            },
        ];
        
        // Filter by type
        let query = GroupQuery {
            group_type: Some(GroupType::Public),
            status: None,
            search: None,
            joinable_only: false,
            user_id: None,
            limit: 10,
            offset: 0,
        };
        
        let filtered = handler.filter_groups(groups.clone(), &query).await;
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].group_type, GroupType::Public);
        
        // Search filter
        let search_query = GroupQuery {
            group_type: None,
            status: None,
            search: Some("emergency".to_string()),
            joinable_only: false,
            user_id: None,
            limit: 10,
            offset: 0,
        };
        
        let search_filtered = handler.filter_groups(groups, &search_query).await;
        assert_eq!(search_filtered.len(), 1);
        assert!(search_filtered[0].name.contains("Emergency"));
    }
}
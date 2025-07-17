use anyhow::Result;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use uuid::Uuid;

/// Time utilities for consistent timestamp handling
pub mod time {
    use super::*;
    
    /// Get current Unix timestamp in seconds
    pub fn unix_timestamp() -> i64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64
    }
    
    /// Convert SystemTime to Unix timestamp
    pub fn to_unix_timestamp(time: SystemTime) -> i64 {
        time.duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64
    }
    
    /// Convert Unix timestamp to SystemTime
    pub fn from_unix_timestamp(timestamp: i64) -> SystemTime {
        UNIX_EPOCH + Duration::from_secs(timestamp as u64)
    }
    
    /// Format duration as human readable string
    pub fn format_duration(duration: Duration) -> String {
        let secs = duration.as_secs();
        if secs < 60 {
            format!("{}s", secs)
        } else if secs < 3600 {
            format!("{}m {}s", secs / 60, secs % 60)
        } else if secs < 86400 {
            format!("{}h {}m", secs / 3600, (secs % 3600) / 60)
        } else {
            format!("{}d {}h", secs / 86400, (secs % 86400) / 3600)
        }
    }
    
    /// Format SystemTime as RFC3339 string
    pub fn format_rfc3339(time: SystemTime) -> String {
        use chrono::{DateTime, Utc};
        let datetime: DateTime<Utc> = time.into();
        datetime.to_rfc3339()
    }
}

/// Text processing utilities
pub mod text {
    /// Sanitize text for display (remove control characters)
    pub fn sanitize(input: &str) -> String {
        input.chars()
            .filter(|c| !c.is_control() || *c == '\n' || *c == '\t')
            .collect()
    }
    
    /// Truncate text to max length with ellipsis
    pub fn truncate(input: &str, max_len: usize) -> String {
        if input.len() <= max_len {
            input.to_string()
        } else {
            format!("{}...", &input[..max_len.saturating_sub(3)])
        }
    }
    
    /// Convert text to ASCII-safe format for RF transmission
    pub fn to_ascii_safe(input: &str) -> String {
        input.chars()
            .map(|c| if c.is_ascii() && !c.is_control() { c } else { '?' })
            .collect()
    }
    
    /// Word wrap text to specified width
    pub fn word_wrap(input: &str, width: usize) -> Vec<String> {
        let mut lines = Vec::new();
        let mut current_line = String::new();
        
        for word in input.split_whitespace() {
            if current_line.len() + word.len() + 1 > width {
                if !current_line.is_empty() {
                    lines.push(current_line);
                    current_line = String::new();
                }
            }
            
            if !current_line.is_empty() {
                current_line.push(' ');
            }
            current_line.push_str(word);
        }
        
        if !current_line.is_empty() {
            lines.push(current_line);
        }
        
        lines
    }
}

/// Callsign utilities
pub mod callsign {
    use regex::Regex;
    use once_cell::sync::Lazy;
    
    static CALLSIGN_REGEX: Lazy<Regex> = Lazy::new(|| {
        Regex::new(r"^[A-Z]{1,2}[0-9][A-Z]{1,3}(/[A-Z0-9]{1,4})?$").unwrap()
    });
    
    /// Normalize callsign to uppercase and validate format
    pub fn normalize(callsign: &str) -> Result<String, String> {
        let normalized = callsign.trim().to_uppercase();
        
        if CALLSIGN_REGEX.is_match(&normalized) {
            Ok(normalized)
        } else {
            Err(format!("Invalid callsign format: {}", callsign))
        }
    }
    
    /// Extract base callsign (remove portable/mobile suffixes)
    pub fn base_callsign(callsign: &str) -> String {
        callsign.split('/').next().unwrap_or(callsign).to_string()
    }
    
    /// Get country prefix from callsign
    pub fn country_prefix(callsign: &str) -> Option<String> {
        let base = base_callsign(callsign);
        
        // Extract letter prefix
        let prefix = base.chars()
            .take_while(|c| c.is_alphabetic())
            .collect::<String>();
        
        if prefix.is_empty() {
            None
        } else {
            Some(prefix)
        }
    }
}

/// Network utilities
pub mod network {
    use std::net::{IpAddr, SocketAddr};
    
    /// Check if IP address is in private range
    pub fn is_private_ip(ip: IpAddr) -> bool {
        match ip {
            IpAddr::V4(ipv4) => {
                let octets = ipv4.octets();
                // 10.0.0.0/8
                octets[0] == 10 ||
                // 172.16.0.0/12
                (octets[0] == 172 && octets[1] >= 16 && octets[1] <= 31) ||
                // 192.168.0.0/16
                (octets[0] == 192 && octets[1] == 168) ||
                // 127.0.0.0/8 (localhost)
                octets[0] == 127
            }
            IpAddr::V6(_) => {
                // Simplified IPv6 check
                ip.is_loopback() || ip.to_string().starts_with("fe80:")
            }
        }
    }
    
    /// Parse socket address with default port
    pub fn parse_socket_addr(addr: &str, default_port: u16) -> Result<SocketAddr, String> {
        if addr.contains(':') {
            addr.parse().map_err(|e| format!("Invalid address: {}", e))
        } else {
            format!("{}:{}", addr, default_port).parse()
                .map_err(|e| format!("Invalid address: {}", e))
        }
    }
}

/// Hashing utilities
pub mod hash {
    use blake3::Hasher;
    
    /// Generate Blake3 hash of data
    pub fn blake3_hash(data: &[u8]) -> String {
        let hash = blake3::hash(data);
        hex::encode(hash.as_bytes())
    }
    
    /// Generate Blake3 hash of string
    pub fn blake3_hash_str(data: &str) -> String {
        blake3_hash(data.as_bytes())
    }
    
    /// Generate content hash for message deduplication
    pub fn message_hash(sender: &str, content: &str, timestamp: i64) -> String {
        let mut hasher = Hasher::new();
        hasher.update(sender.as_bytes());
        hasher.update(content.as_bytes());
        hasher.update(&timestamp.to_le_bytes());
        hex::encode(hasher.finalize().as_bytes())
    }
}

/// Compression utilities
pub mod compression {
    use anyhow::Result;
    
    /// Compress data using LZ4
    pub fn compress_lz4(data: &[u8]) -> Result<Vec<u8>> {
        use lz4_flex::compress_prepend_size;
        Ok(compress_prepend_size(data))
    }
    
    /// Decompress LZ4 data
    pub fn decompress_lz4(data: &[u8]) -> Result<Vec<u8>> {
        use lz4_flex::decompress_size_prepended;
        decompress_size_prepended(data)
            .map_err(|e| anyhow::anyhow!("LZ4 decompression failed: {}", e))
    }
    
    /// Estimate compression ratio
    pub fn compression_ratio(original_size: usize, compressed_size: usize) -> f64 {
        if original_size == 0 {
            1.0
        } else {
            compressed_size as f64 / original_size as f64
        }
    }
}

/// File utilities
pub mod fs {
    use std::path::Path;
    use anyhow::Result;
    
    /// Ensure directory exists, create if needed
    pub async fn ensure_dir(path: &Path) -> Result<()> {
        if !path.exists() {
            tokio::fs::create_dir_all(path).await?;
        }
        Ok(())
    }
    
    /// Get file size safely
    pub async fn file_size(path: &Path) -> Result<u64> {
        let metadata = tokio::fs::metadata(path).await?;
        Ok(metadata.len())
    }
    
    /// Check if file exists and is readable
    pub async fn is_readable(path: &Path) -> bool {
        tokio::fs::File::open(path).await.is_ok()
    }
}

/// ID generation utilities
pub mod id {
    use super::*;
    
    /// Generate a new UUID v4
    pub fn new_uuid() -> Uuid {
        Uuid::new_v4()
    }
    
    /// Generate short ID (8 characters)
    pub fn short_id() -> String {
        new_uuid().to_string()[..8].to_string()
    }
    
    /// Generate message ID with timestamp prefix
    pub fn message_id() -> String {
        format!("msg_{}", short_id())
    }
    
    /// Generate thread ID
    pub fn thread_id() -> String {
        format!("thread_{}", short_id())
    }
}

/// Validation utilities
pub mod validation {
    /// Validate email address format
    pub fn is_valid_email(email: &str) -> bool {
        email.contains('@') && email.len() < 255 && !email.starts_with('@') && !email.ends_with('@')
    }
    
    /// Validate password strength
    pub fn validate_password(password: &str) -> Result<(), String> {
        if password.len() < 8 {
            return Err("Password must be at least 8 characters".to_string());
        }
        
        let has_upper = password.chars().any(|c| c.is_uppercase());
        let has_lower = password.chars().any(|c| c.is_lowercase());
        let has_digit = password.chars().any(|c| c.is_numeric());
        
        if !has_upper || !has_lower || !has_digit {
            return Err("Password must contain uppercase, lowercase, and digits".to_string());
        }
        
        Ok(())
    }
    
    /// Validate node description
    pub fn validate_description(desc: &str) -> Result<(), String> {
        if desc.is_empty() {
            return Err("Description cannot be empty".to_string());
        }
        
        if desc.len() > 200 {
            return Err("Description too long (max 200 characters)".to_string());
        }
        
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_callsign_normalization() {
        assert_eq!(callsign::normalize("w1aw").unwrap(), "W1AW");
        assert_eq!(callsign::normalize("K2ABC/P").unwrap(), "K2ABC/P");
        assert!(callsign::normalize("invalid").is_err());
    }
    
    #[test]
    fn test_text_utilities() {
        assert_eq!(text::truncate("Hello World", 5), "He...");
        assert_eq!(text::truncate("Hi", 5), "Hi");
        
        let lines = text::word_wrap("This is a test string", 10);
        assert!(lines.len() > 1);
    }
    
    #[test]
    fn test_time_formatting() {
        let dur = Duration::from_secs(3661);
        assert_eq!(time::format_duration(dur), "1h 1m");
    }
}

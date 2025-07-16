use anyhow::Result;
use argon2::{
    password_hash::{rand_core::OsRng, PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
    Argon2, Params, Version,
};
use serde::{Deserialize, Serialize};
use std::time::{Duration, SystemTime};
use zxcvbn::zxcvbn;

/// Password manager for secure password handling
/// 
/// Uses Argon2id for password hashing with configurable parameters.
/// Provides password strength validation and secure verification.
#[derive(Clone)]
pub struct PasswordManager {
    /// Argon2 hasher instance
    hasher: Argon2<'static>,
    
    /// Minimum password length
    min_length: usize,
    
    /// Maximum password length (to prevent DoS)
    max_length: usize,
    
    /// Require mixed case
    require_mixed_case: bool,
    
    /// Require numbers
    require_numbers: bool,
    
    /// Require special characters
    require_special_chars: bool,
    
    /// Minimum entropy score (0-4, from zxcvbn)
    min_entropy_score: u8,
}

/// Password validation result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PasswordValidation {
    /// Whether the password is valid
    pub valid: bool,
    
    /// Password strength score (0-4)
    pub strength_score: u8,
    
    /// Estimated time to crack
    pub crack_time_display: String,
    
    /// Validation errors
    pub errors: Vec<String>,
    
    /// Warnings (non-blocking issues)
    pub warnings: Vec<String>,
    
    /// Suggestions for improvement
    pub suggestions: Vec<String>,
}

/// Password hash information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PasswordHashInfo {
    /// The password hash
    pub hash: String,
    
    /// Hash algorithm used
    pub algorithm: String,
    
    /// Hash creation timestamp
    pub created_at: SystemTime,
    
    /// Hash parameters (for Argon2)
    pub params: HashParams,
}

/// Hash parameters for Argon2
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HashParams {
    /// Memory usage in KiB
    pub memory: u32,
    
    /// Number of iterations
    pub iterations: u32,
    
    /// Parallelism factor
    pub parallelism: u32,
    
    /// Hash length in bytes
    pub hash_length: u32,
    
    /// Argon2 variant
    pub variant: String,
}

/// Password policy configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PasswordPolicy {
    /// Minimum length
    pub min_length: usize,
    
    /// Maximum length
    pub max_length: usize,
    
    /// Require uppercase letters
    pub require_uppercase: bool,
    
    /// Require lowercase letters
    pub require_lowercase: bool,
    
    /// Require numbers
    pub require_numbers: bool,
    
    /// Require special characters
    pub require_special_chars: bool,
    
    /// Minimum entropy score
    pub min_entropy_score: u8,
    
    /// Disallowed passwords (common passwords, dictionary words)
    pub disallowed_passwords: Vec<String>,
    
    /// Maximum password age in days (0 = no expiration)
    pub max_age_days: u32,
}

impl PasswordManager {
    /// Create a new password manager with default settings
    pub fn new(min_length: usize) -> Self {
        // Configure Argon2 with secure parameters
        // These parameters provide good security while maintaining reasonable performance
        let params = Params::new(
            65536, // 64 MiB memory usage
            3,     // 3 iterations
            4,     // 4 threads parallelism
            Some(32), // 32-byte output length
        ).expect("Invalid Argon2 parameters");
        
        let hasher = Argon2::new_with_secret(
            &[], // No secret key
            argon2::Algorithm::Argon2id, // Use Argon2id variant
            Version::V0x13, // Latest version
            params,
        ).expect("Failed to create Argon2 hasher");
        
        Self {
            hasher,
            min_length,
            max_length: 128, // Reasonable maximum to prevent DoS
            require_mixed_case: true,
            require_numbers: true,
            require_special_chars: false, // Not required by default for ham radio users
            min_entropy_score: 2, // Moderate strength requirement
        }
    }
    
    /// Create password manager with custom policy
    pub fn with_policy(policy: PasswordPolicy) -> Self {
        let params = Params::new(
            65536, // 64 MiB memory usage
            3,     // 3 iterations  
            4,     // 4 threads parallelism
            Some(32), // 32-byte output length
        ).expect("Invalid Argon2 parameters");
        
        let hasher = Argon2::new_with_secret(
            &[],
            argon2::Algorithm::Argon2id,
            Version::V0x13,
            params,
        ).expect("Failed to create Argon2 hasher");
        
        Self {
            hasher,
            min_length: policy.min_length,
            max_length: policy.max_length,
            require_mixed_case: policy.require_uppercase && policy.require_lowercase,
            require_numbers: policy.require_numbers,
            require_special_chars: policy.require_special_chars,
            min_entropy_score: policy.min_entropy_score,
        }
    }
    
    /// Hash a password using Argon2id
    pub fn hash_password(&self, password: &str) -> Result<String> {
        // Generate a random salt
        let salt = SaltString::generate(&mut OsRng);
        
        // Hash the password
        let password_hash = self.hasher
            .hash_password(password.as_bytes(), &salt)
            .map_err(|e| anyhow::anyhow!("Password hashing failed: {}", e))?;
        
        Ok(password_hash.to_string())
    }
    
    /// Verify a password against its hash
    pub fn verify_password(&self, password: &str, hash: &str) -> Result<bool> {
        // Parse the hash
        let parsed_hash = PasswordHash::new(hash)
            .map_err(|e| anyhow::anyhow!("Invalid password hash format: {}", e))?;
        
        // Verify the password
        match self.hasher.verify_password(password.as_bytes(), &parsed_hash) {
            Ok(()) => Ok(true),
            Err(argon2::password_hash::Error::Password) => Ok(false),
            Err(e) => Err(anyhow::anyhow!("Password verification failed: {}", e)),
        }
    }
    
    /// Validate password strength and policy compliance
    pub fn validate_password(&self, password: &str) -> Result<PasswordValidation> {
        let mut errors = Vec::new();
        let mut warnings = Vec::new();
        let mut suggestions = Vec::new();
        
        // Check length requirements
        if password.len() < self.min_length {
            errors.push(format!("Password must be at least {} characters long", self.min_length));
        }
        
        if password.len() > self.max_length {
            errors.push(format!("Password must be no more than {} characters long", self.max_length));
        }
        
        // Check character requirements
        let has_uppercase = password.chars().any(|c| c.is_uppercase());
        let has_lowercase = password.chars().any(|c| c.is_lowercase());
        let has_numbers = password.chars().any(|c| c.is_numeric());
        let has_special = password.chars().any(|c| "!@#$%^&*()_+-=[]{}|;:,.<>?".contains(c));
        
        if self.require_mixed_case {
            if !has_uppercase {
                errors.push("Password must contain at least one uppercase letter".to_string());
            }
            if !has_lowercase {
                errors.push("Password must contain at least one lowercase letter".to_string());
            }
        }
        
        if self.require_numbers && !has_numbers {
            errors.push("Password must contain at least one number".to_string());
        }
        
        if self.require_special_chars && !has_special {
            errors.push("Password must contain at least one special character".to_string());
        }
        
        // Check for common weak patterns
        if password.to_lowercase().contains("password") {
            errors.push("Password should not contain the word 'password'".to_string());
        }
        
        if password.chars().all(|c| c.is_numeric()) {
            errors.push("Password should not be entirely numeric".to_string());
        }
        
        // Check for keyboard patterns
        if self.is_keyboard_pattern(password) {
            warnings.push("Password appears to follow a keyboard pattern".to_string());
            suggestions.push("Avoid common keyboard patterns like 'qwerty' or '123456'".to_string());
        }
        
        // Check for repetitive characters
        if self.has_repetitive_chars(password) {
            warnings.push("Password contains repetitive characters".to_string());
            suggestions.push("Avoid repeating the same character multiple times".to_string());
        }
        
        // Use zxcvbn for entropy analysis
        let entropy_result = zxcvbn(password, &[]);
        let strength_score = entropy_result.score();
        
        if strength_score < self.min_entropy_score {
            errors.push(format!(
                "Password strength is too low (score: {}/4, required: {}/4)",
                strength_score, self.min_entropy_score
            ));
        }
        
        // Add zxcvbn suggestions
        for suggestion in entropy_result.feedback().suggestions() {
            suggestions.push(suggestion.to_string());
        }
        
        // Add zxcvbn warnings
        if let Some(warning) = entropy_result.feedback().warning() {
            warnings.push(warning.to_string());
        }
        
        // Provide constructive suggestions
        if !has_uppercase && !errors.iter().any(|e| e.contains("uppercase")) {
            suggestions.push("Consider adding uppercase letters for better security".to_string());
        }
        
        if !has_numbers && !errors.iter().any(|e| e.contains("number")) {
            suggestions.push("Consider adding numbers for better security".to_string());
        }
        
        if !has_special && password.len() < 12 {
            suggestions.push("Consider adding special characters or making the password longer".to_string());
        }
        
        Ok(PasswordValidation {
            valid: errors.is_empty(),
            strength_score,
            crack_time_display: entropy_result.crack_times().offline_slow_hashing_1e4_per_second().to_string(),
            errors,
            warnings,
            suggestions,
        })
    }
    
    /// Check if password needs to be updated due to policy changes
    pub fn needs_rehash(&self, hash: &str) -> Result<bool> {
        match PasswordHash::new(hash) {
            Ok(parsed_hash) => {
                // Check if hash uses current algorithm and parameters
                let current_params = self.hasher.params();
                
                // For Argon2, we can check the parameters in the hash
                if let Some(hash_params) = parsed_hash.params {
                    let memory_ok = hash_params.get("m")
                        .and_then(|v| v.decimal())
                        .map(|m| m >= current_params.m_cost())
                        .unwrap_or(false);
                    
                    let iterations_ok = hash_params.get("t")
                        .and_then(|v| v.decimal())
                        .map(|t| t >= current_params.t_cost())
                        .unwrap_or(false);
                    
                    let parallelism_ok = hash_params.get("p")
                        .and_then(|v| v.decimal())
                        .map(|p| p >= current_params.p_cost())
                        .unwrap_or(false);
                    
                    Ok(!(memory_ok && iterations_ok && parallelism_ok))
                } else {
                    // If we can't parse parameters, assume rehash is needed
                    Ok(true)
                }
            }
            Err(_) => {
                // Invalid hash format, definitely needs rehash
                Ok(true)
            }
        }
    }
    
    /// Generate a secure random password
    pub fn generate_password(&self, length: usize, include_special: bool) -> String {
        use rand::seq::SliceRandom;
        use rand::thread_rng;
        
        let mut chars = Vec::new();
        
        // Ensure we have all required character types
        chars.extend_from_slice(b"ABCDEFGHIJKLMNOPQRSTUVWXYZ");
        chars.extend_from_slice(b"abcdefghijklmnopqrstuvwxyz");
        chars.extend_from_slice(b"0123456789");
        
        if include_special {
            chars.extend_from_slice(b"!@#$%^&*");
        }
        
        let mut rng = thread_rng();
        let password: String = (0..length)
            .map(|_| *chars.choose(&mut rng).unwrap() as char)
            .collect();
        
        password
    }
    
    /// Get password hash information
    pub fn get_hash_info(&self, hash: &str) -> Result<PasswordHashInfo> {
        let parsed_hash = PasswordHash::new(hash)
            .map_err(|e| anyhow::anyhow!("Invalid password hash format: {}", e))?;
        
        let algorithm = parsed_hash.algorithm.to_string();
        
        // Extract Argon2 parameters
        let params = if let Some(hash_params) = parsed_hash.params {
            HashParams {
                memory: hash_params.get("m")
                    .and_then(|v| v.decimal())
                    .unwrap_or(0),
                iterations: hash_params.get("t")
                    .and_then(|v| v.decimal())
                    .unwrap_or(0),
                parallelism: hash_params.get("p")
                    .and_then(|v| v.decimal())
                    .unwrap_or(0),
                hash_length: 32, // Argon2 default
                variant: algorithm.clone(),
            }
        } else {
            HashParams {
                memory: 0,
                iterations: 0,
                parallelism: 0,
                hash_length: 0,
                variant: algorithm.clone(),
            }
        };
        
        Ok(PasswordHashInfo {
            hash: hash.to_string(),
            algorithm,
            created_at: SystemTime::now(), // We don't store creation time in hash
            params,
        })
    }
    
    /// Check for common keyboard patterns
    fn is_keyboard_pattern(&self, password: &str) -> bool {
        let common_patterns = [
            "qwerty", "qwertyuiop", "asdf", "asdfghjkl", "zxcv", "zxcvbnm",
            "1234", "12345", "123456", "1234567", "12345678", "123456789",
            "abcd", "abcde", "abcdef", "abcdefg",
        ];
        
        let lower_password = password.to_lowercase();
        for pattern in &common_patterns {
            if lower_password.contains(pattern) {
                return true;
            }
        }
        
        false
    }
    
    /// Check for repetitive characters
    fn has_repetitive_chars(&self, password: &str) -> bool {
        let chars: Vec<char> = password.chars().collect();
        let mut consecutive_count = 1;
        
        for i in 1..chars.len() {
            if chars[i] == chars[i-1] {
                consecutive_count += 1;
                if consecutive_count >= 3 {
                    return true;
                }
            } else {
                consecutive_count = 1;
            }
        }
        
        false
    }
}

impl Default for PasswordPolicy {
    fn default() -> Self {
        Self {
            min_length: 8,
            max_length: 128,
            require_uppercase: true,
            require_lowercase: true,
            require_numbers: true,
            require_special_chars: false,
            min_entropy_score: 2,
            disallowed_passwords: vec![
                "password".to_string(),
                "123456".to_string(),
                "qwerty".to_string(),
                "admin".to_string(),
                "letmein".to_string(),
                "welcome".to_string(),
            ],
            max_age_days: 0, // No expiration by default
        }
    }
}

/// Simple password hashing function (convenience wrapper)
pub fn hash_password(password: &str) -> Result<String> {
    let manager = PasswordManager::new(8);
    manager.hash_password(password)
}

/// Simple password verification function (convenience wrapper)
pub fn verify_password(password: &str, hash: &str) -> Result<bool> {
    let manager = PasswordManager::new(8);
    manager.verify_password(password, hash)
}

/// Simple password validation function (convenience wrapper)
pub fn validate_password_strength(password: &str) -> Result<PasswordValidation> {
    let manager = PasswordManager::new(8);
    manager.validate_password(password)
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_password_hashing() {
        let manager = PasswordManager::new(8);
        let password = "TestPassword123!";
        
        let hash = manager.hash_password(password).unwrap();
        assert!(!hash.is_empty());
        assert!(hash.starts_with("$argon2id$"));
        
        // Verify correct password
        assert!(manager.verify_password(password, &hash).unwrap());
        
        // Verify incorrect password
        assert!(!manager.verify_password("WrongPassword", &hash).unwrap());
    }
    
    #[test]
    fn test_password_validation() {
        let manager = PasswordManager::new(8);
        
        // Test weak password
        let weak_result = manager.validate_password("123456").unwrap();
        assert!(!weak_result.valid);
        assert!(!weak_result.errors.is_empty());
        
        // Test strong password
        let strong_result = manager.validate_password("MyStr0ngP@ssw0rd!").unwrap();
        assert!(strong_result.valid);
        assert!(strong_result.strength_score >= 3);
        
        // Test medium password
        let medium_result = manager.validate_password("TestPassword123").unwrap();
        assert!(medium_result.valid);
    }
    
    #[test]
    fn test_password_policy() {
        let policy = PasswordPolicy {
            min_length: 12,
            require_special_chars: true,
            min_entropy_score: 3,
            ..Default::default()
        };
        
        let manager = PasswordManager::with_policy(policy);
        
        // This should fail due to length and missing special chars
        let result = manager.validate_password("Password123").unwrap();
        assert!(!result.valid);
        
        // This should pass
        let result = manager.validate_password("MyVeryStrongPassword123!").unwrap();
        assert!(result.valid);
    }
    
    #[test]
    fn test_keyboard_patterns() {
        let manager = PasswordManager::new(8);
        
        assert!(manager.is_keyboard_pattern("qwerty123"));
        assert!(manager.is_keyboard_pattern("123456789"));
        assert!(manager.is_keyboard_pattern("asdfgh"));
        assert!(!manager.is_keyboard_pattern("RandomPass123"));
    }
    
    #[test]
    fn test_repetitive_characters() {
        let manager = PasswordManager::new(8);
        
        assert!(manager.has_repetitive_chars("aaa123"));
        assert!(manager.has_repetitive_chars("Pass111word"));
        assert!(!manager.has_repetitive_chars("Password123"));
    }
    
    #[test]
    fn test_password_generation() {
        let manager = PasswordManager::new(8);
        
        let password = manager.generate_password(12, true);
        assert_eq!(password.len(), 12);
        
        // Generated password should pass validation
        let validation = manager.validate_password(&password).unwrap();
        assert!(validation.valid);
    }
    
    #[test]
    fn test_hash_info() {
        let manager = PasswordManager::new(8);
        let password = "TestPassword123!";
        let hash = manager.hash_password(password).unwrap();
        
        let info = manager.get_hash_info(&hash).unwrap();
        assert_eq!(info.algorithm, "argon2id");
        assert!(info.params.memory > 0);
        assert!(info.params.iterations > 0);
        assert!(info.params.parallelism > 0);
    }
    
    #[test]
    fn test_needs_rehash() {
        let manager = PasswordManager::new(8);
        let password = "TestPassword123!";
        let hash = manager.hash_password(password).unwrap();
        
        // Newly created hash should not need rehashing
        assert!(!manager.needs_rehash(&hash).unwrap());
        
        // Invalid hash should need rehashing
        assert!(manager.needs_rehash("invalid_hash").unwrap());
    }
    
    #[test]
    fn test_convenience_functions() {
        let password = "TestPassword123!";
        
        let hash = hash_password(password).unwrap();
        assert!(verify_password(password, &hash).unwrap());
        assert!(!verify_password("WrongPassword", &hash).unwrap());
        
        let validation = validate_password_strength(password).unwrap();
        assert!(validation.valid);
    }
}
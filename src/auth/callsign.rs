use anyhow::Result;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::SystemTime;
use once_cell::sync::Lazy;

use crate::database::Database;

/// Callsign validator for amateur radio callsigns worldwide
#[derive(Clone)]
pub struct CallsignValidator {
    database: Database,
    cached_prefixes: HashMap<String, CountryInfo>,
    last_cache_update: SystemTime,
}

/// Country/territory information for callsign prefixes
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CountryInfo {
    /// Country name
    pub name: String,
    
    /// ITU region (1, 2, or 3)
    pub itu_region: u8,
    
    /// CQ zone
    pub cq_zone: u8,
    
    /// Continent code
    pub continent: String,
    
    /// Country code (ISO 3166-1 alpha-2)
    pub country_code: String,
    
    /// Licensing authority
    pub authority: String,
    
    /// Common prefixes for this country
    pub prefixes: Vec<String>,
}

/// License information from database lookup
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LicenseInfo {
    /// Callsign
    pub callsign: String,
    
    /// License holder name
    pub name: Option<String>,
    
    /// License class
    pub license_class: Option<String>,
    
    /// License status
    pub status: LicenseStatus,
    
    /// Expiration date (if available)
    pub expires: Option<SystemTime>,
    
    /// Address information (if available)
    pub address: Option<AddressInfo>,
    
    /// Country of license
    pub country: String,
    
    /// Verification timestamp
    pub verified_at: SystemTime,
}

/// License status
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum LicenseStatus {
    /// Active license
    Active,
    
    /// Expired license
    Expired,
    
    /// Canceled license
    Canceled,
    
    /// Suspended license
    Suspended,
    
    /// Unknown status
    Unknown,
}

/// Address information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AddressInfo {
    pub street: Option<String>,
    pub city: Option<String>,
    pub state: Option<String>,
    pub postal_code: Option<String>,
    pub country: String,
    pub grid_square: Option<String>,
}

/// Callsign validation result
#[derive(Debug, Clone)]
pub struct ValidationResult {
    /// Whether the callsign format is valid
    pub valid_format: bool,
    
    /// Whether the callsign exists in license database
    pub verified: bool,
    
    /// Country information
    pub country: Option<CountryInfo>,
    
    /// License information (if found)
    pub license: Option<LicenseInfo>,
    
    /// Validation errors
    pub errors: Vec<String>,
}

// Regular expressions for different callsign formats
static US_CALLSIGN: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^[AKNW][A-Z]?[0-9][A-Z]{1,3}$").unwrap()
});

static CANADIAN_CALLSIGN: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^V[A-Z][0-9][A-Z]{1,3}$").unwrap()
});

static UK_CALLSIGN: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^[GM]([0-9][A-Z]{2,3}|[A-Z][0-9][A-Z]{1,3})$").unwrap()
});

static GERMAN_CALLSIGN: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^D[A-K][0-9][A-Z]{1,3}$").unwrap()
});

static JAPANESE_CALLSIGN: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^J[A-S][0-9][A-Z]{1,3}$").unwrap()
});

static AUSTRALIAN_CALLSIGN: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^VK[0-9][A-Z]{1,3}$").unwrap()
});

// Generic international callsign pattern (ITU format)
static INTERNATIONAL_CALLSIGN: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^[A-Z]{1,2}[0-9][A-Z]{1,4}$").unwrap()
});

// Special callsigns (contest, special event, etc.)
static SPECIAL_CALLSIGN: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^[A-Z0-9]{3,7}(/[A-Z0-9]{1,4})?$").unwrap()
});

impl CallsignValidator {
    /// Create a new callsign validator
    pub async fn new(database: Database) -> Result<Self> {
        let mut validator = Self {
            database,
            cached_prefixes: HashMap::new(),
            last_cache_update: SystemTime::UNIX_EPOCH,
        };
        
        // Load country prefix data
        validator.load_country_prefixes().await?;
        
        Ok(validator)
    }
    
    /// Validate a callsign format and optionally verify against license database
    pub async fn validate(&self, callsign: &str) -> Result<ValidationResult> {
        let callsign = callsign.to_uppercase().trim().to_string();
        
        // Basic format validation
        let format_result = self.validate_format(&callsign);
        
        if !format_result.valid_format {
            return Ok(format_result);
        }
        
        // Country identification
        let country = self.identify_country(&callsign);
        
        // License database verification (if enabled and available)
        let license = self.verify_license(&callsign).await.ok();
        
        Ok(ValidationResult {
            valid_format: true,
            verified: license.is_some(),
            country,
            license,
            errors: Vec::new(),
        })
    }
    
    /// Verify callsign against license database
    pub async fn verify_callsign(&self, callsign: &str) -> Result<bool> {
        let result = self.validate(callsign).await?;
        Ok(result.verified)
    }
    
    /// Get license information for a callsign
    pub async fn get_license_info(&self, callsign: &str) -> Result<Option<LicenseInfo>> {
        let callsign = callsign.to_uppercase();
        self.verify_license(&callsign).await.ok()
    }
    
    /// Validate callsign format only (no database lookup)
    fn validate_format(&self, callsign: &str) -> ValidationResult {
        let mut errors = Vec::new();
        
        // Check length
        if callsign.len() < 3 || callsign.len() > 7 {
            errors.push("Callsign must be 3-7 characters".to_string());
        }
        
        // Check for invalid characters
        if !callsign.chars().all(|c| c.is_ascii_alphanumeric()) {
            errors.push("Callsign contains invalid characters".to_string());
        }
        
        // Must contain at least one number
        if !callsign.chars().any(|c| c.is_ascii_digit()) {
            errors.push("Callsign must contain at least one digit".to_string());
        }
        
        // Must start with a letter
        if !callsign.chars().next().map_or(false, |c| c.is_ascii_alphabetic()) {
            errors.push("Callsign must start with a letter".to_string());
        }
        
        if !errors.is_empty() {
            return ValidationResult {
                valid_format: false,
                verified: false,
                country: None,
                license: None,
                errors,
            };
        }
        
        // Check specific country patterns
        let valid_format = self.check_country_patterns(callsign);
        
        ValidationResult {
            valid_format,
            verified: false,
            country: None,
            license: None,
            errors: if valid_format {
                Vec::new()
            } else {
                vec!["Invalid callsign format for known regions".to_string()]
            },
        }
    }
    
    /// Check callsign against known country patterns
    fn check_country_patterns(&self, callsign: &str) -> bool {
        // Check specific country patterns first
        if US_CALLSIGN.is_match(callsign) ||
           CANADIAN_CALLSIGN.is_match(callsign) ||
           UK_CALLSIGN.is_match(callsign) ||
           GERMAN_CALLSIGN.is_match(callsign) ||
           JAPANESE_CALLSIGN.is_match(callsign) ||
           AUSTRALIAN_CALLSIGN.is_match(callsign) {
            return true;
        }
        
        // Check generic international pattern
        if INTERNATIONAL_CALLSIGN.is_match(callsign) {
            return true;
        }
        
        // Check special callsigns
        if SPECIAL_CALLSIGN.is_match(callsign) {
            return true;
        }
        
        false
    }
    
    /// Identify country from callsign prefix
    fn identify_country(&self, callsign: &str) -> Option<CountryInfo> {
        // Try different prefix lengths (2 chars, then 1 char)
        for prefix_len in [2, 1] {
            if callsign.len() >= prefix_len {
                let prefix = &callsign[..prefix_len];
                if let Some(country) = self.cached_prefixes.get(prefix) {
                    return Some(country.clone());
                }
            }
        }
        
        // Try to match based on known patterns
        if US_CALLSIGN.is_match(callsign) {
            return Some(CountryInfo {
                name: "United States".to_string(),
                itu_region: 2,
                cq_zone: 4, // Most common
                continent: "NA".to_string(),
                country_code: "US".to_string(),
                authority: "FCC".to_string(),
                prefixes: vec!["A".to_string(), "K".to_string(), "N".to_string(), "W".to_string()],
            });
        }
        
        if CANADIAN_CALLSIGN.is_match(callsign) {
            return Some(CountryInfo {
                name: "Canada".to_string(),
                itu_region: 2,
                cq_zone: 1, // Most common
                continent: "NA".to_string(),
                country_code: "CA".to_string(),
                authority: "ISED".to_string(),
                prefixes: vec!["VA".to_string(), "VB".to_string(), "VC".to_string(), 
                              "VD".to_string(), "VE".to_string(), "VF".to_string(),
                              "VG".to_string(), "VH".to_string(), "VI".to_string(),
                              "VJ".to_string(), "VK".to_string(), "VL".to_string(),
                              "VM".to_string(), "VN".to_string(), "VO".to_string(),
                              "VP".to_string(), "VQ".to_string(), "VR".to_string(),
                              "VS".to_string(), "VT".to_string(), "VU".to_string(),
                              "VV".to_string(), "VW".to_string(), "VX".to_string(),
                              "VY".to_string(), "VZ".to_string()],
            });
        }
        
        None
    }
    
    /// Verify callsign against license database
    async fn verify_license(&self, callsign: &str) -> Result<LicenseInfo> {
        // First check our local database cache
        if let Some(cached) = self.database.get_cached_license_info(callsign).await? {
            // Check if cache is still fresh (within 30 days)
            if cached.verified_at + std::time::Duration::from_secs(30 * 24 * 3600) > SystemTime::now() {
                return Ok(cached);
            }
        }
        
        // Try to fetch from external sources
        let license_info = self.fetch_license_info(callsign).await?;
        
        // Cache the result
        self.database.cache_license_info(&license_info).await?;
        
        Ok(license_info)
    }
    
    /// Fetch license information from external sources
    async fn fetch_license_info(&self, callsign: &str) -> Result<LicenseInfo> {
        // Identify country to determine which database to query
        let country = self.identify_country(callsign);
        
        match country.as_ref().map(|c| c.country_code.as_str()) {
            Some("US") => self.fetch_fcc_info(callsign).await,
            Some("CA") => self.fetch_ised_info(callsign).await,
            Some("UK") | Some("GB") => self.fetch_ofcom_info(callsign).await,
            Some("DE") => self.fetch_bundesnetzagentur_info(callsign).await,
            Some("JP") => self.fetch_mic_info(callsign).await,
            Some("AU") => self.fetch_acma_info(callsign).await,
            _ => {
                // Try generic lookup or return unknown
                Ok(LicenseInfo {
                    callsign: callsign.to_string(),
                    name: None,
                    license_class: None,
                    status: LicenseStatus::Unknown,
                    expires: None,
                    address: None,
                    country: country.map(|c| c.name).unwrap_or_else(|| "Unknown".to_string()),
                    verified_at: SystemTime::now(),
                })
            }
        }
    }
    
    /// Fetch license info from FCC database (US)
    async fn fetch_fcc_info(&self, callsign: &str) -> Result<LicenseInfo> {
        // This would integrate with FCC ULS database
        // For now, return a placeholder implementation
        
        // In a real implementation, this would:
        // 1. Query FCC ULS API or database export
        // 2. Parse the response
        // 3. Extract license information
        
        Ok(LicenseInfo {
            callsign: callsign.to_string(),
            name: Some("Example Name".to_string()), // Would come from FCC data
            license_class: Some("General".to_string()),
            status: LicenseStatus::Active,
            expires: Some(SystemTime::now() + std::time::Duration::from_secs(365 * 24 * 3600)),
            address: Some(AddressInfo {
                street: Some("123 Example St".to_string()),
                city: Some("Anytown".to_string()),
                state: Some("ST".to_string()),
                postal_code: Some("12345".to_string()),
                country: "United States".to_string(),
                grid_square: Some("FN42".to_string()),
            }),
            country: "United States".to_string(),
            verified_at: SystemTime::now(),
        })
    }
    
    /// Fetch license info from ISED database (Canada)
    async fn fetch_ised_info(&self, callsign: &str) -> Result<LicenseInfo> {
        // Placeholder for Industry Canada ISED database integration
        Ok(LicenseInfo {
            callsign: callsign.to_string(),
            name: None,
            license_class: None,
            status: LicenseStatus::Unknown,
            expires: None,
            address: None,
            country: "Canada".to_string(),
            verified_at: SystemTime::now(),
        })
    }
    
    /// Fetch license info from Ofcom database (UK)
    async fn fetch_ofcom_info(&self, callsign: &str) -> Result<LicenseInfo> {
        // Placeholder for UK Ofcom database integration
        Ok(LicenseInfo {
            callsign: callsign.to_string(),
            name: None,
            license_class: None,
            status: LicenseStatus::Unknown,
            expires: None,
            address: None,
            country: "United Kingdom".to_string(),
            verified_at: SystemTime::now(),
        })
    }
    
    /// Fetch license info from Bundesnetzagentur database (Germany)
    async fn fetch_bundesnetzagentur_info(&self, callsign: &str) -> Result<LicenseInfo> {
        // Placeholder for German BNetzA database integration
        Ok(LicenseInfo {
            callsign: callsign.to_string(),
            name: None,
            license_class: None,
            status: LicenseStatus::Unknown,
            expires: None,
            address: None,
            country: "Germany".to_string(),
            verified_at: SystemTime::now(),
        })
    }
    
    /// Fetch license info from MIC database (Japan)
    async fn fetch_mic_info(&self, callsign: &str) -> Result<LicenseInfo> {
        // Placeholder for Japan MIC database integration
        Ok(LicenseInfo {
            callsign: callsign.to_string(),
            name: None,
            license_class: None,
            status: LicenseStatus::Unknown,
            expires: None,
            address: None,
            country: "Japan".to_string(),
            verified_at: SystemTime::now(),
        })
    }
    
    /// Fetch license info from ACMA database (Australia)
    async fn fetch_acma_info(&self, callsign: &str) -> Result<LicenseInfo> {
        // Placeholder for Australian ACMA database integration
        Ok(LicenseInfo {
            callsign: callsign.to_string(),
            name: None,
            license_class: None,
            status: LicenseStatus::Unknown,
            expires: None,
            address: None,
            country: "Australia".to_string(),
            verified_at: SystemTime::now(),
        })
    }
    
    /// Load country prefix data into cache
    async fn load_country_prefixes(&mut self) -> Result<()> {
        // This would load from database or embedded data
        // For now, add some common prefixes
        
        let prefixes = vec![
            ("A", "United States"),
            ("K", "United States"),
            ("N", "United States"),
            ("W", "United States"),
            ("VA", "Canada"),
            ("VB", "Canada"),
            ("VC", "Canada"),
            ("VD", "Canada"),
            ("VE", "Canada"),
            ("VF", "Canada"),
            ("VG", "Canada"),
            ("VH", "Canada"),
            ("VI", "Canada"),
            ("VJ", "Canada"),
            ("VK", "Australia"),
            ("VL", "Australia"),
            ("VM", "Australia"),
            ("VN", "Australia"),
            ("VO", "Canada"),
            ("VP", "Various"),
            ("VQ", "Various"),
            ("VR", "Various"),
            ("VS", "Various"),
            ("VT", "India"),
            ("VU", "India"),
            ("VV", "Canada"),
            ("VW", "Canada"),
            ("VX", "Canada"),
            ("VY", "Canada"),
            ("VZ", "Canada"),
            ("G", "United Kingdom"),
            ("M", "United Kingdom"),
            ("D", "Germany"),
            ("J", "Japan"),
        ];
        
        for (prefix, country) in prefixes {
            let country_info = match country {
                "United States" => CountryInfo {
                    name: "United States".to_string(),
                    itu_region: 2,
                    cq_zone: 4,
                    continent: "NA".to_string(),
                    country_code: "US".to_string(),
                    authority: "FCC".to_string(),
                    prefixes: vec![prefix.to_string()],
                },
                "Canada" => CountryInfo {
                    name: "Canada".to_string(),
                    itu_region: 2,
                    cq_zone: 1,
                    continent: "NA".to_string(),
                    country_code: "CA".to_string(),
                    authority: "ISED".to_string(),
                    prefixes: vec![prefix.to_string()],
                },
                "Australia" => CountryInfo {
                    name: "Australia".to_string(),
                    itu_region: 3,
                    cq_zone: 29,
                    continent: "OC".to_string(),
                    country_code: "AU".to_string(),
                    authority: "ACMA".to_string(),
                    prefixes: vec![prefix.to_string()],
                },
                "United Kingdom" => CountryInfo {
                    name: "United Kingdom".to_string(),
                    itu_region: 1,
                    cq_zone: 14,
                    continent: "EU".to_string(),
                    country_code: "GB".to_string(),
                    authority: "Ofcom".to_string(),
                    prefixes: vec![prefix.to_string()],
                },
                "Germany" => CountryInfo {
                    name: "Germany".to_string(),
                    itu_region: 1,
                    cq_zone: 14,
                    continent: "EU".to_string(),
                    country_code: "DE".to_string(),
                    authority: "BNetzA".to_string(),
                    prefixes: vec![prefix.to_string()],
                },
                "Japan" => CountryInfo {
                    name: "Japan".to_string(),
                    itu_region: 3,
                    cq_zone: 25,
                    continent: "AS".to_string(),
                    country_code: "JP".to_string(),
                    authority: "MIC".to_string(),
                    prefixes: vec![prefix.to_string()],
                },
                "India" => CountryInfo {
                    name: "India".to_string(),
                    itu_region: 3,
                    cq_zone: 22,
                    continent: "AS".to_string(),
                    country_code: "IN".to_string(),
                    authority: "WPC".to_string(),
                    prefixes: vec![prefix.to_string()],
                },
                _ => continue,
            };
            
            self.cached_prefixes.insert(prefix.to_string(), country_info);
        }
        
        self.last_cache_update = SystemTime::now();
        Ok(())
    }
    
    /// Update country prefix cache from database
    pub async fn update_prefix_cache(&mut self) -> Result<()> {
        self.load_country_prefixes().await
    }
    
    /// Get list of supported countries
    pub fn get_supported_countries(&self) -> Vec<CountryInfo> {
        self.cached_prefixes.values().cloned().collect()
    }
}

/// Simple callsign format validation (for use without database)
pub fn validate_callsign(callsign: &str) -> Result<(), String> {
    let callsign = callsign.to_uppercase().trim();
    
    // Check length
    if callsign.len() < 3 || callsign.len() > 7 {
        return Err("Callsign must be 3-7 characters".to_string());
    }
    
    // Check for invalid characters
    if !callsign.chars().all(|c| c.is_ascii_alphanumeric()) {
        return Err("Callsign contains invalid characters".to_string());
    }
    
    // Must contain at least one number
    if !callsign.chars().any(|c| c.is_ascii_digit()) {
        return Err("Callsign must contain at least one digit".to_string());
    }
    
    // Must start with a letter
    if !callsign.chars().next().map_or(false, |c| c.is_ascii_alphabetic()) {
        return Err("Callsign must start with a letter".to_string());
    }
    
    // Check basic patterns
    if !US_CALLSIGN.is_match(callsign) &&
       !CANADIAN_CALLSIGN.is_match(callsign) &&
       !UK_CALLSIGN.is_match(callsign) &&
       !GERMAN_CALLSIGN.is_match(callsign) &&
       !JAPANESE_CALLSIGN.is_match(callsign) &&
       !AUSTRALIAN_CALLSIGN.is_match(callsign) &&
       !INTERNATIONAL_CALLSIGN.is_match(callsign) &&
       !SPECIAL_CALLSIGN.is_match(callsign) {
        return Err("Invalid callsign format for known regions".to_string());
    }
    
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_us_callsigns() {
        assert!(validate_callsign("W1AW").is_ok());
        assert!(validate_callsign("K2ABC").is_ok());
        assert!(validate_callsign("N1XYZ").is_ok());
        assert!(validate_callsign("AA0A").is_ok());
        assert!(validate_callsign("KC1DEF").is_ok());
    }
    
    #[test]
    fn test_canadian_callsigns() {
        assert!(validate_callsign("VE3ABC").is_ok());
        assert!(validate_callsign("VA1XYZ").is_ok());
        assert!(validate_callsign("VO1DEF").is_ok());
    }
    
    #[test]
    fn test_uk_callsigns() {
        assert!(validate_callsign("G0ABC").is_ok());
        assert!(validate_callsign("M0XYZ").is_ok());
        assert!(validate_callsign("G3DEF").is_ok());
    }
    
    #[test]
    fn test_invalid_callsigns() {
        assert!(validate_callsign("").is_err());
        assert!(validate_callsign("AB").is_err()); // Too short
        assert!(validate_callsign("ABCDEFGH").is_err()); // Too long
        assert!(validate_callsign("ABC").is_err()); // No numbers
        assert!(validate_callsign("123").is_err()); // No letters
        assert!(validate_callsign("1ABC").is_err()); // Starts with number
        assert!(validate_callsign("A-BC").is_err()); // Invalid character
    }
    
    #[test]
    fn test_special_callsigns() {
        assert!(validate_callsign("W1AW/1").is_ok());
        assert!(validate_callsign("DL0ABC").is_ok());
        assert!(validate_callsign("JA1XYZ").is_ok());
    }
    
    #[test]
    fn test_case_normalization() {
        assert!(validate_callsign("w1aw").is_ok());
        assert!(validate_callsign("k2abc").is_ok());
        assert!(validate_callsign("Ve3def").is_ok());
    }
    
    #[tokio::test]
    async fn test_callsign_validator() {
        use crate::config::DatabaseConfig;
        use tempfile::tempdir;
        
        let temp_dir = tempdir().unwrap();
        let db_path = temp_dir.path().join("test.db");
        
        let mut db_config = DatabaseConfig::default();
        db_config.path = db_path;
        
        let database = Database::new(&db_config).await.unwrap();
        database.migrate().await.unwrap();
        
        let validator = CallsignValidator::new(database).await.unwrap();
        
        let result = validator.validate("W1AW").await.unwrap();
        assert!(result.valid_format);
        assert!(result.country.is_some());
        
        let country = result.country.unwrap();
        assert_eq!(country.name, "United States");
        assert_eq!(country.country_code, "US");
    }
}
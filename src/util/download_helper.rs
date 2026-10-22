use reqwest::header::{HeaderMap, HeaderValue, USER_AGENT, HeaderName};
use serde_json::{json, Value};
use std::fs;
use std::path::{Path, PathBuf};
use sha2::{Sha256, Digest};
use std::io::Read;
use chrono::Local;

/// Structure to hold download metadata (checksum, size, timestamp, etc.)
#[derive(Debug, Clone)]
pub struct DownloadMetadata {
    pub app_id: String,
    pub version: Option<String>,
    pub filename: String,
    pub sha256: String,
    pub file_size: u64,
    pub download_url: String,
    pub timestamp: String,
    pub source: String,
}

impl DownloadMetadata {
    /// Create new metadata
    pub fn new(
        app_id: String,
        version: Option<String>,
        filename: String,
        sha256: String,
        file_size: u64,
        download_url: String,
        source: String,
    ) -> Self {
        DownloadMetadata {
            app_id,
            version,
            filename,
            sha256,
            file_size,
            download_url,
            timestamp: Local::now().to_rfc3339(),
            source,
        }
    }
    
    /// Convert to JSON for storage
    pub fn to_json(&self) -> Value {
        json!({
            "app_id": self.app_id,
            "version": self.version,
            "filename": self.filename,
            "sha256": self.sha256,
            "file_size": self.file_size,
            "download_url": self.download_url,
            "timestamp": self.timestamp,
            "source": self.source,
        })
    }

    /// Load from JSON
    pub fn from_json(value: &Value) -> Option<Self> {
        Some(DownloadMetadata {
            app_id: value.get("app_id")?.as_str()?.to_string(),
            version: value.get("version").and_then(|v| v.as_str()).map(|s| s.to_string()),
            filename: value.get("filename")?.as_str()?.to_string(),
            sha256: value.get("sha256")?.as_str()?.to_string(),
            file_size: value.get("file_size")?.as_u64()?,
            download_url: value.get("download_url")?.as_str()?.to_string(),
            timestamp: value.get("timestamp")?.as_str()?.to_string(),
            source: value.get("source")?.as_str()?.to_string(),
        })
    }
}

/// Build request headers with anti-blocking measures
pub fn build_headers(
    custom_user_agent: Option<&str>,
    custom_headers: Option<&str>,
) -> HeaderMap {
    let mut headers = HeaderMap::new();

    // Set User-Agent to avoid detection as bot
    let user_agent = custom_user_agent.unwrap_or(
        "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 \
         (KHTML, like Gecko) Chrome/119.0.0.0 Safari/537.36"
    );
    headers.insert(USER_AGENT, HeaderValue::from_str(user_agent).unwrap_or_else(|_| {
        HeaderValue::from_static("Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36")
    }));

    // Add standard headers to look more like a real browser
    headers.insert("Accept", HeaderValue::from_static("*/*"));
    headers.insert("Accept-Language", HeaderValue::from_static("en-US,en;q=0.9"));
    headers.insert("Cache-Control", HeaderValue::from_static("no-cache"));
    headers.insert("Pragma", HeaderValue::from_static("no-cache"));
    headers.insert("Sec-Fetch-Dest", HeaderValue::from_static("document"));
    headers.insert("Sec-Fetch-Mode", HeaderValue::from_static("navigate"));
    headers.insert("Sec-Fetch-Site", HeaderValue::from_static("none"));
    headers.insert("Upgrade-Insecure-Requests", HeaderValue::from_static("1"));

    // Parse custom headers if provided (format: "Header1:Value1,Header2:Value2")
    if let Some(custom) = custom_headers {
        for header_pair in custom.split(',') {
            if let Some((k, v)) = header_pair.split_once(':') {
                let key = k.trim();
                let value = v.trim();
                if let Ok(header_value) = HeaderValue::from_str(value) {
                    // parse key into owned HeaderName to avoid borrowing
                    if let Ok(header_name) = key.parse::<HeaderName>() {
                        let _ = headers.insert(header_name, header_value);
                    }
                }
            }
        }
    }

    headers
}

/// Compute SHA256 checksum of a file
pub fn compute_sha256(file_path: &Path) -> Result<String, Box<dyn std::error::Error>> {
    let mut file = fs::File::open(file_path)?;
    let mut hasher = Sha256::new();
    let mut buffer = [0; 8192];

    loop {
        let count = file.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        hasher.update(&buffer[..count]);
    }

    Ok(format!("{:x}", hasher.finalize()))
}

/// Save metadata to a JSON file alongside the APK
pub fn save_metadata(
    apk_path: &Path,
    metadata: &DownloadMetadata,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut metadata_path = PathBuf::from(apk_path);
    metadata_path.set_extension("json");

    let json_data = serde_json::json!({
        "metadata": metadata.to_json()
    });

    fs::write(metadata_path, json_data.to_string())?;
    Ok(())
}

/// Load metadata from JSON file if it exists
pub fn load_metadata(apk_path: &Path) -> Option<DownloadMetadata> {
    let mut metadata_path = PathBuf::from(apk_path);
    metadata_path.set_extension("json");

    if let Ok(content) = fs::read_to_string(&metadata_path) {
        if let Ok(json) = serde_json::from_str::<Value>(&content) {
            return DownloadMetadata::from_json(json.get("metadata")?);
        }
    }
    None
}

/// Verify file integrity using stored metadata
pub fn verify_file_integrity(
    apk_path: &Path,
    expected_sha256: Option<&str>,
) -> Result<bool, Box<dyn std::error::Error>> {
    let computed_hash = compute_sha256(apk_path)?;

    if let Some(expected) = expected_sha256 {
        Ok(computed_hash.to_lowercase() == expected.to_lowercase())
    } else if let Some(metadata) = load_metadata(apk_path) {
        Ok(computed_hash.to_lowercase() == metadata.sha256.to_lowercase())
    } else {
        Ok(true) // No checksum to verify against
    }
}

/// Check if a partial download exists (for resume support)
pub fn get_partial_file_size(file_path: &Path) -> u64 {
    if let Ok(metadata) = fs::metadata(file_path) {
        metadata.len()
    } else {
        0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_build_headers_default() {
        let headers = build_headers(None, None);
        assert!(headers.contains_key("user-agent"));
        assert!(headers.contains_key("accept"));
    }

    #[test]
    fn test_build_headers_custom() {
        let headers = build_headers(Some("TestBot/1.0"), Some("X-Test:value"));
        assert_eq!(headers.get("user-agent").unwrap().to_str().unwrap(), "TestBot/1.0");
    }

    #[test]
    fn test_metadata_json_roundtrip() {
        let metadata = DownloadMetadata::new(
            "com.test".to_string(),
            Some("1.0".to_string()),
            "test.apk".to_string(),
            "abc123".to_string(),
            1024,
            "https://test.com".to_string(),
            "test".to_string(),
        );
        let json = metadata.to_json();
        let restored = DownloadMetadata::from_json(&json).unwrap();
        assert_eq!(metadata.app_id, restored.app_id);
    }

    #[test]
    fn test_compute_sha256() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("test.txt");
        fs::write(&file_path, b"hello world").unwrap();
        let hash = compute_sha256(&file_path).unwrap();
        assert_eq!(hash, "b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9");
    }

    #[test]
    fn test_get_partial_file_size() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("test.txt");
        fs::write(&file_path, b"hello").unwrap();
        assert_eq!(get_partial_file_size(&file_path), 5);
        assert_eq!(get_partial_file_size(&dir.path().join("none.txt")), 0);
    }
}

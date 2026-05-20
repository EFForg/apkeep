#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::Path;
    use tempfile::tempdir;

    #[test]
    fn test_build_headers_default() {
        let headers = build_headers(None, None);
        assert!(headers.contains_key("user-agent"));
        assert!(headers.contains_key("accept"));
    }

    #[test]
    fn test_build_headers_custom_user_agent() {
        let headers = build_headers(Some("CustomBot/1.0"), None);
        let ua = headers.get("user-agent").unwrap().to_str().unwrap();
        assert_eq!(ua, "CustomBot/1.0");
    }

    #[test]
    fn test_build_headers_custom_headers() {
        let headers = build_headers(None, Some("X-Custom:test,X-Another:value"));
        assert!(headers.contains_key("x-custom"));
        assert!(headers.contains_key("x-another"));
    }

    #[test]
    fn test_metadata_creation() {
        let metadata = DownloadMetadata::new(
            "com.test.app".to_string(),
            Some("1.0.0".to_string()),
            "test.apk".to_string(),
            "abc123".to_string(),
            1024,
            "https://example.com/test.apk".to_string(),
            "test-source".to_string(),
        );
        
        assert_eq!(metadata.app_id, "com.test.app");
        assert_eq!(metadata.version, Some("1.0.0".to_string()));
        assert_eq!(metadata.filename, "test.apk");
    }

    #[test]
    fn test_metadata_json_roundtrip() {
        let metadata = DownloadMetadata::new(
            "com.test.app".to_string(),
            Some("1.0.0".to_string()),
            "test.apk".to_string(),
            "abc123".to_string(),
            1024,
            "https://example.com/test.apk".to_string(),
            "test-source".to_string(),
        );
        
        let json = metadata.to_json();
        let restored = DownloadMetadata::from_json(&json).unwrap();
        
        assert_eq!(metadata.app_id, restored.app_id);
        assert_eq!(metadata.version, restored.version);
        assert_eq!(metadata.sha256, restored.sha256);
    }

    #[test]
    fn test_compute_sha256() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("test.txt");
        fs::write(&file_path, b"hello world").unwrap();
        
        let hash = compute_sha256(&file_path).unwrap();
        // SHA256 of "hello world"
        assert_eq!(hash, "b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9");
    }

    #[test]
    fn test_save_and_load_metadata() {
        let dir = tempdir().unwrap();
        let apk_path = dir.path().join("test.apk");
        fs::write(&apk_path, b"fake apk").unwrap();
        
        let metadata = DownloadMetadata::new(
            "com.test.app".to_string(),
            Some("1.0.0".to_string()),
            "test.apk".to_string(),
            "abc123".to_string(),
            1024,
            "https://example.com/test.apk".to_string(),
            "test-source".to_string(),
        );
        
        save_metadata(&apk_path, &metadata).unwrap();
        let loaded = load_metadata(&apk_path).unwrap();
        
        assert_eq!(metadata.app_id, loaded.app_id);
        assert_eq!(metadata.sha256, loaded.sha256);
    }

    #[test]
    fn test_get_partial_file_size() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("test.txt");
        fs::write(&file_path, b"hello").unwrap();
        
        let size = get_partial_file_size(&file_path);
        assert_eq!(size, 5);
        
        let nonexistent = dir.path().join("nonexistent.txt");
        let size = get_partial_file_size(&nonexistent);
        assert_eq!(size, 0);
    }
}

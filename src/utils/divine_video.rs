fn is_hex_hash(s: &str) -> bool {
    s.len() == 64 && s.bytes().all(|b| b.is_ascii_hexdigit())
}

pub fn extract_divine_blob_hash(url: &str) -> Option<String> {
    let parsed = url::Url::parse(url).ok()?;
    if !parsed
        .host_str()
        .map(|h| h.to_lowercase().contains("divine.video"))
        .unwrap_or(false)
    {
        return None;
    }
    let mut segments = parsed.path_segments()?;
    let hash = segments.next()?;
    if is_hex_hash(hash) {
        Some(hash.to_string())
    } else {
        None
    }
}

pub fn divine_hls_url(hash: &str) -> String {
    format!("https://media.divine.video/{}/hls/master.m3u8", hash)
}

pub struct VideoSrc {
    pub direct_url: Option<String>,
    pub hls_url: Option<String>,
}

pub fn resolve_video_src(url: &str) -> VideoSrc {
    if let Some(hash) = extract_divine_blob_hash(url) {
        VideoSrc {
            direct_url: None,
            hls_url: Some(divine_hls_url(&hash)),
        }
    } else {
        VideoSrc {
            direct_url: Some(url.to_string()),
            hls_url: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_divine_blob_hash_valid() {
        let url = "https://media.divine.video/5f6310fa0c9615988b6b58054734cf0368663c2f07841857bab4cbce050c8af6";
        assert_eq!(
            extract_divine_blob_hash(url),
            Some("5f6310fa0c9615988b6b58054734cf0368663c2f07841857bab4cbce050c8af6".to_string())
        );
    }

    #[test]
    fn test_extract_divine_blob_hash_with_path_suffix() {
        let url = "https://media.divine.video/5f6310fa0c9615988b6b58054734cf0368663c2f07841857bab4cbce050c8af6/hls/master.m3u8";
        assert_eq!(
            extract_divine_blob_hash(url),
            Some("5f6310fa0c9615988b6b58054734cf0368663c2f07841857bab4cbce050c8af6".to_string())
        );
    }

    #[test]
    fn test_extract_divine_blob_hash_wrong_host() {
        let url = "https://example.com/5f6310fa0c9615988b6b58054734cf0368663c2f07841857bab4cbce050c8af6";
        assert_eq!(extract_divine_blob_hash(url), None);
    }

    #[test]
    fn test_extract_divine_blob_hash_short_hash() {
        let url = "https://media.divine.video/abc123";
        assert_eq!(extract_divine_blob_hash(url), None);
    }

    #[test]
    fn test_extract_divine_blob_hash_invalid_url() {
        assert_eq!(extract_divine_blob_hash("not a url"), None);
    }

    #[test]
    fn test_divine_hls_url() {
        assert_eq!(
            divine_hls_url("abc123"),
            "https://media.divine.video/abc123/hls/master.m3u8"
        );
    }
}

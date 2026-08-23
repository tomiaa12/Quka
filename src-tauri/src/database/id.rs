use sha2::{Digest, Sha256};

pub fn normalize_path(path: &str) -> String {
    path.replace('\\', "/").trim_end_matches('/').to_lowercase()
}

pub fn generate_id(path: &str, bundle_id: Option<&str>) -> String {
    if let Some(bundle_id) = bundle_id {
        if !bundle_id.is_empty() {
            return bundle_id.to_string();
        }
    }

    let mut hasher = Sha256::new();
    hasher.update(normalize_path(path).as_bytes());
    format!("{:x}", hasher.finalize())
}

#[cfg(test)]
mod tests {
    use super::{generate_id, normalize_path};

    #[test]
    fn prefers_bundle_id() {
        assert_eq!(
            generate_id("/Applications/Google Chrome.app", Some("com.google.Chrome")),
            "com.google.Chrome"
        );
    }

    #[test]
    fn hashes_normalized_path() {
        let left = generate_id(r"C:\Program Files\App\app.exe", None);
        let right = generate_id("c:/program files/app/app.exe", None);
        assert_eq!(left, right);
        assert_eq!(left.len(), 64);
    }

    #[test]
    fn normalizes_separators() {
        assert_eq!(normalize_path(r"C:\Apps\Foo\"), "c:/apps/foo");
    }
}

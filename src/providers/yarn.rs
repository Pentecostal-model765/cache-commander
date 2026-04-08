use super::MetadataField;
use std::path::Path;

/// Returns true if the path is within a known Yarn cache location.
#[allow(dead_code)]
pub fn is_yarn_cache(path: &Path) -> bool {
    let path_str = path.to_string_lossy();
    path_str.contains(".yarn-cache")
        || path_str.contains(".cache/yarn")
        || path_str.contains("Library/Caches/Yarn")
        || path_str.contains(".yarn/cache")
        || path_str.contains("yarn/berry/cache")
}

/// Returns true if this is a Yarn Berry (v2+) cache path.
pub fn is_berry(path: &Path) -> bool {
    let path_str = path.to_string_lossy();
    path_str.contains(".yarn/cache") || path_str.contains("berry/cache")
}

/// Normalize a scoped package name: "@babel-core" → "@babel/core".
/// Only the first hyphen after '@' becomes a '/'.
pub fn normalize_scoped_name(name: &str) -> String {
    if let Some(rest) = name.strip_prefix('@') {
        if let Some(hyphen_pos) = rest.find('-') {
            let scope = &rest[..hyphen_pos];
            let pkg = &rest[hyphen_pos + 1..];
            return format!("@{}/{}", scope, pkg);
        }
    }
    name.to_string()
}

/// Parse a Yarn Berry filename: `<name>-npm-<version>-<hash>.zip`
///
/// Examples:
/// - `lodash-npm-4.17.21-6382d821f21d.zip` → `("lodash", "4.17.21")`
/// - `@babel-core-npm-7.24.0-abc123def456.zip` → `("@babel/core", "7.24.0")`
pub fn parse_berry_filename(filename: &str) -> Option<(String, String)> {
    let stem = filename.strip_suffix(".zip")?;

    let npm_marker = "-npm-";

    // Find the correct `-npm-` boundary. Package names can contain `-npm-`
    // (e.g., `use-npm-module`), so we search from right to left for
    // a `-npm-` that is followed by a valid version (digit-starting).
    let mut search_from = stem.len();
    let npm_pos = loop {
        // rfind within stem[..search_from]
        let slice = &stem[..search_from];
        let pos = slice.rfind(npm_marker)?;
        let after = &stem[pos + npm_marker.len()..];
        // Check if what follows starts with a digit (version)
        if after
            .chars()
            .next()
            .map(|c| c.is_ascii_digit())
            .unwrap_or(false)
        {
            break pos;
        }
        // Try earlier occurrence
        if pos == 0 {
            return None;
        }
        search_from = pos;
    };

    let raw_name = &stem[..npm_pos];
    let after_npm = &stem[npm_pos + npm_marker.len()..];

    // after_npm = "<version>-<hash>"
    let parts: Vec<&str> = after_npm.split('-').collect();
    if parts.len() < 2 {
        return None;
    }

    // Last part should be the hash
    let hash = parts.last()?;
    if !is_hex_hash(hash) {
        return None;
    }

    // Everything before the hash is the version
    let version_parts = &parts[..parts.len() - 1];
    let version = version_parts.join("-");

    if version.is_empty() {
        return None;
    }

    // Version must start with a digit (already guaranteed by the search above, but be safe)
    if !version
        .chars()
        .next()
        .map(|c| c.is_ascii_digit())
        .unwrap_or(false)
    {
        return None;
    }

    let name = normalize_scoped_name(raw_name);
    Some((name, version))
}

/// Parse a Yarn Classic filename: `npm-<name>-<version>-<hash>.tgz`
///
/// Examples:
/// - `npm-lodash-4.17.21-6382d821f21d.tgz` → `("lodash", "4.17.21")`
/// - `npm-@babel-core-7.24.0-abc123def456.tgz` → `("@babel/core", "7.24.0")`
/// - `npm-is-even-1.0.0-abc123def456.tgz` → `("is-even", "1.0.0")`
pub fn parse_classic_filename(filename: &str) -> Option<(String, String)> {
    let stem = filename.strip_suffix(".tgz")?;

    // Must start with "npm-"
    let after_npm = stem.strip_prefix("npm-")?;

    // after_npm = "<name>-<version>-<hash>"
    // Split on '-' and walk from right to find hash, then version, then name
    let parts: Vec<&str> = after_npm.split('-').collect();
    if parts.len() < 3 {
        return None;
    }

    // Last part is hash
    let hash = parts.last()?;
    if !is_hex_hash(hash) {
        return None;
    }

    // Find version boundary: walk backwards from (len-2) to find the first
    // segment that starts with a digit — that is the start of the version
    let without_hash = &parts[..parts.len() - 1];

    // Find the rightmost index where a digit-starting segment exists
    // that, together with subsequent segments (before hash), forms a valid version
    let mut version_start = None;
    for i in (0..without_hash.len()).rev() {
        if without_hash[i]
            .chars()
            .next()
            .map(|c| c.is_ascii_digit())
            .unwrap_or(false)
        {
            version_start = Some(i);
            break;
        }
    }

    let ver_idx = version_start?;
    if ver_idx == 0 {
        // No name before the version
        return None;
    }

    let name_parts = &without_hash[..ver_idx];
    let version_parts = &without_hash[ver_idx..];

    let raw_name = name_parts.join("-");
    let version = version_parts.join("-");

    let name = normalize_scoped_name(&raw_name);
    Some((name, version))
}

/// Returns true if the string looks like a hex hash (8+ lowercase hex chars).
fn is_hex_hash(s: &str) -> bool {
    s.len() >= 8
        && s.chars()
            .all(|c| c.is_ascii_digit() || ('a'..='f').contains(&c))
}

pub fn semantic_name(path: &Path) -> Option<String> {
    let name = path.file_name()?.to_string_lossy().to_string();

    // Cache root directory
    if name == "cache" {
        if is_berry(path) {
            return Some("Yarn Berry Cache".to_string());
        } else {
            return Some("Yarn Cache".to_string());
        }
    }

    // .yarn-cache directory
    if name == ".yarn-cache" {
        return Some("Yarn Classic Cache".to_string());
    }

    // Berry zip files
    if name.ends_with(".zip") {
        if let Some((pkg, ver)) = parse_berry_filename(&name) {
            return Some(format!("{} {}", pkg, ver));
        }
    }

    // Classic tgz files
    if name.ends_with(".tgz") {
        if let Some((pkg, ver)) = parse_classic_filename(&name) {
            return Some(format!("{} {}", pkg, ver));
        }
    }

    None
}

pub fn package_id(path: &Path) -> Option<super::PackageId> {
    let name = path.file_name()?.to_string_lossy().to_string();

    if name.ends_with(".zip") {
        let (pkg, ver) = parse_berry_filename(&name)?;
        return Some(super::PackageId {
            ecosystem: "npm",
            name: pkg,
            version: ver,
        });
    }

    if name.ends_with(".tgz") {
        let (pkg, ver) = parse_classic_filename(&name)?;
        return Some(super::PackageId {
            ecosystem: "npm",
            name: pkg,
            version: ver,
        });
    }

    None
}

pub fn metadata(path: &Path) -> Vec<MetadataField> {
    let mut fields = Vec::new();
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();

    if name.ends_with(".zip") {
        fields.push(MetadataField {
            label: "Format".to_string(),
            value: "Yarn Berry (.zip)".to_string(),
        });
        return fields;
    }

    if name.ends_with(".tgz") {
        fields.push(MetadataField {
            label: "Format".to_string(),
            value: "Yarn Classic (.tgz)".to_string(),
        });
        return fields;
    }

    // Cache root directories: count .zip/.tgz files
    if path.is_dir() {
        if let Ok(entries) = std::fs::read_dir(path) {
            let count = entries
                .filter_map(|e| e.ok())
                .filter(|e| {
                    let p = e.path();
                    let ext = p
                        .extension()
                        .map(|x| x.to_string_lossy().to_string())
                        .unwrap_or_default();
                    ext == "zip" || ext == "tgz"
                })
                .count();
            if count > 0 {
                fields.push(MetadataField {
                    label: "Packages".to_string(),
                    value: count.to_string(),
                });
            }
        }
    }

    fields
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    // --- Detection ---

    #[test]
    fn detects_classic_yarn_cache() {
        assert!(is_yarn_cache(&PathBuf::from("/home/user/.yarn-cache/v6")));
    }

    #[test]
    fn detects_classic_xdg_yarn_cache() {
        assert!(is_yarn_cache(&PathBuf::from("/home/user/.cache/yarn/v6")));
    }

    #[test]
    fn detects_berry_per_project_cache() {
        assert!(is_yarn_cache(&PathBuf::from(
            "/home/user/project/.yarn/cache"
        )));
    }

    #[test]
    fn detects_macos_yarn_cache() {
        assert!(is_yarn_cache(&PathBuf::from(
            "/Users/user/Library/Caches/Yarn/v6"
        )));
    }

    #[test]
    fn does_not_detect_unrelated_path() {
        assert!(!is_yarn_cache(&PathBuf::from("/home/user/.npm/_cacache")));
    }

    // --- Berry parsing ---

    #[test]
    fn parse_berry_simple_package() {
        let result = parse_berry_filename("lodash-npm-4.17.21-6382d821f21d.zip");
        assert_eq!(result, Some(("lodash".to_string(), "4.17.21".to_string())));
    }

    #[test]
    fn parse_berry_scoped_package() {
        let result = parse_berry_filename("@babel-core-npm-7.24.0-abc123def456.zip");
        assert_eq!(
            result,
            Some(("@babel/core".to_string(), "7.24.0".to_string()))
        );
    }

    #[test]
    fn parse_berry_prerelease_version() {
        let result = parse_berry_filename("typescript-npm-5.0.0-beta.1-abcdef012345.zip");
        assert_eq!(
            result,
            Some(("typescript".to_string(), "5.0.0-beta.1".to_string()))
        );
    }

    #[test]
    fn parse_berry_invalid_no_npm_marker() {
        let result = parse_berry_filename("lodash-4.17.21-6382d821f21d.zip");
        assert_eq!(result, None);
    }

    // --- Classic parsing ---

    #[test]
    fn parse_classic_simple_package() {
        let result = parse_classic_filename("npm-lodash-4.17.21-6382d821f21d.tgz");
        assert_eq!(result, Some(("lodash".to_string(), "4.17.21".to_string())));
    }

    #[test]
    fn parse_classic_scoped_package() {
        let result = parse_classic_filename("npm-@babel-core-7.24.0-abc123def456.tgz");
        assert_eq!(
            result,
            Some(("@babel/core".to_string(), "7.24.0".to_string()))
        );
    }

    #[test]
    fn parse_classic_hyphenated_name() {
        let result = parse_classic_filename("npm-is-even-1.0.0-abc123def456.tgz");
        assert_eq!(result, Some(("is-even".to_string(), "1.0.0".to_string())));
    }

    #[test]
    fn parse_classic_invalid_no_npm_prefix() {
        let result = parse_classic_filename("lodash-4.17.21-6382d821f21d.tgz");
        assert_eq!(result, None);
    }

    // --- Semantic name ---

    #[test]
    fn semantic_name_berry_zip() {
        let path = PathBuf::from("/project/.yarn/cache/lodash-npm-4.17.21-6382d821f21d.zip");
        assert_eq!(semantic_name(&path), Some("lodash 4.17.21".into()));
    }

    #[test]
    fn semantic_name_classic_tgz() {
        let path = PathBuf::from("/home/user/.yarn-cache/v6/npm-express-4.21.0-abc123def456.tgz");
        assert_eq!(semantic_name(&path), Some("express 4.21.0".into()));
    }

    #[test]
    fn semantic_name_cache_dir_berry() {
        let path = PathBuf::from("/project/.yarn/cache");
        assert_eq!(semantic_name(&path), Some("Yarn Berry Cache".into()));
    }

    #[test]
    fn semantic_name_cache_dir_classic() {
        // .cache/yarn/cache is not a berry path
        let path = PathBuf::from("/home/user/.cache/yarn/cache");
        assert_eq!(semantic_name(&path), Some("Yarn Cache".into()));
    }

    #[test]
    fn semantic_name_yarn_cache_dir() {
        let path = PathBuf::from("/home/user/.yarn-cache");
        assert_eq!(semantic_name(&path), Some("Yarn Classic Cache".into()));
    }

    #[test]
    fn semantic_name_unknown_file() {
        let path = PathBuf::from("/home/user/.yarn/cache/README.md");
        assert_eq!(semantic_name(&path), None);
    }

    // --- Normalize ---

    #[test]
    fn normalize_scoped() {
        assert_eq!(normalize_scoped_name("@babel-core"), "@babel/core");
    }

    #[test]
    fn normalize_unscoped() {
        assert_eq!(normalize_scoped_name("lodash"), "lodash");
    }

    #[test]
    fn normalize_scoped_deep() {
        assert_eq!(normalize_scoped_name("@types-node"), "@types/node");
    }

    // --- Package ID ---

    #[test]
    fn package_id_berry_zip() {
        let path = PathBuf::from("/project/.yarn/cache/lodash-npm-4.17.21-6382d821f21d.zip");
        let id = package_id(&path).unwrap();
        assert_eq!(id.ecosystem, "npm");
        assert_eq!(id.name, "lodash");
        assert_eq!(id.version, "4.17.21");
    }

    #[test]
    fn package_id_classic_tgz() {
        let path = PathBuf::from("/home/user/.cache/yarn/v6/npm-express-4.21.0-abc123def456.tgz");
        let id = package_id(&path).unwrap();
        assert_eq!(id.ecosystem, "npm");
        assert_eq!(id.name, "express");
        assert_eq!(id.version, "4.21.0");
    }

    #[test]
    fn package_id_scoped_berry() {
        let path = PathBuf::from("/project/.yarn/cache/@babel-core-npm-7.24.0-abc123def456.zip");
        let id = package_id(&path).unwrap();
        assert_eq!(id.name, "@babel/core");
    }

    #[test]
    fn package_id_non_package_file() {
        let path = PathBuf::from("/project/.yarn/cache/.gitignore");
        assert_eq!(package_id(&path), None);
    }

    #[test]
    fn package_id_directory_returns_none() {
        let path = PathBuf::from("/project/.yarn/cache");
        assert_eq!(package_id(&path), None);
    }

    // --- Bug 1: package name contains "-npm-" ---

    #[test]
    fn parse_berry_package_name_contains_npm() {
        // Package name contains "-npm-" substring
        let result = parse_berry_filename("use-npm-module-npm-1.0.0-abcdef012345.zip");
        assert_eq!(
            result,
            Some(("use-npm-module".to_string(), "1.0.0".to_string()))
        );
    }

    #[test]
    fn parse_berry_npm_run_all() {
        // Real-world popular package with "npm" in name
        let result = parse_berry_filename("npm-run-all-npm-4.1.5-abcdef012345.zip");
        assert_eq!(
            result,
            Some(("npm-run-all".to_string(), "4.1.5".to_string()))
        );
    }

    // --- Bug 3: is_hex_hash boundary tests ---

    #[test]
    fn hex_hash_rejects_7_chars() {
        assert!(!is_hex_hash("abcdef0"));
    }

    #[test]
    fn hex_hash_accepts_8_chars() {
        assert!(is_hex_hash("abcdef01"));
    }

    #[test]
    fn hex_hash_rejects_uppercase() {
        assert!(!is_hex_hash("ABCDEF01"));
    }

    // --- Edge case tests ---

    #[test]
    fn parse_classic_digit_in_package_name() {
        // Package name starts with or contains digits
        let result = parse_classic_filename("npm-base64-js-1.5.1-abcdef012345.tgz");
        assert_eq!(result, Some(("base64-js".to_string(), "1.5.1".to_string())));
    }

    #[test]
    fn parse_classic_2to3() {
        let result = parse_classic_filename("npm-2to3-1.0.0-abcdef012345.tgz");
        assert_eq!(result, Some(("2to3".to_string(), "1.0.0".to_string())));
    }

    #[test]
    fn normalize_scoped_multi_hyphen() {
        // Scoped package with hyphens in package name
        assert_eq!(
            normalize_scoped_name("@babel-plugin-transform-runtime"),
            "@babel/plugin-transform-runtime"
        );
    }

    #[test]
    fn semantic_name_berry_global_cache() {
        let path = PathBuf::from("/home/user/.cache/yarn/berry/cache");
        assert_eq!(semantic_name(&path), Some("Yarn Berry Cache".into()));
    }

    // --- Metadata ---

    #[test]
    fn metadata_berry_zip_shows_format() {
        let path = PathBuf::from("/project/.yarn/cache/lodash-npm-4.17.21-6382d821f21d.zip");
        let fields = metadata(&path);
        assert!(!fields.is_empty());
        assert!(
            fields
                .iter()
                .any(|f| f.label == "Format" && f.value.contains("Berry"))
        );
    }

    #[test]
    fn metadata_classic_tgz_shows_format() {
        let path = PathBuf::from("/home/user/.cache/yarn/v6/npm-lodash-4.17.21-abc123def456.tgz");
        let fields = metadata(&path);
        assert!(!fields.is_empty());
        assert!(
            fields
                .iter()
                .any(|f| f.label == "Format" && f.value.contains("Classic"))
        );
    }

    #[test]
    fn metadata_cache_dir_counts_packages() {
        let tmp = tempfile::tempdir().unwrap();
        let cache_dir = tmp.path().join("cache");
        std::fs::create_dir_all(&cache_dir).unwrap();
        std::fs::write(cache_dir.join("lodash-npm-4.17.21-abc123def456.zip"), "x").unwrap();
        std::fs::write(cache_dir.join("express-npm-4.21.0-def789abc012.zip"), "x").unwrap();
        std::fs::write(cache_dir.join("npm-lodash-4.17.21-abc123def456.tgz"), "x").unwrap();
        std::fs::write(cache_dir.join("README.md"), "x").unwrap();

        let fields = metadata(&cache_dir);
        let pkg_field = fields.iter().find(|f| f.label == "Packages");
        assert!(pkg_field.is_some(), "Expected Packages field");
        assert_eq!(pkg_field.unwrap().value, "3");
    }
}

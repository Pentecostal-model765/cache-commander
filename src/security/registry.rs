pub fn parse_pypi_latest(json: &str) -> Option<String> {
    let val: serde_json::Value = serde_json::from_str(json).ok()?;
    val["info"]["version"]
        .as_str()
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
}

pub fn parse_crates_io_latest(json: &str) -> Option<String> {
    let val: serde_json::Value = serde_json::from_str(json).ok()?;
    val["crate"]["max_version"]
        .as_str()
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
}

pub fn parse_npm_latest(json: &str) -> Option<String> {
    let val: serde_json::Value = serde_json::from_str(json).ok()?;
    val["version"]
        .as_str()
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
}

/// Build the registry URL for a given package, or `None` if the ecosystem is unsupported.
pub fn build_registry_url(pkg: &crate::providers::PackageId) -> Option<String> {
    match pkg.ecosystem {
        "PyPI" => Some(format!("https://pypi.org/pypi/{}/json", pkg.name)),
        "crates.io" => Some(format!("https://crates.io/api/v1/crates/{}", pkg.name)),
        "npm" => Some(format!("https://registry.npmjs.org/{}/latest", pkg.name)),
        _ => None,
    }
}

/// Parse the registry JSON response for the given ecosystem.
pub fn parse_registry_response(ecosystem: &str, body: &str) -> Option<String> {
    match ecosystem {
        "PyPI" => parse_pypi_latest(body),
        "crates.io" => parse_crates_io_latest(body),
        "npm" => parse_npm_latest(body),
        _ => None,
    }
}

pub fn check_latest(pkg: &crate::providers::PackageId) -> Result<Option<String>, String> {
    let url = match build_registry_url(pkg) {
        Some(u) => u,
        None => return Ok(None),
    };

    let resp = ureq::agent()
        .get(&url)
        .timeout(std::time::Duration::from_secs(10))
        .set(
            "User-Agent",
            &format!(
                "ccmd/{} (https://github.com/juliensimon/cache-commander)",
                env!("CARGO_PKG_VERSION")
            ),
        )
        .call()
        .map_err(|e| format!("Registry request failed: {e}"))?;
    let text = resp
        .into_string()
        .map_err(|e| format!("Registry read failed: {e}"))?;

    Ok(parse_registry_response(pkg.ecosystem, &text))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_pypi_response() {
        let json = r#"{"info":{"version":"2.32.3"}}"#;
        assert_eq!(parse_pypi_latest(json), Some("2.32.3".into()));
    }

    #[test]
    fn parse_crates_io_response() {
        let json = r#"{"crate":{"max_version":"1.0.200"}}"#;
        assert_eq!(parse_crates_io_latest(json), Some("1.0.200".into()));
    }

    #[test]
    fn parse_npm_response() {
        let json = r#"{"version":"10.8.1"}"#;
        assert_eq!(parse_npm_latest(json), Some("10.8.1".into()));
    }

    #[test]
    fn parse_invalid_json_returns_none() {
        assert_eq!(parse_pypi_latest("not json"), None);
    }

    #[test]
    fn parse_pypi_missing_info_key() {
        assert_eq!(parse_pypi_latest(r#"{"other": "data"}"#), None);
    }

    #[test]
    fn parse_pypi_null_version() {
        assert_eq!(parse_pypi_latest(r#"{"info": {"version": null}}"#), None);
    }

    #[test]
    fn parse_crates_io_missing_crate_key() {
        assert_eq!(parse_crates_io_latest(r#"{"other": "data"}"#), None);
    }

    #[test]
    fn parse_npm_empty_version() {
        assert_eq!(parse_npm_latest(r#"{"version": ""}"#), None);
    }

    #[test]
    fn parse_npm_whitespace_only_version() {
        // Whitespace-only version is technically non-empty but would be useless;
        // current impl returns Some(" ") which is acceptable — the version comparison
        // will handle it by treating it as no numeric parts.
        let result = parse_npm_latest(r#"{"version": " "}"#);
        assert_eq!(result, Some(" ".to_string()));
    }

    #[test]
    fn parse_pypi_empty_version() {
        assert_eq!(parse_pypi_latest(r#"{"info": {"version": ""}}"#), None);
    }

    fn pkg(ecosystem: &'static str, name: &str) -> crate::providers::PackageId {
        crate::providers::PackageId {
            ecosystem,
            name: name.to_string(),
            version: "1.0.0".to_string(),
        }
    }

    #[test]
    fn build_registry_url_pypi() {
        assert_eq!(
            build_registry_url(&pkg("PyPI", "requests")),
            Some("https://pypi.org/pypi/requests/json".to_string())
        );
    }

    #[test]
    fn build_registry_url_crates_io() {
        assert_eq!(
            build_registry_url(&pkg("crates.io", "serde")),
            Some("https://crates.io/api/v1/crates/serde".to_string())
        );
    }

    #[test]
    fn build_registry_url_npm() {
        assert_eq!(
            build_registry_url(&pkg("npm", "lodash")),
            Some("https://registry.npmjs.org/lodash/latest".to_string())
        );
    }

    #[test]
    fn build_registry_url_unknown_ecosystem_returns_none() {
        assert_eq!(build_registry_url(&pkg("Homebrew", "whatever")), None);
        assert_eq!(build_registry_url(&pkg("", "whatever")), None);
    }

    #[test]
    fn parse_registry_response_dispatches_by_ecosystem() {
        assert_eq!(
            parse_registry_response("PyPI", r#"{"info":{"version":"1.2.3"}}"#),
            Some("1.2.3".to_string())
        );
        assert_eq!(
            parse_registry_response("crates.io", r#"{"crate":{"max_version":"0.5.0"}}"#),
            Some("0.5.0".to_string())
        );
        assert_eq!(
            parse_registry_response("npm", r#"{"version":"9.9.9"}"#),
            Some("9.9.9".to_string())
        );
        assert_eq!(parse_registry_response("unknown", r#"{}"#), None);
    }

    #[test]
    fn parse_crates_io_empty_version() {
        assert_eq!(
            parse_crates_io_latest(r#"{"crate": {"max_version": ""}}"#),
            None
        );
    }
}

// SwiftPM provider.
//
// No `package_id` / `upgrade_command` arms — SwiftPM's package identity
// requires git-ref parsing that is too brittle for v1, and there's no
// public registry for version lookups. OSV `SwiftURL` coverage is
// sparse. Tier-3 E2E tests are intentionally exempt (see design spec
// 2026-04-20-swiftpm-xcode-providers-design.md).

use super::MetadataField;
use std::path::Path;

pub fn semantic_name(_path: &Path) -> Option<String> {
    None
}

pub fn metadata(_path: &Path) -> Vec<MetadataField> {
    Vec::new()
}
